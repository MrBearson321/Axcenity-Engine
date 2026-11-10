//! The test world: a copy of a game's level with both sides running, and
//! no window or audio. Frames run steps 1 to 4 of the frame order
//! (`docs/architecture/frame-loop.md`) at a fixed 60 a second, as the
//! player does, then the render step's scripts and the game camera; there
//! is nothing to draw.

use std::path::PathBuf;

use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::bench::Laps;
use axcenity_core::input::{Input, InputEvent, InputKind, InputMap, InputState};
use axcenity_core::{ClassName, InstanceId, KeyCode, UserInputType, Value, World};
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_script::{
    CameraState, OutputKind, OutputLine, ScriptRuntime, TestReport, TestRequest,
};

/// Each frame is this long, so a run is the same on every computer.
pub const FRAME_SECONDS: f64 = 1.0 / 60.0;
/// The view's size LocalScripts see (`Camera.ViewportSize`).
const VIEW_SIZE: (f32, f32) = (1280.0, 720.0);

/// One level of a game, as a test sees it.
pub struct TestGame {
    pub world: World,
    pub scripts: ScriptRuntime,
    physics: Physics,
    characters: Controllers,
    agents: Agents,
    animators: Animators,
    assets: Assets,
    /// What the test holds down, for the controllers.
    input: InputState,
    /// Input since the last frame, for LocalScripts.
    events: Vec<InputEvent>,
    /// What scripts printed, and their errors, as text.
    pub output: Vec<String>,
    /// The first thing the spec asked for that can't be done, which fails it.
    pub problem: Option<String>,
    /// Game time since the test started.
    pub seconds: f64,
    /// The last frame's steps' times, for benchmarks (roadmap I1).
    pub laps: Laps,
}

/// What [`TestGame::start`] needs of a project's level.
pub struct GameSetup<'a> {
    pub folder: &'a std::path::Path,
    pub world: World,
    pub scripts: &'a [ScriptFile],
    pub input_map: &'a InputMap,
    /// `LevelService`'s levels: the start level's name, the current one's,
    /// and every level.
    pub levels: (&'a str, &'a str, std::collections::BTreeMap<String, World>),
    /// Where `SaveService` writes, never the project.
    pub save_folder: PathBuf,
    /// False only collects a spec's tests: the game's scripts don't start.
    pub start_scripts: bool,
}

impl TestGame {
    /// The game as the player starts it: the starter character, Animators
    /// in their first states, then every script in the project's order.
    pub fn start(setup: GameSetup) -> Self {
        let assets = Assets::new(setup.folder);
        let mut game = TestGame {
            world: setup.world,
            scripts: ScriptRuntime::new(),
            physics: Physics::with_assets(assets.clone()),
            characters: Controllers::default(),
            agents: Agents::default(),
            animators: Animators::with_ground(axcenity_character::foot_ground),
            assets,
            input: InputState::default(),
            events: Vec::new(),
            output: Vec::new(),
            problem: None,
            seconds: 0.0,
            laps: Laps::start(),
        };
        if let Err(err) = axcenity_project::load_starter_character(setup.folder, &mut game.world) {
            game.output.push(format!("warning: {err}"));
        }
        game.scripts.set_assets(game.assets.clone());
        game.scripts.set_save_folder(Some(setup.save_folder));
        game.scripts.set_input_map(setup.input_map.clone());
        let (start, current, worlds) = setup.levels;
        game.scripts.set_levels(start, current, worlds);
        if setup.start_scripts {
            game.animators.update(&mut game.world, &game.assets, 0.0);
            for script in setup.scripts {
                game.scripts.start_script(
                    &mut game.world,
                    script.instance,
                    &script.path,
                    &script.source,
                );
            }
        }
        game.collect_output();
        game
    }

    /// Starts the spec `path` (`source`) with the runner's Lua, as a Script
    /// of its own in ServerScriptService. `selected` is the test to run,
    /// from 1, or `None` to only collect them.
    pub fn start_spec(&mut self, runner: &str, path: &str, source: &str, selected: Option<usize>) {
        let script = self.spec_script(path);
        self.scripts
            .start_spec(&mut self.world, script, runner, path, source, selected);
        self.collect_output();
    }

    /// The Script the spec runs as, named after its file.
    fn spec_script(&mut self, path: &str) -> InstanceId {
        let parent = self.world.service(ClassName::ServerScriptService);
        let script = self
            .world
            .create(ClassName::Script)
            .expect("a Script isn't a service");
        let name = path.rsplit('/').next().unwrap_or(path);
        let _ = self.world.set_name(script, name);
        let _ = self.world.set_parent(script, parent);
        script
    }

    /// Runs one frame: first what the spec asked for at the last one, then
    /// the frame order's steps.
    pub fn frame(&mut self) {
        profiling::scope!("frame");
        self.laps = Laps::start();
        let seconds = FRAME_SECONDS;
        for request in self.scripts.take_test_requests() {
            self.apply(request);
        }
        self.seconds += seconds;

        // 1. Input: the controllers, then Humanoids, players, agents, then
        //    the input for LocalScripts, ProximityPrompts and Tools.
        let map = self.scripts.input_map();
        let motion = if self.scripts.movement_locked(&self.world) {
            MoveInput::default()
        } else {
            MoveInput::from_actions(&map, &self.input, [0.0, 0.0], 0.0, seconds as f32)
        };
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
        self.scripts
            .step_humanoids(&mut self.world, &self.characters, seconds);
        self.scripts.step_players(&mut self.world, seconds);
        self.laps.lap("Characters");
        self.agents
            .step(&mut self.world, &self.assets, seconds as f32);
        for warning in self.agents.take_warnings() {
            self.output.push(format!("warning: {warning}"));
        }
        self.laps.lap("Agents");
        for sensed in self.agents.take_sensed() {
            let sense = sensed.sense.as_str().to_owned();
            self.scripts.fire_instance_event(
                &mut self.world,
                sensed.agent,
                "PlayerSensed",
                &[sense],
            );
        }
        self.scripts.set_viewport_size(VIEW_SIZE.0, VIEW_SIZE.1);
        self.scripts.set_camera(self.camera());
        let events = std::mem::take(&mut self.events);
        self.input.end_frame();
        self.scripts.input_events(&mut self.world, &events);
        self.scripts.step_prompts(&mut self.world, seconds);
        self.scripts.step_tools(&mut self.world);
        self.scripts.step_abilities(&mut self.world, seconds);
        self.laps.lap("Input");

        // 2. Tweens, waiting scripts (the spec too), then `Stepped`.
        self.scripts.step(&mut self.world, seconds);
        self.scripts.stepped(&mut self.world, seconds);
        self.laps.lap("Scripts");

        // 3. Physics, with the characters and agents held kinematic.
        self.characters.keep_kinematic(&mut self.world);
        self.agents.keep_kinematic(&mut self.world);
        self.scripts
            .apply_shatters(&mut self.world, &mut self.physics);
        self.physics.step(&mut self.world, seconds);
        self.laps.lap("Physics");

        // 4. Touches, breaks, health, `Heartbeat`, then Animators and Tools.
        //    There's no audio: sounds asked for are let go.
        let touches = self.physics.take_touches();
        self.scripts.fire_touched(&mut self.world, &touches);
        let ends = self.physics.take_touch_ends();
        self.scripts.fire_touch_ended(&mut self.world, &ends);
        self.scripts.fire_breaks(&mut self.world, &mut self.physics);
        self.scripts.step_health(&mut self.world, seconds);
        self.scripts.heartbeat(&mut self.world, seconds);
        self.laps.lap("Heartbeat");
        self.scripts.take_sound_actions();
        self.scripts.take_particle_emits();
        let events = self
            .animators
            .update(&mut self.world, &self.assets, seconds);
        for event in events {
            let (animator, name, args) = event.script_event();
            self.scripts
                .fire_instance_event(&mut self.world, animator, name, &args);
        }
        for error in self.animators.take_errors() {
            self.output.push(format!("error: {error}"));
        }
        self.scripts
            .place_tools(&mut self.world, self.animators.poses());
        self.scripts.take_level_loads();
        self.scripts.take_loading_screen_removed();
        self.laps.lap("Animation");

        // Before a frame would be drawn: the render step, then the camera.
        self.scripts.render_step(&mut self.world, seconds);
        self.scripts.take_camera_move();
        self.scripts.step_camera(&self.world, seconds);
        self.collect_output();
        self.laps.lap("RenderStep");
        profiling::finish_frame!();
    }

    /// What the spec told the host since the last call.
    pub fn take_reports(&mut self) -> Vec<TestReport> {
        self.scripts.take_test_reports()
    }

    /// The camera LocalScripts see: the character's when one has a body.
    fn camera(&self) -> CameraState {
        match self.characters.view(&self.world) {
            Some(view) => CameraState {
                position: view.position,
                orientation: view.orientation,
            },
            None => CameraState {
                position: axcenity_core::Vector3::new(0.0, 10.0, 20.0),
                orientation: axcenity_core::Vector3::new(0.0, 0.0, 0.0),
            },
        }
    }

    fn apply(&mut self, request: TestRequest) {
        match request {
            TestRequest::Press { input, down } => {
                match input_kind(&self.scripts.input_map(), &input, down) {
                    Some(kind) => {
                        let event = InputEvent::new(kind);
                        self.input.follow(&event);
                        self.events.push(event);
                    }
                    None => self.fail(format!(
                        "`{input}` is not a key, a mouse button or an input action"
                    )),
                }
            }
            TestRequest::Click(button) => {
                self.scripts.activate_ui_button(&mut self.world, button);
            }
            TestRequest::Type { text_box, text } => {
                let typed = self
                    .world
                    .get(text_box)
                    .is_some_and(|instance| instance.class() == ClassName::TextBox);
                if typed {
                    let _ = self
                        .world
                        .set_property(text_box, "Text", Value::String(text));
                    self.scripts.release_text_box(&mut self.world, text_box);
                } else {
                    self.fail("Test.type needs a TextBox".to_owned());
                }
            }
        }
    }

    fn fail(&mut self, problem: String) {
        self.problem.get_or_insert(problem);
    }

    fn collect_output(&mut self) {
        let lines = self.scripts.take_output();
        self.output.extend(lines.iter().map(output_text));
    }
}

/// How the console shows a line: errors and warnings say so, errors with
/// their trace.
pub fn output_text(line: &OutputLine) -> String {
    match line.kind {
        OutputKind::Print => line.text.clone(),
        OutputKind::Warning => format!("warning: {}", line.text),
        OutputKind::Error => {
            let mut text = format!("error: {}", line.text);
            for frame in &line.trace {
                text.push_str("\n    ");
                text.push_str(frame);
            }
            text
        }
    }
}

/// The input `name` names, pressed or let go: a key (`W`), a controller
/// button (`ButtonA`), a mouse button (`MouseButton1`), or an action of
/// `map` (`Jump`), which presses its first input.
fn input_kind(map: &InputMap, name: &str, pressed: bool) -> Option<InputKind> {
    let key = |key: KeyCode| {
        if key.is_gamepad() {
            InputKind::PadButton {
                pad: 0,
                key,
                pressed,
            }
        } else {
            InputKind::Key { key, pressed }
        }
    };
    let mouse = |button: UserInputType| InputKind::MouseButton {
        button,
        pressed,
        position: [VIEW_SIZE.0 / 2.0, VIEW_SIZE.1 / 2.0],
    };
    if let Some(code) = KeyCode::from_name(name) {
        return Some(key(code));
    }
    if let Some(button) = UserInputType::from_name(name)
        && button.mouse_button().is_some()
    {
        return Some(mouse(button));
    }
    match map.action(name)?.inputs.first()? {
        Input::Key(code) => Some(key(*code)),
        Input::Keys(codes) => Some(key(codes[0])),
        Input::Mouse(button) => Some(mouse(*button)),
        Input::MouseMovement => None,
    }
}
