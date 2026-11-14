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
//! RemoteFunctions ask and wait: `remote:InvokeServer(...)` sends an
//! [`MessageKind::Invoke`] and pauses the LocalScript. The server runs its
//! `OnServerInvoke` as a coroutine of its own (so it may wait too), and
//! sends what it returned, or its error, back as a [`MessageKind::Reply`],
//! which resumes the caller. `remote:InvokeClient(player, ...)` asks a
//! player's `OnClientInvoke` the same way (Phase 64).
//!
//! Both sides in one process talk through [`Loopback`]: two queues.
//! Messages for another device (a Player who joined from elsewhere, or the
//! server a replica follows) leave through the runtime's
//! `take_remote_messages`, and the `net` crate carries them.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_core::{ClassName, Color3, InstanceId, Vector3};
use mlua::{FromLua, Function, IntoLua, Lua, MultiValue, Table, Thread, Value};

use crate::client::LocalPlayer;
use crate::events::{self, Owner};
use crate::instance::{LuaInstance, world};
use crate::sides::{self, Side};
use crate::types::{EnumType, LuaColor3, LuaEnumItem, LuaVector3, type_name};
use crate::{bindable, push_error, scheduler};

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
    /// An Instance is its ID, which means the same Instance on the server
    /// and every replica of it (Phase 64).
    Instance(InstanceId),
    /// A table's keys and values, in the order Lua listed them.
    Table(Vec<(RemoteValue, RemoteValue)>),
}

/// A RemoteEvent firing, or a RemoteFunction's question or answer, on its
/// way to the other side.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The side it's going to.
    pub to: Side,
    /// The RemoteEvent or RemoteFunction it's for.
    pub remote: InstanceId,
    /// Going to the server: the Player who sent it. Going to a client: the
    /// Player it's for.
    pub player: InstanceId,
    pub kind: MessageKind,
    /// The event's or the question's arguments, or the answer's values.
    pub args: Vec<RemoteValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageKind {
    /// A RemoteEvent firing.
    Event,
    /// `InvokeServer` asking. `call` numbers it on the asking side, so the
    /// reply finds the script that waits for it.
    Invoke { call: u64 },
    /// The answer to call `call`: the values in `args`, or the error
    /// `OnServerInvoke` raised (`path:line: message`).
    Reply { call: u64, error: Option<String> },
    /// A player asking the server to activate the Ability `remote`
    /// (Phase 76). The runtime takes it before scripts see messages.
    Activate,
}

impl RemoteValue {
    /// The enum item `enum_name.item`, if scripts know it (a message read
    /// from the network names it).
    pub fn enum_item(enum_name: &str, item: &str) -> Option<RemoteValue> {
        let enum_type = EnumType::ALL
            .into_iter()
            .find(|enum_type| enum_type.name() == enum_name)?;
        let found = LuaEnumItem::find(enum_type, item)?;
        Some(RemoteValue::EnumItem {
            enum_name: enum_type.name(),
            item: found.name,
        })
    }
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
/// for each message, runs `OnServerInvoke` for each question and resumes
/// the caller of each answer. Messages for a remote that's gone are
/// dropped, and so are messages with nothing connected to receive them. A
/// question for a RemoteFunction without `OnServerInvoke` waits until a
/// script sets it, as in Roblox.
pub fn deliver(lua: &Lua, messages: Vec<Message>) {
    let waiting = lua
        .app_data_mut::<RemoteFunctions>()
        .map(|mut functions| std::mem::take(&mut functions.unanswered))
        .unwrap_or_default();
    for message in waiting.into_iter().chain(messages) {
        let result = match message.kind {
            MessageKind::Event => deliver_one(lua, &message),
            MessageKind::Invoke { call } => serve(lua, &message, call),
            MessageKind::Reply { call, ref error } => answer(lua, &message, call, error.clone()),
            MessageKind::Activate => Ok(()),
        };
        if let Err(err) = result {
            push_error(lua, &err);
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
        Side::Client if is_mine(lua, message) => "OnClientEvent",
        Side::Client => return Ok(()),
    };
    for arg in &message.args {
        args.push(from_remote(lua, arg)?);
    }
    let owner = Owner::Instance(message.remote);
    // A RemoteEvent in a Tool with `LagCompensated` on: the server's
    // raycasts in its handlers (until they first wait) are the sender's
    // shots (Phase 81).
    let compensated = message.to == Side::Server && in_compensated_tool(lua, message.remote)?;
    if compensated {
        lua.set_app_data(crate::lag::Compensating(Some(message.player)));
    }
    events::fire(lua, owner, event, MultiValue::from_vec(args));
    if compensated {
        lua.remove_app_data::<crate::lag::Compensating>();
    }
    Ok(())
}

/// True if `remote` is inside a Tool with `LagCompensated` on.
fn in_compensated_tool(lua: &Lua, remote: InstanceId) -> mlua::Result<bool> {
    let world = world(lua)?;
    let mut at = world.get(remote).and_then(|instance| instance.parent());
    while let Some(id) = at {
        let Some(instance) = world.get(id) else {
            break;
        };
        if instance.class() == ClassName::Tool {
            return Ok(
                instance.property("LagCompensated") == Some(&axcenity_core::Value::Bool(true))
            );
        }
        at = instance.parent();
    }
    Ok(false)
}

/// True if `message` is for the player on this device.
fn is_mine(lua: &Lua, message: &Message) -> bool {
    lua.app_data_ref::<LocalPlayer>()
        .is_some_and(|me| me.0 == message.player)
}

/// The RemoteEvent methods, made once per interpreter: `FireServer`,
/// `FireClient` and `FireAllClients`, and the RemoteFunction's
/// `InvokeServer`.
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
            let player = player_arg(lua, "FireClient", &player)?;
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

    let functions = RemoteFunctions::new(lua)?;
    let invoke_server = functions.invoke_server.clone();
    let invoke_client = functions.invoke_client.clone();
    lua.set_app_data(functions);

    Ok(vec![
        (ClassName::RemoteEvent, "FireServer", fire_server),
        (ClassName::RemoteEvent, "FireClient", fire_client),
        (ClassName::RemoteEvent, "FireAllClients", fire_all_clients),
        (ClassName::RemoteFunction, "InvokeServer", invoke_server),
        (ClassName::RemoteFunction, "InvokeClient", invoke_client),
    ])
}

fn send(lua: &Lua, to: Side, remote: InstanceId, player: InstanceId, args: Vec<RemoteValue>) {
    send_kind(lua, to, remote, player, MessageKind::Event, args);
}

fn send_kind(
    lua: &Lua,
    to: Side,
    remote: InstanceId,
    player: InstanceId,
    kind: MessageKind,
    args: Vec<RemoteValue>,
) {
    if let Some(mut outbox) = lua.app_data_mut::<Outbox>() {
        outbox.0.push(Message {
            to,
            remote,
            player,
            kind,
            args,
        });
    }
}

/// `InvokeServer` must pause the LocalScript until the answer comes, and
/// only Lua can pause (as for `task.wait`). `ask` (Rust) sends the question
/// and remembers the coroutine; the answer resumes it with `true` and the
/// values, or `false` and the other side's error, which is raised here as
/// it is, so it names that script's line. `InvokeClient` is the same.
const INVOKE: &str = r#"
local running, yield, error, ask, method = ...

local function finish(ok, ...)
    if ok then
        return ...
    end
    error((...), 0)
end

return function(remote, ...)
    local thread, is_main = running()
    if is_main then
        error(method .. " can only pause a script", 2)
    end
    ask(thread, remote, ...)
    return finish(yield())
end
"#;

/// Runs `OnServerInvoke` on the server (with the Player who asked first)
/// or `OnClientInvoke` on a client, as a coroutine of its own, and sends
/// back what it returns or the error it raises. `pcall` is the engine's,
/// which passes errors on as `path:line: message` and lets the handler
/// wait.
const SERVE: &str = r#"
local pcall, reply = ...

return function(callback, remote, call, player, with_player, ...)
    if with_player then
        reply(remote, call, player, pcall(callback, player, ...))
    else
        reply(remote, call, player, pcall(callback, ...))
    end
end
"#;

/// RemoteFunction calls on one side: the questions this side is waiting to
/// hear back about, and questions that came before their `OnServerInvoke`
/// (or `OnClientInvoke`) was set.
pub struct RemoteFunctions {
    invoke_server: Function,
    invoke_client: Function,
    serve: Function,
    next_call: u64,
    /// Each waiting call's coroutine and the script it belongs to.
    waiting: BTreeMap<u64, (Thread, Option<InstanceId>)>,
    unanswered: Vec<Message>,
}

impl RemoteFunctions {
    fn new(lua: &Lua) -> mlua::Result<Self> {
        let coroutine: Table = lua.globals().raw_get("coroutine")?;
        let ask_server =
            lua.create_function(|lua, (thread, this, args): (Thread, Value, MultiValue)| {
                let remote = function_self(lua, "InvokeServer", this)?;
                let player = lua
                    .app_data_ref::<LocalPlayer>()
                    .map(|player| player.0)
                    .ok_or_else(|| {
                        mlua::Error::runtime("InvokeServer: this player hasn't joined yet")
                    })?;
                let args = remote_args(lua, "InvokeServer", args)?;
                ask(lua, thread, Side::Server, remote, player, args)
            })?;
        let ask_client = lua.create_function(
            |lua, (thread, this, player, args): (Thread, Value, Value, MultiValue)| {
                let remote = function_self(lua, "InvokeClient", this)?;
                let player = player_arg(lua, "InvokeClient", &player)?;
                let args = remote_args(lua, "InvokeClient", args)?;
                ask(lua, thread, Side::Client, remote, player, args)
            },
        )?;
        let invoke = |method: &str, ask: Function| -> mlua::Result<Function> {
            lua.load(INVOKE).set_name("=[engine]").call((
                coroutine.raw_get::<Function>("running")?,
                coroutine.raw_get::<Function>("yield")?,
                lua.globals().raw_get::<Function>("error")?,
                ask,
                method,
            ))
        };
        let invoke_server = invoke("InvokeServer", ask_server)?;
        let invoke_client = invoke("InvokeClient", ask_client)?;

        let reply = lua.create_function(
            |lua,
             (remote, call, player, ok, values): (
                LuaInstance,
                u64,
                LuaInstance,
                bool,
                MultiValue,
            )| {
                let side = lua
                    .app_data_ref::<Side>()
                    .map_or(Side::Server, |side| *side);
                let (to, member) = match side {
                    Side::Server => (Side::Client, "OnServerInvoke"),
                    Side::Client => (Side::Server, "OnClientInvoke"),
                };
                let (error, args) = if ok {
                    match remote_args(lua, member, values) {
                        Ok(args) => (None, args),
                        Err(err) => (Some(crate::errors::error_message(&err)), Vec::new()),
                    }
                } else {
                    let message = values.into_iter().next().unwrap_or(Value::Nil);
                    let message = match lua.coerce_string(message.clone())? {
                        Some(text) => text.to_string_lossy(),
                        None => format!("{member} raised a {}", type_name(lua, &message)),
                    };
                    (Some(message), Vec::new())
                };
                send_kind(
                    lua,
                    to,
                    remote.0,
                    player.0,
                    MessageKind::Reply { call, error },
                    args,
                );
                Ok(())
            },
        )?;
        let pcall = lua
            .app_data_ref::<crate::sandbox::Shared>()
            .map(|shared| shared.pcall().clone())
            .ok_or_else(|| mlua::Error::runtime("the script functions aren't ready"))?;
        let serve = lua.load(SERVE).set_name("=[engine]").call((pcall, reply))?;
        Ok(Self {
            invoke_server,
            invoke_client,
            serve,
            next_call: 0,
            waiting: BTreeMap::new(),
            unanswered: Vec::new(),
        })
    }

    /// Forgets the calls `script` waits on, so their answers resume nothing.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.waiting.retain(|_, (_, owner)| *owner != Some(script));
    }
}

/// Sends a question to `to` and remembers the coroutine waiting for it.
fn ask(
    lua: &Lua,
    thread: Thread,
    to: Side,
    remote: InstanceId,
    player: InstanceId,
    args: Vec<RemoteValue>,
) -> mlua::Result<()> {
    let script = scheduler::running_script(lua);
    let call = {
        let mut functions = functions_mut(lua)?;
        let call = functions.next_call;
        functions.next_call += 1;
        functions.waiting.insert(call, (thread.clone(), script));
        call
    };
    crate::debugger::waiting(lua, &thread);
    send_kind(lua, to, remote, player, MessageKind::Invoke { call }, args);
    Ok(())
}

fn functions_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, RemoteFunctions>> {
    lua.app_data_mut::<RemoteFunctions>()
        .ok_or_else(|| mlua::Error::runtime("RemoteFunctions aren't ready"))
}

/// A question arrived: runs the RemoteFunction's `OnServerInvoke` with the
/// Player who asked and the arguments on the server, or its
/// `OnClientInvoke` with the arguments on the player it's for.
fn serve(lua: &Lua, message: &Message, call: u64) -> mlua::Result<()> {
    if !world(lua)?.contains(message.remote) {
        return Ok(());
    }
    let member = match message.to {
        Side::Server => "OnServerInvoke",
        Side::Client if is_mine(lua, message) => "OnClientInvoke",
        Side::Client => return Ok(()),
    };
    let Some((callback, script)) = bindable::callback(lua, message.remote, member) else {
        functions_mut(lua)?.unanswered.push(message.clone());
        return Ok(());
    };
    let serve = functions_mut(lua)?.serve.clone();
    let mut args = vec![
        Value::Function(callback),
        LuaInstance(message.remote).into_lua(lua)?,
        call.into_lua(lua)?,
        LuaInstance(message.player).into_lua(lua)?,
        Value::Boolean(message.to == Side::Server),
    ];
    for arg in &message.args {
        args.push(from_remote(lua, arg)?);
    }
    let thread = lua.create_thread(serve)?;
    scheduler::resume_as(lua, &thread, MultiValue::from_vec(args), script);
    Ok(())
}

/// An answer arrived: resumes the script waiting in `InvokeServer` or
/// `InvokeClient`.
fn answer(lua: &Lua, message: &Message, call: u64, error: Option<String>) -> mlua::Result<()> {
    if message.to == Side::Client && !is_mine(lua, message) {
        return Ok(());
    }
    let Some((thread, script)) = functions_mut(lua)?.waiting.remove(&call) else {
        return Ok(());
    };
    let mut values = Vec::new();
    match error {
        Some(error) => {
            values.push(Value::Boolean(false));
            values.push(error.into_lua(lua)?);
        }
        None => {
            values.push(Value::Boolean(true));
            for arg in &message.args {
                values.push(from_remote(lua, arg)?);
            }
        }
    }
    scheduler::resume_as(lua, &thread, MultiValue::from_vec(values), script);
    Ok(())
}

/// The RemoteFunction a method was called on, as [`remote_self`] checks.
fn function_self(lua: &Lua, method: &str, this: Value) -> mlua::Result<InstanceId> {
    instance_self(lua, method, ClassName::RemoteFunction, "remote", this)
}

/// The RemoteEvent a method was called on. Calling with `.` instead of `:`
/// leaves it out, which is an easy mistake, so the message says so.
fn remote_self(lua: &Lua, method: &str, this: Value) -> mlua::Result<InstanceId> {
    instance_self(lua, method, ClassName::RemoteEvent, "remote", this)
}

/// The Instance of `class` a method was called on, or an error showing how
/// to call it with a colon (`variable:method(...)`).
pub(crate) fn instance_self(
    lua: &Lua,
    method: &str,
    class: ClassName,
    variable: &str,
    this: Value,
) -> mlua::Result<InstanceId> {
    sides::check(lua, method)?;
    let is_remote = |id: InstanceId| {
        world(lua).is_ok_and(|world| world.get(id).is_some_and(|i| i.class() == class))
    };
    match LuaInstance::from_lua(this, lua) {
        Ok(LuaInstance(id)) if is_remote(id) => Ok(id),
        _ => Err(mlua::Error::runtime(format!(
            "call {method} with a colon: {variable}:{method}(...)"
        ))),
    }
}

/// The Player `FireClient` (or `InvokeClient`) sends to.
fn player_arg(lua: &Lua, method: &str, value: &Value) -> mlua::Result<InstanceId> {
    if let Ok(LuaInstance(id)) = LuaInstance::from_lua(value.clone(), lua)
        && world(lua)?
            .get(id)
            .is_some_and(|i| i.class() == ClassName::Player)
    {
        return Ok(id);
    }
    Err(mlua::Error::runtime(format!(
        "{method} needs the Player to send to first, not a {}",
        type_name(lua, value)
    )))
}

/// How deep tables may nest in what a RemoteEvent or RemoteFunction sends:
/// a table in an argument is 1 deep, a table in that 2, and so on. The
/// sender raises an error past it, and the receiving device drops a
/// message nested deeper as it reads it (multiplayer readiness MP3).
pub const MAX_TABLE_DEPTH: usize = 32;

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
            if tables.len() > MAX_TABLE_DEPTH {
                return Err(mlua::Error::runtime(format!(
                    "{method} can't send tables nested more than {MAX_TABLE_DEPTH} deep"
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
