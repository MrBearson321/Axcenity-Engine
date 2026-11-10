//! Where a Tool is (Phase 45), for the crates that treat carried Tools
//! differently: a Tool in an Inventory is carried, either in hand (the
//! Inventory's `Equipped`) or put away. A put-away Tool's parts aren't in
//! the world: they're not drawn, don't collide and aren't hit by queries.
//! A Tool in hand follows the hand, so physics holds its parts still and
//! they don't collide. Every other Tool is a Model like any other.

use crate::{ClassName, InstanceId, Value, World};

/// How a carried Tool is carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carried {
    /// In hand: its Inventory's `Equipped`.
    Held,
    /// Put away in its Inventory.
    Stored,
}

fn class(world: &World, id: InstanceId) -> Option<ClassName> {
    world.get(id).map(|instance| instance.class())
}

/// The Tool `id` is, or is inside (the nearest), if any.
pub fn tool_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let mut at = Some(id);
    while let Some(current) = at {
        let instance = world.get(current)?;
        if instance.class() == ClassName::Tool {
            return Some(current);
        }
        at = instance.parent();
    }
    None
}

/// The Inventory a Tool is carried in: its parent, when that's one.
pub fn inventory_holding(world: &World, tool: InstanceId) -> Option<InstanceId> {
    let parent = world.get(tool)?.parent()?;
    (class(world, parent) == Some(ClassName::Inventory)).then_some(parent)
}

/// The Tool an Inventory has in hand, if it still holds it.
pub fn equipped(world: &World, inventory: InstanceId) -> Option<InstanceId> {
    match world.get(inventory)?.property("Equipped") {
        Some(Value::Instance(Some(tool))) if inventory_holding(world, *tool) == Some(inventory) => {
            Some(*tool)
        }
        _ => None,
    }
}

/// How `id` (a Tool, or anything in one) is carried, or `None` when its
/// Tool isn't in an Inventory, or it isn't in a Tool.
pub fn carried(world: &World, id: InstanceId) -> Option<Carried> {
    let (tool, inventory) = carrier(world, id)?;
    Some(if equipped(world, inventory) == Some(tool) {
        Carried::Held
    } else {
        Carried::Stored
    })
}

/// The Tool `id` is carried with (a Tool inside a Tool is carried with the
/// outer one) and the Inventory holding it, or `None` when it isn't carried.
fn carrier(world: &World, id: InstanceId) -> Option<(InstanceId, InstanceId)> {
    let mut at = tool_of(world, id);
    while let Some(tool) = at {
        if let Some(inventory) = inventory_holding(world, tool) {
            return Some((tool, inventory));
        }
        at = world
            .get(tool)
            .and_then(|instance| instance.parent())
            .and_then(|parent| tool_of(world, parent));
    }
    None
}

/// The Inventory whose `Equipped` decides whether `id` is in the world, if
/// it's carried. Its Tool can go in and out of hand with no change of
/// parent, so a cache of what's in the world watches this Inventory too.
pub fn carrying_inventory(world: &World, id: InstanceId) -> Option<InstanceId> {
    carrier(world, id).map(|(_, inventory)| inventory)
}

/// True when `id` is in a put-away Tool, so it's out of the world.
pub fn is_stored(world: &World, id: InstanceId) -> bool {
    carried(world, id) == Some(Carried::Stored)
}

/// A Tool's Handle: its `Handle`, if that's a part in it, else its first
/// child part named "Handle".
pub fn handle(world: &World, tool: InstanceId) -> Option<InstanceId> {
    let instance = world.get(tool)?;
    if let Some(Value::Instance(Some(part))) = instance.property("Handle")
        && world
            .get(*part)
            .is_some_and(|part| part.class().is_base_part())
        && world.is_descendant_of(*part, tool)
    {
        return Some(*part);
    }
    instance.children().iter().copied().find(|&child| {
        world
            .get(child)
            .is_some_and(|part| part.class().is_base_part() && part.name() == "Handle")
    })
}

/// A Tool's Grip: its `Grip`, if that's an Attachment in its Handle, else
/// the Handle's Attachment named "Grip". `None` holds it by the Handle's
/// middle.
pub fn grip(world: &World, tool: InstanceId) -> Option<InstanceId> {
    let handle = handle(world, tool)?;
    let in_handle = |id: InstanceId| {
        world.get(id).is_some_and(|attachment| {
            attachment.class() == ClassName::Attachment && attachment.parent() == Some(handle)
        })
    };
    if let Some(Value::Instance(Some(attachment))) = world.get(tool)?.property("Grip")
        && in_handle(*attachment)
    {
        return Some(*attachment);
    }
    world
        .get(handle)?
        .children()
        .iter()
        .copied()
        .find(|&child| in_handle(child) && world.get(child).is_some_and(|a| a.name() == "Grip"))
}

/// The first Inventory in `model`, if it has one.
pub fn inventory_in(world: &World, model: InstanceId) -> Option<InstanceId> {
    world
        .get(model)?
        .children()
        .iter()
        .copied()
        .find(|&child| class(world, child) == Some(ClassName::Inventory))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(world: &mut World, parent: InstanceId, class: ClassName, name: &str) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    #[test]
    fn a_tool_in_an_inventory_is_held_when_equipped_and_stored_otherwise() {
        let mut world = World::new();
        let workspace = world.workspace();
        let character = child(&mut world, workspace, ClassName::Model, "Character");
        let inventory = child(&mut world, character, ClassName::Inventory, "Inventory");
        let sword = child(&mut world, workspace, ClassName::Tool, "Sword");
        let handle = child(&mut world, sword, ClassName::Part, "Handle");
        let grip = child(&mut world, handle, ClassName::Attachment, "Grip");
        assert_eq!(carried(&world, handle), None);
        assert_eq!(tool_of(&world, handle), Some(sword));
        assert_eq!(self::handle(&world, sword), Some(handle));
        assert_eq!(self::grip(&world, sword), Some(grip));

        world.set_parent(sword, Some(inventory)).unwrap();
        assert_eq!(carried(&world, handle), Some(Carried::Stored));
        assert!(is_stored(&world, handle));
        world
            .set_property(inventory, "Equipped", Value::Instance(Some(sword)))
            .unwrap();
        assert_eq!(carried(&world, handle), Some(Carried::Held));
        assert_eq!(equipped(&world, inventory), Some(sword));
        assert_eq!(inventory_in(&world, character), Some(inventory));
    }
}
