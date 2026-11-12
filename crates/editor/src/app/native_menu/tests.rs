//! The macOS menu bar's menus, as data, on every OS. That they hold what
//! the header's menus hold is checked in `palette_tests.rs`, which can
//! open those.

use super::super::tests::{copy_of_example, example};
use super::*;

fn items(menus: &[TopMenu], title: &str) -> Vec<MenuNode> {
    menus
        .iter()
        .find(|menu| menu.title == title)
        .unwrap_or_else(|| panic!("a {title} menu"))
        .items
        .clone()
}

/// Every item in `nodes`, submenus' too.
fn all_items(nodes: &[MenuNode]) -> Vec<MenuItem> {
    let mut found = Vec::new();
    for node in nodes {
        match node {
            MenuNode::Item(item) => found.push(item.clone()),
            MenuNode::Submenu { items, .. } => found.extend(all_items(items)),
            _ => {}
        }
    }
    found
}

fn find(menus: &[TopMenu], title: &str, label: &str) -> MenuItem {
    all_items(&items(menus, title))
        .into_iter()
        .find(|item| item.label == label)
        .unwrap_or_else(|| panic!("`{label}` in {title}"))
}

#[test]
fn the_menus_are_laid_out_the_mac_way() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let menus = app.native_menus();
    let titles: Vec<&str> = menus.iter().map(|menu| menu.title).collect();
    assert_eq!(
        titles,
        [
            APP_NAME, "File", "Edit", "View", "Viewport", "Debug", "Window", "Help"
        ]
    );
    assert_eq!(menus[0].role, MenuRole::App);
    assert_eq!(menus[6].role, MenuRole::Window);
    assert_eq!(menus[7].role, MenuRole::Help);

    // About, Settings and Quit are the app's, not File's or Help's.
    let app_menu = all_items(&items(&menus, APP_NAME));
    for label in ["About Axcenity", "Settings…", "Check for Updates…"] {
        assert!(app_menu.iter().any(|item| item.label == label), "{label}");
    }
    let quit = find(&menus, APP_NAME, &format!("Quit {APP_NAME}"));
    assert_eq!(quit.action, MenuAction::Command(Command::Quit));
    for (menu, label) in [
        ("File", "Quit"),
        ("File", "Settings…"),
        ("Help", "About Axcenity"),
    ] {
        assert!(
            !all_items(&items(&menus, menu))
                .iter()
                .any(|item| item.label == label),
            "{label} stays out of {menu}"
        );
    }
    for system in [SystemItem::Hide, SystemItem::Services] {
        assert!(items(&menus, APP_NAME).contains(&MenuNode::System(system)));
    }
    // Full screen in View, the window's own commands in Window.
    assert!(items(&menus, "View").contains(&MenuNode::System(SystemItem::FullScreen)));
    assert!(items(&menus, "Window").contains(&MenuNode::System(SystemItem::Minimize)));
    // Keys from the shortcut table.
    assert_eq!(
        find(&menus, "File", "Save").shortcut,
        Some(shortcuts::Action::Save)
    );
    assert_eq!(
        find(&menus, "Edit", "Undo").shortcut,
        Some(shortcuts::Action::Undo)
    );
}

#[test]
fn items_follow_the_editor_state() {
    // With no project open, little can be done.
    let mut app = EditorApp::new(None);
    let menus = app.native_menus();
    assert!(!find(&menus, "File", "Save").enabled);
    assert!(find(&menus, "File", "Open Project…").enabled);
    assert!(!find(&menus, "Help", "Learn").enabled);

    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let menus = app.native_menus();
    assert!(find(&menus, "File", "Save").enabled);
    assert!(!find(&menus, "Edit", "Undo").enabled, "nothing to undo yet");
    let grid = find(&menus, "Viewport", "Grid");
    assert_eq!(grid.checked, Some(app.grid));
    let explorer = find(&menus, "View", Pane::Explorer.label());
    assert_eq!(explorer.checked, Some(app.layout.is_open(Pane::Explorer)));

    // A menu bar click runs what the header's item runs.
    let ctx = theme::test_context();
    let grid_was = app.grid;
    app.run_menu_action(&ctx, grid.action);
    assert_eq!(app.grid, !grid_was);
    assert_eq!(
        find(&app.native_menus(), "Viewport", "Grid").checked,
        Some(!grid_was)
    );

    // While a project loads, nothing but About and Quit.
    app.start_loading(&ctx, &example("hello-part"), None);
    let menus = app.native_menus();
    let on: Vec<String> = menus
        .iter()
        .flat_map(|menu| all_items(&menu.items))
        .filter(|item| item.enabled)
        .map(|item| item.label)
        .collect();
    assert_eq!(on, ["About Axcenity", format!("Quit {APP_NAME}").as_str()]);
}

#[test]
fn every_command_item_is_on_when_the_palette_says_it_can_run() {
    let ctx = theme::test_context();
    for folder in [None, Some(example("hello-touched"))] {
        let mut app = EditorApp::new(folder.as_deref());
        let menus = app.native_menus();
        let commands = app.palette_commands(&ctx);
        for item in menus.iter().flat_map(|menu| all_items(&menu.items)) {
            let MenuAction::Command(command) = &item.action else {
                continue;
            };
            let entry = commands
                .iter()
                .find(|entry| entry.command == *command)
                .unwrap_or_else(|| {
                    panic!("`{}` ({command:?}) can run from the palette", item.label)
                });
            assert_eq!(
                item.enabled, entry.entry.enabled,
                "`{}` on in the menu bar when it is in the palette",
                item.label
            );
        }
    }
}

#[test]
fn build_game_has_the_loading_screen_and_its_time() {
    // A copy: choosing writes the project's manifest.
    let folder = copy_of_example("hello-touched", "native-menu-loading");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let splash = find(&app.native_menus(), "File", "Axcenity splash");
    assert!(splash.checked.is_some());

    // A ScreenGui as the loading screen offers how long it shows.
    let workspace = app.world.workspace();
    let gui = app.world.create(ClassName::ScreenGui).unwrap();
    app.world.set_name(gui, "Intro").unwrap();
    app.world.set_parent(gui, Some(workspace)).unwrap();
    let intro = find(&app.native_menus(), "File", "Intro");
    app.run_menu_action(&ctx, intro.action);
    let menus = app.native_menus();
    assert_eq!(find(&menus, "File", "Intro").checked, Some(true));
    let five = find(&menus, "File", "5 s");
    app.run_menu_action(&ctx, five.action);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.loading_screen.screen_gui, "Intro");
    assert_eq!(project.loading_screen.min_seconds, 5.0);
    assert_eq!(find(&app.native_menus(), "File", "5 s").checked, Some(true));
}
