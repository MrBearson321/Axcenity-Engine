//! `game:GetService("SaveService")`: numbered save slots, player data,
//! settings and checkpoints (see the `save` crate).
//!
//! Both sides share one [`Saves`], so a LocalScript and a Script see the
//! same player data and the same active checkpoint. Slots go in the folder
//! the host gives with [`ScriptRuntime::set_save_folder`](crate::ScriptRuntime::set_save_folder);
//! without one, saving and loading fail with a message.

use std::cell::RefCell;
use std::rc::Rc;

use axcenity_save::{SaveError, Saves, place_character, slot_number};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, UserData, UserDataMethods};

use crate::instance::{LuaInstance, world, world_mut};
use crate::types::{any_value_from_lua, type_name, value_to_lua};

/// The game's saves, shared by both sides' interpreters.
#[derive(Clone, Default)]
pub(crate) struct SharedSaves(pub Rc<RefCell<Saves>>);

/// `SaveService`, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaSaveService;

fn saves(lua: &Lua) -> mlua::Result<Rc<RefCell<Saves>>> {
    lua.app_data_ref::<SharedSaves>()
        .map(|shared| shared.0.clone())
        .ok_or_else(|| mlua::Error::runtime("saving isn't ready"))
}

fn failed(err: SaveError) -> mlua::Error {
    mlua::Error::runtime(err.to_string())
}

/// A slot number given to `method`.
fn slot(lua: &Lua, value: &mlua::Value, method: &str) -> mlua::Result<u32> {
    let number = match value {
        mlua::Value::Integer(n) => *n as f64,
        mlua::Value::Number(n) => *n,
        other => {
            return Err(mlua::Error::runtime(format!(
                "{method} needs a slot number, such as SaveService:{method}(1), not a {}",
                type_name(lua, other)
            )));
        }
    };
    slot_number(number).map_err(failed)
}

/// A name of player data or a setting, given to `method`.
fn key(lua: &Lua, value: &mlua::Value, method: &str) -> mlua::Result<String> {
    match value {
        mlua::Value::String(name) => Ok(name.to_string_lossy()),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a name, such as SaveService:{method}(\"Coins\", ...), not a {}",
            type_name(lua, other)
        ))),
    }
}

/// Sets (or, for nil, removes) a value in player data or settings.
fn set_value(
    lua: &Lua,
    method: &str,
    name: mlua::Value,
    value: mlua::Value,
    field: impl FnOnce(&mut Saves) -> &mut std::collections::BTreeMap<String, axcenity_core::Value>,
) -> mlua::Result<()> {
    let name = key(lua, &name, method)?;
    let converted = match &value {
        mlua::Value::Nil => None,
        other => Some(any_value_from_lua(lua, other).ok_or_else(|| {
            mlua::Error::runtime(format!(
                "{method} keeps a boolean, number, string, Vector3, Vector2, Color3, UDim2 or EnumItem (or nil to remove it), not a {}",
                type_name(lua, other)
            ))
        })?),
    };
    let saves = saves(lua)?;
    let mut saves = saves.borrow_mut();
    let values = field(&mut saves);
    match converted {
        Some(value) => values.insert(name, value),
        None => values.remove(&name),
    };
    Ok(())
}

impl UserData for LuaSaveService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("SaveSlot", |lua, _, number: mlua::Value| {
            let slot = slot(lua, &number, "SaveSlot")?;
            let saves = saves(lua)?;
            let world = world(lua)?;
            saves.borrow().save(&world, slot).map_err(failed)?;
            Ok(())
        });
        methods.add_method("LoadSlot", |lua, _, number: mlua::Value| {
            let slot = slot(lua, &number, "LoadSlot")?;
            let saves = saves(lua)?;
            let mut world = world_mut(lua)?;
            saves.borrow_mut().load(&mut world, slot).map_err(failed)
        });
        methods.add_method("HasSlot", |lua, _, number: mlua::Value| {
            let slot = slot(lua, &number, "HasSlot")?;
            Ok(saves(lua)?.borrow().has_slot(slot))
        });
        methods.add_method(
            "SetPlayerData",
            |lua, _, (name, value): (mlua::Value, mlua::Value)| {
                set_value(lua, "SetPlayerData", name, value, |saves| {
                    &mut saves.player_data
                })
            },
        );
        methods.add_method("GetPlayerData", |lua, _, name: mlua::Value| {
            let name = key(lua, &name, "GetPlayerData")?;
            let value = saves(lua)?.borrow().player_data.get(&name).cloned();
            match value {
                Some(value) => value_to_lua(lua, &value),
                None => Ok(mlua::Value::Nil),
            }
        });
        methods.add_method(
            "SetSetting",
            |lua, _, (name, value): (mlua::Value, mlua::Value)| {
                set_value(lua, "SetSetting", name, value, |saves| &mut saves.settings)
            },
        );
        methods.add_method("GetSetting", |lua, _, name: mlua::Value| {
            let name = key(lua, &name, "GetSetting")?;
            let value = saves(lua)?.borrow().settings.get(&name).cloned();
            match value {
                Some(value) => value_to_lua(lua, &value),
                None => Ok(mlua::Value::Nil),
            }
        });
        methods.add_method("SetCheckpoint", |lua, _, checkpoint: mlua::Value| {
            let Ok(checkpoint) = LuaInstance::from_lua(checkpoint.clone(), lua) else {
                return Err(mlua::Error::runtime(format!(
                    "SetCheckpoint needs a Checkpoint, such as SaveService:SetCheckpoint(workspace.Cave), not a {}",
                    type_name(lua, &checkpoint)
                )));
            };
            let saves = saves(lua)?;
            let world = world(lua)?;
            saves
                .borrow_mut()
                .set_checkpoint(&world, checkpoint.0)
                .map_err(failed)
        });
        methods.add_method("GetCheckpoint", |lua, _, ()| {
            let saves = saves(lua)?;
            let world = world(lua)?;
            let checkpoint = saves.borrow().checkpoint(&world);
            drop(world);
            checkpoint.map(LuaInstance).into_lua(lua)
        });
        // Puts back what was captured at the checkpoint, then stands the
        // player character (the one whose controller owns the camera) on it.
        methods.add_method("RestoreCheckpoint", |lua, _, ()| {
            let saves = saves(lua)?;
            let mut world = world_mut(lua)?;
            let at = saves
                .borrow_mut()
                .restore_checkpoint(&mut world)
                .map_err(failed)?;
            if let Some(character) = axcenity_character::player_character(&world) {
                let parts = axcenity_character::body_parts(&world, character);
                place_character(&mut world, &parts, at);
            }
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("SaveService"));
    }
}
