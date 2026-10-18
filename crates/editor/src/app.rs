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

mod files;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_project::ScriptFile;
use axcenity_script::{CameraState, KeyEvent};
use eframe::egui;

use crate::autocomplete;
use crate::explorer::{self, Action, Explorer, ExplorerOutput};
use crate::gizmo::{Snap, Tool};
use crate::history::{self, Field, History, Move, Place};
use crate::import_dialog::ImportDialog;
use crate::output::Output;
use crate::play::{self, Crash, GameKeys, Session};
use crate::properties::{self, Edit};
use crate::ribbon::{self, RibbonButton};
use crate::scripts::ScriptTabs;
use crate::side_warnings::{self, WarningCache};
use crate::start::{self, Brand, ProjectLink, StartAction};
use crate::theme::{self, filled_icons, icons};
use crate::tree::{self, Insertable};
use crate::viewport::{DragEdit, DragKind, Viewport};
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
    /// The Instance shown in Properties and highlighted in the Explorer and viewport.
    selected: Option<InstanceId>,
    /// Every change made to `world` since it was opened, for undo and redo.
    /// It also knows whether the world is as saved.
    history: History,
    /// The tool picked in the toolbar, and its snapping settings. Both are
    /// preferences, so they stay when another project is opened.
    tool: Tool,
    snap: Snap,
    /// Something waiting for an answer to "Save changes?".
    asking: Option<Discard>,
    /// Set once closing has been allowed, so the window's close request
    /// isn't stopped again.
    closing: bool,
    explorer: Explorer,
    output: Output,
    viewport: Viewport,
    /// The window title last sent to the window, to send it again only when it changes.
    shown_title: String,
    /// The game, while Play is on. `world` stays as it was before Play.
    play: Option<Session>,
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
    /// The example projects, for the start screen.
    examples: Vec<ProjectLink>,
    /// True while Help > About is open.
    about_open: bool,
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
    rename: bool,
    duplicate: bool,
    delete: bool,
}

/// What the editor remembers about the open project.
struct OpenProject {
    name: String,
    folder: PathBuf,
    /// Its server scripts, sorted by path, with each file's text as last
    /// read or written. Play reads their files again, for the newest code.
    scripts: Vec<ScriptFile>,
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
            history: History::new(),
            tool: Tool::default(),
            snap: Snap::default(),
            asking: None,
            closing: false,
            output: Output::default(),
            viewport: Viewport::new(),
            shown_title: String::new(),
            play: None,
            last_frame: None,
            tabs: ScriptTabs::default(),
            confirm_delete: None,
            importing: None,
            game_keys: GameKeys::default(),
            warnings: WarningCache::default(),
            theme: egui::ThemePreference::Dark,
            brand: None,
            recent: Vec::new(),
            examples: std::env::current_dir()
                .map(|folder| start::find_examples(&folder))
                .unwrap_or_default(),
            about_open: false,
        };
        match folder {
            Some(folder) => app.open(folder),
            None => app.output.info(
                "No project open. Use File > Open Project…, or start the editor with a \
                 folder: axcenity-editor examples/hello-touched",
            ),
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
        self.assets = Assets::new(&project.folder);
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
                folder,
            },
        );
        self.project = Some(OpenProject {
            name: project.manifest.name,
            watcher: Watcher::start(project.folder.clone()),
            folder: project.folder,
            scripts: project.scripts,
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
            self.recent.truncate(start::MAX_RECENT);
        }
        ctx.set_theme(self.theme);
    }

    /// Puts `world` in place of the world being edited, with nothing
    /// selected and a new undo history (its steps belong to the old world).
    fn replace_world(&mut self, world: World) {
        self.world = world;
        self.selected = None;
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
        self.selected = id;
        if let Some(id) = id {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }

    fn playing(&self) -> bool {
        self.play.is_some()
    }

    /// True when the world or a script tab has changes that aren't saved.
    /// Undoing back to where it was saved counts as no changes.
    fn unsaved(&self) -> bool {
        !self.history.is_saved() || self.tabs.any_unsaved()
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
        };
        self.change(id, key, label, vec![edit]);
    }

    /// Applies a drag from the viewport (a part or one of its handles), as
    /// an undo step such as "Move Pad".
    fn apply_drag(&mut self, edit: DragEdit) {
        let Some(instance) = self.world.get(edit.part) else {
            return;
        };
        let (property, verb) = match edit.kind {
            DragKind::Move => ("Position", "Move"),
            DragKind::Resize => ("Size", "Resize"),
            DragKind::Rotate => ("Orientation", "Rotate"),
        };
        let label = format!("{verb} {}", instance.name());
        let edits = edit
            .values
            .into_iter()
            .map(|(name, value)| Edit::Property(name, value))
            .collect();
        self.change(edit.part, Field::Property(property), label, edits);
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
        if let Some(id) = self.selected
            && !world.is_descendant_of(id, world.root())
        {
            self.selected = None;
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
            self.forget_selection_outside_tree();
        }
    }

    /// Runs one frame of the game, `seconds` long, with the keys pressed and
    /// released since the last one. LocalScripts see the viewport's camera
    /// as `workspace.CurrentCamera`, and can move it. A crash stops the game.
    fn play_frame(&mut self, seconds: f64, keys: &[KeyEvent]) {
        let Some(session) = &mut self.play else {
            return;
        };
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
            Action::Duplicate(id) => self.duplicate(id),
            // Deleting a script deletes its file, which can't be undone, so ask first.
            Action::Delete(id) if is_script(id) => self.confirm_delete = Some(id),
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
    fn insert(&mut self, what: Insertable, near: Option<InstanceId>) {
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
    fn duplicate(&mut self, id: InstanceId) {
        let Some(name) = self.editable_name(id, "duplicate") else {
            return;
        };
        let Some(original) = history::place_of(&self.world, id) else {
            return;
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
            Discard::CloseTab(path) => self.tabs.get(path).is_some_and(|tab| tab.unsaved()),
            Discard::Quit | Discard::OpenAnother => self.unsaved(),
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
                Discard::Quit | Discard::OpenAnother => self.save().then_some(discard),
            },
            Answer::DontSave => Some(discard),
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
            Discard::CloseTab(path) => self.tabs.close(&path),
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
            Discard::CloseTab(path) => (axcenity_project::script_name(path), "closing it"),
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("unsaved changes")).show(ctx, |ui| {
            ui.heading("Save changes?");
            ui.label(format!(
                "\"{name}\" has unsaved changes. Save them before {doing}?"
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(Answer::DontSave);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        // Escape or a click outside the box means Cancel.
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
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
            .show(ctx, |ui| {
                start::show(ui, brand, &self.recent, &self.examples)
            })
            .inner;
        match picked {
            Some(StartAction::OpenDialog) => self.open_with_dialog(),
            Some(StartAction::Open(folder)) => self.open(&folder),
            None => {}
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            if let Some(brand) = &self.brand {
                let tint = if ui.visuals().dark_mode {
                    theme::BRAND_BLUE_BRIGHT
                } else {
                    theme::BRAND_BLUE
                };
                ui.add(
                    egui::Image::new(&brand.mark)
                        .fit_to_exact_size(egui::vec2(20.0, 20.0))
                        .tint(tint),
                )
                .on_hover_text("Axcenity Engine");
            }
            ui.menu_button("File", |ui| {
                if ui
                    .button(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
                }
                let save = egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save"))
                    .shortcut_text(ui.ctx().format_shortcut(&SAVE_SHORTCUT));
                if ui.add_enabled(self.project.is_some(), save).clicked() {
                    self.save();
                }
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
                if ui.add_enabled(editable, duplicate).clicked()
                    && let Some(id) = selected
                {
                    self.act(Action::Duplicate(id));
                }
                let deletable =
                    selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
                let delete = egui::Button::new(theme::with_icon(icons::TRASH, "Delete"))
                    .shortcut_text(DELETE_KEY_TEXT);
                if ui.add_enabled(deletable, delete).clicked()
                    && let Some(id) = selected
                {
                    self.act(Action::Delete(id));
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
                        let shortcut = ui.ctx().format_shortcut(&tool.shortcut());
                        let button = RibbonButton::new(theme::tool_icon(tool), tool.label())
                            .selected(self.tool == tool);
                        if ui
                            .add(button)
                            .on_hover_text(format!("{} tool ({shortcut})", tool.label()))
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
                    let color = theme::class_color(ClassName::MeshPart, ui.visuals());
                    let button = RibbonButton::new(icons::FILE_ARROW_DOWN, "Import").color(color);
                    import = ui
                        .add_enabled(!playing, button)
                        .on_hover_text(IMPORT_HOVER)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked();
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
        let keys = ctx.input_mut(|input| {
            if typing {
                let tool = Tool::ALL
                    .into_iter()
                    .find(|tool| input.consume_shortcut(&tool.shortcut()));
                return Keys {
                    tool,
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
                tool: Tool::ALL
                    .into_iter()
                    .find(|tool| input.consume_shortcut(&tool.shortcut())),
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
        // Undo, duplicate and delete check for Play themselves; renaming
        // only opens a text box, so it's checked here.
        if let Some(id) = self.selected {
            if keys.rename && !self.playing() {
                self.explorer.start_rename(&self.world, id);
            }
            if keys.duplicate {
                self.act(Action::Duplicate(id));
            }
            if keys.delete {
                self.act(Action::Delete(id));
            }
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
        // Turned into a property edit by `apply`.
        Edit::PickTexture(_) => {}
    }
    Ok(())
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
            let typing = ctx.wants_keyboard_input();
            let (events, modifiers, focused) =
                ctx.input(|input| (input.events.clone(), input.modifiers, input.focused));
            let keys = self.game_keys.read(&events, modifiers, focused, typing);
            self.play_frame(seconds, &keys);
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
        let mut explorer = ExplorerOutput::default();
        let editing = !self.playing();
        // The game's world during Play, otherwise the one being edited.
        let world = shown(&self.world, &self.play);
        if has_project {
            egui::SidePanel::right("explorer")
                .default_width(300.0)
                .show(ctx, |ui| {
                    egui::TopBottomPanel::bottom("properties")
                        .resizable(true)
                        .default_height(340.0)
                        .show_inside(ui, |ui| {
                            edit = properties::show(ui, world, self.selected, !editing);
                        });
                    explorer = self.explorer.show(ui, world, self.selected, editing);
                });
        }
        // Applied before the viewport draws, so it shows the change this frame.
        if let (Some(id), Some(edit)) = (self.selected, edit) {
            self.apply(id, edit);
        }
        if let Some(id) = explorer.clicked {
            self.selected = Some(id);
        }
        if let Some(action) = explorer.action {
            self.act(action);
            // The Explorer was drawn before this change, so draw it again.
            ctx.request_repaint();
        }
        if let Some(id) = explorer.open {
            self.open_script_instance(id);
        }
        let clicked_error = egui::TopBottomPanel::bottom("output")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| self.output.show(ui))
            .inner;
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

        // Again, as the edits above may have changed which world to show.
        let world = shown(&self.world, &self.play);
        // No tool during Play, so nothing can be dragged.
        let tool = (!self.playing()).then_some(self.tool);
        let output = if !has_project {
            self.start_screen(ctx);
            Default::default()
        } else if self.tabs.world_shown() {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| match frame.wgpu_render_state() {
                    Some(render_state) => self.viewport.show(
                        ui,
                        render_state,
                        world,
                        &self.assets,
                        self.selected,
                        tool,
                        self.snap,
                    ),
                    // Can't happen: main.rs always picks the wgpu renderer.
                    None => {
                        ui.label("The viewport needs the wgpu renderer.");
                        Default::default()
                    }
                })
                .inner
        } else {
            egui::CentralPanel::default().show(ctx, |ui| self.tabs.show_active(ui));
            Default::default()
        };
        if let Some(clicked) = output.clicked {
            self.select_from_viewport(clicked.part);
            // The panels were already drawn this frame, so draw again to
            // show the new selection there.
            ctx.request_repaint();
        }
        if let Some(edit) = output.drag {
            self.apply_drag(edit);
            // The viewport was drawn before this change, so draw it again.
            ctx.request_repaint();
        }

        // A drag or a typed value is one undo step: it ends when the mouse
        // is let go and no text box is being typed in.
        let busy = ctx.is_using_pointer() || ctx.wants_keyboard_input();
        if output.drag_released || (!busy && !self.viewport.is_dragging()) {
            self.history.seal();
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

        self.unsaved_changes_prompt(ctx);
        self.delete_script_prompt(ctx);
        self.import_prompt(ctx);
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
    fn starting_without_a_project_explains_how_to_open_one() {
        let app = EditorApp::new(None);
        assert_eq!(app.title(), "Axcenity Editor");
        assert!(last_line(&app).text.contains("File > Open Project"));
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
            .find(|item| item.material.emissive == [0.1, 0.725, 0.333])
            .expect("the pad glows in its new color");
        let center = drawn_pad.transform.transform_point3(Vec3::ZERO);
        assert!(center.abs_diff_eq(Vec3::new(1.25, 0.3, -7.1), 1e-6));
        let corner = drawn_pad.transform.transform_point3(Vec3::splat(0.5));
        assert!((corner - center).abs_diff_eq(Vec3::new(1.65, 0.0005, 6.0), 1e-6));
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
                part: id,
                kind: DragKind::of(handle),
                values,
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
        assert_eq!(property(&app, pad, "Orientation"), vector(0.0, 0.0, 90.0));
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
                part: ball,
                kind: DragKind::Move,
                values: vec![("Position", position)],
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
            app.play_frame(1.0 / 60.0, &[]);
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
        app.play_frame(1.0 / 60.0, &[]);

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
}
