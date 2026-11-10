//! Phase 42 in the editor: a Fracture inserted into a part, and Generate
//! in its Properties saving the pieces.

use super::tests::{copy_of_example, inserted};
use super::*;
use crate::properties::Edit;

fn pieces_id(app: &EditorApp, fracture: InstanceId) -> String {
    let id = app.world.get(fracture).unwrap().property("PiecesId");
    id.and_then(Value::as_str).unwrap().to_owned()
}

#[test]
fn insert_fracture_then_generate_saves_the_pieces_as_one_undo_step() {
    let folder = copy_of_example("hello-welds", "fracture-generate");
    let mut app = EditorApp::new(Some(&folder));
    let top = app
        .world
        .find_first_child(app.world.workspace(), "TableTop")
        .unwrap();
    let fracture = inserted(&mut app, Insertable::Fracture, Some(top));
    assert_eq!(app.world.get(fracture).unwrap().parent(), Some(top));
    assert_eq!(pieces_id(&app, fracture), "");

    app.apply(fracture, Edit::GenerateFracture);
    let path = "assets/models/TableTop.fracture.glb";
    assert_eq!(pieces_id(&app, fracture), path);
    assert!(folder.join(path).is_file());
    assert_eq!(app.history.undo_label(), Some("Generate Fracture"));
    // The 12 pieces load from the project like any model.
    assert!(
        app.assets
            .model_blocking(&format!("{path}#Piece12"))
            .is_some()
    );

    // Undone, the Fracture has no pieces again; the file stays.
    app.undo();
    assert_eq!(pieces_id(&app, fracture), "");
    assert!(folder.join(path).is_file());
    app.redo();
    assert_eq!(pieces_id(&app, fracture), path);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn generating_a_fracture_outside_a_part_says_why() {
    let folder = copy_of_example("hello-welds", "fracture-nowhere");
    let mut app = EditorApp::new(Some(&folder));
    let fracture = inserted(&mut app, Insertable::Fracture, None);
    app.apply(fracture, Edit::GenerateFracture);
    assert_eq!(pieces_id(&app, fracture), "");
    let _ = std::fs::remove_dir_all(&folder);
}
