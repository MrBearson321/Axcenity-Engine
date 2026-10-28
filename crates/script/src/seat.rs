//! Seats (Phase 39): `seat:Sit(controller)`. It only writes the seat's
//! `Occupant` and the controller's `Sit`; the character controller sits
//! the character on its next step (see `character`). `Occupant` is
//! read-only for scripts.

use axcenity_core::{ClassName, InstanceId, Value, World};
use mlua::{FromLua, Function, Lua};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::type_name;

/// Why `Occupant` can't be set.
pub const OCCUPANT_READ_ONLY: &str = "Seat.Occupant is read-only; use seat:Sit(controller) to sit a character, or set controller.Sit = false to stand it up";

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let sit = lua.create_function(|lua, (this, controller): (LuaInstance, mlua::Value)| {
        let wrong = || {
            mlua::Error::runtime(format!(
                "Sit(controller) needs a ThirdPersonController or FirstPersonController, not {}",
                type_name(lua, &controller)
            ))
        };
        let controller = LuaInstance::from_lua(controller.clone(), lua)
            .map_err(|_| wrong())?
            .0;
        let is_controller = world(lua)?
            .get(controller)
            .is_some_and(|instance| instance.class().is_character_controller());
        if !is_controller {
            return Err(wrong());
        }
        let mut world = world_mut(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        sit(&mut world, this.0, controller);
        Ok(())
    })?;
    Ok(vec![(ClassName::Seat, "Sit", sit)])
}

/// Makes `controller` the occupant of `seat` and takes it off any other seat.
fn sit(world: &mut World, seat: InstanceId, controller: InstanceId) {
    let occupied: Vec<InstanceId> = world
        .descendants(world.root())
        .into_iter()
        .filter(|&id| {
            id != seat
                && world.get(id).is_some_and(|other| {
                    other.class() == ClassName::Seat
                        && other.property("Occupant") == Some(&Value::Instance(Some(controller)))
                })
        })
        .collect();
    for other in occupied {
        let _ = world.set_property(other, "Occupant", Value::Instance(None));
    }
    let _ = world.set_property(seat, "Occupant", Value::Instance(Some(controller)));
    let _ = world.set_property(controller, "Sit", Value::Bool(true));
}
