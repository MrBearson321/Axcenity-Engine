//! The UI Designer, driven the way the tab drives the app: a menu built and
//! styled there looks the same in Play, and every edit made there undoes.

use std::fmt::Write as _;

use axcenity_core::UDim2;
use axcenity_ui::UiRect;

use super::tests::copy_of_example;
use super::*;
use crate::ui_handles::{Grip, Handles, UiDrag};

/// What the end of an editor frame does when nothing is held: the gesture
/// ends, so the next edit is its own step.
fn frame_ends(app: &mut EditorApp) {
    app.history.seal();
    app.ui_drag_baselines.clear();
}

/// Does `action` in the designer, as one frame.
fn act(app: &mut EditorApp, action: DesignerAction) {
    app.designer_actions(vec![action]);
    frame_ends(app);
}

/// Drops `what` from the palette into `parent`, and returns what it made.
fn drop_in(app: &mut EditorApp, what: Insertable, parent: InstanceId, at: [f32; 2]) -> InstanceId {
    act(app, DesignerAction::Insert { what, parent, at });
    app.selected.expect("an insert selects what it made")
}

fn add_style(app: &mut EditorApp, id: InstanceId, property: &str, value: &str) {
    act(
        app,
        DesignerAction::Style(
            id,
            StyleEdit::Add {
                property: property.into(),
                value: value.into(),
            },
        ),
    );
}

fn stylesheets(app: &EditorApp) -> Vec<axcenity_ui::Stylesheet> {
    app.project.as_ref().unwrap().stylesheets.clone()
}

/// Drags `grip` of `id` on the artboard by `by` design pixels over four
/// frames of one gesture, as the artboard does at 100% zoom.
fn drag(app: &mut EditorApp, gui: InstanceId, id: InstanceId, grip: Grip, by: egui::Vec2) {
    let sheets = stylesheets(app);
    let layout = axcenity_ui::layout_artboard(&app.world, gui, &sheets, 1.0, [0.0, 0.0]);
    let (width, height) = axcenity_ui::design_size(&app.world, gui);
    let screen = UiRect {
        x: 0.0,
        y: 0.0,
        width: width as f32,
        height: height as f32,
    };
    let element = *layout.element(id).unwrap();
    let handles = Handles::new(element, &app.world, &sheets, egui::Pos2::ZERO, 1.0);
    let rect = handles.rect;
    let start = egui::pos2(
        [rect.left(), rect.center().x, rect.right()][(grip.x + 1) as usize],
        [rect.top(), rect.center().y, rect.bottom()][(grip.y + 1) as usize],
    );
    let drag = UiDrag::start(&handles, grip, start, &app.world).expect("draggable");
    for step in 1..=4 {
        let pointer = start + by * (step as f32 / 4.0);
        let edit = drag.update(pointer, 1.0, &app.world, &layout.elements, screen);
        app.designer_actions(vec![DesignerAction::Drag(edit)]);
    }
    frame_ends(app);
}

fn udim2(app: &EditorApp, id: InstanceId, name: &str) -> UDim2 {
    app.world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_udim2)
        .unwrap()
}

/// Everything the designer can change: the Workspace's Instances with
/// their fields, and every stylesheet with its text.
fn state(app: &EditorApp) -> String {
    let world = &app.world;
    let mut out = String::new();
    for id in world.descendants(world.workspace()) {
        let instance = world.get(id).unwrap();
        let fields = history::snapshot(world, id);
        writeln!(out, "{id:?} {:?} {fields:?}", instance.class()).unwrap();
    }
    for sheet in &app.project.as_ref().unwrap().stylesheets {
        let text = app.stylesheet_text(sheet.path());
        writeln!(out, "{} {text:?}", sheet.path()).unwrap();
    }
    out
}

/// A Main Menu built in the designer: a ScreenGui designed at 1280×720 in
/// Fit, a Frame holding a title and Play and Quit buttons, styled with CSS
/// from the designer's Style section and placed with its handles.
struct Menu {
    gui: InstanceId,
    frame: InstanceId,
    title: InstanceId,
    play: InstanceId,
    quit: InstanceId,
}

fn build_menu(app: &mut EditorApp) -> Menu {
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
    act(app, DesignerAction::UseFit(gui));
    let frame = drop_in(app, Insertable::Frame, gui, [440.0, 160.0]);
    act(
        app,
        DesignerAction::Edit(frame, Edit::Name("MainMenu".into())),
    );
    let mut element = |what, at, name: &str, text: &str| {
        let id = drop_in(app, what, frame, at);
        act(app, DesignerAction::Edit(id, Edit::Name(name.into())));
        let text = Value::String(text.into());
        act(app, DesignerAction::Edit(id, Edit::Property("Text", text)));
        id
    };
    let title = element(Insertable::TextLabel, [20.0, 16.0], "Title", "Main Menu");
    let play = element(Insertable::TextButton, [20.0, 80.0], "Play", "Play");
    let quit = element(Insertable::TextButton, [20.0, 150.0], "Quit", "Quit");

    add_style(app, frame, "background-color", "#1e2230");
    add_style(app, frame, "border-radius", "12px");
    add_style(app, frame, "width", "400px");
    add_style(app, title, "color", "#ffd166");
    add_style(app, title, "font-size", "28px");
    add_style(app, play, "background-color", "#3a86ff");
    add_style(app, quit, "background-color", "#ef476f");
    // A value changed in place, as typing in the Style section does.
    let sheets = stylesheets(app);
    let radius = axcenity_ui::applied_declarations(&app.world, frame, &sheets)
        .into_iter()
        .find(|declaration| declaration.property == "border-radius")
        .unwrap();
    act(
        app,
        DesignerAction::Style(
            frame,
            StyleEdit::Set {
                path: radius.path,
                span: radius.span,
                value: "16px".into(),
            },
        ),
    );

    // Moved by its body, and widened from its right edge (which rewrites
    // the CSS width).
    drag(app, gui, frame, Grip::BODY, egui::vec2(-37.0, 23.0));
    drag(app, gui, frame, Grip { x: 1, y: 0 }, egui::vec2(41.0, 0.0));
    Menu {
        gui,
        frame,
        title,
        play,
        quit,
    }
}

#[test]
fn a_menu_built_and_styled_in_the_designer_alone_looks_the_same_in_play() {
    let folder = copy_of_example("hello-touched", "ui-designer-menu");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);

    // Built where it was dropped, then moved and resized by its handles.
    assert_eq!(app.world.get(menu.frame).unwrap().name(), "MainMenu");
    for (id, text) in [
        (menu.title, "Main Menu"),
        (menu.play, "Play"),
        (menu.quit, "Quit"),
    ] {
        assert_eq!(app.world.get(id).unwrap().parent(), Some(menu.frame));
        assert_eq!(
            app.world.get(id).unwrap().property("Text"),
            Some(&Value::String(text.into()))
        );
    }
    assert_eq!(
        udim2(&app, menu.frame, "Position"),
        UDim2::new(0.0, 403.0, 0.0, 183.0)
    );
    let css = app
        .stylesheet_text("ui.css")
        .expect("styles went in ui.css");
    for rule in [
        "#MainMenu",
        "border-radius: 16px",
        "width: 441px",
        "#Title",
        "color: #ffd166",
    ] {
        assert!(css.contains(rule), "{rule} in:\n{css}");
    }

    // What the artboard draws at 100%: a 1280×720 screen.
    let designed =
        axcenity_ui::layout_artboard(&app.world, menu.gui, &stylesheets(&app), 1.0, [0.0, 0.0]);
    // A frame, a title, and two buttons with their text.
    assert_eq!(designed.draw_items.len(), 6, "{:?}", designed.draw_items);

    // Play saves the project and runs a copy of the world: the same UI.
    app.play();
    let session = app.play.as_ref().expect("playing");
    let in_play = axcenity_ui::layout_ui(session.world(), 1280, 720, &stylesheets(&app));
    assert_eq!(
        format!("{:?}", in_play.draw_items),
        format!("{:?}", designed.draw_items)
    );
    app.stop();

    // The player, reading the saved project: the same at the design size,
    // and scaled up evenly on a bigger screen (the ScreenGui is in Fit).
    let project = axcenity_project::load(&folder).unwrap();
    let sheets: Vec<_> = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let player = axcenity_ui::layout_ui(&project.world, 1280, 720, &sheets);
    assert_eq!(
        format!("{:?}", player.draw_items),
        format!("{:?}", designed.draw_items)
    );
    let big = axcenity_ui::layout_ui(&project.world, 1920, 1080, &sheets);
    let scaled =
        axcenity_ui::layout_artboard(&app.world, menu.gui, &stylesheets(&app), 1.5, [0.0, 0.0]);
    assert_eq!(
        format!("{:?}", big.draw_items),
        format!("{:?}", scaled.draw_items)
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn every_designer_edit_undoes_and_redoes() {
    let folder = copy_of_example("hello-touched", "ui-designer-undo");
    let mut app = EditorApp::new(Some(&folder));
    assert!(
        app.stylesheet_text("ui.css").is_none(),
        "the first style makes the stylesheet"
    );
    let mut states = vec![state(&app)];
    let mut labels = Vec::new();
    let mut step = |app: &mut EditorApp, what: &dyn Fn(&mut EditorApp)| {
        what(app);
        labels.push(app.history.undo_label().unwrap_or_default().to_owned());
        let now = state(app);
        assert_ne!(
            states.last(),
            Some(&now),
            "{:?} changed nothing",
            labels.last()
        );
        states.push(now);
    };

    step(&mut app, &|app| act(app, DesignerAction::NewScreenGui));
    let gui = app.selected.unwrap();
    step(&mut app, &|app| {
        act(
            app,
            DesignerAction::DesignSize {
                gui,
                width: 390,
                height: 844,
            },
        )
    });
    step(&mut app, &|app| act(app, DesignerAction::UseFit(gui)));
    step(&mut app, &|app| {
        drop_in(app, Insertable::Frame, gui, [10.0, 10.0]);
    });
    let frame = app.selected.unwrap();
    step(&mut app, &|app| {
        drop_in(app, Insertable::TextButton, frame, [8.0, 8.0]);
    });
    let button = app.selected.unwrap();
    step(&mut app, &|app| {
        act(
            app,
            DesignerAction::Edit(button, Edit::Name("Start".into())),
        )
    });
    step(&mut app, &|app| {
        act(
            app,
            DesignerAction::Edit(button, Edit::Property("Text", Value::String("Go".into()))),
        )
    });
    step(&mut app, &|app| {
        drop_in(app, Insertable::PageLayout, frame, [0.0, 0.0]);
    });
    let pages = app.selected.unwrap();
    step(&mut app, &|app| {
        drop_in(app, Insertable::Page, pages, [0.0, 0.0]);
    });
    // The first style makes ui.css and gives the button an Id.
    step(&mut app, &|app| add_style(app, button, "width", "120px"));
    step(&mut app, &|app| add_style(app, button, "color", "#ffffff"));
    step(&mut app, &|app| {
        let sheets = stylesheets(app);
        let color = axcenity_ui::applied_declarations(&app.world, button, &sheets)
            .into_iter()
            .find(|declaration| declaration.property == "color")
            .unwrap();
        act(
            app,
            DesignerAction::Style(
                button,
                StyleEdit::Set {
                    path: color.path,
                    span: color.span,
                    value: "#000000".into(),
                },
            ),
        );
    });
    step(&mut app, &|app| {
        drag(app, gui, frame, Grip::BODY, egui::vec2(30.0, 40.0))
    });
    // From the left edge: Size and Position change together.
    step(&mut app, &|app| {
        drag(
            app,
            gui,
            frame,
            Grip { x: -1, y: 0 },
            egui::vec2(-20.0, 0.0),
        )
    });
    // The button's width is CSS, so this drag rewrites the stylesheet.
    step(&mut app, &|app| {
        drag(app, gui, button, Grip { x: 1, y: 0 }, egui::vec2(30.0, 0.0))
    });
    assert!(
        app.stylesheet_text("ui.css")
            .unwrap()
            .contains("width: 150px")
    );

    // Each edit was one step: undo walks back through every state.
    for (index, label) in labels.iter().enumerate().rev() {
        assert_eq!(app.history.undo_label(), Some(label.as_str()));
        app.undo();
        assert_eq!(state(&app), states[index], "undoing {label}");
    }
    assert!(
        !folder.join("ui.css").exists(),
        "undoing the first style takes away the stylesheet it made"
    );
    for (index, label) in labels.iter().enumerate() {
        app.redo();
        assert_eq!(state(&app), states[index + 1], "redoing {label}");
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// Draws the UI Designer tab at `size` with `events`, and returns what was
/// drawn and the room there was for it.
fn draw_designer(
    app: &mut EditorApp,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> (egui::Rect, egui::Rect) {
    let mut fit = None;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let room = ui.max_rect();
            app.ui_designer_pane(ui, None);
            fit = Some((ui.min_rect(), room));
        });
    });
    if !ctx.is_using_pointer() {
        frame_ends(app);
    }
    fit.expect("drawn")
}

#[test]
fn the_designer_fits_any_shape_and_its_artboard_drags_an_element() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let folder = copy_of_example("hello-touched", "ui-designer-draw");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(&mut app, DesignerAction::Select(Some(menu.play)));

    for (size, shape) in [
        (egui::vec2(260.0, 900.0), "tall"),
        (egui::vec2(900.0, 260.0), "wide"),
        (egui::vec2(1100.0, 600.0), "large"),
    ] {
        let ctx = theme::test_context();
        let mut drawn = None;
        for _ in 0..3 {
            drawn = Some(draw_designer(&mut app, &ctx, size, Vec::new()));
        }
        let (used, room) = drawn.unwrap();
        if let Some(folder) = &snapshots {
            // Drawn by the game's renderer, as in the editor. Each snapshot
            // has its own GPU, so the artboard's texture starts again.
            app.ui_designer = UiDesigner::default();
            crate::snapshot::snapshot_rendered(
                &folder.join(format!("ui-designer-{shape}.png")),
                size,
                vec![Vec::new(); 3],
                |ctx, render_state| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.ui_designer_pane(ui, Some(render_state)));
                },
            );
        }
        assert!(
            used.width() <= room.width() + 0.5 && used.height() <= room.height() + 0.5,
            "at {size:?} drew {used:?} in {room:?}"
        );
        let (board, _) = app.ui_designer.board().expect("the artboard was drawn");
        assert!(
            board.width() > 100.0 && room.contains_rect(board),
            "at {size:?} the artboard {board:?} fits in {room:?}"
        );
    }

    // On the large shape: press on the Quit button on the artboard, drag
    // it 60 points right, and let go. It's selected and moved, as one step.
    let ctx = theme::test_context();
    let size = egui::vec2(1100.0, 600.0);
    for _ in 0..2 {
        draw_designer(&mut app, &ctx, size, Vec::new());
    }
    let (board, zoom) = app.ui_designer.board().unwrap();
    let layout = axcenity_ui::layout_artboard(
        &app.world,
        menu.gui,
        &stylesheets(&app),
        zoom * ctx.pixels_per_point(),
        [0.0, 0.0],
    );
    let quit = layout.element(menu.quit).unwrap().rect;
    let at = board.min
        + egui::vec2(quit.x + quit.width * 0.5, quit.y + quit.height * 0.5)
            / ctx.pixels_per_point();
    let before = udim2(&app, menu.quit, "Position");
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let moved = egui::Event::PointerMoved;
    let frames = vec![
        vec![moved(at)],
        vec![button(at, true)],
        vec![moved(at + egui::vec2(20.0, 0.0))],
        vec![moved(at + egui::vec2(60.0, 0.0))],
        vec![button(at + egui::vec2(60.0, 0.0), false)],
        Vec::new(),
    ];
    let undo_label = app.history.undo_label().map(str::to_owned);
    for events in frames {
        draw_designer(&mut app, &ctx, size, events);
    }
    assert_eq!(app.selected, Some(menu.quit));
    let after = udim2(&app, menu.quit, "Position");
    let moved_by = after.x.offset - before.x.offset;
    let expected = 60.0 / zoom;
    // Snapping can pull it up to 8 points to another edge.
    assert!(
        (moved_by - expected).abs() <= 8.0 / zoom + 1.0,
        "moved {moved_by} design pixels, expected about {expected}"
    );
    assert!(
        (after.y.offset - before.y.offset).abs() <= 8.0 / zoom + 1.0,
        "a sideways drag only snaps up or down"
    );
    assert_eq!(app.history.undo_label(), Some("Move Quit"));
    app.undo();
    assert_eq!(udim2(&app, menu.quit, "Position"), before);
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_label);
    let _ = std::fs::remove_dir_all(&folder);
}
