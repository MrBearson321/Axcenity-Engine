//! Undo and redo in the editor: gizmo drags, stacking, property edits, history
//! that survives saving, and asset edits (clips, controllers and prefabs) as
//! one step each.

use axcenity_core::{ClassName, Color3, Value};
use axcenity_render::glam::Vec3;
use axcenity_scene::Ray;

use super::tests::{copy_of_example, edit_the_pad, example, part, property, vector};
use super::*;
use crate::gizmo::{Axis, Drag, Handle, Pose, Side};
use crate::placing::PlaceDrag;

/// Drags `handle` of `id` as the viewport does: one edit per frame as
/// the mouse moves along `rays`, then the mouse is let go.
fn drag(app: &mut EditorApp, id: InstanceId, handle: Handle, rays: &[Ray]) {
    let pose = Pose::of(app.world.get(id).unwrap()).unwrap();
    let mut drag = Drag::start(handle, id, pose, rays[0]).unwrap();
    for &ray in &rays[1..] {
        let values = drag.update(ray, &app.snap).unwrap();
        app.apply_drag(DragEdit {
            kind: DragKind::of(handle),
            parts: vec![(id, values)],
        });
    }
    app.history.seal();
}

/// Turns `id` `degrees` around the world's Z with its Rotate ring, as the
/// viewport does, seen from in front: one edit per frame as the mouse moves
/// along the ring, then the mouse is let go.
fn turn_around_z(app: &mut EditorApp, id: InstanceId, degrees: f32) {
    let pose = Pose::of(app.world.get(id).unwrap()).unwrap();
    let camera = axcenity_render::Camera {
        position: pose.position + Vec3::Z * 20.0,
        look_at: pose.position,
        fov_y_degrees: 50.0,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = crate::screen::Screen::new(camera, rect);
    let frame = Pose {
        rotation: axcenity_render::glam::Quat::IDENTITY,
        ..pose
    };
    let grab = crate::gizmo::ring_point(&screen, frame, Axis::Z, 0.0);
    let handle = Handle::Rotate(Axis::Z);
    let mut drag = Drag::turn(handle, id, pose, &screen, grab).unwrap();
    let points = degrees.to_radians() / crate::gizmo::TURN_PER_POINT;
    for step in 1..=3 {
        let at = grab + drag.spin() * points * step as f32 / 3.0;
        let pointer = crate::gizmo::Pointer::on(&screen, at, false);
        let values = drag.update(pointer, &app.snap).unwrap();
        app.apply_drag(DragEdit {
            kind: DragKind::of(handle),
            parts: vec![(id, values)],
        });
    }
    app.history.seal();
}

/// Rays from in front of the scene, straight down -Z, through (x, y).
fn rays_along_z(points: &[(f32, f32)]) -> Vec<Ray> {
    points
        .iter()
        .map(|&(x, y)| Ray {
            origin: Vec3::new(x, y, 50.0),
            direction: Vec3::NEG_Z,
        })
        .collect()
}

#[test]
fn each_gizmo_drag_is_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = part(&app, "Pad");
    let start = (
        property(&app, pad, "Position"),
        property(&app, pad, "Size"),
        property(&app, pad, "Orientation"),
    );

    // Move 5 studs right in 5 frames.
    let steps: Vec<(f32, f32)> = (0..=5).map(|x| (x as f32, 0.25)).collect();
    drag(&mut app, pad, Handle::Move(Axis::X), &rays_along_z(&steps));
    assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
    assert_eq!(app.history.undo_label(), Some("Move Pad"));
    // Pull the top face up 2 studs.
    let steps = [(5.0, 0.5), (5.0, 1.5), (5.0, 2.5)];
    drag(
        &mut app,
        pad,
        Handle::Scale(Axis::Y, Side::Plus),
        &rays_along_z(&steps),
    );
    assert_eq!(property(&app, pad, "Size"), vector(8.0, 2.5, 8.0));
    assert_eq!(app.history.undo_label(), Some("Resize Pad"));
    // Turn it a quarter around Z, in steps.
    turn_around_z(&mut app, pad, 90.0);
    let orientation = property(&app, pad, "Orientation").as_vector3().unwrap();
    let actual_rotation = axcenity_scene::rotation_from_orientation(orientation);
    let initial_orientation = start.2.as_vector3().unwrap();
    let expected_rotation = axcenity_render::glam::Quat::from_rotation_z(90.0_f32.to_radians())
        * axcenity_scene::rotation_from_orientation(initial_orientation);
    assert!(actual_rotation.dot(expected_rotation).abs() > 1.0 - 1e-6);
    assert_eq!(app.history.undo_label(), Some("Rotate Pad"));

    // Three drags, three undos, back to the start.
    app.undo();
    assert_eq!(property(&app, pad, "Orientation"), start.2);
    app.undo();
    assert_eq!(property(&app, pad, "Size"), start.1);
    assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
    app.undo();
    assert_eq!(property(&app, pad, "Position"), start.0);
    assert_eq!(app.history.undo_label(), None);
    assert!(!app.unsaved(), "back as it was opened");

    app.redo();
    assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
    assert_eq!(app.history.redo_label(), Some("Resize Pad"));
}

#[test]
fn dragging_a_part_onto_another_stacks_it_in_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ball = part(&app, "Ball");
    let start = property(&app, ball, "Position");
    // Grab the ball (2 studs across, at (0, 10, 0)) on top, then drag it
    // across the floor and onto the pad (8×0.5×8 at (0, 0.25, 0)).
    let grabbed = Vec3::new(0.0, 11.0, 0.0);
    let mut place = PlaceDrag::start(&app.world, ball, grabbed).unwrap();
    let down = |x: f32, z: f32| Ray {
        origin: Vec3::new(x, 50.0, z),
        direction: Vec3::NEG_Y,
    };
    for (x, z) in [(12.0, 6.0), (8.0, 3.0), (2.0, 1.0)] {
        let position = place.update(&app.world, down(x, z), &app.snap).unwrap();
        app.apply_drag(DragEdit {
            kind: DragKind::Move,
            parts: vec![(ball, vec![("Position", position)])],
        });
    }
    app.history.seal();
    // On top of the pad (y = 0.5) and lifted by its radius.
    let (radius, top) = (1.0, 0.5);
    assert_eq!(
        property(&app, ball, "Position"),
        vector(2.0, top + radius, 1.0)
    );
    assert_eq!(app.history.undo_label(), Some("Move Ball"));
    app.undo();
    assert_eq!(property(&app, ball, "Position"), start);
    assert_eq!(app.history.undo_label(), None);
}

#[test]
fn property_edits_undo_and_redo_and_typing_a_name_is_one_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = part(&app, "Pad");
    let gray = property(&app, pad, "Color");
    let green = Value::Color3(Color3::new(0.1, 0.725, 0.333));
    app.apply(pad, Edit::Property("Color", green.clone()));
    app.history.seal();
    // Typing "Door" one letter a frame, then leaving the text box.
    for name in ["D", "Do", "Doo", "Door"] {
        app.apply(pad, Edit::Name(name.to_string()));
    }
    app.history.seal();
    assert_eq!(app.history.undo_label(), Some("Rename Pad"));

    app.undo();
    assert_eq!(app.world.get(pad).unwrap().name(), "Pad");
    assert_eq!(property(&app, pad, "Color"), green);
    assert_eq!(app.history.undo_label(), Some("Change Color of Pad"));
    app.undo();
    assert_eq!(property(&app, pad, "Color"), gray);

    app.redo();
    app.redo();
    assert_eq!(property(&app, pad, "Color"), green);
    assert_eq!(app.world.get(pad).unwrap().name(), "Door");
}

#[test]
fn undo_history_survives_saving() {
    let folder = copy_of_example("hello-touched", "undo-save");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    let start = property(&app, pad, "Position");
    let steps = [(0.0, 0.25), (3.0, 0.25)];
    drag(&mut app, pad, Handle::Move(Axis::X), &rays_along_z(&steps));
    assert!(app.save());
    assert_eq!(app.title(), "Hello Touched - Axcenity Editor");

    // Saving kept the drag in the history.
    assert_eq!(app.history.undo_label(), Some("Move Pad"));
    app.undo();
    assert_eq!(property(&app, pad, "Position"), start);
    assert!(app.unsaved(), "the file still has the moved pad");
    assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");
    // Redoing back to what's on disk counts as saved again.
    app.redo();
    assert!(!app.unsaved());

    // Undo, then save that: the file gets the undone position, and redo still works.
    app.undo();
    assert!(app.save());
    let on_disk = axcenity_project::load(&folder).unwrap().world;
    let saved_pad = on_disk
        .find_first_child(on_disk.workspace(), "Pad")
        .unwrap();
    let saved = on_disk
        .get(saved_pad)
        .unwrap()
        .property("Position")
        .cloned();
    assert_eq!(saved, Some(start));
    app.redo();
    assert_eq!(property(&app, pad, "Position"), vector(3.0, 0.25, 0.0));
    assert!(app.unsaved());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn clip_controller_and_prefab_edits_undo_as_one_step_and_survive_saving() {
    let folder = copy_of_example("hello-touched", "asset-undo");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.selected = Some(pad);

    app.save_prefab();
    let prefab = "assets/prefabs/Pad.prefab.json";
    let prefab_text = std::fs::read_to_string(folder.join(prefab)).unwrap();
    assert_eq!(app.history.undo_label(), Some("Create Pad"));
    app.undo();
    assert!(!folder.join(prefab).exists());
    assert!(
        !app.asset_browser
            .entries()
            .iter()
            .any(|entry| entry.path == prefab),
        "the Assets panel drops the undone prefab"
    );
    app.redo();
    assert_eq!(
        std::fs::read_to_string(folder.join(prefab)).unwrap(),
        prefab_text
    );
    assert!(
        app.asset_browser
            .entries()
            .iter()
            .any(|entry| entry.path == prefab),
        "redo puts the prefab back in the Assets panel"
    );

    app.rename_prefab(prefab, "Platform");
    assert_eq!(app.history.undo_label(), Some("Rename Platform"));
    assert!(folder.join("assets/prefabs/Platform.prefab.json").is_file());
    app.undo();
    assert_eq!(
        std::fs::read_to_string(folder.join(prefab)).unwrap(),
        prefab_text
    );

    app.duplicate_prefab(prefab);
    let copy = "assets/prefabs/Pad copy.prefab.json";
    assert_eq!(app.history.undo_label(), Some("Duplicate Pad"));
    assert_eq!(
        std::fs::read_to_string(folder.join(copy)).unwrap(),
        prefab_text
    );
    app.delete_assets("Pad copy", &[copy.to_owned()]);
    assert!(!folder.join(copy).exists());
    assert_eq!(app.history.undo_label(), Some("Delete Pad copy"));
    assert!(app.save());
    assert_eq!(
        app.history.undo_label(),
        Some("Delete Pad copy"),
        "saving keeps the asset step"
    );
    app.undo();
    assert_eq!(
        std::fs::read_to_string(folder.join(copy)).unwrap(),
        prefab_text
    );
    app.redo();
    assert!(!folder.join(copy).exists());

    let take = axcenity_assets::Take {
        name: "Moves".into(),
        duration: 1.0,
        frame_rate: 30.0,
        tracks: Vec::new(),
    };
    let written =
        crate::animation::store::new_clip(&folder, "assets/models/hero.fbx", &take, "Walk")
            .unwrap();
    let clip = written.path.clone();
    let clip_text = std::fs::read_to_string(folder.join(&clip)).unwrap();
    app.record_asset_files("Create Walk".into(), written.files, None);
    app.undo();
    assert!(!folder.join(&clip).exists());
    app.redo();
    assert_eq!(
        std::fs::read_to_string(folder.join(&clip)).unwrap(),
        clip_text
    );

    let controller = crate::animation::store::new_controller(&folder, "Hero", &[]).unwrap();
    app.record_asset_files("Create Hero".into(), controller.files, None);
    let animator = app.world.create(ClassName::Animator).unwrap();
    app.world.set_parent(animator, Some(pad)).unwrap();
    app.world
        .set_property(
            animator,
            "StateMachine",
            Value::String(controller.path.clone()),
        )
        .unwrap();
    let renamed =
        crate::animation::store::rename_controller(&folder, &controller.path, "Villain").unwrap();
    app.record_asset_files(
        "Rename Hero".into(),
        renamed.files,
        Some((controller.path.clone(), renamed.path.clone())),
    );
    let bound = |app: &EditorApp| {
        app.world
            .get(animator)
            .unwrap()
            .property("StateMachine")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    assert_eq!(bound(&app), renamed.path);
    assert_eq!(app.history.undo_label(), Some("Rename Hero"));
    app.undo();
    assert_eq!(bound(&app), controller.path);
    assert!(folder.join(&controller.path).is_file());
    assert!(!folder.join(&renamed.path).exists());
    app.redo();
    assert_eq!(bound(&app), renamed.path);
    assert!(folder.join(&renamed.path).is_file());

    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn opening_another_project_starts_a_new_history() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    edit_the_pad(&mut app);
    app.tool = Tool::Rotate;
    app.open(&example("hello-part"));
    assert_eq!(app.history.undo_label(), None);
    assert!(!app.unsaved());
    assert_eq!(app.tool, Tool::Rotate, "the tool is a preference");
}
