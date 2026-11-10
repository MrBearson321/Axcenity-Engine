//! Phase 41 in the editor: a Ragdoll inserted and built from its
//! Properties, its overlay, and hello-character's Dummy knocked over in Play
//! as the player runs it.

use axcenity_anim::Animators;
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::Vector3;
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, inserted, play_for};
use super::*;
use crate::properties::Edit;
use crate::screen::Screen;

fn children(world: &World, id: InstanceId) -> usize {
    world.get(id).unwrap().children().len()
}

#[test]
fn insert_ragdoll_then_build_makes_its_bodies_and_joints_as_one_undo_step() {
    let folder = copy_of_example("hello-character", "ragdoll-build");
    let mut app = EditorApp::new(Some(&folder));
    let walking = app
        .world
        .find_first_child(app.world.workspace(), "Walking")
        .unwrap();
    let ragdoll = inserted(&mut app, Insertable::Ragdoll, Some(walking));
    assert_eq!(app.world.get(ragdoll).unwrap().parent(), Some(walking));
    assert_eq!(children(&app.world, ragdoll), 0);

    app.apply(ragdoll, Edit::BuildRagdoll);
    assert_eq!(children(&app.world, ragdoll), 11, "a capsule per main bone");
    assert_eq!(app.history.undo_label(), Some("Build Ragdoll"));
    let joints = app
        .world
        .descendants(ragdoll)
        .into_iter()
        .filter(|&id| app.world.get(id).unwrap().class().is_joint())
        .count();
    assert_eq!(joints, 10);

    // A body's size and a joint's limits are its own properties.
    let head = axcenity_anim::ragdoll::body_of(&app.world, ragdoll, "Head").unwrap();
    let neck = app.world.find_first_child(head, "Neck").unwrap();
    assert_eq!(
        app.world.get(neck).unwrap().property("UpperAngle"),
        Some(&Value::Number(35.0))
    );

    app.undo();
    assert_eq!(children(&app.world, ragdoll), 0);
    app.redo();
    assert_eq!(children(&app.world, ragdoll), 11);

    // Built again, it replaces what was there.
    app.apply(ragdoll, Edit::BuildRagdoll);
    assert_eq!(children(&app.world, ragdoll), 11);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn building_a_ragdoll_outside_a_skinned_mesh_part_says_why() {
    let folder = copy_of_example("hello-character", "ragdoll-nowhere");
    let mut app = EditorApp::new(Some(&folder));
    let floor = app
        .world
        .find_first_child(app.world.workspace(), "Floor")
        .unwrap();
    let ragdoll = inserted(&mut app, Insertable::Ragdoll, Some(floor));
    app.apply(ragdoll, Edit::BuildRagdoll);
    assert_eq!(children(&app.world, ragdoll), 0);
    let last = app.output.lines().last().unwrap().text.clone();
    assert!(last.contains("skinned MeshPart"), "{last}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// How many shapes the constraints overlay paints for `world` with
/// `selection`, the overlay itself off, looking at the Dummy.
fn overlay_shapes(world: &World, selection: &[InstanceId]) -> usize {
    let camera = Camera {
        position: Vec3::new(-8.0, 3.0, -30.0),
        look_at: Vec3::new(-8.0, 1.5, -20.0),
        fov_y_degrees: 50.0,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 360.0));
    let screen = Screen::new(camera, rect);
    let ctx = theme::test_context();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            crate::constraints::paint(ui.painter(), &screen, world, selection, false);
        });
    });
    output.shapes.len()
}

#[test]
fn the_overlay_shows_a_ragdoll_while_it_or_its_mesh_part_is_selected() {
    let folder = copy_of_example("hello-character", "ragdoll-overlay");
    let app = EditorApp::new(Some(&folder));
    let dummy = app
        .world
        .find_first_child(app.world.workspace(), "Dummy")
        .unwrap();
    let ragdoll = app.world.find_first_child(dummy, "Ragdoll").unwrap();
    let nothing = overlay_shapes(&app.world, &[]);
    let shown = overlay_shapes(&app.world, &[ragdoll]);
    assert!(shown > nothing + 11 * 20, "{nothing} -> {shown}");
    assert_eq!(overlay_shapes(&app.world, &[dummy]), shown);
    let _ = std::fs::remove_dir_all(&folder);
}

/// Where each body of the Dummy's ragdoll is, by name.
fn bodies(world: &World) -> Vec<(String, Vector3)> {
    let dummy = world.find_first_child(world.workspace(), "Dummy").unwrap();
    let ragdoll = world.find_first_child(dummy, "Ragdoll").unwrap();
    world
        .get(ragdoll)
        .unwrap()
        .children()
        .iter()
        .map(|&id| {
            let body = world.get(id).unwrap();
            let at = body
                .property("Position")
                .and_then(Value::as_vector3)
                .unwrap();
            (body.name().to_owned(), at)
        })
        .collect()
}

#[test]
fn the_dummy_is_knocked_down_the_same_in_play_as_the_player_runs_it() {
    let folder = copy_of_example("hello-character", "ragdoll-play");
    let mut app = EditorApp::new(Some(&folder));
    // Played in the editor, past the push at 2 seconds...
    app.play();
    play_for(&mut app, 3.5);
    let game = app.play.as_ref().unwrap().world();
    let dummy = game.find_first_child(game.workspace(), "Dummy").unwrap();
    let ragdoll = game.find_first_child(dummy, "Ragdoll").unwrap();
    assert_eq!(
        game.get(ragdoll).unwrap().property("Enabled"),
        Some(&Value::Bool(true))
    );
    let in_editor = bodies(game);

    // ...and run the way the player runs it, from the same files.
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let assets = Assets::new(&folder);
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_assets(assets.clone());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    for script in &project.scripts {
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
    }
    let mut physics = Physics::with_assets(assets.clone());
    let mut characters = Controllers::default();
    let mut agents = Agents::default();
    let dt = 1.0 / 60.0;
    for _ in 0..210 {
        characters.step(&mut world, &assets, MoveInput::default(), dt);
        agents.step(&mut world, &assets, dt);
        scripts.step(&mut world, f64::from(dt));
        scripts.stepped(&mut world, f64::from(dt));
        characters.keep_kinematic(&mut world);
        agents.keep_kinematic(&mut world);
        physics.step(&mut world, f64::from(dt));
        let touches = physics.take_touches();
        scripts.fire_touched(&mut world, &touches);
        let ends = physics.take_touch_ends();
        scripts.fire_touch_ended(&mut world, &ends);
        scripts.heartbeat(&mut world, f64::from(dt));
        animators.update(&mut world, &assets, f64::from(dt));
    }
    let in_player = bodies(&world);
    assert_eq!(in_editor.len(), 11);
    for ((name, editor), (_, player)) in in_editor.iter().zip(&in_player) {
        let apart = (Vec3::new(player.x, player.y, player.z)
            - Vec3::new(editor.x, editor.y, editor.z))
        .length();
        assert!(
            apart < 1e-3,
            "{name}: {player:?} in the player, {editor:?} in the editor"
        );
    }
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
