//! Lua versions of the property value types (`Vector3`, `Color3`, enum
//! items), and conversion between Lua values and core [`Value`]s.
//!
//! Rust's "orphan rule" only allows implementing a trait from another crate
//! (here mlua's `UserData`) for a type from *this* crate. So each core type
//! is wrapped in a small struct, such as `LuaVector3(Vector3)`.

use axcenity_core::{
    Color3, KeyCode, Material, PartType, ParticleType, TextureChannel, UDim, UDim2, Value,
    ValueKind, Vector3,
};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::instance::LuaInstance;

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
        let v = self.0;
        (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
    }
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
        methods.add_meta_function(MetaMethod::Div, |_, (a, b): (LuaVector3, Operand)| {
            Ok(match b {
                Operand::Vector(b) => a.zip(b, |a, b| a / b),
                Operand::Number(n) => a.map(|c| c / n),
            })
        });
        methods.add_meta_method(MetaMethod::Unm, |_, v, ()| Ok(v.map(|c| -c)));
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

#[derive(Clone, Copy, Debug)]
pub struct LuaColor3(pub Color3);

impl UserData for LuaColor3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("R", |_, c| Ok(f32_to_lua(c.0.r)));
        fields.add_field_method_get("G", |_, c| Ok(f32_to_lua(c.0.g)));
        fields.add_field_method_get("B", |_, c| Ok(f32_to_lua(c.0.b)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
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

impl UserData for LuaUDim2 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, value| Ok(LuaUDim(value.0.x)));
        fields.add_field_method_get("Y", |_, value| Ok(LuaUDim(value.0.y)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
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
/// `Enum.TextureChannel` and `Enum.ParticleType` are its members.
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
}

impl EnumType {
    pub const ALL: [EnumType; 5] = [
        EnumType::Material,
        EnumType::PartType,
        EnumType::KeyCode,
        EnumType::TextureChannel,
        EnumType::ParticleType,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EnumType::Material => "Material",
            EnumType::PartType => "PartType",
            EnumType::KeyCode => "KeyCode",
            EnumType::TextureChannel => "TextureChannel",
            EnumType::ParticleType => "ParticleType",
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
        }
    }

    /// The enum that a property of `kind` holds, if it holds one.
    pub fn of_kind(kind: ValueKind) -> Option<EnumType> {
        match kind {
            ValueKind::Material => Some(EnumType::Material),
            ValueKind::PartType => Some(EnumType::PartType),
            ValueKind::TextureChannel => Some(EnumType::TextureChannel),
            ValueKind::ParticleType => Some(EnumType::ParticleType),
            _ => None,
        }
    }
}

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
            _ => return None,
        };
        Some(Self { enum_type, name })
    }

    /// This item as a property value, if it's an item of a property enum.
    fn to_value(self) -> Option<Value> {
        match self.enum_type {
            EnumType::Material => Material::from_name(self.name).map(Value::Material),
            EnumType::PartType => PartType::from_name(self.name).map(Value::PartType),
            EnumType::TextureChannel => {
                TextureChannel::from_name(self.name).map(Value::TextureChannel)
            }
            EnumType::ParticleType => ParticleType::from_name(self.name).map(Value::ParticleType),
            EnumType::KeyCode => None,
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
    LuaColor3 => "Color3",
    LuaUDim => "UDim",
    LuaUDim2 => "UDim2",
    LuaEnumItem => "EnumItem",
}

/// A property value as a Lua value.
pub fn value_to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    match value {
        Value::Bool(b) => Ok(mlua::Value::Boolean(*b)),
        Value::Number(n) => Ok(mlua::Value::Number(*n)),
        Value::String(s) => s.as_str().into_lua(lua),
        Value::Vector3(v) => LuaVector3(*v).into_lua(lua),
        Value::UDim2(v) => LuaUDim2(*v).into_lua(lua),
        Value::Color3(c) => LuaColor3(*c).into_lua(lua),
        Value::PartType(_)
        | Value::Material(_)
        | Value::TextureChannel(_)
        | Value::ParticleType(_) => LuaEnumItem::from_value(value)
            .expect("enum property values are enum items")
            .into_lua(lua),
    }
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
            | ValueKind::ParticleType,
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
            | ValueKind::ParticleType,
            _,
        ) => LuaEnumItem::from_lua(value.clone(), lua)
            .ok()
            .and_then(LuaEnumItem::to_value)
            .filter(|item| item.kind() == kind),
        _ => None,
    };
    converted.ok_or_else(|| wrong_type(&value))
}

/// The type name to show in messages: the engine type for engine values
/// (`Vector3`, `Part`, `Material`), otherwise Lua's own (`number`, `nil`).
pub fn type_name(lua: &Lua, value: &mlua::Value) -> String {
    if let mlua::Value::UserData(data) = value {
        if data.is::<LuaVector3>() {
            return "Vector3".to_owned();
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
        if let Ok(item) = data.borrow::<LuaEnumItem>() {
            return item.enum_type.name().to_owned();
        }
        // Engine objects that aren't Instances, by the names scripts know them by.
        let others = [
            (data.is::<crate::client::LuaCamera>(), "Camera"),
            (data.is::<crate::client::LuaInputObject>(), "InputObject"),
            (
                data.is::<crate::client::LuaUserInputService>(),
                "UserInputService",
            ),
            (data.is::<crate::events::LuaSignal>(), "Signal"),
            (data.is::<crate::events::LuaConnection>(), "Connection"),
            (data.is::<crate::run_service::LuaRunService>(), "RunService"),
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
