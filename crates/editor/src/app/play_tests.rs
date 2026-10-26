//! Play and Stop in the editor: the game runs as it would in the player,
//! endless loops and engine panics are stopped, and Stop puts the world back.

use axcenity_core::{Color3, PartType, Value};

use super::tests::{
    copy_of_example, describe, example, inserted, last_line, part, part_exists_on_disk, play_for,
    press, printed, property, vector,
};
use super::*;
use crate::output::OutputKind;

/// The whole tree, every Instance out of the tree that undo still holds,
/// and the undo history: everything Stop has to put back.
fn everything(app: &EditorApp) -> (Vec<String>, String, usize, Option<String>, bool) {
    (
        describe(&app.world, app.world.root()),
        axcenity_project::write_world(&app.world),
        app.world.len(),
        app.history.undo_label().map(str::to_string),
        app.unsaved(),
    )
}

#[test]
fn play_runs_the_milestone_1_demo_in_the_editor() {
    let folder = example("hello-touched");
    let file_before = std::fs::read_to_string(folder.join("world.json")).unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    let gray = property(&app, pad, "Color");
    app.play();
    assert!(app.playing());
    assert!(
        last_line(&app)
            .text
            .starts_with("Playing \"Hello Touched\"")
    );

    // The red ball lands on the pad within 3 seconds, and the pad changes color.
    play_for(&mut app, 2.9);
    assert!(printed(&app).contains(&"Ball touched the pad"));
    let shown_pad = |app: &EditorApp| {
        shown(&app.world, &app.play)
            .get(pad)
            .unwrap()
            .property("Color")
            .cloned()
    };
    assert_ne!(
        shown_pad(&app),
        Some(gray.clone()),
        "the panels show the game"
    );
    assert_eq!(
        property(&app, pad, "Color"),
        gray,
        "the edit world is untouched"
    );
    // At 3 s the script drops a blue ball; at 6 s it stops listening.
    play_for(&mut app, 3.2);
    assert!(printed(&app).contains(&"BlueBall touched the pad"));
    assert_eq!(last_line(&app).text, "stopped listening for touches");
    let game = shown(&app.world, &app.play);
    assert!(
        game.find_first_child(game.workspace(), "BlueBall")
            .is_some()
    );

    // Nothing was unsaved, so Play didn't write the world file.
    let file_after = std::fs::read_to_string(folder.join("world.json")).unwrap();
    assert_eq!(file_after, file_before);
}

#[test]
fn an_endless_loop_during_play_is_stopped_and_the_editor_carries_on() {
    let folder = copy_of_example("hello-touched", "endless-loop");
    std::fs::write(
        folder.join("scripts/stuck.server.lua"),
        "print('looping')\nwhile true do\nend\n",
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let started = std::time::Instant::now();
    app.play();
    let took = started.elapsed();

    // The real limit, three seconds, then the editor gets control back.
    assert!(app.playing(), "the game keeps running");
    let limit = axcenity_script::WATCHDOG_LIMIT;
    assert!(took >= limit && took < limit * 2, "{took:?}");
    let stopped = app
        .output
        .lines()
        .iter()
        .find(|line| line.text.contains("without pausing"))
        .unwrap();
    assert_eq!(stopped.kind, OutputKind::Error);
    assert!(
        stopped.text.starts_with(
            "scripts/stuck.server.lua:2: this script ran for 3 seconds without pausing"
        ),
        "{}",
        stopped.text
    );
    // It links to the script, like any error.
    assert!(stopped.location.is_some());

    // The other script and the frames carry on, and Stop works.
    play_for(&mut app, 2.9);
    assert!(printed(&app).contains(&"Ball touched the pad"));
    app.stop();
    assert!(!app.playing());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn stop_puts_every_instance_and_property_back_as_before_play() {
    let folder = copy_of_example("hello-touched", "play-stop");
    let mut app = EditorApp::new(Some(&folder));
    // Some edits first: a moved pad, a new Model with a part in it, and a
    // deleted Folder that undo still holds outside the tree.
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Property("Position", vector(1.5, 0.25, -0.5)));
    let model = inserted(&mut app, Insertable::Model, None);
    inserted(&mut app, Insertable::Part(PartType::Wedge), Some(model));
    let folder_id = inserted(&mut app, Insertable::Folder, None);
    app.act(Action::Delete(folder_id));
    app.select_from_viewport(Some(pad));
    app.play();
    let before = everything(&app);
    let game_ball = part(&app, "Ball");

    // Scripts recolor the pad and add balls, physics moves them all.
    play_for(&mut app, 9.0);
    let game = shown(&app.world, &app.play);
    assert!(
        game.find_first_child(game.workspace(), "IgnoredBall")
            .is_some()
    );
    assert_ne!(
        game.get(game_ball).unwrap().property("Position"),
        app.world.get(game_ball).unwrap().property("Position"),
        "the ball fell in the game"
    );

    app.stop();
    assert!(!app.playing());
    assert_eq!(
        last_line(&app).text,
        "Stopped. The world is back as it was before Play."
    );
    assert_eq!(everything(&app), before);
    assert_eq!(app.selected, Some(pad), "the selection stays");
    // The history still works on the same Instances: undo the delete.
    app.undo();
    assert!(app.world.is_descendant_of(folder_id, app.world.workspace()));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn something_the_game_made_is_unselected_at_stop() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    app.play();
    play_for(&mut app, 3.1);
    let game = shown(&app.world, &app.play);
    let blue = game.find_first_child(game.workspace(), "BlueBall").unwrap();
    app.select_from_viewport(Some(blue));
    app.stop();
    assert_eq!(app.selected, None);
}

#[test]
fn play_saves_unsaved_changes_first_and_does_not_start_if_saving_fails() {
    let folder = copy_of_example("hello-touched", "play-save");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Name("Launch Pad".to_string()));
    app.play();
    assert!(app.playing());
    assert!(!app.unsaved());
    assert!(part_exists_on_disk(&folder, "Launch Pad"));
    assert!(printed(&app).iter().any(|line| line.starts_with("Saved")));
    app.stop();

    // With the folder gone, saving fails, so the game doesn't start.
    std::fs::remove_dir_all(&folder).unwrap();
    app.apply(pad, Edit::Name("Pad".to_string()));
    app.play();
    assert!(!app.playing());
    assert!(app.unsaved());
    assert_eq!(last_line(&app).kind, OutputKind::Error);
    assert!(last_line(&app).text.starts_with("Play didn't start"));
}

#[test]
fn play_runs_the_script_file_as_it_is_now() {
    let folder = copy_of_example("hello-touched", "play-reread");
    let mut app = EditorApp::new(Some(&folder));
    std::fs::write(
        folder.join("scripts/pad.server.lua"),
        "print('edited in another program')",
    )
    .unwrap();
    app.play();
    assert_eq!(last_line(&app).text, "edited in another program");
    app.stop();

    // A script file that's gone is reported, and the rest of the game runs.
    std::fs::remove_file(folder.join("scripts/pad.server.lua")).unwrap();
    app.play();
    assert!(app.playing());
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(
        line.text.starts_with("scripts/pad.server.lua won't run"),
        "{}",
        line.text
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_engine_panic_during_play_is_reported_and_goes_back_to_editing() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let before = everything(&app);
    app.play();
    play_for(&mut app, 1.0);
    app.play.as_mut().unwrap().panic_next_frame();
    app.play_frame(1.0 / 60.0, &[], None, None);

    assert!(!app.playing(), "back in edit mode");
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(
        line.text
            .starts_with("The engine crashed during Play: a deliberate engine panic"),
        "{}",
        line.text
    );
    assert_eq!(everything(&app), before);
    // The editor carries on: editing works, and so does playing again.
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
    assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
    app.undo();
    app.play();
    play_for(&mut app, 1.0);
    assert!(app.playing());
}

#[test]
fn only_properties_can_be_edited_while_playing_and_stop_undoes_them() {
    // A copy, because Play saves the unsaved edit below.
    let folder = copy_of_example("hello-touched", "play-read-only");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
    app.play();
    let before = everything(&app);

    // The Properties panel changes the game's world...
    app.apply(pad, Edit::Name("Door".to_string()));
    // ...and nothing else changes anything.
    app.act(Action::Delete(pad));
    app.act(Action::Duplicate(pad));
    app.act(Action::Insert {
        what: Insertable::Folder,
        near: None,
    });
    app.undo();
    app.selected = Some(pad);
    press(&mut app, egui::Modifiers::NONE, egui::Key::F2);
    assert_eq!(app.explorer.renaming(), None);
    let game = shown(&app.world, &app.play);
    assert_eq!(game.get(pad).unwrap().name(), "Door");
    assert!(game.is_descendant_of(pad, game.workspace()));
    assert!(game.find_first_child(game.workspace(), "Folder").is_none());
    assert_eq!(everything(&app), before, "no undo step, nothing unsaved");

    app.stop();
    assert_eq!(everything(&app), before);
    assert_eq!(app.world.get(pad).unwrap().name(), "Pad");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn editing_a_property_during_play_fires_changed_in_the_game() {
    // A copy: opening an example from an older format upgrades it.
    let folder = copy_of_example("hello-events", "edit-during-play");
    let mut app = EditorApp::new(Some(&folder));
    let lamp = part(&app, "Lamp");
    app.play();
    play_for(&mut app, 0.5);
    let changed = "Lamp's Color changed";
    assert!(!printed(&app).contains(&changed));

    let green = Value::Color3(Color3::new(0.1, 0.8, 0.2));
    app.apply(lamp, Edit::Property("Color", green.clone()));
    play_for(&mut app, 1.0 / 60.0);
    assert_eq!(last_line(&app).text, changed);
    let game = shown(&app.world, &app.play);
    assert_eq!(game.get(lamp).unwrap().property("Color"), Some(&green));

    // The edit world never changed, so there's nothing to save or undo.
    assert_ne!(property(&app, lamp, "Color"), green);
    assert!(!app.unsaved());
    assert_eq!(app.history.undo_label(), None);
}

#[test]
fn f5_plays_and_shift_f5_stops() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    press(&mut app, egui::Modifiers::SHIFT, egui::Key::F5);
    assert!(!app.playing());
    press(&mut app, egui::Modifiers::NONE, egui::Key::F5);
    assert!(app.playing());
    // F5 again does nothing; Shift+F5 isn't taken for F5.
    press(&mut app, egui::Modifiers::NONE, egui::Key::F5);
    assert!(app.playing());
    press(&mut app, egui::Modifiers::SHIFT, egui::Key::F5);
    assert!(!app.playing());

    // Nothing to play without a project.
    let mut empty = EditorApp::new(None);
    press(&mut empty, egui::Modifiers::NONE, egui::Key::F5);
    assert!(!empty.playing());
}
