//! Phase 50 in the editor: the ribbon's Solid Modeling group.

use super::solid::SolidAction;
use super::tests::copy_of_example;
use super::*;

/// A Part named `name` in the Workspace with this size and position.
fn part(app: &mut EditorApp, name: &str, size: [f32; 3], at: [f32; 3]) -> InstanceId {
    let id = app.world.create(ClassName::Part).unwrap();
    app.world.set_name(id, name).unwrap();
    let v = |[x, y, z]: [f32; 3]| Value::Vector3(axcenity_core::Vector3::new(x, y, z));
    app.world.set_property(id, "Size", v(size)).unwrap();
    app.world.set_property(id, "Position", v(at)).unwrap();
    app.world
        .set_property(id, "Anchored", Value::Bool(true))
        .unwrap();
    let workspace = app.world.workspace();
    app.world.set_parent(id, Some(workspace)).unwrap();
    id
}

fn class_of(app: &EditorApp, id: InstanceId) -> ClassName {
    app.world.get(id).unwrap().class()
}

fn workspace_names(app: &EditorApp) -> Vec<String> {
    let workspace = app.world.get(app.world.workspace()).unwrap();
    workspace
        .children()
        .iter()
        .map(|&id| app.world.get(id).unwrap().name().to_owned())
        .collect()
}

/// A wall and a block through it, with the block negated (one step).
fn wall_and_negated_block(app: &mut EditorApp) -> (InstanceId, InstanceId) {
    let wall = part(app, "Wall", [12.0, 8.0, 1.0], [0.0, 4.0, 0.0]);
    let block = part(app, "Door", [4.0, 6.0, 3.0], [0.0, 3.0, 0.0]);
    app.select_many(vec![block]);
    app.solid_action(SolidAction::Negate);
    assert_eq!(app.history.undo_label(), Some("Negate 1 parts"));
    let door = app.selected.unwrap();
    assert_eq!(class_of(app, door), ClassName::NegateOperation);
    assert_eq!(app.world.get(door).unwrap().name(), "Door");
    (wall, door)
}

#[test]
fn negate_toggles_a_part_as_one_undo_step() {
    let folder = copy_of_example("hello-part", "solid-negate");
    let mut app = EditorApp::new(Some(&folder));
    let (_, door) = wall_and_negated_block(&mut app);
    let size = app.world.get(door).unwrap().property("Size").cloned();
    // Negating again turns it back, keeping its properties.
    app.solid_action(SolidAction::Negate);
    let back = app.selected.unwrap();
    assert_eq!(class_of(&app, back), ClassName::Part);
    assert_eq!(app.world.get(back).unwrap().property("Size").cloned(), size);
    app.undo();
    assert_eq!(class_of(&app, door), ClassName::NegateOperation);
    assert!(app.world.get(door).unwrap().parent().is_some());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn union_cuts_the_negated_block_out_and_separate_gives_the_parts_back_unchanged() {
    let folder = copy_of_example("hello-part", "solid-union");
    let mut app = EditorApp::new(Some(&folder));
    let (wall, door) = wall_and_negated_block(&mut app);
    // A red door, so its cut faces keep a look of their own (parts that
    // look the same share one).
    app.world
        .set_property(
            door,
            "Color",
            Value::Color3(axcenity_core::Color3::new(1.0, 0.0, 0.0)),
        )
        .unwrap();
    let before: Vec<_> = [wall, door]
        .iter()
        .map(|&id| crate::history::snapshot(&app.world, id))
        .collect();
    let names = workspace_names(&app);

    app.select_many(vec![wall, door]);
    app.solid_action(SolidAction::Union);
    assert_eq!(app.history.undo_label(), Some("Union 2 parts"));
    let union = app.selected.unwrap();
    let instance = app.world.get(union).unwrap();
    assert_eq!(instance.class(), ClassName::UnionOperation);
    assert_eq!(instance.name(), "Union");
    // It takes the wall's place; the parts are inside it, out of the world.
    assert_eq!(instance.parent(), Some(app.world.workspace()));
    assert_eq!(instance.children(), [wall, door]);
    assert!(axcenity_core::csg::in_union(&app.world, wall));
    let mesh_id = instance.property("MeshId").and_then(Value::as_str).unwrap();
    assert_eq!(mesh_id, "assets/models/Union.union.glb");
    assert!(folder.join(mesh_id).is_file());
    let size = instance
        .property("Size")
        .and_then(Value::as_vector3)
        .unwrap();
    assert_eq!((size.x, size.y, size.z), (12.0, 8.0, 1.0));
    let model = app
        .assets
        .model_blocking(mesh_id)
        .expect("the union's model loads");
    assert_eq!(model.primitives.len(), 2, "the wall's look and the door's");

    // Undo puts the parts back; redo makes the union again.
    app.undo();
    assert_eq!(workspace_names(&app), names);
    app.redo();
    assert_eq!(app.world.get(union).unwrap().children(), [wall, door]);

    // Separate gives back the same parts, as they were.
    app.select_many(vec![union]);
    app.solid_action(SolidAction::Separate);
    assert_eq!(app.history.undo_label(), Some("Separate into 2 parts"));
    assert!(app.world.get(union).unwrap().parent().is_none());
    assert_eq!(app.selection, [wall, door]);
    let after: Vec<_> = [wall, door]
        .iter()
        .map(|&id| crate::history::snapshot(&app.world, id))
        .collect();
    assert_eq!(after, before);
    assert_eq!(workspace_names(&app), names);
    assert!(!axcenity_core::csg::in_union(&app.world, wall));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn intersect_keeps_only_the_overlap() {
    let folder = copy_of_example("hello-part", "solid-intersect");
    let mut app = EditorApp::new(Some(&folder));
    let a = part(&mut app, "A", [4.0, 4.0, 4.0], [0.0, 2.0, 0.0]);
    let b = part(&mut app, "B", [4.0, 4.0, 4.0], [2.0, 2.0, 0.0]);
    app.select_many(vec![a, b]);
    app.solid_action(SolidAction::Intersect);
    assert_eq!(app.history.undo_label(), Some("Intersect 2 parts"));
    let union = app.selected.unwrap();
    let instance = app.world.get(union).unwrap();
    let size = instance
        .property("Size")
        .and_then(Value::as_vector3)
        .unwrap();
    assert_eq!((size.x, size.y, size.z), (2.0, 4.0, 4.0));
    let at = instance
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap();
    assert_eq!((at.x, at.y, at.z), (1.0, 2.0, 0.0));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_failed_union_says_why_and_leaves_the_parts_unchanged() {
    let folder = copy_of_example("hello-part", "solid-fails");
    let mut app = EditorApp::new(Some(&folder));
    let a = part(&mut app, "Pill", [4.0, 2.0, 2.0], [0.0, 2.0, 0.0]);
    app.world
        .set_property(
            a,
            "Shape",
            Value::PartType(axcenity_core::PartType::Capsule),
        )
        .unwrap();
    let b = part(&mut app, "Block", [2.0, 2.0, 2.0], [1.0, 2.0, 0.0]);
    let names = workspace_names(&app);
    let label = app.history.undo_label().map(str::to_owned);
    app.select_many(vec![a, b]);
    app.solid_action(SolidAction::Union);
    let last = app.output.lines().last().unwrap().text.clone();
    assert!(
        last.contains("Could not union the parts")
            && last.contains("Capsules can't be combined")
            && last.contains("Nothing was changed"),
        "{last}"
    );
    assert_eq!(workspace_names(&app), names);
    assert_eq!(app.history.undo_label().map(str::to_owned), label);
    assert!(!folder.join("assets/models/Union.union.glb").exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_explorer_hides_the_parts_kept_inside_a_union() {
    let folder = copy_of_example("hello-csg", "solid-explorer");
    let mut app = EditorApp::new(Some(&folder));
    let doorway = app
        .world
        .find_first_child(app.world.workspace(), "Doorway")
        .unwrap();
    // Even opened down to one of its parts, the union shows none of them.
    let wall = app.world.find_first_child(doorway, "Wall").unwrap();
    app.explorer.reveal(&app.world, wall);
    let rows = app.explorer.rows(&app.world);
    let row = rows.iter().find(|row| row.id == doorway).unwrap();
    assert!(!row.has_children);
    assert!(
        !rows
            .iter()
            .any(|row| row.name == "Wall" || row.name == "Door")
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn to_terrain_raises_the_ground_to_a_part_s_top_and_removes_it_as_one_step() {
    // hello-part has a 512-stud Terrain from y = -32 to 96, with Grass,
    // Dirt and Rock layers.
    let folder = copy_of_example("hello-part", "solid-to-terrain");
    let mut app = EditorApp::new(Some(&folder));
    let terrain = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .find(|&id| class_of(&app, id) == ClassName::Terrain)
        .unwrap();
    let placement = axcenity_terrain::Placement::of(app.world.get(terrain).unwrap()).unwrap();
    let ground = |app: &EditorApp, x: f32, z: f32| {
        placement
            .ground_y(app.world.terrain(terrain).unwrap(), x, z)
            .unwrap()
    };
    let before = ground(&app, 100.0, 100.0);
    // A hill, as a block well above the ground.
    let hill = part(
        &mut app,
        "Hill",
        [24.0, 4.0, 24.0],
        [100.0, before + 20.0, 100.0],
    );
    app.select_many(vec![hill]);
    assert!(SolidAction::ToTerrain.usable(&app.world, &app.solid_parts()));
    app.solid_action(SolidAction::ToTerrain);
    assert_eq!(app.history.undo_label(), Some("Convert 1 part to terrain"));
    assert!(
        app.world.get(hill).unwrap().parent().is_none(),
        "the part is gone"
    );
    let top = before + 22.0;
    assert!(
        (ground(&app, 100.0, 100.0) - top).abs() < 0.6,
        "{}",
        ground(&app, 100.0, 100.0)
    );
    assert!(
        (ground(&app, 150.0, 150.0) - before).abs() < 1.0,
        "elsewhere as it was"
    );

    app.undo();
    assert!(app.world.get(hill).unwrap().parent().is_some());
    assert!((ground(&app, 100.0, 100.0) - before).abs() < 1e-3);

    // A part off the terrain changes nothing, and says why.
    let away = part(&mut app, "Away", [4.0, 4.0, 4.0], [2000.0, 0.0, 0.0]);
    app.select_many(vec![away]);
    app.solid_action(SolidAction::ToTerrain);
    assert!(app.world.get(away).unwrap().parent().is_some());
    assert!(
        super::tests::last_line(&app)
            .text
            .contains("over the Terrain")
    );
}
