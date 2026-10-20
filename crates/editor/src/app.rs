//! The editor window: a menu bar, the ribbon with its tabs, and dockable
//! panels (`layout.rs`): by default the Explorer and Properties on the
//! right, Output and Assets along the bottom, and the Scene (the 3D
//! viewport) filling the rest, like Roblox Studio.
//!
//! Play runs the game on a copy of the world (see `play.rs`). While it
//! plays, every panel shows that copy. Only the Properties panel can change
//! it (the game's scripts see those changes); everything else is read-only.
//! Stop drops the copy.
//!
//! Above the viewport, tabs switch between the viewport ("World") and the
//! open scripts.
//!
//! Where to change behavior:
//! - `app.rs` — the frame, saved settings, and the dock shell
//! - `session.rs` — open, close, Play, Stop, and the unsaved-changes prompt
//! - `edits.rs` — selection, tools, property and style edits, undo
//! - `tree_ops.rs` — insert, duplicate, delete, reparent
//! - `chrome.rs` — menu bar, ribbon, panels, shortcuts
//! - `files.rs` — save, scripts, import, and files changed outside the editor
//! - `assets.rs` — Assets panel drops, prefabs, and imported files

mod assets;
mod chrome;
mod edits;
mod files;
mod session;
mod tree_ops;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_blocks::{BlockScript, translate};
use axcenity_core::{ClassName, Color3, InstanceId, Material, PartType, Value, Vector3, World};
use axcenity_project::ScriptFile;
use axcenity_scene::orientation_from_rotation;
use axcenity_script::{CameraState, KeyEvent};
use eframe::egui;

use crate::angular;
use crate::animation::{AnimationStudio, SelectedModel, StudioAction};
use crate::asset_browser::{AssetBrowser, AssetPayload, BrowserAction};
use crate::autocomplete;
use crate::block_editor;
use crate::build_tools::{self, BuildSettings, PanelScene, ToolAction};
use crate::dialog;
use crate::explorer::{self, Action, Explorer};
use crate::gizmo::{Modes, Pose, Snap, Tool};
use crate::history::{self, Field, History, Move, Place};
use crate::import_dialog::ImportDialog;
use crate::layout::{self, Layout, Pane};
use crate::output::{Output, OutputKind};
use crate::play::{self, Crash, GameKeys, Session};
use crate::properties::{self, Edit};
use crate::ribbon::{self, RibbonButton};
use crate::ribbon_tabs::{RibbonGroup, RibbonTabs};
use crate::scripts::{self, ScriptTabs};
use crate::side_warnings::{self, WarningCache};
use crate::start::{self, Brand, ProjectLink, StartAction, StartScreen};
use crate::style_panel::{self, StyleEdit};
use crate::theme::{self, filled_icons, icons};
use crate::thumbnail;
use crate::tree::{self, Insertable};
use crate::ui_handles::{CssEdit, UiDragEdit, apply_css_edits};
use crate::viewport::{self, DragEdit, DragKind, Viewport, ViewportOutput, ViewportScene};
use crate::watcher::Watcher;
use eframe::egui_wgpu::RenderState;
use egui_dock::DockState;

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
    /// Particle bursts for the viewport's next frame: `Emit` calls from
    /// scripts during Play, and the Emission tool's Burst.
    pending_emits: Vec<(InstanceId, u32)>,
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
    /// The Animation panel's clips, open clip and preview.
    animation: AnimationStudio,
    /// Where the dockable panels are. Saved between runs.
    layout: Layout,
    /// The ribbon's tabs, including the ones people made. Saved between runs.
    ribbon_tabs: RibbonTabs,
    /// View > Light Cones: show every light's reach in the viewport, not
    /// just the selected one's. Saved between runs.
    light_cones: bool,
}

/// Ribbon buttons that act on the whole app, noted while the ribbon is
/// drawn and acted on after.
#[derive(Default)]
struct RibbonClicks {
    insert: Option<Insertable>,
    insert_script: Option<ClassName>,
    import: bool,
    play: bool,
    stop: bool,
}

/// Draws each dockable panel for `egui_dock`.
struct DockViewer<'a> {
    app: &'a mut EditorApp,
    render_state: Option<&'a RenderState>,
    /// What happened in the viewport, handled once everything is drawn.
    viewport: &'a mut ViewportOutput,
}

impl egui_dock::TabViewer for DockViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, pane: &mut Pane) -> egui::WidgetText {
        pane.title().into()
    }

    fn id(&mut self, pane: &mut Pane) -> egui::Id {
        egui::Id::new(("dock pane", *pane))
    }

    fn ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane) {
        match pane {
            Pane::Scene => *self.viewport = self.app.scene_pane(ui, self.render_state),
            Pane::Explorer => self.app.explorer_pane(ui),
            Pane::Properties => self.app.properties_pane(ui),
            Pane::Output => self.app.output_pane(ui),
            Pane::Assets => self.app.assets_pane(ui),
            Pane::Animation => self.app.animation_pane(ui, self.render_state),
        }
    }

    fn is_closeable(&self, pane: &Pane) -> bool {
        *pane != Pane::Scene
    }

    /// The Scene stays docked, so the main window is never left empty.
    fn allowed_in_windows(&self, pane: &mut Pane) -> bool {
        *pane != Pane::Scene
    }

    /// Each panel scrolls its own contents.
    fn scroll_bars(&self, _pane: &Pane) -> [bool; 2] {
        [false, false]
    }

    /// The viewport goes right to the panel's edges.
    fn tab_style_override(
        &self,
        pane: &Pane,
        global: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        (*pane == Pane::Scene).then(|| {
            let mut style = global.clone();
            style.tab_body.inner_margin = egui::Margin::ZERO;
            style
        })
    }
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
const LAYOUT_KEY: &str = "panel layout";
const RIBBON_TABS_KEY: &str = "ribbon tabs";
const LIGHT_CONES_KEY: &str = "light cones";

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
            pending_emits: Vec::new(),
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
            animation: AnimationStudio::default(),
            layout: Layout::default(),
            ribbon_tabs: RibbonTabs::default(),
            light_cones: false,
        };
        if let Some(folder) = folder {
            app.open(folder);
        }
        app
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
            if let Some(text) = storage.get_string(LAYOUT_KEY) {
                self.layout = Layout::from_text(&text);
            }
            if let Some(text) = storage.get_string(RIBBON_TABS_KEY) {
                self.ribbon_tabs = RibbonTabs::from_text(&text);
            }
            self.light_cones = storage.get_string(LIGHT_CONES_KEY).as_deref() == Some("on");
        }
        ctx.set_theme(self.theme);
    }
}

fn quoted_list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
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
        // Every panel is a tab in the docking area, which fills the rest of
        // the window (see `layout.rs`). Each panel acts on what was done in
        // it as soon as it's drawn; the viewport's output is handled below.
        self.viewport_rect = None;
        let mut output = ViewportOutput::default();
        if has_project {
            // Taken out while it's drawn, so the panels can borrow the app.
            let mut dock = std::mem::replace(&mut self.layout.dock, DockState::new(Vec::new()));
            egui_dock::DockArea::new(&mut dock)
                .style(layout::style(&ctx.style()))
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show(
                    ctx,
                    &mut DockViewer {
                        app: self,
                        render_state: frame.wgpu_render_state(),
                        viewport: &mut output,
                    },
                );
            self.layout.dock = dock;
        } else {
            self.start_screen(ctx);
        }
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
            } else if output.viewport_clicked
                && let Some(id) = self.focused_text.take()
            {
                self.pending_focus_lost = Some(id);
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
        self.ribbon_tabs.editor_window(ctx);
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
        storage.set_string(LAYOUT_KEY, self.layout.to_text());
        storage.set_string(RIBBON_TABS_KEY, self.ribbon_tabs.to_text());
        let cones = if self.light_cones { "on" } else { "off" };
        storage.set_string(LIGHT_CONES_KEY, cones.to_owned());
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

        app.select_from_viewport(Some(wheel));
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
        app.select_from_viewport(Some(pad));
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

    #[test]
    fn the_angular_tool_places_its_copies_in_one_undo_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let workspace = app.world.workspace();
        let block = inserted(&mut app, Insertable::Part(PartType::Block), None);
        let before = children(&app, workspace).len();
        app.tool = Tool::Angular;
        app.build.angular.count = 23;
        let preview = app.angular_preview();
        assert_eq!(preview.len(), 23);
        app.apply_tool_action(ToolAction::Extend);
        assert_eq!(children(&app, workspace).len(), before + 23);
        let last = app.selected.unwrap();
        assert_ne!(last, block);
        let pose = Pose::of(app.world.get(last).unwrap()).unwrap();
        assert!(pose.position.abs_diff_eq(preview[22].position, 1e-3));
        assert_eq!(app.history.undo_label(), Some("Extend Part 23 times"));
        app.undo();
        assert_eq!(children(&app, workspace).len(), before);
    }
}
