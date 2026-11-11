//! The test world: a copy of a game's level with both sides running, and
//! no window or audio. Frames run the test runner's steps of the shared
//! schedule (`axcenity_sim`, `docs/architecture/frame-loop.md`) at a fixed
//! 60 a second; there is nothing to draw.

use std::path::PathBuf;

use axcenity_assets::Assets;
use axcenity_character::MoveInput;
use axcenity_core::bench::Laps;
use axcenity_core::input::{Input, InputEvent, InputKind, InputMap, InputState};
use axcenity_core::{ClassName, InstanceId, KeyCode, UserInputType, Value, World};
use axcenity_project::ScriptFile;
use axcenity_script::{CameraState, OutputKind, OutputLine, TestReport, TestRequest};
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};

/// Each frame is this long, so a run is the same on every computer.
pub const FRAME_SECONDS: f64 = 1.0 / 60.0;
/// The view's size LocalScripts see (`Camera.ViewportSize`).
const VIEW_SIZE: (f32, f32) = (1280.0, 720.0);

/// One level of a game, as a test sees it.
pub struct TestGame {
    pub sim: Sim,
    /// The test runner's frame.
    schedule: Schedule,
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
    #[cfg(debug_assertions)]
    frames_since_index_check: u32,
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
            sim: Sim::new(setup.world, axcenity_sim::script_runtime(), assets),
            schedule: Schedule::for_host(Hosts::TEST),
            input: InputState::default(),
            events: Vec::new(),
            output: Vec::new(),
            problem: None,
            seconds: 0.0,
            laps: Laps::start(),
            #[cfg(debug_assertions)]
            frames_since_index_check: 0,
        };
        if let Err(err) =
            axcenity_project::load_starter_character(setup.folder, &mut game.sim.world)
        {
            game.output.push(format!("warning: {err}"));
        }
        game.sim.scripts.set_assets(game.sim.assets.clone());
        game.sim.scripts.set_save_folder(Some(setup.save_folder));
        game.sim.scripts.set_input_map(setup.input_map.clone());
        let (start, current, worlds) = setup.levels;
        // Scripts inside objects in levels LevelService loads later, and in
        // the StarterCharacter each player spawns as (G2).
        let all = worlds.values().chain([&game.sim.world]);
        for (path, source) in axcenity_project::level_code(setup.folder, all) {
            game.sim.scripts.know_code(&path, &source);
        }
        game.sim.scripts.set_levels(start, current, worlds);
        if setup.start_scripts {
            game.sim
                .animators
                .update(&mut game.sim.world, &game.sim.assets, 0.0);
            for script in setup.scripts {
                game.sim.scripts.start_script(
                    &mut game.sim.world,
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
        self.sim
            .scripts
            .start_spec(&mut self.sim.world, script, runner, path, source, selected);
        self.collect_output();
    }

    /// The Script the spec runs as, named after its file.
    fn spec_script(&mut self, path: &str) -> InstanceId {
        let parent = self.sim.world.service(ClassName::ServerScriptService);
        let script = self
            .sim
            .world
            .create(ClassName::Script)
            .expect("a Script isn't a service");
        let name = path.rsplit('/').next().unwrap_or(path);
        let _ = self.sim.world.set_name(script, name);
        let _ = self.sim.world.set_parent(script, parent);
        script
    }

    /// Runs one frame: first what the spec asked for at the last one, then
    /// the frame order's steps.
    pub fn frame(&mut self) {
        profiling::scope!("frame");
        self.laps = Laps::start();
        let seconds = FRAME_SECONDS;
        for request in self.sim.scripts.take_test_requests() {
            self.apply(request);
        }
        self.seconds += seconds;

        // Each step's place and what it does: `axcenity_sim`. Taken out for
        // the frame so the steps can have the whole game.
        let schedule = std::mem::take(&mut self.schedule);
        let mut laps = std::mem::take(&mut self.laps);
        schedule.run(&mut FrameCtx::new(seconds, Hosts::TEST, self), &mut laps);
        self.schedule = schedule;
        self.laps = laps;
        self.collect_output();
        self.laps.lap("RenderStep");
        // In debug builds, every game test also checks the class and tag
        // index against the tree (it walks everything, so not every frame).
        #[cfg(debug_assertions)]
        {
            self.frames_since_index_check += 1;
            if self.frames_since_index_check == 60 {
                self.frames_since_index_check = 0;
                self.sim.world.check_index().expect("index out of date");
            }
        }
        profiling::finish_frame!();
    }

    /// What the spec told the host since the last call.
    pub fn take_reports(&mut self) -> Vec<TestReport> {
        self.sim.scripts.take_test_reports()
    }

    /// The camera LocalScripts see: the character's when one has a body.
    fn camera(&self) -> CameraState {
        match self.sim.characters.view(&self.sim.world) {
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
                match input_kind(&self.sim.scripts.input_map(), &input, down) {
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
                self.sim
                    .scripts
                    .activate_ui_button(&mut self.sim.world, button);
            }
            TestRequest::Type { text_box, text } => {
                let typed = self
                    .sim
                    .world
                    .get(text_box)
                    .is_some_and(|instance| instance.class() == ClassName::TextBox);
                if typed {
                    let _ = self
                        .sim
                        .world
                        .set_property(text_box, "Text", Value::String(text));
                    self.sim
                        .scripts
                        .release_text_box(&mut self.sim.world, text_box);
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
        let lines = self.sim.scripts.take_output();
        self.output.extend(lines.iter().map(output_text));
    }
}

impl HostIo for TestGame {
    fn sim(&mut self) -> &mut Sim {
        &mut self.sim
    }

    /// The input map's actions in what the test holds down, or none while a
    /// dialogue runs or a Sequence locks input.
    fn character_motion(&mut self, seconds: f32) -> MoveInput {
        let map = self.sim.scripts.input_map();
        if self.sim.scripts.movement_locked(&self.sim.world) {
            MoveInput::default()
        } else {
            MoveInput::from_actions(&map, &self.input, [0.0, 0.0], 0.0, seconds)
        }
    }

    fn agent_warning(&mut self, warning: String) {
        self.output.push(format!("warning: {warning}"));
    }

    fn frame_input(&mut self, _seconds: f64) -> Vec<InputEvent> {
        self.sim.scripts.set_viewport_size(VIEW_SIZE.0, VIEW_SIZE.1);
        let camera = self.camera();
        self.sim.scripts.set_camera(camera);
        let events = std::mem::take(&mut self.events);
        self.input.end_frame();
        events
    }

    fn animator_error(&mut self, error: String) {
        self.output.push(format!("error: {error}"));
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
