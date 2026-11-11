//! The Console tab counts errors you haven't seen (design system V3).

use super::tests::{drawn_texts, example};
use super::*;

fn frame(app: &mut EditorApp, ctx: &egui::Context) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    crate::theme::run(ctx, input, |ui| app.frame(ui, None))
}

fn count_drawn(app: &mut EditorApp, ctx: &egui::Context, count: usize) -> bool {
    let output = frame(app, ctx);
    let badge = format!("{} {count}", icons::WARNING_CIRCLE);
    drawn_texts(&output)
        .iter()
        .any(|(text, _)| text.contains("Console") && text.ends_with(&badge))
}

#[test]
fn the_console_tab_counts_errors_until_the_console_shows() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    // Assets in front of the Console.
    app.layout.open(Pane::Assets);
    for _ in 0..3 {
        frame(&mut app, &ctx);
    }
    assert!(!count_drawn(&mut app, &ctx, 1), "no errors, no count");
    app.output.error("scripts/Script.server.lua:1: first");
    app.output.error("scripts/Script.server.lua:2: second");
    frame(&mut app, &ctx);
    assert!(
        count_drawn(&mut app, &ctx, 2),
        "two unseen errors on the tab"
    );
    assert_eq!(app.output.unseen_errors(&ctx), 2);
    // Showing the Console clears it; a later error counts from there.
    app.layout.open(Pane::Console);
    for _ in 0..2 {
        frame(&mut app, &ctx);
    }
    assert_eq!(app.output.unseen_errors(&ctx), 0);
    app.layout.open(Pane::Assets);
    frame(&mut app, &ctx);
    app.output.error("scripts/Script.server.lua:3: third");
    frame(&mut app, &ctx);
    assert!(count_drawn(&mut app, &ctx, 1), "only the new one");
}
