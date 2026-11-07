//! Phase 73 in the editor: Insert > Spline, and each point edit as one
//! undo step.

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::glam::Vec3;

use super::tests::{example, inserted};
use super::*;
use crate::viewport::SplineAction;

fn points(app: &EditorApp, spline: InstanceId) -> Vec<Vector3> {
    crate::splines::points(&app.world, spline)
        .into_iter()
        .map(|id| {
            app.world
                .get(id)
                .unwrap()
                .property("Position")
                .and_then(Value::as_vector3)
                .unwrap()
        })
        .collect()
}

#[test]
fn insert_spline_makes_two_points_in_one_step_and_selects_it_for_editing() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let spline = inserted(&mut app, Insertable::Spline, None);
    assert_eq!(app.world.get(spline).unwrap().class(), ClassName::Spline);
    assert_eq!(points(&app, spline).len(), 2);
    assert_eq!(app.spline_editing(), Some(spline));
    app.undo();
    assert!(app.world.get(spline).and_then(|s| s.parent()).is_none());
}

#[test]
fn dragging_a_point_adding_one_and_deleting_one_are_each_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let spline = inserted(&mut app, Insertable::Spline, None);
    let before = points(&app, spline);
    let ids = crate::splines::points(&app.world, spline);

    // A drag over several frames, then let go: one step.
    app.spline_action(SplineAction::Pick(ids[1]));
    assert_eq!(app.selected, Some(ids[1]));
    assert_eq!(app.spline_editing(), Some(spline));
    for z in 1..=5 {
        app.spline_action(SplineAction::Move {
            point: ids[1],
            to: Vec3::new(before[1].x, before[1].y, before[1].z + z as f32),
        });
    }
    app.history.seal();
    assert_eq!(points(&app, spline)[1].z, before[1].z + 5.0);
    app.undo();
    assert_eq!(points(&app, spline), before);
    app.redo();

    // Alt-drag from the first point adds one between the two.
    let at = Vec3::new(before[0].x, before[0].y, before[0].z - 3.0);
    app.spline_action(SplineAction::Add { after: ids[0], at });
    app.history.seal();
    let now = points(&app, spline);
    assert_eq!(now.len(), 3);
    assert_eq!(now[1], Vector3::new(at.x, at.y, at.z));
    let added = app.selected.unwrap();
    app.undo();
    assert_eq!(points(&app, spline).len(), 2);
    app.redo();

    // Delete removes the selected point, and undo puts it back.
    app.select_from_viewport(Some(added));
    app.delete_selected();
    assert_eq!(points(&app, spline).len(), 2);
    app.undo();
    assert_eq!(points(&app, spline).len(), 3);
}

#[test]
fn a_spline_follower_inserted_into_a_part_follows_the_first_spline() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let spline = inserted(&mut app, Insertable::Spline, None);
    let part = inserted(&mut app, Insertable::Part(PartType::Block), None);
    let follower = inserted(&mut app, Insertable::SplineFollower, Some(part));
    let instance = app.world.get(follower).unwrap();
    assert_eq!(instance.parent(), Some(part));
    assert_eq!(
        instance.property("Spline"),
        Some(&Value::Instance(Some(spline)))
    );
    // With the follower selected, no Spline is being edited.
    assert_eq!(app.spline_editing(), None);
}
