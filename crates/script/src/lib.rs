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
//!   one side talk (`bindable`); `Debris` destroys Instances later
//!   (`debris`), `HttpService` reads and writes JSON (`http`), and
//!   `DataService` reads the project's data tables (`data`).
//!   `TweenService` tweens move properties smoothly,
//!   each frame step (`tween`). `SaveService` writes and loads save slots
//!   and checkpoints (`save`, over the `save` crate). Change events (`part.Changed`) come from the World's
//!   change log (`changes`). Client-only APIs (keyboard, camera) live in
//!   `client`, and `sides` says which APIs work on which side.
//!   `UserGameSettings` (`user_settings`) lets LocalScripts change the
//!   player's graphics settings.
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

mod api;
mod attachment;
pub mod audio_settings;
mod bindable;
mod camera;
mod cframe;
mod changes;
mod click;
mod client;
mod collection;
mod console;
mod context;
mod csg;
mod data;
mod debris;
mod debugger;
mod definitions;
mod errors;
mod events;
mod gui_service;
mod health;
mod http;
mod humanoid;
mod input;
mod instance;
mod levels;
mod localization;
mod modules;
mod physical;
mod players;
mod prompt;
mod query;
mod random;
mod remote;
mod run_service;
mod sandbox;
mod save;
mod scheduler;
mod seat;
mod sequence;
mod sides;
mod task;
mod tool;
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
use axcenity_audio::SoundAction;
use axcenity_core::input::{InputEvent, InputMap};
use axcenity_core::{ClassName, InstanceId, KeyCode, MouseBehavior, Value, Vector3, World};
use axcenity_physics::RayHit;
use axcenity_render::{Capabilities, GraphicsSettings};
use axcenity_save::Saves;
use mlua::Lua;

use crate::context::Context;

pub use camera::{CameraState, CameraType, CameraView, DEFAULT_FIELD_OF_VIEW, GameCamera};
pub use debugger::{Breakpoints, Local, Pause, PausedFrame};
pub use definitions::{ApiEntry, ApiKind, DEFINITIONS_PATH, api_reference, lua_definitions};
pub use input::KeyEvent;
pub use levels::Levels;
pub use prompt::ShownPrompt;
pub use remote::{Loopback, Message, MessageKind, RemoteValue, Transport};
pub use sequence::sound_actions as sequence_sound_actions;
pub use sides::{SIDE_ONLY_APIS, Side, SideOnlyApi, side_only};
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
    /// Every Health's regeneration wait, and their events waiting to fire.
    health: health::Healths,
    /// The ProximityPrompts showing and their holds (Phase 44).
    prompts: prompt::Prompts,
    /// What the mouse points at and presses, for ClickDetectors.
    clicks: click::Clicks,
    /// The Tools in hand last fired, dropped Tools, and the hotbar's keys
    /// (Phase 45).
    tools: tool::Tools,
    /// Humanoids' walks and states (Phase 46).
    humanoids: humanoid::Humanoids,
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
    /// Levels loaded since the host last asked, for its loading screen.
    level_loads: Vec<String>,
    /// Sequences played on either side (Phase 51).
    sequences: sequence::SharedSequences,
    /// The default audio settings UI and volume sliders (Phase 52).
    volume_sliders: audio_settings::VolumeSliders,
    /// Every script and ModuleScript started, in the order they first
    /// started, with the code they run now, so they can run again.
    started: Vec<Started>,
    /// Scripts copied by `Clone`, with their original's code, waiting to
    /// be somewhere scripts run (see [`Self::start_clones`]).
    clones: Vec<Started>,
    /// True while [`Self::start_clones`] runs, so starting a clone doesn't
    /// start more inside it.
    starting_clones: bool,
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
        Self {
            server,
            client,
            transport,
            output: Vec::new(),
            sound_actions: Vec::new(),
            particle_emits: Vec::new(),
            shatters: Vec::new(),
            health: health::Healths::default(),
            prompts: prompt::Prompts::default(),
            clicks: click::Clicks::default(),
            tools: tool::Tools::default(),
            humanoids: humanoid::Humanoids::default(),
            players: players::Players::default(),
            player_events: Vec::new(),
            reported_change_loop: false,
            saves,
            levels,
            level_loads: Vec::new(),
            sequences,
            volume_sliders: audio_settings::VolumeSliders::default(),
            started: Vec::new(),
            clones: Vec::new(),
            starting_clones: false,
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
    pub fn start_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) {
        self.remember(script, path, source);
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
    pub fn reload_script(
        &mut self,
        world: &mut World,
        script: InstanceId,
        path: &str,
        source: &str,
    ) -> Result<(), String> {
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
            });
            return Err(text);
        }
        if class != ClassName::ModuleScript {
            self.context(side).stop_script(script);
            self.start_script(world, script, path, source);
            return Ok(());
        }
        self.remember(script, path, source);
        let mut again = BTreeSet::new();
        for side in [Side::Server, Side::Client] {
            again.extend(self.context(side).replace_module(script, path, source));
        }
        let again: Vec<(InstanceId, String, String)> = self
            .started
            .iter()
            .filter(|started| again.contains(&started.script))
            .map(|started| (started.script, started.path.clone(), started.source.clone()))
            .collect();
        for (script, path, source) in again {
            let Some(class) = world.get(script).map(|instance| instance.class()) else {
                continue;
            };
            let side = Side::of_class(class).unwrap_or(Side::Server);
            self.context(side).stop_script(script);
            self.start_script(world, script, &path, &source);
        }
        Ok(())
    }

    /// The player on this device joins the game, once: a Player Instance
    /// named `Player` goes into `Players`, and becomes `LocalPlayer`. It
    /// goes on the AutoAssignable Team with the fewest players, and
    /// `PlayerAdded` fires (Phase 47).
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
        players::assign_team(world, player);
        self.player_events
            .push(players::PlayerEvent::Added { player });
        self.fire_changes(world);
    }

    /// Makes `character`, placed in the Workspace before Play, `player`'s,
    /// and keeps a copy of it (with no parent, so out of the game) to
    /// respawn as. Its scripts' copies start when a copy goes in.
    fn adopt(&mut self, world: &mut World, player: InstanceId, character: InstanceId) {
        if let Ok(template) = world.duplicate(character) {
            let originals = std::iter::once(character).chain(world.descendants(character));
            let copies = std::iter::once(template).chain(world.descendants(template));
            for (original, copy) in originals.zip(copies) {
                let code = self
                    .started
                    .iter()
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
            self.server.set_respawn_template(template);
        }
        let _ = world.set_property(player, "Character", Value::Instance(Some(character)));
        self.player_events
            .push(players::PlayerEvent::CharacterAdded { player, character });
    }

    /// Fires the player events `LoadCharacter`, joining and respawns
    /// queued, on both sides: an old character is destroyed once its
    /// `CharacterRemoving` has fired.
    /// Fires `LevelLoaded` on both sides for each level loaded, after the
    /// changes the load made have fired.
    fn fire_level_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            let loaded = std::mem::take(&mut self.levels.0.borrow_mut().loaded);
            if loaded.is_empty() {
                return;
            }
            for name in loaded {
                self.fire_change_rounds(world);
                for side in [Side::Server, Side::Client] {
                    self.context(side).fire_level_loaded(world, &name);
                    self.collect(side);
                    self.fire_change_rounds(world);
                }
                self.level_loads.push(name);
            }
        }
    }

    fn fire_player_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            if self.player_events.is_empty() {
                return;
            }
            for event in std::mem::take(&mut self.player_events) {
                let (owner, name, arg) = match event {
                    players::PlayerEvent::Added { player } => {
                        (world.players(), "PlayerAdded", player)
                    }
                    players::PlayerEvent::CharacterRemoving { player, character } => {
                        (player, "CharacterRemoving", character)
                    }
                    players::PlayerEvent::CharacterAdded { player, character } => {
                        (player, "CharacterAdded", character)
                    }
                };
                for side in [Side::Server, Side::Client] {
                    self.context(side)
                        .fire_with_instance(world, owner, name, arg);
                    self.collect(side);
                    self.fire_change_rounds(world);
                }
                if let players::PlayerEvent::CharacterRemoving { character, .. } = event
                    && world.get(character).is_some_and(|c| c.parent().is_none())
                {
                    let _ = world.destroy(character);
                }
            }
        }
    }

    /// Frame step 1, after [`step_humanoids`](Self::step_humanoids)
    /// (Phase 47). The first time, the player joins and gets a character:
    /// one already in the Workspace becomes theirs (and a copy of it, out
    /// of the game, is what they respawn as), or else, with
    /// `Players.CharacterAutoLoads` on, a copy of the StarterCharacter
    /// spawns. Then spawn ForceFields whose time is up go, and players
    /// whose `RespawnTime` has passed since their Humanoid died get a new
    /// character. `CharacterAdded` and the rest fire in Scripts and then in
    /// LocalScripts.
    pub fn step_players(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        let placed = if self.players.started {
            None
        } else {
            axcenity_character::player_character(world)
        };
        self.join(world);
        let auto_loads = |world: &World| {
            world
                .get(world.players())
                .and_then(|service| service.property("CharacterAutoLoads"))
                .and_then(Value::as_bool)
                != Some(false)
        };
        let mut due = Vec::new();
        if !self.players.started {
            self.players.started = true;
            let player = self.local_player().expect("just joined");
            match placed {
                Some(character) => self.adopt(world, player, character),
                None if auto_loads(world) => due.push(player),
                None => {}
            }
        }
        due.extend(self.players.step(world, seconds));
        for player in due {
            if auto_loads(world) {
                self.server.load_character(world, player);
                self.collect(Side::Server);
            }
        }
        self.fire_changes(world);
    }

    /// Whether to draw the default leaderboard (`ui::leaderboard_items`):
    /// Tab shows and hides it. Its `Players.LeaderboardEnabled` is the
    /// leaderboard's own to check.
    pub fn leaderboard_shown(&self) -> bool {
        !self.players.leaderboard_hidden
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
        let events: Vec<InputEvent> = keys.iter().map(|&key| key.into()).collect();
        self.input_events(world, &events);
    }

    /// Frame-order step 1: everything the keyboard, mouse, touch screen
    /// and controllers did since the last frame, in order. LocalScripts see
    /// it through `UserInputService` and `ContextActionService`.
    pub fn input_events(&mut self, world: &mut World, events: &[InputEvent]) {
        // Changes made since the last frame, such as by the editor.
        self.fire_changes(world);
        // The Menu action skips a skippable Sequence (and goes no further);
        // a Sequence that locks input lets only releases through.
        let menu = self.input_map();
        let skips = events.iter().any(|event| presses(&menu, "Menu", event))
            && self.sequences.0.borrow().skippable(world);
        if skips {
            let stepped = {
                let mut camera = self.client_camera();
                self.sequences.0.borrow_mut().skip(world, &mut camera)
            };
            self.after_sequences(world, stepped);
        }
        let kept: Vec<InputEvent>;
        let events = if skips || self.input_locked(world) {
            let locked = self.input_locked(world);
            kept = events
                .iter()
                .filter(|event| !(skips && presses(&menu, "Menu", event)))
                .filter(|event| !locked || releases(event))
                .cloned()
                .collect();
            &kept[..]
        } else {
            events
        };
        // Even with no events, so `GetMouseDelta` is this frame's.
        self.client.fire_input(world, events);
        let tab = self.client.prompt_input().game.is_down(KeyCode::Tab);
        self.players.follow_tab(tab);
        if events.is_empty() {
            return;
        }
        self.join(world);
        self.collect(Side::Client);
        self.fire_changes(world);
    }

    /// The project's actions and bindings (`input.json`), for
    /// `UserInputService`'s actions. Call it before starting scripts.
    pub fn set_input_map(&mut self, map: InputMap) {
        self.client.set_input_map(map);
    }

    /// The input map with the player's rebinds (`RebindAction`, kept in
    /// SaveService's settings), for the built-in controllers and UI
    /// navigation.
    pub fn input_map(&self) -> InputMap {
        self.client.input_map()
    }

    /// `UserInputService.MouseBehavior` and `MouseIconEnabled`, which the
    /// host applies to the cursor.
    pub fn mouse(&self) -> (MouseBehavior, bool) {
        self.client.mouse()
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
        // Then Sequences move on, after scripts that play them.
        let assets = self
            .client
            .lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        let stepped = {
            let mut camera = self.client_camera();
            self.sequences
                .0
                .borrow_mut()
                .step(world, &assets, &mut camera, seconds)
        };
        self.after_sequences(world, stepped);
        // Volume sliders follow the volumes scripts set, and set the ones
        // the player dragged.
        self.volume_sliders.step(world);
        self.fire_changes(world);
    }

    /// Whether a Sequence with `LockPlayerInput` is playing (or paused):
    /// hosts then give the character controllers no movement, and scripts
    /// see only releases.
    pub fn input_locked(&self, world: &World) -> bool {
        self.sequences.0.borrow().input_locked(world)
    }

    /// The client's `workspace.CurrentCamera`.
    fn client_camera(&self) -> mlua::AppDataRefMut<'_, camera::Camera> {
        self.client
            .lua
            .app_data_mut::<camera::Camera>()
            .expect("the client side has a camera")
    }

    /// Does what stepping the Sequences left: Sounds, levels, warnings and
    /// their events.
    fn after_sequences(&mut self, world: &mut World, stepped: sequence::Stepped) {
        let actions = sequence::sound_actions(world, stepped.sounds);
        self.sound_actions.extend(actions);
        {
            let mut levels = self.levels.0.borrow_mut();
            for name in &stepped.remove_levels {
                let _ = levels::remove(&mut levels, world, name);
            }
            for name in &stepped.add_levels {
                if let Err(why) = levels::add(&mut levels, world, name) {
                    self.output.push(OutputLine {
                        kind: OutputKind::Warning,
                        text: format!("a sequence couldn't add a level: {why}"),
                        trace: Vec::new(),
                    });
                }
            }
            if !(stepped.add_levels.is_empty() && stepped.remove_levels.is_empty()) {
                self.saves.0.borrow_mut().levels = levels.saved();
            }
        }
        for warning in stepped.warnings {
            self.output.push(OutputLine {
                kind: OutputKind::Warning,
                text: warning,
                trace: Vec::new(),
            });
        }
        for (id, event, args) in stepped.events {
            self.fire_instance_event(world, id, event, &args);
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
        self.fire_touch_event(world, "Touched", touches);
    }

    /// Frame step 1, after the character controllers step: fires
    /// `Seated(active, seat)` on each controller that sat down (`true`) or
    /// stood up (`false`), in Scripts and then in LocalScripts.
    pub fn fire_seated(&mut self, world: &mut World, changes: &[(InstanceId, bool, InstanceId)]) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_seated(world, changes);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame-order step 4, after [`fire_touched`](Self::fire_touched):
    /// fires `TouchEnded` on both parts of each pair that stopped
    /// touching, the same way.
    pub fn fire_touch_ended(&mut self, world: &mut World, ends: &[(InstanceId, InstanceId)]) {
        self.fire_touch_event(world, "TouchEnded", ends);
    }

    /// Frame-order step 3, before physics steps: hands the scripts'
    /// `fracture:Shatter(point, force)` calls to `physics`, which replaces
    /// the parts with their pieces now (Phase 42).
    pub fn apply_shatters(&mut self, world: &mut World, physics: &mut axcenity_physics::Physics) {
        for shatter in std::mem::take(&mut self.shatters) {
            physics.shatter(world, shatter.fracture, shatter.point, shatter.force);
        }
    }

    /// Frame-order step 4, after [`fire_touch_ended`](Self::fire_touch_ended):
    /// fires `Broken` on each joint or weld that broke in physics' steps,
    /// then `Shattered` on each Fracture whose part shattered (Phase 42).
    pub fn fire_breaks(&mut self, world: &mut World, physics: &mut axcenity_physics::Physics) {
        for constraint in physics.take_broken() {
            self.fire_instance_event(world, constraint, "Broken", &[]);
        }
        for fracture in physics.take_shattered() {
            self.fire_instance_event(world, fracture, "Shattered", &[]);
        }
    }

    /// Frame step 1, after [`fire_seated`](Self::fire_seated): for each
    /// character controller that landed this frame and how fast it was
    /// falling, the Health in its Model takes fall damage (if its
    /// `FallDamage` is on), then `Landed(fallSpeed)` fires, in Scripts and
    /// then in LocalScripts (Phase 43).
    pub fn fire_landed(&mut self, world: &mut World, landings: &[(InstanceId, f32)]) {
        self.fire_changes(world);
        for &(controller, speed) in landings {
            let events = health::fall_damage(world, controller, f64::from(speed));
            self.health.pending.extend(events);
            self.fire_changes(world);
            for side in [Side::Server, Side::Client] {
                self.context(side)
                    .fire_landed(world, controller, f64::from(speed));
                self.collect(side);
                self.fire_changes(world);
            }
        }
    }

    /// Frame step 1, after [`input_events`](Self::input_events) (Phase
    /// 44): ProximityPrompts in reach of the player character show and
    /// follow their actions, and ClickDetectors follow the mouse, through
    /// the camera; their events fire with the Player, in Scripts and then
    /// in LocalScripts. Then [`shown_prompts`](Self::shown_prompts) and
    /// [`click_cursor`](Self::click_cursor) say what to draw.
    pub fn step_prompts(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        let input = self.client.prompt_input();
        let ray = self.client.mouse_ray(input.mouse);
        let camera = ray.map(|ray| cframe::to_vec3(ray.origin));
        let assets = self.server.assets();
        let mut fired = self.prompts.step(world, &assets, &input, camera, seconds);
        fired.extend(self.clicks.step(world, &assets, &input, ray));
        if fired.is_empty() {
            return;
        }
        self.join(world);
        let player = self.local_player();
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_prompt_events(world, &fired, player);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame step 1, after [`step_prompts`](Self::step_prompts) (Phase 45):
    /// characters with an Inventory pick up the dropped Tools they touch,
    /// while there's room; the player's number keys, mouse wheel and
    /// controller bumpers pick a hotbar slot, Drop drops the Tool in hand,
    /// and Primary fires its `Activated` and `Deactivated`, in Scripts and
    /// then in LocalScripts, after `Equipped` and `Unequipped`.
    pub fn step_tools(&mut self, world: &mut World) {
        self.fire_changes(world);
        let input = self.client.prompt_input();
        let assets = self.server.assets();
        let fired = self.tools.step(world, &assets, &input);
        // Equipped and Unequipped first, then Activated.
        self.fire_tool_events(world);
        self.fire_fired(world, &fired);
    }

    /// Frame step 4, after the Animators pose the characters: each Tool in
    /// hand moves to its character's right hand (Phase 45), the bone posed
    /// as `poses` has it.
    pub fn place_tools(&mut self, world: &mut World, poses: &axcenity_anim::Poses) {
        self.fire_changes(world);
        let assets = self.server.assets();
        tool::place_held(world, &assets, poses);
        self.fire_changes(world);
    }

    /// The player character's Inventory, which the default hotbar shows
    /// unless its `HotbarEnabled` is off.
    pub fn player_inventory(&self, world: &World) -> Option<InstanceId> {
        tool::player_inventory(world)
    }

    /// Fires `Unequipped` and `Equipped` for the Inventories scripts and
    /// the tools step changed, on both sides, for up to
    /// [`MAX_CHANGE_ROUNDS`] rounds of handlers that change more.
    fn fire_tool_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            if self.tools.dirty.is_empty() {
                return;
            }
            let fired = self.tools.sync(world);
            self.fire_fired(world, &fired);
        }
    }

    /// Fires events with no arguments on their Instances, on both sides.
    fn fire_fired(&mut self, world: &mut World, fired: &[prompt::Fired]) {
        if fired.is_empty() {
            return;
        }
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_prompt_events(world, fired, None);
            self.collect(side);
            self.fire_change_rounds(world);
        }
    }

    /// The ProximityPrompts showing, as of [`step_prompts`](Self::step_prompts).
    pub fn shown_prompts(&self) -> &[ShownPrompt] {
        self.prompts.shown()
    }

    /// The cursor over the ClickDetector the mouse points at (its
    /// `CursorIcon`, a CSS cursor name, `pointer` by default), or `None`
    /// when it points at none.
    pub fn click_cursor(&self, world: &World) -> Option<String> {
        self.clicks.cursor(world)
    }

    /// Frame-order step 4, after [`fire_breaks`](Self::fire_breaks) and
    /// before `Heartbeat` (Phase 43): DamageZones damage the Healths with
    /// parts inside them, Healths regenerate, their events fire, and health
    /// bars (ProgressBars bound to a Health) show where they are now.
    pub fn step_health(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        let assets = self.server.assets();
        let events = self.health.step(world, &assets, seconds);
        self.health.pending.extend(events);
        self.fire_changes(world);
        health::bind_bars(world);
        self.fire_changes(world);
    }

    /// Fires the Health events scripts and the health step queued, on both
    /// sides, noting damage for regeneration. Handlers that cause more fire
    /// those too, for up to [`MAX_CHANGE_ROUNDS`] rounds.
    fn fire_health_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            if self.health.pending.is_empty() {
                return;
            }
            let events = std::mem::take(&mut self.health.pending);
            self.health.note(&events);
            for side in [Side::Server, Side::Client] {
                self.context(side).fire_health_events(world, &events);
                self.collect(side);
                self.fire_change_rounds(world);
            }
            // Then the Humanoids of the Healths that died (Phase 46).
            let died: Vec<InstanceId> = events.iter().filter_map(humanoid::died_health).collect();
            for health in died {
                let fired = self.humanoids.died(world, health);
                self.fire_humanoid_events(world, &fired);
            }
        }
    }

    /// Frame step 1, after the character controllers and `Landed`
    /// (Phase 46): every Humanoid in the Workspace makes the pieces it
    /// lacks, walks its `MoveTo` on (by its controller's `MoveDirection`),
    /// and fires `MoveToFinished`, `StateChanged` and `Died`, in Scripts
    /// and then in LocalScripts; its Animator plays the matching state.
    pub fn step_humanoids(
        &mut self,
        world: &mut World,
        controllers: &axcenity_character::Controllers,
        seconds: f64,
    ) {
        self.fire_changes(world);
        let assets = self.server.assets();
        let events = self.humanoids.step(world, &assets, controllers, seconds);
        self.fire_humanoid_events(world, &events);
        self.fire_changes(world);
    }

    /// Fires Humanoid events on both sides, after handing both the
    /// Humanoids' states for `GetState`.
    fn fire_humanoid_events(&mut self, world: &mut World, events: &[humanoid::HumanoidEvent]) {
        for side in [Side::Server, Side::Client] {
            self.context(side)
                .set_humanoid_states(&self.humanoids.states);
        }
        if events.is_empty() {
            return;
        }
        // A player's character dying respawns them later (Phase 47).
        for event in events {
            if let humanoid::HumanoidEvent::Died { humanoid } = *event {
                self.players.died(world, humanoid);
            }
        }
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_humanoid_events(world, events);
            self.collect(side);
            self.fire_change_rounds(world);
        }
    }

    fn fire_touch_event(
        &mut self,
        world: &mut World,
        event: &'static str,
        pairs: &[(InstanceId, InstanceId)],
    ) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_touch_event(world, event, pairs);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Between frame steps 2 and 3, before physics: fires
    /// `RunService.Stepped` with the game time and the frame's length in
    /// seconds, in Scripts and then in LocalScripts.
    pub fn stepped(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_stepped(world, seconds);
            self.collect(side);
            self.fire_changes(world);
        }
    }

    /// Frame step 6, just before the frame is built and drawn (and before
    /// [`Self::step_camera`]): runs the LocalScripts' functions bound with
    /// `RunService:BindToRenderStep` in priority order, then fires
    /// `RunService.RenderStepped`, each with the frame's length in seconds.
    pub fn render_step(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        self.client.render_step(world, seconds);
        self.collect(Side::Client);
        self.fire_changes(world);
    }

    /// Says the game runs in the editor's Play, for
    /// `RunService:IsStudio()`. The player never calls it.
    pub fn set_studio(&mut self, studio: bool) {
        let flags = run_service::RunFlags {
            studio,
            running: true,
        };
        self.server.set_run_flags(flags);
        self.client.set_run_flags(flags);
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

    /// Delivers a button activation only to LocalScripts. A button whose
    /// `Interactable` is off doesn't fire.
    pub fn activate_ui_button(&mut self, world: &mut World, button: InstanceId) {
        let fires = world.get(button).is_some_and(|instance| {
            instance.class() == ClassName::TextButton
                && instance.property("Interactable").and_then(Value::as_bool) != Some(false)
        });
        if fires {
            self.fire_ui_event(world, button, "Activated");
        }
    }

    /// Fires `event` of a UI element (such as `MouseEnter`) only in
    /// LocalScripts. An element that's gone is skipped.
    pub fn fire_ui_event(&mut self, world: &mut World, element: InstanceId, event: &'static str) {
        self.fire_changes(world);
        if world.contains(element) {
            self.client.fire_instance_event(world, element, event);
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
    ///
    /// Then scripts whose Instance was destroyed stop, and cloned scripts
    /// that are now where scripts run start.
    pub fn fire_changes(&mut self, world: &mut World) {
        self.fire_change_rounds(world);
        self.fire_health_events(world);
        self.fire_tool_events(world);
        self.fire_player_events(world);
        self.fire_level_events(world);
        self.stop_destroyed_scripts(world);
        self.start_clones(world);
    }

    fn fire_change_rounds(&mut self, world: &mut World) {
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
                trace: Vec::new(),
            });
        }
    }

    /// Stops every started script whose Instance is gone (`Destroy`). A
    /// script that destroyed itself has finished its current run by now,
    /// so this is "after the current resume".
    fn stop_destroyed_scripts(&mut self, world: &World) {
        let (server, client) = (&self.server, &self.client);
        self.started.retain(|started| {
            let alive = world.contains(started.script);
            if !alive {
                server.stop_script(started.script);
                client.stop_script(started.script);
            }
            alive
        });
    }

    /// Starts each cloned script that's now where scripts of its kind run:
    /// a Script in the Workspace or ServerScriptService, a LocalScript in
    /// the Workspace, StarterPlayerScripts or Players. A cloned ModuleScript
    /// can be required wherever it is. The rest wait for the next call;
    /// clones destroyed before they ran are forgotten.
    fn start_clones(&mut self, world: &mut World) {
        if self.starting_clones || self.clones.is_empty() {
            return;
        }
        self.starting_clones = true;
        for clone in std::mem::take(&mut self.clones) {
            let Some(class) = world.get(clone.script).map(|instance| instance.class()) else {
                continue;
            };
            let under = |place| world.is_descendant_of(clone.script, place);
            let runs = match class {
                ClassName::Script => {
                    under(world.workspace()) || under(world.server_script_service())
                }
                ClassName::LocalScript => {
                    under(world.workspace())
                        || under(world.starter_player_scripts())
                        || under(world.players())
                }
                _ => true,
            };
            if runs {
                self.start_script(world, clone.script, &clone.path, &clone.source);
            } else {
                self.clones.push(clone);
            }
        }
        self.starting_clones = false;
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

    /// How long a script may run without pausing before the watchdog stops
    /// it with an error. [`WATCHDOG_LIMIT`] (3 seconds) unless changed;
    /// tests use a shorter one.
    pub fn set_watchdog_limit(&mut self, limit: Duration) {
        self.server.set_watchdog_limit(limit);
        self.client.set_watchdog_limit(limit);
    }

    /// Turns on breakpoints, for the editor's Play. Call it before starting
    /// any script: only coroutines made after it can stop at a breakpoint.
    /// It makes every script line call a hook, so the player never calls it.
    pub fn enable_debugger(&mut self) {
        self.server.enable_debugger();
        self.client.enable_debugger();
    }

    /// Replaces every breakpoint: lines (from 1) by script path, as passed
    /// to [`Self::start_script`]. Does nothing unless
    /// [`Self::enable_debugger`] was called. Removing the breakpoint that's
    /// paused doesn't continue; the host does that.
    pub fn set_breakpoints(&mut self, breakpoints: &Breakpoints) {
        self.server.set_breakpoints(breakpoints);
        self.client.set_breakpoints(breakpoints);
    }

    /// Where a script is paused at a breakpoint, if one is. While paused,
    /// the host should stop running frames: nothing in the game should move.
    /// Anything it fires anyway (a Console command, an event) waits, and
    /// runs on [`Self::continue_from_pause`].
    pub fn pause(&self) -> Option<Pause> {
        self.server.pause().or_else(|| self.client.pause())
    }

    /// Continues from the pause: the paused script runs on from its line,
    /// then everything the pause held, in order. A breakpoint hit again
    /// pauses again, holding the rest.
    pub fn continue_from_pause(&mut self, world: &mut World) {
        let Some(side) = [Side::Server, Side::Client]
            .into_iter()
            .find(|&side| self.context(side).pause().is_some())
        else {
            return;
        };
        self.fire_changes(world);
        self.context(side).continue_from_pause(world);
        self.collect(side);
        self.fire_changes(world);
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
        self.output.extend(context.take_output());
        self.sound_actions.extend(context.take_sound_actions());
        self.particle_emits.extend(context.take_particle_emits());
        self.shatters.extend(context.take_shatters());
        self.health.pending.extend(context.take_health_events());
        self.tools.note(context.take_tool_changes());
        self.humanoids.note(context.take_humanoid_requests());
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
    // Not Play: `RunService:IsRunning()` is false, and it's the editor.
    context.set_run_flags(run_service::RunFlags {
        studio: true,
        running: false,
    });
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
    push_line(
        lua,
        OutputLine {
            kind,
            text,
            trace: Vec::new(),
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
mod sandbox_tests;

#[cfg(test)]
mod watchdog_tests;

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
mod data_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod joint_tests;
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
mod tool_tests;
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
