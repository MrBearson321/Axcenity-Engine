//! The player's input: clicks the UI takes, the input map driving the
//! built-in controllers (from `input.json` and from a script's rebind), and
//! a game controller walking the character with no script.

use std::path::{Path, PathBuf};

use axcenity_core::input::{Input, InputEvent, InputKind, InputMap};
use axcenity_core::{ClassName, KeyCode, Value};
use axcenity_ui::PadNavigation;

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

/// A copy of example `name`, with `input.json` written from `map` when
/// given, opened as the player opens a game: the input map, then the scripts.
pub(super) fn open(name: &str, map: Option<&InputMap>) -> (App, PathBuf) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-input-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&example, &folder);
    if let Some(map) = map {
        axcenity_project::save_input_map(&folder, map).unwrap();
    }
    let project = axcenity_project::load(&folder).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let mut world = project.world;
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_input_map(project.input_map.clone());
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let stylesheets = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let app = App::new(
        world,
        scripts,
        stylesheets,
        Assets::new(&folder),
        Animators::new(),
        camera,
    );
    (app, folder)
}

impl App {
    /// One frame's input and character step, as the frame loop runs them,
    /// then scripts, the kinematic hold and physics.
    fn input_frame(&mut self) {
        self.input.map = self.sim.scripts.input_map();
        let motion = self.input.motion_input([0.0, 0.0], 0.0, FRAME);
        self.sim
            .characters
            .step(&mut self.sim.world, &self.sim.assets, motion, FRAME);
        let events = self.input.take_events(self.pointer);
        self.sim.scripts.input_events(&mut self.sim.world, &events);
        self.sim.scripts.step(&mut self.sim.world, f64::from(FRAME));
        self.sim.characters.keep_kinematic(&mut self.sim.world);
        self.sim.physics.step(&mut self.sim.world, f64::from(FRAME));
        self.sim
            .scripts
            .heartbeat(&mut self.sim.world, f64::from(FRAME));
    }

    fn output(&mut self) -> Vec<String> {
        self.sim
            .scripts
            .take_output()
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    fn body_height(&self) -> f32 {
        let model = self
            .sim
            .world
            .find_first_child(self.sim.world.workspace(), "Character")
            .unwrap();
        let body = self.sim.world.find_first_child(model, "Body").unwrap();
        self.sim
            .world
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
            .y
    }

    /// Taps `key` and lets the character rise for a third of a second.
    /// True if it left the ground.
    fn jumps_on(&mut self, key: KeyCode) -> bool {
        for _ in 0..60 {
            self.input_frame();
        }
        let floor = self.body_height();
        self.push_key(key, true, false);
        self.input_frame();
        self.push_key(key, false, false);
        let mut highest = floor;
        for _ in 0..20 {
            self.input_frame();
            highest = highest.max(self.body_height());
        }
        highest > floor + 0.5
    }

    /// Swaps the character's ThirdPersonController for a FirstPersonController.
    fn first_person(&mut self) {
        let model = self
            .sim
            .world
            .find_first_child(self.sim.world.workspace(), "Character")
            .unwrap();
        let third = self
            .sim
            .world
            .find_first_child(model, "ThirdPersonController")
            .unwrap();
        self.sim.world.destroy(third).unwrap();
        let first = self
            .sim
            .world
            .create(ClassName::FirstPersonController)
            .unwrap();
        self.sim.world.set_parent(first, Some(model)).unwrap();
        self.sim.characters = Controllers::default();
    }
}

#[test]
fn a_local_script_gets_the_click_and_a_click_on_a_text_button_is_game_processed() {
    let (mut app, _) = open("hello-menu", None);
    let script = app.sim.world.create(ClassName::LocalScript).unwrap();
    app.sim
        .world
        .set_parent(script, Some(app.sim.world.starter_player_scripts()))
        .unwrap();
    app.sim.scripts.start_script(
        &mut app.sim.world,
        script,
        "clicks.client.lua",
        r#"
        game:GetService("UserInputService").InputBegan:Connect(function(input, processed)
            if input.UserInputType == Enum.UserInputType.MouseButton1 then
                print("click", input.Position.X, input.Position.Y, processed)
            end
        end)
        "#,
    );
    let size = (1280, 720);
    let layout = axcenity_ui::layout_ui_interactive(
        &app.sim.world,
        size.0,
        size.1,
        &app.stylesheets,
        UiInput::default(),
        None,
    );
    let play = app
        .sim
        .world
        .find_first_descendant(app.sim.world.workspace(), "Play")
        .unwrap();
    let rect = layout.element(play).unwrap().rect;
    let on_play = [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0];

    for pointer in [on_play, [5.0, 5.0]] {
        app.pointer = Some(pointer);
        app.ui_mouse_events.extend([true, false]);
        app.process_ui_mouse_events(size);
        app.input_frame();
    }
    let clicks: Vec<String> = app
        .output()
        .into_iter()
        .filter(|line| line.starts_with("click"))
        .collect();
    assert_eq!(
        clicks,
        [
            format!("click\t{}\t{}\ttrue", on_play[0], on_play[1]),
            "click\t5\t5\tfalse".to_owned(),
        ]
    );
}

#[test]
fn rebinding_jump_to_e_in_the_input_map_makes_both_controllers_jump_on_e() {
    let mut map = InputMap::default();
    map.rebind("Jump", vec![Input::Key(KeyCode::E)]).unwrap();
    for first_person in [false, true] {
        let (mut app, _) = open("hello-character", Some(&map));
        if first_person {
            app.first_person();
        }
        assert!(
            !app.jumps_on(KeyCode::Space),
            "first person: {first_person}"
        );
        assert!(app.jumps_on(KeyCode::E), "first person: {first_person}");
    }
}

#[test]
fn a_scripts_rebind_changes_what_the_controller_jumps_on_at_once() {
    let (mut app, _) = open("hello-character", None);
    assert!(app.jumps_on(KeyCode::Space));
    let script = app.sim.world.create(ClassName::LocalScript).unwrap();
    app.sim
        .world
        .set_parent(script, Some(app.sim.world.starter_player_scripts()))
        .unwrap();
    app.sim.scripts.start_script(
        &mut app.sim.world,
        script,
        "rebind.client.lua",
        "game:GetService('UserInputService'):RebindAction('Jump', {Enum.KeyCode.J})",
    );
    assert!(!app.jumps_on(KeyCode::Space));
    assert!(app.jumps_on(KeyCode::J));
}

#[test]
fn a_gamepads_left_stick_walks_the_character_and_its_a_button_jumps_with_no_script() {
    let (mut app, _) = open("hello-character", None);
    let mut pads = PadNavigation::default();
    // What `poll_pads` does with what the controller reports.
    let mut pad = |app: &mut App, kind: InputKind| {
        let mut events = [InputEvent::new(kind)];
        let map = app.input.map.clone();
        let keys = pads.update(&app.ui, &app.sim.world, &map, &mut events, FRAME);
        assert!(
            keys.is_empty(),
            "nothing in this game takes focus: {keys:?}"
        );
        app.input.push(events[0]);
    };
    for _ in 0..60 {
        app.input_frame();
    }
    let model = app
        .sim
        .world
        .find_first_child(app.sim.world.workspace(), "Character")
        .unwrap();
    let body = app.sim.world.find_first_child(model, "Body").unwrap();
    let at = |app: &App| {
        app.sim
            .world
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    };
    let start = at(&app);
    pad(&mut app, InputKind::PadConnected(0));
    pad(
        &mut app,
        InputKind::PadStick {
            pad: 0,
            stick: KeyCode::Thumbstick1,
            position: [0.0, 1.0],
        },
    );
    for _ in 0..30 {
        app.input_frame();
    }
    let walked = at(&app);
    let distance = (walked.x - start.x).hypot(walked.z - start.z);
    assert!(distance > 3.0, "{start:?} -> {walked:?}");
    pad(
        &mut app,
        InputKind::PadStick {
            pad: 0,
            stick: KeyCode::Thumbstick1,
            position: [0.0, 0.0],
        },
    );
    for _ in 0..30 {
        app.input_frame();
    }
    let floor = app.body_height();
    pad(
        &mut app,
        InputKind::PadButton {
            pad: 0,
            key: KeyCode::ButtonA,
            pressed: true,
        },
    );
    app.input_frame();
    pad(
        &mut app,
        InputKind::PadButton {
            pad: 0,
            key: KeyCode::ButtonA,
            pressed: false,
        },
    );
    let mut highest = floor;
    for _ in 0..20 {
        app.input_frame();
        highest = highest.max(app.body_height());
    }
    assert!(highest > floor + 0.5, "{floor} -> {highest}");
}
