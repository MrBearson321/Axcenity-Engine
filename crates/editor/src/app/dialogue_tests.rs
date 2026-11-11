//! Dialogues in the editor (Phase 77): the Dialogue tab follows the
//! selected prompt, makes and saves dialogues, and gives one to a prompt.

use axcenity_core::Value;

use super::tests::{copy_of_example, property};
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1200.0, 800.0);

fn frame(app: &mut EditorApp, ctx: &egui::Context) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        ..Default::default()
    };
    let _ = crate::theme::run(ctx, input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.dialogue_pane(ui));
    });
}

#[test]
fn the_tab_follows_the_farmers_prompt_and_a_new_dialogue_saves_and_goes_on_it() {
    let folder = copy_of_example("hello-dialogue", "dialogue-tab");
    let mut app = EditorApp::new(Some(&folder));
    let farmer = app
        .world
        .find_first_child(app.world.workspace(), "Farmer")
        .unwrap();
    let body = app.world.find_first_child(farmer, "Body").unwrap();
    let prompt = app.world.find_first_child(body, "ProximityPrompt").unwrap();

    // Selecting the Farmer shows the dialogue his prompt starts.
    app.select_clicked(Some(farmer), egui::Modifiers::NONE, None, false);
    let ctx = theme::test_context();
    frame(&mut app, &ctx);
    let shown = "assets/dialogue/Farmer.dialogue.json";
    assert_eq!(app.dialogues.current(), Some(shown));
    let farmer_graph = &app.dialogues.document(shown).unwrap().graph;
    assert_eq!(crate::dialogue_editor::check(farmer_graph), Ok(()));
    let selected = app.selected_prompt().unwrap();
    assert_eq!((selected.id, selected.name.as_str()), (prompt, "Farmer"));

    // New dialogue: named after him, with a number since his is taken. His
    // prompt keeps its dialogue until Use is pressed.
    app.new_dialogue(Some(&selected));
    let made = "assets/dialogue/Farmer2.dialogue.json";
    assert_eq!(app.dialogues.current(), Some(made));
    assert!(app.layout.is_open(Pane::Dialogue));
    assert_eq!(
        property(&app, prompt, "Dialogue"),
        Value::String(shown.into())
    );
    assert!(app.unsaved());
    frame(&mut app, &ctx);
    app.use_dialogue(prompt, made);
    assert_eq!(
        property(&app, prompt, "Dialogue"),
        Value::String(made.into())
    );
    app.undo();
    assert_eq!(
        property(&app, prompt, "Dialogue"),
        Value::String(shown.into())
    );
    app.redo();

    // Saved with the project, ready to run.
    assert!(app.save());
    let text = std::fs::read_to_string(folder.join(made)).unwrap();
    assert!(axcenity_gameplay::dialogue::Dialogue::from_json(&text).is_ok());
    assert!(!app.unsaved());
}
