//! Phase 74 in the editor: SplineMeshes and SplineScatters made again as
//! their splines change, Insert and the Assets panel, Bake, and Flatten
//! Terrain Along.

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, inserted};
use super::*;
use crate::properties::Edit;

fn find(app: &EditorApp, path: &[&str]) -> InstanceId {
    path.iter().fold(app.world.workspace(), |at, name| {
        app.world.find_first_child(at, name).unwrap()
    })
}

fn children(app: &EditorApp, id: InstanceId) -> Vec<InstanceId> {
    app.world.get(id).unwrap().children().to_vec()
}

fn vector(app: &EditorApp, id: InstanceId, name: &str) -> Vector3 {
    app.world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

/// Where each thing `generator` made is, in order.
fn layout(app: &EditorApp, generator: InstanceId) -> Vec<(Vector3, Vector3)> {
    children(app, generator)
        .into_iter()
        .map(|id| (vector(app, id, "Position"), vector(app, id, "Size")))
        .collect()
}

#[test]
fn moving_a_spline_point_makes_its_scatter_again_and_reopening_gives_the_same_layout() {
    let folder = copy_of_example("hello-roads", "spline-scatter-move");
    let mut app = EditorApp::new(Some(&folder));
    app.generate_frame();
    let trees = find(&app, &["Treeline", "Trees"]);
    let first = layout(&app, trees);
    assert!(first.len() >= 8, "{}", first.len());
    // Not an undo step, and the Explorer doesn't list what it made.
    assert_eq!(app.history.undo_label(), None);
    let rows = app.explorer.rows(&app.world);
    assert!(
        rows.iter()
            .all(|row| !first_ids(&app, trees).contains(&row.id))
    );

    // Dragging a point is one step, and the trees follow the new curve.
    let point = crate::splines::points(&app.world, find(&app, &["Treeline"]))[2];
    let was = vector(&app, point, "Position");
    app.move_spline_point(point, Vec3::new(was.x - 30.0, was.y, was.z - 20.0));
    app.history.seal();
    app.generate_frame();
    let moved = layout(&app, trees);
    assert_ne!(moved, first);
    app.undo();
    assert_eq!(layout(&app, trees), first);

    // Saved and opened again, the same seed gives the same layout.
    app.redo();
    assert!(app.save());
    let mut again = EditorApp::new(Some(&folder));
    again.generate_frame();
    assert_eq!(layout(&again, find(&again, &["Treeline", "Trees"])), moved);
    let _ = std::fs::remove_dir_all(&folder);
}

fn first_ids(app: &EditorApp, generator: InstanceId) -> Vec<InstanceId> {
    children(app, generator)
}

#[test]
fn a_scatter_saves_only_its_settings_until_bake_keeps_what_it_made_as_one_step() {
    let folder = copy_of_example("hello-roads", "spline-scatter-bake");
    let mut app = EditorApp::new(Some(&folder));
    app.generate_frame();
    let woods = find(&app, &["Grove", "Woods"]);
    let made = layout(&app, woods);
    assert!(made.len() > 20);
    let saved = |app: &mut EditorApp| {
        assert!(app.save());
        std::fs::read_to_string(folder.join("world.json")).unwrap()
    };
    let before = saved(&mut app);
    assert!(before.contains("\"SplineScatter\"") && !before.contains("MeshPart"));

    // Bake: a Model named Woods beside the Grove spline holds the same
    // models, the scatter is gone, and the file has them all.
    app.select_from_viewport(Some(woods));
    app.apply(woods, Edit::BakeSpline);
    assert_eq!(app.history.undo_label(), Some("Bake Woods"));
    assert!(app.world.get(woods).is_none_or(|w| w.parent().is_none()));
    let model = find(&app, &["Woods"]);
    assert_eq!(app.world.get(model).unwrap().class(), ClassName::Model);
    assert_eq!(layout(&app, model), made);
    app.generate_frame();
    assert_eq!(layout(&app, model), made, "baked models are left alone");
    let after = saved(&mut app);
    assert_eq!(after.matches("\"MeshPart\"").count(), made.len());
    assert!(!after.contains("\"SplineScatter\": {") && after.contains("tree.glb"));

    // Undo puts the scatter back, making the same layout.
    app.undo();
    let woods = find(&app, &["Grove", "Woods"]);
    assert_eq!(layout(&app, woods), made);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Woods")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn baking_a_road_saves_its_bent_pieces_which_load_as_they_were() {
    let folder = copy_of_example("hello-roads", "spline-mesh-bake");
    let mut app = EditorApp::new(Some(&folder));
    app.generate_frame();
    let asphalt = find(&app, &["Road", "Asphalt"]);
    let made = layout(&app, asphalt);
    assert!(made.len() > 20);
    app.apply(asphalt, Edit::BakeSpline);
    let path = "assets/models/Asphalt.spline.glb";
    assert!(folder.join(path).is_file());
    let model = find(&app, &["Asphalt"]);
    let pieces = children(&app, model);
    assert_eq!(layout(&app, model), made);
    for (i, &piece) in pieces.iter().enumerate() {
        let mesh_id = app.world.get(piece).unwrap().property("MeshId").unwrap();
        let mesh_id = mesh_id.as_str().unwrap();
        assert_eq!(mesh_id, format!("{path}#Piece{}", i + 1));
        // Each loads from the file at the size it was made.
        let loaded = app.assets.model(mesh_id).unwrap();
        let size = vector(&app, piece, "Size");
        for (a, b) in loaded.size.iter().zip([size.x, size.y, size.z]) {
            assert!(
                (a - b).abs() < 1e-3,
                "{mesh_id}: {:?} vs {size:?}",
                loaded.size
            );
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn insert_puts_a_spline_mesh_in_the_selected_spline_and_assets_clicks_pick_its_models() {
    let folder = copy_of_example("hello-roads", "spline-insert");
    let mut app = EditorApp::new(Some(&folder));
    let road = find(&app, &["Road"]);
    let mesh = inserted(&mut app, Insertable::SplineMesh, Some(road));
    assert_eq!(app.world.get(mesh).unwrap().parent(), Some(road));
    let (picking, models) = app.spline_models_picking().unwrap();
    assert_eq!((picking, models), (mesh, Vec::new()));
    // Clicking the road model uses it, sized as the model is.
    app.pick_spline_model(mesh, "assets/models/road.glb");
    assert_eq!(vector(&app, mesh, "Size"), Vector3::new(12.0, 1.05, 8.0));
    assert!(!children(&app, mesh).is_empty());
    app.undo();
    assert!(children(&app, mesh).is_empty());

    // A scatter takes models in and out.
    let scatter = inserted(&mut app, Insertable::SplineScatter, Some(road));
    let models = |app: &EditorApp| app.spline_models_picking().unwrap().1;
    app.pick_spline_model(scatter, "assets/models/tree.glb");
    app.pick_spline_model(scatter, "assets/models/rock.glb");
    assert_eq!(
        models(&app),
        ["assets/models/tree.glb", "assets/models/rock.glb"]
    );
    app.pick_spline_model(scatter, "assets/models/tree.glb");
    assert_eq!(models(&app), ["assets/models/rock.glb"]);

    // With nothing selected, Insert makes a Spline for it too.
    app.select_from_viewport(None);
    let lone = inserted(&mut app, Insertable::SplineScatter, None);
    let parent = app.world.get(lone).unwrap().parent().unwrap();
    assert_eq!(app.world.get(parent).unwrap().class(), ClassName::Spline);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn flatten_terrain_along_levels_a_road_wide_strip_as_one_step() {
    // hello-part has a 512-stud Terrain from y = -32 to 96.
    let folder = copy_of_example("hello-part", "spline-flatten");
    let mut app = EditorApp::new(Some(&folder));
    let terrain = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .find(|&id| app.world.terrain(id).is_some())
        .unwrap();
    let placement = axcenity_terrain::Placement::of(app.world.get(terrain).unwrap()).unwrap();
    let ground = |app: &EditorApp, x: f32, z: f32| {
        placement
            .ground_y(app.world.terrain(terrain).unwrap(), x, z)
            .unwrap()
    };
    let spline = inserted(&mut app, Insertable::Spline, None);
    let points = crate::splines::points(&app.world, spline);
    let height = ground(&app, 100.0, 100.0) + 3.0;
    for (point, x) in points.iter().zip([80.0, 140.0]) {
        app.move_spline_point(*point, Vec3::new(x, height, 100.0));
        app.history.seal();
    }
    let road = inserted(&mut app, Insertable::SplineMesh, Some(spline));
    app.apply(
        road,
        Edit::Property("Size", Value::Vector3(Vector3::new(10.0, 1.0, 8.0))),
    );
    let was = ground(&app, 110.0, 100.0 + 9.0);
    app.apply(spline, Edit::FlattenTerrainAlong);
    assert_eq!(
        app.history.undo_label(),
        Some(
            format!(
                "Flatten Terrain Along {}",
                app.world.get(spline).unwrap().name()
            )
            .as_str()
        )
    );
    // Within the road's half width and its shoulder, the ground is at the
    // curve; well outside, it's as it was.
    for side in [-5.5, 0.0, 5.5] {
        let at = ground(&app, 110.0, 100.0 + side);
        assert!((at - height).abs() < 0.2, "{side}: {at} vs {height}");
    }
    assert_eq!(ground(&app, 110.0, 100.0 + 9.0), was);
    app.undo();
    assert!((ground(&app, 110.0, 100.0) - height).abs() > 1.0);
    let _ = std::fs::remove_dir_all(&folder);
}
