//! The tool windows (`widgets::tool_window`): each opens over the editor
//! and Escape closes it, after a focused text field has had its Escape.

use super::tests::example;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let _ = crate::theme::run(ctx, input, |ui| app.frame(ui, None));
}

fn escape() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

type Window = (&'static str, fn(&mut EditorApp), fn(&EditorApp) -> bool);

#[test]
fn every_tool_window_opens_and_escape_closes_it() {
    let windows: [Window; 9] = [
        (
            "Settings",
            |app| app.settings.open = true,
            |app| app.settings.open,
        ),
        (
            "Input",
            |app| app.input_settings.open = true,
            |app| app.input_settings.open,
        ),
        (
            "Collision Groups",
            |app| app.collision_groups.open = true,
            |app| app.collision_groups.open,
        ),
        (
            "Version History",
            |app| app.version_history.open = true,
            |app| app.version_history.open,
        ),
        (
            "Find in Scripts",
            |app| app.find.show_and_focus(),
            |app| app.find.open,
        ),
        (
            "New Custom Material",
            |app| app.custom_material = Some(Default::default()),
            |app| app.custom_material.is_some(),
        ),
        (
            "References",
            |app| app.find_asset_references("scripts/Script.server.lua"),
            |app| app.references.is_some(),
        ),
        ("Learn", |app| app.learn_open = true, |app| app.learn_open),
        (
            "New Tab",
            |app| app.ribbon_tabs.edit(None),
            |app| app.ribbon_tabs.is_editing(),
        ),
    ];
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec::new());
    }
    for (name, open, is_open) in windows {
        open(&mut app);
        for _ in 0..2 {
            frame(&mut app, &ctx, Vec::new());
        }
        assert!(is_open(&app), "{name} opens");
        if name == "Find in Scripts" {
            assert!(
                ctx.egui_wants_keyboard_input(),
                "Find in Scripts focuses its field"
            );
        }
        // A focused text field takes the first Escape and lets go of focus.
        if ctx.egui_wants_keyboard_input() {
            frame(&mut app, &ctx, vec![escape()]);
            assert!(is_open(&app), "{name}: Escape leaves its text field first");
            assert!(!ctx.egui_wants_keyboard_input(), "{name}");
        }
        frame(&mut app, &ctx, vec![escape()]);
        assert!(!is_open(&app), "Escape closes {name}");
    }
}

/// Input's "Press a key…" takes Escape to stop listening, and the window
/// stays open.
#[test]
fn escape_while_input_listens_for_a_key_keeps_the_window() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    app.input_settings.open = true;
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec::new());
    }
    app.input_settings.listening = Some("Jump".into());
    frame(&mut app, &ctx, vec![escape()]);
    assert!(
        app.input_settings.listening.is_none(),
        "Escape stops listening"
    );
    assert!(app.input_settings.open, "and doesn't close the window");
    frame(&mut app, &ctx, vec![escape()]);
    assert!(!app.input_settings.open, "the next Escape does");
}

/// Two windows open: Escape closes the one on top only.
#[test]
fn escape_closes_only_the_window_on_top() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    app.collision_groups.open = true;
    for _ in 0..2 {
        frame(&mut app, &ctx, Vec::new());
    }
    app.input_settings.open = true;
    for _ in 0..2 {
        frame(&mut app, &ctx, Vec::new());
    }
    frame(&mut app, &ctx, vec![escape()]);
    assert!(!app.input_settings.open, "the newer window closes");
    assert!(app.collision_groups.open, "the one under it stays");
    frame(&mut app, &ctx, vec![escape()]);
    assert!(!app.collision_groups.open);
}
