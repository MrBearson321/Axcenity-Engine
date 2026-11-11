use std::path::PathBuf;
use std::time::Instant;

use axcenity_core::{ClassName, Value};

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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
    no_character(&mut world);
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
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
    // The Profiler's Memory tab (G7) agrees with the Game tab.
    let memory = session.memory();
    assert_eq!(memory.physics.count, load.bodies);
    assert_eq!(memory.lua, load.lua_bytes);
    assert!(memory.instances.total.count >= 3);
    assert_eq!(memory.sounds.count, 0, "no sound has played");
}

/// Play spawns no character in `world` (Phase 47).
fn no_character(world: &mut World) {
    world
        .set_property(world.players(), "CharacterAutoLoads", Value::Bool(false))
        .unwrap();
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
    // With a character, its controller would hold the camera.
    no_character(&mut world);
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
    )
    .unwrap();
    let space = KeyEvent {
        key: KeyCode::Space,
        pressed: true,
    };
    let moved = session
        .frame(
            1.0 / 60.0,
            &[space.into()],
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

/// `RenderStepped` sees the pose the Animators set this frame, as in the
/// player: Play used to run the render step before them, a frame behind
/// (roadmap M1).
#[test]
fn render_stepped_sees_this_frames_animated_pose() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-door");
    let mut world = axcenity_project::load(&folder).unwrap().world;
    no_character(&mut world);
    let instance = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(instance, Some(world.starter_player_scripts()))
        .unwrap();
    let scripts = [ScriptFile {
        instance,
        path: "scripts/pose.client.lua".to_string(),
        source: r#"
                local door = workspace.Door
                door.Animator:LoadAnimation(door.Open):Play()
                game:GetService("RunService").RenderStepped:Connect(function()
                    print(string.format("%.4f", door.Position.Y))
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
        Assets::new(&folder),
        CameraState::default(),
        &LoadingScreen::default(),
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
    )
    .unwrap();
    let door = session
        .world()
        .find_first_child(session.world().workspace(), "Door")
        .unwrap();
    for _ in 0..10 {
        session
            .frame(
                1.0 / 60.0,
                &[],
                MoveInput::default(),
                CameraState::default(),
                &mut output,
            )
            .unwrap();
        let y = session
            .world()
            .get(door)
            .and_then(|door| door.property("Position"))
            .and_then(Value::as_vector3)
            .unwrap()
            .y;
        assert!(y > 2.0, "the door opens");
        assert_eq!(
            output.lines().last().unwrap().text,
            format!("{:.4}", f64::from(y))
        );
    }
}

/// Particles and world streaming move on in Play's frame, as in the
/// player, whether or not a Scene tab draws them: Play used to leave both
/// to the viewport (roadmap M1).
#[test]
fn particles_and_streaming_move_on_in_the_frame_without_a_scene_tab() {
    let mut world = World::new();
    no_character(&mut world);
    world
        .set_property(world.workspace(), "StreamingEnabled", Value::Bool(true))
        .unwrap();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let emitter = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_parent(emitter, Some(part)).unwrap();
    world
        .set_property(emitter, "Rate", Value::Number(0.0))
        .unwrap();
    let scripts = [script(
        &mut world,
        "workspace.Part.ParticleEmitter:Emit(10)",
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
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
    )
    .unwrap();
    session
        .frame(
            1.0 / 60.0,
            &[],
            MoveInput::default(),
            CameraState::default(),
            &mut output,
        )
        .unwrap();
    // What a Scene tab would draw, had one been open.
    let mut particles = Particles::new();
    let mut streaming = Streaming::default();
    session.swap_effects(&mut particles, &mut streaming);
    assert_eq!(particles.count(), 10, "the script's Emit(10)");
    assert!(streaming.settings().is_some(), "the world streams");
}

/// Starts Play of `world` with `clients` clients.
fn play(world: &World, scripts: &[ScriptFile], clients: usize, output: &mut Output) -> Play {
    Play::start(
        world,
        scripts,
        &Breakpoints::new(),
        output,
        Assets::default(),
        CameraState::default(),
        &LoadingScreen::default(),
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
        PlaySettings {
            clients,
            ..PlaySettings::default()
        },
    )
    .unwrap()
}

fn said(output: &Output, text: &str) -> bool {
    output.lines().iter().any(|line| line.text == text)
}

#[test]
fn a_remote_event_fired_by_client_2_reaches_the_server_with_their_player_and_fire_all_clients_reaches_all_three()
 {
    let mut world = World::new();
    let shout = world.create(ClassName::RemoteEvent).unwrap();
    world.set_name(shout, "Shout").unwrap();
    world
        .set_parent(shout, Some(world.replicated_storage()))
        .unwrap();
    let server = script(
        &mut world,
        "local shout = game.ReplicatedStorage.Shout\n\
         shout.OnServerEvent:Connect(function(player, text)\n\
             print('server got', text, 'from', player.Name)\n\
             shout:FireAllClients('hello from ' .. player.Name)\n\
         end)",
    );
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    let client = ScriptFile {
        instance: local,
        path: "scripts/shout.client.lua".to_owned(),
        source: "local me = game.Players.LocalPlayer\n\
                 game.ReplicatedStorage.Shout.OnClientEvent:Connect(function(text)\n\
                     print(me.Name, 'heard', text)\n\
                 end)\n\
                 if me.Name == 'Player2' then\n\
                     task.wait(0.5)\n\
                     game.ReplicatedStorage.Shout:FireServer('hi')\n\
                 end"
        .to_owned(),
    };
    let mut output = Output::default();
    let mut play = play(&world, &[server, client], 3, &mut output);
    assert_eq!(play.clients(), 3);
    for _ in 0..90 {
        play.frame(
            1.0 / 60.0,
            &[],
            MoveInput::default(),
            CameraState::default(),
            &mut output,
        )
        .unwrap();
    }
    let lines: Vec<&str> = output.lines().iter().map(|l| l.text.as_str()).collect();
    assert!(said(&output, "server got\thi\tfrom\tPlayer2"), "{lines:#?}");
    assert!(
        said(&output, "Player\theard\thello from Player2"),
        "{lines:#?}"
    );
    assert!(
        said(&output, "Player2\theard\thello from Player2 (Player2)"),
        "{lines:#?}"
    );
    assert!(
        said(&output, "Player3\theard\thello from Player2 (Player3)"),
        "{lines:#?}"
    );
    // Each client has its own world, and the viewport shows one at a time.
    let names: Vec<String> = play.views().into_iter().map(|(_, name)| name).collect();
    assert_eq!(names, ["Server", "Player", "Player2", "Player3"]);
    play.set_view(PlayView::Client(2));
    assert!(!std::ptr::eq(play.world(), play.server().world()));
}

#[test]
fn with_10_percent_packet_loss_play_stays_usable_and_the_network_section_shows_the_loss() {
    let mut world = World::new();
    let ping = world.create(ClassName::RemoteEvent).unwrap();
    world.set_name(ping, "Ping").unwrap();
    world
        .set_parent(ping, Some(world.replicated_storage()))
        .unwrap();
    let crate_part = world.create(ClassName::Part).unwrap();
    world.set_name(crate_part, "Crate").unwrap();
    world
        .set_property(crate_part, "Anchored", Value::Bool(true))
        .unwrap();
    world
        .set_parent(crate_part, Some(world.workspace()))
        .unwrap();
    let server = script(
        &mut world,
        "local count = 0\n\
         game.ReplicatedStorage.Ping.OnServerEvent:Connect(function(player, n)\n\
             count += 1\n\
             if n == 20 then print('pings', count) end\n\
         end)\n\
         local t = 0\n\
         game:GetService('RunService').Heartbeat:Connect(function(dt)\n\
             t += dt\n\
             workspace.Crate.Position = Vector3.new(t, 1, 0)\n\
         end)",
    );
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    let client = ScriptFile {
        instance: local,
        path: "scripts/ping.client.lua".to_owned(),
        source: "if game.Players.LocalPlayer.Name == 'Player2' then\n\
                     for n = 1, 20 do\n\
                         task.wait(0.1)\n\
                         game.ReplicatedStorage.Ping:FireServer(n)\n\
                     end\n\
                 end"
        .to_owned(),
    };
    let mut output = Output::default();
    let mut play = play(&world, &[server, client], 2, &mut output);
    play.emulate(axcenity_net::Conditions {
        loss: 0.1,
        ..axcenity_net::Conditions::default()
    });
    let mut profiler = crate::profiler::Profiler::default();
    profiler.start();
    for _ in 0..360 {
        play.frame(
            1.0 / 60.0,
            &[],
            MoveInput::default(),
            CameraState::default(),
            &mut output,
        )
        .unwrap();
    }
    // Every message arrives, resent where it was lost.
    let lines: Vec<&str> = output.lines().iter().map(|l| l.text.as_str()).collect();
    assert!(said(&output, "pings\t20"), "{lines:#?}");
    // The crate the server moves keeps moving on client 2.
    let x = |world: &World| {
        world
            .get(crate_part)
            .and_then(|c| c.property("Position"))
            .and_then(Value::as_vector3)
            .unwrap()
            .x
    };
    let client_two = play.device(1).unwrap();
    let behind = x(play.server().world()) - x(client_two.world());
    assert!((0.0..0.5).contains(&behind), "client 2 keeps up: {behind}");
    // The Profiler's Network section shows the loss.
    let stats = play.network_stats();
    assert_eq!(stats.len(), 1);
    let (name, stats, instances) = &stats[0];
    assert_eq!(name, "Player2");
    assert!(*instances > 0);
    assert!(
        (0.03..0.25).contains(&stats.link.packet_loss),
        "about a tenth lost: {}",
        stats.link.packet_loss
    );
}

/// What Play and a selection cost in a project named by
/// `AXCENITY_BENCH_PROJECT` (such as S10k, roadmap I1: `axcenity-test
/// --make-s10k <folder>`): the game frame's steps without drawing, over
/// `AXCENITY_BENCH_FRAMES` frames (300 unless set) after a warm-up, then
/// the selection box. Skipped when it isn't set.
/// `docs/development/benchmarks.md` has the other hosts.
///
/// `cargo test -p axcenity-editor --release -- --ignored bench_project_play --nocapture --test-threads 1`
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_project_play() {
    let Some(folder) = std::env::var_os("AXCENITY_BENCH_PROJECT") else {
        eprintln!("skipped bench_project_play: set AXCENITY_BENCH_PROJECT to a project folder");
        return;
    };
    let folder = PathBuf::from(folder);
    let world = axcenity_project::load(&folder).unwrap().world;
    let assets = Assets::new(&folder);
    let mut output = Output::default();
    let started = Instant::now();
    let mut session = Session::start(
        &world,
        &[],
        &Breakpoints::new(),
        &mut output,
        assets,
        CameraState::default(),
        &LoadingScreen::default(),
        &InputMap::default(),
        None,
        &GraphicsSettings::default(),
        &Capabilities::default(),
        PlayLevels::default(),
    )
    .unwrap();
    eprintln!("Session::start: {:?}", started.elapsed());
    let frames = std::env::var("AXCENITY_BENCH_FRAMES")
        .ok()
        .and_then(|frames| frames.parse().ok())
        .unwrap_or(300);
    let mut run = axcenity_core::bench::BenchRun::new(
        axcenity_core::bench::BenchRun::DEFAULT_WARM_UP,
        frames,
    );
    loop {
        session
            .frame(
                1.0 / 60.0,
                &[],
                MoveInput::default(),
                CameraState::default(),
                &mut output,
            )
            .unwrap();
        if run.record(session.laps()) {
            break;
        }
    }
    let scene = axcenity_core::bench::scene_name(&folder);
    print!("{}", run.finish_text("editor Play", &scene));

    let eye = axcenity_render::glam::Vec3::new(0.0, 40.0, 80.0);
    let parts: Vec<_> = world
        .descendants(world.workspace())
        .into_iter()
        .filter(|id| {
            world
                .get(*id)
                .is_some_and(|i| i.class() == ClassName::MeshPart)
        })
        .take(5)
        .collect();
    for id in parts {
        let started = Instant::now();
        let lines = axcenity_scene::Occluders::of(&world).visible_box_edges(id, eye);
        eprintln!(
            "visible_box_edges: {:?} ({} lines)",
            started.elapsed(),
            lines.map_or(0, |l| l.len())
        );
    }
}
