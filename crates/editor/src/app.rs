//! The editor window: a menu bar, the Explorer and Properties on the right,
//! the Output panel along the bottom, and the 3D viewport filling the rest,
//! like Roblox Studio.
//!
//! Play runs the game on a copy of the world (see `play.rs`). While it
//! plays, every panel shows that copy. Only the Properties panel can change
//! it (the game's scripts see those changes); everything else is read-only.
//! Stop drops the copy.
//!
//! Above the viewport, tabs switch between the viewport ("World") and the
//! open scripts. Everything that reads or writes project files (saving,
//! scripts, outside changes) is in `app/files.rs`.

mod assets;
mod files;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_blocks::{BlockScript, translate};
use axcenity_core::{ClassName, Color3, InstanceId, Material, PartType, Value, Vector3, World};
use axcenity_project::ScriptFile;
use axcenity_script::{CameraState, KeyEvent};
use eframe::egui;

use crate::asset_browser::{AssetBrowser, AssetPayload, BrowserAction};
use crate::autocomplete;
use crate::block_editor;
use crate::build_tools::{self, BuildSettings, PanelScene, ToolAction};
use crate::dialog;
use crate::explorer::{self, Action, Explorer, ExplorerOutput};
use crate::gizmo::{Modes, Snap, Tool};
use crate::history::{self, Field, History, Move, Place};
use crate::import_dialog::ImportDialog;
use crate::output::{Output, OutputKind};
use crate::play::{self, Crash, GameKeys, Session};
use crate::properties::{self, Edit};
use crate::ribbon::{self, RibbonButton};
use crate::scripts::{self, ScriptTabs};
use crate::side_warnings::{self, WarningCache};
use crate::start::{self, Brand, ProjectLink, StartAction, StartScreen};
use crate::style_panel::{self, StyleEdit};
use crate::theme::{self, filled_icons, icons};
use crate::thumbnail;
use crate::tree::{self, Insertable};
use crate::ui_handles::{CssEdit, UiDragEdit, apply_css_edits};
use crate::viewport::{self, DragEdit, DragKind, Viewport, ViewportScene};
use crate::watcher::Watcher;

/// The hover text of Import Model, in the File menu and the toolbar.
const IMPORT_HOVER: &str = "Import a 3D model (.gltf, .glb or .fbx), such as one exported from \
    Blender. It's copied into the project's assets/models folder and inserted as a MeshPart \
    at the camera's focus point.";

/// F5 plays and Shift+F5 stops, as in Roblox Studio.
const PLAY_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F5);
const STOP_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::F5);
/// During Play, after a long pause (such as dragging the window), don't jump
/// the game ahead by the whole pause; treat the frame as at most this long.
/// The same cap as the player's.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// Ctrl+S, or Cmd+S on macOS (`COMMAND` picks the right one).
const SAVE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
const UNDO_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Y, as on Windows, and Ctrl+Shift+Z (Cmd+Shift+Z on macOS) both redo.
const REDO_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
const REDO_SHIFT_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);
/// Ctrl+D (Cmd+D on macOS) duplicates the selection, as in Studio.
pub const DUPLICATE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::D);
/// F2 renames the selection in the Explorer, as in Studio and most file managers.
pub const RENAME_KEY: egui::Key = egui::Key::F2;
/// The Delete key deletes the selection. On macOS, the key labelled
/// "delete" is Backspace, so that works there too.
pub const DELETE_KEY_TEXT: &str = if cfg!(target_os = "macos") {
    "⌫"
} else {
    "Del"
};

pub struct EditorApp {
    /// The open project's Instances, or an empty world when none is open.
    world: World,
    project: Option<OpenProject>,
    /// The open project's models, loaded as MeshParts are drawn, and shared
    /// with Play's physics. Has none when no project is open.
    assets: Assets,
    /// The Instance shown in Properties and given handles. The last of [`selection`](Self::selection).
    selected: Option<InstanceId>,
    /// Every selected Instance, in the order they were added. The viewport and
    /// Explorer highlight all of them. Ctrl toggles; Shift extends.
    selection: Vec<InstanceId>,
    /// File > Version History, and the same window after a restore.
    versions_open: bool,
    /// Every change made to `world` since it was opened, for undo and redo.
    /// It also knows whether the world is as saved.
    history: History,
    /// The tool picked in the toolbar, and its snapping settings. Both are
    /// preferences, so they stay when another project is opened.
    tool: Tool,
    snap: Snap,
    /// The Move, Resize and Rotate tools' axes, directions and pivot, and
    /// the other building tools' settings. Preferences, like `snap`.
    modes: Modes,
    build: BuildSettings,
    /// Something waiting for an answer to "Save changes?".
    asking: Option<Discard>,
    /// Set once closing has been allowed, so the window's close request
    /// isn't stopped again.
    closing: bool,
    explorer: Explorer,
    output: Output,
    viewport: Viewport,
    /// Original stylesheet contents for the current viewport drag; CSS edit
    /// spans remain relative to this text while the pointer moves.
    ui_drag_baselines: HashMap<String, String>,
    /// The window title last sent to the window, to send it again only when it changes.
    shown_title: String,
    /// The game, while Play is on. `world` stays as it was before Play.
    play: Option<Session>,
    /// A button clicked in the viewport, delivered on the next Play frame.
    pending_ui_activation: Option<InstanceId>,
    /// A TextBox the player is typing in during Play.
    focused_text: Option<InstanceId>,
    pending_focus_lost: Option<InstanceId>,
    /// UI layout warnings already shown, so a PageLayout problem is not
    /// repeated every frame.
    ui_warnings: Vec<String>,
    /// When the last Play frame ran, to measure how much game time passed.
    last_frame: Option<Instant>,
    /// Turns keyboard events into key presses for LocalScripts during Play.
    game_keys: GameKeys,
    /// Each script's uses of APIs from the other side, for the warning badges.
    warnings: WarningCache,
    /// The open script tabs, and whether the viewport or a script is shown.
    tabs: ScriptTabs,
    /// A Script waiting for an answer to "Delete script?".
    confirm_delete: Option<InstanceId>,
    /// A model file waiting for its objects to be chosen.
    importing: Option<ImportDialog>,
    /// Dark, light or the system's, picked in View > Theme. Saved between runs.
    theme: egui::ThemePreference,
    /// The logo images, loaded on the first frame (they need the egui context).
    brand: Option<Brand>,
    /// Projects opened lately, newest first, for the start screen. Saved between runs.
    recent: Vec<ProjectLink>,
    /// The start screen's thumbnails, search and dialogs.
    start: StartScreen,
    /// The thumbnail being captured, if one was asked for.
    thumbnail: Option<ThumbnailShot>,
    /// Set when a thumbnail should be taken at the end of this frame. True
    /// inside means the project closes once it's saved.
    thumbnail_wanted: Option<bool>,
    /// Where the viewport was drawn last frame, to crop the thumbnail to.
    viewport_rect: Option<egui::Rect>,
    /// True while Help > About is open.
    about_open: bool,
    /// A Sound being listened to from the Properties panel, outside Play.
    sound_preview: Option<axcenity_audio::AudioEngine>,
    /// The Assets panel, which shares the bottom of the window with Output.
    asset_browser: AssetBrowser,
    bottom_tab: BottomTab,
}

/// Which panel the bottom of the window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BottomTab {
    #[default]
    Output,
    Assets,
}

struct BlockDocument {
    instance: InstanceId,
    path: String,
    script: BlockScript,
    saved: BlockScript,
    highlighted: Option<u64>,
}

/// The keys the editor's settings are saved under (see [`EditorApp::load_settings`]).
const THEME_KEY: &str = "theme";
const RECENT_KEY: &str = "recent projects";

/// Something that throws unsaved changes away, so it has to ask first if
/// there are any.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Discard {
    Quit,
    OpenAnother,
    /// Closing the tab of the script at this path.
    CloseTab(String),
    /// Going back to the start screen.
    CloseProject,
}

/// A screenshot asked for, to make the open project's thumbnail from.
struct ThumbnailShot {
    folder: PathBuf,
    /// Where the viewport was, in points.
    area: egui::Rect,
    /// Close the project once it's saved.
    then_close: bool,
}

/// The answers to "Save changes?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Save,
    DontSave,
    Cancel,
}

/// The shortcut keys pressed this frame.
#[derive(Default)]
struct Keys {
    undo: bool,
    redo: bool,
    tool: Option<Tool>,
    /// Enter, which switches Anchored or CanCollide with those tools.
    toggle: bool,
    rename: bool,
    duplicate: bool,
    delete: bool,
}

/// What the editor remembers about the open project.
struct OpenProject {
    name: String,
    show_splash: bool,
    folder: PathBuf,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    /// Each stylesheet's text as last read or written, by path, to tell the
    /// editor's own saves from changes made elsewhere.
    stylesheet_texts: HashMap<String, String>,
    /// Its server scripts, sorted by path, with each file's text as last
    /// read or written. Play reads their files again, for the newest code.
    scripts: Vec<ScriptFile>,
    block_scripts: Vec<BlockDocument>,
    /// Blocks switched to Code, by the code file's path, with the code they
    /// became: switched back with that code unchanged, they come back as
    /// they were (loose blocks and where each stack was).
    remembered_blocks: HashMap<String, (String, BlockScript)>,
    /// Notices files changed by other programs.
    watcher: Watcher,
    /// The world file's text as last read or written, to tell the editor's
    /// own saves from changes made elsewhere. `None` if it couldn't be read.
    world_text: Option<String>,
    /// True when the world file holds something the editor never loaded:
    /// it was damaged, or changed elsewhere while the world had unsaved
    /// changes. Saving keeps it as `world.backup.json` first.
    keep_world_file: bool,
}

impl EditorApp {
    /// An editor with `folder` open, or with nothing open if it's `None`.
    pub fn new(folder: Option<&Path>) -> Self {
        let world = World::new();
        let mut app = Self {
            explorer: Explorer::new(&world),
            world,
            project: None,
            assets: Assets::default(),
            selected: None,
            selection: Vec::new(),
            versions_open: false,
            history: History::new(),
            tool: Tool::default(),
            snap: Snap::default(),
            modes: Modes::default(),
            build: BuildSettings::default(),
            asking: None,
            closing: false,
            output: Output::default(),
            viewport: Viewport::new(),
            ui_drag_baselines: HashMap::new(),
            shown_title: String::new(),
            play: None,
            pending_ui_activation: None,
            focused_text: None,
            pending_focus_lost: None,
            ui_warnings: Vec::new(),
            last_frame: None,
            tabs: ScriptTabs::default(),
            confirm_delete: None,
            importing: None,
            game_keys: GameKeys::default(),
            warnings: WarningCache::default(),
            theme: egui::ThemePreference::Dark,
            brand: None,
            recent: Vec::new(),
            start: StartScreen::default(),
            thumbnail: None,
            thumbnail_wanted: None,
            viewport_rect: None,
            about_open: false,
            sound_preview: None,
            asset_browser: AssetBrowser::default(),
            bottom_tab: BottomTab::default(),
        };
        if let Some(folder) = folder {
            app.open(folder);
        }
        app
    }

    /// Opens the project in `folder` in place of the current one. If it
    /// can't be loaded, the error goes to Output and the current project
    /// stays open.
    pub fn open(&mut self, folder: &Path) {
        let project = match axcenity_project::load(folder) {
            Ok(project) => project,
            Err(err) => {
                // `{:#}` on an `anyhow::Error` shows the whole chain of
                // causes on one line, such as which line of which file is wrong.
                let err = anyhow::Error::new(err);
                self.output.error(format!(
                    "Could not open the project in `{}`: {err:#}",
                    folder.display()
                ));
                return;
            }
        };

        let scripts = project.scripts.len();
        let show_splash = project.manifest.show_splash;
        self.output.info(format!(
            "Opened \"{}\" from `{}`: {} objects, {scripts} {}. Press Play (F5) \
             to run it.",
            project.manifest.name,
            folder.display(),
            // `game` and its services are always there, so don't count them.
            project
                .world
                .descendants(project.world.root())
                .into_iter()
                .filter(|&id| project
                    .world
                    .get(id)
                    .is_some_and(|instance| !instance.class().is_service()))
                .count(),
            if scripts == 1 { "script" } else { "scripts" },
        ));
        files::report_problems(&mut self.output, &project);
        let mut stylesheets = Vec::new();
        let stylesheet_texts = project
            .stylesheets
            .iter()
            .map(|file| (file.path.clone(), file.source.clone()))
            .collect();
        for file in &project.stylesheets {
            let (stylesheet, warnings) = axcenity_ui::parse_stylesheet(&file.path, &file.source);
            stylesheets.push(stylesheet);
            for warning in warnings {
                self.output
                    .warning(format!("{}: {}", warning.path, warning.message));
            }
        }
        // A project set up for autocomplete by an older engine gets this
        // engine's API.
        match autocomplete::refresh(&project.folder) {
            Ok(true) => self.output.info(format!(
                "Updated `{}` to this version of the engine's script API.",
                autocomplete::DEFINITIONS_FILE
            )),
            Ok(false) => {}
            Err(err) => self.output.error(format!(
                "Could not update `{}`: {err}",
                autocomplete::DEFINITIONS_FILE
            )),
        }
        // The game being played belongs to the old project, and so do the models.
        self.play = None;
        self.thumbnail = None;
        self.thumbnail_wanted = None;
        self.pending_ui_activation = None;
        self.focused_text = None;
        self.pending_focus_lost = None;
        self.assets = Assets::new(&project.folder);
        self.asset_browser.open(Some(&project.folder));
        self.tabs.close_all();
        self.viewport.reset_camera();
        let world_text = files::read_world_text(&project.folder);
        // Saved as a full path, so it opens from the start screen wherever
        // the editor is started.
        let folder = std::path::absolute(&project.folder).unwrap_or(project.folder.clone());
        start::remember(
            &mut self.recent,
            ProjectLink {
                name: project.manifest.name.clone(),
                description: project.manifest.description.clone(),
                folder,
            },
        );
        let block_scripts = project
            .block_scripts
            .into_iter()
            .map(|file| BlockDocument {
                instance: file.instance,
                path: file.path,
                script: file.script.clone(),
                saved: file.script,
                highlighted: None,
            })
            .collect();
        self.project = Some(OpenProject {
            name: project.manifest.name,
            show_splash,
            watcher: Watcher::start(project.folder.clone()),
            folder: project.folder,
            stylesheets,
            stylesheet_texts,
            scripts: project.scripts,
            block_scripts,
            remembered_blocks: HashMap::new(),
            world_text,
            keep_world_file: !project.world_loaded,
        });
        self.replace_world(project.world);
    }

    /// Restores the settings saved by [`eframe::App::save`] (none in tests, or
    /// on the first run) and applies the theme.
    pub fn load_settings(&mut self, ctx: &egui::Context, storage: Option<&dyn eframe::Storage>) {
        if let Some(storage) = storage {
            self.theme = match storage.get_string(THEME_KEY).as_deref() {
                Some("light") => egui::ThemePreference::Light,
                Some("system") => egui::ThemePreference::System,
                _ => egui::ThemePreference::Dark,
            };
            // One folder per line. The project opened at startup stays first.
            let opened = std::mem::take(&mut self.recent);
            for line in storage.get_string(RECENT_KEY).unwrap_or_default().lines() {
                if let Some(link) = ProjectLink::read(Path::new(line)) {
                    self.recent.push(link);
                }
            }
            for link in opened.into_iter().rev() {
                start::remember(&mut self.recent, link);
            }
            self.recent.truncate(start::MAX_TRACKED);
        }
        ctx.set_theme(self.theme);
    }

    /// Puts `world` in place of the world being edited, with nothing
    /// selected and a new undo history (its steps belong to the old world).
    fn replace_world(&mut self, world: World) {
        self.world = world;
        self.selected = None;
        self.selection.clear();
        self.history = History::new();
        self.explorer = Explorer::new(&self.world);
    }

    /// File > Set Up VS Code Autocomplete: writes the files Lua Language
    /// Server needs into the project, and says what to do next.
    fn set_up_autocomplete(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        match autocomplete::set_up(&project.folder) {
            Ok(result) => {
                let mut message = format!(
                    "Set up autocomplete: wrote {}.",
                    quoted_list(&result.written)
                );
                if !result.kept.is_empty() {
                    message.push_str(&format!(" Kept your own {}.", quoted_list(&result.kept)));
                }
                message.push_str(
                    " Open the project folder in VS Code and install the Lua extension \
                     (sumneko.lua) when it asks; scripts then autocomplete the engine's API.",
                );
                self.output.info(message);
            }
            Err(err) => self.output.error(format!(
                "Could not set up autocomplete in `{}`: {err}",
                project.folder.display()
            )),
        }
    }

    /// Asks for a project folder with the system's folder picker, then opens it.
    fn open_with_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Open an Axcenity project folder");
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.folder);
        }
        // `None` means the dialog was cancelled.
        if let Some(folder) = dialog.pick_folder() {
            self.open(&folder);
        }
    }

    /// Selects `id` (or nothing), from a click in the viewport. The Explorer
    /// opens the branches above it and scrolls to it, so it's highlighted there too.
    fn select_from_viewport(&mut self, id: Option<InstanceId>) {
        self.selection = id.into_iter().collect();
        self.selected = id;
        if let Some(id) = id {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }

    /// A click in the Explorer or the viewport. `order` is the Explorer's
    /// visible rows, so Shift can select the range; the viewport passes `None`.
    fn select_clicked(
        &mut self,
        id: Option<InstanceId>,
        modifiers: egui::Modifiers,
        order: Option<&[InstanceId]>,
        keep_group: bool,
    ) {
        explorer::update_selection(&mut self.selection, id, modifiers, order, keep_group);
        self.selected = self.selection.last().copied();
        if let Some(id) = self.selected {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }

    /// The selected Instances that aren't inside another selected one, so
    /// deleting or duplicating a parent doesn't also do it to its children.
    fn selection_roots(&self) -> Vec<InstanceId> {
        let ids = if self.selection.is_empty() {
            self.selected.into_iter().collect::<Vec<_>>()
        } else {
            self.selection.clone()
        };
        ids.iter()
            .copied()
            .filter(|id| {
                !ids.iter()
                    .any(|other| self.world.is_descendant_of(*id, *other))
            })
            .collect()
    }

    fn playing(&self) -> bool {
        self.play.is_some()
    }

    /// True when the world or a script tab has changes that aren't saved.
    /// Undoing back to where it was saved counts as no changes.
    fn unsaved(&self) -> bool {
        !self.history.is_saved()
            || self.tabs.any_unsaved()
            || self.project.as_ref().is_some_and(|project| {
                project
                    .block_scripts
                    .iter()
                    .any(|document| document.script != document.saved)
            })
    }

    /// Applies a change from the Properties panel, as an undo step such as
    /// "Change Color of Pad". During Play it changes the game instead.
    fn apply(&mut self, id: InstanceId, edit: Edit) {
        let Some(instance) = shown(&self.world, &self.play).get(id) else {
            return;
        };
        let (key, label) = match &edit {
            Edit::Name(_) => (Field::Name, format!("Rename {}", instance.name())),
            Edit::Property(name, _) => (
                Field::Property(name),
                format!("Change {name} of {}", instance.name()),
            ),
            &Edit::PickTexture(name) => {
                if let Some(path) = self.pick_texture() {
                    self.apply(id, Edit::Property(name, Value::String(path)));
                    // Picking is one whole step, not more typing.
                    self.history.seal();
                }
                return;
            }
            Edit::PickAudio => {
                if let Some(path) = self.pick_audio() {
                    self.apply(id, Edit::Property("SoundId", Value::String(path)));
                    self.history.seal();
                }
                return;
            }
            &Edit::PreviewSound(play) => {
                self.preview_sound(id, play);
                return;
            }
        };
        self.change(id, key, label, vec![edit]);
    }

    /// Applies a drag from the viewport (a part or one of its handles), as
    /// an undo step such as "Move Pad".
    fn apply_drag(&mut self, edit: DragEdit) {
        let Some(instance) = edit.parts.last().and_then(|(id, _)| self.world.get(*id)) else {
            return;
        };
        let (property, verb) = match edit.kind {
            DragKind::Move => ("Position", "Move"),
            DragKind::Resize => ("Size", "Resize"),
            DragKind::Rotate => ("Orientation", "Rotate"),
        };
        let label = match edit.parts.len() {
            1 => format!("{verb} {}", instance.name()),
            n => format!("{verb} {n} parts"),
        };
        let edits = edit
            .parts
            .into_iter()
            .map(|(id, values)| {
                let edits = values
                    .into_iter()
                    .map(|(name, value)| Edit::Property(name, value))
                    .collect();
                (id, edits)
            })
            .collect();
        self.change_many(Field::Property(property), label, edits);
    }

    /// The parts the building tools act on: the selected parts and those
    /// inside selected Models, the last selected last.
    fn tool_parts(&self) -> Vec<InstanceId> {
        let world = shown(&self.world, &self.play);
        viewport::selected_parts(world, &self.selection, self.selected)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Does what a building tool's panel asked, to the parts it acts on,
    /// as one undo step.
    fn apply_tool_action(&mut self, action: ToolAction) {
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let parts = self.tool_parts();
        match action {
            ToolAction::Set { property, value } => self.set_on_parts(&parts, property, value),
            ToolAction::AddLight(class) => self.add_lights(&parts, class),
            ToolAction::RemoveLights(class) => {
                let lights = build_tools::lights_in(&self.world, &parts, class);
                let moves: Vec<Move> = lights
                    .iter()
                    .filter_map(|&id| {
                        let before = history::place_of(&self.world, id)?;
                        history::put(&mut self.world, id, None).ok()?;
                        Some(Move {
                            id,
                            before: Some(before),
                            after: None,
                        })
                    })
                    .collect();
                let label = format!("Remove {} {class}s", moves.len());
                self.history.record_tree(&mut self.world, label, moves);
                self.forget_selection_outside_tree();
            }
            ToolAction::SetLight {
                class,
                property,
                value,
            } => {
                let lights = build_tools::lights_in(&self.world, &parts, class);
                let edits = lights
                    .into_iter()
                    .map(|id| (id, vec![Edit::Property(property, value.clone())]))
                    .collect();
                self.change_many(
                    Field::Property(property),
                    format!("Change {property} of {class}s"),
                    edits,
                );
            }
        }
    }

    /// Sets `property` to `value` on each of `parts` that has it, as one step.
    fn set_on_parts(&mut self, parts: &[InstanceId], property: &'static str, value: Value) {
        let world = shown(&self.world, &self.play);
        let targets: Vec<InstanceId> = parts
            .iter()
            .copied()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|i| i.property(property).is_some())
            })
            .collect();
        let Some(name) = targets
            .last()
            .and_then(|&id| world.get(id))
            .map(|i| i.name().to_owned())
        else {
            return;
        };
        let label = build_tools::step_label(property, &value, targets.len(), &name);
        let edits = targets
            .into_iter()
            .map(|id| (id, vec![Edit::Property(property, value.clone())]))
            .collect();
        self.change_many(Field::Property(property), label, edits);
    }

    /// Puts a new `class` light in each of `parts` that hasn't one, as one step.
    fn add_lights(&mut self, parts: &[InstanceId], class: ClassName) {
        let mut moves = Vec::new();
        for &part in parts {
            if !build_tools::lights_in(&self.world, &[part], class).is_empty() {
                continue;
            }
            let Ok(id) = self.world.create(class) else {
                continue;
            };
            let name = tree::unique_name(&self.world, part, class.as_str());
            let _ = self.world.set_name(id, name);
            let index = self.world.get(part).map_or(0, |p| p.children().len());
            let place = Place {
                parent: part,
                index,
            };
            if history::put(&mut self.world, id, Some(place)).is_ok() {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            }
        }
        let label = match moves.len() {
            1 => format!("Add {class}"),
            n => format!("Add {n} {class}s"),
        };
        self.history.record_tree(&mut self.world, label, moves);
    }

    /// The Paint tool: a click on a part paints it, and the rest of the
    /// selection too if it's selected.
    fn paint_clicked(&mut self, part: InstanceId) {
        let parts = if self.selection.contains(&part) {
            self.tool_parts()
        } else {
            vec![part]
        };
        let color = Value::Color3(self.build.paint_color);
        self.set_on_parts(&parts, "Color", color);
        self.history.seal();
    }

    /// The selected building tool's options panel, at the top left of the viewport.
    fn tool_panel(&mut self, ctx: &egui::Context, viewport: egui::Rect) {
        if self.tool == Tool::Select || self.playing() {
            return;
        }
        let parts = self.tool_parts();
        let scene = PanelScene {
            world: &self.world,
            parts: &parts,
        };
        let (tool, settings, modes, snap) =
            (self.tool, &mut self.build, &mut self.modes, &mut self.snap);
        let actions = egui::Area::new(egui::Id::new("building tool panel"))
            .fixed_pos(viewport.min + egui::vec2(10.0, 10.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| {
                        ui.set_max_width(260.0);
                        build_tools::panel(ui, tool, &scene, settings, modes, snap)
                    })
                    .inner
            })
            .inner;
        for action in actions {
            self.apply_tool_action(action);
            ctx.request_repaint();
        }
    }

    fn apply_ui_drag(&mut self, edit: UiDragEdit) {
        if self.playing() || self.world.get(edit.id).is_none() {
            return;
        }
        for (property, value) in edit.values {
            self.apply(edit.id, Edit::Property(property, value));
        }

        let mut edits_by_path: HashMap<String, Vec<CssEdit>> = HashMap::new();
        for css_edit in edit.css {
            edits_by_path
                .entry(css_edit.path.clone())
                .or_default()
                .push(css_edit);
        }
        for (path, edits) in edits_by_path {
            if !self.ui_drag_baselines.contains_key(&path) {
                let source = self
                    .tabs
                    .get(&path)
                    .map(|tab| tab.text.clone())
                    .or_else(|| {
                        self.project.as_ref().and_then(|project| {
                            std::fs::read_to_string(project.folder.join(&path)).ok()
                        })
                    });
                let Some(source) = source else {
                    self.output.error(format!(
                        "Could not edit stylesheet `{path}`: its source file could not be read."
                    ));
                    continue;
                };
                self.ui_drag_baselines.insert(path.clone(), source.clone());
                self.tabs.open_in_background(&path, &source);
            }
            let Some(source) = self.ui_drag_baselines.get(&path) else {
                continue;
            };
            let updated = apply_css_edits(source, edits.iter());
            if let Some(tab) = self.tabs.get_mut(&path) {
                tab.text = updated.clone();
            }
            self.refresh_stylesheet(&path, &updated);
        }
    }

    /// The text of the stylesheet at `path`: its tab's, if it's open (it may
    /// have unsaved changes), otherwise the file's.
    fn stylesheet_text(&self, path: &str) -> Option<String> {
        self.tabs.get(path).map(|tab| tab.text.clone()).or_else(|| {
            let project = self.project.as_ref()?;
            std::fs::read_to_string(project.folder.join(path)).ok()
        })
    }

    /// Replaces the text of the stylesheet at `path` with `updated`. It goes
    /// in the stylesheet's tab (opened in the background), unsaved like any
    /// typed change, and the viewport is restyled with it at once.
    fn set_stylesheet_text(&mut self, path: &str, updated: &str) {
        if self.tabs.get(path).is_none() {
            let saved = self.stylesheet_text(path).unwrap_or_default();
            self.tabs.open_in_background(path, &saved);
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = updated.to_owned();
        }
        self.refresh_stylesheet(path, updated);
    }

    /// Does what was done in the Style section for the UI element `id`.
    fn apply_style(&mut self, id: InstanceId, edit: StyleEdit) {
        if self.playing() {
            return;
        }
        match edit {
            StyleEdit::Set { path, span, value } => {
                let Some(source) = self.stylesheet_text(&path) else {
                    self.output.error(format!(
                        "Could not edit stylesheet `{path}`: its file could not be read."
                    ));
                    return;
                };
                let edits = [CssEdit {
                    path: path.clone(),
                    span,
                    value,
                }];
                let updated = apply_css_edits(&source, edits.iter());
                self.set_stylesheet_text(&path, &updated);
            }
            StyleEdit::Open { path, line } => {
                let source = self.stylesheet_text(&path).unwrap_or_default();
                self.tabs.open(&path, &source, Some(line));
            }
            StyleEdit::Add { property, value } => self.add_style(id, &property, &value),
        }
    }

    /// Adds `property: value` to the rule for `id`'s own `Id`, giving it an
    /// `Id` (from its name) if it has none. The rule is the first one with
    /// that selector, or a new one at the end of the first stylesheet; a
    /// project with no stylesheet gets `ui.css`.
    fn add_style(&mut self, id: InstanceId, property: &str, value: &str) {
        let Some(instance) = self.world.get(id) else {
            return;
        };
        let mut element_id = instance
            .property("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if element_id.is_empty() {
            element_id = style_panel::id_from_name(instance.name());
            self.apply(id, Edit::Property("Id", Value::String(element_id.clone())));
            self.history.seal();
        }
        let selector = style_panel::own_selector(&element_id);
        let Some(project) = &self.project else {
            return;
        };
        let target = project
            .stylesheets
            .iter()
            .find_map(|sheet| Some((sheet.path().to_owned(), sheet.rule_end(&selector)?)))
            .map(|(path, end)| (path, Some(end)))
            .or_else(|| {
                project
                    .stylesheets
                    .first()
                    .map(|sheet| (sheet.path().to_owned(), None))
            });
        match target {
            Some((path, end)) => {
                let Some(source) = self.stylesheet_text(&path) else {
                    return;
                };
                let updated =
                    style_panel::add_declaration(&source, end, &selector, property, value);
                self.set_stylesheet_text(&path, &updated);
                self.output.info(format!(
                    "Added `{property}: {value}` to `{selector}` in `{path}`."
                ));
            }
            None => {
                let folder = project.folder.clone();
                let source = style_panel::add_declaration("", None, &selector, property, value);
                match axcenity_project::create_stylesheet(&folder, "ui", &source) {
                    Ok(path) => {
                        if let Some(project) = &mut self.project {
                            project
                                .stylesheet_texts
                                .insert(path.clone(), source.clone());
                        }
                        self.refresh_stylesheet(&path, &source);
                        self.tabs.open_in_background(&path, &source);
                        self.output.info(format!(
                            "Made `{path}` for this project's UI styles, with `{selector}` in it."
                        ));
                    }
                    Err(err) => self
                        .output
                        .error(format!("Could not make a stylesheet: {err}")),
                }
            }
        }
    }

    fn refresh_stylesheet(&mut self, path: &str, source: &str) {
        let (stylesheet, warnings) = axcenity_ui::parse_stylesheet(path, source);
        let Some(project) = &mut self.project else {
            return;
        };
        match project
            .stylesheets
            .iter_mut()
            .find(|existing| existing.path() == path)
        {
            Some(existing) => *existing = stylesheet,
            None => {
                let index = project
                    .stylesheets
                    .partition_point(|existing| existing.path() < path);
                project.stylesheets.insert(index, stylesheet);
            }
        }
        for warning in warnings {
            self.output.warning(format!(
                "Stylesheet `{}`: {}",
                warning.path, warning.message
            ));
        }
    }

    /// Makes `edits` to `id` and records them in the history. `key` is the
    /// field being edited, so a drag or typing keeps adding to one undo step.
    /// Only real changes are recorded: typing the same value again isn't one.
    ///
    /// During Play the panels show the game's world, so the edits go there
    /// instead: the game's scripts see them (`Changed` fires), there's no
    /// undo step, and Stop throws them away with the rest of the game.
    fn change(&mut self, id: InstanceId, key: Field, label: String, edits: Vec<Edit>) {
        if let Some(session) = &mut self.play {
            for edit in edits {
                if let Err(err) = set(session.world_mut(), id, edit) {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            return;
        }
        let before = history::snapshot(&self.world, id);
        for edit in edits {
            if let Err(err) = set(&mut self.world, id, edit) {
                // Can't happen from the panel or the gizmos, which only make valid values.
                self.output.error(format!("Could not change that: {err:#}"));
            }
        }
        let after = history::snapshot(&self.world, id);
        self.history
            .record(&mut self.world, id, key, label, before, after);
    }

    /// Like [`change`](Self::change), for edits to several Instances that
    /// make one undo step, such as painting the whole selection.
    fn change_many(&mut self, key: Field, label: String, edits: Vec<(InstanceId, Vec<Edit>)>) {
        if let Some(session) = &mut self.play {
            for (id, edits) in edits {
                for edit in edits {
                    if let Err(err) = set(session.world_mut(), id, edit) {
                        self.output.error(format!("Could not change that: {err:#}"));
                    }
                }
            }
            return;
        }
        let mut changed = Vec::new();
        for (id, edits) in edits {
            let before = history::snapshot(&self.world, id);
            for edit in edits {
                if let Err(err) = set(&mut self.world, id, edit) {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            let after = history::snapshot(&self.world, id);
            changed.push(history::Changed { id, before, after });
        }
        self.history
            .record_many(&mut self.world, key, label, changed);
    }

    /// Undoes the last step, unless a handle is being dragged or the game
    /// is playing (undo would change a world that isn't on screen).
    fn undo(&mut self) {
        if !self.viewport.is_dragging() && !self.playing() {
            self.history.undo(&mut self.world);
            self.forget_selection_outside_tree();
        }
    }

    fn redo(&mut self) {
        if !self.viewport.is_dragging() && !self.playing() {
            self.history.redo(&mut self.world);
            self.forget_selection_outside_tree();
        }
    }

    /// Clears the selection if it was taken out of the tree (deleted, or an
    /// insert undone), so nothing edits an Instance that isn't in the world.
    /// After Stop, that includes anything the game made.
    fn forget_selection_outside_tree(&mut self) {
        let world = shown(&self.world, &self.play);
        let root = world.root();
        self.selection
            .retain(|id| *id == root || world.is_descendant_of(*id, root));
        if self
            .selected
            .is_some_and(|id| !self.selection.contains(&id))
        {
            self.selected = self.selection.last().copied();
        }
    }

    /// Starts the game: saves the project if it has unsaved changes, then
    /// runs its scripts and physics on a copy of the world. If saving fails,
    /// Play doesn't start, so a crash during Play can't lose the changes.
    fn play(&mut self) {
        if self.playing() || self.viewport.is_dragging() || self.project.is_none() {
            return;
        }
        // A half-typed value is part of the world being saved, so end its step.
        self.history.seal();
        // The game has its own audio; a preview would play over it.
        self.sound_preview = None;
        if self.unsaved() && !self.save() {
            self.output
                .error("Play didn't start, because the project couldn't be saved first.");
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        // Show the game, not a script.
        self.tabs.show_world();
        self.output.info(format!(
            "Playing \"{}\". Press Stop (Shift+F5) to go back to editing; the world \
             will be as it was before Play.",
            project.name
        ));
        // `project` borrows `self.project` and `output` borrows `self.output`:
        // different fields, so Rust allows both borrows at once.
        let scripts = play::read_scripts(&project.folder, &project.scripts, &mut self.output);
        match Session::start(&self.world, &scripts, &mut self.output, self.assets.clone()) {
            Ok(session) => {
                self.play = Some(session);
                self.pending_ui_activation = None;
                self.focused_text = None;
                self.pending_focus_lost = None;
                self.last_frame = None;
                self.game_keys = GameKeys::default();
            }
            Err(crash) => self.crashed(crash),
        }
    }

    /// Stops the game and throws its world away. The world being edited was
    /// never touched, so it's back exactly as before Play.
    fn stop(&mut self) {
        if self.play.take().is_some() {
            self.output
                .info("Stopped. The world is back as it was before Play.");
            self.focused_text = None;
            self.pending_focus_lost = None;
            self.forget_selection_outside_tree();
        }
    }

    /// Runs one frame of the game, `seconds` long, with the keys pressed and
    /// released since the last one. LocalScripts see the viewport's camera
    /// as `workspace.CurrentCamera`, and can move it. A crash stops the game.
    fn play_frame(
        &mut self,
        seconds: f64,
        keys: &[KeyEvent],
        activation: Option<InstanceId>,
        focus_lost: Option<InstanceId>,
    ) {
        let Some(session) = &mut self.play else {
            return;
        };
        if let Some(button) = activation
            && let Err(crash) = session.activate_ui_button(button, &mut self.output)
        {
            self.crashed(crash);
            return;
        }
        if let Some(text_box) = focus_lost
            && let Err(crash) = session.release_text_box(text_box, &mut self.output)
        {
            self.crashed(crash);
            return;
        }
        let (position, orientation) = self.viewport.camera_view();
        let camera = CameraState {
            position,
            orientation,
        };
        match session.frame(seconds, keys, camera, &mut self.output) {
            Ok(Some(moved)) => self
                .viewport
                .set_camera_view(moved.position, moved.orientation),
            Ok(None) => {}
            Err(crash) => self.crashed(crash),
        }
    }

    fn update_block_lua(&mut self, path: &str) {
        let Some(source) = self.project.as_ref().and_then(|project| {
            let document = project
                .block_scripts
                .iter()
                .find(|document| document.path == path)?;
            Some(translate(&document.script).ok()?.source)
        }) else {
            return;
        };
        if let Some(project) = &mut self.project
            && let Some(script) = project
                .scripts
                .iter_mut()
                .find(|script| script.path == path)
        {
            script.source = source.clone();
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = source;
        }
    }

    /// Works out which scripts use an API that only works on the other side
    /// (from the code in their tab, if open), for the warning badges in the
    /// Explorer and on the tabs.
    fn update_warnings(&mut self) {
        let mut badges = HashMap::new();
        let mut by_path = HashMap::new();
        if let Some(project) = &self.project {
            for script in &project.scripts {
                let source = self
                    .tabs
                    .get(&script.path)
                    .map_or(script.source.as_str(), |tab| tab.text.as_str());
                let warnings = self.warnings.get(&script.path, source);
                if !warnings.is_empty() {
                    badges.insert(script.instance, side_warnings::badge_text(warnings));
                    by_path.insert(script.path.clone(), warnings.to_vec());
                }
            }
        }
        self.explorer.warnings = badges;
        self.tabs.warnings = by_path;
    }

    /// The engine panicked during Play: say so, and go back to editing.
    fn crashed(&mut self, crash: Crash) {
        self.play = None;
        self.output.error(format!(
            "The engine crashed during Play: {}. Play stopped, and the world is back \
             as it was before Play. This is a bug in Axcenity; if you started the \
             editor from a terminal, it shows where it happened.",
            crash.0
        ));
        self.forget_selection_outside_tree();
    }

    /// Does something to the tree from the Explorer, the toolbar or a
    /// shortcut. Ignored while a part is being dragged in the viewport, and
    /// during Play.
    fn act(&mut self, action: Action) {
        if self.viewport.is_dragging() || self.playing() {
            return;
        }
        let is_script = |id| {
            self.world
                .get(id)
                .is_some_and(|instance| instance.class().is_script())
        };
        match action {
            Action::Insert { what, near } => self.insert(what, near),
            Action::InsertScript(class) => self.insert_script(class),
            // A script's name is its file's name, so the file is renamed.
            Action::Rename(id, name) if is_script(id) => self.rename_script(id, &name),
            Action::Rename(id, name) => {
                self.apply(id, Edit::Name(name));
                // Typing in the Explorer is done, so this is one whole step.
                self.history.seal();
            }
            Action::Duplicate(id) => {
                if self.selection.contains(&id) && self.selection.len() > 1 {
                    self.duplicate_selected();
                } else {
                    self.duplicate(id);
                }
            }
            // Deleting a script deletes its file, which can't be undone, so ask first.
            Action::Delete(id) if is_script(id) => self.confirm_delete = Some(id),
            Action::Delete(id) if self.selection.contains(&id) && self.selection.len() > 1 => {
                self.delete_selected();
            }
            Action::Delete(id) => self.delete(id),
            Action::MoveInto { id, parent } => {
                self.move_into(id, parent);
            }
        }
    }

    /// Where a new part goes: the camera's focus point, on the move grid
    /// when snapping is on, so new parts line up.
    fn insert_position(&self) -> Vector3 {
        let focus = self.viewport.focus();
        // Rounded to 3 decimals so the numbers stay tidy; `+ 0.0` turns -0 into 0.
        let tidy = |value: f32| (self.snap.studs(value) * 1000.0).round() / 1000.0 + 0.0;
        Vector3::new(tidy(focus.x), tidy(focus.y), tidy(focus.z))
    }

    /// Inserts `what` where [`tree::insert_parent`] says, with a name no
    /// sibling has, and selects it. A part goes at the camera's focus point.
    ///
    /// UI elements only show inside a ScreenGui, so inserting one where
    /// there's none makes a ScreenGui for it first.
    fn insert(&mut self, what: Insertable, near: Option<InstanceId>) {
        if matches!(what, Insertable::PointLight | Insertable::SpotLight)
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            self.insert_named(
                Insertable::Part(PartType::Ball),
                near.or(self.selected),
                "Lamp",
                vec![
                    ("Anchored", Value::Bool(true)),
                    ("Material", Value::Material(Material::Neon)),
                    ("Size", Value::Vector3(Vector3::new(1.2, 1.2, 1.2))),
                    ("Color", Value::Color3(Color3::new(1.0, 0.86, 0.55))),
                ],
            );
            let lamp = self.selected;
            self.insert_named(what, lamp, what.class().as_str(), Vec::new());
            return;
        }
        if what == Insertable::Page {
            let layout =
                tree::page_layout_for(&self.world, near.or(self.selected)).unwrap_or_else(|| {
                    self.insert(Insertable::PageLayout, near);
                    self.selected.expect("inserting a PageLayout selects it")
                });
            self.insert_named(Insertable::Frame, Some(layout), "Page", Vec::new());
            return;
        }
        if what == Insertable::BillboardGui
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            self.output
                .warning("BillboardGui shows when it is inside a part.");
        }
        if tree::needs_screen_gui(&self.world, near, what) {
            self.insert_named(Insertable::ScreenGui, None, "ScreenGui", Vec::new());
            let gui = self.selected;
            self.insert_named(what, gui, what.class().as_str(), Vec::new());
            return;
        }
        self.insert_named(what, near, what.class().as_str(), Vec::new());
    }

    /// Like [`insert`](Self::insert), but named `wanted` (with a number on
    /// the end if a sibling has that name) and with `properties` set.
    fn insert_named(
        &mut self,
        what: Insertable,
        near: Option<InstanceId>,
        wanted: &str,
        properties: Vec<(&'static str, Value)>,
    ) {
        let parent = tree::insert_parent(&self.world, near, what);
        // Worked out first: `create` borrows the world mutably, and
        // `insert_position` needs to borrow `self` while that lasts.
        let position = self.insert_position();
        let id = tree::create(&mut self.world, what, position);
        for (property, value) in properties {
            self.world
                .set_property(id, property, value)
                .expect("callers pass properties the class has");
        }
        let name = tree::unique_name(&self.world, parent, wanted);
        self.world.set_name(id, &name).expect("just created");
        let end = self.world.get(parent).map_or(0, |p| p.children().len());
        let place = Place { parent, index: end };
        if self.place(id, Some(place), format!("Insert {name}")) {
            self.select_from_viewport(Some(id));
        }
    }

    /// Takes `id` and everything inside it out of the world, as one undo step.
    fn delete(&mut self, id: InstanceId) {
        let Some(name) = self.editable_name(id, "delete") else {
            return;
        };
        if self.place(id, None, format!("Delete {name}")) {
            self.forget_selection_outside_tree();
        }
    }

    /// Copies `id` and everything inside it, puts the copy right after it
    /// with a name no sibling has, and selects the copy.
    fn duplicate(&mut self, id: InstanceId) -> Option<InstanceId> {
        let Some(name) = self.editable_name(id, "duplicate") else {
            return None;
        };
        let Some(original) = history::place_of(&self.world, id) else {
            return None;
        };
        let copy = self.world.duplicate(id).expect("checked it can be edited");
        let copy_name = tree::unique_name(&self.world, original.parent, &name);
        self.world.set_name(copy, copy_name).expect("just made");
        let after = Place {
            parent: original.parent,
            index: original.index + 1,
        };
        if self.place(copy, Some(after), format!("Duplicate {name}")) {
            self.select_from_viewport(Some(copy));
            return Some(copy);
        }
        None
    }

    /// Duplicates every selected Instance that isn't inside another one, and
    /// selects the copies.
    fn duplicate_selected(&mut self) {
        let ids = self.selection_roots();
        let mut copies = Vec::new();
        for id in ids.into_iter().rev() {
            if let Some(copy) = self.duplicate(id) {
                copies.push(copy);
            }
        }
        if copies.is_empty() {
            return;
        }
        copies.reverse();
        let primary = *copies.last().unwrap();
        self.selection = copies;
        self.selected = Some(primary);
        self.explorer.reveal(&self.world, primary);
    }

    /// Deletes every selected Instance that isn't inside another one. One
    /// script still goes through the "delete its file?" question. Several
    /// scripts are left, and a note says to delete those one at a time.
    fn delete_selected(&mut self) {
        let ids = self.selection_roots();
        if ids.len() == 1
            && self
                .world
                .get(ids[0])
                .is_some_and(|instance| instance.class().is_script())
        {
            self.confirm_delete = Some(ids[0]);
            return;
        }
        let mut skipped_script = false;
        for id in ids {
            if self
                .world
                .get(id)
                .is_some_and(|instance| instance.class().is_script())
            {
                skipped_script = true;
                continue;
            }
            self.delete(id);
        }
        if skipped_script {
            self.output.info(
                "Scripts are deleted one at a time, from their row's menu, so the file can be removed too.",
            );
        }
    }

    /// Moves `id` into `parent` as its last child, as one undo step. Moves
    /// into itself, into something inside it, or out of the Workspace are
    /// rejected with the reason in Output. Returns true if it moved.
    fn move_into(&mut self, id: InstanceId, parent: InstanceId) -> bool {
        let (Some(instance), Some(target)) = (self.world.get(id), self.world.get(parent)) else {
            return false;
        };
        let (name, target_name) = (instance.name().to_string(), target.name().to_string());
        if let Err(reason) = tree::check_move(&self.world, id, parent) {
            self.output
                .error(format!("Can't move {name} into {target_name}: {reason}"));
            return false;
        }
        if instance.parent() == Some(parent) {
            return false;
        }
        let place = Place {
            parent,
            index: target.children().len(),
        };
        let label = format!("Move {name} into {target_name}");
        let moved = self.place(id, Some(place), label);
        if moved {
            // Opens the new parent, so the moved row stays in view.
            self.explorer.reveal(&self.world, id);
        }
        moved
    }

    /// `id`'s name, if it can be deleted, duplicated or moved. Otherwise
    /// says why not in Output (`doing` is "delete", say) and returns `None`.
    fn editable_name(&mut self, id: InstanceId, doing: &str) -> Option<String> {
        let name = self.world.get(id)?.name().to_string();
        match tree::check_editable(&self.world, id) {
            Ok(()) => Some(name),
            Err(reason) => {
                self.output.error(format!("Can't {doing} {name}: {reason}"));
                None
            }
        }
    }

    /// Puts `id` at `place` in the tree (`None` takes it out), and records
    /// that as an undo step named `label`. Returns false, with the reason in
    /// Output, if the tree said no.
    fn place(&mut self, id: InstanceId, place: Option<Place>, label: String) -> bool {
        let before = history::place_of(&self.world, id);
        if let Err(err) = history::put(&mut self.world, id, place) {
            self.output.error(format!("Could not do that: {err}"));
            return false;
        }
        let moves = vec![Move {
            id,
            before,
            after: place,
        }];
        self.history.record_tree(&mut self.world, label, moves);
        true
    }

    /// Asks to `discard` the open world. Returns it when it can go ahead now,
    /// or `None` after putting up "Save changes?" because there are unsaved changes.
    fn request(&mut self, discard: Discard) -> Option<Discard> {
        let unsaved = match &discard {
            Discard::CloseTab(path) => {
                self.tabs.get(path).is_some_and(|tab| tab.unsaved())
                    || self.project.as_ref().is_some_and(|project| {
                        project.block_scripts.iter().any(|document| {
                            document.path == *path && document.script != document.saved
                        })
                    })
            }
            Discard::Quit | Discard::OpenAnother | Discard::CloseProject => self.unsaved(),
        };
        if unsaved {
            self.asking = Some(discard);
            None
        } else {
            Some(discard)
        }
    }

    /// Takes the answer to "Save changes?" and returns what to go ahead with,
    /// if anything. A failed save goes nowhere, so the changes aren't lost.
    fn answer(&mut self, answer: Answer) -> Option<Discard> {
        let discard = self.asking.take()?;
        match answer {
            // Closing a tab only needs that script saved.
            Answer::Save => match &discard {
                Discard::CloseTab(path) => self.save_tab(path).then_some(discard),
                Discard::Quit | Discard::OpenAnother | Discard::CloseProject => {
                    self.save().then_some(discard)
                }
            },
            Answer::DontSave => {
                if let Discard::CloseTab(path) = &discard {
                    let source = self
                        .project
                        .as_mut()
                        .and_then(|project| {
                            project
                                .block_scripts
                                .iter_mut()
                                .find(|document| document.path == *path)
                        })
                        .map(|document| {
                            document.script = document.saved.clone();
                            translate(&document.script)
                        });
                    match source {
                        Some(Ok(translation)) => {
                            if let Some(script) = self.project.as_mut().and_then(|project| {
                                project
                                    .scripts
                                    .iter_mut()
                                    .find(|script| script.path == *path)
                            }) {
                                script.source = translation.source;
                            }
                        }
                        Some(Err(err)) => self.output.error(format!(
                            "Could not restore the saved blocks in `{path}`: {err}"
                        )),
                        None => {}
                    }
                }
                Some(discard)
            }
            Answer::Cancel => None,
        }
    }

    fn go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        match discard {
            Discard::Quit => {
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Discard::OpenAnother => self.open_with_dialog(),
            Discard::CloseProject => self.close_project(),
            Discard::CloseTab(path) => self.tabs.close(&path),
        }
    }

    /// Goes back to the start screen. The project's thumbnail is taken first
    /// if the viewport is showing, and the project closes once it's saved.
    fn close_project(&mut self) {
        self.thumbnail_wanted = Some(true);
    }

    /// Puts the open project away and shows the start screen.
    fn finish_closing_project(&mut self) {
        self.project = None;
        self.play = None;
        self.pending_ui_activation = None;
        self.focused_text = None;
        self.pending_focus_lost = None;
        self.thumbnail = None;
        self.thumbnail_wanted = None;
        self.assets = Assets::default();
        self.asset_browser.open(None);
        self.tabs.close_all();
        self.replace_world(World::new());
        self.start.reload_thumbnails();
    }

    /// Asks for a screenshot when a thumbnail is wanted and the viewport is
    /// showing. Without a viewport to photograph, a pending close just happens.
    fn start_thumbnail(&mut self, ctx: &egui::Context) {
        let Some(then_close) = self.thumbnail_wanted.take() else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match self.viewport_rect {
            Some(area) if !self.playing() => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.thumbnail = Some(ThumbnailShot {
                    folder,
                    area,
                    then_close,
                });
            }
            _ if then_close => self.finish_closing_project(),
            _ => {}
        }
    }

    /// Saves the screenshot asked for by [`Self::start_thumbnail`], when it arrives.
    fn finish_thumbnail(&mut self, ctx: &egui::Context) {
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let (Some(image), Some(shot)) = (screenshot, self.thumbnail.take()) else {
            return;
        };
        let png = thumbnail::encode(&image, shot.area, ctx.pixels_per_point());
        if let Some(png) = png
            && let Err(err) =
                std::fs::write(shot.folder.join(axcenity_project::THUMBNAIL_FILE), png)
        {
            log::warn!("could not save the project thumbnail: {err}");
        }
        if shot.then_close {
            self.finish_closing_project();
        }
    }

    /// `request` then, if nothing needs asking, `go_ahead`.
    fn request_and_go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        if let Some(discard) = self.request(discard) {
            self.go_ahead(ctx, discard);
        }
    }

    /// Shows "Save changes?" while something is waiting for an answer.
    fn unsaved_changes_prompt(&mut self, ctx: &egui::Context) {
        let Some(discard) = &self.asking else {
            return;
        };
        let project = self.project.as_ref().map_or("", |project| &project.name);
        let (name, doing) = match discard {
            Discard::Quit => (project, "closing"),
            Discard::OpenAnother => (project, "opening another project"),
            Discard::CloseProject => (project, "closing it"),
            Discard::CloseTab(path) => (axcenity_project::script_name(path), "closing it"),
        };
        // What isn't saved: only the tab, when closing a tab.
        let mut items = Vec::new();
        if !matches!(discard, Discard::CloseTab(_)) {
            if !self.history.is_saved() {
                items.push((icons::GLOBE_HEMISPHERE_WEST, "The world".to_owned()));
            }
            for path in self.tabs.unsaved_paths() {
                items.push((icons::FILE_CODE, path));
            }
            if let Some(project) = &self.project {
                for document in &project.block_scripts {
                    if document.script != document.saved {
                        items.push((icons::PUZZLE_PIECE, document.path.clone()));
                    }
                }
            }
        }
        let (icon, accent) = dialog::unsaved_style();
        let message = format!("\"{name}\" has unsaved changes. Save them before {doing}?");
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "unsaved changes",
                icon,
                accent,
                title: "Save changes?",
                message: &message,
                items,
            },
            &[
                dialog::Button {
                    icon: icons::X,
                    label: "Cancel",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::TRASH,
                    label: "Don't Save",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::FLOPPY_DISK,
                    label: "Save",
                    kind: dialog::Kind::Main,
                },
            ],
        );
        let answer = outcome.map(|outcome| match outcome {
            dialog::Outcome::Picked(2) => Answer::Save,
            dialog::Outcome::Picked(1) => Answer::DontSave,
            _ => Answer::Cancel,
        });
        if let Some(answer) = answer
            && let Some(discard) = self.answer(answer)
        {
            self.go_ahead(ctx, discard);
        }
    }

    fn title(&self) -> String {
        // A `*` after the name means unsaved changes, as in most editors.
        let unsaved = if self.unsaved() { "*" } else { "" };
        match &self.project {
            Some(project) => format!("{}{unsaved} - Axcenity Editor", project.name),
            None => "Axcenity Editor".to_string(),
        }
    }

    /// Fills the window under the menu bar with the start screen, and opens
    /// what's picked there.
    fn start_screen(&mut self, ctx: &egui::Context) {
        let Some(brand) = &self.brand else {
            return;
        };
        let picked = egui::CentralPanel::default()
            .show(ctx, |ui| self.start.show(ui, brand, &mut self.recent))
            .inner;
        match picked {
            Some(StartAction::OpenDialog) => self.open_with_dialog(),
            Some(StartAction::Open(folder)) => self.open(&folder),
            None => return,
        }
        // Nothing opened, so say why here: Output isn't shown without a project.
        if self.project.is_none()
            && let Some(line) = self.output.lines().last()
            && line.kind == OutputKind::Error
        {
            self.start.set_notice(line.text.clone());
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .button(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new(theme::with_icon(icons::HOUSE, "Close Project")),
                    )
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::CloseProject);
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new("Open Stylesheet…"),
                    )
                    .clicked()
                {
                    self.open_stylesheet_with_dialog();
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new("New Stylesheet"),
                    )
                    .clicked()
                {
                    self.new_stylesheet();
                }
                let save = egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save"))
                    .shortcut_text(ui.ctx().format_shortcut(&SAVE_SHORTCUT));
                if ui.add_enabled(self.project.is_some(), save).clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new(theme::with_icon(
                            icons::CLOCK_COUNTER_CLOCKWISE,
                            "Version History…",
                        )),
                    )
                    .clicked()
                {
                    self.versions_open = true;
                }
                let export_enabled = self.project.is_some() && !self.playing();
                ui.menu_button(
                    theme::with_icon(icons::FILE_ARROW_UP, "Export Game"),
                    |ui| {
                        if ui
                            .add_enabled(export_enabled, egui::Button::new("For this OS"))
                            .clicked()
                        {
                            self.export_game_with_dialog();
                            ui.close();
                        }
                        ui.separator();
                        for (target, label) in [
                            (files::ExportTarget::Windows, "For Windows"),
                            (files::ExportTarget::MacOS, "For macOS"),
                            (files::ExportTarget::Linux, "For Linux"),
                        ] {
                            if ui
                                .add_enabled(export_enabled, egui::Button::new(label))
                                .clicked()
                            {
                                self.export_game_for_with_dialog(target);
                                ui.close();
                            }
                        }
                        if !export_enabled {
                            ui.weak(if self.playing() {
                                "Press Stop before exporting."
                            } else {
                                "Open a project first."
                            });
                        }
                        ui.separator();
                        let mut show_splash = self
                            .project
                            .as_ref()
                            .is_some_and(|project| project.show_splash);
                        if ui
                            .add_enabled(
                                self.project.is_some(),
                                egui::Checkbox::new(
                                    &mut show_splash,
                                    "Show splash in exported games",
                                ),
                            )
                            .changed()
                        {
                            self.set_show_splash(show_splash);
                        }
                    },
                );
                ui.separator();
                let import =
                    egui::Button::new(theme::with_icon(icons::FILE_ARROW_DOWN, "Import Model…"));
                if ui
                    .add_enabled(self.project.is_some() && !self.playing(), import)
                    .on_hover_text(IMPORT_HOVER)
                    .on_disabled_hover_text(if self.playing() {
                        "Press Stop to edit."
                    } else {
                        "Open a project first."
                    })
                    .clicked()
                {
                    self.import_model_with_dialog();
                }
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::new(theme::with_icon(
                            icons::SPARKLE,
                            "Set Up VS Code Autocomplete",
                        )),
                    )
                    .on_hover_text(
                        "Add the engine's API to this project for Lua Language Server, \
                         so VS Code autocompletes it in scripts.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked()
                {
                    self.set_up_autocomplete();
                }
                ui.separator();
                if ui
                    .button(theme::with_icon(icons::SIGN_OUT, "Quit"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                // Nothing can be edited mid-drag, or while the game plays.
                let busy = self.viewport.is_dragging() || self.playing();
                // "Undo Move Pad", so it's clear what will be undone.
                let undo_label = self.history.undo_label().map(str::to_string);
                let undo = egui::Button::new(theme::with_icon(
                    icons::ARROW_COUNTER_CLOCKWISE,
                    &match &undo_label {
                        Some(label) => format!("Undo {label}"),
                        None => "Undo".to_string(),
                    },
                ))
                .shortcut_text(ui.ctx().format_shortcut(&UNDO_SHORTCUT));
                if ui
                    .add_enabled(undo_label.is_some() && !busy, undo)
                    .clicked()
                {
                    self.undo();
                }
                let redo_label = self.history.redo_label().map(str::to_string);
                let redo = egui::Button::new(theme::with_icon(
                    icons::ARROW_CLOCKWISE,
                    &match &redo_label {
                        Some(label) => format!("Redo {label}"),
                        None => "Redo".to_string(),
                    },
                ))
                .shortcut_text(ui.ctx().format_shortcut(&REDO_SHORTCUT));
                if ui
                    .add_enabled(redo_label.is_some() && !busy, redo)
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();

                // These act on the selection, the same as its right-click menu in the Explorer.
                let selected = self.selected;
                let renamable = selected
                    .and_then(|id| self.world.get(id))
                    .is_some_and(properties::can_rename)
                    && !self.playing();
                let editable = selected
                    .is_some_and(|id| tree::check_editable(&self.world, id).is_ok())
                    && !busy;
                let rename = egui::Button::new(theme::with_icon(icons::PENCIL_SIMPLE, "Rename"))
                    .shortcut_text(RENAME_KEY.name());
                if ui.add_enabled(renamable, rename).clicked()
                    && let Some(id) = selected
                {
                    self.explorer.start_rename(&self.world, id);
                }
                let duplicate = egui::Button::new(theme::with_icon(icons::COPY, "Duplicate"))
                    .shortcut_text(ui.ctx().format_shortcut(&DUPLICATE_SHORTCUT));
                if ui.add_enabled(editable, duplicate).clicked() && selected.is_some() {
                    self.duplicate_selected();
                }
                let deletable =
                    selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
                let delete = egui::Button::new(theme::with_icon(icons::TRASH, "Delete"))
                    .shortcut_text(DELETE_KEY_TEXT);
                if ui.add_enabled(deletable, delete).clicked() && selected.is_some() {
                    self.delete_selected();
                }
            });
            ui.menu_button("View", |ui| {
                ui.weak("Theme");
                for (preference, icon, label) in [
                    (egui::ThemePreference::Dark, icons::MOON, "Dark"),
                    (egui::ThemePreference::Light, icons::SUN, "Light"),
                    (
                        egui::ThemePreference::System,
                        icons::DESKTOP,
                        "Match System",
                    ),
                ] {
                    let item = egui::Button::selectable(
                        self.theme == preference,
                        theme::with_icon(icon, label),
                    );
                    if ui.add(item).clicked() {
                        self.theme = preference;
                        ui.ctx().set_theme(preference);
                    }
                }
            });
            // Only in debug builds (`cargo run`), never in a release.
            if cfg!(debug_assertions) {
                ui.menu_button("Debug", |ui| {
                    let panic =
                        egui::Button::new(theme::with_icon(icons::WARNING, "Panic during Play"));
                    if ui
                        .add_enabled(self.playing(), panic)
                        .on_hover_text(
                            "Makes the engine panic on the next frame, to check that \
                             Play stops and reports it instead of closing the editor.",
                        )
                        .on_disabled_hover_text("Press Play first.")
                        .clicked()
                        && let Some(session) = &mut self.play
                    {
                        session.panic_next_frame();
                    }
                });
            }
            ui.menu_button("Help", |ui| {
                if ui
                    .button(theme::with_icon(icons::INFO, "About Axcenity"))
                    .clicked()
                {
                    self.about_open = true;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.unsaved() {
                    ui.weak(theme::with_icon(icons::FLOPPY_DISK, "Unsaved changes"));
                }
                if self.playing() {
                    playing_badge(ui);
                }
            });
        });
    }

    /// The ribbon under the menu bar: Play and Stop, the tools, the snapping
    /// settings, then buttons that insert each primitive and each kind of script.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let mut insert = None;
        let mut insert_script = None;
        let mut import = false;
        let (mut play, mut stop) = (false, false);
        let playing = self.playing();
        let can_play = self.project.is_some();
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ribbon::group(ui, "Game", |ui| {
                    let play_button = RibbonButton::new(filled_icons::PLAY, "Play")
                        .family(theme::filled())
                        .color(theme::PLAY_GREEN);
                    play = ui
                        .add_enabled(!playing && can_play, play_button)
                        .on_hover_text(format!(
                            "Save, then run the game here ({})",
                            ui.ctx().format_shortcut(&PLAY_SHORTCUT)
                        ))
                        .on_disabled_hover_text(if playing {
                            "The game is playing."
                        } else {
                            "Open a project first."
                        })
                        .clicked();
                    let stop_button = RibbonButton::new(filled_icons::STOP, "Stop")
                        .family(theme::filled())
                        .color(theme::STOP_RED);
                    stop = ui
                        .add_enabled(playing, stop_button)
                        .on_hover_text(format!(
                            "Stop the game and go back to editing ({})",
                            ui.ctx().format_shortcut(&STOP_SHORTCUT)
                        ))
                        .clicked();
                });
                ui.separator();

                ribbon::group(ui, "Tools", |ui| {
                    for tool in Tool::ALL {
                        let keys: Vec<String> = tool
                            .shortcut()
                            .map(|shortcut| ui.ctx().format_shortcut(&shortcut))
                            .into_iter()
                            .chain(tool.key().map(|key| key.name().to_owned()))
                            .collect();
                        let hover = match keys.as_slice() {
                            [] => format!("{} tool", tool.label()),
                            keys => format!("{} tool ({})", tool.label(), keys.join(" or ")),
                        };
                        let button = RibbonButton::new(theme::tool_icon(tool), tool.label())
                            .selected(self.tool == tool);
                        if ui
                            .add(button)
                            .on_hover_text(hover)
                            .clicked()
                        {
                            self.tool = tool;
                        }
                    }
                });
                ui.separator();

                ribbon::group(ui, "Snapping", |ui| {
                    let snap = &mut self.snap;
                    egui::Grid::new("snapping")
                        .num_columns(2)
                        .spacing([6.0, 4.0])
                        .show(ui, |ui| {
                            ui.checkbox(
                                &mut snap.move_on,
                                theme::with_icon(icons::GRID_FOUR, "Move"),
                            )
                            .on_hover_text("Move and scale in steps of this many studs.");
                            ui.add_enabled(
                                snap.move_on,
                                egui::DragValue::new(&mut snap.move_step)
                                    .range(Snap::MIN_MOVE_STEP..=1000.0)
                                    .speed(0.05)
                                    .max_decimals(3)
                                    .suffix(" studs"),
                            );
                            ui.end_row();
                            ui.checkbox(
                                &mut snap.rotate_on,
                                theme::with_icon(icons::ANGLE, "Rotate"),
                            )
                            .on_hover_text("Rotate in steps of this many degrees.");
                            ui.add_enabled(
                                snap.rotate_on,
                                egui::DragValue::new(&mut snap.rotate_step)
                                    .range(Snap::MIN_ROTATE_STEP..=180.0)
                                    .speed(0.5)
                                    .max_decimals(2)
                                    .suffix("°"),
                            );
                            ui.end_row();
                        });
                });
                ui.separator();

                ribbon::group(ui, "Insert", |ui| {
                    for what in Insertable::PARTS {
                        let hover = format!(
                            "Insert a {} part at the camera's focus point, inside the selected \
                             Model or Folder if there is one.",
                            what.label().to_lowercase()
                        );
                        let color = theme::class_color(what.class(), ui.visuals());
                        let button =
                            RibbonButton::new(theme::insert_icon(what), what.label()).color(color);
                        if ui
                            .add_enabled(!playing, button)
                            .on_hover_text(hover)
                            .on_disabled_hover_text("Press Stop to edit.")
                            .clicked()
                        {
                            insert = Some(what);
                        }
                    }
                    for what in Insertable::AUDIO {
                        let button = RibbonButton::new(theme::insert_icon(what), what.label())
                            .color(theme::class_color(what.class(), ui.visuals()));
                        if ui
                            .add_enabled(!playing, button)
                            .on_hover_text(
                                "Insert a Sound into the selected part or the Workspace.",
                            )
                            .on_disabled_hover_text("Press Stop to edit.")
                            .clicked()
                        {
                            insert = Some(what);
                        }
                    }
                    for what in Insertable::LIGHTS {
                        let button = RibbonButton::new(theme::insert_icon(what), what.label())
                            .color(theme::class_color(what.class(), ui.visuals()));
                        if ui
                            .add_enabled(!playing, button)
                            .on_hover_text("Insert a light into the selected part.")
                            .on_disabled_hover_text("Press Stop to edit.")
                            .clicked()
                        {
                            insert = Some(what);
                        }
                    }
                    let color = theme::class_color(ClassName::MeshPart, ui.visuals());
                    let button = RibbonButton::new(icons::FILE_ARROW_DOWN, "Import").color(color);
                    import = ui
                        .add_enabled(!playing, button)
                        .on_hover_text(IMPORT_HOVER)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked();
                });
                ui.separator();

                ribbon::group(ui, "UI", |ui| {
                    for what in Insertable::UI {
                        let label = match what {
                            Insertable::ScreenGui => "Screen",
                            Insertable::Frame => "Frame",
                            Insertable::TextLabel => "Text",
                            Insertable::TextButton => "Button",
                            Insertable::ImageLabel => "Image",
                            Insertable::TextBox => "TextBox",
                            Insertable::PageLayout => "Pages",
                            Insertable::Page => "Page",
                            Insertable::Table => "Table",
                            Insertable::BillboardGui => "Billboard",
                            _ => what.label(),
                        };
                        let hover = match what {
                            Insertable::ScreenGui => {
                                "Insert a ScreenGui: a layer of UI over the game.".to_owned()
                            }
                            Insertable::PageLayout => {
                                "Insert a PageLayout. Its child Frames are pages; a script or the \
                                 show-page block sets CurrentPage."
                                    .to_owned()
                            }
                            Insertable::Page => {
                                "Insert a page Frame into the selected PageLayout (a PageLayout \
                                 and a ScreenGui are made if there isn't one)."
                                    .to_owned()
                            }
                            Insertable::BillboardGui => {
                                "Insert a BillboardGui into the selected part. It floats in the \
                                 world and is drawn with the screen UI."
                                    .to_owned()
                            }
                            _ => format!(
                                "Insert a {} into the selected ScreenGui or Frame (or the first \
                                 ScreenGui; one is made if there's none). Drag its handles in the \
                                 viewport, and style it with CSS in the Style section of Properties.",
                                what.label()
                            ),
                        };
                        let button = RibbonButton::new(theme::insert_icon(what), label)
                            .color(theme::class_color(what.class(), ui.visuals()));
                        if ui
                            .add_enabled(!playing, button)
                            .on_hover_text(hover)
                            .on_disabled_hover_text("Press Stop to edit.")
                            .clicked()
                        {
                            insert = Some(what);
                        }
                    }
                });
                ui.separator();

                ribbon::group(ui, "Scripts", |ui| {
                    for class in explorer::SCRIPT_CLASSES {
                        let color = theme::class_color(class, ui.visuals());
                        let button = RibbonButton::new(theme::class_icon(class), class.as_str())
                            .color(color);
                        if ui
                            .add_enabled(!playing && can_play, button)
                            .on_hover_text(explorer::script_hover(class))
                            .on_disabled_hover_text(if playing {
                                "Press Stop to edit."
                            } else {
                                "Open a project first."
                            })
                            .clicked()
                        {
                            insert_script = Some(class);
                        }
                    }
                });

            });
        });
        if let Some(what) = insert {
            let near = self.selected;
            self.act(Action::Insert { what, near });
        }
        if let Some(class) = insert_script {
            self.act(Action::InsertScript(class));
        }
        if import {
            self.import_model_with_dialog();
        }
        if play {
            self.play();
        }
        if stop {
            self.stop();
        }
    }

    /// The keyboard shortcuts for undo, redo, the tools, and renaming,
    /// duplicating and deleting the selection.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        // Stop first: egui counts Shift+F5 as F5 too.
        let (stop, play) = ctx.input_mut(|input| {
            let stop = input.consume_shortcut(&STOP_SHORTCUT);
            (stop, input.consume_shortcut(&PLAY_SHORTCUT))
        });
        if stop {
            self.stop();
        } else if play {
            self.play();
        }

        // A text box being typed in has its own undo, and uses Delete and
        // Backspace itself, so leave those keys to it then.
        let typing = ctx.wants_keyboard_input();
        let playing = self.playing();
        let studio_tool = |input: &mut egui::InputState| {
            Tool::ALL.into_iter().find(|tool| {
                tool.shortcut()
                    .is_some_and(|shortcut| input.consume_shortcut(&shortcut))
            })
        };
        // F3X's single keys only while editing, so a game gets every key.
        let f3x_tool = |input: &mut egui::InputState| {
            if playing {
                return None;
            }
            Tool::ALL.into_iter().find(|tool| {
                tool.key()
                    .is_some_and(|key| input.consume_key(egui::Modifiers::NONE, key))
            })
        };
        let keys = ctx.input_mut(|input| {
            if typing {
                return Keys {
                    tool: studio_tool(input),
                    ..Keys::default()
                };
            }
            // Redo first: egui counts Ctrl+Shift+Z as Ctrl+Z too.
            let redo = input.consume_shortcut(&REDO_SHIFT_SHORTCUT)
                || input.consume_shortcut(&REDO_SHORTCUT);
            let none = egui::Modifiers::NONE;
            Keys {
                redo,
                undo: input.consume_shortcut(&UNDO_SHORTCUT),
                tool: studio_tool(input).or_else(|| f3x_tool(input)),
                toggle: !playing && input.consume_key(none, egui::Key::Enter),
                // Taken before the viewport sees them, so Ctrl+D doesn't
                // also fly the camera right.
                duplicate: input.consume_shortcut(&DUPLICATE_SHORTCUT),
                rename: input.consume_key(none, RENAME_KEY),
                delete: input.consume_key(none, egui::Key::Delete)
                    || (cfg!(target_os = "macos") && input.consume_key(none, egui::Key::Backspace)),
            }
        });
        if keys.redo {
            self.redo();
        }
        if keys.undo {
            self.undo();
        }
        if let Some(tool) = keys.tool {
            self.tool = tool;
        }
        if keys.toggle && matches!(self.tool, Tool::Anchor | Tool::Collision) {
            let property = if self.tool == Tool::Anchor {
                "Anchored"
            } else {
                "CanCollide"
            };
            let parts = self.tool_parts();
            let all_on = parts.iter().all(|&id| {
                self.world
                    .get(id)
                    .and_then(|i| i.property(property))
                    .and_then(Value::as_bool)
                    != Some(false)
            });
            self.set_on_parts(&parts, property, Value::Bool(!all_on));
            self.history.seal();
        }
        // Undo, duplicate and delete check for Play themselves; renaming
        // only opens a text box, so it's checked here.
        if let Some(id) = self.selected {
            if keys.rename && !self.playing() {
                self.explorer.start_rename(&self.world, id);
            }
            if keys.duplicate {
                self.duplicate_selected();
            }
            if keys.delete {
                self.delete_selected();
            }
        }
    }

    /// File > Version History: the saves kept for this project, and restoring one.
    fn version_window(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.versions_open = false;
            return;
        };
        let mut open = self.versions_open;
        let mut restore = None;
        let mut checkpoint = false;
        egui::Window::new("Version History")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Each save keeps the world, scripts and styles. Restoring puts that \
                     version back. Assets and the project name stay as they are.",
                );
                ui.add_space(6.0);
                if ui.button("Save a version now").clicked() {
                    checkpoint = true;
                }
                ui.add_space(6.0);
                let versions = axcenity_project::list_versions(&folder);
                if versions.is_empty() {
                    ui.weak("No versions yet. Save the project to keep the first one.");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for version in &versions {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(version.label.as_str());
                                    ui.weak(version.when());
                                });
                                if ui.button("Restore").clicked() {
                                    restore = Some(version.id.clone());
                                }
                            });
                            ui.separator();
                        }
                    });
            });
        self.versions_open = open;
        if checkpoint {
            if self.unsaved() {
                self.save();
            } else {
                match axcenity_project::record_version(&folder, "Checkpoint") {
                    Ok(true) => self
                        .output
                        .info("Kept a version of the world, scripts and styles."),
                    Ok(false) => self
                        .output
                        .info("The newest version already matches these files."),
                    Err(err) => self
                        .output
                        .warning(format!("Could not keep a version: {err}")),
                }
            }
        }
        if let Some(id) = restore {
            self.restore_version(&id);
        }
    }

    /// Puts version `id` back, keeping the current files as a version first.
    fn restore_version(&mut self, id: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.unsaved() && !self.save() {
            return;
        }
        if let Err(err) = axcenity_project::record_version(&folder, "Before restore") {
            self.output.warning(format!(
                "Could not keep the current files before restoring: {err}"
            ));
        }
        match axcenity_project::restore_version(&folder, id) {
            Ok(()) => {
                self.open(&folder);
                self.output.info("Restored that version. The files it replaced were kept as a version named \"Before restore\".");
                self.versions_open = true;
            }
            Err(err) => self
                .output
                .error(format!("Could not restore that version: {err}")),
        }
    }
}

/// `a`, `b` and `c`, each in backticks.
/// The green "Playing" badge in the menu bar during Play.
fn playing_badge(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(theme::PLAY_GREEN.gamma_multiply(0.2))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                egui::RichText::new("Playing")
                    .color(theme::PLAY_GREEN)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(filled_icons::CIRCLE)
                    .family(theme::filled())
                    .size(9.0)
                    .color(theme::PLAY_GREEN),
            );
        })
        .response
        .on_hover_text(
            "Only the Properties panel can change the game, until you press \
             Stop. Those changes are undone at Stop.",
        );
}

fn quoted_list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Makes one edit to `world`, without recording it.
fn set(world: &mut World, id: InstanceId, edit: Edit) -> anyhow::Result<()> {
    let Some(instance) = world.get(id) else {
        return Ok(());
    };
    match edit {
        Edit::Name(name) => {
            if properties::can_rename_here(instance) {
                world.set_name(id, name)?;
            }
        }
        Edit::Property(name, value) => {
            // Setting a Material resets Metalness and Roughness, so
            // picking the material it already has must do nothing.
            if instance.property(name) != Some(&value) {
                world.set_property(id, name, value)?;
            }
        }
        // Turned into a property edit, or done, by `apply`.
        Edit::PickTexture(_) | Edit::PickAudio | Edit::PreviewSound(_) => {}
    }
    Ok(())
}

/// Every object in the Workspace, for the block editor's object menus: its
/// path of names below the Workspace (`Pad.Chime`) and its class.
fn block_objects(world: &World) -> Vec<block_editor::ObjectInfo> {
    let workspace = world.workspace();
    world
        .descendants(workspace)
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            let mut names = vec![instance.name().to_owned()];
            let mut parent = instance.parent();
            while let Some(above) = parent.filter(|&above| above != workspace) {
                let above = world.get(above)?;
                names.push(above.name().to_owned());
                parent = above.parent();
            }
            names.reverse();
            Some(block_editor::ObjectInfo {
                path: names.join("."),
                class: instance.class(),
            })
        })
        .collect()
}

/// The world the panels and viewport show: the game's during Play, otherwise
/// the one being edited.
///
/// This takes the two fields instead of `&self`, so the result only borrows
/// those two, and other fields (the Explorer, the viewport) can still be
/// changed while it's in use. `'a` says the returned world lives as long as
/// the borrowed fields.
fn shown<'a>(world: &'a World, play: &'a Option<Session>) -> &'a World {
    play.as_ref().map_or(world, Session::world)
}

impl eframe::App for EditorApp {
    /// Called by eframe to draw each frame (only when something changed,
    /// such as mouse movement, unless a frame is requested).
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // The window's close button: stop it and ask first if there are
        // unsaved changes. `closing` lets the close through once allowed.
        if ctx.input(|input| input.viewport().close_requested()) && !self.closing && self.unsaved()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.asking = Some(Discard::Quit);
        }
        // Files changed by other programs, such as a script saved in VS Code.
        if let Some(project) = &self.project {
            project.watcher.wake(ctx);
        }
        self.check_disk();
        self.finish_thumbnail(ctx);
        // Taken from the input before the viewport sees it, so the S doesn't
        // also fly the camera backward.
        if ctx.input_mut(|input| input.consume_shortcut(&SAVE_SHORTCUT)) {
            self.save();
        }
        self.shortcuts(ctx);

        // During Play, run a game frame before anything is drawn, so every
        // panel shows the world as it is now. Then ask for the next frame
        // straight away: games keep moving without input.
        if self.playing() {
            let now = Instant::now();
            let seconds = self
                .last_frame
                .map_or(0.0, |last| (now - last).as_secs_f64())
                .min(MAX_FRAME_SECONDS);
            self.last_frame = Some(now);
            // Keys go to LocalScripts, unless a text box is being typed in.
            // (Read inside `input`, then used outside it: see the deadlock
            // trap in the crate's CLAUDE.md.)
            let typing = self.focused_text.is_some() || ctx.wants_keyboard_input();
            let (events, modifiers, focused) =
                ctx.input(|input| (input.events.clone(), input.modifiers, input.focused));
            if let Some(id) = self.focused_text
                && let Some(session) = &mut self.play
            {
                for event in &events {
                    let command = match event {
                        egui::Event::Text(text) => {
                            Some(axcenity_ui::TextCommand::Insert(text.clone()))
                        }
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            ..
                        } => Some(axcenity_ui::TextCommand::Backspace),
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        } => Some(axcenity_ui::TextCommand::Submit),
                        _ => None,
                    };
                    if matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    ) {
                        self.focused_text = None;
                        self.pending_focus_lost = Some(id);
                        break;
                    }
                    let Some(command) = command else {
                        continue;
                    };
                    match session.edit_text(id, command, &mut self.output) {
                        Ok(axcenity_ui::TextOutcome::Finished) => {
                            self.focused_text = None;
                            self.pending_focus_lost = Some(id);
                            break;
                        }
                        Ok(_) => {}
                        Err(crash) => {
                            self.crashed(crash);
                            break;
                        }
                    }
                }
            }
            let keys = self.game_keys.read(&events, modifiers, focused, typing);
            let activation = self.pending_ui_activation.take();
            let focus_lost = self.pending_focus_lost.take();
            self.play_frame(seconds, &keys, activation, focus_lost);
            ctx.request_repaint();
        }

        let title = self.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }

        if self.brand.is_none() {
            self.brand = Some(Brand::load(ctx));
        }
        // With no project open, the start screen takes the place of the
        // toolbar, the Explorer, the tabs and the viewport.
        let has_project = self.project.is_some();

        // Panels take their space in the order they're added: the menu bar
        // spans the whole top, then the Explorer (with Properties under it)
        // runs down the whole right side, then the Output fits under the viewport.
        self.update_warnings();
        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| self.menu_bar(ui));
        if has_project {
            egui::TopBottomPanel::top("toolbar")
                .frame(
                    egui::Frame::side_top_panel(&ctx.style())
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ctx, |ui| self.toolbar(ui));
        }
        let mut edit = None;
        let mut style_edit = None;
        let mut explorer = ExplorerOutput::default();
        let mut explorer_rows = Vec::new();
        let editing = !self.playing();
        // The game's world during Play, otherwise the one being edited.
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        if has_project {
            egui::SidePanel::right("explorer")
                .default_width(300.0)
                .show(ctx, |ui| {
                    egui::TopBottomPanel::bottom("properties")
                        .resizable(true)
                        .default_height(340.0)
                        .show_inside(ui, |ui| {
                            let selected = self.selected;
                            edit = properties::show(
                                ui,
                                world,
                                selected,
                                !editing,
                                self.selection.len(),
                                |ui| {
                                    // Stylesheets are edited, not the game, so not during Play.
                                    if editing {
                                        style_edit =
                                            style_panel::show(ui, world, selected, stylesheets);
                                    }
                                },
                            );
                        });
                    explorer = self.explorer.show(ui, world, &self.selection, editing);
                    if explorer.clicked.is_some() {
                        explorer_rows = self
                            .explorer
                            .rows(world)
                            .into_iter()
                            .map(|row| row.id)
                            .collect();
                    }
                });
        }
        // Applied before the viewport draws, so it shows the change this frame.
        if let (Some(id), Some(edit)) = (self.selected, edit) {
            self.apply(id, edit);
        }
        if let (Some(id), Some(edit)) = (self.selected, style_edit) {
            self.apply_style(id, edit);
        }
        if explorer.clicked.is_some() || explorer.keep_group {
            self.select_clicked(
                explorer.clicked,
                explorer.modifiers,
                Some(&explorer_rows),
                explorer.keep_group,
            );
            ctx.request_repaint();
        }
        if let Some(action) = explorer.action {
            self.act(action);
            // The Explorer was drawn before this change, so draw it again.
            ctx.request_repaint();
        }
        if let Some(id) = explorer.open {
            self.open_script_instance(id);
        }
        let mut browser_action = None;
        let clicked_error = has_project
            .then(|| {
                egui::TopBottomPanel::bottom("output")
                    .resizable(true)
                    .default_height(160.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            for (tab, icon, label) in [
                                (BottomTab::Output, icons::TERMINAL_WINDOW, "Output"),
                                (BottomTab::Assets, icons::SHAPES, "Assets"),
                            ] {
                                if ui
                                    .selectable_label(
                                        self.bottom_tab == tab,
                                        theme::with_icon(icon, label),
                                    )
                                    .clicked()
                                {
                                    self.bottom_tab = tab;
                                }
                            }
                        });
                        match self.bottom_tab {
                            BottomTab::Output => self.output.show(ui),
                            BottomTab::Assets => {
                                let can_save =
                                    assets::any_prefab(&self.world, &self.selection_roots());
                                browser_action = self.asset_browser.show(
                                    ui,
                                    &self.assets,
                                    !self.playing(),
                                    can_save,
                                );
                                None
                            }
                        }
                    })
                    .inner
            })
            .flatten();
        match browser_action {
            Some(BrowserAction::Place(entry)) => self.drop_asset(&entry, None),
            Some(BrowserAction::Import { image_folder }) => self.import_assets(image_folder),
            Some(BrowserAction::SavePrefab) => self.save_prefab(),
            None => {}
        }
        if let Some(location) = clicked_error {
            self.open_script(&location.path, Some(location.line));
        }
        // The tabs sit above the center only, between the panels added so far.
        let close_tab = has_project
            .then(|| {
                egui::TopBottomPanel::top("tabs")
                    .show(ctx, |ui| self.tabs.tab_bar(ui))
                    .inner
            })
            .flatten();
        if let Some(path) = close_tab {
            self.request_and_go_ahead(ctx, Discard::CloseTab(path));
        }
        // Scripts and LocalScripts switch between Code and Blocks here.
        let switchable = self
            .tabs
            .active_path()
            .filter(|path| axcenity_project::switched_script_path(path).is_some())
            .map(str::to_owned);
        if has_project && let Some(path) = switchable {
            let blocks = axcenity_project::is_block_script(&path);
            let playing = self.playing();
            let switch = egui::TopBottomPanel::top("script mode")
                .frame(
                    egui::Frame::side_top_panel(&ctx.style())
                        .inner_margin(egui::Margin::symmetric(10, 5)),
                )
                .show(ctx, |ui| scripts::mode_bar(ui, blocks, !playing))
                .inner;
            if switch {
                self.switch_script_mode(&path);
                ctx.request_repaint();
            }
        }

        // Again, as the edits above may have changed which world to show.
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        // No tool during Play, so nothing can be dragged.
        let tool = (!self.playing()).then_some(self.tool);
        let active_block_path = self
            .tabs
            .active_path()
            .filter(|path| axcenity_project::is_block_script(path))
            .map(str::to_owned);
        self.viewport_rect = None;
        let output = if !has_project {
            self.start_screen(ctx);
            Default::default()
        } else if self.tabs.world_shown() {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| match frame.wgpu_render_state() {
                    Some(render_state) => {
                        self.viewport_rect = Some(ui.available_rect_before_wrap());
                        self.viewport.show(
                            ui,
                            render_state,
                            ViewportScene {
                                world,
                                assets: &self.assets,
                                selected: self.selected,
                                selection: &self.selection,
                                tool,
                                snap: self.snap,
                                modes: self.modes,
                                stylesheets,
                                focused_text: self.focused_text,
                            },
                        )
                    }
                    // Can't happen: main.rs always picks the wgpu renderer.
                    None => {
                        ui.label("The viewport needs the wgpu renderer.");
                        Default::default()
                    }
                })
                .inner
        } else if let Some(path) = &active_block_path {
            let mut changed = false;
            let objects = block_objects(&self.world);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    if let Some(document) = self.project.as_mut().and_then(|project| {
                        project
                            .block_scripts
                            .iter_mut()
                            .find(|document| document.path == *path)
                    }) {
                        changed = block_editor::show(
                            ui,
                            &mut document.script,
                            block_editor::BlockContext {
                                key: path,
                                client: axcenity_project::script_class(path)
                                    == ClassName::LocalScript,
                                highlighted: document.highlighted,
                                objects: &objects,
                            },
                        );
                        if changed {
                            document.highlighted = None;
                        }
                    }
                });
            if changed {
                self.update_block_lua(path);
            }
            Default::default()
        } else {
            let stylesheet_path = self
                .tabs
                .active_path()
                .filter(|path| axcenity_project::is_stylesheet(path))
                .map(str::to_owned);
            let previous_source = stylesheet_path
                .as_deref()
                .and_then(|path| self.tabs.get(path))
                .map(|tab| tab.text.clone());
            egui::CentralPanel::default().show(ctx, |ui| self.tabs.show_active(ui));
            if let (Some(path), Some(previous_source)) = (stylesheet_path, previous_source) {
                let current_source = self
                    .tabs
                    .get(&path)
                    .map(|tab| tab.text.clone())
                    .unwrap_or_default();
                if current_source != previous_source {
                    self.refresh_stylesheet(&path, &current_source);
                    ctx.request_repaint();
                }
            }
            Default::default()
        };
        if let Some(clicked) = output.clicked {
            if self.tool == Tool::Paint
                && !self.playing()
                && let Some(part) = clicked.part
                && self.world.get(part).is_some_and(|i| {
                    i.property("Color").is_some() && i.property("Position").is_some()
                })
            {
                self.paint_clicked(part);
            }
            self.select_clicked(clicked.part, clicked.modifiers, None, clicked.keep_group);
            // The panels were already drawn this frame, so draw again to
            // show the new selection there.
            ctx.request_repaint();
        }
        if output.activated.is_some() {
            self.pending_ui_activation = output.activated;
        }
        if self.playing() {
            if output.text_box.is_some() {
                self.focused_text = output.text_box;
            } else if output.viewport_clicked {
                if let Some(id) = self.focused_text.take() {
                    self.pending_focus_lost = Some(id);
                }
            }
        }
        let fresh: Vec<String> = output
            .ui_warnings
            .iter()
            .filter(|warning| !self.ui_warnings.contains(warning))
            .cloned()
            .collect();
        self.ui_warnings.clone_from(&output.ui_warnings);
        for warning in fresh {
            self.output.warning(warning);
        }
        // An asset let go over the viewport is placed where it was dropped.
        if let Some(target) = output.drop.filter(|target| target.released)
            && let Some(payload) = egui::DragAndDrop::take_payload::<AssetPayload>(ctx)
        {
            self.drop_asset(&payload.0, Some(target));
            ctx.request_repaint();
        }
        if let Some(edit) = output.drag {
            self.apply_drag(edit);
            // The viewport was drawn before this change, so draw it again.
            ctx.request_repaint();
        }
        if let Some(edit) = output.ui_drag {
            self.apply_ui_drag(edit);
            ctx.request_repaint();
        }
        if let Some(rect) = self.viewport_rect {
            self.tool_panel(ctx, rect);
        }

        // A drag or a typed value is one undo step: it ends when the mouse
        // is let go and no text box is being typed in.
        let busy = ctx.is_using_pointer() || ctx.wants_keyboard_input();
        if output.drag_released || (!busy && !self.viewport.is_dragging()) {
            self.history.seal();
            self.ui_drag_baselines.clear();
        }

        // Models that couldn't be loaded, found while drawing or playing.
        let model_errors = self.assets.take_errors();
        if !model_errors.is_empty() {
            for error in model_errors {
                self.output.error(error);
            }
            // Output was already drawn this frame.
            ctx.request_repaint();
        }

        if self.versions_open {
            self.version_window(ctx);
        }
        self.unsaved_changes_prompt(ctx);
        self.delete_script_prompt(ctx);
        self.import_prompt(ctx);
        self.start_thumbnail(ctx);
        if self.about_open
            && let Some(brand) = &self.brand
        {
            self.about_open = start::about(ctx, brand);
        }
    }

    /// Saves the theme and the recent projects. eframe calls this now and
    /// then, and when the window closes.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let theme = match self.theme {
            egui::ThemePreference::Dark => "dark",
            egui::ThemePreference::Light => "light",
            egui::ThemePreference::System => "system",
        };
        storage.set_string(THEME_KEY, theme.to_string());
        let recent: Vec<String> = self
            .recent
            .iter()
            .map(|link| link.folder.display().to_string())
            .collect();
        storage.set_string(RECENT_KEY, recent.join("\n"));
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Color3, Material, PartType, Value, Vector3};
    use axcenity_render::glam::Vec3;
    use axcenity_scene::Ray;

    use crate::gizmo::{Axis, Drag, Handle, Pose, Side};
    use crate::output::{OutputKind, OutputLine};
    use crate::placing::PlaceDrag;

    use super::*;

    pub(super) fn example(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    pub(super) fn last_line(app: &EditorApp) -> &OutputLine {
        app.output.lines().last().unwrap()
    }

    /// A project folder in the temp directory with these files in it.
    fn temp_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-editor-{name}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (file, text) in files {
            std::fs::write(folder.join(file), text).unwrap();
        }
        folder
    }

    #[test]
    fn opens_the_milestone_1_demo_and_says_so_in_output() {
        let app = EditorApp::new(Some(&example("hello-touched")));

        let workspace = app.world.workspace();
        for part in ["Floor", "Pad", "Ball"] {
            assert!(app.world.find_first_child(workspace, part).is_some());
        }
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Info);
        assert!(
            line.text.starts_with("Opened \"Hello Touched\" from")
                && line.text.contains(": 4 objects, 1 script."),
            "{}",
            line.text
        );
    }

    #[test]
    fn unsupported_css_properties_are_reported_as_output_warnings() {
        let folder = temp_project(
            "unsupported-css",
            &[
                ("axcenity.json", r#"{"name":"CSS","format_version":1}"#),
                ("world.json", r#"{"format_version":1}"#),
                ("ui.css", ".hud { animation: fade 1s; opacity: .5 }"),
            ],
        );
        let app = EditorApp::new(Some(&folder));
        let warning = app
            .output
            .lines()
            .iter()
            .find(|line| line.kind == OutputKind::Warning)
            .expect("unknown CSS properties should be reported");

        assert!(warning.text.contains("ui.css"));
        assert!(
            warning
                .text
                .contains("unsupported CSS property `animation`")
        );
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn stylesheet_edits_restyle_the_viewport_immediately_and_save() {
        let folder = copy_of_example("hello-css", "stylesheet-live-edit");
        let mut app = EditorApp::new(Some(&folder));
        let path = "ui.css";
        let original = std::fs::read_to_string(folder.join(path)).unwrap();
        let updated = original.replace("width: 280px", "width: 320px");
        assert_ne!(original, updated);
        app.tabs.open(path, &original, None);

        let hud = app
            .world
            .find_first_child(app.world.workspace(), "Hud")
            .unwrap();
        let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
        let layout = |app: &EditorApp| {
            axcenity_ui::layout_ui(
                &app.world,
                800,
                600,
                &app.project.as_ref().unwrap().stylesheets,
            )
            .element(panel)
            .unwrap()
            .rect
            .width
        };
        assert_eq!(layout(&app), 280.0);

        app.tabs.get_mut(path).unwrap().text = updated.clone();
        app.refresh_stylesheet(path, &updated);
        assert_eq!(layout(&app), 320.0);
        assert!(app.unsaved());
        assert!(app.save());
        assert_eq!(std::fs::read_to_string(folder.join(path)).unwrap(), updated);
        assert!(!app.unsaved());
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn the_style_section_rewrites_and_adds_css_for_the_selected_element() {
        let folder = copy_of_example("hello-css", "style-section");
        let mut app = EditorApp::new(Some(&folder));
        let hud = app
            .world
            .find_first_child(app.world.workspace(), "Hud")
            .unwrap();
        let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
        let width = |app: &EditorApp| {
            axcenity_ui::layout_ui(
                &app.world,
                800,
                600,
                &app.project.as_ref().unwrap().stylesheets,
            )
            .element(panel)
            .unwrap()
            .rect
            .width
        };
        let declaration = |app: &EditorApp, property: &str| {
            axcenity_ui::applied_declaration(
                &app.world,
                panel,
                &app.project.as_ref().unwrap().stylesheets,
                property,
            )
        };

        // Changing a value rewrites the rule that set it, live.
        let applied = declaration(&app, "width").unwrap();
        app.apply_style(
            panel,
            StyleEdit::Set {
                path: applied.path,
                span: applied.span,
                value: "300px".to_owned(),
            },
        );
        assert_eq!(width(&app), 300.0);
        assert!(
            app.tabs
                .get("ui.css")
                .unwrap()
                .text
                .contains("width: 300px;")
        );

        // An element with no Id gets one from its name, and its own rule.
        app.apply_style(
            panel,
            StyleEdit::Add {
                property: "opacity".to_owned(),
                value: "0.5".to_owned(),
            },
        );
        let id = app.world.get(panel).unwrap().property("Id").cloned();
        assert_eq!(id, Some(Value::String("HudPanel".to_owned())));
        let added = declaration(&app, "opacity").unwrap();
        assert_eq!(
            (added.selector.as_str(), added.value.as_str()),
            ("#HudPanel", "0.5")
        );
        // A second style joins the same rule.
        app.apply_style(
            panel,
            StyleEdit::Add {
                property: "border-radius".to_owned(),
                value: "4px".to_owned(),
            },
        );
        let text = &app.tabs.get("ui.css").unwrap().text;
        assert_eq!(text.matches("#HudPanel {").count(), 1, "{text}");
        assert!(app.unsaved());
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_stylesheet_edited_in_another_program_restyles_the_viewport() {
        let folder = copy_of_example("hello-css", "stylesheet-outside-edit");
        let mut app = EditorApp::new(Some(&folder));
        let hud = app
            .world
            .find_first_child(app.world.workspace(), "Hud")
            .unwrap();
        let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
        let width = |app: &EditorApp| {
            axcenity_ui::layout_ui(
                &app.world,
                800,
                600,
                &app.project.as_ref().unwrap().stylesheets,
            )
            .element(panel)
            .unwrap()
            .rect
            .width
        };
        assert_eq!(width(&app), 280.0);
        let path = folder.join("ui.css");
        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("width: 280px", "width: 340px");
        std::fs::write(&path, &edited).unwrap();
        app.disk_changed(&["ui.css".to_owned()]);
        assert_eq!(width(&app), 340.0);
        assert!(!app.unsaved(), "nothing to save: the file already has it");
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn ui_elements_inserted_with_no_screen_gui_get_one() {
        let folder = copy_of_example("hello-part", "ui-insert");
        let mut app = EditorApp::new(Some(&folder));
        app.insert(Insertable::TextButton, None);
        let button = app.selected.unwrap();
        let parent = app.world.get(button).unwrap().parent().unwrap();
        assert_eq!(app.world.get(parent).unwrap().class(), ClassName::ScreenGui);
        assert_eq!(
            app.world.get(button).unwrap().class(),
            ClassName::TextButton
        );
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn starting_without_a_project_shows_nothing_open() {
        let app = EditorApp::new(None);
        assert_eq!(app.title(), "Axcenity Editor");
        assert!(app.output.lines().is_empty());
        // Only `game`'s services.
        let world = &app.world;
        let descendants = world.descendants(world.root());
        assert!(
            descendants
                .iter()
                .all(|&id| world.get(id).unwrap().class().is_service())
        );
    }

    #[test]
    fn closing_a_project_returns_to_the_start_screen() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-part");
        let mut app = EditorApp::new(Some(&root));
        assert!(app.project.is_some());
        app.finish_closing_project();
        assert!(app.project.is_none());
        assert_eq!(app.title(), "Axcenity Editor");
        // It stays in the project list.
        assert_eq!(app.recent.len(), 1);
    }

    #[test]
    fn a_missing_project_folder_is_reported_in_output() {
        let app = EditorApp::new(Some(&example("no-such-project")));
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text.starts_with("Could not open the project in")
                && line.text.contains("axcenity.json"),
            "{}",
            line.text
        );
    }

    #[test]
    fn a_broken_world_file_is_reported_with_its_line_and_the_rest_still_opens() {
        let folder = temp_project(
            "broken-world",
            &[
                (
                    "axcenity.json",
                    r#"{ "name": "Broken", "format_version": 1 }"#,
                ),
                (
                    "world.json",
                    "{\n  \"format_version\": 1,\n  \"workspace\": oops\n}\n",
                ),
            ],
        );
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.open(&folder);

        let error = app
            .output
            .lines()
            .iter()
            .rfind(|line| line.kind == OutputKind::Error)
            .unwrap();
        assert!(
            error.text.contains("world.json") && error.text.contains("line 3"),
            "{}",
            error.text
        );
        // The project opened anyway, with an empty Workspace.
        assert_eq!(app.title(), "Broken - Axcenity Editor");
        assert!(last_line(&app).text.contains("The Workspace is empty"));
        let workspace = app.world.get(app.world.workspace()).unwrap();
        assert!(workspace.children().is_empty());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A copy of an example project in the temp directory, safe to save over.
    pub(super) fn copy_of_example(name: &str, test: &str) -> PathBuf {
        fn copy_folder(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_folder(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let folder = std::env::temp_dir().join(format!(
            "axcenity-editor-{test}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        copy_folder(&example(name), &folder);
        folder
    }

    pub(super) fn part(app: &EditorApp, name: &str) -> InstanceId {
        app.world
            .find_first_child(app.world.workspace(), name)
            .unwrap()
    }

    fn vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3(Vector3::new(x, y, z))
    }

    /// The edits the criteria name: Position, Size, Color, Material and
    /// Anchored, plus a new name. Odd numbers check nothing gets rounded.
    fn edit_the_pad(app: &mut EditorApp) -> InstanceId {
        let pad = part(app, "Pad");
        let edits = [
            Edit::Name("Launch Pad".to_string()),
            Edit::Property("Position", vector(1.25, 0.3, -7.1)),
            Edit::Property("Size", vector(3.3, 0.001, 12.0)),
            Edit::Property("Color", Value::Color3(Color3::new(0.1, 0.725, 0.333))),
            Edit::Property("Material", Value::Material(Material::Neon)),
            Edit::Property("Anchored", Value::Bool(false)),
        ];
        for edit in edits {
            app.apply(pad, edit);
        }
        pad
    }

    #[test]
    fn selecting_a_part_in_the_viewport_highlights_it_in_the_explorer_too() {
        // A part two levels down, inside a closed branch.
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        let child = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(child, Some(pad)).unwrap();
        assert!(!app.explorer.rows(&app.world).iter().any(|r| r.id == child));

        app.select_from_viewport(Some(child));
        assert_eq!(app.selected, Some(child));
        // The Explorer highlights the row matching `selected`, and it's visible.
        assert!(app.explorer.rows(&app.world).iter().any(|r| r.id == child));

        // A click on empty space clears the selection.
        app.select_from_viewport(None);
        assert_eq!(app.selected, None);
    }

    #[test]
    fn edits_show_in_the_next_draw_list_and_count_as_unsaved() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        assert!(!app.unsaved());
        let pad = edit_the_pad(&mut app);
        assert!(app.unsaved());
        assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");

        let instance = app.world.get(pad).unwrap();
        assert_eq!(instance.name(), "Launch Pad");
        assert_eq!(instance.property("Anchored"), Some(&Value::Bool(false)));
        // The viewport draws from the world each frame, so the next draw list
        // already has the new position, size and neon glow.
        let camera = axcenity_render::Camera {
            position: Vec3::new(0.0, 50.0, 50.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
        let drawn_pad = list
            .items
            .iter()
            .find(|item| {
                let boosted = [0.25, 1.8125, 0.8325];
                item.material
                    .emissive
                    .iter()
                    .zip(boosted)
                    .all(|(a, b)| (a - b).abs() < 1e-5)
            })
            .expect("the pad glows in its new color");
        let center = drawn_pad.transform.transform_point3(Vec3::ZERO);
        assert!(center.abs_diff_eq(Vec3::new(1.25, 0.3, -7.1), 1e-6));
        let corner = drawn_pad.transform.transform_point3(Vec3::splat(0.5));
        assert!((corner - center).abs_diff_eq(Vec3::new(-6.0, 0.0005, 1.65), 1e-6));
    }

    #[test]
    fn setting_a_value_it_already_has_is_not_an_unsaved_change() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Name("Pad".to_string()));
        app.apply(pad, Edit::Property("Anchored", Value::Bool(true)));
        assert!(!app.unsaved());
    }

    #[test]
    fn scripts_cannot_be_renamed_from_properties() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let script = app
            .world
            .find_first_child(app.world.server_script_service(), "pad")
            .unwrap();
        app.apply(script, Edit::Name("door".to_string()));
        assert_eq!(app.world.get(script).unwrap().name(), "pad");
        assert!(!app.unsaved());
    }

    #[test]
    fn save_close_and_reopen_reproduces_the_edits_exactly() {
        let folder = copy_of_example("hello-touched", "save-reopen");
        let mut app = EditorApp::new(Some(&folder));
        edit_the_pad(&mut app);
        assert!(app.save());
        assert!(!app.unsaved());
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        assert!(last_line(&app).text.starts_with("Saved \"Hello Touched\""));
        let saved = axcenity_project::write_world(&app.world);
        drop(app);

        let reopened = EditorApp::new(Some(&folder));
        // Same world file text means every Instance, name and property
        // value came back exactly, down to the last bit of every number.
        assert_eq!(axcenity_project::write_world(&reopened.world), saved);
        let pad = part(&reopened, "Launch Pad");
        let pad = reopened.world.get(pad).unwrap();
        assert_eq!(pad.property("Position"), Some(&vector(1.25, 0.3, -7.1)));
        assert_eq!(
            pad.property("Color"),
            Some(&Value::Color3(Color3::new(0.1, 0.725, 0.333)))
        );
        assert_eq!(
            pad.property("Material"),
            Some(&Value::Material(Material::Neon))
        );
        // The script file is left alone and still loads.
        assert!(
            reopened
                .world
                .find_first_child(reopened.world.server_script_service(), "pad")
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn closing_with_unsaved_changes_asks_first() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        // Nothing to lose: go straight ahead.
        assert_eq!(app.request(Discard::Quit), Some(Discard::Quit));
        assert_eq!(app.asking, None);

        edit_the_pad(&mut app);
        assert_eq!(app.request(Discard::Quit), None);
        assert_eq!(app.asking, Some(Discard::Quit));
        // Cancel keeps the editor open with the changes.
        assert_eq!(app.answer(Answer::Cancel), None);
        assert_eq!(app.asking, None);
        assert!(app.unsaved());
        // Don't Save goes ahead without saving.
        app.request(Discard::OpenAnother);
        assert_eq!(app.answer(Answer::DontSave), Some(Discard::OpenAnother));
        assert!(app.unsaved());
    }

    #[test]
    fn save_in_the_prompt_saves_then_goes_ahead_but_not_if_saving_fails() {
        let folder = copy_of_example("hello-touched", "prompt-save");
        let mut app = EditorApp::new(Some(&folder));
        edit_the_pad(&mut app);
        app.request(Discard::Quit);
        assert_eq!(app.answer(Answer::Save), Some(Discard::Quit));
        assert!(!app.unsaved());
        assert!(part_exists_on_disk(&folder, "Launch Pad"));

        // With the folder gone, saving fails: say why and stay open.
        std::fs::remove_dir_all(&folder).unwrap();
        app.apply(part(&app, "Floor"), Edit::Name("Ground".to_string()));
        app.request(Discard::Quit);
        assert_eq!(app.answer(Answer::Save), None);
        assert!(app.unsaved());
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(line.text.starts_with("Could not save"), "{}", line.text);
    }

    fn part_exists_on_disk(folder: &Path, name: &str) -> bool {
        let world = axcenity_project::load(folder).unwrap().world;
        world.find_first_child(world.workspace(), name).is_some()
    }

    fn property(app: &EditorApp, id: InstanceId, name: &str) -> Value {
        app.world.get(id).unwrap().property(name).unwrap().clone()
    }

    /// Drags `handle` of `id` as the viewport does: one edit per frame as
    /// the mouse moves along `rays`, then the mouse is let go.
    fn drag(app: &mut EditorApp, id: InstanceId, handle: Handle, rays: &[Ray]) {
        let pose = Pose::of(app.world.get(id).unwrap()).unwrap();
        let mut drag = Drag::start(handle, id, pose, rays[0]).unwrap();
        for &ray in &rays[1..] {
            let values = drag.update(ray, &app.snap).unwrap();
            app.apply_drag(DragEdit {
                kind: DragKind::of(handle),
                parts: vec![(id, values)],
            });
        }
        app.history.seal();
    }

    /// Rays from in front of the scene, straight down -Z, through (x, y).
    fn rays_along_z(points: &[(f32, f32)]) -> Vec<Ray> {
        points
            .iter()
            .map(|&(x, y)| Ray {
                origin: Vec3::new(x, y, 50.0),
                direction: Vec3::NEG_Z,
            })
            .collect()
    }

    #[test]
    fn each_gizmo_drag_is_one_undo_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        let start = (
            property(&app, pad, "Position"),
            property(&app, pad, "Size"),
            property(&app, pad, "Orientation"),
        );

        // Move 5 studs right in 5 frames.
        let steps: Vec<(f32, f32)> = (0..=5).map(|x| (x as f32, 0.25)).collect();
        drag(&mut app, pad, Handle::Move(Axis::X), &rays_along_z(&steps));
        assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
        assert_eq!(app.history.undo_label(), Some("Move Pad"));
        // Pull the top face up 2 studs.
        let steps = [(5.0, 0.5), (5.0, 1.5), (5.0, 2.5)];
        drag(
            &mut app,
            pad,
            Handle::Scale(Axis::Y, Side::Plus),
            &rays_along_z(&steps),
        );
        assert_eq!(property(&app, pad, "Size"), vector(8.0, 2.5, 8.0));
        assert_eq!(app.history.undo_label(), Some("Resize Pad"));
        // Turn it a quarter around Z, in steps.
        let around = [(8.0, 1.25), (7.0, 3.0), (5.0, 4.25)];
        drag(
            &mut app,
            pad,
            Handle::Rotate(Axis::Z),
            &rays_along_z(&around),
        );
        let orientation = property(&app, pad, "Orientation").as_vector3().unwrap();
        let actual_rotation = axcenity_scene::rotation_from_orientation(orientation);
        let initial_orientation = start.2.as_vector3().unwrap();
        let expected_rotation = axcenity_render::glam::Quat::from_rotation_z(90.0_f32.to_radians())
            * axcenity_scene::rotation_from_orientation(initial_orientation);
        assert!(actual_rotation.dot(expected_rotation).abs() > 1.0 - 1e-6);
        assert_eq!(app.history.undo_label(), Some("Rotate Pad"));

        // Three drags, three undos, back to the start.
        app.undo();
        assert_eq!(property(&app, pad, "Orientation"), start.2);
        app.undo();
        assert_eq!(property(&app, pad, "Size"), start.1);
        assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
        app.undo();
        assert_eq!(property(&app, pad, "Position"), start.0);
        assert_eq!(app.history.undo_label(), None);
        assert!(!app.unsaved(), "back as it was opened");

        app.redo();
        assert_eq!(property(&app, pad, "Position"), vector(5.0, 0.25, 0.0));
        assert_eq!(app.history.redo_label(), Some("Resize Pad"));
    }

    #[test]
    fn dragging_a_part_onto_another_stacks_it_in_one_undo_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let ball = part(&app, "Ball");
        let start = property(&app, ball, "Position");
        // Grab the ball (2 studs across, at (0, 10, 0)) on top, then drag it
        // across the floor and onto the pad (8×0.5×8 at (0, 0.25, 0)).
        let grabbed = Vec3::new(0.0, 11.0, 0.0);
        let place = PlaceDrag::start(&app.world, ball, grabbed).unwrap();
        let down = |x: f32, z: f32| Ray {
            origin: Vec3::new(x, 50.0, z),
            direction: Vec3::NEG_Y,
        };
        for (x, z) in [(12.0, 6.0), (8.0, 3.0), (2.0, 1.0)] {
            let position = place.update(&app.world, down(x, z), &app.snap).unwrap();
            app.apply_drag(DragEdit {
                kind: DragKind::Move,
                parts: vec![(ball, vec![("Position", position)])],
            });
        }
        app.history.seal();
        // On top of the pad (y = 0.5) and lifted by its radius.
        let (radius, top) = (1.0, 0.5);
        assert_eq!(
            property(&app, ball, "Position"),
            vector(2.0, top + radius, 1.0)
        );
        assert_eq!(app.history.undo_label(), Some("Move Ball"));
        app.undo();
        assert_eq!(property(&app, ball, "Position"), start);
        assert_eq!(app.history.undo_label(), None);
    }

    #[test]
    fn property_edits_undo_and_redo_and_typing_a_name_is_one_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        let gray = property(&app, pad, "Color");
        let green = Value::Color3(Color3::new(0.1, 0.725, 0.333));
        app.apply(pad, Edit::Property("Color", green.clone()));
        app.history.seal();
        // Typing "Door" one letter a frame, then leaving the text box.
        for name in ["D", "Do", "Doo", "Door"] {
            app.apply(pad, Edit::Name(name.to_string()));
        }
        app.history.seal();
        assert_eq!(app.history.undo_label(), Some("Rename Pad"));

        app.undo();
        assert_eq!(app.world.get(pad).unwrap().name(), "Pad");
        assert_eq!(property(&app, pad, "Color"), green);
        assert_eq!(app.history.undo_label(), Some("Change Color of Pad"));
        app.undo();
        assert_eq!(property(&app, pad, "Color"), gray);

        app.redo();
        app.redo();
        assert_eq!(property(&app, pad, "Color"), green);
        assert_eq!(app.world.get(pad).unwrap().name(), "Door");
    }

    #[test]
    fn undo_history_survives_saving() {
        let folder = copy_of_example("hello-touched", "undo-save");
        let mut app = EditorApp::new(Some(&folder));
        let pad = part(&app, "Pad");
        let start = property(&app, pad, "Position");
        let steps = [(0.0, 0.25), (3.0, 0.25)];
        drag(&mut app, pad, Handle::Move(Axis::X), &rays_along_z(&steps));
        assert!(app.save());
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");

        // Saving kept the drag in the history.
        assert_eq!(app.history.undo_label(), Some("Move Pad"));
        app.undo();
        assert_eq!(property(&app, pad, "Position"), start);
        assert!(app.unsaved(), "the file still has the moved pad");
        assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");
        // Redoing back to what's on disk counts as saved again.
        app.redo();
        assert!(!app.unsaved());

        // Undo, then save that: the file gets the undone position, and redo still works.
        app.undo();
        assert!(app.save());
        let on_disk = axcenity_project::load(&folder).unwrap().world;
        let saved_pad = on_disk
            .find_first_child(on_disk.workspace(), "Pad")
            .unwrap();
        let saved = on_disk
            .get(saved_pad)
            .unwrap()
            .property("Position")
            .cloned();
        assert_eq!(saved, Some(start));
        app.redo();
        assert_eq!(property(&app, pad, "Position"), vector(3.0, 0.25, 0.0));
        assert!(app.unsaved());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn opening_another_project_starts_a_new_history() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        edit_the_pad(&mut app);
        app.tool = Tool::Rotate;
        app.open(&example("hello-part"));
        assert_eq!(app.history.undo_label(), None);
        assert!(!app.unsaved());
        assert_eq!(app.tool, Tool::Rotate, "the tool is a preference");
    }

    fn name(app: &EditorApp, id: InstanceId) -> &str {
        app.world.get(id).unwrap().name()
    }

    fn children(app: &EditorApp, id: InstanceId) -> Vec<InstanceId> {
        app.world.get(id).unwrap().children().to_vec()
    }

    /// How many parts the viewport would draw.
    fn drawn_parts(app: &EditorApp) -> usize {
        let camera = axcenity_render::Camera {
            position: Vec3::new(0.0, 50.0, 50.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        axcenity_scene::build_draw_list(&app.world, &app.assets, camera)
            .items
            .len()
    }

    /// The Instance just inserted or duplicated, which is selected.
    fn inserted(app: &mut EditorApp, what: Insertable, near: Option<InstanceId>) -> InstanceId {
        app.act(Action::Insert { what, near });
        app.selected.expect("inserting selects the new Instance")
    }

    #[test]
    fn each_primitive_is_inserted_at_the_camera_focus_point() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let workspace = app.world.workspace();
        app.viewport
            .look_at(Vec3::new(30.0, 20.0, 10.0), Vec3::new(5.25, 3.0, -7.5));
        app.snap.move_on = false;
        let mut shapes = Vec::new();
        for what in Insertable::PARTS {
            let part = inserted(&mut app, what, None);
            assert_eq!(app.world.get(part).unwrap().parent(), Some(workspace));
            assert_eq!(property(&app, part, "Position"), vector(5.25, 3.0, -7.5));
            shapes.push(property(&app, part, "Shape"));
            let label = format!("Insert {}", name(&app, part));
            assert_eq!(app.history.undo_label(), Some(label.as_str()));
        }
        let shape = |shape| Value::PartType(shape);
        assert_eq!(
            shapes,
            [
                shape(PartType::Block),
                shape(PartType::Ball),
                shape(PartType::Cylinder),
                shape(PartType::Wedge)
            ]
        );
        // They're in the world, so the viewport draws them.
        assert_eq!(drawn_parts(&app), 3 + 4);

        // With Snap move on, the point goes to the move grid.
        app.snap.move_on = true;
        let part = inserted(&mut app, Insertable::Part(PartType::Block), None);
        assert_eq!(property(&app, part, "Position"), vector(5.0, 3.0, -8.0));

        // Each insert is one undo step, and undoing it clears the selection.
        app.undo();
        assert!(!app.world.is_descendant_of(part, workspace));
        assert_eq!(app.selected, None);
        app.redo();
        assert_eq!(children(&app, workspace).last(), Some(&part));
    }

    #[test]
    fn inserting_gives_a_unique_default_name_among_siblings() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let block = Insertable::Part(PartType::Block);
        let names: Vec<String> = [block, block, Insertable::Model, Insertable::Folder]
            .into_iter()
            .chain([Insertable::Model, Insertable::Folder, block])
            .map(|what| {
                let id = inserted(&mut app, what, None);
                name(&app, id).to_string()
            })
            .collect();
        assert_eq!(
            names,
            [
                "Part", "Part2", "Model", "Folder", "Model2", "Folder2", "Part3"
            ]
        );

        // Inside a Model, only its own children count.
        let model = part(&app, "Model");
        let inside = inserted(&mut app, block, Some(model));
        assert_eq!(app.world.get(inside).unwrap().parent(), Some(model));
        assert_eq!(name(&app, inside), "Part");
        let inside2 = inserted(&mut app, block, Some(model));
        assert_eq!(name(&app, inside2), "Part2");

        // Duplicates follow the same rule, and go right after the original.
        let pad = part(&app, "Pad");
        app.act(Action::Duplicate(pad));
        let copy = app.selected.unwrap();
        assert_eq!(name(&app, copy), "Pad2");
        let workspace = children(&app, app.world.workspace());
        let at = workspace.iter().position(|&id| id == pad).unwrap();
        assert_eq!(workspace[at + 1], copy);
        assert_eq!(
            property(&app, copy, "Position"),
            property(&app, pad, "Position")
        );
        app.act(Action::Duplicate(copy));
        assert_eq!(name(&app, app.selected.unwrap()), "Pad3");
        assert_eq!(app.history.undo_label(), Some("Duplicate Pad2"));
    }

    /// Everything about `id` and the Instances inside it: IDs, classes, names,
    /// parents, child order and every property value.
    fn describe(world: &World, id: InstanceId) -> Vec<String> {
        std::iter::once(id)
            .chain(world.descendants(id))
            .map(|id| {
                let instance = world.get(id).unwrap();
                format!(
                    "{id:?} {} {:?} in {:?} {:?} {:?}",
                    instance.class(),
                    instance.name(),
                    instance.parent(),
                    instance.children(),
                    instance.properties().collect::<Vec<_>>()
                )
            })
            .collect()
    }

    #[test]
    fn deleting_a_model_deletes_its_children_and_undo_restores_them_all() {
        let folder = copy_of_example("hello-touched", "delete-model");
        let mut app = EditorApp::new(Some(&folder));
        let workspace = app.world.workspace();
        let model = inserted(&mut app, Insertable::Model, None);
        // A part in the model, a part inside that part, and edited properties.
        let wheel = inserted(&mut app, Insertable::Part(PartType::Cylinder), Some(model));
        app.apply(
            wheel,
            Edit::Property("Color", Value::Color3(Color3::new(0.1, 0.2, 0.3))),
        );
        app.apply(
            wheel,
            Edit::Property("Material", Value::Material(Material::Metal)),
        );
        app.apply(wheel, Edit::Property("Roughness", Value::Number(0.75)));
        let hub = inserted(&mut app, Insertable::Folder, Some(model));
        assert!(app.move_into(wheel, hub));
        app.move_into(part(&app, "Pad"), model);
        // Something after the model, so its place among its siblings matters.
        inserted(&mut app, Insertable::Folder, None);
        app.history.seal();
        let before = describe(&app.world, model);
        let siblings = children(&app, workspace);
        let world_file = axcenity_project::write_world(&app.world);
        let drawn_before = drawn_parts(&app);

        app.selected = Some(wheel);
        app.act(Action::Delete(model));
        assert!(!children(&app, workspace).contains(&model));
        for id in [model, wheel, hub] {
            assert!(!app.world.is_descendant_of(id, workspace));
        }
        assert_eq!(app.selected, None, "the selection was inside it");
        assert_eq!(
            drawn_parts(&app),
            drawn_before - 2,
            "the wheel and the pad are gone"
        );
        assert_eq!(app.history.undo_label(), Some("Delete Model"));
        // Saving now leaves them out of the file.
        assert!(app.save());
        assert!(!part_exists_on_disk(&folder, "Pad"));

        app.undo();
        assert_eq!(describe(&app.world, model), before);
        assert_eq!(children(&app, workspace), siblings);
        assert_eq!(axcenity_project::write_world(&app.world), world_file);
        // Redo deletes them again, and undo brings them back again.
        app.redo();
        assert!(!app.world.is_descendant_of(wheel, workspace));
        app.undo();
        assert_eq!(describe(&app.world, model), before);
        // The steps before the delete still work on the same Instances:
        // undo "Insert Folder", "Move Pad into Model", then "Move Part into Folder".
        app.undo();
        app.undo();
        assert_eq!(app.world.get(wheel).unwrap().parent(), Some(hub));
        app.undo();
        assert_eq!(app.world.get(wheel).unwrap().parent(), Some(model));
        assert_eq!(property(&app, wheel, "Roughness"), Value::Number(0.75));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn moving_into_another_parent_is_one_step_and_moves_into_itself_are_rejected() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let workspace = app.world.workspace();
        let model = inserted(&mut app, Insertable::Model, None);
        let pad = part(&app, "Pad");
        app.act(Action::MoveInto {
            id: pad,
            parent: model,
        });
        assert_eq!(children(&app, model), [pad]);
        assert_eq!(app.history.undo_label(), Some("Move Pad into Model"));

        // Into its own child, into itself, out of the Workspace, or moving a
        // service or a Script: nothing changes, and Output says why.
        let steps = app.history.undo_label().map(str::to_string);
        let world_file = axcenity_project::write_world(&app.world);
        let script = app
            .world
            .find_first_child(app.world.server_script_service(), "pad")
            .unwrap();
        for (id, parent) in [
            (model, pad),
            (model, model),
            (model, app.world.server_script_service()),
            (workspace, model),
            (script, model),
        ] {
            assert!(!app.move_into(id, parent));
            assert_eq!(last_line(&app).kind, OutputKind::Error);
            assert!(
                last_line(&app).text.starts_with("Can't move"),
                "{}",
                last_line(&app).text
            );
        }
        assert_eq!(axcenity_project::write_world(&app.world), world_file);
        assert_eq!(app.history.undo_label().map(str::to_string), steps);

        app.undo();
        assert_eq!(app.world.get(pad).unwrap().parent(), Some(workspace));
        assert!(children(&app, model).is_empty());
    }

    #[test]
    fn services_and_scripts_cannot_be_deleted_or_duplicated() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let script = app
            .world
            .find_first_child(app.world.server_script_service(), "pad")
            .unwrap();
        let count = app.world.len();
        for id in [app.world.workspace(), app.world.root(), script] {
            app.act(Action::Delete(id));
            app.act(Action::Duplicate(id));
        }
        assert_eq!(app.world.len(), count);
        assert!(app.world.is_descendant_of(script, app.world.root()));
        assert!(!app.unsaved());
        assert!(last_line(&app).text.starts_with("Can't duplicate pad"));
    }

    #[test]
    fn models_and_folders_save_and_reopen() {
        let folder = copy_of_example("hello-touched", "save-groups");
        let mut app = EditorApp::new(Some(&folder));
        let props = inserted(&mut app, Insertable::Folder, None);
        let car = inserted(&mut app, Insertable::Model, Some(props));
        inserted(&mut app, Insertable::Part(PartType::Wedge), Some(car));
        app.act(Action::Rename(car, "Car".to_string()));
        assert_eq!(app.history.undo_label(), Some("Rename Model"));
        assert!(app.save());
        let saved = axcenity_project::write_world(&app.world);

        let reopened = EditorApp::new(Some(&folder));
        assert_eq!(axcenity_project::write_world(&reopened.world), saved);
        let wedge = reopened
            .world
            .find_first_descendant(reopened.world.workspace(), "Part")
            .unwrap();
        assert_eq!(
            reopened.world.full_name(wedge).as_deref(),
            Some("Workspace.Folder.Car.Part")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn delete_ctrl_d_and_f2_act_on_the_selection() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        app.selected = Some(pad);
        press(&mut app, egui::Modifiers::COMMAND, egui::Key::D);
        let copy = app.selected.unwrap();
        assert_eq!(name(&app, copy), "Pad2");
        press(&mut app, egui::Modifiers::NONE, egui::Key::Delete);
        assert!(!app.world.is_descendant_of(copy, app.world.root()));
        assert_eq!(app.selected, None);
        app.selected = Some(pad);
        press(&mut app, egui::Modifiers::NONE, egui::Key::F2);
        assert_eq!(app.explorer.renaming(), Some(pad));
    }

    /// Runs one egui frame in which `key` is pressed with `modifiers`, and
    /// lets the app read its shortcuts. No window or GPU needed.
    fn press(app: &mut EditorApp, modifiers: egui::Modifiers, key: egui::Key) {
        let input = egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let _ = egui::Context::default().run(input, |ctx| app.shortcuts(ctx));
    }

    #[test]
    fn ctrl_1_to_4_pick_the_tools_and_ctrl_z_and_ctrl_y_undo_and_redo() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let command = egui::Modifiers::COMMAND;
        assert_eq!(app.tool, Tool::Select);
        for (key, tool) in [
            (egui::Key::Num2, Tool::Move),
            (egui::Key::Num3, Tool::Scale),
            (egui::Key::Num4, Tool::Rotate),
            (egui::Key::Num1, Tool::Select),
        ] {
            press(&mut app, command, key);
            assert_eq!(app.tool, tool);
        }

        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
        press(&mut app, command, egui::Key::Z);
        assert_eq!(property(&app, pad, "Anchored"), Value::Bool(true));
        press(&mut app, command, egui::Key::Y);
        assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
        // Ctrl+Shift+Z redoes too, and isn't taken as Ctrl+Z.
        press(&mut app, command, egui::Key::Z);
        press(&mut app, command | egui::Modifiers::SHIFT, egui::Key::Z);
        assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
    }

    /// Runs the game for `seconds` at 60 frames per second.
    pub(super) fn play_for(app: &mut EditorApp, seconds: f64) {
        for _ in 0..(seconds * 60.0).round() as u32 {
            app.play_frame(1.0 / 60.0, &[], None, None);
        }
    }

    pub(super) fn printed(app: &EditorApp) -> Vec<&str> {
        app.output.lines().iter().map(|l| l.text.as_str()).collect()
    }

    /// The whole tree, every Instance out of the tree that undo still holds,
    /// and the undo history: everything Stop has to put back.
    fn everything(app: &EditorApp) -> (Vec<String>, String, usize, Option<String>, bool) {
        (
            describe(&app.world, app.world.root()),
            axcenity_project::write_world(&app.world),
            app.world.len(),
            app.history.undo_label().map(str::to_string),
            app.unsaved(),
        )
    }

    #[test]
    fn play_runs_the_milestone_1_demo_in_the_editor() {
        let folder = example("hello-touched");
        let file_before = std::fs::read_to_string(folder.join("world.json")).unwrap();
        let mut app = EditorApp::new(Some(&folder));
        let pad = part(&app, "Pad");
        let gray = property(&app, pad, "Color");
        app.play();
        assert!(app.playing());
        assert!(
            last_line(&app)
                .text
                .starts_with("Playing \"Hello Touched\"")
        );

        // The red ball lands on the pad within 3 seconds, and the pad changes color.
        play_for(&mut app, 2.9);
        assert!(printed(&app).contains(&"Ball touched the pad"));
        let shown_pad = |app: &EditorApp| {
            shown(&app.world, &app.play)
                .get(pad)
                .unwrap()
                .property("Color")
                .cloned()
        };
        assert_ne!(
            shown_pad(&app),
            Some(gray.clone()),
            "the panels show the game"
        );
        assert_eq!(
            property(&app, pad, "Color"),
            gray,
            "the edit world is untouched"
        );
        // At 3 s the script drops a blue ball; at 6 s it stops listening.
        play_for(&mut app, 3.2);
        assert!(printed(&app).contains(&"BlueBall touched the pad"));
        assert_eq!(last_line(&app).text, "stopped listening for touches");
        let game = shown(&app.world, &app.play);
        assert!(
            game.find_first_child(game.workspace(), "BlueBall")
                .is_some()
        );

        // Nothing was unsaved, so Play didn't write the world file.
        let file_after = std::fs::read_to_string(folder.join("world.json")).unwrap();
        assert_eq!(file_after, file_before);
    }

    #[test]
    fn an_endless_loop_during_play_is_stopped_and_the_editor_carries_on() {
        let folder = copy_of_example("hello-touched", "endless-loop");
        std::fs::write(
            folder.join("scripts/stuck.server.lua"),
            "print('looping')\nwhile true do\nend\n",
        )
        .unwrap();
        let mut app = EditorApp::new(Some(&folder));
        let started = std::time::Instant::now();
        app.play();
        let took = started.elapsed();

        // The real limit, three seconds, then the editor gets control back.
        assert!(app.playing(), "the game keeps running");
        let limit = axcenity_script::WATCHDOG_LIMIT;
        assert!(took >= limit && took < limit * 2, "{took:?}");
        let stopped = app
            .output
            .lines()
            .iter()
            .find(|line| line.text.contains("without pausing"))
            .unwrap();
        assert_eq!(stopped.kind, OutputKind::Error);
        assert!(
            stopped.text.starts_with(
                "scripts/stuck.server.lua:2: this script ran for 3 seconds without pausing"
            ),
            "{}",
            stopped.text
        );
        // It links to the script, like any error.
        assert!(stopped.location.is_some());

        // The other script and the frames carry on, and Stop works.
        play_for(&mut app, 2.9);
        assert!(printed(&app).contains(&"Ball touched the pad"));
        app.stop();
        assert!(!app.playing());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn stop_puts_every_instance_and_property_back_as_before_play() {
        let folder = copy_of_example("hello-touched", "play-stop");
        let mut app = EditorApp::new(Some(&folder));
        // Some edits first: a moved pad, a new Model with a part in it, and a
        // deleted Folder that undo still holds outside the tree.
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Property("Position", vector(1.5, 0.25, -0.5)));
        let model = inserted(&mut app, Insertable::Model, None);
        inserted(&mut app, Insertable::Part(PartType::Wedge), Some(model));
        let folder_id = inserted(&mut app, Insertable::Folder, None);
        app.act(Action::Delete(folder_id));
        app.selected = Some(pad);
        app.play();
        let before = everything(&app);
        let game_ball = part(&app, "Ball");

        // Scripts recolor the pad and add balls, physics moves them all.
        play_for(&mut app, 9.0);
        let game = shown(&app.world, &app.play);
        assert!(
            game.find_first_child(game.workspace(), "IgnoredBall")
                .is_some()
        );
        assert_ne!(
            game.get(game_ball).unwrap().property("Position"),
            app.world.get(game_ball).unwrap().property("Position"),
            "the ball fell in the game"
        );

        app.stop();
        assert!(!app.playing());
        assert_eq!(
            last_line(&app).text,
            "Stopped. The world is back as it was before Play."
        );
        assert_eq!(everything(&app), before);
        assert_eq!(app.selected, Some(pad), "the selection stays");
        // The history still works on the same Instances: undo the delete.
        app.undo();
        assert!(app.world.is_descendant_of(folder_id, app.world.workspace()));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn something_the_game_made_is_unselected_at_stop() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.play();
        play_for(&mut app, 3.1);
        let game = shown(&app.world, &app.play);
        let blue = game.find_first_child(game.workspace(), "BlueBall").unwrap();
        app.select_from_viewport(Some(blue));
        app.stop();
        assert_eq!(app.selected, None);
    }

    #[test]
    fn play_saves_unsaved_changes_first_and_does_not_start_if_saving_fails() {
        let folder = copy_of_example("hello-touched", "play-save");
        let mut app = EditorApp::new(Some(&folder));
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Name("Launch Pad".to_string()));
        app.play();
        assert!(app.playing());
        assert!(!app.unsaved());
        assert!(part_exists_on_disk(&folder, "Launch Pad"));
        assert!(printed(&app).iter().any(|line| line.starts_with("Saved")));
        app.stop();

        // With the folder gone, saving fails, so the game doesn't start.
        std::fs::remove_dir_all(&folder).unwrap();
        app.apply(pad, Edit::Name("Pad".to_string()));
        app.play();
        assert!(!app.playing());
        assert!(app.unsaved());
        assert_eq!(last_line(&app).kind, OutputKind::Error);
        assert!(last_line(&app).text.starts_with("Play didn't start"));
    }

    #[test]
    fn play_runs_the_script_file_as_it_is_now() {
        let folder = copy_of_example("hello-touched", "play-reread");
        let mut app = EditorApp::new(Some(&folder));
        std::fs::write(
            folder.join("scripts/pad.server.lua"),
            "print('edited in another program')",
        )
        .unwrap();
        app.play();
        assert_eq!(last_line(&app).text, "edited in another program");
        app.stop();

        // A script file that's gone is reported, and the rest of the game runs.
        std::fs::remove_file(folder.join("scripts/pad.server.lua")).unwrap();
        app.play();
        assert!(app.playing());
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text.starts_with("scripts/pad.server.lua won't run"),
            "{}",
            line.text
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_engine_panic_during_play_is_reported_and_goes_back_to_editing() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let before = everything(&app);
        app.play();
        play_for(&mut app, 1.0);
        app.play.as_mut().unwrap().panic_next_frame();
        app.play_frame(1.0 / 60.0, &[], None, None);

        assert!(!app.playing(), "back in edit mode");
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text
                .starts_with("The engine crashed during Play: a deliberate engine panic"),
            "{}",
            line.text
        );
        assert_eq!(everything(&app), before);
        // The editor carries on: editing works, and so does playing again.
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
        assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
        app.undo();
        app.play();
        play_for(&mut app, 1.0);
        assert!(app.playing());
    }

    #[test]
    fn only_properties_can_be_edited_while_playing_and_stop_undoes_them() {
        // A copy, because Play saves the unsaved edit below.
        let folder = copy_of_example("hello-touched", "play-read-only");
        let mut app = EditorApp::new(Some(&folder));
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
        app.play();
        let before = everything(&app);

        // The Properties panel changes the game's world...
        app.apply(pad, Edit::Name("Door".to_string()));
        // ...and nothing else changes anything.
        app.act(Action::Delete(pad));
        app.act(Action::Duplicate(pad));
        app.act(Action::Insert {
            what: Insertable::Folder,
            near: None,
        });
        app.undo();
        app.selected = Some(pad);
        press(&mut app, egui::Modifiers::NONE, egui::Key::F2);
        assert_eq!(app.explorer.renaming(), None);
        let game = shown(&app.world, &app.play);
        assert_eq!(game.get(pad).unwrap().name(), "Door");
        assert!(game.is_descendant_of(pad, game.workspace()));
        assert!(game.find_first_child(game.workspace(), "Folder").is_none());
        assert_eq!(everything(&app), before, "no undo step, nothing unsaved");

        app.stop();
        assert_eq!(everything(&app), before);
        assert_eq!(app.world.get(pad).unwrap().name(), "Pad");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn editing_a_property_during_play_fires_changed_in_the_game() {
        let mut app = EditorApp::new(Some(&example("hello-events")));
        let lamp = part(&app, "Lamp");
        app.play();
        play_for(&mut app, 0.5);
        let changed = "Lamp's Color changed";
        assert!(!printed(&app).contains(&changed));

        let green = Value::Color3(Color3::new(0.1, 0.8, 0.2));
        app.apply(lamp, Edit::Property("Color", green.clone()));
        play_for(&mut app, 1.0 / 60.0);
        assert_eq!(last_line(&app).text, changed);
        let game = shown(&app.world, &app.play);
        assert_eq!(game.get(lamp).unwrap().property("Color"), Some(&green));

        // The edit world never changed, so there's nothing to save or undo.
        assert_ne!(property(&app, lamp, "Color"), green);
        assert!(!app.unsaved());
        assert_eq!(app.history.undo_label(), None);
    }

    #[test]
    fn f5_plays_and_shift_f5_stops() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        press(&mut app, egui::Modifiers::SHIFT, egui::Key::F5);
        assert!(!app.playing());
        press(&mut app, egui::Modifiers::NONE, egui::Key::F5);
        assert!(app.playing());
        // F5 again does nothing; Shift+F5 isn't taken for F5.
        press(&mut app, egui::Modifiers::NONE, egui::Key::F5);
        assert!(app.playing());
        press(&mut app, egui::Modifiers::SHIFT, egui::Key::F5);
        assert!(!app.playing());

        // Nothing to play without a project.
        let mut empty = EditorApp::new(None);
        press(&mut empty, egui::Modifiers::NONE, egui::Key::F5);
        assert!(!empty.playing());
    }

    #[test]
    fn set_up_autocomplete_writes_the_files_and_opening_refreshes_old_definitions() {
        let folder = copy_of_example("hello-events", "autocomplete");
        let mut app = EditorApp::new(Some(&folder));
        app.set_up_autocomplete();
        assert_eq!(
            last_line(&app).text,
            "Set up autocomplete: wrote `.axcenity/axcenity.lua`, `.luarc.json` and \
             `.vscode/extensions.json`. Open the project folder in VS Code and install the \
             Lua extension (sumneko.lua) when it asks; scripts then autocomplete the \
             engine's API."
        );
        let definitions = folder.join(autocomplete::DEFINITIONS_FILE);
        assert_eq!(
            std::fs::read_to_string(&definitions).unwrap(),
            axcenity_script::lua_definitions()
        );
        // The project still opens as before: the new files aren't scripts.
        assert_eq!(app.project.as_ref().unwrap().scripts.len(), 1);

        // Definitions from an older engine are updated when the project opens.
        std::fs::write(&definitions, "---@meta\n").unwrap();
        app.open(&folder);
        assert!(printed(&app).contains(
            &"Updated `.axcenity/axcenity.lua` to this version of the engine's script API."
        ),);
        assert_eq!(
            std::fs::read_to_string(&definitions).unwrap(),
            axcenity_script::lua_definitions()
        );
        assert_eq!(app.project.as_ref().unwrap().scripts.len(), 1);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn quoted_lists_read_like_a_sentence() {
        assert_eq!(quoted_list(&["a"]), "`a`");
        assert_eq!(quoted_list(&["a", "b"]), "`a` and `b`");
        assert_eq!(quoted_list(&["a", "b", "c"]), "`a`, `b` and `c`");
    }

    #[test]
    fn opening_another_project_stops_the_game() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.play();
        app.open(&example("hello-part"));
        assert!(!app.playing());
    }

    #[test]
    fn building_tools_change_the_whole_selection_in_one_undo_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let first = inserted(&mut app, Insertable::Part(PartType::Block), None);
        let second = inserted(&mut app, Insertable::Part(PartType::Ball), None);
        app.selection = vec![first, second];
        app.selected = Some(second);

        let red = Value::Color3(Color3::new(1.0, 0.0, 0.0));
        app.apply_tool_action(ToolAction::Set {
            property: "Color",
            value: red.clone(),
        });
        app.history.seal();
        assert_eq!(property(&app, first, "Color"), red);
        assert_eq!(property(&app, second, "Color"), red);
        assert_eq!(app.history.undo_label(), Some("Paint 2 parts"));

        app.apply_tool_action(ToolAction::Set {
            property: "CanCollide",
            value: Value::Bool(false),
        });
        app.history.seal();
        assert_eq!(property(&app, first, "CanCollide"), Value::Bool(false));

        app.apply_tool_action(ToolAction::AddLight(ClassName::PointLight));
        assert_eq!(children(&app, first).len(), 1);
        assert_eq!(children(&app, second).len(), 1);
        assert_eq!(app.history.undo_label(), Some("Add 2 PointLights"));
        // Adding again leaves parts that already have one alone.
        app.apply_tool_action(ToolAction::AddLight(ClassName::PointLight));
        assert_eq!(children(&app, first).len(), 1);

        app.undo();
        assert!(children(&app, first).is_empty() && children(&app, second).is_empty());
        app.undo();
        assert_eq!(property(&app, first, "CanCollide"), Value::Bool(true));
        app.undo();
        assert_ne!(property(&app, first, "Color"), red);
        assert_ne!(property(&app, second, "Color"), red);
    }

    #[test]
    fn f3x_keys_pick_tools_while_editing() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        for (key, tool) in [
            (egui::Key::Z, Tool::Move),
            (egui::Key::X, Tool::Scale),
            (egui::Key::V, Tool::Paint),
            (egui::Key::M, Tool::Anchor),
            (egui::Key::K, Tool::Collision),
            (egui::Key::U, Tool::Lighting),
            (egui::Key::P, Tool::Emission),
        ] {
            press(&mut app, egui::Modifiers::NONE, key);
            assert_eq!(app.tool, tool);
        }
    }
}
