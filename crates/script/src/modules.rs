//! `require`: loading ModuleScripts.
//!
//! A ModuleScript is the code of a `*.module.lua` file in the project's
//! `scripts` folder; the host hands it over with `start_script`. The first
//! `require(module)` on a side runs that code (in its own environment, with
//! `script` set to the ModuleScript), and every later `require` on that side
//! returns the same value. Only ModuleScripts can be required, never a file
//! path or a library name, so nothing outside the project can be loaded.
//!
//! Each side remembers which scripts required each module, and which
//! modules required it while they loaded, so a changed module can be
//! loaded again by the scripts that use it ([`replace`]).

use std::collections::{BTreeSet, HashMap};

use axcenity_core::{ClassName, InstanceId};
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, Table, Thread, Value};

use crate::instance::{LuaInstance, world};
use crate::sandbox::{self, ScriptIds};
use crate::scheduler;
use crate::types::type_name;

/// `require` runs the module's code in the requiring coroutine, so that a
/// module can pause (`task.wait`) while it loads; only Lua can do that, so
/// this part is Lua. `begin`, `finish` and `failed` are Rust (below).
///
/// - `begin` returns `"done", value` for a loaded module, `"run", code` for
///   one to load now, or `"wait"` while another coroutine is loading it.
/// - The real `pcall` catches an error in the module's code, so the module
///   can be forgotten (`failed`) and loaded again next time, then the error
///   goes on to the script that called `require`.
const REQUIRE: &str = r#"
local begin, finish, failed, wait, pcall, pack, error = ...

return function(module)
    local state, value = begin(module)
    while state == "wait" do
        wait()
        state, value = begin(module)
    end
    if state == "done" then
        return value
    end
    local results = pack(pcall(value))
    if not results[1] then
        failed(module)
        error(results[2], 0)
    end
    return finish(module, results.n - 1, results[2])
end
"#;

/// The code of every ModuleScript, and the ones loaded so far. One per side
/// (in Lua's app data), so each side runs a module once.
#[derive(Default)]
pub struct Modules {
    sources: HashMap<InstanceId, ModuleSource>,
    states: HashMap<InstanceId, State>,
    /// The scripts that required each module, directly or through another
    /// module (the module's code runs as the requiring script).
    required_by: HashMap<InstanceId, BTreeSet<InstanceId>>,
    /// The modules that required each module while they loaded.
    dependents: HashMap<InstanceId, BTreeSet<InstanceId>>,
}

impl Modules {
    /// Forgets that `script` required anything: it's about to run again.
    pub fn forget_requirer(&mut self, script: InstanceId) {
        for scripts in self.required_by.values_mut() {
            scripts.remove(&script);
        }
    }
}

struct ModuleSource {
    /// The file's path, for error messages.
    path: String,
    source: String,
}

enum State {
    /// Its code is running in this coroutine, which may have paused.
    Loading(Thread),
    /// Its code returned this, which every `require` of it gets.
    Loaded(Value),
}

/// Makes `module` (a ModuleScript) loadable with `require`, running `source`
/// from the file at `path`.
pub fn add(lua: &Lua, module: InstanceId, path: &str, source: &str) {
    if let Some(mut modules) = lua.app_data_mut::<Modules>() {
        modules.sources.insert(
            module,
            ModuleSource {
                path: path.to_owned(),
                source: source.to_owned(),
            },
        );
    }
}

/// Gives `module` new code: it, and every module that required it while
/// loading, load again the next time they're required. Returns the
/// scripts that required any of them, which should run again.
pub fn replace(lua: &Lua, module: InstanceId, path: &str, source: &str) -> BTreeSet<InstanceId> {
    add(lua, module, path, source);
    let Some(mut modules) = lua.app_data_mut::<Modules>() else {
        return BTreeSet::new();
    };
    let mut stale = vec![module];
    let mut seen = BTreeSet::new();
    let mut scripts = BTreeSet::new();
    while let Some(next) = stale.pop() {
        if !seen.insert(next) {
            continue;
        }
        modules.states.remove(&next);
        scripts.extend(modules.required_by.get(&next).into_iter().flatten());
        stale.extend(modules.dependents.get(&next).into_iter().flatten());
    }
    scripts
}

/// Creates the `require` function every script shares. `wait` is
/// `task.wait`, for waiting while another coroutine loads a module.
pub fn require_function(lua: &Lua, wait: &Function) -> mlua::Result<Function> {
    let globals = lua.globals();
    let table: Table = globals.raw_get("table")?;
    lua.load(REQUIRE).set_name("=[engine]").call((
        lua.create_function(begin)?,
        lua.create_function(finish)?,
        lua.create_function(|lua, module: Value| {
            let id = module_id(lua, &module)?;
            modules_mut(lua)?.states.remove(&id);
            Ok(())
        })?,
        wait.clone(),
        globals.raw_get::<Function>("pcall")?,
        table.raw_get::<Function>("pack")?,
        globals.raw_get::<Function>("error")?,
    ))
}

fn modules_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Modules>> {
    lua.app_data_mut::<Modules>()
        .ok_or_else(|| mlua::Error::runtime("modules can't be loaded here"))
}

/// The first step of `require(module)`: see [`REQUIRE`].
fn begin(lua: &Lua, module: Value) -> mlua::Result<(&'static str, Value)> {
    let id = module_id(lua, &module)?;
    let script = scheduler::running_script(lua);
    // The block ends the borrow of `Modules` before compiling, which needs
    // other app data.
    let (path, source) = {
        let mut modules = modules_mut(lua)?;
        if let Some(script) = script {
            modules.required_by.entry(id).or_default().insert(script);
        }
        // Modules loading in this coroutine required this one.
        let thread = lua.current_thread();
        let loading: Vec<InstanceId> = modules
            .states
            .iter()
            .filter(|(other, state)| {
                **other != id && matches!(state, State::Loading(t) if *t == thread)
            })
            .map(|(other, _)| *other)
            .collect();
        for other in loading {
            modules.dependents.entry(id).or_default().insert(other);
        }
        match modules.states.get(&id) {
            Some(State::Loaded(value)) => return Ok(("done", value.clone())),
            Some(State::Loading(thread)) if *thread == lua.current_thread() => {
                let path = modules.sources.get(&id).map_or("", |m| m.path.as_str());
                return Err(mlua::Error::runtime(format!(
                    "`{path}` is required again while it's still loading: modules that \
                     require each other never finish loading. Move what they both need \
                     into a third module"
                )));
            }
            Some(State::Loading(_)) => return Ok(("wait", Value::Nil)),
            None => {}
        }
        let module = modules.sources.get(&id).ok_or_else(|| {
            mlua::Error::runtime(
                "this ModuleScript has no code: only ModuleScripts made from `.module.lua` \
                 files in the project's scripts folder can be required",
            )
        })?;
        (module.path.clone(), module.source.clone())
    };

    let ids = {
        let world = world(lua)?;
        ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: id,
        }
    };
    let code = lua
        .load(source)
        // `@` makes messages start with the module's path, as for scripts.
        .set_name(format!("@{path}"))
        // Only source text, never precompiled bytecode (see `context.rs`).
        .set_mode(ChunkMode::Text)
        .set_environment(sandbox::environment(lua, ids)?)
        .into_function()?;
    modules_mut(lua)?
        .states
        .insert(id, State::Loading(lua.current_thread()));
    Ok(("run", Value::Function(code)))
}

/// The last step of `require(module)`: the module's code returned `count`
/// values, the first of them `value`.
fn finish(lua: &Lua, (module, count, value): (Value, i64, Value)) -> mlua::Result<Value> {
    let id = module_id(lua, &module)?;
    let mut modules = modules_mut(lua)?;
    if count != 1 {
        modules.states.remove(&id);
        let path = modules.sources.get(&id).map_or("", |m| m.path.as_str());
        return Err(mlua::Error::runtime(format!(
            "`{path}` must return exactly one value (usually a table of functions), \
             but it returned {count}"
        )));
    }
    modules.states.insert(id, State::Loaded(value.clone()));
    Ok(value)
}

/// The ModuleScript `value` is, or an error that explains what to pass.
fn module_id(lua: &Lua, value: &Value) -> mlua::Result<InstanceId> {
    let wrong = |what: String| {
        mlua::Error::runtime(format!(
            "require takes a ModuleScript, such as `require(game.ReplicatedStorage.Utils)`, \
             not {what}"
        ))
    };
    if let Value::UserData(data) = value
        && let Ok(instance) = data.borrow::<LuaInstance>()
    {
        let class = world(lua)?
            .get(instance.0)
            .map(|instance| instance.class())
            .ok_or_else(|| mlua::Error::runtime("this ModuleScript has been destroyed"))?;
        return match class {
            ClassName::ModuleScript => Ok(instance.0),
            other => Err(wrong(format!("a {other}"))),
        };
    }
    Err(match value {
        // Plain Lua finds modules by name or path; here they're in the game tree.
        Value::String(text) => wrong(format!(
            "a string (\"{}\"): modules are found in the game, not by file name. The file \
             `scripts/Utils.module.lua` is `game.ReplicatedStorage.Utils`",
            text.to_string_lossy()
        )),
        other => wrong(format!("a {}", type_name(lua, other))),
    })
}
