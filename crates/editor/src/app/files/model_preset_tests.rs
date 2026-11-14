//! Model import presets (P7.3).

use std::fs;

use super::super::tests::{copy_of_example, printed};
use super::tests::{example_model, position, write_props};
use super::*;
use axcenity_assets::{BringIn, ImportCollision, ModelSettings, UpAxis};

fn property<'a>(app: &'a EditorApp, id: InstanceId, name: &str) -> &'a Value {
    app.world.get(id).unwrap().property(name).unwrap()
}

/// A folder of this test's own beside the project, for files imported
/// from outside it.
fn outside(folder: &std::path::Path) -> std::path::PathBuf {
    let outside = folder.with_extension("outside");
    fs::create_dir_all(&outside).unwrap();
    outside
}

fn mesh_parts(app: &EditorApp) -> Vec<InstanceId> {
    app.world
        .descendants(app.world.workspace())
        .into_iter()
        .filter(|&id| app.world.get(id).unwrap().class() == ClassName::MeshPart)
        .collect()
}

#[test]
fn the_dialog_starts_with_default_and_a_preset_sets_its_choices() {
    let folder = copy_of_example("hello-part", "model-preset-pick");
    let mut app = EditorApp::new(Some(&folder));
    let props = write_props(&outside(&folder));
    app.import_model(&props);
    let mut dialog = app.importing.take().expect("several objects ask first");
    assert_eq!(dialog.preset, "Default");
    assert_eq!(
        dialog
            .presets
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Default", "Static prop", "Building kit", "Character"]
    );
    assert_eq!(dialog.placement, Placement::Grouped);
    assert!(dialog.matches_preset());

    let kit = dialog.presets[2].1;
    dialog.apply_preset("Building kit", kit);
    assert_eq!(dialog.placement, Placement::Grouped);
    assert!(dialog.weld);
    assert!(dialog.settings.anchored);
    assert!(dialog.matches_preset());
    let prop = dialog.presets[1].1;
    dialog.apply_preset("Static prop", prop);
    assert_eq!(dialog.placement, Placement::Merged);
    assert!(!dialog.weld);
    // Changing a choice by hand shows it isn't the preset any more.
    dialog.placement = Placement::Separate;
    assert!(!dialog.matches_preset());
    assert_eq!(dialog.current_settings().bring_in, BringIn::Separate);
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_static_prop_import_is_anchored_with_hull_collision_and_remembered() {
    let folder = copy_of_example("hello-part", "model-preset-prop");
    let mut app = EditorApp::new(Some(&folder));
    let props = write_props(&outside(&folder));
    app.import_model(&props);
    let mut dialog = app.importing.take().unwrap();
    let prop = dialog.presets[1].1;
    dialog.apply_preset("Static prop", prop);
    app.import_chosen(&dialog);
    let parts = mesh_parts(&app);
    let new = *parts.last().unwrap();
    assert_eq!(app.world.get(new).unwrap().name(), "props");
    assert_eq!(property(&app, new, "Anchored"), &Value::Bool(true));
    assert!(matches!(
        property(&app, new, "CollisionFidelity"),
        Value::EnumItem(item) if item.item == "Hull"
    ));
    assert_eq!(
        axcenity_assets::last_model_preset(&folder).as_deref(),
        Some("Static prop")
    );

    // The next import starts with it.
    app.import_model(&props);
    let dialog = app.importing.take().unwrap();
    assert_eq!(dialog.preset, "Static prop");
    assert_eq!(dialog.placement, Placement::Merged);
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn scale_and_z_up_shape_every_part_about_the_insert_point() {
    let folder = copy_of_example("hello-part", "model-preset-shape");
    let mut app = EditorApp::new(Some(&folder));
    let props = write_props(&outside(&folder));
    app.import_model(&props);
    let mut dialog = app.importing.take().unwrap();
    dialog.placement = Placement::Grouped;
    let plain_origin = app.insert_position();
    app.import_chosen(&dialog);
    let plain: Vec<(String, Vector3, Vector3)> = mesh_parts(&app)
        .into_iter()
        .rev()
        .take(3)
        .map(|id| {
            let Value::Vector3(size) = property(&app, id, "Size").clone() else {
                panic!()
            };
            (
                app.world.get(id).unwrap().name().to_owned(),
                position(&app, id),
                size,
            )
        })
        .collect();
    app.undo();

    app.import_model(&props);
    let mut dialog = app.importing.take().unwrap();
    dialog.placement = Placement::Grouped;
    dialog.settings = ModelSettings {
        scale: 0.5,
        up_axis: UpAxis::Z,
        collision: ImportCollision::Box,
        ..ModelSettings::default()
    };
    let origin = app.insert_position();
    assert_eq!(origin, plain_origin);
    app.import_chosen(&dialog);
    let shaped: Vec<InstanceId> = mesh_parts(&app).into_iter().rev().take(3).collect();
    for (id, (name, at, size)) in shaped.iter().zip(&plain) {
        assert_eq!(app.world.get(*id).unwrap().name(), name);
        let Value::Vector3(new_size) = property(&app, *id, "Size").clone() else {
            panic!()
        };
        // Half the size, in the part's own axes.
        assert!((new_size.x - size.x * 0.5).abs() < 1e-4, "{name}");
        assert!((new_size.y - size.y * 0.5).abs() < 1e-4, "{name}");
        // Half as far from the insert point, and Z up: the file's -Z (Small,
        // 10 along -Z) ends up below, its Y forward.
        let offset = [at.x - origin.x, at.y - origin.y, at.z - origin.z];
        let new = position(&app, *id);
        let expected = [
            origin.x + offset[0] * 0.5,
            origin.y + offset[2] * 0.5,
            origin.z - offset[1] * 0.5,
        ];
        assert!(
            (new.x - expected[0]).abs() < 1e-3
                && (new.y - expected[1]).abs() < 1e-3
                && (new.z - expected[2]).abs() < 1e-3,
            "{name}: {new:?} != {expected:?}"
        );
        assert!(matches!(
            property(&app, *id, "CollisionFidelity"),
            Value::EnumItem(item) if item.item == "Box"
        ));
    }
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn save_as_preset_writes_the_project_file_as_one_undo_step() {
    let folder = copy_of_example("hello-part", "model-preset-save");
    let mut app = EditorApp::new(Some(&folder));
    let file = folder.join(axcenity_assets::PRESETS_FILE);
    let _ = fs::remove_file(&file);
    let settings = ModelSettings {
        bring_in: BringIn::Separate,
        scale: 0.01,
        ..ModelSettings::default()
    };
    app.save_model_preset("Centimetre props", settings);
    let presets = axcenity_assets::ImportPresets::read(&folder).unwrap();
    assert_eq!(presets.model_settings("Centimetre props"), Some(settings));
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.contains("Saved the import preset Centimetre props"))
    );
    assert_eq!(
        app.history.undo_label(),
        Some("Save import preset Centimetre props")
    );
    // It's offered next time.
    let props = write_props(&outside(&folder));
    app.import_model(&props);
    let dialog = app.importing.take().unwrap();
    assert!(
        dialog
            .presets
            .iter()
            .any(|(name, _)| name == "Centimetre props")
    );
    app.undo();
    assert!(!file.exists());
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_model_with_nothing_to_ask_still_gets_the_last_preset() {
    let folder = copy_of_example("hello-part", "model-preset-direct");
    let mut app = EditorApp::new(Some(&folder));
    axcenity_assets::remember_model_preset(&folder, "Static prop").unwrap();
    // One object, no maps: no dialog.
    let text = fs::read_to_string(example_model()).unwrap();
    let mut gltf: serde_json::Value = serde_json::from_str(&text).unwrap();
    gltf.as_object_mut().unwrap().remove("images");
    gltf.as_object_mut().unwrap().remove("textures");
    gltf.as_object_mut().unwrap().remove("samplers");
    for material in gltf["materials"].as_array_mut().into_iter().flatten() {
        material["pbrMetallicRoughness"]
            .as_object_mut()
            .map(|pbr| pbr.remove("baseColorTexture"));
    }
    let plain = outside(&folder).join("model-preset-plain.gltf");
    fs::write(&plain, gltf.to_string()).unwrap();
    app.import_model(&plain);
    assert!(app.importing.is_none(), "{:?}", printed(&app));
    let new = *mesh_parts(&app).last().unwrap();
    assert_eq!(property(&app, new, "Anchored"), &Value::Bool(true));
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn the_preset_row_and_settings_tab_draw() {
    let folder = copy_of_example("hello-part", "model-preset-draw");
    let mut app = EditorApp::new(Some(&folder));
    let props = write_props(&outside(&folder));
    app.import_model(&props);
    let mut dialog = app.importing.take().unwrap();
    dialog.tab = crate::import_dialog::Tab::Settings;
    assert_eq!(dialog.current(), crate::import_dialog::Tab::Settings);
    let ctx = crate::theme::test_context();
    for frame in 1..4 {
        let input = egui::RawInput {
            time: Some(f64::from(frame) * 0.1),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            assert_eq!(dialog.show(ctx, None), None);
        });
    }
    let _ = fs::remove_dir_all(outside(&folder));
    let _ = fs::remove_dir_all(&folder);
}
