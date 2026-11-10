//! Phase 37 in the editor: movers from the Constraint tool, the overlay's
//! movers, and the hello-movers example saved, reloaded and played.

use std::path::PathBuf;

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_physics::Physics;
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, example, inserted, play_for, printed};
use super::*;
use crate::screen::Screen;
use crate::viewport::SurfacePick;

fn v(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

/// Two blocks 6 studs apart and a pick on the facing side of each.
fn two_blocks(app: &mut EditorApp) -> (SurfacePick, SurfacePick) {
    let a = inserted(app, Insertable::Part(PartType::Block), None);
    let b = inserted(app, Insertable::Part(PartType::Block), None);
    app.world
        .set_property(a, "Position", v(-3.0, 2.0, 0.0))
        .unwrap();
    app.world
        .set_property(b, "Position", v(3.0, 2.0, 0.0))
        .unwrap();
    let first = SurfacePick {
        part: a,
        point: Vec3::new(-1.0, 2.5, 0.0),
        normal: Vec3::X,
    };
    let second = SurfacePick {
        part: b,
        point: Vec3::new(1.0, 2.5, 0.0),
        normal: -Vec3::X,
    };
    (first, second)
}

fn attachment(app: &EditorApp, mover: InstanceId, name: &str) -> Option<InstanceId> {
    match app.world.get(mover).unwrap().property(name) {
        Some(Value::Instance(id)) => *id,
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn the_constraint_tool_puts_a_mover_on_one_clicked_surface_in_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let (first, second) = two_blocks(&mut app);
    let count = |app: &EditorApp| app.world.descendants(app.world.root()).len();
    let before = count(&app);

    // One click makes a VectorForce and its Attachment.
    app.pick_constraint_tool(Some(ClassName::VectorForce));
    app.constraint_surface(first);
    assert_eq!(count(&app), before + 2, "an Attachment and the VectorForce");
    let force = app.selected.expect("the VectorForce is selected");
    assert_eq!(
        app.world.get(force).unwrap().class(),
        ClassName::VectorForce
    );
    assert_eq!(app.world.get(force).unwrap().parent(), Some(first.part));
    let a0 = attachment(&app, force, "Attachment0").unwrap();
    assert_eq!(app.world.get(a0).unwrap().parent(), Some(first.part));
    assert_eq!(attachment(&app, force, "Attachment1"), None);
    let (at, turn) = crate::constraints::world_pose(&app.world, a0).unwrap();
    assert!(at.abs_diff_eq(first.point, 1e-3), "{at:?}");
    assert!((turn * Vec3::X).abs_diff_eq(Vec3::X, 1e-3));
    app.undo();
    assert_eq!(count(&app), before);
    app.redo();
    assert_eq!(count(&app), before + 2);

    // An align takes a second click, on the part it follows.
    app.pick_constraint_tool(Some(ClassName::AlignPosition));
    app.constraint_surface(first);
    assert_eq!(count(&app), before + 2);
    app.constraint_surface(second);
    assert_eq!(count(&app), before + 5, "two Attachments and the align");
    let align = app.selected.unwrap();
    assert_eq!(
        app.world.get(align).unwrap().class(),
        ClassName::AlignPosition
    );
    let a1 = attachment(&app, align, "Attachment1").unwrap();
    assert_eq!(app.world.get(a1).unwrap().parent(), Some(second.part));
    app.undo();
    assert_eq!(count(&app), before + 2);
}

#[test]
fn the_constraints_overlay_draws_movers() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let (first, second) = two_blocks(&mut app);
    let made = |app: &mut EditorApp, class, picks: &[SurfacePick]| {
        app.pick_constraint_tool(Some(class));
        for &pick in picks {
            app.constraint_surface(pick);
        }
        app.selected.unwrap()
    };
    let force = made(&mut app, ClassName::VectorForce, &[first]);
    let torque = made(&mut app, ClassName::Torque, &[first]);
    let align = made(&mut app, ClassName::AlignOrientation, &[first, second]);
    let camera = Camera {
        position: Vec3::new(0.0, 8.0, 16.0),
        look_at: Vec3::new(0.0, 2.0, 0.0),
        fov_y_degrees: 50.0,
    };
    let size = egui::vec2(640.0, 360.0);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let screen = Screen::new(camera, rect);
    let draw = |ctx: &egui::Context, world: &World, overlay: bool, selection: &[InstanceId]| {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_gray(40)))
            .show(ctx, |ui| {
                crate::constraints::paint(ui.painter(), &screen, world, selection, overlay);
            });
    };
    let count = |world: &World, selection: &[InstanceId]| {
        let ctx = theme::test_context();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, world, false, selection)
        });
        output.shapes.len()
    };
    let background = count(&app.world, &[]);
    // A force with no Force is only its dot; with one, an arrow.
    let dot = count(&app.world, &[force]);
    assert!(dot > background);
    app.world
        .set_property(force, "Force", v(0.0, 100.0, 0.0))
        .unwrap();
    assert!(
        count(&app.world, &[force]) >= dot + 3,
        "a line and its head"
    );
    // A torque circles its axis.
    app.world
        .set_property(torque, "Torque", v(0.0, 5.0, 0.0))
        .unwrap();
    assert!(count(&app.world, &[torque]) > dot + 10);
    // An align's line to its target, and the axes it turns to.
    assert!(count(&app.world, &[align]) > dot + 5);

    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(snapshots).join("constraints-overlay-movers.png");
        let world = &app.world;
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 2], |ctx| {
            draw(ctx, world, true, &[]);
        });
    }
}

/// The moving parts of hello-movers.
const MOVING: [&str; 6] = [
    "Balloon",
    "SlowFollower",
    "QuickFollower",
    "Drone",
    "Top",
    "Spinner",
];

#[test]
fn every_mover_saves_reloads_and_plays_as_the_player_runs_it() {
    let folder = copy_of_example("hello-movers", "movers-play");
    let app = EditorApp::new(Some(&folder));
    let movers = |world: &World| -> Vec<(String, Vec<(&'static str, Value)>)> {
        world
            .descendants(world.workspace())
            .into_iter()
            .filter_map(|id| {
                let instance = world.get(id)?;
                instance.class().is_mover().then(|| {
                    let properties = instance
                        .class()
                        .default_properties()
                        .into_iter()
                        .map(|(name, _)| (name, instance.property(name).cloned().unwrap()))
                        .collect();
                    (instance.name().to_owned(), properties)
                })
            })
            .collect()
    };
    let before = movers(&app.world);
    let classes: std::collections::HashSet<ClassName> = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .filter_map(|id| app.world.get(id).map(|i| i.class()))
        .filter(|class| class.is_mover())
        .collect();
    assert_eq!(classes.len(), 6, "the example has every mover");

    // Saved by the editor and opened again, every mover is the same.
    let mut app = app;
    assert!(app.save());
    drop(app);
    let mut app = EditorApp::new(Some(&folder));
    let after = movers(&app.world);
    assert_eq!(before.len(), after.len());
    for ((name, was), (_, now)) in before.iter().zip(&after) {
        for ((property, a), (_, b)) in was.iter().zip(now) {
            // Instances are new IDs after loading: compare where they point.
            match (a, b) {
                (Value::Instance(Some(_)), Value::Instance(Some(_))) => {}
                _ => assert_eq!(a, b, "{name}.{property}"),
            }
        }
    }

    // Played in the editor...
    app.play();
    play_for(&mut app, 4.5);
    assert!(
        printed(&app).contains(&"balloon 6.0, quick closer true, drone tilt 0, top 4.0"),
        "{:?}",
        printed(&app)
    );
    let game = shown(&app.world, &app.play);
    let in_editor: Vec<Vector3> = MOVING.iter().map(|name| position(game, name)).collect();

    // ...and run the way the player runs it, from the saved files.
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let mut scripts = axcenity_sim::script_runtime();
    for script in &project.scripts {
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
    }
    let mut physics = Physics::new();
    for _ in 0..270 {
        scripts.step(&mut world, 1.0 / 60.0);
        physics.step(&mut world, 1.0 / 60.0);
        let touches = physics.take_touches();
        scripts.fire_touched(&mut world, &touches);
        scripts.heartbeat(&mut world, 1.0 / 60.0);
    }
    for (name, editor) in MOVING.iter().zip(&in_editor) {
        let player = position(&world, name);
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

fn position(world: &World, name: &str) -> Vector3 {
    let id = world.find_first_child(world.workspace(), name).unwrap();
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}
