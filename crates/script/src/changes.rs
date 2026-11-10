//! Change events: `Changed`, `GetPropertyChangedSignal`, `ChildAdded`,
//! `ChildRemoved`, `DescendantAdded`, `DescendantRemoving`,
//! `AncestryChanged`, a widget's `ValueChanged`, `AttributeChanged` and
//! `CollectionService`'s tag signals (see `collection`).
//!
//! The World keeps a log of every change, whoever made it: a script, physics
//! moving a part, or the editor during Play. [`ScriptRuntime::fire_changes`]
//! (crate::ScriptRuntime::fire_changes) takes that log and calls [`fire`] on
//! each side, so handlers run a moment after the change, not in the middle
//! of the code that made it.

use axcenity_core::{Change, InstanceId, RecordedChange};
use mlua::Lua;

use crate::events::{self, Event, Owner};
use crate::instance::{self, LuaInstance};

/// Fires the events for `changes`, oldest first, on this side. Changes to
/// Instances that no longer exist are skipped.
pub fn fire(lua: &Lua, changes: &[RecordedChange]) {
    for (index, recorded) in changes.iter().enumerate() {
        let number = recorded.number;
        match recorded.change {
            Change::Property { instance, name } => {
                if !exists(lua, instance) {
                    continue;
                }
                let owner = Owner::Instance(instance);
                // `Changed` gets the property's name; a property's own signal
                // gets nothing, as in Roblox. A Value object's `Changed`
                // only fires for its `Value`, with the new value.
                match value_object_value(lua, instance) {
                    None => events::fire_change(lua, owner, Event::Named("Changed"), number, name),
                    Some(value) if name == "Value" => {
                        events::fire_change(lua, owner, Event::Named("Changed"), number, value);
                    }
                    Some(_) => {}
                }
                events::fire_change(lua, owner, Event::PropertyChanged(name), number, ());
                // A Slider, Toggle, Dropdown or ProgressBar also says what
                // its new Value is.
                if name == "Value"
                    && let Some(value) = widget_value(lua, instance)
                {
                    events::fire_change(lua, owner, Event::Named("ValueChanged"), number, value);
                }
                if name == "Parent" {
                    fire_ancestry_changed(lua, instance, number);
                }
            }
            Change::Attribute { instance, ref name } => {
                if exists(lua, instance) {
                    crate::collection::fire_attribute_changed(lua, instance, name, number);
                }
            }
            // Fired together after the rest (see `collection`).
            Change::Tag { .. } => {}
            Change::ChildAdded { parent, child } => {
                fire_child_event(lua, parent, "ChildAdded", child, number);
                let later = &changes[index + 1..];
                fire_descendant_added(lua, parent, child, number, later);
            }
            Change::ChildRemoved { parent, child } => {
                fire_child_event(lua, parent, "ChildRemoved", child, number);
                // A destroyed child's were fired by `Destroy`, while it
                // could still be read (see `tree`).
                fire_descendant_event(lua, parent, "DescendantRemoving", child, number);
            }
        }
    }
    crate::collection::fire_tag_changes(lua, changes);
}

fn fire_child_event(
    lua: &Lua,
    parent: InstanceId,
    event: &'static str,
    child: InstanceId,
    number: u64,
) {
    if exists(lua, parent) {
        let owner = Owner::Instance(parent);
        events::fire_change(lua, owner, Event::Named(event), number, LuaInstance(child));
    }
}

/// `DescendantAdded` for `child`, unless a `later` change adds something
/// it's inside now: that one reports `child` too, in the right order. (The
/// log is read after the fact, so `child` may have been put in a model
/// that only joined the Workspace afterwards.)
fn fire_descendant_added(
    lua: &Lua,
    parent: InstanceId,
    child: InstanceId,
    number: u64,
    later: &[RecordedChange],
) {
    let reported_later = instance::world(lua).is_ok_and(|world| {
        later.iter().any(|recorded| {
            matches!(recorded.change, Change::ChildAdded { child: added, .. }
                if world.is_descendant_of(child, added))
        })
    });
    if !reported_later {
        fire_descendant_event(lua, parent, "DescendantAdded", child, number);
    }
}

/// `DescendantAdded` or `DescendantRemoving` on `parent` and every Instance
/// above it, once for `child` and once for each Instance inside it.
fn fire_descendant_event(
    lua: &Lua,
    parent: InstanceId,
    event: &'static str,
    child: InstanceId,
    number: u64,
) {
    let (above, moved) = {
        let Ok(world) = instance::world(lua) else {
            return;
        };
        if !world.contains(child) {
            return;
        }
        let mut above = Vec::new();
        let mut next = Some(parent);
        while let Some(id) = next {
            above.push(id);
            next = world.get(id).and_then(|instance| instance.parent());
        }
        let mut moved = vec![child];
        moved.extend(world.descendants_iter(child));
        (above, moved)
    };
    for ancestor in above {
        for &descendant in &moved {
            let owner = Owner::Instance(ancestor);
            let args = LuaInstance(descendant);
            events::fire_change(lua, owner, Event::Named(event), number, args);
        }
    }
}

/// `AncestryChanged` on `instance` and everything inside it, with
/// `instance` and its new Parent.
fn fire_ancestry_changed(lua: &Lua, instance: InstanceId, number: u64) {
    let (parent, moved) = {
        let Ok(world) = instance::world(lua) else {
            return;
        };
        let parent = world.get(instance).and_then(|i| i.parent());
        let mut moved = vec![instance];
        moved.extend(world.descendants_iter(instance));
        (parent, moved)
    };
    for id in moved {
        let args = (LuaInstance(instance), parent.map(LuaInstance));
        events::fire_change(
            lua,
            Owner::Instance(id),
            Event::Named("AncestryChanged"),
            number,
            args,
        );
    }
}

/// The `Value` of a Value object (see `ClassName::is_value_object`), as
/// Lua; `None` for any other class.
fn value_object_value(lua: &Lua, id: InstanceId) -> Option<mlua::Value> {
    let value = {
        let world = instance::world(lua).ok()?;
        let object = world.get(id)?;
        if !object.class().is_value_object() {
            return None;
        }
        object.property("Value")?.clone()
    };
    crate::types::value_to_lua(lua, &value).ok()
}

/// The `Value` of a widget (see `ClassName::is_value_widget`), as Lua.
/// The World is let go of before it's returned, so handlers can use it.
fn widget_value(lua: &Lua, id: InstanceId) -> Option<mlua::Value> {
    let value = {
        let world = instance::world(lua).ok()?;
        let widget = world.get(id)?;
        if !widget.class().is_value_widget() {
            return None;
        }
        widget.property("Value")?.clone()
    };
    crate::types::value_to_lua(lua, &value).ok()
}

fn exists(lua: &Lua, id: InstanceId) -> bool {
    instance::world(lua).is_ok_and(|world| world.contains(id))
}
