//! World presets in the editor (procedural worlds PW3).

use super::*;
use crate::terrain_studio::{built_in_world_presets, scatter_rules};

fn app_with_terrain(name: &str) -> (EditorApp, PathBuf, InstanceId) {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-editor-world-presets-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let folder = axcenity_project::create_project(&parent, "World Preset Test").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.terrain_action(TerrainAction::Create {
        width: 128.0,
        length: 128.0,
        height: 32.0,
        samples: 65,
    });
    let terrain = app.terrain.target.expect("the new terrain is the target");
    app.refresh_world_presets();
    (app, parent, terrain)
}

fn children(app: &EditorApp, terrain: InstanceId) -> Vec<String> {
    axcenity_project::preset_objects(&app.world, terrain)
        .iter()
        .map(|&id| app.world.get(id).unwrap().name().to_owned())
        .collect()
}

#[test]
fn six_worlds_are_built_in_and_each_sets_up_a_terrain() {
    let presets = built_in_world_presets();
    let names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Temperate Forest",
            "Alpine",
            "Desert",
            "Tropical Island",
            "Tundra",
            "Meadow"
        ]
    );
    let (mut app, folder, terrain) = app_with_terrain("built-in");
    for (index, preset) in presets.iter().enumerate() {
        app.terrain_action(TerrainAction::ApplyWorldPreset(index));
        let rules = scatter_rules(&app.world, terrain);
        assert!(!rules.is_empty(), "{} has rules", preset.name);
        assert!(
            materials::layers(&app.world, terrain).len() >= 2,
            "{} has layers",
            preset.name
        );
        let water = app.world.get(terrain).unwrap().property("Water");
        assert_eq!(
            water,
            Some(&Value::Bool(preset.water.is_some())),
            "{}",
            preset.name
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn materials_and_rules_only_keeps_the_shape_as_one_undo_step() {
    let (mut app, folder, terrain) = app_with_terrain("objects");
    let before = children(&app, terrain);
    let heights = app.world.terrain(terrain).unwrap().heights().to_vec();
    app.terrain_action(TerrainAction::ApplyWorldPreset(0));
    assert_eq!(
        children(&app, terrain),
        [
            "Grass",
            "Dirt",
            "Rock",
            "Forest Floor",
            "Lowland Forest",
            "Meadow Grass",
            "Meadow"
        ]
    );
    assert_eq!(
        app.world.terrain(terrain).unwrap().heights(),
        heights.as_slice()
    );
    assert_eq!(
        app.history.undo_label(),
        Some("Use Temperate Forest Materials and Rules")
    );
    app.undo();
    assert_eq!(children(&app, terrain), before);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn generating_with_a_preset_applies_landscape_objects_and_water_together() {
    let (mut app, folder, terrain) = app_with_terrain("generate");
    app.terrain.new_terrain.samples = 65;
    app.terrain.new_terrain.width = 128.0;
    app.terrain.new_terrain.length = 128.0;
    let before = children(&app, terrain);
    let island = 3;
    app.terrain_action(TerrainAction::GenerateWorldPreset(island));
    assert_eq!(app.terrain.generator.preset, generate::Preset::Island);
    let ctx = egui::Context::default();
    while app.terrain.job.is_some() {
        std::thread::sleep(Duration::from_millis(5));
        app.poll_terrain_job(&ctx);
    }
    assert_eq!(children(&app, terrain), before, "nothing until Apply");
    app.terrain_action(TerrainAction::ApplyPreview);
    assert!(children(&app, terrain).contains(&"Jungle".to_owned()));
    assert_eq!(
        app.world.get(terrain).unwrap().property("Water"),
        Some(&Value::Bool(true))
    );
    assert!(app.terrain.pending_world.is_none());
    assert_eq!(app.history.undo_label(), Some("Generate Tropical Island"));
    let generated = app.world.terrain(terrain).unwrap().heights().to_vec();
    app.undo();
    assert_eq!(children(&app, terrain), before);
    assert_ne!(
        app.world.terrain(terrain).unwrap().heights(),
        generated.as_slice()
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_terrain_saved_as_a_preset_is_listed_and_makes_the_same_setup() {
    let (mut app, folder, terrain) = app_with_terrain("save");
    app.terrain_action(TerrainAction::ApplyWorldPreset(1));
    let setup = children(&app, terrain);
    app.terrain_action(TerrainAction::SaveWorldPreset("My Valley".into()));
    let project = app.project.as_ref().unwrap().folder.clone();
    assert!(project.join("presets/My Valley.axworld").is_file());
    let index = app.terrain.world_preset;
    assert_eq!(app.world_presets.list[index].name, "My Valley");
    assert_eq!(app.world_presets.list.len(), 7);

    app.terrain_action(TerrainAction::ApplyWorldPreset(2));
    assert_ne!(children(&app, terrain), setup);
    app.terrain_action(TerrainAction::ApplyWorldPreset(index));
    assert_eq!(children(&app, terrain), setup);
    std::fs::remove_dir_all(folder).unwrap();
}
