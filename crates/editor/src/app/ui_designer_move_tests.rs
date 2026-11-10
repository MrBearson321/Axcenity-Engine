//! Moving elements between frames from the UI Designer's Hierarchy (they
//! stay where they are on screen), and the inspector's X, Y, W and H.

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, designer_frame, drop_in, key, press, rect_on_artboard, udim2, widget_named,
};
use super::*;
use axcenity_core::UDim2;

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

/// A ScreenGui with a Frame "Card" at 100, 100 and a TextLabel "Title" at
/// 500, 300 beside it, both straight in the ScreenGui.
fn card_and_title(app: &mut EditorApp) -> (InstanceId, InstanceId, InstanceId) {
    act(app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let card = drop_in(app, Insertable::Frame, gui, [100.0, 100.0]);
    act(app, DesignerAction::Edit(card, Edit::Name("Card".into())));
    let title = drop_in(app, Insertable::TextLabel, gui, [500.0, 300.0]);
    act(app, DesignerAction::Edit(title, Edit::Name("Title".into())));
    (gui, card, title)
}

/// Drags from `from` to `to` over several frames, as a mouse does.
fn drag(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    from: egui::Pos2,
    to: egui::Pos2,
) {
    let mut frames = vec![
        vec![egui::Event::PointerMoved(from)],
        vec![press(from, true)],
    ];
    for step in 1..=4 {
        frames.push(vec![egui::Event::PointerMoved(
            from + (to - from) * (step as f32 / 4.0),
        )]);
    }
    frames.push(vec![press(to, false)]);
    frames.push(Vec::new());
    for events in frames {
        designer_frame(app, ctx, clock, SIZE, events);
    }
}

fn near(a: egui::Rect, b: egui::Rect) -> bool {
    (a.min - b.min).length() < 0.5 && (a.max - b.max).length() < 0.5
}

#[test]
fn moving_an_element_into_a_frame_keeps_it_where_it_is_on_screen() {
    let folder = copy_of_example("hello-touched", "ui-designer-move-keeps");
    let mut app = EditorApp::new(Some(&folder));
    let (gui, card, title) = card_and_title(&mut app);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    let before = rect_on_artboard(&app, gui, title);
    let output = designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    let ppp = ctx.pixels_per_point();
    let from = widget_named(&output, "Title", ppp).expect("Title's row is drawn");
    let to = widget_named(&output, "Card", ppp).expect("Card's row is drawn");

    drag(&mut app, &ctx, &mut clock, from, to);
    designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    assert_eq!(
        app.world.get(title).unwrap().parent(),
        Some(card),
        "dropped on the Frame's row, it's inside the Frame"
    );
    assert!(
        near(rect_on_artboard(&app, gui, title), before),
        "it hasn't moved on screen: {:?} then {:?}",
        before,
        rect_on_artboard(&app, gui, title)
    );
    let position = udim2(&app, title, "Position");
    assert_eq!(
        (position.x.offset, position.y.offset),
        (400.0, 200.0),
        "its Position is from the Frame's corner now"
    );
    assert_eq!(app.history.undo_label(), Some("Move Title into Card"));

    app.undo();
    assert_eq!(app.world.get(title).unwrap().parent(), Some(gui));
    assert_eq!(
        udim2(&app, title, "Position"),
        UDim2::new(0.0, 500.0, 0.0, 300.0),
        "one undo puts it back, Position and all"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_drop_on_an_element_that_isnt_a_frame_goes_into_its_frame_and_a_frame_cant_go_in_itself() {
    let folder = copy_of_example("hello-touched", "ui-designer-move-targets");
    let mut app = EditorApp::new(Some(&folder));
    let (gui, card, title) = card_and_title(&mut app);
    let world = &app.world;
    // A label isn't a container: a drop on it goes where it is.
    assert_eq!(
        crate::ui_designer::hierarchy_drop_target(world, gui, &[card], title),
        None,
        "Title is in the ScreenGui, where Card is already"
    );
    assert_eq!(
        crate::ui_designer::hierarchy_drop_target(world, gui, &[title], card),
        Some(card)
    );
    assert_eq!(
        crate::ui_designer::hierarchy_drop_target(world, gui, &[card], card),
        None,
        "not into itself"
    );
    act(
        &mut app,
        DesignerAction::MoveInto {
            ids: vec![title],
            parent: card,
        },
    );
    let button = drop_in(&mut app, Insertable::TextButton, gui, [50.0, 600.0]);
    assert_eq!(
        crate::ui_designer::hierarchy_drop_target(&app.world, gui, &[button], title),
        Some(card),
        "dropped on Title, the button goes into Title's Frame"
    );
    assert_eq!(
        crate::ui_designer::hierarchy_drop_target(&app.world, gui, &[card], title),
        None,
        "a Frame can't go into itself through its child's row"
    );
    // Moving the Frame and its child together moves the Frame; the child
    // goes along inside it.
    let other = drop_in(&mut app, Insertable::Frame, gui, [700.0, 500.0]);
    act(
        &mut app,
        DesignerAction::MoveInto {
            ids: vec![card, title],
            parent: other,
        },
    );
    assert_eq!(app.world.get(card).unwrap().parent(), Some(other));
    assert_eq!(app.world.get(title).unwrap().parent(), Some(card));
    let _ = std::fs::remove_dir_all(&folder);
}

/// Clicks the inspector's number box named `name` and types `value`.
fn type_number(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64, name: &str, value: &str) {
    let output = designer_frame(app, ctx, clock, SIZE, Vec::new());
    let at = widget_named(&output, name, ctx.pixels_per_point())
        .unwrap_or_else(|| panic!("{name} is drawn"));
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
        Vec::new(),
        vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
        vec![egui::Event::Text(value.into())],
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        Vec::new(),
    ] {
        designer_frame(app, ctx, clock, SIZE, events);
    }
}

#[test]
fn x_y_w_and_h_move_and_resize_in_design_pixels_keeping_the_anchor() {
    let folder = copy_of_example("hello-touched", "ui-designer-xywh");
    let mut app = EditorApp::new(Some(&folder));
    let (gui, card, _) = card_and_title(&mut app);
    // Pinned to the right: X still means pixels from the parent's left.
    let pinned = crate::ui_designer::anchor_action(
        &app.world,
        &app.project.as_ref().unwrap().stylesheets,
        gui,
        card,
        crate::ui_designer::Pin::End,
        crate::ui_designer::Pin::Start,
    )
    .unwrap();
    act(&mut app, pinned);
    act(&mut app, DesignerAction::Select(Some(card)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    let (board, zoom) = app.ui_designer.board().unwrap();
    let left_before = rect_on_artboard(&app, gui, card).left();

    type_number(&mut app, &ctx, &mut clock, "X", "300");
    let position = udim2(&app, card, "Position");
    assert_eq!(position.x.scale, 1.0, "still pinned to the right");
    let moved = rect_on_artboard(&app, gui, card);
    assert!(
        ((moved.left() - board.min.x) / zoom - 300.0).abs() < 0.5,
        "its left edge is 300 design pixels in, was {}",
        (left_before - board.min.x) / zoom
    );
    assert_eq!(app.history.undo_label(), Some("Move Card"));

    type_number(&mut app, &ctx, &mut clock, "W", "320");
    let resized = rect_on_artboard(&app, gui, card);
    assert!((resized.width() / zoom - 320.0).abs() < 0.5);
    assert!(
        (resized.left() - moved.left()).abs() < 0.5,
        "its left edge stays"
    );
    assert_eq!(app.history.undo_label(), Some("Resize Card"));
    app.undo();
    assert!(
        near(rect_on_artboard(&app, gui, card), moved),
        "one undo takes the typed width back"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
