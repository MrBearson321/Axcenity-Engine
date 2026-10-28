//! Lua versions of the property value types (`Vector3`, `Vector2`, `Color3`, enum
//! items), and conversion between Lua values and core [`Value`]s.
//!
//! Rust's "orphan rule" only allows implementing a trait from another crate
//! (here mlua's `UserData`) for a type from *this* crate. So each core type
//! is wrapped in a small struct, such as `LuaVector3(Vector3)`.

use axcenity_anim::EasingDirection;
use axcenity_core::{
    Color3, KeyCode, Material, MouseBehavior, PartType, ParticleType, TextureChannel, ToneMapping,
    UDim, UDim2, UserInputState, UserInputType, Value, ValueKind, Vector2, Vector3,
};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::camera::CameraType;
use crate::cframe::LuaCFrame;
use crate::instance::LuaInstance;
use crate::tween::{PlaybackState, TweenStyle};
use crate::user_settings;

/// Lua stores numbers as `f64`, and core stores components as `f32`. A plain
/// `as f64` turns `0.1f32` into `0.10000000149011612`, so this goes through
/// the shortest decimal text instead, and scripts see `0.1`.
pub fn f32_to_lua(x: f32) -> f64 {
    x.to_string().parse().unwrap_or(f64::from(x))
}

/// Formats a component for `tostring`. Adding 0.0 turns -0 into 0.
fn component_text(x: f32) -> String {
    (x + 0.0).to_string()
}

#[derive(Clone, Copy, Debug)]
pub struct LuaVector3(pub Vector3);

impl LuaVector3 {
    fn map(self, f: impl Fn(f32) -> f32) -> Self {
        let v = self.0;
        Self(Vector3::new(f(v.x), f(v.y), f(v.z)))
    }

    fn zip(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self {
        let (a, b) = (self.0, other.0);
        Self(Vector3::new(f(a.x, b.x), f(a.y, b.y), f(a.z, b.z)))
    }

    fn magnitude(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn dot(self, other: Self) -> f32 {
        let (a, b) = (self.0, other.0);
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    fn cross(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        Self(Vector3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        ))
    }
}

/// The angle between two vectors in radians, from their dot product and
/// the length of their cross product (steadier than `acos` near 0 and 180°).
fn angle_between(dot: f32, cross_length: f32) -> f32 {
    cross_length.atan2(dot)
}

/// Each of `others` folded into `first` with `f`, for `Min` and `Max`.
fn fold<T: Copy>(first: T, others: mlua::Variadic<T>, f: impl Fn(T, T) -> T) -> T {
    others.into_iter().fold(first, f)
}

/// What `*` and `/` accept on either side: a Vector3 or a number.
enum Operand {
    Vector(LuaVector3),
    Number(f32),
}

impl FromLua for Operand {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::Integer(n) => Ok(Operand::Number(n as f32)),
            mlua::Value::Number(n) => Ok(Operand::Number(n as f32)),
            other => LuaVector3::from_lua(other, lua).map(Operand::Vector),
        }
    }
}

impl UserData for LuaVector3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, v| Ok(f32_to_lua(v.0.x)));
        fields.add_field_method_get("Y", |_, v| Ok(f32_to_lua(v.0.y)));
        fields.add_field_method_get("Z", |_, v| Ok(f32_to_lua(v.0.z)));
        fields.add_field_method_get("Magnitude", |_, v| Ok(f32_to_lua(v.magnitude())));
        // The same direction with length 1. The zero vector has no direction, so it stays zero.
        fields.add_field_method_get("Unit", |_, v| {
            let length = v.magnitude();
            Ok(if length == 0.0 {
                *v
            } else {
                v.map(|c| c / length)
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // `add_meta_function` receives both operands, because Lua calls the
        // metamethod for `2 * v` as well as for `v * 2`.
        methods.add_meta_function(MetaMethod::Add, |_, (a, b): (LuaVector3, LuaVector3)| {
            Ok(a.zip(b, |a, b| a + b))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (a, b): (LuaVector3, LuaVector3)| {
            Ok(a.zip(b, |a, b| a - b))
        });
        methods.add_meta_function(MetaMethod::Mul, |_, (a, b): (Operand, Operand)| {
            match (a, b) {
                (Operand::Vector(a), Operand::Vector(b)) => Ok(a.zip(b, |a, b| a * b)),
                (Operand::Vector(v), Operand::Number(n))
                | (Operand::Number(n), Operand::Vector(v)) => Ok(v.map(|c| c * n)),
                (Operand::Number(_), Operand::Number(_)) => {
                    unreachable!("Lua multiplies numbers itself")
                }
            }
        });
        methods.add_meta_function(MetaMethod::Div, |_, (a, b): (Operand, Operand)| {
            match (a, b) {
                (Operand::Vector(a), Operand::Vector(b)) => Ok(a.zip(b, |a, b| a / b)),
                (Operand::Vector(v), Operand::Number(n)) => Ok(v.map(|c| c / n)),
                (Operand::Number(n), Operand::Vector(v)) => Ok(v.map(|c| n / c)),
                (Operand::Number(_), Operand::Number(_)) => {
                    unreachable!("Lua divides numbers itself")
                }
            }
        });
        methods.add_meta_method(MetaMethod::Unm, |_, v, ()| Ok(v.map(|c| -c)));
        methods.add_method("Dot", |_, v, other: LuaVector3| {
            Ok(f32_to_lua(v.dot(other)))
        });
        methods.add_method("Cross", |_, v, other: LuaVector3| Ok(v.cross(other)));
        methods.add_method("Lerp", |_, v, (goal, alpha): (LuaVector3, f64)| {
            let alpha = alpha as f32;
            Ok(v.zip(goal, |a, b| a + (b - a) * alpha))
        });
        // With an axis, the angle is negative when turning from `v` to
        // `other` goes clockwise looking down the axis.
        methods.add_method(
            "Angle",
            |_, v, (other, axis): (LuaVector3, Option<LuaVector3>)| {
                let cross = v.cross(other);
                let angle = angle_between(v.dot(other), cross.magnitude());
                let sign = match axis {
                    Some(axis) if cross.dot(axis) < 0.0 => -1.0,
                    _ => 1.0,
                };
                Ok(f32_to_lua(angle * sign))
            },
        );
        methods.add_method(
            "FuzzyEq",
            |_, v, (other, epsilon): (LuaVector3, Option<f64>)| {
                let epsilon = epsilon.unwrap_or(1e-5) as f32;
                let (a, b) = (v.0, other.0);
                Ok([a.x - b.x, a.y - b.y, a.z - b.z]
                    .iter()
                    .all(|d| d.abs() <= epsilon))
            },
        );
        methods.add_method("Min", |_, v, others: mlua::Variadic<LuaVector3>| {
            Ok(fold(*v, others, |a, b| a.zip(b, f32::min)))
        });
        methods.add_method("Max", |_, v, others: mlua::Variadic<LuaVector3>| {
            Ok(fold(*v, others, |a, b| a.zip(b, f32::max)))
        });
        methods.add_method("Abs", |_, v, ()| Ok(v.map(f32::abs)));
        methods.add_method("Floor", |_, v, ()| Ok(v.map(f32::floor)));
        methods.add_method("Ceil", |_, v, ()| Ok(v.map(f32::ceil)));
        methods.add_meta_method(MetaMethod::Eq, |lua, v, other: mlua::Value| {
            Ok(LuaVector3::from_lua(other, lua).is_ok_and(|other| other.0 == v.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, v, ()| {
            let v = v.0;
            Ok(format!(
                "{}, {}, {}",
                component_text(v.x),
                component_text(v.y),
                component_text(v.z)
            ))
        });
    }
}

/// A point or size on the screen in pixels, such as a ScrollingFrame's
/// `CanvasPosition`.
#[derive(Clone, Copy, Debug)]
pub struct LuaVector2(pub Vector2);

impl LuaVector2 {
    fn zip(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self {
        Self(Vector2::new(f(self.0.x, other.0.x), f(self.0.y, other.0.y)))
    }

    fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self(Vector2::new(f(self.0.x), f(self.0.y)))
    }

    fn dot(self, other: Self) -> f32 {
        self.0.x * other.0.x + self.0.y * other.0.y
    }

    /// The Z of the 3D cross product: positive when `other` is clockwise
    /// from `self` on the screen (Y points down).
    fn cross(self, other: Self) -> f32 {
        self.0.x * other.0.y - self.0.y * other.0.x
    }
}

/// A Lua number, if `value` is one.
fn number(value: &mlua::Value) -> Option<f32> {
    match value {
        mlua::Value::Integer(n) => Some(*n as f32),
        mlua::Value::Number(n) => Some(*n as f32),
        _ => None,
    }
}

/// A Vector2, if `value` is one.
fn vector2(value: &mlua::Value) -> Option<LuaVector2> {
    match value {
        mlua::Value::UserData(data) => data.borrow::<LuaVector2>().ok().map(|v| *v),
        _ => None,
    }
}

impl UserData for LuaVector2 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, v| Ok(f32_to_lua(v.0.x)));
        fields.add_field_method_get("Y", |_, v| Ok(f32_to_lua(v.0.y)));
        fields.add_field_method_get("Magnitude", |_, v| Ok(f32_to_lua(v.dot(*v).sqrt())));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Add, |_, (a, b): (LuaVector2, LuaVector2)| {
            Ok(a.zip(b, |a, b| a + b))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (a, b): (LuaVector2, LuaVector2)| {
            Ok(a.zip(b, |a, b| a - b))
        });
        methods.add_meta_function(MetaMethod::Mul, |_, (a, b): (mlua::Value, mlua::Value)| {
            match (number(&a), number(&b)) {
                (Some(n), None) => vector2(&b).map(|v| v.map(|c| c * n)),
                (None, Some(n)) => vector2(&a).map(|v| v.map(|c| c * n)),
                _ => match (vector2(&a), vector2(&b)) {
                    (Some(a), Some(b)) => Some(a.zip(b, |a, b| a * b)),
                    _ => None,
                },
            }
            .ok_or_else(|| mlua::Error::runtime("a Vector2 multiplies by a Vector2 or a number"))
        });
        methods.add_meta_function(MetaMethod::Div, |_, (a, b): (mlua::Value, mlua::Value)| {
            match (number(&a), number(&b)) {
                (Some(n), None) => vector2(&b).map(|v| v.map(|c| n / c)),
                (None, Some(n)) => vector2(&a).map(|v| v.map(|c| c / n)),
                _ => match (vector2(&a), vector2(&b)) {
                    (Some(a), Some(b)) => Some(a.zip(b, |a, b| a / b)),
                    _ => None,
                },
            }
            .ok_or_else(|| mlua::Error::runtime("a Vector2 divides by a Vector2 or a number"))
        });
        methods.add_meta_method(MetaMethod::Unm, |_, v, ()| Ok(v.map(|c| -c)));
        methods.add_method("Dot", |_, v, other: LuaVector2| {
            Ok(f32_to_lua(v.dot(other)))
        });
        methods.add_method("Lerp", |_, v, (goal, alpha): (LuaVector2, f64)| {
            let alpha = alpha as f32;
            Ok(v.zip(goal, |a, b| a + (b - a) * alpha))
        });
        // Signed, the angle is negative when `other` is anticlockwise from
        // `v` on the screen.
        methods.add_method(
            "Angle",
            |_, v, (other, signed): (LuaVector2, Option<bool>)| {
                let cross = v.cross(other);
                let angle = angle_between(v.dot(other), cross.abs());
                Ok(f32_to_lua(if signed == Some(true) && cross < 0.0 {
                    -angle
                } else {
                    angle
                }))
            },
        );
        methods.add_method(
            "FuzzyEq",
            |_, v, (other, epsilon): (LuaVector2, Option<f64>)| {
                let epsilon = epsilon.unwrap_or(1e-5) as f32;
                Ok((v.0.x - other.0.x).abs() <= epsilon && (v.0.y - other.0.y).abs() <= epsilon)
            },
        );
        methods.add_method("Min", |_, v, others: mlua::Variadic<LuaVector2>| {
            Ok(fold(*v, others, |a, b| a.zip(b, f32::min)))
        });
        methods.add_method("Max", |_, v, others: mlua::Variadic<LuaVector2>| {
            Ok(fold(*v, others, |a, b| a.zip(b, f32::max)))
        });
        methods.add_method("Abs", |_, v, ()| Ok(v.map(f32::abs)));
        methods.add_method("Floor", |_, v, ()| Ok(v.map(f32::floor)));
        methods.add_method("Ceil", |_, v, ()| Ok(v.map(f32::ceil)));
        methods.add_meta_method(MetaMethod::Eq, |lua, v, other: mlua::Value| {
            Ok(LuaVector2::from_lua(other, lua).is_ok_and(|other| other.0 == v.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, v, ()| {
            Ok(format!(
                "{}, {}",
                component_text(v.0.x),
                component_text(v.0.y)
            ))
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaColor3(pub Color3);

impl LuaColor3 {
    /// Hue, saturation and value, each from 0 to 1. A grey has hue 0.
    pub fn to_hsv(self) -> (f32, f32, f32) {
        let Color3 { r, g, b } = self.0;
        let max = r.max(g).max(b);
        let range = max - r.min(g).min(b);
        let saturation = if max == 0.0 { 0.0 } else { range / max };
        // Which sixth of the colour wheel, from the largest channel.
        let sixths = if range == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / range).rem_euclid(6.0)
        } else if max == g {
            (b - r) / range + 2.0
        } else {
            (r - g) / range + 4.0
        };
        (sixths / 6.0, saturation, max)
    }

    pub fn from_hsv(hue: f32, saturation: f32, value: f32) -> Self {
        let [hue, saturation, value] = [hue, saturation, value].map(|c| c.clamp(0.0, 1.0));
        // `k` walks round the wheel for each channel; red starts at 5, green
        // at 3 and blue at 1 sixths.
        let channel = |n: f32| {
            let k = (n + hue * 6.0) % 6.0;
            value - value * saturation * k.min(4.0 - k).clamp(0.0, 1.0)
        };
        Self(Color3::new(channel(5.0), channel(3.0), channel(1.0)))
    }

    /// The colour as six hex digits, such as `"ff8000"`.
    pub fn to_hex(self) -> String {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let Color3 { r, g, b } = self.0;
        format!("{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    }

    /// `"#ff8000"`, `"ff8000"`, `"#f80"` or `"f80"`, in either case.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let channels: Vec<u8> = match digits.len() {
            3 => digits
                .chars()
                .map(|c| u8::from_str_radix(&format!("{c}{c}"), 16))
                .collect::<Result<_, _>>()
                .ok()?,
            6 => (0..3)
                .map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16))
                .collect::<Result<_, _>>()
                .ok()?,
            _ => return None,
        };
        let [r, g, b] = [0, 1, 2].map(|i| f32::from(channels[i]) / 255.0);
        Some(Self(Color3::new(r, g, b)))
    }
}

impl UserData for LuaColor3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("R", |_, c| Ok(f32_to_lua(c.0.r)));
        fields.add_field_method_get("G", |_, c| Ok(f32_to_lua(c.0.g)));
        fields.add_field_method_get("B", |_, c| Ok(f32_to_lua(c.0.b)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Colours can't go past their ends, even when `alpha` does.
        methods.add_method("Lerp", |_, c, (goal, alpha): (LuaColor3, f64)| {
            let alpha = alpha as f32;
            let channel = |a: f32, b: f32| (a + (b - a) * alpha).clamp(0.0, 1.0);
            let (a, b) = (c.0, goal.0);
            Ok(LuaColor3(Color3::new(
                channel(a.r, b.r),
                channel(a.g, b.g),
                channel(a.b, b.b),
            )))
        });
        methods.add_method("ToHSV", |_, c, ()| {
            let (h, s, v) = c.to_hsv();
            Ok((f32_to_lua(h), f32_to_lua(s), f32_to_lua(v)))
        });
        methods.add_method("ToHex", |_, c, ()| Ok(c.to_hex()));
        methods.add_meta_method(MetaMethod::Eq, |lua, c, other: mlua::Value| {
            Ok(LuaColor3::from_lua(other, lua).is_ok_and(|other| other.0 == c.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, c, ()| {
            let c = c.0;
            Ok(format!(
                "{}, {}, {}",
                component_text(c.r),
                component_text(c.g),
                component_text(c.b)
            ))
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaUDim(pub UDim);

impl UserData for LuaUDim {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Scale", |_, value| Ok(f32_to_lua(value.0.scale)));
        fields.add_field_method_get("Offset", |_, value| Ok(f32_to_lua(value.0.offset)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, value, other: mlua::Value| {
            Ok(LuaUDim::from_lua(other, lua).is_ok_and(|other| other.0 == value.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, value, ()| {
            Ok(format!(
                "{}, {}",
                component_text(value.0.scale),
                component_text(value.0.offset)
            ))
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaUDim2(pub UDim2);

impl LuaUDim2 {
    /// Each scale and offset of `self` and `other` put through `f`.
    fn zip(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self {
        let axis = |a: UDim, b: UDim| UDim::new(f(a.scale, b.scale), f(a.offset, b.offset));
        Self(UDim2 {
            x: axis(self.0.x, other.0.x),
            y: axis(self.0.y, other.0.y),
        })
    }
}

impl UserData for LuaUDim2 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, value| Ok(LuaUDim(value.0.x)));
        fields.add_field_method_get("Y", |_, value| Ok(LuaUDim(value.0.y)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Add, |_, (a, b): (LuaUDim2, LuaUDim2)| {
            Ok(a.zip(b, |a, b| a + b))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (a, b): (LuaUDim2, LuaUDim2)| {
            Ok(a.zip(b, |a, b| a - b))
        });
        methods.add_meta_method(MetaMethod::Eq, |lua, value, other: mlua::Value| {
            Ok(LuaUDim2::from_lua(other, lua).is_ok_and(|other| other.0 == value.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, value, ()| {
            let v = value.0;
            Ok(format!(
                "{}, {}, {}, {}",
                component_text(v.x.scale),
                component_text(v.x.offset),
                component_text(v.y.scale),
                component_text(v.y.offset)
            ))
        });
    }
}

/// The `Enum` global. `Enum.Material`, `Enum.PartType`, `Enum.KeyCode`,
/// `Enum.TextureChannel`, `Enum.ParticleType`, `Enum.ToneMapping`, the tween enums
/// (`Enum.EasingStyle`, `Enum.EasingDirection`, `Enum.PlaybackState`) and
/// the graphics settings' enums (`Enum.GraphicsQuality` and the ones
/// `UserGameSettings` overrides take) are its members.
#[derive(Clone, Copy, Debug)]
pub struct LuaEnum;

/// One enum scripts can use through `Enum`, such as `Enum.Material`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumType {
    Material,
    PartType,
    KeyCode,
    TextureChannel,
    ParticleType,
    ToneMapping,
    EasingStyle,
    EasingDirection,
    PlaybackState,
    GraphicsQuality,
    ShadowQuality,
    TextureQuality,
    AntiAliasing,
    AmbientOcclusion,
    ReflectionQuality,
    CloudQuality,
    UserInputType,
    UserInputState,
    MouseBehavior,
    CameraType,
    RenderPriority,
    ActuatorType,
    ActuatorRelativeTo,
    PositionAlignmentMode,
    OrientationAlignmentMode,
    RaycastFilterType,
    FracturePattern,
    ProximityPromptExclusivity,
}

impl EnumType {
    pub const ALL: [EnumType; 28] = [
        EnumType::Material,
        EnumType::PartType,
        EnumType::KeyCode,
        EnumType::TextureChannel,
        EnumType::ParticleType,
        EnumType::ToneMapping,
        EnumType::EasingStyle,
        EnumType::EasingDirection,
        EnumType::PlaybackState,
        EnumType::GraphicsQuality,
        EnumType::ShadowQuality,
        EnumType::TextureQuality,
        EnumType::AntiAliasing,
        EnumType::AmbientOcclusion,
        EnumType::ReflectionQuality,
        EnumType::CloudQuality,
        EnumType::UserInputType,
        EnumType::UserInputState,
        EnumType::MouseBehavior,
        EnumType::CameraType,
        EnumType::RenderPriority,
        EnumType::ActuatorType,
        EnumType::ActuatorRelativeTo,
        EnumType::PositionAlignmentMode,
        EnumType::OrientationAlignmentMode,
        EnumType::RaycastFilterType,
        EnumType::FracturePattern,
        EnumType::ProximityPromptExclusivity,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EnumType::Material => "Material",
            EnumType::PartType => "PartType",
            EnumType::KeyCode => "KeyCode",
            EnumType::TextureChannel => "TextureChannel",
            EnumType::ParticleType => "ParticleType",
            EnumType::ToneMapping => "ToneMapping",
            EnumType::EasingStyle => "EasingStyle",
            EnumType::EasingDirection => "EasingDirection",
            EnumType::PlaybackState => "PlaybackState",
            EnumType::GraphicsQuality => "GraphicsQuality",
            EnumType::ShadowQuality => "ShadowQuality",
            EnumType::TextureQuality => "TextureQuality",
            EnumType::AntiAliasing => "AntiAliasing",
            EnumType::AmbientOcclusion => "AmbientOcclusion",
            EnumType::ReflectionQuality => "ReflectionQuality",
            EnumType::CloudQuality => "CloudQuality",
            EnumType::UserInputType => "UserInputType",
            EnumType::UserInputState => "UserInputState",
            EnumType::MouseBehavior => "MouseBehavior",
            EnumType::CameraType => "CameraType",
            EnumType::RenderPriority => "RenderPriority",
            EnumType::ActuatorType => "ActuatorType",
            EnumType::ActuatorRelativeTo => "ActuatorRelativeTo",
            EnumType::PositionAlignmentMode => "PositionAlignmentMode",
            EnumType::OrientationAlignmentMode => "OrientationAlignmentMode",
            EnumType::RaycastFilterType => "RaycastFilterType",
            EnumType::FracturePattern => "FracturePattern",
            EnumType::ProximityPromptExclusivity => "ProximityPromptExclusivity",
        }
    }

    /// Every item name, in order.
    pub fn item_names(self) -> Vec<&'static str> {
        match self {
            EnumType::Material => Material::ALL.iter().map(|m| m.as_str()).collect(),
            EnumType::PartType => PartType::ALL.iter().map(|t| t.as_str()).collect(),
            EnumType::KeyCode => KeyCode::ALL.iter().map(|k| k.as_str()).collect(),
            EnumType::TextureChannel => TextureChannel::ALL.iter().map(|c| c.as_str()).collect(),
            EnumType::ParticleType => ParticleType::ALL.iter().map(|t| t.as_str()).collect(),
            EnumType::ToneMapping => ToneMapping::ALL.iter().map(|t| t.as_str()).collect(),
            EnumType::EasingStyle => TweenStyle::names(),
            EnumType::EasingDirection => EasingDirection::ALL.iter().map(|d| d.as_str()).collect(),
            EnumType::PlaybackState => PlaybackState::ALL.iter().map(|s| s.as_str()).collect(),
            EnumType::GraphicsQuality => user_settings::quality_names(),
            EnumType::ShadowQuality => user_settings::shadow_quality_names(),
            EnumType::TextureQuality => user_settings::texture_quality_names(),
            EnumType::AntiAliasing => user_settings::anti_aliasing_names(),
            EnumType::AmbientOcclusion => user_settings::ambient_occlusion_names(),
            EnumType::ReflectionQuality => user_settings::reflection_quality_names(),
            EnumType::CloudQuality => user_settings::cloud_quality_names(),
            EnumType::UserInputType => UserInputType::ALL.iter().map(|t| t.as_str()).collect(),
            EnumType::UserInputState => UserInputState::ALL.iter().map(|s| s.as_str()).collect(),
            EnumType::MouseBehavior => MouseBehavior::ALL.iter().map(|b| b.as_str()).collect(),
            EnumType::CameraType => CameraType::ALL.iter().map(|t| t.as_str()).collect(),
            EnumType::RenderPriority => RENDER_PRIORITIES.iter().map(|(name, _)| *name).collect(),
            // What drives a hinge, prismatic or cylindrical constraint.
            EnumType::ActuatorType => vec!["None", "Motor", "Servo"],
            // The axes a mover's vector is in.
            EnumType::ActuatorRelativeTo => vec!["Attachment0", "Attachment1", "World"],
            // What an align follows: Attachment1, or its own target.
            EnumType::PositionAlignmentMode | EnumType::OrientationAlignmentMode => {
                vec!["OneAttachment", "TwoAttachment"]
            }
            // Whether a query's filter list is passed by or the only parts found.
            EnumType::RaycastFilterType => vec!["Exclude", "Include"],
            // How a Fracture's pieces are spread (Phase 42).
            EnumType::FracturePattern => vec!["Uniform", "Impact"],
            // Which ProximityPrompts in reach show (Phase 44).
            EnumType::ProximityPromptExclusivity => {
                vec!["OnePerButton", "OneGlobally", "AlwaysShow"]
            }
        }
    }

    /// The enum that a property of `kind` holds, if it holds one.
    pub fn of_kind(kind: ValueKind) -> Option<EnumType> {
        match kind {
            ValueKind::Material => Some(EnumType::Material),
            ValueKind::PartType => Some(EnumType::PartType),
            ValueKind::TextureChannel => Some(EnumType::TextureChannel),
            ValueKind::ParticleType => Some(EnumType::ParticleType),
            ValueKind::ToneMapping => Some(EnumType::ToneMapping),
            _ => None,
        }
    }
}

/// `Enum.RenderPriority`'s items and their `Value`s: where in the frame a
/// function bound with `BindToRenderStep` runs, lowest first.
const RENDER_PRIORITIES: [(&str, u32); 5] = [
    ("First", 0),
    ("Input", 100),
    ("Camera", 200),
    ("Character", 300),
    ("Last", 2000),
];

/// One enum item, such as `Enum.Material.Neon`. `name` is always one of
/// `enum_type`'s item names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaEnumItem {
    pub enum_type: EnumType,
    pub name: &'static str,
}

impl LuaEnumItem {
    /// The item of `enum_type` called `name`, if there is one.
    pub fn find(enum_type: EnumType, name: &str) -> Option<Self> {
        enum_type
            .item_names()
            .into_iter()
            .find(|item| *item == name)
            .map(|name| Self { enum_type, name })
    }

    /// The item's `Value`: a RenderPriority's priority, or for any other
    /// enum the item's place in its list, from 0.
    pub fn value(self) -> u32 {
        if self.enum_type == EnumType::RenderPriority
            && let Some((_, value)) = RENDER_PRIORITIES
                .iter()
                .find(|(name, _)| *name == self.name)
        {
            return *value;
        }
        let names = self.enum_type.item_names();
        names
            .iter()
            .position(|name| *name == self.name)
            .unwrap_or(0) as u32
    }

    pub fn key(key: KeyCode) -> Self {
        Self {
            enum_type: EnumType::KeyCode,
            name: key.as_str(),
        }
    }

    pub fn as_key(self) -> Option<KeyCode> {
        match self.enum_type {
            EnumType::KeyCode => KeyCode::from_name(self.name),
            _ => None,
        }
    }

    /// The item an enum property value is, such as `Enum.Material.Neon`.
    fn from_value(value: &Value) -> Option<Self> {
        let (enum_type, name) = match value {
            Value::Material(m) => (EnumType::Material, m.as_str()),
            Value::PartType(t) => (EnumType::PartType, t.as_str()),
            Value::TextureChannel(c) => (EnumType::TextureChannel, c.as_str()),
            Value::ParticleType(t) => (EnumType::ParticleType, t.as_str()),
            Value::ToneMapping(t) => (EnumType::ToneMapping, t.as_str()),
            _ => return None,
        };
        Some(Self { enum_type, name })
    }

    /// This item as a property value, if it's an item of a property enum.
    pub(crate) fn to_value(self) -> Option<Value> {
        match self.enum_type {
            EnumType::Material => Material::from_name(self.name).map(Value::Material),
            EnumType::PartType => PartType::from_name(self.name).map(Value::PartType),
            EnumType::TextureChannel => {
                TextureChannel::from_name(self.name).map(Value::TextureChannel)
            }
            EnumType::ParticleType => ParticleType::from_name(self.name).map(Value::ParticleType),
            EnumType::ToneMapping => ToneMapping::from_name(self.name).map(Value::ToneMapping),
            EnumType::KeyCode
            | EnumType::EasingStyle
            | EnumType::EasingDirection
            | EnumType::PlaybackState
            | EnumType::GraphicsQuality
            | EnumType::ShadowQuality
            | EnumType::TextureQuality
            | EnumType::AntiAliasing
            | EnumType::AmbientOcclusion
            | EnumType::ReflectionQuality
            | EnumType::CloudQuality
            | EnumType::UserInputType
            | EnumType::UserInputState
            | EnumType::MouseBehavior
            | EnumType::CameraType
            | EnumType::RenderPriority
            | EnumType::ActuatorType
            | EnumType::ActuatorRelativeTo
            | EnumType::PositionAlignmentMode
            | EnumType::OrientationAlignmentMode
            | EnumType::RaycastFilterType
            | EnumType::FracturePattern
            | EnumType::ProximityPromptExclusivity => None,
        }
    }
}

/// The message for a name that isn't an item of `enum_type`, listing the items.
fn not_an_item(enum_type: EnumType, name: &str) -> String {
    format!(
        "{name} is not a valid member of Enum.{}; choose from {}",
        enum_type.name(),
        enum_type.item_names().join(", ")
    )
}

impl UserData for LuaEnum {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, _, name: String| {
            EnumType::ALL
                .into_iter()
                .find(|enum_type| enum_type.name() == name)
                .map(LuaEnumType)
                .ok_or_else(|| {
                    mlua::Error::runtime(format!("{name} is not a valid member of Enum"))
                })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Enum"));
    }
}

/// One enum, as scripts see it: `Enum.Material`.
#[derive(Clone, Copy, Debug)]
pub struct LuaEnumType(EnumType);

impl UserData for LuaEnumType {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, name: String| {
            LuaEnumItem::find(this.0, &name)
                .ok_or_else(|| mlua::Error::runtime(not_an_item(this.0, &name)))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Enum.{}", this.0.name()))
        });
    }
}

impl UserData for LuaEnumItem {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, item| Ok(item.name));
        fields.add_field_method_get("Value", |_, item| Ok(item.value()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, item, other: mlua::Value| {
            Ok(LuaEnumItem::from_lua(other, lua).is_ok_and(|other| other == *item))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, item, ()| {
            Ok(format!("Enum.{}.{}", item.enum_type.name(), item.name))
        });
    }
}

/// Reads a userdata value of type `T` back out of Lua, for types that are
/// `Clone`. mlua leaves this to each type, so this macro writes the same
/// `FromLua` code for all of them.
macro_rules! from_lua_by_clone {
    ($($ty:ident => $name:literal),* $(,)?) => {$(
        impl FromLua for $ty {
            fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
                if let mlua::Value::UserData(data) = &value
                    && let Ok(inner) = data.borrow::<$ty>()
                {
                    return Ok(inner.clone());
                }
                Err(mlua::Error::FromLuaConversionError {
                    from: value.type_name(),
                    to: $name.to_owned(),
                    message: None,
                })
            }
        }
    )*};
}

from_lua_by_clone! {
    LuaVector3 => "Vector3",
    LuaVector2 => "Vector2",
    LuaColor3 => "Color3",
    LuaUDim => "UDim",
    LuaUDim2 => "UDim2",
    LuaEnumItem => "EnumItem",
    LuaCFrame => "CFrame",
}

/// A property value as a Lua value.
pub fn value_to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    match value {
        Value::Bool(b) => Ok(mlua::Value::Boolean(*b)),
        Value::Number(n) => Ok(mlua::Value::Number(*n)),
        Value::String(s) => s.as_str().into_lua(lua),
        Value::Vector3(v) => LuaVector3(*v).into_lua(lua),
        Value::Vector2(v) => LuaVector2(*v).into_lua(lua),
        Value::UDim2(v) => LuaUDim2(*v).into_lua(lua),
        Value::Color3(c) => LuaColor3(*c).into_lua(lua),
        Value::PartType(_)
        | Value::Material(_)
        | Value::TextureChannel(_)
        | Value::ParticleType(_)
        | Value::ToneMapping(_) => LuaEnumItem::from_value(value)
            .expect("enum property values are enum items")
            .into_lua(lua),
        Value::CFrame(cf) => crate::cframe::LuaCFrame::from_core(*cf).into_lua(lua),
        // An item this engine no longer has reads as its name.
        Value::EnumItem(item) => match EnumType::ALL
            .into_iter()
            .find(|enum_type| enum_type.name() == item.enum_name)
            .and_then(|enum_type| LuaEnumItem::find(enum_type, &item.item))
        {
            Some(found) => found.into_lua(lua),
            None => item.item.as_str().into_lua(lua),
        },
        Value::PhysicalProperties(p) => p.map(crate::physical::LuaPhysicalProperties).into_lua(lua),
        // An ObjectValue pointing at a destroyed Instance holds nil.
        Value::Instance(id) => {
            let alive = id
                .filter(|&id| crate::instance::world(lua).map_or(true, |world| world.contains(id)));
            alive.map(LuaInstance).into_lua(lua)
        }
    }
}

/// A Lua value as an attribute's value (`SetAttribute`): `None` for nil,
/// which removes the attribute. Errors name what was given instead.
pub fn attribute_from_lua(lua: &Lua, value: &mlua::Value) -> Result<Option<Value>, String> {
    let converted = match value {
        mlua::Value::Nil => return Ok(None),
        mlua::Value::Boolean(b) => Some(Value::Bool(*b)),
        mlua::Value::Integer(n) => Some(Value::Number(*n as f64)),
        mlua::Value::Number(n) => Some(Value::Number(*n)),
        mlua::Value::String(s) => Some(Value::String(s.to_string_lossy())),
        mlua::Value::UserData(data) => {
            if let Ok(item) = data.borrow::<LuaEnumItem>() {
                Some(Value::EnumItem(axcenity_core::EnumItem {
                    enum_name: item.enum_type.name().to_owned(),
                    item: item.name.to_owned(),
                }))
            } else if let Ok(cf) = data.borrow::<crate::cframe::LuaCFrame>() {
                Some(Value::CFrame(cf.to_core()))
            } else {
                [
                    ValueKind::Vector3,
                    ValueKind::Vector2,
                    ValueKind::UDim2,
                    ValueKind::Color3,
                ]
                .into_iter()
                .find_map(|kind| value_from_lua(lua, value.clone(), kind).ok())
            }
        }
        _ => None,
    };
    converted.map(Some).ok_or_else(|| {
        format!(
            "an attribute can be a boolean, number, string, Vector3, Vector2, Color3, UDim2, \
             CFrame, EnumItem or nil, not a {}",
            type_name(lua, value)
        )
    })
}

/// A Lua value as a property value of `kind`, or a message ending a sentence
/// that starts with the property's name, such as "must be a Vector3, not a number".
pub fn value_from_lua(lua: &Lua, value: mlua::Value, kind: ValueKind) -> Result<Value, String> {
    let wrong_type =
        |value: &mlua::Value| format!("must be a {kind}, not a {}", type_name(lua, value));
    let converted = match (kind, &value) {
        (ValueKind::Bool, mlua::Value::Boolean(b)) => Some(Value::Bool(*b)),
        (ValueKind::Number, mlua::Value::Integer(n)) => Some(Value::Number(*n as f64)),
        (ValueKind::Number, mlua::Value::Number(n)) => Some(Value::Number(*n)),
        (ValueKind::String, mlua::Value::String(s)) => Some(Value::String(s.to_string_lossy())),
        (ValueKind::Vector3, _) => LuaVector3::from_lua(value.clone(), lua)
            .ok()
            .map(|v| Value::Vector3(v.0)),
        (ValueKind::Vector2, _) => LuaVector2::from_lua(value.clone(), lua)
            .ok()
            .map(|v| Value::Vector2(v.0)),
        (ValueKind::UDim2, _) => LuaUDim2::from_lua(value.clone(), lua)
            .ok()
            .map(|v| Value::UDim2(v.0)),
        (ValueKind::Color3, _) => LuaColor3::from_lua(value.clone(), lua)
            .ok()
            .map(|c| Value::Color3(c.0)),
        (
            ValueKind::Material
            | ValueKind::PartType
            | ValueKind::TextureChannel
            | ValueKind::ParticleType
            | ValueKind::ToneMapping,
            mlua::Value::String(s),
        ) => {
            let enum_type = EnumType::of_kind(kind).expect("an enum kind");
            let name = s.to_string_lossy();
            return LuaEnumItem::find(enum_type, &name)
                .and_then(LuaEnumItem::to_value)
                .ok_or_else(|| format!("can't be \"{name}\": {}", not_an_item(enum_type, &name)));
        }
        (
            ValueKind::Material
            | ValueKind::PartType
            | ValueKind::TextureChannel
            | ValueKind::ParticleType
            | ValueKind::ToneMapping,
            _,
        ) => LuaEnumItem::from_lua(value.clone(), lua)
            .ok()
            .and_then(LuaEnumItem::to_value)
            .filter(|item| item.kind() == kind),
        (ValueKind::CFrame, _) => crate::cframe::LuaCFrame::from_lua(value.clone(), lua)
            .ok()
            .map(|cf| Value::CFrame(cf.to_core())),
        (ValueKind::EnumItem, _) => LuaEnumItem::from_lua(value.clone(), lua).ok().map(|item| {
            Value::EnumItem(axcenity_core::EnumItem {
                enum_name: item.enum_type.name().to_owned(),
                item: item.name.to_owned(),
            })
        }),
        (ValueKind::PhysicalProperties, mlua::Value::Nil) => Some(Value::PhysicalProperties(None)),
        (ValueKind::PhysicalProperties, _) => {
            crate::physical::LuaPhysicalProperties::from_lua(value.clone(), lua)
                .ok()
                .map(|p| Value::PhysicalProperties(Some(p.0.clamped())))
        }
        (ValueKind::Instance, mlua::Value::Nil) => Some(Value::Instance(None)),
        (ValueKind::Instance, _) => LuaInstance::from_lua(value.clone(), lua)
            .ok()
            .map(|instance| Value::Instance(Some(instance.0))),
        _ => None,
    };
    converted.ok_or_else(|| wrong_type(&value))
}

/// A Lua value as an item of the enum `enum_name`, for a property that
/// holds one (`HingeConstraint.ActuatorType`): the item, or its name.
pub fn enum_item_from_lua(lua: &Lua, value: mlua::Value, enum_name: &str) -> Result<Value, String> {
    let Some(enum_type) = EnumType::ALL
        .into_iter()
        .find(|enum_type| enum_type.name() == enum_name)
    else {
        return value_from_lua(lua, value, ValueKind::EnumItem);
    };
    let item = match &value {
        mlua::Value::String(s) => {
            let name = s.to_string_lossy();
            LuaEnumItem::find(enum_type, &name)
                .ok_or_else(|| format!("can't be \"{name}\": {}", not_an_item(enum_type, &name)))?
        }
        _ => LuaEnumItem::from_lua(value.clone(), lua)
            .ok()
            .filter(|item| item.enum_type == enum_type)
            .ok_or_else(|| {
                format!(
                    "must be an Enum.{enum_name} item, not a {}",
                    type_name(lua, &value)
                )
            })?,
    };
    Ok(Value::EnumItem(axcenity_core::EnumItem {
        enum_name: enum_name.to_owned(),
        item: item.name.to_owned(),
    }))
}

/// A Lua value as whichever property value it is, for places that keep any
/// value (`SaveService:SetPlayerData`). `None` for nil, tables, functions
/// and Instances.
pub fn any_value_from_lua(lua: &Lua, value: &mlua::Value) -> Option<Value> {
    match value {
        mlua::Value::Boolean(b) => Some(Value::Bool(*b)),
        mlua::Value::Integer(n) => Some(Value::Number(*n as f64)),
        mlua::Value::Number(n) => Some(Value::Number(*n)),
        mlua::Value::String(s) => Some(Value::String(s.to_string_lossy())),
        mlua::Value::UserData(_) => [
            ValueKind::Vector3,
            ValueKind::Vector2,
            ValueKind::UDim2,
            ValueKind::Color3,
        ]
        .into_iter()
        .find_map(|kind| value_from_lua(lua, value.clone(), kind).ok())
        .or_else(|| {
            LuaEnumItem::from_lua(value.clone(), lua)
                .ok()
                .and_then(LuaEnumItem::to_value)
        }),
        _ => None,
    }
}

/// The type name to show in messages: the engine type for engine values
/// (`Vector3`, `Part`, `Material`), otherwise Lua's own (`number`, `nil`).
pub fn type_name(lua: &Lua, value: &mlua::Value) -> String {
    if let mlua::Value::UserData(data) = value {
        if data.is::<LuaVector3>() {
            return "Vector3".to_owned();
        }
        if data.is::<LuaVector2>() {
            return "Vector2".to_owned();
        }
        if data.is::<LuaColor3>() {
            return "Color3".to_owned();
        }
        if data.is::<LuaUDim>() {
            return "UDim".to_owned();
        }
        if data.is::<LuaUDim2>() {
            return "UDim2".to_owned();
        }
        if data.is::<crate::cframe::LuaCFrame>() {
            return "CFrame".to_owned();
        }
        if data.is::<crate::random::LuaRandom>() {
            return "Random".to_owned();
        }
        if data.is::<crate::physical::LuaPhysicalProperties>() {
            return "PhysicalProperties".to_owned();
        }
        if let Ok(item) = data.borrow::<LuaEnumItem>() {
            return item.enum_type.name().to_owned();
        }
        // Engine objects that aren't Instances, by the names scripts know them by.
        let others = [
            (data.is::<crate::camera::LuaCamera>(), "Camera"),
            (data.is::<crate::camera::Ray>(), "Ray"),
            (data.is::<crate::camera::LuaMouse>(), "Mouse"),
            (data.is::<crate::input::LuaInputObject>(), "InputObject"),
            (
                data.is::<crate::input::LuaUserInputService>(),
                "UserInputService",
            ),
            (
                data.is::<crate::input::LuaContextActionService>(),
                "ContextActionService",
            ),
            (data.is::<crate::events::LuaSignal>(), "Signal"),
            (data.is::<crate::events::LuaConnection>(), "Connection"),
            (data.is::<crate::run_service::LuaRunService>(), "RunService"),
            (data.is::<crate::tween::LuaTweenService>(), "TweenService"),
            (
                data.is::<crate::collection::LuaCollectionService>(),
                "CollectionService",
            ),
            (data.is::<crate::save::LuaSaveService>(), "SaveService"),
            (data.is::<crate::data::LuaDataService>(), "DataService"),
            (data.is::<crate::data::LuaDataTable>(), "DataTable"),
            (data.is::<crate::data::LuaDataRow>(), "DataRow"),
            (data.is::<crate::tween::LuaTween>(), "Tween"),
            (data.is::<crate::tween::LuaTweenInfo>(), "TweenInfo"),
            (data.is::<crate::query::LuaRaycastParams>(), "RaycastParams"),
            (data.is::<crate::query::LuaOverlapParams>(), "OverlapParams"),
            (
                data.is::<crate::query::LuaPhysicsService>(),
                "PhysicsService",
            ),
        ];
        if let Some((_, name)) = others.into_iter().find(|(is, _)| *is) {
            return name.to_owned();
        }
        if let Ok(instance) = data.borrow::<LuaInstance>()
            && let Ok(world) = crate::instance::world(lua)
            && let Some(instance) = world.get(instance.0)
        {
            return instance.class().to_string();
        }
    }
    match value.type_name() {
        // mlua hands whole numbers over as "integer", but they are `number` to scripts.
        "integer" => "number".to_owned(),
        name => name.to_owned(),
    }
}

/// What `typeof` gives: [`type_name`], except that every Instance is
/// `"Instance"` (even a destroyed one) and enums are `"EnumItem"`, `"Enum"`
/// (`Enum.Material`) and `"Enums"` (`Enum` itself), as in Roblox. Luau's own
/// `typeof` would call them all `userdata`.
pub fn typeof_name(lua: &Lua, value: &mlua::Value) -> String {
    if let mlua::Value::UserData(data) = value {
        let engine = [
            (data.is::<LuaInstance>(), "Instance"),
            (data.is::<LuaEnumItem>(), "EnumItem"),
            (data.is::<LuaEnumType>(), "Enum"),
            (data.is::<LuaEnum>(), "Enums"),
        ];
        if let Some((_, name)) = engine.into_iter().find(|(is, _)| *is) {
            return name.to_owned();
        }
    }
    type_name(lua, value)
}
