//! `DataLayerService` in a game (roadmap R16.4): `SetLayerActive`,
//! `IsLayerActive` and `LayerChanged`. Parking is `core::data_layers`;
//! a layer's `Active` is read-only for scripts.

use axcenity_core::{ClassName, InstanceId, Value, data_layers};
use mlua::{Function, Lua};

use crate::events::{self, Event, Owner};
use crate::instance::{world, world_mut};
use crate::sides::Side;
use crate::types::type_name;

/// Why `DataLayer.Active` can't be set.
pub const ACTIVE_READ_ONLY: &str = "DataLayer.Active is read-only: use DataLayerService:SetLayerActive(name, active) to switch a layer on or off";

fn layer_name(lua: &Lua, value: &mlua::Value, call: &str) -> mlua::Result<String> {
    match value {
        mlua::Value::String(name) => Ok(name.to_string_lossy()),
        other => Err(mlua::Error::runtime(format!(
            "{call} needs a layer's name, such as \"Night\", not a {}",
            type_name(lua, other)
        ))),
    }
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let set = lua.create_function(
        |lua, (_this, name, active): (mlua::Value, mlua::Value, mlua::Value)| {
            let call = "SetLayerActive(name, active)";
            let name = layer_name(lua, &name, call)?;
            let mlua::Value::Boolean(active) = active else {
                return Err(mlua::Error::runtime(format!(
                    "{call} needs true or false, not a {}",
                    type_name(lua, &active)
                )));
            };
            if crate::system::side(lua) == Side::Client {
                return Err(mlua::Error::runtime(
                    "SetLayerActive can only be called from a Script: the server switches layers, and players see the change",
                ));
            }
            let mut world = world_mut(lua)?;
            data_layers::set_active(&mut world, &name, active)
                .map_err(|err| mlua::Error::runtime(format!("SetLayerActive: {err}")))?;
            Ok(())
        },
    )?;
    let is = lua.create_function(|lua, (_this, name): (mlua::Value, mlua::Value)| {
        let name = layer_name(lua, &name, "IsLayerActive(name)")?;
        let world = world(lua)?;
        let layer = data_layers::layer(&world, &name).ok_or_else(|| {
            mlua::Error::runtime(format!(
                "IsLayerActive: {}",
                data_layers::NoSuchLayer(name.clone())
            ))
        })?;
        Ok(data_layers::is_active(&world, layer))
    })?;
    Ok(vec![
        (ClassName::DataLayerService, "SetLayerActive", set),
        (ClassName::DataLayerService, "IsLayerActive", is),
    ])
}

/// `LayerChanged(name, active)` when a layer's `Active` changed, on this
/// side: the server's own change, or a player's replica of it.
pub(crate) fn fire_layer_changed(lua: &Lua, instance: InstanceId, number: u64) {
    let (owner, name, active) = {
        let Ok(world) = world(lua) else {
            return;
        };
        let service = world.data_layer_service();
        let owner = Owner::Instance(service);
        if !events::has_handlers(lua, owner) {
            return;
        }
        let Some(layer) = world.get(instance) else {
            return;
        };
        if layer.class() != ClassName::DataLayer || layer.parent() != Some(service) {
            return;
        }
        let active = !matches!(layer.property("Active"), Some(Value::Bool(false)));
        (owner, layer.name().to_owned(), active)
    };
    events::fire_change(
        lua,
        owner,
        Event::Named("LayerChanged"),
        number,
        (name, active),
    );
}
