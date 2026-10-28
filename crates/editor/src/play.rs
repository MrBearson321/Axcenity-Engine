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
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_audio::{GameAudio, SoundAction};
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::input::{InputEvent, InputMap, InputState};
use axcenity_core::{InstanceId, KeyCode, MouseBehavior, Vector3, World};
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_project::{LoadingScreen, ScriptFile};
use axcenity_render::{Capabilities, GraphicsSettings};
use axcenity_script::{
    Breakpoints, CameraState, CameraView, GameCamera, KeyEvent, OutputKind, Pause, ScriptRuntime,
};
use axcenity_ui::LoadingGui;
use eframe::egui;

use crate::output::Output;
use crate::profiler::{FrameCost, GameLoad};

pub mod kept;

pub use kept::PlayEdits;

/// The message of a Rust panic caught during Play.
#[derive(Debug)]
pub struct Crash(pub String);

/// The game while it plays: the copied world, the scripts running in it and
/// the physics moving its parts.
pub struct Session {
    world: World,
    scripts: ScriptRuntime,
    physics: Physics,
    /// Walks third-person characters and owns the camera when one is present.
    characters: Controllers,
    /// Walks NavigationAgents along their paths.
    agents: Agents,
    /// Plays the Animators (state machines, AnimationTracks and played
    /// Animations), posing skeletons and setting keyed properties.
    animators: Animators,
    assets: Assets,
    /// Dropped at Stop, which silences every sound.
    audio: GameAudio,
    /// Buttons' hover and click sounds, played with the next frame's audio.
    ui_sounds: Vec<SoundAction>,
    /// The project's own loading screen, as the built game shows it.
    loading_gui: Option<LoadingGui>,
    /// Set by Debug > Panic during Play, to check that a crash is caught.
    panic_next_frame: bool,
    /// What the last frame's steps took, for the Profiler; `None` when it
    /// didn't run (paused at a breakpoint).
    cost: Option<FrameCost>,
    /// The graphics settings as a LocalScript last changed them
    /// (`UserGameSettings`), for the viewport until Stop. `None` until one
    /// does: the editor's own settings are used.
    graphics: Option<GraphicsSettings>,
    /// What scripts did to the camera last frame (`CameraType`, field of
    /// view, shake), for the viewport.
    game_camera: GameCamera,
    /// Values edited in Properties during Play, and which Stop keeps.
    kept: PlayEdits,
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
    ) -> Result<Session, Crash> {
        let mut session = Session {
            world: edit_world.clone(),
            scripts: ScriptRuntime::new(),
            physics: Physics::with_assets(assets.clone()),
            characters: Controllers::default(),
            agents: Agents::default(),
            animators: Animators::new(),
            assets,
            audio: GameAudio::default(),
            ui_sounds: Vec::new(),
            loading_gui: None,
            panic_next_frame: false,
            cost: None,
            graphics: None,
            game_camera: GameCamera::default(),
            kept: PlayEdits::default(),
        };
        // Up before any script runs, as in the player.
        session.loading_gui = LoadingGui::start(
            &mut session.world,
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
        if let Some(folder) = session.assets.folder()
            && let Err(err) = axcenity_project::load_starter_character(&folder, &mut session.world)
        {
            output.warning(err);
        }
        session.scripts.set_assets(session.assets.clone());
        session.scripts.set_save_folder(save_folder);
        session.scripts.set_input_map(input_map.clone());
        session.scripts.set_graphics(graphics, capabilities);
        session.scripts.enable_debugger();
        // `RunService:IsStudio()` is true in the editor's Play.
        session.scripts.set_studio(true);
        session.scripts.set_breakpoints(breakpoints);
        guard(|| {
            // Animators start in their default states before any script
            // runs, as in the player, so scripts can read them.
            session
                .animators
                .update(&mut session.world, &session.assets, 0.0);
            session.forward_animator_errors(output);
            for script in scripts {
                session.scripts.start_script(
                    &mut session.world,
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
        self.scripts.player_inventory(&self.world)
    }

    /// Whether the viewport draws the default leaderboard (Phase 47): Tab
    /// hides and shows it.
    pub fn leaderboard(&self) -> bool {
        self.scripts.leaderboard_shown()
    }

    /// The ProximityPrompts that show (Phase 44), for the viewport to draw.
    pub fn prompts(&self) -> Vec<axcenity_ui::PromptView> {
        self.scripts
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
        self.scripts.click_cursor(&self.world)
    }

    /// The world as the game has it now, for the panels and the viewport.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The game's world, for the Properties panel's edits during Play. The
    /// game's scripts see those changes at the next frame.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
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
        self.scripts.pause()
    }

    pub fn set_breakpoints(&mut self, breakpoints: &Breakpoints) {
        self.scripts.set_breakpoints(breakpoints);
    }

    /// Continues from the pause: the paused script runs on from its line,
    /// then whatever waited for it. It may pause again at a breakpoint.
    pub fn continue_from_pause(&mut self, output: &mut Output) -> Result<(), Crash> {
        guard(|| {
            self.scripts.continue_from_pause(&mut self.world);
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
            self.scripts.activate_ui_button(&mut self.world, button);
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
                        self.scripts
                            .fire_ui_event(&mut self.world, id, "MouseEnter");
                    }
                    axcenity_ui::UiEvent::MouseLeave(id) => {
                        self.scripts
                            .fire_ui_event(&mut self.world, id, "MouseLeave");
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
            outcome = axcenity_ui::edit_text_box(&mut self.world, text_box, command);
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
            self.scripts.release_text_box(&mut self.world, text_box);
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
            result = self.scripts.run_command(&mut self.world, source);
            self.forward_output(output);
        })?;
        Ok(result)
    }

    /// The follow camera of a third-person controller, if the play world has
    /// one with a body. The fly camera is left alone; Stop shows it again.
    #[cfg(test)]
    pub fn character_camera(&self) -> Option<axcenity_character::CharacterCamera> {
        self.characters.view(&self.world)
    }

    /// Whether the game holds the view (a character controller, or a
    /// CameraType other than Custom), so the fly camera stays put.
    pub fn holds_camera(&self) -> bool {
        self.characters.view(&self.world).is_some() || self.scripts.camera_scripted()
    }

    /// The camera the viewport draws during Play, given the fly camera:
    /// a character controller's or the fly camera, unless scripts hold it,
    /// with the scripts' field of view and shake. The fly camera itself is
    /// never moved by it, so Stop shows it where it was.
    pub fn shown_camera(&self, fly: CameraState) -> CameraView {
        self.game_camera.view(self.script_camera(fly))
    }

    /// The viewport's size in pixels (`Camera.ViewportSize`), before a frame.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.scripts.set_viewport_size(width, height);
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
        let mut moved = None;
        self.cost = None;
        if self.scripts.pause().is_some() {
            return Ok(None);
        }
        // Each step's time, for the Profiler. Reading the clock costs next
        // to nothing, so it's measured whether or not the panel is open.
        let mut cost = FrameCost::default();
        let started = Instant::now();
        let result = guard(|| {
            if self.panic_next_frame {
                panic!("a deliberate engine panic, from Debug > Panic during Play");
            }
            self.characters
                .step(&mut self.world, &self.assets, motion, seconds as f32);
            let seats: Vec<_> = self
                .characters
                .take_seat_changes()
                .into_iter()
                .map(|change| (change.controller, change.seated, change.seat))
                .collect();
            if !seats.is_empty() {
                self.scripts.fire_seated(&mut self.world, &seats);
            }
            let landings = self.characters.take_landings();
            if !landings.is_empty() {
                self.scripts.fire_landed(&mut self.world, &landings);
            }
            // Then Humanoids: MoveTo, states and dying (Phase 46).
            self.scripts
                .step_humanoids(&mut self.world, &self.characters, seconds);
            // Then players: their first character, respawns and spawn
            // ForceFields (Phase 47).
            self.scripts.step_players(&mut self.world, seconds);
            // Then navigation agents, toward their destinations.
            self.agents
                .step(&mut self.world, &self.assets, seconds as f32);
            for warning in self.agents.take_warnings() {
                output.warning(warning);
            }
            for sensed in self.agents.take_sensed() {
                let sense = sensed.sense.as_str().to_owned();
                self.scripts.fire_instance_event(
                    &mut self.world,
                    sensed.agent,
                    "PlayerSensed",
                    &[sense],
                );
            }
            let mut clock = Instant::now();
            // The time since `clock`, which then starts again.
            let mut lap = || {
                let now = Instant::now();
                let took = now - clock;
                clock = now;
                took
            };
            // 1. Input, for LocalScripts: the camera and the input. A
            //    third-person controller owns the camera when one is present.
            let seen = self.script_camera(camera);
            self.scripts.set_camera(seen);
            self.scripts.input_events(&mut self.world, input);
            // Then ProximityPrompts and ClickDetectors, by that input,
            // then Tools: pickups, the hotbar, Drop and Primary.
            self.scripts.step_prompts(&mut self.world, seconds);
            self.scripts.step_tools(&mut self.world);
            self.forward_output(output);
            if self.scripts.pause().is_some() {
                cost.scripts += lap();
                return;
            }
            // 2. Resume scripts whose wait is over (and due `task.delay`s),
            //    after delivering RemoteEvent messages.
            self.scripts.step(&mut self.world, seconds);
            // `RunService.Stepped`, before physics.
            if self.scripts.pause().is_none() {
                self.scripts.stepped(&mut self.world, seconds);
            }
            self.forward_output(output);
            cost.scripts += lap();
            if self.scripts.pause().is_some() {
                return;
            }
            // The character is not a dynamic body. Anchoring here, after
            // scripts and before physics, keeps a script from dropping it
            // for this step. Gravity and friction of other parts are unchanged.
            self.characters.keep_kinematic(&mut self.world);
            self.agents.keep_kinematic(&mut self.world);
            // 3. Step physics at a fixed 60 Hz (0 to N steps this frame),
            //    after the parts scripts shattered are replaced.
            self.scripts
                .apply_shatters(&mut self.world, &mut self.physics);
            cost.physics_steps = self.physics.step(&mut self.world, seconds);
            cost.physics += lap();
            // 4. Fire `Touched` for parts that started touching (and
            //    `Changed` for parts that moved), `TouchEnded` for those
            //    that stopped, then `RunService.Heartbeat`.
            let touches = self.physics.take_touches();
            self.scripts.fire_touched(&mut self.world, &touches);
            let ends = self.physics.take_touch_ends();
            self.scripts.fire_touch_ended(&mut self.world, &ends);
            self.scripts.fire_breaks(&mut self.world, &mut self.physics);
            if self.scripts.pause().is_none() {
                self.scripts.step_health(&mut self.world, seconds);
                self.scripts.heartbeat(&mut self.world, seconds);
            }
            self.forward_output(output);
            let ready = self.scripts.take_loading_screen_removed();
            if let Some(loading) = &mut self.loading_gui {
                loading.advance(&mut self.world, seconds as f32, ready);
            }
            // Before the frame is drawn: functions bound with
            // `BindToRenderStep`, then `RenderStepped`, so a camera they
            // move is this frame's.
            if self.scripts.pause().is_none() {
                self.scripts.render_step(&mut self.world, seconds);
                self.forward_output(output);
            }
            moved = self.scripts.take_camera_move();
            if let Some(graphics) = self.scripts.take_graphics_change() {
                self.graphics = Some(graphics);
            }
            cost.scripts += lap();
            if self.scripts.pause().is_some() {
                return;
            }
            let listener = self.shown_camera(moved.unwrap_or(camera));
            let listener = CameraState {
                position: listener.position,
                orientation: listener.orientation,
            };
            if self.characters.view(&self.world).is_some() {
                // The controller owns the camera, so a LocalScript's move
                // is not copied onto the fly camera.
                moved = None;
            }
            self.apply_audio(listener, output);
            cost.audio += lap();
            // Animators move on and pose their models, as in the player.
            let events = self
                .animators
                .update(&mut self.world, &self.assets, seconds);
            for event in events {
                let (animator, name, args) = event.script_event();
                self.scripts
                    .fire_instance_event(&mut self.world, animator, name, &args);
            }
            // Tools in hand follow the posed hands.
            self.scripts
                .place_tools(&mut self.world, self.animators.poses());
            self.forward_output(output);
            self.forward_animator_errors(output);
            cost.animation += lap();
            // Then a Follow or Track camera follows its subject, which
            // physics and animation have moved, and shakes move on.
            self.game_camera = self.scripts.step_camera(&self.world, seconds);
        });
        cost.total = started.elapsed();
        self.cost = Some(cost);
        result?;
        Ok(moved)
    }

    /// The camera scripts should see: the follow camera when a controller
    /// owns it, otherwise `fallback` (the fly camera, or a LocalScript's move).
    fn script_camera(&self, fallback: CameraState) -> CameraState {
        match self.characters.view(&self.world) {
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
        self.scripts.input_map()
    }

    /// `UserInputService.MouseBehavior` and `MouseIconEnabled`.
    pub fn mouse(&self) -> (MouseBehavior, bool) {
        self.scripts.mouse()
    }

    /// What the last frame's steps took, or `None` if it didn't run.
    pub fn cost(&self) -> Option<FrameCost> {
        self.cost
    }

    /// How much the game has to simulate, for the Profiler.
    pub fn load(&self) -> GameLoad {
        GameLoad {
            bodies: self.physics.body_count(),
            sounds_playing: self.audio.playing_count(),
            lua_bytes: self.scripts.used_memory(),
        }
    }

    /// The posed models of the animated MeshParts, for the viewport.
    pub fn poses(&self) -> &axcenity_scene::Poses {
        self.animators.poses()
    }

    /// The path each NavigationAgent is walking, from its feet, for the
    /// Agent Paths overlay. Reading them changes nothing.
    pub fn agent_paths(&self) -> Vec<(InstanceId, Vec<Vector3>)> {
        self.agents.paths()
    }

    /// Recent `workspace:Raycast` calls, for the editor overlay.
    pub fn debug_rays(&self) -> Vec<axcenity_script::DebugRay> {
        self.scripts.debug_rays()
    }

    /// The asset file at `path` changed on disk during Play: Animators
    /// using it as a clip, state machine or model pick it up at the next
    /// frame, in the state they're in, and a Sound uses it the next time it
    /// plays. A sound that can't be decoded keeps its last good version.
    /// True if a playing Animator or a Sound that has played uses it.
    pub fn asset_changed(&mut self, path: &str, output: &mut Output) -> bool {
        let animated = self.animators.reload(path);
        let heard = match self.audio.reload(path, &self.assets) {
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
            let reloaded = self
                .scripts
                .reload_script(&mut self.world, script, path, source);
            self.forward_output(output);
            match reloaded {
                Ok(()) => output.info(format!("Reloaded {name}")),
                Err(_) if !self.world.contains(script) => output.info(format!(
                    "{name} isn't in the game any more, so it wasn't reloaded."
                )),
                Err(_) => output.warning(format!(
                    "{name} still runs its last good version until the error is fixed."
                )),
            }
        })
    }

    fn forward_animator_errors(&mut self, output: &mut Output) {
        for error in self.animators.take_errors() {
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
        self.scripts.take_particle_emits()
    }

    /// Moves what scripts printed, and their errors, into the Output panel.
    fn forward_output(&mut self, output: &mut Output) {
        for line in self.scripts.take_output() {
            match line.kind {
                OutputKind::Print => output.info(line.text),
                OutputKind::Warning => output.warning(line.text),
                OutputKind::Error => output.script_error(line.text, line.trace),
            }
        }
    }

    fn apply_audio(&mut self, camera: CameraState, output: &mut Output) {
        let mut actions = self.scripts.take_sound_actions();
        actions.append(&mut self.ui_sounds);
        let listener = axcenity_audio::Listener::from_view(camera.position, camera.orientation);
        let update = self
            .audio
            .update(actions, &self.world, &self.assets, listener);
        for err in update.errors {
            output.error(format!("Audio: {err}"));
        }
        if !update.ended.is_empty() {
            self.scripts
                .fire_sounds_ended(&mut self.world, &update.ended);
            self.forward_output(output);
        }
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

#[cfg(test)]
mod tests;
