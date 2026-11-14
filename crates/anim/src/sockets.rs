//! Sockets (default humanoid HD4): places on a skinned character's bones
//! that accessories hang from.
//!
//! - A **socket** is an Attachment inside a skinned MeshPart with a `Bone`
//!   attribute naming one of its model's bones. Each frame its `Position`
//!   and `Orientation` (relative to the MeshPart, as an Attachment's are)
//!   are set to where that bone is in the pose: at the bone's origin,
//!   turned as the bone has turned from its rest. At rest a socket faces
//!   the way the character does (-Z forward, with the MeshPart turned
//!   180°, as the default humanoid is, since a glTF character faces +Z).
//! - An **accessory** is a Part or MeshPart inside the same MeshPart with
//!   an Attachment named like a socket: each frame it's placed so its
//!   Attachment sits on the socket, so it follows the bone. Model a hat
//!   facing -Z with its Attachment where it meets the head.
//!
//! Both run after the pose is made (with IK and followers), from
//! `Animators::update`.

use std::collections::HashMap;
use std::sync::Arc;

use axcenity_assets::{Pose, Rig};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{Mat4, Quat, Vec3};

use crate::ragdoll::{rotation_of, to_vec3};

/// The attribute naming a socket's bone.
pub const SOCKET_BONE: &str = "Bone";

/// Moves the sockets in every posed skinned MeshPart in `local` (its
/// skeleton, pose and bone shapes) to their bones, then the accessories
/// onto them.
pub(crate) fn place_sockets(
    world: &mut World,
    local: &HashMap<InstanceId, (Arc<Rig>, Pose, Vec<Vec3>)>,
) {
    for (&part, (rig, pose, _)) in local {
        let Some(children) = world.get(part).map(|instance| instance.children().to_vec()) else {
            continue;
        };
        let sockets: Vec<(InstanceId, usize)> = children
            .iter()
            .filter_map(|&child| {
                let instance = world.get(child)?;
                if instance.class() != ClassName::Attachment {
                    return None;
                }
                let name = instance.attribute(SOCKET_BONE).and_then(Value::as_str)?;
                Some((child, rig.bone(name)?))
            })
            .collect();
        if sockets.is_empty() {
            continue;
        }
        let Some(file_to_part) = file_to_part(world, part, rig) else {
            continue;
        };
        let posed = rig.world(pose);
        let rest = rig.world(&rig.rest_pose());
        // At rest a socket faces the way the character does: the MeshPart's
        // axes turned 180° round Y (a glTF character faces +Z).
        let facing = Quat::from_rotation_y(std::f32::consts::PI);
        for &(socket, bone) in &sockets {
            let origin = file_to_part.transform_point3(posed[bone].transform_point3(Vec3::ZERO));
            let turned = rotation(posed[bone]) * rotation(rest[bone]).inverse();
            set_vector(world, socket, "Position", origin);
            set_orientation(world, socket, turned * facing);
        }
        place_accessories(world, part, &children, &sockets);
    }
}

/// From the model file's space into the MeshPart's own (studs, its axes):
/// its `Size` and the rig's fit, without its place in the world.
fn file_to_part(world: &World, part: InstanceId, rig: &Rig) -> Option<Mat4> {
    let size = world
        .get(part)?
        .property("Size")
        .and_then(Value::as_vector3)?;
    let matrix = Mat4::from_scale(to_vec3(size)) * rig.fit;
    matrix.is_finite().then_some(matrix)
}

fn rotation(matrix: Mat4) -> Quat {
    let (_, rotation, _) = matrix.to_scale_rotation_translation();
    rotation.normalize()
}

/// Places each Part or MeshPart among `children` that has an Attachment
/// named like one of `sockets` so that Attachment sits on the socket.
fn place_accessories(
    world: &mut World,
    part: InstanceId,
    children: &[InstanceId],
    sockets: &[(InstanceId, usize)],
) {
    let Some(holder) = rigid_of(world, part) else {
        return;
    };
    for &child in children {
        let Some(instance) = world.get(child) else {
            continue;
        };
        if !instance.class().is_base_part() {
            continue;
        }
        let found = instance.children().iter().find_map(|&grip| {
            let grip_instance = world.get(grip)?;
            if grip_instance.class() != ClassName::Attachment {
                return None;
            }
            let socket = sockets.iter().find(|(socket, _)| {
                world
                    .get(*socket)
                    .is_some_and(|socket| socket.name() == grip_instance.name())
            })?;
            Some((grip, socket.0))
        });
        let Some((grip, socket)) = found else {
            continue;
        };
        let (Some(socket_local), Some(grip_local)) =
            (rigid_of(world, socket), rigid_of(world, grip))
        else {
            continue;
        };
        let wanted = holder * socket_local * grip_local.inverse();
        let (_, turn, at) = wanted.to_scale_rotation_translation();
        set_vector(world, child, "Position", at);
        set_orientation(world, child, turn);
    }
}

/// A Part's or Attachment's `Position` and `Orientation` as one transform
/// (an Attachment's relative to its part).
fn rigid_of(world: &World, id: InstanceId) -> Option<Mat4> {
    let instance = world.get(id)?;
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let matrix = Mat4::from_rotation_translation(
        rotation_of(vector("Orientation")?),
        to_vec3(vector("Position")?),
    );
    matrix.is_finite().then_some(matrix)
}

/// Sets a Vector3 property when it moved, so a socket at rest isn't a
/// change every frame.
fn set_vector(world: &mut World, id: InstanceId, name: &str, value: Vec3) {
    if !value.is_finite() {
        return;
    }
    let now = world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_vector3);
    if now.is_some_and(|now| to_vec3(now).abs_diff_eq(value, 1e-4)) {
        return;
    }
    let _ = world.set_property(
        id,
        name,
        Value::Vector3(Vector3::new(value.x, value.y, value.z)),
    );
}

fn set_orientation(world: &mut World, id: InstanceId, rotation: Quat) {
    let (y, x, z) = rotation.normalize().to_euler(glam::EulerRot::YXZ);
    set_vector(
        world,
        id,
        "Orientation",
        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()),
    );
}
