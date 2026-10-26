use axcenity_core::ClassName;

use super::*;

fn script(world: &mut World, source: &str) -> ScriptFile {
    let instance = world.create(ClassName::Script).unwrap();
    world
        .set_parent(instance, Some(world.server_script_service()))
        .unwrap();
    ScriptFile {
        instance,
        path: "scripts/test.server.lua".to_string(),
        source: source.to_string(),
    }
}

#[test]
fn the_projects_loading_screen_covers_play_until_a_local_script_is_ready() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(gui, "Loading").unwrap();
    world
        .set_property(gui, "Enabled", axcenity_core::Value::Bool(false))
        .unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let ready = world.create(ClassName::LocalScript).unwrap();
    let scripts = [ScriptFile {
        instance: ready,
        path: "scripts/ready.client.lua".to_owned(),
        source: "task.wait(1)\ngame:GetService('ReplicatedFirst'):RemoveDefaultLoadingScreen()"
            .to_owned(),
    }];
    let loading = LoadingScreen {
        screen_gui: "Loading".to_owned(),
        min_seconds: 0.25,
        wait_for_script: true,
    };
    let mut output = Output::default();
    let mut session = Session::start(
        &world,
        &scripts,
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &loading,
    )
    .unwrap();
    let enabled = |session: &Session| {
        session.world().get(gui).unwrap().property("Enabled")
            == Some(&axcenity_core::Value::Bool(true))
    };
    assert_eq!(session.loading_screen(), Some(gui));
    assert!(enabled(&session), "shown though it's saved disabled");
    let mut frame = |session: &mut Session| {
        session
            .frame(
                1.0 / 60.0,
                &[],
                MoveInput::default(),
                CameraState::default(),
                &mut output,
            )
            .unwrap();
    };
    for _ in 0..30 {
        frame(&mut session);
    }
    assert_eq!(
        session.loading_screen(),
        Some(gui),
        "the script isn't ready"
    );
    for _ in 0..40 {
        frame(&mut session);
    }
    assert_eq!(session.loading_screen(), None);
    assert!(!enabled(&session));
}

#[test]
fn scripts_change_only_the_copy() {
    let mut world = World::new();
    let scripts = [script(
        &mut world,
        "local p = Instance.new('Part') p.Name = 'Made' p.Parent = workspace print('hi')",
    )];
    let mut output = Output::default();
    let session = Session::start(
        &world,
        &scripts,
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();

    let made = session.world().find_first_child(world.workspace(), "Made");
    assert!(made.is_some());
    assert!(world.find_first_child(world.workspace(), "Made").is_none());
    assert_eq!(output.lines().last().unwrap().text, "hi");
}

#[test]
fn script_errors_go_to_output_and_the_game_keeps_running() {
    let mut world = World::new();
    let scripts = [script(&mut world, "task.wait(1)\nerror('boom')")];
    let mut output = Output::default();
    let mut session = Session::start(
        &world,
        &scripts,
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();
    for _ in 0..90 {
        session
            .frame(
                1.0 / 60.0,
                &[],
                MoveInput::default(),
                CameraState::default(),
                &mut output,
            )
            .unwrap();
    }
    let line = output.lines().last().unwrap();
    assert_eq!(line.kind, crate::output::OutputKind::Error);
    assert_eq!(line.text, "scripts/test.server.lua:2: boom");
}

#[test]
fn a_panic_in_a_frame_is_caught_with_its_message() {
    let mut output = Output::default();
    let mut session = Session::start(
        &World::new(),
        &[],
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();
    let camera = CameraState::default();
    session
        .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
        .unwrap();
    session.panic_next_frame();
    let crash = session
        .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
        .unwrap_err();
    assert!(crash.0.contains("deliberate engine panic"), "{crash:?}");
    // Formatted messages carry a `String`, which is read too.
    let crash = guard(|| panic!("{} went wrong", "this")).unwrap_err();
    assert_eq!(crash.0, "this went wrong");
}

#[test]
fn a_breakpoint_holds_the_whole_frame_until_continue() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    world.set_name(part, "Crate").unwrap();
    world
        .set_property(part, "Anchored", axcenity_core::Value::Bool(false))
        .unwrap();
    let scripts = [script(
        &mut world,
        "while true do
  task.wait()
  print('tick')
end",
    )];
    let breakpoints = Breakpoints::from([("scripts/test.server.lua".to_string(), [3].into())]);
    let mut output = Output::default();
    let mut session = Session::start(
        &world,
        &scripts,
        &breakpoints,
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();
    let camera = CameraState::default();
    let position = |session: &Session| {
        session
            .world()
            .get(part)
            .and_then(|part| part.property("Position"))
            .and_then(axcenity_core::Value::as_vector3)
            .unwrap()
    };
    let start = position(&session);
    session
        .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
        .unwrap();
    let pause = session.pause().expect("paused before line 3");
    assert_eq!(
        (pause.path.as_str(), pause.line),
        ("scripts/test.server.lua", 3)
    );
    let at_pause = position(&session);
    // Paused frames move nothing: no scripts, no physics.
    for _ in 0..30 {
        session
            .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
            .unwrap();
    }
    assert_eq!(position(&session), at_pause);
    assert!(!output.lines().iter().any(|line| line.text == "tick"));
    // A paused frame costs nothing, so the Profiler skips it.
    assert!(session.cost().is_none());

    session.continue_from_pause(&mut output).unwrap();
    assert_eq!(output.lines().last().unwrap().text, "tick");
    assert!(session.pause().is_none());
    // The next frame pauses on the same line again.
    session
        .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
        .unwrap();
    assert!(session.pause().is_some());
    // With the breakpoint cleared, Continue lets the game run on.
    session.set_breakpoints(&Breakpoints::new());
    session.continue_from_pause(&mut output).unwrap();
    for _ in 0..30 {
        session
            .frame(1.0 / 60.0, &[], MoveInput::default(), camera, &mut output)
            .unwrap();
    }
    assert!(session.pause().is_none());
    assert!(position(&session).y < start.y, "the unanchored part fell");
}

#[test]
fn each_frame_reports_its_steps_cost_and_the_load_for_the_profiler() {
    let mut world = World::new();
    for _ in 0..3 {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
    }
    let scripts = [script(
        &mut world,
        "local x = {} for i = 1, 1000 do x[i] = i end",
    )];
    let mut output = Output::default();
    let mut session = Session::start(
        &world,
        &scripts,
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();
    assert!(session.cost().is_none(), "no frame has run yet");
    session
        .frame(
            1.0 / 30.0,
            &[],
            MoveInput::default(),
            CameraState::default(),
            &mut output,
        )
        .unwrap();
    let cost = session.cost().expect("the frame ran");
    assert_eq!(cost.physics_steps, 2, "1/30 s is two 60 Hz steps");
    let steps = cost.scripts + cost.physics + cost.audio + cost.animation;
    assert!(steps <= cost.total, "{cost:?}");
    let load = session.load();
    assert_eq!(load.bodies, 3);
    assert_eq!(load.sounds_playing, 0);
    assert!(load.lua_bytes > 0);
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn game_keys_report_presses_releases_and_modifiers_once() {
    let mut keys = GameKeys::default();
    let none = egui::Modifiers::NONE;
    let pressed = keys.read(
        &[
            key(egui::Key::ArrowRight, true),
            // Held keys repeat; that isn't a new press.
            key(egui::Key::ArrowRight, true),
            key(egui::Key::F1, true),
        ],
        egui::Modifiers::SHIFT,
        true,
        false,
    );
    let event = |key, pressed| KeyEvent { key, pressed };
    assert_eq!(
        pressed,
        [event(KeyCode::LeftShift, true), event(KeyCode::Right, true)]
    );
    // Typing in a script tab lets go of everything.
    let typing = keys.read(&[key(egui::Key::A, true)], none, true, true);
    assert_eq!(
        typing,
        [
            event(KeyCode::Right, false),
            event(KeyCode::LeftShift, false)
        ]
    );
    assert_eq!(keys.read(&[], none, true, false), []);
}

#[test]
fn a_local_script_in_play_reads_keys_and_moves_the_camera() {
    let mut world = World::new();
    let instance = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(instance, Some(world.starter_player_scripts()))
        .unwrap();
    let scripts = [ScriptFile {
        instance,
        path: "scripts/keys.client.lua".to_string(),
        source: r#"
                game:GetService("UserInputService").InputBegan:Connect(function(input)
                    print(input.KeyCode.Name)
                    workspace.CurrentCamera.Position = Vector3.new(0, 50, 0)
                end)
            "#
        .to_string(),
    }];
    let mut output = Output::default();
    let mut session = Session::start(
        &world,
        &scripts,
        &Breakpoints::new(),
        &mut output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
    )
    .unwrap();
    let space = KeyEvent {
        key: KeyCode::Space,
        pressed: true,
    };
    let moved = session
        .frame(
            1.0 / 60.0,
            &[space],
            MoveInput::default(),
            CameraState::default(),
            &mut output,
        )
        .unwrap();
    assert_eq!(output.lines().last().unwrap().text, "Space");
    assert_eq!(
        moved.map(|camera| camera.position),
        Some(axcenity_core::Vector3::new(0.0, 50.0, 0.0))
    );
}
