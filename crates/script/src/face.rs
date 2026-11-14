//! FaceControls methods (facial animation F4): expressions, overrides and
//! layer settings. They change the face's live state in the World
//! (`World::face_mut`); the host's animation blends it in each frame.

use axcenity_assets::Assets;
use axcenity_core::ClassName;
use axcenity_core::face::{
    ExpressionPlay, FaceBlend, FaceLayer, FaceLayerSetting, face_control, face_mask_controls,
};
use mlua::{Function, Lua, Table};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::type_name;

/// A number argument of `method` that must be finite and within `range`.
fn number_in(
    value: Option<f64>,
    default: f64,
    range: std::ops::RangeInclusive<f64>,
    what: &str,
    method: &str,
) -> mlua::Result<f64> {
    let value = value.unwrap_or(default);
    if !(value.is_finite() && range.contains(&value)) {
        return Err(mlua::Error::runtime(format!(
            "{method}'s {what} must be a number from {} to {}",
            range.start(),
            range.end()
        )));
    }
    Ok(value)
}

/// An optional seconds argument: finite and 0 or more.
fn seconds(value: Option<f64>, what: &str, method: &str) -> mlua::Result<Option<f64>> {
    match value {
        Some(value) if !(value.is_finite() && value >= 0.0) => Err(mlua::Error::runtime(format!(
            "{method}'s {what} must be a number of seconds, 0 or more"
        ))),
        other => Ok(other),
    }
}

/// Checks a script names an expression the project has or one built in.
fn check_expression(lua: &Lua, name: &str) -> mlua::Result<()> {
    let Some(assets) = lua.app_data_ref::<Assets>() else {
        return Ok(());
    };
    let project = assets.face_expressions();
    if axcenity_anim::find_expression(&project, name).is_some() {
        return Ok(());
    }
    Err(mlua::Error::runtime(format!(
        "SetExpression: there's no expression `{name}`; there are {}",
        axcenity_anim::expression_names(&project).join(", ")
    )))
}

/// The controls a mask names: nil for all, or a group or control name, or
/// a list of them.
fn mask_of(lua: &Lua, value: mlua::Value) -> mlua::Result<Option<Vec<usize>>> {
    let names: Vec<String> = match value {
        mlua::Value::Nil => return Ok(None),
        mlua::Value::String(name) => vec![name.to_str()?.to_owned()],
        mlua::Value::Table(list) => list
            .sequence_values::<String>()
            .collect::<mlua::Result<_>>()?,
        other => {
            return Err(mlua::Error::runtime(format!(
                "SetLayer's mask must be nil, a name such as \"Eyes\" or \"Jaw_Open\", or a list of them, not a {}",
                type_name(lua, &other)
            )));
        }
    };
    let mut controls = Vec::new();
    for name in names {
        let Some(found) = face_mask_controls(&name) else {
            return Err(mlua::Error::runtime(format!(
                "SetLayer: `{name}` isn't a face control or group (Eyes, Eyebrows, Mouth, CheeksAndNose, JawAndTongue)"
            )));
        };
        for control in found {
            if !controls.contains(&control) {
                controls.push(control);
            }
        }
    }
    controls.sort_unstable();
    Ok(Some(controls))
}

/// `SetExpression`, `ClearExpression`, `GetExpressions`, `SetOverride`,
/// `ClearOverrides` and `SetLayer`.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let set_expression = lua.create_function(
        |lua,
         (this, name, intensity, duration, options): (
            LuaInstance,
            String,
            Option<f64>,
            Option<f64>,
            Option<Table>,
        )| {
            const METHOD: &str = "SetExpression";
            let intensity = number_in(intensity, 1.0, 0.0..=1.0, "intensity", METHOD)?;
            let transition = seconds(duration, "transition time", METHOD)?;
            let option = |key: &str| -> mlua::Result<Option<f64>> {
                match &options {
                    Some(options) => options.get::<Option<f64>>(key),
                    None => Ok(None),
                }
            };
            let weight = number_in(option("Weight")?, 1.0, 0.0..=1.0, "Weight", METHOD)?;
            let priority = option("Priority")?.unwrap_or(0.0);
            if !priority.is_finite() {
                return Err(mlua::Error::runtime(
                    "SetExpression's Priority must be a number",
                ));
            }
            let asymmetry = number_in(option("Asymmetry")?, 0.0, -1.0..=1.0, "Asymmetry", METHOD)?;
            check_expression(lua, &name)?;
            let mut world = world_mut(lua)?;
            let state = world.face_mut(this.0).ok_or_else(destroyed)?;
            let order = state.next_order;
            state.next_order += 1;
            match state
                .expressions
                .iter_mut()
                .find(|play| play.name.eq_ignore_ascii_case(&name))
            {
                Some(play) => {
                    play.target_intensity = intensity;
                    play.target_presence = 1.0;
                    play.transition = transition;
                    play.weight = weight;
                    play.priority = priority as i32;
                    play.asymmetry = asymmetry;
                    play.order = order;
                }
                None => state.expressions.push(ExpressionPlay {
                    name,
                    intensity,
                    target_intensity: intensity,
                    presence: 0.0,
                    target_presence: 1.0,
                    transition,
                    weight,
                    priority: priority as i32,
                    asymmetry,
                    order,
                }),
            }
            Ok(())
        },
    )?;
    let clear_expression = lua.create_function(
        |lua, (this, name, duration): (LuaInstance, Option<String>, Option<f64>)| {
            let transition = seconds(duration, "transition time", "ClearExpression")?;
            let mut world = world_mut(lua)?;
            let state = world.face_mut(this.0).ok_or_else(destroyed)?;
            for play in &mut state.expressions {
                if name
                    .as_deref()
                    .is_none_or(|name| play.name.eq_ignore_ascii_case(name))
                {
                    play.target_presence = 0.0;
                    if transition.is_some() {
                        play.transition = transition;
                    }
                }
            }
            Ok(())
        },
    )?;
    let get_expressions = lua.create_function(|lua, this: LuaInstance| {
        let plays: Vec<ExpressionPlay> = {
            let world = world(lua)?;
            if world.get(this.0).is_none() {
                return Err(destroyed());
            }
            world
                .face(this.0)
                .map(|state| {
                    state
                        .expressions
                        .iter()
                        .filter(|play| play.target_presence > 0.0)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        let list = lua.create_table()?;
        for play in plays {
            let entry = lua.create_table()?;
            entry.set("Name", play.name)?;
            entry.set("Intensity", play.target_intensity)?;
            entry.set("Weight", play.weight)?;
            entry.set("Priority", play.priority)?;
            entry.set("Asymmetry", play.asymmetry)?;
            list.push(entry)?;
        }
        Ok(list)
    })?;
    let set_override = lua.create_function(
        |lua, (this, control, value): (LuaInstance, String, Option<f64>)| {
            let Some(index) = face_control(&control) else {
                return Err(mlua::Error::runtime(format!(
                    "SetOverride: `{control}` isn't a face control (they're the FaceControls' number properties, such as \"Jaw_Open\")"
                )));
            };
            let mut world = world_mut(lua)?;
            let state = world.face_mut(this.0).ok_or_else(destroyed)?;
            match value {
                Some(value) => {
                    let value = number_in(Some(value), 0.0, 0.0..=1.0, "value", "SetOverride")?;
                    state.overrides.insert(index, value);
                }
                None => {
                    state.overrides.remove(&index);
                }
            }
            Ok(())
        },
    )?;
    let clear_overrides = lua.create_function(|lua, this: LuaInstance| {
        let mut world = world_mut(lua)?;
        world
            .face_mut(this.0)
            .ok_or_else(destroyed)?
            .overrides
            .clear();
        Ok(())
    })?;
    let set_layer = lua.create_function(
        |lua,
         (this, layer, weight, blend, mask): (
            LuaInstance,
            String,
            Option<f64>,
            Option<String>,
            mlua::Value,
        )| {
            let Some(layer) = FaceLayer::from_name(&layer) else {
                return Err(mlua::Error::runtime(format!(
                    "SetLayer: `{layer}` isn't a face layer (Base, Expression, Dialogue, Procedural or Override)"
                )));
            };
            let weight = number_in(weight, 1.0, 0.0..=1.0, "weight", "SetLayer")?;
            let blend = match blend.as_deref().map(str::trim) {
                None => FaceBlend::Override,
                Some(text) if text.eq_ignore_ascii_case("Override") => FaceBlend::Override,
                Some(text) if text.eq_ignore_ascii_case("Additive") => FaceBlend::Additive,
                Some(other) => {
                    return Err(mlua::Error::runtime(format!(
                        "SetLayer's blend is \"Override\" or \"Additive\", not `{other}`"
                    )));
                }
            };
            let mask = mask_of(lua, mask)?;
            let mut world = world_mut(lua)?;
            let state = world.face_mut(this.0).ok_or_else(destroyed)?;
            let setting = FaceLayerSetting {
                weight,
                blend,
                mask,
            };
            if setting == FaceLayerSetting::default() {
                state.layers.remove(&layer);
            } else {
                state.layers.insert(layer, setting);
            }
            Ok(())
        },
    )?;
    Ok(vec![
        (ClassName::FaceControls, "SetExpression", set_expression),
        (ClassName::FaceControls, "ClearExpression", clear_expression),
        (ClassName::FaceControls, "GetExpressions", get_expressions),
        (ClassName::FaceControls, "SetOverride", set_override),
        (ClassName::FaceControls, "ClearOverrides", clear_overrides),
        (ClassName::FaceControls, "SetLayer", set_layer),
    ])
}
