//! Explorer double-click (cohesion pass CW3.2): each Instance opens in its
//! own editor, and an open tab is shown again rather than doubled.

use super::tests::copy_of_example;
use super::*;

/// The first Instance of `class` in the world.
fn first_of(app: &EditorApp, class: ClassName) -> InstanceId {
    let world = &app.world;
    world
        .descendants(world.root())
        .into_iter()
        .find(|&id| world.get(id).is_some_and(|i| i.class() == class))
        .unwrap()
}

#[test]
fn a_script_opens_in_one_tab_however_often_it_is_opened() {
    let folder = copy_of_example("hello-touched", "open-script");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let pad = first_of(&app, ClassName::Script);
    app.open_instance(&ctx, pad);
    app.open_instance(&ctx, pad);
    assert_eq!(app.tabs.ids().len(), 1);
    assert_eq!(app.tabs.active().unwrap().path, "scripts/pad.server.lua");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_part_is_selected_and_framed_in_the_viewport() {
    let folder = copy_of_example("hello-touched", "open-part");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let part = first_of(&app, ClassName::Part);
    // With reduced motion the camera is there at once.
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let (center, _) = crate::explorer::scene_bounds(&app.world, part).unwrap();
    app.open_instance(&ctx, part);
    assert_eq!(app.selected, Some(part));
    assert!(
        app.viewport.focus().distance(center) < 0.01,
        "{} vs {center}",
        app.viewport.focus()
    );
    assert!(app.tabs.ids().is_empty(), "no tab for a part");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn ui_opens_in_the_ui_designer_and_an_animation_in_the_animation_tab() {
    let folder = copy_of_example("hello-ui", "open-ui");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let gui = first_of(&app, ClassName::ScreenGui);
    app.open_instance(&ctx, gui);
    assert_eq!(app.selected, Some(gui));
    assert!(app.layout.is_open(Pane::UiDesigner));
    let _ = std::fs::remove_dir_all(&folder);

    let folder = copy_of_example("hello-door", "open-animation");
    let mut app = EditorApp::new(Some(&folder));
    let animation = first_of(&app, ClassName::Animation);
    app.open_instance(&ctx, animation);
    assert!(app.layout.is_open(Pane::Animation));
    let _ = std::fs::remove_dir_all(&folder);
}
