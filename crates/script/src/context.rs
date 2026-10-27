//! One side of the game: a Lua interpreter with the scripts of that side.
//!
//! The server and the client each get their own [`Context`], so their
//! scripts can't share Lua globals, tables or metatables: the only way
//! between them is a RemoteEvent. [`ScriptRuntime`](crate::ScriptRuntime)
//! owns both and passes messages between them.

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, RecordedChange, World};
use axcenity_render::{Capabilities, GraphicsSettings};
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
    Output, OutputLine, bindable, changes, console, debris, debugger, events, modules, push_error,
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
        // First, so that every coroutine made from here on is watched.
        watchdog::install(&lua).expect("setting a hook only fails when out of memory");
        // Which side this is, for `sides::check`.
        lua.set_app_data(side);
        lua.set_app_data(Output::default());
        lua.set_app_data(crate::SoundActions::default());
        lua.set_app_data(crate::ParticleEmits::default());
        lua.set_app_data(crate::DebugRays::default());
        lua.set_app_data(Assets::default());
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
            instance::workspace_methods(&lua)
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

    /// Resumes what the pause held, in order (see `debugger`).
    pub fn continue_from_pause(&self, world: &mut World) {
        with_world(&self.lua, world, || {
            debugger::continue_from_pause(&self.lua)
        });
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
    /// arrived, moves tweens on, then moves the clock forward and resumes
    /// due tasks.
    pub fn step(&self, world: &mut World, messages: Vec<Message>, seconds: f64) {
        let tweens = self
            .lua
            .app_data_mut::<tween::Tweens>()
            .map(|mut tweens| tweens.alive())
            .unwrap_or_default();
        if let Some(mut connections) = self.lua.app_data_mut::<events::Connections>() {
            connections.forget(|id| world.contains(id), &tweens);
        }
        if let Some(mut callbacks) = self.lua.app_data_mut::<bindable::Callbacks>() {
            callbacks.forget(|id| world.contains(id));
        }
        with_world(&self.lua, world, || {
            remote::deliver(&self.lua, messages);
            // Before scripts resume, so a script that waited sees this
            // frame's tweened values.
            tween::step(&self.lua, seconds);
            scheduler::step(&self.lua, seconds);
            debris::step(&self.lua);
        });
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

    pub fn take_sound_actions(&self) -> Vec<axcenity_audio::SoundAction> {
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
