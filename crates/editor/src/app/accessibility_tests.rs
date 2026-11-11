//! The UI Designer's game UI settings menu: it shows the game UI with a
//! player's text scale, contrast and reduced motion, Play starts with them,
//! and the editor's own theme never changes.

use axcenity_core::UiSettings;

use super::tests::copy_of_example;
use super::ui_designer_tests::{act, click_named, designer_frame, stylesheets};
use super::*;
use crate::theme::icons;

/// The title's text as the artboard draws hello-accessibility's menu.
fn title(app: &EditorApp, world: &World) -> axcenity_render::UiText {
    let gui = world
        .find_first_child(world.workspace(), "Settings")
        .unwrap();
    let layout =
        axcenity_ui::layout_artboard(world, gui, &stylesheets(app), (1280, 720), 1.0, [0.0, 0.0]);
    layout
        .draw_items
        .into_iter()
        .find_map(|item| match item {
            axcenity_render::UiDrawItem::Text(text) if text.text == "Settings" => Some(text),
            _ => None,
        })
        .expect("the title is drawn")
}

#[test]
fn the_designer_previews_game_ui_settings_play_starts_with_them_and_the_editor_theme_stays() {
    let folder = copy_of_example("hello-accessibility", "accessibility-preview");
    let mut app = EditorApp::new(Some(&folder));
    let appearance = app.appearance.clone();
    let undo = app.history.undo_label().map(str::to_owned);
    assert_eq!(title(&app, &app.world).font_size, 24.0);

    // The toolbar's menu: ticking Higher contrast shows the artboard that way.
    let ctx = theme::test_context();
    let palette = theme::palette(&ctx);
    let zoom = ctx.zoom_factor();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 700.0);
    designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    click_named(&mut app, &ctx, &mut clock, size, icons::PERSON_ARMS_SPREAD);
    click_named(&mut app, &ctx, &mut clock, size, "Higher contrast");
    assert!(app.world.ui_settings().high_contrast);
    assert_eq!(title(&app, &app.world).color[..3], [1.0; 3]);

    let chosen = UiSettings {
        text_scale: 1.5,
        high_contrast: true,
        reduced_motion: true,
    };
    act(&mut app, DesignerAction::UiSettings(chosen));
    assert_eq!(title(&app, &app.world).font_size, 36.0);
    // Not an undo step, and the editor looks as it did.
    assert_eq!(app.history.undo_label().map(str::to_owned), undo);
    designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert_eq!(app.appearance, appearance);
    assert_eq!(theme::palette(&ctx), palette);
    assert_eq!(ctx.zoom_factor(), zoom);
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(folder).join("accessibility-preview.png");
        crate::snapshot::snapshot_rendered(&path, size, vec![Vec::new(); 3], |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, Some(render)));
        });
    }

    // The editor's own reduced motion is a different setting.
    app.appearance.reduce_motion = true;
    app.designer_actions(vec![DesignerAction::UiSettings(UiSettings::default())]);
    assert_eq!(app.world.ui_settings(), UiSettings::default());
    assert!(app.appearance.reduce_motion);
    app.appearance.reduce_motion = false;
    act(&mut app, DesignerAction::UiSettings(chosen));

    // Play starts with them, the menu changes the game's while it runs,
    // and Stop leaves the designer's.
    app.play();
    let session = app.play.as_ref().expect("playing");
    assert_eq!(session.world().ui_settings(), chosen);
    app.designer_actions(vec![DesignerAction::UiSettings(UiSettings::default())]);
    let session = app.play.as_ref().expect("playing");
    assert_eq!(session.world().ui_settings(), UiSettings::default());
    app.stop();
    assert_eq!(app.world.ui_settings(), chosen);

    // They're the player's, not the project's: a game starts without them.
    let project = axcenity_project::load(&folder).unwrap();
    assert_eq!(project.world.ui_settings(), UiSettings::default());
    let _ = std::fs::remove_dir_all(&folder);
}
