//! Attachments and welds in the editor (Phase 35).
//!
//! An Attachment's `Position` and `Orientation` are an offset on the part
//! it's parented to, so it follows the part. The transform tools work in
//! the world: [`tool_pose`] gives them each selected Attachment's world
//! pose, and [`localize`] turns what a drag sets back
//! into the offset. The Weld button and Import's "Weld parts" weld parts
//! to the largest of them ([`weld_pairs`], [`new_weld`]). The viewport
//! draws Attachments as small gizmos and welds as lines ([`paint`]).

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::{orientation_from_rotation, rotation_from_orientation};
use eframe::egui;

use crate::gizmo::{PartValues, Pose};
use crate::screen::Screen;

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
