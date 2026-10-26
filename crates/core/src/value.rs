//! Property values. Every Instance property holds one [`Value`], and its
//! kind never changes after the Instance is created.

use std::fmt;

use crate::enums::{Material, PartType, ParticleType, TextureChannel};

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
        };
        f.write_str(name)
    }
}
