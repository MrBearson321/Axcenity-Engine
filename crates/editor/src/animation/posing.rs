//! Posing a skinned model in the Animation tab's previews: its bones as
//! joints you can pick, the gizmo that turns a bone (or moves the root),
//! and the bone tree in the drawer.
//!
//! A [`Skeleton`] is a rig posed on a MeshPart in a preview's world. The
//! gizmo works in the world, as it does on parts; [`Skeleton::turned`] and
//! [`Skeleton::moved`] turn what it did back into the bone's own transform,
//! relative to its parent bone, which is what a `Bone` key stores.
//!
//! [`PoseSettings`] are the snap steps and axes above the preview, kept
//! while the editor runs. Holding Ctrl (Cmd on macOS) during a drag flips
//! snapping for as long as it's held; Shift turns finely.

use std::collections::{HashMap, HashSet};

use axcenity_assets::{BoneTransform, Rig};
use axcenity_core::{InstanceId, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui;

use crate::gizmo::{self, Drag, Handle, Modes, MoveAxes, Pointer, RotatePivot, Snap, Tool};
use crate::screen::Screen;
use crate::theme::{self, icons};

/// How close (in points) the mouse must be to a joint to pick it.
const PICK: f32 = 9.0;
/// A joint's radius, in points.
const JOINT: f32 = 4.0;
/// The Turn snap steps, in degrees, and the Move snap steps, in studs.
pub const TURN_STEPS: [f32; 4] = [1.0, 5.0, 15.0, 45.0];
pub const MOVE_STEPS: [f32; 4] = [0.01, 0.05, 0.1, 0.5];

/// Which axes the gizmo turns and moves a bone along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axes {
    /// The world's.
    #[default]
    World,
    /// The bone's own.
    Local,
}

/// The snap steps and axes posing uses, set above the preview. Off keeps
/// its step, which holding Ctrl snaps to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseSettings {
    pub snap: Snap,
    pub axes: Axes,
}

impl Default for PoseSettings {
    /// Turns in 5° steps and moves in 0.05-stud steps, on the world's axes.
    fn default() -> Self {
        Self {
            snap: Snap {
                move_on: true,
                move_step: 0.05,
                rotate_on: true,
                rotate_step: 5.0,
            },
            axes: Axes::World,
        }
    }
}

impl PoseSettings {
    /// The gizmo's tool options for these axes.
    fn modes(&self) -> Modes {
        match self.axes {
            Axes::World => Modes::default(),
            Axes::Local => Modes {
                axes: MoveAxes::Local,
                pivot: RotatePivot::Local,
                ..Modes::default()
            },
        }
    }

    /// Turn snap, Move snap and World/Local, for the toolbar.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let snap = &mut self.snap;
        snap_picker(
            ui,
            "turn snap",
            icons::ARROW_CLOCKWISE,
            "Turn snap: bones turn in steps of this. Hold Ctrl to flip it while dragging.",
            (&mut snap.rotate_on, &mut snap.rotate_step),
            &TURN_STEPS,
            |step| format!("{step}°"),
        );
        snap_picker(
            ui,
            "move snap",
            icons::ARROWS_OUT_CARDINAL,
            "Move snap: the root moves in steps of this. Hold Ctrl to flip it while dragging.",
            (&mut snap.move_on, &mut snap.move_step),
            &MOVE_STEPS,
            |step| format!("{step} studs"),
        );
        let (text, hover) = match self.axes {
            Axes::World => (
                format!("{} World", icons::GLOBE),
                "The gizmo follows the world's axes. Click for the bone's own.",
            ),
            Axes::Local => (
                format!("{} Local", icons::CUBE),
                "The gizmo follows the bone's own axes. Click for the world's.",
            ),
        };
        if ui.button(text).on_hover_text(hover).clicked() {
            self.axes = match self.axes {
                Axes::World => Axes::Local,
                Axes::Local => Axes::World,
            };
        }
    }
}

/// A snap setting's menu: Off, then each of `steps`. Off keeps the step.
fn snap_picker(
    ui: &mut egui::Ui,
    id: &str,
    icon: &str,
    hover: &str,
    (on, step): (&mut bool, &mut f32),
    steps: &[f32],
    name: fn(f32) -> String,
) {
    let shown = if *on { name(*step) } else { "Off".to_owned() };
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{icon} {shown}"))
        .show_ui(ui, |ui| {
            if ui.selectable_label(!*on, "Off").clicked() {
                *on = false;
            }
            for &choice in steps {
                if ui
                    .selectable_label(*on && *step == choice, name(choice))
                    .clicked()
                {
                    *on = true;
                    *step = choice;
                }
            }
        })
        .response
        .on_hover_text(hover);
}

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

    /// The Rotate and Move buttons (Move only with a root selected), then
    /// the snap steps and axes.
    pub fn tools(&mut self, ui: &mut egui::Ui, rig: &Rig, settings: &mut PoseSettings) {
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
        settings.controls(ui);
    }

    /// The selected bone, where its gizmo starts, the tool it shows and
    /// where its handles go with `settings`' axes, unless nothing posable
    /// is selected.
    fn gizmo_frame(
        &self,
        skeleton: &Skeleton,
        settings: &PoseSettings,
    ) -> Option<(usize, gizmo::Pose, Tool, gizmo::Pose)> {
        let bone = self.selected.filter(|&bone| !skeleton.locked(bone))?;
        let pose = skeleton.gizmo_pose(bone)?;
        let tool = self.tool_for(skeleton, bone);
        let frame = gizmo::handle_frame(tool, settings.modes(), &[pose])?;
        Some((bone, pose, tool, frame))
    }

    /// Picks joints and drags the selected bone's gizmo, in a preview drawn
    /// in `response`'s rectangle through `screen`, snapping and turning on
    /// the axes `settings` say.
    pub fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        skeleton: &Skeleton,
        settings: &PoseSettings,
    ) -> Interaction {
        let mut out = Interaction::default();
        if self.selected.is_some_and(|bone| !skeleton.posable(bone)) {
            self.selected = None;
        }
        let gizmo = self.gizmo_frame(skeleton, settings);
        out.shapes = gizmo.map_or_else(Vec::new, |(_, _, tool, frame)| {
            gizmo::shapes(tool, frame, screen)
        });

        let (pressed, down, pointer, flip, fine) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.modifiers.command,
                input.modifiers.shift,
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
            && let (Some(handle), Some(pointer), Some((bone, start, _, frame))) =
                (handle, pointer, gizmo)
        {
            self.drag = Drag::start_many(
                handle,
                frame,
                vec![(skeleton.part, start)],
                settings.modes(),
                screen,
                pointer,
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
                // Ctrl flips snapping only while it's held: the drag works
                // its pose out from where it started each frame.
                let snap = if flip {
                    settings.snap.flipped()
                } else {
                    settings.snap
                };
                out.event = drag
                    .update_all(Pointer::on(screen, pointer, fine), &snap)
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
    use axcenity_render::Camera;

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

    /// A camera `back` model heights in front of `joint` (and `off` to the
    /// side and up).
    fn camera_at(model: &Model, joint: Vec3, off: Vec3) -> Camera {
        let height = model.size.iter().copied().fold(0.0, f32::max) * 2.0;
        Camera {
            position: joint + (Vec3::Z * 3.0 + off) * height,
            look_at: joint,
            fov_y_degrees: 50.0,
        }
    }

    /// One frame of a preview filling a 640×480 window, seen through
    /// `camera`: what posing found, and the screen it drew through.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the frame.
    fn frame(
        ctx: &egui::Context,
        camera: Camera,
        poser: &mut Poser,
        skeleton: &Skeleton,
        settings: &PoseSettings,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> (Interaction, Screen) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events,
            modifiers,
            ..Default::default()
        };
        let mut out = None;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let size = ui.available_size();
                    let (rect, response) =
                        ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                    let screen = Screen::new(camera, rect);
                    let done = poser.interact(ui, &response, &screen, skeleton, settings);
                    out = Some((done, screen));
                });
        });
        out.expect("drawn")
    }

    /// Grabs the selected bone's `handle` and moves the mouse each of
    /// `moves` points from where it grabbed, forward along the handle as
    /// drawn there, with those keys held, then lets go.
    /// Returns the bone's transform after each move.
    fn drag_handle(
        camera: Camera,
        poser: &mut Poser,
        skeleton: &Skeleton,
        settings: &PoseSettings,
        handle: Handle,
        moves: &[(f32, egui::Modifiers)],
    ) -> Vec<BoneTransform> {
        let ctx = theme::test_context();
        let none = egui::Modifiers::NONE;
        let away = vec![egui::Event::PointerMoved(egui::pos2(2.0, 2.0))];
        let (done, _) = frame(&ctx, camera, poser, skeleton, settings, away, none);
        let shape = done
            .shapes
            .iter()
            .find(|shape| shape.handle == handle)
            .expect("the handle shows");
        let lines = shape.lines();
        let [a, b] = lines[lines.len() / 2];
        let grab = a.lerp(b, 0.5);
        let forward = (b - a).normalized();
        let at = |pos| vec![egui::Event::PointerMoved(pos)];
        frame(&ctx, camera, poser, skeleton, settings, at(grab), none);
        let press = |pressed| egui::Event::PointerButton {
            pos: grab,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: none,
        };
        frame(
            &ctx,
            camera,
            poser,
            skeleton,
            settings,
            vec![press(true)],
            none,
        );
        assert!(poser.drag.is_some(), "{handle:?} was grabbed");
        let mut posed = Vec::new();
        for &(by, modifiers) in moves {
            let (done, _) = frame(
                &ctx,
                camera,
                poser,
                skeleton,
                settings,
                at(grab + forward * by),
                modifiers,
            );
            match done.event {
                Some(BoneEvent::Pose(_, transform)) => posed.push(transform),
                other => panic!("the bone wasn't posed: {other:?}"),
            }
        }
        frame(
            &ctx,
            camera,
            poser,
            skeleton,
            settings,
            vec![press(false)],
            none,
        );
        posed
    }

    const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
    const NONE: egui::Modifiers = egui::Modifiers::NONE;

    /// How far `bone`'s joint is moved in the world with `transform`.
    fn joint_moved(skeleton: &Skeleton, bone: usize, transform: BoneTransform) -> Vec3 {
        let mut pose = skeleton.pose.to_vec();
        pose[bone] = transform;
        let moved = Skeleton {
            pose: &pose,
            ..*skeleton
        }
        .joints();
        moved[bone] - skeleton.joints()[bone]
    }

    /// How many `step`s `distance` is, and how far that is from whole.
    fn steps(distance: f32, step: f32) -> (f32, f32) {
        let steps = distance / step;
        (steps, (steps - steps.round()).abs())
    }

    #[test]
    fn move_snap_moves_the_root_in_steps_and_ctrl_lets_it_go_free_while_held() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let root = rig.bone("Root").unwrap();
        let camera = camera_at(&model, skeleton.joints()[root], Vec3::new(0.3, 0.4, 0.0));
        let mut poser = Poser {
            selected: Some(root),
            tool: Tool::Move,
            ..Poser::default()
        };
        let settings = PoseSettings::default();
        assert_eq!(settings.snap.move_step, 0.05, "it starts at 0.05 studs");
        let posed = drag_handle(
            camera,
            &mut poser,
            &skeleton,
            &settings,
            Handle::Move(gizmo::Axis::X),
            &[(23.0, NONE), (61.0, NONE), (61.0, CTRL), (61.0, NONE)],
        );
        let moved: Vec<Vec3> = posed
            .into_iter()
            .map(|transform| joint_moved(&skeleton, root, transform))
            .collect();
        for (i, by) in moved.iter().enumerate() {
            assert!(by.y.abs() < 1e-4 && by.z.abs() < 1e-4, "only along X: {by}");
            assert!(by.x > 0.0, "move {i} goes right: {by}");
        }
        let (count, off) = steps(moved[0].x, 0.05);
        assert!(
            off < 1e-2 && count >= 1.0,
            "a whole number of steps: {count}"
        );
        assert!(steps(moved[1].x, 0.05).1 < 1e-2);
        // Ctrl: free, off the steps; letting go snaps back to the step.
        let (_, free_off) = steps(moved[2].x, 0.05);
        assert!(free_off > 0.02, "free with Ctrl: {}", moved[2].x);
        assert!((moved[3].x - moved[1].x).abs() < 1e-4, "snapped again");
        assert!((moved[3].x - (moved[2].x / 0.05).round() * 0.05).abs() < 1e-4);
    }

    #[test]
    fn ctrl_flips_snapping_for_turns_and_moves_whether_it_is_off_or_on() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let top = rig.bone("Top").unwrap();
        let root = rig.bone("Root").unwrap();
        let joints = skeleton.joints();
        let camera = camera_at(&model, joints[top], Vec3::ZERO);
        // How far each drag turns Top with its Z ring (facing the camera),
        // in degrees, with and without Ctrl.
        let turns = |settings: &PoseSettings| -> Vec<f32> {
            let mut poser = Poser {
                selected: Some(top),
                ..Poser::default()
            };
            let along = 50.0;
            drag_handle(
                camera,
                &mut poser,
                &skeleton,
                settings,
                Handle::Rotate(gizmo::Axis::Z),
                &[(along, NONE), (along, CTRL), (along, NONE)],
            )
            .into_iter()
            .map(|t| t.rotation.angle_between(pose[top].rotation).to_degrees())
            .collect()
        };
        let mut fifteen = PoseSettings::default();
        fifteen.snap.rotate_step = 15.0;
        let snapped = turns(&fifteen);
        assert!(
            steps(snapped[0], 15.0).1 < 1e-2 && snapped[0] > 1.0,
            "{snapped:?}"
        );
        assert!(
            steps(snapped[1], 15.0).1 > 0.05,
            "Ctrl turns freely: {snapped:?}"
        );
        assert_eq!(snapped[0], snapped[2], "letting go snaps again");

        let mut off = fifteen;
        off.snap.rotate_on = false;
        let free = turns(&off);
        assert!(steps(free[0], 15.0).1 > 0.05, "Off turns freely: {free:?}");
        assert!((free[0] - snapped[1]).abs() < 1e-2, "as freely as Ctrl did");
        assert!(
            (free[1] - snapped[0]).abs() < 1e-2,
            "Ctrl snaps to the step Off kept"
        );
        assert!((free[2] - free[0]).abs() < 1e-3);

        // Moves too: Off with Ctrl moves in the kept 0.05-stud steps.
        let mut poser = Poser {
            selected: Some(root),
            tool: Tool::Move,
            ..Poser::default()
        };
        let mut no_move_snap = PoseSettings::default();
        no_move_snap.snap.move_on = false;
        let camera = camera_at(&model, joints[root], Vec3::new(0.3, 0.4, 0.0));
        let right = 61.0;
        let moved: Vec<f32> = drag_handle(
            camera,
            &mut poser,
            &skeleton,
            &no_move_snap,
            Handle::Move(gizmo::Axis::X),
            &[(right, NONE), (right, CTRL)],
        )
        .into_iter()
        .map(|t| joint_moved(&skeleton, root, t).x)
        .collect();
        assert!(steps(moved[0], 0.05).1 > 0.02, "free when Off: {moved:?}");
        assert!(
            steps(moved[1], 0.05).1 < 1e-2,
            "stepped with Ctrl: {moved:?}"
        );
    }

    #[test]
    fn world_and_local_turn_the_gizmo_without_moving_the_bone() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let top = rig.bone("Top").unwrap();
        let camera = camera_at(&model, skeleton.joints()[top], Vec3::new(0.8, 0.6, 0.0));
        let mut poser = Poser {
            selected: Some(top),
            ..Poser::default()
        };
        let mut settings = PoseSettings::default();
        settings.snap.rotate_on = false;
        let bone_turn = skeleton.gizmo_pose(top).unwrap().rotation;
        assert!(
            bone_turn.angle_between(Quat::IDENTITY) > 0.1,
            "Top is turned in the world"
        );

        // Switching moves only the gizmo: same place, the bone's own turn,
        // and nothing is posed.
        let (_, _, _, world_frame) = poser.gizmo_frame(&skeleton, &settings).unwrap();
        settings.axes = Axes::Local;
        let (_, _, _, local_frame) = poser.gizmo_frame(&skeleton, &settings).unwrap();
        assert!(world_frame.position.abs_diff_eq(local_frame.position, 1e-5));
        assert!(world_frame.rotation.angle_between(Quat::IDENTITY) < 1e-5);
        assert!(local_frame.rotation.angle_between(bone_turn) < 1e-5);
        let ctx = theme::test_context();
        for axes in [Axes::World, Axes::Local, Axes::World] {
            settings.axes = axes;
            let still = vec![egui::Event::PointerMoved(egui::pos2(2.0, 2.0))];
            let (done, _) = frame(&ctx, camera, &mut poser, &skeleton, &settings, still, NONE);
            assert_eq!(done.event, None, "switching to {axes:?} poses nothing");
        }

        // Dragging the X ring turns Top around the world's X, or its own.
        let turn_axis = |settings: &PoseSettings, poser: &mut Poser| {
            let posed = drag_handle(
                camera,
                poser,
                &skeleton,
                settings,
                Handle::Rotate(gizmo::Axis::X),
                &[(30.0, NONE)],
            );
            let mut after = pose.clone();
            after[top] = posed[0];
            let turned = Skeleton {
                pose: &after,
                ..skeleton
            }
            .gizmo_pose(top)
            .unwrap()
            .rotation;
            let (axis, angle) = (turned * bone_turn.inverse()).to_axis_angle();
            assert!(angle > 0.05, "it turned");
            // Turning around the joint leaves the joint where it was.
            let joint = Skeleton {
                pose: &after,
                ..skeleton
            }
            .joints()[top];
            assert!(joint.abs_diff_eq(skeleton.joints()[top], 1e-4));
            axis
        };
        settings.axes = Axes::World;
        let around = turn_axis(&settings, &mut poser);
        assert!(around.dot(Vec3::X).abs() > 0.999, "the world's X: {around}");
        settings.axes = Axes::Local;
        let around = turn_axis(&settings, &mut poser);
        let own_x = bone_turn * Vec3::X;
        assert!(
            around.dot(own_x).abs() > 0.999,
            "Top's own X: {around} vs {own_x}"
        );
    }

    #[test]
    fn shift_turns_a_bone_a_quarter_as_fast_and_letting_go_does_not_jump() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let mut world = World::new();
        let part = part(&mut world, &model);
        let pose = rig.rest_pose();
        let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
        let top = rig.bone("Top").unwrap();
        let camera = camera_at(&model, skeleton.joints()[top], Vec3::ZERO);
        let mut poser = Poser {
            selected: Some(top),
            ..Poser::default()
        };
        let mut settings = PoseSettings::default();
        settings.snap.rotate_on = false;
        let shift = egui::Modifiers::SHIFT;
        let angles: Vec<f32> = drag_handle(
            camera,
            &mut poser,
            &skeleton,
            &settings,
            Handle::Rotate(gizmo::Axis::Z),
            &[(40.0, NONE), (80.0, shift), (80.0, NONE), (120.0, NONE)],
        )
        .into_iter()
        .map(|t| t.rotation.angle_between(pose[top].rotation))
        .collect();
        let normal = angles[0];
        assert!(
            (normal - 40.0 * gizmo::TURN_PER_POINT).abs() < 1e-3,
            "{angles:?}"
        );
        assert!(
            (angles[1] - normal * 1.25).abs() < 1e-3,
            "a quarter: {angles:?}"
        );
        assert!((angles[2] - angles[1]).abs() < 1e-4, "no jump: {angles:?}");
        assert!(
            (angles[3] - normal * 2.25).abs() < 1e-3,
            "full speed: {angles:?}"
        );
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
