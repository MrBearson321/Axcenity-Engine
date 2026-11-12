use super::*;
use crate::script_completion::Definition;
use crate::scripts::{ScriptTabs, editor_id};
use crate::theme;

fn typed(assist: &mut Assist, text: &mut String, keys: Keys) -> Option<usize> {
    let cursor = text.len();
    assist.update(text, Some(cursor), true, keys, &World::default(), false)
}

#[test]
fn a_dot_opens_the_list_typing_narrows_it_and_enter_puts_the_pick_in() {
    let world = World::default();
    let mut assist = Assist::default();
    let mut text = "local door: Part\ndoor.".to_owned();
    typed(&mut assist, &mut text, Keys::default());
    assert!(assist.labels().contains(&"Anchored"));

    text.push_str("Anch");
    typed(&mut assist, &mut text, Keys::default());
    assert_eq!(assist.labels(), ["Anchored"]);

    let accept = Keys {
        accept: true,
        ..Keys::default()
    };
    let at = text.len();
    let cursor = assist.update(&mut text, Some(at), false, accept, &world, false);
    assert_eq!(text, "local door: Part\ndoor.Anchored");
    assert_eq!(cursor, Some(text.len()));
    assert!(!assist.is_open());
}

#[test]
fn the_list_closes_on_escape_on_other_typing_and_when_the_cursor_moves() {
    let world = World::default();
    let mut assist = Assist::default();
    let mut text = "workspace.".to_owned();
    let open = Keys {
        open: true,
        ..Keys::default()
    };
    let close = Keys {
        close: true,
        ..Keys::default()
    };

    typed(&mut assist, &mut text, Keys::default());
    assert!(assist.is_open());
    typed(&mut assist, &mut text, close);
    assert!(!assist.is_open());

    // Ctrl+Space opens it anywhere a name can go.
    text.push_str("\npri");
    typed(&mut assist, &mut text, open);
    assert!(assist.labels().contains(&"print"));
    // A bracket isn't part of a name.
    text.push('(');
    typed(&mut assist, &mut text, Keys::default());
    assert!(!assist.is_open());

    text.push_str("wor");
    typed(&mut assist, &mut text, open);
    assert!(assist.labels().contains(&"workspace"));
    assert!(
        assist
            .update(&mut text, Some(3), false, Keys::default(), &world, false)
            .is_none()
    );
    assert!(!assist.is_open());
}

#[test]
fn up_and_down_wrap_around_the_list() {
    let mut assist = Assist::default();
    let mut text = "workspace.".to_owned();
    typed(&mut assist, &mut text, Keys::default());
    let count = assist.labels().len();
    let last = assist.labels()[count - 1].to_owned();
    let up = Keys {
        up: true,
        ..Keys::default()
    };
    let at = text.len();
    assist.update(&mut text, Some(at), false, up, &World::default(), false);
    let accept = Keys {
        accept: true,
        ..Keys::default()
    };
    assist.update(&mut text, Some(at), false, accept, &World::default(), false);
    assert_eq!(text, format!("workspace.{last}"));
}

#[test]
fn the_list_stays_open_while_it_is_being_clicked() {
    let mut assist = Assist::default();
    let mut text = "workspace.".to_owned();
    typed(&mut assist, &mut text, Keys::default());
    // Pressing on the list took focus from the text box.
    assist.update(
        &mut text,
        None,
        false,
        Keys::default(),
        &World::default(),
        true,
    );
    assert!(assist.is_open());
    let first = assist.labels()[0].to_owned();
    let cursor = assist.last_cursor().unwrap();
    let after = assist.accept(&mut text, cursor, 0);
    assert_eq!(text, format!("workspace.{first}"));
    assert_eq!(after, text.len());
}

#[test]
fn a_bracket_or_comma_shows_the_signature_on_its_argument() {
    let mut assist = Assist::default();
    let mut text = "workspace:FindFirstChild(".to_owned();
    typed(&mut assist, &mut text, Keys::default());
    let signature = assist.signature().unwrap().clone();
    assert_eq!(signature.argument, 0);
    let range = argument_range(&signature.label, 0).unwrap();
    assert!(signature.label[range].starts_with("name"));

    text.push_str("\"Door\",");
    typed(&mut assist, &mut text, Keys::default());
    let signature = assist.signature().unwrap().clone();
    assert_eq!(signature.argument, 1);
    let range = argument_range(&signature.label, 1).unwrap();
    assert!(signature.label[range].starts_with("recursive"));

    text.push_str(" true)");
    typed(&mut assist, &mut text, Keys::default());
    assert!(assist.signature().is_none());
}

#[test]
fn arguments_split_at_the_outer_commas_only() {
    let label = "f(a: number, b: {x: number, y: number}, c: (string) -> ()): ()";
    let part = |index| argument_range(label, index).map(|range| &label[range]);
    assert_eq!(part(0), Some("a: number"));
    assert_eq!(part(1), Some("b: {x: number, y: number}"));
    assert_eq!(part(2), Some("c: (string) -> ()"));
    assert_eq!(part(3), None);
    assert_eq!(argument_range("f()", 0), None);
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Draws the code tab for `path` with `events`, putting the cursor at the
/// end of `source` the first time.
fn draw(ui: &mut egui::Ui, tabs: &mut ScriptTabs, path: &str, source: &str) {
    if egui::TextEdit::load_state(ui.ctx(), editor_id(path)).is_none() {
        let mut state = egui::text_edit::TextEditState::default();
        let end = egui::text::CCursor::new(source.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ui.ctx(), editor_id(path));
        ui.memory_mut(|memory| memory.request_focus(editor_id(path)));
    }
    egui::CentralPanel::default().show(ui, |ui| tabs.show(ui, path, &World::default()));
}

fn frame(
    ctx: &egui::Context,
    tabs: &mut ScriptTabs,
    path: &str,
    source: &str,
    events: Vec<egui::Event>,
) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(640.0, 480.0),
        )),
        events,
        ..Default::default()
    };
    let _ = theme::run(ctx, input, |ui| draw(ui, tabs, path, source));
}

/// Typing in a real code tab: `.` opens the list over the text, Down picks
/// the second item, and Enter puts it in instead of a new line.
#[test]
fn the_code_tab_completes_from_the_keyboard() {
    let path = "scripts/door.server.lua";
    let source = "local door: Part\ndoor";
    let frames = vec![
        Vec::new(),
        vec![egui::Event::Text(".".to_owned())],
        Vec::new(),
        vec![key(egui::Key::ArrowDown)],
    ];
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let mut tabs = ScriptTabs::default();
        tabs.open(path, source, None);
        crate::snapshot::snapshot(
            &std::path::PathBuf::from(folder).join("script-completion.png"),
            egui::vec2(640.0, 480.0),
            frames.clone(),
            |ui| draw(ui, &mut tabs, path, source),
        );
    }
    let mut tabs = ScriptTabs::default();
    tabs.open(path, source, None);
    let ctx = theme::test_context();
    for events in frames {
        frame(&ctx, &mut tabs, path, source, events);
    }
    let tab = tabs.get(path).unwrap();
    assert!(tab.assist.is_open(), "the list is open after `.`");
    let second = tab.assist.labels()[1].to_owned();

    frame(&ctx, &mut tabs, path, source, vec![key(egui::Key::Enter)]);
    let tab = tabs.get(path).unwrap();
    assert_eq!(tab.text, format!("{source}.{second}"));
    assert!(!tab.assist.is_open());
}

/// F12 in a real code tab with the cursor on a local's use moves to the
/// line it's declared on; on an API member it asks the editor to open the
/// API Reference (G4's Go to Definition).
#[test]
fn f12_goes_to_a_local_declaration_or_hands_out_an_api_entry() {
    let path = "scripts/door.server.lua";
    // The cursor starts at the end, just after the use of `open`.
    let source = "local open = false\n\nprint(open";
    let mut tabs = ScriptTabs::default();
    tabs.open(path, source, None);
    let ctx = theme::test_context();
    frame(&ctx, &mut tabs, path, source, Vec::new());
    frame(&ctx, &mut tabs, path, source, vec![key(egui::Key::F12)]);
    let tab = tabs.get(path).unwrap();
    assert_eq!(tab.highlighted, Some(1), "the declaration's line is shown");
    assert_eq!(tabs.take_definition(), None, "nothing to open outside");

    let path = "scripts/wait.server.lua";
    let source = "task.wait";
    tabs.open(path, source, None);
    frame(&ctx, &mut tabs, path, source, Vec::new());
    frame(&ctx, &mut tabs, path, source, vec![key(egui::Key::F12)]);
    let Some(Definition::Api(name)) = tabs.take_definition() else {
        panic!("an API entry to open");
    };
    assert!(name.contains("wait"), "{name}");
}
