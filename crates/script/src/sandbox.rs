//! Each script's own table of global variables (its "environment").
//!
//! Scripts never see Lua's real global table. Each one gets a fresh table
//! holding an allowlist of safe standard functions plus the engine API, so:
//! - a global set by one script (`score = 10`) is invisible to the others, and
//! - anything not on the list (`io`, `debug`, `load`, `dofile`, `loadfile`,
//!   `require`, `collectgarbage`, most of `os`) doesn't exist inside scripts.

use axcenity_core::InstanceId;
use mlua::{Function, Lua, Table, Value};

use crate::{api, errors};

/// Standard functions scripts may use as they are.
const SAFE_FUNCTIONS: &[&str] = &[
    "assert",
    "error",
    "getmetatable",
    "ipairs",
    "next",
    "pairs",
    "rawequal",
    "rawget",
    "rawlen",
    "rawset",
    "select",
    "setmetatable",
    "tonumber",
    "tostring",
    "type",
    "_VERSION",
];

/// Standard library tables. Each script gets its own copy, so one script
/// adding to `math` doesn't change `math` for the others.
const SAFE_LIBRARIES: &[&str] = &["coroutine", "math", "string", "table", "utf8"];

/// Library functions left out of those copies. `string.dump` turns functions
/// into bytecode, which is only useful together with the missing `load`.
const UNSAFE_LIBRARY_FUNCTIONS: &[(&str, &str)] = &[("string", "dump")];

/// The parts of `os` that only read the clock. The rest (`execute`, `remove`,
/// `exit`, `getenv`, ...) reaches outside the game.
const SAFE_OS_FUNCTIONS: &[&str] = &["clock", "date", "difftime", "time"];

/// Functions every script's environment shares. They hold no state, so one
/// copy is enough, made once when the runtime starts.
pub struct Shared {
    pcall: Function,
    xpcall: Function,
}

impl Shared {
    pub fn new(lua: &Lua) -> mlua::Result<Self> {
        let (pcall, xpcall) = errors::protected_calls(lua)?;
        Ok(Self { pcall, xpcall })
    }
}

/// The Instances a script's environment refers to.
#[derive(Clone, Copy)]
pub struct ScriptIds {
    pub game: InstanceId,
    pub workspace: InstanceId,
    /// The script's own Script Instance.
    pub script: InstanceId,
}

/// Builds a new environment for one script.
pub fn environment(lua: &Lua, shared: &Shared, ids: ScriptIds) -> mlua::Result<Table> {
    let globals = lua.globals();
    let env = lua.create_table()?;

    for &name in SAFE_FUNCTIONS {
        env.raw_set(name, globals.raw_get::<Value>(name)?)?;
    }
    env.raw_set("pcall", &shared.pcall)?;
    env.raw_set("xpcall", &shared.xpcall)?;

    for &library in SAFE_LIBRARIES {
        let original: Table = globals.raw_get(library)?;
        let copy = lua.create_table()?;
        // `pairs` visits every key, so the copy gets every function.
        for pair in original.pairs::<Value, Value>() {
            let (key, value) = pair?;
            copy.raw_set(key, value)?;
        }
        for &(table, function) in UNSAFE_LIBRARY_FUNCTIONS {
            if table == library {
                copy.raw_set(function, Value::Nil)?;
            }
        }
        env.raw_set(library, copy)?;
    }

    let os: Table = globals.raw_get("os")?;
    let safe_os = lua.create_table()?;
    for &name in SAFE_OS_FUNCTIONS {
        safe_os.raw_set(name, os.raw_get::<Value>(name)?)?;
    }
    env.raw_set("os", safe_os)?;

    // `_G` is the script's own globals, not a table shared between scripts.
    env.raw_set("_G", &env)?;

    api::install(lua, &env, ids)?;
    Ok(env)
}
