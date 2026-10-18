//! Instances as Lua values. A Lua Instance only holds an [`InstanceId`];
//! every read and write looks the Instance up in the World, which is lent to
//! Lua while scripts run (see `with_world` in `lib.rs`).

use axcenity_core::{InstanceId, World};
use mlua::{
    AppDataRef, AppDataRefMut, FromLua, IntoLua, Lua, MetaMethod, UserData, UserDataMethods,
};

use crate::events::LuaSignal;
use crate::types::{type_name, value_from_lua, value_to_lua};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaInstance(pub InstanceId);

/// The World, for reading. Only fails if called while no script is running.
pub fn world(lua: &Lua) -> mlua::Result<AppDataRef<'_, World>> {
    lua.app_data_ref::<World>().ok_or_else(no_world)
}

/// The World, for changing. Drop the returned guard before calling back into
/// Lua, because the World can't be borrowed twice at once.
pub fn world_mut(lua: &Lua) -> mlua::Result<AppDataRefMut<'_, World>> {
    lua.app_data_mut::<World>().ok_or_else(no_world)
}

fn no_world() -> mlua::Error {
    mlua::Error::runtime("the game world can only be used while scripts are running")
}

fn destroyed() -> mlua::Error {
    mlua::Error::runtime("this Instance has been destroyed")
}

impl FromLua for LuaInstance {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        if let mlua::Value::UserData(data) = &value
            && let Ok(instance) = data.borrow::<LuaInstance>()
        {
            return Ok(*instance);
        }
        Err(mlua::Error::FromLuaConversionError {
            from: value.type_name(),
            to: "Instance".to_owned(),
            message: None,
        })
    }
}

impl UserData for LuaInstance {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Methods are looked up before `__index` below, so they win over
        // properties and children with the same name.
        methods.add_method(
            "FindFirstChild",
            |lua, this, (name, recursive): (String, Option<bool>)| {
                let world = world(lua)?;
                let found = if recursive.unwrap_or(false) {
                    world.find_first_descendant(this.0, &name)
                } else {
                    world.find_first_child(this.0, &name)
                };
                Ok(found.map(LuaInstance))
            },
        );

        // `instance.Key`: a field, then a property, then an event, then a
        // child with that name.
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            get(lua, this.0, &key)
        });
        // `instance.Key = value`.
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, value): (String, mlua::Value)| set(lua, this.0, &key, value),
        );
        methods.add_meta_method(MetaMethod::Eq, |lua, this, other: mlua::Value| {
            Ok(LuaInstance::from_lua(other, lua).is_ok_and(|other| other == *this))
        });
        // Like Roblox, an Instance prints as its name.
        methods.add_meta_method(MetaMethod::ToString, |lua, this, ()| {
            let world = world(lua)?;
            Ok(world.get(this.0).map_or_else(
                || "(destroyed Instance)".to_owned(),
                |i| i.name().to_owned(),
            ))
        });
    }
}

fn get(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<mlua::Value> {
    let world = world(lua)?;
    let instance = world.get(id).ok_or_else(destroyed)?;
    match key {
        "Name" => return instance.name().into_lua(lua),
        "ClassName" => return instance.class().as_str().into_lua(lua),
        "Parent" => return instance.parent().map(LuaInstance).into_lua(lua),
        _ => {}
    }
    if let Some(value) = instance.property(key) {
        return value_to_lua(lua, value);
    }
    if let Some(&event) = instance
        .class()
        .events()
        .iter()
        .find(|&&event| event == key)
    {
        return LuaSignal {
            instance: id,
            event,
        }
        .into_lua(lua);
    }
    if let Some(child) = world.find_first_child(id, key) {
        return LuaInstance(child).into_lua(lua);
    }
    Err(not_a_member(&world, id, key))
}

fn set(lua: &Lua, id: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    // Converting `value` may need to read the World (to name an Instance's
    // class in a message), so convert first and borrow the World mutably after.
    match key {
        "Name" => {
            let mlua::Value::String(name) = &value else {
                return Err(mlua::Error::runtime(format!(
                    "Name must be a string, not a {}",
                    type_name(lua, &value)
                )));
            };
            let name = name.to_string_lossy();
            world_mut(lua)?.set_name(id, name).map_err(|_| destroyed())
        }
        "Parent" => {
            let parent = match &value {
                mlua::Value::Nil => None,
                other => match LuaInstance::from_lua(other.clone(), lua) {
                    Ok(parent) => Some(parent.0),
                    Err(_) => {
                        return Err(mlua::Error::runtime(format!(
                            "Parent must be an Instance or nil, not a {}",
                            type_name(lua, other)
                        )));
                    }
                },
            };
            world_mut(lua)?
                .set_parent(id, parent)
                .map_err(|err| mlua::Error::runtime(format!("can't set Parent: {err}")))
        }
        "ClassName" => Err(mlua::Error::runtime("ClassName can't be changed")),
        _ => {
            let (class, kind) = {
                let world = world(lua)?;
                let instance = world.get(id).ok_or_else(destroyed)?;
                let class = instance.class();
                match instance.property(key) {
                    Some(current) => (class, current.kind()),
                    None if class.events().contains(&key) => {
                        return Err(mlua::Error::runtime(format!(
                            "{class}.{key} is an event and can't be set; use {key}:Connect(function) to react to it"
                        )));
                    }
                    None => return Err(not_a_member(&world, id, key)),
                }
            };
            let new_value = value_from_lua(lua, value, kind)
                .map_err(|message| mlua::Error::runtime(format!("{class}.{key} {message}")))?;
            world_mut(lua)?
                .set_property(id, key, new_value)
                .map_err(mlua::Error::runtime)
        }
    }
}

/// The error for `instance.Key` when `Key` is neither a property nor a child,
/// worded like Roblox's so it's familiar.
fn not_a_member(world: &World, id: InstanceId, key: &str) -> mlua::Error {
    let Some(instance) = world.get(id) else {
        return destroyed();
    };
    let class = instance.class();
    // `full_name` leaves `game` out, so `game` itself has an empty one.
    let name = match world.full_name(id) {
        Some(name) if !name.is_empty() => name,
        _ => instance.name().to_owned(),
    };
    mlua::Error::runtime(format!("{key} is not a valid member of {class} \"{name}\""))
}
