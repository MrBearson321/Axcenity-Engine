//! The Model Studio tab (Phase 55): opening a `.glb`, `.gltf` or `.fbx`
//! (or making a New shape), editing its meshes in Object and Edit mode, and
//! saving it as a `.glb` that MeshParts using it reload.
//!
//! - **Object mode** picks whole objects; G, R and S move them.
//! - **Edit mode** (Tab) picks the active object's vertices, edges or faces
//!   (1, 2, 3): click, Shift-click, box (drag), loop (Alt-click), linked
//!   (L), all (A), grow and shrink (Ctrl + and Ctrl −).
//! - G, R and S (`modal.rs`) or the editor's handles (Move, Rotate and
//!   Scale in the Studio's toolbar) move what's picked, with the toolbar's
//!   snapping and Global or Local axes, round the pivot, and with
//!   proportional editing (O) moving what's near too (`proportional.rs`).
//! - Modeling operations (Phase 56: E, I, Ctrl+B, Ctrl+R, K, M, F and the
//!   Mesh menu; `tools.rs`) run on what's picked as one undo step each;
//!   the adjust panel runs the last one again with new settings in place
//!   of it.
//! - Modifiers (Phase 57; `modifiers.rs`): a stack per object, drawn on
//!   the mesh in the view and saved on it in the `.glb`; the mesh and the
//!   stack go in the model's sidecar.
//! - UVs (Phase 58; `uv_editor.rs`): seams, unwrapping and projections
//!   (U, or the toolbar's UV menu), and the UV editor pane beside the
//!   view, with a checker drawn on the uvs in the view.
//! - Paint mode (Phase 59; `paint.rs`): the active object's material maps
//!   painted in layers, in the view or the UV editor, saved as PNGs.
//! - Sculpt mode (Phase 60; `sculpt.rs`): brushes push the active
//!   object's surface about, with a mask, symmetry, Voxel Remesh and
//!   Subdivide; the view draws it in chunks, sending only touched ones to
//!   the GPU again.
//! - Retopology and baking (Phase 61; `retopo.rs`): a reference object
//!   to snap to and build quads on (Poly Build), Make Low Poly, and Bake
//!   (normal, occlusion, curvature and base color maps onto the active
//!   object's uvs).
//! - Rigging (Phase 62; `rig.rs`): Rig mode edits the armature (the
//!   Humanoid template, bones added, extruded, parented, rolled and
//!   mirrored) and skins objects to it; Weights mode paints the weights;
//!   Pose mode turns bones to test them. Save writes a skinned `.glb`.
//!
//! The Studio keeps its own undo history while the mouse is over it; each
//! edit is one step. Save writes the file and records it as one step of the
//! editor's history (`app/model_studio.rs`). The 3D view, picking and the
//! overlay are `view.rs`.

mod modal;
mod modifiers;
pub(crate) mod paint;
mod proportional;
pub(crate) mod retopo;
pub(crate) mod rig;
pub(crate) mod sculpt;
mod tools;
pub(crate) mod uv_editor;
mod view;

pub use modal::{Modal, ModalKind};
pub use paint::PaintState;
pub use proportional::{Falloff, Pivot, Proportional};
pub use retopo::RetopoState;
pub use rig::RigState;
pub use sculpt::SculptState;
pub use uv_editor::UvEditor;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use axcenity_mesh_edit::{
    MeshOp, ModelDoc, PrimitiveSettings, PrimitiveShape, Purpose, Reference, SelectMode, Selection,
    apply,
};
use axcenity_render::glam::{Mat4, Vec3};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::gizmo::{MoveAxes, Snap, Tool};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// Object mode, Edit mode, Paint, Sculpt, Rig, Weights or Pose mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Object,
    Edit,
    /// Texture painting on the active object (Phase 59).
    Paint,
    /// Sculpting the active object (Phase 60).
    Sculpt,
    /// Editing the model's armature and skinning objects to it (Phase 62).
    Armature,
    /// Painting the active object's weights to the active bone.
    Weights,
    /// Turning bones to test the deformation (not saved).
    Pose,
}

/// What each of the toolbar's modes is for, in the switch's order.
const MODE_TIPS: [&str; 5] = [
    "Object: pick, move, turn and scale whole objects (Tab goes to Edit)",
    "Edit: change the active object's shape, its vertices, edges and faces (Tab)",
    "Paint: paint colour and surface maps onto the active object",
    "Sculpt: push, pull and smooth the surface like clay",
    "Rig: give the model bones so it can be animated, paint how much each bone moves, and test a pose",
];

/// What each of Rig's steps is for.
const RIG_TIPS: [&str; 3] = [
    "Bones: add and place bones (Add Humanoid fits a ready-made skeleton), then give the mesh weights",
    "Weights: paint how strongly the picked bone moves each part of the mesh",
    "Pose: turn bones to test how the mesh bends (the pose isn't saved)",
];

/// What the tab asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StudioAction {
    /// Open the model file at this project path.
    Open(String),
    /// Save the open model.
    Save,
    /// Read this project image as the paint brush's image.
    BrushImage(String),
    /// Bake the reference onto the active object and write the maps.
    Bake,
}

/// Everything an undo step puts back.
#[derive(Clone, Debug)]
struct Snapshot {
    model: ModelDoc,
    mode: Mode,
    active: usize,
    objects: BTreeSet<usize>,
    selection: Selection,
}

/// The last modeling operation run, which the adjust panel can run again
/// with other settings, from the state before it.
#[derive(Clone, Debug)]
pub struct LastOp {
    pub op: MeshOp,
    before: Snapshot,
    /// Each object it ran on, and what was picked of it.
    targets: Vec<(usize, Selection)>,
}

/// A model open in the Studio.
pub struct StudioDoc {
    /// The project path Save writes (a `.glb`).
    pub path: String,
    pub model: ModelDoc,
    pub mode: Mode,
    /// The object Edit mode edits (an index into `model.objects`; one with
    /// a mesh).
    pub active: usize,
    /// The objects picked in Object mode.
    pub objects: BTreeSet<usize>,
    /// What's picked of the active object in Edit mode.
    pub selection: Selection,
    undo: Vec<(String, Snapshot)>,
    redo: Vec<(String, Snapshot)>,
    /// Goes up with every change to the meshes, so the view knows to draw
    /// them again.
    pub revision: u64,
    /// Goes up as a stroke paints, so the view shows the materials again
    /// (the meshes are the same).
    pub paint_revision: u64,
    /// The revision last saved.
    saved: u64,
    /// The last operation, until anything else is done.
    pub last_op: Option<LastOp>,
    /// The object and undo step a modifier change under way goes into.
    modifier_edit: Option<(usize, usize)>,
    /// The high-poly object retopology builds on and bakes from (Phase
    /// 61). Not an undo step.
    pub reference: Option<usize>,
    /// What Edit mode's moves and operations snap to, if anything: set by
    /// the Studio before each (`ModelStudio::refresh_snap`).
    pub(crate) snap: Option<Arc<Reference>>,
}

impl StudioDoc {
    /// `model`, to be saved at `path`. `saved` says whether the file
    /// already holds it.
    pub fn new(path: String, model: ModelDoc, saved: bool) -> StudioDoc {
        let active = model
            .objects
            .iter()
            .position(|object| object.mesh.is_some())
            .unwrap_or(0);
        StudioDoc {
            path,
            model,
            mode: Mode::Object,
            active,
            objects: BTreeSet::from([active]),
            selection: Selection::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 1,
            paint_revision: 0,
            saved: if saved { 1 } else { 0 },
            last_op: None,
            modifier_edit: None,
            reference: None,
            snap: None,
        }
    }

    pub fn unsaved(&self) -> bool {
        self.revision != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.revision;
    }

    /// Its file's name, for the tab.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            model: self.model.clone(),
            mode: self.mode,
            active: self.active,
            objects: self.objects.clone(),
            selection: self.selection.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.model = snapshot.model;
        self.mode = snapshot.mode;
        self.active = snapshot.active;
        self.objects = snapshot.objects;
        self.selection = snapshot.selection;
        self.revision += 1;
    }

    /// Records `before` (taken with [`snapshot`](Self::snapshot) before an
    /// edit) as an undo step named `label`.
    fn record(&mut self, label: &str, before: Snapshot) {
        self.undo.push((label.to_owned(), before));
        self.redo.clear();
        self.revision += 1;
        self.last_op = None;
    }

    /// Runs `op` on what's picked, as one undo step: in Edit mode on the
    /// active object's picked parts; in Object mode (shading only) on every
    /// face of each picked object.
    pub fn run(&mut self, op: MeshOp) -> Result<(), String> {
        let targets: Vec<(usize, Selection)> = match self.mode {
            Mode::Edit => vec![(self.active, self.selection.clone())],
            Mode::Object
            | Mode::Paint
            | Mode::Sculpt
            | Mode::Armature
            | Mode::Weights
            | Mode::Pose => self
                .objects
                .iter()
                .filter_map(|&object| {
                    let mesh = self.model.objects.get(object)?.mesh.as_ref()?;
                    let mut all = Selection {
                        mode: SelectMode::Face,
                        ..Selection::default()
                    };
                    all.faces = (0..mesh.face_count() as u32).collect();
                    Some((object, all))
                })
                .collect(),
        };
        if targets.is_empty() {
            return Err("Pick something to run it on".to_owned());
        }
        let shading = matches!(
            op,
            MeshOp::ShadeSmooth | MeshOp::ShadeFlat | MeshOp::AutoSmooth { .. }
        );
        if self.mode != Mode::Edit && !shading {
            return Err(format!("{} works in Edit mode (Tab)", op.label()));
        }
        let before = self.snapshot();
        self.perform(&op, &before, &targets)?;
        self.record(op.label(), before.clone());
        self.last_op = Some(LastOp {
            op,
            before,
            targets,
        });
        Ok(())
    }

    /// Runs the last operation again with `op`'s settings, in place of how
    /// it ran: no new undo step. On an error the last result stays.
    pub fn adjust(&mut self, op: MeshOp) -> Result<(), String> {
        let Some(last) = self.last_op.take() else {
            return Err("No operation to adjust".to_owned());
        };
        let result = self.perform(&op, &last.before, &last.targets);
        self.last_op = Some(LastOp {
            op: if result.is_ok() { op } else { last.op },
            ..last
        });
        result
    }

    /// Takes the last operation back without a step to redo (a drag that
    /// was cancelled).
    pub fn cancel_last(&mut self) {
        if self.last_op.take().is_some()
            && let Some((_, before)) = self.undo.pop()
        {
            self.restore(before);
        }
    }

    /// `op` on each target's mesh as it was in `before`, put in place.
    fn perform(
        &mut self,
        op: &MeshOp,
        before: &Snapshot,
        targets: &[(usize, Selection)],
    ) -> Result<(), String> {
        let mut results = Vec::new();
        for (object, selection) in targets {
            let Some(mesh) = before
                .model
                .objects
                .get(*object)
                .and_then(|o| o.mesh.as_ref())
            else {
                continue;
            };
            let (mut result, picked) = apply(mesh, selection, op)?;
            if let Some(surface) = &self.snap {
                snap_new_vertices(mesh, &mut result, surface);
            }
            results.push((*object, (result, picked)));
        }
        for (object, (mesh, selection)) in results {
            self.model.objects[object].mesh = Some(mesh);
            if before.mode == Mode::Edit {
                self.selection = selection;
            }
        }
        self.revision += 1;
        Ok(())
    }

    /// Puts the last edit back. False if there's none.
    pub fn undo(&mut self) -> bool {
        let Some((label, before)) = self.undo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.restore(before);
        self.redo.push((label, now));
        self.last_op = None;
        self.modifier_edit = None;
        true
    }

    /// Does the last undone edit again. False if there's none.
    pub fn redo(&mut self) -> bool {
        let Some((label, after)) = self.redo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.restore(after);
        self.undo.push((label, now));
        self.last_op = None;
        self.modifier_edit = None;
        true
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(label, _)| label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(label, _)| label.as_str())
    }

    /// The active object's mesh.
    pub fn mesh(&self) -> Option<&axcenity_mesh_edit::EditMesh> {
        self.model.objects.get(self.active)?.mesh.as_ref()
    }

    /// Tab: into Edit mode on the active object, or back to Object mode.
    pub fn toggle_mode(&mut self) {
        self.last_op = None;
        self.mode = match self.mode {
            Mode::Object if self.mesh().is_some() => Mode::Edit,
            _ => Mode::Object,
        };
    }

    /// Into `mode`, unless [`refuses`](Self::refuses) says why not.
    pub fn set_mode(&mut self, mode: Mode) {
        if self.refuses(mode).is_none() {
            self.last_op = None;
            self.mode = mode;
        }
    }

    /// Why `mode` can't be entered now, in words for the toolbar, or
    /// `None` if it can. Edit, Paint, Sculpt and Weights mode need the
    /// active object to have a mesh; Weights and Pose mode need bones.
    pub fn refuses(&self, mode: Mode) -> Option<&'static str> {
        let bones = self
            .model
            .armature
            .as_ref()
            .is_some_and(|arm| !arm.bones.is_empty());
        match mode {
            Mode::Object | Mode::Armature => None,
            Mode::Edit | Mode::Paint | Mode::Sculpt | Mode::Weights if self.mesh().is_none() => {
                Some("Pick an object with a mesh first (a group has none).")
            }
            Mode::Weights | Mode::Pose if !bones => {
                Some("Add bones first: in Bones, click Add Humanoid or Add Bone.")
            }
            _ => None,
        }
    }

    /// 1, 2 or 3: what Edit mode picks, keeping what's covered.
    pub fn set_select_mode(&mut self, mode: SelectMode) {
        let Some(mesh) = self
            .model
            .objects
            .get(self.active)
            .and_then(|o| o.mesh.as_ref())
        else {
            return;
        };
        self.selection.convert(mesh, mode);
    }

    /// Makes `object` the active one and picks it (or with `extend` adds
    /// or takes it off).
    pub fn click_object(&mut self, object: Option<usize>, extend: bool) {
        match (object, extend) {
            (Some(object), true) => {
                if !self.objects.remove(&object) {
                    self.objects.insert(object);
                }
            }
            (Some(object), false) => self.objects = BTreeSet::from([object]),
            (None, false) => self.objects.clear(),
            (None, true) => {}
        }
        if let Some(object) = object
            && self.objects.contains(&object)
            && self.model.objects[object].mesh.is_some()
            && object != self.active
        {
            self.active = object;
            self.selection.clear();
        }
    }

    /// A selection command on the active mesh (picking, loops, linked,
    /// grow, shrink), which isn't an undo step.
    pub fn select(&mut self, change: impl FnOnce(&mut Selection, &axcenity_mesh_edit::EditMesh)) {
        let Some(mesh) = self
            .model
            .objects
            .get(self.active)
            .and_then(|o| o.mesh.as_ref())
        else {
            return;
        };
        change(&mut self.selection, mesh);
        self.last_op = None;
    }

    /// What a move moves: each object and its vertices.
    pub fn targets(&self) -> Vec<(usize, Vec<u32>)> {
        match self.mode {
            Mode::Edit => self
                .mesh()
                .map(|mesh| vec![(self.active, self.selection.vertices_of(mesh))])
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, vertices)| !vertices.is_empty())
                .collect(),
            Mode::Object
            | Mode::Paint
            | Mode::Sculpt
            | Mode::Armature
            | Mode::Weights
            | Mode::Pose => self
                .objects
                .iter()
                .filter_map(|&object| {
                    let mesh = self.model.objects.get(object)?.mesh.as_ref()?;
                    Some((object, (0..mesh.vertex_count() as u32).collect()))
                })
                .collect(),
        }
    }

    /// The middle of what a move moves, and its own axes' Y (the picked
    /// faces' normal in Edit mode).
    pub fn pivot(&self) -> Option<(Vec3, Vec3)> {
        if self.mode == Mode::Edit {
            let mesh = self.mesh()?;
            return Some((self.selection.center(mesh)?, self.selection.normal(mesh)));
        }
        let (sum, count) = self
            .targets()
            .iter()
            .flat_map(|(object, vertices)| {
                let mesh = self.model.objects[*object].mesh.as_ref();
                vertices
                    .iter()
                    .filter_map(move |&v| Some(mesh?.position(v)))
            })
            .fold((Vec3::ZERO, 0), |(sum, count), p| (sum + p, count + 1));
        (count > 0).then(|| (sum / count as f32, Vec3::ZERO))
    }

    /// The positions of `targets` now, to move from or put back.
    fn positions_of(&self, targets: &[(usize, Vec<u32>)]) -> Vec<Vec<Vec3>> {
        targets
            .iter()
            .map(|(object, vertices)| {
                let mesh = self.model.objects[*object].mesh.as_ref();
                vertices
                    .iter()
                    .map(|&v| mesh.map_or(Vec3::ZERO, |mesh| mesh.position(v)))
                    .collect()
            })
            .collect()
    }

    /// Puts `targets` at `positions` (one each).
    fn set_points(&mut self, targets: &[(usize, Vec<u32>)], positions: &[Vec<Vec3>]) {
        for ((object, vertices), positions) in targets.iter().zip(positions) {
            if let Some(mesh) = self.model.objects[*object].mesh.as_mut() {
                mesh.set_positions(vertices, positions);
            }
        }
        self.revision += 1;
    }

    /// Moves what's picked by `matrix` at once, as one undo step named
    /// `label`.
    #[cfg(test)]
    pub fn transform(&mut self, label: &str, matrix: Mat4) -> bool {
        let targets = self.targets();
        if targets.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let moved: Vec<Vec<Vec3>> = self
            .positions_of(&targets)
            .into_iter()
            .map(|start| start.iter().map(|&p| matrix.transform_point3(p)).collect())
            .collect();
        self.set_points(&targets, &moved);
        self.record(label, before);
        true
    }
}

/// A move being dragged (a handle or a key's): what it moves, from where,
/// and the state to undo to.
struct Moving {
    targets: Vec<(usize, Vec<u32>)>,
    start: Vec<Vec<Vec3>>,
    /// Each vertex's distance from the nearest picked one (0 for picked
    /// ones; others are there for proportional editing).
    distances: Vec<Vec<f32>>,
    /// Each vertex's own pivot, with Individual Origins.
    origins: Vec<Vec<Option<Vec3>>>,
    /// The move's pivot.
    center: Vec3,
    before: Snapshot,
    /// The surface what moves sticks to (Snap to surface, Phase 61).
    snap: Option<Arc<Reference>>,
}

impl Moving {
    /// What a move of `doc`'s picked parts round `center` moves: with
    /// proportional editing on (in Edit mode), every vertex, by its
    /// distance.
    fn begin(doc: &StudioDoc, center: Vec3, how: &MoveSettings) -> Option<Moving> {
        let picked = doc.targets();
        if picked.is_empty() {
            return None;
        }
        let mut targets = Vec::new();
        let mut distances = Vec::new();
        let mut origins = Vec::new();
        for (object, vertices) in picked {
            let Some(mesh) = doc.model.objects[object].mesh.as_ref() else {
                continue;
            };
            let near: Vec<(u32, f32)> = if doc.mode == Mode::Edit && how.proportional.on {
                let mut near: Vec<(u32, f32)> =
                    proportional::distances(mesh, &vertices, f32::INFINITY)
                        .into_iter()
                        .collect();
                near.sort_by_key(|&(v, _)| v);
                near
            } else {
                vertices.iter().map(|&v| (v, 0.0)).collect()
            };
            let own: HashMap<u32, Vec3> = match (how.pivot, doc.mode) {
                (Pivot::Individual, Mode::Edit) => proportional::origins(mesh, &doc.selection),
                (Pivot::Individual, Mode::Object) => {
                    let middle = vertices.iter().map(|&v| mesh.position(v)).sum::<Vec3>()
                        / vertices.len().max(1) as f32;
                    vertices.iter().map(|&v| (v, middle)).collect()
                }
                _ => HashMap::new(),
            };
            origins.push(near.iter().map(|(v, _)| own.get(v).copied()).collect());
            distances.push(near.iter().map(|&(_, d)| d).collect());
            targets.push((object, near.into_iter().map(|(v, _)| v).collect()));
        }
        Some(Moving {
            start: doc.positions_of(&targets),
            targets,
            distances,
            origins,
            center,
            before: doc.snapshot(),
            snap: doc.snap.clone(),
        })
    }

    /// Shows the move as `matrix` takes it, each vertex as much of the way
    /// as proportional editing says. In Edit mode a clipping Mirror keeps
    /// vertices from crossing its plane.
    fn update(&self, doc: &mut StudioDoc, matrix: Mat4, proportional: &Proportional) {
        let mut moved: Vec<Vec<Vec3>> = self
            .start
            .iter()
            .zip(&self.distances)
            .zip(&self.origins)
            .map(|((start, distances), origins)| {
                start
                    .iter()
                    .zip(distances)
                    .zip(origins)
                    .map(|((&p, &distance), origin)| {
                        let weight = if proportional.on {
                            proportional.falloff.weight(distance, proportional.radius)
                        } else if distance == 0.0 {
                            1.0
                        } else {
                            0.0
                        };
                        let (matrix, center) = match origin {
                            Some(origin) => {
                                (proportional::around(matrix, self.center, *origin), *origin)
                            }
                            None => (matrix, self.center),
                        };
                        proportional::partial(matrix, weight, center).transform_point3(p)
                    })
                    .collect()
            })
            .collect();
        if doc.mode == Mode::Edit {
            for ((object, _), (moved, start)) in
                self.targets.iter().zip(moved.iter_mut().zip(&self.start))
            {
                let stack = &doc.model.objects[*object].modifiers;
                for (point, &from) in moved.iter_mut().zip(start) {
                    if let Some(surface) = &self.snap
                        && *point != from
                    {
                        *point = surface.snap(*point);
                    }
                    *point = axcenity_mesh_edit::clip(stack, from, *point);
                }
            }
        }
        doc.set_points(&self.targets, &moved);
    }

    /// Keeps the move as one undo step named `label`.
    fn finish(self, doc: &mut StudioDoc, label: &str) {
        doc.record(label, self.before);
    }

    /// Puts everything back where it started.
    fn cancel(self, doc: &mut StudioDoc) {
        doc.set_points(&self.targets, &self.start);
    }
}

/// How moves move: the pivot, the 3D cursor and proportional editing.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveSettings {
    pub pivot: Pivot,
    pub cursor: Vec3,
    pub proportional: Proportional,
}

impl MoveSettings {
    /// The point a move of `doc`'s picked parts goes round.
    fn center(&self, doc: &StudioDoc) -> Option<Vec3> {
        let (median, _) = doc.pivot()?;
        Some(match self.pivot {
            Pivot::Median | Pivot::Individual => median,
            Pivot::Cursor => self.cursor,
            Pivot::BoundingBox => {
                let (min, max) = doc
                    .targets()
                    .iter()
                    .flat_map(|(object, vertices)| {
                        let mesh = doc.model.objects[*object].mesh.as_ref();
                        vertices
                            .iter()
                            .filter_map(move |&v| Some(mesh?.position(v)))
                    })
                    .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
                        (min.min(p), max.max(p))
                    });
                (min + max) / 2.0
            }
        })
    }
}

/// The Model Studio tab.
pub struct ModelStudio {
    pub doc: Option<StudioDoc>,
    /// The shape and sizes New makes.
    pub new_settings: PrimitiveSettings,
    /// Which handles the view shows (Select shows none).
    pub tool: Tool,
    /// Global or Local axes, for the handles.
    pub axes: MoveAxes,
    /// Draw each face's normal.
    pub show_normals: bool,
    /// The pivot, the 3D cursor and proportional editing.
    pub moves: MoveSettings,
    /// The UV editor pane and the uv tools' settings.
    pub uv: UvEditor,
    /// Paint mode's brush, map, layer and strokes.
    pub paint: PaintState,
    /// Sculpt mode's brush, the sculpt and its strokes.
    pub sculpt: SculptState,
    /// The reference, Poly Build, Make Low Poly and Bake (Phase 61).
    pub retopo: RetopoState,
    /// The active bone, the weight brush and the pose tested (Phase 62).
    pub rig: RigState,
    /// The project's images, for a paint brush's image.
    pub images: Vec<String>,
    /// A keyboard move under way, and what it moves.
    modal: Option<(Modal, Moving)>,
    /// The knife or loop cut under way, a value being dragged, or a menu.
    tools: tools::Tools,
    view: view::View,
    /// The mouse was over the tab last frame, so undo and keys are its.
    hovered: bool,
    /// A model waiting to replace one with unsaved changes.
    pending: Option<StudioDoc>,
    /// Why the last mode picked on the toolbar couldn't be entered, and
    /// the mode the Studio stayed in (the note goes once that changes).
    mode_note: Option<(Mode, &'static str)>,
    /// Lines for the Console.
    pub notices: Vec<String>,
    /// The active object's modifiers that couldn't run, worked out for
    /// this revision and object.
    stack_errors: (u64, usize, Vec<(usize, String)>),
}

impl Default for ModelStudio {
    fn default() -> Self {
        Self {
            doc: None,
            new_settings: PrimitiveSettings::default(),
            tool: Tool::Select,
            axes: MoveAxes::Global,
            show_normals: false,
            moves: MoveSettings::default(),
            uv: UvEditor::default(),
            paint: PaintState::default(),
            sculpt: SculptState::default(),
            retopo: RetopoState::default(),
            rig: RigState::default(),
            images: Vec::new(),
            modal: None,
            tools: tools::Tools::default(),
            view: view::View::default(),
            hovered: false,
            pending: None,
            mode_note: None,
            notices: Vec::new(),
            stack_errors: (0, 0, Vec::new()),
        }
    }
}

impl ModelStudio {
    /// True while the mouse is over the tab: undo, redo and the Studio's
    /// keys are its, not the world's.
    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    /// Until the tab is drawn again, the mouse isn't over it.
    pub fn forget_pointer(&mut self) {
        self.hovered = false;
    }

    /// As if the mouse were over the tab (or not), for tests.
    #[cfg(test)]
    pub fn set_pointer(&mut self, over: bool) {
        self.hovered = over;
    }

    /// True while a move is being dragged, which undo mustn't interrupt.
    pub fn is_dragging(&self) -> bool {
        self.modal.is_some()
            || self.view.dragging()
            || self.tools.busy()
            || self.uv.busy()
            || self.paint.busy()
            || self.sculpt.busy()
            || self.rig.busy()
    }

    /// Shows `doc`, unless the open one has unsaved changes: then it waits
    /// for an answer.
    pub fn open(&mut self, doc: StudioDoc) {
        if self.doc.as_ref().is_some_and(StudioDoc::unsaved) {
            self.pending = Some(doc);
        } else {
            self.show_doc(doc);
        }
    }

    fn show_doc(&mut self, doc: StudioDoc) {
        self.modal = None;
        self.tools = tools::Tools::default();
        self.uv.forget();
        self.paint.forget();
        self.sculpt.forget();
        self.retopo.forget();
        self.rig.forget();
        self.view.frame_doc(&doc);
        self.doc = Some(doc);
    }

    /// Undoes the open model's last edit. False if there was none.
    pub fn undo(&mut self) -> bool {
        if self.is_dragging() {
            return true;
        }
        self.doc.as_mut().is_some_and(StudioDoc::undo)
    }

    pub fn redo(&mut self) -> bool {
        if self.is_dragging() {
            return true;
        }
        self.doc.as_mut().is_some_and(StudioDoc::redo)
    }

    /// The tab: its toolbar, object list and 3D view. `models` are the
    /// project's model files, for Open.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        models: &[String],
        snap: Snap,
        editable: bool,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        self.hovered = ui.rect_contains_pointer(ui.max_rect());
        self.pending_bar(ui);
        self.toolbar(ui, models, editable, &mut actions);
        ui.separator();
        if self.doc.is_none() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.weak("Open a model, or make a New shape, to edit it here.");
                ui.weak("Edit in Model Studio on a MeshPart or a model in Assets opens it too.");
            });
            return actions;
        };
        responsive::split(ui, 420.0, 0.28, |ui, pane| {
            if pane == 0 {
                egui::ScrollArea::vertical()
                    .id_salt("studio side")
                    .show(ui, |ui| {
                        self.object_list(ui);
                        ui.add_space(8.0);
                        match self.doc.as_ref().map(|d| d.mode) {
                            Some(Mode::Paint) => self.paint_panel(ui, editable, &mut actions),
                            Some(Mode::Sculpt) => self.sculpt_panel(ui, editable),
                            Some(Mode::Armature | Mode::Weights | Mode::Pose) => {
                                self.rig_panel(ui, editable)
                            }
                            _ => {
                                self.modifier_panel(ui, editable);
                                self.retopo_panel(ui, editable, &mut actions);
                            }
                        }
                    });
            } else if self.uv.open && self.doc.as_ref().is_some_and(|doc| shows_uvs(doc.mode)) {
                self.view_and_uvs(ui, render_state, snap, editable);
            } else {
                self.viewport(ui, render_state, snap, editable);
            }
        });
        actions
    }

    /// Asks before a model replaces one with unsaved changes.
    fn pending_bar(&mut self, ui: &mut egui::Ui) {
        let Some(pending) = &self.pending else {
            return;
        };
        let current = self.doc.as_ref().map_or("", StudioDoc::name).to_owned();
        let palette = theme::palette(ui.ctx());
        let mut answer = None;
        responsive::toolbar(ui, |ui| {
            ui.colored_label(
                palette.warning,
                format!(
                    "{current} has unsaved changes. Open {} anyway?",
                    pending.name()
                ),
            );
            if ui
                .add(widgets::button(
                    Role::Danger,
                    None,
                    "Discard and open",
                    &palette,
                ))
                .clicked()
            {
                answer = Some(true);
            }
            if ui.button("Keep editing").clicked() {
                answer = Some(false);
            }
        });
        match answer {
            Some(true) => {
                let doc = self.pending.take().expect("checked above");
                self.show_doc(doc);
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        models: &[String],
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        responsive::toolbar(ui, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.menu_button(theme::with_icon(icons::PLUS, "New"), |ui| {
                    if self.new_menu(ui) {
                        let doc = StudioDoc::new(
                            String::new(),
                            ModelDoc::primitive(&self.new_settings),
                            false,
                        );
                        self.open(doc);
                        ui.close();
                    }
                });
                ui.menu_button(theme::with_icon(icons::FOLDER_OPEN, "Open"), |ui| {
                    if models.is_empty() {
                        ui.weak("The project has no models yet.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for path in models {
                                if ui.button(path).clicked() {
                                    actions.push(StudioAction::Open(path.clone()));
                                    ui.close();
                                }
                            }
                        });
                });
            });
            let Some(doc) = &mut self.doc else {
                return;
            };
            let label = if doc.unsaved() { "Save*" } else { "Save" };
            if ui
                .add_enabled(
                    editable,
                    widgets::button(Role::Primary, Some(icons::FLOPPY_DISK), label, &palette),
                )
                .on_hover_text(format!(
                    "Write {} as a .glb. MeshParts using it reload.",
                    save_name(doc)
                ))
                .clicked()
            {
                actions.push(StudioAction::Save);
            }
            ui.separator();
            // Five modes; Rig's three steps (bones, weights, a test pose)
            // are a second switch beside it, as Edit's vertex, edge and
            // face are.
            let shown = match doc.mode {
                Mode::Weights | Mode::Pose => Mode::Armature,
                mode => mode,
            };
            let mut picked = shown;
            let (changed, _) = widgets::segmented_tips(
                ui,
                &mut picked,
                &[
                    (Mode::Object, Some(icons::CUBE), "Object"),
                    (Mode::Edit, Some(icons::POLYGON), "Edit"),
                    (Mode::Paint, Some(icons::PAINT_BRUSH), "Paint"),
                    (Mode::Sculpt, Some(icons::HAND_GRABBING), "Sculpt"),
                    (Mode::Armature, Some(icons::BONE), "Rig"),
                ],
                &MODE_TIPS,
            );
            if changed {
                self.mode_note = doc.refuses(picked).map(|why| (doc.mode, why));
                doc.set_mode(picked);
            }
            if matches!(doc.mode, Mode::Armature | Mode::Weights | Mode::Pose) {
                let mut step = doc.mode;
                let (changed, _) = widgets::segmented_tips(
                    ui,
                    &mut step,
                    &[
                        (Mode::Armature, Some(icons::BONE), "Bones"),
                        (Mode::Weights, Some(icons::DROP_HALF), "Weights"),
                        (Mode::Pose, Some(icons::PERSON_SIMPLE_RUN), "Pose"),
                    ],
                    &RIG_TIPS,
                );
                if changed {
                    self.mode_note = doc.refuses(step).map(|why| (doc.mode, why));
                    doc.set_mode(step);
                }
            }
            if doc.mode == Mode::Edit {
                let mut select = doc.selection.mode;
                if widgets::segmented(
                    ui,
                    &mut select,
                    &[
                        (SelectMode::Vertex, Some(icons::DOT_OUTLINE), "Vertex"),
                        (SelectMode::Edge, Some(icons::LINE_SEGMENT), "Edge"),
                        (SelectMode::Face, Some(icons::SELECTION), "Face"),
                    ],
                ) {
                    doc.set_select_mode(select);
                }
            }
            match self.mode_note {
                Some((stayed, note)) if stayed == doc.mode => {
                    ui.colored_label(palette.warning, theme::with_icon(icons::WARNING, note));
                }
                _ => self.mode_note = None,
            }
            // Only what works in this mode: the handles, axes, Mesh menu
            // and pivot are for modeling; Paint keeps the UV tools, and
            // Rig the 3D cursor its Add Bone uses.
            let mode = doc.mode;
            let modeling = matches!(mode, Mode::Object | Mode::Edit);
            if modeling {
                ui.separator();
                for (tool, icon, name) in [
                    (Tool::Select, icons::CURSOR, "Select (no handles)"),
                    (Tool::Move, icons::ARROWS_OUT_CARDINAL, "Move handles (G)"),
                    (Tool::Rotate, icons::ARROWS_CLOCKWISE, "Rotate handles (R)"),
                    (Tool::Scale, icons::ARROWS_OUT, "Scale handles (S)"),
                ] {
                    if widgets::icon_button(ui, icon, name, self.tool == tool).clicked() {
                        self.tool = tool;
                    }
                }
                let mut local = self.axes == MoveAxes::Local;
                if widgets::icon_button(
                    ui,
                    icons::GLOBE,
                    "Local axes: the selection's own (the picked faces' normal is Y)",
                    local,
                )
                .clicked()
                {
                    local = !local;
                    self.axes = if local {
                        MoveAxes::Local
                    } else {
                        MoveAxes::Global
                    };
                }
                if widgets::icon_button(
                    ui,
                    icons::ARROW_UP_RIGHT,
                    "Show normals",
                    self.show_normals,
                )
                .clicked()
                {
                    self.show_normals = !self.show_normals;
                }
            }
            ui.separator();
            let undo = doc
                .undo_label()
                .map(|label| format!("Undo {label} (Ctrl+Z)"));
            if ui
                .add_enabled(undo.is_some(), egui::Button::new(icons::ARROW_U_UP_LEFT))
                .on_hover_text(undo.unwrap_or_else(|| "Nothing to undo".to_owned()))
                .clicked()
            {
                doc.undo();
            }
            let redo = doc
                .redo_label()
                .map(|label| format!("Redo {label} (Ctrl+Y)"));
            if ui
                .add_enabled(redo.is_some(), egui::Button::new(icons::ARROW_U_UP_RIGHT))
                .on_hover_text(redo.unwrap_or_else(|| "Nothing to redo".to_owned()))
                .clicked()
            {
                doc.redo();
            }
            let uvs = shows_uvs(mode);
            if editable && (uvs || mode == Mode::Armature) {
                ui.separator();
                if modeling {
                    self.mesh_menu(ui);
                }
                if uvs {
                    self.uv_menu(ui);
                }
                if modeling || mode == Mode::Armature {
                    self.pivot_menu(ui);
                }
            }
            if uvs
                && widgets::icon_button(
                    ui,
                    icons::SQUARES_FOUR,
                    "UV editor: the active object's uvs beside the view",
                    self.uv.open,
                )
                .clicked()
            {
                self.uv.open = !self.uv.open;
            }
        });
    }

    /// New's menu: the shape and its sizes. True when Create is clicked.
    fn new_menu(&mut self, ui: &mut egui::Ui) -> bool {
        let settings = &mut self.new_settings;
        let mut shape = settings.shape;
        egui::ComboBox::from_id_salt("studio new shape")
            .selected_text(shape.label())
            .show_ui(ui, |ui| {
                for option in PrimitiveShape::ALL {
                    ui.selectable_value(&mut shape, option, option.label());
                }
            });
        if shape != settings.shape {
            *settings = PrimitiveSettings::of(shape);
        }
        let (height, thickness, segments, rings, subdivisions) = shape.uses();
        let width_name = match shape {
            PrimitiveShape::Cube | PrimitiveShape::Plane => "Size",
            _ => "Width",
        };
        egui::Grid::new("studio new sizes")
            .num_columns(2)
            .show(ui, |ui| {
                let row = |ui: &mut egui::Ui, name: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.label(name);
                    add(ui);
                    ui.end_row();
                };
                row(ui, width_name, &mut |ui| {
                    ui.add(
                        egui::DragValue::new(&mut settings.size)
                            .speed(0.05)
                            .range(0.01..=1000.0)
                            .suffix(" studs"),
                    );
                });
                if height {
                    row(ui, "Height", &mut |ui| {
                        ui.add(
                            egui::DragValue::new(&mut settings.height)
                                .speed(0.05)
                                .range(0.01..=1000.0)
                                .suffix(" studs"),
                        );
                    });
                }
                if thickness {
                    row(ui, "Thickness", &mut |ui| {
                        ui.add(
                            egui::DragValue::new(&mut settings.thickness)
                                .speed(0.02)
                                .range(0.01..=1000.0)
                                .suffix(" studs"),
                        );
                    });
                }
                if segments {
                    let name = match shape {
                        PrimitiveShape::Cube | PrimitiveShape::Plane => "Cuts",
                        _ => "Segments",
                    };
                    row(ui, name, &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.segments).range(1..=256));
                    });
                }
                if rings {
                    let name = match shape {
                        PrimitiveShape::Torus => "Ring steps",
                        PrimitiveShape::UvSphere => "Rings",
                        _ => "Height cuts",
                    };
                    row(ui, name, &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.rings).range(1..=256));
                    });
                }
                if subdivisions {
                    row(ui, "Subdivisions", &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.subdivisions).range(0..=5));
                    });
                }
            });
        let palette = theme::palette(ui.ctx());
        ui.add(widgets::button(
            Role::Primary,
            Some(icons::PLUS),
            &format!("Create {}", shape.label()),
            &palette,
        ))
        .clicked()
    }

    /// The file's objects, indented inside their groups. Clicking one picks
    /// it and makes it the one Edit mode edits.
    fn object_list(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &mut self.doc else {
            return;
        };
        widgets::section_label(ui, "Objects");
        let mut clicked = None;
        for (index, object) in doc.model.objects.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(object.depth as f32 * 12.0);
                let icon = if object.mesh.is_some() {
                    icons::CUBE
                } else {
                    icons::FOLDER_OPEN
                };
                let mut text = theme::with_icon(icon, &object.name);
                if index == doc.active && object.mesh.is_some() {
                    text.push_str("  ·  active");
                }
                let mut response = ui.selectable_label(doc.objects.contains(&index), text);
                if doc.model.is_lod_copy(index) {
                    response =
                        response.on_hover_text("A far-away copy: games draw it, this view doesn't");
                }
                if response.clicked() && object.mesh.is_some() {
                    clicked = Some((index, ui.input(|i| i.modifiers.shift)));
                }
            });
        }
        if let Some((index, extend)) = clicked {
            doc.click_object(Some(index), extend);
        }
    }

    /// The active object's modifier stack, and what couldn't run in it.
    fn modifier_panel(&mut self, ui: &mut egui::Ui, editable: bool) {
        let dragging = self.is_dragging();
        let Some(doc) = &mut self.doc else {
            return;
        };
        let (revision, active, _) = &self.stack_errors;
        if !dragging && (*revision != doc.revision || *active != doc.active) {
            let errors = doc
                .model
                .objects
                .get(doc.active)
                .filter(|object| !object.modifiers.is_empty())
                .and_then(|object| {
                    let mesh = object.mesh.as_ref()?;
                    let stack = &object.modifiers;
                    Some(axcenity_mesh_edit::evaluate(mesh, stack, Purpose::Viewport).errors)
                })
                .unwrap_or_default();
            self.stack_errors = (doc.revision, doc.active, errors);
        }
        modifiers::panel(ui, doc, &self.stack_errors.2, editable, &mut self.notices);
    }

    /// The 3D view and what the mouse and keys do in it.
    fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        snap: Snap,
        editable: bool,
    ) {
        let size = ui.available_size().max(egui::vec2(64.0, 64.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let mode = self.doc.as_ref().map(|doc| doc.mode);
        let painting = mode == Some(Mode::Paint);
        let sculpting = mode == Some(Mode::Sculpt);
        let rigging = mode.is_some_and(rig::shows_bones);
        let context = view::Frame {
            rect,
            response: &response,
            snap,
            tool: if editable && matches!(mode, Some(Mode::Object | Mode::Edit)) {
                self.tool
            } else {
                Tool::Select
            },
            axes: self.axes,
            show_normals: self.show_normals,
            cursor: self.moves.cursor,
            checker: self
                .uv
                .checker
                .then_some(self.uv.channel)
                .filter(|_| !painting && !sculpting),
            sculpt: self.sculpt_items(),
        };
        if editable && response.contains_pointer() {
            if painting || sculpting || rigging {
                if painting {
                    self.paint_keys(ui);
                } else if sculpting {
                    self.sculpt_keys(ui);
                } else {
                    let at = ui
                        .input(|input| input.pointer.latest_pos())
                        .unwrap_or(rect.center());
                    self.rig_keys(ui, at);
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                    && let Some(doc) = &mut self.doc
                {
                    doc.toggle_mode();
                }
            } else {
                self.keys(ui, &context);
            }
        }
        let mut extras = self.extras();
        self.retopo_extras(&mut extras);
        if painting {
            extras.hint = self.paint.status.clone();
        }
        if sculpting {
            extras.hint = self.sculpt.status.clone();
        }
        // Weights and Pose mode draw the model skinned.
        match self.rig_model_key() {
            Some(key) if self.view.handed() != Some(key) => {
                let model = self.rig_model();
                self.view.use_model(Some(key), model);
            }
            Some(_) => {}
            None => self.view.use_model(None, None),
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        let modal = self.modal.as_mut().map(|(modal, _)| &*modal);
        let event = self
            .view
            .show(ui, &context, render_state, doc, modal, &extras);
        if rigging {
            self.paint_bones(&ui.painter_at(rect), &event.screen);
            if editable {
                self.rig_in_view(ui, &response, &event.screen);
            }
            return;
        }
        if painting || sculpting {
            if editable && painting {
                self.paint_in_view(ui, &response, &event.screen);
            } else if editable {
                self.sculpt_in_view(ui, &response, &event.screen);
            }
            return;
        }
        self.handle(event, ui, &context);
        if editable {
            self.overlays(ui, rect);
            self.uv_key_menu(ui);
        }
    }

    /// The 3D view and the UV editor, side by side (or one above the
    /// other when the tab is narrow).
    fn view_and_uvs(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        snap: Snap,
        editable: bool,
    ) {
        let size = ui.available_size();
        let wide = size.x >= 520.0;
        let half = if wide {
            egui::vec2((size.x - 8.0) / 2.0, size.y)
        } else {
            egui::vec2(size.x, (size.y - 8.0) / 2.0)
        };
        let layout = if wide {
            egui::Layout::left_to_right(egui::Align::Min)
        } else {
            egui::Layout::top_down(egui::Align::Min)
        };
        ui.with_layout(layout, |ui| {
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(half, top_down, |ui| {
                self.viewport(ui, render_state, snap, editable);
            });
            ui.separator();
            ui.allocate_ui_with_layout(half, top_down, |ui| {
                self.uv_pane(ui, editable);
            });
        });
    }

    /// The Studio's keys, while the mouse is over its view.
    fn keys(&mut self, ui: &mut egui::Ui, frame: &view::Frame) {
        let pointer = ui
            .input(|input| input.pointer.latest_pos())
            .unwrap_or(frame.rect.center());
        if self.tool_keys(ui) || self.poly_build_keys(ui) {
            return;
        }
        if let Some((modal, _)) = &mut self.modal {
            let proportional = self.moves.proportional.on;
            let (grab_again, wider) = ui.input_mut(|input| {
                let grab_again = modal.kind == ModalKind::Grab
                    && input.consume_key(egui::Modifiers::NONE, egui::Key::G);
                // Proportional editing's radius: Page Up and Down, or the
                // wheel (which then doesn't zoom).
                let mut wider = 0.0;
                if proportional {
                    if input.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
                        wider += 1.0;
                    }
                    if input.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
                        wider -= 1.0;
                    }
                    if input.raw_scroll_delta.y != 0.0 {
                        wider += input.raw_scroll_delta.y.signum();
                    }
                    input.raw_scroll_delta = egui::Vec2::ZERO;
                    input.smooth_scroll_delta = egui::Vec2::ZERO;
                }
                (grab_again, wider)
            });
            if grab_again {
                // G G: edge slide in place of the grab.
                self.cancel_modal();
                self.begin_edge_slide(pointer);
                return;
            }
            if wider != 0.0 {
                let radius = &mut self.moves.proportional.radius;
                *radius = (*radius * 1.1_f32.powf(wider)).clamp(0.001, 10_000.0);
            }
            let Some((modal, _)) = &mut self.modal else {
                return;
            };
            let (axes, typed, back, enter, escape) = ui.input_mut(|input| {
                let mut axes = Vec::new();
                for (axis, key) in [egui::Key::X, egui::Key::Y, egui::Key::Z]
                    .into_iter()
                    .enumerate()
                {
                    if input.consume_key(egui::Modifiers::NONE, key) {
                        axes.push(axis);
                    }
                }
                let typed: Vec<char> = input
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::Text(text) => Some(text.chars().collect::<Vec<_>>()),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                (
                    axes,
                    typed,
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            });
            for axis in axes {
                modal.lock = modal.lock.press(axis);
            }
            for c in typed {
                modal.type_char(c);
            }
            if back {
                modal.typed.pop();
            }
            if enter {
                self.finish_modal();
            } else if escape {
                self.cancel_modal();
            }
            return;
        }
        if self.uv_keys(ui, pointer) || self.op_keys(ui, pointer) {
            return;
        }
        let none = egui::Modifiers::NONE;
        let command = egui::Modifiers::COMMAND;
        let pressed = ui.input_mut(|input| StudioKeys {
            tab: input.consume_key(none, egui::Key::Tab),
            modes: [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3]
                .map(|key| input.consume_key(none, key)),
            grab: input.consume_key(none, egui::Key::G),
            rotate: input.consume_key(none, egui::Key::R),
            scale: input.consume_key(none, egui::Key::S),
            linked: input.consume_key(none, egui::Key::L),
            all: input.consume_key(none, egui::Key::A),
            grow: input.consume_key(command, egui::Key::Plus)
                || input.consume_key(command, egui::Key::Equals),
            shrink: input.consume_key(command, egui::Key::Minus),
        });
        let Some(doc) = &mut self.doc else {
            return;
        };
        if pressed.tab {
            doc.toggle_mode();
        }
        if doc.mode == Mode::Edit {
            for (pressed, mode) in pressed.modes.into_iter().zip([
                SelectMode::Vertex,
                SelectMode::Edge,
                SelectMode::Face,
            ]) {
                if pressed {
                    doc.set_select_mode(mode);
                }
            }
            if pressed.linked {
                doc.select(|selection, mesh| selection.linked(mesh));
            }
            if pressed.all {
                doc.select(|selection, mesh| selection.all(mesh));
            }
            if pressed.grow {
                doc.select(|selection, mesh| selection.grow(mesh));
            }
            if pressed.shrink {
                doc.select(|selection, mesh| selection.shrink(mesh));
            }
        } else if pressed.all {
            doc.objects = (0..doc.model.objects.len())
                .filter(|&k| doc.model.objects[k].mesh.is_some())
                .collect();
        }
        let kind = [
            (pressed.grab, ModalKind::Grab),
            (pressed.rotate, ModalKind::Rotate),
            (pressed.scale, ModalKind::Scale),
        ]
        .into_iter()
        .find_map(|(pressed, kind)| pressed.then_some(kind));
        if let Some(kind) = kind {
            self.begin_modal(kind, pointer);
        }
    }

    /// Starts a keyboard move of what's picked, from the mouse at `at`.
    pub fn begin_modal(&mut self, kind: ModalKind, at: egui::Pos2) -> bool {
        self.refresh_snap();
        let Some(doc) = &self.doc else {
            return false;
        };
        let Some((_, normal)) = doc.pivot() else {
            return false;
        };
        let Some(center) = self.moves.center(doc) else {
            return false;
        };
        let Some(moving) = Moving::begin(doc, center, &self.moves) else {
            return false;
        };
        self.modal = Some((
            Modal::new(kind, at, center, modal::frame_of(normal)),
            moving,
        ));
        true
    }

    /// Shows the keyboard move with the mouse at `mouse`.
    pub fn update_modal(&mut self, screen: &crate::screen::Screen, mouse: egui::Pos2, snap: &Snap) {
        let (Some((modal, moving)), Some(doc)) = (&self.modal, &mut self.doc) else {
            return;
        };
        moving.update(
            doc,
            modal.matrix(screen, mouse, snap),
            &self.moves.proportional,
        );
    }

    /// Keeps the keyboard move as one undo step.
    pub fn finish_modal(&mut self) {
        if let (Some((modal, moving)), Some(doc)) = (self.modal.take(), &mut self.doc) {
            moving.finish(doc, modal.kind.label());
        }
    }

    /// Puts back what the keyboard move moved.
    pub fn cancel_modal(&mut self) {
        if let (Some((_, moving)), Some(doc)) = (self.modal.take(), &mut self.doc) {
            moving.cancel(doc);
        }
    }

    /// Acts on what happened in the view.
    fn handle(&mut self, event: view::Event, ui: &egui::Ui, frame: &view::Frame) {
        if self.handle_tools(&event) || self.handle_poly_build(&event) {
            return;
        }
        let screen = event.screen;
        if self.modal.is_some() {
            if let Some(mouse) = ui.input(|input| input.pointer.latest_pos()) {
                let snap = if ui.input(|input| input.modifiers.command) {
                    frame.snap.flipped()
                } else {
                    frame.snap
                };
                self.update_modal(&screen, mouse, &snap);
            }
            if event.clicked {
                self.finish_modal();
            } else if event.secondary_clicked {
                self.cancel_modal();
            }
            return;
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        match event.action {
            view::ViewAction::None => {}
            view::ViewAction::PickObject { object, extend } => doc.click_object(object, extend),
            view::ViewAction::Pick { element, extend } => {
                doc.select(|selection, _| selection.click(element, extend));
            }
            view::ViewAction::Loop { edge, extend } => {
                doc.select(|selection, mesh| selection.select_loop(mesh, edge, extend));
            }
            view::ViewAction::Box { elements, extend } => doc.select(|selection, _| {
                let mode = selection.mode;
                let set = selection.set_mut(mode);
                if !extend {
                    set.clear();
                }
                set.extend(elements);
            }),
            view::ViewAction::BeginHandle => {
                self.refresh_snap();
                let Some(doc) = &mut self.doc else {
                    return;
                };
                // The handles go round the middle of their box.
                let center = view::handle_pose(doc, self.axes).map(|pose| pose.position);
                self.view.moving =
                    center.and_then(|center| Moving::begin(doc, center, &self.moves));
            }
            view::ViewAction::Handle(matrix) => {
                if let Some(moving) = &self.view.moving {
                    moving.update(doc, matrix, &self.moves.proportional);
                }
            }
            view::ViewAction::Cursor(at) => self.moves.cursor = at,
            view::ViewAction::EndHandle(label) => {
                if let Some(moving) = self.view.moving.take() {
                    moving.finish(doc, label);
                }
            }
        }
    }
}

/// The Studio keys pressed this frame.
struct StudioKeys {
    tab: bool,
    modes: [bool; 3],
    grab: bool,
    rotate: bool,
    scale: bool,
    linked: bool,
    all: bool,
    grow: bool,
    shrink: bool,
}

/// Puts the vertices of `after` (an operation's result on `before`) that
/// aren't where one of `before`'s was, the new and moved ones, on
/// `surface`.
/// The UV menu and editor work in Object, Edit and Paint mode.
fn shows_uvs(mode: Mode) -> bool {
    matches!(mode, Mode::Object | Mode::Edit | Mode::Paint)
}

fn snap_new_vertices(
    before: &axcenity_mesh_edit::EditMesh,
    after: &mut axcenity_mesh_edit::EditMesh,
    surface: &Reference,
) {
    let key = |p: Vec3| p.to_array().map(f32::to_bits);
    let old: std::collections::HashSet<[u32; 3]> =
        before.positions().iter().map(|&p| key(p)).collect();
    let (vertices, places): (Vec<u32>, Vec<Vec3>) = after
        .positions()
        .iter()
        .enumerate()
        .filter(|(_, p)| !old.contains(&key(**p)))
        .map(|(v, &p)| (v as u32, surface.snap(p)))
        .unzip();
    if !vertices.is_empty() {
        after.set_positions(&vertices, &places);
    }
}

/// Where Save writes `doc`, as the tooltip names it.
fn save_name(doc: &StudioDoc) -> String {
    if doc.path.is_empty() {
        let name = doc
            .model
            .objects
            .first()
            .map_or("Model", |o| o.name.as_str());
        format!("assets/models/{name}.glb")
    } else {
        doc.path.clone()
    }
}

#[cfg(test)]
pub(crate) mod modifiers_tests;
#[cfg(test)]
pub(crate) mod paint_tests;
#[cfg(test)]
pub(crate) mod retopo_tests;
#[cfg(test)]
pub(crate) mod rig_tests;
#[cfg(test)]
pub(crate) mod sculpt_tests;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
pub(crate) mod tools_tests;
#[cfg(test)]
mod usability_tests;
#[cfg(test)]
mod uv_tests;
