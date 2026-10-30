//! The Developer Console in the player (Phase 71): F9 in a built game that
//! turns it on, `timescale`, a game's registered command, and none in a
//! shipped game with it off.

use std::path::{Path, PathBuf};

use axcenity_core::Value;
use winit::keyboard::KeyCode;

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

const COMMANDS: &str = r#"
game:GetService("ConsoleService"):RegisterCommand("heal", "Heals by an amount", function(player, amount)
    print("healed", player.Name, amount)
end)
task.wait(1)
print("a second of game time")
"#;

/// `hello-part` with a script that registers `heal`, built into a
/// `game.axgame` as File > Build Game does, with the Developer Console
/// setting `on` or off. Returns the built game's path.
fn built_game(name: &str, on: bool) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-part");
    let root = std::env::temp_dir().join(format!(
        "axcenity-player-console-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    copy_folder(&example, &project);
    std::fs::write(project.join("scripts/Commands.server.lua"), COMMANDS).unwrap();
    axcenity_project::set_developer_console(&project, on).unwrap();
    let game = root.join("build").join(GAME_FILE);
    std::fs::create_dir_all(game.parent().unwrap()).unwrap();
    std::fs::write(&game, axcenity_project::pack_game(&project).unwrap()).unwrap();
    game
}

/// Opens a built game as the player does: unpacked to a folder of its
/// own, its scripts started, and the console allowed by the same rule.
fn play(game: &Path, dev_flag: bool) -> App {
    let unpacked = game.with_extension("unpacked");
    let _ = std::fs::remove_dir_all(&unpacked);
    axcenity_project::unpack_game(&std::fs::read(game).unwrap(), &unpacked).unwrap();
    let project = axcenity_project::load(&unpacked).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let allowed =
        dev_console::console_allowed(Some(game), dev_flag, project.manifest.developer_console);
    let mut world = project.world;
    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let mut app = App::new(
        world,
        scripts,
        Vec::new(),
        Assets::new(&unpacked),
        Animators::new(),
        camera,
    );
    app.console_allowed = allowed;
    app
}

impl App {
    /// Presses and lets go of `key`, as the window reports it.
    fn tap(&mut self, key: KeyCode) -> bool {
        let taken = self.console_key(Some(key), true, false, None);
        self.console_key(Some(key), false, false, None);
        taken
    }

    /// Types `line` in the console and presses Enter.
    fn type_line(&mut self, line: &str) {
        self.console_key(None, true, false, Some(line));
        self.tap(KeyCode::Enter);
    }

    /// One frame of scripts, at the console's time scale, with what they
    /// printed moved into the console.
    fn scripts_frame(&mut self) {
        let seconds = self.game_seconds(FRAME);
        self.scripts.step(&mut self.world, f64::from(seconds));
        print_output(&mut self.scripts);
        self.take_log();
    }

    fn console_said(&self, text: &str) -> bool {
        self.console.lines().any(|line| line.text == text)
    }
}

#[test]
fn in_a_built_game_with_the_console_on_f9_opens_it_timescale_slows_the_game_and_a_lua_command_runs()
{
    let game = built_game("on", true);
    let mut app = play(&game, false);
    assert!(app.console_allowed, "the project setting turns it on");

    assert!(app.tap(KeyCode::F9));
    assert!(app.console.open);
    let stats = app.console_stats();
    let items = app.console_items(app.shown_camera(), (800, 600));
    assert!(!items.is_empty(), "drawn while open: {stats:?}");

    app.type_line("timescale 0.5");
    assert!(app.console_said("> timescale 0.5"));
    assert_eq!(app.dev.time_scale, 0.5);
    assert_eq!(app.game_seconds(FRAME), FRAME * 0.5);
    // The script's `task.wait(1)` takes two real seconds at half speed.
    for _ in 0..90 {
        app.scripts_frame();
    }
    assert!(!app.console_said("a second of game time"));
    for _ in 0..40 {
        app.scripts_frame();
    }
    assert!(app.console_said("a second of game time"));

    app.type_line("heal 25");
    assert!(app.console_said("healed\tPlayer\t25"), "{:?}", {
        app.console
            .lines()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
    });
    app.type_line("help");
    assert!(
        app.console
            .lines()
            .any(|line| line.text.contains("heal") && line.text.contains("Heals by an amount")),
        "help lists the game's commands"
    );
    assert!(app.console_said("Anything else runs as Lua."));

    // Anything else is Lua, and a word no script knows is said to be unknown.
    app.type_line("workspace.Gravity = 50");
    assert_eq!(
        app.world
            .get(app.world.workspace())
            .and_then(|workspace| workspace.property("Gravity"))
            .cloned(),
        Some(Value::Number(50.0))
    );
    app.type_line("dance");
    assert!(app.console_said("Unknown command `dance`. Type help for the list."));

    // F9 closes it again, and keys reach the game once more.
    assert!(app.tap(KeyCode::F9));
    assert!(!app.console.open);
    assert!(!app.tap(KeyCode::KeyW));
    let _ = std::fs::remove_dir_all(game.parent().unwrap().parent().unwrap());
}

#[test]
fn a_shipped_game_with_the_console_off_ignores_f9_unless_started_with_dev() {
    let game = built_game("off", false);
    let mut app = play(&game, false);
    assert!(!app.console_allowed);
    assert!(!app.tap(KeyCode::F9), "F9 goes to the game");
    assert!(!app.console.open);
    app.console_key(None, true, false, Some("timescale 0.5"));
    assert_eq!(app.dev.time_scale, 1.0);
    assert!(app.console_items(app.shown_camera(), (800, 600)).is_empty());

    let mut dev = play(&game, true);
    assert!(dev.console_allowed, "--dev turns it on");
    assert!(dev.tap(KeyCode::F9) && dev.console.open);

    // A project folder is a development run, which always has it.
    let folder = game.with_extension("unpacked");
    assert!(dev_console::console_allowed(Some(&folder), false, false));
    let _ = std::fs::remove_dir_all(game.parent().unwrap().parent().unwrap());
}

#[test]
fn the_built_in_commands_change_the_game_and_bad_ones_say_why() {
    let game = built_game("built-ins", true);
    let mut app = play(&game, false);
    app.tap(KeyCode::F9);
    app.type_line("fps");
    assert!(app.dev.show_fps && app.console_said("fps on"));
    app.type_line("collisions");
    assert!(app.dev.collisions);
    app.type_line("wireframe");
    assert!(app.dev.wireframe, "kept for when the renderer exists");
    app.type_line("noclip");
    assert!(app.characters.noclip());
    // With no character, teleport moves the camera.
    app.type_line("teleport 1 20 3");
    let (position, _) = app.camera.view();
    assert_eq!(position, axcenity_core::Vector3::new(1.0, 20.0, 3.0));
    app.type_line("timescale fast");
    assert!(app.console_said("`fast` isn't a number"));
    assert_eq!(app.dev.time_scale, 1.0);
    let items = app.console_items(app.shown_camera(), (800, 600));
    let texts: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            axcenity_render::UiDrawItem::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|text| text.ends_with("FPS  0.0 ms")),
        "{texts:?}"
    );
    assert!(
        items.len() > texts.len() + 20,
        "the baseplate's collider outline is drawn"
    );
    app.type_line("clear");
    assert_eq!(app.console.lines().count(), 0);
    let _ = std::fs::remove_dir_all(game.parent().unwrap().parent().unwrap());
}

/// The open console drawn over the game, with `fps` and `collisions` on:
/// dark across the top with light text, the game below. Set
/// `AXCENITY_SNAPSHOTS` to a folder to keep the picture.
#[test]
fn the_open_console_draws_over_the_top_of_the_game() {
    let Some((device, queue)) = super::tests::test_gpu() else {
        return;
    };
    let game = built_game("drawn", true);
    let mut app = play(&game, false);
    // The first frame makes the player on this device.
    app.scripts.step_players(&mut app.world, f64::from(FRAME));
    app.tap(KeyCode::F9);
    app.type_line("fps");
    app.type_line("collisions");
    app.type_line("heal 25");
    let (width, height) = (640, 360);
    let camera = app.shown_camera();
    let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, width, height);
    let mut list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    list.ui.extend(app.console_items(camera, (width, height)));
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        super::tests::write_png(
            &Path::new(&folder).join("developer-console.png"),
            &pixels,
            (width, height),
        );
    }
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // The console's background, between the lines of text.
    let dark = at(width / 2, height / 4);
    assert!(dark.iter().all(|&c| c < 60), "{dark:?}");
    // Light text somewhere in the console's rows.
    let lit = (0..width)
        .flat_map(|x| (30..height / 2).map(move |y| (x, y)))
        .filter(|&(x, y)| at(x, y).iter().all(|&c| c > 170))
        .count();
    assert!(lit > 200, "text in the console: {lit} light pixels");
    let _ = std::fs::remove_dir_all(game.parent().unwrap().parent().unwrap());
}
