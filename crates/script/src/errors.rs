//! Turning Lua and engine errors into messages that start with `path:line:`,
//! and the stack trace of script calls that led to them.

use mlua::{Function, IntoLua, Lua};

use crate::watchdog;

/// The message to show for a script error, starting with `path:line:`.
pub fn error_message(err: &mlua::Error) -> String {
    match err {
        // Errors raised by Lua code (including `error("...")`) already start
        // with the location. mlua adds a traceback, which beginners don't need.
        mlua::Error::RuntimeError(message) | mlua::Error::SyntaxError { message, .. } => message
            .split("\nstack traceback:")
            .next()
            .unwrap_or(message)
            .to_owned(),
        // An error from the engine's Rust code, such as a property set to the
        // wrong type. The location comes from the Lua call stack at that moment.
        mlua::Error::CallbackError { traceback, cause } => {
            let message = error_message(cause);
            match script_location(traceback) {
                Some(location) if !message.starts_with(location) => {
                    format!("{location}: {message}")
                }
                _ => message,
            }
        }
        other => other.to_string(),
    }
}

/// The script calls that led to an error, innermost first, each as
/// `path:line: in <function>` (`scripts/util.module.lua:5: in field 'explode'`).
/// Calls through other scripts and modules are included; the engine's own
/// code isn't. Empty for an error with no stack, such as a syntax error.
pub fn error_trace(err: &mlua::Error) -> Vec<String> {
    match err {
        mlua::Error::RuntimeError(message) => message
            .split_once("\nstack traceback:")
            .map(|(_, traceback)| script_frames(traceback))
            .unwrap_or_default(),
        // An engine error inside Lua called from the engine (a Lua function
        // that a Rust callback ran) has its own, deeper trace in `cause`.
        // That trace covers the whole coroutine, so it's the one to show.
        mlua::Error::CallbackError { traceback, cause } => {
            let inner = error_trace(cause);
            if inner.is_empty() {
                script_frames(traceback)
            } else {
                inner
            }
        }
        _ => Vec::new(),
    }
}

/// The lines of a traceback that are script code. Tracebacks look like:
///
/// ```text
/// stack traceback:
///     [C]: in function 'error'
///     scripts/util.module.lua:5: in field 'explode'
///     scripts/build.server.lua:3: in main chunk
/// ```
///
/// Lines starting with `[` are Rust code (`[C]`) or the engine's own Lua
/// (`[engine]`), and `__mlua` lines are Lua code that mlua generates (such as
/// the `__index` that checks methods first). None of those are the script.
/// A very deep stack has a `...` line where Lua left calls out; it's kept
/// so the trace doesn't look like the calls were next to each other.
fn script_frames(traceback: &str) -> Vec<String> {
    traceback
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            if is_script_frame(line) {
                Some(line.to_owned())
            } else if line.starts_with("...") {
                Some(line.replace('\t', " "))
            } else {
                None
            }
        })
        .collect()
}

fn is_script_frame(line: &str) -> bool {
    !line.starts_with('[') && !line.starts_with("__mlua") && line.contains(": in ")
}

/// The first `path:line` of script code in a traceback (see [`script_frames`]).
fn script_location(traceback: &str) -> Option<&str> {
    traceback
        .lines()
        .map(str::trim)
        .filter(|line| is_script_frame(line))
        .find_map(|line| line.split_once(": in ").map(|(location, _)| location))
}

/// Lua's `pcall` and `xpcall` hand engine errors to scripts as mlua's own
/// error objects, which print with a Rust-style prefix and a traceback. These
/// versions pass the same errors on as plain `path:line: message` strings.
///
/// They're written in Lua so they still call the real `pcall`; a Rust version
/// would stop scripts from pausing (`task.wait`) inside a `pcall`.
///
/// `pcall` and `xpcall` don't catch the watchdog's error (`stopped` returns
/// it once the watchdog has stopped this coroutine): it's raised again, so a
/// script can't keep running forever by catching it. `xpcall` doesn't call
/// the script's handler for it either, because Lua runs a handler for an
/// error from the watchdog's hook with hooks off, where it can't be stopped.
const PROTECTED_CALLS: &str = r#"
local pcall, xpcall, clean, stopped = ...

local function raise_if_stopped()
    local message = stopped()
    if message then
        error(message, 0)
    end
end

local function finish(ok, ...)
    if ok then
        return true, ...
    end
    raise_if_stopped()
    return false, clean((...))
end

local function finish_xpcall(ok, ...)
    if not ok then
        raise_if_stopped()
    end
    return ok, ...
end

local function safe_pcall(f, ...)
    return finish(pcall(f, ...))
end

local function safe_xpcall(f, handler, ...)
    return finish_xpcall(xpcall(f, function(err)
        if stopped() then
            return err
        end
        return handler(clean(err))
    end, ...))
end

return safe_pcall, safe_xpcall
"#;

/// The engine's `pcall` and `xpcall`, and `clean`, which turns an mlua
/// error object into a plain string (and passes any other value on).
pub struct ProtectedCalls {
    pub pcall: Function,
    pub xpcall: Function,
    pub clean: Function,
}

/// Creates the `pcall` and `xpcall` every script gets. They hold no state,
/// so one pair is shared by all scripts.
pub fn protected_calls(lua: &Lua) -> mlua::Result<ProtectedCalls> {
    let globals = lua.globals();
    let clean = lua.create_function(|lua, value: mlua::Value| match value {
        // An error object made by mlua; other error values (strings, tables) pass through.
        mlua::Value::Error(err) => error_message(&err).into_lua(lua),
        other => Ok(other),
    })?;
    let stopped = lua.create_function(|lua, ()| Ok(watchdog::stopped(lua)))?;
    let (pcall, xpcall) = lua.load(PROTECTED_CALLS).set_name("=[engine]").call((
        globals.raw_get::<Function>("pcall")?,
        globals.raw_get::<Function>("xpcall")?,
        &clean,
        stopped,
    ))?;
    Ok(ProtectedCalls {
        pcall,
        xpcall,
        clean,
    })
}
