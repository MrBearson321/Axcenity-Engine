//! Keyboard and controller focus in Play, and the UI Designer's focus
//! arrows that set `NextSelectionUp`, `Down`, `Left` and `Right`.

use axcenity_core::Value;

use super::tests::{copy_of_example, play_for, printed};
use super::ui_designer_tests::act;
use super::*;

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Lays the game's UI out in the viewport, as drawing it does each frame.
fn lay_out(app: &mut EditorApp) {
    let sheets = app.project.as_ref().unwrap().stylesheets.clone();
    let world = shown(&app.world, &app.play);
    app.viewport.lay_out_ui(world, &sheets, (1280, 720));
}

fn named(app: &EditorApp, name: &str) -> InstanceId {
    let world = shown(&app.world, &app.play);
    world
        .find_first_descendant(world.workspace(), name)
        .unwrap()
}

/// One frame of input: the keys the game UI takes come out of `events`
/// before egui sees them, and are used before the game frame.
fn frame(app: &mut EditorApp, events: Vec<egui::Event>) -> Vec<egui::Event> {
    let mut input = egui::RawInput {
        events,
        ..Default::default()
    };
    app.take_nav_keys(&egui::Context::default(), &mut input);
    app.navigate_play_ui(1.0 / 60.0);
    play_for(app, 1.0 / 60.0);
    lay_out(app);
    input.events
}

#[test]
fn in_play_tab_arrows_and_enter_move_focus_and_click_and_other_keys_reach_the_game() {
    let folder = copy_of_example("hello-menu", "navigation");
    let mut app = EditorApp::new(Some(&folder));
    let none = egui::Modifiers::NONE;

    // Not while editing: the keys stay egui's.
    let mut input = egui::RawInput {
        events: vec![key(egui::Key::Tab, none)],
        ..Default::default()
    };
    app.take_nav_keys(&egui::Context::default(), &mut input);
    assert_eq!(input.events.len(), 1);

    app.play();
    lay_out(&mut app);
    // Nothing is focused yet, so the arrows are the game's; Tab isn't.
    let left = frame(&mut app, vec![key(egui::Key::ArrowDown, none)]);
    assert_eq!(left.len(), 1, "the down arrow goes on to the game");
    assert_eq!(app.viewport.ui_selected(), None);
    let left = frame(
        &mut app,
        vec![key(egui::Key::Tab, none), key(egui::Key::W, none)],
    );
    assert_eq!(left.len(), 1, "W goes on to the game");
    assert_eq!(app.viewport.ui_selected(), Some(named(&app, "PlayerName")));

    // Down twice reaches Options, and Enter clicks it.
    frame(&mut app, vec![key(egui::Key::ArrowDown, none)]);
    frame(&mut app, vec![key(egui::Key::ArrowDown, none)]);
    assert_eq!(app.viewport.ui_selected(), Some(named(&app, "Options")));
    let left = frame(&mut app, vec![key(egui::Key::Enter, none)]);
    assert!(left.is_empty());
    assert!(
        printed(&app).contains(&"Options pressed"),
        "{:?}",
        printed(&app)
    );

    // Shift+Tab goes back; Enter on the name box types into it, and
    // Escape (as the app's typing does) fires FocusLost.
    frame(&mut app, vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)]);
    frame(&mut app, vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)]);
    let name = named(&app, "PlayerName");
    assert_eq!(app.viewport.ui_selected(), Some(name));
    frame(&mut app, vec![key(egui::Key::Enter, none)]);
    assert_eq!(app.focused_text, Some(name));
    // While typing, nothing is taken: the keys are the text box's.
    let left = frame(&mut app, vec![key(egui::Key::ArrowDown, none)]);
    assert_eq!(left.len(), 1);
    // A controller's B finishes typing as Escape does.
    app.nav_keys.push(axcenity_ui::NavKey::Back);
    app.navigate_play_ui(1.0 / 60.0);
    assert_eq!(app.focused_text, None);
    let focus_lost = app.pending_focus_lost.take();
    app.play_frame(1.0 / 60.0, &[], None, focus_lost);
    assert!(printed(&app).contains(&"Hello, "), "{:?}", printed(&app));
    assert_eq!(app.viewport.ui_selected(), Some(name), "focus comes back");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dragging_a_focus_arrow_onto_an_element_points_it_there_in_one_undo_step() {
    let folder = copy_of_example("hello-menu", "focus-arrows");
    let mut app = EditorApp::new(Some(&folder));
    let play = named(&app, "Play");
    let status = named(&app, "Status");
    let property = |app: &EditorApp, id: InstanceId, name: &str| {
        app.world.get(id).unwrap().property(name).cloned().unwrap()
    };

    // Onto an element with no Id: it gets one from its name, in the same step.
    app.apply(status, Edit::Property("Id", Value::String(String::new())));
    app.history.seal();
    act(
        &mut app,
        DesignerAction::NextSelection {
            id: play,
            property: "NextSelectionUp",
            target: Some(status),
        },
    );
    assert_eq!(property(&app, status, "Id"), Value::String("Status".into()));
    assert_eq!(
        property(&app, play, "NextSelectionUp"),
        Value::String("Status".into())
    );
    app.undo();
    assert_eq!(
        property(&app, play, "NextSelectionUp"),
        Value::String("".into())
    );
    assert_eq!(property(&app, status, "Id"), Value::String("".into()));
    app.redo();
    assert_eq!(
        property(&app, play, "NextSelectionUp"),
        Value::String("Status".into())
    );

    // Onto nothing clears it.
    act(
        &mut app,
        DesignerAction::NextSelection {
            id: play,
            property: "NextSelectionUp",
            target: None,
        },
    );
    assert_eq!(
        property(&app, play, "NextSelectionUp"),
        Value::String("".into())
    );

    // It saves.
    let quit = named(&app, "Quit");
    act(
        &mut app,
        DesignerAction::NextSelection {
            id: quit,
            property: "NextSelectionRight",
            target: Some(play),
        },
    );
    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let quit = named(&reopened, "Quit");
    assert_eq!(
        property(&reopened, quit, "NextSelectionRight"),
        Value::String("play".into())
    );
    assert_eq!(
        property(&reopened, quit, "NextSelectionDown"),
        Value::String("playername".into())
    );
    let _ = std::fs::remove_dir_all(&folder);
}
