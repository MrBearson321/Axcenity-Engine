//! The editor window: a menu bar, the ribbon with its tabs, and dockable
//! panels (`layout.rs`): by default the Explorer and Properties on the
//! right, the Console and Assets along the bottom, and the Scene (the 3D
//! viewport) filling the rest, like Roblox Studio.
//!
//! Play runs the game on a copy of the world (see `play.rs`). While it
//! plays, every panel shows that copy. Only the Properties panel can change
//! it (the game's scripts see those changes); everything else is read-only.
//! Stop drops the copy.
//!
//! Open scripts get tabs at the top of the Scene View; clicking the Scene
//! View's own tab shows the viewport again.
//!
//! Where to change behavior:
//! - `app.rs` — the frame, saved settings, and the dock shell
//! - `session.rs` — open, close, Play, Stop, and the unsaved-changes prompt
//! - `edits.rs` — selection, tools, property and style edits, undo
//! - `tree_ops.rs` — insert, duplicate, delete, reparent
//! - `chrome.rs` — menu bar, ribbon, panels, shortcuts
//! - `commands.rs` — the Console panel and what typed commands do
//! - `files.rs` — save, scripts, import, and files changed outside the editor
//! - `assets.rs` — Assets panel drops, prefabs, and imported files
//! - `asset_fixes.rs` — the Asset Check scan and its fixes

mod asset_fixes;
mod assets;
mod behavior_trees;
mod chrome;
mod commands;
mod edits;
mod engine;
mod files;
mod material_files;
mod nav_overlay;
mod session;
mod settings;
mod tags;
mod terrain;
mod texture_set;
mod tree_ops;
mod ui_graphs;
mod ui_selection;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_blocks::{BlockScript, translate};
use axcenity_core::{ClassName, Color3, InstanceId, Material, PartType, Value, Vector3, World};
use axcenity_project::ScriptFile;
use axcenity_scene::{TerrainCache, orientation_from_rotation};
use axcenity_script::{CameraState, KeyEvent};
use eframe::egui;

use crate::angular;
use crate::animation::{AnimationStudio, StudioAction};
use crate::asset_browser::{AssetBrowser, AssetPayload, BrowserAction};
use crate::asset_check::AssetCheck;
use crate::autocomplete;
use crate::behavior_tree::BehaviorTreeStudio;
use crate::block_editor;
use crate::build_tools::{self, BuildSettings, PanelScene, ToolAction};
use crate::console::Console;
use crate::dialog;
use crate::explorer::{self, Action, Explorer};
use crate::gizmo::{Modes, Pose, Snap, Tool};
use crate::history::{self, Field, History, Move, Place};
use crate::hot_patch;
use crate::import_dialog::ImportDialog;
use crate::layout::{self, Layout, Pane};
use crate::loading::{self, Loading};
use crate::output::Output;
use crate::play::{self, Crash, GameKeys, Session};
use crate::profiler::Profiler;
use crate::properties::{self, Edit};
use crate::rebuild::{EngineSource, Rebuilds};
use crate::relaunch::Handoff;
use crate::ribbon::{self, RibbonButton};
use crate::ribbon_tabs::{RibbonGroup, RibbonTabs};
use crate::scripts::{self, ScriptTabs};
use crate::side_warnings::{self, WarningCache};
use crate::splash::Splash;
use crate::start::{self, Brand, ProjectLink, StartAction, StartScreen};
use crate::style_panel::{self, StyleEdit};
use crate::terrain_studio::TerrainStudio;
use crate::theme::{self, Appearance, filled_icons, icons};
use crate::thumbnail;
use crate::toast::{self, Toasts};
use crate::tree::{self, Insertable};
use crate::ui_designer::{DesignerAction, DesignerScene, GraphPane, UiDesigner};
use crate::ui_handles::{CssEdit, UiDragEdit, apply_css_edits};
use crate::viewport::{
    self, DragEdit, DragKind, OverlayToggles, Viewport, ViewportOutput, ViewportScene,
};
use crate::watcher::Watcher;
use crate::widgets;
use crate::window_frame;
use eframe::egui_wgpu::RenderState;
use egui_dock::DockState;

/// The hover text of Import…, in the File menu and the ribbon.
const IMPORT_HOVER: &str = "Import models (.gltf, .glb or .fbx), images, sounds and prefabs. A \
    model opens the Import dialog and is inserted at the camera's focus point; images and \
    sounds are copied into the project's assets and reported in the Console. You can also \
    drop files onto the editor.";

/// F5 plays and Shift+F5 stops, as in Roblox Studio.
const PLAY_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F5);
const STOP_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::F5);
/// During Play, after a long pause (such as dragging the window), don't jump
/// the game ahead by the whole pause; treat the frame as at most this long.
/// The same cap as the player's.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// How many frames the loading screen stays up after a project opens.
const FINISHING_FRAMES: u64 = 2;
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
    /// The Console's log.
    output: Output,
    /// The Console's command line. Its history is saved between runs.
    console: Console,
    /// A line typed into the Console, run once the dock is drawn.
    pending_command: Option<String>,
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
    /// `MouseEnter`, `MouseLeave` and buttons' sounds from the viewport,
    /// for the next Play frame.
    pending_ui_events: Vec<axcenity_ui::UiEvent>,
    /// Key presses for the game UI's focus, taken out of egui's input
    /// during Play (`take_nav_keys`) and used before the next game frame.
    nav_keys: Vec<axcenity_ui::NavKey>,
    /// Game controllers, opened the first time a game frame runs.
    gamepads: Option<axcenity_ui::Gamepads>,
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
    /// What Play's frames cost, for the Profiler panel.
    profiler: Profiler,
    /// The last asset check, for the Asset Check panel.
    asset_check: AssetCheck,
    /// The Tags panel's rename box.
    tags: crate::tags::TagsPanel,
    /// Each script's uses of APIs from the other side, for the warning badges.
    warnings: WarningCache,
    /// The open script tabs, and whether the viewport or a script is shown.
    tabs: ScriptTabs,
    /// A Script waiting for an answer to "Delete script?".
    confirm_delete: Option<InstanceId>,
    /// A model file waiting for its objects to be chosen.
    importing: Option<ImportDialog>,
    /// Model files imported together, waiting for the dialog to be free.
    import_queue: VecDeque<PathBuf>,
    /// The theme, accent, density and motion, picked in Settings. Saved between runs.
    appearance: Appearance,
    /// The Settings window.
    settings: settings::SettingsWindow,
    /// Confirmations such as "Saved", in the bottom-right corner.
    toasts: Toasts,
    /// The logo images, loaded on the first frame (they need the egui context).
    brand: Option<Brand>,
    /// The launch splash, timed from the first frame the window is drawn.
    splash: Splash,
    /// The project being read in the background, if one is (see `loading.rs`).
    loading: Option<Loading>,
    /// The frame (egui's count) the loading screen goes at, after a
    /// project opened. Frames, not calls: egui may draw a frame twice.
    cover_until: Option<u64>,
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
    /// True while Help > Learn is open over the project.
    learn_open: bool,
    /// A Sound being listened to from the Properties panel, outside Play.
    sound_preview: Option<axcenity_audio::AudioEngine>,
    /// The Assets panel, which shares the bottom of the window with Output.
    asset_browser: AssetBrowser,
    /// The Animation panel's clips, open clip and preview.
    animation: AnimationStudio,
    /// The Behavior Tree tab and the trees open in it.
    behavior: BehaviorTreeStudio,
    /// The UI Designer tab's view of a ScreenGui.
    ui_designer: UiDesigner,
    /// The Terrain panel's tools, settings, previews and work under way.
    terrain: TerrainStudio,
    /// Where the dockable panels are. Saved between runs.
    layout: Layout,
    /// The ribbon's tabs, including the ones people made. Saved between runs.
    ribbon_tabs: RibbonTabs,
    /// View > Light Cones: show every light's reach in the viewport, not
    /// just the selected one's. Saved between runs.
    light_cones: bool,
    /// View overlay toggles (colliders, raycasts, bounds, emitters, the
    /// navigation mesh). Saved between runs. The player never draws them.
    overlays: OverlayToggles,
    /// The navigation mesh the overlay draws, while it's on.
    nav_mesh: Option<nav_overlay::NavCache>,
    /// This computer's graphics settings (Settings > Graphics), for the
    /// viewport and Play. Saved between runs, never in a project.
    graphics: axcenity_render::GraphicsSettings,
    /// What the GPU can do, read on the first frame that has one.
    capabilities: Option<axcenity_render::Capabilities>,
    /// Settings > Engine: rebuilding the editor when engine code changes
    /// (a debug editor run from the source tree only; see `engine.rs`).
    rebuilds: Rebuilds,
    /// A handoff written for the rebuilt editor, which `update` starts.
    launching: Option<PathBuf>,
    /// A handoff from the editor this one was relaunched from, with its
    /// folder, applied once its project has opened.
    pending_handoff: Option<(PathBuf, Handoff)>,
}

/// Ribbon buttons that act on the whole app, noted while the ribbon is
/// drawn and acted on after.
#[derive(Default)]
struct RibbonClicks {
    insert: Option<Insertable>,
    insert_script: Option<ClassName>,
    import: bool,
    check_assets: bool,
    play: bool,
    stop: bool,
}

/// Draws each dockable panel for `egui_dock`.
struct DockViewer<'a> {
    app: &'a mut EditorApp,
    render_state: Option<&'a RenderState>,
    /// What happened in the viewport, handled once everything is drawn.
    viewport: &'a mut ViewportOutput,
    /// For script tab titles, which use class colours.
    visuals: egui::Visuals,
    ctx: egui::Context,
}

impl egui_dock::TabViewer for DockViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, pane: &mut Pane) -> egui::WidgetText {
        match pane {
            Pane::Script(id) => self.app.tabs.title(*id, &self.visuals),
            _ => pane.title(),
        }
    }

    fn id(&mut self, pane: &mut Pane) -> egui::Id {
        egui::Id::new(("dock pane", *pane))
    }

    fn ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane) {
        match pane {
            Pane::Scene => *self.viewport = self.app.scene_pane(ui, self.render_state),
            Pane::Explorer => self.app.explorer_pane(ui),
            Pane::Properties => self.app.properties_pane(ui),
            Pane::Console => self.app.console_pane(ui),
            Pane::Assets => self.app.assets_pane(ui),
            Pane::Animation => self.app.animation_pane(ui, self.render_state),
            Pane::UiDesigner => self.app.ui_designer_pane(ui, self.render_state),
            Pane::Terrain => self.app.terrain_pane(ui),
            Pane::Profiler => self.app.profiler.show(ui),
            Pane::AssetCheck => self.app.asset_check_pane(ui),
            Pane::BehaviorTree => self.app.behavior_tree_pane(ui),
            Pane::Tags => self.app.tags_pane(ui),
            Pane::Script(id) => self.app.script_pane(ui, *id),
        }
    }

    fn is_closeable(&self, pane: &Pane) -> bool {
        *pane != Pane::Scene
    }

    /// A script tab stays until the app closes its script, after asking
    /// about unsaved changes; then `Layout::sync_scripts` removes it.
    fn on_close(&mut self, pane: &mut Pane) -> egui_dock::tab_viewer::OnCloseResponse {
        if let Pane::Script(id) = pane {
            if let Some(path) = self.app.tabs.path_of(*id).map(str::to_owned) {
                self.app
                    .request_and_go_ahead(&self.ctx, Discard::CloseTab(path));
            }
            return egui_dock::tab_viewer::OnCloseResponse::Ignore;
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    /// Clicking a tab makes it the one used last: the Scene, or a script.
    fn on_tab_button(&mut self, pane: &mut Pane, response: &egui::Response) {
        if !response.clicked() {
            return;
        }
        match pane {
            Pane::Scene => self.app.tabs.show_world(),
            Pane::Script(id) => self.app.tabs.activate(*id),
            _ => {}
        }
    }

    /// The Scene stays docked, so the main window is never left empty.
    fn allowed_in_windows(&self, pane: &mut Pane) -> bool {
        *pane != Pane::Scene
    }

    /// Each panel scrolls its own contents.
    fn scroll_bars(&self, _pane: &Pane) -> [bool; 2] {
        [false, false]
    }

    /// The viewport and scripts go right to the panel's edges.
    fn tab_style_override(
        &self,
        pane: &Pane,
        global: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        matches!(pane, Pane::Scene | Pane::Script(_)).then(|| {
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
/// Only read, from saves made before Settings had more than the theme.
const THEME_KEY: &str = "theme";
const APPEARANCE_KEY: &str = "appearance";
const RECENT_KEY: &str = "recent projects";
const LAYOUT_KEY: &str = "panel layout";
const RIBBON_TABS_KEY: &str = "ribbon tabs";
const LIGHT_CONES_KEY: &str = "light cones";
const OVERLAY_COLLIDERS_KEY: &str = "overlay colliders";
const OVERLAY_RAYCASTS_KEY: &str = "overlay raycasts";
const OVERLAY_BOUNDS_KEY: &str = "overlay bounds";
const OVERLAY_EMITTERS_KEY: &str = "overlay emitters";
const OVERLAY_NAV_MESH_KEY: &str = "overlay nav mesh";
const OVERLAY_AGENT_PATHS_KEY: &str = "overlay agent paths";
const CONSOLE_HISTORY_KEY: &str = "console history";
/// The graphics settings, as JSON.
const GRAPHICS_KEY: &str = "graphics";
/// eframe's own key for the window's size and position. eframe still reads
/// it at startup with `persist_window` off, so `save` blanks it.
const WINDOW_KEY: &str = "window";

/// Something that throws unsaved changes away, so it has to ask first if
/// there are any.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Discard {
    Quit,
    OpenAnother,
    /// Opening a tutorial's sample from Help > Learn.
    OpenSample(PathBuf),
    /// Closing the tab of the script at this path.
    CloseTab(String),
    /// Going back to the start screen.
    CloseProject,
    /// File > New Project…: back to the start screen, asking for the new one.
    NewProject,
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
    /// How built games' loading screen goes (File > Build Game).
    loading_screen: axcenity_project::LoadingScreen,
    folder: PathBuf,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    /// Each stylesheet's text as last read or written, by path, to tell the
    /// editor's own saves from changes made elsewhere.
    stylesheet_texts: HashMap<String, String>,
    /// Its server scripts, sorted by path, with each file's text as last
    /// read or written. Play reads their files again, for the newest code.
    scripts: Vec<ScriptFile>,
    block_scripts: Vec<BlockDocument>,
    /// Each ScreenGui's UI Event Graph, as edited (see `app/ui_graphs.rs`).
    ui_graphs: Vec<ui_graphs::UiGraphDocument>,
    /// Blocks switched to Code, by the code file's path, with the code they
    /// became: switched back with that code unchanged, they come back as
    /// they were (loose blocks and where each stack was).
    remembered_blocks: HashMap<String, (String, BlockScript)>,
    /// Notices files changed by other programs.
    watcher: Watcher,
    /// Changed files that wait for Stop: the world, scripts and
    /// stylesheets changed during Play.
    after_play: Vec<String>,
    /// The world file's text as last read or written, to tell the editor's
    /// own saves from changes made elsewhere. `None` if it couldn't be read.
    world_text: Option<String>,
    /// True when the world file holds something the editor never loaded:
    /// it was damaged, or changed elsewhere while the world had unsaved
    /// changes. Saving keeps it as `world.backup.json` first.
    keep_world_file: bool,
    /// Terrains' data folders that couldn't be loaded: saving keeps each as
    /// `<folder>.backup` before writing over it.
    damaged_terrains: Vec<String>,
    /// Each terrain's landscape (number and revision) as last loaded or
    /// saved, so saving writes only the ones that changed.
    saved_terrains: HashMap<InstanceId, (u64, u64)>,
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
            console: Console::default(),
            pending_command: None,
            viewport: Viewport::new(),
            ui_drag_baselines: HashMap::new(),
            shown_title: String::new(),
            play: None,
            pending_ui_activation: None,
            pending_ui_events: Vec::new(),
            nav_keys: Vec::new(),
            gamepads: None,
            focused_text: None,
            pending_focus_lost: None,
            ui_warnings: Vec::new(),
            last_frame: None,
            profiler: Profiler::default(),
            asset_check: AssetCheck::default(),
            tags: crate::tags::TagsPanel::default(),
            tabs: ScriptTabs::default(),
            confirm_delete: None,
            importing: None,
            import_queue: VecDeque::new(),
            game_keys: GameKeys::default(),
            warnings: WarningCache::default(),
            appearance: Appearance::default(),
            settings: settings::SettingsWindow::default(),
            toasts: Toasts::default(),
            brand: None,
            splash: Splash::default(),
            loading: None,
            cover_until: None,
            recent: Vec::new(),
            start: StartScreen::default(),
            thumbnail: None,
            thumbnail_wanted: None,
            viewport_rect: None,
            about_open: false,
            learn_open: false,
            sound_preview: None,
            asset_browser: AssetBrowser::default(),
            animation: AnimationStudio::default(),
            behavior: BehaviorTreeStudio::default(),
            ui_designer: UiDesigner::default(),
            terrain: TerrainStudio::default(),
            layout: Layout::default(),
            ribbon_tabs: RibbonTabs::default(),
            light_cones: false,
            overlays: OverlayToggles::default(),
            nav_mesh: None,
            graphics: axcenity_render::GraphicsSettings::default(),
            capabilities: None,
            rebuilds: Rebuilds {
                source: EngineSource::find(),
                ..Rebuilds::default()
            },
            launching: None,
            pending_handoff: None,
        };
        if let Some(folder) = folder {
            app.open(folder);
        }
        app
    }

    /// Restores the settings saved by [`eframe::App::save`] (none in tests, or
    /// on the first run) and applies the look.
    pub fn load_settings(&mut self, ctx: &egui::Context, storage: Option<&dyn eframe::Storage>) {
        if let Some(storage) = storage {
            self.appearance = match storage.get_string(APPEARANCE_KEY) {
                Some(text) => Appearance::from_text(&text),
                None => Appearance {
                    theme: match storage.get_string(THEME_KEY).as_deref() {
                        Some("light") => theme::ThemeChoice::Light,
                        Some("system") => theme::ThemeChoice::System,
                        _ => theme::ThemeChoice::Dark,
                    },
                    ..Appearance::default()
                },
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
            self.overlays.colliders =
                storage.get_string(OVERLAY_COLLIDERS_KEY).as_deref() == Some("on");
            self.overlays.raycasts =
                storage.get_string(OVERLAY_RAYCASTS_KEY).as_deref() == Some("on");
            self.overlays.bounds = storage.get_string(OVERLAY_BOUNDS_KEY).as_deref() == Some("on");
            self.overlays.emitters =
                storage.get_string(OVERLAY_EMITTERS_KEY).as_deref() == Some("on");
            self.overlays.nav_mesh =
                storage.get_string(OVERLAY_NAV_MESH_KEY).as_deref() == Some("on");
            self.overlays.agent_paths =
                storage.get_string(OVERLAY_AGENT_PATHS_KEY).as_deref() == Some("on");
            if let Some(text) = storage.get_string(CONSOLE_HISTORY_KEY) {
                self.console.load_history(&text);
            }
            if let Some(settings) = storage
                .get_string(GRAPHICS_KEY)
                .and_then(|text| serde_json::from_str(&text).ok())
            {
                self.graphics = settings;
            }
            self.rebuilds.on_change =
                storage.get_string(engine::REBUILD_KEY).as_deref() == Some("on");
            self.rebuilds.relaunch_automatically =
                storage.get_string(engine::RELAUNCH_KEY).as_deref() == Some("on");
            self.rebuilds.expected = storage
                .get_string(engine::BUILD_UNITS_KEY)
                .and_then(|text| text.parse().ok())
                .unwrap_or(0);
        }
        theme::apply(ctx, &self.appearance);
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
    /// Takes the keys that move the game UI's focus during Play before
    /// egui sees them, so Tab and the arrows don't move egui's own focus.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.take_nav_keys(ctx, raw_input);
    }

    /// Called by eframe to draw each frame (only when something changed,
    /// such as mouse movement, unless a frame is requested).
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // While dx serves the editor, through the newest code it patched in.
        let patched = self.rebuilds.patched_frames();
        hot_patch::call(patched, || self.frame(ctx, frame.wgpu_render_state()));
        // Relaunching into a rebuilt editor saves the settings first.
        self.launch(ctx, frame);
        // Tests call `frame` without a window, so the window's resize edges
        // and the splash live here.
        window_frame::edges(ctx);
        if let Some(brand) = &self.brand {
            let project = match &self.loading {
                Some(loading) => Some(loading.name.as_str()),
                None => self.project.as_ref().map(|project| project.name.as_str()),
            };
            self.splash
                .show(ctx, brand, project, self.appearance.reduce_motion);
        }
    }

    /// Saves the look, the layout and the recent projects. eframe calls
    /// this now and then, and when the window closes.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(APPEARANCE_KEY, self.appearance.to_text());
        let recent: Vec<String> = self
            .recent
            .iter()
            .map(|link| link.folder.display().to_string())
            .collect();
        storage.set_string(LAYOUT_KEY, self.layout.to_text());
        storage.set_string(RIBBON_TABS_KEY, self.ribbon_tabs.to_text());
        let cones = if self.light_cones { "on" } else { "off" };
        storage.set_string(LIGHT_CONES_KEY, cones.to_owned());
        let flag = |on| if on { "on" } else { "off" };
        storage.set_string(
            OVERLAY_COLLIDERS_KEY,
            flag(self.overlays.colliders).to_owned(),
        );
        storage.set_string(
            OVERLAY_RAYCASTS_KEY,
            flag(self.overlays.raycasts).to_owned(),
        );
        storage.set_string(OVERLAY_BOUNDS_KEY, flag(self.overlays.bounds).to_owned());
        storage.set_string(
            OVERLAY_EMITTERS_KEY,
            flag(self.overlays.emitters).to_owned(),
        );
        storage.set_string(
            OVERLAY_NAV_MESH_KEY,
            flag(self.overlays.nav_mesh).to_owned(),
        );
        storage.set_string(
            OVERLAY_AGENT_PATHS_KEY,
            flag(self.overlays.agent_paths).to_owned(),
        );
        storage.set_string(CONSOLE_HISTORY_KEY, self.console.history_text());
        if let Ok(text) = serde_json::to_string(&self.graphics) {
            storage.set_string(GRAPHICS_KEY, text);
        }
        storage.set_string(
            engine::REBUILD_KEY,
            flag(self.rebuilds.on_change).to_owned(),
        );
        storage.set_string(
            engine::RELAUNCH_KEY,
            flag(self.rebuilds.relaunch_automatically).to_owned(),
        );
        storage.set_string(engine::BUILD_UNITS_KEY, self.rebuilds.expected.to_string());
        storage.set_string(RECENT_KEY, recent.join("\n"));
        // An empty value doesn't parse, so the next start uses the default size.
        storage.set_string(WINDOW_KEY, String::new());
    }
}

impl EditorApp {
    /// One frame: game time during Play, then the menu bar, the ribbon and
    /// the dock (or the start screen), then windows and overlays. Separate
    /// from `update` so tests can draw it without a window.
    pub(crate) fn frame(&mut self, ctx: &egui::Context, render_state: Option<&RenderState>) {
        // The window's close button: stop it and ask first if there are
        // unsaved changes. `closing` lets the close through once allowed.
        if ctx.input(|input| input.viewport().close_requested()) && !self.closing && self.unsaved()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.asking = Some(Discard::Quit);
        }
        if self.brand.is_none() {
            self.brand = Some(Brand::load(ctx));
        }
        if self.capabilities.is_none()
            && let Some(render_state) = render_state
        {
            self.capabilities = Some(axcenity_render::Capabilities::read(
                &render_state.device,
                Some(&render_state.adapter.get_info()),
            ));
        }
        // Engine code changed: rebuild in the background (debug editors only).
        self.engine_frame(ctx);
        // While a project loads, only the header and the loading screen.
        if self.follow_loading(ctx) {
            self.loading_frame(ctx);
            return;
        }
        // A texture dropped on or set on something is read on other threads,
        // so the window keeps going while a 4K set decodes (the bottom-right
        // note says how far it is). Each opened project's assets start
        // reading on the frame they arrive.
        if !self.assets.loads_textures_in_background() {
            let ctx = ctx.clone();
            self.assets
                .load_textures_in_background(move || ctx.request_repaint());
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
        if ctx.input_mut(|input| input.consume_shortcut(&settings::SETTINGS_SHORTCUT)) {
            self.settings.open = !self.settings.open;
        }
        self.terrain_frame(ctx);
        self.shortcuts(ctx);

        // During Play, run a game frame before anything is drawn, so every
        // panel shows the world as it is now. Then ask for the next frame
        // straight away: games keep moving without input. Paused at a
        // breakpoint, the game holds still and waits for Continue.
        self.sync_breakpoints();
        if self.playing() && !self.paused() {
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
            // Focus moves before the game's input, as in the player.
            self.navigate_play_ui(seconds as f32);
            let keys = self.game_keys.read(&events, modifiers, focused, typing);
            let activation = self.pending_ui_activation.take();
            let focus_lost = self.pending_focus_lost.take();
            self.play_frame(seconds, &keys, activation, focus_lost);
            ctx.request_repaint();
        }
        // However Play ended (Stop, a crash, another project), the
        // Profiler's readings freeze.
        if !self.playing() {
            self.profiler.stop();
        }

        let title = self.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }

        // With no project open, the start screen takes the place of the
        // toolbar, the Explorer, the tabs and the viewport.
        let has_project = self.project.is_some();

        // Panels take their space in the order they're added: the menu bar
        // spans the whole top, then the Explorer (with Properties under it)
        // runs down the whole right side, then the Output fits under the viewport.
        self.update_warnings();
        let palette = theme::palette(ctx);
        // The header is the window's only title bar (see `window_frame.rs`).
        egui::TopBottomPanel::top("menu bar")
            .exact_height(window_frame::HEIGHT)
            .frame(egui::Frame::new().fill(palette.app_bg))
            .show(ctx, |ui| self.title_bar(ui));
        if has_project {
            // The toolbar draws its own bands: the tabs on the app
            // background, the ribbon on a panel's (see `chrome.rs`).
            egui::TopBottomPanel::top("toolbar")
                .frame(egui::Frame::new().fill(palette.app_bg))
                .show_separator_line(false)
                .show(ctx, |ui| self.toolbar(ui));
        }
        // Every panel is a tab in the docking area, which fills the rest of
        // the window (see `layout.rs`). Each panel acts on what was done in
        // it as soon as it's drawn; the viewport's output is handled below.
        self.viewport_rect = None;
        let mut output = ViewportOutput::default();
        if has_project {
            self.layout.sync_scripts(
                &self.tabs.ids(),
                self.tabs.take_focus(),
                self.tabs.active_id(),
            );
            // Taken out while it's drawn, so the panels can borrow the app.
            let mut dock = std::mem::replace(&mut self.layout.dock, DockState::new(Vec::new()));
            egui_dock::DockArea::new(&mut dock)
                .style(layout::style(&ctx.style(), &palette))
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show(
                    ctx,
                    &mut DockViewer {
                        app: self,
                        render_state,
                        viewport: &mut output,
                        visuals: ctx.style().visuals.clone(),
                        ctx: ctx.clone(),
                    },
                );
            self.layout.dock = dock;
            if let Some(command) = self.pending_command.take() {
                self.run_console_command(&command);
                // The panels were drawn before it ran.
                ctx.request_repaint();
            }
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
        if let Some(pointer) = output.terrain {
            self.terrain_pointer(pointer);
            ctx.request_repaint();
        }
        if output.activated.is_some() {
            self.pending_ui_activation = output.activated;
        }
        if let Some(play) = self.play.as_mut() {
            for (id, at) in output.scrolled {
                let _ = play.world_mut().set_property(
                    id,
                    "CanvasPosition",
                    axcenity_core::Value::Vector2(at),
                );
            }
            // Setting a widget's Value fires its ValueChanged in the game.
            for (id, value) in output.widget_values {
                let _ = play.world_mut().set_property(id, "Value", value);
            }
        }
        if self.playing() {
            self.pending_ui_events.extend(output.ui_events);
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
        self.import_dropped(ctx);
        self.import_prompt(ctx, render_state);
        self.ribbon_tabs.editor_window(ctx);
        self.start_thumbnail(ctx);
        if self.learn_open && self.project.is_some() {
            let folder = self.project.as_ref().map(|project| project.folder.clone());
            let playing = self.playing();
            if let Some(sample) =
                self.start
                    .learn_window(ctx, &mut self.learn_open, folder.as_deref(), playing)
            {
                self.request_and_go_ahead(ctx, Discard::OpenSample(sample));
            }
        }
        if self.about_open
            && let Some(brand) = &self.brand
        {
            self.about_open = start::about(ctx, brand);
        }
        self.follow_sound_preview();
        self.settings_window(ctx);
        let textures = self.assets.texture_progress().map(|progress| {
            let file = progress.current.rsplit('/').next().unwrap_or_default();
            toast::Busy {
                text: format!(
                    "Loading textures {} of {}: {file}",
                    progress.done + 1,
                    progress.total
                ),
                fraction: progress.done as f32 / progress.total.max(1) as f32,
            }
        });
        self.toasts.busy = self.engine_busy().or(textures);
        let offer = self.toasts.show(ctx);
        self.follow_offer(offer);
        self.cover_while_finishing(ctx);
        theme::paint_crossfade(ctx);
    }
}

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod asset_fix_tests;
#[cfg(test)]
mod attribute_tests;
#[cfg(test)]
mod behavior_tree_tests;
#[cfg(test)]
mod hot_patch_tests;
#[cfg(test)]
mod ik_tests;
#[cfg(test)]
mod imported_clip_tests;
#[cfg(test)]
mod live_script_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod play_tests;
#[cfg(test)]
mod posing_tests;
#[cfg(test)]
mod relaunch_tests;
#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod strings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_designer_tests;
#[cfg(test)]
mod ui_graph_tests;
#[cfg(test)]
mod ui_layering_tests;
#[cfg(test)]
mod ui_selection_tests;
#[cfg(test)]
mod ui_text_style_tests;
#[cfg(test)]
mod undo_tests;
