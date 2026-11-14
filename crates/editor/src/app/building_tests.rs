//! Procedural worlds PW6 in the editor: Insert a Building standing free or
//! in a Spline, and Bake it, one undo step.

use axcenity_core::{ClassName, Value, Vector3};

use super::tests::inserted;
use super::*;
use crate::properties::Edit;

#[test]
fn an_inserted_building_stands_in_front_of_the_camera_and_bakes_into_a_model() {
    let mut app = EditorApp::new(None);
    let building = inserted(&mut app, Insertable::BuildingGenerator, None);
    let instance = app.world.get(building).unwrap();
    assert_eq!(instance.class(), ClassName::BuildingGenerator);
    assert_eq!(instance.name(), "Building");
    assert_eq!(instance.parent(), Some(app.world.workspace()));
    let made = instance.children().to_vec();
    assert!(made.len() > 30, "{}", made.len());
    // What it made isn't an edit: Insert is the only undo step.
    assert_eq!(app.history.undo_label(), Some("Insert Building"));
    app.history.seal();
    // The handles move its footprint box, not the parts it made.
    let handled = crate::viewport::movable_parts(&app.world, &[building], Some(building));
    assert_eq!(handled.len(), 1);
    assert_eq!(handled[0].0, building);
    assert_eq!(handled[0].1.size.x, 24.0);

    app.apply(building, Edit::BakeSpline);
    assert!(app.world.get(building).is_none());
    let model = app.selected.unwrap();
    let baked = app.world.get(model).unwrap();
    assert_eq!(baked.class(), ClassName::Model);
    assert_eq!(baked.name(), "Building");
    assert_eq!(baked.parent(), Some(app.world.workspace()));
    assert_eq!(baked.children(), made.as_slice());
    assert_eq!(app.history.undo_label(), Some("Bake Building"));
    app.undo();
    app.generate_frame();
    let back = app.world.get(building).unwrap();
    assert_eq!(back.children().len(), made.len());
}

#[test]
fn a_building_inserted_in_a_selected_spline_uses_its_points_as_corners() {
    let mut app = EditorApp::new(None);
    let spline = inserted(&mut app, Insertable::Spline, None);
    // A third point makes a triangle of the two it starts with.
    let points = crate::splines::points(&app.world, spline);
    let first = app
        .world
        .get(points[0])
        .unwrap()
        .property("Position")
        .cloned();
    let Some(Value::Vector3(first)) = first else {
        panic!("a point has a Position");
    };
    let third = splines::new_point(
        &mut app.world,
        spline,
        Vector3::new(first.x + 8.0, first.y, first.z + 20.0),
    );
    app.world.set_parent(third, Some(spline)).unwrap();
    let building = inserted(&mut app, Insertable::BuildingGenerator, Some(spline));
    assert_eq!(app.world.get(building).unwrap().parent(), Some(spline));
    let made = app.world.get(building).unwrap().children().to_vec();
    assert!(!made.is_empty());
    // A triangle's slabs are wedges.
    let wedges = made
        .iter()
        .filter(|&&id| {
            app.world.get(id).unwrap().property("Shape")
                == Some(&Value::PartType(axcenity_core::PartType::Wedge))
        })
        .count();
    assert!(wedges >= 6, "{wedges}");
}
