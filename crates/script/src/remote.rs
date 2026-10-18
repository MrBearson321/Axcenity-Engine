//! RemoteEvents: messages between Scripts (the server) and LocalScripts
//! (the player's side).
//!
//! `remote:FireServer(...)` and `remote:FireClient(player, ...)` turn their
//! arguments into [`RemoteValue`]s (plain data, no Lua values, so a message
//! could cross a real network) and put a [`Message`] in the sending side's
//! `Outbox`. The runtime hands those to a [`Transport`], and at the next
//! frame step 2 the other side takes its messages and fires `OnServerEvent`
//! or `OnClientEvent`.
//!
//! v1 runs both sides in one process, so the transport is [`Loopback`]: two
//! queues. A real network only needs another [`Transport`].

use std::collections::BTreeSet;

use axcenity_core::{ClassName, Color3, InstanceId, Vector3};
use mlua::{FromLua, Function, IntoLua, Lua, MultiValue, Value};

use crate::client::LocalPlayer;
use crate::errors::error_message;
use crate::events::{self, Owner};
use crate::instance::{LuaInstance, world};
use crate::sides::{self, Side};
use crate::types::{EnumType, LuaColor3, LuaEnumItem, LuaVector3, type_name};
use crate::{OutputKind, push_output};

/// One argument of a message, as plain data.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteValue {
    Nil,
    Bool(bool),
    /// A whole Lua number, kept apart from `Number` so `1` arrives as `1`, not `1.0`.
    Integer(i64),
    Number(f64),
    /// Lua strings are bytes, and may not be valid text.
    String(Vec<u8>),
    Vector3(Vector3),
    Color3(Color3),
    /// An enum item by its names, such as `("KeyCode", "W")`.
    EnumItem {
        enum_name: &'static str,
        item: &'static str,
    },
    /// Both sides share one World in v1, so an Instance is its ID.
    Instance(InstanceId),
    /// A table's keys and values, in the order Lua listed them.
    Table(Vec<(RemoteValue, RemoteValue)>),
}

/// A RemoteEvent firing, on its way to the other side.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The side it's going to.
    pub to: Side,
    /// The RemoteEvent that was fired.
    pub remote: InstanceId,
    /// Going to the server: the Player who sent it. Going to a client: the
    /// Player it's for.
    pub player: InstanceId,
    pub args: Vec<RemoteValue>,
}

/// Carries messages between the server and the clients. The runtime only
/// ever talks to this trait, so the loopback can be swapped for a network.
pub trait Transport {
    /// Sends `message` toward `message.to`.
    fn send(&mut self, message: Message);
    /// Takes every message that has arrived for `side`, oldest first.
    fn receive(&mut self, side: Side) -> Vec<Message>;
}

/// Both sides in one process: a message is simply kept until the other side
/// asks for it.
#[derive(Default)]
pub struct Loopback {
    to_server: Vec<Message>,
    to_client: Vec<Message>,
}

impl Transport for Loopback {
    fn send(&mut self, message: Message) {
        match message.to {
            Side::Server => self.to_server.push(message),
            Side::Client => self.to_client.push(message),
        }
    }

    fn receive(&mut self, side: Side) -> Vec<Message> {
        match side {
            Side::Server => std::mem::take(&mut self.to_server),
            Side::Client => std::mem::take(&mut self.to_client),
        }
    }
}

/// The messages one side has sent since the runtime last collected them.
#[derive(Default)]
pub struct Outbox(pub Vec<Message>);

/// Fires `OnServerEvent` (on the server) or `OnClientEvent` (on a client)
/// for each message. Messages for a RemoteEvent that's gone are dropped,
/// and so are messages with nothing connected to receive them.
pub fn deliver(lua: &Lua, messages: Vec<Message>) {
    for message in messages {
        if let Err(err) = deliver_one(lua, &message) {
            push_output(lua, OutputKind::Error, error_message(&err));
        }
    }
}

fn deliver_one(lua: &Lua, message: &Message) -> mlua::Result<()> {
    if !world(lua)?.contains(message.remote) {
        return Ok(());
    }
    let mut args = Vec::new();
    let event = match message.to {
        // The server's handlers get who sent it first, as in Roblox.
        Side::Server => {
            args.push(LuaInstance(message.player).into_lua(lua)?);
            "OnServerEvent"
        }
        Side::Client => {
            let mine = lua
                .app_data_ref::<LocalPlayer>()
                .is_some_and(|me| me.0 == message.player);
            if !mine {
                return Ok(());
            }
            "OnClientEvent"
        }
    };
    for arg in &message.args {
        args.push(from_remote(lua, arg)?);
    }
    let owner = Owner::Instance(message.remote);
    events::fire(lua, owner, event, MultiValue::from_vec(args));
    Ok(())
}

/// The RemoteEvent methods, made once per interpreter: `FireServer`,
/// `FireClient` and `FireAllClients`.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let fire_server = lua.create_function(|lua, (this, args): (Value, MultiValue)| {
        let remote = remote_self(lua, "FireServer", this)?;
        let player = lua
            .app_data_ref::<LocalPlayer>()
            .map(|player| player.0)
            .ok_or_else(|| mlua::Error::runtime("FireServer: this player hasn't joined yet"))?;
        let args = remote_args(lua, "FireServer", args)?;
        send(lua, Side::Server, remote, player, args);
        Ok(())
    })?;

    let fire_client =
        lua.create_function(|lua, (this, player, args): (Value, Value, MultiValue)| {
            let remote = remote_self(lua, "FireClient", this)?;
            let player = player_arg(lua, &player)?;
            let args = remote_args(lua, "FireClient", args)?;
            send(lua, Side::Client, remote, player, args);
            Ok(())
        })?;

    let fire_all_clients = lua.create_function(|lua, (this, args): (Value, MultiValue)| {
        let remote = remote_self(lua, "FireAllClients", this)?;
        let args = remote_args(lua, "FireAllClients", args)?;
        let players: Vec<InstanceId> = {
            let world = world(lua)?;
            let players = world.players();
            world.get(players).map_or_else(Vec::new, |players| {
                players
                    .children()
                    .iter()
                    .copied()
                    .filter(|&id| {
                        world
                            .get(id)
                            .is_some_and(|p| p.class() == ClassName::Player)
                    })
                    .collect()
            })
        };
        for player in players {
            send(lua, Side::Client, remote, player, args.clone());
        }
        Ok(())
    })?;

    Ok(vec![
        (ClassName::RemoteEvent, "FireServer", fire_server),
        (ClassName::RemoteEvent, "FireClient", fire_client),
        (ClassName::RemoteEvent, "FireAllClients", fire_all_clients),
    ])
}

fn send(lua: &Lua, to: Side, remote: InstanceId, player: InstanceId, args: Vec<RemoteValue>) {
    if let Some(mut outbox) = lua.app_data_mut::<Outbox>() {
        outbox.0.push(Message {
            to,
            remote,
            player,
            args,
        });
    }
}

/// The RemoteEvent a method was called on. Calling with `.` instead of `:`
/// leaves it out, which is an easy mistake, so the message says so.
fn remote_self(lua: &Lua, method: &str, this: Value) -> mlua::Result<InstanceId> {
    sides::check(lua, method)?;
    let is_remote = |id: InstanceId| {
        world(lua).is_ok_and(|world| {
            world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::RemoteEvent)
        })
    };
    match LuaInstance::from_lua(this, lua) {
        Ok(LuaInstance(id)) if is_remote(id) => Ok(id),
        _ => Err(mlua::Error::runtime(format!(
            "call {method} with a colon: remote:{method}(...)"
        ))),
    }
}

/// The Player `FireClient` sends to.
fn player_arg(lua: &Lua, value: &Value) -> mlua::Result<InstanceId> {
    if let Ok(LuaInstance(id)) = LuaInstance::from_lua(value.clone(), lua)
        && world(lua)?
            .get(id)
            .is_some_and(|i| i.class() == ClassName::Player)
    {
        return Ok(id);
    }
    Err(mlua::Error::runtime(format!(
        "FireClient needs the Player to send to first, not a {}",
        type_name(lua, value)
    )))
}

fn remote_args(lua: &Lua, method: &str, args: MultiValue) -> mlua::Result<Vec<RemoteValue>> {
    args.into_iter()
        .map(|arg| to_remote(lua, method, &arg, &mut BTreeSet::new()))
        .collect()
}

/// `value` as plain data. `tables` holds the tables being converted around
/// it, to catch a table that contains itself.
fn to_remote(
    lua: &Lua,
    method: &str,
    value: &Value,
    tables: &mut BTreeSet<usize>,
) -> mlua::Result<RemoteValue> {
    let converted = match value {
        Value::Nil => RemoteValue::Nil,
        Value::Boolean(b) => RemoteValue::Bool(*b),
        Value::Integer(n) => RemoteValue::Integer(*n),
        Value::Number(n) => RemoteValue::Number(*n),
        Value::String(s) => RemoteValue::String(s.as_bytes().to_vec()),
        Value::Table(table) => {
            // The table's address, only to recognize it again.
            let address = table.to_pointer() as usize;
            if !tables.insert(address) {
                return Err(mlua::Error::runtime(format!(
                    "{method} can't send a table that contains itself"
                )));
            }
            let mut pairs = Vec::new();
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;
                pairs.push((
                    to_remote(lua, method, &key, tables)?,
                    to_remote(lua, method, &value, tables)?,
                ));
            }
            tables.remove(&address);
            RemoteValue::Table(pairs)
        }
        Value::UserData(data) => {
            if let Ok(v) = data.borrow::<LuaVector3>() {
                RemoteValue::Vector3(v.0)
            } else if let Ok(c) = data.borrow::<LuaColor3>() {
                RemoteValue::Color3(c.0)
            } else if let Ok(item) = data.borrow::<LuaEnumItem>() {
                RemoteValue::EnumItem {
                    enum_name: item.enum_type.name(),
                    item: item.name,
                }
            } else if let Ok(instance) = data.borrow::<LuaInstance>() {
                RemoteValue::Instance(instance.0)
            } else {
                return Err(cannot_send(lua, method, value));
            }
        }
        _ => return Err(cannot_send(lua, method, value)),
    };
    Ok(converted)
}

fn cannot_send(lua: &Lua, method: &str, value: &Value) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{method} can't send a {}; send numbers, strings, booleans, tables, Vector3s, \
         Color3s, enum items or Instances",
        type_name(lua, value)
    ))
}

/// Plain data back as a Lua value, on the receiving side. An Instance that
/// has been destroyed meanwhile arrives as nil.
fn from_remote(lua: &Lua, value: &RemoteValue) -> mlua::Result<Value> {
    match value {
        RemoteValue::Nil => Ok(Value::Nil),
        RemoteValue::Bool(b) => Ok(Value::Boolean(*b)),
        RemoteValue::Integer(n) => Ok(Value::Integer(*n)),
        RemoteValue::Number(n) => Ok(Value::Number(*n)),
        RemoteValue::String(bytes) => lua.create_string(bytes).map(Value::String),
        RemoteValue::Vector3(v) => LuaVector3(*v).into_lua(lua),
        RemoteValue::Color3(c) => LuaColor3(*c).into_lua(lua),
        RemoteValue::EnumItem { enum_name, item } => EnumType::ALL
            .into_iter()
            .find(|enum_type| enum_type.name() == *enum_name)
            .and_then(|enum_type| LuaEnumItem::find(enum_type, item))
            .into_lua(lua),
        RemoteValue::Instance(id) => {
            let exists = world(lua)?.contains(*id);
            exists.then_some(LuaInstance(*id)).into_lua(lua)
        }
        RemoteValue::Table(pairs) => {
            let table = lua.create_table()?;
            for (key, value) in pairs {
                table.raw_set(from_remote(lua, key)?, from_remote(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
