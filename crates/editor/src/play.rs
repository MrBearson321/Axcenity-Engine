//! Play mode: the game running inside the editor.
//!
//! A [`Session`] runs on its own deep copy of the edit world, with its own
//! scripts and physics, so nothing that happens during Play touches the
//! world being edited. Stop simply drops the session.
//!
//! Every call into the game is guarded: a Rust panic (an engine bug) is
//! caught here and returned as a [`Crash`], so the editor can report it and
//! go back to editing instead of closing.

use std::any::Any;
use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_audio::{GameAudio, SoundAction};
use axcenity_character::MoveInput;
use axcenity_core::input::{InputEvent, InputMap, InputState};
use axcenity_core::{InstanceId, KeyCode, MouseBehavior, Vector3, World};
use axcenity_net::Network;
use axcenity_project::{LoadingScreen, ScriptFile};
use axcenity_render::{Capabilities, GraphicsSettings};
use axcenity_script::{
    Breakpoints, CameraState, CameraView, GameCamera, KeyEvent, NetworkRequest, OutputKind, Pause,
    ScriptRuntime,
};
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};
use axcenity_ui::LoadingGui;
use eframe::egui;

use crate::output::Output;
use crate::profiler::{FrameCost, GameLoad};

pub mod kept;
pub mod multi;

pub use kept::PlayEdits;
#[cfg(test)]
pub use multi::PlayView;
pub use multi::{Play, PlaySettings};

/// `NetworkService.ConnectionFailed`'s reason for `Host` and `Join` in Play.
const PLAY_IS_OFFLINE: &str = "Play doesn't host or join games: run the game in the player \
                               (axcenity-player --host, or --join address:port).";

/// The message of a Rust panic caught during Play.
#[derive(Debug)]
pub struct Crash(pub String);

/// The game while it plays: the copied world, the scripts running in it and
/// the physics moving its parts.
pub struct Session {
    /// The copied world, the scripts running in it, its physics and so on.
    sim: Sim,
    /// Editor Play's frame.
    schedule: Schedule,
    /// Dropped at Stop, which silences every sound.
    audio: GameAudio,
    /// Buttons' hover and click sounds, played with the next frame's audio.
    ui_sounds: Vec<SoundAction>,
    /// The project's own loading screen, as the built game shows it.
    loading_gui: Option<LoadingGui>,
    /// How it goes, to show it again for each level loaded (Phase 48).
    loading: LoadingScreen,
    /// Set by Debug > Panic during Play, to check that a crash is caught.
    panic_next_frame: bool,
    /// What the last frame's steps took, for the Profiler; `None` when it
    /// didn't run (paused at a breakpoint).
    cost: Option<FrameCost>,
    /// The last frame's steps' times under the names every host's
    /// benchmark uses (roadmap I1, `docs/development/benchmarks.md`).
    laps: axcenity_core::bench::Laps,
    /// The graphics settings as a LocalScript last changed them
    /// (`UserGameSettings`), for the viewport until Stop. `None` until one
    /// does: the editor's own settings are used.
    graphics: Option<GraphicsSettings>,
    /// What scripts did to the camera last frame (`CameraType`, field of
    /// view, shake), for the viewport.
    game_camera: GameCamera,
    /// Values edited in Properties during Play, and which Stop keeps.
    kept: PlayEdits,
    /// Play with several clients (Phase 64): the first device hosts in
    /// memory, the others join it. Offline with one.
    network: Network,
    /// The player's name on a client device, for its Output lines; `None`
    /// on the first device, whose lines are as they were.
    label: Option<String>,
    /// The server view: no input, and the fly camera.
    observing: bool,
}

/// The game's levels for Play's `LevelService` (Phase 48): every level by
/// name, each a world of its own, with the start level's name and the
/// level Play starts in. Empty for a game without levels.
#[derive(Clone, Default)]
pub struct PlayLevels {
    pub start: String,
    pub current: String,
    pub worlds: std::collections::BTreeMap<String, World>,
}

impl Session {
    /// Copies `edit_world` and starts every script in it, in the order given
    /// (Scripts, then LocalScripts, as the project lists them), the same way
    /// the player does. Script output and errors go to `output`. MeshParts
    /// collide as their models from `assets`. Unlike the player, scripts
    /// stop at `breakpoints`; one in a script's first run pauses here, and
    /// the scripts after it start on Continue. The project's own `loading`
    /// screen shows as in the built game; the Axcenity splash doesn't.
    /// `SaveService` writes its slots in `save_folder` (the player's data for
    /// the game, as in the built game), never the project. LocalScripts see
    /// the editor's `graphics` settings (with the GPU's `capabilities`) as
    /// `UserGameSettings`, and their changes last until Stop. Actions come
    /// from `input_map` (the project's `input.json`).
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        edit_world: &World,
        scripts: &[ScriptFile],
        breakpoints: &Breakpoints,
        output: &mut Output,
        assets: Assets,
        camera: CameraState,
        loading: &LoadingScreen,
        input_map: &InputMap,
        save_folder: Option<PathBuf>,
        graphics: &GraphicsSettings,
        capabilities: &Capabilities,
        levels: PlayLevels,
    ) -> Result<Session, Crash> {
        let mut session = Session {
            sim: Sim::new(edit_world.clone(), ScriptRuntime::new(), assets),
            schedule: Schedule::for_host(Hosts::EDITOR_PLAY),
            audio: GameAudio::default(),
            ui_sounds: Vec::new(),
            loading_gui: None,
            loading: loading.clone(),
            panic_next_frame: false,
            cost: None,
            laps: axcenity_core::bench::Laps::start(),
            graphics: None,
            game_camera: GameCamera::default(),
            kept: PlayEdits::default(),
            network: Network::new(multi::play_game()),
            label: None,
            observing: false,
        };
        // Up before any script runs, as in the player.
        session.loading_gui = LoadingGui::start(
            &mut session.sim.world,
            &loading.screen_gui,
            loading.min_seconds,
            loading.wait_for_script,
        );
        if !loading.screen_gui.is_empty() && session.loading_gui.is_none() {
            output.warning(format!(
                "The loading screen \"{}\" is not a ScreenGui in the Workspace, so built games \
                 show the Axcenity splash (or nothing) instead. Pick another in File > Build Game.",
                loading.screen_gui
            ));
        }
        // The character each player spawns as (Phase 47), before scripts
        // look for it.
        if let Some(folder) = session.sim.assets.folder()
            && let Err(err) =
                axcenity_project::load_starter_character(&folder, &mut session.sim.world)
        {
            output.warning(err);
        }
        session.sim.scripts.set_assets(session.sim.assets.clone());
        session.sim.scripts.set_save_folder(save_folder);
        // Who plays, the same each time on this install (Phase 64).
        session.sim.scripts.set_user_id(play_user_id());
        session.sim.scripts.set_input_map(input_map.clone());
        if !levels.worlds.is_empty() {
            session
                .sim
                .scripts
                .set_levels(&levels.start, &levels.current, levels.worlds);
        }
        session.sim.scripts.set_graphics(graphics, capabilities);
        session.sim.scripts.enable_debugger();
        // `RunService:IsStudio()` is true in the editor's Play.
        session.sim.scripts.set_studio(true);
        session.sim.scripts.set_breakpoints(breakpoints);
        guard(|| {
            // Animators start in their default states before any script
            // runs, as in the player, so scripts can read them.
            session
                .sim
                .animators
                .update(&mut session.sim.world, &session.sim.assets, 0.0);
            session.forward_animator_errors(output);
            for script in scripts {
                session.sim.scripts.start_script(
                    &mut session.sim.world,
                    script.instance,
                    &script.path,
                    &script.source,
                );
                session.forward_output(output);
            }
            session.apply_audio(camera, output);
        })?;
        Ok(session)
    }

    /// The loading screen's ScreenGui while it covers the game.
    pub fn loading_screen(&self) -> Option<InstanceId> {
        self.loading_gui.as_ref().and_then(LoadingGui::on_top)
    }

    /// The player character's Inventory (Phase 45), whose hotbar the
    /// viewport draws.
    pub fn hotbar(&self) -> Option<InstanceId> {
        self.sim.scripts.player_inventory(&self.sim.world)
    }

    /// The player's Abilities and their cooldowns (Phase 76), for the
    /// ability bar the viewport draws.
    pub fn ability_bar(&self) -> Vec<axcenity_core::ability::AbilitySlot> {
        self.sim.scripts.ability_bar(&self.sim.world)
    }

    /// The active quests (Phase 77), for the quest tracker the viewport
    /// draws.
    pub fn quest_tracker(&self) -> Vec<axcenity_core::dialogue::QuestView> {
        self.sim.scripts.quest_tracker(&self.sim.world)
    }

    /// What the dialogue box shows while a dialogue runs (Phase 77).
    pub fn dialogue_view(&self) -> Option<axcenity_core::dialogue::DialogueView> {
        self.sim.scripts.dialogue_view(&self.sim.world)
    }

    /// Whether the viewport draws the default leaderboard (Phase 47): Tab
    /// hides and shows it.
    pub fn leaderboard(&self) -> bool {
        self.sim.scripts.leaderboard_shown()
    }

    /// The chat window's lines (Phase 67): who said each (`None` for a
    /// system message) and what.
    pub fn chat_lines(&self) -> Vec<(Option<String>, String)> {
        self.sim
            .scripts
            .chat_lines()
            .into_iter()
            .map(|line| (line.from, line.text))
            .collect()
    }

    /// Sends a line typed in the chat window, as this device's player.
    pub fn send_chat(&mut self, text: &str) {
        self.sim.scripts.send_chat(text);
    }

    /// Why this device's player was kicked (`Player:Kick`), once.
    pub fn take_kicked(&mut self) -> Option<String> {
        self.network.take_kicked()
    }

    /// The ProximityPrompts that show (Phase 44), for the viewport to draw.
    pub fn prompts(&self) -> Vec<axcenity_ui::PromptView> {
        self.sim
            .scripts
            .shown_prompts()
            .iter()
            .map(|shown| axcenity_ui::PromptView {
                prompt: shown.prompt,
                position: [shown.position.x, shown.position.y, shown.position.z],
                progress: shown.progress,
                key: shown.key.clone(),
            })
            .collect()
    }

    /// The cursor over the ClickDetector the mouse points at, as a CSS
    /// cursor name, if it points at one.
    pub fn click_cursor(&self) -> Option<String> {
        self.sim.scripts.click_cursor(&self.sim.world)
    }

    /// The world as the game has it now, for the panels and the viewport.
    pub fn world(&self) -> &World {
        &self.sim.world
    }

    /// The audio's peak levels since the last call, for the Mixer panel.
    pub fn take_audio_levels(&mut self) -> axcenity_audio::Levels {
        self.audio.take_levels()
    }

    /// The Mixer panel's mute and solo, heard from the next frame on.
    pub fn set_audio_monitor(&mut self, monitor: axcenity_audio::Monitor) {
        self.audio.set_monitor(monitor);
    }

    /// The game's world, for the Properties panel's edits during Play. The
    /// game's scripts see those changes at the next frame.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.sim.world
    }

    /// The values edited in Properties during Play, and which are kept.
    pub fn kept(&self) -> &PlayEdits {
        &self.kept
    }

    pub fn kept_mut(&mut self) -> &mut PlayEdits {
        &mut self.kept
    }

    /// Where a script is paused at a breakpoint, if one is. While paused,
    /// [`Self::frame`] does nothing, so the whole game holds still.
    pub fn pause(&self) -> Option<Pause> {
        self.sim.scripts.pause()
    }

    pub fn set_breakpoints(&mut self, breakpoints: &Breakpoints) {
        self.sim.scripts.set_breakpoints(breakpoints);
    }

    /// Continues from the pause: the paused script runs on from its line,
    /// then whatever waited for it. It may pause again at a breakpoint.
    pub fn continue_from_pause(&mut self, output: &mut Output) -> Result<(), Crash> {
        guard(|| {
            self.sim.scripts.continue_from_pause(&mut self.sim.world);
            self.forward_output(output);
        })
    }

    /// Makes the next frame panic, as an engine bug would.
    pub fn panic_next_frame(&mut self) {
        self.panic_next_frame = true;
    }

    /// Fires a UI button event inside the guarded Play session.
    pub fn activate_ui_button(
        &mut self,
        button: axcenity_core::InstanceId,
        output: &mut Output,
    ) -> Result<(), Crash> {
        guard(|| {
            self.sim
                .scripts
                .activate_ui_button(&mut self.sim.world, button);
            self.forward_output(output);
        })
    }

    /// Passes on what the viewport's UI did, inside the guarded Play
    /// session: `MouseEnter` and `MouseLeave` to LocalScripts, and buttons'
    /// sounds to the next frame's audio.
    pub fn ui_events(
        &mut self,
        events: Vec<axcenity_ui::UiEvent>,
        output: &mut Output,
    ) -> Result<(), Crash> {
        guard(|| {
            for event in events {
                match event {
                    axcenity_ui::UiEvent::MouseEnter(id) => {
                        self.sim
                            .scripts
                            .fire_ui_event(&mut self.sim.world, id, "MouseEnter");
                    }
                    axcenity_ui::UiEvent::MouseLeave(id) => {
                        self.sim
                            .scripts
                            .fire_ui_event(&mut self.sim.world, id, "MouseLeave");
                    }
                    axcenity_ui::UiEvent::PlaySound(sound_id) => {
                        self.ui_sounds.push(SoundAction::PlayOnce { sound_id });
                    }
                }
                self.forward_output(output);
            }
        })
    }

    /// Types into a TextBox inside the guarded Play session.
    pub fn edit_text(
        &mut self,
        text_box: axcenity_core::InstanceId,
        command: axcenity_ui::TextCommand,
        output: &mut Output,
    ) -> Result<axcenity_ui::TextOutcome, Crash> {
        let mut outcome = axcenity_ui::TextOutcome::Ignored;
        guard(|| {
            outcome = axcenity_ui::edit_text_box(&mut self.sim.world, text_box, command);
            self.forward_output(output);
        })?;
        Ok(outcome)
    }

    /// Fires `FocusLost` for a TextBox the player finished typing in.
    pub fn release_text_box(
        &mut self,
        text_box: axcenity_core::InstanceId,
        output: &mut Output,
    ) -> Result<(), Crash> {
        guard(|| {
            self.sim
                .scripts
                .release_text_box(&mut self.sim.world, text_box);
            self.forward_output(output);
        })
    }

    /// Runs Lua typed into the Console against the game (see
    /// `ScriptRuntime::run_command`). What it prints, and its errors, go to
    /// `output`. The inner `Err` is the message for code that isn't valid Lua.
    pub fn run_command(
        &mut self,
        source: &str,
        output: &mut Output,
    ) -> Result<Result<(), String>, Crash> {
        let mut result = Ok(());
        guard(|| {
            result = self.sim.scripts.run_command(&mut self.sim.world, source);
            self.forward_output(output);
        })?;
        Ok(result)
    }

    /// Runs `line`, typed in the Developer Console (Phase 71), as one of
    /// the game's `ConsoleService` commands. What it prints goes to `output`.
    pub fn run_console_command(
        &mut self,
        line: &str,
        output: &mut Output,
    ) -> Result<axcenity_script::ConsoleRun, Crash> {
        let mut run = axcenity_script::ConsoleRun::Unknown;
        guard(|| {
            run = self
                .sim
                .scripts
                .run_console_command(&mut self.sim.world, line);
            self.forward_output(output);
        })?;
        Ok(run)
    }

    /// The commands the game's scripts registered, for the console's `help`.
    pub fn console_commands(&self) -> Vec<axcenity_script::ConsoleCommandInfo> {
        self.sim.scripts.console_commands()
    }

    /// Whether `name` means something to Lua typed in the console.
    pub fn console_knows(&self, name: &str) -> bool {
        self.sim.scripts.console_knows(&self.sim.world, name)
    }

    /// Turns the console's `noclip` on or off; returns whether it's on.
    pub fn toggle_noclip(&mut self) -> bool {
        let on = !self.sim.characters.noclip();
        self.sim.characters.set_noclip(on);
        on
    }

    /// Moves the player's character so its feet are at `to` (the console's
    /// `teleport`). False when there's none.
    pub fn teleport(&mut self, to: Vector3) -> bool {
        self.sim.characters.teleport(&mut self.sim.world, to)
    }

    /// The follow camera of a third-person controller, if the play world has
    /// one with a body. The fly camera is left alone; Stop shows it again.
    #[cfg(test)]
    pub fn character_camera(&self) -> Option<axcenity_character::CharacterCamera> {
        self.sim.characters.view(&self.sim.world)
    }

    /// Whether the game holds the view (a character controller, or a
    /// CameraType other than Custom), so the fly camera stays put.
    pub fn holds_camera(&self) -> bool {
        !self.observing
            && (self.sim.characters.view(&self.sim.world).is_some()
                || self.sim.scripts.camera_scripted())
    }

    /// The camera the viewport draws during Play, given the fly camera:
    /// a character controller's or the fly camera, unless scripts hold it,
    /// with the scripts' field of view and shake. The fly camera itself is
    /// never moved by it, so Stop shows it where it was.
    pub fn shown_camera(&self, fly: CameraState) -> CameraView {
        if self.observing {
            // The server view: the fly camera, as the scripts don't move it.
            return GameCamera::default().view(fly);
        }
        self.game_camera.view(self.script_camera(fly))
    }

    /// The viewport's size in pixels (`Camera.ViewportSize`), before a frame.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.sim.scripts.set_viewport_size(width, height);
    }

    /// Runs one frame, `seconds` long, with the input since the last frame
    /// (keys, the mouse over the viewport, controllers), character
    /// `motion`, and the viewport's fly
    /// camera. Character controllers step after input and before scripts;
    /// their parts are anchored just before physics. The viewport lays out
    /// UI and draws. Returns where a LocalScript moved the camera, if one
    /// did and no character controller owns the camera.
    ///
    /// While a script is paused at a breakpoint it does nothing, and a pause
    /// during the frame ends it there: physics, animation and the frame's
    /// later events wait until after Continue.
    pub fn frame(
        &mut self,
        seconds: f64,
        input: &[InputEvent],
        motion: MoveInput,
        camera: CameraState,
        output: &mut Output,
    ) -> Result<Option<CameraState>, Crash> {
        self.cost = None;
        if self.sim.scripts.pause().is_some() {
            return Ok(None);
        }
        // Each step's time, for the Profiler. Reading the clock costs next
        // to nothing, so it's measured whether or not the panel is open.
        let started = Instant::now();
        self.laps = axcenity_core::bench::Laps::start();
        profiling::scope!("Play frame");
        let schedule = std::mem::take(&mut self.schedule);
        let mut laps = std::mem::take(&mut self.laps);
        let mut host = PlayHost {
            session: self,
            input,
            motion,
            camera,
            output,
            moved: None,
            physics_steps: 0,
        };
        let result = guard(|| {
            if host.session.panic_next_frame {
                panic!("a deliberate engine panic, from Debug > Panic during Play");
            }
            schedule.run(
                &mut FrameCtx::new(seconds, Hosts::EDITOR_PLAY, &mut host),
                &mut laps,
            );
        });
        let (moved, physics_steps) = (host.moved, host.physics_steps);
        self.schedule = schedule;
        self.laps = laps;
        self.cost = Some(FrameCost::from_laps(
            &self.laps,
            physics_steps,
            started.elapsed(),
        ));
        result?;
        // The controller owns the camera, so a LocalScript's move is not
        // copied onto the fly camera. (A frame a breakpoint ended keeps it,
        // as Continue would.)
        let held = self.sim.scripts.pause().is_none()
            && self.sim.characters.view(&self.sim.world).is_some();
        Ok(if held { None } else { moved })
    }

    /// The camera scripts should see: the follow camera when a controller
    /// owns it, otherwise `fallback` (the fly camera, or a LocalScript's move).
    fn script_camera(&self, fallback: CameraState) -> CameraState {
        match self.sim.characters.view(&self.sim.world) {
            Some(view) => CameraState {
                position: view.position,
                orientation: view.orientation,
            },
            None => fallback,
        }
    }

    /// The input map with the player's rebinds, for the controllers and
    /// UI navigation.
    pub fn input_map(&self) -> InputMap {
        self.sim.scripts.input_map()
    }

    /// `UserInputService.MouseBehavior` and `MouseIconEnabled`.
    pub fn mouse(&self) -> (MouseBehavior, bool) {
        self.sim.scripts.mouse()
    }

    /// What the last frame's steps took, or `None` if it didn't run.
    /// The last frame's steps' times, by the benchmark's stage names. Only
    /// `bench_project_play` reads them so far; the Profiler shows `cost`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn laps(&self) -> &axcenity_core::bench::Laps {
        &self.laps
    }

    pub fn cost(&self) -> Option<FrameCost> {
        self.cost
    }

    /// How much the game has to simulate, for the Profiler.
    pub fn load(&self) -> GameLoad {
        GameLoad {
            bodies: self.sim.physics.body_count(),
            sounds_playing: self.audio.playing_count(),
            lua_bytes: self.sim.scripts.used_memory(),
        }
    }

    /// The posed models of the animated MeshParts, for the viewport.
    pub fn poses(&self) -> &axcenity_scene::Poses {
        self.sim.animators.poses()
    }

    /// The path each NavigationAgent is walking, from its feet, for the
    /// Agent Paths overlay. Reading them changes nothing.
    pub fn agent_paths(&self) -> Vec<(InstanceId, Vec<Vector3>)> {
        self.sim.agents.paths()
    }

    /// Recent `workspace:Raycast` calls, for the editor overlay.
    pub fn debug_rays(&self) -> Vec<axcenity_script::DebugRay> {
        self.sim.scripts.debug_rays()
    }

    /// The asset file at `path` changed on disk during Play: Animators
    /// using it as a clip, state machine or model pick it up at the next
    /// frame, in the state they're in, and a Sound uses it the next time it
    /// plays. A sound that can't be decoded keeps its last good version.
    /// True if a playing Animator or a Sound that has played uses it.
    pub fn asset_changed(&mut self, path: &str, output: &mut Output) -> bool {
        let animated = self.sim.animators.reload(path);
        let heard = match self.audio.reload(path, &self.sim.assets) {
            Ok(heard) => heard,
            Err(err) => {
                output.error(format!(
                    "Could not reload `{path}`: {err}. The last good version stays."
                ));
                false
            }
        };
        animated || heard
    }

    /// Runs a script's changed file in place (see
    /// `ScriptRuntime::reload_script`), inside the guard. Output says
    /// "Reloaded Door.server.lua", or shows the syntax error with its line
    /// and that the last good version still runs.
    pub fn reload_script(
        &mut self,
        script: InstanceId,
        path: &str,
        source: &str,
        output: &mut Output,
    ) -> Result<(), Crash> {
        let name = path.rsplit('/').next().unwrap_or(path);
        guard(|| {
            let reloaded =
                self.sim
                    .scripts
                    .reload_script(&mut self.sim.world, script, path, source);
            self.forward_output(output);
            match reloaded {
                Ok(()) => output.info(format!("Reloaded {name}")),
                Err(_) if !self.sim.world.contains(script) => output.info(format!(
                    "{name} isn't in the game any more, so it wasn't reloaded."
                )),
                Err(_) => output.warning(format!(
                    "{name} still runs its last good version until the error is fixed."
                )),
            }
        })
    }

    fn forward_animator_errors(&mut self, output: &mut Output) {
        for error in self.sim.animators.take_errors() {
            output.error(error);
        }
    }

    /// The graphics settings a LocalScript changed during this Play, which
    /// the viewport uses in place of the editor's until Stop.
    pub fn graphics(&self) -> Option<&GraphicsSettings> {
        self.graphics.as_ref()
    }

    /// The `emitter:Emit(count)` calls scripts made since the last call.
    pub fn take_particle_emits(&mut self) -> Vec<(InstanceId, u32)> {
        self.sim.scripts.take_particle_emits()
    }

    /// Moves what scripts printed, and their errors, into the Output panel.
    fn forward_output(&mut self, output: &mut Output) {
        for line in self.sim.scripts.take_output() {
            // A client device's lines say whose they are.
            let text = match &self.label {
                Some(label) => format!("{} ({label})", line.text),
                None => line.text,
            };
            match line.kind {
                OutputKind::Print => output.info(text),
                OutputKind::Warning => output.warning(text),
                OutputKind::Error => output.script_error(text, line.trace),
            }
        }
    }

    /// Who joined and left, and corrections, from the network's log.
    fn forward_network_log(&mut self, output: &mut Output) {
        for line in self.network.take_log() {
            let text = match &self.label {
                Some(label) => format!("{line} ({label})"),
                None => line,
            };
            match text.strip_prefix("warning: ") {
                Some(warning) => output.warning(warning.to_owned()),
                None => output.info(text),
            }
        }
    }

    fn apply_audio(&mut self, camera: CameraState, output: &mut Output) {
        let mut actions = self.sim.scripts.take_sound_actions();
        actions.append(&mut self.ui_sounds);
        let listener = axcenity_audio::Listener::from_view(camera.position, camera.orientation);
        let update = self
            .audio
            .update(actions, &self.sim.world, &self.sim.assets, listener);
        for err in update.errors {
            output.error(format!("Audio: {err}"));
        }
        if !update.ended.is_empty() {
            self.sim
                .scripts
                .fire_sounds_ended(&mut self.sim.world, &update.ended);
            self.forward_output(output);
        }
    }
}

/// What Play's frame needs that isn't the game itself: the input and the
/// camera this frame, and the Output panel. It lives only as long as a
/// frame, unlike the [`Session`].
struct PlayHost<'a> {
    session: &'a mut Session,
    input: &'a [InputEvent],
    motion: MoveInput,
    /// The viewport's fly camera.
    camera: CameraState,
    output: &'a mut Output,
    /// Where a LocalScript moved the camera, for the viewport.
    moved: Option<CameraState>,
    physics_steps: u32,
}

impl HostIo for PlayHost<'_> {
    fn sim(&mut self) -> &mut Sim {
        &mut self.session.sim
    }

    fn following(&self) -> bool {
        self.session.network.is_following()
    }

    /// Play doesn't host or join (Phase 63): `NetworkService` calls fail
    /// with a reason, and LAN searches find nothing. Then what the other
    /// Play devices sent (Phase 64).
    fn net_receive(&mut self, seconds: f64) -> ControlFlow<()> {
        let session = &mut *self.session;
        for request in session.sim.scripts.take_network_requests() {
            match request {
                NetworkRequest::FindLanGames => {
                    session
                        .sim
                        .scripts
                        .finish_lan_search(&mut session.sim.world, &[]);
                }
                NetworkRequest::Disconnect => {}
                // The viewport streams around the camera, and Play has
                // no other device to send a region to.
                NetworkRequest::StreamAround { request, .. } => {
                    session
                        .sim
                        .scripts
                        .finish_stream_around(&mut session.sim.world, request);
                }
                NetworkRequest::Host { .. } | NetworkRequest::Join { .. } => {
                    session
                        .sim
                        .scripts
                        .fire_connection_failed(&mut session.sim.world, PLAY_IS_OFFLINE);
                }
            }
        }
        session
            .network
            .receive(&mut session.sim.world, &mut session.sim.scripts, seconds);
        session.forward_network_log(self.output);
        session.forward_output(self.output);
        ControlFlow::Continue(())
    }

    /// A Sequence that locks input leaves the character still, and the
    /// server view takes none. Each device moves its own player's
    /// character (Phase 66).
    fn character_motion(&mut self, _seconds: f32) -> MoveInput {
        let session = &mut *self.session;
        let motion = if session.sim.scripts.movement_locked(&session.sim.world) || session.observing
        {
            MoveInput::default()
        } else {
            std::mem::take(&mut self.motion)
        };
        let others = session
            .network
            .other_characters(&session.sim.world, &session.sim.scripts);
        let player = if session.observing {
            None
        } else {
            session.sim.scripts.local_player()
        };
        session.sim.characters.set_players(player, others);
        session.network.set_running(motion.run);
        motion
    }

    fn flush_output(&mut self) {
        self.session.forward_output(self.output);
    }

    fn agent_warning(&mut self, warning: String) {
        self.output.warning(warning);
    }

    /// A third-person controller owns the camera when one is present. The
    /// server view takes no input.
    fn frame_input(&mut self, _seconds: f64) -> Vec<InputEvent> {
        let session = &mut *self.session;
        let seen = session.script_camera(self.camera);
        session.sim.scripts.set_camera(seen);
        if session.observing {
            Vec::new()
        } else {
            self.input.to_vec()
        }
    }

    /// Parts another device moves are held still here, and physics sleeps
    /// far from the camera and the other devices (Phase 80).
    fn prepare_physics(&mut self) {
        let session = &mut *self.session;
        let held = session
            .network
            .held_parts(&session.sim.world, &session.sim.scripts);
        session.sim.physics.set_held(held);
        let far = axcenity_core::streaming::Settings::of(&session.sim.world).map(|settings| {
            let at = self.camera.position;
            let mut focuses = vec![axcenity_core::Vector3::new(at.x, at.y, at.z)];
            focuses.extend(session.network.player_focuses(&session.sim.world));
            (focuses, settings.unload_radius())
        });
        session.sim.physics.set_far_sleep(far);
    }

    fn owns_physics(&self) -> bool {
        self.session.network.simulates_physics()
    }

    fn physics_stepped(&mut self, steps: u32) {
        self.physics_steps = steps;
    }

    /// Only the first device is heard: the others are the same game on
    /// more players' devices.
    fn audio(&mut self) {
        let session = &mut *self.session;
        let listener = session.shown_camera(self.moved.unwrap_or(self.camera));
        let listener = CameraState {
            position: listener.position,
            orientation: listener.orientation,
        };
        if session.label.is_none() {
            session.apply_audio(listener, self.output);
        } else {
            drop(session.sim.scripts.take_sound_actions());
            session.ui_sounds.clear();
        }
    }

    fn animator_error(&mut self, error: String) {
        self.output.error(error);
    }

    /// A level loaded this frame shows the loading screen again.
    fn level_loads(&mut self) {
        let session = &mut *self.session;
        if !session.sim.scripts.take_level_loads().is_empty()
            && let Some(shown) = LoadingGui::start(
                &mut session.sim.world,
                &session.loading.screen_gui,
                session.loading.min_seconds,
                false,
            )
        {
            session.loading_gui = Some(shown);
        }
    }

    fn loading_screen(&mut self, seconds: f64) {
        let session = &mut *self.session;
        let ready = session.sim.scripts.take_loading_screen_removed();
        if let Some(loading) = &mut session.loading_gui {
            loading.advance(&mut session.sim.world, seconds as f32, ready);
        }
    }

    fn camera_moved(&mut self, moved: Option<CameraState>) {
        self.moved = moved;
    }

    fn game_camera(&mut self, camera: GameCamera) {
        self.session.game_camera = camera;
    }

    fn graphics_changed(&mut self) {
        let session = &mut *self.session;
        if let Some(graphics) = session.sim.scripts.take_graphics_change() {
            session.graphics = Some(graphics);
        }
    }

    /// Last: the world's changes and remote messages for the other Play
    /// devices.
    fn net_send(&mut self) {
        let session = &mut *self.session;
        session
            .network
            .send(&session.sim.world, &mut session.sim.scripts);
        session.forward_network_log(self.output);
    }
}

/// Turns the editor's keyboard events into key presses for LocalScripts
/// during Play, and follows what the game holds for the controllers.
#[derive(Default)]
pub struct GameKeys {
    held: BTreeSet<KeyCode>,
    /// What the game holds: everything but the presses the UI took.
    game: InputState,
    /// Shift, Ctrl and Alt as last seen. egui reports them as modifiers, not
    /// keys, so a change in them is a press or release of the left one.
    modifiers: egui::Modifiers,
}

impl GameKeys {
    /// The presses and releases in this frame's `events`. While `typing`
    /// (a text box has the keyboard), or when the window lost focus, every
    /// held key is let go and new presses are ignored.
    pub fn read(
        &mut self,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        focused: bool,
        typing: bool,
    ) -> Vec<KeyEvent> {
        let mut keys = Vec::new();
        if typing || !focused {
            for key in std::mem::take(&mut self.held) {
                keys.push(KeyEvent {
                    key,
                    pressed: false,
                });
            }
            self.modifiers = egui::Modifiers::NONE;
            return keys;
        }
        let mut change = |key: KeyCode, pressed: bool| {
            let changed = if pressed {
                self.held.insert(key)
            } else {
                self.held.remove(&key)
            };
            if changed {
                keys.push(KeyEvent { key, pressed });
            }
        };
        let was = self.modifiers;
        for (key, before, now) in [
            (KeyCode::LeftShift, was.shift, modifiers.shift),
            (KeyCode::LeftControl, was.ctrl, modifiers.ctrl),
            (KeyCode::LeftAlt, was.alt, modifiers.alt),
        ] {
            if before != now {
                change(key, now);
            }
        }
        self.modifiers = modifiers;
        for event in events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed,
                ..
            } = event
                // The key's place on the keyboard, like the player, when known.
                && let Some(key) = script_key(physical_key.unwrap_or(*key))
            {
                change(key, *pressed);
            }
        }
        keys
    }

    /// Follows a frame's `input` as the game sees it.
    pub fn follow(&mut self, input: &[InputEvent]) {
        for event in input {
            self.game.follow(event);
        }
    }

    /// A character controller's input from `map`'s actions in what the
    /// game holds, plus mouse look and wheel notches since the last frame,
    /// `seconds` after it. Then the next frame starts.
    pub fn motion(&mut self, map: &InputMap, look: [f32; 2], zoom: f32, seconds: f32) -> MoveInput {
        let motion = MoveInput::from_actions(map, &self.game, look, zoom, seconds);
        self.game.end_frame();
        motion
    }
}

/// What a key does to the game UI's focus during Play, by `map`'s UI
/// actions: by default Tab and Shift+Tab step through it, the arrows move
/// it, Enter uses the focused element and Escape lets go, as in the player.
pub fn nav_key(map: &InputMap, key: egui::Key, shift: bool) -> Option<axcenity_ui::NavKey> {
    axcenity_ui::nav_key(map, script_key(key)?, shift)
}

/// The key scripts know as `Enum.KeyCode.<name>` for an egui key, if it's
/// one they can see. Shift, Ctrl and Alt come from the modifiers instead.
pub fn script_key(key: egui::Key) -> Option<KeyCode> {
    use egui::Key as E;
    let key = match key {
        E::A => KeyCode::A,
        E::B => KeyCode::B,
        E::C => KeyCode::C,
        E::D => KeyCode::D,
        E::E => KeyCode::E,
        E::F => KeyCode::F,
        E::G => KeyCode::G,
        E::H => KeyCode::H,
        E::I => KeyCode::I,
        E::J => KeyCode::J,
        E::K => KeyCode::K,
        E::L => KeyCode::L,
        E::M => KeyCode::M,
        E::N => KeyCode::N,
        E::O => KeyCode::O,
        E::P => KeyCode::P,
        E::Q => KeyCode::Q,
        E::R => KeyCode::R,
        E::S => KeyCode::S,
        E::T => KeyCode::T,
        E::U => KeyCode::U,
        E::V => KeyCode::V,
        E::W => KeyCode::W,
        E::X => KeyCode::X,
        E::Y => KeyCode::Y,
        E::Z => KeyCode::Z,
        E::Num0 => KeyCode::Zero,
        E::Num1 => KeyCode::One,
        E::Num2 => KeyCode::Two,
        E::Num3 => KeyCode::Three,
        E::Num4 => KeyCode::Four,
        E::Num5 => KeyCode::Five,
        E::Num6 => KeyCode::Six,
        E::Num7 => KeyCode::Seven,
        E::Num8 => KeyCode::Eight,
        E::Num9 => KeyCode::Nine,
        E::Space => KeyCode::Space,
        E::Enter => KeyCode::Return,
        E::Escape => KeyCode::Escape,
        E::Tab => KeyCode::Tab,
        E::Backspace => KeyCode::Backspace,
        E::ArrowUp => KeyCode::Up,
        E::ArrowDown => KeyCode::Down,
        E::ArrowLeft => KeyCode::Left,
        E::ArrowRight => KeyCode::Right,
        _ => return None,
    };
    Some(key)
}

/// Runs `f`, catching a panic inside it and returning its message instead.
///
/// `catch_unwind` wants a closure that's "unwind safe": one that can't leave
/// half-changed state behind that someone uses after the panic. The session
/// (or world copy) a panic interrupts is thrown away, never used again, so
/// `AssertUnwindSafe` tells the compiler that's fine.
pub fn guard(f: impl FnOnce()) -> Result<(), Crash> {
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|payload| Crash(panic_message(&*payload)))
}

/// The text a panic was started with. `panic!("...")` carries a `&str`, and
/// `panic!("{x}")` a `String`; anything else has no readable message.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text.to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "no message".to_string()
    }
}

/// The newest text of each script in the project in `folder`, read again
/// from disk so Play runs what was last saved in the code editor. A script
/// that can't be read is left out, with the reason in `output`.
pub fn read_scripts(
    folder: &Path,
    scripts: &[ScriptFile],
    world: &World,
    output: &mut Output,
) -> Vec<ScriptFile> {
    let mut fresh = Vec::new();
    for script in scripts {
        // A UI Event Graph compiles against the world as it is now; one
        // with problems doesn't run.
        if axcenity_blocks::ui_graph::is_ui_graph(&script.path) {
            let compiled = axcenity_project::read_ui_graph(folder, &script.path)
                .map_err(|err| format!("{:#}", anyhow::Error::new(err)))
                .and_then(|graph| {
                    axcenity_project::compile_graph_for(world, &script.path, &graph).map_err(
                        |errors| {
                            let problems: Vec<String> =
                                errors.iter().map(ToString::to_string).collect();
                            format!(
                                "its Event Graph has problems, shown on its nodes in the UI \
                                 Designer: {}",
                                problems.join("; ")
                            )
                        },
                    )
                });
            match compiled {
                Ok(source) => fresh.push(ScriptFile {
                    instance: script.instance,
                    path: script.path.clone(),
                    source,
                }),
                Err(why) => output.error(format!("{} won't run: {why}", script.path)),
            }
            continue;
        }
        match axcenity_project::read_script(folder, &script.path) {
            Ok(source) => fresh.push(ScriptFile {
                instance: script.instance,
                path: script.path.clone(),
                source,
            }),
            Err(err) => {
                let err = anyhow::Error::new(err);
                output.error(format!("{} won't run: {err:#}", script.path));
            }
        }
    }
    fresh
}

/// This install's `UserId` for the Play's Player (Phase 64), kept in the
/// player's saves folder as in the built game.
#[cfg(not(test))]
fn play_user_id() -> f64 {
    axcenity_save::user_id() as f64
}

/// In tests, a fixed one, so tests never write to the real player's data.
#[cfg(test)]
fn play_user_id() -> f64 {
    1.0
}

#[cfg(test)]
mod tests;
