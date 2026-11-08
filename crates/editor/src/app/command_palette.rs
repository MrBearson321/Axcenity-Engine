//! The command palette's commands and Quick Open's entries (unified
//! Phase 70). Every menu item and ribbon button is a [`Command`] here,
//! listed by [`EditorApp::palette_commands`] and run by
//! [`EditorApp::run_command`] the way its menu or button runs it. The list
//! window is `command_palette.rs`.

use crate::command_palette::{Entry, Mode};
use crate::find_in_scripts::FindAction;
use crate::ribbon_tabs::{ActiveTab, BuiltInTab};
use crate::terrain_studio::TerrainTool;
use crate::theme::ThemeChoice;

use super::files::ExportTarget;
use super::settings;
use super::solid::SolidAction;
use super::*;
use axcenity_project as project;

/// Ctrl+Shift+P (Cmd+Shift+P on macOS) opens the command palette.
pub(super) const PALETTE_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::P,
);
/// Ctrl+P opens Quick Open.
pub(super) const QUICK_OPEN_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::P);
/// Ctrl+Shift+F opens Find in Scripts.
pub(super) const FIND_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::F,
);
/// Where the recent commands are saved between runs.
pub(super) const RECENT_COMMANDS_KEY: &str = "recent commands";

/// One viewport overlay of View ▸ Overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Overlay {
    Colliders,
    Raycasts,
    Bounds,
    Emitters,
    NavMesh,
    AgentPaths,
    Constraints,
    Culling,
}

impl Overlay {
    const ALL: [Overlay; 8] = [
        Overlay::Colliders,
        Overlay::Raycasts,
        Overlay::Bounds,
        Overlay::Emitters,
        Overlay::NavMesh,
        Overlay::AgentPaths,
        Overlay::Constraints,
        Overlay::Culling,
    ];

    fn label(self) -> &'static str {
        match self {
            Overlay::Colliders => "Colliders",
            Overlay::Raycasts => "Raycasts",
            Overlay::Bounds => "Bounds",
            Overlay::Emitters => "Emitters",
            Overlay::NavMesh => "Nav Mesh",
            Overlay::AgentPaths => "Agent Paths",
            Overlay::Constraints => "Constraints",
            Overlay::Culling => "Culling",
        }
    }

    fn flag(self, overlays: &mut viewport::OverlayToggles) -> &mut bool {
        match self {
            Overlay::Colliders => &mut overlays.colliders,
            Overlay::Raycasts => &mut overlays.raycasts,
            Overlay::Bounds => &mut overlays.bounds,
            Overlay::Emitters => &mut overlays.emitters,
            Overlay::NavMesh => &mut overlays.nav_mesh,
            Overlay::AgentPaths => &mut overlays.agent_paths,
            Overlay::Constraints => &mut overlays.constraints,
            Overlay::Culling => &mut overlays.culling,
        }
    }
}

/// One of the Snapping group's switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapSwitch {
    Move,
    Rotate,
    Surfaces,
    Collisions,
    Join,
    Points,
}

impl SnapSwitch {
    const ALL: [SnapSwitch; 6] = [
        SnapSwitch::Move,
        SnapSwitch::Rotate,
        SnapSwitch::Surfaces,
        SnapSwitch::Collisions,
        SnapSwitch::Join,
        SnapSwitch::Points,
    ];

    fn label(self) -> &'static str {
        match self {
            SnapSwitch::Move => "Move",
            SnapSwitch::Rotate => "Rotate",
            SnapSwitch::Surfaces => "Surfaces",
            SnapSwitch::Collisions => "Collisions",
            SnapSwitch::Join => "Join Surfaces",
            SnapSwitch::Points => "Snap Points",
        }
    }

    fn flag(self, snap: &mut Snap) -> &mut bool {
        match self {
            SnapSwitch::Move => &mut snap.move_on,
            SnapSwitch::Rotate => &mut snap.rotate_on,
            SnapSwitch::Surfaces => &mut snap.surfaces,
            SnapSwitch::Collisions => &mut snap.collisions,
            SnapSwitch::Join => &mut snap.join,
            SnapSwitch::Points => &mut snap.points,
        }
    }
}

/// File ▸ Build Game's loading screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LoadingChoice {
    Splash,
    Nothing,
    Gui(String),
}

/// Something a menu item or ribbon button does.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Command {
    NewProject,
    OpenProject,
    Save,
    VersionHistory,
    SaveCheckpoint,
    BackUpProject,
    Input,
    CloseProject,
    Import,
    /// `None`: for this OS.
    BuildGame(Option<ExportTarget>),
    LoadingScreen(LoadingChoice),
    WaitForScript,
    /// File ▸ Build Game's Developer Console switch (Phase 71).
    DeveloperConsole,
    OpenStylesheet,
    NewStylesheet,
    CheckAssets,
    SetUpAutocomplete,
    Settings,
    Quit,
    Undo,
    Redo,
    Rename,
    Duplicate,
    Delete,
    Theme(ThemeChoice),
    Appearance,
    TogglePane(Pane),
    ResetLayout,
    LightCones,
    ViewCube,
    Grid,
    ViewMode(ViewMode),
    Overlay(Overlay),
    NewRibbonTab,
    RibbonTab(BuiltInTab),
    Learn,
    About,
    PanicDuringPlay,
    /// Play, or Continue at a breakpoint.
    Play,
    Stop,
    Tool(Tool),
    Snap(SnapSwitch),
    CollisionGroups,
    Insert(Insertable),
    BakeProbes,
    Solid(SolidAction),
    Weld,
    /// `None` stops the Constraint tool.
    Constraint(Option<ClassName>),
    InsertScript(ClassName),
    InsertControls,
    TerrainStudio,
    TerrainTool(TerrainTool),
    /// A button on a plugin's toolbar.
    PluginButton {
        plugin: usize,
        button: u32,
    },
    /// The command palette itself.
    Palette,
    QuickOpen,
    FindInScripts,
}

/// A command and its row in the palette.
pub(super) struct CommandEntry {
    pub command: Command,
    pub entry: Entry,
}

/// What a Quick Open row opens.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum OpenTarget {
    Script(String),
    Asset(String),
    /// `None`: the start level.
    Level(Option<String>),
    Instance(InstanceId),
}

/// The insertables the ribbon's buttons and dropdowns offer, by group.
fn ribbon_insertables() -> Vec<(&'static str, Vec<Insertable>)> {
    let all = |lists: &[&[Insertable]]| lists.concat();
    vec![
        ("Parts", Insertable::PARTS.to_vec()),
        (
            "Effects",
            all(&[
                &Insertable::AUDIO,
                &[Insertable::ParticleEmitter],
                &Insertable::LENS,
                &Insertable::VOLUMES,
            ]),
        ),
        (
            "Lights & Sky",
            all(&[&Insertable::LIGHTS, &Insertable::PROBES, &Insertable::SKY]),
        ),
        (
            "Gameplay",
            all(&[
                &Insertable::HUMANOIDS,
                &Insertable::CHARACTERS,
                &Insertable::HEALTH,
                &Insertable::TOOLS,
                &Insertable::AGENTS,
                &Insertable::SPLINES,
                &Insertable::INTERACTION,
                &Insertable::SEATS,
                &Insertable::SPAWNING,
                &Insertable::CHECKPOINTS,
                &Insertable::CAMERAS,
                &Insertable::VEHICLES,
            ]),
        ),
        ("Constraints", Insertable::CONSTRAINTS.to_vec()),
        (
            "UI",
            vec![
                Insertable::ScreenGui,
                Insertable::Frame,
                Insertable::ScrollingFrame,
                Insertable::Table,
                Insertable::PageLayout,
                Insertable::Page,
                Insertable::BillboardGui,
                Insertable::TextLabel,
                Insertable::TextButton,
                Insertable::ImageLabel,
                Insertable::TextBox,
                Insertable::Slider,
                Insertable::Toggle,
                Insertable::Dropdown,
                Insertable::ProgressBar,
                Insertable::HealthBar,
            ],
        ),
    ]
}

impl EditorApp {
    /// Every command, as the palette lists it: File, Edit, View, Help (and
    /// Debug in debug builds), then the ribbon's groups, then plugins'
    /// toolbar buttons. Each row's aliases are what its menu item or button
    /// says, so searching that finds it.
    pub(super) fn palette_commands(&self, ctx: &egui::Context) -> Vec<CommandEntry> {
        let mut list = Vec::new();
        let project = self.project.is_some();
        let playing = self.playing();
        let editing = project && !playing;
        let not_editing = if playing {
            "Press Stop to edit."
        } else {
            "Open a project first."
        };
        let shortcut = |shortcut: &egui::KeyboardShortcut| Some(ctx.format_shortcut(shortcut));
        let mut add = |command: Command,
                       title: &str,
                       detail: &str,
                       aliases: &[&str],
                       shortcut: Option<String>,
                       enabled: Result<(), &str>| {
            let key = match &command {
                Command::PluginButton { .. } => format!("plugin {detail} {title}"),
                other => format!("{other:?}"),
            };
            let mut entry = Entry::new(key, title, detail);
            entry.aliases = aliases.iter().map(|alias| (*alias).to_owned()).collect();
            entry.shortcut = shortcut;
            if let Err(why) = enabled {
                entry.enabled = false;
                entry.disabled = why.to_owned();
            }
            list.push(CommandEntry { command, entry });
        };
        let when = |on: bool, why: &'static str| if on { Ok(()) } else { Err(why) };

        // File.
        add(
            Command::NewProject,
            "New Project…",
            "File",
            &[],
            None,
            Ok(()),
        );
        add(
            Command::OpenProject,
            "Open Project…",
            "File",
            &[],
            None,
            Ok(()),
        );
        add(
            Command::Save,
            "Save",
            "File",
            &[],
            shortcut(&SAVE_SHORTCUT),
            when(project, "Open a project first."),
        );
        add(
            Command::VersionHistory,
            "Version History…",
            "File",
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::SaveCheckpoint,
            "Save Checkpoint…",
            "File",
            &["version", "pin", "history"],
            None,
            when(editing, not_editing),
        );
        add(
            Command::BackUpProject,
            "Back Up Project",
            "File",
            &["zip", "backup", "archive"],
            None,
            when(editing, not_editing),
        );
        add(
            Command::Input,
            "Input…",
            "File",
            &[],
            None,
            when(project, "Open a project first."),
        );
        add(
            Command::CloseProject,
            "Close Project",
            "File",
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::Import,
            "Import…",
            "File",
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::BuildGame(None),
            "Build Game for This OS",
            "File › Build Game",
            &["For this OS"],
            None,
            when(editing, not_editing),
        );
        for (target, label) in [
            (ExportTarget::Windows, "For Windows"),
            (ExportTarget::MacOS, "For macOS"),
            (ExportTarget::Linux, "For Linux"),
        ] {
            add(
                Command::BuildGame(Some(target)),
                &format!("Build Game {}", label.replacen("For", "for", 1)),
                "File › Build Game",
                &[label],
                None,
                when(editing, not_editing),
            );
        }
        if let Some(open) = &self.project {
            let mut choices = vec![
                (LoadingChoice::Splash, "Axcenity splash".to_owned()),
                (LoadingChoice::Nothing, "None".to_owned()),
            ];
            let workspace = self.world.workspace();
            for id in self.world.descendants(workspace) {
                if let Some(gui) = self.world.get(id)
                    && gui.class() == ClassName::ScreenGui
                    && !choices
                        .iter()
                        .any(|(choice, _)| *choice == LoadingChoice::Gui(gui.name().to_owned()))
                {
                    choices.push((
                        LoadingChoice::Gui(gui.name().to_owned()),
                        gui.name().to_owned(),
                    ));
                }
            }
            let saved = &open.loading_screen.screen_gui;
            if !saved.is_empty() && !choices.iter().any(|(_, name)| name == saved) {
                choices.push((
                    LoadingChoice::Gui(saved.clone()),
                    format!("{saved} (missing)"),
                ));
            }
            for (choice, label) in choices {
                add(
                    Command::LoadingScreen(choice),
                    &format!("Loading Screen: {label}"),
                    "File › Build Game",
                    &[&label],
                    None,
                    Ok(()),
                );
            }
            add(
                Command::WaitForScript,
                "Loading Screen: Wait for a Script",
                "File › Build Game",
                &["Wait for a script"],
                None,
                Ok(()),
            );
            add(
                Command::DeveloperConsole,
                "Developer Console in Built Games",
                "File › Build Game",
                &["Developer Console (F9)"],
                None,
                Ok(()),
            );
        }
        add(
            Command::OpenStylesheet,
            "Open Stylesheet…",
            "File",
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::NewStylesheet,
            "New Stylesheet",
            "File",
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::CheckAssets,
            "Check Assets",
            "File",
            &["Assets"],
            None,
            when(project, "Open a project first."),
        );
        add(
            Command::SetUpAutocomplete,
            "Set Up VS Code Autocomplete",
            "File",
            &[],
            None,
            when(project, "Open a project first."),
        );
        add(
            Command::Settings,
            "Settings…",
            "File",
            &["Settings"],
            shortcut(&settings::SETTINGS_SHORTCUT),
            Ok(()),
        );
        add(Command::Quit, "Quit", "File", &[], None, Ok(()));

        // Edit.
        let busy = self.viewport.is_dragging() || playing;
        let undo = self
            .history
            .undo_label()
            .map(|label| format!("Undo {label}"));
        let redo = self
            .history
            .redo_label()
            .map(|label| format!("Redo {label}"));
        add(
            Command::Undo,
            undo.as_deref().unwrap_or("Undo"),
            "Edit",
            &["Undo"],
            shortcut(&UNDO_SHORTCUT),
            when(undo.is_some() && !busy, "Nothing to undo."),
        );
        add(
            Command::Redo,
            redo.as_deref().unwrap_or("Redo"),
            "Edit",
            &["Redo"],
            shortcut(&REDO_SHORTCUT),
            when(redo.is_some() && !busy, "Nothing to redo."),
        );
        let selected = self.selected;
        let renamable = selected
            .and_then(|id| self.world.get(id))
            .is_some_and(properties::can_rename)
            && !playing;
        let editable =
            selected.is_some_and(|id| tree::check_editable(&self.world, id).is_ok()) && !busy;
        let deletable =
            selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
        add(
            Command::Rename,
            "Rename",
            "Edit",
            &[],
            Some(RENAME_KEY.name().to_owned()),
            when(renamable, "Select something to rename."),
        );
        add(
            Command::Duplicate,
            "Duplicate",
            "Edit",
            &[],
            shortcut(&DUPLICATE_SHORTCUT),
            when(editable, "Select something to duplicate."),
        );
        add(
            Command::Delete,
            "Delete",
            "Edit",
            &[],
            Some(DELETE_KEY_TEXT.to_owned()),
            when(deletable, "Select something to delete."),
        );
        add(
            Command::Palette,
            "Command Palette",
            "Edit",
            &[],
            shortcut(&PALETTE_SHORTCUT),
            Ok(()),
        );
        add(
            Command::QuickOpen,
            "Quick Open…",
            "Edit",
            &["Go to file"],
            shortcut(&QUICK_OPEN_SHORTCUT),
            when(project, "Open a project first."),
        );
        add(
            Command::FindInScripts,
            "Find in Scripts…",
            "Edit",
            &["Search", "Replace"],
            shortcut(&FIND_SHORTCUT),
            when(project, "Open a project first."),
        );

        // View.
        for choice in ThemeChoice::ALL {
            let label = match choice {
                ThemeChoice::System => "Match System",
                other => other.label(),
            };
            add(
                Command::Theme(choice),
                &format!("Theme: {label}"),
                "View",
                &[label],
                None,
                Ok(()),
            );
        }
        add(
            Command::Appearance,
            "Appearance…",
            "View",
            &[],
            None,
            Ok(()),
        );
        for pane in Pane::CLOSABLE {
            let shown = if self.layout.is_open(pane) {
                "Hide"
            } else {
                "Show"
            };
            add(
                Command::TogglePane(pane),
                &format!("{shown} {} Panel", pane.label()),
                "View › Panels",
                &[pane.label()],
                None,
                Ok(()),
            );
        }
        add(
            Command::ResetLayout,
            "Reset Layout",
            "View › Panels",
            &[],
            None,
            Ok(()),
        );
        add(
            Command::LightCones,
            "Light Cones",
            "View",
            &[],
            None,
            Ok(()),
        );
        add(Command::ViewCube, "View Cube", "View", &[], None, Ok(()));
        add(Command::Grid, "Grid", "View", &[], None, Ok(()));
        for mode in ViewMode::all() {
            add(
                Command::ViewMode(mode),
                &format!("View Mode: {}", mode.label()),
                "View",
                &[mode.label()],
                None,
                Ok(()),
            );
        }
        for overlay in Overlay::ALL {
            add(
                Command::Overlay(overlay),
                &format!("Overlay: {}", overlay.label()),
                "View › Overlays",
                &[overlay.label()],
                None,
                Ok(()),
            );
        }
        // The + after the ribbon's tabs is named by its tooltip.
        add(
            Command::NewRibbonTab,
            "New Ribbon Tab…",
            "View",
            &["Make your own tab from any of the ribbon's groups."],
            None,
            Ok(()),
        );
        for tab in BuiltInTab::ALL {
            add(
                Command::RibbonTab(tab),
                &format!("Ribbon Tab: {}", tab.label()),
                "View",
                &[tab.label()],
                None,
                Ok(()),
            );
        }

        // Help, and Debug in debug builds.
        add(
            Command::Learn,
            "Learn",
            "Help",
            &[],
            None,
            when(
                project,
                "Learn is on the start screen until a project is open.",
            ),
        );
        add(Command::About, "About Axcenity", "Help", &[], None, Ok(()));
        if cfg!(debug_assertions) {
            add(
                Command::PanicDuringPlay,
                "Panic during Play",
                "Debug",
                &[],
                None,
                when(playing, "Press Play first."),
            );
        }

        // The ribbon.
        let ribbon = |group: &str| format!("Ribbon › {group}");
        let (play, play_alias) = if self.paused() {
            ("Continue", "Continue")
        } else {
            ("Play", "Play")
        };
        add(
            Command::Play,
            play,
            &ribbon("Game"),
            &[play_alias],
            shortcut(&PLAY_SHORTCUT),
            when(
                (!playing || self.paused()) && project,
                "Open a project, and press Stop.",
            ),
        );
        add(
            Command::Stop,
            "Stop",
            &ribbon("Game"),
            &[],
            shortcut(&STOP_SHORTCUT),
            when(playing, "The game isn't playing."),
        );
        for tool in Tool::ALL {
            let group = if Tool::TRANSFORM.contains(&tool) {
                "Tools"
            } else {
                "Build"
            };
            let key = tool.key().map(|key| key.name().to_owned());
            add(
                Command::Tool(tool),
                &format!("{} Tool", tool.label()),
                &ribbon(group),
                &[tool.label()],
                key,
                Ok(()),
            );
        }
        for switch in SnapSwitch::ALL {
            add(
                Command::Snap(switch),
                &format!("Snap: {}", switch.label()),
                &ribbon("Snapping"),
                &[switch.label()],
                None,
                Ok(()),
            );
        }
        add(
            Command::CollisionGroups,
            "Collision Groups…",
            &ribbon("Snapping"),
            &[],
            None,
            Ok(()),
        );
        for (group, insertables) in ribbon_insertables() {
            for what in insertables {
                let mut aliases = vec![what.label()];
                match what {
                    Insertable::ParticleEmitter => aliases.push("Particles"),
                    Insertable::ScreenGui => aliases.push("Screen"),
                    _ => {}
                }
                add(
                    Command::Insert(what),
                    &format!("Insert {}", what.label()),
                    &ribbon(group),
                    &aliases,
                    None,
                    when(!playing, "Press Stop to edit."),
                );
            }
        }
        add(
            Command::BakeProbes,
            "Bake Probes",
            &ribbon("Lights & Sky"),
            &[],
            None,
            when(!playing, "Press Stop to edit."),
        );
        let parts = self.solid_parts();
        for action in SolidAction::ALL {
            add(
                Command::Solid(action),
                action.label(),
                &ribbon("Solid Modeling"),
                &[],
                None,
                when(
                    !playing && action.usable(&self.world, &parts),
                    if playing {
                        "Press Stop to edit."
                    } else {
                        action.needs()
                    },
                ),
            );
        }
        let can_weld = !crate::constraints::weld_pairs(&self.world, &self.tool_parts()).is_empty();
        add(
            Command::Weld,
            "Weld",
            &ribbon("Constraints"),
            &[],
            None,
            when(!playing && can_weld, "Select two or more parts to weld."),
        );
        for &class in crate::constraints::JOINTS
            .iter()
            .chain(crate::constraints::MOVERS.iter())
        {
            add(
                Command::Constraint(Some(class)),
                &format!("Constraint Tool: {class}"),
                &ribbon("Constraints"),
                &[class.as_str()],
                None,
                when(!playing, "Press Stop to edit."),
            );
        }
        add(
            Command::Constraint(None),
            "Stop the Constraint Tool",
            &ribbon("Constraints"),
            &["Stop"],
            None,
            when(
                self.constraint_tool.is_some(),
                "The Constraint tool isn't on.",
            ),
        );
        add(
            Command::Import,
            "Import…",
            &ribbon("Import"),
            &[],
            None,
            when(editing, not_editing),
        );
        add(
            Command::InsertControls,
            "Insert Controls Menu",
            &ribbon("UI"),
            &["Controls"],
            None,
            when(editing, "Open a project, and press Stop to edit."),
        );
        for class in explorer::SCRIPT_CLASSES {
            add(
                Command::InsertScript(class),
                &format!("Insert {class}"),
                &ribbon("Scripts"),
                &[class.as_str()],
                None,
                when(editing, not_editing),
            );
        }
        add(
            Command::TerrainStudio,
            "Terrain Studio",
            &ribbon("Terrain"),
            &["Terrain"],
            None,
            when(project, "Open a project first."),
        );
        for tool in [
            TerrainTool::Raise,
            TerrainTool::Paint,
            TerrainTool::Foliage,
            TerrainTool::Generate,
        ] {
            add(
                Command::TerrainTool(tool),
                &format!("Terrain Brush: {}", tool.label()),
                &ribbon("Terrain"),
                &[tool.label()],
                None,
                when(editing, not_editing),
            );
        }

        // Plugins' toolbar buttons.
        for (index, plugin) in self.plugins.entries.iter().enumerate() {
            let Some(running) = &plugin.running else {
                continue;
            };
            for toolbar in running.toolbars() {
                for button in &toolbar.buttons {
                    add(
                        Command::PluginButton {
                            plugin: index,
                            button: button.id,
                        },
                        &button.name,
                        &format!("Plugin › {}", toolbar.name),
                        &[],
                        None,
                        when(
                            button.enabled && !playing,
                            "Plugins only run while editing.",
                        ),
                    );
                }
            }
        }
        list
    }

    /// Does what `command`'s menu item or button does.
    pub(super) fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        let playing = self.playing();
        match command {
            Command::NewProject => self.request_and_go_ahead(ctx, Discard::NewProject),
            Command::OpenProject => self.request_and_go_ahead(ctx, Discard::OpenAnother),
            Command::Save => {
                if self.project.is_some() {
                    self.save();
                }
            }
            Command::VersionHistory => self.version_history.open = true,
            Command::SaveCheckpoint => self.version_history.open_for_checkpoint(),
            Command::BackUpProject => self.back_up_project(false),
            Command::Input => self.input_settings.open = true,
            Command::CloseProject => self.request_and_go_ahead(ctx, Discard::CloseProject),
            Command::Import => self.import_with_dialog(axcenity_assets::PROJECT_TEXTURES_FOLDER),
            Command::BuildGame(None) => self.export_game_with_dialog(),
            Command::BuildGame(Some(target)) => self.export_game_for_with_dialog(target),
            Command::LoadingScreen(choice) => {
                let Some(project) = &self.project else {
                    return;
                };
                let mut loading = project.loading_screen.clone();
                match choice {
                    LoadingChoice::Splash | LoadingChoice::Nothing => {
                        loading.screen_gui.clear();
                        self.set_loading_screen(loading);
                        self.set_show_splash(choice == LoadingChoice::Splash);
                    }
                    LoadingChoice::Gui(name) => {
                        loading.screen_gui = name;
                        self.set_loading_screen(loading);
                    }
                }
            }
            Command::DeveloperConsole => {
                if let Some(project) = &self.project {
                    let on = !project.developer_console;
                    self.set_developer_console(on);
                }
            }
            Command::WaitForScript => {
                if let Some(project) = &self.project {
                    let mut loading = project.loading_screen.clone();
                    loading.wait_for_script = !loading.wait_for_script;
                    self.set_loading_screen(loading);
                }
            }
            Command::OpenStylesheet => self.open_stylesheet_with_dialog(),
            Command::NewStylesheet => self.new_stylesheet(),
            Command::CheckAssets => self.run_asset_check(),
            Command::SetUpAutocomplete => self.set_up_autocomplete(),
            Command::Settings => self.settings.open = true,
            Command::Quit => self.request_and_go_ahead(ctx, Discard::Quit),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Rename => {
                if let Some(id) = self.selected
                    && !playing
                {
                    self.explorer.start_rename(&self.world, id);
                }
            }
            Command::Duplicate => {
                if self.selected.is_some() {
                    self.duplicate_selected();
                }
            }
            Command::Delete => {
                if self.selected.is_some() {
                    self.delete_selected();
                }
            }
            Command::Theme(theme) => {
                let appearance = theme::Appearance {
                    theme,
                    ..self.appearance.clone()
                };
                self.set_appearance(ctx, appearance);
            }
            Command::Appearance => {
                self.settings.open = true;
                self.settings.page = settings::SettingsPage::Appearance;
            }
            Command::TogglePane(pane) => self.layout.toggle(pane),
            Command::ResetLayout => self.layout.reset(),
            Command::LightCones => self.light_cones = !self.light_cones,
            Command::ViewCube => self.view_cube = !self.view_cube,
            Command::Grid => self.grid = !self.grid,
            Command::ViewMode(mode) => self.view_mode = mode,
            Command::Overlay(overlay) => {
                let flag = overlay.flag(&mut self.overlays);
                *flag = !*flag;
            }
            Command::NewRibbonTab => self.ribbon_tabs.edit(None),
            Command::RibbonTab(tab) => self.ribbon_tabs.set_active(ActiveTab::BuiltIn(tab)),
            Command::Learn => {
                if self.project.is_some() {
                    self.learn_open = !self.learn_open;
                }
            }
            Command::About => self.about_open = true,
            Command::PanicDuringPlay => {
                if let Some(session) = &mut self.play {
                    session.panic_next_frame();
                }
            }
            Command::Play => {
                if self.paused() {
                    self.continue_play();
                } else if !playing && self.project.is_some() {
                    self.play();
                }
            }
            Command::Stop => self.stop(),
            Command::Tool(tool) => self.toggle_tool(tool),
            Command::Snap(switch) => {
                let flag = switch.flag(&mut self.snap);
                *flag = !*flag;
            }
            Command::CollisionGroups => self.collision_groups.open = true,
            Command::Insert(what) if !playing => {
                let near = self.selected;
                self.act(Action::Insert { what, near });
            }
            Command::BakeProbes if !playing => self.bake_probes_asked = true,
            Command::Solid(action) if !playing => self.solid_action(action),
            Command::Weld if !playing => self.weld_selected(),
            Command::Constraint(class) => {
                if !playing || class.is_none() {
                    self.pick_constraint_tool(class);
                }
            }
            Command::InsertScript(class) if !playing => self.act(Action::InsertScript(class)),
            Command::InsertControls if !playing => self.insert_controls_ui(),
            Command::TerrainStudio => self.layout.open(Pane::Terrain),
            Command::TerrainTool(tool) if !playing => {
                self.terrain.tool = Some(tool);
                self.layout.open(Pane::Terrain);
            }
            Command::PluginButton { plugin, button } if !playing => {
                self.click_plugin_button(plugin, button);
            }
            Command::Palette => self.palette.toggle(Mode::Commands),
            Command::QuickOpen => self.open_quick_open(),
            Command::FindInScripts => self.find.show_and_focus(),
            Command::Insert(_)
            | Command::BakeProbes
            | Command::Solid(_)
            | Command::Weld
            | Command::InsertScript(_)
            | Command::InsertControls
            | Command::TerrainTool(_)
            | Command::PluginButton { .. } => {
                self.output.error("Press Stop to edit.");
            }
        }
    }

    /// The palette's keys, its window, Quick Open and Find in Scripts, run
    /// before the other shortcuts so their keys reach them first.
    pub(super) fn palette_frame(&mut self, ctx: &egui::Context) {
        // Shift first: egui counts Ctrl+Shift+P as Ctrl+P too.
        let (palette, find, quick) = ctx.input_mut(|input| {
            let palette = input.consume_shortcut(&PALETTE_SHORTCUT);
            let find = input.consume_shortcut(&FIND_SHORTCUT);
            (palette, find, input.consume_shortcut(&QUICK_OPEN_SHORTCUT))
        });
        if palette {
            self.palette.toggle(Mode::Commands);
        }
        if quick && self.project.is_some() {
            self.open_quick_open();
        }
        if find && self.project.is_some() {
            self.find.show_and_focus();
            self.search_scripts();
        }
        match self.palette.open {
            Some(Mode::Commands) => {
                let commands = self.palette_commands(ctx);
                let entries: Vec<Entry> = commands.iter().map(|c| c.entry.clone()).collect();
                if let Some(index) = self.palette.ui(ctx, &entries) {
                    let command = commands[index].command.clone();
                    self.run_command(ctx, command);
                }
            }
            Some(Mode::QuickOpen) => {
                let entries: Vec<Entry> = self
                    .quick_open
                    .iter()
                    .map(|(entry, _)| entry.clone())
                    .collect();
                if let Some(index) = self.palette.ui(ctx, &entries) {
                    let target = self.quick_open[index].1.clone();
                    self.open_target(ctx, target);
                }
            }
            None => {}
        }
        let can_replace = !self.playing();
        match self.find.ui(ctx, can_replace) {
            Some(FindAction::Search) => self.search_scripts(),
            Some(FindAction::GoTo(hit)) => self.go_to_hit(&hit),
            Some(FindAction::ReplaceAll) => self.replace_all_in_scripts(),
            None => {}
        }
    }

    /// Opens Quick Open, listing what's in the project now.
    pub(super) fn open_quick_open(&mut self) {
        if self.project.is_none() {
            return;
        }
        self.quick_open = self.quick_open_entries();
        self.palette.toggle(Mode::QuickOpen);
    }

    /// Quick Open's rows: scripts, then assets, levels and Instances.
    pub(super) fn quick_open_entries(&self) -> Vec<(Entry, OpenTarget)> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let mut list = Vec::new();
        for script in &project.scripts {
            let mut entry = Entry::new(
                format!("script {}", script.path),
                project::script_name(&script.path),
                &script.path,
            );
            entry.icon = theme::class_icon(project::script_class(&script.path));
            list.push((entry, OpenTarget::Script(script.path.clone())));
        }
        for path in project::list_project_files(&project.folder) {
            if !path.starts_with("assets/") || project::is_model_part(&path) {
                continue;
            }
            let name = path.rsplit('/').next().unwrap_or(&path);
            let mut entry = Entry::new(format!("asset {path}"), name, &path);
            entry.icon = icons::FILE;
            list.push((entry, OpenTarget::Asset(path.clone())));
        }
        let start = project.start_level.clone();
        let mut entry = Entry::new(format!("level {start}"), &start, "Level (start)");
        entry.icon = icons::MAP_TRIFOLD;
        list.push((entry, OpenTarget::Level(None)));
        for name in project::level_names(&project.folder) {
            if name == start {
                continue;
            }
            let mut entry = Entry::new(format!("level {name}"), &name, "Level");
            entry.icon = icons::MAP_TRIFOLD;
            list.push((entry, OpenTarget::Level(Some(name))));
        }
        let world = shown(&self.world, &self.play);
        for id in world.descendants(world.root()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let path = world.full_name(id).unwrap_or_default();
            let mut entry = Entry::new(format!("instance {path}"), instance.name(), &path);
            entry.icon = theme::class_icon(instance.class());
            list.push((entry, OpenTarget::Instance(id)));
        }
        list
    }

    /// Opens or selects what a Quick Open row names: a script in its tab, a
    /// material or data table in its tab and other assets in the Assets
    /// panel, a level (asking to save first), or an Instance selected.
    pub(super) fn open_target(&mut self, ctx: &egui::Context, target: OpenTarget) {
        match target {
            OpenTarget::Script(path) => self.open_script(&path, None),
            OpenTarget::Asset(path) => {
                if axcenity_assets::is_material_file(&path) {
                    self.open_material(&path);
                } else if axcenity_assets::is_data_table(&path) {
                    self.open_data_table(&path);
                } else {
                    self.layout.open(Pane::Assets);
                    self.asset_browser.reveal(&path);
                }
            }
            OpenTarget::Level(level) => {
                let open = self.project.as_ref().and_then(|p| p.level.clone());
                if open != level {
                    self.request_and_go_ahead(ctx, Discard::OpenLevel(level));
                }
            }
            OpenTarget::Instance(id) => {
                self.select_from_viewport(Some(id));
                self.layout.open(Pane::Explorer);
            }
        }
    }
}
