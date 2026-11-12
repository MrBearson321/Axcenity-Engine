//! One side of the game: a Lua interpreter with the scripts of that side.
//!
//! The server and the client each get their own [`Context`], so their
//! scripts can't share Lua globals, tables or metatables: the only way
//! between them is a RemoteEvent. [`ScriptRuntime`](crate::ScriptRuntime)
//! owns both and passes messages between them.

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, RecordedChange, World};
use axcenity_render_types::{Capabilities, GraphicsSettings};
use mlua::chunk::ChunkMode;
use mlua::{Lua, LuaOptions, StdLib, Thread};

use crate::camera::{Camera, CameraState, GameCamera};
use crate::client::{LoadingScreenRemoved, LocalPlayer};
use crate::errors::error_message;
use crate::input::{self, Inputs};
use crate::instance::{self, ClassMethods};
use crate::remote::{self, Message, Outbox};
use crate::sides::Side;
use crate::user_settings::UserGraphics;
use crate::{
    Output, OutputLine, bindable, changes, console, debugger, events, modules, push_error,
    run_service, sandbox, scheduler, tree, tween, watchdog, with_world,
};
use axcenity_core::MouseBehavior;
use axcenity_core::input::{InputEvent, InputMap};

pub struct Context {
    pub lua: Lua,
}

impl Context {
    pub fn new(side: Side) -> Self {
        // Only these libraries are loaded at all. `sandbox` then picks the
        // safe parts of them for each script.
        let libraries = StdLib::BIT
            | StdLib::BUFFER
            | StdLib::COROUTINE
            | StdLib::MATH
            | StdLib::OS
            | StdLib::STRING
            | StdLib::TABLE
            | StdLib::UTF8;
        let lua = Lua::new_with(libraries, LuaOptions::default())
            .expect("loading Lua's standard libraries only fails when out of memory");
        // Native code only for `--!native` scripts (`chunks::load` turns it
        // on for those); everything else stays in the interpreter.
        lua.enable_jit(false);
        // What `lua.load` compiles with (`chunks` uses the same).
        lua.set_compiler(crate::sandbox::compiler());
        // First, so that every coroutine made from here on is watched.
        watchdog::install(&lua).expect("setting a hook only fails when out of memory");
        // Which side this is, for `sides::check`.
        lua.set_app_data(side);
        // The slot `with_world` lends the World through.
        crate::lent::install(&lua);
        lua.set_app_data(Output::default());
        lua.set_app_data(crate::SoundActions::default());
        lua.set_app_data(crate::ParticleEmits::default());
        lua.set_app_data(crate::Shatters::default());
        lua.set_app_data(crate::players::Spawned::default());
        lua.set_app_data(crate::players::Spawning::default());
        lua.set_app_data(crate::players::Kicks::default());
        lua.set_app_data(crate::DebugRays::default());
        lua.set_app_data(Assets::default());
        lua.set_app_data(crate::content_provider::PreloadJobs::default());
        lua.set_app_data(Outbox::default());
        lua.set_app_data(scheduler::Scheduler::default());
        lua.set_app_data(events::Connections::default());
        lua.set_app_data(tween::Tweens::default());
        lua.set_app_data(modules::Modules::default());
        lua.set_app_data(bindable::Callbacks::default());
        lua.set_app_data(run_service::RunFlags::default());
        if side == Side::Client {
            lua.set_app_data(Inputs::default());
            lua.set_app_data(Camera::default());
            lua.set_app_data(LoadingScreenRemoved::default());
            lua.set_app_data(UserGraphics::default());
        }
        sandbox::Shared::install(&lua)
            .expect("creating the shared script functions only fails when out of memory");
        let wait = lua
            .app_data_ref::<sandbox::Shared>()
            .map(|shared| shared.wait().clone())
            .expect("installed just above");
        tree::install(&lua, &wait).expect("creating WaitForChild only fails when out of memory");
        sandbox::lock_string_metatable(&lua)
            .expect("changing the string metatable only fails when out of memory");
        let mut methods = remote::methods(&lua)
            .expect("creating the RemoteEvent methods only fails when out of memory");
        methods.extend(
            bindable::methods(&lua)
                .expect("creating the Bindable methods only fails when out of memory"),
        );
        methods.extend(
            instance::sound_methods(&lua)
                .expect("creating Sound methods only fails when out of memory"),
        );
        methods.extend(
            instance::particle_methods(&lua)
                .expect("creating ParticleEmitter methods only fails when out of memory"),
        );
        methods.extend(
            instance::animator_methods(&lua)
                .expect("creating Animator methods only fails when out of memory"),
        );
        methods.extend(
            instance::property_animation_methods(&lua)
                .expect("creating Animation methods only fails when out of memory"),
        );
        methods.extend(
            instance::animation_track_methods(&lua)
                .expect("creating AnimationTrack methods only fails when out of memory"),
        );
        methods.extend(
            crate::query::workspace_methods(&lua)
                .expect("creating Workspace methods only fails when out of memory"),
        );
        methods.extend(
            crate::client::player_methods(&lua)
                .expect("creating Player methods only fails when out of memory"),
        );
        methods.extend(
            crate::cframe::pivot_methods(&lua)
                .expect("creating the pivot methods only fails when out of memory"),
        );
        methods.extend(
            crate::physical::body_methods(&lua)
                .expect("creating the body methods only fails when out of memory"),
        );
        methods.extend(
            crate::csg::methods(&lua)
                .expect("creating the solid modeling methods only fails when out of memory"),
        );
        methods.extend(
            crate::decals::methods(&lua)
                .expect("creating Workspace:SpawnDecal only fails when out of memory"),
        );
        methods.extend(
            crate::seat::methods(&lua)
                .expect("creating Seat methods only fails when out of memory"),
        );
        methods.extend(
            crate::chat::methods(&lua)
                .expect("creating the TextChatService methods only fails when out of memory"),
        );
        methods.extend(
            crate::ownership::methods(&lua)
                .expect("creating the network ownership methods only fails when out of memory"),
        );
        methods.extend(
            crate::players::methods(&lua)
                .expect("creating Player methods only fails when out of memory"),
        );
        methods.extend(
            crate::sequence::methods(&lua)
                .expect("creating Sequence methods only fails when out of memory"),
        );
        methods.extend(
            crate::spline::methods(&lua)
                .expect("creating Spline methods only fails when out of memory"),
        );
        methods.push((
            axcenity_core::ClassName::DataModel,
            "GetService",
            instance::get_service(&lua).expect("only fails when out of memory"),
        ));
        lua.set_app_data(ClassMethods::new(methods));
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

    /// Turns on breakpoints for this side (see `debugger`).
    pub fn enable_debugger(&self) {
        debugger::install(&self.lua).expect("setting a hook only fails when out of memory");
    }

    pub fn set_breakpoints(&self, breakpoints: &debugger::Breakpoints) {
        debugger::set_breakpoints(&self.lua, breakpoints);
    }

    /// Where this side is paused at a breakpoint, if it is.
    pub fn pause(&self) -> Option<debugger::Pause> {
        debugger::pause(&self.lua)
    }

    /// Continues or steps from the pause, resuming what it held in order
    /// (see `debugger`).
    pub fn resume_from_pause(&self, world: &mut World, how: debugger::Resume) {
        with_world(&self.lua, world, || {
            debugger::resume_from_pause(&self.lua, how)
        });
    }

    pub fn set_pause_on_error(&self, on: bool) {
        debugger::set_pause_on_error(&self.lua, on);
    }

    /// A watch expression's value in a paused frame (see `debugger`).
    pub fn evaluate(
        &self,
        world: &mut World,
        frame: usize,
        expression: &str,
    ) -> Result<String, String> {
        with_world(&self.lua, world, || {
            debugger::evaluate(&self.lua, frame, expression)
        })
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
                Ok(thread) => scheduler::resume_as(&self.lua, &thread, (), Some(script)),
                Err(err) => push_error(&self.lua, &err),
            }
        });
    }

    /// Stops `script` without touching the world: disconnects its handlers
    /// and drops its waiting coroutines, so none of its code runs again.
    pub fn stop_script(&self, script: InstanceId) {
        crate::hot_reload::forget(&self.lua, script);
        if let Some(mut connections) = self.lua.app_data_mut::<events::Connections>() {
            connections.forget_script(script);
        }
        if let Ok(mut scheduler) = scheduler::scheduler_mut(&self.lua) {
            scheduler.forget_script(script);
        }
        if let Some(mut modules) = self.lua.app_data_mut::<modules::Modules>() {
            modules.forget_requirer(script);
        }
        if let Some(mut inputs) = self.lua.app_data_mut::<Inputs>() {
            inputs.forget_script(script);
        }
        if let Some(mut callbacks) = self.lua.app_data_mut::<bindable::Callbacks>() {
            callbacks.forget_script(script);
        }
        if let Some(mut functions) = self.lua.app_data_mut::<remote::RemoteFunctions>() {
            functions.forget_script(script);
        }
        if let Some(mut steps) = self.lua.app_data_mut::<run_service::RenderSteps>() {
            steps.forget_script(script);
        }
        if let Some(mut jobs) = self.lua.app_data_mut::<crate::csg::CsgJobs>() {
            jobs.forget_script(script);
        }
        if let Some(mut jobs) = self
            .lua
            .app_data_mut::<crate::content_provider::PreloadJobs>()
        {
            jobs.forget_script(script);
        }
        if let Some(mut waits) = self.lua.app_data_mut::<crate::network::StreamWaits>() {
            waits.forget_script(script);
        }
        if let Some(mut searches) = self.lua.app_data_mut::<crate::network::LanSearches>() {
            searches.forget_script(script);
        }
        if let Some(mut commands) = self
            .lua
            .app_data_mut::<crate::console_service::ConsoleCommands>()
        {
            commands.forget_script(script);
        }
    }

    /// Compiles `source` without running it, for its syntax errors.
    pub fn check_syntax(&self, path: &str, source: &str) -> mlua::Result<()> {
        self.lua
            .load(source)
            .set_name(format!("@{path}"))
            .set_mode(ChunkMode::Text)
            .into_function()
            .map(|_| ())
    }

    /// Hot reload (Phase 89): reads `script`'s top-level locals before it
    /// stops, for its next run (see `hot_reload`).
    pub fn hot_snapshot(&self, script: InstanceId) {
        crate::hot_reload::snapshot(&self.lua, script);
    }

    /// Hot reload: the names `script`'s new run kept, now it has started.
    pub fn hot_finish(&self, script: InstanceId) -> Vec<String> {
        crate::hot_reload::finish(&self.lua, script)
    }

    /// Hot reload of a ModuleScript that loaded as a table on this side:
    /// runs its new code now with its locals kept, and copies the new table
    /// into the one scripts already hold. Returns the scripts that required
    /// it (they run again, keeping their state) and the names it kept, or
    /// `None` when hot reload is off or it didn't load as a table here.
    pub fn reload_module_keeping(
        &self,
        world: &mut World,
        module: InstanceId,
        path: &str,
        source: &str,
    ) -> Option<(std::collections::BTreeSet<InstanceId>, Vec<String>)> {
        if !crate::hot_reload::enabled(&self.lua) {
            return None;
        }
        let old = modules::loaded_table(&self.lua, module)?;
        crate::hot_reload::snapshot(&self.lua, module);
        // What the module's last hot run started belongs to it.
        self.stop_script(module);
        let scripts = modules::replace_keeping(&self.lua, module, path, source, old);
        let ids = sandbox::ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: module,
        };
        with_world(&self.lua, world, || {
            // `require(module)` in a coroutine of its own, as a script's
            // first `require` runs it.
            let thread = sandbox::environment(&self.lua, ids)
                .and_then(|env| env.raw_get::<mlua::Function>("require"))
                .and_then(|require| self.lua.create_thread(require));
            match thread {
                Ok(thread) => scheduler::resume_as(
                    &self.lua,
                    &thread,
                    crate::instance::LuaInstance(module),
                    Some(module),
                ),
                Err(err) => push_error(&self.lua, &err),
            }
        });
        modules::forget_self(&self.lua, module);
        let kept = crate::hot_reload::finish(&self.lua, module);
        Some((scripts, kept))
    }

    /// Gives the ModuleScript `module` new code (see `modules::replace`),
    /// and returns the scripts on this side that required it.
    pub fn replace_module(
        &self,
        module: InstanceId,
        path: &str,
        source: &str,
    ) -> std::collections::BTreeSet<InstanceId> {
        modules::replace(&self.lua, module, path, source)
    }

    /// Runs a Console command as a new coroutine until it finishes, fails or
    /// pauses (see `console`). `Err` is the message for code that isn't
    /// valid Lua; other errors go to the output, as a script's do.
    pub fn run_command(&self, world: &mut World, source: &str) -> Result<(), String> {
        let function = console::environment(&self.lua, world)
            .map_err(|err| error_message(&err))
            .and_then(|env| console::compile(&self.lua, &env, source))?;
        let thread = self
            .lua
            .create_thread(function)
            .map_err(|err| error_message(&err))?;
        with_world(&self.lua, world, || {
            scheduler::resume(&self.lua, &thread, ())
        });
        Ok(())
    }

    /// The commands scripts on this side registered with
    /// `ConsoleService:RegisterCommand`, by name.
    pub fn console_commands(&self) -> Vec<crate::ConsoleCommandInfo> {
        crate::console_service::commands(&self.lua)
    }

    /// Starts this side's console command `name` with `args`, for
    /// `player`. `remote` says the player typed it on another device, so
    /// they must be in `AllowedUserIds`.
    pub fn run_console_command(
        &self,
        world: &mut World,
        name: &str,
        args: &[String],
        player: Option<InstanceId>,
        remote: bool,
    ) -> crate::ConsoleRun {
        let Some(command) = crate::console_service::find(&self.lua, name) else {
            return crate::ConsoleRun::Unknown;
        };
        if remote && !crate::console_service::allowed(&self.lua, world, player) {
            return crate::ConsoleRun::NotAllowed;
        }
        with_world(&self.lua, world, || {
            crate::console_service::start(&self.lua, command, player, args);
        });
        crate::ConsoleRun::Ran
    }

    /// Whether `name` means something to Lua typed in the console: one of
    /// its globals, or one an earlier command set.
    pub fn console_knows(&self, world: &World, name: &str) -> bool {
        console::environment(&self.lua, world)
            .and_then(|env| env.get::<mlua::Value>(name))
            .is_ok_and(|value| !value.is_nil())
    }

    /// Compiles a script into a coroutine that hasn't started yet.
    fn load_script(
        &self,
        ids: sandbox::ScriptIds,
        path: &str,
        source: &str,
    ) -> mlua::Result<Thread> {
        let env = sandbox::environment(&self.lua, ids)?;
        // Compiled once per source (see `chunks`), so clones start quickly.
        let function = crate::chunks::load_hot(&self.lua, ids.script, path, source, env)?;
        self.lua.create_thread(function)
    }

    /// Frame step 2 for this side: fires the RemoteEvent messages that
    /// arrived, moves tweens on, then moves the clock forward and resumes
    /// due tasks. `destroyed` is what was destroyed since the last step
    /// ([`World::take_destroyed`]).
    pub fn step(
        &self,
        world: &mut World,
        messages: Vec<Message>,
        seconds: f64,
        destroyed: &[InstanceId],
    ) {
        self.forget_gone(world, destroyed);
        with_world(&self.lua, world, || {
            remote::deliver(&self.lua, messages);
            // Before scripts resume, so a script that waited sees this
            // frame's tweened values.
            tween::step(&self.lua, seconds);
            scheduler::step(&self.lua, seconds);
            crate::csg::step(&self.lua);
            crate::content_provider::step(&self.lua);
            crate::system::step(&self.lua);
            crate::decals::step(&self.lua);
        });
    }

    /// Drops the handlers and callbacks of what went away since the last
    /// step, so their functions (and whatever those use) can be freed. The
    /// cost follows what went away, not how many handlers exist.
    fn forget_gone(&self, world: &World, destroyed: &[InstanceId]) {
        let dead_tweens = self
            .lua
            .app_data_mut::<tween::Tweens>()
            .map(|mut tweens| tweens.take_dead())
            .unwrap_or_default();
        let streamed_out = self.lua.app_data_ref::<crate::network::StreamedOut>();
        // A handler on something streamed out is waiting for it to come
        // back (Phase 80).
        let is_out = |id| streamed_out.as_ref().is_some_and(|out| out.0.contains(&id));
        let gone = |id| !world.contains(id) && !is_out(id);
        if let Some(mut connections) = self.lua.app_data_mut::<events::Connections>() {
            for &id in destroyed.iter().filter(|&&id| gone(id)) {
                connections.forget_owner(events::Owner::Instance(id));
            }
            for tween in dead_tweens {
                connections.forget_owner(events::Owner::Tween(tween));
            }
            for owner in connections.take_connected_while_gone() {
                match owner {
                    events::Owner::Instance(id) if !gone(id) => {}
                    owner => connections.forget_owner(owner),
                }
            }
        }
        drop(streamed_out);
        if let Some(mut callbacks) = self.lua.app_data_mut::<bindable::Callbacks>() {
            for &id in destroyed.iter().filter(|&&id| !world.contains(id)) {
                callbacks.forget_instance(id);
            }
        }
    }

    /// Drops the handlers and callbacks of every Instance that doesn't
    /// exist, for when the whole World was replaced.
    pub fn forget_missing_instances(&self, world: &World) {
        if let Some(mut connections) = self.lua.app_data_mut::<events::Connections>() {
            connections.forget_missing_instances(|id| world.contains(id));
        }
        if let Some(mut callbacks) = self.lua.app_data_mut::<bindable::Callbacks>() {
            callbacks.forget_missing_instances(|id| world.contains(id));
        }
    }

    /// Fires `RunService.Stepped`, before physics.
    pub fn fire_stepped(&self, world: &mut World, seconds: f64) {
        with_world(&self.lua, world, || {
            run_service::fire_stepped(&self.lua, seconds)
        });
    }

    /// Runs the functions bound to the render step, then fires
    /// `RunService.RenderStepped`, just before the frame is drawn.
    pub fn render_step(&self, world: &mut World, seconds: f64) {
        with_world(&self.lua, world, || {
            run_service::render_step(&self.lua, seconds)
        });
    }

    /// Whether this is the editor's Play (`RunService:IsStudio`), and
    /// whether the game runs (`IsRunning`).
    pub fn set_run_flags(&self, flags: run_service::RunFlags) {
        self.lua.set_app_data(flags);
    }

    /// Frame step 4: fires `event` (`Touched` or `TouchEnded`) on both
    /// parts of each pair.
    pub fn fire_touch_event(
        &self,
        world: &mut World,
        event: &'static str,
        pairs: &[(InstanceId, InstanceId)],
    ) {
        with_world(&self.lua, world, || {
            for &(a, b) in pairs {
                for (part, other) in [(a, b), (b, a)] {
                    let both_exist = instance::world(&self.lua)
                        .is_ok_and(|world| world.contains(part) && world.contains(other));
                    if both_exist {
                        events::fire(
                            &self.lua,
                            events::Owner::Instance(part),
                            event,
                            instance::LuaInstance(other),
                        );
                    }
                }
            }
        });
    }

    /// Frame step 1: fires `Seated(active, seat)` on each controller.
    pub fn fire_seated(&self, world: &mut World, changes: &[(InstanceId, bool, InstanceId)]) {
        with_world(&self.lua, world, || {
            for &(controller, active, seat) in changes {
                let exists = instance::world(&self.lua)
                    .is_ok_and(|world| world.contains(controller) && world.contains(seat));
                if exists {
                    events::fire(
                        &self.lua,
                        events::Owner::Instance(controller),
                        "Seated",
                        (active, instance::LuaInstance(seat)),
                    );
                }
            }
        });
    }

    /// Fires each prompt and click detector event on its Instance, with
    /// `player` for those that pass the Player, skipping Instances that
    /// are gone (Phase 44).
    pub fn fire_prompt_events(
        &self,
        world: &mut World,
        fired: &[crate::prompt::Fired],
        player: Option<InstanceId>,
    ) {
        with_world(&self.lua, world, || {
            for event in fired {
                if !instance::world(&self.lua).is_ok_and(|world| world.contains(event.instance)) {
                    continue;
                }
                let owner = events::Owner::Instance(event.instance);
                if event.with_player {
                    let player = player.map(instance::LuaInstance);
                    events::fire(&self.lua, owner, event.event, player);
                } else {
                    events::fire(&self.lua, owner, event.event, ());
                }
            }
        });
    }

    /// This frame's input for prompts and click detectors (client).
    pub fn prompt_input(&self) -> input::PromptInput {
        input::prompt_input(&self.lua)
    }

    /// The camera's place, and the ray through `mouse` (client).
    pub fn mouse_ray(&self, mouse: [f32; 2]) -> Option<crate::camera::Ray> {
        let camera = self.lua.app_data_ref::<Camera>()?;
        Some(camera.ray(mouse[0], mouse[1]))
    }

    /// Fires `event` of `owner` with the Instance `arg`, if `owner` is
    /// still there, such as `CharacterAdded(character)` on a Player.
    pub fn fire_with_instance(
        &self,
        world: &mut World,
        owner: InstanceId,
        event: &'static str,
        arg: InstanceId,
    ) {
        with_world(&self.lua, world, || {
            if instance::world(&self.lua).is_ok_and(|world| world.contains(owner)) {
                let owner = events::Owner::Instance(owner);
                events::fire(&self.lua, owner, event, instance::LuaInstance(arg));
            }
        });
    }

    /// `player:LoadCharacter()` from the runtime (server): a join or a
    /// respawn. Its events wait in [`Self::take_spawned`].
    pub fn load_character(&self, world: &mut World, player: InstanceId) {
        with_world(&self.lua, world, || {
            let _ = crate::players::load_character(&self.lua, player);
        });
    }

    /// Notes `template` as what players respawn as without a
    /// StarterCharacter: the copy of a character placed in the Workspace.
    pub fn set_respawn_template(&self, template: InstanceId) {
        if let Some(mut spawning) = self.lua.app_data_mut::<crate::players::Spawning>() {
            spawning.template = Some(template);
        }
    }

    /// What `LoadCharacter` did since the last call.
    pub fn take_spawned(&self) -> crate::players::Spawned {
        self.lua
            .app_data_mut::<crate::players::Spawned>()
            .map(|mut spawned| std::mem::take(&mut *spawned))
            .unwrap_or_default()
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

    /// Fires `TextChatService.MessageReceived` with the message (Phase 67).
    pub fn fire_chat_message(&self, world: &mut World, player: Option<InstanceId>, text: &str) {
        let service = world.text_chat_service();
        with_world(&self.lua, world, || {
            if let Ok(message) = crate::chat::message_table(&self.lua, player, text) {
                events::fire(
                    &self.lua,
                    events::Owner::Instance(service),
                    "MessageReceived",
                    message,
                );
            }
        });
    }

    /// Runs `TextChatService.OnIncomingMessage` on a message: the text to
    /// send on, or `None` to drop it. Without one, the message as it is;
    /// an error is reported and keeps it as it is.
    pub fn filter_chat(
        &self,
        world: &mut World,
        player: Option<InstanceId>,
        text: &str,
    ) -> Option<String> {
        let service = world.text_chat_service();
        let mut result = Some(text.to_owned());
        with_world(&self.lua, world, || {
            let Some((callback, _)) =
                crate::bindable::callback(&self.lua, service, "OnIncomingMessage")
            else {
                return;
            };
            let outcome = crate::chat::message_table(&self.lua, player, text)
                .and_then(|message| callback.call::<mlua::Value>(message));
            match outcome {
                Ok(mlua::Value::String(changed)) => {
                    result = crate::chat::clean(&changed.to_string_lossy());
                }
                Ok(mlua::Value::Nil | mlua::Value::Boolean(false)) => result = None,
                Ok(_) => {}
                Err(err) => crate::push_output(
                    &self.lua,
                    crate::OutputKind::Error,
                    format!("TextChatService.OnIncomingMessage: {err}"),
                ),
            }
        });
        result
    }

    /// Fires `LevelService.LevelLoaded` with the level's name (Phase 48).
    pub fn fire_level_loaded(&self, world: &mut World, name: &str) {
        with_world(&self.lua, world, || {
            let name = name.to_owned();
            events::fire(&self.lua, events::Owner::LevelService, "LevelLoaded", name);
        });
    }

    /// Fires `NetworkService.ConnectionFailed` with why (Phase 63).
    pub fn fire_connection_failed(&self, world: &mut World, reason: &str) {
        with_world(&self.lua, world, || {
            let reason = reason.to_owned();
            events::fire(
                &self.lua,
                events::Owner::NetworkService,
                "ConnectionFailed",
                reason,
            );
        });
    }

    /// Resumes the scripts waiting in `NetworkService:FindLanGames()`.
    pub fn finish_lan_search(&self, world: &mut World, games: &[crate::network::LanGame]) {
        with_world(&self.lua, world, || {
            crate::network::finish_search(&self.lua, games);
        });
    }

    /// Streaming (Phase 80): `gone` left this device (or was never sent
    /// and is `StreamingState` StreamedOut), and `arrived` came back, which
    /// fires `StreamedIn` on each that had gone.
    pub fn note_streaming(&self, world: &mut World, gone: &[InstanceId], arrived: &[InstanceId]) {
        if self
            .lua
            .app_data_ref::<crate::network::StreamedOut>()
            .is_none()
        {
            self.lua
                .set_app_data(crate::network::StreamedOut::default());
        }
        let mut back = Vec::new();
        if let Some(mut out) = self.lua.app_data_mut::<crate::network::StreamedOut>() {
            out.0.extend(gone.iter().copied());
            for &id in arrived {
                if out.0.remove(&id) {
                    back.push(id);
                }
            }
        }
        with_world(&self.lua, world, || {
            for id in back {
                events::fire(&self.lua, events::Owner::Instance(id), "StreamedIn", ());
            }
        });
    }

    /// Resumes the script waiting on a `RequestStreamAroundAsync`.
    pub fn finish_stream_around(&self, world: &mut World, request: u64) {
        with_world(&self.lua, world, || {
            crate::network::finish_stream_around(&self.lua, request);
        });
    }

    /// The `NetworkService` calls made on this side since the last take.
    pub fn take_network_requests(&self) -> Vec<crate::network::NetworkRequest> {
        crate::network::take_requests(&self.lua)
    }

    /// The `ReplayService` calls made on this side since the last take.
    pub fn take_replay_requests(&self) -> Vec<crate::replay::ReplayRequest> {
        crate::replay::take_requests(&self.lua)
    }

    /// Fires a no-argument event on one Instance.
    pub fn fire_instance_event(
        &self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
    ) {
        self.fire_instance_event_with(world, instance, event, &[]);
    }

    /// Like [`fire_instance_event`](Self::fire_instance_event), giving
    /// handlers `args`.
    pub fn fire_instance_event_with(
        &self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
        args: &[String],
    ) {
        with_world(&self.lua, world, || {
            if instance::world(&self.lua).is_ok_and(|world| world.contains(instance)) {
                let owner = events::Owner::Instance(instance);
                let variadic = mlua::Variadic::from_iter(args.iter().cloned());
                events::fire(&self.lua, owner, event, variadic);
                // A track's events also reach `GetMarkerReachedSignal(name)`.
                if event == "KeyframeReached"
                    && let Some(name) = args.first()
                {
                    events::fire_marker(&self.lua, owner, name);
                }
            }
        });
    }

    /// Frame step 1 on the client: the frame's input.
    pub fn fire_input(&self, world: &mut World, events: &[InputEvent]) {
        with_world(&self.lua, world, || input::fire_input(&self.lua, events));
    }

    pub fn set_input_map(&self, map: InputMap) {
        if let Some(mut inputs) = self.lua.app_data_mut::<Inputs>() {
            inputs.map = map;
        }
    }

    /// The input map, with the player's rebinds.
    pub fn input_map(&self) -> InputMap {
        input::input_map(&self.lua)
    }

    /// `UserInputService.MouseBehavior` and `MouseIconEnabled`.
    pub fn mouse(&self) -> (MouseBehavior, bool) {
        self.lua
            .app_data_ref::<Inputs>()
            .map_or((MouseBehavior::Default, true), |inputs| {
                (inputs.mouse_behavior, inputs.mouse_icon_enabled)
            })
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
            camera.set_host(state);
        }
    }

    /// The view's size in pixels, for `ViewportSize` and screen points.
    pub fn set_viewport_size(&self, width: f32, height: f32) {
        if let Some(mut camera) = self.lua.app_data_mut::<Camera>() {
            camera.viewport = axcenity_core::Vector2::new(width, height);
        }
    }

    /// Where a script moved a Custom camera, if one did since the last call.
    pub fn take_camera_move(&self) -> Option<CameraState> {
        self.lua.app_data_mut::<Camera>()?.take_move()
    }

    /// Moves a following camera and the shakes on, and says what to draw.
    pub fn step_camera(&self, world: &World, seconds: f64) -> GameCamera {
        self.lua
            .app_data_mut::<Camera>()
            .map(|mut camera| camera.step(world, seconds))
            .unwrap_or_default()
    }

    /// What a Follow or Track camera follows (`CameraSubject`), if it's one
    /// of those.
    pub fn camera_followed(&self) -> Option<InstanceId> {
        let camera = self.lua.app_data_ref::<Camera>()?;
        let following = matches!(
            camera.camera_type,
            crate::camera::CameraType::Follow | crate::camera::CameraType::Track
        );
        camera.subject.filter(|_| following)
    }

    /// Whether scripts hold the camera (any `CameraType` but Custom), so
    /// the host shouldn't fly or orbit it.
    pub fn camera_scripted(&self) -> bool {
        self.lua
            .app_data_ref::<Camera>()
            .is_some_and(|camera| camera.camera_type != crate::camera::CameraType::Custom)
    }

    pub fn set_graphics(&self, settings: &GraphicsSettings, capabilities: &Capabilities) {
        if let Some(mut graphics) = self.lua.app_data_mut::<UserGraphics>() {
            graphics.settings = settings.clone();
            graphics.capabilities = *capabilities;
        }
    }

    pub fn take_graphics_change(&self) -> Option<GraphicsSettings> {
        let mut graphics = self.lua.app_data_mut::<UserGraphics>()?;
        std::mem::take(&mut graphics.changed).then(|| graphics.settings.clone())
    }

    pub fn take_loading_screen_removed(&self) -> bool {
        self.lua
            .app_data_mut::<LoadingScreenRemoved>()
            .is_some_and(|mut removed| std::mem::take(&mut removed.0))
    }

    pub fn take_output(&self) -> Vec<OutputLine> {
        match self.lua.app_data_mut::<Output>() {
            Some(mut output) => std::mem::take(&mut output.0),
            None => Vec::new(),
        }
    }

    /// Scripts `Clone` copied since the last call: each copy and its original.
    pub fn take_cloned_scripts(&self) -> Vec<(InstanceId, InstanceId)> {
        match self.lua.app_data_mut::<tree::ClonedScripts>() {
            Some(mut cloned) => std::mem::take(&mut cloned.0),
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

    /// The models and tables this side's queries use.
    pub fn assets(&self) -> Assets {
        self.lua
            .app_data_ref::<Assets>()
            .as_deref()
            .cloned()
            .unwrap_or_default()
    }

    pub fn take_shatters(&self) -> Vec<crate::Shatter> {
        match self.lua.app_data_mut::<crate::Shatters>() {
            Some(mut shatters) => std::mem::take(&mut shatters.0),
            None => Vec::new(),
        }
    }

    pub fn take_sound_actions(&self) -> Vec<axcenity_core::sound::SoundAction> {
        match self.lua.app_data_mut::<crate::SoundActions>() {
            Some(mut actions) => std::mem::take(&mut actions.0),
            None => Vec::new(),
        }
    }

    /// Models `workspace:Raycast` uses for MeshParts.
    pub fn set_assets(&self, assets: Assets) {
        self.lua.set_app_data(assets);
    }

    /// Recent raycasts on this side, oldest first.
    pub fn debug_rays(&self) -> Vec<crate::DebugRay> {
        match self.lua.app_data_ref::<crate::DebugRays>() {
            Some(rays) => rays.0.iter().copied().collect(),
            None => Vec::new(),
        }
    }
}
