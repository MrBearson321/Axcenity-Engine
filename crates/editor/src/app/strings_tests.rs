//! The string table in the editor: binding a label's text to a key in the
//! UI Designer, the language it's shown in there and in Play, and the
//! Strings tab's edits, which undo and save as `strings.json`.

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, click_named, designer_frame, drop_in, frame_ends, stylesheets,
};
use super::*;

fn texts(layout: axcenity_ui::UiLayout) -> Vec<String> {
    layout
        .draw_items
        .into_iter()
        .filter_map(|item| match item {
            axcenity_render::UiDrawItem::Text(text) => Some(text.text),
            _ => None,
        })
        .collect()
}

/// The texts the ScreenGui `gui` draws in `world`, as the artboard lays it out.
fn artboard_texts(app: &EditorApp, world: &World, gui: InstanceId) -> Vec<String> {
    texts(axcenity_ui::layout_artboard(
        world,
        gui,
        &stylesheets(app),
        (1280, 720),
        1.0,
        [0.0, 0.0],
    ))
}

fn texts_in_play(app: &EditorApp) -> Vec<String> {
    let session = app.play.as_ref().expect("playing");
    texts(axcenity_ui::layout_ui(
        session.world(),
        1280,
        720,
        &stylesheets(app),
    ))
}

/// Whether a frame painted `text` (all of one piece of text, an icon aside).
fn drew_text(output: &egui::FullOutput, text: &str) -> bool {
    fn has(shape: &egui::Shape, text: &str) -> bool {
        match shape {
            egui::Shape::Text(shape) => {
                let drawn = shape.galley.job.text.as_str();
                drawn == text || drawn.ends_with(&format!("  {text}"))
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has(shape, text)),
            _ => false,
        }
    }
    output
        .shapes
        .iter()
        .any(|clipped| has(&clipped.shape, text))
}

fn count(texts: &[String], text: &str) -> usize {
    texts.iter().filter(|each| *each == text).count()
}

/// hello-strings' Menu ScreenGui and its Panel.
fn menu(app: &EditorApp) -> (InstanceId, InstanceId) {
    let gui = app
        .world
        .find_first_child(app.world.workspace(), "Menu")
        .unwrap();
    (gui, app.world.find_first_child(gui, "Panel").unwrap())
}

#[test]
fn a_label_bound_in_the_designer_shows_its_entry_in_the_designer_and_in_play() {
    let folder = copy_of_example("hello-strings", "strings-bound-label");
    let mut app = EditorApp::new(Some(&folder));
    let (gui, panel) = menu(&app);

    // A new label, bound with the inspector's Key to menu.play.
    let label = drop_in(&mut app, Insertable::TextLabel, panel, [0.0, 0.0]);
    let source = app
        .world
        .get(label)
        .and_then(|label| label.property("Text"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    assert!(artboard_texts(&app, &app.world, gui).contains(&source));
    act(
        &mut app,
        DesignerAction::Edit(
            label,
            Edit::Property("TextKey", Value::String("menu.play".into())),
        ),
    );
    let shown = artboard_texts(&app, &app.world, gui);
    assert!(!shown.contains(&source), "{shown:?}");
    assert_eq!(count(&shown, "Play"), 2, "{shown:?}");
    // Undo unbinds it.
    let bound = app.history.undo_label().map(str::to_owned);
    app.undo();
    assert!(artboard_texts(&app, &app.world, gui).contains(&source));
    app.redo();
    assert_eq!(app.history.undo_label().map(str::to_owned), bound);

    // The toolbar's language: every bound control on the artboard changes,
    // and it isn't an undo step.
    act(&mut app, DesignerAction::Language("Français".into()));
    assert_eq!(app.world.language(), "Français");
    assert_eq!(app.history.undo_label().map(str::to_owned), bound);
    let shown = artboard_texts(&app, &app.world, gui);
    assert_eq!(count(&shown, "Jouer"), 2, "{shown:?}");
    assert_eq!(count(&shown, "Bienvenue !"), 1, "{shown:?}");

    // The designer draws the Key and the language for the label.
    act(&mut app, DesignerAction::Select(Some(label)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 700.0);
    let mut output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    for _ in 0..2 {
        output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    for text in ["Key", "Shows its Français text", "menu.play", "Français"] {
        assert!(drew_text(&output, text), "{text} is drawn");
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(folder).join("strings-bound-label.png");
        crate::snapshot::snapshot_rendered(&path, size, vec![Vec::new(); 3], |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, Some(render)));
        });
    }

    // Play starts in the language the designer shows, and the picker
    // changes the game's while it runs.
    app.play();
    let in_play = texts_in_play(&app);
    assert_eq!(count(&in_play, "Jouer"), 2, "{in_play:?}");
    app.designer_actions(vec![DesignerAction::Language("Español".into())]);
    let in_play = texts_in_play(&app);
    assert_eq!(count(&in_play, "Jugar"), 2, "{in_play:?}");
    app.stop();
    assert_eq!(app.world.language(), "Français");

    // Play saved the project: the key and the table reload, and a game
    // starts in the table's first language.
    let project = axcenity_project::load(&folder).unwrap();
    let saved = project
        .world
        .find_first_child(project.world.workspace(), "Menu")
        .unwrap();
    let shown = artboard_texts(&app, &project.world, saved);
    assert_eq!(count(&shown, "Play"), 2, "{shown:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_strings_tab_edits_the_table_as_undo_steps_and_saves_it() {
    let folder = copy_of_example("hello-strings", "strings-tab");
    let mut app = EditorApp::new(Some(&folder));
    let original = app.world.strings().clone();
    let table = |app: &EditorApp| (**app.world.strings()).clone();

    // Adding a language is one step.
    let mut edited = table(&app);
    edited.add_language("Deutsch");
    act(
        &mut app,
        DesignerAction::Strings {
            key: String::new(),
            label: "Add language Deutsch".into(),
            table: edited,
        },
    );
    assert_eq!(app.history.undo_label(), Some("Add language Deutsch"));
    assert!(app.unsaved());

    // Typing "Spielen" into one box, a letter or two a frame, is one more step.
    for typed in ["S", "Sp", "Spiel", "Spielen"] {
        let mut edited = table(&app);
        edited.set("menu.play", "Deutsch", typed);
        app.designer_actions(vec![DesignerAction::Strings {
            key: "menu.play\nDeutsch".into(),
            label: "Edit menu.play (Deutsch)".into(),
            table: edited,
        }]);
    }
    frame_ends(&mut app);
    assert_eq!(
        app.world.strings().get("menu.play", "Deutsch"),
        Some("Spielen")
    );
    assert_eq!(app.history.undo_label(), Some("Edit menu.play (Deutsch)"));

    // Saving writes strings.json; reopening reads the same table.
    let with_german = app.world.strings().clone();
    assert!(app.save());
    assert!(!app.unsaved());
    let text = std::fs::read_to_string(folder.join(axcenity_project::STRINGS_FILE)).unwrap();
    assert!(text.contains("\"Deutsch\": \"Spielen\""), "{text}");
    let reopened = EditorApp::new(Some(&folder));
    assert_eq!(reopened.world.strings(), &with_german);

    // Undo takes the typing away in one step, then the language.
    app.undo();
    assert_eq!(app.world.strings().get("menu.play", "Deutsch"), None);
    assert!(app.world.strings().has_language("Deutsch"));
    app.undo();
    assert_eq!(app.world.strings(), &original);
    app.redo();
    app.redo();
    assert_eq!(app.world.strings(), &with_german);
    assert!(!app.unsaved());

    // Deleting a key: the control bound to it shows its own Text again.
    let (gui, panel) = menu(&app);
    let title = app.world.find_first_child(panel, "Title").unwrap();
    act(
        &mut app,
        DesignerAction::Edit(title, Edit::Property("Text", Value::String("Hi".into()))),
    );
    assert!(!artboard_texts(&app, &app.world, gui).contains(&"Hi".to_owned()));
    let mut edited = table(&app);
    edited.entries.remove("menu.title");
    act(
        &mut app,
        DesignerAction::Strings {
            key: String::new(),
            label: "Delete key menu.title".into(),
            table: edited,
        },
    );
    assert!(artboard_texts(&app, &app.world, gui).contains(&"Hi".to_owned()));
    app.undo();
    assert!(app.world.strings().entries.contains_key("menu.title"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_strings_tab_lists_the_keys_and_fits_any_shape() {
    let folder = copy_of_example("hello-strings", "strings-tab-shapes");
    let mut app = EditorApp::new(Some(&folder));
    let (gui, _) = menu(&app);
    act(&mut app, DesignerAction::Select(Some(gui)));
    for size in [egui::vec2(260.0, 900.0), egui::vec2(900.0, 260.0)] {
        app.ui_designer = UiDesigner::default();
        let ctx = theme::test_context();
        let mut clock = 0.0;
        click_named(&mut app, &ctx, &mut clock, size, "Strings");
        let mut output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
        for _ in 0..2 {
            output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
        }
        for text in [
            "LANGUAGES",
            "Add language",
            "Add key",
            "menu.english",
            "Français",
        ] {
            assert!(drew_text(&output, text), "{text} is drawn at {size:?}");
        }
        let used = ctx.globally_used_rect();
        assert!(
            used.width() <= size.x + 0.5,
            "at {size:?} the Strings tab is {used:?}"
        );
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let path = std::path::PathBuf::from(folder)
                .join(format!("strings-tab-{}x{}.png", size.x, size.y));
            crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, None));
            });
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
}
