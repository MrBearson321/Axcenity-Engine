//! `game:GetService("NetworkService")` (Phase 63): hosting a game, joining
//! one and finding the games on the local network, so a menu can do them.
//!
//! - `Host(port?, password?)` starts a listen server on this device;
//!   `Join(address, password?)` joins one at `"address:port"`;
//!   `Disconnect()` leaves (or stops hosting).
//! - `FindLanGames()` pauses the script, as `task.wait` does, while the
//!   host asks the local network, then returns a list of tables:
//!   `{ Name, Version, Address, Players, MaxPlayers, HasPassword }`.
//! - `ConnectionFailed(reason)` fires when hosting or joining fails, a join
//!   is refused (a version mismatch, a wrong password, a full game) or a
//!   connection is lost.
//!
//! Scripts don't touch the network themselves: each call queues a
//! [`NetworkRequest`] for the host (the player, or `axcenity-server`),
//! which takes them with
//! [`ScriptRuntime::take_network_requests`](crate::ScriptRuntime::take_network_requests)
//! and answers with `finish_lan_search` and `fire_connection_failed`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use axcenity_core::{InstanceId, Vector3};
use mlua::{
    FromLua, Function, Lua, MetaMethod, Table, Thread, UserData, UserDataFields, UserDataMethods,
};

use crate::events::{LuaSignal, Owner};
use crate::instance::LuaInstance;
use crate::scheduler;
use crate::types::{LuaVector3, type_name};
use crate::{OutputKind, push_output};

/// What a script asked of the network, for the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum NetworkRequest {
    /// `Host(port, password)`: `None` for the default port.
    Host {
        port: Option<u16>,
        password: Option<String>,
    },
    /// `Join(address, password)`, the address as the script gave it.
    Join {
        address: String,
        password: Option<String>,
    },
    /// `Disconnect()`.
    Disconnect,
    /// `FindLanGames()`: a script waits for
    /// [`ScriptRuntime::finish_lan_search`](crate::ScriptRuntime::finish_lan_search).
    FindLanGames,
    /// `Player:RequestStreamAroundAsync(position)` (Phase 80): a script
    /// waits for
    /// [`ScriptRuntime::finish_stream_around`](crate::ScriptRuntime::finish_stream_around)
    /// with this `request`.
    StreamAround {
        player: InstanceId,
        position: Vector3,
        request: u64,
    },
}

/// A game found on the local network, as `FindLanGames` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanGame {
    pub name: String,
    pub version: String,
    /// `"address:port"`, ready for `Join`.
    pub address: String,
    pub players: u32,
    pub max_players: u32,
    pub has_password: bool,
}

/// The requests one side's scripts made since the host last took them.
#[derive(Default)]
pub(crate) struct NetworkRequests(pub Vec<NetworkRequest>);

/// The scripts paused in `FindLanGames`, each with the script it runs as.
#[derive(Default)]
pub(crate) struct LanSearches(Vec<(Thread, Option<InstanceId>)>);

impl LanSearches {
    /// Forgets the searches `script` waits on, so the answer resumes nothing.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.0.retain(|(_, owner)| *owner != Some(script));
    }
}

/// Scripts paused in `RequestStreamAroundAsync`, by request number, each
/// with the script it runs as.
#[derive(Default)]
pub(crate) struct StreamWaits(HashMap<u64, (Thread, Option<InstanceId>)>);

impl StreamWaits {
    pub fn forget_script(&mut self, script: InstanceId) {
        self.0.retain(|_, (_, owner)| *owner != Some(script));
    }
}

/// What this device doesn't have (streaming, Phase 80): the Instances
/// taken off it, and the Models that stayed without their contents. They
/// read `StreamingState` as StreamedOut, and handlers on them are kept.
#[derive(Default)]
pub(crate) struct StreamedOut(pub HashSet<InstanceId>);

/// Whether `id` isn't on this device: streamed out, or destroyed.
pub(crate) fn is_streamed_out(lua: &Lua, id: InstanceId, exists: bool) -> bool {
    !exists
        || lua
            .app_data_ref::<StreamedOut>()
            .is_some_and(|out| out.0.contains(&id))
}

/// Request numbers, unique in this process.
static NEXT_STREAM_REQUEST: AtomicU64 = AtomicU64::new(1);

/// Pauses the script until the host has loaded the region.
const STREAM_AROUND: &str = r#"
local running, yield, error, start = ...
return function(player, position)
    local thread, is_main = running()
    if is_main then
        error("RequestStreamAroundAsync can only pause a script", 2)
    end
    start(player, position, thread)
    return yield()
end
"#;

const STREAM_AROUND_KEY: &str = "axcenity.network.stream_around";

/// The function behind `Player:RequestStreamAroundAsync`, made once.
pub(crate) fn stream_around_function(lua: &Lua) -> mlua::Result<Function> {
    if let Ok(function) = lua.named_registry_value::<Function>(STREAM_AROUND_KEY) {
        return Ok(function);
    }
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let start = lua.create_function(
        |lua, (player, position, thread): (LuaInstance, mlua::Value, Thread)| {
            let position = LuaVector3::from_lua(position.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "RequestStreamAroundAsync: the position must be a Vector3, not a {}",
                    type_name(lua, &position)
                ))
            })?;
            crate::debugger::waiting(lua, &thread);
            let script = scheduler::running_script(lua);
            let id = NEXT_STREAM_REQUEST.fetch_add(1, Ordering::Relaxed);
            if lua.app_data_ref::<StreamWaits>().is_none() {
                lua.set_app_data(StreamWaits::default());
            }
            if let Some(mut waits) = lua.app_data_mut::<StreamWaits>() {
                waits.0.insert(id, (thread, script));
            }
            request(
                lua,
                NetworkRequest::StreamAround {
                    player: player.0,
                    position: position.0,
                    request: id,
                },
            );
            Ok(())
        },
    )?;
    let function: Function = lua.load(STREAM_AROUND).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("running")?,
        coroutine.raw_get::<Function>("yield")?,
        lua.globals().raw_get::<Function>("error")?,
        start,
    ))?;
    lua.set_named_registry_value(STREAM_AROUND_KEY, function.clone())?;
    Ok(function)
}

/// Resumes the script waiting on `request`, if it's on this side.
pub(crate) fn finish_stream_around(lua: &Lua, request: u64) {
    let waiting = lua
        .app_data_mut::<StreamWaits>()
        .and_then(|mut waits| waits.0.remove(&request));
    if let Some((thread, script)) = waiting {
        scheduler::resume_as(lua, &thread, mlua::Value::Nil, script);
    }
}

/// Pauses the script until the host has searched; `start` notes the thread.
const FIND: &str = r#"
local running, yield, error, start = ...
return function(_service)
    local thread, is_main = running()
    if is_main then
        error("FindLanGames can only pause a script", 2)
    end
    start(thread)
    return yield()
end
"#;

const FIND_KEY: &str = "axcenity.network.find_lan_games";

fn request(lua: &Lua, request: NetworkRequest) {
    if lua.app_data_ref::<NetworkRequests>().is_none() {
        lua.set_app_data(NetworkRequests::default());
    }
    if let Some(mut requests) = lua.app_data_mut::<NetworkRequests>() {
        requests.0.push(request);
    }
}

/// The requests waiting on this side, oldest first.
pub(crate) fn take_requests(lua: &Lua) -> Vec<NetworkRequest> {
    lua.app_data_mut::<NetworkRequests>()
        .map(|mut requests| std::mem::take(&mut requests.0))
        .unwrap_or_default()
}

/// Resumes every script waiting in `FindLanGames` with `games`.
pub(crate) fn finish_search(lua: &Lua, games: &[LanGame]) {
    let waiting = lua
        .app_data_mut::<LanSearches>()
        .map(|mut searches| std::mem::take(&mut searches.0))
        .unwrap_or_default();
    for (thread, script) in waiting {
        // Each script gets a list of its own to change.
        let list = match games_table(lua, games) {
            Ok(list) => mlua::Value::Table(list),
            Err(err) => {
                push_output(lua, OutputKind::Error, format!("FindLanGames: {err}"));
                mlua::Value::Nil
            }
        };
        scheduler::resume_as(lua, &thread, list, script);
    }
}

fn games_table(lua: &Lua, games: &[LanGame]) -> mlua::Result<Table> {
    let list = lua.create_table()?;
    for game in games {
        let entry = lua.create_table()?;
        entry.raw_set("Name", game.name.as_str())?;
        entry.raw_set("Version", game.version.as_str())?;
        entry.raw_set("Address", game.address.as_str())?;
        entry.raw_set("Players", game.players)?;
        entry.raw_set("MaxPlayers", game.max_players)?;
        entry.raw_set("HasPassword", game.has_password)?;
        list.raw_push(entry)?;
    }
    Ok(list)
}

/// The Lua function `FindLanGames` is, made the first time it's asked for.
fn find_function(lua: &Lua) -> mlua::Result<Function> {
    if let Ok(function) = lua.named_registry_value::<Function>(FIND_KEY) {
        return Ok(function);
    }
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let start = lua.create_function(|lua, thread: Thread| {
        crate::debugger::waiting(lua, &thread);
        let script = scheduler::running_script(lua);
        if lua.app_data_ref::<LanSearches>().is_none() {
            lua.set_app_data(LanSearches::default());
        }
        if let Some(mut searches) = lua.app_data_mut::<LanSearches>() {
            searches.0.push((thread, script));
        }
        request(lua, NetworkRequest::FindLanGames);
        Ok(())
    })?;
    let function: Function = lua.load(FIND).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("running")?,
        coroutine.raw_get::<Function>("yield")?,
        lua.globals().raw_get::<Function>("error")?,
        start,
    ))?;
    lua.set_named_registry_value(FIND_KEY, function.clone())?;
    Ok(function)
}

/// An optional password argument: `nil` or text.
fn password(lua: &Lua, value: &mlua::Value, call: &str) -> mlua::Result<Option<String>> {
    match value {
        mlua::Value::Nil => Ok(None),
        mlua::Value::String(text) => {
            let text = text.to_string_lossy();
            Ok((!text.is_empty()).then_some(text))
        }
        other => Err(mlua::Error::runtime(format!(
            "{call}'s password is text, such as \"secret\", not a {}",
            type_name(lua, other)
        ))),
    }
}

/// `NetworkService`, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaNetworkService;

impl UserData for LuaNetworkService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("ConnectionFailed", |_, _| {
            Ok(LuaSignal::named(Owner::NetworkService, "ConnectionFailed"))
        });
        // A field holding a function: `NetworkService:FindLanGames()` calls
        // it with the service, as a method. Only a Lua function can pause.
        fields.add_field_method_get("FindLanGames", |lua, _| find_function(lua));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "Host",
            |lua, _, (port, secret): (mlua::Value, mlua::Value)| {
                let port = match &port {
                    mlua::Value::Nil => None,
                    mlua::Value::Integer(port) => Some(*port as f64),
                    mlua::Value::Number(port) => Some(*port),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "Host's port is a number from 1 to 65535, such as \
                             NetworkService:Host(47800), not a {}",
                            type_name(lua, other)
                        )));
                    }
                };
                let port = match port {
                    None => None,
                    Some(port) if port.fract() == 0.0 && (1.0..=65535.0).contains(&port) => {
                        Some(port as u16)
                    }
                    Some(port) => {
                        return Err(mlua::Error::runtime(format!(
                            "Host's port is a whole number from 1 to 65535, not {port}"
                        )));
                    }
                };
                let password = password(lua, &secret, "Host")?;
                request(lua, NetworkRequest::Host { port, password });
                Ok(())
            },
        );
        methods.add_method(
            "Join",
            |lua, _, (address, secret): (mlua::Value, mlua::Value)| {
                let address = match &address {
                    mlua::Value::String(address) => address.to_string_lossy(),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "Join's address is text, such as \
                             NetworkService:Join(\"192.168.1.20:47800\"), not a {}",
                            type_name(lua, other)
                        )));
                    }
                };
                let password = password(lua, &secret, "Join")?;
                request(lua, NetworkRequest::Join { address, password });
                Ok(())
            },
        );
        methods.add_method("Disconnect", |lua, _, ()| {
            request(lua, NetworkRequest::Disconnect);
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("NetworkService"));
    }
}
