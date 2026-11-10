//! A sandboxed interpreter that runs against the world being edited and
//! lives on between frames, for the editor's plugins (Phase 54; the plugin
//! API itself is the `plugins` crate, which the player never links).
//!
//! It's the Console's edit-mode interpreter (`run_edit_command`) kept
//! alive: the same sandbox and Instance API on the server side,
//! `RunService:IsStudio()` true and `IsRunning()` false. What makes it
//! different is set up here:
//!
//! - `Destroy` on an Instance in the tree only takes it out of the tree, as
//!   the editor's Delete does, so the editor can undo it. Instances outside
//!   the tree (a plugin's own UI) are destroyed as usual.
//! - [`EditRuntime::add_service`] gives `game:GetService` more services,
//!   such as `Selection`.

use axcenity_core::{InstanceId, RecordedChange, World};
use mlua::chunk::ChunkMode;
use mlua::{Function, IntoLuaMulti, Lua, Table, Value};

use crate::context::Context;
use crate::sides::Side;
use crate::{OutputLine, run_service, scheduler, with_world};

/// In Lua's app data while `Destroy` should only take tree Instances out of
/// the tree (see the module documentation).
pub(crate) struct DetachOnDestroy;

/// Services `game:GetService` finds besides the engine's own, by name.
#[derive(Default)]
pub(crate) struct ExtraServices(pub(crate) Vec<(String, Value)>);

/// See the module documentation.
pub struct EditRuntime {
    context: Context,
}

impl EditRuntime {
    pub fn new() -> Self {
        let context = Context::new(Side::Server);
        context.set_run_flags(run_service::RunFlags {
            studio: true,
            running: false,
        });
        context.lua.set_app_data(DetachOnDestroy);
        context.lua.set_app_data(ExtraServices::default());
        Self { context }
    }

    /// The interpreter, for making the functions and values a host adds.
    pub fn lua(&self) -> &Lua {
        &self.context.lua
    }

    /// Makes `game:GetService(name)` return `service`.
    pub fn add_service(&self, name: &str, service: Value) {
        if let Some(mut services) = self.context.lua.app_data_mut::<ExtraServices>() {
            services.0.push((name.to_owned(), service));
        }
    }

    /// A new script environment over `world`: the sandbox's globals, with
    /// `script` nil, as for Console commands.
    pub fn environment(&self, world: &World) -> mlua::Result<Table> {
        let env = crate::sandbox::environment(
            &self.context.lua,
            crate::sandbox::ScriptIds {
                game: world.root(),
                workspace: world.workspace(),
                script: world.root(),
            },
        )?;
        env.raw_set("script", Value::Nil)?;
        Ok(env)
    }

    /// Compiles the file `path` (its code `source`) to run in `env`. Its
    /// errors read `path:line: message`.
    pub fn load(&self, env: &Table, path: &str, source: &str) -> mlua::Result<Function> {
        self.context
            .lua
            .load(source)
            .set_name(format!("@{path}"))
            // Only source text: bytecode can crash the interpreter.
            .set_mode(ChunkMode::Text)
            .set_environment(env.clone())
            .into_function()
    }

    /// Calls `function` with `args` as a coroutine of its own, with `world`
    /// lent to it, until it finishes, pauses (`task.wait`) or fails. Errors
    /// join the output.
    pub fn run(&self, world: &mut World, function: &Function, args: impl IntoLuaMulti) {
        let lua = &self.context.lua;
        let Ok(thread) = lua.create_thread(function.clone()) else {
            return;
        };
        with_world(lua, world, || scheduler::resume(lua, &thread, args));
    }

    /// Moves the clock on by `seconds` and resumes the coroutines that
    /// are due (`task.wait`, `task.delay`).
    pub fn step(&self, world: &mut World, seconds: f64) {
        let destroyed = world.take_destroyed();
        self.context.step(world, Vec::new(), seconds, &destroyed);
    }

    /// True while a coroutine waits for [`step`](Self::step).
    pub fn has_waiting(&self) -> bool {
        self.context
            .lua
            .app_data_ref::<scheduler::Scheduler>()
            .is_some_and(|scheduler| scheduler.has_waiting())
    }

    /// Fires `instance`'s no-argument event `event`, such as a TextButton's
    /// `Activated`. An Instance that's gone is skipped.
    pub fn fire_instance_event(
        &self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
    ) {
        if world.contains(instance) {
            self.context.fire_instance_event(world, instance, event);
        }
    }

    /// Fires `Changed`, `ChildAdded` and the other change events for
    /// `changes`, taken from `world` by the host.
    pub fn fire_changes(&self, world: &mut World, changes: &[RecordedChange]) {
        self.context.fire_changes(world, changes);
    }

    /// What `print` printed and the errors, since the last call.
    pub fn take_output(&self) -> Vec<OutputLine> {
        self.context.take_output()
    }
}

impl Default for EditRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// The service `game:GetService(name)` was given by
/// [`EditRuntime::add_service`], if any.
pub(crate) fn extra_service(lua: &Lua, name: &str) -> Option<Value> {
    lua.app_data_ref::<ExtraServices>().and_then(|services| {
        services
            .0
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, service)| service.clone())
    })
}
