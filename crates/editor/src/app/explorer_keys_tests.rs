//! The Explorer's keys in the editor (design system V6): once a row is
//! clicked the arrows and Enter are the Explorer's, and the editor's own
//! shortcuts (tool keys, Delete) still work.

use super::tests::example;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| app.frame(ctx, None));
}

fn press(app: &mut EditorApp, ctx: &egui::Context, key: egui::Key) {
    let event = |pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(app, ctx, vec![event(true), event(false)]);
    frame(app, ctx, Vec::new());
}

fn click(app: &mut EditorApp, ctx: &egui::Context, at: egui::Pos2) {
    frame(app, ctx, vec![egui::Event::PointerMoved(at)]);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    frame(app, ctx, Vec::new());
}

#[test]
fn a_clicked_explorer_takes_the_arrows_and_enter_and_shortcuts_still_work() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec::new());
    }
    ctx.memory_mut(|memory| memory.stop_text_input());
    let children = app
        .world
        .get(app.world.workspace())
        .unwrap()
        .children()
        .to_vec();
    let first = children[0];
    let at = app
        .explorer
        .row_rect(first)
        .expect("the Workspace's first child has a row")
        .center();
    click(&mut app, &ctx, at);
    assert_eq!(app.selected, Some(first));

    press(&mut app, &ctx, egui::Key::ArrowDown);
    let rows: Vec<_> = app
        .explorer
        .rows(&app.world)
        .iter()
        .map(|row| row.id)
        .collect();
    let next = rows[rows.iter().position(|id| *id == first).unwrap() + 1];
    assert_eq!(app.selected, Some(next), "Down selects the next row");
    assert_eq!(app.selection, [next]);
    press(&mut app, &ctx, egui::Key::ArrowUp);
    assert_eq!(app.selected, Some(first));

    press(&mut app, &ctx, egui::Key::Enter);
    assert_eq!(app.explorer.renaming(), Some(first), "Enter renames");
    press(&mut app, &ctx, egui::Key::Escape);
    assert_eq!(app.explorer.renaming(), None);
    assert_eq!(app.selection, [first], "Escape leaves the selection");

    // The tools' keys and Delete aren't the Explorer's.
    press(&mut app, &ctx, egui::Key::Num2);
    assert_eq!(app.tool, Tool::Move, "2 still picks Move");
    assert_eq!(app.selection, [first], "after 2");
    press(&mut app, &ctx, egui::Key::Delete);
    assert!(
        !app.world.is_descendant_of(first, app.world.root()),
        "Delete still deletes"
    );
}
