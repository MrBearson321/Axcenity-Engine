//! `object_duplicate`: the editor's Duplicate (a copy of each object and
//! everything inside it, right after the original, with a name no sibling
//! has), as one undo step. `count` and `offset` make a row: copy `n` moves
//! by `n × offset`, so `{"count":4,"offset":[0,0,6]}` lays out four more
//! fence posts 6 studs apart.

use axcenity_core::{InstanceId, Value, Vector3, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::object_set::targets;
use super::ops::write;
use super::refs::{label, resolve};
use super::{Host, ID, IDS};
use crate::history::{self, Place};
use crate::tree;

pub const SPEC: ToolSpec = ToolSpec {
    name: "object_duplicate",
    title: "Duplicate objects",
    summary: "Copy objects with what's inside them; count and offset make a row.",
    profile: Profile::Standard,
    class: Class::Add,
    idempotent: false,
    params: &[
        ID,
        IDS,
        Param::optional("count", Kind::Integer, "copies of each, default 1"),
        Param::optional("offset", Kind::Vector3, "copy n moves n times this"),
        Param::optional("parent", Kind::String, "default: the original's"),
    ],
};

/// The most copies of each object one call makes.
const MAX_COUNT: u64 = 100;
/// The most objects (counting what's inside) one call makes.
const MAX_OBJECTS: usize = 2000;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let what = {
        let world = &host.app.world;
        match targets(world, args)?.as_slice() {
            [one] => world
                .get(*one)
                .map_or_else(String::new, |i| i.name().to_owned()),
            many => format!("{} objects", many.len()),
        }
    };
    let written = write(host, &format!("Duplicate {what}"), |world| {
        step(world, args)
    })?;
    Ok(format!(
        "{}{}",
        result(&host.app.world, &written.value),
        written.undo_note()
    ))
}

/// `made 41 Part "Post2", 42 Part "Post3"`, at most five named.
pub(super) fn result(world: &World, copies: &[InstanceId]) -> String {
    let named: Vec<String> = copies.iter().take(5).map(|&id| label(world, id)).collect();
    let mut text = format!("made {}", named.join(", "));
    if copies.len() > 5 {
        text.push_str(&format!(" and {} more", copies.len() - 5));
    }
    text
}

/// Makes the copies `args` asks for in `world`; returns them, each
/// original's in order.
pub(super) fn step(world: &mut World, args: &Args) -> Result<Vec<InstanceId>, ToolError> {
    let ids = targets(world, args)?;
    let count = args.u64("count").unwrap_or(1);
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(ToolError::new(
            Code::BadValue,
            format!("count {count}; give 1 to {MAX_COUNT}"),
        ));
    }
    let offset = args
        .vector3("offset")
        .map(|[x, y, z]| [x as f32, y as f32, z as f32]);
    let parent = args
        .str("parent")
        .map(|text| resolve(world, text))
        .transpose()?;
    let mut size = 0;
    for &id in &ids {
        check(world, id)?;
        if let Some(parent) = parent {
            tree::check_move(world, id, parent).map_err(|reason| {
                ToolError::new(
                    Code::NotAllowed,
                    format!("can't put a copy of {} there: {reason}", label(world, id)),
                )
            })?;
        }
        size += 1 + world.descendants_iter(id).count();
    }
    let total = size * count as usize;
    if total > MAX_OBJECTS {
        return Err(ToolError::new(
            Code::TooLarge,
            format!("that makes {total} objects; at most {MAX_OBJECTS} at a time"),
        ));
    }
    let mut copies = Vec::new();
    for &id in &ids {
        let original = history::place_of(world, id)
            .ok_or_else(|| ToolError::internal("the object has no place in the tree"))?;
        let name = world
            .get(id)
            .map_or_else(String::new, |i| i.name().to_owned());
        for n in 1..=count {
            let copy = world
                .duplicate(id)
                .map_err(|error| ToolError::new(Code::NotAllowed, error.to_string()))?;
            // Each copy right after the one before, so they read in order.
            let place = match parent {
                Some(parent) => Place {
                    parent,
                    index: world.get(parent).map_or(0, |p| p.children().len()),
                },
                None => Place {
                    parent: original.parent,
                    index: original.index + n as usize,
                },
            };
            let copy_name = tree::unique_name(world, place.parent, &name);
            world
                .set_name(copy, copy_name)
                .map_err(|error| ToolError::internal(error.to_string()))?;
            history::put(world, copy, Some(place))
                .map_err(|error| ToolError::new(Code::NotAllowed, error.to_string()))?;
            // In the tree first: whether a light stands free depends on
            // what it's in.
            if let Some(offset) = offset {
                let n = n as f32;
                move_by(
                    world,
                    copy,
                    Vector3::new(offset[0] * n, offset[1] * n, offset[2] * n),
                );
            }
            copies.push(copy);
        }
    }
    Ok(copies)
}

/// Whether `id` can be copied as the editor's Duplicate does it here.
fn check(world: &World, id: InstanceId) -> Result<(), ToolError> {
    tree::check_editable(world, id).map_err(|reason| {
        ToolError::new(
            Code::NotAllowed,
            format!("can't duplicate {}: {reason}", label(world, id)),
        )
    })?;
    if world.is_locked(id) {
        return Err(
            ToolError::new(Code::Locked, format!("{} is locked", label(world, id)))
                .with_fix("object_set props {\"Locked\":false} on it first"),
        );
    }
    // A UI element's copy needs a fresh Id and copies of its style rules,
    // which only the editor's Duplicate makes.
    let inside = std::iter::once(id).chain(world.descendants_iter(id));
    if let Some(element) = inside
        .into_iter()
        .find(|&id| crate::ui_designer::arrange::element_id(world, id).is_some())
    {
        return Err(ToolError::new(
            Code::Unsupported,
            format!("{} is a UI element with an Id", label(world, element)),
        )
        .with_fix("duplicate UI in the editor (Ctrl+D)"));
    }
    Ok(())
}

/// Moves everything in `id` that stands in the world (parts, and lights
/// or emitters not on a part) by `offset`. What sits on a part, such as an
/// Attachment, is placed relative to it and follows.
fn move_by(world: &mut World, id: InstanceId, offset: Vector3) {
    let inside: Vec<InstanceId> = std::iter::once(id)
        .chain(world.descendants_iter(id))
        .collect();
    for id in inside {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let Some(Value::Vector3(position)) = instance.property("Position").cloned() else {
            continue;
        };
        let is_part = instance.property("Size").is_some();
        let free = matches!(
            axcenity_scene::mount_of(world, instance),
            Some(axcenity_scene::Mount::Free { .. })
        );
        if is_part || free {
            let moved = Vector3::new(
                position.x + offset.x,
                position.y + offset.y,
                position.z + offset.z,
            );
            let _ = world.set_property(id, "Position", Value::Vector3(moved));
        }
    }
}
