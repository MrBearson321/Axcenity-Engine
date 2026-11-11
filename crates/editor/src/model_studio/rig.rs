//! Rigging in the Model Studio (Phase 62): Armature mode (bones: add,
//! extrude with E, move ends with G, parent, rename, roll, symmetrize,
//! the Humanoid template), skinning (Automatic Weights, Weights by
//! Distance), Weights mode (painting the active bone's weights, Normalize,
//! Limit, Mirror) and Pose mode (turning bones to test the deformation;
//! not saved, not an undo step).
//!
//! The armature and the weights are the `ModelDoc`'s (`mesh_edit::rig`),
//! so each change is an undo step of the Studio's and Save writes them as
//! a skinned `.glb`. Weights and Pose mode draw the model through
//! `ModelDoc::to_model_posed`, which skins it as the game does.

use std::hash::{Hash, Hasher};

use axcenity_mesh_edit::paint::BrushFalloff;
use axcenity_mesh_edit::rig::{
    Armature, MAX_INFLUENCES, WeightBrush, WeightPainter, WeightTool, automatic_weights,
    mirror_weights, weights_by_distance,
};
use axcenity_render::glam::{EulerRot, Quat, Vec3};
use eframe::egui;

use super::view::{bounds, pick_face};
use super::{Mode, ModelStudio, Snapshot, StudioDoc};
use crate::responsive;
use crate::screen::Screen;
use crate::shortcuts::{self, Action};
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// How close (in points) a click must be to a bone to pick it.
const BONE_PICK: f32 = 10.0;

/// Which part of a bone is picked: G moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BonePart {
    Head,
    Tail,
    /// The whole bone.
    #[default]
    Body,
}

/// Rigging's state: the bone picked, the pose being tested, the weight
/// brush and the strokes and moves under way.
#[derive(Default)]
pub struct RigState {
    /// The active bone (Armature mode picks it; Weights mode paints it;
    /// Pose mode turns it), an index into the armature's bones.
    pub bone: Option<usize>,
    pub part: BonePart,
    /// Pose mode: each bone's turn from rest, in degrees round its own X,
    /// Y and Z. Not saved.
    pub pose: Vec<Vec3>,
    pub brush: WeightBrush,
    stroke: Option<WeightStroke>,
    /// The mesh the brush paints on, for the object and revision it was
    /// made from.
    painter: Option<(usize, u64, WeightPainter)>,
    /// A bone end following the mouse (E or G), or a turn (R in Pose
    /// mode).
    moving: Option<BoneMove>,
    /// Goes up as weights are painted and the pose changes, so the view
    /// draws them again.
    version: u64,
    /// The roll drag under way joins one undo step.
    rolling: Option<Snapshot>,
    pub status: Option<String>,
}

struct WeightStroke {
    before: Snapshot,
    last: Option<egui::Pos2>,
    travelled: f32,
    changed: bool,
}

/// A bone being moved with the mouse.
struct BoneMove {
    bone: usize,
    part: BonePart,
    /// Where the mouse was, and where the moved point was, when it began.
    start: egui::Pos2,
    head: Vec3,
    tail: Vec3,
    /// What to put back if it's cancelled (none for a turn).
    before: Option<Snapshot>,
    label: &'static str,
    /// Pose mode's turn: the bone's turn when it began.
    turn: Option<Vec3>,
}

impl RigState {
    pub fn busy(&self) -> bool {
        self.stroke.is_some() || self.moving.is_some() || self.rolling.is_some()
    }

    pub(super) fn forget(&mut self) {
        self.bone = None;
        self.pose.clear();
        self.stroke = None;
        self.painter = None;
        self.moving = None;
        self.rolling = None;
        self.status = None;
        self.version += 1;
    }

    /// Each bone's turn from rest, for `armature`.
    fn turns(&self, armature: &Armature) -> Vec<Quat> {
        (0..armature.bones.len())
            .map(|bone| {
                let degrees = self.pose.get(bone).copied().unwrap_or(Vec3::ZERO);
                turn_of(degrees)
            })
            .collect()
    }
}

/// A turn of `degrees` round X, then Y, then Z.
fn turn_of(degrees: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        degrees.x.to_radians(),
        degrees.y.to_radians(),
        degrees.z.to_radians(),
    )
}

/// True in the modes that show the armature.
pub(super) fn shows_bones(mode: Mode) -> bool {
    matches!(mode, Mode::Armature | Mode::Weights | Mode::Pose)
}

impl StudioDoc {
    /// Changes the armature (made if there's none), as one undo step.
    /// `change` returns false to leave it as it was.
    pub(crate) fn edit_armature(
        &mut self,
        label: &str,
        change: impl FnOnce(&mut Armature, &mut [axcenity_mesh_edit::DocObject]) -> bool,
    ) -> bool {
        let before = self.snapshot();
        let model = &mut self.model;
        let armature = model.armature.get_or_insert_with(Armature::default);
        if !change(armature, &mut model.objects) {
            self.restore_quietly(before);
            return false;
        }
        self.record(label, before);
        true
    }

    /// Puts `snapshot` back without counting it a change.
    fn restore_quietly(&mut self, snapshot: Snapshot) {
        let revision = self.revision;
        self.restore(snapshot);
        self.revision = revision;
    }
}

impl ModelStudio {
    /// The active bone, if it's still one of the armature's.
    pub(crate) fn active_bone(&self) -> Option<usize> {
        let armature = self.doc.as_ref()?.model.armature.as_ref()?;
        self.rig.bone.filter(|&bone| bone < armature.bones.len())
    }

    /// Add Humanoid: the template rig fitted to the active object's box
    /// (or the model's), in place of any armature; skins on the old one
    /// go. One undo step.
    pub(crate) fn add_humanoid(&mut self) -> bool {
        let Some(doc) = &mut self.doc else {
            return false;
        };
        let (min, max) = match doc.mesh() {
            Some(mesh) if mesh.vertex_count() > 0 => mesh
                .positions()
                .iter()
                .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), &p| {
                    (lo.min(p), hi.max(p))
                }),
            _ => bounds(doc),
        };
        let done = doc.edit_armature("Add Humanoid Rig", |armature, objects| {
            *armature = Armature::humanoid(min, max);
            for object in objects {
                object.skin = None;
            }
            true
        });
        if done {
            self.rig.bone = Some(0);
            self.rig.pose.clear();
            self.rig.version += 1;
            self.rig.status = Some(
                "The Humanoid rig, in Mixamo's bone names. Move its bones to fit, then Automatic Weights."
                    .to_owned(),
            );
        }
        done
    }

    /// Add Bone: a bone standing up from the 3D cursor, a tenth of the
    /// model's size long, inside the active bone. One undo step.
    pub(crate) fn add_bone(&mut self) -> Option<usize> {
        let parent = self.active_bone();
        let cursor = self.moves.cursor;
        let doc = self.doc.as_mut()?;
        let (min, max) = bounds(doc);
        let length = ((max - min).length() * 0.1).max(0.05);
        let mut added = None;
        doc.edit_armature("Add Bone", |armature, _| {
            added = Some(armature.add_bone("Bone", cursor, cursor + Vec3::Y * length, parent));
            true
        });
        self.rig.bone = added;
        self.rig.part = BonePart::Tail;
        added
    }

    /// E: a child of the active bone from its tail, as one undo step;
    /// in the view its tail then follows the mouse from `at`.
    pub(crate) fn extrude_bone(&mut self, at: Option<egui::Pos2>) -> Option<usize> {
        let bone = self.active_bone()?;
        let doc = self.doc.as_mut()?;
        let before = doc.snapshot();
        let mut made = None;
        doc.edit_armature("Extrude Bone", |armature, _| {
            made = armature.extrude(bone);
            made.is_some()
        });
        let made = made?;
        self.rig.bone = Some(made);
        self.rig.part = BonePart::Tail;
        if let Some(at) = at {
            self.begin_bone_move(at, "Extrude Bone", Some(before));
        }
        Some(made)
    }

    /// G (or E's follow): the active bone's picked part follows the mouse
    /// from `at` until a click keeps it or Esc puts it back.
    pub(super) fn begin_bone_move(
        &mut self,
        at: egui::Pos2,
        label: &'static str,
        before: Option<Snapshot>,
    ) {
        let (Some(bone), Some(doc)) = (self.active_bone(), &self.doc) else {
            return;
        };
        let Some(armature) = &doc.model.armature else {
            return;
        };
        let b = &armature.bones[bone];
        self.rig.moving = Some(BoneMove {
            bone,
            part: self.rig.part,
            start: at,
            head: b.head,
            tail: b.tail(),
            before: Some(before.unwrap_or_else(|| doc.snapshot())),
            label,
            turn: None,
        });
    }

    /// Moves the bone being moved to follow the mouse at `at`, on the
    /// plane facing the camera through where it started.
    pub(super) fn follow_bone_move(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(moving) = &self.rig.moving else {
            return;
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        if let Some(turn) = moving.turn {
            // Pose mode's R: round the axis from the camera, by how far the
            // mouse went round the bone's head on screen.
            let posed_head = posed_bones(doc, &self.rig)
                .get(moving.bone)
                .map_or(moving.head, |(head, _)| *head);
            let Some(center) = screen.point(posed_head) else {
                return;
            };
            let (from, to) = (moving.start - center, at - center);
            let angle = to.y.atan2(to.x) - from.y.atan2(from.x);
            let mut degrees = turn;
            degrees.z += -angle.to_degrees();
            if let Some(slot) = self.rig.pose.get_mut(moving.bone) {
                *slot = degrees;
            }
            self.rig.version += 1;
            return;
        }
        let anchor = match moving.part {
            BonePart::Head | BonePart::Body => moving.head,
            BonePart::Tail => moving.tail,
        };
        let (Some(from), to) = (plane_point(screen, moving.start, anchor), at) else {
            return;
        };
        let Some(to) = plane_point(screen, to, anchor) else {
            return;
        };
        let shift = to - from;
        let (bone, part, head, tail) = (moving.bone, moving.part, moving.head, moving.tail);
        let Some(armature) = doc.model.armature.as_mut() else {
            return;
        };
        let children: Vec<usize> = (0..armature.bones.len())
            .filter(|&c| {
                armature.bones[c].parent == Some(bone)
                    && armature.bones[c].head.distance(tail) < 1e-4 * (1.0 + tail.length())
            })
            .collect();
        let b = &mut armature.bones[bone];
        match part {
            BonePart::Head => b.set_head(head + shift),
            BonePart::Tail => b.set_tail(tail + shift),
            BonePart::Body => {
                b.head = head + shift;
            }
        }
        let new_tail = b.tail();
        // Children joined at the tail keep joined.
        for child in children {
            armature.bones[child].set_head(new_tail);
        }
        doc.revision += 1;
    }

    pub(super) fn finish_bone_move(&mut self) {
        let Some(moving) = self.rig.moving.take() else {
            return;
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        if moving.turn.is_some() {
            return;
        }
        if let Some(before) = moving.before {
            // E's own step already holds the extrude: the move joins it.
            if moving.label == "Extrude Bone" {
                doc.undo.pop();
            } else if before.model.armature == doc.model.armature {
                return;
            }
            doc.record(moving.label, before);
        }
    }

    pub(super) fn cancel_bone_move(&mut self) {
        let Some(moving) = self.rig.moving.take() else {
            return;
        };
        if let Some(turn) = moving.turn {
            if let Some(slot) = self.rig.pose.get_mut(moving.bone) {
                *slot = turn;
            }
            self.rig.version += 1;
            return;
        }
        if let (Some(doc), Some(before)) = (&mut self.doc, moving.before) {
            if moving.label == "Extrude Bone" {
                doc.undo.pop();
                // Back to the bone it was extruded from.
                self.rig.bone = doc
                    .model
                    .armature
                    .as_ref()
                    .and_then(|arm| arm.bones.get(moving.bone))
                    .and_then(|bone| bone.parent);
            }
            doc.restore(before);
        }
    }

    /// Renames the active bone (made unique), as one undo step.
    pub(crate) fn rename_bone(&mut self, name: &str) -> bool {
        let (Some(bone), Some(doc)) = (self.active_bone(), &mut self.doc) else {
            return false;
        };
        doc.edit_armature("Rename Bone", |armature, _| {
            let before = armature.bones[bone].name.clone();
            armature.rename(bone, name) != before
        })
    }

    /// Makes `parent` the active bone's parent, as one undo step.
    pub(crate) fn parent_bone(&mut self, parent: Option<usize>) -> bool {
        let (Some(bone), Some(doc)) = (self.active_bone(), &mut self.doc) else {
            return false;
        };
        doc.edit_armature("Parent Bone", |armature, _| {
            armature.bones[bone].parent != parent && armature.set_parent(bone, parent)
        })
    }

    /// Sets the active bone's roll (degrees); a drag joins one undo step
    /// until [`end_roll`](Self::end_roll).
    pub(crate) fn set_roll(&mut self, degrees: f32) {
        let (Some(bone), Some(doc)) = (self.active_bone(), &mut self.doc) else {
            return;
        };
        if self.rig.rolling.is_none() {
            self.rig.rolling = Some(doc.snapshot());
        }
        if let Some(armature) = &mut doc.model.armature {
            armature.bones[bone].set_roll(degrees.to_radians());
            doc.revision += 1;
        }
    }

    pub(crate) fn end_roll(&mut self) {
        if let (Some(before), Some(doc)) = (self.rig.rolling.take(), &mut self.doc) {
            doc.record("Roll Bone", before);
        }
    }

    /// Deletes the active bone; its children go to its parent and its
    /// weights go. One undo step.
    pub(crate) fn delete_bone(&mut self) -> bool {
        let (Some(bone), Some(doc)) = (self.active_bone(), &mut self.doc) else {
            return false;
        };
        let parent = doc
            .model
            .armature
            .as_ref()
            .and_then(|arm| arm.bones[bone].parent);
        let done = doc.edit_armature("Delete Bone", |armature, objects| {
            let map = armature.delete(bone);
            for object in objects {
                if let Some(skin) = &mut object.skin {
                    skin.remap(&map);
                }
            }
            true
        });
        self.rig.bone = parent.map(|p| if p > bone { p - 1 } else { p });
        self.rig.pose.clear();
        done
    }

    /// Symmetrize: the active bone and those inside it mirrored across
    /// X = 0 with their sides' names swapped. One undo step.
    pub(crate) fn symmetrize_bones(&mut self) -> usize {
        let (Some(bone), Some(doc)) = (self.active_bone(), &mut self.doc) else {
            return 0;
        };
        let mut made = 0;
        doc.edit_armature("Symmetrize", |armature, _| {
            let inside: Vec<usize> = (0..armature.bones.len())
                .filter(|&b| armature.is_inside(b, bone))
                .collect();
            made = armature.symmetrize(&inside).len();
            made > 0
        });
        self.rig.status = Some(match made {
            0 => "Symmetrize needs bones with a side in their name (.L, _R, Left…)".to_owned(),
            n => format!("Symmetrize: {n} bones on the other side"),
        });
        made
    }

    /// The objects skinning works on: those picked with meshes, or the
    /// active one.
    fn skinned_targets(doc: &StudioDoc) -> Vec<usize> {
        let picked: Vec<usize> = doc
            .objects
            .iter()
            .copied()
            .filter(|&k| doc.model.objects.get(k).is_some_and(|o| o.mesh.is_some()))
            .collect();
        if picked.is_empty() {
            vec![doc.active]
        } else {
            picked
        }
    }

    /// Automatic Weights (bone heat) or, with `by_distance`, Weights by
    /// Distance, on the picked objects, as one undo step.
    pub(crate) fn weigh(&mut self, by_distance: bool) -> bool {
        let Some(doc) = &mut self.doc else {
            return false;
        };
        let Some(armature) = doc.model.armature.clone().filter(|a| !a.bones.is_empty()) else {
            self.rig.status = Some("Add an armature first (Add Humanoid or Add Bone)".to_owned());
            return false;
        };
        let targets = Self::skinned_targets(doc);
        let label = if by_distance {
            "Weights by Distance"
        } else {
            "Automatic Weights"
        };
        let before = doc.snapshot();
        let mut count = 0;
        for k in targets {
            let Some(object) = doc.model.objects.get_mut(k) else {
                continue;
            };
            let Some(mesh) = &object.mesh else {
                continue;
            };
            object.skin = Some(if by_distance {
                weights_by_distance(mesh, &armature)
            } else {
                automatic_weights(mesh, &armature)
            });
            count += 1;
        }
        if count == 0 {
            return false;
        }
        doc.record(label, before);
        self.rig.status = Some(format!("{label}: {count} object(s) skinned"));
        self.rig.version += 1;
        true
    }

    /// Normalize All, Limit Total (to 4) or Mirror on the active object's
    /// weights, as one undo step.
    pub(crate) fn weight_action(&mut self, action: WeightAction) -> bool {
        let bone = self.active_bone();
        let Some(doc) = &mut self.doc else {
            return false;
        };
        let before = doc.snapshot();
        let active = doc.active;
        let model = &mut doc.model;
        let (Some(armature), Some(object)) = (&model.armature, model.objects.get_mut(active))
        else {
            return false;
        };
        let (Some(skin), Some(mesh)) = (object.skin.as_mut(), object.mesh.as_ref()) else {
            self.rig.status = Some("The active object has no weights yet".to_owned());
            return false;
        };
        skin.touch(mesh);
        let (label, status) = match action {
            WeightAction::Normalize => {
                skin.normalize();
                (
                    "Normalize All",
                    "Each vertex's weights add up to 1".to_owned(),
                )
            }
            WeightAction::Limit => {
                skin.limit(MAX_INFLUENCES);
                skin.normalize();
                (
                    "Limit Total",
                    format!("Each vertex keeps its {MAX_INFLUENCES} strongest bones"),
                )
            }
            WeightAction::Mirror => {
                // From the side the active bone is on (+X if it's in the middle).
                let positive = bone.is_none_or(|b| armature.bones[b].head.x >= 0.0);
                let changed = mirror_weights(skin, mesh, armature, positive);
                (
                    "Mirror Weights",
                    format!("Mirror: {changed} vertices take the other side's weights"),
                )
            }
        };
        doc.record(label, before);
        self.rig.status = Some(status);
        self.rig.version += 1;
        true
    }

    /// Pose mode: turns `bone` to `degrees` from rest (round its X, Y, Z).
    pub(crate) fn set_pose(&mut self, bone: usize, degrees: Vec3) {
        let count = self
            .doc
            .as_ref()
            .and_then(|doc| doc.model.armature.as_ref())
            .map_or(0, |arm| arm.bones.len());
        if bone >= count {
            return;
        }
        self.rig.pose.resize(count, Vec3::ZERO);
        self.rig.pose[bone] = degrees;
        self.rig.version += 1;
    }

    pub(crate) fn reset_pose(&mut self) {
        self.rig.pose.clear();
        self.rig.version += 1;
    }

    /// The pose being tested, in the saved rig's order.
    pub(crate) fn test_pose(&self) -> Option<axcenity_assets::Pose> {
        let armature = self.doc.as_ref()?.model.armature.as_ref()?;
        Some(armature.pose(&self.rig.turns(armature)))
    }

    /// What the view draws in Weights and Pose mode: the model skinned in
    /// the pose (at rest for Weights), with the active bone's weights as
    /// colours in Weights mode. The key changes when it would.
    pub(super) fn rig_model_key(&self) -> Option<u64> {
        let doc = self.doc.as_ref()?;
        // Without bones there's no skinned model to hand the view, so it
        // keeps drawing the document's own.
        let bones = doc
            .model
            .armature
            .as_ref()
            .is_some_and(|arm| !arm.bones.is_empty());
        if !bones || !matches!(doc.mode, Mode::Weights | Mode::Pose) {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (doc.revision, self.rig.version, doc.mode == Mode::Weights).hash(&mut hasher);
        if doc.mode == Mode::Weights {
            self.active_bone().hash(&mut hasher);
        }
        Some(hasher.finish() | 1)
    }

    pub(crate) fn rig_model(&self) -> Option<axcenity_assets::Model> {
        let doc = self.doc.as_ref()?;
        let armature = doc.model.armature.as_ref()?;
        match doc.mode {
            Mode::Weights => doc.model.to_model_posed(
                &armature.rest_pose(),
                Some(self.active_bone().unwrap_or(usize::MAX)),
            ),
            _ => doc.model.to_model_posed(&self.test_pose()?, None),
        }
    }

    /// Starts painting the active bone's weights on the active object.
    fn begin_weights(&mut self) -> bool {
        let Some(doc) = &self.doc else {
            return false;
        };
        if self.active_bone().is_none() {
            self.rig.status = Some("Pick a bone to paint its weights".to_owned());
            return false;
        }
        let Some(object) = doc.model.objects.get(doc.active) else {
            return false;
        };
        let Some(mesh) = &object.mesh else {
            return false;
        };
        let fresh = matches!(&self.rig.painter, Some((k, revision, _)) if *k == doc.active && *revision == doc.revision);
        if !fresh {
            self.rig.painter = Some((doc.active, doc.revision, WeightPainter::new(mesh)));
        }
        let before = doc.snapshot();
        let doc = self.doc.as_mut().expect("checked above");
        let object = &mut doc.model.objects[doc.active];
        let mesh = object.mesh.as_ref().expect("checked above");
        let skin = object.skin.get_or_insert_with(Default::default);
        let fitted = skin.fitted(mesh).into_owned();
        *skin = fitted;
        skin.touch(mesh);
        self.rig.stroke = Some(WeightStroke {
            before,
            last: None,
            travelled: 0.0,
            changed: false,
        });
        true
    }

    /// One dab of the weight brush where `at` meets the active object.
    fn weight_dab(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(bone) = self.active_bone() else {
            return;
        };
        let (Some(doc), Some((_, _, painter)), Some(stroke)) =
            (&mut self.doc, &self.rig.painter, &mut self.rig.stroke)
        else {
            return;
        };
        stroke.last = Some(at);
        let object = &mut doc.model.objects[doc.active];
        let Some(mesh) = &object.mesh else {
            return;
        };
        let ray = screen.ray(at);
        let Some((_, t)) = pick_face(mesh, ray) else {
            return;
        };
        let center = ray.origin + ray.direction * t;
        let radius = self.rig.brush.radius * screen.studs_per_point(center);
        let Some(skin) = &mut object.skin else {
            return;
        };
        if painter.dab(skin, &self.rig.brush, bone, center, radius) {
            stroke.changed = true;
            self.rig.version += 1;
        }
    }

    /// Dabs from the last one toward `at`, a quarter of the radius apart.
    fn weight_drag_to(&mut self, screen: &Screen, at: egui::Pos2) {
        let Some(stroke) = &self.rig.stroke else {
            return;
        };
        let Some(from) = stroke.last else {
            self.weight_dab(screen, at);
            return;
        };
        let step = (self.rig.brush.radius * 0.25).max(1.0);
        let distance = from.distance(at);
        let mut along = step - stroke.travelled;
        while along <= distance {
            self.weight_dab(screen, from + (at - from) * (along / distance));
            along += step;
        }
        if let Some(stroke) = &mut self.rig.stroke {
            stroke.travelled = (stroke.travelled + distance) % step;
            stroke.last = Some(at);
        }
    }

    fn end_weights(&mut self) {
        let Some(stroke) = self.rig.stroke.take() else {
            return;
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        if stroke.changed {
            doc.record("Weight Stroke", stroke.before);
            // The painter's mesh is the same; only the weights changed.
            if let Some((_, revision, _)) = &mut self.rig.painter {
                *revision = doc.revision;
            }
        } else {
            doc.restore_quietly(stroke.before);
        }
    }

    /// A whole weight stroke through `points` in the 3D view.
    #[cfg(test)]
    pub(crate) fn weight_stroke(&mut self, screen: &Screen, points: &[egui::Pos2]) -> bool {
        if !self.begin_weights() {
            return false;
        }
        for &at in points {
            self.weight_drag_to(screen, at);
        }
        self.end_weights();
        true
    }

    /// The bone nearest `at` on screen (and which part), within reach.
    pub(super) fn pick_bone(&self, screen: &Screen, at: egui::Pos2) -> Option<(usize, BonePart)> {
        let doc = self.doc.as_ref()?;
        let bones = posed_bones(doc, &self.rig);
        let mut best: Option<(f32, usize, BonePart)> = None;
        for (k, &(head, tail)) in bones.iter().enumerate() {
            let ends = [(head, BonePart::Head), (tail, BonePart::Tail)];
            for (point, part) in ends {
                if let Some(p) = screen.point(point) {
                    let d = p.distance(at) - 2.0;
                    if d < BONE_PICK && best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, k, part));
                    }
                }
            }
            if let Some([a, b]) = screen.line(head, tail) {
                let d = segment_distance(at, a, b) + 1.0;
                if d < BONE_PICK && best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, k, BonePart::Body));
                }
            }
        }
        best.map(|(_, k, part)| (k, part))
    }

    /// The mouse in the 3D view in Armature, Weights and Pose mode.
    pub(super) fn rig_in_view(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
    ) {
        let (pointer, modifiers, pressed, down, clicked) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.modifiers,
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.primary_clicked(),
            )
        });
        let over =
            pointer.is_some_and(|at| response.rect.contains(at)) && response.contains_pointer();
        let mode = self.doc.as_ref().map(|doc| doc.mode);
        if self.rig.moving.is_some() {
            if let Some(at) = pointer {
                self.follow_bone_move(screen, at);
            }
            if pressed || ui.input(|i| shortcuts::pressed(i, Action::FinishBoneMove)) {
                self.finish_bone_move();
            } else if response.secondary_clicked()
                || ui.input(|i| shortcuts::pressed(i, Action::CancelBoneMove))
            {
                self.cancel_bone_move();
            }
            return;
        }
        if mode == Some(Mode::Weights) && !modifiers.command {
            if pressed && over && self.rig.stroke.is_none() {
                if self.begin_weights()
                    && let Some(at) = pointer
                {
                    self.weight_dab(screen, at);
                }
            } else if self.rig.stroke.is_some() {
                if let Some(at) = pointer.filter(|_| down) {
                    self.weight_drag_to(screen, at);
                }
                if !down {
                    self.end_weights();
                }
            }
            if let Some(at) = pointer.filter(|_| over) {
                let accent = theme::palette(ui.ctx()).accent;
                let painter = ui.painter_at(response.rect);
                let radius = self.rig.brush.radius;
                painter.circle_stroke(at, radius, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
                painter.circle_stroke(at, radius, egui::Stroke::new(1.0_f32, accent));
            }
            return;
        }
        if clicked
            && over
            && let Some(at) = pointer
        {
            match self.pick_bone(screen, at) {
                Some((bone, part)) => {
                    self.rig.bone = Some(bone);
                    self.rig.part = part;
                }
                None if mode == Some(Mode::Armature) => self.rig.bone = None,
                None => {}
            }
        }
    }

    /// The keys of Armature mode (E, G, X) and Pose mode (R), while the
    /// mouse is over the view.
    pub(super) fn rig_keys(&mut self, ui: &mut egui::Ui, at: egui::Pos2) {
        let Some(mode) = self.doc.as_ref().map(|doc| doc.mode) else {
            return;
        };
        if self.rig.moving.is_some() {
            return;
        }
        let key =
            |ui: &mut egui::Ui, action| ui.input_mut(|input| shortcuts::consume(input, action));
        match mode {
            Mode::Armature => {
                if key(ui, Action::ExtrudeBone) {
                    self.extrude_bone(Some(at));
                } else if key(ui, Action::MoveBone) {
                    self.begin_bone_move(at, "Move Bone", None);
                } else if key(ui, Action::DeleteBone) {
                    self.delete_bone();
                }
            }
            Mode::Pose => {
                if key(ui, Action::TurnBone)
                    && let Some(bone) = self.active_bone()
                {
                    let count = self
                        .doc
                        .as_ref()
                        .and_then(|d| d.model.armature.as_ref())
                        .map_or(0, |a| a.bones.len());
                    self.rig.pose.resize(count, Vec3::ZERO);
                    let turn = self.rig.pose[bone];
                    self.rig.moving = Some(BoneMove {
                        bone,
                        part: BonePart::Body,
                        start: at,
                        head: Vec3::ZERO,
                        tail: Vec3::ZERO,
                        before: None,
                        label: "Turn Bone",
                        turn: Some(turn),
                    });
                }
            }
            Mode::Weights => {
                let (smaller, bigger) =
                    (key(ui, Action::BrushSmaller), key(ui, Action::BrushBigger));
                let brush = &mut self.rig.brush;
                if smaller {
                    brush.radius = (brush.radius / 1.15).max(1.0);
                }
                if bigger {
                    brush.radius = (brush.radius * 1.15).min(1000.0);
                }
            }
            _ => {}
        }
    }

    /// The bones over the view: lines from head to tail with a dot at
    /// each end, the active one in the accent colour.
    pub(super) fn paint_bones(&self, painter: &egui::Painter, screen: &Screen) {
        let Some(doc) = &self.doc else {
            return;
        };
        if !shows_bones(doc.mode) {
            return;
        }
        let palette = theme::palette(painter.ctx());
        let active = self.active_bone();
        let bones = posed_bones(doc, &self.rig);
        for (k, &(head, tail)) in bones.iter().enumerate() {
            let picked = Some(k) == active;
            let color = if picked {
                palette.accent
            } else {
                palette.text_secondary
            };
            if let Some([a, b]) = screen.line(head, tail) {
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        if picked { 4.0_f32 } else { 2.5 },
                        egui::Color32::BLACK.gamma_multiply(0.6),
                    ),
                );
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(if picked { 2.5_f32 } else { 1.5 }, color),
                );
            }
            for (point, radius, part) in [(head, 4.0, BonePart::Head), (tail, 3.0, BonePart::Tail)]
            {
                if let Some(p) = screen.point(point) {
                    let lit = picked && (self.rig.part == part || self.rig.part == BonePart::Body);
                    painter.circle_filled(p, radius, if lit { palette.accent } else { color });
                    painter.circle_stroke(
                        p,
                        radius,
                        egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                    );
                }
            }
        }
    }

    /// The Rig panel: the armature, skinning, and Weights or Pose mode's
    /// own settings.
    pub(super) fn rig_panel(&mut self, ui: &mut egui::Ui, editable: bool) {
        let Some(mode) = self.doc.as_ref().map(|doc| doc.mode) else {
            return;
        };
        ui.add_enabled_ui(editable, |ui| {
            if mode == Mode::Armature {
                self.armature_section(ui);
            }
            self.bone_list(ui);
            match mode {
                Mode::Armature => {
                    self.bone_section(ui);
                    ui.add_space(8.0);
                    self.skin_section(ui);
                }
                Mode::Weights => self.weights_section(ui),
                Mode::Pose => self.pose_section(ui),
                _ => {}
            }
        });
        if let Some(status) = &self.rig.status {
            ui.weak(status);
        }
    }

    fn armature_section(&mut self, ui: &mut egui::Ui) {
        widgets::section_label(ui, "Armature");
        responsive::toolbar(ui, |ui| {
            if ui
                .button(theme::with_icon(icons::PERSON_ARMS_SPREAD, "Add Humanoid"))
                .on_hover_text(
                    "The Humanoid template rig (Mixamo's bone names), fitted to the active object",
                )
                .clicked()
            {
                self.add_humanoid();
            }
            if ui
                .button(theme::with_icon(icons::PLUS, "Add Bone"))
                .on_hover_text("A bone at the 3D cursor, inside the picked one")
                .clicked()
            {
                self.add_bone();
            }
            let picked = self.active_bone().is_some();
            if ui
                .add_enabled(
                    picked,
                    egui::Button::new(theme::with_icon(icons::ARROW_BEND_UP_RIGHT, "Extrude")),
                )
                .on_hover_text("E: a new bone from the picked one's tail")
                .clicked()
            {
                self.extrude_bone(None);
            }
            if ui
                .add_enabled(picked, egui::Button::new(theme::with_icon(icons::FLIP_HORIZONTAL, "Symmetrize")))
                .on_hover_text("Mirror the picked bone and those in it across X, swapping .L and .R (or Left and Right)")
                .clicked()
            {
                self.symmetrize_bones();
            }
            if ui
                .add_enabled(
                    picked,
                    egui::Button::new(theme::with_icon(icons::TRASH, "Delete")),
                )
                .on_hover_text("X: delete the picked bone (its children go to its parent)")
                .clicked()
            {
                self.delete_bone();
            }
        });
        ui.weak("Click a bone · E: extrude · G: move its head, tail or all · X: delete");
    }

    /// Every bone, indented by depth; a click makes it the active one.
    fn bone_list(&mut self, ui: &mut egui::Ui) {
        let Some(armature) = self
            .doc
            .as_ref()
            .and_then(|doc| doc.model.armature.as_ref())
        else {
            ui.weak("No bones yet: click Add Humanoid or Add Bone above.");
            return;
        };
        let order = armature.order();
        let rows: Vec<(usize, usize, String)> = order
            .iter()
            .map(|&b| (b, armature.depth(b), armature.bones[b].name.clone()))
            .collect();
        let active = self.active_bone();
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("studio bones")
            .max_height(180.0)
            .show(ui, |ui| {
                for (bone, depth, name) in rows {
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 10.0);
                        let label = theme::with_icon(icons::BONE, &name);
                        if ui.selectable_label(active == Some(bone), label).clicked() {
                            picked = Some(bone);
                        }
                    });
                }
            });
        if let Some(bone) = picked {
            self.rig.bone = Some(bone);
            self.rig.part = BonePart::Body;
        }
    }

    fn bone_section(&mut self, ui: &mut egui::Ui) {
        let Some(bone) = self.active_bone() else {
            return;
        };
        let Some(armature) = self.doc.as_ref().and_then(|doc| doc.model.armature.clone()) else {
            return;
        };
        let current = armature.bones[bone].clone();
        widgets::section_label(ui, "Bone");
        let mut name = current.name.clone();
        let mut parent = current.parent;
        let mut roll = current.roll().to_degrees();
        let mut deform = current.deform;
        let mut roll_changed = None;
        let mut roll_done = false;
        responsive::form(ui, "studio bone", |ui| {
            ui.label("Name");
            let response = ui.text_edit_singleline(&mut name);
            if response.lost_focus() && name != current.name {
                self.rename_bone(&name);
            }
            ui.end_row();
            ui.label("Parent");
            let label =
                |p: Option<usize>| p.map_or("None".to_owned(), |p| armature.bones[p].name.clone());
            egui::ComboBox::from_id_salt("studio bone parent")
                .selected_text(label(parent))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut parent, None, "None");
                    for other in armature.order() {
                        if !armature.is_inside(other, bone) {
                            ui.selectable_value(&mut parent, Some(other), label(Some(other)));
                        }
                    }
                });
            ui.end_row();
            ui.label("Roll");
            let response = ui.add(egui::DragValue::new(&mut roll).speed(0.5).suffix("°"));
            if response.changed() {
                roll_changed = Some(roll);
            }
            roll_done = response.drag_stopped() || response.lost_focus();
            ui.end_row();
            ui.label("Deform");
            ui.checkbox(&mut deform, "Gets automatic weights");
            ui.end_row();
            ui.label("Length");
            ui.weak(format!("{:.3} studs", current.length));
            ui.end_row();
        });
        if parent != current.parent {
            self.parent_bone(parent);
        }
        if let Some(roll) = roll_changed {
            self.set_roll(roll);
        }
        if roll_done
            || (roll_changed.is_none()
                && self.rig.rolling.is_some()
                && !ui.ctx().is_using_pointer())
        {
            self.end_roll();
        }
        if deform != current.deform
            && let Some(doc) = &mut self.doc
        {
            doc.edit_armature("Deform", |armature, _| {
                armature.bones[bone].deform = deform;
                true
            });
        }
    }

    fn skin_section(&mut self, ui: &mut egui::Ui) {
        widgets::section_label(ui, "Skin");
        responsive::toolbar(ui, |ui| {
            if ui
                .add(widgets::button(
                    Role::Primary,
                    Some(icons::LIGHTNING),
                    "Automatic Weights",
                    &theme::palette(ui.ctx()),
                ))
                .on_hover_text(
                    "Bone heat: weights that spread from each bone over the picked objects",
                )
                .clicked()
            {
                self.weigh(false);
            }
            if ui
                .button("Weights by Distance")
                .on_hover_text("Each vertex weighted to its nearest bones")
                .clicked()
            {
                self.weigh(true);
            }
        });
        ui.weak("Weights (beside Bones) paints them; Pose tests them.");
    }

    fn weights_section(&mut self, ui: &mut egui::Ui) {
        widgets::section_label(ui, "Weight brush");
        let brush = &mut self.rig.brush;
        ui.horizontal_wrapped(|ui| {
            for tool in WeightTool::ALL {
                if ui
                    .selectable_label(brush.tool == tool, tool.label())
                    .clicked()
                {
                    brush.tool = tool;
                }
            }
        });
        responsive::form(ui, "studio weight brush", |ui| {
            ui.label("Radius");
            ui.add(
                egui::Slider::new(&mut brush.radius, 1.0..=400.0)
                    .logarithmic(true)
                    .suffix(" pt"),
            );
            ui.end_row();
            ui.label("Strength");
            ui.add(egui::Slider::new(&mut brush.strength, 0.0..=1.0));
            ui.end_row();
            ui.label("Falloff");
            egui::ComboBox::from_id_salt("studio weight falloff")
                .selected_text(brush.falloff.label())
                .show_ui(ui, |ui| {
                    for falloff in BrushFalloff::ALL {
                        ui.selectable_value(&mut brush.falloff, falloff, falloff.label());
                    }
                });
            ui.end_row();
            ui.label("Auto normalize");
            ui.checkbox(&mut brush.auto_normalize, "Keep each vertex adding up to 1");
            ui.end_row();
        });
        let mut action = None;
        responsive::toolbar(ui, |ui| {
            if ui
                .button("Normalize All")
                .on_hover_text("Each vertex's weights scaled to add up to 1")
                .clicked()
            {
                action = Some(WeightAction::Normalize);
            }
            if ui
                .button(format!("Limit to {MAX_INFLUENCES}"))
                .on_hover_text("Each vertex keeps its strongest bones (the game uses 4)")
                .clicked()
            {
                action = Some(WeightAction::Limit);
            }
            if ui
                .button(theme::with_icon(icons::FLIP_HORIZONTAL, "Mirror"))
                .on_hover_text(
                    "Copy the active bone's side's weights onto the other side, .L and .R swapped",
                )
                .clicked()
            {
                action = Some(WeightAction::Mirror);
            }
        });
        if let Some(action) = action {
            self.weight_action(action);
        }
        ui.weak("Drag: paint the picked bone · Ctrl-click: pick a bone · [ ]: radius · blue none, red all");
    }

    fn pose_section(&mut self, ui: &mut egui::Ui) {
        widgets::section_label(ui, "Pose test");
        if let Some(bone) = self.active_bone() {
            let mut degrees = self.rig.pose.get(bone).copied().unwrap_or(Vec3::ZERO);
            let mut changed = false;
            responsive::form(ui, "studio pose", |ui| {
                for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                    ui.label(format!("Turn {name}"));
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut degrees[axis])
                                .speed(1.0)
                                .suffix("°"),
                        )
                        .changed();
                    ui.end_row();
                }
            });
            if changed {
                self.set_pose(bone, degrees);
            }
        }
        if ui
            .button(theme::with_icon(
                icons::ARROW_COUNTER_CLOCKWISE,
                "Reset Pose",
            ))
            .clicked()
        {
            self.reset_pose();
        }
        ui.weak("Click a bone · R: turn it with the mouse · the pose isn't saved");
    }
}

/// Normalize All, Limit Total and Mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightAction {
    Normalize,
    Limit,
    Mirror,
}

/// Each bone's head and tail as drawn: posed in Pose mode.
fn posed_bones(doc: &StudioDoc, rig: &RigState) -> Vec<(Vec3, Vec3)> {
    let Some(armature) = &doc.model.armature else {
        return Vec::new();
    };
    if doc.mode != Mode::Pose {
        return armature.bones.iter().map(|b| (b.head, b.tail())).collect();
    }
    let matrices = armature.skin_matrices(&armature.pose(&rig.turns(armature)));
    armature
        .bones
        .iter()
        .zip(matrices)
        .map(|(b, m)| (m.transform_point3(b.head), m.transform_point3(b.tail())))
        .collect()
}

/// Where the mouse at `at` is on the plane facing the camera through
/// `anchor`.
fn plane_point(screen: &Screen, at: egui::Pos2, anchor: Vec3) -> Option<Vec3> {
    let ray = screen.ray(at);
    let normal = ray.direction;
    let across = ray.direction.dot(normal);
    if across.abs() < 1e-6 {
        return None;
    }
    let t = (anchor - ray.origin).dot(normal) / across;
    Some(ray.origin + ray.direction * t)
}

fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}
