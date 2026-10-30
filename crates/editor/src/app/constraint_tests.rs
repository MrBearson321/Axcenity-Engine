//! Phase 35 in the editor: the Weld button, Attachments moved with the
//! transform tools, and the Constraints overlay.

use std::path::PathBuf;

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, example, inserted, part};
use super::*;
use crate::gizmo::{Axis, Drag, Handle, Pointer};
use crate::screen::Screen;

fn vector(app: &EditorApp, id: InstanceId, name: &str) -> Vector3 {
    app.world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn set(app: &mut EditorApp, id: InstanceId, name: &'static str, value: Value) {
    app.world.set_property(id, name, value).unwrap();
}

fn v(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn close(a: Vec3, b: Vec3) -> bool {
    a.abs_diff_eq(b, 1e-3)
}

/// Drags the Move tool's `axis` arrow `studs` along, as the viewport does,
/// with whatever is selected: the handles' frame and parts from
/// `selected_parts`, the drag's values through `localize`, then
/// `apply_drag`.
fn move_selection(app: &mut EditorApp, axis: Axis, studs: f32) {
    let parts = viewport::selected_parts(&app.world, &app.selection, app.selected);
    let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
    let frame = crate::gizmo::handle_frame(Tool::Move, Modes::default(), &poses).unwrap();
    let camera = Camera {
        position: frame.position + Vec3::new(8.0, 12.0, 30.0),
        look_at: frame.position,
        fov_y_degrees: 50.0,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = Screen::new(camera, rect);
    let direction = match axis {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    };
    let grab = screen.point(frame.position + direction).unwrap();
    let handle = Handle::Move(axis);
    let mut drag = Drag::start_many(handle, frame, parts, Modes::default(), &screen, grab).unwrap();
    let to = screen
        .point(frame.position + direction * (1.0 + studs))
        .unwrap();
    let mut values = drag
        .update_all(Pointer::on(&screen, to, false), &app.snap)
        .unwrap();
    crate::constraints::localize(&app.world, &mut values);
    app.apply_drag(DragEdit {
        kind: DragKind::of(handle),
        parts: values,
    });
    app.history.seal();
}

fn select(app: &mut EditorApp, ids: &[InstanceId]) {
    app.selection = ids.to_vec();
    app.selected = ids.last().copied();
}

#[test]
fn the_weld_button_welds_the_selection_to_its_largest_part() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let small = inserted(&mut app, Insertable::Part(PartType::Block), None);
    let big = inserted(&mut app, Insertable::Part(PartType::Block), None);
    set(&mut app, big, "Size", v(6.0, 1.0, 6.0));
    let ball = inserted(&mut app, Insertable::Part(PartType::Ball), None);

    // One part isn't enough.
    select(&mut app, &[small]);
    app.weld_selected();
    assert!(app.world.descendants(small).is_empty());

    select(&mut app, &[small, big, ball]);
    app.weld_selected();
    let weld_in = |app: &EditorApp, id: InstanceId| {
        app.world
            .get(id)
            .unwrap()
            .children()
            .iter()
            .copied()
            .find(|&child| app.world.get(child).unwrap().class() == ClassName::WeldConstraint)
    };
    for other in [small, ball] {
        let weld = weld_in(&app, other).expect("a weld inside each other part");
        let weld = app.world.get(weld).unwrap();
        assert_eq!(weld.name(), "WeldConstraint");
        assert_eq!(weld.property("Part0"), Some(&Value::Instance(Some(big))));
        assert_eq!(weld.property("Part1"), Some(&Value::Instance(Some(other))));
        assert_eq!(weld.property("Enabled"), Some(&Value::Bool(true)));
    }
    assert!(weld_in(&app, big).is_none());
    // The engine sees one assembly.
    assert_eq!(
        axcenity_physics::assembly::connected_parts(&app.world, small, true).len(),
        3
    );
    assert_eq!(app.history.undo_label(), Some("Weld 3 parts"));

    // One undo takes both welds out.
    app.undo();
    assert!(weld_in(&app, small).is_none() && weld_in(&app, ball).is_none());
    app.redo();
    assert!(weld_in(&app, small).is_some() && weld_in(&app, ball).is_some());
}

#[test]
fn attachments_keep_their_offsets_when_the_part_moves_and_save_and_reload() {
    let folder = copy_of_example("hello-part", "attachment-offsets");
    let mut app = EditorApp::new(Some(&folder));
    let block = part(&app, "Block");
    select(&mut app, &[block]);
    let attachment = inserted(&mut app, Insertable::Attachment, Some(block));
    {
        let instance = app.world.get(attachment).unwrap();
        assert_eq!(instance.class(), ClassName::Attachment);
        assert_eq!(instance.parent(), Some(block));
        // It starts at the middle of the part.
        assert_eq!(instance.property("Position"), Some(&v(0.0, 0.0, 0.0)));
    }
    set(&mut app, attachment, "Position", v(1.5, 0.5, 0.0));
    set(&mut app, attachment, "Orientation", v(0.0, 90.0, 0.0));
    set(&mut app, block, "Orientation", v(0.0, 0.0, 30.0));
    let world_before = crate::constraints::world_pose(&app.world, attachment).unwrap();
    let block_before = vector(&app, block, "Position");

    // Moving the part (with the Attachment selected too) moves the
    // Attachment with it, once, and leaves its offset alone.
    select(&mut app, &[block, attachment]);
    move_selection(&mut app, Axis::X, 3.0);
    let moved = vector(&app, block, "Position");
    assert_eq!(moved.x, block_before.x + 3.0);
    assert_eq!(
        vector(&app, attachment, "Position"),
        Vector3::new(1.5, 0.5, 0.0)
    );
    let world_after = crate::constraints::world_pose(&app.world, attachment).unwrap();
    assert!(close(world_after.0, world_before.0 + Vec3::X * 3.0));

    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let block = part(&reopened, "Block");
    let again = reopened
        .world
        .find_first_child(block, "Attachment")
        .expect("the Attachment saved");
    let instance = reopened.world.get(again).unwrap();
    assert_eq!(instance.class(), ClassName::Attachment);
    assert_eq!(instance.property("Position"), Some(&v(1.5, 0.5, 0.0)));
    assert_eq!(instance.property("Orientation"), Some(&v(0.0, 90.0, 0.0)));
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn moving_a_selected_attachment_moves_it_in_the_world_and_stores_its_offset_on_the_turned_part() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let block = inserted(&mut app, Insertable::Part(PartType::Block), None);
    // Turned a quarter around Y: its own X points along the world's -Z.
    set(&mut app, block, "Orientation", v(0.0, 90.0, 0.0));
    let attachment = inserted(&mut app, Insertable::Attachment, Some(block));
    let block_at = vector(&app, block, "Position");
    let (start, _) = crate::constraints::world_pose(&app.world, attachment).unwrap();

    select(&mut app, &[attachment]);
    move_selection(&mut app, Axis::X, 2.0);
    let (now, _) = crate::constraints::world_pose(&app.world, attachment).unwrap();
    assert!(close(now, start + Vec3::X * 2.0), "{now} from {start}");
    // The world's +X is the turned part's +Z.
    let offset = vector(&app, attachment, "Position");
    assert!(
        close(
            Vec3::new(offset.x, offset.y, offset.z),
            Vec3::new(0.0, 0.0, 2.0)
        ),
        "{offset:?}"
    );
    assert_eq!(vector(&app, block, "Position"), block_at, "the part stays");
    assert_eq!(app.history.undo_label(), Some("Move Attachment"));
    app.undo();
    assert_eq!(vector(&app, attachment, "Position"), Vector3::default());
}

#[test]
fn the_constraints_overlay_draws_welds_and_attachments() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let a = inserted(&mut app, Insertable::Part(PartType::Block), None);
    let b = inserted(&mut app, Insertable::Part(PartType::Block), None);
    set(&mut app, a, "Position", v(-4.0, 2.0, 0.0));
    set(&mut app, b, "Position", v(4.0, 2.0, 0.0));
    select(&mut app, &[a, b]);
    app.weld_selected();
    let attachment = inserted(&mut app, Insertable::Attachment, Some(a));
    set(&mut app, attachment, "Position", v(0.0, 0.5, 0.0));

    let camera = Camera {
        position: Vec3::new(0.0, 8.0, 16.0),
        look_at: Vec3::new(0.0, 2.0, 0.0),
        fov_y_degrees: 50.0,
    };
    let size = egui::vec2(640.0, 360.0);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let screen = Screen::new(camera, rect);
    let world = &app.world;
    let draw = |ctx: &egui::Context, overlay: bool, selection: &[InstanceId]| {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_gray(40)))
            .show(ctx, |ui| {
                crate::constraints::paint(ui.painter(), &screen, world, selection, overlay);
            });
    };
    let count = |overlay: bool, selection: &[InstanceId]| {
        let ctx = theme::test_context();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, overlay, selection)
        });
        output.shapes.len()
    };
    // Off, nothing is drawn but a selected Attachment.
    let background = count(false, &[]);
    assert!(count(false, &[attachment]) > background);
    // On, the weld and the Attachment are.
    assert!(count(true, &[]) > count(false, &[attachment]));

    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(snapshots).join("constraints-overlay.png");
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 2], |ctx| {
            draw(ctx, true, &[attachment]);
        });
    }
}

#[test]
fn join_surfaces_welds_a_moved_part_to_the_parts_it_is_left_touching_as_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let place = |app: &mut EditorApp, at: Vec3| {
        let id = inserted(app, Insertable::Part(PartType::Block), None);
        for (name, value) in [("Position", at), ("Size", Vec3::splat(2.0))] {
            app.world
                .set_property(
                    id,
                    name,
                    Value::Vector3(Vector3::new(value.x, value.y, value.z)),
                )
                .unwrap();
        }
        id
    };
    // High above everything else: one block, one flush beside it, one apart.
    let base = place(&mut app, Vec3::new(0.0, 100.0, 0.0));
    let moved = place(&mut app, Vec3::new(2.0, 100.0, 0.0));
    let apart = place(&mut app, Vec3::new(0.0, 100.0, 9.0));
    app.join_surfaces(&[moved, apart]);
    let welds: Vec<InstanceId> = app
        .world
        .get(moved)
        .unwrap()
        .children()
        .iter()
        .copied()
        .filter(|&id| app.world.get(id).unwrap().class() == ClassName::WeldConstraint)
        .collect();
    assert_eq!(welds.len(), 1);
    let weld = app.world.get(welds[0]).unwrap();
    assert_eq!(weld.property("Part0"), Some(&Value::Instance(Some(base))));
    assert_eq!(weld.property("Part1"), Some(&Value::Instance(Some(moved))));
    assert!(app.world.get(apart).unwrap().children().is_empty());
    assert_eq!(app.history.undo_label(), Some("Join 1 surface"));
    // Joining again doesn't weld them twice.
    app.join_surfaces(&[moved]);
    assert_eq!(app.world.get(moved).unwrap().children().len(), 1);
    app.undo();
    assert!(app.world.get(welds[0]).is_none_or(|w| w.parent().is_none()));
}
