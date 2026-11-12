//! Where an Attachment is in the world. Its `Position` and `Orientation`
//! are an offset on the part it's parented to, so it follows the part.
//! The transform tools and snapping read its world pose from here; the
//! editor's `constraints` module does the rest of Attachments and welds.

use axcenity_core::{Instance, InstanceId, ClassName, Value, Vector3, World};
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::rotation_from_orientation;

use crate::gizmo::Pose;

fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn is_part(instance: &Instance) -> bool {
    instance.class().is_base_part()
}

/// The part an Attachment is on, if its parent is one.
pub fn part_of(world: &World, attachment: &Instance) -> Option<InstanceId> {
    let parent = attachment.parent()?;
    world.get(parent).filter(|part| is_part(part))?;
    Some(parent)
}

/// Where the part an Attachment is on is, and how it's turned: nowhere
/// off when it isn't on a part.
pub fn part_frame(world: &World, attachment: &Instance) -> (Vec3, Quat) {
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

/// True if `id` is an Attachment on one of `parts`: it follows its part,
/// so the tools don't move it as well.
pub fn on_one_of(world: &World, id: InstanceId, parts: &[InstanceId]) -> bool {
    world
        .get(id)
        .filter(|instance| instance.class() == ClassName::Attachment)
        .and_then(|attachment| part_of(world, attachment))
        .is_some_and(|part| parts.contains(&part))
}
