//! Phase 36 in the editor: the Constraint tool, the overlay's joints, and
//! the hello-joints example saved, reloaded and played.

use std::path::PathBuf;

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_physics::Physics;
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, example, inserted, play_for, printed};
use super::*;
use crate::screen::Screen;
use crate::viewport::SurfacePick;

fn set(app: &mut EditorApp, id: InstanceId, name: &'static str, value: Value) {
    app.world.set_property(id, name, value).unwrap();
}

fn v(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn close(a: Vec3, b: Vec3) -> bool {
    a.abs_diff_eq(b, 1e-3)
}

/// Two blocks 6 studs apart, the second turned, and a pick on the facing
/// side of each.
fn two_blocks(app: &mut EditorApp) -> (SurfacePick, SurfacePick) {
    let a = inserted(app, Insertable::Part(PartType::Block), None);
    let b = inserted(app, Insertable::Part(PartType::Block), None);
    set(app, a, "Position", v(-3.0, 2.0, 0.0));
    set(app, b, "Position", v(3.0, 2.0, 0.0));
    set(app, b, "Orientation", v(0.0, 90.0, 0.0));
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

#[test]
fn the_constraint_tool_joins_two_clicked_surfaces_in_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let (first, second) = two_blocks(&mut app);
    app.pick_constraint_tool(Some(ClassName::RopeConstraint));
    let count = |app: &EditorApp| app.world.descendants(app.world.root()).len();
    let before = count(&app);

    // The first click only marks the surface; a click on the same part
    // again doesn't join it to itself.
    app.constraint_surface(first);
    app.constraint_surface(SurfacePick {
        point: Vec3::new(-3.0, 3.0, 0.0),
        normal: Vec3::Y,
        ..first
    });
    assert_eq!(count(&app), before);
    app.constraint_surface(second);
    assert_eq!(count(&app), before + 3, "two Attachments and the rope");

    let rope = app.selected.expect("the rope is selected");
    let instance = app.world.get(rope).unwrap();
    assert_eq!(instance.class(), ClassName::RopeConstraint);
    assert_eq!(instance.parent(), Some(first.part));
    assert_eq!(instance.property("Length"), Some(&Value::Number(2.0)));
    let attachment = |name| match instance.property(name) {
        Some(Value::Instance(Some(id))) => *id,
        other => panic!("{name} is {other:?}"),
    };
    let (a0, a1) = (attachment("Attachment0"), attachment("Attachment1"));
    assert_eq!(app.world.get(a0).unwrap().parent(), Some(first.part));
    assert_eq!(app.world.get(a1).unwrap().parent(), Some(second.part));
    // Each sits where it was clicked, both with their X axis out of the
    // first surface, though the second part is turned.
    for (id, pick) in [(a0, first), (a1, second)] {
        let (at, turn) = crate::constraints::world_pose(&app.world, id).unwrap();
        assert!(close(at, pick.point), "{at:?}");
        assert!(close(turn * Vec3::X, Vec3::X), "{:?}", turn * Vec3::X);
    }
    // The tool stays on for the next joint.
    assert_eq!(
        app.constraint_tool.map(|tool| (tool.class, tool.first)),
        Some((ClassName::RopeConstraint, None))
    );

    // One undo takes all three away, and redo puts them back.
    app.undo();
    assert_eq!(count(&app), before);
    app.redo();
    assert_eq!(count(&app), before + 3);
    assert_eq!(
        app.world.get(rope).unwrap().property("Attachment1"),
        Some(&Value::Instance(Some(a1)))
    );

    app.pick_constraint_tool(None);
    assert!(app.constraint_tool.is_none());
}

#[test]
fn the_constraints_overlay_draws_joints_with_their_axes_and_limits() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let (first, second) = two_blocks(&mut app);
    let hinge = {
        app.pick_constraint_tool(Some(ClassName::HingeConstraint));
        app.constraint_surface(first);
        app.constraint_surface(second);
        app.selected.unwrap()
    };
    let (third, fourth) = two_blocks(&mut app);
    let spring = {
        app.pick_constraint_tool(Some(ClassName::SpringConstraint));
        app.constraint_surface(third);
        app.constraint_surface(fourth);
        app.selected.unwrap()
    };
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
    let count = |world: &World, overlay: bool, selection: &[InstanceId]| {
        let ctx = theme::test_context();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, world, overlay, selection)
        });
        output.shapes.len()
    };
    let background = count(&app.world, false, &[]);
    // A selected joint is drawn with the overlay off.
    let hinge_alone = count(&app.world, false, &[hinge]);
    assert!(hinge_alone > background);
    // Its limits add an arc.
    set(&mut app, hinge, "LimitsEnabled", Value::Bool(true));
    assert!(count(&app.world, false, &[hinge]) > hinge_alone);
    // The spring is a zigzag of many lines.
    assert!(count(&app.world, false, &[spring]) > hinge_alone + 8);
    assert!(count(&app.world, true, &[]) > count(&app.world, false, &[spring]));

    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(snapshots).join("constraints-overlay-joints.png");
        let world = &app.world;
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 2], |ctx| {
            draw(ctx, world, true, &[]);
        });
    }
}

/// The moving parts of hello-joints.
const MOVING: [&str; 9] = [
    "Door", "Arm", "Wheel", "Bob", "Crate", "Weight", "Pendulum", "Lift", "Bit",
];

#[test]
fn every_constraint_saves_reloads_and_plays_as_the_player_runs_it() {
    let folder = copy_of_example("hello-joints", "joints-play");
    let app = EditorApp::new(Some(&folder));
    let joints = |world: &World| -> Vec<(String, Vec<(&'static str, Value)>)> {
        world
            .descendants(world.workspace())
            .into_iter()
            .filter_map(|id| {
                let instance = world.get(id)?;
                instance.class().is_joint().then(|| {
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
    let before = joints(&app.world);
    let classes: std::collections::HashSet<ClassName> = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .filter_map(|id| app.world.get(id).map(|i| i.class()))
        .filter(|class| class.is_joint())
        .collect();
    assert_eq!(classes.len(), 7, "the example has every joint");

    // Saved by the editor and opened again, every joint is the same.
    let mut app = app;
    assert!(app.save());
    drop(app);
    let mut app = EditorApp::new(Some(&folder));
    let after = joints(&app.world);
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
    play_for(&mut app, 5.5);
    assert!(
        printed(&app).contains(&"door 110, arm -45, spring 5.0, rope 4.0"),
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
    for _ in 0..330 {
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
    let id = part_named(world, name);
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

fn part_named(world: &World, name: &str) -> InstanceId {
    world.find_first_child(world.workspace(), name).unwrap()
}

#[test]
fn the_constraint_tool_is_off_during_play() {
    // A copy: Play saves the new blocks first.
    let folder = copy_of_example("hello-touched", "constraint-tool-play");
    let mut app = EditorApp::new(Some(&folder));
    let (first, _) = two_blocks(&mut app);
    app.pick_constraint_tool(Some(ClassName::HingeConstraint));
    app.play();
    app.constraint_surface(first);
    assert!(app.constraint_tool.unwrap().first.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
