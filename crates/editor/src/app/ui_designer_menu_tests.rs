//! The UI Designer's right-click menu has Rename and Delete, like the
//! Explorer's, and its empty tab says what a ScreenGui is.

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, artboard_point, click_named, designer_frame, drop_in, widget_named,
};
use super::*;
use crate::theme::icons;

/// Right-clicks `at` on the artboard, so its menu opens.
fn right_click(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64, at: egui::Pos2) {
    let size = SIZE;
    let secondary = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![secondary(true)],
        vec![secondary(false)],
        Vec::new(),
    ] {
        designer_frame(app, ctx, clock, size, events);
    }
}

const SIZE: egui::Vec2 = egui::vec2(1200.0, 800.0);

#[test]
fn delete_on_the_right_click_menu_deletes_as_one_undo_step() {
    let folder = copy_of_example("hello-touched", "ui-designer-menu-delete");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let button = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 100.0]);
    let other = drop_in(&mut app, Insertable::TextLabel, gui, [400.0, 100.0]);
    act(&mut app, DesignerAction::Select(Some(other)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    // Right-click the button (not selected) and pick Delete.
    let at = artboard_point(&app, gui, [120.0, 110.0]);
    right_click(&mut app, &ctx, &mut clock, at);
    assert_eq!(app.selected, Some(button), "the right-click selects it");
    click_named(
        &mut app,
        &ctx,
        &mut clock,
        SIZE,
        // A button's label is its text and its shortcut.
        &format!(
            "{} {}",
            theme::with_icon(icons::TRASH, "Delete"),
            crate::shortcuts::Action::Delete.menu_text(&ctx)
        ),
    );
    assert!(
        !app.world.is_descendant_of(button, gui),
        "the button is deleted"
    );
    assert!(
        app.world.is_descendant_of(other, gui),
        "only the one right-clicked"
    );
    app.undo();
    assert!(
        app.world.is_descendant_of(button, gui),
        "one undo brings it back"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn rename_on_the_right_click_menu_types_over_the_name_in_the_inspector() {
    let folder = copy_of_example("hello-touched", "ui-designer-menu-rename");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let button = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 100.0]);
    act(&mut app, DesignerAction::Select(Some(gui)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    let at = artboard_point(&app, gui, [120.0, 110.0]);
    right_click(&mut app, &ctx, &mut clock, at);
    click_named(
        &mut app,
        &ctx,
        &mut clock,
        SIZE,
        &theme::with_icon(icons::PENCIL_SIMPLE, "Rename"),
    );
    designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    assert!(
        ctx.wants_keyboard_input(),
        "the Name field has the keyboard"
    );
    // The old name is all selected, so typing replaces it.
    designer_frame(
        &mut app,
        &ctx,
        &mut clock,
        SIZE,
        vec![egui::Event::Text("Start".into())],
    );
    assert_eq!(app.world.get(button).unwrap().name(), "Start");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_empty_designer_says_what_a_screen_gui_is_and_makes_one() {
    let folder = copy_of_example("hello-touched", "ui-designer-empty");
    let mut app = EditorApp::new(Some(&folder));
    assert!(
        crate::ui_designer::screen_gui_of(&app.world, None).is_none()
            && app
                .world
                .descendants_iter(app.world.workspace())
                .all(|id| app.world.get(id).unwrap().class() != ClassName::ScreenGui),
        "the example has no ScreenGui"
    );
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let output = designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    let ppp = ctx.pixels_per_point();
    let said = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.value().or(node.label()))
        .any(|text| text.contains("2D interface"));
    assert!(said, "it says what the tab is for");
    let new = theme::with_icon(icons::PLUS, "New ScreenGui");
    assert!(widget_named(&output, &new, ppp).is_some());
    click_named(&mut app, &ctx, &mut clock, SIZE, &new);
    let made = app
        .selected
        .and_then(|id| app.world.get(id))
        .map(|i| i.class());
    assert_eq!(made, Some(ClassName::ScreenGui));
    let _ = std::fs::remove_dir_all(&folder);
}
