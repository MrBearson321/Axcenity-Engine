//! Axcenity scripting: runs Lua 5.4 scripts against the Instance tree.
//!
//! - [`ScriptRuntime`] owns the Lua interpreter and runs scripts. Each script
//!   runs as a coroutine, and the `scheduler` resumes paused ones each frame
//!   (`task.wait`, `task.spawn` and `task.delay` live in `task`).
//! - Every script gets its own global variables, and only safe parts of the
//!   standard library (see `sandbox`).
//! - The engine API (`game`, `workspace`, `Instance`, `Vector3`, ...) lives in
//!   `api`, `instance` and `types`.
//! - [`lua_definitions`] describes that API for editors (see `definitions`).
//! - `print` output and errors are collected as [`OutputLine`]s for the host to
//!   show: the player's console now, the editor's Output panel later.

mod api;
mod definitions;
mod errors;
mod instance;
mod sandbox;
mod scheduler;
mod task;
mod types;

use axcenity_core::{InstanceId, World};
use mlua::chunk::ChunkMode;
use mlua::{Lua, LuaOptions, StdLib, Thread};

use crate::errors::error_message;

pub use definitions::{DEFINITIONS_PATH, lua_definitions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    /// Text from `print`.
    Print,
    /// A script error, starting with the script's path and line.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
}

/// The output lines waiting for the host. A named type, so that this is the
/// only `Vec<OutputLine>` stored in Lua's app data.
#[derive(Default)]
struct Output(Vec<OutputLine>);

/// The Lua interpreter that runs every script. There is one per game.
pub struct ScriptRuntime {
    lua: Lua,
    shared: sandbox::Shared,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        // Only these libraries are loaded at all. `sandbox` then picks the
        // safe parts of them for each script.
        let libraries = StdLib::COROUTINE
            | StdLib::MATH
            | StdLib::OS
            | StdLib::STRING
            | StdLib::TABLE
            | StdLib::UTF8;
        let lua = Lua::new_with(libraries, LuaOptions::default())
            .expect("loading Lua's standard libraries only fails when out of memory");
        lua.set_app_data(Output::default());
        lua.set_app_data(scheduler::Scheduler::default());
        let shared = sandbox::Shared::new(&lua)
            .expect("creating the shared script functions only fails when out of memory");
        Self { lua, shared }
    }

    /// Starts one script: runs its code as a new coroutine until it finishes,
    /// fails or pauses (`task.wait`). Paused scripts carry on in [`Self::step`].
    ///
    /// `script` is its Script Instance (the `script` global), and `path` is
    /// the file path shown in error messages. Errors don't stop anything else:
    /// they're added to the output, and the next script can run as normal.
    pub fn start_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) {
        let ids = sandbox::ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script,
        };
        with_world(&self.lua, world, || {
            match self.load_script(ids, path, source) {
                Ok(thread) => scheduler::resume(&self.lua, &thread, ()),
                Err(err) => push_output(&self.lua, OutputKind::Error, error_message(&err)),
            }
        });
    }

    /// Compiles a script into a coroutine that hasn't started yet.
    fn load_script(
        &self,
        ids: sandbox::ScriptIds,
        path: &str,
        source: &str,
    ) -> mlua::Result<Thread> {
        let env = sandbox::environment(&self.lua, &self.shared, ids)?;
        let function = self
            .lua
            .load(source)
            // The `@` tells Lua this is a file path, so messages read `path:line:`.
            .set_name(format!("@{path}"))
            // Precompiled bytecode can crash the interpreter, so only accept source text.
            .set_mode(ChunkMode::Text)
            .set_environment(env)
            .into_function()?;
        self.lua.create_thread(function)
    }

    /// Frame-order step 2: moves the game clock forward by `seconds` and
    /// resumes every paused script whose wait is over, and every
    /// `task.delay` whose time has come.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        with_world(&self.lua, world, || scheduler::step(&self.lua, seconds));
    }

    /// Removes and returns the output produced since the last call.
    pub fn take_output(&mut self) -> Vec<OutputLine> {
        match self.lua.app_data_mut::<Output>() {
            Some(mut output) => std::mem::take(&mut output.0),
            None => Vec::new(),
        }
    }
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Lends `world` to Lua while `f` runs.
///
/// Lua callbacks can't borrow `world`, because Lua may keep those callbacks
/// around for longer than this call. So the World itself is moved into Lua's
/// "app data" (leaving an empty World behind), and moved back afterwards.
///
/// `impl FnOnce() -> R` means "any closure that can be called once and
/// returns an R", so callers can pass `|| ...` with whatever result they need.
fn with_world<R>(lua: &Lua, world: &mut World, f: impl FnOnce() -> R) -> R {
    lua.set_app_data(std::mem::take(world));
    let result = f();
    *world = lua
        .remove_app_data::<World>()
        .expect("the World was lent to Lua above");
    result
}

fn push_output(lua: &Lua, kind: OutputKind, text: String) {
    if let Some(mut output) = lua.app_data_mut::<Output>() {
        output.0.push(OutputLine { kind, text });
    }
}

#[cfg(test)]
mod tests;
