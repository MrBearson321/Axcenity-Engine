//! `NumberSequence`, `ColorSequence` and their keypoints for scripts
//! (roadmap R17.1), as a ParticleEmitter's `SizeOverLife`,
//! `TransparencyOverLife` and `ColorOverLife` take them. A sequence
//! property that's unset (empty) reads as nil, and setting nil unsets it.

use axcenity_core::{
    Color3, ColorKeypoint, ColorSequence, NumberKeypoint, NumberSequence, SequenceError,
};
use mlua::{FromLua, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::types::{LuaColor3, f32_to_lua, type_name};

#[derive(Clone, Copy, Debug)]
pub struct LuaNumberKeypoint(pub NumberKeypoint);

#[derive(Clone, Copy, Debug)]
pub struct LuaColorKeypoint(pub ColorKeypoint);

#[derive(Clone, Debug)]
pub struct LuaNumberSequence(pub NumberSequence);

#[derive(Clone, Debug)]
pub struct LuaColorSequence(pub ColorSequence);

impl UserData for LuaNumberKeypoint {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Time", |_, k| Ok(f32_to_lua(k.0.time)));
        fields.add_field_method_get("Value", |_, k| Ok(f32_to_lua(k.0.value)));
        fields.add_field_method_get("Envelope", |_, k| Ok(f32_to_lua(k.0.envelope)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, k, other: mlua::Value| {
            Ok(LuaNumberKeypoint::from_lua(other, lua).is_ok_and(|other| other.0 == k.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, k, ()| {
            Ok(format!("{} {} {}", k.0.time, k.0.value, k.0.envelope))
        });
    }
}

impl UserData for LuaColorKeypoint {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Time", |_, k| Ok(f32_to_lua(k.0.time)));
        fields.add_field_method_get("Value", |_, k| Ok(LuaColor3(k.0.color)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, k, other: mlua::Value| {
            Ok(LuaColorKeypoint::from_lua(other, lua).is_ok_and(|other| other.0 == k.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, k, ()| {
            let c = k.0.color;
            Ok(format!("{} {} {} {}", k.0.time, c.r, c.g, c.b))
        });
    }
}

impl UserData for LuaNumberSequence {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Keypoints", |lua, s| {
            lua.create_sequence_from(s.0.keypoints().iter().copied().map(LuaNumberKeypoint))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, s, other: mlua::Value| {
            Ok(LuaNumberSequence::from_lua(other, lua).is_ok_and(|other| other.0 == s.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, s, ()| {
            let points: Vec<String> =
                s.0.keypoints()
                    .iter()
                    .map(|k| format!("{} {} {}", k.time, k.value, k.envelope))
                    .collect();
            Ok(points.join(" "))
        });
    }
}

impl UserData for LuaColorSequence {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Keypoints", |lua, s| {
            lua.create_sequence_from(s.0.keypoints().iter().copied().map(LuaColorKeypoint))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, s, other: mlua::Value| {
            Ok(LuaColorSequence::from_lua(other, lua).is_ok_and(|other| other.0 == s.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, s, ()| {
            let points: Vec<String> =
                s.0.keypoints()
                    .iter()
                    .map(|k| format!("{} {} {} {}", k.time, k.color.r, k.color.g, k.color.b))
                    .collect();
            Ok(points.join(" "))
        });
    }
}

macro_rules! from_lua_userdata {
    ($($ty:ty => $name:literal),* $(,)?) => {$(
        impl FromLua for $ty {
            fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
                match &value {
                    mlua::Value::UserData(data) => Ok(data.borrow::<Self>()?.clone()),
                    _ => Err(mlua::Error::FromLuaConversionError {
                        from: value.type_name(),
                        to: $name.to_owned(),
                        message: None,
                    }),
                }
            }
        }
    )*};
}

from_lua_userdata! {
    LuaNumberKeypoint => "NumberSequenceKeypoint",
    LuaColorKeypoint => "ColorSequenceKeypoint",
    LuaNumberSequence => "NumberSequence",
    LuaColorSequence => "ColorSequence",
}

fn number(value: &mlua::Value) -> Option<f32> {
    match value {
        mlua::Value::Number(n) => Some(*n as f32),
        mlua::Value::Integer(n) => Some(*n as f32),
        _ => None,
    }
}

fn sequence_error(call: &str, error: SequenceError) -> mlua::Error {
    mlua::Error::runtime(format!("{call}: {error}"))
}

/// The `NumberSequenceKeypoint` global: `new(time, value, envelope?)`.
pub fn number_keypoint_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|_, (time, value, envelope): (f32, f32, Option<f32>)| {
            Ok(LuaNumberKeypoint(NumberKeypoint::new(
                time,
                value,
                envelope.unwrap_or(0.0).max(0.0),
            )))
        })?,
    )?;
    Ok(library)
}

/// The `ColorSequenceKeypoint` global: `new(time, color)`.
pub fn color_keypoint_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, (time, color): (f32, mlua::Value)| {
            let color = LuaColor3::from_lua(color.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "ColorSequenceKeypoint.new(time, color) needs a Color3, not a {}",
                    type_name(lua, &color)
                ))
            })?;
            Ok(LuaColorKeypoint(ColorKeypoint::new(time, color.0)))
        })?,
    )?;
    Ok(library)
}

/// The `NumberSequence` global: `new(n)`, `new(from, to)`, or
/// `new({ keypoints })`, from time 0 to 1 in order.
pub fn number_sequence_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, args: mlua::MultiValue| {
            const CALL: &str = "NumberSequence.new";
            let args: Vec<mlua::Value> = args.into_iter().collect();
            let sequence = match args.as_slice() {
                [only] if number(only).is_some() => {
                    NumberSequence::constant(number(only).expect("checked"))
                }
                [from, to] if number(from).is_some() && number(to).is_some() => {
                    NumberSequence::ramp(number(from).expect("checked"), number(to).expect("checked"))
                }
                [mlua::Value::Table(table)] => {
                    let keypoints = table
                        .sequence_values::<mlua::Value>()
                        .map(|value| {
                            LuaNumberKeypoint::from_lua(value?, lua).map(|keypoint| keypoint.0)
                        })
                        .collect::<mlua::Result<Vec<_>>>()
                        .map_err(|_| {
                            mlua::Error::runtime(
                                "NumberSequence.new({ ... }) needs a list of NumberSequenceKeypoints",
                            )
                        })?;
                    NumberSequence::new(keypoints).map_err(|error| sequence_error(CALL, error))?
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "NumberSequence.new takes a number, two numbers (start and end), or a list of NumberSequenceKeypoints",
                    ));
                }
            };
            Ok(LuaNumberSequence(sequence))
        })?,
    )?;
    Ok(library)
}

/// The `ColorSequence` global: `new(color)`, `new(from, to)`, or
/// `new({ keypoints })`, from time 0 to 1 in order.
pub fn color_sequence_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, args: mlua::MultiValue| {
            const CALL: &str = "ColorSequence.new";
            let args: Vec<mlua::Value> = args.into_iter().collect();
            let color = |value: &mlua::Value| -> Option<Color3> {
                LuaColor3::from_lua(value.clone(), lua).ok().map(|c| c.0)
            };
            let sequence = match args.as_slice() {
                [only] if color(only).is_some() => {
                    let only = color(only).expect("checked");
                    ColorSequence::ramp(only, only)
                }
                [from, to] if color(from).is_some() && color(to).is_some() => {
                    ColorSequence::ramp(color(from).expect("checked"), color(to).expect("checked"))
                }
                [mlua::Value::Table(table)] => {
                    let keypoints = table
                        .sequence_values::<mlua::Value>()
                        .map(|value| {
                            LuaColorKeypoint::from_lua(value?, lua).map(|keypoint| keypoint.0)
                        })
                        .collect::<mlua::Result<Vec<_>>>()
                        .map_err(|_| {
                            mlua::Error::runtime(
                                "ColorSequence.new({ ... }) needs a list of ColorSequenceKeypoints",
                            )
                        })?;
                    ColorSequence::new(keypoints).map_err(|error| sequence_error(CALL, error))?
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "ColorSequence.new takes a Color3, two Color3s (start and end), or a list of ColorSequenceKeypoints",
                    ));
                }
            };
            Ok(LuaColorSequence(sequence))
        })?,
    )?;
    Ok(library)
}
