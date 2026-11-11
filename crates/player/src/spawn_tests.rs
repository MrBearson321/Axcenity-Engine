//! Phase 47: players, spawns, teams and the leaderboard, in the player's
//! frame order with physics.

use std::path::Path;

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, Color3, InstanceId, KeyCode, STARTER_CHARACTER, Value, Vector3};
use axcenity_render::UiDrawItem;
use axcenity_script::CameraState;

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;
const RED: Color3 = Color3::new(0.85, 0.2, 0.2);
const BLUE: Color3 = Color3::new(0.2, 0.4, 0.9);

/// An Instance made with these properties, in `parent`.
fn make(
    world: &mut World,
    class: ClassName,
    name: &str,
    parent: InstanceId,
    properties: Vec<(&str, Value)>,
) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    for (property, value) in properties {
        world.set_property(id, property, value).unwrap();
    }
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn v(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

/// A 120-stud floor.
fn floor(world: &mut World) {
    let workspace = world.workspace();
    make(
        world,
        ClassName::Part,
        "Floor",
        workspace,
        vec![
            ("Size", v(120.0, 1.0, 120.0)),
            ("Position", v(0.0, -0.5, 0.0)),
            ("Anchored", Value::Bool(true)),
        ],
    );
}

/// A SpawnLocation standing on the floor at (x, z).
fn spawn(world: &mut World, name: &str, x: f32, z: f32, team: Option<Color3>) -> InstanceId {
    let workspace = world.workspace();
    let mut properties = vec![("Position", v(x, 0.5, z))];
    if let Some(color) = team {
        properties.push(("Neutral", Value::Bool(false)));
        properties.push(("TeamColor", Value::Color3(color)));
    }
    make(world, ClassName::SpawnLocation, name, workspace, properties)
}

/// The player's frame, from the controllers to the Animators.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    animators: Animators,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    printed: Vec<String>,
}

impl Game {
    fn new(world: World) -> Self {
        let assets = Assets::default();
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            animators: Animators::new(),
            assets,
            characters: Controllers::default(),
            stylesheets: Vec::new(),
            printed: Vec::new(),
        }
    }

    /// hello-spawn from a copy, its scripts started as the player does.
    fn example(test: &str) -> Self {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-spawn");
        let folder = std::env::temp_dir().join(format!(
            "axcenity-player-spawn-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        copy_folder(&example, &folder);
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        axcenity_project::load_starter_character(&folder, &mut world).unwrap();
        let mut game = Self::new(world);
        for script in &project.scripts {
            game.scripts.start_script(
                &mut game.world,
                script.instance,
                &script.path,
                &script.source,
            );
        }
        game.take_printed();
        game
    }

    fn frame(&mut self, events: &[InputEvent]) {
        let seconds = f64::from(FRAME);
        self.characters
            .step(&mut self.world, &self.assets, MoveInput::default(), FRAME);
        let landings = self.characters.take_landings();
        axcenity_gameplay::fire_landed(&mut self.scripts, &mut self.world, &landings);
        axcenity_gameplay::step_humanoids(
            &mut self.scripts,
            &mut self.world,
            &self.characters,
            seconds,
        );
        self.scripts.step_players(&mut self.world, seconds);
        self.scripts.input_events(&mut self.world, events);
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        axcenity_gameplay::step_health(&mut self.scripts, &mut self.world, seconds);
        self.scripts.heartbeat(&mut self.world, seconds);
        self.animators
            .update(&mut self.world, &self.assets, seconds);
        self.take_printed();
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(&[]);
        }
    }

    /// Runs Lua on the server, as the editor's Console does.
    fn command(&mut self, source: &str) {
        self.scripts.run_command(&mut self.world, source).unwrap();
        self.take_printed();
    }

    fn take_printed(&mut self) {
        for line in self.scripts.take_output() {
            assert!(
                !matches!(line.kind, OutputKind::Error),
                "script error: {}",
                line.text
            );
            self.printed.push(line.text.replace('\t', " "));
        }
    }

    fn player(&self) -> InstanceId {
        self.scripts.local_player().expect("the player joined")
    }

    fn character(&self) -> Option<InstanceId> {
        match self.world.get(self.player())?.property("Character")? {
            Value::Instance(character) => *character,
            _ => None,
        }
    }

    /// The middle of the bottom of the character's parts' boxes.
    fn feet(&self) -> Vector3 {
        let character = self.character().expect("the player has a character");
        let parts = axcenity_character::body_parts(&self.world, character);
        assert!(!parts.is_empty());
        let mut low = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut high = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for part in parts {
            let (at, size) = (
                vector(&self.world, part, "Position"),
                vector(&self.world, part, "Size"),
            );
            low = Vector3::new(
                low.x.min(at.x - size.x / 2.0),
                low.y.min(at.y - size.y / 2.0),
                low.z.min(at.z - size.z / 2.0),
            );
            high = Vector3::new(
                high.x.max(at.x + size.x / 2.0),
                high.y.max(at.y + size.y / 2.0),
                high.z.max(at.z + size.z / 2.0),
            );
        }
        Vector3::new((low.x + high.x) / 2.0, low.y, (low.z + high.z) / 2.0)
    }

    /// The SpawnLocation the character stands on, if any.
    fn standing_on(&self) -> Option<InstanceId> {
        let feet = self.feet();
        self.world
            .descendants(self.world.workspace())
            .into_iter()
            .filter(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::SpawnLocation)
            })
            .find(|&spawn| {
                let at = vector(&self.world, spawn, "Position");
                let size = vector(&self.world, spawn, "Size");
                (feet.x - at.x).abs() <= size.x / 2.0
                    && (feet.z - at.z).abs() <= size.z / 2.0
                    && (feet.y - (at.y + size.y / 2.0)).abs() < 0.2
            })
    }

    fn leaderboard(&self) -> Vec<String> {
        if !self.scripts.leaderboard_shown() {
            return Vec::new();
        }
        axcenity_ui::leaderboard_items(&self.world, &self.stylesheets, 800, 450)
            .into_iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.text),
                _ => None,
            })
            .collect()
    }
}

fn child_of_class(world: &World, parent: InstanceId, class: ClassName) -> Option<InstanceId> {
    world
        .get(parent)?
        .children()
        .iter()
        .copied()
        .find(|&child| world.get(child).is_some_and(|c| c.class() == class))
}

#[test]
fn with_no_script_play_spawns_the_starter_character_on_a_spawn_location_and_respawns_there() {
    let mut world = World::new();
    floor(&mut world);
    let pad = spawn(&mut world, "Pad", 30.0, 10.0, None);
    world
        .set_property(pad, "Duration", Value::Number(0.5))
        .unwrap();
    let players = world.players();
    world
        .set_property(players, "RespawnTime", Value::Number(1.0))
        .unwrap();
    let mut game = Game::new(world);
    game.frame(&[]);

    // The character Insert > Character makes, named after the player, on
    // the pad, with a spawn shield.
    let first = game.character().expect("Play spawned a character");
    let world = &game.world;
    assert_eq!(world.get(first).unwrap().parent(), Some(world.workspace()));
    assert_eq!(world.get(first).unwrap().name(), "Player");
    for name in [
        "Torso",
        "Head",
        "Humanoid",
        "ThirdPersonController",
        "Health",
    ] {
        assert!(world.find_first_child(first, name).is_some(), "{name}");
    }
    assert_eq!(game.standing_on(), Some(pad));
    assert_eq!(axcenity_character::player_character(world), Some(first));
    assert!(child_of_class(world, first, ClassName::ForceField).is_some());

    // The ForceField blocks damage until its Duration is up.
    game.command("workspace.Player.Humanoid:TakeDamage(30)");
    game.frame(&[]);
    let health = game.world.find_first_child(first, "Health").unwrap();
    let health_now = |game: &Game| {
        game.world
            .get(health)
            .unwrap()
            .property("Health")
            .and_then(Value::as_number)
            .unwrap()
    };
    assert_eq!(health_now(&game), 100.0);
    game.frames(40);
    assert!(child_of_class(&game.world, first, ClassName::ForceField).is_none());
    game.command("workspace.Player.Humanoid:TakeDamage(30)");
    assert_eq!(health_now(&game), 70.0);

    // It dies, and RespawnTime later a new one stands on the pad.
    game.command(
        r#"
        local player = game.Players.Player
        player.CharacterRemoving:Connect(function(c) print("removing", c.Name, c.Parent) end)
        player.CharacterAdded:Connect(function(c) print("added", c.Name, c.Parent.Name) end)
        player.Character.Humanoid.Health = 0
        "#,
    );
    game.frames(30);
    assert_eq!(game.character(), Some(first), "not before RespawnTime");
    game.frames(40);
    let second = game.character().expect("it respawned");
    assert_ne!(second, first);
    assert!(!game.world.contains(first), "the old one is destroyed");
    assert_eq!(game.standing_on(), Some(pad));
    assert_eq!(
        game.printed,
        ["removing Player nil", "added Player Workspace"]
    );
}

#[test]
fn a_starter_character_model_in_starter_player_is_what_spawns() {
    let mut world = World::new();
    floor(&mut world);
    let pad = spawn(&mut world, "Pad", -12.0, 4.0, None);
    // As hosts put `StarterPlayer.StarterCharacter`'s prefab.
    let starter = world.starter_player();
    let model = make(
        &mut world,
        ClassName::Model,
        STARTER_CHARACTER,
        starter,
        Vec::new(),
    );
    make(
        &mut world,
        ClassName::Part,
        "Robot",
        model,
        vec![
            ("Size", v(3.0, 6.0, 3.0)),
            ("Position", v(50.0, 20.0, 50.0)),
        ],
    );
    make(
        &mut world,
        ClassName::ThirdPersonController,
        "ThirdPersonController",
        model,
        Vec::new(),
    );
    let mut game = Game::new(world);
    game.frame(&[]);
    let character = game.character().unwrap();
    assert!(game.world.find_first_child(character, "Robot").is_some());
    assert_eq!(game.standing_on(), Some(pad));
    // The original stays in StarterPlayer for the next spawn.
    assert!(game.world.contains(model));
}

#[test]
fn a_character_placed_in_the_workspace_becomes_the_players_and_respawns_as_it_was() {
    let mut world = World::new();
    floor(&mut world);
    let workspace = world.workspace();
    let hero = make(&mut world, ClassName::Model, "Hero", workspace, Vec::new());
    make(
        &mut world,
        ClassName::Part,
        "Body",
        hero,
        vec![("Size", v(2.0, 5.0, 1.0)), ("Position", v(7.0, 2.5, -3.0))],
    );
    make(
        &mut world,
        ClassName::Humanoid,
        "Humanoid",
        hero,
        Vec::new(),
    );
    let players = world.players();
    world
        .set_property(players, "RespawnTime", Value::Number(0.5))
        .unwrap();
    let mut game = Game::new(world);
    game.frame(&[]);
    assert_eq!(game.character(), Some(hero), "no second character");
    let characters = game
        .world
        .descendants(game.world.workspace())
        .into_iter()
        .filter(|&id| {
            game.world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::Humanoid)
        })
        .count();
    assert_eq!(characters, 1);

    game.command("workspace.Hero.Humanoid.Health = 0");
    game.frames(40);
    let again = game.character().unwrap();
    assert_ne!(again, hero);
    assert_eq!(game.world.get(again).unwrap().name(), "Player");
    let feet = game.feet();
    assert!(
        (feet.x - 7.0).abs() < 0.01 && (feet.z + 3.0).abs() < 0.01,
        "{feet:?}"
    );
}

#[test]
fn a_player_on_the_red_team_spawns_only_on_red_spawn_locations() {
    let mut world = World::new();
    floor(&mut world);
    let teams = world.teams();
    let red = make(
        &mut world,
        ClassName::Team,
        "Red",
        teams,
        vec![("TeamColor", Value::Color3(RED))],
    );
    make(
        &mut world,
        ClassName::Team,
        "Blue",
        teams,
        vec![
            ("TeamColor", Value::Color3(BLUE)),
            ("AutoAssignable", Value::Bool(false)),
        ],
    );
    let reds: Vec<InstanceId> = (0..3)
        .map(|i| spawn(&mut world, "Red", -40.0, -30.0 + 30.0 * i as f32, Some(RED)))
        .collect();
    for i in 0..3 {
        spawn(
            &mut world,
            "Blue",
            40.0,
            -30.0 + 30.0 * i as f32,
            Some(BLUE),
        );
    }
    spawn(&mut world, "Anyone", 0.0, 0.0, None);
    let mut game = Game::new(world);
    game.frame(&[]);

    // Joining put it on Red, the only AutoAssignable team.
    let player = game.player();
    let get = |game: &Game, name: &str| game.world.get(player).unwrap().property(name).cloned();
    assert_eq!(get(&game, "Team"), Some(Value::Instance(Some(red))));
    assert_eq!(get(&game, "Neutral"), Some(Value::Bool(false)));
    game.command(
        "local red = game.Teams.Red print(#red:GetPlayers(), red:GetPlayers()[1] == game.Players.Player)",
    );
    assert_eq!(game.printed.last().unwrap(), "1 true");

    let mut used = Vec::new();
    for _ in 0..40 {
        let on = game.standing_on().expect("on a SpawnLocation");
        assert!(reds.contains(&on), "a red player spawned off a red pad");
        used.push(on);
        game.command("game.Players.Player:LoadCharacter()");
        game.frame(&[]);
    }
    // Chosen at random among them.
    used.sort();
    used.dedup();
    assert!(used.len() > 1, "always the same pad");

    // A neutral player spawns only on the Neutral pad.
    game.command("game.Players.Player.Team = nil game.Players.Player:LoadCharacter()");
    assert_eq!(get(&game, "Neutral"), Some(Value::Bool(true)));
    let anyone = game.standing_on().unwrap();
    assert_eq!(game.world.get(anyone).unwrap().name(), "Anyone");
}

#[test]
fn with_no_spawn_location_the_character_is_at_the_origin_with_a_warning() {
    let mut world = World::new();
    floor(&mut world);
    let mut game = Game::new(world);
    game.frame(&[]);
    let feet = game.feet();
    assert!(feet.x.abs() < 0.01 && feet.y.abs() < 0.01 && feet.z.abs() < 0.01);
    assert!(
        game.printed
            .iter()
            .any(|line| line.contains("no enabled SpawnLocation")),
        "{:?}",
        game.printed
    );
}

#[test]
fn coins_in_leaderstats_show_on_the_leaderboard_and_update() {
    let mut game = Game::example("coins");
    game.frame(&[]);
    assert_eq!(
        game.printed,
        ["Player joined, on the Red team", "Player spawned"]
    );
    let on = game.standing_on().unwrap();
    assert_eq!(game.world.get(on).unwrap().name(), "RedSpawn");
    assert_eq!(game.leaderboard(), ["Player", "Coins", "Player", "0"]);

    // The script adds a coin when the character reaches the coin.
    let character = game.character().unwrap();
    let coin = game
        .world
        .find_first_child(game.world.workspace(), "Coin")
        .unwrap();
    let at = vector(&game.world, coin, "Position");
    let feet = game.feet();
    for part in axcenity_character::body_parts(&game.world, character) {
        let p = vector(&game.world, part, "Position");
        game.world
            .set_property(
                part,
                "Position",
                v(p.x + at.x - feet.x, p.y, p.z + at.z - feet.z),
            )
            .unwrap();
    }
    game.frames(2);
    assert_eq!(game.leaderboard(), ["Player", "Coins", "Player", "1"]);

    // Tab hides it, and shows it again.
    let tab = |down: bool| {
        InputEvent::new(InputKind::Key {
            key: KeyCode::Tab,
            pressed: down,
        })
    };
    game.frame(&[tab(true)]);
    game.frame(&[tab(false)]);
    assert!(game.leaderboard().is_empty());
    game.frame(&[tab(true)]);
    game.frame(&[tab(false)]);
    assert_eq!(game.leaderboard().len(), 4);
}

/// hello-spawn after spawning, from the character's camera with the
/// leaderboard over it: saved as `spawn-leaderboard.png` when
/// `AXCENITY_SNAPSHOTS` names a folder.
#[test]
fn hello_spawn_draws_the_leaderboard_over_the_game() {
    use axcenity_render::{TextureRenderer, wgpu};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let mut game = Game::example("snapshot");
    game.frames(30);
    let view = game
        .characters
        .view(&game.world)
        .expect("a character camera");
    let camera = game
        .scripts
        .step_camera(&game.world, 0.0)
        .view(CameraState {
            position: view.position,
            orientation: view.orientation,
        })
        .render_camera();
    let (width, height) = (640, 360);
    let mut draw_list = axcenity_scene::build_draw_list_with_ui(
        &game.world,
        &game.assets,
        camera,
        (width, height),
        &game.stylesheets,
    );
    let board = axcenity_ui::leaderboard_items(&game.world, &game.stylesheets, width, height);
    assert!(!board.is_empty());
    draw_list.ui.extend(board);
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    renderer.render(&draw_list).unwrap();
    let pixels = renderer.read_pixels();
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        let file = fs::File::create(folder.join("spawn-leaderboard.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    // Inside the board's top right, the translucent dark panel darkens
    // the sky beside it.
    let brightness = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        pixels[i..i + 3].iter().map(|&c| u32::from(c)).sum::<u32>()
    };
    let (panel, sky) = (brightness(width - 24, 24), brightness(width - 6, 24));
    assert!(panel + 150 < sky, "panel {panel}, sky {sky}");
}
