//! Axcenity scripting: runs Lua 5.4 scripts against the Instance tree.
//!
//! - [`ScriptRuntime`] runs every script of a game. Scripts run on the
//!   server side and LocalScripts on the client side, each side in its own
//!   Lua interpreter (`context`), so they share no globals. RemoteEvents
//!   carry messages between them through a [`Transport`] (`remote`).
//! - Each script runs as a coroutine, and the `scheduler` resumes paused
//!   ones each frame (`task.wait`, `task.spawn` and `task.delay` live in `task`).
//! - Every script gets its own global variables, and only safe parts of the
//!   standard library (see `sandbox`). `require` loads ModuleScripts (see
//!   `modules`), and the `watchdog` stops a script that runs too long
//!   without pausing.
//! - The engine API (`game`, `workspace`, `Instance`, `Vector3`, ...) lives in
//!   `api`, `instance` and `types`. Events (`part.Touched:Connect`) live in
//!   `events`, and the host fires them with methods such as
//!   [`ScriptRuntime::fire_touched`] and [`ScriptRuntime::heartbeat`]
//!   (`run_service`). Change events (`part.Changed`) come from the World's
//!   change log (`changes`). Client-only APIs (keyboard, camera) live in
//!   `client`, and `sides` says which APIs work on which side.
//! - [`lua_definitions`] describes that API for editors (see `definitions`).
//! - `print` output and errors are collected as [`OutputLine`]s for the host to
//!   show: the player's console, or the editor's Console panel during Play.
//! - The editor's Console runs typed Lua with [`ScriptRuntime::run_command`]
//!   during Play and [`run_edit_command`] outside it (see `console`).

mod api;
mod changes;
mod client;
mod console;
mod context;
mod definitions;
mod errors;
mod events;
mod instance;
mod modules;
mod remote;
mod run_service;
mod sandbox;
mod scheduler;
mod sides;
mod task;
mod types;
mod watchdog;

use std::time::Duration;

use axcenity_audio::SoundAction;
use axcenity_core::{ClassName, InstanceId, Value, World};
use mlua::Lua;

use crate::context::Context;

pub use client::{CameraState, KeyEvent};
pub use definitions::{DEFINITIONS_PATH, lua_definitions};
pub use remote::{Loopback, Message, RemoteValue, Transport};
pub use sides::{SIDE_ONLY_APIS, Side, SideOnlyApi, side_only};
pub use watchdog::DEFAULT_LIMIT as WATCHDOG_LIMIT;

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

/// How many rounds of change events [`ScriptRuntime::fire_changes`] runs in
/// one call. Handlers that change things fire more change events (the next
/// round); a handler that keeps changing what it listens to would never
/// stop, so after this many rounds the rest wait for the next call.
const MAX_CHANGE_ROUNDS: usize = 10;

/// The output lines waiting for the host. A named type, so that this is the
/// only `Vec<OutputLine>` stored in Lua's app data.
#[derive(Default)]
struct Output(Vec<OutputLine>);

#[derive(Default)]
struct SoundActions(Vec<SoundAction>);

/// `emitter:Emit(count)` calls waiting for the host's particles.
#[derive(Default)]
struct ParticleEmits(Vec<(InstanceId, u32)>);

/// Runs every script of one game: Scripts on the server side, LocalScripts
/// on the client side (one player, on this device), and the RemoteEvent
/// messages between them.
pub struct ScriptRuntime {
    server: Context,
    client: Context,
    /// Carries RemoteEvent messages between the two sides.
    transport: Box<dyn Transport>,
    /// Output from both sides, in the order it happened.
    output: Vec<OutputLine>,
    /// Sound commands waiting for the game host to apply.
    sound_actions: Vec<SoundAction>,
    /// Particle bursts waiting for the game host's particles.
    particle_emits: Vec<(InstanceId, u32)>,
    /// Whether the error for change events that never stop has been shown,
    /// so it's shown once rather than every frame.
    reported_change_loop: bool,
}

impl ScriptRuntime {
    /// A runtime whose two sides talk through the in-memory [`Loopback`].
    pub fn new() -> Self {
        Self::with_transport(Box::new(Loopback::default()))
    }

    /// A runtime whose RemoteEvent messages go through `transport`.
    ///
    /// `Box<dyn Transport>` holds any type that implements the `Transport`
    /// trait; which one is only known when the program runs.
    pub fn with_transport(transport: Box<dyn Transport>) -> Self {
        Self {
            server: Context::new(Side::Server),
            client: Context::new(Side::Client),
            transport,
            output: Vec::new(),
            sound_actions: Vec::new(),
            particle_emits: Vec::new(),
            reported_change_loop: false,
        }
    }

    fn context(&self, side: Side) -> &Context {
        match side {
            Side::Server => &self.server,
            Side::Client => &self.client,
        }
    }

    /// Starts one script: runs its code as a new coroutine until it finishes,
    /// fails or pauses (`task.wait`). Paused scripts carry on in [`Self::step`].
    ///
    /// `script` is its Script or LocalScript Instance (the `script` global),
    /// which decides the side it runs on; `path` is the file path shown in
    /// error messages. Errors don't stop anything else: they're added to the
    /// output, and the next script can run as normal.
    ///
    /// Start the Scripts before the LocalScripts, as a real server is running
    /// before any player joins. The player joins (gets a Player in
    /// `Players`) just before the first LocalScript starts.
    ///
    /// A ModuleScript doesn't run here: its code is kept for `require`, on
    /// both sides. So pass the ModuleScripts first, before any script that
    /// might require them.
    pub fn start_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) {
        let class = world.get(script).map(|instance| instance.class());
        if class == Some(ClassName::ModuleScript) {
            self.server.add_module(script, path, source);
            self.client.add_module(script, path, source);
            return;
        }
        let side = class.and_then(Side::of_class).unwrap_or(Side::Server);
        self.fire_changes(world);
        if side == Side::Client {
            self.join(world);
        }
        self.context(side).start_script(world, script, path, source);
        self.collect(side);
        self.fire_changes(world);
    }

    /// The player on this device joins the game, once: a Player Instance
    /// named `Player` goes into `Players`, and becomes `LocalPlayer`.
    fn join(&mut self, world: &mut World) {
        if self.client.local_player().is_some() {
            return;
        }
        let player = world
            .create(ClassName::Player)
            .expect("Player is not a service");
        world.set_name(player, "Player").expect("just created");
        world
            .set_parent(player, Some(world.players()))
            .expect("both exist");
        self.client.set_local_player(player);
    }

    /// The Player of whoever plays on this device, once they've joined.
    pub fn local_player(&self) -> Option<InstanceId> {
        self.client.local_player()
    }

    /// Runs Lua typed into the editor's Console during Play, on the server
    /// side, as a coroutine of its own (so `task.wait` works). Commands
    /// share one set of globals. What it prints, and its errors, join the
    /// output. `Err` is the message for code that isn't valid Lua.
    pub fn run_command(&mut self, world: &mut World, source: &str) -> Result<(), String> {
        self.fire_changes(world);
        let result = self.server.run_command(world, source);
        self.collect(Side::Server);
        self.fire_changes(world);
        result
    }

    /// Frame-order step 1: the keys pressed and released since the last
    /// frame, in order. LocalScripts see them through `UserInputService`.
    pub fn input(&mut self, world: &mut World, keys: &[KeyEvent]) {
        // Changes made since the last frame, such as by the editor.
        self.fire_changes(world);
        if keys.is_empty() {
            return;
        }
        self.join(world);
        self.client.fire_input(world, keys);
        self.collect(Side::Client);
        self.fire_changes(world);
    }

    /// Frame-order step 2, first on the server, then on the client: fires
    /// the RemoteEvent messages that arrived for that side, then moves its
    /// clock forward by `seconds` and resumes every paused script whose
    /// wait is over, and every `task.delay` whose time has come.
    ///
    /// So a message sent before this step (during step 1, or by the server
    /// during this step) arrives in this step; anything later, next frame.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        // A game with no LocalScripts still has its player.
        self.join(world);
        for side in [Side::Server, Side::Client] {
            let messages = self.transport.receive(side);
            self.context(side).step(world, messages, seconds);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame-order step 4: fires `Touched` on both parts of each pair that
    /// started touching, in the order given, in Scripts and then in
    /// LocalScripts. Each handler gets the other part and runs as its own
    /// coroutine, so one that waits doesn't hold up the rest. Pairs with a
    /// part that a handler destroyed are skipped.
    ///
    /// Changes physics made (parts moving) fire their change events first.
    pub fn fire_touched(&mut self, world: &mut World, touches: &[(InstanceId, InstanceId)]) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_touched(world, touches);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// The end of frame step 4, after `Touched`: fires `RunService.Heartbeat`
    /// with the frame's length in seconds, in Scripts and then in LocalScripts.
    pub fn heartbeat(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_heartbeat(world, seconds);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Delivers a button activation only to LocalScripts.
    pub fn activate_ui_button(&mut self, world: &mut World, button: InstanceId) {
        self.fire_changes(world);
        if world
            .get(button)
            .is_some_and(|instance| instance.class() == ClassName::TextButton)
        {
            self.client.fire_instance_event(world, button, "Activated");
            self.collect(Side::Client);
            self.fire_changes(world);
        }
    }

    /// Delivers `FocusLost` only to LocalScripts, when the player finishes a TextBox.
    pub fn release_text_box(&mut self, world: &mut World, text_box: InstanceId) {
        self.fire_changes(world);
        if world
            .get(text_box)
            .is_some_and(|instance| instance.class() == ClassName::TextBox)
        {
            self.client
                .fire_instance_event(world, text_box, "FocusLost");
            self.collect(Side::Client);
            self.fire_changes(world);
        }
    }

    /// Sounds that played to their end (see `GameAudio::update`): sets each
    /// one's `Playing` to false, then fires its `Ended` in Scripts and then
    /// in LocalScripts. Sounds a handler destroyed are skipped.
    pub fn fire_sounds_ended(&mut self, world: &mut World, sounds: &[InstanceId]) {
        self.fire_changes(world);
        for &sound in sounds {
            if world
                .set_property(sound, "Playing", Value::Bool(false))
                .is_err()
            {
                continue;
            }
            self.fire_changes(world);
            for side in [Side::Server, Side::Client] {
                self.context(side)
                    .fire_instance_event(world, sound, "Ended");
                self.collect(side);
                self.fire_changes(world);
            }
        }
    }

    /// Fires `event` of `instance` with `args`, in Scripts and then in
    /// LocalScripts, for events the host finds, such as an Animator's
    /// `StateChanged` (new state, old state). An Instance a handler
    /// destroyed is skipped.
    pub fn fire_instance_event(
        &mut self,
        world: &mut World,
        instance: InstanceId,
        event: &'static str,
        args: &[String],
    ) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side)
                .fire_instance_event_with(world, instance, event, args);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Fires `Changed`, `GetPropertyChangedSignal`, `ChildAdded` and
    /// `ChildRemoved` for everything that changed in `world` since the last
    /// call, in the order it happened, in Scripts and then in LocalScripts.
    ///
    /// Every other method here calls this before and after running scripts,
    /// so the host never has to. Changes made while handlers run fire too,
    /// up to [`MAX_CHANGE_ROUNDS`] rounds; the rest wait for the next call.
    /// Only changes made after the first call are seen: it turns on the
    /// World's change log.
    pub fn fire_changes(&mut self, world: &mut World) {
        world.record_changes();
        for _ in 0..MAX_CHANGE_ROUNDS {
            let changes = world.take_changes();
            if changes.is_empty() {
                return;
            }
            for side in [Side::Server, Side::Client] {
                self.context(side).fire_changes(world, &changes);
                self.collect(side);
            }
        }
        if world.has_changes() && !self.reported_change_loop {
            self.reported_change_loop = true;
            self.output.push(OutputLine {
                kind: OutputKind::Error,
                text: format!(
                    "Changed, ChildAdded or ChildRemoved handlers kept changing things \
                     that fire more of them ({MAX_CHANGE_ROUNDS} rounds in a row), so the \
                     rest will fire next frame. Look for a handler that changes the \
                     property it listens to."
                ),
            });
        }
    }

    /// Tells LocalScripts where the camera is now (`workspace.CurrentCamera`).
    /// Call it each frame after the player has moved the camera.
    pub fn set_camera(&mut self, camera: CameraState) {
        self.client.set_camera(camera);
    }

    /// Where a LocalScript moved the camera, if one did since the last call.
    pub fn take_camera_move(&mut self) -> Option<CameraState> {
        self.client.take_camera_move()
    }

    /// How long a script may run without pausing before the watchdog stops
    /// it with an error. [`WATCHDOG_LIMIT`] (3 seconds) unless changed;
    /// tests use a shorter one.
    pub fn set_watchdog_limit(&mut self, limit: Duration) {
        self.server.set_watchdog_limit(limit);
        self.client.set_watchdog_limit(limit);
    }

    /// Removes and returns the output produced since the last call.
    pub fn take_output(&mut self) -> Vec<OutputLine> {
        std::mem::take(&mut self.output)
    }

    /// Removes sound requests produced since the last call. Hosts apply
    /// these after each script step using their session-owned audio engine.
    pub fn take_sound_actions(&mut self) -> Vec<SoundAction> {
        std::mem::take(&mut self.sound_actions)
    }

    /// Removes the `Emit` calls made since the last call: each emitter and
    /// how many particles it gives off now.
    pub fn take_particle_emits(&mut self) -> Vec<(InstanceId, u32)> {
        std::mem::take(&mut self.particle_emits)
    }

    /// Collects what `side` printed, and hands the messages it sent to the
    /// transport.
    fn collect(&mut self, side: Side) {
        let context = match side {
            Side::Server => &self.server,
            Side::Client => &self.client,
        };
        self.output.extend(context.take_output());
        self.sound_actions.extend(context.take_sound_actions());
        self.particle_emits.extend(context.take_particle_emits());
        for message in context.take_outbox() {
            self.transport.send(message);
        }
    }
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// What [`run_edit_command`] did.
#[derive(Debug)]
pub struct EditCommand {
    /// What the command printed, and its errors.
    pub output: Vec<OutputLine>,
    /// True if it changed `world`.
    pub changed: bool,
}

/// Runs Lua typed into the editor's Console outside Play, against the
/// world being edited, with the same sandbox as scripts. It runs on the
/// server side, in an interpreter of its own that's thrown away after, so
/// nothing it connects or schedules (`task.wait`) lives on. `Err` is the
/// message for code that isn't valid Lua.
pub fn run_edit_command(world: &mut World, source: &str) -> Result<EditCommand, String> {
    let context = Context::new(Side::Server);
    let was_recording = world.is_recording_changes();
    let first = world.next_change_number();
    world.record_changes();
    let result = context.run_command(world, source);
    let changed = world.next_change_number() != first;
    if !was_recording {
        world.stop_recording_changes();
    }
    result.map(|()| EditCommand {
        output: context.take_output(),
        changed,
    })
}

/// Every global name a script has, for the Console's autocomplete.
pub fn global_names() -> Vec<&'static str> {
    sandbox::global_names()
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

#[cfg(test)]
mod client_server_tests;

#[cfg(test)]
mod events_tests;

#[cfg(test)]
mod sandbox_tests;

#[cfg(test)]
mod watchdog_tests;
