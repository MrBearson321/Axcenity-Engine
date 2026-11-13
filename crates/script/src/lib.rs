//! Axcenity scripting: runs Luau scripts against the Instance tree.
//!
//! - [`ScriptRuntime`] runs every script of a game. Scripts run on the
//!   server side and LocalScripts on the client side, each side in its own
//!   Lua interpreter (`context`), so they share no globals. RemoteEvents
//!   carry messages between them through a [`Transport`] (`remote`).
//! - Each script runs as a coroutine, and the `scheduler` resumes paused
//!   ones each frame (`task.wait`, `task.spawn`, `task.delay`, `task.defer`
//!   and `task.cancel` live in `task`).
//! - Every script gets its own global variables, and only safe parts of the
//!   standard library (see `sandbox`). `require` loads ModuleScripts (see
//!   `modules`), and the `watchdog` stops a script that runs too long
//!   without pausing.
//! - The engine API (`game`, `workspace`, `Instance`, `Vector3`, ...) lives in
//!   `api`, `instance` and `types`. Events (`part.Touched:Connect`) live in
//!   `events`, and the host fires them with methods such as
//!   [`ScriptRuntime::fire_touched`] and [`ScriptRuntime::heartbeat`]
//!   (`run_service`). BindableEvents and BindableFunctions let scripts on
//!   one side talk (`bindable`); `HttpService` reads and writes JSON (`http`), and
//!   `DataService` reads the project's data tables (`data`), and
//!   `NetworkService` asks the host to host, join and search (`network`).
//!   `TweenService` tweens move properties smoothly,
//!   each frame step (`tween`). `SaveService` writes and loads save slots
//!   and checkpoints (`save`, over the `save` crate). Change events (`part.Changed`) come from the World's
//!   change log (`changes`). Client-only APIs (keyboard, camera) live in
//!   `client`, and `sides` says which APIs work on which side.
//!   `UserGameSettings` (`user_settings`) lets LocalScripts change the
//!   player's graphics settings.
//! - Game systems from other crates (the `gameplay` crate's Debris, for
//!   one) add services and a per-frame step through [`GameSystem`]
//!   ([`ScriptRuntime::add_system`], see `system`).
//! - [`lua_definitions`] describes that API for editors, and [`api_reference`]
//!   lists it for the editor's API reference (see `definitions`).
//! - `print` output and errors are collected as [`OutputLine`]s for the host to
//!   show: the player's console, or the editor's Console panel during Play.
//! - The editor's Console runs typed Lua with [`ScriptRuntime::run_command`]
//!   during Play and [`run_edit_command`] outside it (see `console`).
//! - The editor's breakpoints pause its Play on a line, show the paused
//!   script's locals, and continue ([`ScriptRuntime::enable_debugger`], see
//!   `debugger`). The player never turns them on, and scripts can't.
//! - [`ScriptRuntime::reload_script`] runs a script's changed file in place
//!   while the game runs: its handlers and waiting coroutines go, and the
//!   new code runs against the same world.
//! - [`ScriptRuntime::start_spec`] runs a game test's spec file for the
//!   `testing` crate's runner (see `spec`).

mod api;
mod attachment;
pub mod audio_settings;
mod bindable;
mod camera;
mod cframe;
mod changes;
mod chat;
mod chunks;
mod click;
mod client;
mod collection;
mod console;
mod console_service;
mod content_provider;
mod context;
#[cfg(feature = "csg")]
mod csg;
#[cfg(not(feature = "csg"))]
#[path = "csg_off.rs"]
mod csg;
mod data;
mod debugger;
mod decals;
mod definitions;
mod edit;
mod errors;
mod events;
mod gui_service;
mod handler_threads;
pub mod hot_reload;
mod http;
mod input;
mod instance;
mod keypoints;
mod lag;
mod lent;
mod levels;
mod localization;
mod modules;
mod network;
mod ownership;
mod physical;
mod players;
mod prompt;
mod query;
mod random;
mod remote;
mod replay;
mod run_service;
mod runtime;
mod sandbox;
mod save;
mod scheduler;
mod seat;
mod sequence;
mod sides;
mod spec;
mod spline;
pub mod system;
mod task;
mod tree;
mod tween;
mod types;
mod user_settings;
mod watchdog;

use std::cell::Ref;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use axcenity_assets::Assets;
use axcenity_core::input::{InputEvent, InputMap};
use axcenity_core::sound::SoundAction;
use axcenity_core::{
    Change, ClassName, InstanceId, KeyCode, MouseBehavior, RecordedChange, Value, Vector3, World,
};
use axcenity_physics::RayHit;
use axcenity_render_types::{Capabilities, GraphicsSettings};
use axcenity_save::Saves;
use mlua::Lua;

use crate::context::Context;
use crate::lent::with_world;

pub use camera::{CameraState, CameraType, CameraView, DEFAULT_FIELD_OF_VIEW, GameCamera};
pub use chat::ChatLine;
pub use console_service::{ConsoleCommandInfo, ConsoleRun, split_command};
pub use debugger::{
    BreakpointRule, BreakpointRules, Breakpoints, EXPANDED_ENTRIES, Local, Pause, PausedFrame,
    Resume,
};
pub use definitions::{
    API_DOCS_PATH, ApiEntry, ApiKind, DEFINITIONS_PATH, SUMMARY_HEADING, SUMMARY_PATH, api_example,
    api_markdown, api_reference, api_summary, lua_definitions, property_description,
    with_api_summary,
};
pub use edit::EditRuntime;
pub use errors::error_message as error_text;
pub use input::{KeyEvent, PromptInput};
pub use instance::LuaInstance;
pub use lent::{WorldMut, WorldRef, world as lent_world, world_mut as lent_world_mut};
pub use levels::Levels;
pub use network::{LanGame, NetworkRequest};
pub use prompt::ShownPrompt;
pub use remote::{Loopback, Message, MessageKind, RemoteValue, Transport};
pub use replay::{MAX_BUFFER_SECONDS, ReplayRequest};
pub use sandbox::{SyntaxError, find_syntax_error};
pub use sequence::sound_actions as sequence_sound_actions;
pub use sides::{SIDE_ONLY_APIS, Side, SideOnlyApi, side_only};
pub use spec::{FoundTest, TestReport, TestRequest};
pub use system::GameSystem;
pub use watchdog::DEFAULT_LIMIT as WATCHDOG_LIMIT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    /// Text from `print`.
    Print,
    /// A script error, starting with the script's path and line.
    Error,
    /// Something that may be a mistake but doesn't stop the script, such
    /// as a `WaitForChild` that has waited a long time.
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
    /// For an error raised while a script ran, the script calls that led
    /// to it, innermost first: `scripts/util.module.lua:5: in field 'explode'`.
    /// Empty for `print`, syntax errors and engine messages.
    pub trace: Vec<String>,
    /// The side whose scripts it came from; `None` for the runtime's own
    /// messages, such as a script that can't start.
    pub side: Option<Side>,
}

/// How many rounds of change events [`ScriptRuntime::fire_changes`] runs in
/// one call. Handlers that change things fire more change events (the next
/// round); a handler that keeps changing what it listens to would never
/// stop, so after this many rounds the rest wait for the next call. Game
/// systems that fire their own rounds keep to it too.
pub const MAX_CHANGE_ROUNDS: usize = 10;

/// The output lines waiting for the host. A named type, so that this is the
/// only `Vec<OutputLine>` stored in Lua's app data.
#[derive(Default)]
struct Output(Vec<OutputLine>);

#[derive(Default)]
struct SoundActions(Vec<SoundAction>);

/// `emitter:Emit(count)` calls waiting for the host's particles.
#[derive(Default)]
struct ParticleEmits(Vec<(InstanceId, u32)>);

/// `fracture:Shatter(point, force)` calls waiting for the host's physics.
#[derive(Default)]
struct Shatters(Vec<Shatter>);

/// A script's `fracture:Shatter(point, force)`: the Fracture, where the
/// pieces fly from (`None` for the part's middle), and how hard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shatter {
    pub fracture: InstanceId,
    pub point: Option<axcenity_core::Vector3>,
    pub force: f32,
}

/// How many recent `workspace:Raycast` calls the editor overlay can show.
const MAX_DEBUG_RAYS: usize = 64;

/// A `workspace:Raycast` the editor can draw while the raycast overlay is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugRay {
    pub origin: Vector3,
    pub direction: Vector3,
    pub hit: Option<RayHit>,
}

/// The last [`MAX_DEBUG_RAYS`] casts on one side, oldest first.
#[derive(Default)]
pub(crate) struct DebugRays(VecDeque<DebugRay>);

impl DebugRays {
    pub(crate) fn push(&mut self, ray: DebugRay) {
        if self.0.len() == MAX_DEBUG_RAYS {
            self.0.pop_front();
        }
        self.0.push_back(ray);
    }
}

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
    /// Shatters waiting for the game host's physics.
    shatters: Vec<Shatter>,
    /// Game systems from other crates (roadmap L7), in the order added.
    systems: Vec<std::rc::Rc<dyn GameSystem>>,
    /// The ProximityPrompts showing and their holds (Phase 44).
    prompts: prompt::Prompts,
    /// What the mouse points at and presses, for ClickDetectors.
    clicks: click::Clicks,
    /// Respawns, spawn ForceFields and the leaderboard's Tab (Phase 47).
    players: players::Players,
    /// Player events waiting to fire on both sides.
    player_events: Vec<players::PlayerEvent>,
    /// Whether the error for change events that never stop has been shown,
    /// so it's shown once rather than every frame.
    reported_change_loop: bool,
    /// `SaveService`'s player data, settings and checkpoint, which both
    /// sides share.
    saves: save::SharedSaves,
    /// `LevelService`'s levels, which both sides share (Phase 48).
    levels: levels::SharedLevels,
    /// Network ownership's settings (Phase 66).
    ownership: ownership::SharedOwnership,
    /// Where moving parts were, for lag-compensated raycasts (Phase 81).
    lag: lag::SharedLag,
    /// The default chat's messages (Phase 67).
    chat: chat::SharedChat,
    /// Seconds since players' data was last written.
    save_clock: f64,
    /// Players who joined from other devices and get their first
    /// character in the next `step_players`.
    joined: Vec<InstanceId>,
    /// Levels loaded since the host last asked, for its loading screen.
    level_loads: Vec<String>,
    /// Sequences played on either side (Phase 51).
    sequences: sequence::SharedSequences,
    /// SplineFollowers moving their parts (Phase 73), on the server.
    followers: spline::Followers,
    /// What SplineMeshes and SplineScatters make (Phase 74), on every
    /// device: it isn't sent.
    generators: axcenity_procedural::Generators,
    /// The default audio settings UI and volume sliders (Phase 52).
    volume_sliders: audio_settings::VolumeSliders,
    /// No player plays on this device (a dedicated server, Phase 63).
    dedicated: bool,
    /// This device follows a server elsewhere (Phase 64): only its
    /// LocalScripts run, against its replica of the server's world, and
    /// its player is the server's Player for it.
    client_only: bool,
    /// This device's player's `UserId` (Phase 64).
    user_id: f64,
    /// Messages for another device: to Players who joined from elsewhere,
    /// or (on a replica) to the server. The host's network carries them.
    outgoing: Vec<Message>,
    /// Every script and ModuleScript started, in the order they first
    /// started, with the code they run now, so they can run again.
    started: Vec<Started>,
    /// Scripts waiting to be somewhere scripts run, with their code (see
    /// [`Self::start_clones`]): copies made by `Clone`, with their
    /// original's, and scripts inside objects started somewhere else, such
    /// as a template in ServerStorage, so its clones have code too (G2).
    clones: Vec<Started>,
    /// True while [`Self::start_clones`] runs, so starting a clone doesn't
    /// start more inside it.
    starting_clones: bool,
    /// The code of each file scripts inside objects take theirs from, by
    /// `SourceFile` (G2): from every start, and the hosts'
    /// [`Self::know_code`]. A script inside an object that arrives with no
    /// code of its own (replicated to this device, streamed in, or in a
    /// level LevelService loads) starts with it.
    known_code: BTreeMap<String, String>,
}

/// A script as [`ScriptRuntime::start_script`] was last given it.
struct Started {
    script: InstanceId,
    path: String,
    source: String,
}

impl ScriptRuntime {
    /// A runtime whose two sides talk through the in-memory [`Loopback`].
    pub fn new() -> Self {
        Self::with_transport(Box::new(Loopback::default()))
    }

    /// Adds a game system from another crate ([`GameSystem`]) to both
    /// sides: its services and its per-frame step. Add systems before any
    /// script starts, so scripts find their services.
    pub fn add_system(&mut self, system: impl GameSystem) {
        let system: std::rc::Rc<dyn GameSystem> = std::rc::Rc::new(system);
        for lua in [&self.server.lua, &self.client.lua] {
            system.added(lua);
            system::add(lua, std::rc::Rc::clone(&system));
            let methods = system
                .methods(lua)
                .expect("creating a game system's methods only fails when out of memory");
            if let Some(mut class_methods) = lua.app_data_mut::<instance::ClassMethods>() {
                class_methods.extend(methods);
            }
        }
        self.systems.push(system);
    }

    /// The added game system of type `T`, for the functions hosts call on
    /// it (the `gameplay` crate's `dialogue_view`, for one).
    pub fn system<T: GameSystem>(&self) -> Option<&T> {
        self.systems
            .iter()
            .find_map(|system| (&**system as &dyn std::any::Any).downcast_ref::<T>())
    }

    /// The interpreter of `side`, for game systems (see `system`).
    pub fn lua(&self, side: Side) -> &Lua {
        &self.context(side).lua
    }

    /// For game systems: runs `f` on `side`'s interpreter with `world`
    /// lent, then gathers what it did (output, sounds, clones). Follow it
    /// with [`fire_changes`](Self::fire_changes), or inside
    /// [`GameSystem::fire_events`] with
    /// [`fire_change_rounds`](Self::fire_change_rounds).
    pub fn run_on<R>(&mut self, side: Side, world: &mut World, f: impl FnOnce(&Lua) -> R) -> R {
        let result = with_world(&self.context(side).lua, world, || {
            f(&self.context(side).lua)
        });
        self.collect(side);
        result
    }

    /// For game systems: adds an engine error (one no script caused) to
    /// the output.
    pub fn report_error(&mut self, err: &mlua::Error) {
        self.output.push(OutputLine {
            kind: OutputKind::Error,
            text: errors::error_message(err),
            trace: Vec::new(),
            side: None,
        });
    }

    /// The project's files, as set by [`set_assets`](Self::set_assets).
    pub fn assets(&self) -> Assets {
        self.server.assets()
    }

    /// A runtime whose RemoteEvent messages go through `transport`.
    ///
    /// `Box<dyn Transport>` holds any type that implements the `Transport`
    /// trait; which one is only known when the program runs.
    pub fn with_transport(transport: Box<dyn Transport>) -> Self {
        let saves = save::SharedSaves::default();
        let levels = levels::SharedLevels::default();
        let server = Context::new(Side::Server);
        let client = Context::new(Side::Client);
        server.lua.set_app_data(saves.clone());
        client.lua.set_app_data(saves.clone());
        server.lua.set_app_data(levels.clone());
        client.lua.set_app_data(levels.clone());
        let sequences = sequence::SharedSequences::default();
        server.lua.set_app_data(sequences.clone());
        client.lua.set_app_data(sequences.clone());
        let ownership = ownership::SharedOwnership::default();
        server.lua.set_app_data(ownership.clone());
        client.lua.set_app_data(ownership.clone());
        // Only the server's raycasts are compensated.
        let lag = lag::SharedLag::default();
        server.lua.set_app_data(lag.clone());
        let chat = chat::SharedChat::default();
        server.lua.set_app_data(chat.clone());
        client.lua.set_app_data(chat.clone());
        Self {
            server,
            client,
            transport,
            output: Vec::new(),
            sound_actions: Vec::new(),
            particle_emits: Vec::new(),
            shatters: Vec::new(),
            systems: Vec::new(),
            prompts: prompt::Prompts::default(),
            clicks: click::Clicks::default(),
            players: players::Players::default(),
            player_events: Vec::new(),
            reported_change_loop: false,
            saves,
            levels,
            level_loads: Vec::new(),
            sequences,
            followers: spline::Followers::default(),
            generators: axcenity_procedural::Generators::default(),
            ownership,
            lag,
            chat,
            joined: Vec::new(),
            save_clock: 0.0,
            volume_sliders: audio_settings::VolumeSliders::default(),
            dedicated: false,
            client_only: false,
            user_id: 0.0,
            outgoing: Vec::new(),
            started: Vec::new(),
            clones: Vec::new(),
            starting_clones: false,
            known_code: BTreeMap::new(),
        }
    }

    /// Where `SaveService` writes its slots: the player's data for this
    /// game (`axcenity_save::game_folder`), never the project folder.
    /// Without one, `SaveSlot` and `LoadSlot` fail with a message. Call it
    /// before starting scripts, so a script that loads at the start can.
    pub fn set_save_folder(&mut self, folder: Option<PathBuf>) {
        self.saves.0.borrow_mut().set_folder(folder);
    }

    /// `SaveService`'s player data, settings and active checkpoint.
    pub fn saves(&self) -> Ref<'_, Saves> {
        self.saves.0.borrow()
    }

    /// Every level of the game for `LevelService` (Phase 48), by name, each
    /// a world of its own (only its Workspace is used), with `start` among
    /// them. `current` is the one in the world now: the start level in a
    /// built game, the open level in the editor's Play. Call it before
    /// starting scripts.
    pub fn set_levels(&mut self, start: &str, current: &str, worlds: BTreeMap<String, World>) {
        let mut levels = Levels::new(start, worlds);
        levels.set_current(current);
        self.saves.0.borrow_mut().levels = levels.saved();
        *self.levels.0.borrow_mut() = levels;
    }

    /// `LevelService`'s levels: the current one and those added.
    pub fn levels(&self) -> Ref<'_, Levels> {
        self.levels.0.borrow()
    }

    /// On a device following a server (Phase 65): the server's level is
    /// `current`, with `added` beside it, whose Workspace replication
    /// brings. When `loaded` and it's another level, `LevelLoaded` fires
    /// in the next step and the host shows its loading screen.
    pub fn follow_levels(&mut self, current: &str, added: Vec<(String, InstanceId)>, loaded: bool) {
        self.levels.0.borrow_mut().follow(current, added, loaded);
    }

    /// `Player:Kick` calls since the last call (Phase 67), each Player with
    /// the reason: hosts close a remote player's connection with it, and
    /// end the game for the player on this device.
    pub fn take_kicks(&mut self) -> Vec<(InstanceId, String)> {
        self.server
            .lua
            .app_data_mut::<players::Kicks>()
            .map(|mut kicks| std::mem::take(&mut kicks.0))
            .unwrap_or_default()
    }

    /// The levels loaded (`LoadLevel`, or a slot in another level) since
    /// the last call, oldest first. Hosts show their loading screen again.
    pub fn take_level_loads(&mut self) -> Vec<String> {
        std::mem::take(&mut self.level_loads)
    }

    /// Bytes the Lua heaps of both sides hold now, for the editor's profiler.
    pub fn used_memory(&self) -> usize {
        self.server.lua.used_memory() + self.client.lua.used_memory()
    }

    /// MeshParts `workspace:Raycast` collides as, matching physics, and the
    /// data tables `DataService` reads. Cloning `assets` shares the same
    /// loaded models and tables. The player and editor Play both call this;
    /// without it every MeshPart is a box and there are no tables.
    pub fn set_assets(&mut self, assets: Assets) {
        self.server.set_assets(assets.clone());
        self.client.set_assets(assets);
    }

    /// Recent `workspace:Raycast` calls on both sides, oldest first, at most
    /// 64. The editor draws them; the player never asks.
    pub fn debug_rays(&self) -> Vec<DebugRay> {
        let mut rays = self.server.debug_rays();
        rays.extend(self.client.debug_rays());
        rays
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
    ///
    /// A script inside an object (one with a `SourceFile`, G2) only runs
    /// where a clone would ([`Self::start_clones`]): elsewhere, such as a
    /// template in ServerStorage, its code is kept, and it starts once it's
    /// moved there, as does a `Clone` of it.
    pub fn start_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) {
        let class = world.get(script).map(|instance| instance.class());
        if world.get(script).and_then(|i| i.source_file()) == Some(path) {
            self.known_code.insert(path.to_owned(), source.to_owned());
        }
        if class == Some(ClassName::ModuleScript) {
            self.remember(script, path, source);
            self.server.add_module(script, path, source);
            self.client.add_module(script, path, source);
            return;
        }
        let waits = world.get(script).is_some_and(|instance| {
            instance.source_file().is_some() && !world.runs_scripts_at(script)
        });
        if waits {
            // New code (a reload) replaces what it waited with.
            self.started.retain(|started| started.script != script);
            self.clones.retain(|clone| clone.script != script);
            self.clones.push(Started {
                script,
                path: path.to_owned(),
                source: source.to_owned(),
            });
            return;
        }
        self.remember(script, path, source);
        let side = class.and_then(Side::of_class).unwrap_or(Side::Server);
        if side == Side::Server && self.client_only {
            // The server runs it.
            return;
        }
        self.fire_changes(world);
        if side == Side::Client {
            self.join(world);
        }
        self.context(side).start_script(world, script, path, source);
        self.collect(side);
        self.fire_changes(world);
    }

    /// The code in the file at `path`, for scripts inside objects whose
    /// `SourceFile` names it and that arrive later with no code: a level's
    /// (hosts pass every level's before LevelService loads one), or a
    /// LocalScript the server places on a device following it (G2).
    pub fn know_code(&mut self, path: &str, source: &str) {
        self.known_code.insert(path.to_owned(), source.to_owned());
    }

    /// Each script inside an object in what `changes` put in the tree that
    /// has no code here yet: see [`Self::note_arrived`].
    fn note_arrived_scripts(&mut self, world: &World, changes: &[RecordedChange]) {
        if self.known_code.is_empty() {
            return;
        }
        let added: Vec<InstanceId> = changes
            .iter()
            .filter_map(|change| match change.change {
                Change::ChildAdded { child, .. } => Some(child),
                _ => None,
            })
            .collect();
        self.note_arrived(world, &added);
    }

    /// Starts each script inside an object in `roots` (or inside one of
    /// them) that has no code here yet, with the code its `SourceFile`
    /// names, once it's where scripts of its kind run: for a device that
    /// joins, what the server placed before it did (G2).
    pub fn start_arrived(&mut self, world: &mut World, roots: &[InstanceId]) {
        self.note_arrived(world, roots);
        self.fire_changes(world);
    }

    /// Each script inside an object in `arrived` (or inside one of them)
    /// that has no code here yet (not started, not waiting) waits with the
    /// code its `SourceFile` names, if known, and starts once it's where
    /// scripts of its kind run ([`Self::start_clones`]).
    fn note_arrived(&mut self, world: &World, arrived: &[InstanceId]) {
        if self.known_code.is_empty() {
            return;
        }
        for &root in arrived {
            for id in std::iter::once(root).chain(world.descendants_iter(root)) {
                let Some(path) = world.get(id).and_then(|i| i.source_file()) else {
                    continue;
                };
                let has_code = self
                    .started
                    .iter()
                    .chain(&self.clones)
                    .any(|started| started.script == id);
                if has_code {
                    continue;
                }
                if let Some(source) = self.known_code.get(path) {
                    self.clones.push(Started {
                        script: id,
                        path: path.to_owned(),
                        source: source.clone(),
                    });
                }
            }
        }
    }

    /// Keeps `script`'s code for running it again, in the place it first
    /// started.
    fn remember(&mut self, script: InstanceId, path: &str, source: &str) {
        match self.started.iter_mut().find(|s| s.script == script) {
            Some(started) => {
                started.path = path.to_owned();
                started.source = source.to_owned();
            }
            None => self.started.push(Started {
                script,
                path: path.to_owned(),
                source: source.to_owned(),
            }),
        }
    }

    /// Runs `script`'s new code in place while the game runs. Its handlers
    /// are disconnected and its waiting coroutines (`task.wait`,
    /// `task.spawn`, `task.delay`) are dropped, then `source` runs as a new
    /// start against the world as it is now. Every other script, and the
    /// world, carry on.
    ///
    /// A ModuleScript loads again the next time it's required, with every
    /// module that required it while loading, and the scripts that required
    /// any of them run again, in the order they first started.
    ///
    /// Code with a syntax error isn't run: the old code keeps running, and
    /// the error (`path:line: message`) joins the output and is returned.
    /// A script no longer in the game isn't run either.
    ///
    /// With [`Self::enable_hot_reload`], the new run keeps the old run's
    /// top-level locals (see `hot_reload`), a ModuleScript's table keeps its
    /// identity, and the scripts that run again keep theirs too. Returns
    /// the names the changed script kept (none without hot reload).
    pub fn reload_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) -> Result<Vec<String>, String> {
        let Some(class) = world.get(script).map(|instance| instance.class()) else {
            return Err(format!("`{path}` isn't in the game any more."));
        };
        let side = Side::of_class(class).unwrap_or(Side::Server);
        if let Err(err) = self.context(side).check_syntax(path, source) {
            let text = errors::error_message(&err);
            self.output.push(OutputLine {
                kind: OutputKind::Error,
                text: text.clone(),
                trace: Vec::new(),
                side: None,
            });
            return Err(text);
        }
        if class != ClassName::ModuleScript {
            return Ok(self.run_again(world, script, path, source));
        }
        self.remember(script, path, source);
        let mut again = BTreeSet::new();
        let mut kept = Vec::new();
        for side in [Side::Server, Side::Client] {
            match self
                .context(side)
                .reload_module_keeping(world, script, path, source)
            {
                // The scripts that required it hold its table, now filled
                // with the new code, so they keep running as they are.
                Some((_, names)) => {
                    for name in names {
                        if !kept.contains(&name) {
                            kept.push(name);
                        }
                    }
                }
                None => again.extend(self.context(side).replace_module(script, path, source)),
            }
        }
        let again: Vec<(InstanceId, String, String)> = self
            .started
            .iter()
            .filter(|started| again.contains(&started.script))
            .map(|started| (started.script, started.path.clone(), started.source.clone()))
            .collect();
        for (script, path, source) in again {
            if world.contains(script) {
                self.run_again(world, script, &path, &source);
            }
        }
        Ok(kept)
    }

    /// Stops `script` and runs `source` as its new start, keeping its
    /// top-level locals when hot reload is on. Returns the names it kept.
    fn run_again(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) -> Vec<String> {
        let class = world.get(script).map(|instance| instance.class());
        let side = class.and_then(Side::of_class).unwrap_or(Side::Server);
        self.context(side).hot_snapshot(script);
        self.context(side).stop_script(script);
        self.start_script(world, script, path, source);
        self.context(side).hot_finish(script)
    }

    /// Turns on hot reload that keeps scripts' state (unified Phase 89, the
    /// editor's Play only): `wrap` rewrites each script's top-level locals
    /// as it loads (`axcenity_blocks::hot_reload::wrap`). Call it before
    /// scripts start. The player never does, so its scripts load as written.
    pub fn enable_hot_reload(&mut self, wrap: hot_reload::Wrap) {
        hot_reload::install(&self.server.lua, wrap);
        hot_reload::install(&self.client.lua, wrap);
    }

    /// Tells LocalScripts where the camera is now (`workspace.CurrentCamera`).
    /// Call it each frame after the player has moved the camera.
    pub fn set_camera(&mut self, camera: CameraState) {
        self.client.set_camera(camera);
    }

    /// The game view's size in pixels (`Camera.ViewportSize`): the
    /// window in the player, the viewport in the editor. Screen points
    /// (`ScreenPointToRay`, `WorldToScreenPoint`) are pixels from its top
    /// left, like mouse positions and UI offsets.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.client.set_viewport_size(width, height);
    }

    /// Where a LocalScript moved the camera, if one did since the last
    /// call, while it's Custom (the host's camera). Other camera types
    /// are drawn through [`Self::step_camera`] instead.
    pub fn take_camera_move(&mut self) -> Option<CameraState> {
        self.client.take_camera_move()
    }

    /// Call once a frame, after scripts, physics and animation and just
    /// before drawing: moves a Follow or Track camera after its subject and
    /// shakes on by `seconds`. The result's [`GameCamera::view`] turns the
    /// host's camera (a character controller's, or the fly camera with any
    /// `take_camera_move` applied) into the one to draw and listen from.
    pub fn step_camera(&mut self, world: &World, seconds: f64) -> GameCamera {
        self.client.step_camera(world, seconds)
    }

    /// What a Follow or Track camera follows (`CameraSubject`), if it's one
    /// of those: physics keeps it drawn where it's stepped, with
    /// everything welded to it (roadmap G3).
    pub fn camera_followed(&self) -> Option<InstanceId> {
        self.client.camera_followed()
    }

    /// Whether scripts hold the camera (`CameraType` isn't Custom): the
    /// host then leaves its fly camera where it is, and doesn't draw from
    /// a character controller.
    pub fn camera_scripted(&self) -> bool {
        self.client.camera_scripted()
    }

    /// True once after a LocalScript calls
    /// `ReplicatedFirst:RemoveDefaultLoadingScreen()`: the game says it's
    /// ready for the loading screen to go.
    pub fn take_loading_screen_removed(&mut self) -> bool {
        self.client.take_loading_screen_removed()
    }

    /// Tells LocalScripts this computer's graphics settings
    /// (`UserGameSettings`), and what its GPU can do (which picks the
    /// quality for `Enum.GraphicsQuality.Automatic`). It isn't a change for
    /// [`ScriptRuntime::take_graphics_change`]. Call it before starting
    /// scripts, so a script that reads them at the start sees them.
    pub fn set_graphics(&mut self, settings: &GraphicsSettings, capabilities: &Capabilities) {
        self.client.set_graphics(settings, capabilities);
    }

    /// The graphics settings as a LocalScript left them, if one changed
    /// them since the last call.
    pub fn take_graphics_change(&mut self) -> Option<GraphicsSettings> {
        self.client.take_graphics_change()
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

    /// Takes the `fracture:Shatter(point, force)` calls since the last
    /// call, in order, for the host to hand to physics
    /// (`Physics::shatter`) before it steps.
    pub fn take_shatters(&mut self) -> Vec<Shatter> {
        std::mem::take(&mut self.shatters)
    }

    /// Collects what `side` printed, and hands the messages it sent to the
    /// transport.
    fn collect(&mut self, side: Side) {
        let context = match side {
            Side::Server => &self.server,
            Side::Client => &self.client,
        };
        self.output
            .extend(context.take_output().into_iter().map(|mut line| {
                line.side = Some(side);
                line
            }));
        self.sound_actions.extend(context.take_sound_actions());
        self.particle_emits.extend(context.take_particle_emits());
        self.shatters.extend(context.take_shatters());
        self.player_events
            .extend(self.players.note(context.take_spawned()));
        for (copy, original) in context.take_cloned_scripts() {
            // A clone of a clone that hasn't started yet has its code too.
            let code = self
                .started
                .iter()
                .chain(&self.clones)
                .find(|started| started.script == original)
                .map(|started| (started.path.clone(), started.source.clone()));
            if let Some((path, source)) = code {
                self.clones.push(Started {
                    script: copy,
                    path,
                    source,
                });
            }
        }
        let local = context.local_player().or(self.client.local_player());
        for message in context.take_outbox() {
            // Another device's: the server, from a replica, or a Player
            // who joined from elsewhere.
            let elsewhere = match message.to {
                Side::Server => self.client_only,
                Side::Client => !self.client_only && Some(message.player) != local,
            };
            if elsewhere {
                self.outgoing.push(message);
            } else if message.kind == MessageKind::Activate {
                // For this device's server: a game system serves it.
                self.activation_requested(&message);
            } else {
                self.transport.send(message);
            }
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
    // Not Play: `RunService:IsRunning()` is false, and it's the editor.
    context.set_run_flags(run_service::RunFlags {
        studio: true,
        running: false,
    });
    let was_recording = world.is_recording_changes();
    let was_watching = world.is_watching_destroyed();
    let first = world.next_change_number();
    world.record_changes();
    let result = context.run_command(world, source);
    let changed = world.next_change_number() != first;
    if !was_recording {
        world.stop_recording_changes();
    }
    if !was_watching {
        world.stop_watching_destroyed();
    }
    result.map(|()| EditCommand {
        output: context.take_output(),
        changed,
    })
}

/// The first syntax error in `source`, the code of the script at `path`,
/// as Luau reports it (`path:line: message`), or `None` when it compiles.
/// Compiles only, with the scripts' own compiler: nothing runs.
pub fn syntax_error(path: &str, source: &str) -> Option<String> {
    let lua = Lua::new();
    lua.set_compiler(sandbox::compiler());
    lua.load(source)
        .set_name(format!("@{path}"))
        .set_mode(mlua::chunk::ChunkMode::Text)
        .into_function()
        .err()
        .map(|error| errors::error_message(&error))
}

/// Every global name a script has, for the Console's autocomplete.
pub fn global_names() -> Vec<&'static str> {
    sandbox::global_names()
}

fn push_output(lua: &Lua, kind: OutputKind, text: String) {
    push_line(
        lua,
        OutputLine {
            kind,
            text,
            trace: Vec::new(),
            side: None,
        },
    );
}

/// Adds a script error to the output, with the stack trace that led to it.
fn push_error(lua: &Lua, err: &mlua::Error) {
    push_line(
        lua,
        OutputLine {
            kind: OutputKind::Error,
            text: errors::error_message(err),
            trace: errors::error_trace(err),
            side: None,
        },
    );
}

fn push_line(lua: &Lua, line: OutputLine) {
    if let Some(mut output) = lua.app_data_mut::<Output>() {
        output.0.push(line);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod client_server_tests;

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod camera_tests;

#[cfg(test)]
mod events_tests;

#[cfg(test)]
mod scheduler_tests;

#[cfg(test)]
mod property_bench;

#[cfg(test)]
mod chunks_tests;

#[cfg(test)]
mod native_tests;

#[cfg(test)]
mod native_vector_tests;

#[cfg(test)]
mod handler_threads_tests;

#[cfg(test)]
mod sandbox_tests;

#[cfg(test)]
mod watchdog_tests;

#[cfg(test)]
mod debugger_step_tests;
#[cfg(test)]
mod debugger_tests;

#[cfg(test)]
mod tree_tests;

#[cfg(test)]
mod tween_tests;

#[cfg(test)]
mod gui_service_tests;
#[cfg(test)]
mod levels_tests;
#[cfg(test)]
mod localization_tests;
#[cfg(test)]
mod reload_tests;
#[cfg(test)]
mod save_tests;
#[cfg(test)]
mod sequence_tests;

#[cfg(test)]
mod user_settings_tests;

#[cfg(test)]
mod cframe_tests;

#[cfg(test)]
mod assembly_tests;
#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod console_service_tests;
#[cfg(test)]
mod content_provider_tests;
#[cfg(test)]
mod data_tests;
#[cfg(test)]
mod decal_tests;
#[cfg(test)]
mod devices_tests;
#[cfg(test)]
mod instance_identity_tests;
#[cfg(test)]
mod joint_tests;
#[cfg(test)]
mod luau_pin_tests;
#[cfg(test)]
mod mover_tests;
#[cfg(test)]
mod physical_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod query_tests;
#[cfg(test)]
mod seat_tests;
#[cfg(test)]
mod spline_tests;
#[cfg(test)]
mod utilities_tests;

/// Whether `event` presses something bound to the action `name`.
fn presses(map: &InputMap, name: &str, event: &InputEvent) -> bool {
    use axcenity_core::input::{Input, InputKind};
    let Some(action) = map.action(name) else {
        return false;
    };
    let pressed = match event.kind {
        InputKind::Key { key, pressed: true }
        | InputKind::PadButton {
            key, pressed: true, ..
        } => key,
        _ => return false,
    };
    action.inputs.iter().any(|input| match input {
        Input::Key(key) => *key == pressed,
        Input::Keys(keys) => keys.contains(&pressed),
        _ => false,
    })
}

/// Whether `event` lets go of something (or a controller comes or goes),
/// which gets through while input is locked so nothing stays held.
fn releases(event: &InputEvent) -> bool {
    use axcenity_core::UserInputState;
    use axcenity_core::input::InputKind;
    match event.kind {
        InputKind::Key { pressed, .. }
        | InputKind::MouseButton { pressed, .. }
        | InputKind::PadButton { pressed, .. } => !pressed,
        InputKind::Touch { state, .. } => {
            matches!(state, UserInputState::End | UserInputState::Cancel)
        }
        InputKind::PadConnected(_) | InputKind::PadDisconnected(_) => true,
        InputKind::MouseMove { .. } | InputKind::Wheel { .. } | InputKind::PadStick { .. } => false,
    }
}
#[cfg(test)]
mod keypoints_tests;
