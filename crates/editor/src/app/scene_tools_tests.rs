//! World tools while the Scene isn't shown (design system V4): their
//! ribbon buttons are greyed out and their keys do nothing, and both work
//! again once the Scene shows, beside the tab that hid it or in front.

use super::tests::example;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| app.frame(ctx, None))
}

fn settle(app: &mut EditorApp, ctx: &egui::Context) {
    for _ in 0..3 {
        frame(app, ctx, Vec::new());
    }
    // So the script editor's focus doesn't swallow the key instead.
    ctx.memory_mut(|memory| memory.stop_text_input());
}

fn key(key: egui::Key) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
        .into()
}

/// Whether the ribbon's Move button can be pressed, from AccessKit.
fn move_button_enabled(app: &mut EditorApp, ctx: &egui::Context) -> bool {
    ctx.enable_accesskit();
    let output = frame(app, ctx, Vec::new());
    let update = output
        .platform_output
        .accesskit_update
        .expect("AccessKit is on");
    let buttons: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button
                && node.label().is_some_and(|label| label.ends_with("Move"))
        })
        .collect();
    assert!(!buttons.is_empty(), "the ribbon has a Move button");
    buttons.iter().all(|(_, node)| !node.is_disabled())
}

/// Picks Move with its key, then reports whether it took and goes back to
/// Select.
fn move_key_works(app: &mut EditorApp, ctx: &egui::Context) -> bool {
    app.tool = Tool::Select;
    frame(app, ctx, key(egui::Key::Num2));
    let worked = app.tool == Tool::Move;
    app.tool = Tool::Select;
    worked
}

fn script_tab(app: &EditorApp) -> Pane {
    app.layout
        .dock
        .iter_all_tabs()
        .map(|(_, pane)| *pane)
        .find(|pane| matches!(pane, Pane::Script(_)))
        .expect("the script has a tab")
}

#[test]
fn world_tools_wait_for_the_scene_to_show() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    settle(&mut app, &ctx);
    assert!(app.scene_shown(&ctx));
    assert!(move_button_enabled(&mut app, &ctx));
    assert!(
        move_key_works(&mut app, &ctx),
        "2 picks Move over the Scene"
    );

    // A script opens as a tab in front of the Scene.
    app.open_script("scripts/Script.server.lua", None);
    settle(&mut app, &ctx);
    assert!(!app.scene_shown(&ctx), "the script hides the Scene");
    assert!(!move_button_enabled(&mut app, &ctx), "Move is greyed out");
    assert!(!move_key_works(&mut app, &ctx), "and 2 does nothing");

    // Split beside the Scene: both show, so the tools work.
    let script = script_tab(&app);
    let found = app.layout.dock.find_tab(&script).unwrap();
    app.layout.dock.remove_tab(found);
    let (scene, _) = app.layout.dock.find_main_surface_tab(&Pane::Scene).unwrap();
    app.layout
        .dock
        .main_surface_mut()
        .split_right(scene, 0.5, vec![script]);
    app.layout.open(Pane::Scene);
    settle(&mut app, &ctx);
    assert!(app.scene_shown(&ctx), "split, the Scene shows");
    assert!(move_button_enabled(&mut app, &ctx));
    assert!(move_key_works(&mut app, &ctx));

    // A script in a window of its own doesn't hide the Scene.
    let found = app.layout.dock.find_tab(&script).unwrap();
    app.layout.dock.remove_tab(found);
    app.layout.open(script);
    settle(&mut app, &ctx);
    assert!(!app.scene_shown(&ctx), "back in front of the Scene");
    assert!(app.layout.float(script, None).is_some());
    // Taking the tab out leaves the next tab in front; bring the Scene.
    app.layout.open(Pane::Scene);
    settle(&mut app, &ctx);
    assert!(app.layout.is_open(script), "the script is still open");
    assert!(
        app.scene_shown(&ctx),
        "with the script in its own window, the Scene shows"
    );
    assert!(move_key_works(&mut app, &ctx));
}

/// The ribbon's Show Scene button brings the Scene back to the front.
#[test]
fn show_scene_brings_the_scene_back() {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let ctx = theme::test_context();
    app.open_script("scripts/Script.server.lua", None);
    settle(&mut app, &ctx);
    assert!(!app.scene_shown(&ctx));
    ctx.enable_accesskit();
    let output = frame(&mut app, &ctx, Vec::new());
    let update = output.platform_output.accesskit_update.unwrap();
    let bounds = update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label()
                .is_some_and(|label| label.ends_with("Show Scene"))
        })
        .and_then(|(_, node)| node.bounds())
        .expect("the ribbon offers Show Scene");
    assert!(
        bounds.x1 <= SIZE.x as f64,
        "Show Scene is on screen: {bounds:?}"
    );
    let at = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    settle(&mut app, &ctx);
    assert!(app.scene_shown(&ctx), "Show Scene put it in front");
}
