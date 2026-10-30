//! The UI Designer's eyes (what the artboard shows while designing), its
//! Pages tab, and picking an element's image.

use std::path::PathBuf;

use super::tests::copy_of_example;
use super::ui_designer_tests::act;
use super::*;
use crate::ui_designer::{DesignerAction, DesignerTab};

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

fn draw_designer(app: &mut EditorApp, ctx: &egui::Context) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
    });
}

fn child(world: &World, parent: InstanceId, name: &str) -> InstanceId {
    world.find_first_child(parent, name).unwrap()
}

/// hello-ui-graph open in the editor: its Menu, Pages, and the Home and
/// Settings pages.
fn menu(test: &str) -> (PathBuf, EditorApp, [InstanceId; 4]) {
    let folder = copy_of_example("hello-ui-graph", test);
    let app = EditorApp::new(Some(&folder));
    let menu = child(&app.world, app.world.workspace(), "Menu");
    let pages = child(&app.world, menu, "Pages");
    let home = child(&app.world, pages, "Home");
    let settings = child(&app.world, pages, "Settings");
    (folder, app, [menu, pages, home, settings])
}

#[test]
fn eyes_pick_the_page_and_hide_elements_on_the_artboard_only() {
    let (folder, mut app, [_, pages, home, settings]) = menu("ui-pages-eyes");
    let world = app.world.clone();
    let designer = &mut app.ui_designer;
    assert_eq!(designer.artboard_page(&world, pages), Some(home));
    // A page's eye shows that page instead.
    designer.toggle_eye(&world, settings);
    assert_eq!(designer.artboard_page(&world, pages), Some(settings));
    assert!(designer.on_shown_page(&world, child(&world, settings, "Back")));
    assert!(!designer.on_shown_page(&world, child(&world, home, "Title")));
    // Any other element's eye hides it, and what's in it.
    designer.toggle_eye(&world, settings);
    let title = child(&world, settings, "Title");
    designer.toggle_eye(&world, title);
    assert!(designer.hidden_on_artboard(&world, title));
    designer.toggle_eye(&world, title);
    assert!(!designer.hidden_on_artboard(&world, title));
    designer.toggle_eye(&world, pages);
    assert!(designer.hidden_on_artboard(&world, title));
    // None of it changes the game, or is an edit.
    assert_eq!(axcenity_ui::shown_page(&app.world, pages), Some(home));
    assert!(!app.unsaved());

    // Selecting an element on another page shows that page.
    app.ui_designer.toggle_eye(&world, pages);
    act(
        &mut app,
        DesignerAction::Select(Some(child(&world, home, "SettingsButton"))),
    );
    let ctx = theme::test_context();
    draw_designer(&mut app, &ctx);
    assert_eq!(app.ui_designer.artboard_page(&world, pages), Some(home));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_pages_tab_shows_every_page_and_opens_one_to_edit() {
    let (folder, mut app, [menu, pages, home, settings]) = menu("ui-pages-tab");
    let slides = crate::ui_designer::pages::slides(&app.world);
    let titles: Vec<&str> = slides.iter().map(|slide| slide.title.as_str()).collect();
    assert_eq!(titles, ["Menu › Home", "Menu › Settings"]);
    assert_eq!(slides[1].page, Some((pages, settings)));

    let ctx = theme::test_context();
    app.ui_designer.set_tab(DesignerTab::Pages);
    draw_designer(&mut app, &ctx);
    draw_designer(&mut app, &ctx);
    // → moves to the next page.
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events: vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
    });
    assert_eq!(app.ui_designer.slide(), 1);
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        crate::snapshot::snapshot_rendered(
            &snapshots.join("ui-designer-pages.png"),
            SIZE,
            vec![Vec::new(); 3],
            |ctx, render_state| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }

    // Edit page: the Design tab, with Settings shown and selected.
    let mut actions = Vec::new();
    let world = app.world.clone();
    app.ui_designer.edit_slide(&world, &slides[1], &mut actions);
    act(&mut app, actions.remove(0));
    assert_eq!(app.ui_designer.tab(), DesignerTab::Design);
    assert_eq!(app.selected, Some(settings));
    assert_eq!(app.ui_designer.current_gui(), Some(menu));
    draw_designer(&mut app, &ctx);
    assert_eq!(
        app.ui_designer.artboard_page(&app.world, pages),
        Some(settings)
    );
    // The game still starts on Home.
    assert_eq!(axcenity_ui::shown_page(&app.world, pages), Some(home));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn picking_a_screen_gui_in_the_pages_tab_shows_its_page_layouts_and_pages() {
    use crate::ui_designer::pages::{page_layouts, screen_slides};
    let (folder, mut app, [menu, pages, home, settings]) = menu("ui-pages-screens");
    assert_eq!(
        page_layouts(&app.world, menu),
        [(pages, vec![home, settings])]
    );
    // A second ScreenGui, with no pages: one slide, the ScreenGui itself.
    act(&mut app, DesignerAction::NewScreenGui);
    let hud = app.selected.unwrap();
    assert_eq!(app.world.get(hud).unwrap().class(), ClassName::ScreenGui);
    assert!(page_layouts(&app.world, hud).is_empty());
    let hud_slides = screen_slides(&app.world, hud);
    assert_eq!(hud_slides.len(), 1);
    assert_eq!(hud_slides[0].page, None);

    // Its card shows only its pages; picking Menu shows Menu's two.
    let ctx = theme::test_context();
    app.ui_designer.set_tab(DesignerTab::Pages);
    let mut actions = Vec::new();
    app.ui_designer.pick_screen(menu, &mut actions);
    assert_eq!(actions, [DesignerAction::Select(Some(menu))]);
    act(&mut app, actions.remove(0));
    draw_designer(&mut app, &ctx);
    assert_eq!(app.ui_designer.current_gui(), Some(menu));
    assert_eq!(screen_slides(&app.world, menu).len(), 2);
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        for (name, size) in [
            ("ui-designer-pages-screens.png", SIZE),
            ("ui-designer-pages-narrow.png", egui::vec2(420.0, 900.0)),
        ] {
            app.ui_designer.forget_canvases();
            crate::snapshot::snapshot_rendered(
                &snapshots.join(name),
                size,
                vec![Vec::new(); 3],
                |ctx, render_state| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.ui_designer_pane(ui, Some(render_state)));
                },
            );
        }
    }

    // Add pages to the HUD: a PageLayout with its first page, one step.
    act(
        &mut app,
        DesignerAction::Insert {
            what: crate::tree::Insertable::Page,
            parent: hud,
            at: [0.0, 0.0],
        },
    );
    let hud_layouts = page_layouts(&app.world, hud);
    assert_eq!(hud_layouts.len(), 1);
    assert_eq!(hud_layouts[0].1.len(), 1);
    // Start on Settings: the PageLayout's CurrentPage.
    act(
        &mut app,
        DesignerAction::Edit(
            pages,
            crate::properties::Edit::Property("CurrentPage", Value::String("Settings".into())),
        ),
    );
    assert_eq!(axcenity_ui::shown_page(&app.world, pages), Some(settings));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn picking_an_image_sets_it_as_one_undo_step() {
    let (folder, mut app, [menu, ..]) = menu("ui-pages-image");
    act(&mut app, DesignerAction::Select(Some(menu)));
    let label = super::ui_designer_tests::drop_in(
        &mut app,
        crate::tree::Insertable::ImageLabel,
        menu,
        [10.0, 10.0],
    );
    let image = |app: &EditorApp| {
        app.world
            .get(label)
            .and_then(|instance| instance.property("Image"))
            .cloned()
    };
    act(
        &mut app,
        DesignerAction::Image(label, "assets/images/logo.png".into()),
    );
    act(
        &mut app,
        DesignerAction::Image(label, "assets/images/badge.png".into()),
    );
    assert_eq!(
        image(&app),
        Some(Value::String("assets/images/badge.png".into()))
    );
    app.undo();
    assert_eq!(
        image(&app),
        Some(Value::String("assets/images/logo.png".into()))
    );
    let _ = std::fs::remove_dir_all(&folder);
}
