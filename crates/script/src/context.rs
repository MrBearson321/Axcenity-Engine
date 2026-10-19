//! One side of the game: a Lua interpreter with the scripts of that side.
//!
//! The server and the client each get their own [`Context`], so their
//! scripts can't share Lua globals, tables or metatables: the only way
//! between them is a RemoteEvent. [`ScriptRuntime`](crate::ScriptRuntime)
//! owns both and passes messages between them.

use axcenity_core::{InstanceId, RecordedChange, World};
use mlua::chunk::ChunkMode;
use mlua::{Lua, LuaOptions, StdLib, Thread};

use crate::client::{self, Camera, CameraState, HeldKeys, KeyEvent, LocalPlayer};
use crate::errors::error_message;
use crate::instance::{self, ClassMethods};
use crate::remote::{self, Message, Outbox};
use crate::sides::Side;
use crate::{
    Output, OutputKind, OutputLine, changes, events, modules, push_output, sandbox, scheduler,
    watchdog, with_world,
};

pub struct Context {
    pub lua: Lua,
}

impl Context {
    pub fn new(side: Side) -> Self {
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
        // First, so that every coroutine made from here on is watched.
        watchdog::install(&lua).expect("setting a hook only fails when out of memory");
        // Which side this is, for `sides::check`.
        lua.set_app_data(side);
        lua.set_app_data(Output::default());
        lua.set_app_data(crate::SoundActions::default());
        lua.set_app_data(crate::ParticleEmits::default());
        lua.set_app_data(Outbox::default());
        lua.set_app_data(scheduler::Scheduler::default());
        lua.set_app_data(events::Connections::default());
        lua.set_app_data(modules::Modules::default());
        if side == Side::Client {
            lua.set_app_data(HeldKeys::default());
            lua.set_app_data(Camera::default());
        }
        sandbox::Shared::install(&lua)
            .expect("creating the shared script functions only fails when out of memory");
        sandbox::lock_string_metatable(&lua)
            .expect("changing the string metatable only fails when out of memory");
        let mut methods = remote::methods(&lua)
            .expect("creating the RemoteEvent methods only fails when out of memory");
        methods.extend(
            instance::sound_methods(&lua)
                .expect("creating Sound methods only fails when out of memory"),
        );
        methods.extend(
            instance::particle_methods(&lua)
                .expect("creating ParticleEmitter methods only fails when out of memory"),
        );
        methods.push((
            axcenity_core::ClassName::DataModel,
            "GetService",
            instance::get_service(&lua).expect("only fails when out of memory"),
        ));
        lua.set_app_data(ClassMethods(methods));
        Self { lua }
    }

    /// Makes the ModuleScript `module` loadable with `require` on this side.
    /// Its code only runs when a script first requires it.
    pub fn add_module(&self, module: InstanceId, path: &str, source: &str) {
        modules::add(&self.lua, module, path, source);
    }

    /// How long a coroutine may run without pausing before the watchdog
    /// stops it.
    pub fn set_watchdog_limit(&self, limit: std::time::Duration) {
        watchdog::set_limit(&self.lua, limit);
    }

    /// Runs a script's code as a new coroutine until it finishes, fails or
    /// pauses. See [`ScriptRuntime::start_script`](crate::ScriptRuntime::start_script).
    pub fn start_script(&self, world: &mut World, script: InstanceId, path: &str, source: &str) {
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
        let env = sandbox::environment(&self.lua, ids)?;
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

    /// Frame step 2 for this side: fires the RemoteEvent messages that
    /// arrived, then moves the clock forward and resumes due tasks.
    pub fn step(&self, world: &mut World, messages: Vec<Message>, seconds: f64) {
        if let Some(mut connections) = self.lua.app_data_mut::<events::Connections>() {
            connections.forget(|id| world.contains(id));
        }
        with_world(&self.lua, world, || {
            remote::deliver(&self.lua, messages);
            scheduler::step(&self.lua, seconds);
        });
    }

    /// Frame step 4: fires `Touched` on both parts of each pair.
    pub fn fire_touched(&self, world: &mut World, touches: &[(InstanceId, InstanceId)]) {
        with_world(&self.lua, world, || {
            for &(a, b) in touches {
                for (part, other) in [(a, b), (b, a)] {
                    let both_exist = instance::world(&self.lua)
                        .is_ok_and(|world| world.contains(part) && world.contains(other));
                    if both_exist {
                        events::fire(
                            &self.lua,
                            events::Owner::Instance(part),
                            "Touched",
                            instance::LuaInstance(other),
                        );
                    }
                }
            }
        });
    }

    /// Fires `Changed`, property signals, `ChildAdded` and `ChildRemoved`
    /// for `changes`.
    pub fn fire_changes(&self, world: &mut World, changes: &[RecordedChange]) {
        with_world(&self.lua, world, || changes::fire(&self.lua, changes));
    }

    /// Fires `RunService.Heartbeat` with the frame's length in seconds.
    pub fn fire_heartbeat(&self, world: &mut World, seconds: f64) {
        with_world(&self.lua, world, || {
            events::fire(&self.lua, events::Owner::RunService, "Heartbeat", seconds);
        });
    }

    /// Fires a no-argument event on one Instance.
    pub fn fire_instance_event(
        &self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
    ) {
        with_world(&self.lua, world, || {
            if instance::world(&self.lua).is_ok_and(|world| world.contains(instance)) {
                events::fire(&self.lua, events::Owner::Instance(instance), event, ());
            }
        });
    }

    /// Frame step 1 on the client: key presses and releases.
    pub fn fire_input(&self, world: &mut World, keys: &[KeyEvent]) {
        with_world(&self.lua, world, || client::fire_input(&self.lua, keys));
    }

    pub fn local_player(&self) -> Option<InstanceId> {
        self.lua
            .app_data_ref::<LocalPlayer>()
            .map(|player| player.0)
    }

    pub fn set_local_player(&self, player: InstanceId) {
        self.lua.set_app_data(LocalPlayer(player));
    }

    /// Tells scripts where the camera is now (after the player flew it).
    pub fn set_camera(&self, state: CameraState) {
        if let Some(mut camera) = self.lua.app_data_mut::<Camera>() {
            camera.state = state;
        }
    }

    /// Where a script moved the camera, if one did since the last call.
    pub fn take_camera_move(&self) -> Option<CameraState> {
        let mut camera = self.lua.app_data_mut::<Camera>()?;
        std::mem::take(&mut camera.moved).then_some(camera.state)
    }

    pub fn take_output(&self) -> Vec<OutputLine> {
        match self.lua.app_data_mut::<Output>() {
            Some(mut output) => std::mem::take(&mut output.0),
            None => Vec::new(),
        }
    }

    pub fn take_outbox(&self) -> Vec<Message> {
        match self.lua.app_data_mut::<Outbox>() {
            Some(mut outbox) => std::mem::take(&mut outbox.0),
            None => Vec::new(),
        }
    }

    pub fn take_particle_emits(&self) -> Vec<(axcenity_core::InstanceId, u32)> {
        match self.lua.app_data_mut::<crate::ParticleEmits>() {
            Some(mut emits) => std::mem::take(&mut emits.0),
            None => Vec::new(),
        }
    }

    pub fn take_sound_actions(&self) -> Vec<axcenity_audio::SoundAction> {
        match self.lua.app_data_mut::<crate::SoundActions>() {
            Some(mut actions) => std::mem::take(&mut actions.0),
            None => Vec::new(),
        }
    }
}
