//! Property values. Every Instance property holds one [`Value`], and its
//! kind never changes after the Instance is created.

use std::fmt;

use crate::enums::{Material, PartType, ParticleType, TextureChannel, ToneMapping};
use crate::world::InstanceId;

/// A position, size or direction in studs. Y is up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A point or size on the screen, in pixels: across and down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A scale and pixel offset on one UI axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UDim {
    pub scale: f32,
    pub offset: f32,
}

impl UDim {
    pub const fn new(scale: f32, offset: f32) -> Self {
        Self { scale, offset }
    }
}

/// A UI position or size with scale and pixel-offset components.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UDim2 {
    pub x: UDim,
    pub y: UDim,
}

impl UDim2 {
    pub const fn new(x_scale: f32, x_offset: f32, y_scale: f32, y_offset: f32) -> Self {
        Self {
            x: UDim::new(x_scale, x_offset),
            y: UDim::new(y_scale, y_offset),
        }
    }
}

/// A color as people pick it (sRGB), with each channel from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3 {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
}

/// A position and a rotation together. The rotation is a unit quaternion
/// (`x`, `y`, `z`, `w`), so it keeps a script's CFrame exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CFrame {
    pub position: Vector3,
    pub rotation: [f32; 4],
}

impl CFrame {
    pub const IDENTITY: Self = Self {
        position: Vector3::new(0.0, 0.0, 0.0),
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
}

impl Default for CFrame {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// An item of any enum scripts know, by name, such as `Material.Neon` or
/// `KeyCode.E`. Attributes keep enum items this way; `core` doesn't list
/// every enum, so the script side checks the names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumItem {
    pub enum_name: String,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Number(f64),
    String(String),
    Vector3(Vector3),
    Vector2(Vector2),
    UDim2(UDim2),
    Color3(Color3),
    PartType(PartType),
    Material(Material),
    TextureChannel(TextureChannel),
    ParticleType(ParticleType),
    ToneMapping(ToneMapping),
    CFrame(CFrame),
    EnumItem(EnumItem),
    /// Another Instance, or none (`ObjectValue.Value`).
    Instance(Option<InstanceId>),
}

/// The type of a [`Value`], without the data. Used to check assignments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Bool,
    Number,
    String,
    Vector3,
    Vector2,
    UDim2,
    Color3,
    PartType,
    Material,
    TextureChannel,
    ParticleType,
    ToneMapping,
    CFrame,
    EnumItem,
    Instance,
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Bool(_) => ValueKind::Bool,
            Value::Number(_) => ValueKind::Number,
            Value::String(_) => ValueKind::String,
            Value::Vector3(_) => ValueKind::Vector3,
            Value::Vector2(_) => ValueKind::Vector2,
            Value::UDim2(_) => ValueKind::UDim2,
            Value::Color3(_) => ValueKind::Color3,
            Value::PartType(_) => ValueKind::PartType,
            Value::Material(_) => ValueKind::Material,
            Value::TextureChannel(_) => ValueKind::TextureChannel,
            Value::ParticleType(_) => ValueKind::ParticleType,
            Value::ToneMapping(_) => ValueKind::ToneMapping,
            Value::CFrame(_) => ValueKind::CFrame,
            Value::EnumItem(_) => ValueKind::EnumItem,
            Value::Instance(_) => ValueKind::Instance,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_vector3(&self) -> Option<Vector3> {
        match self {
            Value::Vector3(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_vector2(&self) -> Option<Vector2> {
        match self {
            Value::Vector2(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_udim2(&self) -> Option<UDim2> {
        match self {
            Value::UDim2(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_color3(&self) -> Option<Color3> {
        match self {
            Value::Color3(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_part_type(&self) -> Option<PartType> {
        match self {
            Value::PartType(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_material(&self) -> Option<Material> {
        match self {
            Value::Material(m) => Some(*m),
            _ => None,
        }
    }

    pub fn as_texture_channel(&self) -> Option<TextureChannel> {
        match self {
            Value::TextureChannel(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_particle_type(&self) -> Option<ParticleType> {
        match self {
            Value::ParticleType(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_tone_mapping(&self) -> Option<ToneMapping> {
        match self {
            Value::ToneMapping(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_cframe(&self) -> Option<CFrame> {
        match self {
            Value::CFrame(c) => Some(*c),
            _ => None,
        }
    }

    /// The Instance an `Instance` value points at, if it points at one.
    pub fn as_instance(&self) -> Option<InstanceId> {
        match self {
            Value::Instance(id) => *id,
            _ => None,
        }
    }

    /// A property enum's value as a plain enum item (`Material.Neon`), as
    /// attributes keep them.
    pub fn to_enum_item(&self) -> Option<EnumItem> {
        let (enum_name, item) = match self {
            Value::PartType(t) => ("PartType", t.as_str()),
            Value::Material(m) => ("Material", m.as_str()),
            Value::TextureChannel(c) => ("TextureChannel", c.as_str()),
            Value::ParticleType(t) => ("ParticleType", t.as_str()),
            Value::ToneMapping(t) => ("ToneMapping", t.as_str()),
            _ => return None,
        };
        Some(EnumItem {
            enum_name: enum_name.to_owned(),
            item: item.to_owned(),
        })
    }

    /// Whether an attribute can hold this value: a boolean, number,
    /// string, Vector3, Vector2, Color3, UDim2, CFrame or enum item.
    /// Property enums count as enum items.
    pub fn is_attribute(&self) -> bool {
        !matches!(self, Value::Instance(_))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ValueKind::Bool => "bool",
            ValueKind::Number => "number",
            ValueKind::String => "string",
            ValueKind::Vector3 => "Vector3",
            ValueKind::Vector2 => "Vector2",
            ValueKind::UDim2 => "UDim2",
            ValueKind::Color3 => "Color3",
            ValueKind::PartType => "PartType",
            ValueKind::Material => "Material",
            ValueKind::TextureChannel => "TextureChannel",
            ValueKind::ParticleType => "ParticleType",
            ValueKind::ToneMapping => "ToneMapping",
            ValueKind::CFrame => "CFrame",
            ValueKind::EnumItem => "EnumItem",
            ValueKind::Instance => "Instance",
        };
        f.write_str(name)
    }
}
