//! Change events: `Changed`, `GetPropertyChangedSignal`, `ChildAdded` and
//! `ChildRemoved`, and a widget's `ValueChanged`.
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
    for recorded in changes {
        let number = recorded.number;
        match recorded.change {
            Change::Property { instance, name } => {
                if !exists(lua, instance) {
                    continue;
                }
                let owner = Owner::Instance(instance);
                // `Changed` gets the property's name; a property's own signal
                // gets nothing, as in Roblox.
                events::fire_change(lua, owner, Event::Named("Changed"), number, name);
                events::fire_change(lua, owner, Event::PropertyChanged(name), number, ());
                // A Slider, Toggle, Dropdown or ProgressBar also says what
                // its new Value is.
                if name == "Value"
                    && let Some(value) = widget_value(lua, instance)
                {
                    events::fire_change(lua, owner, Event::Named("ValueChanged"), number, value);
                }
            }
            Change::ChildAdded { parent, child } => {
                fire_child_event(lua, parent, "ChildAdded", child, number);
            }
            Change::ChildRemoved { parent, child } => {
                fire_child_event(lua, parent, "ChildRemoved", child, number);
            }
        }
    }
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
