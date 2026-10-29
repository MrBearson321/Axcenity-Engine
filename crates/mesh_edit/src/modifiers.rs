//! Modifiers (Phase 57): a stack per object that changes its mesh on the
//! way out, leaving the mesh itself as it was, as Blender's do. Mirror,
//! Subdivision Surface, Array, Solidify, Bevel, Decimate and Triangulate.
//!
//! [`evaluate`] runs the shown modifiers in order on a copy of the mesh:
//! the Studio draws that with the viewport settings, and the `.glb` holds
//! it with the export ones. The mesh and its stack are kept in the model's
//! sidecar (`sidecar.rs`), so reopening it shows them still editable.
//! Applying one ([`ModifierKind::run`] on the mesh itself) bakes it in.

mod generate;
pub(crate) mod reduce;
mod subdivide;

use std::collections::BTreeSet;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::half_edge::EditMesh;
use crate::ops::{MeshOp, apply};
use crate::select::{SelectMode, Selection};

/// Copies an Array makes at most.
pub const MAX_COPIES: u32 = 256;
/// Subdivision levels at most.
pub const MAX_LEVELS: u32 = 6;

/// An object's own axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    #[default]
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

/// What a stack is evaluated for: drawing in the Studio, or the `.glb`.
/// Only Subdivision Surface tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Viewport,
    Export,
}

/// A modifier and its settings. Lengths are in the file's units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModifierKind {
    /// A copy reflected across the plane through the object's origin
    /// square to `axis`. With `merge`, vertices within `merge_distance` of
    /// the plane join their reflections, so a half closes into a whole;
    /// with `clip`, moves in Edit mode keep vertices from crossing it.
    Mirror {
        axis: Axis,
        clip: bool,
        merge: bool,
        merge_distance: f32,
    },
    /// Catmull-Clark subdivision, `viewport` levels in the Studio and
    /// `export` levels in the `.glb`.
    Subdivision { viewport: u32, export: u32 },
    /// `count` copies, each `offset` (in the mesh's own sizes) from the
    /// last. With `merge`, a copy's vertices within `merge_distance` of
    /// the last copy's join them, and the faces between go.
    Array {
        count: u32,
        offset: [f32; 3],
        merge: bool,
        merge_distance: f32,
    },
    /// A second skin `thickness` behind the surface (`offset` −1), in
    /// front of it (1) or either side (0), joined round open borders.
    Solidify { thickness: f32, offset: f32 },
    /// Edges sharper than `angle` degrees beveled.
    Bevel {
        width: f32,
        segments: u32,
        angle: f32,
    },
    /// Fewer triangles, `ratio` of them, through meshoptimizer.
    Decimate { ratio: f32 },
    /// Every face cut into triangles.
    Triangulate,
}

impl ModifierKind {
    /// One of each, with its usual settings, for the Add Modifier menu.
    pub fn defaults() -> Vec<ModifierKind> {
        vec![
            ModifierKind::Mirror {
                axis: Axis::X,
                clip: false,
                merge: true,
                merge_distance: 0.001,
            },
            ModifierKind::Subdivision {
                viewport: 1,
                export: 2,
            },
            ModifierKind::Array {
                count: 2,
                offset: [1.0, 0.0, 0.0],
                merge: false,
                merge_distance: 0.001,
            },
            ModifierKind::Solidify {
                thickness: 0.1,
                offset: -1.0,
            },
            ModifierKind::Bevel {
                width: 0.1,
                segments: 1,
                angle: 30.0,
            },
            ModifierKind::Decimate { ratio: 0.5 },
            ModifierKind::Triangulate,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ModifierKind::Mirror { .. } => "Mirror",
            ModifierKind::Subdivision { .. } => "Subdivision Surface",
            ModifierKind::Array { .. } => "Array",
            ModifierKind::Solidify { .. } => "Solidify",
            ModifierKind::Bevel { .. } => "Bevel",
            ModifierKind::Decimate { .. } => "Decimate",
            ModifierKind::Triangulate => "Triangulate",
        }
    }

    /// `mesh` with this modifier on it, or why it can't be.
    pub fn run(&self, mesh: &EditMesh, purpose: Purpose) -> Result<EditMesh, String> {
        match *self {
            ModifierKind::Mirror {
                axis,
                merge,
                merge_distance,
                ..
            } => Ok(generate::mirror(mesh, axis, merge, merge_distance)),
            ModifierKind::Subdivision { viewport, export } => {
                let levels = match purpose {
                    Purpose::Viewport => viewport,
                    Purpose::Export => export,
                };
                subdivide::catmull_clark(mesh, levels.min(MAX_LEVELS))
            }
            ModifierKind::Array {
                count,
                offset,
                merge,
                merge_distance,
            } => Ok(generate::array(
                mesh,
                count.clamp(1, MAX_COPIES),
                Vec3::from(offset),
                merge,
                merge_distance,
            )),
            ModifierKind::Solidify { thickness, offset } => {
                Ok(generate::solidify(mesh, thickness, offset.clamp(-1.0, 1.0)))
            }
            ModifierKind::Bevel {
                width,
                segments,
                angle,
            } => bevel(mesh, width, segments, angle),
            ModifierKind::Decimate { ratio } => Ok(reduce::decimate(mesh, ratio)),
            ModifierKind::Triangulate => Ok(reduce::triangulate(mesh)),
        }
    }
}

/// One modifier in a stack. A hidden one (`show` off) is skipped, in the
/// Studio and in the `.glb`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    #[serde(flatten)]
    pub kind: ModifierKind,
    pub show: bool,
}

impl Modifier {
    pub fn new(kind: ModifierKind) -> Modifier {
        Modifier { kind, show: true }
    }
}

/// A stack's result: the mesh, and each modifier that couldn't run (by
/// its place in the stack) and why. One that can't run is passed over.
#[derive(Clone, Debug)]
pub struct Evaluated {
    pub mesh: EditMesh,
    pub errors: Vec<(usize, String)>,
}

/// `mesh` with the shown modifiers of `stack` on it, in order.
pub fn evaluate(mesh: &EditMesh, stack: &[Modifier], purpose: Purpose) -> Evaluated {
    let mut current = mesh.clone();
    let mut errors = Vec::new();
    for (index, modifier) in stack.iter().enumerate() {
        if !modifier.show {
            continue;
        }
        match modifier.kind.run(&current, purpose) {
            Ok(next) => current = next,
            Err(why) => errors.push((index, why)),
        }
    }
    Evaluated {
        mesh: current,
        errors,
    }
}

/// Where a vertex moved from `start` to `moved` ends up with the clipping
/// Mirrors of `stack`: one on a mirror's plane stays on it, and none
/// crosses it.
pub fn clip(stack: &[Modifier], start: Vec3, moved: Vec3) -> Vec3 {
    let mut clipped = moved;
    for modifier in stack.iter().filter(|modifier| modifier.show) {
        if let ModifierKind::Mirror {
            axis,
            clip: true,
            merge_distance,
            ..
        } = modifier.kind
        {
            let i = axis.index();
            let on_plane = start[i].abs() <= merge_distance.max(1e-6);
            if on_plane || start[i] * clipped[i] < 0.0 {
                clipped[i] = 0.0;
            }
        }
    }
    clipped
}

/// The edges of `mesh` between faces meeting at more than `angle` degrees
/// beveled by `width`, in `segments`, round.
fn bevel(mesh: &EditMesh, width: f32, segments: u32, angle: f32) -> Result<EditMesh, String> {
    let cos = angle.to_radians().cos();
    let edges: BTreeSet<u32> = mesh
        .edges()
        .into_iter()
        .filter(|&edge| {
            let Some(twin) = mesh.half_edge(edge).twin else {
                return false;
            };
            let a = mesh.face_normal(mesh.half_edge(edge).face);
            let b = mesh.face_normal(mesh.half_edge(twin).face);
            a.dot(b) < cos
        })
        .collect();
    if edges.is_empty() || width <= 0.0 {
        return Ok(mesh.clone());
    }
    let selection = Selection {
        mode: SelectMode::Edge,
        edges,
        ..Selection::default()
    };
    let op = MeshOp::Bevel {
        width,
        segments: segments.clamp(1, 64),
        profile: 0.5,
        vertices: false,
    };
    apply(mesh, &selection, &op).map(|(mesh, _)| mesh)
}
