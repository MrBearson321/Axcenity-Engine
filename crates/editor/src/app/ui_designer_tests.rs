//! The UI Designer, driven the way the tab drives the app: a menu built and
//! styled there looks the same in Play on any screen, every edit made there
//! undoes, anchors, fonts, typing on the artboard and ruler guides.

use std::fmt::Write as _;

use axcenity_core::UDim2;
use axcenity_render::glam::Vec3;
use axcenity_ui::UiRect;

use super::tests::copy_of_example;
use super::*;
use crate::ui_designer::Pin;
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
    let (width, height) = axcenity_ui::design_size(&app.world, gui);
    let layout =
        axcenity_ui::layout_artboard(&app.world, gui, &sheets, (width, height), 1.0, [0.0, 0.0]);
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
    assert_eq!(
        app.world.get(gui).unwrap().property("ScaleMode"),
        Some(&Value::String("Scale".into())),
        "a new ScreenGui scales with the screen"
    );
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
    let artboard_on = |app: &EditorApp, screen: (u32, u32)| {
        axcenity_ui::layout_artboard(
            &app.world,
            menu.gui,
            &stylesheets(app),
            screen,
            1.0,
            [0.0, 0.0],
        )
    };
    let designed = artboard_on(&app, (1280, 720));
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
    // and on other screens the same as the designer previews them.
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
    for (width, height) in [(1920, 1080), (3440, 1440), (390, 844)] {
        let player = axcenity_ui::layout_ui(&project.world, width, height, &sheets);
        assert_eq!(
            format!("{:?}", player.draw_items),
            format!("{:?}", artboard_on(&app, (width, height)).draw_items),
            "on {width}×{height}"
        );
    }
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
    step(&mut app, &|app| {
        act(app, DesignerAction::ScaleMode(gui, "Fit"))
    });
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
        (1280, 720),
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

/// Runs `frames` of the UI Designer tab at `size`, a twentieth of a second
/// apart on `clock`, ending each frame as the editor does.
fn run_designer(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    size: egui::Vec2,
    frames: Vec<Vec<egui::Event>>,
) {
    for events in frames {
        *clock += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            time: Some(*clock),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
        });
        if !ctx.is_using_pointer() && !ctx.wants_keyboard_input() {
            frame_ends(app);
        }
    }
}

fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Where the middle of `id` is on the artboard, in points.
fn on_artboard(app: &EditorApp, gui: InstanceId, id: InstanceId) -> egui::Pos2 {
    let (board, zoom) = app.ui_designer.board().unwrap();
    let screen = axcenity_ui::design_size(&app.world, gui);
    let layout = axcenity_ui::layout_artboard(
        &app.world,
        gui,
        &stylesheets(app),
        screen,
        zoom,
        [board.min.x, board.min.y],
    );
    let rect = layout.element(id).unwrap().rect;
    egui::pos2(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

#[test]
fn anchored_elements_keep_their_place_on_every_screen() {
    let folder = copy_of_example("hello-touched", "ui-designer-anchors");
    let mut app = EditorApp::new(Some(&folder));
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
    // A minimap 20 from the top right, a button centred 22 off the bottom,
    // and a health bar stretched along the top with 20 at each side.
    let minimap = drop_in(&mut app, Insertable::Frame, gui, [1060.0, 20.0]);
    let menu = drop_in(&mut app, Insertable::TextButton, gui, [560.0, 650.0]);
    let health = drop_in(&mut app, Insertable::Frame, gui, [20.0, 20.0]);
    act(
        &mut app,
        DesignerAction::Edit(
            health,
            Edit::Property("Size", Value::UDim2(UDim2::new(0.0, 1240.0, 0.0, 16.0))),
        ),
    );
    let before = state(&app);
    let sheets = stylesheets(&app);
    for (id, x, y) in [
        (minimap, Pin::End, Pin::Start),
        (menu, Pin::Center, Pin::End),
        (health, Pin::Stretch, Pin::Start),
    ] {
        let action = crate::ui_designer::anchor_action(&app.world, &sheets, gui, id, x, y)
            .expect("on the artboard");
        act(&mut app, action);
    }
    // Nothing moved at the design size.
    let at_design = axcenity_ui::layout_ui(&app.world, 1280, 720, &sheets);
    let rect = |layout: &axcenity_ui::UiLayout, id| layout.element(id).unwrap().rect;
    assert_eq!(rect(&at_design, minimap).x, 1060.0);
    assert_eq!(rect(&at_design, menu).y, 650.0);
    assert_eq!(rect(&at_design, health).width, 1240.0);

    // On an ultrawide screen twice as tall, everything is twice the size
    // and keeps its distance from its own edges.
    let wide = axcenity_ui::layout_ui(&app.world, 3440, 1440, &sheets);
    let map = rect(&wide, minimap);
    assert_eq!(map.x + map.width, 3440.0 - 40.0);
    assert_eq!(map.y, 40.0);
    assert_eq!(map.width, 400.0);
    let button = rect(&wide, menu);
    assert_eq!(button.x + button.width * 0.5, 3440.0 * 0.5);
    assert_eq!(button.y + button.height, 1440.0 - 44.0);
    let bar = rect(&wide, health);
    assert_eq!((bar.x, bar.x + bar.width), (40.0, 3440.0 - 40.0));

    // A phone held upright: scaled down to fit its width, still pinned.
    let phone = axcenity_ui::layout_ui(&app.world, 390, 844, &sheets);
    let map = rect(&phone, minimap);
    let scale = 390.0 / 1280.0;
    assert!((390.0 - (map.x + map.width) - 20.0 * scale).abs() < 0.01);

    // Each anchor was one undo step.
    for _ in 0..3 {
        assert!(app.history.undo_label().unwrap().starts_with("Anchor"));
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn fonts_are_picked_from_the_built_in_set_and_the_project() {
    let folder = copy_of_example("hello-touched", "ui-designer-fonts");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let title = drop_in(&mut app, Insertable::TextLabel, gui, [10.0, 10.0]);
    act(
        &mut app,
        DesignerAction::Edit(title, Edit::Name("Title".into())),
    );
    let before = state(&app);
    let font_of = |app: &EditorApp| {
        let layout = axcenity_ui::layout_ui(&app.world, 1920, 1080, &stylesheets(app));
        layout
            .draw_items
            .iter()
            .find_map(|item| match item {
                axcenity_render::UiDrawItem::Text(text) => Some(text.font_family.clone()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(font_of(&app), "Montserrat");

    let pick = |app: &mut EditorApp, family: &str| {
        act(
            app,
            DesignerAction::Font {
                id: title,
                family: family.into(),
            },
        );
    };
    pick(&mut app, "Bebas Neue");
    assert_eq!(font_of(&app), "Bebas Neue");
    let css = app.stylesheet_text("ui.css").unwrap();
    assert!(css.contains("#Title"), "{css}");
    assert!(css.contains("font-family: \"Bebas Neue\""), "{css}");
    // Picking again changes that declaration, instead of adding another.
    pick(&mut app, "Press Start 2P");
    assert_eq!(font_of(&app), "Press Start 2P");
    let css = app.stylesheet_text("ui.css").unwrap();
    assert_eq!(css.matches("font-family").count(), 1, "{css}");

    // A font file anywhere in the project is listed by its family, and
    // goes to the renderer in the draw list.
    let fonts = folder.join("assets/fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/Lobster-Regular.ttf"),
        fonts.join("Fancy.ttf"),
    )
    .unwrap();
    app.assets.forget_fonts();
    let files = app.assets.fonts();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "assets/fonts/Fancy.ttf");
    assert_eq!(axcenity_render::font_families(&files[0].data), ["Lobster"]);
    let camera = axcenity_render::Camera {
        position: Vec3::new(0.0, 5.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    assert_eq!(list.fonts.len(), 1);
    assert_eq!(list.fonts[0].path, "assets/fonts/Fancy.ttf");
    pick(&mut app, "Lobster");
    assert_eq!(font_of(&app), "Lobster");

    // Each pick undoes on its own; the first one also takes away ui.css.
    for _ in 0..3 {
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn double_clicking_text_on_the_artboard_types_over_it() {
    let folder = copy_of_example("hello-touched", "ui-designer-typing");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let at = on_artboard(&app, menu.gui, menu.title);
    let double_click = vec![
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
        vec![press(at, true)],
        vec![press(at, false)],
        Vec::new(),
    ];
    let undo_before = app.history.undo_label().map(str::to_owned);
    run_designer(&mut app, &ctx, &mut clock, size, double_click.clone());
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
            vec![egui::Event::Text("Start Game".into())],
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            Vec::new(),
        ],
    );
    let text = |app: &EditorApp| {
        app.world
            .get(menu.title)
            .unwrap()
            .property("Text")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    assert_eq!(text(&app), "Start Game");
    // All the typing was one step.
    assert_eq!(app.history.undo_label(), Some("Change Text of Title"));
    app.undo();
    assert_eq!(text(&app), "Main Menu");
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    app.redo();

    // Escape puts the text back, and leaves no step behind.
    run_designer(&mut app, &ctx, &mut clock, size, double_click);
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![egui::Event::Text("zzz".into())],
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
            Vec::new(),
        ],
    );
    assert_eq!(text(&app), "Start Game");
    assert_eq!(app.history.undo_label(), Some("Change Text of Title"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dragging_in_the_box_being_typed_in_selects_text_not_the_element() {
    let folder = copy_of_example("hello-touched", "ui-designer-typing-drag");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let at = on_artboard(&app, menu.gui, menu.title);
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![egui::Event::PointerMoved(at)],
            vec![press(at, true)],
            vec![press(at, false)],
            vec![press(at, true)],
            vec![press(at, false)],
            Vec::new(),
        ],
    );
    let undo_before = app.history.undo_label().map(str::to_owned);
    let to = at + egui::vec2(40.0, 0.0);
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![egui::Event::PointerMoved(at + egui::vec2(-20.0, 0.0))],
            vec![press(at + egui::vec2(-20.0, 0.0), true)],
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerMoved(to)],
            vec![press(to, false)],
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
            Vec::new(),
        ],
    );
    assert_eq!(app.selected, Some(menu.title));
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_guide_dragged_out_of_a_ruler_stays_until_dragged_back() {
    let folder = copy_of_example("hello-touched", "ui-designer-guides");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let canvas = app.ui_designer.canvas().unwrap();
    let (board, zoom) = app.ui_designer.board().unwrap();
    let undo_before = app.history.undo_label().map(str::to_owned);

    // Down from the top ruler to design y 300.
    let ruler = egui::pos2(canvas.center().x, canvas.top() - 9.0);
    let target = egui::pos2(canvas.center().x, board.top() + 300.0 * zoom);
    let drag = |from: egui::Pos2, to: egui::Pos2| {
        vec![
            vec![egui::Event::PointerMoved(from)],
            vec![press(from, true)],
            vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![press(to, false)],
            Vec::new(),
        ]
    };
    run_designer(&mut app, &ctx, &mut clock, size, drag(ruler, target));
    assert_eq!(
        app.ui_designer.guides(menu.gui),
        [crate::ui_designer::Guide {
            vertical: false,
            at: 300.0
        }]
    );
    // Guides belong to the designer, not the project: no undo step.
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);

    // Moved, then dragged back onto the ruler, which takes it away.
    let lower = egui::pos2(target.x + 40.0, board.top() + 320.0 * zoom);
    run_designer(&mut app, &ctx, &mut clock, size, drag(target, lower));
    assert_eq!(app.ui_designer.guides(menu.gui)[0].at, 320.0);
    run_designer(&mut app, &ctx, &mut clock, size, drag(lower, ruler));
    assert!(app.ui_designer.guides(menu.gui).is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_artboard_previews_the_ui_on_another_screen() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let folder = copy_of_example("hello-touched", "ui-designer-preview");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let sheets = stylesheets(&app);
    let action = crate::ui_designer::anchor_action(
        &app.world,
        &sheets,
        menu.gui,
        menu.frame,
        Pin::Center,
        Pin::Center,
    )
    .unwrap();
    act(&mut app, action);
    act(&mut app, DesignerAction::Select(Some(menu.play)));
    app.ui_designer.add_guide(
        menu.gui,
        crate::ui_designer::Guide {
            vertical: true,
            at: 640.0,
        },
    );
    app.ui_designer.preview_at(Some((3440, 1440)));

    let size = egui::vec2(1100.0, 600.0);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let (board, _) = app.ui_designer.board().unwrap();
    assert!(
        (board.width() / board.height() - 3440.0 / 1440.0).abs() < 0.01,
        "the artboard is the previewed screen's shape: {board:?}"
    );
    // Pinned to the centre, the menu keeps its distance from the middle of
    // the wide screen, in design pixels.
    let design = axcenity_ui::layout_ui(&app.world, 1280, 720, &sheets);
    let at_design = design.element(menu.frame).unwrap().rect;
    let off_centre = at_design.x + at_design.width * 0.5 - 640.0;
    let (board, zoom) = app.ui_designer.board().unwrap();
    let layout = axcenity_ui::layout_artboard(
        &app.world,
        menu.gui,
        &sheets,
        (3440, 1440),
        zoom,
        [board.min.x, board.min.y],
    );
    let middle = layout.element(menu.frame).unwrap().rect;
    // 1440 tall is twice 720: two screen pixels per design pixel.
    let points_per_unit = zoom * 2.0;
    let off_centre_here = (middle.x + middle.width * 0.5 - board.center().x) / points_per_unit;
    assert!(
        (off_centre_here - off_centre).abs() < 0.5,
        "{off_centre_here} vs {off_centre}"
    );

    if let Some(folder) = &snapshots {
        app.ui_designer = UiDesigner::default();
        app.ui_designer.add_guide(
            menu.gui,
            crate::ui_designer::Guide {
                vertical: true,
                at: 640.0,
            },
        );
        app.ui_designer.preview_at(Some((3440, 1440)));
        crate::snapshot::snapshot_rendered(
            &folder.join("ui-designer-ultrawide.png"),
            size,
            vec![Vec::new(); 3],
            |ctx, render_state| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// A press at `from`, a drag to `to` over two frames, and a let go.
fn drag_on_artboard(from: egui::Pos2, to: egui::Pos2) -> Vec<Vec<egui::Event>> {
    vec![
        vec![egui::Event::PointerMoved(from)],
        vec![press(from, true)],
        vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
        vec![egui::Event::PointerMoved(to)],
        vec![press(to, false)],
        Vec::new(),
    ]
}

/// Where `id` is on the artboard, in points.
fn rect_on_artboard(app: &EditorApp, gui: InstanceId, id: InstanceId) -> egui::Rect {
    let (board, zoom) = app.ui_designer.board().unwrap();
    let screen = axcenity_ui::design_size(&app.world, gui);
    let layout = axcenity_ui::layout_artboard(
        &app.world,
        gui,
        &stylesheets(app),
        screen,
        zoom,
        [board.min.x, board.min.y],
    );
    let rect = layout.element(id).unwrap().rect;
    egui::Rect::from_min_size(
        egui::pos2(rect.x, rect.y),
        egui::vec2(rect.width, rect.height),
    )
}

/// Where `id` is laid out on a `width`×`height` screen, as the player does.
fn rect_in_game(app: &EditorApp, id: InstanceId, width: u32, height: u32) -> UiRect {
    let layout = axcenity_ui::layout_ui(&app.world, width, height, &stylesheets(app));
    layout.element(id).unwrap().rect
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

#[test]
fn dragging_the_anchor_to_the_bottom_right_corner_pins_an_element_there() {
    let folder = copy_of_example("hello-touched", "ui-designer-anchor-corner");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(&mut app, DesignerAction::Select(Some(menu.frame)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let before = rect_in_game(&app, menu.frame, 1280, 720);
    let position_before = udim2(&app, menu.frame, "Position");
    let undo_before = app.history.undo_label().map(str::to_owned);

    // Anchored top left, its four diamonds meet at the screen's corner.
    // Grabbed there, they all go to the bottom right, snapping onto it.
    let (board, _) = app.ui_designer.board().unwrap();
    let frames = drag_on_artboard(board.min, board.max - egui::vec2(2.0, 2.0));
    run_designer(&mut app, &ctx, &mut clock, size, frames);
    let (position, size_now) = (
        udim2(&app, menu.frame, "Position"),
        udim2(&app, menu.frame, "Size"),
    );
    assert_eq!((position.x.scale, position.y.scale), (1.0, 1.0));
    assert_eq!((size_now.x.scale, size_now.y.scale), (0.0, 0.0));

    // It didn't move at the design size.
    let after = rect_in_game(&app, menu.frame, 1280, 720);
    assert!(
        near(after.x, before.x) && near(after.y, before.y),
        "{after:?}"
    );
    let right = 1280.0 - (before.x + before.width);
    let bottom = 720.0 - (before.y + before.height);

    // On a wider Preview screen twice as tall it's twice the size, and
    // keeps (twice) its distance from the bottom right corner.
    let wide = |rect: UiRect| {
        (
            3440.0 - (rect.x + rect.width),
            1440.0 - (rect.y + rect.height),
        )
    };
    app.ui_designer.preview_at(Some((3440, 1440)));
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 2]);
    let preview = axcenity_ui::layout_artboard(
        &app.world,
        menu.gui,
        &stylesheets(&app),
        (3440, 1440),
        1.0,
        [0.0, 0.0],
    );
    let (r, b) = wide(preview.element(menu.frame).unwrap().rect);
    assert!(near(r, right * 2.0) && near(b, bottom * 2.0), "{r}, {b}");
    // And in Play, the same.
    app.play();
    let session = app.play.as_ref().expect("playing");
    let in_play = axcenity_ui::layout_ui(session.world(), 3440, 1440, &stylesheets(&app));
    let (r, b) = wide(in_play.element(menu.frame).unwrap().rect);
    assert!(near(r, right * 2.0) && near(b, bottom * 2.0), "{r}, {b}");
    app.stop();

    // The whole drag is one undo step.
    assert_eq!(app.history.undo_label(), Some("Anchor MainMenu"));
    app.undo();
    assert_eq!(udim2(&app, menu.frame, "Position"), position_before);
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dragging_the_horizontal_diamonds_to_both_edges_stretches_an_element_across() {
    let folder = copy_of_example("hello-touched", "ui-designer-anchor-stretch");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(&mut app, DesignerAction::Select(Some(menu.title)));
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let frame = rect_in_game(&app, menu.frame, 1280, 720);
    let title = rect_in_game(&app, menu.title, 1280, 720);
    let margins = |frame: UiRect, title: UiRect| {
        (
            title.x - frame.x,
            (frame.x + frame.width) - (title.x + title.width),
        )
    };
    let (left, right) = margins(frame, title);

    // The title's anchor is the frame's top left: both horizontal diamonds
    // are there. The left one stays; the right one (up and to the right of
    // the point) is dragged to the frame's right edge.
    let parent = rect_on_artboard(&app, menu.gui, menu.frame);
    let grab = parent.min + egui::vec2(6.0, -6.0);
    let frames = drag_on_artboard(grab, grab + egui::vec2(parent.width() - 2.0, 0.0));
    run_designer(&mut app, &ctx, &mut clock, size, frames);
    let (position, size_now) = (
        udim2(&app, menu.title, "Position"),
        udim2(&app, menu.title, "Size"),
    );
    assert_eq!((position.x.scale, size_now.x.scale), (0.0, 1.0));
    assert_eq!((position.y.scale, size_now.y.scale), (0.0, 0.0));
    let now = rect_in_game(&app, menu.title, 1280, 720);
    assert!(
        near(now.x, title.x) && near(now.width, title.width),
        "{now:?}"
    );

    // Widening the frame widens the title, keeping both margins.
    let sheets = stylesheets(&app);
    let width = axcenity_ui::applied_declarations(&app.world, menu.frame, &sheets)
        .into_iter()
        .find(|declaration| declaration.property == "width")
        .unwrap();
    act(
        &mut app,
        DesignerAction::Style(
            menu.frame,
            StyleEdit::Set {
                path: width.path,
                span: width.span,
                value: "600px".into(),
            },
        ),
    );
    let frame = rect_in_game(&app, menu.frame, 1280, 720);
    assert_eq!(frame.width, 600.0);
    let (l, r) = margins(frame, rect_in_game(&app, menu.title, 1280, 720));
    assert!(
        near(l, left) && near(r, right),
        "{l}, {r} vs {left}, {right}"
    );

    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        // Partway through dragging the right diamonds back to the middle:
        // the snap line and the percent label.
        run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 2]);
        let parent = rect_on_artboard(&app, menu.gui, menu.frame);
        let grab = egui::pos2(parent.right(), parent.top()) + egui::vec2(6.0, -6.0);
        let to = grab - egui::vec2(parent.width() * 0.5 - 2.0, 0.0);
        app.ui_designer = UiDesigner::default();
        let mut frames = vec![Vec::new(); 3];
        frames.extend(drag_on_artboard(grab, to).into_iter().take(4));
        crate::snapshot::snapshot_rendered(
            &snapshots.join("ui-designer-anchor-drag.png"),
            size,
            frames,
            |ctx, render_state| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// Runs one frame of the UI Designer tab, as [`run_designer`] does, and
/// returns egui's output (with AccessKit on, so it says where each widget is).
fn designer_frame(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *clock += 0.05;
    ctx.enable_accesskit();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        time: Some(*clock),
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
    });
    if !ctx.is_using_pointer() && !ctx.wants_keyboard_input() {
        frame_ends(app);
    }
    output
}

/// The middle of the widget labelled `label` in `output`, in points.
fn widget_named(output: &egui::FullOutput, label: &str, ppp: f32) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    let (_, node) = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(label))?;
    let bounds = node.bounds()?;
    Some(egui::pos2(
        ((bounds.x0 + bounds.x1) * 0.5) as f32 / ppp,
        ((bounds.y0 + bounds.y1) * 0.5) as f32 / ppp,
    ))
}

/// Clicks the widget labelled `label`, as the next frame draws it.
fn click_named(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    size: egui::Vec2,
    label: &str,
) {
    let output = designer_frame(app, ctx, clock, size, Vec::new());
    let at = widget_named(&output, label, ctx.pixels_per_point())
        .unwrap_or_else(|| panic!("{label} is drawn"));
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
        Vec::new(),
    ] {
        designer_frame(app, ctx, clock, size, events);
    }
}

#[test]
fn reset_anchors_an_element_at_the_top_left_without_moving_it() {
    let folder = copy_of_example("hello-touched", "ui-designer-anchor-reset");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    let sheets = stylesheets(&app);
    let centre = crate::ui_designer::anchor_action(
        &app.world,
        &sheets,
        menu.gui,
        menu.frame,
        Pin::Center,
        Pin::Center,
    )
    .unwrap();
    act(&mut app, centre);
    act(&mut app, DesignerAction::Select(Some(menu.frame)));
    let centred = udim2(&app, menu.frame, "Position");
    assert_eq!((centred.x.scale, centred.y.scale), (0.5, 0.5));
    let placed = rect_in_game(&app, menu.frame, 1280, 720);
    let reset = |app: &EditorApp| {
        let (position, size) = (
            udim2(app, menu.frame, "Position"),
            udim2(app, menu.frame, "Size"),
        );
        let now = rect_in_game(app, menu.frame, 1280, 720);
        assert_eq!(
            [
                position.x.scale,
                position.y.scale,
                size.x.scale,
                size.y.scale
            ],
            [0.0; 4]
        );
        assert!(near(now.x, placed.x) && near(now.y, placed.y), "{now:?}");
    };

    // The Anchor section's Reset button.
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    click_named(&mut app, &ctx, &mut clock, size, "Reset");
    reset(&app);
    assert_eq!(app.history.undo_label(), Some("Anchor MainMenu"));
    app.undo();
    assert_eq!(udim2(&app, menu.frame, "Position"), centred);

    // Right-click on the diamonds, which meet at the screen's middle, and
    // Reset anchor.
    let (board, _) = app.ui_designer.board().unwrap();
    let diamond = board.center() + egui::vec2(6.0, 6.0);
    let secondary = |pressed| egui::Event::PointerButton {
        pos: diamond,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![egui::Event::PointerMoved(diamond)],
        vec![secondary(true)],
        vec![secondary(false)],
        Vec::new(),
    ] {
        designer_frame(&mut app, &ctx, &mut clock, size, events);
    }
    click_named(&mut app, &ctx, &mut clock, size, "Reset anchor");
    reset(&app);
    assert_eq!(app.history.undo_label(), Some("Anchor MainMenu"));
    app.undo();
    assert_eq!(udim2(&app, menu.frame, "Position"), centred);
    let _ = std::fs::remove_dir_all(&folder);
}
