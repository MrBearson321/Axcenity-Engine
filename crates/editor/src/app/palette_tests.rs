//! The command palette and Quick Open (unified Phase 70), now views of
//! universal search (`search_tests.rs`).

use std::collections::BTreeSet;

use crate::command_palette::Category;
use crate::ribbon_tabs::{ActiveTab, BuiltInTab};

use super::command_palette::Command;
use super::search::SearchTarget;
use super::tests::example;
use super::*;

/// The menu bar and the ribbon are drawn this wide, so no ribbon group
/// scrolls out of sight.
const SIZE: egui::Vec2 = egui::vec2(3200.0, 900.0);

/// Draws the menu bar and the ribbon, with AccessKit on so every widget's
/// label and place come back.
fn chrome_frame(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *clock += 0.05;
    ctx.enable_accesskit();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        time: Some(*clock),
        ..Default::default()
    };
    crate::theme::run(ctx, input, |ui| {
        let ctx = &ui.ctx().clone();
        // Nothing fills the Scene's place here: count it as shown, so the
        // ribbon's Scene tools can be pressed as in a whole frame.
        app.scene_drawn_pass = Some(theme::frame_number(ctx));
        egui::Panel::top("menu bar").show(ui, |ui| {
            app.menu_bar(ui);
        });
        egui::Panel::top("ribbon").show(ui, |ui| app.toolbar(ui));
        egui::CentralPanel::default().show(ui, |_| {});
    })
}

/// `ctx` after one frame, so it has fonts (shortcut text needs them).
fn warm(ctx: egui::Context) -> egui::Context {
    let _ = crate::theme::run(&ctx, egui::RawInput::default(), |_ui| {});
    ctx
}

/// A label without its Phosphor icon (a private-use character) or spaces.
fn plain(label: &str) -> String {
    label
        .chars()
        .filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c))
        .collect::<String>()
        .trim()
        .trim_end_matches('⏵')
        .trim()
        .to_owned()
}

/// Every clickable widget drawn: its label (plain) and middle, in points.
fn clickables(output: &egui::FullOutput) -> Vec<(String, egui::Pos2)> {
    use egui::accesskit::Role;
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter(|(_, node)| {
            matches!(
                node.role(),
                Role::Button
                    | Role::CheckBox
                    | Role::RadioButton
                    | Role::Switch
                    | Role::Tab
                    | Role::MenuItem
                    | Role::MenuItemCheckBox
                    | Role::MenuItemRadio
                    | Role::Link
            )
        })
        .filter_map(|(_, node)| {
            let label = plain(node.label()?);
            let bounds = node.bounds()?;
            let middle = egui::pos2(
                ((bounds.x0 + bounds.x1) * 0.5) as f32,
                ((bounds.y0 + bounds.y1) * 0.5) as f32,
            );
            (!label.is_empty()).then_some((label, middle))
        })
        .collect()
}

fn click_at(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64, at: egui::Pos2) {
    use super::ui_designer_tests::press;
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
    ] {
        chrome_frame(app, ctx, clock, events);
    }
}

/// Clicks the widget labelled `label` and returns what's drawn after.
fn open(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    label: &str,
) -> egui::FullOutput {
    let output = chrome_frame(app, ctx, clock, Vec::new());
    let at = clickables(&output)
        .into_iter()
        .find(|(found, _)| found == label)
        .unwrap_or_else(|| panic!("`{label}` is drawn"))
        .1;
    click_at(app, ctx, clock, at);
    chrome_frame(app, ctx, clock, Vec::new())
}

/// Hovers the submenu `label` in what's open and returns what's drawn after.
fn hover(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    label: &str,
) -> egui::FullOutput {
    let output = chrome_frame(app, ctx, clock, Vec::new());
    let at = clickables(&output)
        .into_iter()
        .find(|(found, _)| found == label)
        .unwrap_or_else(|| panic!("`{label}` is drawn"))
        .1;
    chrome_frame(app, ctx, clock, vec![egui::Event::PointerMoved(at)]);
    chrome_frame(app, ctx, clock, Vec::new());
    chrome_frame(app, ctx, clock, Vec::new())
}

/// Closes whatever menu is open with a click on nothing.
fn close(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64) {
    click_at(app, ctx, clock, egui::pos2(SIZE.x - 10.0, SIZE.y - 10.0));
    chrome_frame(app, ctx, clock, Vec::new());
}

#[test]
fn every_ribbon_and_menu_command_can_be_run_from_the_palette() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = warm(theme::test_context());
    let mut clock = 0.0;
    let mut drawn = BTreeSet::new();
    let mut note = |output: &egui::FullOutput| {
        for (label, _) in clickables(output) {
            drawn.insert(label);
        }
    };

    // The menus, and their submenus.
    let menus: &[(&str, &[&str])] = &[
        ("File", &["Build Game"]),
        ("Edit", &["Undo History"]),
        ("View", &["Layout"]),
        ("Viewport", &["View Mode"]),
        ("Debug", &[]),
        ("Help", &[]),
    ];
    for (menu, submenus) in menus {
        note(&open(&mut app, &ctx, &mut clock, menu));
        for submenu in *submenus {
            note(&hover(&mut app, &ctx, &mut clock, submenu));
        }
        close(&mut app, &ctx, &mut clock);
    }

    // Each built-in ribbon tab, and each dropdown on it.
    let dropdowns = [
        "Light",
        "Sky",
        "Sound",
        "Post FX",
        "Character",
        "Interact",
        "Spawn",
        "Vehicle",
        "Layout",
        "Elements",
        "Constraint",
        "Brush",
        "Panels",
        "Overlays",
        "View Mode",
    ];
    for tab in BuiltInTab::ALL {
        app.ribbon_tabs.set_active(ActiveTab::BuiltIn(tab));
        let output = chrome_frame(&mut app, &ctx, &mut clock, Vec::new());
        note(&output);
        let here: Vec<String> = clickables(&output).into_iter().map(|(l, _)| l).collect();
        for dropdown in dropdowns {
            if here.iter().any(|label| label == dropdown) {
                note(&open(&mut app, &ctx, &mut clock, dropdown));
                close(&mut app, &ctx, &mut clock);
            }
        }
    }

    // What opens a menu isn't a command; the Clients group's Network menu
    // holds values (latency, jitter, loss), not commands.
    let openers: BTreeSet<&str> = menus
        .iter()
        .flat_map(|(menu, submenus)| std::iter::once(*menu).chain(submenus.iter().copied()))
        .chain(dropdowns)
        .chain(["Network"])
        .collect();
    let commands = app.palette_commands(&ctx);
    // A menu item's label ends with its shortcut ("Save ⌘S").
    let names: BTreeSet<String> = commands
        .iter()
        .flat_map(|command| {
            let entry = &command.entry;
            std::iter::once(&entry.title)
                .chain(&entry.aliases)
                .flat_map(|name| {
                    let with_shortcut = entry
                        .shortcut
                        .as_ref()
                        .map(|shortcut| format!("{name} {shortcut}"));
                    std::iter::once(name.clone()).chain(with_shortcut)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let missing: Vec<&String> = drawn
        .iter()
        .filter(|label| !openers.contains(label.as_str()) && !names.contains(label.as_str()))
        .collect();
    assert!(missing.is_empty(), "not in the palette: {missing:?}");
    // It really went through the menus and dropdowns.
    for label in [
        "Version History…",
        "For Windows",
        "Normals",
        "PointLight",
        "HingeConstraint",
        "Bake Probes",
        "Raise / Lower",
    ] {
        assert!(drawn.contains(label), "`{label}` was drawn: {drawn:?}");
    }
    assert!(drawn.len() > 150, "{} drawn", drawn.len());
}

/// Runs one frame of the palette with `events`.
fn palette_frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let _ = crate::theme::run(ctx, input, |ui| {
        let ctx = &ui.ctx().clone();
        app.palette_frame(ctx)
    });
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    super::ui_designer_tests::key(key, modifiers)
}

#[test]
fn ctrl_shift_p_finds_a_command_by_a_few_letters_and_enter_runs_it() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    let workspace = app.world.workspace();
    let before = app.world.get(workspace).unwrap().children().len();
    let shift_command = egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT);
    palette_frame(&mut app, &ctx, vec![key(egui::Key::P, shift_command)]);
    assert!(app.palette.open);
    assert!(app.palette.query.starts_with('>'), "commands only");
    palette_frame(&mut app, &ctx, vec![egui::Event::Text("ins blo".into())]);
    assert_eq!(app.palette.query, "> ins blo");
    palette_frame(&mut app, &ctx, Vec::new());
    palette_frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(!app.palette.open);
    assert_eq!(
        app.world.get(workspace).unwrap().children().len(),
        before + 1,
        "Insert Block ran"
    );
    // Its shortcut is listed.
    let commands = app.palette_commands(&ctx);
    let save = commands
        .iter()
        .find(|command| command.command == Command::Save)
        .unwrap();
    assert!(
        save.entry
            .shortcut
            .as_deref()
            .is_some_and(|s| s.contains('S'))
    );
}

#[test]
fn the_palette_shows_the_last_commands_run_first() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = warm(theme::test_context());
    for command in [
        Command::Overlay(super::command_palette::Overlay::Bounds),
        Command::ViewCube,
    ] {
        let commands = app.palette_commands(&ctx);
        let entry = commands.iter().find(|c| c.command == command).unwrap();
        app.palette.used(&entry.entry.key);
        app.run_command(&ctx, command);
    }
    assert!(app.overlays.bounds);
    let commands = app.palette_commands(&ctx);
    let entries: Vec<_> = commands.iter().map(|c| c.entry.clone()).collect();
    let ranked = app.palette.ranked(&[&entries], 0).to_vec();
    assert_eq!(commands[ranked[0].1].command, Command::ViewCube);
    assert_eq!(
        commands[ranked[1].1].command,
        Command::Overlay(super::command_palette::Overlay::Bounds)
    );
}

#[test]
fn plugin_toolbar_buttons_are_commands_too() {
    let app = EditorApp::new(Some(&example("hello-plugins")));
    let ctx = warm(theme::test_context());
    let commands = app.palette_commands(&ctx);
    let plugin: Vec<_> = commands
        .iter()
        .filter(|c| matches!(c.command, Command::PluginButton { .. }))
        .collect();
    assert!(
        !plugin.is_empty(),
        "hello-plugins' toolbar buttons are listed"
    );
    assert!(
        plugin
            .iter()
            .all(|c| c.entry.detail.starts_with("Plugin › "))
    );
}

#[test]
fn quick_open_finds_scripts_assets_levels_and_instances_and_opens_them() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    let rows = |app: &EditorApp, category: Category| -> Vec<(String, SearchTarget)> {
        app.search
            .rows(category)
            .into_iter()
            .map(|(entry, target)| (entry.title.clone(), target.clone()))
            .collect()
    };
    let find = |category: Category, name: &str| {
        rows(&app, category)
            .into_iter()
            .find(|(title, _)| title == name)
            .map(|(_, target)| target)
            .unwrap_or_else(|| panic!("{name} is listed"))
    };
    let pad = find(Category::Object, "Pad");
    app.run_search_target(&ctx, pad);
    assert_eq!(
        app.selected
            .and_then(|id| app.world.get(id))
            .map(|i| i.name().to_owned()),
        Some("Pad".to_owned())
    );
    let (_, script) = rows(&app, Category::Script).into_iter().next().unwrap();
    let SearchTarget::Open(super::command_palette::OpenTarget::Script(path)) = script.clone()
    else {
        panic!("a script row opens its script")
    };
    app.run_search_target(&ctx, script);
    assert_eq!(app.tabs.active_path(), Some(path.as_str()));
    assert!(
        rows(&app, Category::Level)
            .iter()
            .any(|(_, target)| matches!(
                target,
                SearchTarget::Open(super::command_palette::OpenTarget::Level(None))
            ))
    );
}

/// The palette, Find in Scripts and References windows draw on screen
/// (with `AXCENITY_SNAPSHOTS`, as PNGs to look at).
#[test]
fn the_palette_and_find_windows_draw_on_screen() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let size = egui::vec2(1100.0, 700.0);
    app.palette.toggle_commands();
    app.palette.query = "> light cones".to_owned();
    app.find.show_and_focus();
    app.find.query = "Pad".to_owned();
    app.search_scripts();
    let mut draw = |ui: &mut egui::Ui| {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |_| {});
        app.palette_frame(ctx);
    };
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    let mut output = None;
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        output = Some(crate::theme::run(&ctx, input, |ui| draw(ui)));
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(snapshots).join("command-palette.png");
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let rows = clickables(output.as_ref().unwrap());
    assert!(
        rows.iter()
            .any(|(label, _)| label == "Overlay: Light Cones" || label == "Light Cones")
    );
    assert!(rows.iter().all(|(_, at)| screen.contains(*at)), "{rows:?}");
}

/// Edit ▸ Undo History lists the steps, newest first; clicking one undoes
/// it and every step after it, and clicking a dimmed one redoes up to it.
#[test]
fn undo_history_jumps_back_and_forward_several_steps() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = warm(theme::test_context());
    let mut clock = 0.0;
    let workspace = app.world.workspace();
    let parts: Vec<InstanceId> = app.world.get(workspace).unwrap().children().to_vec();
    for (i, &id) in parts.iter().take(3).enumerate() {
        app.act(Action::Rename(id, format!("Renamed{i}")));
    }
    let name = |app: &EditorApp, i: usize| app.world.get(parts[i]).unwrap().name().to_owned();
    let steps = app.history.undo_labels().len();
    assert!(steps >= 3, "{:?}", app.history.undo_labels());
    let third_newest = app.history.undo_labels()[2].to_owned();

    open(&mut app, &ctx, &mut clock, "Edit");
    let shown = hover(&mut app, &ctx, &mut clock, "Undo History");
    let rows: Vec<(String, egui::Pos2)> = clickables(&shown);
    let at = rows
        .iter()
        .filter(|(label, _)| *label == third_newest)
        .map(|(_, at)| *at)
        .next()
        .unwrap_or_else(|| panic!("`{third_newest}` is listed: {rows:?}"));
    click_at(&mut app, &ctx, &mut clock, at);
    // All three renames are undone, in one click.
    assert_eq!(app.history.undo_labels().len(), steps - 3);
    assert_eq!(app.history.redo_labels().len(), 3);
    assert!((0..3).all(|i| !name(&app, i).starts_with("Renamed")));

    // Redo up to the second rename, from the dimmed rows.
    app.jump_in_history(2);
    assert_eq!(name(&app, 0), "Renamed0");
    assert_eq!(name(&app, 1), "Renamed1");
    assert!(!name(&app, 2).starts_with("Renamed"));
    assert_eq!(app.history.redo_labels().len(), 1);
    // Asking for more than there is stops at the end.
    app.jump_in_history(10);
    assert_eq!(name(&app, 2), "Renamed2");
    assert!(app.history.redo_labels().is_empty());
}

/// Every ribbon tab is as tall as the tallest (Home, with its snapping
/// grid), so the layout under it doesn't jump when tabs change.
#[test]
fn the_ribbon_keeps_one_height_across_tabs() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let ctx = warm(theme::test_context());
    let mut clock = 0.0;
    let mut heights = Vec::new();
    for tab in BuiltInTab::ALL.into_iter().chain([BuiltInTab::World]) {
        app.ribbon_tabs.set_active(ActiveTab::BuiltIn(tab));
        for _ in 0..3 {
            chrome_frame(&mut app, &ctx, &mut clock, Vec::new());
        }
        let ribbon = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("ribbon"))
            .expect("the ribbon was drawn")
            .outer_rect;
        heights.push((tab, ribbon.height()));
    }
    let first = heights[0].1;
    assert!(
        heights.iter().all(|(_, h)| (h - first).abs() < 0.5),
        "{heights:?}"
    );
}
