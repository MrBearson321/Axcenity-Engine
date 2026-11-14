//! Biomes in the editor (procedural worlds PW2).

use super::*;
use crate::terrain_studio::biomes::{LOWLAND_FOREST, MEADOW, biomes};
use crate::terrain_studio::{scatter_rules, terrain_of};

fn app_with_terrain(name: &str) -> (EditorApp, PathBuf, InstanceId) {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-editor-biomes-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let folder = axcenity_project::create_project(&parent, "Biome Test").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.terrain_action(TerrainAction::Create {
        width: 128.0,
        length: 128.0,
        height: 32.0,
        samples: 65,
    });
    let terrain = app.terrain.target.expect("the new terrain is the target");
    (app, parent, terrain)
}

fn layer_names(app: &EditorApp, terrain: InstanceId) -> Vec<String> {
    materials::layers(&app.world, terrain)
        .iter()
        .map(|&l| app.world.get(l).unwrap().name().to_owned())
        .collect()
}

#[test]
fn a_biome_preset_brings_its_ground_layer_and_rules_as_one_step() {
    let (mut app, folder, terrain) = app_with_terrain("preset");
    let layers_before = layer_names(&app, terrain);
    app.terrain_action(TerrainAction::AddBiome(LOWLAND_FOREST));
    let biome = biomes(&app.world, terrain)[0];
    let instance = app.world.get(biome).unwrap();
    assert_eq!(instance.name(), "Lowland Forest");
    assert_eq!(
        instance.property("GroundLayer"),
        Some(&Value::String("Forest Floor".into()))
    );
    assert_eq!(app.selected, Some(biome));
    assert!(layer_names(&app, terrain).contains(&"Forest Floor".to_owned()));
    let rules = scatter_rules(&app.world, terrain);
    assert_eq!(rules.len(), 3);
    for &rule in &rules {
        let rule = app.world.get(rule).unwrap();
        assert_eq!(rule.parent(), Some(biome), "inside the biome");
        // The biome says where they go: no tree line of their own.
        assert_eq!(rule.property("MaxHeight"), Some(&Value::Number(10000.0)));
        assert_eq!(terrain_of(&app.world, rules[0]), Some(terrain));
    }
    assert_eq!(app.history.undo_label(), Some("Add Lowland Forest Biome"));

    // Lowland and meadow split the flat ground by moisture: rules place.
    app.terrain_action(TerrainAction::AddBiome(MEADOW));
    app.generate_frame();
    let placed = app.world.terrain(terrain).unwrap().generated_count();
    assert!(placed > 100, "{placed} placed");

    // A second forest reuses the Forest Floor layer.
    let layers = layer_names(&app, terrain).len();
    app.terrain_action(TerrainAction::AddBiome(LOWLAND_FOREST));
    assert_eq!(layer_names(&app, terrain).len(), layers);

    app.undo();
    app.undo();
    app.undo();
    assert!(biomes(&app.world, terrain).is_empty());
    assert_eq!(layer_names(&app, terrain), layers_before);
    app.generate_frame();
    assert_eq!(app.world.terrain(terrain).unwrap().generated_count(), 0);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_rule_added_with_a_biome_selected_goes_inside_it() {
    let (mut app, folder, terrain) = app_with_terrain("rules");
    app.terrain_action(TerrainAction::AddBiome(
        crate::terrain_studio::biomes::EMPTY_BIOME,
    ));
    let biome = biomes(&app.world, terrain)[0];
    let flowers = vec![crate::terrain_studio::scatter::MEADOW_FLOWERS];
    app.terrain_action(TerrainAction::AddScatterRules(
        flowers.clone(),
        "Add Meadow Flowers".into(),
    ));
    let inside = app.selected.unwrap();
    assert_eq!(app.world.get(inside).unwrap().parent(), Some(biome));
    // With the rule inside it selected, the next one goes in too.
    app.terrain_action(TerrainAction::AddScatterRules(
        flowers.clone(),
        "Add Meadow Flowers".into(),
    ));
    assert_eq!(
        app.world.get(app.selected.unwrap()).unwrap().parent(),
        Some(biome)
    );
    // With the terrain selected, on the terrain itself.
    app.select_from_viewport(Some(terrain));
    app.terrain_action(TerrainAction::AddScatterRules(
        flowers,
        "Add Meadow Flowers".into(),
    ));
    let outside = app.world.get(app.selected.unwrap()).unwrap();
    assert_eq!(outside.parent(), Some(terrain));
    assert_eq!(scatter_rules(&app.world, terrain).len(), 3);
    std::fs::remove_dir_all(folder).unwrap();
}
