//! Scatter rules in the editor (procedural worlds PW1).

use super::*;
use crate::terrain_studio::{RulePreset, scatter_rules};

fn app_with_terrain(name: &str) -> (EditorApp, PathBuf, InstanceId) {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-editor-scatter-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let folder = axcenity_project::create_project(&parent, "Scatter Test").unwrap();
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

fn placed(app: &EditorApp, terrain: InstanceId) -> usize {
    app.world.terrain(terrain).unwrap().generated_count()
}

#[test]
fn a_forest_preset_adds_three_rules_as_one_step_and_they_place_at_once() {
    let (mut app, folder, terrain) = app_with_terrain("forest");
    app.terrain_action(TerrainAction::AddScatterRules(
        crate::terrain_studio::scatter::FOREST_SET.to_vec(),
        "Add Forest Rules".into(),
    ));
    let rules = scatter_rules(&app.world, terrain);
    let names: Vec<String> = rules
        .iter()
        .map(|&r| app.world.get(r).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, ["Forest Trees", "Undergrowth", "Grass Tufts"]);
    // The last one is selected, so Assets clicks go into it.
    assert_eq!(app.selected, rules.last().copied());
    let trees = placed(&app, terrain);
    assert!(trees > 100, "{trees} placed");
    // What they place isn't an edit: the terrain needn't be saved for it.
    assert_eq!(app.history.undo_label(), Some("Add Forest Rules"));

    app.undo();
    assert!(scatter_rules(&app.world, terrain).is_empty());
    app.generate_frame();
    assert_eq!(placed(&app, terrain), 0);
    app.redo();
    app.generate_frame();
    assert_eq!(placed(&app, terrain), trees, "the same again");
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn clicking_props_and_models_in_assets_fills_the_selected_rule() {
    let (mut app, folder, terrain) = app_with_terrain("pick");
    let empty = RulePreset {
        name: "Rocks",
        ..crate::terrain_studio::scatter::YOUR_MODELS
    };
    app.terrain_action(TerrainAction::AddScatterRules(
        vec![empty],
        "Add Rocks".into(),
    ));
    let rule = scatter_rules(&app.world, terrain)[0];
    let (picked, entries) = app.scatter_rule_picking().expect("a rule is selected");
    assert_eq!((picked, entries.len()), (rule, 0));
    assert_eq!(placed(&app, terrain), 0, "nothing to place yet");

    let crate_prop = axcenity_core::FoliageSource::Prop(axcenity_core::FoliageProp::Crate);
    app.pick_scatter_model(rule, &crate_prop);
    let model = axcenity_core::FoliageSource::Model("assets/models/Rock.glb".into());
    app.pick_scatter_model(rule, &model);
    let models = |app: &EditorApp| {
        app.world
            .get(rule)
            .unwrap()
            .property("Models")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    assert_eq!(models(&app), "prop:Crate; assets/models/Rock.glb");
    assert!(placed(&app, terrain) > 0);
    // Clicked again, it's taken out.
    app.pick_scatter_model(rule, &crate_prop);
    assert_eq!(models(&app), "assets/models/Rock.glb");
    app.undo();
    assert_eq!(models(&app), "prop:Crate; assets/models/Rock.glb");
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn rules_save_without_what_they_place_and_bake_keeps_it_as_paint() {
    let (mut app, folder, terrain) = app_with_terrain("bake");
    app.terrain_action(TerrainAction::AddScatterRules(
        vec![crate::terrain_studio::scatter::FOREST_TREES],
        "Add Forest Trees".into(),
    ));
    let trees = placed(&app, terrain);
    assert!(trees > 50);

    // Saved and opened again: the rule comes back and places the same,
    // but the files hold none of it.
    assert!(app.save());
    let project = app.project.as_ref().unwrap().folder.clone();
    let foliage = project.join("assets/terrain/Terrain/foliage.json");
    assert!(!foliage.exists(), "nothing painted, so no foliage file");
    let mut reopened = EditorApp::new(Some(&project));
    let id = crate::terrain_studio::terrains(&reopened.world)[0];
    reopened.generate_frame();
    assert_eq!(scatter_rules(&reopened.world, id).len(), 1);
    assert_eq!(reopened.world.terrain(id).unwrap().generated_count(), trees);

    // Bake: painted foliage now, the rule off, one undo step.
    app.terrain_action(TerrainAction::BakeScatter);
    app.generate_frame();
    let data = app.world.terrain(terrain).unwrap();
    assert_eq!(data.foliage().len(), trees);
    assert_eq!(data.generated_count(), 0);
    let rule = scatter_rules(&app.world, terrain)[0];
    assert_eq!(
        app.world.get(rule).unwrap().property("Enabled"),
        Some(&Value::Bool(false))
    );
    app.undo();
    app.generate_frame();
    let data = app.world.terrain(terrain).unwrap();
    assert_eq!((data.foliage().len(), data.generated_count()), (0, trees));
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn cliff_rocks_stand_only_on_the_steep_side_and_boulders_off_it() {
    let (mut app, folder, terrain) = app_with_terrain("cliff");
    // A step across the middle: flat, 16 studs climbing 32, flat again.
    let data = app.world.terrain_mut(terrain).unwrap();
    let (size_x, _) = data.size();
    let all = data.bounds();
    for (i, height) in data.heights_mut_in(all).iter_mut().enumerate() {
        let x = (i % size_x) as f32;
        *height = ((x - 28.0) / 8.0).clamp(0.0, 1.0);
    }
    app.terrain_action(TerrainAction::AddScatterRules(
        vec![
            crate::terrain_studio::scatter::CLIFF_ROCKS,
            crate::terrain_studio::scatter::BOULDERS,
        ],
        "Add Rocks".into(),
    ));
    let data = app.world.terrain(terrain).unwrap();
    let is = |prop| {
        move |i: &&axcenity_core::FoliageInstance| {
            i.source == axcenity_core::FoliageSource::Prop(prop)
        }
    };
    let cliffs: Vec<_> = data
        .generated_foliage()
        .filter(is(axcenity_core::FoliageProp::Cliff))
        .collect();
    assert!(cliffs.len() > 10, "{} cliff rocks", cliffs.len());
    for rock in &cliffs {
        assert!((27.0..=37.0).contains(&rock.x), "{rock:?}");
        assert!(rock.collide && rock.sink > 1.0 && rock.lean > 0.0);
    }
    // Flat ground is too gentle for boulders, and the cliff too steep.
    assert_eq!(
        data.generated_foliage()
            .filter(is(axcenity_core::FoliageProp::Rock))
            .count(),
        0
    );
    std::fs::remove_dir_all(folder).unwrap();
}
