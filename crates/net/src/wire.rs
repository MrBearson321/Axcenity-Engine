//! The game's data on the wire (Phase 64): property values, and the
//! RemoteEvent and RemoteFunction messages scripts send, as JSON next to
//! the handshake's messages.
//!
//! An Instance is its network ID ([`InstanceId::to_bits`]): a replica
//! keeps the server's Instances at their own IDs, so the number names the
//! same Instance on every computer in the game.

use axcenity_core::{
    CFrame, Color3, EnumItem, InstanceId, Material, PartType, ParticleType, PhysicalProperties,
    TextureChannel, ToneMapping, UDim, UDim2, Value, Vector2, Vector3,
};
use axcenity_script::{Message, MessageKind, RemoteValue, Side};
use serde::{Deserialize, Serialize};

/// A property or attribute value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum WireValue {
    Bool(bool),
    Number(f64),
    String(String),
    Vector3([f32; 3]),
    Vector2([f32; 2]),
    /// X scale, X offset, Y scale, Y offset.
    UDim2([f32; 4]),
    Color3([f32; 3]),
    PartType(String),
    Material(String),
    TextureChannel(String),
    ParticleType(String),
    ToneMapping(String),
    CFrame {
        position: [f32; 3],
        rotation: [f32; 4],
    },
    EnumItem {
        enum_name: String,
        item: String,
    },
    Instance(Option<u64>),
    /// Density, friction, elasticity, friction weight, elasticity weight.
    PhysicalProperties(Option<[f32; 5]>),
}

fn triple(v: Vector3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

fn vector3([x, y, z]: [f32; 3]) -> Vector3 {
    Vector3::new(x, y, z)
}

impl WireValue {
    pub fn from_value(value: &Value) -> WireValue {
        match value {
            Value::Bool(b) => WireValue::Bool(*b),
            Value::Number(n) => WireValue::Number(*n),
            Value::String(s) => WireValue::String(s.clone()),
            Value::Vector3(v) => WireValue::Vector3(triple(*v)),
            Value::Vector2(v) => WireValue::Vector2([v.x, v.y]),
            Value::UDim2(u) => WireValue::UDim2([u.x.scale, u.x.offset, u.y.scale, u.y.offset]),
            Value::Color3(c) => WireValue::Color3([c.r, c.g, c.b]),
            Value::PartType(t) => WireValue::PartType(t.as_str().to_owned()),
            Value::Material(m) => WireValue::Material(m.as_str().to_owned()),
            Value::TextureChannel(t) => WireValue::TextureChannel(t.as_str().to_owned()),
            Value::ParticleType(t) => WireValue::ParticleType(t.as_str().to_owned()),
            Value::ToneMapping(t) => WireValue::ToneMapping(t.as_str().to_owned()),
            Value::CFrame(cf) => WireValue::CFrame {
                position: triple(cf.position),
                rotation: cf.rotation,
            },
            Value::EnumItem(item) => WireValue::EnumItem {
                enum_name: item.enum_name.clone(),
                item: item.item.clone(),
            },
            Value::Instance(id) => WireValue::Instance(id.map(InstanceId::to_bits)),
            Value::PhysicalProperties(p) => WireValue::PhysicalProperties(p.map(|p| {
                [
                    p.density,
                    p.friction,
                    p.elasticity,
                    p.friction_weight,
                    p.elasticity_weight,
                ]
            })),
        }
    }

    /// The value, or `None` for an enum item no engine of this version
    /// has.
    pub fn to_value(&self) -> Option<Value> {
        let value = match self {
            WireValue::Bool(b) => Value::Bool(*b),
            WireValue::Number(n) => Value::Number(*n),
            WireValue::String(s) => Value::String(s.clone()),
            WireValue::Vector3(v) => Value::Vector3(vector3(*v)),
            WireValue::Vector2([x, y]) => Value::Vector2(Vector2 { x: *x, y: *y }),
            WireValue::UDim2([xs, xo, ys, yo]) => Value::UDim2(UDim2 {
                x: UDim {
                    scale: *xs,
                    offset: *xo,
                },
                y: UDim {
                    scale: *ys,
                    offset: *yo,
                },
            }),
            WireValue::Color3([r, g, b]) => Value::Color3(Color3::new(*r, *g, *b)),
            WireValue::PartType(name) => Value::PartType(PartType::from_name(name)?),
            WireValue::Material(name) => Value::Material(Material::from_name(name)?),
            WireValue::TextureChannel(name) => {
                Value::TextureChannel(TextureChannel::from_name(name)?)
            }
            WireValue::ParticleType(name) => Value::ParticleType(ParticleType::from_name(name)?),
            WireValue::ToneMapping(name) => Value::ToneMapping(ToneMapping::from_name(name)?),
            WireValue::CFrame { position, rotation } => Value::CFrame(CFrame {
                position: vector3(*position),
                rotation: *rotation,
            }),
            WireValue::EnumItem { enum_name, item } => Value::EnumItem(EnumItem {
                enum_name: enum_name.clone(),
                item: item.clone(),
            }),
            WireValue::Instance(id) => Value::Instance(id.map(InstanceId::from_bits)),
            WireValue::PhysicalProperties(p) => Value::PhysicalProperties(p.map(
                |[
                    density,
                    friction,
                    elasticity,
                    friction_weight,
                    elasticity_weight,
                ]| {
                    PhysicalProperties {
                        density,
                        friction,
                        elasticity,
                        friction_weight,
                        elasticity_weight,
                    }
                },
            )),
        };
        Some(value)
    }
}

/// One argument of a remote message (a [`RemoteValue`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum WireArg {
    Nil,
    Bool(bool),
    Integer(i64),
    Number(f64),
    /// Lua strings are bytes.
    String(Vec<u8>),
    Vector3([f32; 3]),
    Color3([f32; 3]),
    EnumItem {
        enum_name: String,
        item: String,
    },
    Instance(u64),
    Table(Vec<(WireArg, WireArg)>),
}

impl WireArg {
    /// `value` for the wire. `visible` says which Instances the receiver
    /// may know about; any other arrives as nil.
    pub fn from_remote(value: &RemoteValue, visible: &dyn Fn(InstanceId) -> bool) -> WireArg {
        match value {
            RemoteValue::Nil => WireArg::Nil,
            RemoteValue::Bool(b) => WireArg::Bool(*b),
            RemoteValue::Integer(n) => WireArg::Integer(*n),
            RemoteValue::Number(n) => WireArg::Number(*n),
            RemoteValue::String(bytes) => WireArg::String(bytes.clone()),
            RemoteValue::Vector3(v) => WireArg::Vector3(triple(*v)),
            RemoteValue::Color3(c) => WireArg::Color3([c.r, c.g, c.b]),
            RemoteValue::EnumItem { enum_name, item } => WireArg::EnumItem {
                enum_name: (*enum_name).to_owned(),
                item: (*item).to_owned(),
            },
            RemoteValue::Instance(id) if visible(*id) => WireArg::Instance(id.to_bits()),
            RemoteValue::Instance(_) => WireArg::Nil,
            RemoteValue::Table(pairs) => WireArg::Table(
                pairs
                    .iter()
                    .map(|(k, v)| {
                        (
                            WireArg::from_remote(k, visible),
                            WireArg::from_remote(v, visible),
                        )
                    })
                    .collect(),
            ),
        }
    }

    /// The value as it arrives. `allowed` says which Instances the sender
    /// may name; any other (and an unknown enum item) arrives as nil.
    pub fn to_remote(&self, allowed: &dyn Fn(InstanceId) -> bool) -> RemoteValue {
        match self {
            WireArg::Nil => RemoteValue::Nil,
            WireArg::Bool(b) => RemoteValue::Bool(*b),
            WireArg::Integer(n) => RemoteValue::Integer(*n),
            WireArg::Number(n) => RemoteValue::Number(*n),
            WireArg::String(bytes) => RemoteValue::String(bytes.clone()),
            WireArg::Vector3(v) => RemoteValue::Vector3(vector3(*v)),
            WireArg::Color3([r, g, b]) => RemoteValue::Color3(Color3::new(*r, *g, *b)),
            WireArg::EnumItem { enum_name, item } => {
                RemoteValue::enum_item(enum_name, item).unwrap_or(RemoteValue::Nil)
            }
            WireArg::Instance(bits) => {
                let id = InstanceId::from_bits(*bits);
                if allowed(id) {
                    RemoteValue::Instance(id)
                } else {
                    RemoteValue::Nil
                }
            }
            WireArg::Table(pairs) => RemoteValue::Table(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_remote(allowed), v.to_remote(allowed)))
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WireKind {
    Event,
    Invoke { call: u64 },
    Reply { call: u64, error: Option<String> },
}

/// A RemoteEvent firing, or a RemoteFunction's question or answer, between
/// a player's device and the server. Which Player it's from (going to the
/// server) is the connection's, never what the message says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireMessage {
    pub remote: u64,
    /// Going to a player: their Player (theirs to check).
    pub player: u64,
    pub kind: WireKind,
    pub args: Vec<WireArg>,
}

impl WireMessage {
    pub fn from_message(message: &Message, visible: &dyn Fn(InstanceId) -> bool) -> WireMessage {
        WireMessage {
            remote: message.remote.to_bits(),
            player: message.player.to_bits(),
            kind: match &message.kind {
                MessageKind::Event => WireKind::Event,
                MessageKind::Invoke { call } => WireKind::Invoke { call: *call },
                MessageKind::Reply { call, error } => WireKind::Reply {
                    call: *call,
                    error: error.clone(),
                },
            },
            args: message
                .args
                .iter()
                .map(|arg| WireArg::from_remote(arg, visible))
                .collect(),
        }
    }

    /// The message as it arrives on `to`, from (or for) `player`.
    pub fn to_message(
        &self,
        to: Side,
        player: InstanceId,
        allowed: &dyn Fn(InstanceId) -> bool,
    ) -> Message {
        Message {
            to,
            remote: InstanceId::from_bits(self.remote),
            player,
            kind: match &self.kind {
                WireKind::Event => MessageKind::Event,
                WireKind::Invoke { call } => MessageKind::Invoke { call: *call },
                WireKind::Reply { call, error } => MessageKind::Reply {
                    call: *call,
                    error: error.clone(),
                },
            },
            args: self.args.iter().map(|arg| arg.to_remote(allowed)).collect(),
        }
    }
}
