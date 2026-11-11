//! The UI Designer's work on several elements, driven through the tab as
//! a creator would: a box selects buttons and Distribute spaces them, the
//! box around a selection moves it, Copy and Paste style, Duplicate, Ctrl+C
//! and Ctrl+V between ScreenGuis, and the arrow keys. Each is one undo step,
//! and none of the keys fire while a text field is typed in.

use axcenity_core::UDim2;

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, artboard_point, designer_frame, drag_on_artboard, drop_in, key, on_artboard, stylesheets,
    udim2, widget_named,
};
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1100.0, 700.0);

/// A new ScreenGui designed at 1280×720.
fn new_gui(app: &mut EditorApp) -> InstanceId {
    act(app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    act(
        app,
        DesignerAction::DesignSize {
            gui,
            width: 1280,
            height: 720,
        },
    );
    gui
}

/// A 120×40 button named `name` dropped into `parent` at `at`.
fn button(app: &mut EditorApp, parent: InstanceId, name: &str, at: [f32; 2]) -> InstanceId {
    let id = drop_in(app, Insertable::TextButton, parent, at);
    act(app, DesignerAction::Edit(id, Edit::Name(name.into())));
    let size = Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 40.0));
    act(app, DesignerAction::Edit(id, Edit::Property("Size", size)));
    id
}

fn x_of(app: &EditorApp, id: InstanceId) -> f32 {
    udim2(app, id, "Position").x.offset
}

/// Clicks at `at`, holding `modifiers`, ending each frame as the editor does.
fn click_with(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    at: egui::Pos2,
    modifiers: egui::Modifiers,
) {
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        }],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        }],
        Vec::new(),
    ] {
        *clock += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
                .chain(events)
                .collect(),
            time: Some(*clock),
            ..Default::default()
        };
        let _ = crate::theme::run(ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, None));
        });
        if !ctx.egui_is_using_pointer() && !ctx.egui_wants_keyboard_input() {
            app.history.seal();
            app.ui_drag_baselines.clear();
        }
    }
}

/// Clicks the designer's control named `label` (its tooltip, for an icon).
fn click(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64, label: &str) {
    let output = designer_frame(app, ctx, clock, SIZE, Vec::new());
    let at = widget_named(&output, label, ctx.pixels_per_point())
        .unwrap_or_else(|| panic!("{label} is drawn"));
    click_with(app, ctx, clock, at, egui::Modifiers::NONE);
}

/// One designer frame in which `events` happen with `modifiers` held,
/// returning egui's output.
fn keys(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    *clock += 0.05;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
            .chain(events)
            .collect(),
        time: Some(*clock),
        ..Default::default()
    };
    let output = crate::theme::run(ctx, input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, None));
    });
    app.history.seal();
    output
}

fn same_ids(mut a: Vec<InstanceId>, mut b: Vec<InstanceId>) -> bool {
    a.sort_by_key(|id| format!("{id:?}"));
    b.sort_by_key(|id| format!("{id:?}"));
    a == b
}

#[test]
fn box_selecting_three_buttons_and_distributing_them_spaces_them_evenly_in_one_step() {
    let folder = copy_of_example("hello-touched", "ui-designer-distribute");
    let mut app = EditorApp::new(Some(&folder));
    let gui = new_gui(&mut app);
    let buttons = [
        button(&mut app, gui, "Play", [100.0, 300.0]),
        button(&mut app, gui, "Options", [300.0, 300.0]),
        button(&mut app, gui, "Quit", [900.0, 300.0]),
    ];
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }

    // A box dragged on empty artboard around all three selects them.
    let from = artboard_point(&app, gui, [60.0, 260.0]);
    let to = artboard_point(&app, gui, [1060.0, 380.0]);
    for events in drag_on_artboard(from, to) {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, events);
    }
    assert!(
        same_ids(app.selection.clone(), buttons.to_vec()),
        "{:?}",
        app.selection
    );
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        // Drawn by the game's renderer, with the box around all three.
        let designer = std::mem::take(&mut app.ui_designer);
        crate::snapshot::snapshot_rendered(
            &snapshots.join("ui-designer-multi-select.png"),
            SIZE,
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
        app.ui_designer = designer;
    }

    // From 100 to 1020 with 360 of buttons: gaps of 280.
    let before = app.history.undo_label().map(str::to_owned);
    click(&mut app, &ctx, &mut clock, "Distribute horizontally");
    assert_eq!(buttons.map(|id| x_of(&app, id)), [100.0, 500.0, 900.0]);
    assert_eq!(app.history.undo_label(), Some("Distribute 3 elements"));
    app.undo();
    assert_eq!(buttons.map(|id| x_of(&app, id)), [100.0, 300.0, 900.0]);
    assert_eq!(app.history.undo_label().map(str::to_owned), before);
    app.redo();
    assert_eq!(buttons.map(|id| x_of(&app, id)), [100.0, 500.0, 900.0]);

    // Align lines up their top edges after one is moved down.
    act(
        &mut app,
        DesignerAction::Edit(
            buttons[1],
            Edit::Property("Position", Value::UDim2(UDim2::new(0.0, 500.0, 0.0, 333.0))),
        ),
    );
    click(&mut app, &ctx, &mut clock, "Align top edges");
    assert!(
        buttons
            .iter()
            .all(|&id| udim2(&app, id, "Position").y.offset == 300.0)
    );

    // Pressing inside the selection and dragging moves all three together,
    // as one step.
    for _ in 0..2 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    let at = on_artboard(&app, gui, buttons[1]);
    let starts = buttons.map(|id| udim2(&app, id, "Position"));
    for events in drag_on_artboard(at, at + egui::vec2(0.0, 90.0)) {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, events);
    }
    let moved = buttons.map(|id| udim2(&app, id, "Position").y.offset);
    assert!(moved[0] > starts[0].y.offset + 20.0, "{moved:?}");
    assert!(moved.iter().all(|&y| y == moved[0]), "{moved:?}");
    assert!(
        buttons
            .iter()
            .zip(starts)
            .all(|(&id, start)| udim2(&app, id, "Position").x == start.x),
        "a drag straight down doesn't move them across"
    );
    assert_eq!(app.history.undo_label(), Some("Move 3 elements"));
    app.undo();
    assert_eq!(buttons.map(|id| udim2(&app, id, "Position")), starts);

    // Ctrl-click takes one away; Shift-click puts it back.
    let quit = on_artboard(&app, gui, buttons[2]);
    click_with(&mut app, &ctx, &mut clock, quit, egui::Modifiers::COMMAND);
    assert!(same_ids(app.selection.clone(), buttons[..2].to_vec()));
    click_with(&mut app, &ctx, &mut clock, quit, egui::Modifiers::SHIFT);
    assert!(same_ids(app.selection.clone(), buttons.to_vec()));
    assert_eq!(app.selected, Some(buttons[2]));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn resizing_the_box_around_two_buttons_resizes_both_as_one_step() {
    let folder = copy_of_example("hello-touched", "ui-designer-group-resize");
    let mut app = EditorApp::new(Some(&folder));
    let gui = new_gui(&mut app);
    let left = button(&mut app, gui, "Left", [100.0, 100.0]);
    let right = button(&mut app, gui, "Right", [300.0, 100.0]);
    act(&mut app, DesignerAction::SelectMany(vec![left, right]));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, Vec::new());
    }
    // The box's right edge, from x 420 out to 740: twice as wide.
    let edge = artboard_point(&app, gui, [420.0, 120.0]);
    let to = artboard_point(&app, gui, [740.0, 120.0]);
    for events in drag_on_artboard(edge, to) {
        designer_frame(&mut app, &ctx, &mut clock, SIZE, events);
    }
    let (width, x) = (
        udim2(&app, right, "Size").x.offset,
        udim2(&app, right, "Position").x.offset,
    );
    assert!((width - 240.0).abs() <= 2.0, "{width}");
    assert!((x - 500.0).abs() <= 2.0, "{x}");
    assert!((udim2(&app, left, "Size").x.offset - 240.0).abs() <= 2.0);
    assert_eq!(x_of(&app, left), 100.0);
    assert_eq!(app.history.undo_label(), Some("Resize 2 elements"));
    app.undo();
    assert_eq!(udim2(&app, right, "Size").x.offset, 120.0);
    assert_eq!(x_of(&app, right), 300.0);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn paste_style_matches_a_button_and_duplicate_copies_its_own_id_and_rule() {
    let folder = copy_of_example("hello-touched", "ui-designer-paste-style");
    let mut app = EditorApp::new(Some(&folder));
    let gui = new_gui(&mut app);
    let play = button(&mut app, gui, "Play", [100.0, 100.0]);
    let quit = button(&mut app, gui, "Quit", [100.0, 200.0]);
    let style = |app: &mut EditorApp, id, property: &'static str, value: &str| {
        act(
            app,
            DesignerAction::TextStyle {
                id,
                property,
                value: value.into(),
                dragging: false,
            },
        );
    };
    style(&mut app, play, "background-color", "#3a86ff");
    style(&mut app, play, "border-radius", "8px");
    act(
        &mut app,
        DesignerAction::StateStyle {
            id: play,
            state: axcenity_ui::UiState::Hover,
            property: "background-color",
            value: "#1f5fd1".into(),
            dragging: false,
        },
    );
    style(&mut app, quit, "background-color", "#ef476f");
    let looks = |app: &EditorApp, id| -> Vec<(String, String)> {
        axcenity_ui::applied_declarations(&app.world, id, &stylesheets(app))
            .into_iter()
            .map(|declaration| (declaration.property, declaration.value))
            .collect()
    };

    // Copy style from Play, Paste style onto Quit.
    let ctx = theme::test_context();
    let mut clock = 0.0;
    act(&mut app, DesignerAction::Select(Some(play)));
    click(&mut app, &ctx, &mut clock, "Copy style");
    act(&mut app, DesignerAction::Select(Some(quit)));
    let before = app.history.undo_label().map(str::to_owned);
    let quit_before = looks(&app, quit);
    click(&mut app, &ctx, &mut clock, "Paste style");
    assert_eq!(looks(&app, quit), looks(&app, play));
    assert_eq!(app.history.undo_label(), Some("Paste style onto Quit"));
    app.undo();
    assert_eq!(looks(&app, quit), quit_before);
    assert_eq!(app.history.undo_label().map(str::to_owned), before);
    app.redo();
    assert_eq!(looks(&app, quit), looks(&app, play));

    // Ctrl+D: an identical button with its own Id and rules.
    act(&mut app, DesignerAction::Select(Some(play)));
    let input = egui::RawInput {
        events: std::iter::once(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND))
            .chain(vec![key(egui::Key::D, egui::Modifiers::COMMAND)])
            .collect(),
        ..Default::default()
    };
    let _ = crate::theme::run(&egui::Context::default(), input, |ui| {
        let ctx = &ui.ctx().clone();
        app.shortcuts(ctx)
    });
    let copy = app.selected.unwrap();
    assert_ne!(copy, play);
    let id_of = |app: &EditorApp, id| {
        crate::ui_designer::arrange::element_id(&app.world, id)
            .unwrap()
            .to_owned()
    };
    assert_eq!(id_of(&app, play), "Play");
    assert_eq!(id_of(&app, copy), "Play2");
    let sheets = stylesheets(&app);
    assert_eq!(
        axcenity_ui::own_rules(&sheets, "Play2"),
        axcenity_ui::own_rules(&sheets, "Play"),
        "its own rule and its :hover rule"
    );
    assert_eq!(looks(&app, copy), looks(&app, play));
    for property in ["Position", "Size"] {
        assert_eq!(udim2(&app, copy, property), udim2(&app, play, property));
    }
    assert_eq!(
        app.world.get(copy).unwrap().property("Text"),
        app.world.get(play).unwrap().property("Text")
    );
    assert_eq!(app.history.undo_label(), Some("Duplicate Play"));
    app.undo();
    assert!(!app.world.is_descendant_of(copy, gui));
    assert!(axcenity_ui::own_rules(&stylesheets(&app), "Play2").is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn ctrl_c_and_ctrl_v_copy_a_button_into_another_screen_gui() {
    let folder = copy_of_example("hello-touched", "ui-designer-clipboard");
    let mut app = EditorApp::new(Some(&folder));
    let menu = new_gui(&mut app);
    let play = button(&mut app, menu, "Play", [100.0, 100.0]);
    act(
        &mut app,
        DesignerAction::TextStyle {
            id: play,
            property: "background-color",
            value: "#3a86ff".into(),
            dragging: false,
        },
    );
    let hud = new_gui(&mut app);

    let ctx = theme::test_context();
    let mut clock = 0.0;
    act(&mut app, DesignerAction::Select(Some(play)));
    let output = keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![egui::Event::Copy],
        egui::Modifiers::COMMAND,
    );
    let copied = output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("Ctrl+C puts the elements on the clipboard");

    // The window's Paste, with what Ctrl+C put on the clipboard.
    act(&mut app, DesignerAction::Select(Some(hud)));
    let before = app.history.undo_label().map(str::to_owned);
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![egui::Event::Paste(copied)],
        egui::Modifiers::COMMAND,
    );
    let pasted = app.selected.unwrap();
    let instance = app.world.get(pasted).unwrap();
    assert_eq!(instance.parent(), Some(hud));
    assert_eq!(instance.name(), "Play");
    let element_id = crate::ui_designer::arrange::element_id(&app.world, pasted)
        .unwrap()
        .to_owned();
    assert_eq!(element_id, "Play2");
    assert_eq!(
        udim2(&app, pasted, "Position"),
        udim2(&app, play, "Position")
    );
    let sheets = stylesheets(&app);
    assert_eq!(
        axcenity_ui::own_rules(&sheets, "Play2"),
        axcenity_ui::own_rules(&sheets, "Play")
    );
    assert_eq!(app.history.undo_label(), Some("Paste Play"));
    app.undo();
    assert!(!app.world.is_descendant_of(pasted, hud));
    assert!(axcenity_ui::own_rules(&stylesheets(&app), "Play2").is_empty());
    assert_eq!(app.history.undo_label().map(str::to_owned), before);

    // Ctrl+V as a key works too (and Paste text that isn't ours doesn't).
    act(&mut app, DesignerAction::Select(Some(hud)));
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![egui::Event::Paste("hello".into())],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(app.selected, Some(hud));
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![key(egui::Key::V, egui::Modifiers::COMMAND)],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(app.world.get(hud).unwrap().children().len(), 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn arrow_keys_nudge_by_one_and_shift_by_ten_but_not_while_typing() {
    let folder = copy_of_example("hello-touched", "ui-designer-nudge");
    let mut app = EditorApp::new(Some(&folder));
    let gui = new_gui(&mut app);
    let play = button(&mut app, gui, "Play", [100.0, 100.0]);
    let quit = button(&mut app, gui, "Quit", [100.0, 200.0]);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    act(&mut app, DesignerAction::Select(Some(play)));
    let before = app.history.undo_label().map(str::to_owned);

    let none = egui::Modifiers::NONE;
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![key(egui::Key::ArrowRight, none)],
        none,
    );
    assert_eq!(
        udim2(&app, play, "Position"),
        UDim2::new(0.0, 101.0, 0.0, 100.0)
    );
    assert_eq!(app.history.undo_label(), Some("Nudge Play"));
    let shift = egui::Modifiers::SHIFT;
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![key(egui::Key::ArrowDown, shift)],
        shift,
    );
    assert_eq!(
        udim2(&app, play, "Position"),
        UDim2::new(0.0, 101.0, 0.0, 110.0)
    );
    assert_eq!(app.history.undo_label(), Some("Nudge Play by 10"));
    app.undo();
    assert_eq!(
        udim2(&app, play, "Position"),
        UDim2::new(0.0, 101.0, 0.0, 100.0)
    );
    app.undo();
    assert_eq!(app.history.undo_label().map(str::to_owned), before);

    // The whole selection, as one step.
    act(&mut app, DesignerAction::SelectMany(vec![play, quit]));
    keys(
        &mut app,
        &ctx,
        &mut clock,
        vec![key(egui::Key::ArrowLeft, none)],
        none,
    );
    assert_eq!(x_of(&app, play), 99.0);
    assert_eq!(x_of(&app, quit), 99.0);
    assert_eq!(app.history.undo_label(), Some("Nudge 2 elements"));

    // While a text field is being typed in, the keys are its: not the
    // designer's arrows, Ctrl+C or Ctrl+V, nor the editor's Ctrl+D.
    let state = format!(
        "{:?}",
        (udim2(&app, play, "Position"), udim2(&app, quit, "Position"))
    );
    let steps = app.history.undo_label().map(str::to_owned);
    let children = app.world.get(gui).unwrap().children().len();
    let mut text = String::new();
    for events in [
        vec![key(egui::Key::ArrowRight, none)],
        vec![key(egui::Key::ArrowDown, shift)],
        vec![
            egui::Event::Copy,
            key(egui::Key::C, egui::Modifiers::COMMAND),
        ],
        vec![key(egui::Key::V, egui::Modifiers::COMMAND)],
        vec![key(egui::Key::D, egui::Modifiers::COMMAND)],
    ] {
        clock += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            time: Some(clock),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            // As the editor's frame does: its shortcuts, then the panels.
            app.shortcuts(ctx);
            egui::CentralPanel::default().show(ui, |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
                app.ui_designer_pane(ui, None);
            });
        });
    }
    assert_eq!(
        format!(
            "{:?}",
            (udim2(&app, play, "Position"), udim2(&app, quit, "Position"))
        ),
        state
    );
    assert_eq!(app.history.undo_label().map(str::to_owned), steps);
    assert_eq!(app.world.get(gui).unwrap().children().len(), children);
    let _ = std::fs::remove_dir_all(&folder);
}
