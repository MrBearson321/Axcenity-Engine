//! Posing a skinned model in the Animation tab's previews: its bones as
//! joints you can pick, the gizmo that turns a bone or moves a joint, pins,
//! and the bone tree in the drawer.
//!
//! A [`Skeleton`] is a rig posed on a MeshPart in a preview's world. The
//! gizmo works in the world, as it does on parts; [`Skeleton::turned`] and
//! [`Skeleton::moved`] turn what it did back into the bone's own transform,
//! relative to its parent bone, which is what a `Bone` key stores.
//!
//! Moving a joint that isn't a root turns the bones above it so it gets
//! there (`axcenity_anim::ik`), [`PoseSettings::chain`] of them. A pinned
//! joint stays where it was pinned: after any pose, its own chain turns to
//! put it back ([`Skeleton::pose_with`]). Pins are kept per clip while the
//! editor runs, in the rig's file space, and never saved.
//!
//! [`PoseSettings`] are the snap steps, axes and chain above the preview,
//! kept while the editor runs. Holding Ctrl (Cmd on macOS) during a drag
//! flips snapping for as long as it's held; Shift turns finely. Dragging
//! the selected joint itself (with Move) moves it on a plane facing the
//! camera.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use axcenity_anim::ik;
use axcenity_assets::{BoneTransform, Rig};
use axcenity_core::{InstanceId, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui;

use crate::gizmo::{self, Drag, Handle, Modes, MoveAxes, Pointer, RotatePivot, Snap, Tool};
use crate::screen::Screen;
use crate::theme::{self, icons};

mod tree;

pub use tree::{Folds, bone_row, bone_rows, fold_toggle};
#[cfg(test)]
use tree::{OPEN_LEVELS, TreeRow, named_parents};

/// How close (in points) the mouse must be to a joint to pick it.
const PICK: f32 = 9.0;
/// A joint's radius, in points.
const JOINT: f32 = 4.0;
/// The Turn snap steps, in degrees, and the Move snap steps, in studs.
pub const TURN_STEPS: [f32; 4] = [1.0, 5.0, 15.0, 45.0];
pub const MOVE_STEPS: [f32; 4] = [0.01, 0.05, 0.1, 0.5];
/// How many bones above a moved joint can turn.
pub const CHAIN_LENGTHS: [usize; 4] = [1, 2, 3, 4];

/// Pinned joints, by bone, where they're pinned in the rig's file space.
pub type Pins = BTreeMap<usize, Vec3>;
static NO_PINS: Pins = BTreeMap::new();

/// Which axes the gizmo turns and moves a bone along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axes {
    /// The world's.
    #[default]
    World,
    /// The bone's own.
    Local,
}

/// The snap steps, axes and chain posing uses, set above the preview. Off
/// keeps its step, which holding Ctrl snaps to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseSettings {
    pub snap: Snap,
    pub axes: Axes,
    /// How many bones above a moved joint turn to move it (1 to 4).
    pub chain: usize,
}

impl Default for PoseSettings {
    /// Turns in 5° steps and moves in 0.05-stud steps, on the world's axes,
    /// turning a moved joint's parent and grandparent.
    fn default() -> Self {
        Self {
            snap: Snap {
                move_on: true,
                move_step: 0.05,
                rotate_on: true,
                rotate_step: 5.0,
            },
            axes: Axes::World,
            chain: 2,
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
            "Move snap: joints move in steps of this. Hold Ctrl to flip it while dragging.",
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

/// The Chain menu: how many bones above a moved joint turn.
fn chain_picker(ui: &mut egui::Ui, chain: &mut usize) {
    egui::ComboBox::from_id_salt("ik chain")
        .selected_text(format!("{} Chain {chain}", icons::LINK))
        .show_ui(ui, |ui| {
            for length in CHAIN_LENGTHS {
                let name = match length {
                    1 => "1 bone".to_owned(),
                    n => format!("{n} bones"),
                };
                ui.selectable_value(chain, length, name);
            }
        })
        .response
        .on_hover_text(
            "Chain: how many bones above a moved joint turn to get it there. 2 turns a hand's forearm and upper arm.",
        );
}

/// How a bone was posed by hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Posing {
    /// Turned (or, a root, moved) to this.
    Set(usize, BoneTransform),
    /// Its joint moved to this point in the world: a root moves there, and
    /// any other joint is reached by turning the bones above it.
    Reach(usize, Vec3),
}

impl Posing {
    /// The bone posed by hand.
    pub fn bone(&self) -> usize {
        match *self {
            Posing::Set(bone, _) | Posing::Reach(bone, _) => bone,
        }
    }
}

/// A skeleton after [`Skeleton::pose_with`].
#[derive(Clone, Debug, PartialEq)]
pub struct Posed {
    pub pose: Vec<BoneTransform>,
    /// The bones it changed, in bone order.
    pub changed: Vec<usize>,
    /// Pinned joints that couldn't be put back where they were pinned.
    pub slipped: Vec<usize>,
}

impl Posed {
    /// The bones to send (and so key) for `posing`: those it changed, and
    /// a bone turned by hand even when it's turned back to where it was.
    /// A joint moved with IK isn't itself turned, so it isn't sent.
    pub fn sent(&self, posing: Posing) -> BTreeSet<usize> {
        let mut bones: BTreeSet<usize> = self.changed.iter().copied().collect();
        if let Posing::Set(bone, _) = posing {
            bones.insert(bone);
        }
        bones
    }
}

/// Whether two transforms of a bone differ by more than rounding.
fn differs(a: &BoneTransform, b: &BoneTransform) -> bool {
    let turn = (a.rotation - b.rotation)
        .length()
        .min((a.rotation + b.rotation).length());
    turn > 1e-5
        || !a.translation.abs_diff_eq(b.translation, 1e-6)
        || !a.scale.abs_diff_eq(b.scale, 1e-6)
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

    /// From the rig's file space (where pins are kept) into the world.
    pub fn to_world(&self) -> Mat4 {
        self.place * self.rig.fit
    }

    /// Where each bone's joint is in the world.
    pub fn joints(&self) -> Vec<Vec3> {
        let to_world = self.to_world();
        self.file_joints()
            .into_iter()
            .map(|joint| to_world.transform_point3(joint))
            .collect()
    }

    /// Where each bone's joint is in the rig's file space.
    pub fn file_joints(&self) -> Vec<Vec3> {
        ik::joint_positions(self.rig, self.pose)
    }

    /// The pose after `posing`, then with every other pinned joint in
    /// `pins` put back where it was pinned by turning up to `chain` bones
    /// above it. A moved joint's chain, and a pin's, never turns a bone
    /// above another pinned joint, a locked bone, or the bone posed by
    /// hand, so the result is always the same and a pin only slips (and is
    /// reported) when what may turn can't reach it.
    pub fn pose_with(&self, posing: Posing, chain: usize, pins: &Pins) -> Posed {
        let mut pose = self.pose.to_vec();
        // Bones a short pose leaves out rest.
        let resting = self.rig.bones.get(pose.len()..).unwrap_or_default();
        pose.extend(resting.iter().map(|bone| bone.rest));
        let mut goals = Vec::new();
        let bone = match posing {
            Posing::Set(bone, transform) => {
                if let Some(slot) = pose.get_mut(bone) {
                    *slot = transform;
                }
                bone
            }
            Posing::Reach(bone, at) if self.is_root(bone) => {
                let by = self.joints().get(bone).map(|&joint| at - joint);
                if let Some(moved) = by.and_then(|by| self.moved(bone, by)) {
                    pose[bone] = moved;
                }
                bone
            }
            Posing::Reach(bone, at) => {
                let target = self.to_world().inverse().transform_point3(at);
                goals.push(ik::Goal {
                    joint: bone,
                    target,
                });
                bone
            }
        };
        goals.extend(
            pins.iter()
                .filter(|(pinned, _)| **pinned != bone)
                .map(|(&joint, &target)| ik::Goal { joint, target }),
        );
        let missed = ik::solve(self.rig, &mut pose, &goals, chain, |turning| {
            turning != bone && self.posable(turning) && !self.locked(turning)
        });
        let changed = (0..pose.len())
            .filter(|&b| self.pose.get(b).is_none_or(|was| differs(&pose[b], was)))
            .collect();
        Posed {
            pose,
            changed,
            slipped: missed.into_iter().filter(|&joint| joint != bone).collect(),
        }
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
    /// Bones were posed to these: the one dragged, and the ones turned to
    /// move its joint or to keep pinned joints in place.
    Pose(Vec<(usize, BoneTransform)>),
    /// A drag ended with pinned joints it couldn't keep in place.
    Warn(String),
}

/// What a pose that let these pinned joints of `rig` slip says.
pub fn slipped_message(rig: &Rig, slipped: &[usize]) -> String {
    let names: Vec<&str> = slipped
        .iter()
        .filter_map(|&bone| rig.bones.get(bone).map(|b| b.name.as_str()))
        .collect();
    format!(
        "Pinned {} couldn't stay in place: the bones allowed to turn can't reach. Try a longer Chain, or unpin it.",
        names.join(", ")
    )
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

/// How a drag holds on.
enum Grip {
    /// By a gizmo handle, which was at this pose.
    Handle(Drag, gizmo::Pose),
    /// By the joint itself, which was at `joint`, on a plane facing the
    /// camera through it with this `normal`, grabbed at `grab`.
    Free {
        joint: Vec3,
        grab: Vec3,
        normal: Vec3,
    },
}

/// A bone being dragged.
struct Dragging {
    bone: usize,
    grip: Grip,
    /// The bones as they were when the drag started. Each frame poses from
    /// these, so a drag never adds up on itself.
    pose: Vec<BoneTransform>,
    /// Every bone the drag has changed, so one it turns back is sent too.
    touched: BTreeSet<usize>,
    /// Pinned joints the drag can't keep in place, as of the last frame.
    slipped: Vec<usize>,
}

/// The selected bone, the tool that poses it, its drag, and each clip's
/// pins.
pub struct Poser {
    pub selected: Option<usize>,
    /// Rotate, or Move.
    pub tool: Tool,
    drag: Option<Dragging>,
    /// Each clip's pinned joints, by the clip's path.
    pins: HashMap<String, Pins>,
    /// The clip being posed, whose pins are held and shown.
    clip: String,
    /// The joint whose right-click menu is open.
    menu: Option<usize>,
}

impl Default for Poser {
    fn default() -> Self {
        Self {
            selected: None,
            tool: Tool::Rotate,
            drag: None,
            pins: HashMap::new(),
            clip: String::new(),
            menu: None,
        }
    }
}

impl Poser {
    /// Poses the clip at `path` from now on, with its own pins.
    pub fn for_clip(&mut self, path: &str) {
        if self.clip != path {
            self.clip = path.to_owned();
            self.drag = None;
            self.menu = None;
        }
    }

    /// The pinned joints of the clip being posed.
    pub fn pins(&self) -> &Pins {
        self.pins.get(&self.clip).unwrap_or(&NO_PINS)
    }

    /// Pins `bone`'s joint where it is in `skeleton`.
    pub fn pin(&mut self, skeleton: &Skeleton, bone: usize) {
        if let Some(&at) = skeleton.file_joints().get(bone) {
            self.pins
                .entry(self.clip.clone())
                .or_default()
                .insert(bone, at);
        }
    }

    pub fn unpin(&mut self, bone: usize) {
        if let Some(pins) = self.pins.get_mut(&self.clip) {
            pins.remove(&bone);
        }
    }

    /// `posing` done to `skeleton` with `settings`' chain, holding the
    /// clip's pins. A pinned joint posed by hand is pinned where it goes.
    pub fn pose(&mut self, skeleton: &Skeleton, posing: Posing, settings: &PoseSettings) -> Posed {
        let posed = skeleton.pose_with(posing, settings.chain, self.pins());
        self.repin(skeleton.rig, posing.bone(), &posed.pose);
        posed
    }

    /// Moves `bone`'s pin, if it has one, to where it is in `pose`.
    fn repin(&mut self, rig: &Rig, bone: usize, pose: &[BoneTransform]) {
        if let Some(pin) = self
            .pins
            .get_mut(&self.clip)
            .and_then(|pins| pins.get_mut(&bone))
            && let Some(&at) = ik::joint_positions(rig, pose).get(bone)
        {
            *pin = at;
        }
    }

    /// The Rotate and Move buttons, how many bones a moved joint turns (for
    /// a joint that isn't a root), then the snap steps and axes.
    pub fn tools(&mut self, ui: &mut egui::Ui, rig: &Rig, settings: &mut PoseSettings) {
        let Some(bone) = self.selected else {
            return;
        };
        let root = is_root(rig, bone);
        if crate::widgets::icon_button(
            ui,
            icons::ARROW_CLOCKWISE,
            "Turn the bone",
            self.tool == Tool::Rotate,
        )
        .clicked()
        {
            self.tool = Tool::Rotate;
        }
        let hover = if root {
            "Move the root"
        } else {
            "Move the joint: the bones above it turn to get it there. Drag an arrow, or the joint itself."
        };
        if crate::widgets::icon_button(
            ui,
            icons::ARROWS_OUT_CARDINAL,
            hover,
            self.tool == Tool::Move,
        )
        .clicked()
        {
            self.tool = Tool::Move;
        }
        if self.tool == Tool::Move && !root {
            chain_picker(ui, &mut settings.chain);
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
        let tool = self.tool;
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
        let joints = skeleton.joints();
        // With Move, the middle of the gizmo is the joint itself, which
        // drags freely rather than along an arrow.
        let on_joint = |pointer: egui::Pos2| {
            self.tool == Tool::Move
                && gizmo.is_some_and(|(bone, ..)| {
                    screen
                        .point(joints[bone])
                        .is_some_and(|at| at.distance(pointer) <= JOINT + 2.0)
                })
        };
        let handle = match over {
            Some(pointer) if self.drag.is_none() && !on_joint(pointer) => {
                gizmo::hovered(&out.shapes, pointer)
            }
            _ => None,
        };
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

        let grip = match (pressed, handle, pointer, gizmo) {
            (true, Some(handle), Some(pointer), Some((bone, start, _, frame))) => Drag::start_many(
                handle,
                frame,
                vec![(skeleton.part, start)],
                settings.modes(),
                screen,
                pointer,
            )
            .map(|drag| (bone, Grip::Handle(drag, start))),
            // With Move, the selected joint itself can be dragged.
            (true, None, Some(pointer), Some((bone, ..)))
                if self.tool == Tool::Move && out.hovered == Some(bone) =>
            {
                let joint = joints[bone];
                let normal = screen.ray(pointer).direction;
                on_plane(screen.ray(pointer), joint, normal).map(|grab| {
                    (
                        bone,
                        Grip::Free {
                            joint,
                            grab,
                            normal,
                        },
                    )
                })
            }
            _ => None,
        };
        if let Some((bone, grip)) = grip {
            self.drag = Some(Dragging {
                bone,
                grip,
                pose: skeleton.pose.to_vec(),
                touched: BTreeSet::new(),
                slipped: Vec::new(),
            });
        }
        out.highlighted = match &self.drag {
            Some(Dragging {
                grip: Grip::Handle(drag, _),
                ..
            }) => Some(drag.handle),
            _ => handle,
        };
        // Out of `self` while it poses, which may move a pin.
        if let Some(mut dragging) = self.drag.take() {
            if !down {
                if !dragging.slipped.is_empty() {
                    let message = slipped_message(skeleton.rig, &dragging.slipped);
                    out.event = Some(BoneEvent::Warn(message));
                }
            } else if let Some(pointer) = pointer {
                // Ctrl flips snapping only while it's held: the drag works
                // its pose out from where it started each frame.
                let snap = if flip {
                    settings.snap.flipped()
                } else {
                    settings.snap
                };
                let bone = dragging.bone;
                let start = Skeleton {
                    pose: &dragging.pose,
                    ..*skeleton
                };
                let posing = match &mut dragging.grip {
                    Grip::Handle(drag, at) => drag
                        .update_all(Pointer::on(screen, pointer, fine), &snap)
                        .and_then(|parts| parts.into_iter().next())
                        .and_then(|(_, values)| posing_by(&start, bone, *at, &values)),
                    Grip::Free {
                        joint,
                        grab,
                        normal,
                    } => on_plane(screen.ray(pointer), *grab, *normal)
                        .map(|hit| Posing::Reach(bone, *joint + snapped(hit - *grab, &snap))),
                };
                if let Some(posing) = posing {
                    let posed = self.pose(&start, posing, settings);
                    dragging.touched.extend(posed.sent(posing));
                    dragging.slipped = posed.slipped;
                    if !dragging.touched.is_empty() {
                        out.event = Some(BoneEvent::Pose(
                            dragging
                                .touched
                                .iter()
                                .map(|&b| (b, posed.pose[b]))
                                .collect(),
                        ));
                    }
                }
                self.drag = Some(dragging);
            } else {
                self.drag = Some(dragging);
            }
        }
        if response.clicked()
            && let Some(bone) = out.hovered
        {
            self.selected = Some(bone);
            out.event = Some(BoneEvent::Select(bone));
        }
        // Right-click a joint to pin or unpin it.
        if response.secondary_clicked() {
            self.menu = out.hovered;
        }
        if let Some(bone) = self.menu {
            let pinned = self.pins().contains_key(&bone);
            let name = skeleton.rig.bones[bone].name.clone();
            let mut toggle = false;
            let open = response.context_menu(|ui| {
                ui.label(egui::RichText::new(&name).strong());
                let (text, hover) = if pinned {
                    (
                        format!("{} Unpin", icons::PUSH_PIN_SLASH),
                        "Let the joint move with the bones above it again",
                    )
                } else {
                    (
                        format!("{} Pin", icons::PUSH_PIN),
                        "Keep the joint where it is in the world: when other bones move, the bones above it turn to hold it there",
                    )
                };
                if ui.button(text).on_hover_text(hover).clicked() {
                    toggle = true;
                    ui.close();
                }
            });
            match (toggle, pinned) {
                (true, true) => self.unpin(bone),
                (true, false) => self.pin(skeleton, bone),
                _ => {}
            }
            if open.is_none() {
                self.menu = None;
            }
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
            } else if self.pins().contains_key(&bone) {
                format!("{} {name} (pinned)", icons::PUSH_PIN)
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
        // A pin where each pinned joint is held.
        let to_world = skeleton.to_world();
        for &at in self.pins().values() {
            if let Some(at) = screen.point(to_world.transform_point3(at)) {
                painter.text(
                    at + egui::vec2(0.0, -JOINT - 1.0),
                    egui::Align2::CENTER_BOTTOM,
                    icons::PUSH_PIN,
                    egui::FontId::proportional(14.0),
                    palette.warning,
                );
            }
        }
        gizmo::paint(painter, &interaction.shapes, interaction.highlighted);
    }
}

/// How a gizmo drag from `start` that gave the handle these `values` poses
/// the bone: a turned Orientation turns it, and a moved Position takes its
/// joint there.
fn posing_by(
    skeleton: &Skeleton,
    bone: usize,
    start: gizmo::Pose,
    values: &[(&'static str, axcenity_core::Value)],
) -> Option<Posing> {
    let vector = |name| {
        values
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, value)| value.as_vector3())
    };
    if let Some(orientation) = vector("Orientation") {
        let rotation = axcenity_scene::rotation_from_orientation(orientation);
        let turned = skeleton.turned(bone, rotation * start.rotation.inverse())?;
        return Some(Posing::Set(bone, turned));
    }
    let position = vector("Position")?;
    Some(Posing::Reach(
        bone,
        Vec3::new(position.x, position.y, position.z),
    ))
}

/// Where `ray` meets the plane through `point` facing along `normal`, if
/// it does in front of the camera.
fn on_plane(ray: axcenity_scene::Ray, point: Vec3, normal: Vec3) -> Option<Vec3> {
    let facing = ray.direction.dot(normal);
    if facing.abs() < 1e-6 {
        return None;
    }
    let along = (point - ray.origin).dot(normal) / facing;
    (along > 0.0).then(|| ray.origin + ray.direction * along)
}

/// `by` in whole Move snap steps, when it's on.
fn snapped(by: Vec3, snap: &Snap) -> Vec3 {
    if snap.move_on && snap.move_step > 0.0 {
        (by / snap.move_step).round() * snap.move_step
    } else {
        by
    }
}

#[cfg(test)]
mod tests;
