//! The UI Designer with layered, scrolled, turned and safe-area UI: bring to
//! front, the wheel on a ScrollingFrame, the rotate knob, device safe insets,
//! and a settings page of widgets that works the same in Play.

use axcenity_core::UDim2;

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, add_style, artboard_point, click_named, designer_frame, drop_in, frame_ends, press,
    rect_on_artboard, run_designer, state, stylesheets,
};
use super::*;
use crate::ui_handles::Handles;

#[test]
fn bring_to_front_swaps_overlapping_buttons_and_the_top_one_takes_the_click_in_the_designer_and_in_play()
 {
    let folder = copy_of_example("hello-touched", "ui-designer-layers");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    // Buttons are 160 × 48: these overlap from 140, 120 to 260, 148.
    let back = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 100.0]);
    let front = drop_in(&mut app, Insertable::TextButton, gui, [140.0, 120.0]);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1200.0, 800.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    let click = |app: &mut EditorApp, clock: &mut f64, at: egui::Pos2| {
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![press(at, true)],
            vec![press(at, false)],
            Vec::new(),
        ] {
            designer_frame(app, &ctx, clock, size, events);
        }
    };
    let overlap = artboard_point(&app, gui, [200.0, 135.0]);
    click(&mut app, &mut clock, overlap);
    assert_eq!(app.selected, Some(front), "the later one is on top");

    // Right-click the one underneath where it shows, and Bring to Front.
    let only_back = artboard_point(&app, gui, [110.0, 110.0]);
    let secondary = |pressed| egui::Event::PointerButton {
        pos: only_back,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![egui::Event::PointerMoved(only_back)],
        vec![secondary(true)],
        vec![secondary(false)],
        Vec::new(),
    ] {
        designer_frame(&mut app, &ctx, &mut clock, size, events);
    }
    click_named(&mut app, &ctx, &mut clock, size, "Bring to Front");
    let z = |app: &EditorApp, id| app.world.get(id).unwrap().property("ZIndex").cloned();
    assert_eq!(z(&app, back), Some(Value::Number(2.0)));
    assert_eq!(
        z(&app, front),
        Some(Value::Number(1.0)),
        "only the one moved changes"
    );
    assert_eq!(
        app.history.undo_label(),
        Some("Change layer order of TextButton")
    );
    click(&mut app, &mut clock, overlap);
    assert_eq!(app.selected, Some(back), "now it's on top on the artboard");

    // One undo step puts it back.
    app.undo();
    assert_eq!(z(&app, back), Some(Value::Number(1.0)));
    app.redo();
    assert_eq!(z(&app, back), Some(Value::Number(2.0)));

    // In Play the one on top takes the click.
    app.play();
    let world = app.play.as_ref().expect("playing").world();
    let (width, height) = axcenity_ui::design_size(world, gui);
    let layout = axcenity_ui::layout_ui(world, width, height, &stylesheets(&app));
    assert_eq!(layout.hit_button(200.0, 135.0), Some(back));
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_wheel_scrolls_a_scrolling_frame_on_the_artboard_without_changing_its_canvas_position() {
    let folder = copy_of_example("hello-touched", "ui-designer-scrolling");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let list = drop_in(&mut app, Insertable::ScrollingFrame, gui, [100.0, 100.0]);
    // 300 × 200, its canvas twice as tall: a button near its bottom.
    let low = drop_in(&mut app, Insertable::TextButton, list, [20.0, 340.0]);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1200.0, 800.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    let undo = app.history.undo_label().map(str::to_owned);
    let over = artboard_point(&app, gui, [250.0, 200.0]);
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -60.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    let board_before = app.ui_designer.board().unwrap().0;
    designer_frame(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![egui::Event::PointerMoved(over)],
    );
    designer_frame(&mut app, &ctx, &mut clock, size, vec![wheel]);
    for _ in 0..20 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    let scrolled = app
        .ui_designer
        .scrolled(list)
        .expect("scrolled on the artboard");
    assert!(scrolled[1] > 0.0, "{scrolled:?}");
    assert_eq!(
        app.ui_designer.board().unwrap().0,
        board_before,
        "the artboard didn't pan"
    );
    assert_eq!(
        app.world.get(list).unwrap().property("CanvasPosition"),
        Some(&Value::Vector2(axcenity_core::Vector2::new(0.0, 0.0))),
        "the saved position is the same"
    );
    assert_eq!(
        app.history.undo_label().map(str::to_owned),
        undo,
        "no undo step"
    );
    // Clicking where the low button now shows selects it.
    let shown = artboard_point(&app, gui, [140.0, 100.0 + 340.0 - scrolled[1] + 10.0]);
    for events in [
        vec![egui::Event::PointerMoved(shown)],
        vec![press(shown, true)],
        vec![press(shown, false)],
        Vec::new(),
    ] {
        designer_frame(&mut app, &ctx, &mut clock, size, events);
    }
    assert_eq!(app.selected, Some(low));
    let _ = std::fs::remove_dir_all(&folder);
}

/// Runs `frames` of the designer as [`run_designer`] does, with `modifiers`
/// held throughout.
fn run_designer_holding(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    size: egui::Vec2,
    frames: Vec<Vec<egui::Event>>,
    modifiers: egui::Modifiers,
) {
    for events in frames {
        *clock += 0.05;
        let events: Vec<egui::Event> = events
            .into_iter()
            .map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                },
                other => other,
            })
            .collect();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
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
            frame_ends(app);
        }
    }
}

#[test]
fn a_frame_turned_with_the_rotate_knob_draws_turned_and_clicks_hit_its_turned_shape() {
    let folder = copy_of_example("hello-touched", "ui-designer-rotate");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let frame = drop_in(&mut app, Insertable::Frame, gui, [300.0, 200.0]);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1200.0, 800.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let undo_before = app.history.undo_label().map(str::to_owned);

    // Grab the knob just outside its top right corner and swing it 33°
    // around its middle with Shift held: it snaps to 30°.
    let rect = rect_on_artboard(&app, gui, frame);
    let centre = rect.center();
    let knob = rect.right_top() + egui::vec2(12.0, -12.0) / std::f32::consts::SQRT_2;
    let swing = |degrees: f32| {
        let [x, y] = axcenity_render::turn_point([knob.x, knob.y], [centre.x, centre.y], degrees);
        egui::pos2(x, y)
    };
    let frames = vec![
        vec![egui::Event::PointerMoved(knob)],
        vec![press(knob, true)],
        vec![egui::Event::PointerMoved(swing(15.0))],
        vec![egui::Event::PointerMoved(swing(33.0))],
        vec![press(swing(33.0), false)],
        Vec::new(),
    ];
    run_designer_holding(
        &mut app,
        &ctx,
        &mut clock,
        size,
        frames,
        egui::Modifiers::SHIFT,
    );
    let rotation = |app: &EditorApp| {
        app.world
            .get(frame)
            .unwrap()
            .property("Rotation")
            .and_then(Value::as_number)
    };
    assert_eq!(rotation(&app), Some(30.0));
    assert_eq!(app.selected, Some(frame), "turning keeps the selection");
    // The whole drag is one undo step.
    assert_eq!(app.history.undo_label(), Some("Change Rotation of Frame"));

    // The artboard draws it turned, and a click where only the turned
    // frame is selects it.
    let sheets = stylesheets(&app);
    let (board, zoom) = app.ui_designer.board().unwrap();
    let screen = axcenity_ui::design_size(&app.world, gui);
    let layout = axcenity_ui::layout_artboard(
        &app.world,
        gui,
        &sheets,
        screen,
        zoom,
        [board.min.x, board.min.y],
    );
    let element = *layout.element(frame).unwrap();
    assert!((element.degrees() - 30.0).abs() < 1e-3);
    assert!(layout.draw_items.iter().any(|item| matches!(
        item,
        axcenity_render::UiDrawItem::Rectangle(r) if r.rotation_degrees == 30.0
    )));
    // Its bottom right corner, turned 30° down past the upright box.
    let inside = element.on_screen([
        element.rect.x + element.rect.width - 2.0,
        element.rect.y + element.rect.height - 2.0,
    ]);
    let inside = egui::pos2(inside[0], inside[1]);
    assert!(inside.y > rect.bottom(), "{inside:?} {rect:?}");
    act(&mut app, DesignerAction::Select(Some(gui)));
    for events in [
        vec![egui::Event::PointerMoved(inside)],
        vec![press(inside, true)],
        vec![press(inside, false)],
        Vec::new(),
    ] {
        designer_frame(&mut app, &ctx, &mut clock, size, events);
    }
    assert_eq!(app.selected, Some(frame));
    assert_eq!(rotation(&app), Some(30.0), "a click doesn't turn it");
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        let label = drop_in(&mut app, Insertable::TextLabel, frame, [10.0, 10.0]);
        act(
            &mut app,
            DesignerAction::Edit(
                label,
                Edit::Property("Text", Value::String("Turned 30°".into())),
            ),
        );
        act(&mut app, DesignerAction::Select(Some(frame)));
        // A small design size, so the artboard is zoomed in on it.
        act(
            &mut app,
            DesignerAction::DesignSize {
                gui,
                width: 640,
                height: 400,
            },
        );
        crate::snapshot::snapshot_rendered(
            &folder.join("ui-designer-rotated.png"),
            size,
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
        app.undo();
        app.undo();
        app.undo();
        act(&mut app, DesignerAction::Select(Some(frame)));
        run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 2]);
    }

    // Without Shift it follows the pointer freely.
    let handles_knob = {
        let handles = Handles::new(element, &app.world, &sheets, egui::Pos2::ZERO, 1.0);
        handles.on_screen(rect.right_top() + egui::vec2(12.0, -12.0) / std::f32::consts::SQRT_2)
    };
    let free = |degrees: f32| {
        let [x, y] = axcenity_render::turn_point(
            [handles_knob.x, handles_knob.y],
            [centre.x, centre.y],
            degrees,
        );
        egui::pos2(x, y)
    };
    let frames = vec![
        vec![egui::Event::PointerMoved(handles_knob)],
        vec![press(handles_knob, true)],
        vec![egui::Event::PointerMoved(free(4.0))],
        vec![egui::Event::PointerMoved(free(7.0))],
        vec![press(free(7.0), false)],
        Vec::new(),
    ];
    run_designer(&mut app, &ctx, &mut clock, size, frames);
    let free_turn = rotation(&app).unwrap();
    assert!((free_turn - 37.0).abs() < 0.3, "{free_turn}");
    app.undo();
    assert_eq!(rotation(&app), Some(30.0));

    // In Play it's drawn turned and takes clicks by its turned shape.
    app.play();
    let world = app.play.as_ref().expect("playing").world();
    let (width, height) = axcenity_ui::design_size(world, gui);
    let played = axcenity_ui::layout_ui(world, width, height, &sheets);
    let in_play = *played.element(frame).unwrap();
    assert!((in_play.degrees() - 30.0).abs() < 1e-3);
    let corner = in_play.on_screen([
        in_play.rect.x + in_play.rect.width - 2.0,
        in_play.rect.y + in_play.rect.height - 2.0,
    ]);
    assert!(corner[1] > in_play.rect.y + in_play.rect.height);
    assert_eq!(played.hit(corner[0], corner[1]), Some(frame));
    let upright_corner = [
        in_play.rect.x + in_play.rect.width - 2.0,
        in_play.rect.y + 2.0,
    ];
    assert_ne!(
        played.hit(upright_corner[0], upright_corner[1]),
        Some(frame)
    );
    app.stop();

    app.undo();
    assert_eq!(rotation(&app), Some(0.0));
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_top_bar_in_a_device_safe_insets_gui_sits_below_the_notch_on_a_phone_preview() {
    let folder = copy_of_example("hello-touched", "ui-designer-safe-area");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    // Designed for a phone, in Scale.
    act(
        &mut app,
        DesignerAction::DesignSize {
            gui,
            width: 390,
            height: 844,
        },
    );
    let bar = drop_in(&mut app, Insertable::Frame, gui, [0.0, 0.0]);
    act(
        &mut app,
        DesignerAction::Edit(
            bar,
            Edit::Property("Size", Value::UDim2(UDim2::new(1.0, 0.0, 0.0, 60.0))),
        ),
    );
    act(
        &mut app,
        DesignerAction::Edit(
            gui,
            Edit::Property("ScreenInsets", Value::String("DeviceSafeInsets".into())),
        ),
    );
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1200.0, 900.0);
    // What a click at (x, y) on the previewed screen, in its pixels, selects.
    let click = |app: &mut EditorApp, clock: &mut f64, [x, y]: [f32; 2]| {
        act(app, DesignerAction::Select(None));
        let (board, zoom) = app.ui_designer.board().unwrap();
        let at = board.min + egui::vec2(x, y) * zoom;
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![press(at, true)],
            vec![press(at, false)],
            Vec::new(),
        ] {
            designer_frame(app, &ctx, clock, size, events);
        }
        app.selected
    };

    // On a phone in portrait, the top 47 pixels are the notch: the bar
    // starts under it.
    app.ui_designer.preview_at(Some((390, 844)));
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    assert_ne!(click(&mut app, &mut clock, [195.0, 20.0]), Some(bar));
    assert_eq!(click(&mut app, &mut clock, [195.0, 47.0 + 20.0]), Some(bar));
    assert_eq!(crate::ui_designer::safe_insets((390, 844))[0], 47.0);
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        crate::snapshot::snapshot_rendered(
            &folder.join("ui-designer-safe-area.png"),
            size,
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }

    // On a desktop screen there's no notch: the bar is at the top.
    app.ui_designer.preview_at(Some((1280, 720)));
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    assert_eq!(click(&mut app, &mut clock, [640.0, 20.0]), Some(bar));
    // And in Play, on the desktop player's screen.
    app.play();
    let world = app.play.as_ref().expect("playing").world();
    let played = axcenity_ui::layout_ui(world, 1280, 720, &stylesheets(&app));
    assert_eq!(played.element(bar).unwrap().rect.y, 0.0);
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_settings_page_built_in_the_designer_alone_works_the_same_in_play() {
    use axcenity_ui::{UiInput, UiInteraction};
    let folder = copy_of_example("hello-touched", "ui-designer-settings");
    let mut app = EditorApp::new(Some(&folder));
    let before = state(&app);
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    act(
        &mut app,
        DesignerAction::DesignSize {
            gui,
            width: 1280,
            height: 720,
        },
    );
    let page = drop_in(&mut app, Insertable::Frame, gui, [340.0, 160.0]);
    add_style(&mut app, page, "width", "600px");
    add_style(&mut app, page, "height", "400px");
    // Each widget comes from the designer's Elements.
    for widget in [
        Insertable::Slider,
        Insertable::Toggle,
        Insertable::Dropdown,
        Insertable::ProgressBar,
    ] {
        assert!(
            crate::ui_designer::PALETTE
                .iter()
                .any(|(what, _)| *what == widget)
        );
    }
    let volume = drop_in(&mut app, Insertable::Slider, page, [40.0, 40.0]);
    let fullscreen = drop_in(&mut app, Insertable::Toggle, page, [40.0, 100.0]);
    let quality = drop_in(&mut app, Insertable::Dropdown, page, [40.0, 160.0]);
    let loading = drop_in(&mut app, Insertable::ProgressBar, page, [40.0, 240.0]);
    add_style(&mut app, quality, "background-color", "#111827");
    add_style(&mut app, quality, "color", "#f9fafb");

    // The inspector's Widget section sets each one up.
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 1800.0);
    let edit = |app: &mut EditorApp, id, name, value| {
        act(app, DesignerAction::Edit(id, Edit::Property(name, value)));
    };
    edit(&mut app, volume, "Step", Value::Number(10.0));
    edit(
        &mut app,
        quality,
        "Options",
        Value::String("Low\nMedium\nHigh".into()),
    );
    edit(&mut app, quality, "Value", Value::String("Medium".into()));
    edit(&mut app, loading, "Value", Value::Number(30.0));
    act(&mut app, DesignerAction::Select(Some(fullscreen)));
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    click_named(&mut app, &ctx, &mut clock, size, "Checkbox");
    let style = |app: &EditorApp, id: InstanceId, name: &str| {
        app.world.get(id).unwrap().property(name).cloned().unwrap()
    };
    assert_eq!(
        style(&app, fullscreen, "Style"),
        Value::String("Checkbox".into())
    );
    act(&mut app, DesignerAction::Select(Some(loading)));
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    click_named(&mut app, &ctx, &mut clock, size, "Vertical");
    assert_eq!(
        style(&app, loading, "Orientation"),
        Value::String("Vertical".into())
    );
    edit(
        &mut app,
        loading,
        "Orientation",
        Value::String("Horizontal".into()),
    );

    // Play draws exactly what the artboard does.
    let designed = axcenity_ui::layout_artboard(
        &app.world,
        gui,
        &stylesheets(&app),
        (1280, 720),
        1.0,
        [0.0, 0.0],
    );
    app.play();
    let sheets = stylesheets(&app);
    let in_play = axcenity_ui::layout_ui(app.play.as_ref().unwrap().world(), 1280, 720, &sheets);
    assert_eq!(
        format!("{:?}", in_play.draw_items),
        format!("{:?}", designed.draw_items)
    );

    // And it works there: drag the slider, tick the box, pick an option, as
    // the viewport hands each new Value to the game.
    let mut ui = UiInteraction::default();
    let world = app.play.as_mut().unwrap().world_mut();
    let frame = |ui: &mut UiInteraction, world: &World| {
        ui.layout(world, (1280, 720), &sheets, UiInput::default(), None, 0.0);
    };
    frame(&mut ui, world);
    // The slider is at 380, 200; its thumb's middle runs from 390 to 610.
    ui.press(world, [390.0, 212.0]).apply(world);
    ui.drag(world, [390.0 + 220.0 * 0.7, 212.0]).apply(world);
    let _ = ui.release(world, Some([544.0, 212.0])).apply(world);
    frame(&mut ui, world);
    ui.press(world, [394.0, 274.0]).apply(world);
    let _ = ui.release(world, Some([394.0, 274.0])).apply(world);
    frame(&mut ui, world);
    ui.press(world, [480.0, 338.0]).apply(world);
    let _ = ui.release(world, Some([480.0, 338.0])).apply(world);
    frame(&mut ui, world);
    // The list opens below it: High is its third row.
    let high = [480.0, 356.0 + 4.0 + 4.0 + 36.0 * 2.5];
    ui.press(world, high).apply(world);
    let _ = ui.release(world, Some(high)).apply(world);
    let value = |world: &World, id: InstanceId| world.get(id).unwrap().property("Value").cloned();
    assert_eq!(value(world, volume), Some(Value::Number(70.0)));
    assert_eq!(value(world, fullscreen), Some(Value::Bool(true)));
    assert_eq!(value(world, quality), Some(Value::String("High".into())));
    app.stop();
    assert_eq!(value(&app.world, volume), Some(Value::Number(50.0)));

    // Every edit undoes.
    while app.history.undo_label().is_some() {
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}
