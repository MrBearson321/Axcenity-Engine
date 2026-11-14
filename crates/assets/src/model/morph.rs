//! Blend shapes (morph targets): named shapes a mesh can lean toward, each
//! moving some of its corners by a set amount at weight 1. Faces are made
//! of them (a blink, a smile), and so are corrective shapes.
//!
//! A model keeps one list of shape names ([`Model::morph_targets`]) shared
//! by its primitives, so a face split into several meshes (skin, lashes,
//! teeth) blinks as one: shapes of the same name in different meshes are
//! the same shape. Each primitive keeps only the corners a shape moves
//! ([`PrimitiveMorph`]), which is what the renderer uploads and morphs on
//! the GPU before skinning (`docs/research/facial-animation.md`, D1).
//!
//! [`Model::morph_targets`]: super::Model::morph_targets

use std::collections::HashMap;

use glam::{Quat, Vec3};

use super::Primitive;

/// One blend shape of a model.
#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
    /// Its name in the file (a Blender shape key's, a glTF `targetNames`
    /// entry, an FBX blend channel's), or made up when it has none.
    pub name: String,
    /// The weight the file rests it at, usually 0.
    pub default_weight: f32,
}

/// The corners one blend shape moves in one primitive, at weight 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrimitiveMorph {
    /// Which of the model's [`MorphTarget`]s this is.
    pub target: u16,
    /// The corners it moves, in rising order, without repeats.
    pub corners: Vec<u32>,
    /// How far each of `corners` moves, in the model's fitted space.
    pub positions: Vec<[f32; 3]>,
    /// How each of `corners`' normals changes (added, then made one unit
    /// long again).
    pub normals: Vec<[f32; 3]>,
}

impl PrimitiveMorph {
    /// The bytes it holds.
    pub fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(self.corners.as_slice())
            + std::mem::size_of_val(self.positions.as_slice())
            + std::mem::size_of_val(self.normals.as_slice())
    }

    /// The farthest it moves a corner.
    pub fn reach(&self) -> f32 {
        self.positions
            .iter()
            .map(|delta| Vec3::from(*delta).length())
            .fold(0.0, f32::max)
    }
}

/// Moves smaller than this (in the file's units) don't count: exporters
/// write every corner of a shape, most of them unmoved.
const STILL: f32 = 1e-7;

/// The model-wide list of shape names while a file is read: shapes of one
/// name in several meshes become one target.
#[derive(Default)]
pub(super) struct MorphNames {
    pub(super) targets: Vec<MorphTarget>,
    by_name: HashMap<String, u16>,
}

impl MorphNames {
    /// The target called `name`, added with `default_weight` if it's new.
    /// `None` past 65,535 shapes.
    pub(super) fn target(&mut self, name: &str, default_weight: f32) -> Option<u16> {
        if let Some(&index) = self.by_name.get(name) {
            return Some(index);
        }
        let index = u16::try_from(self.targets.len()).ok()?;
        self.targets.push(MorphTarget {
            name: name.to_owned(),
            default_weight: if default_weight.is_finite() {
                default_weight
            } else {
                0.0
            },
        });
        self.by_name.insert(name.to_owned(), index);
        Some(index)
    }

    pub(super) fn clear(&mut self) {
        self.targets.clear();
        self.by_name.clear();
    }
}

/// A shape of `target` from a delta per corner (`positions`, and `normals`
/// when the file has them, else worked out from the moved triangles),
/// keeping only the corners it moves. `None` if it moves none.
pub(super) fn sparse(
    target: u16,
    primitive_positions: &[[f32; 3]],
    primitive_normals: &[[f32; 3]],
    indices: &[u32],
    positions: &[Vec3],
    normals: Option<&[Vec3]>,
) -> Option<PrimitiveMorph> {
    let mut morph = PrimitiveMorph {
        target,
        ..PrimitiveMorph::default()
    };
    for (corner, delta) in positions.iter().enumerate() {
        let normal = normals.and_then(|normals| normals.get(corner)).copied();
        let moved = delta.is_finite() && delta.length_squared() > STILL * STILL;
        let turned = normal.is_some_and(|n| n.is_finite() && n.length_squared() > STILL * STILL);
        if moved || turned {
            morph.corners.push(corner as u32);
            morph.positions.push(
                if delta.is_finite() {
                    *delta
                } else {
                    Vec3::ZERO
                }
                .to_array(),
            );
            morph.normals.push(
                normal
                    .filter(|n| n.is_finite())
                    .unwrap_or(Vec3::ZERO)
                    .to_array(),
            );
        }
    }
    if morph.corners.is_empty() {
        return None;
    }
    if normals.is_none() {
        morph.normals = turned_normals(
            primitive_positions,
            primitive_normals,
            indices,
            &morph.corners,
            &morph.positions,
        );
    }
    Some(morph)
}

/// How the normals of `corners` change when they move by `deltas`, for a
/// file without the shape's normals: each corner's normal turns as much as
/// the triangles around it do. This keeps flat shading flat (a corner's
/// normal is turned, not replaced by a smoothed one) and smooth shading
/// smooth.
fn turned_normals(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
    corners: &[u32],
    deltas: &[[f32; 3]],
) -> Vec<[f32; 3]> {
    let mut moved: HashMap<u32, Vec3> = HashMap::with_capacity(corners.len());
    for (&corner, delta) in corners.iter().zip(deltas) {
        moved.insert(corner, Vec3::from(*delta));
    }
    let at = |corner: u32, morphed: bool| {
        let base = positions
            .get(corner as usize)
            .map_or(Vec3::ZERO, |p| Vec3::from(*p));
        match morphed {
            true => base + moved.get(&corner).copied().unwrap_or(Vec3::ZERO),
            false => base,
        }
    };
    // The summed (area-weighted) normals of the triangles around each
    // moved corner, before and after.
    let mut around: HashMap<u32, (Vec3, Vec3)> = HashMap::with_capacity(corners.len());
    for triangle in indices.as_chunks::<3>().0 {
        if !triangle.iter().any(|corner| moved.contains_key(corner)) {
            continue;
        }
        let face = |morphed: bool| {
            let [a, b, c] = triangle.map(|corner| at(corner, morphed));
            (b - a).cross(c - a)
        };
        let (before, after) = (face(false), face(true));
        for corner in triangle {
            if moved.contains_key(corner) {
                let sum = around.entry(*corner).or_default();
                sum.0 += before;
                sum.1 += after;
            }
        }
    }
    corners
        .iter()
        .map(|corner| {
            let normal = normals
                .get(*corner as usize)
                .map_or(Vec3::Y, |n| Vec3::from(*n));
            let Some(&(before, after)) = around.get(corner) else {
                return [0.0; 3];
            };
            let (Some(before), Some(after)) = (before.try_normalize(), after.try_normalize())
            else {
                return [0.0; 3];
            };
            let turned = Quat::from_rotation_arc(before, after) * normal;
            (turned - normal).to_array()
        })
        .collect()
}

/// Fits a primitive's shapes as its corners were fitted: moves scaled by
/// `scale`, and each normal change worked out again from the corner's
/// `normals` before fitting, scaled by `normal_scale` as the normals were.
pub(super) fn fit(primitive: &mut Primitive, before: &[[f32; 3]], scale: Vec3, normal_scale: Vec3) {
    for morph in &mut primitive.morphs {
        for delta in &mut morph.positions {
            *delta = (Vec3::from(*delta) * scale).to_array();
        }
        for (corner, delta) in morph.corners.iter().zip(&mut morph.normals) {
            let Some(normal) = before.get(*corner as usize).map(|n| Vec3::from(*n)) else {
                continue;
            };
            let fitted = |n: Vec3| (n * normal_scale).normalize_or_zero();
            *delta = (fitted(normal + Vec3::from(*delta)) - fitted(normal)).to_array();
        }
    }
}

/// `morphs` for a copy keeping only the corners `used` (by their places in
/// the original; `remap` gives each original corner's place in the copy,
/// or `u32::MAX` for one left out).
pub(super) fn remapped(morphs: &[PrimitiveMorph], remap: &[u32]) -> Vec<PrimitiveMorph> {
    let mut copies = Vec::new();
    for morph in morphs {
        let mut kept: Vec<(u32, [f32; 3], [f32; 3])> = morph
            .corners
            .iter()
            .zip(&morph.positions)
            .zip(&morph.normals)
            .filter_map(|((&corner, &position), &normal)| {
                let to = *remap.get(corner as usize)?;
                (to != u32::MAX).then_some((to, position, normal))
            })
            .collect();
        if kept.is_empty() {
            continue;
        }
        kept.sort_by_key(|(corner, _, _)| *corner);
        copies.push(PrimitiveMorph {
            target: morph.target,
            corners: kept.iter().map(|k| k.0).collect(),
            positions: kept.iter().map(|k| k.1).collect(),
            normals: kept.iter().map(|k| k.2).collect(),
        });
    }
    copies
}

/// Adds `other`'s shapes to `ours`, its corners moved up by `before`.
pub(super) fn append(ours: &mut Vec<PrimitiveMorph>, theirs: Vec<PrimitiveMorph>, before: u32) {
    for morph in theirs {
        let shifted = morph.corners.iter().map(|corner| corner + before);
        match ours.iter_mut().find(|own| own.target == morph.target) {
            Some(own) => {
                own.corners.extend(shifted);
                own.positions.extend(morph.positions);
                own.normals.extend(morph.normals);
            }
            None => ours.push(PrimitiveMorph {
                target: morph.target,
                corners: shifted.collect(),
                positions: morph.positions,
                normals: morph.normals,
            }),
        }
    }
}

/// Moves `positions` and `normals` (a primitive's corners) by its
/// `morphs` at `weights` (one per model target; a missing one is 0).
/// Normals are made one unit long again.
pub fn apply(
    morphs: &[PrimitiveMorph],
    weights: &[f32],
    positions: &mut [[f32; 3]],
    normals: &mut [[f32; 3]],
) {
    let mut touched = false;
    for morph in morphs {
        let weight = weights
            .get(usize::from(morph.target))
            .copied()
            .unwrap_or(0.0);
        if weight == 0.0 || !weight.is_finite() {
            continue;
        }
        touched = true;
        for ((&corner, delta), normal_delta) in morph
            .corners
            .iter()
            .zip(&morph.positions)
            .zip(&morph.normals)
        {
            let corner = corner as usize;
            if let Some(position) = positions.get_mut(corner) {
                *position = (Vec3::from(*position) + Vec3::from(*delta) * weight).to_array();
            }
            if let Some(normal) = normals.get_mut(corner) {
                *normal = (Vec3::from(*normal) + Vec3::from(*normal_delta) * weight).to_array();
            }
        }
    }
    if touched {
        for normal in normals {
            *normal = Vec3::from(*normal).normalize_or_zero().to_array();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One quad (two triangles) in the XY plane facing +Z.
    fn quad() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        (
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![[0.0, 0.0, 1.0]; 4],
            vec![0, 1, 2, 0, 2, 3],
        )
    }

    #[test]
    fn only_the_corners_a_shape_moves_are_kept() {
        let (positions, normals, indices) = quad();
        let deltas = [Vec3::ZERO, Vec3::ZERO, Vec3::new(0.0, 0.0, 0.5), Vec3::ZERO];
        let morph = sparse(3, &positions, &normals, &indices, &deltas, None).unwrap();
        assert_eq!(morph.target, 3);
        assert_eq!(morph.corners, vec![2]);
        assert_eq!(morph.positions, vec![[0.0, 0.0, 0.5]]);
        // Lifting a corner tips the triangles around it, so its normal turns
        // away from +Z.
        assert!(
            Vec3::from(morph.normals[0]).length() > 0.05,
            "{:?}",
            morph.normals
        );
        let still = [Vec3::ZERO; 4];
        assert!(sparse(0, &positions, &normals, &indices, &still, None).is_none());
    }

    #[test]
    fn applying_a_shape_moves_its_corners_by_their_weight() {
        let (mut positions, mut normals, indices) = quad();
        let deltas = [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO, Vec3::ZERO];
        let morph = sparse(0, &positions, &normals, &indices, &deltas, None).unwrap();
        apply(&[morph], &[0.5], &mut positions, &mut normals);
        assert_eq!(positions[1], [1.5, 0.0, 0.0]);
        assert_eq!(positions[0], [0.0, 0.0, 0.0]);
        // Stretching in the plane doesn't tip the normals.
        assert!((Vec3::from(normals[1]) - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn a_copy_keeps_the_moves_of_the_corners_it_keeps() {
        let morph = PrimitiveMorph {
            target: 1,
            corners: vec![0, 2, 3],
            positions: vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
            normals: vec![[0.0; 3]; 3],
        };
        // Corner 3 becomes 0, corner 0 becomes 1, corner 2 is left out.
        let remap = [1, u32::MAX, u32::MAX, 0];
        let copy = remapped(&[morph], &remap);
        assert_eq!(copy[0].corners, vec![0, 1]);
        assert_eq!(copy[0].positions, vec![[3.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
    }

    #[test]
    fn shapes_of_one_name_are_one_target() {
        let mut names = MorphNames::default();
        assert_eq!(names.target("Blink_Left", 0.0), Some(0));
        assert_eq!(names.target("Smile", 0.25), Some(1));
        assert_eq!(names.target("Blink_Left", 1.0), Some(0));
        assert_eq!(names.targets[1].default_weight, 0.25);
        assert_eq!(names.targets.len(), 2);
    }
}
