//! An Attachment's fields that aren't stored (Phase 35). Its `Position`
//! and `Orientation` (and so its `CFrame`) are relative to the part it's
//! parented to; these say where it is in the world, and which way its X
//! (`Axis`) and Y (`SecondaryAxis`) point. Not on a part, its world is its
//! own frame.

use axcenity_core::{ClassName, Instance, InstanceId, World};
use glam::{Mat3, Quat, Vec3};
use mlua::{FromLua, IntoLua, Lua};

use crate::cframe::{LuaCFrame, from_vec3, orientation_from_rotation, rotation_from_orientation};
use crate::instance::{destroyed, world, world_mut};
use crate::types::{LuaVector3, type_name};

/// The fields worked out from an Attachment and its part.
pub const FIELDS: [&str; 7] = [
    "WorldCFrame",
    "WorldPosition",
    "WorldOrientation",
    "Axis",
    "SecondaryAxis",
    "WorldAxis",
    "WorldSecondaryAxis",
];

/// Where the part an Attachment is on is, or nowhere off if it isn't on a
/// part.
fn part_frame(world: &World, attachment: &Instance) -> LuaCFrame {
    attachment
        .parent()
        .and_then(|parent| world.get(parent))
        .filter(|part| matches!(part.class(), ClassName::Part | ClassName::MeshPart))
        .and_then(LuaCFrame::of_instance)
        .unwrap_or(LuaCFrame::IDENTITY)
}

/// The Attachment's offset and its part's frame, or `None` if `id` isn't
/// an Attachment or `key` isn't one of [`FIELDS`].
fn frames(
    world: &World,
    id: InstanceId,
    key: &str,
) -> mlua::Result<Option<(LuaCFrame, LuaCFrame)>> {
    let instance = world.get(id).ok_or_else(destroyed)?;
    if instance.class() != ClassName::Attachment || !FIELDS.contains(&key) {
        return Ok(None);
    }
    let offset = LuaCFrame::of_instance(instance).unwrap_or(LuaCFrame::IDENTITY);
    Ok(Some((offset, part_frame(world, instance))))
}

/// One of [`FIELDS`] of the Attachment `id`, or `None` if `key` isn't one
/// or `id` isn't an Attachment.
pub fn computed(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<Option<mlua::Value>> {
    let Some((offset, part)) = frames(&*world(lua)?, id, key)? else {
        return Ok(None);
    };
    let placed = part * offset;
    let vector = |v: Vec3| LuaVector3(from_vec3(v)).into_lua(lua);
    let value = match key {
        "WorldCFrame" => placed.into_lua(lua)?,
        "WorldPosition" => vector(placed.position)?,
        "WorldOrientation" => {
            LuaVector3(orientation_from_rotation(placed.rotation)).into_lua(lua)?
        }
        "Axis" => vector(offset.rotation * Vec3::X)?,
        "SecondaryAxis" => vector(offset.rotation * Vec3::Y)?,
        "WorldAxis" => vector(placed.rotation * Vec3::X)?,
        _ => vector(placed.rotation * Vec3::Y)?,
    };
    Ok(Some(value))
}

/// Sets one of [`FIELDS`] of the Attachment `id` by moving it on its part.
pub fn set(lua: &Lua, id: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    let Some((offset, part)) = frames(&*world(lua)?, id, key)? else {
        return Err(mlua::Error::runtime(format!(
            "{key} is not an Attachment field"
        )));
    };
    let wrong = |expected: &str| {
        mlua::Error::runtime(format!(
            "Attachment.{key} must be a {expected}, not a {}",
            type_name(lua, &value)
        ))
    };
    let vector = || {
        LuaVector3::from_lua(value.clone(), lua)
            .map(|v| crate::cframe::to_vec3(v.0))
            .map_err(|_| wrong("Vector3"))
    };
    let to_offset = |placed: LuaCFrame| part.inverse() * placed;
    let new = match key {
        "WorldCFrame" => {
            to_offset(LuaCFrame::from_lua(value.clone(), lua).map_err(|_| wrong("CFrame"))?)
        }
        "WorldPosition" => LuaCFrame::new(part.inverse().point(vector()?), offset.rotation),
        "WorldOrientation" => {
            let turn = rotation_from_orientation(from_vec3(vector()?));
            LuaCFrame::new(offset.position, part.rotation.inverse() * turn)
        }
        // Setting one axis keeps the other as close to where it was as it
        // can be while they stay square to each other.
        "Axis" => {
            let rotation = from_axes(vector()?, offset.rotation * Vec3::Y);
            LuaCFrame::new(offset.position, rotation.ok_or_else(|| zero(key))?)
        }
        "SecondaryAxis" => {
            let x = offset.rotation * Vec3::X;
            let rotation = from_axes(vector()?, x).map(|turned| {
                // `from_axes` made the new Y its X; turn it back so the old
                // X stays put.
                let (y, x) = (turned * Vec3::X, turned * Vec3::Y);
                Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y)))
            });
            LuaCFrame::new(offset.position, rotation.ok_or_else(|| zero(key))?)
        }
        _ => {
            return Err(mlua::Error::runtime(format!(
                "Attachment.{key} is read-only; set Axis, SecondaryAxis or WorldCFrame instead"
            )));
        }
    };
    new.write_to(&mut *world_mut(lua)?, id)
        .map_err(mlua::Error::runtime)
}

fn zero(key: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "Attachment.{key} can't point nowhere (a zero vector)"
    ))
}

/// The rotation whose X points along `x`, and whose Y is as close to `y`
/// as it can be at a right angle to it. `None` for a zero `x`.
fn from_axes(x: Vec3, y: Vec3) -> Option<Quat> {
    let x = x.normalize_or_zero();
    if x == Vec3::ZERO {
        return None;
    }
    let mut y = (y - x * y.dot(x)).normalize_or_zero();
    if y == Vec3::ZERO {
        y = x.any_orthonormal_vector();
    }
    Some(Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y))))
}
