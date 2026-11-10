//! Coroutines for event handlers, reused when that can't be told apart
//! (roadmap M6).
//!
//! Every handler call runs in a coroutine of its own, as in Roblox: a
//! handler that waits pauses only itself. Most handlers don't wait, though,
//! and making a coroutine for each call is most of what firing an event
//! costs. So a coroutine whose handler **finished without pausing** goes
//! back to a small free list, and the next handler call runs in it after a
//! reset (Luau's `lua_resetthread`, which returns it to the state of a new
//! coroutine).
//!
//! A coroutine is only reused when nothing can still hold it:
//! - it **finished** (not paused, not failed), so the scheduler, the
//!   watchdog and anything waiting on it are done with it;
//! - the script never asked for it: the sandbox's `coroutine.running` notes
//!   every coroutine it hands out (see [`running`]), because a script that
//!   kept one could otherwise resume or inspect someone else's handler
//!   later;
//! - no task of it is waiting in the scheduler;
//! - the debugger isn't on (it remembers coroutines by identity).
//!
//! A script therefore sees the same thing as with a new coroutine per call.

use axcenity_core::InstanceId;
use mlua::thread::ThreadStatus;
use mlua::{Function, IntoLuaMulti, Lua, Table, Thread};

use crate::{debugger, push_error, scheduler};

/// How many finished coroutines are kept for reuse. One is enough for
/// handlers fired one after another; a few more cover a handler that fires
/// another event.
const MAX_FREE: usize = 8;

/// Lives in Lua's app data.
pub struct HandlerThreads {
    free: Vec<Thread>,
    /// The coroutines `coroutine.running` returned to a script, as weak
    /// keys, so Lua still frees them.
    handed_out: Table,
}

fn handed_out(lua: &Lua) -> mlua::Result<Table> {
    if let Some(threads) = lua.app_data_ref::<HandlerThreads>() {
        return Ok(threads.handed_out.clone());
    }
    let handed_out = lua.create_table()?;
    let weak_keys = lua.create_table()?;
    weak_keys.raw_set("__mode", "k")?;
    handed_out.set_metatable(Some(weak_keys))?;
    lua.set_app_data(HandlerThreads {
        free: Vec::new(),
        handed_out: handed_out.clone(),
    });
    Ok(handed_out)
}

const RUNNING: &str = r#"
local running, handed_out = ...
return function()
    local thread, is_main = running()
    if thread ~= nil and not is_main then
        handed_out[thread] = true
    end
    return thread, is_main
end
"#;

/// The sandbox's `coroutine.running`: Luau's, noting each coroutine it
/// returns so that coroutine is never reused for another handler.
pub fn running(lua: &Lua) -> mlua::Result<Function> {
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    lua.load(RUNNING)
        .set_name("=[engine]")
        .call((coroutine.raw_get::<Function>("running")?, handed_out(lua)?))
}

/// Calls `callback` with `args` as event handler code of `script`, in a
/// coroutine of its own (a reused one when possible).
pub fn run(lua: &Lua, callback: Function, args: impl IntoLuaMulti, script: Option<InstanceId>) {
    let reused = lua
        .app_data_mut::<HandlerThreads>()
        .and_then(|mut threads| threads.free.pop());
    let thread = match reused {
        Some(thread) => match thread.reset(callback.clone()) {
            Ok(()) => Ok(thread),
            Err(_) => lua.create_thread(callback),
        },
        None => lua.create_thread(callback),
    };
    match thread {
        Ok(thread) => {
            scheduler::resume_as(lua, &thread, args, script);
            give_back(lua, thread);
        }
        Err(err) => push_error(lua, &err),
    }
}

/// Keeps `thread` for the next handler if nothing can tell (see the
/// module notes).
fn give_back(lua: &Lua, thread: Thread) {
    if thread.status() != ThreadStatus::Finished || debugger::is_installed(lua) {
        return;
    }
    if scheduler::is_waiting(lua, &thread) {
        return;
    }
    let Some(mut threads) = lua.app_data_mut::<HandlerThreads>() else {
        return;
    };
    if threads.free.len() >= MAX_FREE || threads.handed_out.raw_get::<bool>(&thread).unwrap_or(true)
    {
        return;
    }
    threads.free.push(thread);
}

/// How many coroutines are kept for reuse.
#[cfg(test)]
pub(crate) fn free_count(lua: &Lua) -> usize {
    lua.app_data_ref::<HandlerThreads>()
        .map_or(0, |threads| threads.free.len())
}
