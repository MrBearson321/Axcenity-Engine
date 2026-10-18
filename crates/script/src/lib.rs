//! Axcenity scripting: runs Lua 5.4 scripts against the Instance tree.
//!
//! - [`ScriptRuntime`] runs every script of a game. Scripts run on the
//!   server side and LocalScripts on the client side, each side in its own
//!   Lua interpreter (`context`), so they share no globals. RemoteEvents
//!   carry messages between them through a [`Transport`] (`remote`).
//! - Each script runs as a coroutine, and the `scheduler` resumes paused
//!   ones each frame (`task.wait`, `task.spawn` and `task.delay` live in `task`).
//! - Every script gets its own global variables, and only safe parts of the
//!   standard library (see `sandbox`).
//! - The engine API (`game`, `workspace`, `Instance`, `Vector3`, ...) lives in
//!   `api`, `instance` and `types`. Events (`part.Touched:Connect`) live in
//!   `events`, and the host fires them with methods such as
//!   [`ScriptRuntime::fire_touched`]. Client-only APIs (keyboard, camera)
//!   live in `client`, and `sides` says which APIs work on which side.
//! - [`lua_definitions`] describes that API for editors (see `definitions`).
//! - `print` output and errors are collected as [`OutputLine`]s for the host to
//!   show: the player's console, or the editor's Output panel during Play.

mod api;
mod client;
mod context;
mod definitions;
mod errors;
mod events;
mod instance;
mod remote;
mod sandbox;
mod scheduler;
mod sides;
mod task;
mod types;

use axcenity_core::{ClassName, InstanceId, World};
use mlua::Lua;

use crate::context::Context;

pub use client::{CameraState, KeyEvent};
pub use definitions::{DEFINITIONS_PATH, lua_definitions};
pub use remote::{Loopback, Message, RemoteValue, Transport};
pub use sides::{SIDE_ONLY_APIS, Side, SideOnlyApi, side_only};

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
    pub fn start_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) {
        let side = world
            .get(script)
            .and_then(|instance| Side::of_class(instance.class()))
            .unwrap_or(Side::Server);
        if side == Side::Client {
            self.join(world);
        }
        self.context(side).start_script(world, script, path, source);
        self.collect(side);
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

    /// Frame-order step 1: the keys pressed and released since the last
    /// frame, in order. LocalScripts see them through `UserInputService`.
    pub fn input(&mut self, world: &mut World, keys: &[KeyEvent]) {
        if keys.is_empty() {
            return;
        }
        self.join(world);
        self.client.fire_input(world, keys);
        self.collect(Side::Client);
    }

    /// Frame-order step 2, first on the server, then on the client: fires
    /// the RemoteEvent messages that arrived for that side, then moves its
    /// clock forward by `seconds` and resumes every paused script whose
    /// wait is over, and every `task.delay` whose time has come.
    ///
    /// So a message sent before this step (during step 1, or by the server
    /// during this step) arrives in this step; anything later, next frame.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        // A game with no LocalScripts still has its player.
        self.join(world);
        for side in [Side::Server, Side::Client] {
            let messages = self.transport.receive(side);
            self.context(side).step(world, messages, seconds);
            self.collect(side);
        }
    }

    /// Frame-order step 4: fires `Touched` on both parts of each pair that
    /// started touching, in the order given, in Scripts and then in
    /// LocalScripts. Each handler gets the other part and runs as its own
    /// coroutine, so one that waits doesn't hold up the rest. Pairs with a
    /// part that a handler destroyed are skipped.
    pub fn fire_touched(&mut self, world: &mut World, touches: &[(InstanceId, InstanceId)]) {
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_touched(world, touches);
            self.collect(side);
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

    /// Removes and returns the output produced since the last call.
    pub fn take_output(&mut self) -> Vec<OutputLine> {
        std::mem::take(&mut self.output)
    }

    /// Collects what `side` printed, and hands the messages it sent to the
    /// transport.
    fn collect(&mut self, side: Side) {
        let context = match side {
            Side::Server => &self.server,
            Side::Client => &self.client,
        };
        self.output.extend(context.take_output());
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
