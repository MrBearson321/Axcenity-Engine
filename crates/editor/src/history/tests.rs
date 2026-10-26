use axcenity_core::{ClassName, Material, Vector3};

use super::*;

fn world_with_part() -> (World, InstanceId) {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    (world, part)
}

/// Sets a property and records it, as the app does.
fn set(history: &mut History, world: &mut World, id: InstanceId, name: &'static str, value: Value) {
    let before = snapshot(world, id);
    world.set_property(id, name, value).unwrap();
    let after = snapshot(world, id);
    let field = Field::Property(name);
    history.record(world, id, field, format!("Change {name}"), before, after);
}

fn position(world: &World, id: InstanceId) -> Value {
    world.get(id).unwrap().property("Position").unwrap().clone()
}

fn at(x: f32) -> Value {
    Value::Vector3(Vector3::new(x, 0.5, 0.0))
}

#[test]
fn a_drag_that_rewrites_a_stylesheet_is_one_step_that_hands_the_text_back() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let field = Field::Property("Size");
    let text = |before: &str, after: &str| {
        vec![TextChange {
            path: "ui.css".into(),
            before: Some(before.into()),
            after: Some(after.into()),
        }]
    };
    for (before, after) in [("10px", "11px"), ("11px", "12px")] {
        let fields = snapshot(&world, part);
        history.record_with_texts(
            &mut world,
            part,
            field,
            "Resize".into(),
            fields.clone(),
            fields,
            text(before, after),
        );
    }
    history.seal();

    let texts = |restored: Option<Restored>| restored.map(|restored| restored.texts);
    assert_eq!(texts(history.undo(&mut world)), Some(text("12px", "10px")));
    assert_eq!(history.undo(&mut world), None, "both frames are one step");
    assert_eq!(texts(history.redo(&mut world)), Some(text("10px", "12px")));
}

#[test]
fn a_whole_world_step_undoes_property_and_tree_changes_together() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    set(&mut history, &mut world, part, "Position", at(1.0));
    let before = world.clone();
    world.set_property(part, "Position", at(5.0)).unwrap();
    let made = world.create(ClassName::Part).unwrap();
    world.set_parent(made, Some(world.workspace())).unwrap();
    history.record_world(&mut world, "Console".into(), before);
    assert_eq!(history.undo_label(), Some("Console"));

    history.undo(&mut world).unwrap();
    assert_eq!(position(&world, part), at(1.0));
    assert!(!world.contains(made));
    history.redo(&mut world).unwrap();
    assert_eq!(position(&world, part), at(5.0));
    assert!(world.is_descendant_of(made, world.workspace()));
    // Older steps still work on the swapped-in world.
    history.undo(&mut world).unwrap();
    history.undo(&mut world).unwrap();
    assert_ne!(position(&world, part), at(1.0));
}

#[test]
fn creating_a_file_is_one_step_and_saving_keeps_it() {
    let mut world = World::new();
    let mut history = History::new();
    let path = "assets/animations/hero/Walk.anim.json";
    history.record_files(
        &mut world,
        "Create Walk".into(),
        vec![TextChange {
            path: path.into(),
            before: None,
            after: Some("{}\n".into()),
        }],
    );
    history.mark_saved();
    assert!(history.is_saved());
    assert_eq!(history.undo_label(), Some("Create Walk"));

    let undone = history.undo(&mut world).unwrap().texts;
    assert_eq!(undone[0].path, path);
    assert_eq!(undone[0].after, None);
    assert_eq!(history.redo_label(), Some("Create Walk"));
    let redone = history.redo(&mut world).unwrap().texts;
    assert_eq!(redone[0].after.as_deref(), Some("{}\n"));
}

#[test]
fn files_written_as_bytes_are_one_step_that_hands_them_back() {
    let mut world = World::new();
    let mut history = History::new();
    let model = Arc::new(vec![0, 159, 146, 150]);
    let old = Arc::new(b"old".to_vec());
    history.record_blobs(
        &mut world,
        "Import noodle.axpkg".into(),
        vec![
            BlobChange {
                path: "assets/models/noodle.fbx".into(),
                before: None,
                after: Some(model.clone()),
            },
            BlobChange {
                path: "assets/textures/a.png".into(),
                before: Some(old.clone()),
                after: Some(old.clone()),
            },
        ],
    );
    assert_eq!(history.undo_label(), Some("Import noodle.axpkg"));
    let undone = history.undo(&mut world).unwrap();
    assert!(undone.texts.is_empty());
    // Unchanged files aren't kept.
    assert_eq!(
        undone.blobs,
        [BlobChange {
            path: "assets/models/noodle.fbx".into(),
            before: Some(model.clone()),
            after: None,
        }]
    );
    assert_eq!(history.undo(&mut world), None);
    let redone = history.redo(&mut world).unwrap();
    assert_eq!(redone.blobs[0].after, Some(model));
}

#[test]
fn renaming_a_controller_file_restores_the_animator_path_too() {
    let (mut world, part) = world_with_part();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    let from = "assets/animations/Hero.statemachine.json";
    let to = "assets/animations/Hero2.statemachine.json";
    world
        .set_property(animator, "StateMachine", Value::String(from.into()))
        .unwrap();
    let mut history = History::new();
    world
        .set_property(animator, "StateMachine", Value::String(to.into()))
        .unwrap();
    history.record_asset(
        &mut world,
        "Rename Hero".into(),
        vec![
            TextChange {
                path: from.into(),
                before: Some("{}\n".into()),
                after: None,
            },
            TextChange {
                path: to.into(),
                before: None,
                after: Some("{}\n".into()),
            },
        ],
        vec![PropertyEdit {
            id: animator,
            field: Field::Property("StateMachine"),
            before: Value::String(from.into()),
            after: Value::String(to.into()),
        }],
    );
    assert_eq!(history.undo_label(), Some("Rename Hero"));
    history.undo(&mut world);
    let controller = world
        .get(animator)
        .unwrap()
        .property("StateMachine")
        .and_then(Value::as_str);
    assert_eq!(controller, Some(from));
    history.redo(&mut world);
    let controller = world
        .get(animator)
        .unwrap()
        .property("StateMachine")
        .and_then(Value::as_str);
    assert_eq!(controller, Some(to));
}

#[test]
fn undo_and_redo_put_values_back_and_forth() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let start = position(&world, part);
    set(&mut history, &mut world, part, "Position", at(5.0));
    history.seal();
    assert_eq!(history.undo_label(), Some("Change Position"));

    assert!(history.undo(&mut world).is_some());
    assert_eq!(position(&world, part), start);
    assert_eq!(history.undo_label(), None);
    assert_eq!(history.redo_label(), Some("Change Position"));
    assert!(history.undo(&mut world).is_none(), "nothing more to undo");

    assert!(history.redo(&mut world).is_some());
    assert_eq!(position(&world, part), at(5.0));
    assert!(history.redo(&mut world).is_none(), "nothing more to redo");
}

#[test]
fn changes_in_one_gesture_are_one_step() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let start = position(&world, part);
    // A drag: a new position every frame, then the mouse is let go.
    for x in 1..=10 {
        set(&mut history, &mut world, part, "Position", at(x as f32));
    }
    history.seal();
    set(&mut history, &mut world, part, "Position", at(20.0));
    history.seal();

    history.undo(&mut world);
    assert_eq!(position(&world, part), at(10.0));
    history.undo(&mut world);
    assert_eq!(position(&world, part), start);
    assert!(
        history.undo(&mut world).is_none(),
        "two drags are two steps"
    );
}

#[test]
fn a_gesture_on_another_field_or_part_starts_a_new_step() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    set(&mut history, &mut world, part, "Position", at(1.0));
    // No seal between them, but a different field.
    set(
        &mut history,
        &mut world,
        part,
        "Anchored",
        Value::Bool(true),
    );
    history.undo(&mut world);
    assert_eq!(position(&world, part), at(1.0));
    history.undo(&mut world);
    assert_ne!(position(&world, part), at(1.0));
}

#[test]
fn a_drag_that_ends_where_it_started_leaves_nothing_to_undo() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let start = position(&world, part);
    set(&mut history, &mut world, part, "Position", at(3.0));
    set(&mut history, &mut world, part, "Position", start.clone());
    history.seal();
    assert_eq!(history.undo_label(), None);
    assert!(history.is_saved());
}

#[test]
fn a_new_change_after_undo_clears_redo() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    set(&mut history, &mut world, part, "Position", at(1.0));
    history.undo(&mut world);
    set(&mut history, &mut world, part, "Position", at(2.0));
    assert_eq!(history.redo_label(), None);
    assert!(history.redo(&mut world).is_none());
}

#[test]
fn undoing_a_material_change_brings_back_metalness_and_roughness() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    // A custom roughness, then a material that resets it to its preset.
    set(
        &mut history,
        &mut world,
        part,
        "Roughness",
        Value::Number(0.9),
    );
    history.seal();
    set(
        &mut history,
        &mut world,
        part,
        "Material",
        Value::Material(Material::Metal),
    );
    history.seal();
    let roughness = |world: &World| world.get(part).unwrap().property("Roughness").cloned();
    assert_eq!(roughness(&world), Some(Value::Number(0.3)));

    history.undo(&mut world);
    assert_eq!(roughness(&world), Some(Value::Number(0.9)));
    let material = world.get(part).unwrap().property("Material").cloned();
    assert_eq!(material, Some(Value::Material(Material::Plastic)));
    history.redo(&mut world);
    assert_eq!(roughness(&world), Some(Value::Number(0.3)));
}

#[test]
fn renames_undo_too() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let before = snapshot(&world, part);
    world.set_name(part, "Door").unwrap();
    let after = snapshot(&world, part);
    history.record(
        &mut world,
        part,
        Field::Name,
        "Rename".into(),
        before,
        after,
    );
    history.undo(&mut world);
    assert_eq!(world.get(part).unwrap().name(), "Part");
}

/// Moves `id` to `after` and records it, as the app does.
fn put_and_record(history: &mut History, world: &mut World, id: InstanceId, after: Option<Place>) {
    let before = place_of(world, id);
    put(world, id, after).unwrap();
    let moves = vec![Move { id, before, after }];
    history.record_tree(world, "Tree".into(), moves);
}

#[test]
fn undoing_a_delete_puts_the_same_instance_back_in_the_same_spot() {
    let (mut world, first) = world_with_part();
    let workspace = world.workspace();
    let second = world.create(ClassName::Part).unwrap();
    world.set_parent(second, Some(workspace)).unwrap();
    let mut history = History::new();
    set(&mut history, &mut world, first, "Position", at(4.0));
    history.seal();

    put_and_record(&mut history, &mut world, first, None);
    assert_eq!(world.get(workspace).unwrap().children(), &[second]);
    assert!(world.contains(first), "deleting only detaches");

    history.undo(&mut world);
    assert_eq!(world.get(workspace).unwrap().children(), &[first, second]);
    // The step before still works, because the ID is the same.
    history.undo(&mut world);
    assert_ne!(position(&world, first), at(4.0));
    history.redo(&mut world);
    history.redo(&mut world);
    assert_eq!(world.get(workspace).unwrap().children(), &[second]);
}

#[test]
fn moving_into_another_parent_undoes_to_the_old_parent_and_index() {
    let (mut world, a) = world_with_part();
    let workspace = world.workspace();
    let b = world.create(ClassName::Part).unwrap();
    world.set_parent(b, Some(workspace)).unwrap();
    let mut history = History::new();
    let into_b = Some(Place {
        parent: b,
        index: 0,
    });
    put_and_record(&mut history, &mut world, a, into_b);
    assert_eq!(world.get(b).unwrap().children(), &[a]);
    assert!(!history.is_saved());

    history.undo(&mut world);
    assert_eq!(world.get(workspace).unwrap().children(), &[a, b]);
    assert!(history.is_saved());
}

#[test]
fn an_undone_insert_is_destroyed_once_it_cannot_be_redone() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    let new = world.create(ClassName::Folder).unwrap();
    let end = Some(Place {
        parent: world.workspace(),
        index: 1,
    });
    put_and_record(&mut history, &mut world, new, end);
    history.undo(&mut world);
    assert!(world.contains(new), "redo can still bring it back");

    // A new change drops the redo step, and with it the folder.
    set(&mut history, &mut world, part, "Position", at(1.0));
    assert!(!world.contains(new));
    assert!(history.redo(&mut world).is_none());
}

#[test]
fn saving_keeps_the_history_and_undoing_back_to_it_counts_as_saved() {
    let (mut world, part) = world_with_part();
    let mut history = History::new();
    assert!(history.is_saved());
    set(&mut history, &mut world, part, "Position", at(1.0));
    set(&mut history, &mut world, part, "Position", at(2.0));
    assert!(!history.is_saved());
    history.mark_saved();
    assert!(history.is_saved());
    // A gesture still going when saving doesn't merge into the saved step.
    set(&mut history, &mut world, part, "Position", at(3.0));
    assert!(!history.is_saved());

    assert!(history.undo(&mut world).is_some());
    assert_eq!(position(&world, part), at(2.0));
    assert!(history.is_saved());
    assert!(
        history.undo(&mut world).is_some(),
        "saving didn't clear the history"
    );
    assert!(!history.is_saved());
    history.redo(&mut world);
    assert!(history.is_saved());
}

#[test]
fn terrain_strokes_and_new_landscapes_undo_and_redo() {
    use axcenity_render::glam::{Vec2, Vec3};
    use axcenity_terrain::Placement;
    use axcenity_terrain::brush::{self, Brush, EditRecorder, Operation};

    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    let original = world.terrain(terrain).unwrap().heights().to_vec();
    let mut history = History::new();

    // A stroke.
    let brush = Brush::default();
    let data = world.terrain_mut(terrain).unwrap();
    let mut recorder = EditRecorder::new();
    let rect = brush::footprint(data, &placement, Vec2::ZERO, brush.radius);
    recorder.before_change(data, rect);
    brush::apply(data, &placement, &brush, &Operation::Raise, Vec3::ZERO);
    let edit = recorder.finish(data).unwrap();
    let raised = data.heights().to_vec();
    let snapshot_now = snapshot(&world, terrain);
    history.record_terrain(
        &mut world,
        terrain,
        "Raise Terrain".to_owned(),
        TerrainChange::Stroke(edit),
        snapshot_now.clone(),
        snapshot_now,
    );
    assert_eq!(history.undo_label(), Some("Raise Terrain"));
    history.undo(&mut world);
    assert_eq!(
        world.terrain(terrain).unwrap().heights(),
        original.as_slice()
    );
    history.redo(&mut world);
    assert_eq!(world.terrain(terrain).unwrap().heights(), raised.as_slice());

    // A new, bigger landscape with a new Size, in one step.
    let before = world.terrain(terrain).unwrap().clone();
    let after = Arc::new(TerrainData::flat(65, 65, 0.9).unwrap());
    let fields_before = snapshot(&world, terrain);
    world.set_property(terrain, "Size", at(64.0)).unwrap();
    world.terrain_mut(terrain).unwrap().replace_with(&after);
    let fields_after = snapshot(&world, terrain);
    history.record_terrain(
        &mut world,
        terrain,
        "Generate Terrain".to_owned(),
        TerrainChange::Replace { before, after },
        fields_before,
        fields_after,
    );
    history.undo(&mut world);
    assert_eq!(world.terrain(terrain).unwrap().size(), (257, 257));
    assert_eq!(world.terrain(terrain).unwrap().heights(), raised.as_slice());
    assert_ne!(
        world.get(terrain).unwrap().property("Size"),
        Some(&at(64.0))
    );
    history.redo(&mut world);
    assert_eq!(world.terrain(terrain).unwrap().size(), (65, 65));
    assert_eq!(
        world.get(terrain).unwrap().property("Size"),
        Some(&at(64.0))
    );
}
