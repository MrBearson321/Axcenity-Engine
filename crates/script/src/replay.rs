//! `game:GetService("ReplayService")` (Phase 81): recording this device's
//! game to a `.replay` file, and a buffer of the last few seconds for a
//! kill cam or an instant replay.
//!
//! - `StartRecording()` starts (or starts again) a recording;
//!   `StopRecording(name)` ends it and saves `<name>.replay` in the
//!   player's data folder, under `replays`. `IsRecording` says whether one
//!   is going.
//! - `BufferSeconds` (0, off; at most 60) keeps the last that many seconds
//!   in memory; `PlayBuffer(player?)` shows them on this device's screen,
//!   from `player`'s character's eyes if given, then goes back to the game.
//!
//! As with `NetworkService`, scripts don't record anything themselves:
//! each call queues a [`ReplayRequest`] for the host's network step
//! (`axcenity_net::replay`), which records the replication stream.

use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use axcenity_core::InstanceId;

use crate::instance::LuaInstance;
use crate::types::type_name;

/// The longest buffer, in seconds.
pub const MAX_BUFFER_SECONDS: f64 = 60.0;

/// What a script asked of replays, for the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum ReplayRequest {
    /// `StartRecording()`.
    Start,
    /// `StopRecording(name)`: save as `<name>.replay`.
    Stop { name: String },
    /// `BufferSeconds` set: keep the last `seconds` (0 stops the buffer).
    Buffer { seconds: f64 },
    /// `PlayBuffer(player)`: show the buffer, following `follow`'s
    /// character if given.
    PlayBuffer { follow: Option<InstanceId> },
}

/// This side's requests since the host last took them, and what scripts
/// read back.
#[derive(Default)]
struct ReplayState {
    requests: Vec<ReplayRequest>,
    recording: bool,
    buffer_seconds: f64,
}

fn with_state<T>(lua: &Lua, f: impl FnOnce(&mut ReplayState) -> T) -> T {
    if lua.app_data_ref::<ReplayState>().is_none() {
        lua.set_app_data(ReplayState::default());
    }
    let mut state = lua.app_data_mut::<ReplayState>().expect("set just above");
    f(&mut state)
}

/// The requests waiting on this side, oldest first.
pub(crate) fn take_requests(lua: &Lua) -> Vec<ReplayRequest> {
    lua.app_data_mut::<ReplayState>()
        .map(|mut state| std::mem::take(&mut state.requests))
        .unwrap_or_default()
}

/// A replay's name: what its file is called, so only letters, digits,
/// spaces, `-` and `_`, 1 to 64 of them.
fn replay_name(lua: &Lua, value: &mlua::Value) -> mlua::Result<String> {
    let mlua::Value::String(text) = value else {
        return Err(mlua::Error::runtime(format!(
            "StopRecording's name is text, such as ReplayService:StopRecording(\"final round\"), not a {}",
            type_name(lua, value)
        )));
    };
    let name = text.to_string_lossy();
    let fits = name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    let length = name.chars().count();
    if !fits || length == 0 || length > 64 || name.trim().is_empty() {
        return Err(mlua::Error::runtime(format!(
            "StopRecording's name {name:?} must be 1 to 64 letters, digits, spaces, - or _, \
             since it names the file"
        )));
    }
    Ok(name.trim().to_owned())
}

/// `ReplayService`, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaReplayService;

impl UserData for LuaReplayService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("IsRecording", |lua, _| {
            Ok(with_state(lua, |state| state.recording))
        });
        fields.add_field_method_get("BufferSeconds", |lua, _| {
            Ok(with_state(lua, |state| state.buffer_seconds))
        });
        fields.add_field_method_set("BufferSeconds", |lua, _, value: mlua::Value| {
            let seconds = match value {
                mlua::Value::Integer(n) => n as f64,
                mlua::Value::Number(n) => n,
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "BufferSeconds is a number of seconds, such as 5, not a {}",
                        type_name(lua, &other)
                    )));
                }
            };
            if !seconds.is_finite() || !(0.0..=MAX_BUFFER_SECONDS).contains(&seconds) {
                return Err(mlua::Error::runtime(format!(
                    "BufferSeconds is from 0 (off) to {MAX_BUFFER_SECONDS}, not {seconds}"
                )));
            }
            with_state(lua, |state| {
                state.buffer_seconds = seconds;
                state.requests.push(ReplayRequest::Buffer { seconds });
            });
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("StartRecording", |lua, _, ()| {
            with_state(lua, |state| {
                state.recording = true;
                state.requests.push(ReplayRequest::Start);
            });
            Ok(())
        });
        methods.add_method("StopRecording", |lua, _, name: mlua::Value| {
            let name = replay_name(lua, &name)?;
            with_state(lua, |state| {
                if !state.recording {
                    return Err(mlua::Error::runtime(
                        "StopRecording: nothing is being recorded; call StartRecording first",
                    ));
                }
                state.recording = false;
                state.requests.push(ReplayRequest::Stop { name });
                Ok(())
            })
        });
        methods.add_method("PlayBuffer", |lua, _, subject: mlua::Value| {
            let follow = match &subject {
                mlua::Value::Nil => None,
                mlua::Value::UserData(data) if data.is::<LuaInstance>() => {
                    Some(data.borrow::<LuaInstance>()?.0)
                }
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "PlayBuffer's subject is a Player or nil, not a {}",
                        type_name(lua, other)
                    )));
                }
            };
            if let Some(id) = follow {
                let world = crate::instance::world(lua)?;
                let is_player = world
                    .get(id)
                    .is_some_and(|i| i.class() == axcenity_core::ClassName::Player);
                if !is_player {
                    return Err(mlua::Error::runtime(
                        "PlayBuffer's subject is a Player or nil",
                    ));
                }
            }
            with_state(lua, |state| {
                if state.buffer_seconds <= 0.0 {
                    return Err(mlua::Error::runtime(
                        "PlayBuffer: there's no buffer; set ReplayService.BufferSeconds first",
                    ));
                }
                state.requests.push(ReplayRequest::PlayBuffer { follow });
                Ok(())
            })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ReplayService"));
    }
}
