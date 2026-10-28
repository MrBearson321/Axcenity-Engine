//! Attachments and welds in the editor (Phase 35).
//!
//! An Attachment's `Position` and `Orientation` are an offset on the part
//! it's parented to, so it follows the part. The transform tools work in
//! the world: [`tool_pose`] gives them each selected Attachment's world
//! pose, and [`localize`] turns what a drag sets back
//! into the offset. The Weld button and Import's "Weld parts" weld parts
//! to the largest of them ([`weld_pairs`], [`new_weld`]). The viewport
//! draws Attachments as small gizmos and welds as lines ([`paint`]).
//!
//! Phase 36: the Constraint tool joins two clicked surfaces with a joint
//! ([`new_joint`]), and the overlay draws each joint's axis, limits, coil
//! or line.
//!
//! Phase 37: it also puts a mover on a clicked surface ([`new_mover`]; an
//! align takes a second surface to follow), and the overlay draws each
//! mover's push, turn or target.

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::{orientation_from_rotation, rotation_from_orientation};
use eframe::egui;

use crate::gizmo::{PartValues, Pose};
use crate::screen::Screen;
use crate::viewport::SurfacePick;

/// How big an Attachment counts as for the transform tools' handles, in studs.
const ATTACHMENT_SIZE: f32 = 0.5;
/// How far an Attachment's gizmo axes reach, in points on screen.
const AXIS_POINTS: f32 = 18.0;
/// The radius of an Attachment's gizmo ring, in points.
const RING_POINTS: f32 = 5.0;

const ATTACHMENT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 196, 64);
const SELECTED_ATTACHMENT_COLOR: egui::Color32 = egui::Color32::from_rgb(90, 200, 255);
const X_AXIS_COLOR: egui::Color32 = egui::Color32::from_rgb(235, 80, 70);
const Y_AXIS_COLOR: egui::Color32 = egui::Color32::from_rgb(110, 210, 90);
const WELD_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 220, 255);
const DISABLED_WELD_COLOR: egui::Color32 = egui::Color32::from_rgb(150, 150, 150);
const JOINT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 150, 60);
const LIMIT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 220, 90);
const MOVER_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 255, 170);
/// How far a joint's axis and limit arcs reach, in points on screen.
const JOINT_POINTS: f32 = 26.0;

/// The joints the Constraint tool makes, in the order the ribbon lists them.
pub const JOINTS: [ClassName; 7] = [
    ClassName::HingeConstraint,
    ClassName::BallSocketConstraint,
    ClassName::PrismaticConstraint,
    ClassName::CylindricalConstraint,
    ClassName::SpringConstraint,
    ClassName::RopeConstraint,
    ClassName::RodConstraint,
];

/// The movers the Constraint tool makes, in the order the ribbon lists them.
pub const MOVERS: [ClassName; 6] = [
    ClassName::LinearVelocity,
    ClassName::AngularVelocity,
    ClassName::VectorForce,
    ClassName::Torque,
    ClassName::AlignPosition,
    ClassName::AlignOrientation,
];

/// True if the Constraint tool makes `class` from two surfaces: a joint,
/// or an align following Attachment1. Other movers take one.
pub fn takes_two_surfaces(class: ClassName) -> bool {
    !matches!(
        class,
        ClassName::LinearVelocity
            | ClassName::AngularVelocity
            | ClassName::VectorForce
            | ClassName::Torque
    )
}

fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn to_value(v: Vec3) -> Value {
    Value::Vector3(Vector3::new(v.x, v.y, v.z))
}

fn is_part(instance: &Instance) -> bool {
    matches!(instance.class(), ClassName::Part | ClassName::MeshPart)
}

/// The part an Attachment is on, if its parent is one.
fn part_of(world: &World, attachment: &Instance) -> Option<InstanceId> {
    let parent = attachment.parent()?;
    world.get(parent).filter(|part| is_part(part))?;
    Some(parent)
}

/// Where the part an Attachment is on is, and how it's turned: nowhere
/// off when it isn't on a part.
fn part_frame(world: &World, attachment: &Instance) -> (Vec3, Quat) {
    part_of(world, attachment)
        .and_then(|part| world.get(part))
        .and_then(Pose::of)
        .map_or((Vec3::ZERO, Quat::IDENTITY), |pose| {
            (pose.position, pose.rotation)
        })
}

/// Where the Attachment `id` is in the world, and how it's turned, or
/// `None` if `id` isn't an Attachment.
pub fn world_pose(world: &World, id: InstanceId) -> Option<(Vec3, Quat)> {
    let attachment = world.get(id)?;
    if attachment.class() != ClassName::Attachment {
        return None;
    }
    let vector = |name| attachment.property(name).and_then(Value::as_vector3);
    let offset = to_vec3(vector("Position")?);
    let turn = rotation_from_orientation(vector("Orientation")?);
    let (at, rotation) = part_frame(world, attachment);
    Some((at + rotation * offset, rotation * turn))
}

/// The pose the transform tools give the Attachment `id`: where it is in
/// the world, a little box around it. `None` if `id` isn't an Attachment.
pub fn tool_pose(world: &World, id: InstanceId) -> Option<Pose> {
    let (position, rotation) = world_pose(world, id)?;
    Some(Pose {
        position,
        rotation,
        size: Vec3::splat(ATTACHMENT_SIZE),
    })
}

/// True if `id` is an Attachment on one of `parts`: it follows its part,
/// so the tools don't move it as well.
pub fn on_one_of(world: &World, id: InstanceId, parts: &[InstanceId]) -> bool {
    world
        .get(id)
        .filter(|instance| instance.class() == ClassName::Attachment)
        .and_then(|attachment| part_of(world, attachment))
        .is_some_and(|part| parts.contains(&part))
}

/// Turns the world `Position` and `Orientation` a drag sets on each
/// Attachment in `parts` into its offset on its part. Attachments have no
/// Size, so a Resize drag leaves them alone.
pub fn localize(world: &World, parts: &mut PartValues) {
    parts.retain_mut(|(id, values)| {
        let Some((at, turn)) = world_pose(world, *id) else {
            return true;
        };
        if values.iter().any(|(name, _)| *name == "Size") {
            return false;
        }
        let read = |name| {
            values
                .iter()
                .find(|(property, _)| *property == name)
                .and_then(|(_, value)| value.as_vector3())
        };
        let new_at = read("Position").map_or(at, to_vec3);
        let new_turn = read("Orientation").map_or(turn, rotation_from_orientation);
        let attachment = world.get(*id).expect("has a world pose");
        let (part_at, part_turn) = part_frame(world, attachment);
        let inverse = part_turn.inverse();
        let mut local = Vec::new();
        if read("Position").is_some() || read("Orientation").is_some() {
            local.push(("Position", to_value(inverse * (new_at - part_at))));
        }
        if read("Orientation").is_some() {
            let orientation = orientation_from_rotation(inverse * new_turn);
            local.push(("Orientation", Value::Vector3(orientation)));
        }
        *values = local;
        true
    });
}

/// How the Weld button welds `parts`: every other part to the largest
/// (by volume; the first of equals), as `(Part0, Part1)` pairs. Empty
/// with fewer than two parts.
pub fn weld_pairs(world: &World, parts: &[InstanceId]) -> Vec<(InstanceId, InstanceId)> {
    let volume = |id: InstanceId| {
        world
            .get(id)
            .and_then(|part| part.property("Size"))
            .and_then(Value::as_vector3)
            .map_or(0.0, |size| (size.x * size.y * size.z).abs())
    };
    let parts: Vec<InstanceId> = parts
        .iter()
        .copied()
        .filter(|&id| world.get(id).is_some_and(is_part))
        .collect();
    let Some(&largest) = parts.iter().reduce(|best, id| {
        if volume(*id) > volume(*best) {
            id
        } else {
            best
        }
    }) else {
        return Vec::new();
    };
    parts
        .into_iter()
        .filter(|&id| id != largest)
        .map(|id| (largest, id))
        .collect()
}

/// A new WeldConstraint holding `part0` and `part1`, not in the tree yet.
pub fn new_weld(world: &mut World, part0: InstanceId, part1: InstanceId) -> InstanceId {
    let weld = world
        .create(ClassName::WeldConstraint)
        .expect("a WeldConstraint isn't a service");
    for (name, part) in [("Part0", part0), ("Part1", part1)] {
        world
            .set_property(weld, name, Value::Instance(Some(part)))
            .expect("a WeldConstraint has Part0 and Part1");
    }
    weld
}

/// Welds the parts inside `model` to its largest part, each weld inside
/// the part it holds (Part1). Returns the welds made.
pub fn weld_model(world: &mut World, model: InstanceId) -> Vec<InstanceId> {
    let parts = world.descendants(model);
    weld_pairs(world, &parts)
        .into_iter()
        .map(|(part0, part1)| {
            let weld = new_weld(world, part0, part1);
            let name = crate::tree::unique_name(world, part1, ClassName::WeldConstraint.as_str());
            world.set_name(weld, name).expect("just made");
            world
                .set_parent(weld, Some(part1))
                .expect("a weld can go inside a part");
            weld
        })
        .collect()
}

/// A new joint of `class` from the surface `first` to `second` (on another
/// part), not in the tree yet: an Attachment at each point, both turned so
/// their X axis (the joint's axis) points out of `first`'s surface, and
/// the constraint between them. A spring's FreeLength and a rope's or
/// rod's Length are how far apart the points are. Returns the two
/// Attachments and the constraint.
pub fn new_joint(
    world: &mut World,
    class: ClassName,
    first: SurfacePick,
    second: SurfacePick,
) -> [InstanceId; 3] {
    let turn = surface_turn(first);
    let a0 = surface_attachment(world, first, turn);
    let a1 = surface_attachment(world, second, turn);
    let joint = world.create(class).expect("a joint isn't a service");
    let apart = f64::from(first.point.distance(second.point));
    let lengths: &[&str] = match class {
        ClassName::SpringConstraint => &["FreeLength"],
        ClassName::RopeConstraint => &["Length", "WinchTarget"],
        ClassName::RodConstraint => &["Length"],
        _ => &[],
    };
    let ends = [("Attachment0", a0), ("Attachment1", a1)]
        .map(|(name, id)| (name, Value::Instance(Some(id))));
    let lengths = lengths.iter().map(|&name| (name, Value::Number(apart)));
    for (name, value) in ends.into_iter().chain(lengths) {
        world
            .set_property(joint, name, value)
            .expect("joints have their attachments and lengths");
    }
    [a0, a1, joint]
}

/// Turned so its X axis points out of the surface `pick`.
fn surface_turn(pick: SurfacePick) -> Quat {
    Quat::from_rotation_arc(Vec3::X, pick.normal.normalize_or(Vec3::Y))
}

/// A new Attachment at the point `pick`, turned `turn` in the world, as an
/// offset on its part. Not in the tree yet.
fn surface_attachment(world: &mut World, pick: SurfacePick, turn: Quat) -> InstanceId {
    let (at, part_turn) = world
        .get(pick.part)
        .and_then(Pose::of)
        .map_or((Vec3::ZERO, Quat::IDENTITY), |pose| {
            (pose.position, pose.rotation)
        });
    let id = world
        .create(ClassName::Attachment)
        .expect("an Attachment isn't a service");
    let inverse = part_turn.inverse();
    let local = [
        ("Position", to_value(inverse * (pick.point - at))),
        (
            "Orientation",
            Value::Vector3(orientation_from_rotation(inverse * turn)),
        ),
    ];
    for (name, value) in local {
        world
            .set_property(id, name, value)
            .expect("an Attachment has Position and Orientation");
    }
    id
}

/// A new mover of `class` on the surface `first`, not in the tree yet: an
/// Attachment there with its X axis out of the surface, and the mover on
/// it. An align also gets an Attachment at `second` (turned the same way)
/// to follow. Returns the Attachments and then the mover.
pub fn new_mover(
    world: &mut World,
    class: ClassName,
    first: SurfacePick,
    second: Option<SurfacePick>,
) -> Vec<InstanceId> {
    let turn = surface_turn(first);
    let mut ids = vec![surface_attachment(world, first, turn)];
    if let Some(second) = second {
        ids.push(surface_attachment(world, second, turn));
    }
    let mover = world.create(class).expect("a mover isn't a service");
    for (name, id) in ["Attachment0", "Attachment1"].into_iter().zip(&ids) {
        world
            .set_property(mover, name, Value::Instance(Some(*id)))
            .expect("movers have their attachments");
    }
    ids.push(mover);
    ids
}

/// Draws the selected Attachments, and with `overlay` on (View >
/// Overlays > Constraints) every Attachment and a line for each weld
/// between the middles of its parts, dimmed when it's disabled.
pub fn paint(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    selection: &[InstanceId],
    overlay: bool,
) {
    let ids: Vec<InstanceId> = if overlay {
        world.descendants(world.root())
    } else {
        selection.to_vec()
    };
    for &id in &ids {
        let Some(instance) = world.get(id) else {
            continue;
        };
        match instance.class() {
            ClassName::Attachment => {
                paint_attachment(painter, screen, world, id, selection.contains(&id));
            }
            ClassName::WeldConstraint if overlay => paint_weld(painter, screen, world, instance),
            class if class.is_joint() && (overlay || selection.contains(&id)) => {
                paint_joint(painter, screen, world, instance);
            }
            class if class.is_mover() && (overlay || selection.contains(&id)) => {
                paint_mover(painter, screen, world, instance);
            }
            _ => {}
        }
    }
}

fn paint_attachment(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    id: InstanceId,
    selected: bool,
) {
    let Some((at, turn)) = world_pose(world, id) else {
        return;
    };
    let Some(center) = screen.point(at) else {
        return;
    };
    let reach = screen.studs_per_point(at) * AXIS_POINTS;
    for (axis, color) in [(Vec3::X, X_AXIS_COLOR), (Vec3::Y, Y_AXIS_COLOR)] {
        if let Some(line) = screen.line(at, at + turn * axis * reach) {
            painter.line_segment(line, egui::Stroke::new(2.0_f32, color));
        }
    }
    let color = if selected {
        SELECTED_ATTACHMENT_COLOR
    } else {
        ATTACHMENT_COLOR
    };
    painter.circle_filled(center, RING_POINTS, egui::Color32::from_black_alpha(140));
    painter.circle_stroke(center, RING_POINTS, egui::Stroke::new(2.0_f32, color));
}

/// A joint: a line between its attachments (a zigzag for a spring), its
/// axis through Attachment0 for a hinge, prismatic or cylindrical
/// constraint, and an arc between a hinge's limits. Grey when disabled.
fn paint_joint(painter: &egui::Painter, screen: &Screen, world: &World, joint: &Instance) {
    let end = |name| match joint.property(name) {
        Some(Value::Instance(Some(id))) => world_pose(world, *id),
        _ => None,
    };
    let (Some((a, turn)), Some((b, _))) = (end("Attachment0"), end("Attachment1")) else {
        return;
    };
    let number = |name| {
        joint
            .property(name)
            .and_then(Value::as_number)
            .map_or(0.0, |n| n as f32)
    };
    let flag = |name| joint.property(name) == Some(&Value::Bool(true));
    let dim = |color: egui::Color32| {
        if flag("Enabled") {
            color
        } else {
            DISABLED_WELD_COLOR
        }
    };
    let stroke = egui::Stroke::new(2.0_f32, dim(JOINT_COLOR));
    let reach = screen.studs_per_point(a) * JOINT_POINTS;
    let segment = |from: Vec3, to: Vec3, stroke: egui::Stroke| {
        if let Some(line) = screen.line(from, to) {
            painter.line_segment(line, stroke);
        }
    };
    if joint.class() == ClassName::SpringConstraint {
        // A zigzag across the line, so it reads as a spring.
        let along = b - a;
        let side = along.normalize_or_zero().any_orthonormal_vector() * reach * 0.25;
        let turns = number("Coils").clamp(1.0, 50.0).round() as usize * 2;
        let point = |i: usize| {
            let wobble = if i == 0 || i == turns {
                0.0
            } else if i.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            a + along * (i as f32 / turns as f32) + side * wobble
        };
        for i in 0..turns {
            segment(point(i), point(i + 1), stroke);
        }
    } else {
        segment(a, b, stroke);
    }
    let axis = turn * Vec3::X;
    if matches!(
        joint.class(),
        ClassName::HingeConstraint
            | ClassName::PrismaticConstraint
            | ClassName::CylindricalConstraint
    ) {
        segment(a - axis * reach, a + axis * reach, stroke);
    }
    // A hinge's (or cylindrical constraint's) limits: an arc from
    // LowerAngle to UpperAngle around the axis, from Attachment0's Y.
    let limited = match joint.class() {
        ClassName::HingeConstraint => flag("LimitsEnabled"),
        ClassName::CylindricalConstraint => flag("AngularLimitsEnabled"),
        _ => false,
    };
    if limited {
        let (a_angle, b_angle) = (number("LowerAngle"), number("UpperAngle"));
        let lower = a_angle.min(b_angle);
        let upper = a_angle.max(b_angle).min(lower + 360.0);
        let spoke = |degrees: f32| {
            let around = Quat::from_axis_angle(axis, degrees.to_radians());
            a + around * (turn * Vec3::Y) * reach
        };
        let limit = egui::Stroke::new(1.5_f32, dim(LIMIT_COLOR));
        let steps = ((upper - lower) / 10.0).ceil().max(1.0) as usize;
        let step = |i: usize| lower + (upper - lower) * i as f32 / steps as f32;
        for i in 0..steps {
            segment(spoke(step(i)), spoke(step(i + 1)), limit);
        }
        segment(a, spoke(lower), limit);
        segment(a, spoke(upper), limit);
    }
    for at in [a, b] {
        if let Some(at) = screen.point(at) {
            painter.circle_filled(at, 3.0, dim(JOINT_COLOR));
        }
    }
}

/// A mover: an arrow along a LinearVelocity's or VectorForce's vector, a
/// circling arrow around an AngularVelocity's or Torque's axis, and a line
/// from an align's Attachment0 to its target, with a cross there for an
/// AlignPosition and the target's X and Y axes for an AlignOrientation.
/// Grey when disabled.
fn paint_mover(painter: &egui::Painter, screen: &Screen, world: &World, mover: &Instance) {
    let end = |name| match mover.property(name) {
        Some(Value::Instance(Some(id))) => world_pose(world, *id),
        _ => None,
    };
    let Some((a, turn0)) = end("Attachment0") else {
        return;
    };
    let end1 = end("Attachment1");
    let color = if mover.property("Enabled") == Some(&Value::Bool(false)) {
        DISABLED_WELD_COLOR
    } else {
        MOVER_COLOR
    };
    let stroke = egui::Stroke::new(2.0_f32, color);
    let reach = screen.studs_per_point(a) * JOINT_POINTS * 1.5;
    let segment = |from: Vec3, to: Vec3| {
        if let Some(line) = screen.line(from, to) {
            painter.line_segment(line, stroke);
        }
    };
    let arrow = |from: Vec3, to: Vec3| {
        segment(from, to);
        let along = to - from;
        let side = along.any_orthonormal_vector() * along.length() * 0.15;
        for side in [side, -side] {
            segment(to, to - along * 0.25 + side);
        }
    };
    let vector = |name| {
        mover
            .property(name)
            .and_then(Value::as_vector3)
            .map_or(Vec3::ZERO, to_vec3)
    };
    let item = |name| match mover.property(name) {
        Some(Value::EnumItem(item)) => item.item.as_str(),
        _ => "",
    };
    // A vector in the mover's RelativeTo axes, in the world's.
    let turned = |v: Vec3| match item("RelativeTo") {
        "Attachment0" => Some(turn0 * v),
        "Attachment1" => end1.map(|(_, turn1)| turn1 * v),
        _ => Some(v),
    };
    let one_attachment = item("Mode") == "OneAttachment";
    match mover.class() {
        ClassName::LinearVelocity | ClassName::VectorForce => {
            let name = match mover.class() {
                ClassName::VectorForce => "Force",
                _ => "VectorVelocity",
            };
            if let Some(way) = turned(vector(name)).and_then(Vec3::try_normalize) {
                arrow(a, a + way * reach);
            }
        }
        ClassName::AngularVelocity | ClassName::Torque => {
            let name = match mover.class() {
                ClassName::Torque => "Torque",
                _ => "AngularVelocity",
            };
            if let Some(axis) = turned(vector(name)).and_then(Vec3::try_normalize) {
                // The axis, and three quarters of a circle around it the way
                // it turns (right-handed), ending in an arrowhead.
                segment(a, a + axis * reach);
                let spoke = axis.any_orthonormal_vector() * reach * 0.5;
                let point = |i: usize| {
                    let angle = i as f32 / 12.0 * std::f32::consts::TAU * 0.75;
                    a + axis * reach * 0.5 + Quat::from_axis_angle(axis, angle) * spoke
                };
                for i in 0..11 {
                    segment(point(i), point(i + 1));
                }
                arrow(point(11), point(12));
            }
        }
        ClassName::AlignPosition => {
            let goal = if one_attachment {
                Some(vector("Position"))
            } else {
                end1.map(|(at, _)| at)
            };
            if let Some(goal) = goal {
                segment(a, goal);
                let cross = screen.studs_per_point(goal) * 6.0;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    segment(goal - axis * cross, goal + axis * cross);
                }
            }
        }
        ClassName::AlignOrientation => {
            let goal = if one_attachment {
                mover
                    .property("CFrame")
                    .and_then(Value::as_cframe)
                    .map(|cframe| Quat::from_array(cframe.rotation).normalize())
            } else {
                end1.map(|(at, turn)| {
                    segment(a, at);
                    turn
                })
            };
            // Which way its X and Y axes are being turned to, from
            // Attachment0.
            if let Some(goal) = goal.filter(|goal| goal.is_finite()) {
                for axis in [Vec3::X, Vec3::Y] {
                    arrow(a, a + goal * axis * reach);
                }
            }
        }
        _ => {}
    }
    if let Some(at) = screen.point(a) {
        painter.circle_filled(at, 3.0, color);
    }
}

fn paint_weld(painter: &egui::Painter, screen: &Screen, world: &World, weld: &Instance) {
    let middle = |name| match weld.property(name) {
        Some(Value::Instance(Some(part))) => world
            .get(*part)
            .filter(|part| is_part(part))
            .and_then(Pose::of)
            .map(|pose| pose.position),
        _ => None,
    };
    let (Some(a), Some(b)) = (middle("Part0"), middle("Part1")) else {
        return;
    };
    let color = if weld.property("Enabled") == Some(&Value::Bool(false)) {
        DISABLED_WELD_COLOR
    } else {
        WELD_COLOR
    };
    if let Some(line) = screen.line(a, b) {
        painter.line_segment(line, egui::Stroke::new(2.0_f32, color));
    }
    for end in [a, b] {
        if let Some(at) = screen.point(end) {
            painter.circle_filled(at, 3.0, color);
        }
    }
}
