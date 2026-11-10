//! The UI Designer's inspector, beginner-first: everyday sections first,
//! the fine-tuning ones (States, Bevel, Slice) folded until opened, and a
//! folded section's title saying what the element has there.

use axcenity_ui::UiState;

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, build_menu, click_named, designer_frame, key, open_section, stylesheets, widget_named,
};
use super::*;
use crate::theme::icons;
use crate::ui_designer::{Section, sections};

/// A section header's label, as AccessKit reads it.
fn header(icon: &str, title: &str) -> String {
    theme::with_icon(icon, title)
}

#[test]
fn the_inspector_lists_everyday_sections_first_and_the_fine_tuning_last() {
    let folder = copy_of_example("hello-touched", "ui-inspector-order");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let sheets = stylesheets(&app);
    use Section::*;
    // A button: looks, pin, text, picture, see-through, code, then states,
    // bevel and slice.
    assert_eq!(
        sections(&app.world, menu.gui, menu.play, &sheets),
        [
            Styles, Anchor, Text, Image, Look, Code, States, Bevel, Slice
        ]
    );
    // A text label has no states, picture or slice.
    assert_eq!(
        sections(&app.world, menu.gui, menu.title, &sheets),
        [Styles, Anchor, Text, Look, Code, Bevel]
    );
    // A frame has no text; it can be sliced.
    assert_eq!(
        sections(&app.world, menu.gui, menu.frame, &sheets),
        [Styles, Anchor, Look, Code, Bevel, Slice]
    );
    // The ScreenGui itself has none of these.
    assert!(sections(&app.world, menu.gui, menu.gui, &sheets).is_empty());
    // Only the fine-tuning ones start folded.
    let folded: Vec<Section> = [
        Styles, Anchor, Text, Widget, Image, Look, Code, States, Bevel, Slice,
    ]
    .into_iter()
    .filter(|section| !section.starts_open())
    .collect();
    assert_eq!(folded, [States, Bevel, Slice]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folded_section_opens_on_its_header_stays_open_and_names_what_it_has() {
    let folder = copy_of_example("hello-touched", "ui-inspector-fold");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(&mut app, DesignerAction::Select(Some(menu.play)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 2400.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    let ppp = ctx.pixels_per_point();
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    // Everyday sections are open: Styles' Import and the anchor's Reset.
    let import = header(icons::DOWNLOAD_SIMPLE, "Import");
    assert!(widget_named(&output, &import, ppp).is_some());
    assert!(widget_named(&output, "Reset", ppp).is_some());
    // The fine-tuning ones show only their headers.
    assert!(
        widget_named(&output, "Raised", ppp).is_none(),
        "Bevel is folded"
    );
    assert!(
        widget_named(&output, "Hover", ppp).is_none(),
        "States is folded"
    );
    let bevel = header(icons::SQUARE_HALF, "Bevel");
    assert!(widget_named(&output, &bevel, ppp).is_some());

    // Clicking the header opens it.
    click_named(&mut app, &ctx, &mut clock, size, &bevel);
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert!(
        widget_named(&output, "Raised", ppp).is_some(),
        "Bevel opened"
    );

    // Raised: the header says so, and it stays open on another element.
    click_named(&mut app, &ctx, &mut clock, size, "Raised");
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert!(
        widget_named(&output, &header(icons::SQUARE_HALF, "Bevel · Raised"), ppp).is_some(),
        "a bevel shows in the header"
    );
    act(&mut app, DesignerAction::Select(Some(menu.quit)));
    designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert!(
        widget_named(&output, "Raised", ppp).is_some(),
        "Bevel stays open for the next element"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn with_states_folded_the_text_toolbar_edits_the_normal_look() {
    let folder = copy_of_example("hello-touched", "ui-inspector-states");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(&mut app, DesignerAction::Select(Some(menu.play)));
    let own = app
        .world
        .get(menu.play)
        .and_then(|play| play.property("Id"))
        .and_then(Value::as_str)
        .expect("the button has an Id")
        .to_owned();
    let hover_rule = crate::ui_designer::state_selector(&own, UiState::Hover);
    // The Hover tab was picked earlier, then States was folded.
    app.ui_designer.show_in_state(UiState::Hover, false);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 2400.0);
    for _ in 0..2 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    let bold = || key(egui::Key::B, egui::Modifiers::COMMAND);
    designer_frame(&mut app, &ctx, &mut clock, size, vec![bold()]);
    designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    let sheets = stylesheets(&app);
    assert!(
        !axcenity_ui::has_rule(&sheets, &hover_rule),
        "folded, Ctrl+B doesn't write a hover rule"
    );
    let weight = |state| {
        axcenity_ui::style_value_in_state(&app.world, menu.play, &sheets, state, "font-weight")
    };
    assert_eq!(weight(UiState::Normal).as_deref(), Some("bold"));

    // Opened, the picked tab is edited again.
    open_section(&ctx, Section::States);
    for _ in 0..2 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    designer_frame(&mut app, &ctx, &mut clock, size, vec![bold()]);
    designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert!(
        axcenity_ui::has_rule(&stylesheets(&app), &hover_rule),
        "open, Ctrl+B writes the Hover tab's rule"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
