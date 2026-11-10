//! Phase 48: levels, through the hello-levels example in the player's
//! frame order with physics: portals between levels, a level added
//! beside the current one (in a built game too), and a slot that goes
//! back to the level it was saved in.

use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, Value, Vector3};

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

/// hello-levels, copied into a folder of its own (or `packed` from a built
/// game), its levels handed over and its scripts started as the player
/// does, saving to `saves`.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    printed: Vec<String>,
}

fn example_copy(test: &str) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-levels");
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-levels-{test}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    copy_folder(&example, &folder);
    folder
}

fn save_folder(test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-level-saves-{test}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    folder
}

impl Game {
    fn start(folder: &Path, saves: &Path) -> Self {
        let project = axcenity_project::load(folder).unwrap();
        let mut problems = project.problems;
        let start = project.manifest.start_level.clone();
        let levels = game_levels(folder, &start, &project.world, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        let mut world = project.world;
        let assets = Assets::new(folder);
        let mut scripts = ScriptRuntime::new();
        scripts.set_assets(assets.clone());
        scripts.set_save_folder(Some(saves.to_owned()));
        scripts.set_levels(&start, &start, levels);
        for script in &project.scripts {
            scripts.start_script(&mut world, script.instance, &script.path, &script.source);
        }
        let mut game = Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            assets,
            characters: Controllers::default(),
            printed: Vec::new(),
        };
        game.take_printed();
        game
    }

    fn frame(&mut self) {
        let seconds = f64::from(FRAME);
        self.characters
            .step(&mut self.world, &self.assets, MoveInput::default(), FRAME);
        let landings = self.characters.take_landings();
        self.scripts.fire_landed(&mut self.world, &landings);
        self.scripts
            .step_humanoids(&mut self.world, &self.characters, seconds);
        self.scripts.step_players(&mut self.world, seconds);
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        let touches = self.physics.take_touches();
        self.scripts.fire_touched(&mut self.world, &touches);
        self.scripts.heartbeat(&mut self.world, seconds);
        self.take_printed();
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame();
        }
    }

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
            self.printed.push(line.text);
        }
    }

    fn find(&self, path: &[&str]) -> Option<InstanceId> {
        let mut at = self.world.workspace();
        for name in path {
            at = self.world.find_first_child(at, name)?;
        }
        Some(at)
    }

    fn character(&self) -> InstanceId {
        let player = self.scripts.local_player().expect("the player joined");
        match self.world.get(player).unwrap().property("Character") {
            Some(Value::Instance(Some(character))) => *character,
            _ => panic!("the player has no character"),
        }
    }

    /// The middle of the bottom of the character's parts.
    fn feet(&self) -> Vector3 {
        let parts = axcenity_character::body_parts(&self.world, self.character());
        let (mut low, mut sum, mut count) = (f32::INFINITY, (0.0, 0.0), 0.0);
        for part in parts {
            let instance = self.world.get(part).unwrap();
            let at = instance.property("Position").unwrap().as_vector3().unwrap();
            let size = instance.property("Size").unwrap().as_vector3().unwrap();
            low = low.min(at.y - size.y / 2.0);
            sum = (sum.0 + at.x, sum.1 + at.z);
            count += 1.0;
        }
        Vector3::new(sum.0 / count, low, sum.1 / count)
    }

    /// Walks the character into the level's portal, until a level loads.
    fn take_portal(&mut self) {
        self.command(
            "local portal = workspace.Portal\n\
                      game.Players:GetPlayers()[1].Character:PivotTo(CFrame.new(portal.Position))",
        );
        self.frames(3);
    }

    fn current_level(&self) -> String {
        self.scripts.levels().current().to_owned()
    }
}

fn near(a: Vector3, b: Vector3) -> bool {
    (a.x - b.x).abs() < 1.5 && (a.y - b.y).abs() < 1.5 && (a.z - b.z).abs() < 1.5
}

#[test]
fn a_portal_loads_the_castle_at_its_gate_keeping_replicated_storage_and_player_data() {
    let folder = example_copy("portal");
    let saves = save_folder("portal");
    let mut game = Game::start(&folder, &saves);
    game.frames(5);
    assert_eq!(game.current_level(), "Meadow");
    assert!(
        near(game.feet(), Vector3::new(0.0, 1.0, 10.0)),
        "{:?}",
        game.feet()
    );
    let shared = game
        .world
        .find_first_child(game.world.replicated_storage(), "Shared")
        .unwrap();
    let character = game.character();

    game.take_portal();
    assert_eq!(game.current_level(), "Castle");
    assert!(game.find(&["Tower"]).is_some());
    assert!(game.find(&["Lever"]).is_none(), "the Meadow is gone");
    // The same character stands on the Gate, not the Courtyard.
    assert_eq!(game.character(), character);
    assert!(
        near(game.feet(), Vector3::new(30.0, 1.0, 30.0)),
        "{:?}",
        game.feet()
    );
    // ReplicatedStorage's Folder is the same Instance, with its count.
    assert_eq!(
        game.world
            .find_first_child(game.world.replicated_storage(), "Shared"),
        Some(shared)
    );
    let taken = game.world.find_first_child(shared, "PortalsTaken").unwrap();
    assert_eq!(
        game.world.get(taken).unwrap().property("Value"),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        game.scripts.saves().player_data.get("Coins"),
        Some(&Value::Number(3.0))
    );
    assert!(
        game.printed
            .contains(&"Arrived in Castle with 3 coins".to_owned()),
        "{:?}",
        game.printed
    );
    // The Menu ScreenGui is still there.
    assert!(game.find(&["Menu"]).is_some());

    // And back Home through the Castle's portal.
    game.frames(10);
    game.take_portal();
    assert_eq!(game.current_level(), "Meadow");
    assert!(game.find(&["Lever"]).is_some());
    assert!(
        near(game.feet(), Vector3::new(0.0, 1.0, 10.0)),
        "{:?}",
        game.feet()
    );
}

/// Opens and closes the Dungeon beside the Meadow.
fn add_and_remove_the_dungeon(game: &mut Game) {
    game.frames(3);
    game.command("print(game.LevelService:AddLevel('Dungeon'):GetFullName())");
    let dungeon = game.find(&["Dungeon"]).expect("added");
    assert_eq!(game.world.get(dungeon).unwrap().class(), ClassName::Model);
    assert!(game.find(&["Dungeon", "Cell"]).is_some());
    assert!(game.find(&["Lever"]).is_some(), "the Meadow stays");
    game.frames(3);
    // Physics stands things on its stairs like any other part.
    let stairs = game.find(&["Dungeon", "Stairs"]).unwrap();
    assert!(game.physics.has_body(stairs));
    game.command("game.LevelService:RemoveLevel('Dungeon')");
    game.frames(3);
    assert!(game.find(&["Dungeon"]).is_none());
    assert!(game.find(&["Lever"]).is_some());
    assert_eq!(game.current_level(), "Meadow");
    assert!(game.printed.contains(&"Workspace.Dungeon".to_owned()));
}

#[test]
fn add_level_streams_the_dungeon_in_beside_the_meadow_and_remove_level_takes_it_away() {
    let folder = example_copy("dungeon");
    let mut game = Game::start(&folder, &save_folder("dungeon"));
    add_and_remove_the_dungeon(&mut game);
}

#[test]
fn a_built_game_has_every_level_and_adds_the_dungeon_too() {
    let folder = example_copy("built");
    let packed = axcenity_project::pack_game(&folder).unwrap();
    let unpacked = std::env::temp_dir().join(format!(
        "axcenity-player-levels-unpacked-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&unpacked);
    axcenity_project::unpack_game(&packed, &unpacked).unwrap();
    assert_eq!(
        axcenity_project::level_names(&unpacked),
        ["Castle", "Dungeon"]
    );
    let mut game = Game::start(&unpacked, &save_folder("built"));
    add_and_remove_the_dungeon(&mut game);
    game.take_portal();
    assert_eq!(game.current_level(), "Castle");
}

#[test]
fn continue_on_the_main_menu_goes_back_to_the_level_the_slot_was_saved_in() {
    let saves = save_folder("continue");
    let folder = example_copy("continue");
    {
        let mut game = Game::start(&folder, &saves);
        game.frames(5);
        // Arriving in the Castle saves slot 1.
        game.take_portal();
        assert_eq!(game.current_level(), "Castle");
        game.command("workspace.Tower.Transparency = 0.25");
        game.command("game.SaveService:SaveSlot(1)");
    }

    // The next run starts in the Meadow, at its menu.
    let mut game = Game::start(&folder, &saves);
    game.frames(5);
    assert_eq!(game.current_level(), "Meadow");
    let menu = game.find(&["Menu"]).unwrap();
    let continue_button = game.find(&["Menu", "Panel", "Continue"]).unwrap();
    assert_eq!(
        game.world.get(continue_button).unwrap().property("Visible"),
        Some(&Value::Bool(true))
    );
    game.scripts
        .activate_ui_button(&mut game.world, continue_button);
    game.frames(2);
    assert_eq!(game.current_level(), "Castle");
    let tower = game.find(&["Tower"]).expect("back in the Castle");
    assert_eq!(
        game.world.get(tower).unwrap().property("Transparency"),
        Some(&Value::Number(0.25))
    );
    assert!(game.find(&["Lever"]).is_none());
    assert!(
        near(game.feet(), Vector3::new(30.0, 1.0, 30.0)),
        "{:?}",
        game.feet()
    );
    assert_eq!(
        game.world.get(menu).unwrap().property("Enabled"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn a_level_load_shows_the_games_loading_screen_again() {
    let folder = example_copy("loading");
    let mut game = Game::start(&folder, &save_folder("loading"));
    game.frames(3);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let mut app = App::new(
        game.world,
        game.scripts,
        Vec::new(),
        game.assets,
        Animators::new(),
        camera,
    );
    app.loading_screen = axcenity_project::LoadingScreen {
        screen_gui: "Menu".to_owned(),
        min_seconds: 1.0,
        wait_for_script: false,
    };
    app.follow_level_loads();
    assert!(app.loading_gui.is_none(), "nothing was loaded yet");
    app.sim
        .scripts
        .run_command(&mut app.sim.world, "game.LevelService:LoadLevel('Castle')")
        .unwrap();
    app.sim.scripts.fire_changes(&mut app.sim.world);
    app.follow_level_loads();
    let menu = app
        .sim
        .world
        .find_first_child(app.sim.world.workspace(), "Menu");
    assert_eq!(app.loading_gui.as_ref().and_then(LoadingGui::on_top), menu);
}

#[test]
fn a_project_with_only_world_json_has_one_level_and_plays_as_before() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-spawn");
    let project = axcenity_project::load(&example).unwrap();
    assert_eq!(
        project.manifest.start_level,
        axcenity_project::DEFAULT_START_LEVEL
    );
    let mut problems = Vec::new();
    let levels = game_levels(&example, "Start", &project.world, &mut problems);
    assert!(problems.is_empty());
    assert_eq!(levels.keys().collect::<Vec<_>>(), ["Start"]);
    let mut scripts = ScriptRuntime::new();
    scripts.set_levels("Start", "Start", levels);
    let mut world = project.world;
    scripts
        .run_command(&mut world, "print(game.LevelService.CurrentLevel)")
        .unwrap();
    let printed: Vec<_> = scripts.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(printed, ["Start"]);
}
