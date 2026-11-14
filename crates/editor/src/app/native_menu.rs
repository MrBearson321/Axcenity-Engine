//! The menus as data, for the system menu bar on macOS: the same items as
//! the header's menus (`chrome/menus.rs`), laid out the Mac way (About,
//! Settings and Quit under the app's own menu; a Window menu; Enter Full
//! Screen under View). [`EditorApp::native_menus`] builds them each frame
//! from the editor's state; `native_menu/macos.rs` turns them into the
//! real menu bar and hands clicks back as [`MenuAction`]s, which
//! [`EditorApp::run_menu_action`] runs. Windows and Linux keep the menus
//! drawn in the header and never use this.
//!
//! The model is plain data so the tests check it on every OS: each item
//! runs the command its header twin runs, and is on or off when that one is.

// Off macOS only the tests build the menus.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::layout::Preset;
use crate::shortcuts;

use super::chrome::HISTORY_ROWS;
use super::command_palette::{Command, LoadingChoice, Overlay};
use super::*;

#[cfg(target_os = "macos")]
mod macos;

/// The app's name in the menu bar, as on the Hide and Quit items.
pub(super) const APP_NAME: &str = "Axcenity Editor";

/// The loading screen's "Show for at least" choices, in seconds.
const LOADING_SECONDS: [f32; 6] = [0.0, 1.0, 2.0, 3.0, 5.0, 10.0];

/// What a menu item does when it's chosen.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MenuAction {
    /// What the same command does from the header or the palette.
    Command(Command),
    /// Help > Version Explorer.
    VersionExplorer,
    /// Edit > Undo History: steps to undo (negative) or redo (positive).
    HistoryJump(isize),
    /// File > Build Game > Show for at Least.
    LoadingSeconds(f32),
    /// View > Layout > Delete Saved Layout.
    DeleteLayout(String),
}

/// One of macOS's own items, which the system carries out itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SystemItem {
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    /// Enter Full Screen, or Exit Full Screen while there.
    FullScreen,
    BringAllToFront,
}

/// A clickable item.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MenuItem {
    pub label: String,
    pub action: MenuAction,
    pub enabled: bool,
    /// `Some`: a check item, ticked when true.
    pub checked: Option<bool>,
    /// Its keys from the shortcut table, shown at its right.
    pub shortcut: Option<shortcuts::Action>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum MenuNode {
    Item(MenuItem),
    Submenu {
        label: String,
        items: Vec<MenuNode>,
    },
    /// Greyed text that names the items under it.
    Heading(String),
    System(SystemItem),
    Separator,
}

/// What a top-level menu is to macOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuRole {
    /// The menu named after the app: About, Settings, Hide, Quit.
    App,
    Plain,
    /// The Window menu, where macOS lists the open windows.
    Window,
    /// The Help menu, which macOS gives a search field.
    Help,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TopMenu {
    pub title: &'static str,
    pub role: MenuRole,
    pub items: Vec<MenuNode>,
}

/// What the menu bar needs between frames.
#[derive(Default)]
pub(super) struct NativeMenu {
    /// The Workspace's ScreenGuis, for the loading screen choices, and the
    /// Workspace's structure version they were found at.
    screen_guis: (Option<u64>, Vec<InstanceId>),
    /// Items clicked since the last frame, run at the start of the next.
    clicked: Vec<MenuAction>,
    #[cfg(target_os = "macos")]
    bar: Option<macos::Bar>,
}

/// Builds the items of one menu.
#[derive(Default)]
struct Items(Vec<MenuNode>);

impl Items {
    fn item(&mut self, label: impl Into<String>, action: MenuAction, enabled: bool) -> &mut Self {
        self.0.push(MenuNode::Item(MenuItem {
            label: label.into(),
            action,
            enabled,
            checked: None,
            shortcut: None,
        }));
        self
    }

    fn command(&mut self, label: impl Into<String>, command: Command, enabled: bool) -> &mut Self {
        self.item(label, MenuAction::Command(command), enabled)
    }

    fn check(
        &mut self,
        label: impl Into<String>,
        action: MenuAction,
        enabled: bool,
        checked: bool,
    ) -> &mut Self {
        self.item(label, action, enabled);
        self.last().checked = Some(checked);
        self
    }

    /// Gives the item just added the keys of `action` from the shortcut table.
    fn keys(&mut self, action: shortcuts::Action) -> &mut Self {
        self.last().shortcut = Some(action);
        self
    }

    fn last(&mut self) -> &mut MenuItem {
        match self.0.last_mut() {
            Some(MenuNode::Item(item)) => item,
            _ => unreachable!("keys and checks follow an item"),
        }
    }

    fn submenu(&mut self, label: impl Into<String>, items: Items) -> &mut Self {
        self.0.push(MenuNode::Submenu {
            label: label.into(),
            items: items.0,
        });
        self
    }

    fn heading(&mut self, text: impl Into<String>) -> &mut Self {
        self.0.push(MenuNode::Heading(text.into()));
        self
    }

    fn system(&mut self, item: SystemItem) -> &mut Self {
        self.0.push(MenuNode::System(item));
        self
    }

    fn separator(&mut self) -> &mut Self {
        self.0.push(MenuNode::Separator);
        self
    }
}

fn top(title: &'static str, role: MenuRole, items: Items) -> TopMenu {
    TopMenu {
        title,
        role,
        items: items.0,
    }
}

impl EditorApp {
    /// Every menu of the system menu bar, from the editor's state now.
    pub(super) fn native_menus(&mut self) -> Vec<TopMenu> {
        let screen_guis = self.screen_gui_names();
        let project = self.project.is_some();
        let playing = self.playing();
        let editing = project && !playing;
        // The menus act on the open project, so not while another loads.
        let ready = self.loading.is_none();
        let when = |on: bool| on && ready;

        let mut app = Items::default();
        app.command("About Axcenity", Command::About, true)
            .command("Check for Updates…", Command::CheckForUpdates, ready)
            .separator()
            .command("Settings…", Command::Settings, ready)
            .keys(shortcuts::Action::Settings)
            .separator()
            .system(SystemItem::Services)
            .separator()
            .system(SystemItem::Hide)
            .system(SystemItem::HideOthers)
            .system(SystemItem::ShowAll)
            .separator()
            // Asks about unsaved changes, like the window's close button.
            .command(format!("Quit {APP_NAME}"), Command::Quit, true);

        let mut file = Items::default();
        file.command("New Project…", Command::NewProject, ready)
            .command("Open Project…", Command::OpenProject, ready)
            .separator()
            .command("Save", Command::Save, when(project))
            .keys(shortcuts::Action::Save)
            .command("Version History…", Command::VersionHistory, when(editing))
            .command("Save Checkpoint…", Command::SaveCheckpoint, when(editing))
            .command("Back Up Project", Command::BackUpProject, when(editing))
            .command("Save as Template…", Command::SaveAsTemplate, when(editing))
            .separator()
            .command("Input…", Command::Input, when(project))
            .command("Close Project", Command::CloseProject, when(editing))
            .separator()
            .command("Import…", Command::Import, when(editing))
            .separator();
        let solo = self.team.is_none();
        file.command(
            "Start Collaborate Session…",
            Command::ShareTeamCreate,
            when(editing && solo),
        )
        .command(
            "Join Collaborate Session…",
            Command::JoinTeamCreate,
            when(editing && solo),
        )
        .command("Set Up Collaborate…", Command::SetUpCollaborate, ready)
        .separator()
        .submenu(
            "Build Game",
            self.build_game_items(when(editing), screen_guis),
        )
        .separator()
        .command("Open Stylesheet…", Command::OpenStylesheet, when(editing))
        .command("New Stylesheet", Command::NewStylesheet, when(editing))
        .command("Check Assets", Command::CheckAssets, when(project))
        .command(
            "Set Up VS Code Autocomplete",
            Command::SetUpAutocomplete,
            when(project),
        );

        let mut edit = Items::default();
        // Nothing can be edited mid-drag, or while the game plays.
        let busy = self.viewport.is_dragging() || playing;
        // "Undo Move Pad", so it's clear what will be undone.
        let undo = self.history.undo_label();
        let redo = self.history.redo_label();
        edit.command(
            undo.map_or("Undo".to_owned(), |label| format!("Undo {label}")),
            Command::Undo,
            when(undo.is_some() && !busy),
        )
        .keys(shortcuts::Action::Undo)
        .command(
            redo.map_or("Redo".to_owned(), |label| format!("Redo {label}")),
            Command::Redo,
            when(redo.is_some() && !busy),
        )
        .keys(shortcuts::Action::Redo)
        .submenu("Undo History", self.undo_history_items(when(!busy)))
        .separator();
        // These act on the selection, the same as its right-click menu in the Explorer.
        let selected = self.selected;
        let renamable = selected
            .and_then(|id| self.world.get(id))
            .is_some_and(properties::can_rename)
            && !playing;
        let editable =
            selected.is_some_and(|id| tree::check_editable(&self.world, id).is_ok()) && !busy;
        let deletable =
            selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
        edit.command("Rename", Command::Rename, when(renamable))
            .keys(shortcuts::Action::Rename)
            .command("Duplicate", Command::Duplicate, when(editable))
            .keys(shortcuts::Action::Duplicate)
            .command("Delete", Command::Delete, when(deletable))
            .keys(shortcuts::Action::Delete)
            .separator()
            .command("Search Everything…", Command::Search, ready)
            .keys(shortcuts::Action::Search)
            .command("Command Palette", Command::Palette, ready)
            .keys(shortcuts::Action::Palette)
            .command("Quick Open…", Command::QuickOpen, when(project))
            .keys(shortcuts::Action::QuickOpen)
            .command("Find in Scripts…", Command::FindInScripts, when(project))
            .keys(shortcuts::Action::FindInScripts);

        let mut view = Items::default();
        view.heading("Panels");
        for pane in Pane::CLOSABLE {
            view.check(
                pane.label(),
                MenuAction::Command(Command::TogglePane(pane)),
                ready,
                self.layout.is_open(pane),
            );
        }
        view.command("Reset Layout", Command::ResetLayout, ready)
            .submenu("Layout", self.layout_items(ready))
            .separator()
            .command("New Ribbon Tab…", Command::NewRibbonTab, ready)
            .separator()
            .system(SystemItem::FullScreen);

        let mut viewport = Items::default();
        viewport.heading("Guides");
        for (label, command, on) in [
            ("Grid", Command::Grid, self.grid),
            ("View Cube", Command::ViewCube, self.view_cube),
            ("Ray Tracing Badge", Command::RayBadge, self.ray_badge),
            ("Light Cones", Command::LightCones, self.light_cones),
            ("Movement Guide", Command::MovementGuide, self.move_guide),
            (
                "Stream While Editing",
                Command::StreamEditing,
                self.overlays.stream_editing,
            ),
        ] {
            viewport.check(label, MenuAction::Command(command), ready, on);
        }
        viewport.separator();
        let mut modes = Items::default();
        for mode in ViewMode::all() {
            modes.check(
                mode.label(),
                MenuAction::Command(Command::ViewMode(mode)),
                ready,
                self.view_mode == mode,
            );
        }
        viewport.submenu("View Mode", modes).heading("Overlays");
        for overlay in Overlay::ALL {
            let on = *overlay.flag(&mut self.overlays);
            viewport.check(
                overlay.label(),
                MenuAction::Command(Command::Overlay(overlay)),
                ready,
                on,
            );
        }
        viewport
            .separator()
            .command("Viewport Settings…", Command::ViewportSettings, ready);

        let mut debug = Items::default();
        debug.command("Graphics Benchmark…", Command::GraphicsBenchmark, ready);
        debug.command("Animator Inspector…", Command::AnimatorInspector, ready);
        // Only in debug builds (`cargo run`), never in a release.
        if cfg!(debug_assertions) {
            debug
                .separator()
                .command("Panic during Play", Command::PanicDuringPlay, when(playing));
        }

        let mut window = Items::default();
        window
            .system(SystemItem::Minimize)
            .system(SystemItem::Zoom)
            .separator()
            .system(SystemItem::BringAllToFront);

        let mut help = Items::default();
        help.check(
            "Learn",
            MenuAction::Command(Command::Learn),
            when(project),
            self.learn_open,
        )
        .item("Version Explorer", MenuAction::VersionExplorer, ready);

        vec![
            top(APP_NAME, MenuRole::App, app),
            top("File", MenuRole::Plain, file),
            top("Edit", MenuRole::Plain, edit),
            top("View", MenuRole::Plain, view),
            top("Viewport", MenuRole::Plain, viewport),
            top("Debug", MenuRole::Plain, debug),
            top("Window", MenuRole::Window, window),
            top("Help", MenuRole::Help, help),
        ]
    }

    /// File > Build Game: for this OS or another, the loading screen, and
    /// the Developer Console switch.
    fn build_game_items(&self, enabled: bool, screen_guis: Vec<String>) -> Items {
        let mut items = Items::default();
        items
            .command("For this OS", Command::BuildGame(None), enabled)
            .separator();
        for (target, label) in [
            (files::ExportTarget::Windows, "For Windows"),
            (files::ExportTarget::MacOS, "For macOS"),
            (files::ExportTarget::Linux, "For Linux"),
        ] {
            items.command(label, Command::BuildGame(Some(target)), enabled);
        }
        items.separator();
        let Some(project) = &self.project else {
            items.heading("Loading screen: open a project first.");
            return items;
        };
        let ready = self.loading.is_none();
        // Build profiles (R13), the picked one ticked.
        let profiles = axcenity_project::BuildProfiles::read(&project.folder).unwrap_or_default();
        let picked = files::picked_profile(&project.folder);
        items.heading("Build Profile");
        for name in profiles.names() {
            let on = name == picked;
            items.check(
                name.clone(),
                MenuAction::Command(Command::BuildProfile(name)),
                ready,
                on,
            );
        }
        items.separator();
        let loading = &project.loading_screen;
        let current = match loading.screen_gui.as_str() {
            "" if project.show_splash => LoadingChoice::Splash,
            "" => LoadingChoice::Nothing,
            name => LoadingChoice::Gui(name.to_owned()),
        };
        let mut choices = vec![
            (LoadingChoice::Splash, "Axcenity splash".to_owned()),
            (LoadingChoice::Nothing, "None".to_owned()),
        ];
        choices.extend(
            screen_guis
                .into_iter()
                .map(|name| (LoadingChoice::Gui(name.clone()), name)),
        );
        // A saved name with no ScreenGui left: still shown, so it can be changed.
        if let LoadingChoice::Gui(name) = &current
            && !choices.iter().any(|(choice, _)| choice == &current)
        {
            choices.push((current.clone(), format!("{name} (missing)")));
        }
        items.heading("Loading Screen");
        for (choice, label) in choices {
            let on = choice == current;
            items.check(
                label,
                MenuAction::Command(Command::LoadingScreen(choice)),
                ready,
                on,
            );
        }
        if !loading.screen_gui.is_empty() {
            let mut seconds = Items::default();
            let mut shown = false;
            for value in LOADING_SECONDS {
                let on = (loading.min_seconds - value).abs() < 0.05;
                shown |= on;
                seconds.check(
                    seconds_label(value),
                    MenuAction::LoadingSeconds(value),
                    ready,
                    on,
                );
            }
            // A time set elsewhere stays visible, ticked.
            if !shown {
                seconds.check(
                    seconds_label(loading.min_seconds),
                    MenuAction::LoadingSeconds(loading.min_seconds),
                    ready,
                    true,
                );
            }
            items.submenu("Show for at Least", seconds);
        }
        if current != LoadingChoice::Nothing {
            items.check(
                "Wait for a script",
                MenuAction::Command(Command::WaitForScript),
                ready,
                loading.wait_for_script,
            );
        }
        items.separator().check(
            "Developer Console (F9)",
            MenuAction::Command(Command::DeveloperConsole),
            ready,
            project.developer_console,
        );
        items.check(
            "Ray Traced Look by Default",
            MenuAction::Command(Command::RayTracedByDefault),
            ready,
            project.ray_traced_by_default,
        );
        items
    }

    /// Edit > Undo History: the steps that can be redone (the furthest at
    /// the top), then those that can be undone, newest first.
    fn undo_history_items(&self, enabled: bool) -> Items {
        let mut items = Items::default();
        let undo = self.history.undo_labels();
        let redo = self.history.redo_labels();
        if undo.is_empty() && redo.is_empty() {
            items.heading("Nothing to undo yet.");
            return items;
        }
        for (index, label) in redo.iter().enumerate().take(HISTORY_ROWS).rev() {
            items.item(
                format!("Redo {label}"),
                MenuAction::HistoryJump(index as isize + 1),
                enabled,
            );
        }
        if !redo.is_empty() && !undo.is_empty() {
            items.separator();
        }
        for (index, label) in undo.iter().enumerate().take(HISTORY_ROWS) {
            items.item(
                *label,
                MenuAction::HistoryJump(-(index as isize + 1)),
                enabled,
            );
        }
        if undo.len() > HISTORY_ROWS {
            items.heading(format!("and {} older", undo.len() - HISTORY_ROWS));
        }
        items
    }

    /// View > Layout: the presets, the saved layouts (and a submenu to
    /// delete one), then Save Current Layout….
    fn layout_items(&self, enabled: bool) -> Items {
        let mut items = Items::default();
        for preset in Preset::ALL {
            items.command(preset.label(), Command::LayoutPreset(preset), enabled);
        }
        let saved = &self.saved_layouts.0;
        if !saved.is_empty() {
            items.separator();
            let mut delete = Items::default();
            for (name, _) in saved {
                items.command(name.as_str(), Command::SavedLayout(name.clone()), enabled);
                delete.item(
                    name.as_str(),
                    MenuAction::DeleteLayout(name.clone()),
                    enabled,
                );
            }
            items.submenu("Delete Saved Layout", delete);
        }
        items
            .separator()
            .command("Save Current Layout…", Command::SaveLayout, enabled);
        items
    }

    /// The names of the Workspace's ScreenGuis. Which Instances those are
    /// is looked for again only when the Workspace's tree has changed.
    fn screen_gui_names(&mut self) -> Vec<String> {
        let workspace = self.world.workspace();
        let version = self.world.structure_version_of(workspace);
        if self.native_menu.screen_guis.0 != Some(version) {
            let guis = self
                .world
                .descendants_iter(workspace)
                .filter(|&id| {
                    self.world
                        .get(id)
                        .is_some_and(|gui| gui.class() == ClassName::ScreenGui)
                })
                .collect();
            self.native_menu.screen_guis = (Some(version), guis);
        }
        let mut names: Vec<String> = Vec::new();
        for &id in &self.native_menu.screen_guis.1 {
            if let Some(gui) = self.world.get(id)
                && !names.iter().any(|name| name == gui.name())
            {
                names.push(gui.name().to_owned());
            }
        }
        names
    }

    /// Does what a menu bar item asked for.
    pub(super) fn run_menu_action(&mut self, ctx: &egui::Context, action: MenuAction) {
        match action {
            MenuAction::Command(Command::FindInScripts) => {
                // As the header's item does: show the panel and search.
                self.find.show_and_focus();
                self.search_scripts();
            }
            MenuAction::Command(command) => self.run_command(ctx, command),
            MenuAction::VersionExplorer => self.layout.open(Pane::VersionExplorer),
            MenuAction::HistoryJump(jump) => self.jump_in_history(jump),
            MenuAction::LoadingSeconds(seconds) => {
                if let Some(project) = &self.project {
                    let mut loading = project.loading_screen.clone();
                    loading.min_seconds = seconds;
                    self.set_loading_screen(loading);
                }
            }
            MenuAction::DeleteLayout(name) => self.saved_layouts.delete(&name),
        }
    }

    /// Keeps the system menu bar in step with the editor (macOS only).
    /// Called once a frame from `eframe::App::ui`, after `frame` drew it.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub(super) fn sync_native_menu(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        {
            let menus = self.native_menus();
            let bar = self
                .native_menu
                .bar
                .get_or_insert_with(|| macos::Bar::new(ctx));
            bar.sync(&menus);
        }
    }

    /// Before egui reads this frame's input: hands it the keys of menu
    /// items chosen by their shortcut, so they go wherever those keys go
    /// now (a text field's own undo, say), and keeps the clicked ones for
    /// [`Self::run_native_menu_clicks`].
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub(super) fn take_native_menu_input(&mut self, raw_input: &mut egui::RawInput) {
        #[cfg(target_os = "macos")]
        if let Some(bar) = &mut self.native_menu.bar {
            let clicked = bar.take_chosen(raw_input);
            self.native_menu.clicked.extend(clicked);
        }
    }

    /// Runs the menu bar items clicked since the last frame.
    pub(super) fn run_native_menu_clicks(&mut self, ctx: &egui::Context) {
        for action in std::mem::take(&mut self.native_menu.clicked) {
            self.run_menu_action(ctx, action);
        }
    }
}

fn seconds_label(seconds: f32) -> String {
    if seconds == 0.0 {
        "No Minimum".to_owned()
    } else if seconds.fract() == 0.0 {
        format!("{seconds:.0} s")
    } else {
        format!("{seconds:.1} s")
    }
}

#[cfg(test)]
mod tests;
