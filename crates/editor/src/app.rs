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
mod asset_refs;
mod assets;
mod autorecover;
mod behavior_trees;
mod block_undo;
mod branding;
mod chrome;
mod collab_trust;
mod collision_groups;
mod command_palette;
mod commands;
mod constraint_tool;
mod controls_ui;
mod custom_materials;
mod data_tables;
mod dev_console;
mod dialogues;
mod documents;
mod edits;
mod effect_tab;
mod engine;
mod files;
mod find_in_scripts;
mod floating;
mod game_tests;
mod graphics_benchmark;
pub(crate) mod import_presets;
mod input_settings;
mod layers;
mod layouts;
mod levels;
mod luau_checks;
mod material_files;
mod material_tab;
pub(crate) mod mcp;
mod mixer;
mod model_studio;
mod native_menu;
mod nav_overlay;
mod open;
mod people_history;
mod plugins;
mod prefabs;
mod probe_bake;
mod probe_fill;
mod probes;
mod project_upgrade;
mod save_template;
mod search;
mod sequencer;
mod session;
pub(crate) mod settings;

mod collab_conflicts;
mod collab_handoff;
mod collab_project;
mod collab_repo;
mod collab_repo_ui;
mod collab_setup;
mod inventory;
mod sky;
mod solid;
mod spline_content;
mod splines;
mod tags;
pub(crate) mod team;
mod team_audit;
mod team_board;
mod team_chat;
mod team_history;
mod team_panel;
mod terrain;
mod terrain_paths;
mod texture_set;
mod to_terrain;
mod toolbox;
mod tree_ops;
mod ui_graphs;
mod ui_links;
mod ui_selection;
mod update_install;
mod updates;
mod version_history;
mod welcome;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_blocks::{BlockScript, translate};
use axcenity_core::{ClassName, Color3, InstanceId, PartType, Value, Vector3, World};
use axcenity_project::ScriptFile;
use axcenity_scene::{TerrainCache, orientation_from_rotation};
use axcenity_script::CameraState;
use eframe::egui;

use crate::angular;
use crate::animation::{AnimationStudio, StudioAction};
use crate::asset_browser::{AssetBrowser, AssetPayload, BrowserAction, Picking};
use crate::asset_check::AssetCheck;
use crate::autocomplete;
use crate::behavior_tree::BehaviorTreeStudio;
use crate::block_editor;
use crate::build_tools::{self, BuildSettings, PanelScene, ToolAction};
use crate::color_palette;
use crate::console::Console;
use crate::data_table::DataTableStudio;
use crate::dialog;
use crate::dialogue_editor::DialogueStudio;
use crate::explorer::{self, Action, Explorer};
use crate::gizmo::{Modes, Pose, Snap, Tool};
use crate::history::{self, Field, History, Move, Place};
use crate::import_dialog::ImportDialog;
use crate::layout::{self, Layout, Pane};
use crate::loading::{self, Loading};
use crate::output::Output;
use crate::play::{self, Crash, GameKeys, Play, PlaySettings};
use crate::profiler::Profiler;
use crate::properties::{self, Edit};
use crate::rebuild::{EngineSource, Rebuilds};
use crate::relaunch::Handoff;
use crate::ribbon::{self, RibbonButton};
use crate::ribbon_tabs::{RibbonGroup, RibbonTabs};
use crate::scripts::{self, ScriptTabs};
use crate::shortcuts;
use crate::side_warnings::{self, WarningCache};
use crate::start::{self, Brand, ProjectLink, StartAction, StartScreen};
use crate::style_panel::{self, StyleEdit};
use crate::terrain_studio::TerrainStudio;
use crate::theme::{self, Appearance, filled_icons, icons};
use crate::thumbnail;
use crate::toast::{self, Toasts};
use crate::toolbox_panel::ToolboxPanel;
use crate::tree::{self, Insertable};
use crate::ui_designer::{DesignerAction, DesignerScene, GraphPane, UiDesigner};
use crate::ui_handles::{CssEdit, UiDragEdit, apply_css_edits};
use crate::viewport::{
    self, DragEdit, DragKind, OverlayToggles, ViewMode, Viewport, ViewportOutput, ViewportScene,
};
use crate::watcher::Watcher;
use crate::widgets;
use crate::window_frame;
use eframe::egui_wgpu::RenderState;
use egui_dock::DockState;

/// The hover text of Import…, in the File menu and the ribbon.
const IMPORT_HOVER: &str = "Import models (.gltf, .glb, .fbx, .obj, or USD, converted to .glb), images, sounds and prefabs. A \
    model opens the Import dialog and is inserted at the camera's focus point; images and \
    sounds are copied into the project's assets and reported in the Console. You can also \
    drop files onto the editor.";

/// During Play, after a long pause (such as dragging the window), don't jump
/// the game ahead by the whole pause; treat the frame as at most this long.
/// The same cap as the player's.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// How many frames the loading screen stays up after a project opens, at
/// the least: then it stays while the world's meshes and textures are
/// still on their way to the GPU, and until frames come quickly (see
/// `app/session.rs`).
const FINISHING_FRAMES: u64 = 2;
/// The longest the loading screen waits for the world's meshes and
/// textures and for frames to settle, in seconds: what's still on its way
/// after that arrives in the open editor (drawn white, or not yet drawn,
/// until it does).
const FINISHING_LIMIT: f32 = 60.0;
/// After the last input, frames keep coming at the display's rate for this
/// long (see [`EditorApp::keep_frames_steady`]).
const STEADY_FRAME_SECONDS: f64 = 1.0;
/// A frame this long after the last one, when none was asked for, means
/// something woke an idle editor (see [`EditorApp::keep_frames_steady`]).
const WAKE_GAP_SECONDS: f64 = 0.2;
pub struct EditorApp {
    /// The open project's Instances, or an empty world when none is open.
    world: World,
    project: Option<OpenProject>,
    /// The open project's models, loaded as MeshParts are drawn, and shared
    /// with Play's physics. Has none when no project is open.
    assets: Assets,
    /// What the edit world's SplineMeshes and SplineScatters made (Phase
    /// 74), made again as their splines change (`generate_frame`).
    generators: axcenity_procedural::Generators,
    /// The Instance shown in Properties and given handles. The last of [`selection`](Self::selection).
    selected: Option<InstanceId>,
    /// Every selected Instance, in the order they were added. The viewport and
    /// Explorer highlight all of them. Ctrl toggles; Shift extends.
    selection: Vec<InstanceId>,
    /// The LOD Inspector's preview: this MeshPart drawn at this level.
    lod_preview: Option<(InstanceId, usize)>,
    /// File > Version History (its timeline, checkpoints, restores and
    /// backups), and the same window after a restore.
    version_history: version_history::VersionHistory,
    /// File ▸ Input…: the project's input map.
    input_settings: input_settings::InputWindow,
    /// Model ▸ Build ▸ Collision Groups.
    collision_groups: collision_groups::CollisionGroupsWindow,
    /// Where the main window is, in physical pixels, for dropping a panel
    /// back onto it.
    main_window_pixels: Option<egui::Rect>,
    /// A panel to put back in the main window, from a tab's menu.
    pending_main_dock: Option<Pane>,
    /// The Scene's tab was closed while the dock was out being drawn; the
    /// welcome page takes its place once it's back (`Layout::close_scene`).
    pending_close_scene: bool,
    /// The editor's icon, for panel windows' title bars and taskbar buttons.
    window_icon: Option<std::sync::Arc<egui::IconData>>,
    /// The New Custom Material window's form, while it's open.
    custom_material: Option<custom_materials::CustomMaterialForm>,
    /// Universal search's window (Ctrl+K, and the command palette and
    /// Quick Open, Phase 70), its index of providers and rows
    /// (`app/search.rs`), whether it was open last frame, Find in Scripts,
    /// and an asset's references.
    palette: crate::command_palette::Palette,
    search: search::SearchIndex,
    palette_was_open: bool,
    find: crate::find_in_scripts::FindWindow,
    references: Option<asset_refs::ReferencesWindow>,
    /// Every change made to `world` since it was opened, for undo and redo.
    /// It also knows whether the world is as saved.
    history: History,
    /// The Collaborate session this editor is in, if any (`app/team.rs`).
    team: Option<team::Team>,
    /// A session to start once the project opens (`--team-host`,
    /// `--team-join`).
    team_start: Option<team::TeamStart>,
    /// Settings > This project > Branding's previews.
    branding_previews: branding::Previews,
    /// The Collaborate panel's own state (`app/team_panel.rs`).
    team_panel: team_panel::TeamPanel,
    /// The open project's Git remote: automatic checkpoints and uploads
    /// (`app/collab_repo.rs`).
    repo: collab_repo::ProjectRepo,
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
    /// An older project waiting for "Upgrade this project?".
    upgrade_prompt: Option<project_upgrade::UpgradePrompt>,
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
    /// When the last input or wake-up came (egui's clock), for steady frame pacing.
    last_active_at: Option<f64>,
    /// When the last frame was drawn, and whether it asked for another.
    frame_pace: Option<(f64, bool)>,
    /// When the next steady frame is due to be presented (see [`EditorApp::pace_frame`]).
    next_present: Option<Instant>,
    /// The parts the viewport drag under way has moved, for Join Surfaces.
    dragged_parts: Vec<InstanceId>,
    /// The game, while Play is on. `world` stays as it was before Play.
    play: Option<Play>,
    /// Play's clients and emulated network (Phase 64 and 67), kept until
    /// the editor closes.
    play_settings: PlaySettings,
    /// The debugger's watch expressions (roadmap G6).
    watches: crate::watches::Watches,
    /// A button clicked in the viewport, delivered on the next Play frame.
    pending_ui_activation: Option<InstanceId>,
    /// `MouseEnter`, `MouseLeave` and buttons' sounds from the viewport,
    /// for the next Play frame.
    pending_ui_events: Vec<axcenity_ui::UiEvent>,
    /// Key presses for the game UI's focus, taken out of egui's input
    /// during Play (`take_nav_keys`) and used before the next game frame.
    nav_keys: Vec<axcenity_ui::NavKey>,
    /// The game's input from the viewport (the mouse) and from keys the
    /// game UI took, for the next game frame.
    pending_game_input: Vec<axcenity_core::input::InputEvent>,
    /// Turns the controllers' input into the game UI's focus moves.
    pad_nav: axcenity_ui::PadNavigation,
    /// Game controllers, opened the first time a game frame runs.
    gamepads: Option<axcenity_ui::Gamepads>,
    /// A TextBox the player is typing in during Play.
    focused_text: Option<InstanceId>,
    /// The line being typed in the game's chat during Play (`/`, Phase 67).
    chat_typing: Option<String>,
    /// The F9 Developer Console during Play (Phase 71).
    dev: dev_console::DevPlay,
    pending_focus_lost: Option<InstanceId>,
    /// UI layout warnings already shown, so a PageLayout problem is not
    /// repeated every frame.
    ui_warnings: Vec<String>,
    /// When the last Play frame ran, to measure how much game time passed.
    last_frame: Option<Instant>,
    /// Pause was pressed and Play hasn't paused since, so the pause it
    /// makes says so in Output.
    pause_pressed: bool,
    /// Turns keyboard events into key presses for LocalScripts during Play.
    game_keys: GameKeys,
    /// What Play's frames cost, for the Profiler panel.
    profiler: Profiler,
    /// The viewport's last frame, edit or Play, for the AI's
    /// `render_stats`.
    last_drawn: Option<mcp::LastDrawn>,
    /// The Mixer panel's meters, mute and solo.
    mixer: crate::mixer::Mixer,
    /// The Tests panel's tree and the run going now (Phase 53).
    game_tests: crate::game_tests::GameTests,
    /// The editor's Lua plugins, their dock tabs and saved settings (Phase 54).
    plugins: crate::plugins::Plugins,
    /// Whether opening a project starts cooking its textures (roadmap L2).
    /// Only the real editor does; tests open projects without.
    cook_textures: bool,
    /// The open project's texture cooking, while it runs.
    cooker: Option<crate::cooking::Cooker>,
    /// The last asset check, for the Asset Check panel.
    asset_check: AssetCheck,
    /// The Tags panel's rename box.
    tags: crate::tags::TagsPanel,
    /// The Layers panel (R16.2), this person's view of the data layers
    /// (`.axcenity/layers.json`), and what that hides and locks.
    layers: crate::layers::LayersPanel,
    layer_view: crate::layers::LayerView,
    layer_sets: crate::layers::LayerSets,
    /// The Team Board: the project's tasks (`app/team_board.rs`).
    team_board: team_board::TeamBoard,
    /// Each script's uses of APIs from the other side, for the warning badges.
    warnings: WarningCache,
    /// The open script tabs, and whether the viewport or a script is shown.
    tabs: ScriptTabs,
    /// luau-lsp's checks of the open code tabs (G4 stage 2).
    luau: luau_checks::LuauChecks,
    /// A Script waiting for an answer to "Delete script?".
    confirm_delete: Option<InstanceId>,
    /// A script that is a file in its service, dragged onto an object,
    /// while "Attach it?" is asked: the script and the object.
    confirm_attach: Option<(InstanceId, InstanceId)>,
    /// A sky image waiting for "Use this sky?" to be answered (`app/sky.rs`).
    confirm_sky: Option<String>,
    /// The Levels panel's typing and the level it acts on (`app/levels.rs`).
    levels_panel: crate::levels::LevelsPanel,
    /// Play starts in the start level, whichever level is open (the Levels
    /// panel's Play from Start Level).
    play_from_start: bool,
    /// Bake Probes was clicked: the bake starts on the next frame.
    bake_probes_asked: bool,
    /// The probe bake in progress, a little each frame
    /// (`app/probe_bake.rs`).
    probe_bake: Option<Box<probe_bake::ProbeBake>>,
    /// The baked LightProbeVolumes out of date, as of a history revision
    /// (`app/probes.rs`).
    stale_light_probes: Option<((u64, bool), Vec<InstanceId>)>,
    /// The Constraint tool, while it's on (`app/constraint_tool.rs`).
    constraint_tool: Option<constraint_tool::ConstraintTool>,
    /// A model file waiting for its objects to be chosen.
    importing: Option<ImportDialog>,
    /// Model files imported together, waiting for the dialog to be free.
    import_queue: VecDeque<PathBuf>,
    /// Converted `.glb` copies waiting for (or in) the Import dialog, and
    /// the file each was converted from (`app/files/convert.rs`).
    converted: HashMap<PathBuf, PathBuf>,
    /// The theme, accent, density and motion, picked in Settings. Saved between runs.
    appearance: Appearance,
    /// The Settings window.
    settings: settings::SettingsWindow,
    /// Confirmations such as "Saved", in the bottom-right corner.
    toasts: Toasts,
    /// The logo images, loaded on the first frame (they need the egui context).
    brand: Option<Brand>,
    /// The project being read in the background, if one is (see `loading.rs`).
    loading: Option<Loading>,
    /// The frame (egui's count) the loading screen goes at, after a
    /// project opened. Frames, not calls: egui may draw a frame twice.
    cover_until: Option<u64>,
    /// Whether that load took long enough to show its screen; if not, the
    /// cover is only the background (until it's been up for
    /// [`loading::GRACE`] itself).
    cover_shown: bool,
    /// When the cover went up, and the most meshes and textures it has
    /// seen on their way to the GPU at once (its count's total).
    cover_started: Option<Instant>,
    cover_uploads: usize,
    /// Whether frames have come quickly since: the cover stays until they
    /// do, so the editor answers as soon as it's gone.
    cover_settling: loading::Settling,
    /// Projects opened lately, newest first, for the start screen. Saved between runs.
    recent: Vec<ProjectLink>,
    /// The script tabs closed last in each project, for the welcome page.
    recent_tabs: welcome::RecentTabs,
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
    /// The system menu bar on macOS (`native_menu.rs`).
    native_menu: native_menu::NativeMenu,
    /// True while Help > Learn is open over the project.
    learn_open: bool,
    /// A Sound being listened to from the Properties panel, outside Play.
    sound_preview: Option<axcenity_audio::AudioEngine>,
    /// The Assets panel, which shares the bottom of the window with Output.
    asset_browser: AssetBrowser,
    /// The Toolbox tab (`toolbox_panel.rs`, its actions `app/toolbox.rs`).
    toolbox: ToolboxPanel,
    /// The Downloaded / Imported Assets tab.
    inventory: crate::asset_inventory::AssetInventory,
    /// Update checks and their settings (`app/updates.rs`).
    updates: updates::Updates,
    /// The Version Explorer tab (`version_explorer.rs`).
    version_explorer: crate::version_explorer::VersionExplorer,
    /// Models the Toolbox got, waiting for the Import dialog: each file in
    /// the asset directory and the asset key and package it came from.
    toolbox_imports: HashMap<PathBuf, (String, String)>,
    /// The Animation panel's clips, open clip and preview.
    animation: AnimationStudio,
    /// The Behavior Tree tab and the trees open in it.
    behavior: BehaviorTreeStudio,
    /// The Dialogue tab and the dialogues open in it (Phase 77).
    dialogues: DialogueStudio,
    /// The Data Table tab.
    data_tables: DataTableStudio,
    /// The Material tab.
    materials: crate::material_editor::MaterialStudio,
    /// The Sequencer tab and its preview (Phase 51).
    sequencer: crate::sequencer::Sequencer,
    /// The Model Studio tab (Phase 55).
    model_studio: crate::model_studio::ModelStudio,
    /// The Object Viewer tab's object (cohesion pass CW5).
    /// Boxed: two worlds of its own would make every `EditorApp` on the
    /// stack much bigger.
    object_viewer: Box<axcenity_editor_tools::object_viewer::ObjectViewer>,
    /// The Effect tab (roadmap R17.3).
    effect_editor: Box<crate::effect_editor::EffectEditor>,
    /// The block script whose tab is under the mouse, set as it's drawn:
    /// Ctrl+Z and Ctrl+Y are its then, and the world's keys aren't taken.
    block_tab_pointer: Option<String>,
    /// The frame (`theme::frame_number`) a Scene View was last drawn in:
    /// the ribbon's scene tools and their keys work only while one shows.
    scene_drawn_pass: Option<u64>,
    /// The UI Designer tab's view of a ScreenGui.
    ui_designer: UiDesigner,
    /// The Terrain panel's tools, settings, previews and work under way.
    terrain: TerrainStudio,
    /// Where the dockable panels are. Saved between runs.
    layout: Layout,
    /// Layouts saved by name (View › Layout).
    saved_layouts: crate::layout::SavedLayouts,
    /// Save Current Layout…, while it's open.
    layout_name: Option<layouts::LayoutName>,
    /// A saved layout picked in the menu, switched to after the menu is drawn.
    pending_layout: Option<String>,
    /// The ribbon's tabs, including the ones people made. Saved between runs.
    ribbon_tabs: RibbonTabs,
    /// View > Light Cones: show every light's reach in the viewport, not
    /// just the selected one's. Saved between runs.
    /// AutoRecover's setting and snapshots (`app/autorecover.rs`).
    autorecover: autorecover::AutoRecover,
    /// Settings > External tools > Blender: the Blender program Edit in
    /// Blender starts, or empty to look for it (`blender.rs`, R15.4).
    blender_path: String,
    /// Where Save as Template… writes and New Project lists the user's
    /// templates (`app/save_template.rs`).
    templates_folder: Option<PathBuf>,
    /// Save as Template…, while it's open.
    template_draft: Option<save_template::TemplateDraft>,
    /// The world presets the Terrain Studio offers (procedural worlds PW3):
    /// built-in, then the project's, read for the project folder named.
    world_presets: terrain::WorldPresets,
    light_cones: bool,
    /// Whether the viewport shows the view cube (Test > View, View > View
    /// Cube). On unless turned off; saved between runs.
    view_cube: bool,
    /// Whether the viewport shows the ray tracing badge (Viewport > Ray
    /// Tracing Badge). On unless turned off; saved between runs.
    ray_badge: bool,
    /// Settings > AI connections: MCP clients working in this window
    /// (`mcp/live.rs`).
    ai: mcp::AiConnections,
    /// Whether the viewport shows the measuring grid (saved between runs).
    grid: bool,
    /// Viewport > Movement Guide: the line, snap points and distance drawn
    /// while parts are moved, resized or turned. Only what's drawn; moving
    /// works the same without it. On unless turned off; saved between runs.
    move_guide: bool,
    /// How solid the grid's lines are, as a multiple of the usual soft look.
    grid_strength: f32,
    /// The user's saved color palettes, shared by every color control.
    colors: color_palette::Handle,
    /// A tab to move into another window, from its right-click menu.
    pending_window_move: Option<(Pane, u64)>,
    /// View overlay toggles (colliders, raycasts, bounds, emitters, the
    /// navigation mesh). Saved between runs. The player never draws them.
    overlays: OverlayToggles,
    /// View > View Mode: the lit scene or a debug view. Not saved between runs.
    view_mode: ViewMode,
    /// The navigation mesh the overlay draws, while it's on.
    nav_mesh: Option<nav_overlay::NavCache>,
    /// This computer's graphics settings (Settings > Graphics), for the
    /// viewport and Play. Saved between runs, never in a project.
    graphics: axcenity_render::GraphicsSettings,
    /// What the GPU can do, read on the first frame that has one.
    capabilities: Option<axcenity_render::Capabilities>,
    /// The graphics adapter, for the About box's diagnostics.
    adapter: Option<String>,
    /// Debug › Graphics Benchmark: its window, a run, and the last report
    /// (saved between runs).
    benchmark: crate::graphics_benchmark::GraphicsBenchmark,
    /// Settings > Engine: rebuilding the editor when engine code changes
    /// (a debug editor run from the source tree only; see `engine.rs`).
    rebuilds: Rebuilds,
    /// A handoff written for the rebuilt editor, or for this one again
    /// (Restart now), which `update` starts.
    launching: Option<(PathBuf, engine::LaunchInto)>,
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
    /// UI ▸ Controls: the ready-made controls menu.
    insert_controls: bool,
    import: bool,
    /// Model ▸ Constraints ▸ Weld.
    weld: bool,
    /// Model ▸ Solid Modeling: Union, Negate, Intersect or Separate.
    solid: Option<crate::app::solid::SolidAction>,
    /// Model ▸ Constraints ▸ Constraint: the joint picked, or `Some(None)`
    /// to stop the tool.
    constraint: Option<Option<ClassName>>,
    /// Bake Probes, beside Reflection Probe.
    bake_probes: bool,
    /// Fill with Probes, below Bake Probes (roadmap R18.4).
    fill_probes: bool,
    check_assets: bool,
    play: bool,
    /// Run without a player (roadmap G14).
    run: bool,
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
    /// The window these panels are in, unless it's the main one.
    floating: Option<u64>,
}

impl DockViewer<'_> {
    fn draw_pane(&mut self, ui: &mut egui::Ui, pane: Pane) {
        match pane {
            Pane::Scene => *self.viewport = self.app.scene_pane(ui, self.render_state),
            Pane::Welcome => self.app.welcome_pane(ui),
            Pane::Explorer => self.app.explorer_pane(ui),
            Pane::Properties => self.app.properties_pane(ui),
            Pane::Console => self.app.console_pane(ui),
            Pane::Assets => self.app.assets_pane(ui),
            Pane::Animation => self.app.animation_pane(ui, self.render_state),
            Pane::UiDesigner => self.app.ui_designer_pane(ui, self.render_state),
            Pane::Terrain => self.app.terrain_pane(ui),
            Pane::Profiler => {
                self.app.profiler.show(ui);
                if let Some((path, line)) = self.app.profiler.take_open() {
                    self.app.open_script(&path, Some(line));
                }
            }
            Pane::Mixer => self.app.mixer_pane(ui),
            Pane::AssetCheck => self.app.asset_check_pane(ui),
            Pane::LodInspector => self.app.lod_inspector_pane(ui),
            Pane::BehaviorTree => self.app.behavior_tree_pane(ui),
            Pane::Dialogue => self.app.dialogue_pane(ui),
            Pane::DataTable => self.app.data_table_pane(ui),
            Pane::Material => self.app.material_pane(ui, self.render_state),
            Pane::Tags => self.app.tags_pane(ui),
            Pane::Layers => self.app.layers_pane(ui),
            Pane::Levels => self.app.levels_pane(ui),
            Pane::Sequencer => self.app.sequencer_pane(ui),
            Pane::Tests => self.app.tests_pane(ui),
            Pane::Plugins => self.app.plugins_pane(ui),
            Pane::ModelStudio => self.app.model_studio_pane(ui, self.render_state),
            Pane::ObjectViewer => {
                self.app
                    .object_viewer
                    .ui(ui, self.render_state, &self.app.assets)
            }
            Pane::Effect => self.app.effect_pane(ui, self.render_state),
            Pane::Toolbox => self.app.toolbox_pane(ui),
            Pane::Inventory => self.app.inventory_pane(ui),
            Pane::VersionExplorer => self
                .app
                .version_explorer
                .show(ui, &axcenity_release::BuildInfo::current()),
            Pane::TeamCreate => self.app.team_create_pane(ui),
            Pane::TeamBoard => self.app.team_board_pane(ui),
            Pane::Script(id) => self.app.script_pane(ui, id),
            Pane::Plugin(id) => self.app.plugin_pane(ui, id, self.render_state),
        }
    }
}

/// Runs `add` in a `Ui` whose id depends only on `pane`, so scroll positions,
/// open sections and the like stay with a panel wherever it's docked.
fn in_panel_memory(ui: &mut egui::Ui, pane: Pane, add: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.available_rect_before_wrap();
    let mut body = egui::Ui::new(
        ui.ctx().clone(),
        egui::Id::new(("panel body", pane)),
        egui::UiBuilder::new()
            .layer_id(ui.layer_id())
            .max_rect(rect)
            .layout(*ui.layout()),
    );
    body.set_clip_rect(ui.clip_rect());
    body.set_style(ui.style().clone());
    add(&mut body);
    ui.advance_cursor_after_rect(rect);
}

/// The Console's tab title with how many errors came since it was last
/// shown, in the error colour.
fn console_title(pane: Pane, errors: usize, color: egui::Color32) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::new(theme::type_scale::BODY - 0.5, theme::medium_family());
    job.append(
        &theme::with_icon(pane.icon(), pane.label()),
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            // The tab's own colour, idle or active.
            color: egui::Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    job.append(
        &format!("{} {errors}", icons::WARNING_CIRCLE),
        6.0,
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.into()
}

impl egui_dock::TabViewer for DockViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, pane: &mut Pane) -> egui::WidgetText {
        match pane {
            Pane::Script(id) => self.app.tabs.title(*id, &self.visuals),
            // New errors while the Console isn't showing: counted on its tab.
            Pane::Console if self.app.output.unseen_errors(&self.ctx) > 0 => {
                let errors = self.app.output.unseen_errors(&self.ctx);
                console_title(*pane, errors, theme::palette(&self.ctx).error)
            }
            Pane::Plugin(id) => egui::RichText::new(theme::with_icon(
                icons::PUZZLE_PIECE,
                &self.app.plugin_pane_title(*id),
            ))
            .family(theme::medium_family())
            .size(theme::type_scale::BODY - 0.5)
            .into(),
            _ => pane.title(),
        }
    }

    fn id(&mut self, pane: &mut Pane) -> egui::Id {
        egui::Id::new(("dock pane", *pane))
    }

    fn ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane) {
        let pane = *pane;
        in_panel_memory(ui, pane, |ui| self.draw_pane(ui, pane));
    }

    /// Everything but the welcome page, which goes by itself once a
    /// document opens.
    fn is_closeable(&self, pane: &Pane) -> bool {
        *pane != Pane::Welcome
    }

    /// A tab's right-click menu: egui_dock's Float and Close, and a way home.
    fn context_menu(&mut self, ui: &mut egui::Ui, pane: &mut Pane, _path: egui_dock::NodePath) {
        if self.floating.is_some() && ui.button("Dock in Main Window").clicked() {
            self.app.pending_main_dock = Some(*pane);
            ui.close();
        }
        if !matches!(pane, Pane::Scene | Pane::Welcome) {
            for (window, name) in self.app.layout.other_windows(self.floating) {
                if ui.button(format!("Move to {name} Window")).clicked() {
                    self.app.pending_window_move = Some((*pane, window));
                    ui.close();
                }
            }
        }
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
        if let Pane::Plugin(id) = pane {
            self.app.close_plugin_pane(*id);
        }
        // The welcome page takes the Scene's place (when nothing else is
        // open), which needs the dock back first.
        if *pane == Pane::Scene {
            self.app.pending_close_scene = true;
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

    /// The Scene and the welcome page stay docked in the main window.
    fn allowed_in_windows(&self, pane: &mut Pane) -> bool {
        !matches!(pane, Pane::Scene | Pane::Welcome)
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
    /// Its own undo and redo, for Ctrl+Z over its tab.
    undo: block_undo::BlockUndo,
}

/// The keys the editor's settings are saved under (see [`EditorApp::load_settings`]).
/// Only read, from saves made before Settings had more than the theme.
const THEME_KEY: &str = "theme";
const APPEARANCE_KEY: &str = "appearance";
const RECENT_KEY: &str = "recent projects";
const BLENDER_KEY: &str = "blender path";
const LAYOUT_KEY: &str = "panel layout";
const RIBBON_TABS_KEY: &str = "ribbon tabs";
const LIGHT_CONES_KEY: &str = "light cones";
const VIEW_CUBE_KEY: &str = "view cube";
const RAY_BADGE_KEY: &str = "ray tracing badge";
const AI_CONNECTIONS_KEY: &str = "ai connections";
const GRID_KEY: &str = "grid";
const MOVE_GUIDE_KEY: &str = "movement guide";
/// What the Movement Guide toggle does, for its menu item and Settings row.
/// What Viewport > Ray Tracing Badge and its setting say on hover.
const RAY_BADGE_HINT: &str = "The badge in the viewport's corner that says which ray traced \
    effects are drawing and how many items rays see. Off hides it, even when a restart is due.";
const MOVE_GUIDE_HINT: &str = "While a part is moved, resized or turned: the line along the \
    change, the snap points near it and how far it has gone. Off, moving, snapping and the \
    handles work the same; only the guide isn't drawn.";
const GRID_STRENGTH_KEY: &str = "grid strength";
const COLOR_PALETTES_KEY: &str = "color palettes";
const DEFAULT_GRID_STRENGTH: f32 = 1.0;
pub(crate) const MIN_GRID_STRENGTH: f32 = 0.25;
pub(crate) const MAX_GRID_STRENGTH: f32 = 2.5;
const OVERLAY_COLLIDERS_KEY: &str = "overlay colliders";
const OVERLAY_RAYCASTS_KEY: &str = "overlay raycasts";
const OVERLAY_BOUNDS_KEY: &str = "overlay bounds";
const OVERLAY_EMITTERS_KEY: &str = "overlay emitters";
const OVERLAY_NAV_MESH_KEY: &str = "overlay nav mesh";
const OVERLAY_AGENT_PATHS_KEY: &str = "overlay agent paths";
const OVERLAY_CONSTRAINTS_KEY: &str = "overlay constraints";
const OVERLAY_CONTACTS_KEY: &str = "overlay contacts";
const OVERLAY_BODIES_KEY: &str = "overlay bodies";
const OVERLAY_STREAMING_KEY: &str = "overlay streaming";
const STREAM_EDITING_KEY: &str = "stream while editing";
const OVERLAY_LIGHT_PROBES_KEY: &str = "overlay light probes";
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
    /// Opening a project folder in place: a tutorial's sample from Help >
    /// Learn, or a recent project from the welcome page.
    OpenSample(PathBuf),
    /// Closing the tab of the script at this path.
    CloseTab(String),
    /// Going back to the start screen.
    CloseProject,
    /// File > New Project…: back to the start screen, asking for the new one.
    NewProject,
    /// Opening a level from the Levels panel (`None`: the start level).
    OpenLevel(Option<String>),
    /// Closing for the updater to install an update or roll back
    /// (`app/update_install.rs`).
    Update(axcenity_updater::apply::Action),
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
    /// The start level's name (the manifest's `start_level`), in `world.json`.
    start_level: String,
    /// The level being edited, from `levels/`; `None` for the start level
    /// (Phase 48). Saving writes its Workspace there.
    level: Option<String>,
    show_splash: bool,
    /// Whether built games have the F9 Developer Console (File > Build
    /// Game, Phase 71).
    developer_console: bool,
    /// Whether built games start with the ray traced look on GPUs that can
    /// ray trace (File > Build Game, runtime lighting parity RL2).
    ray_traced_by_default: bool,
    /// How built games' loading screen goes (File > Build Game).
    loading_screen: axcenity_project::LoadingScreen,
    /// The game's icon and banner (Settings > This project > Branding).
    branding: axcenity_project::branding::Branding,
    /// The actions and their bindings (`input.json`, the Input window).
    input_map: axcenity_core::input::InputMap,
    folder: PathBuf,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    /// Each stylesheet's text as last read or written, by path, to tell the
    /// editor's own saves from changes made elsewhere.
    stylesheet_texts: HashMap<String, String>,
    /// Its server scripts, sorted by path, with each file's text as last
    /// read or written. Play reads their files again, for the newest code.
    scripts: Vec<ScriptFile>,
    block_scripts: Vec<BlockDocument>,
    /// The code files of scripts inside objects (G2; a script's
    /// `SourceFile`), by path, with each file's text as last read or
    /// written. Copies share a file, so these have no Instance of their
    /// own: the world says which scripts use each.
    object_files: BTreeMap<String, String>,
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
            generators: Default::default(),
            selected: None,
            selection: Vec::new(),
            lod_preview: None,
            version_history: Default::default(),
            input_settings: Default::default(),
            collision_groups: Default::default(),
            main_window_pixels: None,
            pending_main_dock: None,
            pending_close_scene: false,
            window_icon: None,
            custom_material: None,
            palette: Default::default(),
            search: search::SearchIndex::new(),
            palette_was_open: false,
            find: Default::default(),
            references: None,
            history: History::new(),
            team: None,
            team_start: None,
            branding_previews: Default::default(),
            team_panel: Default::default(),
            repo: Default::default(),
            tool: Tool::default(),
            snap: Snap::default(),
            modes: Modes::default(),
            build: BuildSettings::default(),
            pending_emits: Vec::new(),
            asking: None,
            upgrade_prompt: None,
            closing: false,
            output: Output::default(),
            console: Console::default(),
            pending_command: None,
            viewport: Viewport::new(),
            ui_drag_baselines: HashMap::new(),
            shown_title: String::new(),
            last_active_at: None,
            frame_pace: None,
            next_present: None,
            dragged_parts: Vec::new(),
            play: None,
            play_settings: PlaySettings::default(),
            watches: crate::watches::Watches::default(),
            pending_ui_activation: None,
            pending_ui_events: Vec::new(),
            nav_keys: Vec::new(),
            pending_game_input: Vec::new(),
            pad_nav: axcenity_ui::PadNavigation::default(),
            gamepads: None,
            focused_text: None,
            chat_typing: None,
            dev: dev_console::DevPlay::default(),
            pending_focus_lost: None,
            ui_warnings: Vec::new(),
            last_frame: None,
            pause_pressed: false,
            profiler: Profiler::default(),
            last_drawn: None,
            mixer: crate::mixer::Mixer::default(),
            game_tests: crate::game_tests::GameTests::default(),
            plugins: crate::plugins::Plugins::default(),
            cook_textures: false,
            cooker: None,
            asset_check: AssetCheck::default(),
            tags: crate::tags::TagsPanel::default(),
            layers: Default::default(),
            layer_view: Default::default(),
            layer_sets: Default::default(),
            team_board: Default::default(),
            tabs: ScriptTabs::default(),
            luau: Default::default(),
            confirm_delete: None,
            confirm_attach: None,
            confirm_sky: None,
            levels_panel: Default::default(),
            play_from_start: false,
            stale_light_probes: None,
            bake_probes_asked: false,
            probe_bake: None,
            constraint_tool: None,
            importing: None,
            import_queue: VecDeque::new(),
            converted: HashMap::new(),
            game_keys: GameKeys::default(),
            warnings: WarningCache::default(),
            appearance: Appearance::default(),
            settings: settings::SettingsWindow::default(),
            toasts: Toasts::default(),
            brand: None,
            loading: None,
            cover_until: None,
            cover_shown: false,
            cover_started: None,
            cover_uploads: 0,
            cover_settling: loading::Settling::default(),
            recent: Vec::new(),
            recent_tabs: welcome::RecentTabs::default(),
            start: StartScreen::default(),
            thumbnail: None,
            thumbnail_wanted: None,
            viewport_rect: None,
            about_open: false,
            native_menu: Default::default(),
            learn_open: false,
            sound_preview: None,
            asset_browser: AssetBrowser::default(),
            toolbox: ToolboxPanel::default(),
            inventory: Default::default(),
            version_explorer: Default::default(),
            updates: Default::default(),
            toolbox_imports: HashMap::new(),
            animation: AnimationStudio::default(),
            behavior: BehaviorTreeStudio::default(),
            dialogues: DialogueStudio::default(),
            data_tables: DataTableStudio::default(),
            materials: Default::default(),
            sequencer: Default::default(),
            model_studio: Default::default(),
            object_viewer: Default::default(),
            effect_editor: Default::default(),
            block_tab_pointer: None,
            scene_drawn_pass: None,
            ui_designer: UiDesigner::default(),
            terrain: TerrainStudio::default(),
            layout: Layout::default(),
            saved_layouts: Default::default(),
            layout_name: None,
            pending_layout: None,
            ribbon_tabs: RibbonTabs::default(),
            autorecover: Default::default(),
            blender_path: String::new(),
            templates_folder: None,
            template_draft: None,
            world_presets: Default::default(),
            light_cones: false,
            view_cube: true,
            ray_badge: true,
            ai: Default::default(),
            move_guide: true,
            grid: true,
            grid_strength: DEFAULT_GRID_STRENGTH,
            colors: Default::default(),
            pending_window_move: None,
            overlays: OverlayToggles::default(),
            view_mode: ViewMode::Lit,
            nav_mesh: None,
            graphics: axcenity_render::GraphicsSettings::default(),
            capabilities: None,
            adapter: None,
            benchmark: Default::default(),
            rebuilds: Rebuilds {
                source: EngineSource::for_rebuilds(),
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
        // Tests never look at, or tidy, the real app data.
        if !cfg!(test) {
            self.update_results_on_start();
        }
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
            self.saved_layouts = layouts::load_saved_layouts(storage);
            if let Some(text) = storage.get_string(RIBBON_TABS_KEY) {
                self.ribbon_tabs = RibbonTabs::from_text(&text);
            }
            if let Some(text) = storage.get_string(start::StartNav::KEY) {
                self.start.set_nav(start::StartNav::from_text(&text));
            }
            self.light_cones = storage.get_string(LIGHT_CONES_KEY).as_deref() == Some("on");
            if let Some(text) = storage.get_string(autorecover::AUTORECOVER_KEY) {
                self.autorecover.read(&text);
            }
            if let Some(text) = storage.get_string(BLENDER_KEY) {
                self.blender_path = text;
            }
            self.view_cube = storage.get_string(VIEW_CUBE_KEY).as_deref() != Some("off");
            self.ray_badge = storage.get_string(RAY_BADGE_KEY).as_deref() != Some("off");
            self.ai.allowed = storage.get_string(AI_CONNECTIONS_KEY).as_deref() == Some("on");
            if let Some(text) = storage.get_string(team_panel::TEAM_CREATE_KEY) {
                self.team_panel.read_text(&text);
            }
            self.grid = storage.get_string(GRID_KEY).as_deref() != Some("off");
            self.move_guide = storage.get_string(MOVE_GUIDE_KEY).as_deref() != Some("off");
            self.grid_strength = storage
                .get_string(GRID_STRENGTH_KEY)
                .and_then(|text| text.parse::<f32>().ok())
                .map_or(DEFAULT_GRID_STRENGTH, |strength| {
                    strength.clamp(MIN_GRID_STRENGTH, MAX_GRID_STRENGTH)
                });
            if let Some(text) = storage.get_string(COLOR_PALETTES_KEY) {
                self.colors = color_palette::Handle::new(color_palette::Library::from_text(&text));
            }
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
            self.overlays.constraints =
                storage.get_string(OVERLAY_CONSTRAINTS_KEY).as_deref() == Some("on");
            self.overlays.contacts =
                storage.get_string(OVERLAY_CONTACTS_KEY).as_deref() == Some("on");
            self.overlays.bodies = storage.get_string(OVERLAY_BODIES_KEY).as_deref() == Some("on");
            self.overlays.streaming =
                storage.get_string(OVERLAY_STREAMING_KEY).as_deref() == Some("on");
            self.overlays.stream_editing =
                storage.get_string(STREAM_EDITING_KEY).as_deref() == Some("on");
            self.overlays.light_probes =
                storage.get_string(OVERLAY_LIGHT_PROBES_KEY).as_deref() == Some("on");
            if let Some(text) = storage.get_string(welcome::RecentTabs::KEY) {
                self.recent_tabs = welcome::RecentTabs::from_text(&text);
            }
            if let Some(text) = storage.get_string(CONSOLE_HISTORY_KEY) {
                self.console.load_history(&text);
            }
            if let Some(text) = storage.get_string(command_palette::RECENT_COMMANDS_KEY) {
                self.palette.load_recent(&text);
            }
            if let Some(text) = storage.get_string(updates::UPDATES_KEY) {
                self.updates.prefs = updates::Updates::load(&text);
            }
            if let Some(settings) = storage
                .get_string(GRAPHICS_KEY)
                .and_then(|text| serde_json::from_str(&text).ok())
            {
                self.graphics = settings;
            }
            self.benchmark.report = storage
                .get_string(crate::graphics_benchmark::REPORT_KEY)
                .and_then(|text| crate::graphics_benchmark::GraphicsBenchmark::load(&text));
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
fn shown<'a>(world: &'a World, play: &'a Option<Play>) -> &'a World {
    play.as_ref().map_or(world, |play| play.world())
}

impl eframe::App for EditorApp {
    /// Takes the keys that move the game UI's focus during Play before
    /// egui sees them, so Tab and the arrows don't move egui's own focus.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // First, so keys chosen from the macOS menu bar reach Play's too.
        self.take_native_menu_input(raw_input);
        self.take_nav_keys(ctx, raw_input);
    }

    /// Called by eframe to draw each frame (only when something changed,
    /// such as mouse movement, unless a frame is requested). `ui` covers
    /// the whole window.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.run_native_menu_clicks(&ctx);
        self.frame(ui, frame.wgpu_render_state());
        // After the frame, so the menu bar shows what it changed.
        self.sync_native_menu(&ctx);
        self.pace_frame();
        // Restart now, on the viewport's ray tracing badge.
        if self.viewport.take_restart_request() {
            self.restart_editor();
        }
        // Relaunching into a rebuilt editor, or restarting, saves the
        // settings first.
        self.launch(&ctx, frame);
        // Tests call `frame` without a window, so the window's resize edges
        // live here.
        window_frame::edges(&ctx);
    }

    /// Saves the look, the layout and the recent projects. eframe calls
    /// this now and then, and when the window closes.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // History lines still folding go to the project's files.
        self.flush_history();
        storage.set_string(APPEARANCE_KEY, self.appearance.to_text());
        storage.set_string(updates::UPDATES_KEY, self.updates.to_text());
        let recent: Vec<String> = self
            .recent
            .iter()
            .map(|link| link.folder.display().to_string())
            .collect();
        storage.set_string(LAYOUT_KEY, self.layout.to_text());
        storage.set_string(layouts::SAVED_LAYOUTS_KEY, self.saved_layouts.to_text());
        storage.set_string(RIBBON_TABS_KEY, self.ribbon_tabs.to_text());
        storage.set_string(start::StartNav::KEY, self.start.nav().to_text());
        let cones = if self.light_cones { "on" } else { "off" };
        storage.set_string(LIGHT_CONES_KEY, cones.to_owned());
        storage.set_string(autorecover::AUTORECOVER_KEY, self.autorecover.to_text());
        storage.set_string(BLENDER_KEY, self.blender_path.clone());
        let cube = if self.view_cube { "on" } else { "off" };
        storage.set_string(VIEW_CUBE_KEY, cube.to_owned());
        let badge = if self.ray_badge { "on" } else { "off" };
        storage.set_string(RAY_BADGE_KEY, badge.to_owned());
        let ai = if self.ai.allowed { "on" } else { "off" };
        storage.set_string(AI_CONNECTIONS_KEY, ai.to_owned());
        storage.set_string(team_panel::TEAM_CREATE_KEY, self.team_panel.to_text());
        let grid = if self.grid { "on" } else { "off" };
        storage.set_string(GRID_KEY, grid.to_owned());
        let guide = if self.move_guide { "on" } else { "off" };
        storage.set_string(MOVE_GUIDE_KEY, guide.to_owned());
        storage.set_string(GRID_STRENGTH_KEY, self.grid_strength.to_string());
        storage.set_string(COLOR_PALETTES_KEY, self.colors.get().to_text());
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
        storage.set_string(
            OVERLAY_CONSTRAINTS_KEY,
            flag(self.overlays.constraints).to_owned(),
        );
        storage.set_string(
            OVERLAY_CONTACTS_KEY,
            flag(self.overlays.contacts).to_owned(),
        );
        storage.set_string(OVERLAY_BODIES_KEY, flag(self.overlays.bodies).to_owned());
        storage.set_string(
            OVERLAY_STREAMING_KEY,
            flag(self.overlays.streaming).to_owned(),
        );
        storage.set_string(
            STREAM_EDITING_KEY,
            flag(self.overlays.stream_editing).to_owned(),
        );
        storage.set_string(
            OVERLAY_LIGHT_PROBES_KEY,
            flag(self.overlays.light_probes).to_owned(),
        );
        storage.set_string(CONSOLE_HISTORY_KEY, self.console.history_text());
        storage.set_string(welcome::RecentTabs::KEY, self.recent_tabs.to_text());
        storage.set_string(
            command_palette::RECENT_COMMANDS_KEY,
            self.palette.recent_text(),
        );
        if let Ok(text) = serde_json::to_string(&self.graphics) {
            storage.set_string(GRAPHICS_KEY, text);
        }
        settings::remember_ray_tracing(self.graphics.ray_tracing());
        if let Some(text) = self.benchmark.saved() {
            storage.set_string(crate::graphics_benchmark::REPORT_KEY, text);
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

/// When to present the frame built at `now`, and when the one after it is
/// due, on a grid `interval` apart that `scheduled` is the next point of.
/// A frame that's late by less than an interval goes out at once and the
/// grid holds; later than that, the grid restarts from now.
fn next_present(
    now: Instant,
    scheduled: Option<Instant>,
    interval: std::time::Duration,
) -> (Instant, Instant) {
    let due = match scheduled {
        Some(at) if at >= now => at,
        Some(at) if now.duration_since(at) < interval => at,
        _ => now,
    };
    (due, due + interval)
}

/// Sleeps until `when`: the OS sleep for all but the last bit (it can
/// overshoot by a millisecond or two), then spins for the rest.
fn wait_until(when: Instant) {
    const SPIN: std::time::Duration = std::time::Duration::from_millis(2);
    let now = Instant::now();
    if when > now + SPIN {
        std::thread::sleep(when - now - SPIN);
    }
    while Instant::now() < when {
        std::hint::spin_loop();
    }
}

impl EditorApp {
    /// egui only draws when input comes, so small mouse movements give
    /// bursts of frames with gaps between them. On a variable refresh rate
    /// (FreeSync, G-Sync) display the refresh rate then swings with them,
    /// and many panels (OLED and VA especially) brighten and dim as it
    /// does: a flicker. Windows puts windowed apps on VRR too. So after any
    /// input, keep drawing for [`STEADY_FRAME_SECONDS`]: the rate holds
    /// steady while someone works, and an idle editor still draws nothing.
    ///
    /// A frame nobody here asked for after a gap (a file changed, a
    /// background job finished) is the same lone frame, so it counts as
    /// input. With Hold the refresh rate steady on, a focused window never
    /// goes idle at all, for panels that react to the rate dropping too.
    fn keep_frames_steady(&mut self, ctx: &egui::Context) {
        let (now, input, focused) = ctx.input(|input| {
            let active =
                !input.events.is_empty() || input.pointer.any_down() || !input.keys_down.is_empty();
            (input.time, active, input.viewport().focused == Some(true))
        });
        let woke = self
            .frame_pace
            .is_some_and(|(at, asked)| !asked && now - at > WAKE_GAP_SECONDS);
        if input || woke {
            self.last_active_at = Some(now);
        }
        // The start screen's backdrop moves all the time, so while it shows
        // (and the window has focus) frames keep coming on the steady grid
        // rather than on a timer of its own. Reduce motion stills it.
        let start_moving = self.project.is_none() && !self.appearance.reduce_motion;
        let steady = self
            .last_active_at
            .is_some_and(|at| now - at < STEADY_FRAME_SECONDS)
            || ((self.appearance.hold_refresh || start_moving) && focused);
        self.frame_pace = Some((now, steady));
        if steady {
            ctx.request_repaint();
        }
    }

    /// Steady frames come as fast as each is ready, so their lengths still
    /// vary with the work in them, and a variable refresh rate display
    /// follows every one. So while frames are steady, each is held until
    /// the next point on a fixed grid (`Appearance::steady_hz`) before it's
    /// presented, which the display sees as one rate. A frame that misses
    /// its point takes the next one, so the grid holds. Runs after the
    /// frame is built, just before eframe presents it.
    fn pace_frame(&mut self) {
        let steady = self.frame_pace.is_some_and(|(_, steady)| steady);
        if !steady || self.appearance.steady_hz == 0 {
            self.next_present = None;
            return;
        }
        let interval =
            std::time::Duration::from_secs_f64(1.0 / f64::from(self.appearance.steady_hz));
        let (due, next) = next_present(Instant::now(), self.next_present, interval);
        self.next_present = Some(next);
        wait_until(due);
    }

    /// One frame: game time during Play, then the menu bar, the ribbon and
    /// the dock (or the start screen), then windows and overlays. Separate
    /// from `update` so tests can draw it without a window.
    /// One frame of the editor, drawn into `ui`, which covers the window.
    pub(crate) fn frame(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>) {
        // `Context` is a cheap shared handle; this copy leaves `ui` free to
        // lay out the panels below.
        let ctx = &ui.ctx().clone();
        color_palette::install(ctx, &self.colors);
        // The window's close button: stop it and ask first if there are
        // unsaved changes. `closing` lets the close through once allowed.
        if ctx.input(|input| input.viewport().close_requested()) {
            if !self.closing && self.unsaved() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.asking = Some(Discard::Quit);
            } else {
                // Saved, or the changes were let go: a last upload to the
                // project's repository, then a hosted session ends with
                // nothing to recover, and so does AutoRecover.
                self.autorecover_done();
                self.repo_close();
                self.end_team_for_quit();
            }
        }
        self.keep_frames_steady(ctx);
        if self.brand.is_none() {
            self.brand = Some(Brand::load(ctx));
        }
        if self.capabilities.is_none()
            && let Some(render_state) = render_state
        {
            let info = render_state.adapter.get_info();
            self.capabilities = Some(axcenity_render::read_capabilities(
                &render_state.device,
                Some(&info),
            ));
            self.adapter = Some(format!(
                "{} ({:?}, {:?}, driver {} {})",
                info.name, info.backend, info.device_type, info.driver, info.driver_info
            ));
        }
        // Engine code changed: rebuild in the background (debug editors only).
        self.engine_frame(ctx);
        // While a project loads, only the header and the loading screen;
        // meanwhile the viewport's renderer is made, ready for it.
        if self.loading.is_some()
            && let Some(render_state) = render_state
        {
            self.viewport.make_renderer_in_background(render_state);
        }
        if self.follow_loading(ctx) {
            self.loading_frame(ui);
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
        // AI clients' calls (Settings > AI connections), before this frame's
        // input, so what they change is drawn now.
        self.ai_frame(ctx);
        let seconds = ctx
            .input(|input| f64::from(input.stable_dt))
            .min(MAX_FRAME_SECONDS);
        self.plugins_frame(seconds);
        if self.bake_probes_asked {
            self.bake_probes_asked = false;
            self.start_probe_bake();
        }
        if self.probe_bake.is_some()
            && let Some(render_state) = render_state
        {
            self.step_probe_bake(
                &render_state.device,
                &render_state.queue,
                probe_bake::FRAME_BUDGET,
            );
        }
        self.finish_thumbnail(ctx);
        // Taken from the input before the viewport sees it, so the S doesn't
        // also fly the camera backward.
        if ctx.input_mut(|input| shortcuts::consume(input, shortcuts::Action::Save)) {
            self.save();
        }
        if ctx.input_mut(|input| shortcuts::consume(input, shortcuts::Action::Settings)) {
            self.settings.open = !self.settings.open;
        }
        self.terrain_frame(ctx);
        // Collaborate: send what this frame's tools did, then catch up.
        self.team_frame();
        // The project's repository: automatic checkpoints and uploads.
        self.repo_frame();
        if let Some(every) = self.team_wants_frames() {
            ctx.request_repaint_after(every);
        }
        self.generate_frame();
        self.palette_frame(ctx);
        self.shortcuts(ctx);
        // Set again if the Model Studio is drawn under the mouse this frame;
        // a closed or hidden tab mustn't keep the keys.
        self.model_studio.forget_pointer();
        self.block_tab_pointer = None;

        // During Play, run a game frame before anything is drawn, so every
        // panel shows the world as it is now. Then ask for the next frame
        // straight away: games keep moving without input. Paused at a
        // breakpoint, the game holds still and waits for Continue.
        self.sync_breakpoints();
        if self.play.is_none() {
            // Outside Play the console starts over.
            self.type_dev_console(&[], false);
        }
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
            let (events, modifiers, focused) =
                ctx.input(|input| (input.events.clone(), input.modifiers, input.focused));
            // F9 opens the Developer Console, which then has the keys.
            let console = self.type_dev_console(&events, ctx.egui_wants_keyboard_input());
            self.type_chat(&events, ctx.egui_wants_keyboard_input() || console);
            let typing = self.focused_text.is_some()
                || self.chat_typing.is_some()
                || console
                || ctx.egui_wants_keyboard_input();
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
            let mut input = std::mem::take(&mut self.pending_game_input);
            input.extend(keys.into_iter().map(axcenity_core::input::InputEvent::from));
            let activation = self.pending_ui_activation.take();
            let focus_lost = self.pending_focus_lost.take();
            // `timescale` in the console speeds the game up or slows it.
            self.note_dev_frame(seconds);
            let seconds = seconds * self.dev.time_scale;
            self.play_frame(seconds, &input, activation, focus_lost);
            ctx.request_repaint();
        }
        let mouse = self
            .play
            .as_ref()
            .map_or((axcenity_core::MouseBehavior::Default, true), |play| {
                play.mouse()
            });
        self.viewport.hold_cursor(ctx, mouse);
        // The LOD Inspector's preview lasts while it's open on the part.
        if let Some((id, _)) = self.lod_preview
            && (self.selected != Some(id) || !self.layout.is_open(Pane::LodInspector))
        {
            self.lod_preview = None;
        }
        self.viewport.lod_preview = self
            .lod_preview
            .map(|(id, level)| (axcenity_scene::item_id(id), level));
        // However Play ended (Stop, a crash, another project), the
        // Profiler's readings freeze.
        if !self.playing() {
            self.profiler.stop();
            self.mixer.stop();
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
        let now = ctx.input(|input| input.time);
        if self.check_scripts(now) {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        let palette = theme::palette(ctx);
        // The header is the window's only title bar (see `window_frame.rs`).
        egui::Panel::top("menu bar")
            .exact_size(window_frame::HEIGHT)
            .frame(egui::Frame::new().fill(palette.app_bg))
            .show(ui, |ui| self.title_bar(ui));
        if has_project {
            // The toolbar draws its own bands: the tabs on the app
            // background, the ribbon on a panel's (see `chrome.rs`).
            egui::Panel::top("toolbar")
                .frame(egui::Frame::new().fill(palette.app_bg))
                .show_separator_line(false)
                .show(ui, |ui| self.toolbar(ui));
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
            self.layout.keep_a_document();
            self.layout.remember_homes();
            // Taken out while it's drawn, so the panels can borrow the app.
            let mut dock = std::mem::replace(&mut self.layout.dock, DockState::new(Vec::new()));
            dock.translations.tab_context_menu.eject_button = "Float in New Window".to_owned();
            egui_dock::DockArea::new(&mut dock)
                .id(floating::main_area(self.layout.dock_generation))
                .style(layout::style(
                    &ctx.global_style(),
                    &palette,
                    ctx.pixels_per_point(),
                ))
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(
                    ui,
                    &mut DockViewer {
                        app: self,
                        render_state,
                        viewport: &mut output,
                        visuals: ctx.global_style().visuals.clone(),
                        ctx: ctx.clone(),
                        floating: None,
                    },
                );
            self.layout.dock = dock;
            if std::mem::take(&mut self.pending_close_scene) {
                self.layout.close_scene();
            }
            self.after_main_dock(ctx);
            self.floating_windows(ctx, render_state, &palette);
            if let Some(command) = self.pending_command.take() {
                self.run_console_command(&command);
                // The panels were drawn before it ran.
                ctx.request_repaint();
            }
        } else {
            self.start_screen(ui);
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
        if let Some(pick) = output.surface {
            self.constraint_surface(pick);
            ctx.request_repaint();
        }
        if let Some(action) = output.spline {
            self.spline_action(action);
            ctx.request_repaint();
        }
        if let Some(mouse) = output.plugin_mouse {
            self.plugin_mouse(mouse);
        }
        if let Some(moved) = output.plugin_handle {
            self.drag_plugin_handle(moved);
            ctx.request_repaint();
        }
        if self.constraint_tool.is_some()
            && (self.playing()
                || ctx.input(|input| shortcuts::pressed(input, shortcuts::Action::PutToolDown)))
        {
            self.pick_constraint_tool(None);
        }
        if output.activated.is_some() {
            self.pending_ui_activation = output.activated;
        }
        if self.playing() {
            self.pending_game_input.extend(output.game_input);
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
        if let Some(target) = output.drop.filter(|target| target.released) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<AssetPayload>(ctx) {
                self.drop_asset(&payload.0, Some(target));
                ctx.request_repaint();
            } else if let Some(card) =
                egui::DragAndDrop::take_payload::<crate::toolbox_panel::CardDrag>(ctx)
            {
                // A Toolbox card: got first, then placed there (G1).
                self.drop_toolbox_card(ctx, &card, target);
                ctx.request_repaint();
            }
        }
        if let Some(edit) = output.drag {
            self.dragged_parts = edit.parts.iter().map(|(id, _)| *id).collect();
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
        let busy = ctx.egui_is_using_pointer() || ctx.egui_wants_keyboard_input();
        if output.drag_released || (!busy && !self.viewport.is_dragging()) {
            self.history.seal();
            self.ui_drag_baselines.clear();
        }
        // Join Surfaces welds what the drag left touching, once it's let go.
        if output.drag_released {
            let moved = std::mem::take(&mut self.dragged_parts);
            if self.snap.join {
                self.join_surfaces(&moved);
            }
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

        if self.version_history.open {
            self.version_window(ctx);
        }
        self.input_window(ctx);
        self.collision_groups_window(ctx);
        self.probe_bake_window(ctx);
        self.custom_material_window(ctx);
        self.save_layout_dialog(ctx);
        self.references_window(ctx);
        self.unsaved_changes_prompt(ctx);
        self.upgrade_prompt(ctx);
        self.delete_script_prompt(ctx);
        self.attach_script_prompt(ctx);
        self.sky_prompt(ctx);
        self.autorecover_prompt(ctx);
        self.autorecover_frame();
        self.save_template_dialog(ctx);
        self.import_dropped(ctx);
        self.import_prompt(ctx, render_state);
        self.ribbon_tabs.editor_window(ctx);
        self.start_thumbnail(ctx);
        if self.learn_open && self.project.is_some() {
            let folder = self.project.as_ref().map(|project| project.folder.clone());
            let playing = self.playing();
            self.start.set_insert_target(self.tabs.insert_target());
            if let Some(sample) =
                self.start
                    .learn_window(ctx, &mut self.learn_open, folder.as_deref(), playing)
            {
                self.request_and_go_ahead(ctx, Discard::OpenSample(sample));
            }
            // The API Reference's Insert button (scripting P2 PR 5).
            if let Some(code) = self.start.take_insert()
                && let Some(path) = self.tabs.insert_lines(&code)
            {
                let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                self.output
                    .info(format!("Inserted the example into {name}"));
            }
        }
        if self.about_open
            && let Some(brand) = &self.brand
        {
            self.about_open = start::about(ctx, brand, self.adapter.as_deref());
        }
        self.graphics_benchmark_frame(ctx, render_state);
        self.follow_sound_preview();
        self.updates_frame();
        self.update_install_frame();
        self.settings_window(ctx);
        self.recovered_changes_window(ctx);
        self.join_requests_window(ctx);
        self.collab_setup_windows(ctx);
        self.repo_ui_frame(ctx);
        self.team_board_frame(ctx);
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
mod accessibility_tests;
#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod anim_drawer_tests;
#[cfg(test)]
mod anim_transport_tests;
#[cfg(test)]
mod asset_fix_tests;
#[cfg(test)]
mod asset_refs_tests;
#[cfg(test)]
mod attribute_tests;
#[cfg(test)]
mod autorecover_tests;
#[cfg(test)]
mod behavior_tree_tests;
#[cfg(test)]
mod block_undo_tests;
#[cfg(test)]
mod camera_tests;
#[cfg(test)]
mod constraint_tests;
#[cfg(test)]
mod data_table_tests;
#[cfg(test)]
mod dev_console_tests;
#[cfg(test)]
mod dialogue_tests;
#[cfg(test)]
mod documents_tests;
#[cfg(test)]
mod explorer_keys_tests;
#[cfg(test)]
mod feedback_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod fracture_tests;
#[cfg(test)]
mod game_tests_tests;
#[cfg(test)]
mod graph_zoom_tests;
#[cfg(test)]
mod graphics_benchmark_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod ik_tests;
#[cfg(test)]
mod import_preset_tests;
#[cfg(test)]
mod imported_clip_tests;
#[cfg(test)]
mod joint_tests;
#[cfg(test)]
mod layers_tests;
#[cfg(test)]
mod live_script_tests;
#[cfg(test)]
mod mover_tests;
#[cfg(test)]
mod multi_edit_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod object_script_tests;
#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod play_tests;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod posing_tests;
#[cfg(test)]
mod prefab_tests;
#[cfg(test)]
mod scene_tools_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod spline_content_tests;
#[cfg(test)]
mod spline_tests;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
mod terrain_path_tests;
#[cfg(test)]
mod tool_window_tests;
#[cfg(test)]
mod tour_snapshots;
#[cfg(test)]
mod welcome_tests;

#[cfg(test)]
mod collab_handoff_tests;
#[cfg(test)]
mod collab_project_tests;
#[cfg(test)]
mod collab_repo_tests;
#[cfg(test)]
mod collab_scenario_tests;
#[cfg(test)]
mod collab_setup_tests;
#[cfg(test)]
mod collab_trust_tests;
#[cfg(test)]
mod decal_tests;
#[cfg(test)]
mod level_tests;
#[cfg(test)]
mod model_studio_tests;
#[cfg(test)]
mod properties_show_tests;
#[cfg(test)]
mod ragdoll_tests;
#[cfg(test)]
mod relaunch_tests;
#[cfg(test)]
mod rig_tests;
#[cfg(test)]
mod sequencer_tests;
#[cfg(test)]
mod sequencer_transport_tests;
#[cfg(test)]
mod settings_undo_tests;
#[cfg(test)]
mod solid_tests;
#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod strings_tests;
#[cfg(test)]
mod team_board_tests;
#[cfg(test)]
mod team_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_designer_menu_tests;
#[cfg(test)]
mod ui_designer_move_tests;
#[cfg(test)]
mod ui_designer_tests;
#[cfg(test)]
mod ui_graph_tests;
#[cfg(test)]
mod ui_inspector_tests;
#[cfg(test)]
mod ui_layering_tests;
#[cfg(test)]
mod ui_pages_tests;
#[cfg(test)]
mod ui_selection_tests;
#[cfg(test)]
mod ui_styles_tests;
#[cfg(test)]
mod ui_text_style_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod vehicle_tests;
#[cfg(test)]
mod version_history_tests;
