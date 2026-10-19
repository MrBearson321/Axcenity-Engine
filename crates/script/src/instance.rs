//! Instances as Lua values. A Lua Instance only holds an [`InstanceId`];
//! every read and write looks the Instance up in the World, which is lent to
//! Lua while scripts run (see `with_world` in `lib.rs`).

use axcenity_audio::SoundAction;
use axcenity_core::{ClassName, INSTANCE_EVENTS, InstanceId, Value, World};
use mlua::{
    AppDataRef, AppDataRefMut, FromLua, Function, IntoLua, Lua, MetaMethod, UserData,
    UserDataMethods,
};

use crate::client::{LocalPlayer, LuaCamera, LuaUserInputService};
use crate::events::{Event, LuaSignal, Owner};
use crate::run_service::LuaRunService;
use crate::sides;
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
        // `part:GetPropertyChangedSignal("Position")`: an event for one
        // property, whose handlers get no arguments.
        methods.add_method("GetPropertyChangedSignal", |lua, this, property: String| {
            let world = world(lua)?;
            let instance = world.get(this.0).ok_or_else(destroyed)?;
            // `&'static str`s, so the signal can keep the one that matches.
            let names: Vec<&'static str> = ["Name", "Parent"]
                .into_iter()
                .chain(instance.properties().map(|(name, _)| name))
                .collect();
            match names.iter().find(|name| **name == property) {
                Some(&name) => Ok(LuaSignal {
                    owner: Owner::Instance(this.0),
                    event: Event::PropertyChanged(name),
                }),
                None => Err(mlua::Error::runtime(format!(
                    "{property} is not a property of {}; choose from {}",
                    instance.class(),
                    names.join(", ")
                ))),
            }
        });

        // `instance.Key`: a field, then a property, then a member only
        // some classes have, then an event, then a child with that name.
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
    let class = instance.class();
    if let Some(member) = class_member(lua, class, key)? {
        return Ok(member);
    }
    let mut events = class.events().iter().chain(&INSTANCE_EVENTS);
    if let Some(&event) = events.find(|&&event| event == key) {
        // `OnServerEvent` only works on the server, and `OnClientEvent` only
        // on the client.
        sides::check(lua, key)?;
        return LuaSignal::named(Owner::Instance(id), event).into_lua(lua);
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
                    None if class.events().contains(&key) || INSTANCE_EVENTS.contains(&key) => {
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

/// Methods only some classes have, such as `remote:FireServer` and
/// `game:GetService`. Made once per interpreter (see `context.rs`) and kept
/// in its app data.
pub struct ClassMethods(pub Vec<(ClassName, &'static str, Function)>);

/// A member of `class` that isn't a property or an event: a method such as
/// `remote:FireServer`, or a field such as `workspace.CurrentCamera`.
/// Side-only members check the side first.
fn class_member(lua: &Lua, class: ClassName, key: &str) -> mlua::Result<Option<mlua::Value>> {
    let value = match (class, key) {
        // Roblox code uses `game:GetService("UserInputService")`, but
        // `game.UserInputService` works there too.
        (ClassName::DataModel, "UserInputService") => LuaUserInputService.into_lua(lua)?,
        (ClassName::DataModel, "RunService") => LuaRunService.into_lua(lua)?,
        (ClassName::Workspace, "CurrentCamera") => LuaCamera.into_lua(lua)?,
        (ClassName::Players, "LocalPlayer") => lua
            .app_data_ref::<LocalPlayer>()
            .map(|player| LuaInstance(player.0))
            .into_lua(lua)?,
        _ => {
            let method = lua.app_data_ref::<ClassMethods>().and_then(|methods| {
                methods
                    .0
                    .iter()
                    .find(|(c, name, _)| *c == class && *name == key)
                    .map(|(_, _, function)| function.clone())
            });
            match method {
                Some(function) => mlua::Value::Function(function),
                None => return Ok(None),
            }
        }
    };
    sides::check(lua, key)?;
    Ok(Some(value))
}

/// The methods exposed by Sound Instances, which queue host audio actions.
pub fn sound_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(|lua, this: LuaInstance| {
        let (sound_id, volume, looped) = {
            let world = world(lua)?;
            let sound = world.get(this.0).ok_or_else(destroyed)?;
            let sound_id = sound
                .property("SoundId")
                .and_then(|value| value.as_str())
                .ok_or_else(|| mlua::Error::runtime("Sound.SoundId must be a file path"))?;
            let volume = sound
                .property("Volume")
                .and_then(|value| value.as_number())
                .ok_or_else(|| mlua::Error::runtime("Sound.Volume must be a number"))?;
            let looped = sound
                .property("Looped")
                .and_then(|value| value.as_bool())
                .ok_or_else(|| mlua::Error::runtime("Sound.Looped must be a boolean"))?;
            (sound_id.to_owned(), volume, looped)
        };
        if sound_id.is_empty() {
            return Err(mlua::Error::runtime(
                "Sound.SoundId must name an audio file inside the project folder",
            ));
        }
        if !volume.is_finite() || volume < 0.0 {
            return Err(mlua::Error::runtime(
                "Sound.Volume must be a finite number greater than or equal to 0",
            ));
        }
        set_playing(lua, this.0, true)?;
        let mut actions = lua
            .app_data_mut::<crate::SoundActions>()
            .ok_or_else(|| mlua::Error::runtime("sound actions aren't ready"))?;
        actions.0.push(SoundAction::Play {
            instance: this.0,
            sound_id,
            volume,
            looped,
        });
        Ok(())
    })?;
    let stop = lua.create_function(|lua, this: LuaInstance| {
        set_playing(lua, this.0, false)?;
        let mut actions = lua
            .app_data_mut::<crate::SoundActions>()
            .ok_or_else(|| mlua::Error::runtime("sound actions aren't ready"))?;
        actions.0.push(SoundAction::Stop(this.0));
        Ok(())
    })?;
    // Pausing and resuming are `Playing` changes; the host's audio follows it.
    let pause = lua.create_function(|lua, this: LuaInstance| set_playing(lua, this.0, false))?;
    let resume = lua.create_function(|lua, this: LuaInstance| set_playing(lua, this.0, true))?;
    Ok(vec![
        (ClassName::Sound, "Play", play),
        (ClassName::Sound, "Stop", stop),
        (ClassName::Sound, "Pause", pause),
        (ClassName::Sound, "Resume", resume),
    ])
}

/// Sets a Sound's `Playing`, which fires its change events like any property.
fn set_playing(lua: &Lua, sound: InstanceId, playing: bool) -> mlua::Result<()> {
    world_mut(lua)?
        .set_property(sound, "Playing", Value::Bool(playing))
        .map_err(|_| destroyed())
}

/// `game:GetService(name)`: a service by its class name, such as
/// `"Players"`, or `"UserInputService"` (LocalScripts only) or
/// `"RunService"`, which aren't Instances.
pub fn get_service(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|lua, (_game, name): (mlua::Value, String)| {
        match name.as_str() {
            "UserInputService" => {
                sides::check(lua, &name)?;
                return LuaUserInputService.into_lua(lua);
            }
            "RunService" => return LuaRunService.into_lua(lua),
            _ => {}
        }
        let world = world(lua)?;
        let services: Vec<InstanceId> = world
            .get(world.root())
            .map(|game| game.children().to_vec())
            .unwrap_or_default();
        let found = services.iter().copied().find(|&id| {
            world
                .get(id)
                .is_some_and(|service| service.class().as_str() == name)
        });
        match found {
            Some(id) => LuaInstance(id).into_lua(lua),
            None => {
                let mut names: Vec<&str> = services
                    .iter()
                    .filter_map(|&id| world.get(id).map(|s| s.class().as_str()))
                    .collect();
                names.extend(["RunService", "UserInputService"]);
                Err(mlua::Error::runtime(format!(
                    "there is no service named \"{name}\"; choose from {}",
                    names.join(", ")
                )))
            }
        }
    })
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
