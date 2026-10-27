//! `game:GetService("Debris")`: `Debris:AddItem(instance, seconds)`
//! destroys `instance` once `seconds` of game time have passed, without
//! the script waiting. The items belong to the side, not to a script, so
//! they're destroyed even if the script that added them has stopped.

use axcenity_core::InstanceId;
use mlua::{FromLua, Lua, MetaMethod, UserData, UserDataMethods, Value};

use crate::instance::LuaInstance;
use crate::types::type_name;
use crate::{push_error, scheduler, tree};

/// How long an item lasts when `AddItem` isn't told, as in Roblox.
const DEFAULT_SECONDS: f64 = 10.0;

/// The Debris service, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaDebris;

/// The Instances waiting to be destroyed, each with the game time it's due.
#[derive(Default)]
pub struct DebrisItems(Vec<(f64, InstanceId)>);

impl UserData for LuaDebris {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "AddItem",
            |lua, _, (instance, seconds): (Value, Option<Value>)| {
                let Ok(LuaInstance(id)) = LuaInstance::from_lua(instance.clone(), lua) else {
                    return Err(mlua::Error::runtime(format!(
                        "Debris:AddItem takes an Instance to destroy, not a {}",
                        type_name(lua, &instance)
                    )));
                };
                let seconds = match seconds {
                    None | Some(Value::Nil) => DEFAULT_SECONDS,
                    Some(Value::Integer(n)) => n as f64,
                    Some(Value::Number(n)) => n,
                    Some(other) => {
                        return Err(mlua::Error::runtime(format!(
                            "Debris:AddItem takes a number of seconds, not a {}",
                            type_name(lua, &other)
                        )));
                    }
                };
                // `max` also turns NaN into 0: gone at the next step.
                let due = scheduler::now(lua) + seconds.max(0.0);
                if lua.app_data_ref::<DebrisItems>().is_none() {
                    lua.set_app_data(DebrisItems::default());
                }
                if let Some(mut items) = lua.app_data_mut::<DebrisItems>() {
                    items.0.push((due, id));
                }
                Ok(())
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Debris"));
    }
}

/// Destroys every item that is due by now. One already destroyed is
/// skipped (destroying twice does nothing).
pub fn step(lua: &Lua) {
    let now = scheduler::now(lua);
    let due: Vec<InstanceId> = match lua.app_data_mut::<DebrisItems>() {
        Some(mut items) => {
            // `partition` splits the list in two: due now, and still waiting.
            let (due, waiting) = std::mem::take(&mut items.0)
                .into_iter()
                .partition(|(time, _)| *time <= now + scheduler::TOLERANCE_SECONDS);
            items.0 = waiting;
            due.into_iter().map(|(_, id)| id).collect()
        }
        None => return,
    };
    for id in due {
        if let Err(err) = tree::destroy(lua, id) {
            push_error(lua, &err);
        }
    }
}
