use std::path::Path;

use axcenity_core::{Value, Vector3};

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
    use axcenity_render::{TextureRenderer, wgpu};
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
    let (width, height) = (480, 270);
    for name in names {
        let folder = examples.join(&name);
        let project = axcenity_project::load(&folder).unwrap();
        let stylesheets: Vec<_> = project
            .stylesheets
            .iter()
            .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
            .collect();
        let (mut world, mut scripts) = start_example(&name);
        let (mut physics, mut clock) = (Physics::new(), 0);
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, 1.0);
        let mut camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
        if let Some(view) = scripts.take_camera_move() {
            camera.set_view(view.position, view.orientation);
        }
        let assets = Assets::new(&folder);
        // Models load the first time they're drawn, so draw twice.
        let mut renderer = TextureRenderer::new(&device, &queue, width, height);
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
        if let Some(folder) = &snapshots {
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
        // An example can have a reference screenshot of its top-left
        // corner (where hello-css's HUD is), which it must still match.
        if let Ok(file) = fs::File::open(folder.join("reference.png")) {
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
            let average = difference as f64 / f64::from(info.width * info.height * 3);
            assert!(
                average < 6.0,
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
}

/// The example project in `examples/<name>`, with its scripts started.
fn start_example(name: &str) -> (World, ScriptRuntime) {
    let (world, scripts, _) = start_example_in(name);
    (world, scripts)
}

/// Like [`start_example`], with the folder of the copy it opened.
fn start_example_in(name: &str) -> (World, ScriptRuntime, PathBuf) {
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
fn run_until(
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
