//! Data layers in the editor (roadmap R16.2): the Layers panel's edits as
//! undo steps, this person's hidden and locked layers kept out of the world
//! and in `.axcenity/layers.json`, and what the viewport and Explorer do
//! with them.

use axcenity_core::ClassName;

use super::tests::{copy_of_example, part, play_for};
use super::*;
use crate::layers::{self, LayerAction, LayerView};

/// The layer called `name`.
fn layer(app: &EditorApp, name: &str) -> InstanceId {
    layers::layers(&app.world)
        .into_iter()
        .find(|layer| layer.name == name)
        .unwrap_or_else(|| panic!("no layer {name}"))
        .id
}

#[test]
fn a_new_layer_holds_the_selection_and_rename_and_delete_are_one_undo_step_each() {
    let folder = copy_of_example("hello-touched", "layers-edit");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.select_many(vec![pad]);

    app.layer_action(LayerAction::New);
    let made = layer(&app, "Layer");
    assert_eq!(app.world.get(made).unwrap().class(), ClassName::DataLayer);
    assert_eq!(
        app.world.get(made).unwrap().parent(),
        Some(app.world.data_layer_service())
    );
    assert!(app.world.get(pad).unwrap().has_tag("Layer"));
    assert_eq!(app.history.undo_label(), Some("New layer Layer"));
    app.undo();
    assert!(layers::layers(&app.world).is_empty());
    assert!(!app.world.get(pad).unwrap().has_tag("Layer"));
    app.redo();
    let made = layer(&app, "Layer");

    // Renamed: the tag on its members follows, and so does the view.
    app.layer_action(LayerAction::ToggleHidden("Layer".into()));
    app.layer_action(LayerAction::Rename(made, "Lava".into()));
    assert!(app.world.get(pad).unwrap().has_tag("Lava"));
    assert!(!app.world.get(pad).unwrap().has_tag("Layer"));
    assert!(app.layer_view.hidden.contains("Lava"));
    assert_eq!(app.history.undo_label(), Some("Rename layer Layer to Lava"));
    // Another layer can't take its name.
    app.select_many(Vec::new());
    app.layer_action(LayerAction::New);
    let other = layer(&app, "Layer");
    app.layer_action(LayerAction::Rename(other, "Lava".into()));
    assert_eq!(app.world.get(other).unwrap().name(), "Layer");

    // Taken out and put back in.
    app.select_many(vec![pad]);
    app.layer_action(LayerAction::RemoveSelection("Lava".into()));
    assert!(!app.world.get(pad).unwrap().has_tag("Lava"));
    app.layer_action(LayerAction::AddSelection("Lava".into()));
    assert!(app.world.get(pad).unwrap().has_tag("Lava"));
    assert_eq!(app.history.undo_label(), Some("Add to layer Lava"));

    // Deleted: the members stay, without the tag.
    let lava = layer(&app, "Lava");
    app.layer_action(LayerAction::Delete(lava));
    assert!(app.world.get(pad).is_some());
    assert!(!app.world.get(pad).unwrap().has_tag("Lava"));
    assert_eq!(app.history.undo_label(), Some("Delete layer Lava"));
    app.undo();
    assert!(app.world.get(pad).unwrap().has_tag("Lava"));

    // Saved with the world and read back.
    assert!(app.save());
    let text = std::fs::read_to_string(folder.join("world.json")).unwrap();
    assert!(text.contains("\"data_layer_service\""), "{text}");
    let reopened = EditorApp::new(Some(&folder));
    let names: Vec<String> = layers::layers(&reopened.world)
        .into_iter()
        .map(|layer| layer.name)
        .collect();
    assert_eq!(names, ["Lava", "Layer"]);
}

#[test]
fn a_hidden_layer_is_this_persons_view_and_play_shows_it() {
    let folder = copy_of_example("hello-touched", "layers-view");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.select_many(vec![pad]);
    app.layer_action(LayerAction::New);
    let steps = app.history.undo_label().map(str::to_owned);
    let unsaved = app.unsaved();

    app.layer_action(LayerAction::ToggleHidden("Layer".into()));
    app.update_layer_sets();
    assert!(app.layer_sets.hidden.contains(&pad));
    assert!(app.layer_sets.unpickable.contains(&pad));
    assert!(app.explorer.hidden.contains(&pad));
    assert!(app.viewport.terrain.streaming.is_hidden(pad));
    // Not an edit: no undo step, and nothing more to save.
    assert_eq!(app.history.undo_label().map(str::to_owned), steps);
    assert_eq!(app.unsaved(), unsaved);
    // Kept on this computer, beside the project.
    assert_eq!(
        LayerView::load(&folder)
            .hidden
            .into_iter()
            .collect::<Vec<_>>(),
        ["Layer"]
    );

    // Locked: drawn, but a click goes through.
    app.layer_action(LayerAction::ToggleHidden("Layer".into()));
    app.layer_action(LayerAction::ToggleLocked("Layer".into()));
    app.update_layer_sets();
    assert!(!app.viewport.terrain.streaming.is_hidden(pad));
    assert!(app.layer_sets.unpickable.contains(&pad));

    // Taking the tag off brings it back at once.
    app.layer_action(LayerAction::RemoveSelection("Layer".into()));
    app.update_layer_sets();
    assert!(!app.layer_sets.unpickable.contains(&pad));
    app.undo();
    app.update_layer_sets();
    assert!(app.layer_sets.unpickable.contains(&pad));

    // Play shows every layer.
    app.layer_action(LayerAction::ToggleHidden("Layer".into()));
    app.play();
    play_for(&mut app, 0.2);
    app.update_layer_sets();
    assert!(app.explorer.hidden.is_empty());
    app.stop();
    app.update_layer_sets();
    assert!(app.viewport.terrain.streaming.is_hidden(pad));

    // Reopened, the view is still there; once nothing is hidden or
    // locked, the file goes.
    let mut reopened = EditorApp::new(Some(&folder));
    assert!(reopened.layer_view.hidden.contains("Layer"));
    assert!(reopened.layer_view.locked.contains("Layer"));
    reopened.layer_action(LayerAction::ToggleHidden("Layer".into()));
    reopened.layer_action(LayerAction::ToggleLocked("Layer".into()));
    assert!(!folder.join(layers::VIEW_FILE).exists());
}
