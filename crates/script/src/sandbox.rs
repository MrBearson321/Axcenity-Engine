//! Each script's own table of global variables (its "environment").
//!
//! Scripts never see Lua's real global table. Each one gets a fresh table
//! holding an allowlist of safe standard functions plus the engine API, so:
//! - a global set by one script (`score = 10`) is invisible to the others, and
//! - anything not on the list (`io`, `debug`, `load`, `dofile`, `loadfile`,
//!   `package`, most of `os`) doesn't exist inside scripts.
//!
//! `require` is the engine's own (it only loads ModuleScripts, see
//! `modules`), `collectgarbage` only answers `"count"`, `setmetatable`
//! refuses `__gc`, and `typeof` names engine values. Strings share one
//! metatable in Lua, so [`lock_string_metatable`] hides it from scripts.

use axcenity_core::InstanceId;
use mlua::chunk::Compiler;
use mlua::{Function, Lua, Table, Value};

use crate::{api, errors, modules, task, types, watchdog};

/// Standard functions scripts may use as they are.
pub const SAFE_FUNCTIONS: &[&str] = &[
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
    "tonumber",
    "tostring",
    "type",
    "_VERSION",
];

/// Standard library tables. Each script gets its own copy, so one script
/// adding to `math` doesn't change `math` for the others.
/// `bit32` and `buffer` only work on numbers and their own byte buffers
/// (every read and write is bounds-checked), so they reach nothing outside.
pub const SAFE_LIBRARIES: &[&str] = &[
    "bit32",
    "buffer",
    "coroutine",
    "math",
    "string",
    "table",
    "utf8",
];

/// Library functions left out of those copies. `string.dump` turns functions
/// into bytecode, which is only useful together with the missing `load`.
const UNSAFE_LIBRARY_FUNCTIONS: &[(&str, &str)] = &[("string", "dump")];

/// The parts of `os` that only read the clock. The rest (`execute`, `remove`,
/// `exit`, `getenv`, ...) reaches outside the game.
/// Luau builtins whose global the sandbox replaces (`setmetatable` and
/// `typeof` are the engine's own) or leaves out (`unpack`, and the `vector`
/// and `integer` libraries). Script environments are `safeenv` (see
/// [`environment`]), so Luau would otherwise run its own built-in version
/// of these for a call such as `typeof(x)` without looking the name up.
const DISABLED_BUILTINS: &[&str] = &[
    "setmetatable",
    "typeof",
    "unpack",
    "vector.create",
    "vector.magnitude",
    "vector.normalize",
    "vector.cross",
    "vector.dot",
    "vector.floor",
    "vector.ceil",
    "vector.abs",
    "vector.sign",
    "vector.clamp",
    "vector.min",
    "vector.max",
    "vector.lerp",
    "integer.add",
    "integer.sub",
    "integer.mod",
    "integer.mul",
    "integer.div",
    "integer.idiv",
    "integer.udiv",
    "integer.rem",
    "integer.urem",
    "integer.min",
    "integer.max",
    "integer.neg",
    "integer.create",
    "integer.clamp",
    "integer.band",
    "integer.bor",
    "integer.bxor",
    "integer.bnot",
    "integer.btest",
    "integer.bswap",
    "integer.lt",
    "integer.le",
    "integer.ult",
    "integer.ule",
    "integer.gt",
    "integer.ge",
    "integer.ugt",
    "integer.uge",
    "integer.lshift",
    "integer.rshift",
    "integer.arshift",
    "integer.lrotate",
    "integer.rrotate",
    "integer.countrz",
    "integer.countlz",
    "integer.extract",
    "integer.tonumber",
];

/// The Luau compiler every script and console command is compiled with:
/// Luau's builtins on, except [`DISABLED_BUILTINS`].
pub fn compiler() -> Compiler {
    Compiler::new().set_disabled_builtins(DISABLED_BUILTINS.iter().copied())
}

/// A syntax error in a script: the line it's on (from 1) and Luau's
/// message, such as `Expected 'end' (to close 'function' at line 1), got <eof>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub message: String,
}

/// The first syntax error in `source`, if it has one, found by compiling it
/// as a script would be (without running anything). Cheap enough to call
/// as someone types: it needs no interpreter.
pub fn find_syntax_error(source: &str) -> Option<SyntaxError> {
    match compiler().compile(source) {
        Ok(_) => None,
        Err(mlua::Error::SyntaxError { message, .. }) => {
            // Luau writes `line: message`.
            let (line, message) = message
                .split_once(':')
                .and_then(|(line, rest)| Some((line.trim().parse().ok()?, rest.trim())))
                .unwrap_or((1, message.trim()));
            Some(SyntaxError {
                line,
                message: message.to_owned(),
            })
        }
        Err(other) => Some(SyntaxError {
            line: 1,
            message: other.to_string(),
        }),
    }
}

const SAFE_OS_FUNCTIONS: &[&str] = &["clock", "date", "difftime", "time"];

/// What `getmetatable` gives for a string, as in Roblox.
const LOCKED: &str = "The metatable is locked";

/// Luau never runs `__gc` on tables, so a script that sets one gets an error
/// saying so rather than a finalizer that silently never runs.
const SET_METATABLE: &str = r#"
local setmetatable, rawget, type, error = ...

return function(t, metatable)
    if type(metatable) == "table" and rawget(metatable, "__gc") ~= nil then
        error("__gc isn't supported in scripts: the engine frees memory by itself", 2)
    end
    return setmetatable(t, metatable)
end
"#;

/// Functions every script's environment shares. They hold no state of their
/// own, so one copy is enough, made once per side. Kept in Lua's app data,
/// so `require` can make environments for modules too.
pub struct Shared {
    protected: errors::ProtectedCalls,
    /// `coroutine.resume`, `wrap`, `status` and `close`, see `watchdog`.
    coroutine: Table,
    setmetatable: Function,
    task: task::TaskFunctions,
    require: Function,
    collectgarbage: Function,
    type_of: Function,
}

impl Shared {
    /// Creates the shared functions and puts them in `lua`'s app data.
    pub fn install(lua: &Lua) -> mlua::Result<()> {
        let protected = errors::protected_calls(lua)?;
        let coroutine = watchdog::coroutine_functions(lua, &protected.clean)?;
        // Notes what it hands out, so those coroutines aren't reused.
        coroutine.raw_set("running", crate::handler_threads::running(lua)?)?;
        let globals = lua.globals();
        let setmetatable = lua.load(SET_METATABLE).set_name("=[engine]").call((
            globals.raw_get::<Function>("setmetatable")?,
            globals.raw_get::<Function>("rawget")?,
            globals.raw_get::<Function>("type")?,
            globals.raw_get::<Function>("error")?,
        ))?;
        let task = task::TaskFunctions::new(lua)?;
        let require = modules::require_function(lua, task.wait())?;
        let collectgarbage = lua.create_function(|lua, option: Value| match option {
            Value::String(text) if text == "count" => Ok(lua.used_memory() as f64 / 1024.0),
            // Other options start, stop or tune the garbage collector, which
            // the engine runs by itself, for every script at once.
            _ => Err(mlua::Error::runtime(
                "collectgarbage only takes \"count\" in scripts: collectgarbage(\"count\") \
                 is the memory in use, in kilobytes",
            )),
        })?;
        let type_of =
            lua.create_function(|lua, value: Value| Ok(types::typeof_name(lua, &value)))?;
        lua.set_app_data(Self {
            protected,
            coroutine,
            setmetatable,
            task,
            require,
            collectgarbage,
            type_of,
        });
        Ok(())
    }

    /// `task.spawn`, for engine Lua that starts a thread of its own.
    pub fn spawn(&self) -> &Function {
        self.task.spawn()
    }

    /// `task.wait`, for engine Lua that needs to wait.
    pub fn wait(&self) -> &Function {
        self.task.wait()
    }

    /// The engine's `pcall`, which passes errors on as `path:line: message`.
    pub fn pcall(&self) -> &Function {
        &self.protected.pcall
    }
}

/// Every string has the same metatable, whose `__index` is Lua's real
/// `string` table (that's how `("hi"):upper()` works). A script that got
/// hold of it could change `upper` for every script, or reach
/// `string.dump`. So `__index` becomes a copy without `dump`, and
/// `getmetatable("")` only returns [`LOCKED`].
pub fn lock_string_metatable(lua: &Lua) -> mlua::Result<()> {
    let metatable: Table = lua
        .load("return getmetatable('')")
        .set_name("=[engine]")
        .eval()?;
    let methods = safe_copy(lua, &lua.globals().raw_get("string")?, "string")?;
    metatable.raw_set("__index", methods)?;
    metatable.raw_set("__metatable", LOCKED)
}

/// A copy of the standard `library` table, without its unsafe functions.
fn safe_copy(lua: &Lua, original: &Table, library: &str) -> mlua::Result<Table> {
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
    Ok(copy)
}

/// The Instances a script's environment refers to.
#[derive(Clone, Copy)]
pub struct ScriptIds {
    pub game: InstanceId,
    pub workspace: InstanceId,
    /// The script's own Script Instance.
    pub script: InstanceId,
}

/// The engine's own globals, which `api::install` and [`environment`] add.
const ENGINE_GLOBALS: &[&str] = &[
    "print",
    "game",
    "workspace",
    "script",
    "Instance",
    "Vector3",
    "Vector2",
    "Color3",
    "UDim2",
    "CFrame",
    "PhysicalProperties",
    "Random",
    "TweenInfo",
    "RaycastParams",
    "OverlapParams",
    "Enum",
    "task",
    "pcall",
    "xpcall",
    "setmetatable",
    "require",
    "collectgarbage",
    "typeof",
    "os",
    "_G",
];

/// Every global a script's environment has, sorted.
pub fn global_names() -> Vec<&'static str> {
    let mut names: Vec<&str> = SAFE_FUNCTIONS
        .iter()
        .chain(SAFE_LIBRARIES)
        .chain(ENGINE_GLOBALS)
        .copied()
        .collect();
    names.sort_unstable();
    names
}

/// Builds a new environment for one script or module.
pub fn environment(lua: &Lua, ids: ScriptIds) -> mlua::Result<Table> {
    let shared = lua
        .app_data_ref::<Shared>()
        .ok_or_else(|| mlua::Error::runtime("the script functions aren't ready"))?;
    let globals = lua.globals();
    let env = lua.create_table()?;

    for &name in SAFE_FUNCTIONS {
        env.raw_set(name, globals.raw_get::<Value>(name)?)?;
    }
    env.raw_set("pcall", &shared.protected.pcall)?;
    env.raw_set("xpcall", &shared.protected.xpcall)?;
    env.raw_set("setmetatable", &shared.setmetatable)?;
    env.raw_set("require", &shared.require)?;
    env.raw_set("collectgarbage", &shared.collectgarbage)?;
    env.raw_set("typeof", &shared.type_of)?;

    for &library in SAFE_LIBRARIES {
        let copy = safe_copy(lua, &globals.raw_get(library)?, library)?;
        if library == "coroutine" {
            for pair in shared.coroutine.pairs::<Value, Value>() {
                let (name, function) = pair?;
                copy.raw_set(name, function)?;
            }
        }
        // Read-only, as Luau's own libraries are: a call such as
        // `math.sqrt(x)` runs Luau's builtin directly, so a script that
        // replaced `math.sqrt` in its copy wouldn't get its own function.
        copy.set_readonly(true);
        env.raw_set(library, copy)?;
    }

    let os: Table = globals.raw_get("os")?;
    let safe_os = lua.create_table()?;
    for &name in SAFE_OS_FUNCTIONS {
        safe_os.raw_set(name, os.raw_get::<Value>(name)?)?;
    }
    safe_os.set_readonly(true);
    env.raw_set("os", safe_os)?;

    // `_G` is the script's own globals, not a table shared between scripts.
    env.raw_set("_G", &env)?;

    api::install(lua, &env, ids)?;
    env.raw_set("task", shared.task.library(lua)?)?;
    // `safeenv` lets Luau call its builtins (`math.sqrt`, `string.sub`,
    // `type`, ...) directly instead of looking them up and calling them
    // through the C API, about 3x faster for numeric code, and it's what
    // lets native code (`--!native`) run them inline. Luau assumes the
    // environment's builtins are the real ones: true here, as the
    // libraries are read-only and `compiler` turns off the builtins the
    // sandbox replaces or leaves out. A script that reassigns a builtin
    // global by name (`typeof = f`) still gets its own: Luau's compiler
    // sees the assignment and calls it normally.
    env.set_safeenv(true);
    Ok(env)
}
