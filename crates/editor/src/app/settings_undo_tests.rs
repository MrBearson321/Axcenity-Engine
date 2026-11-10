//! The Input and Collision Groups windows write their files at once; each
//! change is an undo step too, so a click on **Restore defaults** or a
//! group's × can be taken back like any other edit.

use axcenity_core::KeyCode;
use axcenity_core::input::{Input, InputMap};

use super::collision_groups::GroupEdit;
use super::input_settings::InputEdit;
use super::tests::copy_of_example;
use super::*;

#[test]
fn restore_defaults_in_the_input_window_undoes_and_redoes() {
    let folder = copy_of_example("hello-character", "input-undo");
    let mut app = EditorApp::new(Some(&folder));
    let file = folder.join(axcenity_project::INPUT_FILE);
    app.edit_input_map(InputEdit::AddAction {
        name: "Wave".into(),
        axis: false,
    });
    app.edit_input_map(InputEdit::Add {
        action: "Wave".into(),
        input: Input::Key(KeyCode::G),
    });
    let customised = app.project.as_ref().unwrap().input_map.clone();
    assert_ne!(customised, InputMap::default());
    let saved = std::fs::read_to_string(&file).unwrap();

    app.edit_input_map(InputEdit::Reset);
    assert_eq!(app.project.as_ref().unwrap().input_map, InputMap::default());
    assert_eq!(app.history.undo_label(), Some("Restore default input"));

    app.undo();
    assert_eq!(
        app.project.as_ref().unwrap().input_map,
        customised,
        "one undo brings the custom actions back"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        saved,
        "and the file"
    );
    app.redo();
    assert_eq!(app.project.as_ref().unwrap().input_map, InputMap::default());
    app.undo();
    app.undo();
    app.undo();
    assert!(
        !app.project
            .as_ref()
            .unwrap()
            .input_map
            .actions
            .iter()
            .any(|action| action.name == "Wave"),
        "undo goes back step by step to before the action was added"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn removing_a_collision_group_undoes_with_its_pairs() {
    let folder = copy_of_example("hello-part", "collision-groups-undo");
    let mut app = EditorApp::new(Some(&folder));
    app.edit_collision_groups(GroupEdit::Add("Players".into()));
    app.edit_collision_groups(GroupEdit::Add("Ghosts".into()));
    app.edit_collision_groups(GroupEdit::SetCollidable(
        "Players".into(),
        "Ghosts".into(),
        false,
    ));
    let before = app.world.collision_groups().clone();

    app.edit_collision_groups(GroupEdit::Remove("Ghosts".into()));
    assert_eq!(app.world.collision_groups().names(), ["Default", "Players"]);
    assert_eq!(
        app.history.undo_label(),
        Some("Remove collision group Ghosts")
    );
    app.undo();
    assert_eq!(app.world.collision_groups(), &before, "back, pairs and all");
    assert!(!app.world.collision_groups().collidable("Ghosts", "Players"));
    let reopened = EditorApp::new(Some(&folder));
    assert_eq!(
        reopened.world.collision_groups(),
        &before,
        "the file has it back too"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
