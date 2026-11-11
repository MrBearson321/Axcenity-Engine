//! Universal search (Ctrl+K): what the index lists, that it follows
//! changes without listing everything again, and that each row opens the
//! right thing.

use std::time::Instant;

use crate::command_palette::{Category, Palette};

use super::search::SearchTarget;
use super::settings::{self, SettingsPage};
use super::tests::{copy_of_example, example};
use super::*;

/// One frame of the palette alone, 1200 by 800.
fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| app.palette_frame(ctx));
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    super::ui_designer_tests::key(key, modifiers)
}

/// The titles of `category`'s rows.
fn titles(app: &EditorApp, category: Category) -> Vec<String> {
    app.search
        .rows(category)
        .into_iter()
        .map(|(entry, _)| entry.title.clone())
        .collect()
}

/// The target of the first row of `category` titled `title`.
fn target(app: &EditorApp, category: Category, title: &str) -> SearchTarget {
    app.search
        .rows(category)
        .into_iter()
        .find(|(entry, _)| entry.title == title)
        .map(|(_, target)| target.clone())
        .unwrap_or_else(|| panic!("`{title}` is listed under {category:?}"))
}

/// The best row for `query`, as the palette ranks it.
fn best(app: &EditorApp, query: &str) -> (String, Category) {
    let entries = app.search.entries();
    let hits = crate::command_palette::rank(&entries, query, None, &[]);
    let (bucket, index) = hits.first().copied().expect("something matches");
    let entry = &entries[bucket][index];
    (entry.title.clone(), entry.category)
}

/// How many times the providers listed their rows, all together.
fn listings(app: &EditorApp) -> usize {
    app.search.listings.iter().sum()
}

#[test]
fn ctrl_k_searches_every_category_and_enter_opens_the_pick() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::K, egui::Modifiers::COMMAND)],
    );
    assert!(app.palette.open);
    assert_eq!(app.palette.query, "", "everything, not only commands");
    // Every kind of row is there.
    for category in [
        Category::Command,
        Category::Tool,
        Category::Object,
        Category::Script,
        Category::Level,
        Category::Setting,
    ] {
        assert!(
            !app.search.rows(category).is_empty(),
            "{category:?} rows are listed"
        );
    }
    frame(&mut app, &ctx, vec![egui::Event::Text("Pad".into())]);
    frame(&mut app, &ctx, Vec::new());
    assert_eq!(best(&app, "Pad"), ("Pad".to_owned(), Category::Object));
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(!app.palette.open);
    let selected = app.selected.and_then(|id| app.world.get(id));
    assert_eq!(selected.map(|pad| pad.name()), Some("Pad"));
    assert!(app.layout.is_open(Pane::Explorer));
    // Ctrl+K again opens it; Escape closes it.
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::K, egui::Modifiers::COMMAND)],
    );
    assert!(app.palette.open);
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.palette.open);
}

#[test]
fn tab_narrows_the_list_to_one_category() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.palette.toggle();
    frame(&mut app, &ctx, Vec::new());
    frame(&mut app, &ctx, vec![egui::Event::Text("pad".into())]);
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    let scope = app.palette.scope.expect("a category is picked");
    let entries = app.search.entries();
    let hits = crate::command_palette::rank(&entries, "pad", Some(scope), &[]);
    assert!(!hits.is_empty());
    assert!(
        hits.iter()
            .all(|&(bucket, index)| entries[bucket][index].category == scope)
    );
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
    );
    assert_eq!(app.palette.scope, None, "Shift+Tab goes back to All");
}

#[test]
fn typing_ranks_again_but_lists_nothing_again() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::K, egui::Modifiers::COMMAND)],
    );
    let opened = listings(&app);
    for letter in ["P", "a", "d"] {
        frame(&mut app, &ctx, vec![egui::Event::Text(letter.into())]);
    }
    assert_eq!(app.palette.query, "Pad");
    assert_eq!(listings(&app), opened, "keystrokes list nothing again");
    // Closing and opening again lists only what has no stamp (commands,
    // levels): objects, scripts, assets and settings are as they were.
    app.palette.open = false;
    frame(&mut app, &ctx, Vec::new());
    let objects_before = app.search.rows(Category::Object).len();
    app.palette.toggle();
    frame(&mut app, &ctx, Vec::new());
    assert_eq!(app.search.rows(Category::Object).len(), objects_before);
    // Two command providers and Levels.
    assert_eq!(listings(&app), opened + 3);
}

#[test]
fn the_index_follows_creating_renaming_and_deleting_objects() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    let listed = listings(&app);
    app.refresh_search(&ctx, false);
    assert_eq!(listings(&app), listed, "nothing changed, nothing listed");

    let part = app.world.create(ClassName::Part).unwrap();
    app.world.set_name(part, "Lantern").unwrap();
    let workspace = app.world.workspace();
    app.world.set_parent(part, Some(workspace)).unwrap();
    app.refresh_search(&ctx, false);
    assert!(titles(&app, Category::Object).contains(&"Lantern".to_owned()));
    assert_eq!(listings(&app), listed + 1, "only the objects again");
    assert_eq!(
        target(&app, Category::Object, "Lantern"),
        SearchTarget::Open(super::command_palette::OpenTarget::Instance(part))
    );

    app.world.set_name(part, "Torch").unwrap();
    app.refresh_search(&ctx, false);
    let objects = titles(&app, Category::Object);
    assert!(objects.contains(&"Torch".to_owned()));
    assert!(!objects.contains(&"Lantern".to_owned()));

    // Moving it changes where it says it is.
    let model = app.world.create(ClassName::Model).unwrap();
    app.world.set_name(model, "Props").unwrap();
    app.world.set_parent(model, Some(workspace)).unwrap();
    app.world.set_parent(part, Some(model)).unwrap();
    app.refresh_search(&ctx, false);
    let row = app
        .search
        .rows(Category::Object)
        .into_iter()
        .find(|(entry, _)| entry.title == "Torch")
        .map(|(entry, _)| entry.detail.clone())
        .unwrap();
    assert!(row.ends_with("Workspace.Props.Torch"), "{row}");

    app.world.destroy(part).unwrap();
    app.refresh_search(&ctx, false);
    assert!(!titles(&app, Category::Object).contains(&"Torch".to_owned()));
}

#[test]
fn play_searches_the_session_world_and_stop_goes_back() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    let editing = titles(&app, Category::Object);
    app.play();
    app.refresh_search(&ctx, false);
    let playing = titles(&app, Category::Object);
    assert!(playing.contains(&"Pad".to_owned()));
    app.stop();
    app.refresh_search(&ctx, false);
    assert_eq!(titles(&app, Category::Object), editing);
}

#[test]
fn assets_show_up_and_go_as_files_come_and_go() {
    let folder = copy_of_example("hello-touched", "search-assets");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    assert!(!titles(&app, Category::Asset).contains(&"lantern.ogg".to_owned()));
    let sounds = folder.join("assets/sounds");
    std::fs::create_dir_all(&sounds).unwrap();
    std::fs::write(sounds.join("lantern.ogg"), b"not really a sound").unwrap();
    // The watcher's note, as a file change arrives.
    app.asset_browser.file_changed("assets/sounds/lantern.ogg");
    app.refresh_search(&ctx, false);
    assert!(titles(&app, Category::Asset).contains(&"lantern.ogg".to_owned()));
    // Its kind finds it too.
    let entries = app.search.entries();
    let hits = crate::command_palette::rank(&entries, "sound lantern", Some(Category::Asset), &[]);
    assert_eq!(hits.len(), 1);
    let listed = listings(&app);
    app.refresh_search(&ctx, false);
    assert_eq!(listings(&app), listed, "the scan found nothing new");

    std::fs::remove_file(sounds.join("lantern.ogg")).unwrap();
    app.asset_browser.refresh();
    app.refresh_search(&ctx, false);
    assert!(!titles(&app, Category::Asset).contains(&"lantern.ogg".to_owned()));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn synonyms_and_descriptions_find_commands_and_tools() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    assert_eq!(best(&app, "> remove").0, "Delete");
    assert!(best(&app, "> export").0.starts_with("Build Game"));
    assert_eq!(best(&app, "> preferences").0, "Settings…");
    // Tools are their own category, and say what they do.
    let (title, category) = best(&app, "> move tool");
    assert_eq!((title.as_str(), category), ("Move Tool", Category::Tool));
    let rows = app.search.rows(Category::Tool);
    let terrain = rows
        .iter()
        .find(|(entry, _)| entry.title == "Terrain Studio")
        .unwrap();
    assert!(!terrain.0.description.is_empty());
    // Every command says what it does.
    let silent: Vec<String> = app
        .search
        .rows(Category::Command)
        .into_iter()
        .chain(app.search.rows(Category::Tool))
        .filter(|(entry, _)| entry.description.is_empty())
        .map(|(entry, _)| entry.title.clone())
        .collect();
    assert!(silent.is_empty(), "no description: {silent:?}");
}

#[test]
fn every_searchable_setting_is_found_on_its_page() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    let mut clock = 0.0;
    for row in settings::SEARCHABLE {
        if row.page == SettingsPage::Engine {
            continue;
        }
        ctx.data_mut(|data| data.remove::<String>(egui::Id::new("settings found")));
        app.run_search_target(&ctx, SearchTarget::Setting(row.page, row.name));
        assert!(app.settings.open);
        assert_eq!(app.settings.page, row.page);
        for _ in 0..2 {
            clock += 0.1;
            let input = egui::RawInput {
                time: Some(clock),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.settings_window(ctx);
            });
        }
        if row.name == "Steady frame rate"
            && let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS")
        {
            let mut asked = false;
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("settings-found.png"),
                egui::vec2(900.0, 640.0),
                vec![Vec::new(); 3],
                |ctx| {
                    if !std::mem::replace(&mut asked, true) {
                        settings::find_setting(ctx, row.name);
                    }
                    egui::CentralPanel::default().show(ctx, |_| {});
                    app.settings_window(ctx);
                },
            );
        }
        let found = ctx.data(|data| data.get_temp::<String>(egui::Id::new("settings found")));
        assert_eq!(
            found.as_deref(),
            Some(row.name),
            "`{}` is drawn on {:?}",
            row.name,
            row.page
        );
    }
}

#[test]
fn a_property_row_shows_that_property_of_the_selection() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    assert!(
        app.search.rows(Category::Property).is_empty(),
        "nothing selected, no properties"
    );
    let pad = app
        .world
        .find_first_child(app.world.workspace(), "Pad")
        .unwrap();
    app.select_from_viewport(Some(pad));
    app.refresh_search(&ctx, false);
    let transparency = target(&app, Category::Property, "Transparency");
    assert_eq!(
        transparency,
        SearchTarget::Property(pad, "Transparency".to_owned())
    );
    app.select_from_viewport(None);
    app.run_search_target(&ctx, transparency);
    assert_eq!(app.selected, Some(pad), "it selects the object again");
    assert!(app.layout.is_open(Pane::Properties));
    let filter = ctx.data(|data| data.get_temp::<String>(egui::Id::new("properties filter")));
    assert_eq!(filter.as_deref(), Some("Transparency"));
}

/// The palette over everything, drawn (with `AXCENITY_SNAPSHOTS`, as a
/// PNG): the chips, rows from several categories, and the line saying
/// what the highlighted one does.
#[test]
fn universal_search_draws_categories_and_what_the_pick_does() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let size = egui::vec2(1100.0, 700.0);
    app.palette.toggle();
    app.palette.query = "pad".to_owned();
    let mut draw = |ctx: &egui::Context| {
        egui::CentralPanel::default().show(ctx, |_| {});
        app.palette_frame(ctx);
    };
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let _ = ctx.run(input, &mut draw);
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let folder = std::path::PathBuf::from(snapshots);
        crate::snapshot::snapshot(
            &folder.join("universal-search.png"),
            size,
            vec![Vec::new(); 3],
            &mut draw,
        );
        app.palette.query = "> shadow".to_owned();
        crate::snapshot::snapshot(
            &folder.join("universal-search-commands.png"),
            size,
            vec![Vec::new(); 3],
            |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.palette_frame(ctx);
            },
        );
        app.palette.query = "shadow".to_owned();
        crate::snapshot::snapshot(
            &folder.join("universal-search-settings.png"),
            size,
            vec![Vec::new(); 3],
            |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.palette_frame(ctx);
            },
        );
    }
    assert!(app.palette.open);
}

/// Ranking and listing with 100 to 10,000 objects. Ignored: run it in
/// release for numbers,
/// `cargo test -p axcenity-editor --release --bins search_cost -- --ignored --nocapture`.
#[test]
#[ignore = "timing; run in release with --ignored --nocapture"]
fn search_cost_at_100_1000_and_10000_objects() {
    let ctx = theme::test_context();
    for count in [100, 1_000, 10_000] {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let workspace = app.world.workspace();
        let folder = app.world.create(ClassName::Folder).unwrap();
        app.world.set_parent(folder, Some(workspace)).unwrap();
        for index in 0..count {
            let part = app.world.create(ClassName::Part).unwrap();
            app.world.set_name(part, format!("Crate {index}")).unwrap();
            app.world.set_parent(part, Some(folder)).unwrap();
        }
        let start = Instant::now();
        app.refresh_search(&ctx, true);
        let list_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        app.refresh_search(&ctx, false);
        let check_ms = start.elapsed().as_secs_f64() * 1000.0;
        let entries = app.search.entries();
        let start = Instant::now();
        let mut palette = Palette::default();
        palette.toggle();
        palette.query.push_str("crate 99");
        let found = palette.ranked(&entries, 1).len();
        let rank_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let _ = palette.ranked(&entries, 1).len();
        let cached_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{count:>6} objects: list {list_ms:.2} ms, unchanged check {check_ms:.3} ms, \
             rank \"crate 99\" {rank_ms:.2} ms ({found} rows), cached {cached_ms:.4} ms"
        );
    }
}
