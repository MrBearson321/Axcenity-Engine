//! The game UI settings in a built game: hello-accessibility's menu changes
//! text scale, contrast and reduced motion through GuiService, the HUD
//! changes and the 3D world doesn't, and a save slot keeps them.

use std::path::Path;

use axcenity_core::{InstanceId, UiSettings, Value};
use axcenity_render::{DrawList, UiDrawItem, UiText};

use super::tests::TempCopy;
use super::*;

/// hello-accessibility, opened from a copy, with its slots in `saves`.
fn start(saves: &Path) -> (World, ScriptRuntime, TempCopy) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-accessibility");
    let folder = TempCopy::of(
        &example,
        &format!("axcenity-player-hello-accessibility-{}", std::process::id()),
    );
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_save_folder(Some(saves.to_path_buf()));
    for script in &project.scripts {
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
    }
    (world, scripts, folder)
}

fn lines(scripts: &mut ScriptRuntime) -> Vec<String> {
    scripts
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            _ => format!("error: {}", line.text),
        })
        .collect()
}

fn text<'a>(list: &'a DrawList, shown: &str) -> &'a UiText {
    list.ui
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Text(text) if text.text == shown => Some(text),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{shown} is drawn"))
}

/// The panel's background: the first, biggest rectangle.
fn panel_alpha(list: &DrawList) -> f32 {
    list.ui
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Rectangle(r) if r.size == [300.0, 388.0] => Some(r.color[3]),
            _ => None,
        })
        .expect("the panel is drawn")
}

/// Everything the 3D scene draws, as text, to compare frames.
fn scene(list: &DrawList) -> String {
    format!(
        "{:?} {:?} {:?} {:?}",
        list.items, list.sprites, list.camera, list.lights
    )
}

#[test]
fn hello_accessibility_changes_the_menu_and_not_the_world_and_keeps_the_settings_in_a_slot() {
    let saves = std::env::temp_dir().join(format!(
        "axcenity-player-accessibility-saves-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&saves);
    let (mut world, mut scripts, folder) = start(&saves);
    let project = axcenity_project::load(&folder).unwrap();
    let stylesheets: Vec<_> = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let assets = Assets::new(&folder);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO).to_render_camera();
    let frame = |world: &World| {
        axcenity_scene::build_draw_list_with_ui(world, &assets, camera, (1280, 720), &stylesheets)
    };
    let panel = world
        .find_first_child(world.workspace(), "Settings")
        .and_then(|gui| world.find_first_child(gui, "Panel"))
        .unwrap();
    let child =
        |world: &World, name| -> InstanceId { world.find_first_child(panel, name).unwrap() };
    let title = child(&world, "Title");
    let title_fade = |world: &World| {
        world
            .get(title)
            .and_then(|t| t.property("TextTransparency"))
            .and_then(Value::as_number)
            .unwrap()
    };
    // The title fades in over 0.6 seconds.
    scripts.step(&mut world, 0.3);
    assert!(title_fade(&world) > 0.0 && title_fade(&world) < 1.0);
    for _ in 0..30 {
        scripts.step(&mut world, 1.0 / 60.0);
    }
    assert_eq!(title_fade(&world), 0.0);
    let before = frame(&world);
    assert_eq!(text(&before, "Settings").font_size, 24.0);
    assert!((panel_alpha(&before) - 0.8).abs() < 1e-3);

    // The menu's buttons change the settings, saying what they did by
    // each button's accessible name.
    for name in ["TextSizeButton", "ContrastButton", "MotionButton"] {
        let button = child(&world, name);
        scripts.activate_ui_button(&mut world, button);
    }
    assert_eq!(
        lines(&mut scripts),
        [
            "Text size: 125%",
            "Higher contrast: on",
            "Reduced motion: on"
        ]
    );
    let chosen = UiSettings {
        text_scale: 1.25,
        high_contrast: true,
        reduced_motion: true,
    };
    assert_eq!(world.ui_settings(), chosen);
    // With reduced motion the title's fade is over on the next frame.
    scripts.step(&mut world, 1.0 / 60.0);
    assert_eq!(title_fade(&world), 0.0);

    let after = frame(&world);
    let title_text = text(&after, "Settings");
    assert_eq!(title_text.font_size, 30.0);
    assert_eq!(title_text.color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(text(&after, "Reduced motion: on").font_size, 20.0);
    assert_eq!(panel_alpha(&after), 1.0);
    assert_eq!(scene(&after), scene(&before), "the 3D world is as it was");

    // Saved in slot 1 with the game's settings, and back when it loads.
    let save = child(&world, "SaveButton");
    scripts.activate_ui_button(&mut world, save);
    world.set_ui_settings(UiSettings::default());
    let load = child(&world, "LoadButton");
    scripts.activate_ui_button(&mut world, load);
    assert_eq!(
        lines(&mut scripts),
        ["Save settings: slot 1", "Load settings: slot 1"]
    );
    assert_eq!(world.ui_settings(), chosen);
    let text_size = child(&world, "TextSizeButton");
    assert_eq!(
        world.get(text_size).unwrap().property("Text"),
        Some(&Value::String("Text size: 125%".into()))
    );
    let slot = std::fs::read_to_string(saves.join("slot-1.json")).unwrap();
    assert!(slot.contains("\"UI/TextScale\": 1.25"), "{slot}");
    let _ = std::fs::remove_dir_all(&saves);
    let _ = std::fs::remove_dir_all(&folder);
}
