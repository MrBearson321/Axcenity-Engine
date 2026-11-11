//! Phase 69: the genre templates as built games. A project made from each
//! is built with Build Game, unpacked and opened as the player opens a
//! game, and plays with no errors or warnings; and the Multiplayer Arena,
//! hosted as `--host` does on a UDP port with two players joined as
//! `--join` does, keeps each player's team, character, menu and stats.

use std::path::{Path, PathBuf};

use axcenity_character::MoveInput;
use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{InstanceId, KeyCode, UserInputType, Value, Vector3};
use axcenity_project::Template;

use super::*;

const FRAME: f32 = 1.0 / 60.0;
const VIEW: (u32, u32) = (1280, 720);

/// A fresh temporary folder for `test`.
fn temp(test: &str, what: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-template-{test}-{what}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

/// A project made from `template`, built with Build Game and unpacked.
fn built_game(template: Template, test: &str) -> PathBuf {
    let parent = temp(test, "source");
    let source = axcenity_project::create_project_from(&parent, "My Game", template).unwrap();
    let packed = axcenity_project::pack_game(&source).unwrap();
    let _ = fs::remove_dir_all(&parent);
    let folder = temp(test, "built");
    axcenity_project::unpack_game(&packed, &folder).unwrap();
    folder
}

/// How a device starts: on its own, hosting, or joining.
enum Start<'a> {
    Alone,
    Host,
    Join(&'a str),
}

impl App {
    /// The built game in `folder`, opened as `main` opens a game, with its
    /// saves in `saves` and the player's `user_id`.
    fn open_game(folder: &Path, saves: &Path, user_id: f64, start: Start) -> (App, Option<u16>) {
        let project = axcenity_project::load(folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let start_level = project.manifest.start_level.clone();
        let mut problems = Vec::new();
        let levels = game_levels(folder, &start_level, &project.world, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        let stylesheets: Vec<_> = project
            .stylesheets
            .iter()
            .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
            .collect();
        let mut world = project.world;
        let assets = Assets::new(folder);
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        scripts.set_save_folder(Some(saves.to_owned()));
        scripts.set_user_id(user_id);
        let mut animators = Animators::new();
        animators.update(&mut world, &assets, 0.0);
        axcenity_project::load_starter_character(folder, &mut world).unwrap();
        scripts.set_input_map(project.input_map.clone());
        scripts.set_levels(&start_level, &start_level, levels);
        start_scripts(&mut scripts, &mut world, &project.scripts);
        // A discovery port of its own, so devices in one test don't share.
        let mut network = Network::new(GameInfo {
            name: project.manifest.name.clone(),
            version: project.manifest.version.clone(),
        })
        .with_discovery_port(47_000 + (std::process::id() % 500) as u16 + user_id as u16);
        network.set_client_scripts(client_scripts(&world, &project.scripts));
        let port = match start {
            Start::Alone => None,
            Start::Host => Some(
                network
                    .host(&mut world, &mut scripts, Some(0), None, true)
                    .unwrap(),
            ),
            Start::Join(address) => {
                network
                    .join(&mut world, &mut scripts, address, None)
                    .unwrap();
                None
            }
        };
        let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
        let mut app = App::new(world, scripts, stylesheets, assets, animators, camera);
        app.network = network;
        (app, port)
    }

    /// One frame in the player's order (`docs/architecture/frame-loop.md`),
    /// without the window: network receive, input and characters, scripts,
    /// physics, touches and health, Heartbeat, animation and Tools, the
    /// render step and the camera, then network send. Returns what the
    /// scripts and the engine reported as warnings and errors.
    fn game_frame(&mut self, events: &[InputEvent], motion: MoveInput) -> Vec<String> {
        let seconds = FRAME;
        let mut problems = Vec::new();
        self.network.receive(
            &mut self.sim.world,
            &mut self.sim.scripts,
            f64::from(seconds),
        );
        problems.extend(
            self.network
                .take_log()
                .into_iter()
                .filter(|line| line.starts_with("warning") || line.starts_with("error")),
        );
        if self.sim.scripts.is_client_only() && !self.following {
            self.follow_server();
        }
        let simulates = !self.following;
        let others = self
            .network
            .other_characters(&self.sim.world, &self.sim.scripts);
        self.sim
            .characters
            .set_players(self.sim.scripts.local_player(), others);
        self.network.set_running(motion.run);
        self.sim
            .characters
            .step(&mut self.sim.world, &self.sim.assets, motion, seconds);
        fire_seated(
            &mut self.sim.scripts,
            &mut self.sim.world,
            &mut self.sim.characters,
        );
        if simulates {
            fire_landed(
                &mut self.sim.scripts,
                &mut self.sim.world,
                &mut self.sim.characters,
            );
            axcenity_gameplay::step_humanoids(
                &mut self.sim.scripts,
                &mut self.sim.world,
                &self.sim.characters,
                f64::from(seconds),
            );
            self.sim
                .scripts
                .step_players(&mut self.sim.world, f64::from(seconds));
            self.sim
                .agents
                .step(&mut self.sim.world, &self.sim.assets, seconds);
            problems.extend(self.sim.agents.take_warnings());
            fire_sensed(
                &mut self.sim.scripts,
                &mut self.sim.world,
                &mut self.sim.agents,
            );
        }
        self.begin_camera(seconds, VIEW);
        self.sim.scripts.input_events(&mut self.sim.world, events);
        if simulates {
            self.sim
                .scripts
                .step_prompts(&mut self.sim.world, f64::from(seconds));
            axcenity_gameplay::step_tools(&mut self.sim.scripts, &mut self.sim.world);
        }
        self.sim
            .scripts
            .step(&mut self.sim.world, f64::from(seconds));
        self.sim
            .scripts
            .stepped(&mut self.sim.world, f64::from(seconds));
        let held = self.network.held_parts(&self.sim.world, &self.sim.scripts);
        self.sim.physics.set_held(held);
        if !simulates && self.network.simulates_physics() {
            self.sim
                .physics
                .step(&mut self.sim.world, f64::from(seconds));
            let _ = self.sim.physics.take_touches();
            let _ = self.sim.physics.take_touch_ends();
        }
        if simulates {
            self.sim.characters.keep_kinematic(&mut self.sim.world);
            self.sim.agents.keep_kinematic(&mut self.sim.world);
            self.sim
                .scripts
                .apply_shatters(&mut self.sim.world, &mut self.sim.physics);
            self.sim
                .physics
                .step(&mut self.sim.world, f64::from(seconds));
            let touches = self.sim.physics.take_touches();
            self.sim.scripts.fire_touched(&mut self.sim.world, &touches);
            let ends = self.sim.physics.take_touch_ends();
            self.sim
                .scripts
                .fire_touch_ended(&mut self.sim.world, &ends);
            self.sim
                .scripts
                .fire_breaks(&mut self.sim.world, &mut self.sim.physics);
            axcenity_gameplay::step_health(
                &mut self.sim.scripts,
                &mut self.sim.world,
                f64::from(seconds),
            );
        }
        self.sim
            .scripts
            .heartbeat(&mut self.sim.world, f64::from(seconds));
        let events =
            self.sim
                .animators
                .update(&mut self.sim.world, &self.sim.assets, f64::from(seconds));
        for event in events {
            let (animator, name, args) = event.script_event();
            self.sim
                .scripts
                .fire_instance_event(&mut self.sim.world, animator, name, &args);
        }
        problems.extend(self.sim.animators.take_errors());
        axcenity_gameplay::place_tools(
            &mut self.sim.scripts,
            &mut self.sim.world,
            self.sim.animators.poses(),
        );
        self.render_step(seconds);
        self.end_camera(seconds);
        problems.extend(self.sim.assets.take_errors());
        self.network.send(&self.sim.world, &mut self.sim.scripts);
        for line in self.sim.scripts.take_output() {
            if !matches!(line.kind, OutputKind::Print) {
                problems.push(line.text);
            }
        }
        problems
    }
}

fn key(key: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::Key { key, pressed })
}

fn click(pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::MouseButton {
        button: UserInputType::MouseButton1,
        pressed,
        position: [VIEW.0 as f32 / 2.0, VIEW.1 as f32 / 2.0],
    })
}

#[test]
fn every_genre_template_plays_as_a_built_game_without_errors() {
    for template in Template::GAMES {
        let test = template.folder().unwrap();
        let folder = built_game(template, test);
        let saves = temp(test, "saves");
        let (mut app, _) = App::open_game(&folder, &saves, 1.0, Start::Alone);
        // Walk forward, click in the middle, and open and close the menu.
        let forward = MoveInput {
            forward: true,
            ..MoveInput::default()
        };
        let mut problems = Vec::new();
        for frame in 0..300 {
            let events = match frame {
                60 => vec![click(true)],
                61 => vec![click(false)],
                120 | 180 => vec![key(KeyCode::Escape, true)],
                121 | 181 => vec![key(KeyCode::Escape, false)],
                _ => Vec::new(),
            };
            let motion = if frame < 60 {
                forward
            } else {
                MoveInput::default()
            };
            problems.extend(app.game_frame(&events, motion));
        }
        assert!(problems.is_empty(), "{}: {problems:#?}", template.name());
        // It ran: the player has a character and the menu is theirs.
        let player = app.sim.scripts.local_player().expect("the player joined");
        assert!(
            character_of(&app.sim.world, player).is_some(),
            "{}",
            template.name()
        );
        let menu = app
            .sim
            .world
            .find_first_child(app.sim.world.workspace(), "GameMenu")
            .unwrap();
        assert_eq!(owner(&app.sim.world, menu).as_deref(), Some("Player"));
        let _ = fs::remove_dir_all(&folder);
        let _ = fs::remove_dir_all(&saves);
    }
}

fn character_of(world: &World, player: InstanceId) -> Option<InstanceId> {
    world.get(player)?.property("Character")?.as_instance()
}

fn owner(world: &World, gui: InstanceId) -> Option<String> {
    match world.get(gui)?.attribute("Owner")? {
        Value::String(name) => Some(name.clone()),
        _ => None,
    }
}

/// The Players in `world`, by name.
fn players(world: &World) -> Vec<(String, InstanceId)> {
    let mut list: Vec<_> = world
        .get(world.players())
        .unwrap()
        .children()
        .iter()
        .map(|&id| (world.get(id).unwrap().name().to_owned(), id))
        .collect();
    list.sort();
    list
}

fn player(world: &World, name: &str) -> InstanceId {
    world.find_first_child(world.players(), name).unwrap()
}

/// `player`'s leaderboard value `stat`, where this device has it.
fn stat(world: &World, player: InstanceId, stat: &str) -> Option<f64> {
    let stats = world.find_first_child(player, "leaderstats")?;
    let value = world.find_first_child(stats, stat)?;
    world.get(value)?.property("Value")?.as_number()
}

fn team(world: &World, player: InstanceId) -> Option<String> {
    let team = world.get(player)?.property("Team")?.as_instance()?;
    Some(world.get(team)?.name().to_owned())
}

/// Where `character`'s first part is.
fn character_at(world: &World, character: InstanceId) -> Vector3 {
    let parts = axcenity_character::body_parts(world, character);
    world
        .get(parts[0])
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

/// Moves every part of `character` by `offset`.
fn move_character(world: &mut World, character: InstanceId, offset: Vector3) {
    for part in axcenity_character::body_parts(world, character) {
        let at = world
            .get(part)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(
                    at.x + offset.x,
                    at.y + offset.y,
                    at.z + offset.z,
                )),
            )
            .unwrap();
    }
}

/// The ScreenGui named `name` that the device's player claimed (Owner).
fn claimed(world: &World, name: &str, player: &str) -> Option<InstanceId> {
    world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|&id| {
            world.get(id).unwrap().name() == name && owner(world, id).as_deref() == Some(player)
        })
}

fn visible(world: &World, gui: InstanceId, child: &str) -> bool {
    let frame = world.find_first_child(gui, child).unwrap();
    world.get(frame).unwrap().property("Visible") == Some(&Value::Bool(true))
}

/// One frame on every device, host first, with `host_events` for the host.
fn frame_all(devices: &mut [App], host_events: &[InputEvent]) {
    for (n, device) in devices.iter_mut().enumerate() {
        let events = if n == 0 { host_events } else { &[] };
        let problems = device.game_frame(events, MoveInput::default());
        assert!(problems.is_empty(), "device {n}: {problems:#?}");
    }
    // Real UDP: give the packets a moment to cross.
    std::thread::sleep(std::time::Duration::from_millis(1));
}

/// Frames on every device until `done`, failing after `seconds`.
fn run_until(
    devices: &mut [App],
    seconds: f32,
    what: &str,
    host_events: &mut dyn FnMut(u32) -> Vec<InputEvent>,
    done: &dyn Fn(&[App]) -> bool,
) {
    for frame in 0..(seconds / FRAME) as u32 {
        if done(devices) {
            return;
        }
        frame_all(devices, &host_events(frame));
    }
    assert!(done(devices), "{what} within {seconds} s");
}

/// The host presses and lets go of `key`, then every device runs a while.
fn host_taps(devices: &mut [App], tap: KeyCode) {
    frame_all(devices, &[key(tap, true)]);
    frame_all(devices, &[key(tap, false)]);
    for _ in 0..20 {
        frame_all(devices, &[]);
    }
}

#[test]
fn the_arena_hosted_with_two_players_joined_keeps_each_players_team_menu_and_stats() {
    let folder = built_game(Template::Arena, "arena-hosted");
    let saves = temp("arena-hosted", "saves");
    let (host, port) = App::open_game(&folder, &saves.join("host"), 1.0, Start::Host);
    let address = format!("127.0.0.1:{}", port.unwrap());
    let mut devices = vec![host];
    for user in [2.0, 3.0] {
        let device_saves = saves.join(format!("player-{user}"));
        let (joined, _) = App::open_game(&folder, &device_saves, user, Start::Join(&address));
        devices.push(joined);
    }
    let no_input = &mut |_| Vec::new();

    // Everyone is in, on alternating teams, with a character and stats,
    // and every device sees all three.
    run_until(
        &mut devices,
        10.0,
        "all three joined with characters",
        no_input,
        &|d| {
            d.iter().all(|device| {
                let world = &device.sim.world;
                let list = players(world);
                list.len() == 3
                    && list.iter().all(|(_, id)| {
                        character_of(world, *id).is_some() && stat(world, *id, "Kills").is_some()
                    })
            })
        },
    );
    let server = &devices[0].sim.world;
    let teams: Vec<_> = ["Player", "Player2", "Player3"]
        .into_iter()
        .map(|name| team(server, player(server, name)))
        .collect();
    assert_eq!(
        teams,
        [Some("Red".into()), Some("Blue".into()), Some("Red".into())]
    );
    for (name, spawn) in [("Player", -36.0), ("Player2", 36.0), ("Player3", -36.0)] {
        let character = character_of(server, player(server, name)).unwrap();
        let at = character_at(server, character);
        assert!((at.x - spawn).abs() < 8.0, "{name} at {at:?}");
    }

    // Each device plays with its own menu: the host's is the world's, and
    // each joined player's is a copy of its own.
    for (n, name) in ["Player", "Player2", "Player3"].into_iter().enumerate() {
        let world = &devices[n].sim.world;
        assert_eq!(
            devices[n].sim.scripts.local_player(),
            Some(player(world, name))
        );
        assert!(claimed(world, "GameMenu", name).is_some(), "{name}'s menu");
        assert!(claimed(world, "Hud", name).is_some(), "{name}'s HUD");
    }
    // The host pauses: their menu opens, and no one else's does.
    host_taps(&mut devices, KeyCode::Escape);
    let host_menu = claimed(&devices[0].sim.world, "GameMenu", "Player").unwrap();
    assert!(visible(&devices[0].sim.world, host_menu, "Pause"));
    for (n, name) in [(1, "Player2"), (2, "Player3")] {
        let world = &devices[n].sim.world;
        let menu = claimed(world, "GameMenu", name).unwrap();
        assert!(!visible(world, menu, "Pause"), "{name}'s menu stays shut");
    }
    host_taps(&mut devices, KeyCode::Escape);
    assert!(!visible(&devices[0].sim.world, host_menu, "Pause"));

    // Chat from a joined player reaches everyone.
    // The join messages went out as each joined, before their device was
    // in the chat, so the host has them all.
    let host_chat = devices[0].sim.scripts.chat_lines();
    for line in [
        "Player joined the Red team",
        "Player2 joined the Blue team",
        "Player3 joined the Red team",
    ] {
        assert!(
            host_chat.iter().any(|said| said.text == line),
            "{host_chat:?}"
        );
    }
    devices[1].sim.scripts.send_chat("good luck!");
    run_until(
        &mut devices,
        3.0,
        "the chat line reached everyone",
        no_input,
        &|d| {
            d.iter().all(|device| {
                device.sim.scripts.chat_lines().iter().any(|line| {
                    line.from.as_deref() == Some("Player2") && line.text == "good luck!"
                })
            })
        },
    );

    // The host swings at Player2 until they're knocked out: a kill for the
    // host, a death for Player2, on every device, and Player2 respawns.
    let server = &mut devices[0].sim.world;
    let host_character = character_of(server, player(server, "Player")).unwrap();
    let blue = player(server, "Player2");
    let target = character_of(server, blue).unwrap();
    for character in [host_character, target] {
        if let Some(field) = server.find_first_child(character, "ForceField") {
            server.destroy(field).unwrap();
        }
    }
    let (to, from) = (
        character_at(server, target),
        character_at(server, host_character),
    );
    let offset = Vector3::new(to.x - from.x, to.y - from.y, to.z + 3.0 - from.z);
    move_character(server, host_character, offset);
    let mut swing = |frame: u32| match frame % 20 {
        0 => vec![click(true)],
        1 => vec![click(false)],
        _ => Vec::new(),
    };
    run_until(
        &mut devices,
        5.0,
        "Player2 was knocked out",
        &mut swing,
        &|d| {
            d.iter().all(|device| {
                let world = &device.sim.world;
                stat(world, player(world, "Player"), "Kills") == Some(1.0)
                    && stat(world, player(world, "Player2"), "Deaths") == Some(1.0)
            })
        },
    );
    run_until(
        &mut devices,
        2.0,
        "everyone heard of the knockout",
        no_input,
        &|d| {
            d.iter().all(|device| {
                device
                    .sim
                    .scripts
                    .chat_lines()
                    .iter()
                    .any(|line| line.text == "Player knocked out Player2")
            })
        },
    );
    run_until(&mut devices, 6.0, "Player2 respawned", no_input, &|d| {
        let world = &d[0].sim.world;
        character_of(world, player(world, "Player2")).is_some_and(|character| {
            character != target && (character_at(world, character).x - 36.0).abs() < 8.0
        })
    });

    for device in &mut devices {
        device
            .network
            .disconnect(&mut device.sim.world, &mut device.sim.scripts);
    }
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&saves);
}

/// Writes each genre template's picture, `templates/<folder>/thumbnail.png`
/// (320 × 180): a second into the game, from the game's own camera, HUD and
/// all, drawn at 1280 × 720 and shrunk. Run it after changing a template's
/// world: `cargo test -p axcenity-player write_template_pictures -- --ignored`.
#[test]
#[ignore = "writes the templates' pictures; run by hand after changing one"]
fn write_template_pictures() {
    use axcenity_render::TextureRenderer;
    let (device, queue) = super::tests::test_gpu().expect("a GPU to draw with");
    for template in Template::GAMES {
        let test = template.folder().unwrap();
        let folder = built_game(template, &format!("picture-{test}"));
        let saves = temp(test, "picture-saves");
        let (mut app, _) = App::open_game(&folder, &saves, 1.0, Start::Alone);
        for _ in 0..60 {
            let problems = app.game_frame(&[], MoveInput::default());
            assert!(problems.is_empty(), "{problems:#?}");
        }
        // The Vehicle and the Arena show more from above than from behind
        // the character.
        let overview = match template {
            Template::Vehicle => Some((Vec3::new(-34.0, 22.0, 62.0), Vec3::new(10.0, 0.0, 14.0))),
            Template::Arena => Some((Vec3::new(0.0, 34.0, 52.0), Vec3::new(0.0, 0.0, 0.0))),
            _ => None,
        };
        let camera = match overview {
            Some((from, to)) => FlyCamera::looking_at(from, to).to_render_camera(),
            None => app.shown_camera(),
        };
        let mut renderer = TextureRenderer::new(&device, &queue, VIEW.0, VIEW.1);
        // Models load the first time they're drawn, so draw twice.
        for _ in 0..2 {
            let draw_list = axcenity_scene::build_draw_list_with_ui(
                &app.sim.world,
                &app.sim.assets,
                camera,
                VIEW,
                &app.stylesheets,
            );
            renderer.render(&draw_list).unwrap();
        }
        let pixels = renderer.read_pixels();
        let (width, height) = (VIEW.0 / 4, VIEW.1 / 4);
        let mut small = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let mut sum = 0u32;
                    for dy in 0..4 {
                        for dx in 0..4 {
                            let at = (((y * 4 + dy) * VIEW.0 + x * 4 + dx) * 4 + channel) as usize;
                            sum += u32::from(pixels[at]);
                        }
                    }
                    small[((y * width + x) * 4 + channel) as usize] = (sum / 16) as u8;
                }
            }
        }
        let out = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates")
            .join(test)
            .join(axcenity_project::THUMBNAIL_FILE);
        super::tests::write_png(&out, &small, (width, height));
        let _ = fs::remove_dir_all(&folder);
        let _ = fs::remove_dir_all(&saves);
    }
}
