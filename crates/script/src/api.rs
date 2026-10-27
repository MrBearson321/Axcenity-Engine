//! The engine's global names in a script: `print`, `game`, `workspace`,
//! `script`, `Instance`, `Vector3`, `Vector2`, `Color3`, `UDim2`, `CFrame`,
//! `PhysicalProperties`, `Random`, `TweenInfo` and `Enum`.
//!
//! Every script gets its own copy of these tables, so a script that replaces
//! `Vector3.new` only breaks itself.

use axcenity_core::{ClassName, Color3, UDim2, Vector2, Vector3};
use mlua::{Function, Lua, MultiValue, Table};

use crate::instance::{LuaInstance, world_mut};
use crate::sandbox::ScriptIds;
use crate::types::{LuaColor3, LuaEnum, LuaUDim2, LuaVector2, LuaVector3};
use crate::{OutputKind, push_output};

pub fn install(lua: &Lua, env: &Table, ids: ScriptIds) -> mlua::Result<()> {
    env.raw_set("print", lua.create_function(print)?)?;
    env.raw_set("game", LuaInstance(ids.game))?;
    env.raw_set("workspace", LuaInstance(ids.workspace))?;
    env.raw_set("script", LuaInstance(ids.script))?;
    env.raw_set("Instance", instance_library(lua)?)?;
    env.raw_set("Vector3", vector3_library(lua)?)?;
    env.raw_set("Vector2", vector2_library(lua)?)?;
    env.raw_set("Color3", color3_library(lua)?)?;
    env.raw_set("UDim2", udim2_library(lua)?)?;
    env.raw_set("CFrame", crate::cframe::library(lua)?)?;
    env.raw_set("PhysicalProperties", crate::physical::library(lua)?)?;
    env.raw_set("Random", crate::random::library(lua)?)?;
    env.raw_set("TweenInfo", crate::tween::tween_info_library(lua)?)?;
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
            if class == ClassName::AnimationTrack {
                return Err(mlua::Error::runtime(
                    "an AnimationTrack comes from animator:LoadAnimation(animation), not Instance.new",
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
    let constants = [
        ("zero", Vector3::new(0.0, 0.0, 0.0)),
        ("one", Vector3::new(1.0, 1.0, 1.0)),
        ("xAxis", Vector3::new(1.0, 0.0, 0.0)),
        ("yAxis", Vector3::new(0.0, 1.0, 0.0)),
        ("zAxis", Vector3::new(0.0, 0.0, 1.0)),
    ];
    for (name, value) in constants {
        library.raw_set(name, LuaVector3(value))?;
    }
    Ok(library)
}

fn vector2_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    // Left-out components are 0, so `Vector2.new()` is the top left.
    library.raw_set(
        "new",
        lua.create_function(|_, (x, y): (Option<f64>, Option<f64>)| {
            Ok(LuaVector2(Vector2::new(
                x.unwrap_or(0.0) as f32,
                y.unwrap_or(0.0) as f32,
            )))
        })?,
    )?;
    let constants = [
        ("zero", Vector2::new(0.0, 0.0)),
        ("one", Vector2::new(1.0, 1.0)),
        ("xAxis", Vector2::new(1.0, 0.0)),
        ("yAxis", Vector2::new(0.0, 1.0)),
    ];
    for (name, value) in constants {
        library.raw_set(name, LuaVector2(value))?;
    }
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
    library.raw_set(
        "fromHSV",
        lua.create_function(|_, (h, s, v): (f64, f64, f64)| {
            Ok(LuaColor3::from_hsv(h as f32, s as f32, v as f32))
        })?,
    )?;
    library.raw_set(
        "fromHex",
        lua.create_function(|_, hex: String| {
            LuaColor3::from_hex(&hex).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "Color3.fromHex takes six hex digits such as \"#ff8000\" (or three, such as \"#f80\"), not \"{hex}\""
                ))
            })
        })?,
    )?;
    Ok(library)
}

fn udim2_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(
            |_,
             (x_scale, x_offset, y_scale, y_offset): (
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
            )| {
                Ok(LuaUDim2(UDim2::new(
                    x_scale.unwrap_or(0.0) as f32,
                    x_offset.unwrap_or(0.0) as f32,
                    y_scale.unwrap_or(0.0) as f32,
                    y_offset.unwrap_or(0.0) as f32,
                )))
            },
        )?,
    )?;
    library.raw_set(
        "fromScale",
        lua.create_function(|_, (x, y): (Option<f64>, Option<f64>)| {
            Ok(LuaUDim2(UDim2::new(
                x.unwrap_or(0.0) as f32,
                0.0,
                y.unwrap_or(0.0) as f32,
                0.0,
            )))
        })?,
    )?;
    library.raw_set(
        "fromOffset",
        lua.create_function(|_, (x, y): (Option<f64>, Option<f64>)| {
            Ok(LuaUDim2(UDim2::new(
                0.0,
                x.unwrap_or(0.0) as f32,
                0.0,
                y.unwrap_or(0.0) as f32,
            )))
        })?,
    )?;
    Ok(library)
}
