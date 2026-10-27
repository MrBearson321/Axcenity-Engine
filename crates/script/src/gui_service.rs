//! `game:GetService("GuiService")`: the player's game UI settings, for a
//! settings menu (see [`axcenity_core::UiSettings`]).
//!
//! `TextScale`, `HighContrast` and `ReducedMotion` change every HUD and
//! menu the next time the UI is drawn, and never the 3D world. They belong
//! to the world, so both sides see the same ones, and a save slot keeps
//! them (as the settings `UI/TextScale`, `UI/HighContrast` and
//! `UI/ReducedMotion`).

use axcenity_core::{TEXT_SCALE_RANGE, UI_SETTING_PREFIX, Value};
use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::instance::{world, world_mut};
use crate::types::type_name;

/// `GuiService`, as scripts see it. Holds nothing itself.
#[derive(Clone, Copy)]
pub struct LuaGuiService;

/// Sets the game UI setting `property` to `value`. An error saying what it
/// takes when it doesn't fit, and nothing changes.
fn set(lua: &mlua::Lua, property: &str, value: &mlua::Value) -> mlua::Result<()> {
    let given = match value {
        mlua::Value::Integer(n) => Some(Value::Number(*n as f64)),
        mlua::Value::Number(n) => Some(Value::Number(*n)),
        mlua::Value::Boolean(on) => Some(Value::Bool(*on)),
        _ => None,
    };
    let mut settings = world(lua)?.ui_settings();
    let name = format!("{UI_SETTING_PREFIX}{property}");
    if let Some(given) = &given
        && settings.set(&name, given) == Ok(true)
    {
        world_mut(lua)?.set_ui_settings(settings);
        return Ok(());
    }
    let (min, max) = TEXT_SCALE_RANGE;
    let what = match property {
        "TextScale" => format!("a number from {min} to {max}"),
        _ => "true or false".to_owned(),
    };
    let not = match value {
        mlua::Value::Integer(n) => n.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        other => format!("a {}", type_name(lua, other)),
    };
    Err(mlua::Error::runtime(format!(
        "GuiService.{property} must be {what}, not {not}"
    )))
}

impl UserData for LuaGuiService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("TextScale", |lua, _| {
            Ok(world(lua)?.ui_settings().text_scale)
        });
        fields.add_field_method_get("HighContrast", |lua, _| {
            Ok(world(lua)?.ui_settings().high_contrast)
        });
        fields.add_field_method_get("ReducedMotion", |lua, _| {
            Ok(world(lua)?.ui_settings().reduced_motion)
        });
        for property in ["TextScale", "HighContrast", "ReducedMotion"] {
            fields.add_field_method_set(property, move |lua, _, value: mlua::Value| {
                set(lua, property, &value)
            });
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("GuiService"));
    }
}
