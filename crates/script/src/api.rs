//! The engine's global names in a script: `print`, `game`, `workspace`,
//! `script`, `Instance`, `Vector3`, `Color3` and `Enum`.
//!
//! Every script gets its own copy of these tables, so a script that replaces
//! `Vector3.new` only breaks itself.

use axcenity_core::{ClassName, Color3, Vector3};
use mlua::{Function, Lua, MultiValue, Table};

use crate::instance::{LuaInstance, world_mut};
use crate::sandbox::ScriptIds;
use crate::types::{LuaColor3, LuaEnum, LuaVector3};
use crate::{OutputKind, push_output};

pub fn install(lua: &Lua, env: &Table, ids: ScriptIds) -> mlua::Result<()> {
    env.raw_set("print", lua.create_function(print)?)?;
    env.raw_set("game", LuaInstance(ids.game))?;
    env.raw_set("workspace", LuaInstance(ids.workspace))?;
    env.raw_set("script", LuaInstance(ids.script))?;
    env.raw_set("Instance", instance_library(lua)?)?;
    env.raw_set("Vector3", vector3_library(lua)?)?;
    env.raw_set("Color3", color3_library(lua)?)?;
    env.raw_set("Enum", LuaEnum)?;
    Ok(())
}

/// Like Lua's own `print`: each value through `tostring`, separated by tabs.
fn print(lua: &Lua, values: MultiValue) -> mlua::Result<()> {
    // The real `tostring`, from Lua's global table rather than the script's,
    // so `print` still works in a script that replaced its own `tostring`.
    let tostring: Function = lua.globals().raw_get("tostring")?;
    let texts = values
        .into_iter()
        .map(|value| tostring.call::<String>(value))
        // Collecting into `Result<Vec<_>>` stops at the first error, if any.
        .collect::<mlua::Result<Vec<String>>>()?;
    push_output(lua, OutputKind::Print, texts.join("\t"));
    Ok(())
}

fn instance_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, class_name: String| {
            let class = ClassName::from_name(&class_name).ok_or_else(|| {
                mlua::Error::runtime(format!("there is no class named \"{class_name}\""))
            })?;
            if class == ClassName::Player {
                return Err(mlua::Error::runtime(
                    "a Player is made by the engine when someone joins, not with Instance.new",
                ));
            }
            let id = world_mut(lua)?
                .create(class)
                .map_err(mlua::Error::runtime)?;
            Ok(LuaInstance(id))
        })?,
    )?;
    Ok(library)
}

fn vector3_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    // Left-out components are 0, so `Vector3.new()` is the origin.
    library.raw_set(
        "new",
        lua.create_function(|_, (x, y, z): (Option<f64>, Option<f64>, Option<f64>)| {
            Ok(LuaVector3(Vector3::new(
                x.unwrap_or(0.0) as f32,
                y.unwrap_or(0.0) as f32,
                z.unwrap_or(0.0) as f32,
            )))
        })?,
    )?;
    Ok(library)
}

fn color3_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|_, (r, g, b): (Option<f64>, Option<f64>, Option<f64>)| {
            let channels = [r, g, b].map(|channel| channel.unwrap_or(0.0));
            if channels.iter().any(|c| !(0.0..=1.0).contains(c)) {
                return Err(mlua::Error::runtime(
                    "Color3.new takes numbers from 0 to 1; for 0 to 255, use Color3.fromRGB",
                ));
            }
            let [r, g, b] = channels.map(|c| c as f32);
            Ok(LuaColor3(Color3::new(r, g, b)))
        })?,
    )?;
    library.raw_set(
        "fromRGB",
        lua.create_function(|_, (r, g, b): (Option<f64>, Option<f64>, Option<f64>)| {
            let channels = [r, g, b].map(|channel| channel.unwrap_or(0.0));
            if channels.iter().any(|c| !(0.0..=255.0).contains(c)) {
                return Err(mlua::Error::runtime(
                    "Color3.fromRGB takes numbers from 0 to 255",
                ));
            }
            let [r, g, b] = channels.map(|c| (c / 255.0) as f32);
            Ok(LuaColor3(Color3::new(r, g, b)))
        })?,
    )?;
    Ok(library)
}
