//! Procedural worlds PW5 in the editor: Conform to Terrain and Blend into
//! Terrain, each one undo step.

use std::sync::Arc;

use axcenity_core::{ClassName, TerrainData, Value, Vector3};

use super::solid::SolidAction;
use super::*;

/// An editor with a 256-stud Terrain round the origin sloping up along X
/// (a stud every 4), and a Model of a floor and a roof floating over it.
fn house() -> (EditorApp, InstanceId, InstanceId, [InstanceId; 2]) {
    let mut app = EditorApp::new(None);
    let workspace = app.world.workspace();
    let terrain = app.world.create(ClassName::Terrain).unwrap();
    app.world.set_parent(terrain, Some(workspace)).unwrap();
    let _ = app
        .world
        .set_property(terrain, "Position", Value::Vector3(Vector3::default()));
    let _ = app.world.set_property(
        terrain,
        "Size",
        Value::Vector3(Vector3::new(256.0, 100.0, 256.0)),
    );
    let mut data = TerrainData::flat(129, 129, 0.0).unwrap();
    let all = data.bounds();
    let heights = data.heights_mut_in(all);
    for z in 0..129 {
        for x in 0..129 {
            heights[z * 129 + x] = (20.0 + (x as f32 * 2.0 - 128.0) / 4.0) / 100.0;
        }
    }
    app.world.set_terrain(terrain, Arc::new(data));
    let layer = app.world.create(ClassName::TerrainLayer).unwrap();
    app.world.set_parent(layer, Some(terrain)).unwrap();
    let model = app.world.create(ClassName::Model).unwrap();
    app.world.set_parent(model, Some(workspace)).unwrap();
    let mut parts = [model; 2];
    for (index, (y, height)) in [(51.0, 2.0), (58.0, 1.0)].into_iter().enumerate() {
        let part = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(part, Some(model)).unwrap();
        let _ = app.world.set_property(part, "Anchored", Value::Bool(true));
        let _ =
            app.world
                .set_property(part, "Position", Value::Vector3(Vector3::new(10.0, y, 0.0)));
        let _ = app.world.set_property(
            part,
            "Size",
            Value::Vector3(Vector3::new(12.0, height, 12.0)),
        );
        parts[index] = part;
    }
    app.history.seal();
    app.select_from_viewport(Some(model));
    (app, terrain, model, parts)
}

fn y(app: &EditorApp, id: InstanceId) -> f32 {
    app.world
        .get(id)
        .unwrap()
        .property("Position")
        .unwrap()
        .as_vector3()
        .unwrap()
        .y
}

#[test]
fn conform_sets_a_floating_model_on_the_ground_as_one_step() {
    let (mut app, _, _, [floor, roof]) = house();
    assert!(SolidAction::ConformToTerrain.usable(&app.world, &app.solid_parts()));
    app.solid_action(SolidAction::ConformToTerrain);
    assert_eq!(app.history.undo_label(), Some("Conform to Terrain"));
    // The ground under its middle (X = 10) is 22.5 studs up: the floor's
    // bottom is there and the roof kept its place above it.
    assert!(
        (y(&app, floor) - 1.0 - 22.5).abs() < 0.01,
        "{}",
        y(&app, floor)
    );
    assert!((y(&app, roof) - y(&app, floor) - 7.0).abs() < 1e-4);
    app.undo();
    assert_eq!(y(&app, floor), 51.0);
}

#[test]
fn blend_levels_the_ground_under_a_model_to_its_bottom_as_one_step() {
    let (mut app, terrain, _, [floor, _]) = house();
    app.solid_action(SolidAction::ConformToTerrain);
    app.solid_action(SolidAction::BlendIntoTerrain);
    assert_eq!(app.history.undo_label(), Some("Blend into Terrain"));
    let ground = |app: &EditorApp, x: f32| {
        let placement = axcenity_terrain::Placement::of(app.world.get(terrain).unwrap()).unwrap();
        placement
            .ground_y(app.world.terrain(terrain).unwrap(), x, 0.0)
            .unwrap()
    };
    let bottom = y(&app, floor) - 1.0;
    // Level under the whole floor, though the slope rose 3 studs across it.
    for x in [4.0, 10.0, 16.0] {
        assert!(
            (ground(&app, x) - bottom).abs() < 0.05,
            "{x}: {}",
            ground(&app, x)
        );
    }
    // Well away from it, the slope is as it was.
    assert!((ground(&app, 40.0) - 30.0).abs() < 0.05);
    app.undo();
    assert!((ground(&app, 4.0) - 21.0).abs() < 0.05);
}
