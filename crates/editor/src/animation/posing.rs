//! Posing a skinned model in the Animation tab's previews: its bones as
//! joints you can pick, the gizmo that turns a bone (or moves the root),
//! and the bone tree in the drawer.
//!
//! A [`Skeleton`] is a rig posed on a MeshPart in a preview's world. The
//! gizmo works in the world, as it does on parts; [`Skeleton::turned`] and
//! [`Skeleton::moved`] turn what it did back into the bone's own transform,
//! relative to its parent bone, which is what a `Bone` key stores.

use std::collections::{HashMap, HashSet};

use axcenity_assets::{BoneTransform, Rig};
use axcenity_core::{InstanceId, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui;

use crate::gizmo::{self, Drag, Handle, Modes, Snap, Tool};
use crate::screen::Screen;
use crate::theme::{self, icons};

/// How close (in points) the mouse must be to a joint to pick it.
const PICK: f32 = 9.0;
/// A joint's radius, in points.
const JOINT: f32 = 4.0;
/// Bones turn in steps of this many degrees.
const TURN_STEP: f32 = 5.0;

/// A rig posed on a MeshPart in a preview's world.
pub struct Skeleton<'a> {
    /// The MeshPart, which the gizmo's drag is for.
    pub part: InstanceId,
    pub rig: &'a Rig,
    pub pose: &'a [BoneTransform],
    /// The MeshPart's placement: its 1×1×1 box into the world.
    pub place: Mat4,
    /// One per bone: moved by the clip's linked take, so it can't be keyed.
    /// Empty when nothing is locked.
    pub locked: &'a [bool],
}

impl<'a> Skeleton<'a> {
    /// `part` in `world` with `rig` posed at `pose`, or `None` if it isn't
    /// a part.
    pub fn on(
        world: &World,
        part: InstanceId,
        rig: &'a Rig,
        pose: &'a [BoneTransform],
        locked: &'a [bool],
    ) -> Option<Self> {
        let at = gizmo::Pose::of(world.get(part)?)?;
        Some(Self {
            part,
            rig,
            pose,
            place: Mat4::from_scale_rotation_translation(at.size, at.rotation, at.position),
            locked,
        })
    }

    pub fn locked(&self, bone: usize) -> bool {
        self.locked.get(bone).copied().unwrap_or(false)
    }

    /// Whether `bone` can be posed: it has a name to key it by.
    pub fn posable(&self, bone: usize) -> bool {
        self.rig
            .bones
            .get(bone)
            .is_some_and(|bone| !bone.name.is_empty())
    }

    /// Whether `bone` is a root: no named bone is above it. A root can be
    /// moved as well as turned.
    pub fn is_root(&self, bone: usize) -> bool {
        is_root(self.rig, bone)
    }

    /// From the space of `bone`'s parent (the file's space for a root) into
    /// the world.
    fn parent_frame(&self, world: &[Mat4], bone: usize) -> Mat4 {
        let parent = self.rig.bones[bone]
            .parent
            .and_then(|parent| world.get(parent).copied())
            .unwrap_or(Mat4::IDENTITY);
        self.place * self.rig.fit * parent
    }

    /// Where each bone's joint is in the world.
    pub fn joints(&self) -> Vec<Vec3> {
        let to_world = self.place * self.rig.fit;
        self.rig
            .world(self.pose)
            .iter()
            .map(|bone| to_world.transform_point3(bone.w_axis.truncate()))
            .collect()
    }

    /// Where the gizmo goes on `bone`: its joint, turned as the bone is.
    fn gizmo_pose(&self, bone: usize) -> Option<gizmo::Pose> {
        let world = self.rig.world(self.pose);
        let local = self.pose.get(bone)?;
        let frame = self.parent_frame(&world, bone);
        let (_, turn, _) = frame.to_scale_rotation_translation();
        Some(gizmo::Pose {
            position: frame.transform_point3(local.translation),
            rotation: (turn * local.rotation).normalize(),
            size: Vec3::ONE,
        })
    }

    /// `bone`'s transform once it's turned by `turn` around the world's
    /// axes, about its joint.
    pub fn turned(&self, bone: usize, turn: Quat) -> Option<BoneTransform> {
        let world = self.rig.world(self.pose);
        let mut local = *self.pose.get(bone)?;
        let (_, frame, _) = self
            .parent_frame(&world, bone)
            .to_scale_rotation_translation();
        // The world turn, seen from the parent's space.
        let along = frame.inverse() * turn * frame;
        local.rotation = (along * local.rotation).normalize();
        Some(local)
    }

    /// `bone`'s transform once its joint is moved `by` (in the world).
    pub fn moved(&self, bone: usize, by: Vec3) -> Option<BoneTransform> {
        let world = self.rig.world(self.pose);
        let mut local = *self.pose.get(bone)?;
        local.translation += self
            .parent_frame(&world, bone)
            .inverse()
            .transform_vector3(by);
        Some(local)
    }
}

/// Whether no named bone is above `bone` in `rig`.
fn is_root(rig: &Rig, bone: usize) -> bool {
    let mut at = rig.bones.get(bone).and_then(|b| b.parent);
    while let Some(parent) = at {
        if !rig.bones[parent].name.is_empty() {
            return false;
        }
        at = rig.bones[parent].parent;
    }
    true
}

/// What was done to the skeleton this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum BoneEvent {
    /// A joint was clicked.
    Select(usize),
    /// The selected bone was turned or moved to this.
    Pose(usize, BoneTransform),
}

/// What [`Poser::interact`] found.
#[derive(Default)]
pub struct Interaction {
    /// The mouse is on a joint or the gizmo, or dragging it: the preview
    /// shouldn't turn its camera or pick a part.
    pub captured: bool,
    pub event: Option<BoneEvent>,
    shapes: Vec<gizmo::Shape>,
    highlighted: Option<Handle>,
    hovered: Option<usize>,
}

/// The selected bone, the tool that poses it, and its drag.
pub struct Poser {
    pub selected: Option<usize>,
    /// Rotate, or Move for a root.
    pub tool: Tool,
    /// The bone being dragged, the drag, and where its gizmo was then.
    drag: Option<(usize, Drag, gizmo::Pose)>,
}

impl Default for Poser {
    fn default() -> Self {
        Self {
            selected: None,
            tool: Tool::Rotate,
            drag: None,
        }
    }
}

impl Poser {
    /// The tool the selected bone's gizmo shows: Move only for a root.
    fn tool_for(&self, skeleton: &Skeleton, bone: usize) -> Tool {
        if self.tool == Tool::Move && skeleton.is_root(bone) {
            Tool::Move
        } else {
            Tool::Rotate
        }
    }

    /// The Rotate and Move buttons (Move only with a root selected).
    pub fn tools(&mut self, ui: &mut egui::Ui, rig: &Rig) {
        let Some(bone) = self.selected else {
            return;
        };
        let root = is_root(rig, bone);
        if crate::widgets::icon_button(
            ui,
            icons::ARROW_CLOCKWISE,
            "Turn the bone",
            self.tool == Tool::Rotate || !root,
        )
        .clicked()
        {
            self.tool = Tool::Rotate;
        }
        if root
            && crate::widgets::icon_button(
                ui,
                icons::ARROWS_OUT_CARDINAL,
                "Move the root",
                self.tool == Tool::Move,
            )
            .clicked()
        {
            self.tool = Tool::Move;
        }
    }

    /// Picks joints and drags the selected bone's gizmo, in a preview drawn
    /// in `response`'s rectangle through `screen`.
    pub fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        skeleton: &Skeleton,
    ) -> Interaction {
        let mut out = Interaction::default();
        if self.selected.is_some_and(|bone| !skeleton.posable(bone)) {
            self.selected = None;
        }
        let gizmo = self
            .selected
            .filter(|&bone| !skeleton.locked(bone))
            .and_then(|bone| Some((bone, skeleton.gizmo_pose(bone)?)));
        let tool = gizmo.map(|(bone, _)| self.tool_for(skeleton, bone));
        let frame = gizmo
            .zip(tool)
            .and_then(|((_, pose), tool)| gizmo::handle_frame(tool, Modes::default(), &[pose]));
        out.shapes = frame
            .zip(tool)
            .map_or_else(Vec::new, |(frame, tool)| gizmo::shapes(tool, frame, screen));

        let (pressed, down, pointer) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
            )
        });
        let over = pointer.filter(|_| response.contains_pointer());
        let handle = match over {
            Some(pointer) if self.drag.is_none() => gizmo::hovered(&out.shapes, pointer),
            _ => None,
        };
        let joints = skeleton.joints();
        out.hovered = over
            .filter(|_| handle.is_none() && self.drag.is_none())
            .and_then(|pointer| {
                joints
                    .iter()
                    .enumerate()
                    .filter(|(bone, _)| skeleton.posable(*bone))
                    .filter_map(|(bone, &joint)| {
                        Some((bone, screen.point(joint)?.distance(pointer)))
                    })
                    .filter(|(_, distance)| *distance <= PICK)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(bone, _)| bone)
            });

        if pressed
            && let (Some(handle), Some(pointer), Some(frame), Some((bone, start))) =
                (handle, pointer, frame, gizmo)
        {
            self.drag = Drag::start_many(
                handle,
                frame,
                vec![(skeleton.part, start)],
                Modes::default(),
                screen.ray(pointer),
            )
            .map(|drag| (bone, drag, start));
        }
        out.highlighted = self
            .drag
            .as_ref()
            .map(|(_, drag, _)| drag.handle)
            .or(handle);
        if let Some((bone, drag, start)) = &mut self.drag {
            if !down {
                self.drag = None;
            } else if let Some(pointer) = pointer {
                let snap = Snap {
                    move_on: false,
                    rotate_step: TURN_STEP,
                    ..Snap::default()
                };
                out.event = drag
                    .update_all(screen.ray(pointer), &snap)
                    .and_then(|parts| parts.into_iter().next())
                    .and_then(|(_, values)| posed_by(skeleton, *bone, *start, &values))
                    .map(|transform| BoneEvent::Pose(*bone, transform));
            }
        }
        if response.clicked()
            && let Some(bone) = out.hovered
        {
            self.selected = Some(bone);
            out.event = Some(BoneEvent::Select(bone));
        }
        out.captured = handle.is_some() || self.drag.is_some() || out.hovered.is_some();
        if out.captured {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if let Some(bone) = out.hovered
            && let Some(at) = screen.point(joints[bone])
        {
            let name = &skeleton.rig.bones[bone].name;
            let text = if skeleton.locked(bone) {
                format!("{} {name} (moved by the take)", icons::LOCK)
            } else {
                name.clone()
            };
            ui.painter().text(
                at + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        }
        out
    }

    /// Draws the bones as lines between their joints, the joints (the
    /// selected one in the accent colour, locked ones dimmed), and the
    /// gizmo.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        screen: &Screen,
        skeleton: &Skeleton,
        interaction: &Interaction,
    ) {
        let palette = theme::palette(painter.ctx());
        let joints = skeleton.joints();
        let bone_line = egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(140));
        for (bone, info) in skeleton.rig.bones.iter().enumerate() {
            if let Some(parent) = info.parent.filter(|&p| skeleton.posable(p))
                && skeleton.posable(bone)
                && let Some(line) = screen.line(joints[parent], joints[bone])
            {
                painter.line_segment(line, bone_line);
            }
        }
        for (bone, &joint) in joints.iter().enumerate() {
            if !skeleton.posable(bone) {
                continue;
            }
            let Some(at) = screen.point(joint) else {
                continue;
            };
            let fill = if self.selected == Some(bone) {
                palette.accent
            } else if skeleton.locked(bone) {
                egui::Color32::from_gray(110)
            } else {
                egui::Color32::WHITE
            };
            let radius = if interaction.hovered == Some(bone) {
                JOINT + 1.5
            } else {
                JOINT
            };
            painter.circle(
                at,
                radius,
                fill,
                egui::Stroke::new(1.0_f32, egui::Color32::from_black_alpha(180)),
            );
        }
        gizmo::paint(painter, &interaction.shapes, interaction.highlighted);
    }
}

/// The bone's transform after a gizmo drag from `start` gave the handle
/// these `values` (a turned Orientation, or a moved Position).
fn posed_by(
    skeleton: &Skeleton,
    bone: usize,
    start: gizmo::Pose,
    values: &[(&'static str, axcenity_core::Value)],
) -> Option<BoneTransform> {
    let vector = |name| {
        values
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, value)| value.as_vector3())
    };
    if let Some(orientation) = vector("Orientation") {
        let rotation = axcenity_scene::rotation_from_orientation(orientation);
        return skeleton.turned(bone, rotation * start.rotation.inverse());
    }
    let position = vector("Position")?;
    let by = Vec3::new(position.x, position.y, position.z) - start.position;
    skeleton.moved(bone, by)
}

/// The rig's named bones in order, parents first, each with its depth
/// (unnamed bones are left out and don't count).
pub fn bone_rows(rig: &Rig) -> Vec<(usize, usize)> {
    let mut depths: Vec<Option<usize>> = Vec::with_capacity(rig.bones.len());
    let mut rows = Vec::new();
    for (index, bone) in rig.bones.iter().enumerate() {
        // The depth of the nearest named bone above, if any.
        let mut above = bone.parent;
        let mut parent_depth = None;
        while let Some(parent) = above {
            if let Some(depth) = depths[parent] {
                parent_depth = Some(depth);
                break;
            }
            above = rig.bones[parent].parent;
        }
        if bone.name.is_empty() {
            depths.push(None);
            continue;
        }
        let depth = parent_depth.map_or(0, |depth| depth + 1);
        depths.push(Some(depth));
        rows.push((index, depth));
    }
    rows
}

/// How many levels of a bone tree show when it's first seen.
const OPEN_LEVELS: usize = 3;
/// The width of a bone row's ▸/▾, so leaves line up with branches.
const FOLD_WIDTH: f32 = 16.0;

/// Which bones are folded in each model's bone tree (by `MeshId`), kept
/// while the editor runs. A tree first opens with [`OPEN_LEVELS`] levels
/// showing.
#[derive(Default)]
pub struct Folds {
    models: HashMap<String, HashSet<usize>>,
}

/// One shown row of a bone tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub bone: usize,
    pub depth: usize,
    /// For a bone with bones under it, whether they're folded away.
    pub folded: Option<bool>,
    /// It's folded and a bone under it has keys.
    pub hidden_keys: bool,
}

/// The nearest named bone above each bone of `rig`.
fn named_parents(rig: &Rig) -> Vec<Option<usize>> {
    rig.bones
        .iter()
        .map(|bone| {
            let mut above = bone.parent;
            while let Some(parent) = above {
                if !rig.bones[parent].name.is_empty() {
                    return Some(parent);
                }
                above = rig.bones[parent].parent;
            }
            None
        })
        .collect()
}

/// The named bones of `rig` that have named bones under them.
fn branches(rig: &Rig) -> HashSet<usize> {
    let parents = named_parents(rig);
    bone_rows(rig)
        .into_iter()
        .filter_map(|(bone, _)| parents[bone])
        .collect()
}

impl Folds {
    /// `model`'s folded bones, starting with everything below
    /// [`OPEN_LEVELS`] folded.
    fn folded(&mut self, model: &str, rig: &Rig) -> &mut HashSet<usize> {
        self.models.entry(model.to_owned()).or_insert_with(|| {
            let branches = branches(rig);
            bone_rows(rig)
                .into_iter()
                .filter(|(bone, depth)| *depth + 1 >= OPEN_LEVELS && branches.contains(bone))
                .map(|(bone, _)| bone)
                .collect()
        })
    }

    /// The rows of `model`'s tree that show, parents first. `keys` is each
    /// bone's key count, for the dot on a folded bone.
    pub fn rows(&mut self, model: &str, rig: &Rig, keys: &[usize]) -> Vec<TreeRow> {
        let folded = self.folded(model, rig).clone();
        let parents = named_parents(rig);
        let rows = bone_rows(rig);
        let branches = branches(rig);
        // Keys at or under each bone, children before parents.
        let mut under = vec![0; rig.bones.len()];
        for &(bone, _) in rows.iter().rev() {
            under[bone] += keys.get(bone).copied().unwrap_or(0);
            if let Some(parent) = parents[bone] {
                under[parent] += under[bone];
            }
        }
        let mut hidden = vec![false; rig.bones.len()];
        let mut shown = Vec::new();
        for (bone, depth) in rows {
            hidden[bone] = parents[bone].is_some_and(|p| hidden[p] || folded.contains(&p));
            if hidden[bone] {
                continue;
            }
            let fold = branches.contains(&bone).then(|| folded.contains(&bone));
            let own = keys.get(bone).copied().unwrap_or(0);
            shown.push(TreeRow {
                bone,
                depth,
                folded: fold,
                hidden_keys: fold == Some(true) && under[bone] > own,
            });
        }
        shown
    }

    /// Folds or unfolds `bone` in `model`'s tree.
    pub fn toggle(&mut self, model: &str, rig: &Rig, bone: usize) {
        let folded = self.folded(model, rig);
        if !folded.remove(&bone) {
            folded.insert(bone);
        }
    }

    /// Folds every bone, leaving only the roots showing.
    pub fn collapse_all(&mut self, model: &str, rig: &Rig) {
        *self.folded(model, rig) = branches(rig);
    }

    pub fn expand_all(&mut self, model: &str, rig: &Rig) {
        self.folded(model, rig).clear();
    }

    /// Collapse all and Expand all, for the drawer's header.
    pub fn buttons(&mut self, ui: &mut egui::Ui, model: &str, rig: &Rig) {
        if ui
            .small_button(icons::ARROWS_IN_LINE_VERTICAL)
            .on_hover_text("Collapse all")
            .clicked()
        {
            self.collapse_all(model, rig);
        }
        if ui
            .small_button(icons::ARROWS_OUT_LINE_VERTICAL)
            .on_hover_text("Expand all")
            .clicked()
        {
            self.expand_all(model, rig);
        }
    }
}

/// A ▸ (folded) or ▾ (open) that flips `folded` when clicked, or the room
/// for one when there's nothing to fold (`None`). Returns true if clicked.
pub fn fold_toggle(ui: &mut egui::Ui, folded: Option<bool>) -> bool {
    let Some(folded) = folded else {
        ui.add_space(FOLD_WIDTH);
        return false;
    };
    let icon = if folded {
        icons::CARET_RIGHT
    } else {
        icons::CARET_DOWN
    };
    ui.add_sized(
        [FOLD_WIDTH, ui.spacing().interact_size.y],
        egui::Button::new(icon).frame(false),
    )
    .on_hover_text(if folded { "Unfold" } else { "Fold" })
    .clicked()
}

/// One bone in the drawer: indented by its depth, with ▸/▾ if it has bones
/// under it, 🔒 if `locked`, `keys` (its key count) if it has any, and a
/// dot when it's folded over keys. Clicking an unlocked bone selects it.
/// Returns the row's response, and whether its ▸/▾ was clicked.
pub fn bone_row(
    ui: &mut egui::Ui,
    name: &str,
    row: &TreeRow,
    selected: bool,
    locked: bool,
    keys: usize,
) -> (egui::Response, bool) {
    ui.horizontal(|ui| {
        ui.add_space((row.depth as f32 * 8.0).min(64.0));
        let toggled = fold_toggle(ui, row.folded);
        let mut text = if locked {
            format!("{} {name}", icons::LOCK)
        } else {
            format!("{} {name}", icons::BONE)
        };
        if keys > 0 {
            text.push_str(&format!("  {} {keys}", icons::KEY));
        }
        if row.hidden_keys {
            text.push_str("  •");
        }
        let button = ui.add_enabled(!locked, egui::Button::selectable(selected, text).truncate());
        let button = if locked {
            button.on_disabled_hover_text(format!(
                "{name} is moved by the take, so it can't be keyed. Convert to keys (⋯) to edit it."
            ))
        } else if row.hidden_keys {
            button.on_hover_text(format!("{name}: a bone folded under it has keys"))
        } else {
            button.on_hover_text(format!("{name}: select it to turn it in the preview"))
        };
        (button, toggled)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use axcenity_assets::{Assets, Model};
    use axcenity_core::{ClassName, Value, Vector3};

    use super::*;

    fn noodle() -> Arc<Model> {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation");
        Assets::new(example)
            .model("assets/models/noodle.fbx")
            .unwrap()
    }

    /// A MeshPart twice the noodle's size, turned and moved.
    fn part(world: &mut World, model: &Model) -> InstanceId {
        let part = world.create(ClassName::MeshPart).unwrap();
        let [x, y, z] = model.size.map(|length| length * 2.0);
        for (name, value) in [
            ("Size", Value::Vector3(Vector3::new(x, y, z))),
            ("Position", Value::Vector3(Vector3::new(4.0, 1.5, 0.0))),
            ("Orientation", Value::Vector3(Vector3::new(0.0, 90.0, 0.0))),
        ] {
            world.set_property(part, name, value).unwrap();
        }
        part
    }

    #[test]
    fn turning_a_bone_in_the_world_turns_its_joint_that_way() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let (mid, top) = (rig.bone("Mid").unwrap(), rig.bone("Top").unwrap());
        let joints = skeleton.joints();
        let turn = Quat::from_rotation_x(0.5);
        let turned = skeleton.turned(mid, turn).unwrap();
        let mut after = pose.clone();
        after[mid] = turned;
        let moved = Skeleton::on(&world, part, rig, &after, &[])
            .unwrap()
            .joints();
        // Mid stays put; Top swings around Mid by the same turn in the world.
        assert!(moved[mid].abs_diff_eq(joints[mid], 1e-4));
        let expected = joints[mid] + turn * (joints[top] - joints[mid]);
        assert!(
            moved[top].abs_diff_eq(expected, 1e-3),
            "{:?} vs {expected:?}",
            moved[top]
        );
    }

    #[test]
    fn moving_the_root_moves_its_joint_by_as_much_in_the_world() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let root = rig.bone("Root").unwrap();
        assert!(skeleton.is_root(root));
        assert!(!skeleton.is_root(rig.bone("Mid").unwrap()));
        let by = Vec3::new(0.0, 0.5, 0.25);
        let mut after = pose.clone();
        after[root] = skeleton.moved(root, by).unwrap();
        let moved = Skeleton::on(&world, part, rig, &after, &[])
            .unwrap()
            .joints();
        assert!(moved[root].abs_diff_eq(skeleton.joints()[root] + by, 1e-3));
    }

    #[test]
    fn the_tree_leaves_out_unnamed_bones() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let rows: Vec<(&str, usize)> = bone_rows(rig)
            .into_iter()
            .map(|(bone, depth)| (rig.bones[bone].name.as_str(), depth))
            .collect();
        assert_eq!(rows, [("Root", 0), ("Mid", 1), ("Top", 2), ("Hat", 3)]);
    }

    /// The names of the shown rows of `model`'s tree.
    fn shown(folds: &mut Folds, model: &str, rig: &Rig, keys: &[usize]) -> Vec<String> {
        folds
            .rows(model, rig, keys)
            .into_iter()
            .map(|row| rig.bones[row.bone].name.clone())
            .collect()
    }

    #[test]
    fn a_mixamo_rigs_bone_tree_folds_unfolds_and_collapses_to_its_root() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-events");
        let walking = "assets/models/Walking/Walking.fbx";
        let model = Assets::new(example).model(walking).unwrap();
        let rig = model.rig.as_deref().unwrap();
        let all = bone_rows(rig);
        assert!(all.len() > 40, "a Mixamo rig has many bones");
        let no_keys = vec![0; rig.bones.len()];
        let mut folds = Folds::default();

        // It opens with three levels showing: Hips, Spine, LeftUpLeg… but
        // not what's under them.
        let rows = folds.rows(walking, rig, &no_keys);
        assert!(rows.iter().all(|row| row.depth < OPEN_LEVELS));
        assert!(rows.len() < all.len());
        let spine = rig.bone("mixamorig:Spine").unwrap();
        let spine1 = rig.bone("mixamorig:Spine1").unwrap();
        let spine2 = rig.bone("mixamorig:Spine2").unwrap();
        let row_of = |rows: &[TreeRow], bone| rows.iter().find(|row| row.bone == bone).cloned();
        assert_eq!(row_of(&rows, spine).unwrap().folded, Some(false));
        assert_eq!(row_of(&rows, spine1).unwrap().folded, Some(true));
        assert!(row_of(&rows, spine2).is_none());

        // Unfolding Spine1 shows Spine2 (folded, as it's below the third
        // level); folding Spine hides both.
        folds.toggle(walking, rig, spine1);
        let rows = folds.rows(walking, rig, &no_keys);
        assert_eq!(row_of(&rows, spine2).unwrap().folded, Some(true));
        folds.toggle(walking, rig, spine);
        let rows = folds.rows(walking, rig, &no_keys);
        assert!(row_of(&rows, spine1).is_none() && row_of(&rows, spine2).is_none());
        // A folded bone shows a dot when a bone under it has keys.
        let mut keys = no_keys.clone();
        keys[rig.bone("mixamorig:Head").unwrap()] = 3;
        assert!(
            row_of(&folds.rows(walking, rig, &keys), spine)
                .unwrap()
                .hidden_keys
        );
        // Unfolding Spine again brings Spine1 back as it was: unfolded.
        folds.toggle(walking, rig, spine);
        let rows = folds.rows(walking, rig, &no_keys);
        assert_eq!(row_of(&rows, spine1).unwrap().folded, Some(false));

        // Collapse all leaves only the root; Expand all shows every bone.
        folds.collapse_all(walking, rig);
        assert_eq!(
            shown(&mut folds, walking, rig, &no_keys),
            ["mixamorig:Hips"]
        );
        folds.expand_all(walking, rig);
        assert_eq!(folds.rows(walking, rig, &no_keys).len(), all.len());

        // Each model keeps its own folds.
        let noodle = noodle();
        let noodle_rig = noodle.rig.as_deref().unwrap();
        folds.collapse_all("assets/models/noodle.fbx", noodle_rig);
        assert_eq!(folds.rows(walking, rig, &no_keys).len(), all.len());
        assert_eq!(
            shown(&mut folds, "assets/models/noodle.fbx", noodle_rig, &[0; 8]),
            ["Root"]
        );
    }
}
