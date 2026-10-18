//! Turning Lua and engine errors into messages that start with `path:line:`.

use mlua::{Function, IntoLua, Lua};

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

/// The first `path:line` of script code in a traceback. Tracebacks look like:
///
/// ```text
/// stack traceback:
///     [C]: in ?
///     scripts/build.server.lua:3: in main chunk
/// ```
///
/// Lines starting with `[` are Rust code (`[C]`) or the engine's own Lua
/// (`[engine]`), and `__mlua` lines are Lua code that mlua generates (such as
/// the `__index` that checks methods first). None of those are the script.
fn script_location(traceback: &str) -> Option<&str> {
    traceback
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('[') && !line.starts_with("__mlua"))
        .find_map(|line| line.split_once(": in ").map(|(location, _)| location))
}

/// Lua's `pcall` and `xpcall` hand engine errors to scripts as mlua's own
/// error objects, which print with a Rust-style prefix and a traceback. These
/// versions pass the same errors on as plain `path:line: message` strings.
///
/// They're written in Lua so they still call the real `pcall`; a Rust version
/// would stop scripts from pausing (`task.wait`) inside a `pcall`.
const PROTECTED_CALLS: &str = r#"
local pcall, xpcall, clean = ...

local function finish(ok, ...)
    if ok then
        return true, ...
    end
    return false, clean((...))
end

local function safe_pcall(f, ...)
    return finish(pcall(f, ...))
end

local function safe_xpcall(f, handler, ...)
    return xpcall(f, function(err)
        return handler(clean(err))
    end, ...)
end

return safe_pcall, safe_xpcall
"#;

/// Creates the `pcall` and `xpcall` every script gets. They hold no state, so
/// one pair is shared by all scripts.
pub fn protected_calls(lua: &Lua) -> mlua::Result<(Function, Function)> {
    let globals = lua.globals();
    let clean = lua.create_function(|lua, value: mlua::Value| match value {
        // An error object made by mlua; other error values (strings, tables) pass through.
        mlua::Value::Error(err) => error_message(&err).into_lua(lua),
        other => Ok(other),
    })?;
    lua.load(PROTECTED_CALLS).set_name("=[engine]").call((
        globals.raw_get::<Function>("pcall")?,
        globals.raw_get::<Function>("xpcall")?,
        clean,
    ))
}
