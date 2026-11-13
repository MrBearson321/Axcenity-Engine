//! The welcome page shows where the documents go once the Scene is closed
//! and nothing else is open, lists the tabs closed last, and goes when a
//! document opens.

use super::tests::{copy_of_example, drawn_texts};
use super::welcome::RecentTabs;
use super::*;

fn frame(app: &mut EditorApp, ctx: &egui::Context) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    let output = crate::theme::run(ctx, input, |ui| app.frame(ui, None));
    drawn_texts(&output)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

#[test]
fn closing_the_scene_shows_the_welcome_page_with_the_tabs_closed_last() {
    let folder = copy_of_example("hello-touched", "welcome");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let path = "scripts/pad.server.lua".to_string();
    app.open_script(&path, None);
    frame(&mut app, &ctx);
    app.request_and_go_ahead(&ctx, Discard::CloseTab(path.clone()));
    assert_eq!(app.recent_tabs.of(&folder), std::slice::from_ref(&path));

    app.pending_close_scene = true;
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = frame(&mut app, &ctx);
    }
    assert!(app.layout.is_open(Pane::Welcome));
    assert!(
        texts.iter().any(|text| text.contains("Nothing is open")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("pad.server.lua")),
        "the closed tab is listed: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("Search everything")),
        "{texts:?}"
    );

    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(folder).join("welcome-page.png");
        // The snapshot has its own context, so the logo loads again in it.
        app.brand = None;
        crate::snapshot::snapshot(
            &path,
            egui::vec2(1440.0, 900.0),
            vec![Vec::new(); 3],
            |ui| app.frame(ui, None),
        );
    }

    // Opening the script again takes the page's place.
    app.open_script(&path, None);
    frame(&mut app, &ctx);
    assert!(!app.layout.is_open(Pane::Welcome));
    assert!(app.layout.is_open(Pane::Script(app.tabs.ids()[0])));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn recent_tabs_are_kept_per_project_newest_first_and_saved() {
    let mut recent = RecentTabs::default();
    let a = std::path::Path::new("projects/a");
    let b = std::path::Path::new("projects/b");
    recent.closed(a, "scripts/one.lua");
    recent.closed(a, "scripts/two.lua");
    recent.closed(a, "scripts/one.lua");
    recent.closed(b, "scripts/other.lua");
    assert_eq!(recent.of(a), ["scripts/one.lua", "scripts/two.lua"]);
    assert_eq!(recent.of(b), ["scripts/other.lua"]);
    for n in 0..20 {
        recent.closed(a, &format!("scripts/{n}.lua"));
    }
    assert_eq!(recent.of(a).len(), 8, "only the last few");
    assert_eq!(RecentTabs::from_text(&recent.to_text()), recent);
    assert_eq!(RecentTabs::from_text("not json"), RecentTabs::default());
}

/// The Home tab's group captions sit on one line, whatever their rows hold
/// (Snapping's two rows of fields used to push its caption lower).
#[test]
fn ribbon_captions_share_one_baseline() {
    let folder = copy_of_example("hello-touched", "ribbon-captions");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    let mut output = None;
    for _ in 0..4 {
        output = Some(crate::theme::run(&ctx, input(), |ui| app.frame(ui, None)));
    }
    // Where each text's line starts (a glyph box's middle moves with
    // descenders: Snapping's p and g).
    let texts: Vec<(String, egui::Pos2)> = output
        .unwrap()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
            _ => None,
        })
        .collect();
    let caption = |name: &str| {
        texts
            .iter()
            // In the ribbon, not the Explorer's Game row below it.
            .filter(|(text, at)| text == name && at.y < 200.0)
            .map(|(_, at)| at.y)
            .fold(f32::MIN, f32::max)
    };
    let game = caption("Game");
    for name in ["Tools", "Snapping", "Parts"] {
        assert!(
            (caption(name) - game).abs() < 0.5,
            "{name} at {} but Game at {game}",
            caption(name)
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// Settings > Appearance > Menu icons places the icons of the ribbon's
/// tick boxes (Snapping's Move, Surfaces, …) as it does in menus: Centered
/// moves the icon to the middle of its column, and the label stays put.
#[test]
fn ribbon_tick_box_icons_follow_the_menu_icons_setting() {
    let folder = copy_of_example("hello-touched", "ribbon-menu-icons");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    // Where Snapping's Move icon's middle and its label's start are drawn.
    let mut placed = |place: theme::MenuIcons| {
        app.appearance.menu_icons = place;
        // As Settings does when a choice changes.
        theme::apply(&ctx, &app.appearance);
        let mut output = None;
        for _ in 0..4 {
            output = Some(crate::theme::run(&ctx, input(), |ui| app.frame(ui, None)));
        }
        let texts: Vec<(String, egui::Rect)> = output
            .unwrap()
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.pos.y < 200.0 => {
                    Some((text.galley.text().to_owned(), text.visual_bounding_rect()))
                }
                _ => None,
            })
            .collect();
        let icon = texts
            .iter()
            .find(|(text, _)| text == icons::GRID_FOUR)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("{place:?}: no Move icon in the ribbon"));
        let label = texts
            .iter()
            .filter(|(text, rect)| text == "Move" && rect.left() > icon.left())
            .map(|(_, rect)| rect.left())
            .fold(f32::MAX, f32::min);
        (icon.center().x, label)
    };
    let (left_icon, left_label) = placed(theme::MenuIcons::Left);
    let (centered_icon, centered_label) = placed(theme::MenuIcons::Centered);
    assert!(
        centered_icon > left_icon + 2.0,
        "Centered icon at {centered_icon}, Left at {left_icon}"
    );
    assert!(
        (centered_label - left_label).abs() < 0.5,
        "label moved from {left_label} to {centered_label}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
