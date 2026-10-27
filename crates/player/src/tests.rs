use std::path::Path;

use axcenity_core::{ClassName, Value, Vector3};

use super::*;

#[test]
fn arrow_and_letter_keys_reach_scripts_by_their_roblox_names() {
    use axcenity_core::KeyCode as K;
    assert_eq!(script_key(KeyCode::ArrowUp), Some(K::Up));
    assert_eq!(script_key(KeyCode::KeyW), Some(K::W));
    assert_eq!(script_key(KeyCode::Digit0), Some(K::Zero));
    assert_eq!(script_key(KeyCode::Enter), Some(K::Return));
    assert_eq!(script_key(KeyCode::F13), None);
    // Every key scripts know can be pressed in the player.
    for key in K::ALL {
        assert!(
            SCRIPT_KEYS.iter().any(|(_, script)| *script == key),
            "{key:?}"
        );
    }
}

#[test]
fn the_wheel_zooms_by_notches_and_a_touchpad_by_pixels() {
    use winit::dpi::PhysicalPosition;
    assert_eq!(wheel_notches(MouseScrollDelta::LineDelta(0.0, 1.0)), 1.0);
    assert_eq!(wheel_notches(MouseScrollDelta::LineDelta(0.0, -2.0)), -2.0);
    let pixels = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 20.0));
    assert_eq!(wheel_notches(pixels), 0.5);
}

#[test]
fn hello_remote_example_moves_the_block_while_an_arrow_key_is_held() {
    let (mut world, mut scripts) = start_example("hello-remote");
    let mover = world.find_first_child(world.workspace(), "Mover").unwrap();
    let start = position_of(&world, "Mover");
    let mut physics = Physics::new();
    let mut frame = |world: &mut World, keys: &[KeyEvent]| {
        scripts.input(world, keys);
        scripts.step(world, 1.0 / 60.0);
        physics.step(world, 1.0 / 60.0);
        let touches = physics.take_touches();
        scripts.fire_touched(world, &touches);
    };
    // Hold Right for one second: the LocalScript sends the direction
    // every frame, and the Script moves the block 12 studs a second.
    let right = axcenity_core::KeyCode::Right;
    frame(
        &mut world,
        &[KeyEvent {
            key: right,
            pressed: true,
        }],
    );
    for _ in 0..59 {
        frame(&mut world, &[]);
    }
    frame(
        &mut world,
        &[KeyEvent {
            key: right,
            pressed: false,
        }],
    );
    let moved = position_of(&world, "Mover");
    assert!(
        (moved.x - start.x - 12.0).abs() < 0.5,
        "{start:?} -> {moved:?}"
    );
    assert_eq!((moved.y, moved.z), (start.y, start.z));
    // Let go: it stays put.
    for _ in 0..30 {
        frame(&mut world, &[]);
    }
    assert_eq!(position_of(&world, "Mover"), moved);
    assert!(world.contains(mover));
    let errors: Vec<String> = scripts
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn hello_character_walks_runs_and_switches_from_lua_and_blocks() {
    use axcenity_core::KeyCode as K;
    let (mut world, mut scripts) = start_example("hello-character");
    let assets = Assets::default();
    let mut characters = Controllers::default();
    let mut physics = Physics::new();
    let model = world
        .find_first_child(world.workspace(), "Character")
        .unwrap();
    let controller = world
        .find_first_child(model, "ThirdPersonController")
        .unwrap();
    let body = world.find_first_child(model, "Body").unwrap();
    let movement = |world: &World| {
        world
            .get(controller)
            .unwrap()
            .property("Movement")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    let body_position = |world: &World| {
        world
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    };
    let walk = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    // The host's frame order: character, scripts, kinematic hold, physics.
    let mut frame = |world: &mut World, keys: &[KeyEvent], input: MoveInput| {
        characters.step(world, &assets, input, 1.0 / 60.0);
        scripts.input(world, keys);
        scripts.step(world, 1.0 / 60.0);
        characters.keep_kinematic(world);
        physics.step(world, 1.0 / 60.0);
    };
    let press = |key, pressed| [KeyEvent { key, pressed }];

    // Walk for one second.
    let start = body_position(&world);
    for _ in 0..60 {
        frame(&mut world, &[], walk);
    }
    let walked = start.z - body_position(&world).z;
    assert!((walked - 16.0).abs() < 0.5, "walked {walked}");
    assert!((body_position(&world).y - start.y).abs() < 1e-3);

    // The LocalScript runs while Shift is held.
    frame(&mut world, &press(K::LeftShift, true), MoveInput::default());
    assert_eq!(movement(&world), "Run");
    let start = body_position(&world);
    for _ in 0..60 {
        frame(&mut world, &[], walk);
    }
    let ran = start.z - body_position(&world).z;
    assert!((ran - 32.0).abs() < 0.5, "ran {ran}");
    frame(
        &mut world,
        &press(K::LeftShift, false),
        MoveInput::default(),
    );
    assert_eq!(movement(&world), "Walk");

    // The block script switches with 2 and 1.
    frame(&mut world, &press(K::Two, true), MoveInput::default());
    assert_eq!(movement(&world), "Run");
    frame(&mut world, &press(K::Two, false), MoveInput::default());
    frame(&mut world, &press(K::One, true), MoveInput::default());
    assert_eq!(movement(&world), "Walk");
    frame(&mut world, &press(K::One, false), MoveInput::default());

    // J jumps from the LocalScript, and 3 from the block script. Each jump
    // rises and lands back on the floor within a second.
    for key in [K::J, K::Three] {
        let start = body_position(&world).y;
        frame(&mut world, &press(key, true), MoveInput::default());
        frame(&mut world, &press(key, false), MoveInput::default());
        let mut peak = start;
        for _ in 0..60 {
            frame(&mut world, &[], MoveInput::default());
            peak = peak.max(body_position(&world).y);
        }
        assert!(peak - start > 6.0, "{key:?} rose {}", peak - start);
        assert!((body_position(&world).y - start).abs() < 1e-3);
    }

    // The body never fell as a physics body; the crate did, onto the floor.
    assert!((body_position(&world).y - 2.5).abs() < 1e-3);
    let crate_y = position_of(&world, "Crate").y;
    assert!(crate_y < 3.0, "crate at {crate_y}");
}

#[test]
fn hello_agent_walks_to_destinations_from_properties_lua_and_blocks() {
    use axcenity_core::KeyCode as K;

    /// The game, stepped in the player's frame order: characters, agents,
    /// scripts, kinematic holds, physics, Heartbeat.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        characters: Controllers,
        agents: Agents,
        physics: Physics,
        printed: Vec<String>,
        warnings: Vec<String>,
    }
    impl Game {
        fn frame(&mut self, keys: &[KeyEvent]) {
            let (world, assets, dt) = (&mut self.world, &Assets::default(), 1.0 / 60.0);
            self.characters
                .step(world, assets, MoveInput::default(), dt);
            self.agents.step(world, assets, dt);
            self.warnings.extend(self.agents.take_warnings());
            self.scripts.input(world, keys);
            self.scripts.step(world, f64::from(dt));
            self.characters.keep_kinematic(world);
            self.agents.keep_kinematic(world);
            self.physics.step(world, f64::from(dt));
            self.scripts.heartbeat(world, f64::from(dt));
            for line in self.scripts.take_output() {
                assert_eq!(line.kind, OutputKind::Print, "{}", line.text);
                self.printed.push(line.text);
            }
        }
        fn run(&mut self, seconds: f32) {
            for _ in 0..(seconds * 60.0).round() as usize {
                self.frame(&[]);
            }
        }
        /// Presses and lets go of `key`.
        fn tap(&mut self, key: K) {
            self.frame(&[KeyEvent { key, pressed: true }]);
            self.frame(&[KeyEvent {
                key,
                pressed: false,
            }]);
        }
    }

    let (world, scripts) = start_example("hello-agent");
    let guard = world.find_first_child(world.workspace(), "Guard").unwrap();
    let agent = world.find_first_child(guard, "NavigationAgent").unwrap();
    let body = world.find_first_child(guard, "Body").unwrap();
    let mut game = Game {
        world,
        scripts,
        characters: Controllers::default(),
        agents: Agents::default(),
        physics: Physics::new(),
        printed: Vec::new(),
        warnings: Vec::new(),
    };
    let feet = |game: &Game| axcenity_nav::feet_of(&game.world, &[body]).unwrap();
    let status = |game: &Game| {
        game.world
            .get(agent)
            .unwrap()
            .property("Status")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    let near = |a: Vector3, x: f32, z: f32| (a.x - x).hypot(a.z - z) < 1e-3;

    // The Destination set in Properties: the Goal, behind the wall. The
    // Guard goes round the wall's end and is never inside it.
    let mut furthest_z = f32::NEG_INFINITY;
    for _ in 0..6 * 60 {
        game.frame(&[]);
        let at = feet(&game);
        furthest_z = furthest_z.max(at.z);
        assert!(at.x.abs() > 1.5 || at.z > 12.0, "in the wall at {at:?}");
    }
    assert!(near(feet(&game), 16.0, 0.0), "{:?}", feet(&game));
    assert!(furthest_z > 12.0, "went round the end: {furthest_z}");
    assert_eq!(status(&game), "Reached");
    assert!(feet(&game).y.abs() < 1e-3, "on the floor, not fallen");

    // P, from Lua.
    game.tap(K::P);
    game.run(3.0);
    assert!(near(feet(&game), 16.0, -16.0), "{:?}", feet(&game));

    // 1, from blocks.
    game.tap(K::One);
    game.run(4.0);
    assert!(near(feet(&game), 16.0, 16.0), "{:?}", feet(&game));
    assert_eq!(status(&game), "Reached");

    // 2: the island across the gap. The Guard stays, and says why once.
    let before = feet(&game);
    game.tap(K::Two);
    game.run(1.0);
    assert_eq!(feet(&game), before);
    assert_eq!(status(&game), "Unreachable");
    assert_eq!(
        game.warnings,
        vec![
            "Workspace.Guard.NavigationAgent can't reach (0, 0, 50): no walkable route \
             leads there."
                .to_owned()
        ]
    );
    assert!(game.printed.iter().any(|line| line == "Guard: Unreachable"));

    // 3 and 4, from blocks: a Velocity moves it with no path, and zero stops it.
    game.tap(K::Three);
    game.run(1.0);
    game.tap(K::Four);
    let pushed = feet(&game);
    assert!(pushed.x - before.x > 7.5, "{before:?} -> {pushed:?}");
    assert_eq!(pushed.z, before.z);
    assert_eq!(game.agents.path(agent), None);
    game.run(0.5);
    assert_eq!(feet(&game), pushed, "it stays where it stopped");
    assert_eq!(status(&game), "Idle");

    // Held V, from Lua.
    game.frame(&[KeyEvent {
        key: K::V,
        pressed: true,
    }]);
    game.run(1.0);
    game.frame(&[KeyEvent {
        key: K::V,
        pressed: false,
    }]);
    let backed = feet(&game);
    assert!(pushed.z - backed.z > 7.5, "{pushed:?} -> {backed:?}");
    let anchored = game.world.get(body).unwrap().property("Anchored").cloned();
    assert_eq!(anchored, Some(Value::Bool(true)), "held kinematic");
}

#[test]
fn hello_behavior_guard_runs_its_tree_and_the_courier_walks_where_its_script_sends_it() {
    /// The game in the player's frame order, reading files from the project.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        characters: Controllers,
        agents: Agents,
        physics: Physics,
        assets: Assets,
        printed: Vec<String>,
    }
    impl Game {
        fn frame(&mut self) {
            let (world, assets, dt) = (&mut self.world, &self.assets, 1.0 / 60.0);
            self.characters
                .step(world, assets, MoveInput::default(), dt);
            self.agents.step(world, assets, dt);
            let warnings = self.agents.take_warnings();
            assert!(warnings.is_empty(), "{warnings:?}");
            fire_sensed(&mut self.scripts, world, &mut self.agents);
            self.scripts.input(world, &[]);
            self.scripts.step(world, f64::from(dt));
            self.characters.keep_kinematic(world);
            self.agents.keep_kinematic(world);
            self.physics.step(world, f64::from(dt));
            self.scripts.heartbeat(world, f64::from(dt));
            for line in self.scripts.take_output() {
                assert_eq!(line.kind, OutputKind::Print, "{}", line.text);
                self.printed.push(line.text);
            }
        }
        fn run(&mut self, seconds: f32) {
            for _ in 0..(seconds * 60.0).round() as usize {
                self.frame();
            }
        }
        fn feet(&self, body: InstanceId) -> Vector3 {
            axcenity_nav::feet_of(&self.world, &[body]).unwrap()
        }
    }
    fn across(a: Vector3, b: Vector3) -> f32 {
        (a.x - b.x).hypot(a.z - b.z)
    }

    let (world, scripts, folder) = start_example_in("hello-behavior");
    let find = |model: &str, child: &str| {
        let model = world.find_first_child(world.workspace(), model).unwrap();
        world.find_first_child(model, child).unwrap()
    };
    let (guard, guard_agent) = (find("Guard", "Body"), find("Guard", "NavigationAgent"));
    let (courier, courier_agent) = (find("Courier", "Body"), find("Courier", "NavigationAgent"));
    let player = find("Character", "Body");
    let mut game = Game {
        world,
        scripts,
        characters: Controllers::default(),
        agents: Agents::default(),
        physics: Physics::new(),
        assets: Assets::new(&folder),
        printed: Vec::new(),
    };

    // Out of sight, the Guard patrols to both ends, and the Courier (no
    // tree) walks to where its script sent it, and is sent back.
    let (mut east, mut west, mut courier_there) = (false, false, false);
    for _ in 0..10 * 60 {
        game.frame();
        let at = game.feet(guard);
        east |= across(at, Vector3::new(15.0, 0.0, -15.0)) < 0.5;
        west |= across(at, Vector3::new(-15.0, 0.0, -15.0)) < 0.5;
        courier_there |= across(game.feet(courier), Vector3::new(30.0, 0.0, 30.0)) < 0.5;
    }
    assert!(east && west, "the Guard patrols");
    assert!(
        courier_there,
        "the Courier walks to its script's Destination"
    );
    let back = game
        .world
        .get(courier_agent)
        .unwrap()
        .property("Destination")
        .cloned();
    assert_eq!(back, Some(Value::Vector3(Vector3::new(-30.0, 0.0, 30.0))));
    assert!(game.printed.is_empty(), "{:?}", game.printed);

    // In front of the patrol: once the Guard faces that way, it sees the
    // player and chases them down.
    let spot = Vector3::new(0.0, 0.0, -8.0);
    let position = Value::Vector3(Vector3::new(spot.x, 2.5, spot.z));
    game.world
        .set_property(player, "Position", position)
        .unwrap();
    let mut waited = 0;
    while game.world.get(guard_agent).unwrap().property("SeesPlayer") != Some(&Value::Bool(true)) {
        game.frame();
        waited += 1;
        assert!(waited < 8 * 60, "never seen");
    }
    game.run(3.0);
    assert!(across(game.feet(guard), game.feet(player)) <= 2.6);
    assert!(game.printed.contains(&"Guard: there you are!".to_owned()));

    // Gone: it loses them and goes back to its rounds.
    let far = Value::Vector3(Vector3::new(45.0, 2.5, 45.0));
    game.world.set_property(player, "Position", far).unwrap();
    game.run(0.3);
    let destination = game
        .world
        .get(guard_agent)
        .unwrap()
        .property("Destination")
        .cloned();
    let patrol = [
        Some(Value::Vector3(Vector3::new(-15.0, 0.0, -15.0))),
        Some(Value::Vector3(Vector3::new(15.0, 0.0, -15.0))),
    ];
    assert!(patrol.contains(&destination), "{destination:?}");
    assert!(
        game.printed
            .contains(&"Guard: lost them. Back to my rounds.".to_owned())
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn hello_perception_guard_sees_and_hears_the_player_from_lua_and_blocks() {
    use axcenity_core::KeyCode as K;

    /// The game in the player's frame order, with the character walking
    /// left while `left` is held.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        characters: Controllers,
        agents: Agents,
        physics: Physics,
        left: bool,
        printed: Vec<String>,
    }
    impl Game {
        fn frame(&mut self, keys: &[KeyEvent]) {
            let (world, assets, dt) = (&mut self.world, &Assets::default(), 1.0 / 60.0);
            let motion = MoveInput {
                left: self.left,
                ..MoveInput::default()
            };
            self.characters.step(world, assets, motion, dt);
            self.agents.step(world, assets, dt);
            fire_sensed(&mut self.scripts, world, &mut self.agents);
            self.scripts.input(world, keys);
            self.scripts.step(world, f64::from(dt));
            self.characters.keep_kinematic(world);
            self.agents.keep_kinematic(world);
            self.physics.step(world, f64::from(dt));
            self.scripts.heartbeat(world, f64::from(dt));
            for line in self.scripts.take_output() {
                assert_eq!(line.kind, OutputKind::Print, "{}", line.text);
                self.printed.push(line.text);
            }
        }
        fn run(&mut self, seconds: f32) {
            for _ in 0..(seconds * 60.0).round() as usize {
                self.frame(&[]);
            }
        }
        fn flag(&self, agent: InstanceId, name: &str) -> bool {
            let value = self.world.get(agent).unwrap().property(name).cloned();
            value == Some(Value::Bool(true))
        }
    }

    let (world, scripts) = start_example("hello-perception");
    let guard = world.find_first_child(world.workspace(), "Guard").unwrap();
    let agent = world.find_first_child(guard, "NavigationAgent").unwrap();
    let guard_body = world.find_first_child(guard, "Body").unwrap();
    let character = world
        .find_first_child(world.workspace(), "Character")
        .unwrap();
    let body = world.find_first_child(character, "Body").unwrap();
    let mut game = Game {
        world,
        scripts,
        characters: Controllers::default(),
        agents: Agents::default(),
        physics: Physics::new(),
        left: false,
        printed: Vec::new(),
    };
    let x = |game: &Game| axcenity_nav::feet_of(&game.world, &[body]).unwrap().x;

    // Hidden behind the wall, in range and in front of the Guard.
    game.run(0.5);
    assert!(!game.flag(agent, "SeesPlayer"));
    assert!(game.printed.is_empty(), "{:?}", game.printed);

    // Walking left, out from behind the wall: seen, once.
    game.left = true;
    let mut steps = 0;
    while !game.flag(agent, "SeesPlayer") {
        game.frame(&[]);
        steps += 1;
        assert!(steps < 120, "never seen, at X = {}", x(&game));
    }
    let seen_at = x(&game);
    assert!((-10.0..-5.0).contains(&seen_at), "seen at X = {seen_at}");
    game.run(0.25);
    game.left = false;
    game.run(0.25);
    assert_eq!(game.printed, vec!["Guard: I see you!"]);
    let red = game
        .world
        .get(guard_body)
        .unwrap()
        .property("Color")
        .cloned();
    assert_eq!(
        red,
        Some(Value::Color3(axcenity_core::Color3::new(0.85, 0.25, 0.25)))
    );

    // Back behind the wall, it loses sight of you.
    game.printed.clear();
    let behind = Value::Vector3(Vector3::new(0.0, 2.5, 20.0));
    game.world.set_property(body, "Position", behind).unwrap();
    game.run(0.25);
    assert!(!game.flag(agent, "SeesPlayer"));
    assert_eq!(game.printed, vec!["Guard: where did you go?"]);

    // F whistles: heard through the wall, from Lua and from blocks.
    game.printed.clear();
    game.frame(&[KeyEvent {
        key: K::F,
        pressed: true,
    }]);
    game.run(0.25);
    assert!(game.flag(agent, "HearsPlayer"));
    assert!(!game.flag(agent, "SeesPlayer"));
    game.printed.sort();
    assert_eq!(
        game.printed,
        vec!["Guard: I hear you!", "blocks: the Guard heard a whistle"]
    );
}

#[test]
fn hello_character_hovers_in_a_custom_mode_from_lua_and_blocks() {
    use axcenity_core::KeyCode as K;
    let (mut world, mut scripts) = start_example("hello-character");
    let assets = Assets::default();
    let mut characters = Controllers::default();
    let mut physics = Physics::new();
    let model = world
        .find_first_child(world.workspace(), "Character")
        .unwrap();
    let controller = world
        .find_first_child(model, "ThirdPersonController")
        .unwrap();
    let body = world.find_first_child(model, "Body").unwrap();
    let movement = |world: &World| {
        world
            .get(controller)
            .unwrap()
            .property("Movement")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    let body_position = |world: &World| {
        world
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    };
    let walk = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    let back = MoveInput {
        back: true,
        ..MoveInput::default()
    };
    // The host's frame order: character, scripts, kinematic hold, physics, Heartbeat.
    let mut frame = |world: &mut World, keys: &[KeyEvent], input: MoveInput| {
        characters.step(world, &assets, input, 1.0 / 60.0);
        scripts.input(world, keys);
        scripts.step(world, 1.0 / 60.0);
        characters.keep_kinematic(world);
        physics.step(world, 1.0 / 60.0);
        scripts.heartbeat(world, 1.0 / 60.0);
    };
    let press = |key, pressed| [KeyEvent { key, pressed }];
    frame(&mut world, &[], MoveInput::default());
    let floor_y = body_position(&world).y;

    // H hovers from the LocalScript: walking does nothing, and nothing falls.
    frame(&mut world, &press(K::H, true), MoveInput::default());
    frame(&mut world, &press(K::H, false), MoveInput::default());
    assert_eq!(movement(&world), "Hover");
    let start = body_position(&world);
    for _ in 0..30 {
        frame(&mut world, &[], walk);
    }
    assert_eq!(body_position(&world), start, "walked while hovering");

    // The script moves it: E rises and W goes where the camera looks, 12
    // studs a second, not WalkSpeed's 16.
    frame(&mut world, &press(K::E, true), MoveInput::default());
    for _ in 0..59 {
        frame(&mut world, &[], MoveInput::default());
    }
    frame(&mut world, &press(K::E, false), MoveInput::default());
    let risen = body_position(&world);
    assert!(
        (risen.y - start.y - 12.0).abs() < 0.5,
        "{start:?} -> {risen:?}"
    );
    frame(&mut world, &press(K::W, true), walk);
    for _ in 0..59 {
        frame(&mut world, &[], walk);
    }
    frame(&mut world, &press(K::W, false), MoveInput::default());
    let flown = body_position(&world);
    assert!(
        (risen.z - flown.z - 12.0).abs() < 0.5,
        "{risen:?} -> {flown:?}"
    );

    // H again clears it: the character walks again, and falls and lands first.
    frame(&mut world, &press(K::H, true), MoveInput::default());
    frame(&mut world, &press(K::H, false), MoveInput::default());
    assert_eq!(movement(&world), "Walk");
    for _ in 0..60 {
        frame(&mut world, &[], MoveInput::default());
    }
    assert!((body_position(&world).y - floor_y).abs() < 1e-3);

    // The blocks: 2 runs, 6 hovers and prints the mode, 7 clears it, and
    // running comes back.
    frame(&mut world, &press(K::Two, true), MoveInput::default());
    frame(&mut world, &press(K::Two, false), MoveInput::default());
    frame(&mut world, &press(K::Six, true), MoveInput::default());
    frame(&mut world, &press(K::Six, false), MoveInput::default());
    assert_eq!(movement(&world), "Hover");
    let start = body_position(&world);
    for _ in 0..30 {
        frame(&mut world, &[], back);
    }
    assert_eq!(body_position(&world), start, "ran while hovering");
    frame(&mut world, &press(K::Seven, true), MoveInput::default());
    frame(&mut world, &press(K::Seven, false), MoveInput::default());
    assert_eq!(movement(&world), "Run");
    let start = body_position(&world);
    for _ in 0..60 {
        frame(&mut world, &[], back);
    }
    let ran = body_position(&world).z - start.z;
    assert!((ran - 32.0).abs() < 0.5, "ran {ran}");

    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect();
    assert!(
        output.iter().any(|line| line == "movement: Hover"),
        "{output:?}"
    );
    assert!(output.iter().any(|line| line == "hover cleared"));
    assert!(
        !output.iter().any(|line| line.starts_with("error")),
        "{output:?}"
    );
}

#[test]
fn hello_character_crouches_from_lua_and_blocks_and_under_the_low_ceiling() {
    use axcenity_core::KeyCode as K;
    let (mut world, mut scripts) = start_example("hello-character");
    let assets = Assets::default();
    let mut characters = Controllers::default();
    let mut physics = Physics::new();
    let model = world
        .find_first_child(world.workspace(), "Character")
        .unwrap();
    let body = world.find_first_child(model, "Body").unwrap();
    let property = |world: &World, name: &str| {
        world
            .get(body)
            .unwrap()
            .property(name)
            .and_then(Value::as_vector3)
            .unwrap()
    };
    let height = |world: &World| property(world, "Size").y;
    let walk = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    let mut frame = |world: &mut World, keys: &[KeyEvent], input: MoveInput| {
        characters.step(world, &assets, input, 1.0 / 60.0);
        scripts.input(world, keys);
        scripts.step(world, 1.0 / 60.0);
        characters.keep_kinematic(world);
        physics.step(world, 1.0 / 60.0);
    };
    let tap =
        |frame: &mut dyn FnMut(&mut World, &[KeyEvent], MoveInput), world: &mut World, key| {
            frame(
                world,
                &[KeyEvent { key, pressed: true }],
                MoveInput::default(),
            );
            frame(
                world,
                &[KeyEvent {
                    key,
                    pressed: false,
                }],
                MoveInput::default(),
            );
            // The controller crouches or stands on the step after the script.
            frame(world, &[], MoveInput::default());
        };

    // K toggles crouch from the LocalScript; 4 and 5 from the block script.
    assert_eq!(height(&world), 5.0);
    tap(&mut frame, &mut world, K::K);
    assert_eq!(height(&world), 3.0);
    tap(&mut frame, &mut world, K::K);
    assert_eq!(height(&world), 5.0);
    tap(&mut frame, &mut world, K::Four);
    assert_eq!(height(&world), 3.0);
    assert!((property(&world, "Position").y - 1.5).abs() < 1e-3);

    // Crouched, it walks at half speed.
    let start = property(&world, "Position").z;
    for _ in 0..60 {
        frame(&mut world, &[], walk);
    }
    let walked = start - property(&world, "Position").z;
    assert!((walked - 8.0).abs() < 0.5, "crouched walk {walked}");
    tap(&mut frame, &mut world, K::Five);
    assert_eq!(height(&world), 5.0);

    // Under LowCeiling, 5 is refused until the character is out the far side.
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(30.0, 2.5, 0.0)),
        )
        .unwrap();
    tap(&mut frame, &mut world, K::Four);
    for _ in 0..60 {
        frame(&mut world, &[], walk);
    }
    let under = property(&world, "Position").z;
    assert!(under < -6.0 && under > -14.0, "under at z = {under}");
    tap(&mut frame, &mut world, K::Five);
    assert_eq!(height(&world), 3.0);
    for _ in 0..90 {
        frame(&mut world, &[], walk);
    }
    assert_eq!(height(&world), 5.0);
    assert!(property(&world, "Position").z < -15.0);
    assert!((property(&world, "Position").y - 2.5).abs() < 1e-3);
}

#[test]
fn hello_character_crosses_the_grey_step_and_ramp_and_is_stopped_by_the_red_ones() {
    let walk = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    // Walks toward -Z for `frames` 60ths of a second from x, and returns
    // where the body's feet end up.
    let walk_from = |x: f32, frames: usize| {
        let (mut world, _) = start_example("hello-character");
        let assets = Assets::default();
        let model = world
            .find_first_child(world.workspace(), "Character")
            .unwrap();
        let body = world.find_first_child(model, "Body").unwrap();
        world
            .set_property(body, "Position", Value::Vector3(Vector3::new(x, 2.5, 0.0)))
            .unwrap();
        let mut characters = Controllers::default();
        for _ in 0..frames {
            characters.step(&mut world, &assets, walk, 1.0 / 60.0);
        }
        let center = world
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap();
        Vector3::new(center.x, center.y - 2.5, center.z)
    };
    let low_step = walk_from(12.0, 40);
    assert!((low_step.y - 0.8).abs() < 1e-3, "LowStep {low_step:?}");
    let ramp = walk_from(20.0, 60);
    assert!((ramp.y - 5.0).abs() < 1e-3, "Ramp to Platform {ramp:?}");
    let tall_step = walk_from(-12.0, 60);
    assert!(
        tall_step.y.abs() < 1e-3 && tall_step.z > -6.0,
        "TallStep {tall_step:?}"
    );
    let steep = walk_from(-20.0, 60);
    assert!(
        steep.y.abs() < 0.05 && steep.z > -4.0,
        "SteepRamp {steep:?}"
    );
}

#[test]
fn keys_are_recorded_and_released() {
    let mut keys = MoveKeys::default();
    set_move_key(&mut keys, KeyCode::KeyW, true);
    set_move_key(&mut keys, KeyCode::ShiftRight, true);
    set_move_key(&mut keys, KeyCode::KeyZ, true);
    assert!(keys.forward && keys.slow);
    set_move_key(&mut keys, KeyCode::KeyW, false);
    assert!(!keys.forward);
}

#[test]
fn hello_script_example_creates_a_part_and_moves_an_existing_one() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-script");
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let parts_before = world.descendants(world.workspace()).len();

    start_scripts(&mut ScriptRuntime::new(), &mut world, &project.scripts);

    let workspace = world.workspace();
    let block = world.find_first_child(workspace, "Block").unwrap();
    assert_eq!(
        world.get(block).unwrap().property("Position"),
        Some(&Value::Vector3(Vector3::new(-8.0, 3.0, 0.0)))
    );
    for i in 1..=5 {
        assert!(
            world
                .find_first_child(workspace, &format!("Ball{i}"))
                .is_some()
        );
    }
    // The new parts are drawn along with the ones from the world file.
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO).to_render_camera();
    let draw_list = axcenity_scene::build_draw_list(&world, &Assets::default(), camera);
    assert_eq!(draw_list.items.len(), parts_before + 5);
}

#[test]
fn hello_modules_example_builds_stairs_through_two_modules() {
    let (world, _) = start_example("hello-modules");
    let workspace = world.workspace();
    let colors: Vec<Option<&Value>> = (1..=8)
        .map(|n| {
            let step = world
                .find_first_child(workspace, &format!("Step{n}"))
                .unwrap();
            world.get(step).unwrap().property("Color")
        })
        .collect();
    // The Palette module, required by the Builder module, goes round
    // four colors.
    assert_eq!(colors[0], colors[4]);
    assert_ne!(colors[0], colors[1]);
    // The modules are in ReplicatedStorage, where both sides find them.
    let storage = world.replicated_storage();
    assert!(world.find_first_child(storage, "Palette").is_some());
    assert!(world.find_first_child(storage, "Builder").is_some());
}

/// Every example renders (shadows, lights, sky and post-processing came
/// after most of them, and mustn't break them): a second after its
/// scripts start, the frame isn't one flat color, and the middle isn't
/// blown out to white. With `AXCENITY_SNAPSHOTS` naming a folder, each is
/// saved there as `example-<name>.png` to look at. Skipped on machines
/// without a GPU.
#[test]
fn every_example_renders() {
    use axcenity_render::wgpu;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut names: Vec<String> = fs::read_dir(&examples)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().join("axcenity.json").exists())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(names.len() >= 10, "{names:?}");
    // One after another this was the slowest test, so a few threads each
    // take the next example, with its own world and renderer on the shared
    // device. `std::thread::scope` lets them borrow these locals, and fails
    // the test if any of them panics.
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some(name) = names.get(next.fetch_add(1, Ordering::Relaxed)) {
                    check_example_renders(name, &examples, &device, &queue, snapshots.as_deref());
                }
            });
        }
    });
}

/// Renders the example `name` for [`every_example_renders`] and checks the
/// frame, saving it in `snapshots` if given.
fn check_example_renders(
    name: &str,
    examples: &Path,
    device: &axcenity_render::wgpu::Device,
    queue: &axcenity_render::wgpu::Queue,
    snapshots: Option<&Path>,
) {
    use axcenity_render::TextureRenderer;
    let (width, height) = (480, 270);
    let folder = examples.join(name);
    let project = axcenity_project::load(&folder).unwrap();
    let stylesheets: Vec<_> = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let (mut world, mut scripts) = start_example(name);
    let (mut physics, mut clock) = (Physics::new(), 0);
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 1.0);
    let mut camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    if let Some(view) = scripts.take_camera_move() {
        camera.set_view(view.position, view.orientation);
    }
    let assets = Assets::new(&folder);
    // Models load the first time they're drawn, so draw twice.
    let mut renderer = TextureRenderer::new(device, queue, width, height);
    for _ in 0..2 {
        let draw_list = axcenity_scene::build_draw_list_with_ui(
            &world,
            &assets,
            camera.to_render_camera(),
            (width, height),
            &stylesheets,
        );
        renderer.render(&draw_list).unwrap();
    }
    let errors = assets.take_errors();
    assert!(errors.is_empty(), "{name}: {errors:?}");
    let pixels = renderer.read_pixels();
    if let Some(folder) = snapshots {
        let file = fs::File::create(folder.join(format!("example-{name}.png"))).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    // An example can have a reference screenshot, which it must still
    // match: of its top-left corner (where hello-css's HUD is), or of
    // the whole frame (hello-graphics, the graphics reference scene).
    if let Some(average) = difference_from(&folder.join("reference.png"), &pixels, width) {
        assert!(
            average < REFERENCE_TOLERANCE,
            "{name} differs from reference.png by {average:.1} per channel on average"
        );
    }
    let colors: BTreeSet<[u8; 3]> = pixels
        .chunks(4)
        .map(|pixel| [pixel[0] / 16, pixel[1] / 16, pixel[2] / 16])
        .collect();
    assert!(colors.len() > 8, "{name} renders almost one color");
    let center = ((height / 2 * width + width / 2) * 4) as usize;
    assert!(
        pixels[center..center + 3]
            .iter()
            .any(|&channel| channel < 250),
        "{name} is white in the middle"
    );
}

/// How far, on average per channel, an example's frame may be from its
/// reference image.
pub(super) const REFERENCE_TOLERANCE: f64 = 6.0;

/// How far `pixels` (RGBA rows `width` wide) are from the reference image
/// at `path`, on average per channel, over the reference's size (it may
/// cover only the top-left corner). `None` when there's no such image.
pub(super) fn difference_from(path: &Path, pixels: &[u8], width: u32) -> Option<f64> {
    let file = fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut reference = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut reference).unwrap();
    let channels = info.color_type.samples();
    let mut difference = 0u64;
    for y in 0..info.height {
        for x in 0..info.width {
            let ours = ((y * width + x) * 4) as usize;
            let theirs = ((y * info.width + x) as usize) * channels;
            for channel in 0..3 {
                difference +=
                    u64::from(pixels[ours + channel].abs_diff(reference[theirs + channel]));
            }
        }
    }
    Some(difference as f64 / f64::from(info.width * info.height * 3))
}

/// Writes RGBA `pixels` as a PNG.
pub(super) fn write_png(path: &Path, pixels: &[u8], (width, height): (u32, u32)) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

/// The example project in `examples/<name>`, with its scripts started.
/// A script that sets `Lighting.ToneMapping` changes the frame drawn after it.
#[test]
fn a_script_setting_filmic_tone_mapping_changes_the_next_frame() {
    use axcenity_render::{TextureRenderer, ToneMapping, wgpu};
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let (mut world, mut scripts, folder) = start_example_in("hello-graphics");
    let (mut physics, mut clock) = (Physics::new(), 0);
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.5);
    let assets = Assets::new(&folder);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let (width, height) = (160, 90);
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut frame = |world: &World| {
        let list = axcenity_scene::build_draw_list(world, &assets, camera.to_render_camera());
        // Models load the first time they're drawn, so draw twice.
        renderer.render(&list).unwrap();
        renderer.render(&list).unwrap();
        (list.post.tone_mapping, renderer.read_pixels())
    };
    let (before, classic) = frame(&world);
    assert_eq!(before, ToneMapping::Classic);

    let script = world.create(axcenity_core::ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    scripts.start_script(
        &mut world,
        script,
        "scripts/look.server.lua",
        "game.Lighting.ToneMapping = Enum.ToneMapping.Filmic",
    );
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.55);
    let (after, filmic) = frame(&world);
    assert_eq!(after, ToneMapping::Filmic);
    let changed = classic
        .chunks(4)
        .zip(filmic.chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    assert!(
        changed > classic.len() / 4 / 2,
        "only {changed} pixels changed"
    );
}

/// A GPU device for render tests, or `None` on machines without one.
pub(super) fn test_gpu() -> Option<(axcenity_render::wgpu::Device, axcenity_render::wgpu::Queue)> {
    use axcenity_render::wgpu;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    pollster::block_on(adapter.request_device(&Default::default())).ok()
}

/// A new, empty project folder for one test, with `stripes.png`: 64×64,
/// red on its left half and blue on its right.
fn stripes_project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-player-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    let pixels: Vec<u8> = (0..64 * 64)
        .flat_map(|i| {
            if i % 64 < 32 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    let file = fs::File::create(folder.join("stripes.png")).unwrap();
    let mut encoder = png::Encoder::new(file, 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    folder
}

/// `world` drawn 64×64 from `eye` toward the origin, lit only by a sun
/// behind the camera: the colors across the middle row, one `R` (red) or
/// `B` (blue) for each stretch of that color, and the row's brightest
/// channel.
fn stripes_seen(
    renderer: &mut axcenity_render::TextureRenderer,
    world: &World,
    assets: &Assets,
    eye: Vec3,
) -> (String, u8) {
    let camera = axcenity_render::Camera {
        position: eye,
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let mut list = axcenity_scene::build_draw_list(world, assets, camera);
    list.sun.direction = eye.normalize();
    list.shadows.enabled = false;
    list.sky = axcenity_render::Sky {
        top_color: [0.0; 3],
        horizon_color: [0.0; 3],
        ground_color: [0.0; 3],
        environment_intensity: 0.0,
    };
    list.ambient.intensity = 0.0;
    // Models and textures load the first time they're drawn.
    renderer.render(&list).unwrap();
    renderer.render(&list).unwrap();
    let errors = assets.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let pixels = renderer.read_pixels();
    let colors: Vec<char> = (0..64)
        .map(|x| {
            let [red, _, blue] = [0, 1, 2].map(|c| u16::from(pixels[(32 * 64 + x) * 4 + c]));
            if red > blue + 40 {
                'R'
            } else if blue > red + 40 {
                'B'
            } else {
                '.'
            }
        })
        .collect();
    // Stretches of at least two pixels: a texture repeating past an edge
    // blends in a pixel of its other side there.
    let mut runs = String::new();
    for stretch in colors.chunk_by(|a, b| a == b) {
        let color = stretch[0];
        if color != '.' && stretch.len() >= 2 && !runs.ends_with(color) {
            runs.push(color);
        }
    }
    let row = &pixels[32 * 64 * 4..33 * 64 * 4];
    let brightest = row
        .chunks(4)
        .flat_map(|pixel| pixel[..3].iter().copied())
        .max()
        .unwrap();
    (runs, brightest)
}

/// A glTF leaf: one square, 2 units across, facing +Z, with `stripes.png`
/// repeated twice across by `KHR_texture_transform`, on a double-sided
/// material.
fn write_leaf_gltf(folder: &Path) {
    let positions: [f32; 12] = [
        -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0,
    ];
    let normals: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let uvs: [f32; 8] = [0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
    let mut bytes: Vec<u8> = positions
        .iter()
        .chain(&normals)
        .chain(&uvs)
        .flat_map(|f| f.to_le_bytes())
        .collect();
    bytes.extend(indices.iter().flat_map(|i| i.to_le_bytes()));
    fs::write(folder.join("leaf.bin"), &bytes).unwrap();
    let gltf = r#"{
        "asset": { "version": "2.0" },
        "extensionsUsed": ["KHR_texture_transform"],
        "buffers": [ { "uri": "leaf.bin", "byteLength": 140 } ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 48, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 96, "byteLength": 32 },
            { "buffer": 0, "byteOffset": 128, "byteLength": 12 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
              "min": [-1, -1, 0], "max": [1, 1, 0] },
            { "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3" },
            { "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2" },
            { "bufferView": 3, "componentType": 5123, "count": 6, "type": "SCALAR" }
        ],
        "images": [ { "uri": "stripes.png" } ],
        "textures": [ { "source": 0 } ],
        "materials": [ {
            "name": "Leaf",
            "doubleSided": true,
            "pbrMetallicRoughness": {
                "baseColorTexture": { "index": 0,
                    "extensions": { "KHR_texture_transform": { "scale": [2, 1] } } },
                "metallicFactor": 0,
                "roughnessFactor": 1
            }
        } ],
        "meshes": [ { "primitives": [ {
            "attributes": { "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 },
            "indices": 3,
            "material": 0
        } ] } ],
        "nodes": [ { "mesh": 0 } ],
        "scenes": [ { "nodes": [0] } ],
        "scene": 0
    }"#;
    fs::write(folder.join("leaf.gltf"), gltf).unwrap();
}

/// A glTF leaf with a texture transform and a double-sided material, in
/// a MeshPart: its stripes repeat twice across, and its back is drawn
/// and lit like its front (mirrored, as a back is).
#[test]
fn a_gltf_leaf_draws_its_texture_transform_and_both_sides_lit() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let folder = stripes_project("leaf");
    write_leaf_gltf(&folder);
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let leaf = world.create(axcenity_core::ClassName::MeshPart).unwrap();
    world.set_parent(leaf, Some(world.workspace())).unwrap();
    world
        .set_property(leaf, "MeshId", Value::String("leaf.gltf".into()))
        .unwrap();
    world
        .set_property(leaf, "Size", Value::Vector3(Vector3::new(8.0, 8.0, 1.0)))
        .unwrap();
    let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, 64, 64);
    let (front, front_light) =
        stripes_seen(&mut renderer, &world, &assets, Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(front, "RBRB");
    let (back, back_light) =
        stripes_seen(&mut renderer, &world, &assets, Vec3::new(0.0, 0.0, -10.0));
    assert_eq!(back, "BRBR");
    assert!(front_light > 60, "the front is lit: {front_light}");
    assert!(
        front_light.abs_diff(back_light) <= 2,
        "front {front_light}, back {back_light}"
    );
}

/// A SurfaceAppearance's `Tiling` of 2 repeats its color map twice as
/// often across a Part's face, and an `Offset` of 0.5 shifts it by half.
#[test]
fn a_surface_appearances_tiling_repeats_its_map_and_its_offset_shifts_it() {
    use axcenity_core::{ClassName, Vector2};
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let folder = stripes_project("surface-tiling");
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    // One copy of the texture across its 8-stud face (TileSize 8), matte
    // so the sun's highlight doesn't wash its colors out.
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(8.0, 8.0, 1.0)))
        .unwrap();
    world
        .set_property(part, "Roughness", Value::Number(1.0))
        .unwrap();
    let look = world.create(ClassName::SurfaceAppearance).unwrap();
    world.set_parent(look, Some(part)).unwrap();
    world
        .set_property(look, "ColorMap", Value::String("stripes.png".into()))
        .unwrap();
    let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, 64, 64);
    let eye = Vec3::new(0.0, 0.0, 10.0);
    let mut seen = |world: &World| stripes_seen(&mut renderer, world, &assets, eye).0;
    assert_eq!(seen(&world), "RB");
    world
        .set_property(look, "Tiling", Value::Vector2(Vector2::new(2.0, 1.0)))
        .unwrap();
    assert_eq!(seen(&world), "RBRB");
    world
        .set_property(look, "Tiling", Value::Vector2(Vector2::new(1.0, 1.0)))
        .unwrap();
    world
        .set_property(look, "Offset", Value::Vector2(Vector2::new(0.5, 0.0)))
        .unwrap();
    assert_eq!(seen(&world), "BR");
}

/// A Part whose `MaterialVariant` is an instance of a base material file
/// (the base has `stripes.png`; the instance repeats it twice as often)
/// saves, reloads and exports with both files, and the exported game
/// draws it the same as the project.
#[test]
fn a_material_variant_saves_reloads_and_exports_with_its_material_files() {
    use axcenity_assets::{MapFile, MaterialFile, MaterialMaps, write_material_file};
    use axcenity_core::ClassName;
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let folder = stripes_project("material-export");
    fs::create_dir_all(folder.join("assets/textures")).unwrap();
    fs::rename(
        folder.join("stripes.png"),
        folder.join("assets/textures/stripes.png"),
    )
    .unwrap();
    fs::write(
        folder.join("axcenity.json"),
        format!(
            r#"{{"name":"Materials","format_version":{}}}"#,
            axcenity_project::FORMAT_VERSION
        ),
    )
    .unwrap();
    let base = "assets/materials/stripes.material.json";
    let twice = "assets/materials/stripes-twice.material.json";
    write_material_file(
        &folder.join(base),
        &MaterialFile {
            roughness: Some(1.0),
            maps: MaterialMaps {
                color: Some(MapFile::of("assets/textures/stripes.png")),
                ..MaterialMaps::default()
            },
            ..MaterialFile::default()
        },
    )
    .unwrap();
    write_material_file(
        &folder.join(twice),
        &MaterialFile {
            base: Some(base.into()),
            tiling: Some([2.0, 1.0]),
            ..MaterialFile::default()
        },
    )
    .unwrap();

    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Size", Value::Vector3(Vector3::new(8.0, 8.0, 1.0))),
        (
            "Color",
            Value::Color3(axcenity_core::Color3::new(1.0, 1.0, 1.0)),
        ),
        ("MaterialVariant", Value::String(twice.into())),
    ] {
        world.set_property(part, name, value).unwrap();
    }
    axcenity_project::save_world(&folder, &world).unwrap();

    let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, 64, 64);
    let eye = Vec3::new(0.0, 0.0, 10.0);
    let mut drawn = |folder: &Path| {
        let project = axcenity_project::load(folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let variant = project
            .world
            .descendants(project.world.workspace())
            .into_iter()
            .find_map(|id| {
                let instance = project.world.get(id)?;
                instance
                    .property("MaterialVariant")?
                    .as_str()
                    .map(str::to_owned)
            });
        assert_eq!(variant.as_deref(), Some(twice));
        let assets = Assets::new(folder);
        let runs = stripes_seen(&mut renderer, &project.world, &assets, eye).0;
        (runs, renderer.read_pixels())
    };
    let (runs, pixels) = drawn(&folder);
    assert_eq!(runs, "RBRB");

    let exported = folder.with_extension("exported");
    let _ = fs::remove_dir_all(&exported);
    let packed = axcenity_project::pack_game(&folder).unwrap();
    axcenity_project::unpack_game(&packed, &exported).unwrap();
    for file in [base, twice, "assets/textures/stripes.png"] {
        assert!(exported.join(file).is_file(), "{file} is exported");
    }
    let (exported_runs, exported_pixels) = drawn(&exported);
    assert_eq!(exported_runs, "RBRB");
    assert!(
        exported_pixels == pixels,
        "the exported game draws the same frame"
    );
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&exported);
}

fn start_example(name: &str) -> (World, ScriptRuntime) {
    let (world, scripts, _) = start_example_in(name);
    (world, scripts)
}

/// Like [`start_example`], with the folder of the copy it opened.
pub(super) fn start_example_in(name: &str) -> (World, ScriptRuntime, PathBuf) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    // A copy, since opening a project from an older format upgrades it.
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&example, &folder);
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);
    (world, scripts, folder)
}

/// Copies the project files in `from` to `to`, leaving out `.axcenity`.
fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        if entry.file_name() == ".axcenity" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Runs frames at 60 fps until the game clock reaches `until` seconds,
/// with scripts, physics, then `Touched` and `Heartbeat` each frame, as
/// the frame loop does.
pub(super) fn run_until(
    world: &mut World,
    scripts: &mut ScriptRuntime,
    physics: &mut Physics,
    clock: &mut u32,
    until: f64,
) {
    while f64::from(*clock) / 60.0 < until {
        scripts.step(world, 1.0 / 60.0);
        physics.step(world, 1.0 / 60.0);
        let touches = physics.take_touches();
        scripts.fire_touched(world, &touches);
        scripts.heartbeat(world, 1.0 / 60.0);
        *clock += 1;
    }
}

#[test]
fn hello_door_opens_through_load_animation_when_something_lands_on_the_pad() {
    let (mut world, mut scripts, folder) = start_example_in("hello-door");
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    let mut physics = Physics::new();
    let door = world.find_first_child(world.workspace(), "Door").unwrap();
    let pad = world.find_first_child(world.workspace(), "Pad").unwrap();
    let height = |world: &World| {
        world
            .get(door)
            .and_then(|d| d.property("Position"))
            .and_then(axcenity_core::Value::as_vector3)
            .unwrap()
            .y
    };
    // A crate dropped onto the Pad.
    let pad_at = world
        .get(pad)
        .and_then(|p| p.property("Position"))
        .and_then(axcenity_core::Value::as_vector3)
        .unwrap();
    let crate_part = world.create(axcenity_core::ClassName::Part).unwrap();
    for (name, value) in [
        ("Anchored", axcenity_core::Value::Bool(false)),
        (
            "Position",
            axcenity_core::Value::Vector3(axcenity_core::Vector3::new(
                pad_at.x,
                pad_at.y + 2.0,
                pad_at.z,
            )),
        ),
    ] {
        world.set_property(crate_part, name, value).unwrap();
    }
    world
        .set_parent(crate_part, Some(world.workspace()))
        .unwrap();
    assert_eq!(height(&world), 2.0);
    for _ in 0..(4 * 60) {
        scripts.step(&mut world, 1.0 / 60.0);
        physics.step(&mut world, 1.0 / 60.0);
        let touches = physics.take_touches();
        scripts.fire_touched(&mut world, &touches);
        scripts.heartbeat(&mut world, 1.0 / 60.0);
        for event in animators.update(&mut world, &assets, 1.0 / 60.0) {
            let (instance, name, args) = event.script_event();
            scripts.fire_instance_event(&mut world, instance, name, &args);
        }
    }
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Door event: Opened", "Door opened"]);
    assert_eq!(height(&world), 8.0, "the door stays open");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn hello_events_reacts_to_physics_heartbeat_new_children_and_script_changes() {
    let (mut world, mut scripts) = start_example("hello-events");
    let mut physics = Physics::new();
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 4.0);

    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect();
    // Physics moved the ball, which fired its Position signal.
    assert_eq!(
        output,
        [
            "the ball reached the floor",
            "Crate was added to the Workspace",
            "Lamp's Color changed",
        ]
    );
    // Heartbeat turned the spinner 90 degrees a second, every frame.
    let spinner = world
        .find_first_child(world.workspace(), "Spinner")
        .unwrap();
    let orientation = world
        .get(spinner)
        .unwrap()
        .property("Orientation")
        .and_then(Value::as_vector3)
        .unwrap();
    assert!((orientation.y - 360.0).abs() < 0.01, "{orientation:?}");
}

#[test]
fn hello_mesh_drops_parts_onto_an_imported_model_which_reports_touches() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh");
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let workspace = world.workspace();
    for child in world.get(workspace).unwrap().children().to_vec() {
        world.destroy(child).unwrap();
    }
    let floor = world.create(axcenity_core::ClassName::Part).unwrap();
    world.set_name(floor, "Floor").unwrap();
    world
        .set_property(
            floor,
            "Position",
            Value::Vector3(Vector3::new(0.0, -0.5, 0.0)),
        )
        .unwrap();
    world
        .set_property(floor, "Size", Value::Vector3(Vector3::new(40.0, 1.0, 40.0)))
        .unwrap();
    world
        .set_property(floor, "Anchored", Value::Bool(true))
        .unwrap();
    world.set_parent(floor, Some(workspace)).unwrap();

    let mut make_mesh = |name: &str, position: Vector3, anchored: bool| {
        let id = world.create(axcenity_core::ClassName::MeshPart).unwrap();
        world.set_name(id, name).unwrap();
        world
            .set_property(
                id,
                "MeshId",
                Value::String("assets/models/pyramid.gltf".to_owned()),
            )
            .unwrap();
        world
            .set_property(id, "Position", Value::Vector3(position))
            .unwrap();
        world
            .set_property(id, "Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0)))
            .unwrap();
        world
            .set_property(id, "Anchored", Value::Bool(anchored))
            .unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    };
    make_mesh("Pyramid", Vector3::new(0.0, 1.5, 0.0), true);
    make_mesh("FallingPyramid", Vector3::new(10.0, 8.0, 0.0), false);
    let ball = world.create(axcenity_core::ClassName::Part).unwrap();
    world.set_name(ball, "Ball").unwrap();
    world
        .set_property(
            ball,
            "Position",
            Value::Vector3(Vector3::new(0.0, 8.0, 0.0)),
        )
        .unwrap();
    world.set_parent(ball, Some(workspace)).unwrap();

    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let assets = Assets::new(folder);
    scripts.set_assets(assets.clone());
    let mut physics = Physics::with_assets(assets.clone());
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 5.0);

    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect();
    // (What the script printed as it started went to the terminal then.)
    assert!(
        output.contains(&"Ball touched the pyramid".to_owned()),
        "{output:?}"
    );
    assert!(
        !output.iter().any(|line| line.starts_with("error")),
        "{output:?}"
    );
    // The ball rests on the imported pyramid instead of falling through it.
    assert!(
        (3.5..4.5).contains(&position_of(&world, "Ball").y),
        "{:?}",
        position_of(&world, "Ball")
    );
    // An unanchored imported pyramid falls and lands on the floor.
    assert!(position_of(&world, "FallingPyramid").y < 3.0);
    assert!(assets.take_errors().is_empty());

    // Both pyramids are drawn as their model.
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO).to_render_camera();
    let meshes = axcenity_scene::build_draw_list(&world, &assets, camera)
        .items
        .iter()
        .filter(|item| matches!(item.shape, axcenity_render::Shape::Mesh(_)))
        .count();
    assert_eq!(meshes, 2);
}

fn position_of(world: &World, name: &str) -> Vector3 {
    let id = world.find_first_child(world.workspace(), name).unwrap();
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn hello_physics_parts_fall_collide_and_come_to_rest_on_the_floor() {
    let (mut world, mut scripts) = start_example("hello-physics");
    let mut physics = Physics::new();
    let mut clock = 0;
    let moving = [
        "Tower1",
        "Tower2",
        "Tower3",
        "Dropper",
        "RampBall",
        "GlassCylinder",
    ];
    let start: Vec<Vector3> = moving.iter().map(|n| position_of(&world, n)).collect();

    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
    let settled: Vec<Vector3> = moving.iter().map(|n| position_of(&world, n)).collect();
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.9);

    for (i, name) in moving.iter().enumerate() {
        let now = position_of(&world, name);
        // On the floor or on another part, not fallen through it.
        assert!(now.y > 0.4 && now.y < 8.0, "{name} is at {now:?}");
        let still = (now.x - settled[i].x).abs() < 0.01
            && (now.y - settled[i].y).abs() < 0.01
            && (now.z - settled[i].z).abs() < 0.01;
        assert!(still, "{name} still moving: {:?} -> {now:?}", settled[i]);
    }
    // Everything except the bottom of the tower (already on the floor) fell,
    // if only by the small gaps between the tower blocks.
    for (i, name) in moving.iter().enumerate().skip(1) {
        assert!(settled[i].y < start[i].y - 0.02, "{name} didn't fall");
    }
    // The ball rolled down the ramp, away from where it landed.
    assert!(
        position_of(&world, "RampBall").z < -4.0,
        "{:?}",
        position_of(&world, "RampBall")
    );
    assert_eq!(position_of(&world, "Floor"), Vector3::new(0.0, -0.5, 0.0));
}

#[test]
fn hello_physics_script_freezes_and_releases_the_dropper() {
    let (mut world, mut scripts) = start_example("hello-physics");
    let mut physics = Physics::new();
    let mut clock = 0;
    let mut at = |until| {
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, until);
        position_of(&world, "Dropper").y
    };

    assert_eq!(at(1.9), 14.0, "anchored at the start");
    let frozen = at(2.5);
    assert!(
        frozen < 13.0 && frozen > 8.0,
        "released, then frozen: {frozen}"
    );
    assert_eq!(at(3.5), frozen, "frozen in mid-air");
    assert!(at(5.0) < 8.0, "released again, it fell onto the tower");
}

#[test]
fn hello_physics_script_teleports_the_bouncy_ball_back_to_its_start() {
    let (mut world, mut scripts) = start_example("hello-physics");
    let mut physics = Physics::new();
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 4.9);
    assert!(position_of(&world, "BouncyBall").y < 2.0, "it fell");
    // The script runs at 5 s; one physics step of falling from rest follows.
    run_until(
        &mut world,
        &mut scripts,
        &mut physics,
        &mut clock,
        5.0 + 1.5 / 60.0,
    );
    let y = position_of(&world, "BouncyBall").y;
    assert!(y > 11.5 && y < 12.0, "{y}");
}

#[test]
fn hello_tasks_example_blinks_spins_and_adds_a_ball_over_time() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-tasks");
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);

    let workspace = world.workspace();
    let lamp = world.find_first_child(workspace, "Lamp").unwrap();
    let spinner = world.find_first_child(workspace, "Spinner").unwrap();
    let property = |world: &World, id, name| world.get(id).unwrap().property(name).cloned();

    // 3.5 seconds at 60 frames per second, the way the frame loop steps it.
    let mut color = property(&world, lamp, "Color");
    let mut color_changes = 0;
    let mut printed = Vec::new();
    for _ in 0..210 {
        scripts.step(&mut world, 1.0 / 60.0);
        printed.extend(scripts.take_output().into_iter().map(|line| line.text));
        if property(&world, lamp, "Color") != color {
            color = property(&world, lamp, "Color");
            color_changes += 1;
        }
    }

    assert_eq!(color_changes, 3);
    assert_eq!(
        printed,
        [
            "blink 1",
            "blink 2",
            "blink 3",
            "a ball appeared after 3 seconds"
        ]
    );
    assert!(world.find_first_child(workspace, "Ball").is_some());
    // 90 degrees a second for 3.5 seconds is 315 degrees.
    let Some(Value::Vector3(orientation)) = property(&world, spinner, "Orientation") else {
        panic!("the spinner has no Orientation");
    };
    assert!((orientation.y - 315.0).abs() < 0.5, "{orientation:?}");
}

/// Like `run_until`, but also collects what scripts printed.
fn run_and_collect(
    world: &mut World,
    scripts: &mut ScriptRuntime,
    physics: &mut Physics,
    clock: &mut u32,
    until: f64,
) -> Vec<String> {
    let mut printed = Vec::new();
    while f64::from(*clock) / 60.0 < until {
        run_until(world, scripts, physics, clock, f64::from(*clock + 1) / 60.0);
        printed.extend(
            scripts
                .take_output()
                .into_iter()
                .map(|line| match line.kind {
                    OutputKind::Print => line.text,
                    OutputKind::Error => format!("error: {}", line.text),
                    OutputKind::Warning => format!("warning: {}", line.text),
                }),
        );
    }
    printed
}

#[test]
fn hello_touched_demo_recolors_the_pad_and_prints_what_landed() {
    let (mut world, mut scripts) = start_example("hello-touched");
    let mut physics = Physics::new();
    let mut clock = 0;
    let pad = world.find_first_child(world.workspace(), "Pad").unwrap();
    let color = |world: &World| world.get(pad).unwrap().property("Color").cloned();
    let start_color = color(&world);

    // The red ball falls about 9 studs and lands within the first second.
    let first = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 2.9);
    assert!(!first.is_empty(), "the ball should have landed");
    assert!(
        first.iter().all(|line| line == "Ball touched the pad"),
        "{first:?}"
    );
    assert_ne!(color(&world), start_color, "the pad changed color");

    // At 3 s a blue ball drops; at 6 s the script disconnects.
    let second = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 6.0);
    let (last, landings) = second.split_last().unwrap();
    assert_eq!(last, "stopped listening for touches");
    assert!(!landings.is_empty(), "the blue ball should have landed");
    assert!(
        landings
            .iter()
            .all(|line| line == "BlueBall touched the pad"),
        "{second:?}"
    );

    // The last ball lands after the disconnect, so nothing more happens.
    let after = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
    assert_eq!(after, Vec::<String>::new());
    let ignored = position_of(&world, "IgnoredBall");
    assert!(ignored.y < 2.0, "the last ball landed: {ignored:?}");
}

#[test]
fn settled_hello_physics_parts_stop_reporting_touches() {
    let (mut world, mut scripts) = start_example("hello-physics");
    let mut physics = Physics::new();
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
    // Everything is at rest now, so 3 more seconds report no new touches.
    let mut touches = Vec::new();
    for _ in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
        touches.extend(physics.take_touches());
    }
    assert_eq!(touches, []);
}

#[test]
fn hello_tween_raises_the_block_and_slides_the_menu_in_then_fades_it_out() {
    use axcenity_core::UDim2;
    use axcenity_render::UiDrawItem;
    let (mut world, mut scripts) = start_example("hello-tween");
    let hud = world.find_first_child(world.workspace(), "Hud").unwrap();
    let menu = world.find_first_child(hud, "Menu").unwrap();
    let (width, height) = (1920, 1080);
    let menu_rect = |world: &World| {
        axcenity_ui::layout_ui(world, width, height, &[])
            .element(menu)
            .map(|element| element.rect)
    };
    // The menu's own background, the first rectangle drawn at its place.
    let menu_alpha = |world: &World| {
        let layout = axcenity_ui::layout_ui(world, width, height, &[]);
        let rect = layout.element(menu).unwrap().rect;
        layout
            .draw_items
            .iter()
            .find_map(|item| match item {
                UiDrawItem::Rectangle(r) if r.position == [rect.x, rect.y] => Some(r.color[3]),
                _ => None,
            })
            .unwrap()
    };
    // Plays sixty frames a second until `seconds` into the game.
    let mut frames = 0;
    let mut play_to = |world: &mut World, seconds: f64| {
        while f64::from(frames) / 60.0 < seconds {
            scripts.step(world, 1.0 / 60.0);
            frames += 1;
        }
    };

    // The "slide in from left" block put it off the left of the screen.
    let start = menu_rect(&world).unwrap();
    assert!(start.x + start.width <= 0.0, "{start:?}");
    play_to(&mut world, 0.75);
    assert_eq!(menu_rect(&world).unwrap().x, 60.0);
    assert_eq!(
        world.get(menu).unwrap().property("Position"),
        Some(&Value::UDim2(UDim2::new(0.0, 60.0, 0.0, 60.0)))
    );
    assert_eq!(menu_alpha(&world), 1.0);
    let rose_from = position_of(&world, "Block");

    // Tween blocks don't wait: after "wait 3", "fade out" fades it from
    // 3 s to 4 s, and "hide" takes it off the screen at 4 s.
    play_to(&mut world, 3.5);
    let fading = menu_alpha(&world);
    assert!(0.2 < fading && fading < 0.8, "{fading}");
    play_to(&mut world, 3.95);
    assert!(menu_alpha(&world) < 0.05);
    play_to(&mut world, 4.1);
    assert!(menu_rect(&world).is_none());
    assert_eq!(
        world.get(menu).unwrap().property("BackgroundTransparency"),
        Some(&Value::Number(1.0))
    );

    // The typed TweenService script raised the block, once.
    assert_eq!(position_of(&world, "Block"), Vector3::new(0.0, 5.0, 0.0));
    assert!(rose_from.y < 5.0);
    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(
        output,
        ["The block finished rising:\tEnum.PlaybackState.Completed"]
    );
}

#[test]
fn hello_rich_text_styles_tags_from_the_world_lua_and_blocks() {
    use axcenity_render::{UiDrawItem, UiText};
    let (mut world, mut scripts) = start_example("hello-rich-text");
    let hud = world.find_first_child(world.workspace(), "Hud").unwrap();
    let text_of = |world: &World, name: &str| -> UiText {
        let layout = axcenity_ui::layout_ui(world, 1280, 720, &[]);
        let rect = layout
            .element(world.find_first_child(hud, name).unwrap())
            .unwrap()
            .rect;
        layout
            .draw_items
            .into_iter()
            .find_map(|item| match item {
                UiDrawItem::Text(text) if text.position == [rect.x + 4.0, rect.y + 4.0] => {
                    Some(text)
                }
                _ => None,
            })
            .unwrap()
    };
    let spans = |text: &UiText| -> Vec<String> {
        text.spans.iter().map(|span| span.text.clone()).collect()
    };

    let prompt = text_of(&world, "Prompt");
    assert_eq!(prompt.text, "Press E to open");
    assert_eq!(spans(&prompt), ["Press ", "E", " to ", "open"]);
    assert_eq!(prompt.spans[1].weight, 700);
    assert_eq!(prompt.spans[3].color, [1.0, 0.0, 0.0, 1.0]);

    // A malformed string is drawn as it's written, with one warning.
    let broken = text_of(&world, "Broken");
    assert_eq!(broken.text, "<b>open");
    assert!(broken.spans.is_empty());
    let warnings = axcenity_ui::layout_ui(&world, 1280, 720, &[]).warnings;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("\"Broken\""), "{warnings:?}");

    // After a second, a LocalScript and a block script set rich text.
    for _ in 0..70 {
        scripts.step(&mut world, 1.0 / 60.0);
    }
    let status = text_of(&world, "Status");
    assert_eq!(status.text, "The door is open");
    assert_eq!(spans(&status), ["The door is ", "open"]);
    assert_eq!(status.spans[1].weight, 700);
    assert_eq!(
        status.spans[1].color,
        [
            0x55 as f32 / 255.0,
            0xdd as f32 / 255.0,
            0x55 as f32 / 255.0,
            1.0
        ]
    );
    let hint = text_of(&world, "Hint");
    assert_eq!(hint.text, "Set by blocks, in italics too");
    assert_eq!(hint.spans[1].weight, 700);
    assert!(hint.spans[3].italic);
    let errors: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn hello_buttons_hovers_presses_and_leaves_the_locked_button_alone() {
    use axcenity_render::{UiDrawItem, UiRectangle};
    use axcenity_ui::{Stylesheet, UiCursor, UiInput, UiInteraction, UiLayout};

    /// The game as the player runs it: each frame passes on last frame's
    /// UI events first, then lays the UI out.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        sheets: Vec<Stylesheet>,
        ui: UiInteraction,
        sounds: Vec<SoundAction>,
        output: Vec<String>,
    }
    impl Game {
        fn frame(
            &mut self,
            pointer: [f32; 2],
            pressed: Option<InstanceId>,
            seconds: f32,
        ) -> UiLayout {
            let events = self.ui.take_events();
            let sounds = fire_ui_events(&mut self.scripts, &mut self.world, events);
            self.sounds.extend(sounds);
            self.output
                .extend(self.scripts.take_output().into_iter().map(|line| line.text));
            let input = UiInput {
                pointer: Some(pointer),
                pressed,
                focused: None,
                on_top: None,
            };
            self.ui
                .layout(&self.world, (1280, 720), &self.sheets, input, None, seconds)
        }

        fn status(&self) -> String {
            let menu = self
                .world
                .find_first_child(self.world.workspace(), "Menu")
                .unwrap();
            let status = self.world.find_first_child(menu, "Status").unwrap();
            self.world
                .get(status)
                .unwrap()
                .property("Text")
                .and_then(Value::as_str)
                .unwrap()
                .to_owned()
        }
    }
    /// The rectangle drawn centred on `id`: its background.
    fn background(layout: &UiLayout, id: InstanceId) -> UiRectangle {
        let rect = layout.element(id).unwrap().rect;
        let centre = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        layout
            .draw_items
            .iter()
            .find_map(|item| match item {
                UiDrawItem::Rectangle(r)
                    if (0..2).all(|axis| {
                        (r.position[axis] + r.size[axis] * 0.5 - centre[axis]).abs() < 0.5
                    }) =>
                {
                    Some(*r)
                }
                _ => None,
            })
            .unwrap()
    }
    let rgb = |hex: u32| {
        [
            (hex >> 16) as f32 / 255.0,
            ((hex >> 8) & 0xff) as f32 / 255.0,
            (hex & 0xff) as f32 / 255.0,
        ]
    };

    let (world, scripts, folder) = start_example_in("hello-buttons");
    let css = std::fs::read_to_string(folder.join("ui.css")).unwrap();
    let (sheet, warnings) = axcenity_ui::parse_stylesheet("ui.css", &css);
    assert!(warnings.is_empty(), "{warnings:?}");
    let menu = world.find_first_child(world.workspace(), "Menu").unwrap();
    let play = world.find_first_child(menu, "Play").unwrap();
    let locked = world.find_first_child(menu, "Locked").unwrap();
    let mut game = Game {
        world,
        scripts,
        sheets: vec![sheet],
        ui: UiInteraction::default(),
        sounds: Vec::new(),
        output: Vec::new(),
    };
    let (over_play, over_locked, away) = ([640.0, 328.0], [640.0, 404.0], [100.0, 100.0]);

    let idle = game.frame(away, None, 1.0 / 60.0);
    assert_eq!(background(&idle, play).color[..3], rgb(0x3b82f6));

    // The pointer comes onto Play: the hover colour eases in over 0.15 s,
    // and MouseEnter fires once in Lua and in blocks, with the hover sound.
    game.frame(over_play, None, 0.0);
    let between = background(&game.frame(over_play, None, 0.05), play).color;
    let hovered = game.frame(over_play, None, 0.2);
    assert_eq!(background(&hovered, play).color[..3], rgb(0x1d4ed8));
    assert!(
        between[2] < rgb(0x3b82f6)[2] && between[2] > rgb(0x1d4ed8)[2],
        "half way between: {between:?}"
    );
    assert_eq!(hovered.cursor, UiCursor::Pointer);
    game.frame(over_play, None, 1.0 / 60.0);
    assert_eq!(game.status(), "Ready?");
    assert_eq!(game.output, ["The pointer is on Play (blocks)"]);
    let click = SoundAction::PlayOnce {
        sound_id: "assets/audio/click.wav".into(),
    };
    assert_eq!(game.sounds, std::slice::from_ref(&click));
    // After a short rest, the tooltip.
    let resting = game.frame(over_play, None, 0.6);
    assert!(
        resting
            .draw_items
            .iter()
            .any(|item| matches!(item, UiDrawItem::Text(text) if text.text == "Start the game"))
    );

    // Held, it shrinks to 0.95 around its centre; let go, it activates
    // with its click sound.
    game.frame(over_play, Some(play), 0.0);
    let held = game.frame(over_play, Some(play), 0.2);
    assert!((background(&held, play).size[0] - 190.0).abs() < 0.01);
    assert!(game.ui.activate(&game.world, play));
    game.scripts.activate_ui_button(&mut game.world, play);
    assert_eq!(game.status(), "Playing!");

    // Locked is disabled: its :disabled style, no hover and no Activated.
    let on_locked = game.frame(over_locked, None, 0.016);
    assert_eq!(on_locked.hovered, None);
    assert_eq!(on_locked.cursor, UiCursor::NotAllowed);
    assert_eq!(background(&on_locked, locked).color[..3], rgb(0x4b5563));
    assert_eq!(on_locked.hit_button(640.0, 404.0), None);
    assert!(!game.ui.activate(&game.world, locked));
    game.scripts.activate_ui_button(&mut game.world, locked);
    game.frame(away, None, 0.016);
    // Going onto Locked left Play: MouseLeave, once.
    assert_eq!(game.status(), "Point at Play");
    assert_eq!(
        game.output,
        [
            "The pointer is on Play (blocks)",
            "The pointer left Play (blocks)"
        ]
    );
    assert_eq!(game.sounds, [click.clone(), click], "hover, then click");
}

#[test]
fn hello_settings_widgets_change_by_pointer_fire_value_changed_and_move_from_scripts() {
    use axcenity_ui::{Stylesheet, UiInput, UiInteraction, UiLayout};

    /// The game as the player runs it: the pointer's presses go to the
    /// UI first, then the scripts step and hear what changed.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        sheets: Vec<Stylesheet>,
        ui: UiInteraction,
        output: Vec<String>,
    }
    impl Game {
        fn frame(&mut self, pointer: [f32; 2]) -> UiLayout {
            self.scripts.step(&mut self.world, 1.0 / 60.0);
            self.output
                .extend(self.scripts.take_output().into_iter().map(|line| line.text));
            let input = UiInput {
                pointer: Some(pointer),
                ..UiInput::default()
            };
            self.ui.layout(
                &self.world,
                (1280, 720),
                &self.sheets,
                input,
                None,
                1.0 / 60.0,
            )
        }
        fn click(&mut self, at: [f32; 2]) {
            self.frame(at);
            assert!(self.ui.press(&self.world, at).apply(&mut self.world));
            let _ = self
                .ui
                .release(&self.world, Some(at))
                .apply(&mut self.world);
            self.frame(at);
        }
        fn value(&self, name: &str) -> Value {
            let id = self
                .world
                .find_first_descendant(self.world.workspace(), name)
                .unwrap();
            self.world
                .get(id)
                .unwrap()
                .property(name_of(name))
                .unwrap()
                .clone()
        }
    }
    fn name_of(name: &str) -> &'static str {
        if name == "Status" { "Text" } else { "Value" }
    }

    let (world, scripts, folder) = start_example_in("hello-settings");
    let css = std::fs::read_to_string(folder.join("ui.css")).unwrap();
    let (sheet, warnings) = axcenity_ui::parse_stylesheet("ui.css", &css);
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut game = Game {
        world,
        scripts,
        sheets: vec![sheet],
        ui: UiInteraction::default(),
        output: Vec::new(),
    };
    let first = game.frame([0.0, 0.0]);
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);

    // Drag the volume from 50 to 75 (in steps of 5): ValueChanged fires in
    // Lua, which sets Loudness, and in blocks.
    game.frame([700.0, 232.0]);
    game.ui
        .press(&game.world, [700.0, 232.0])
        .apply(&mut game.world);
    game.ui
        .drag(&game.world, [776.0, 240.0])
        .apply(&mut game.world);
    let _ = game.ui.release(&game.world, Some([776.0, 240.0]));
    game.frame([776.0, 240.0]);
    assert_eq!(game.value("Volume"), Value::Number(75.0));
    assert_eq!(game.value("Status"), Value::String("Volume 75".into()));
    assert_eq!(
        game.value("Loudness"),
        Value::Number(75.0),
        "the script moved the bar"
    );
    assert!(
        game.output.iter().any(|line| line == "Volume (blocks): 75"),
        "{:?}",
        game.output
    );

    // Flip fullscreen.
    game.click([596.0, 288.0]);
    assert_eq!(game.value("Fullscreen"), Value::Bool(true));
    assert_eq!(game.value("Status"), Value::String("Fullscreen on".into()));

    // Open Quality and pick High from its list.
    game.click([670.0, 344.0]);
    assert!(game.ui.open_dropdown().is_some());
    game.click([670.0, 460.0]);
    assert_eq!(game.value("Quality"), Value::String("High".into()));
    assert_eq!(game.value("Status"), Value::String("Quality High".into()));
    assert!(
        game.output
            .iter()
            .any(|line| line == "Quality (blocks): High"),
        "{:?}",
        game.output
    );
    let errors: Vec<_> = game
        .output
        .iter()
        .filter(|line| line.contains("rror"))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn hello_ui_graph_runs_its_event_graph_as_an_ordinary_local_script() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-ui-graph");
    let project = axcenity_project::load(&folder).unwrap();
    let problems: Vec<String> = project.problems.iter().map(ToString::to_string).collect();
    assert!(problems.is_empty(), "{problems:?}");
    let mut world = project.world;
    let menu = world.find_first_child(world.workspace(), "Menu").unwrap();

    // The graph became a LocalScript under its ScreenGui, of plain Lua.
    let script = world.find_first_child(menu, "EventGraph").unwrap();
    assert_eq!(
        world.get(script).unwrap().class(),
        axcenity_core::ClassName::LocalScript
    );
    let file = project
        .scripts
        .iter()
        .find(|file| file.instance == script)
        .unwrap();
    assert_eq!(file.path, "ui/Menu.uigraph.json");
    assert!(
        file.source
            .contains("gui.Pages.Home.SettingsButton.Activated:Connect(function()\n    gui.Pages.CurrentPage = \"Settings\"\nend)"),
        "{}",
        file.source
    );

    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let pages = world.find_first_child(menu, "Pages").unwrap();
    let home = world.find_first_child(pages, "Home").unwrap();
    let settings = world.find_first_child(home, "SettingsButton").unwrap();
    let page = |world: &World| {
        world
            .get(pages)
            .unwrap()
            .property("CurrentPage")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(page(&world), "");

    // Button Activated → Show Page Settings.
    scripts.activate_ui_button(&mut world, settings);
    assert_eq!(page(&world), "Settings");
    let settings_page = world.find_first_child(pages, "Settings").unwrap();
    let back = world.find_first_child(settings_page, "Back").unwrap();
    scripts.activate_ui_button(&mut world, back);
    assert_eq!(page(&world), "Home");
    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Back on the home page"]);
}

/// Every file under `folder`, by path, for checking nothing was written there.
fn files_in(folder: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            files.extend(files_in(&path));
        } else {
            files.insert(path.clone(), std::fs::read(&path).unwrap());
        }
    }
    files
}

/// hello-saves, opened from a copy as the player opens it, with its slots in
/// `saves` instead of the player's data folder.
fn start_hello_saves(copy: &str, saves: &Path) -> (World, ScriptRuntime, PathBuf) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-saves");
    let folder =
        std::env::temp_dir().join(format!("axcenity-player-{copy}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&example, &folder);
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_save_folder(Some(saves.to_path_buf()));
    // As `start_scripts` does, keeping the output for the test to read.
    for script in &project.scripts {
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
    }
    (world, scripts, folder)
}

fn script_lines(scripts: &mut ScriptRuntime) -> Vec<String> {
    scripts
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

#[test]
fn hello_saves_saves_loads_and_restores_its_checkpoint_from_lua_and_blocks_outside_the_project() {
    use axcenity_core::KeyCode as K;
    let saves = std::env::temp_dir().join(format!("axcenity-player-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&saves);
    let (mut world, mut scripts, folder) = start_hello_saves("hello-saves", &saves);
    let project_files = files_in(&folder);
    assert_eq!(
        script_lines(&mut scripts),
        ["A new game. K saves, L loads, R goes back to the checkpoint."]
    );
    let hero = world
        .find_first_descendant(world.workspace(), "Hero")
        .and_then(|hero| world.find_first_child(hero, "Body"))
        .unwrap();
    let coin = |world: &World, n: u32| {
        let coins = world.find_first_child(world.workspace(), "Coins").unwrap();
        let coin = world.find_first_child(coins, &format!("Coin{n}")).unwrap();
        world.get(coin).unwrap().property("Transparency").cloned()
    };
    let walk_to = |world: &mut World, scripts: &mut ScriptRuntime, at: Vector3| {
        world
            .set_property(hero, "Position", Value::Vector3(at))
            .unwrap();
        scripts.step(world, 1.0 / 60.0);
        scripts.heartbeat(world, 1.0 / 60.0);
    };
    let press = |world: &mut World, scripts: &mut ScriptRuntime, key: K| {
        for pressed in [true, false] {
            scripts.input(world, &[KeyEvent { key, pressed }]);
        }
        scripts.step(world, 1.0 / 60.0);
    };

    walk_to(&mut world, &mut scripts, Vector3::new(8.0, 1.5, 0.0));
    walk_to(&mut world, &mut scripts, Vector3::new(20.0, 2.5, 10.0));
    walk_to(&mut world, &mut scripts, Vector3::new(14.0, 1.5, 6.0));
    assert_eq!(
        script_lines(&mut scripts),
        ["Coins: 1", "Checkpoint!", "Coins: 2"]
    );

    // K saves slot 1 from blocks: in the save folder, not the project.
    press(&mut world, &mut scripts, K::K);
    assert_eq!(script_lines(&mut scripts), ["Saved slot 1"]);
    assert!(saves.join("slot-1.json").is_file());
    assert_eq!(
        files_in(&folder),
        project_files,
        "saving wrote to the project"
    );

    // A third coin, then L loads slot 1 from blocks: two coins, the third back.
    walk_to(&mut world, &mut scripts, Vector3::new(-10.0, 1.5, -10.0));
    assert_eq!(script_lines(&mut scripts), ["Coins: 3"]);
    assert_eq!(coin(&world, 3), Some(Value::Number(1.0)));
    press(&mut world, &mut scripts, K::L);
    assert_eq!(script_lines(&mut scripts), ["Loaded slot 1"]);
    assert_eq!(scripts.saves().player_data["Coins"], Value::Number(2.0));
    assert_eq!(coin(&world, 3), Some(Value::Number(0.0)));
    assert_eq!(position_of_part(&world, hero), Vector3::new(14.0, 1.5, 6.0));

    // Falling off the floor restores the checkpoint from Lua: on the pad,
    // with the one coin collected before it.
    walk_to(&mut world, &mut scripts, Vector3::new(60.0, -40.0, 0.0));
    assert_eq!(
        script_lines(&mut scripts),
        ["Back at the checkpoint, with 1 coins"]
    );
    assert_eq!(
        position_of_part(&world, hero),
        Vector3::new(20.0, 2.6, 10.0)
    );
    assert_eq!(coin(&world, 1), Some(Value::Number(1.0)));
    assert_eq!(coin(&world, 2), Some(Value::Number(0.0)));
    // R does the same from blocks.
    world
        .set_property(
            hero,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.5, 0.0)),
        )
        .unwrap();
    press(&mut world, &mut scripts, K::R);
    assert_eq!(
        position_of_part(&world, hero),
        Vector3::new(20.0, 2.6, 10.0)
    );
    assert_eq!(script_lines(&mut scripts), Vec::<String>::new());

    // The next run of the game starts from slot 1, with its checkpoint.
    let (world, mut scripts, _) = start_hello_saves("hello-saves-again", &saves);
    assert_eq!(script_lines(&mut scripts), ["Loaded slot 1: 2 coins"]);
    assert_eq!(coin(&world, 1), Some(Value::Number(1.0)));
    assert_eq!(coin(&world, 3), Some(Value::Number(0.0)));
    let checkpoint = world.find_first_child(world.workspace(), "Checkpoint");
    assert_eq!(scripts.saves().checkpoint(&world), checkpoint);
    assert_eq!(scripts.saves().settings["Volume"], Value::Number(0.8));
}

#[test]
fn hello_saves_loading_a_missing_slot_from_blocks_fails_and_leaves_the_world_alone() {
    use axcenity_core::KeyCode as K;
    let saves =
        std::env::temp_dir().join(format!("axcenity-player-no-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&saves);
    let (mut world, mut scripts, _) = start_hello_saves("hello-saves-missing", &saves);
    script_lines(&mut scripts);
    let before = axcenity_save::snapshot(&world);
    for pressed in [true, false] {
        scripts.input(&mut world, &[KeyEvent { key: K::L, pressed }]);
    }
    let lines = script_lines(&mut scripts);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("error: scripts/keys.client.blocks.json:")
            && lines[0].contains("save slot 1 doesn't exist yet"),
        "{lines:?}"
    );
    assert_eq!(axcenity_save::snapshot(&world), before);
    assert_eq!(scripts.saves().player_data["Coins"], Value::Number(0.0));
}

fn position_of_part(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn hello_menu_moves_focus_the_same_by_keys_and_controller_and_uses_what_is_focused() {
    use axcenity_ui::{GamepadButton, Stylesheet, TextCommand, TextOutcome, UiLayout, button_key};

    /// The menu as the player runs it: keys and controller buttons go
    /// through `navigate_ui`, typing through `edit_text_box`.
    struct Game {
        world: World,
        scripts: ScriptRuntime,
        sheets: Vec<Stylesheet>,
        ui: UiInteraction,
        typing: Option<InstanceId>,
        output: Vec<String>,
    }
    impl Game {
        fn frame(&mut self) -> UiLayout {
            self.scripts.step(&mut self.world, 1.0 / 60.0);
            self.output
                .extend(self.scripts.take_output().into_iter().map(|line| line.text));
            let input = UiInput {
                focused: self.typing,
                ..UiInput::default()
            };
            self.ui.layout(
                &self.world,
                (1280, 720),
                &self.sheets,
                input,
                None,
                1.0 / 60.0,
            )
        }
        fn nav(&mut self, key: NavKey) {
            assert!(self.ui.takes(&self.world, key) || key == NavKey::Down);
            if let Some(id) = navigate_ui(&mut self.ui, &mut self.world, &mut self.scripts, key) {
                self.typing = Some(id);
            }
            self.frame();
        }
        fn named(&self, name: &str) -> InstanceId {
            self.world
                .find_first_descendant(self.world.workspace(), name)
                .unwrap()
        }
        fn focused(&self) -> Option<String> {
            let id = self.ui.selected()?;
            Some(self.world.get(id)?.name().to_owned())
        }
        fn finish_typing(&mut self) {
            let id = self.typing.take().unwrap();
            self.scripts.release_text_box(&mut self.world, id);
            self.frame();
        }
    }
    let start = || {
        let (world, scripts, folder) = start_example_in("hello-menu");
        let css = std::fs::read_to_string(folder.join("ui.css")).unwrap();
        let (sheet, warnings) = axcenity_ui::parse_stylesheet("ui.css", &css);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut game = Game {
            world,
            scripts,
            sheets: vec![sheet],
            ui: UiInteraction::default(),
            typing: None,
            output: Vec::new(),
        };
        let first = game.frame();
        assert!(first.warnings.is_empty(), "{:?}", first.warnings);
        game
    };

    // The keyboard: Tab to start, then the down arrow. The controller:
    // the d-pad all the way. Both visit the same elements in order, and
    // Quit's NextSelectionDown wraps both round to the name box.
    let keyboard: Vec<NavKey> = [KeyCode::Tab]
        .into_iter()
        .chain([KeyCode::ArrowDown; 5])
        .map(|key| nav_key(key, false).unwrap())
        .collect();
    let controller: Vec<NavKey> = [GamepadButton::DPadDown; 6]
        .into_iter()
        .map(|button| button_key(button).unwrap())
        .collect();
    let mut orders = Vec::new();
    for keys in [keyboard, controller] {
        let mut game = start();
        let mut order = Vec::new();
        for key in keys {
            game.nav(key);
            order.extend(game.focused());
        }
        orders.push(order);
    }
    let expected = [
        "PlayerName",
        "Play",
        "Options",
        "Volume",
        "Quit",
        "PlayerName",
    ];
    assert_eq!(orders[0], expected);
    assert_eq!(orders[1], expected);

    // Enter and the controller's A both activate the focused button.
    for confirm in [
        nav_key(KeyCode::Enter, false).unwrap(),
        button_key(GamepadButton::South).unwrap(),
    ] {
        let mut game = start();
        game.nav(NavKey::Next);
        game.nav(NavKey::Down);
        game.nav(NavKey::Down);
        assert_eq!(game.focused().as_deref(), Some("Options"));
        game.nav(confirm);
        assert!(
            game.output.iter().any(|line| line == "Options pressed"),
            "{:?}",
            game.output
        );
    }

    // The focused buttons show their :focus rule, so no default ring; the
    // slider has none, so it gets the ring, and moves with left and right.
    let mut game = start();
    for _ in 0..4 {
        game.nav(NavKey::Next);
    }
    assert_eq!(game.focused().as_deref(), Some("Volume"));
    game.nav(nav_key(KeyCode::ArrowRight, false).unwrap());
    let volume = game.named("Volume");
    assert_eq!(
        game.world.get(volume).unwrap().property("Value"),
        Some(&Value::Number(55.0))
    );
    let ring = |layout: &UiLayout| {
        layout.draw_items.iter().any(|item| {
            matches!(item, axcenity_render::UiDrawItem::Rectangle(r)
                if r.border_color == axcenity_ui::FOCUS_RING_COLOR)
        })
    };
    assert!(ring(&game.frame()));
    let status = game.named("Status");
    assert_eq!(
        game.world.get(status).unwrap().property("Text"),
        Some(&Value::String("Volume 55".into())),
        "ValueChanged fired"
    );
    game.nav(NavKey::Up);
    game.nav(NavKey::Up);
    assert_eq!(game.focused().as_deref(), Some("Play"));
    assert!(!ring(&game.frame()), "TextButton:focus styles it");

    // Confirm on the name box types into it; Enter finishes and fires the
    // same FocusLost a click away does.
    let mut game = start();
    game.nav(NavKey::Next);
    game.nav(NavKey::Confirm);
    let name = game.named("PlayerName");
    assert_eq!(game.typing, Some(name));
    for command in [TextCommand::Insert("Ana".into()), TextCommand::Submit] {
        let outcome = axcenity_ui::edit_text_box(&mut game.world, name, command);
        if outcome == TextOutcome::Finished {
            game.finish_typing();
        }
    }
    assert_eq!(game.typing, None);
    assert_eq!(
        game.focused().as_deref(),
        Some("PlayerName"),
        "focus comes back"
    );
    assert!(
        game.output.iter().any(|line| line == "Hello, Ana"),
        "{:?}",
        game.output
    );
    // Then the arrows move on from it.
    game.nav(NavKey::Down);
    assert_eq!(game.focused().as_deref(), Some("Play"));

    // A click on the box and a click away: the same FocusLost.
    let mut game = start();
    let layout = game.frame();
    let rect = layout.element(name).unwrap().rect;
    let at = [rect.x + 4.0, rect.y + 4.0];
    let _ = game.ui.press(&game.world, at);
    assert_eq!(layout.hit_text_box(at[0], at[1]), Some(name));
    game.typing = Some(name);
    let _ = axcenity_ui::edit_text_box(&mut game.world, name, TextCommand::Insert("Bo".into()));
    game.finish_typing();
    assert!(
        game.output.iter().any(|line| line == "Hello, Bo"),
        "{:?}",
        game.output
    );
}

#[test]
fn hello_strings_shows_the_active_language_and_switches_from_lua_and_blocks() {
    let (mut world, mut scripts, folder) = start_example_in("hello-strings");
    let project = axcenity_project::load(&folder).unwrap();
    let stylesheets: Vec<_> = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let shown = |world: &World| -> Vec<String> {
        axcenity_ui::layout_ui(world, 1280, 720, &stylesheets)
            .draw_items
            .into_iter()
            .filter_map(|item| match item {
                axcenity_render::UiDrawItem::Text(text) => Some(text.text),
                _ => None,
            })
            .collect()
    };
    let hint = "This line has no translation yet, so it shows its own Text.";
    assert_eq!(
        shown(&world),
        [
            "Welcome!",
            "Play",
            "Language: English",
            "Back to English",
            hint
        ]
    );
    let panel = world
        .find_first_child(world.workspace(), "Menu")
        .and_then(|menu| world.find_first_child(menu, "Panel"))
        .unwrap();
    let button = |world: &World, name| world.find_first_child(panel, name).unwrap();

    // The Lua LocalScript steps to the next language: every bound control
    // changes, and the one with no entry keeps its own Text.
    let language = button(&world, "LanguageButton");
    scripts.activate_ui_button(&mut world, language);
    assert_eq!(world.language(), "Français");
    assert_eq!(
        shown(&world),
        [
            "Bienvenue !",
            "Jouer",
            "Langue : français",
            "Revenir à l'anglais",
            hint
        ]
    );
    let play = button(&world, "PlayButton");
    scripts.activate_ui_button(&mut world, play);
    scripts.activate_ui_button(&mut world, language);
    assert_eq!(shown(&world)[1], "Jugar");

    // The block script's "set language to English".
    let english = button(&world, "EnglishButton");
    scripts.activate_ui_button(&mut world, english);
    assert_eq!(world.language(), "English");
    assert_eq!(shown(&world)[0], "Welcome!");
    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Language: Français", "Jouer", "Language: Español"]);
}

#[test]
fn reading_and_writing_cframes_leaves_a_saved_world_byte_for_byte_the_same() {
    for name in ["primitives", "hello-character", "hello-mesh", "hello-door"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
            .join("world.json");
        let mut world =
            axcenity_project::parse_world(&std::fs::read_to_string(path).unwrap()).unwrap();
        let script = world.create(ClassName::Script).unwrap();
        world
            .set_parent(script, Some(world.server_script_service()))
            .unwrap();
        let before = axcenity_project::write_world(&world);
        let mut scripts = ScriptRuntime::new();
        scripts.start_script(
            &mut world,
            script,
            "scripts/Pivots.server.lua",
            r#"
            local count = 0
            for _, object in workspace:GetDescendants() do
                if object:IsA("BasePart") then
                    object.CFrame = object.CFrame
                    object:PivotTo(object:GetPivot())
                    count += 1
                elseif object:IsA("Model") then
                    object:PivotTo(object:GetPivot())
                end
            end
            print(count)
            "#,
        );
        let output = scripts.take_output();
        assert_eq!(output.len(), 1, "{name}: {output:?}");
        assert_ne!(output[0].text, "0", "{name} has parts");
        assert_eq!(before, axcenity_project::write_world(&world), "{name}");
    }
}
