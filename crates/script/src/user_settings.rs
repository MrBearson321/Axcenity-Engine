//! `game:GetService("UserGameSettings")`: the player's graphics settings,
//! for LocalScripts (such as a settings menu). They belong to the player's
//! computer, not to the game: the player saves them in its `graphics.json`,
//! and the editor's Play starts from the editor's own Graphics settings.
//! `MasterVolume` (Phase 52) is the exception: it's the world's
//! ([`World::master_volume`](axcenity_core::World::master_volume)), kept
//! in save slots.
//!
//! `GraphicsQuality` picks a preset, and each other property overrides one
//! of the preset's values, as in the editor's Settings window. Reading one
//! gives the value chosen (its override, or else the preset's), and setting
//! one to nil removes its override. The host hands the settings in with
//! [`ScriptRuntime::set_graphics`](crate::ScriptRuntime::set_graphics) and
//! takes a script's change with `take_graphics_change`.

use axcenity_render::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DepthOfFieldQuality,
    GraphicsSettings, MotionBlurQuality, Quality, RENDER_SCALE_RANGE, ReflectionQuality,
    ShadowQuality, TextureQuality,
};
use mlua::{FromLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::instance::{world, world_mut};
use crate::types::{EnumType, LuaEnumItem, type_name};

/// The graphics settings LocalScripts see and change, kept in the client
/// interpreter's app data.
#[derive(Default)]
pub struct UserGraphics {
    pub settings: GraphicsSettings,
    /// What the GPU can do, which picks Automatic's quality.
    pub capabilities: Capabilities,
    /// True once a script has changed the settings since the host last
    /// took them.
    pub changed: bool,
}

/// `Enum.GraphicsQuality`'s first item: the quality that suits the machine.
const AUTOMATIC: &str = "Automatic";

/// `Enum.GraphicsQuality`'s items: Automatic, then the presets.
pub(crate) fn quality_names() -> Vec<&'static str> {
    std::iter::once(AUTOMATIC)
        .chain(Quality::ALL.map(Quality::label))
        .collect()
}

pub(crate) fn shadow_quality_names() -> Vec<&'static str> {
    ShadowQuality::ALL.map(ShadowQuality::label).to_vec()
}

pub(crate) fn texture_quality_names() -> Vec<&'static str> {
    TextureQuality::ALL.map(TextureQuality::label).to_vec()
}

pub(crate) fn anti_aliasing_names() -> Vec<&'static str> {
    AntiAliasing::ALL.map(anti_aliasing_name).to_vec()
}

pub(crate) fn ambient_occlusion_names() -> Vec<&'static str> {
    AmbientOcclusion::ALL.map(AmbientOcclusion::label).to_vec()
}

pub(crate) fn reflection_quality_names() -> Vec<&'static str> {
    ReflectionQuality::ALL
        .map(ReflectionQuality::label)
        .to_vec()
}

pub(crate) fn cloud_quality_names() -> Vec<&'static str> {
    CloudQuality::ALL.map(CloudQuality::label).to_vec()
}

pub(crate) fn depth_of_field_quality_names() -> Vec<&'static str> {
    DepthOfFieldQuality::ALL
        .map(DepthOfFieldQuality::label)
        .to_vec()
}

pub(crate) fn motion_blur_quality_names() -> Vec<&'static str> {
    MotionBlurQuality::ALL
        .map(MotionBlurQuality::label)
        .to_vec()
}

/// `mode`'s item name in `Enum.AntiAliasing`. Its label ("MSAA 4×") isn't a
/// Lua name.
fn anti_aliasing_name(mode: AntiAliasing) -> &'static str {
    match mode {
        AntiAliasing::Off => "Off",
        AntiAliasing::Fxaa => "FXAA",
        AntiAliasing::Msaa2 => "MSAA2",
        AntiAliasing::Msaa4 => "MSAA4",
        AntiAliasing::Taa => "TAA",
    }
}

/// `UserGameSettings`, as LocalScripts see it. Holds nothing itself: the
/// settings are the [`UserGraphics`] in app data.
#[derive(Clone, Copy)]
pub struct LuaUserGameSettings;

fn settings(lua: &Lua) -> GraphicsSettings {
    lua.app_data_ref::<UserGraphics>()
        .map(|graphics| graphics.settings.clone())
        .unwrap_or_default()
}

/// Changes the settings with `apply`, and marks them changed.
fn change(lua: &Lua, apply: impl FnOnce(&mut GraphicsSettings, &Capabilities)) {
    if let Some(mut graphics) = lua.app_data_mut::<UserGraphics>() {
        let graphics = &mut *graphics;
        apply(&mut graphics.settings, &graphics.capabilities);
        graphics.changed = true;
    }
}

/// The item of `enum_type` an enum property reads as.
fn item(enum_type: EnumType, name: &'static str) -> LuaEnumItem {
    LuaEnumItem { enum_type, name }
}

/// What a script set an enum override to: one of `all` (by its item name
/// in `enum_type`), or `None` for nil, which goes back to the preset's.
fn enum_choice<T: Copy>(
    lua: &Lua,
    property: &str,
    enum_type: EnumType,
    all: &[T],
    name_of: fn(T) -> &'static str,
    value: mlua::Value,
) -> mlua::Result<Option<T>> {
    if value.is_nil() {
        return Ok(None);
    }
    let chosen = LuaEnumItem::from_lua(value.clone(), lua)
        .ok()
        .filter(|item| item.enum_type == enum_type)
        .and_then(|item| all.iter().copied().find(|&each| name_of(each) == item.name));
    chosen.map(Some).ok_or_else(|| {
        let enum_name = enum_type.name();
        mlua::Error::runtime(format!(
            "UserGameSettings.{property} must be an Enum.{enum_name}, such as Enum.{enum_name}.{}, \
             or nil for the preset's, not a {}",
            name_of(all[all.len() - 1]),
            type_name(lua, &value)
        ))
    })
}

/// What a script set a number override to, or `None` for nil.
fn number_choice(lua: &Lua, property: &str, value: mlua::Value) -> mlua::Result<Option<f32>> {
    let number = match value {
        mlua::Value::Nil => return Ok(None),
        mlua::Value::Integer(n) => n as f64,
        mlua::Value::Number(n) => n,
        ref other => {
            return Err(mlua::Error::runtime(format!(
                "UserGameSettings.{property} must be a number, or nil for the preset's, not a {}",
                type_name(lua, other)
            )));
        }
    };
    if !number.is_finite() {
        return Err(mlua::Error::runtime(format!(
            "UserGameSettings.{property} must be a finite number, not {number}"
        )));
    }
    Ok(Some(number as f32))
}

/// What a script set an on-or-off override to, or `None` for nil.
fn bool_choice(lua: &Lua, property: &str, value: mlua::Value) -> mlua::Result<Option<bool>> {
    match value {
        mlua::Value::Nil => Ok(None),
        mlua::Value::Boolean(on) => Ok(Some(on)),
        ref other => Err(mlua::Error::runtime(format!(
            "UserGameSettings.{property} must be true, false, or nil for the preset's, not a {}",
            type_name(lua, other)
        ))),
    }
}

impl UserData for LuaUserGameSettings {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("GraphicsQuality", |lua, _| {
            let settings = settings(lua);
            let name = if settings.automatic {
                AUTOMATIC
            } else {
                settings.quality.label()
            };
            Ok(item(EnumType::GraphicsQuality, name))
        });
        fields.add_field_method_set("GraphicsQuality", |lua, _, value: mlua::Value| {
            let chosen = LuaEnumItem::from_lua(value.clone(), lua)
                .ok()
                .filter(|item| item.enum_type == EnumType::GraphicsQuality);
            let Some(chosen) = chosen else {
                return Err(mlua::Error::runtime(format!(
                    "UserGameSettings.GraphicsQuality must be an Enum.GraphicsQuality, such as \
                     Enum.GraphicsQuality.High, not a {}",
                    type_name(lua, &value)
                )));
            };
            // The overrides stay, as when the preset changes in the editor.
            change(lua, |settings, capabilities| {
                let quality = Quality::ALL
                    .into_iter()
                    .find(|quality| quality.label() == chosen.name);
                (settings.quality, settings.automatic) = match quality {
                    Some(quality) => (quality, false),
                    None => (GraphicsSettings::automatic(capabilities).quality, true),
                };
            });
            Ok(())
        });

        fields.add_field_method_get("ShadowQuality", |lua, _| {
            let name = settings(lua).shadow_quality().label();
            Ok(item(EnumType::ShadowQuality, name))
        });
        fields.add_field_method_set("ShadowQuality", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "ShadowQuality",
                EnumType::ShadowQuality,
                &ShadowQuality::ALL,
                ShadowQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.shadow_quality = chosen);
            Ok(())
        });

        fields.add_field_method_get("ShadowDistance", |lua, _| {
            Ok(f64::from(settings(lua).shadow_distance()))
        });
        fields.add_field_method_set("ShadowDistance", |lua, _, value: mlua::Value| {
            let chosen = number_choice(lua, "ShadowDistance", value)?;
            change(lua, |settings, _| {
                settings.shadow_distance = chosen.map(|distance| distance.max(0.0));
            });
            Ok(())
        });

        fields.add_field_method_get("PostProcessing", |lua, _| {
            Ok(settings(lua).post_processing())
        });
        fields.add_field_method_set("PostProcessing", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "PostProcessing", value)?;
            change(lua, |settings, _| settings.post_processing = chosen);
            Ok(())
        });

        fields.add_field_method_get("RenderScale", |lua, _| {
            Ok(f64::from(settings(lua).render_scale()))
        });
        fields.add_field_method_set("RenderScale", |lua, _, value: mlua::Value| {
            let chosen = number_choice(lua, "RenderScale", value)?;
            change(lua, |settings, _| {
                settings.render_scale = chosen.map(|scale| {
                    scale.clamp(*RENDER_SCALE_RANGE.start(), *RENDER_SCALE_RANGE.end())
                });
            });
            Ok(())
        });

        fields.add_field_method_get("TextureQuality", |lua, _| {
            let name = settings(lua).texture_quality().label();
            Ok(item(EnumType::TextureQuality, name))
        });
        fields.add_field_method_set("TextureQuality", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "TextureQuality",
                EnumType::TextureQuality,
                &TextureQuality::ALL,
                TextureQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.texture_quality = chosen);
            Ok(())
        });

        fields.add_field_method_get("AntiAliasing", |lua, _| {
            let name = anti_aliasing_name(settings(lua).anti_aliasing());
            Ok(item(EnumType::AntiAliasing, name))
        });
        fields.add_field_method_set("AntiAliasing", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "AntiAliasing",
                EnumType::AntiAliasing,
                &AntiAliasing::ALL,
                anti_aliasing_name,
                value,
            )?;
            change(lua, |settings, _| settings.anti_aliasing = chosen);
            Ok(())
        });

        fields.add_field_method_get("AmbientOcclusion", |lua, _| {
            let name = settings(lua).ambient_occlusion().label();
            Ok(item(EnumType::AmbientOcclusion, name))
        });
        fields.add_field_method_set("AmbientOcclusion", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "AmbientOcclusion",
                EnumType::AmbientOcclusion,
                &AmbientOcclusion::ALL,
                AmbientOcclusion::label,
                value,
            )?;
            change(lua, |settings, _| settings.ambient_occlusion = chosen);
            Ok(())
        });

        fields.add_field_method_get("ReflectionQuality", |lua, _| {
            let name = settings(lua).reflection_quality().label();
            Ok(item(EnumType::ReflectionQuality, name))
        });
        fields.add_field_method_set("ReflectionQuality", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "ReflectionQuality",
                EnumType::ReflectionQuality,
                &ReflectionQuality::ALL,
                ReflectionQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.reflection_quality = chosen);
            Ok(())
        });

        fields.add_field_method_get("CloudQuality", |lua, _| {
            let name = settings(lua).cloud_quality().label();
            Ok(item(EnumType::CloudQuality, name))
        });
        fields.add_field_method_set("CloudQuality", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "CloudQuality",
                EnumType::CloudQuality,
                &CloudQuality::ALL,
                CloudQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.cloud_quality = chosen);
            Ok(())
        });

        fields.add_field_method_get("ContactShadows", |lua, _| {
            Ok(settings(lua).contact_shadows())
        });
        fields.add_field_method_set("ContactShadows", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "ContactShadows", value)?;
            change(lua, |settings, _| settings.contact_shadows = chosen);
            Ok(())
        });

        fields.add_field_method_get("RealtimeGI", |lua, _| Ok(settings(lua).realtime_gi()));
        fields.add_field_method_set("RealtimeGI", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "RealtimeGI", value)?;
            change(lua, |settings, _| settings.realtime_gi = chosen);
            Ok(())
        });

        fields.add_field_method_get("VolumetricFog", |lua, _| Ok(settings(lua).volumetric_fog()));
        fields.add_field_method_set("VolumetricFog", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "VolumetricFog", value)?;
            change(lua, |settings, _| settings.volumetric_fog = chosen);
            Ok(())
        });

        fields.add_field_method_get("DepthOfField", |lua, _| {
            let name = settings(lua).depth_of_field().label();
            Ok(item(EnumType::DepthOfField, name))
        });
        fields.add_field_method_set("DepthOfField", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "DepthOfField",
                EnumType::DepthOfField,
                &DepthOfFieldQuality::ALL,
                DepthOfFieldQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.depth_of_field = chosen);
            Ok(())
        });

        fields.add_field_method_get("Vignette", |lua, _| Ok(settings(lua).vignette()));
        fields.add_field_method_set("Vignette", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "Vignette", value)?;
            change(lua, |settings, _| settings.vignette = chosen);
            Ok(())
        });

        fields.add_field_method_get("FilmGrain", |lua, _| Ok(settings(lua).film_grain()));
        fields.add_field_method_set("FilmGrain", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "FilmGrain", value)?;
            change(lua, |settings, _| settings.film_grain = chosen);
            Ok(())
        });

        fields.add_field_method_get("ChromaticAberration", |lua, _| {
            Ok(settings(lua).chromatic_aberration())
        });
        fields.add_field_method_set("ChromaticAberration", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "ChromaticAberration", value)?;
            change(lua, |settings, _| settings.chromatic_aberration = chosen);
            Ok(())
        });

        fields.add_field_method_get("Sharpen", |lua, _| Ok(settings(lua).sharpen()));
        fields.add_field_method_set("Sharpen", |lua, _, value: mlua::Value| {
            let chosen = bool_choice(lua, "Sharpen", value)?;
            change(lua, |settings, _| settings.sharpen = chosen);
            Ok(())
        });

        fields.add_field_method_get("MotionBlur", |lua, _| {
            let name = settings(lua).motion_blur().label();
            Ok(item(EnumType::MotionBlur, name))
        });
        fields.add_field_method_set("MotionBlur", |lua, _, value: mlua::Value| {
            let chosen = enum_choice(
                lua,
                "MotionBlur",
                EnumType::MotionBlur,
                &MotionBlurQuality::ALL,
                MotionBlurQuality::label,
                value,
            )?;
            change(lua, |settings, _| settings.motion_blur = chosen);
            Ok(())
        });

        fields.add_field_method_get("MasterVolume", |lua, _| Ok(world(lua)?.master_volume()));
        fields.add_field_method_set("MasterVolume", |lua, _, value: mlua::Value| {
            let volume = match value {
                mlua::Value::Integer(n) => n as f64,
                mlua::Value::Number(n) if n.is_finite() => n,
                ref other => {
                    return Err(mlua::Error::runtime(format!(
                        "UserGameSettings.MasterVolume must be a number from 0 to 1, not {}",
                        type_name(lua, other)
                    )));
                }
            };
            world_mut(lua)?.set_master_volume(volume);
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("UserGameSettings"));
    }
}
