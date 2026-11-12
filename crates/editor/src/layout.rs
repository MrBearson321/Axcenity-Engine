//! The editor's dockable panels, like Unity's: every panel is a tab that can
//! be dragged by its title onto another panel's tabs, to any side of one to
//! split it, or out into a floating window. Panels other than the Scene can
//! be closed and opened again from View > Panels or the Home tab's Panels group.
//!
//! Each open script is a tab too ([`Pane::Script`]), opened beside the Scene
//! or the script used last, so code can sit next to the viewport. Script
//! tabs follow `ScriptTabs` (see [`Layout::sync_scripts`]) and aren't saved.
//!
//! Dragging a tab off the edge of its window, or dropping it where a split
//! won't take it, puts it in an OS window of its own (`layout/floating.rs`).
//!
//! [`egui_dock`] does the docking. This module only says which panels exist,
//! how they start out, and how the layout is saved between runs.

use std::collections::HashMap;

use eframe::egui;
use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};

pub mod floating;
use floating::{FloatingWindow, Home, SavedWindow};
pub mod presets;
pub use presets::{Preset, SavedLayouts};

/// How tall a dock's row of tabs is, in points.
pub const TAB_BAR_HEIGHT: f32 = 30.0;

/// One dockable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    /// The viewport. It can be moved and closed but not floated; closed,
    /// it leaves the [`Pane::Welcome`] page in its place.
    Scene,
    /// The welcome page (`app/welcome.rs`): shown where the documents go
    /// whenever no document tab is open (the Scene closed and no script or
    /// editor tab), and gone again as soon as one opens.
    Welcome,
    Explorer,
    Properties,
    /// The log and the command line. Saved layouts from before it was
    /// the Console call it `Output`.
    #[serde(alias = "Output")]
    Console,
    Assets,
    /// The Animation window: the selected Animator's state machine and
    /// clips. Saved layouts from before v3 Phase 8 also had an `Animator`
    /// tab, which becomes this one.
    #[serde(alias = "Animator")]
    Animation,
    /// The UI Designer: a ScreenGui on an artboard at its design size
    /// (`ui_designer.rs`).
    UiDesigner,
    /// The Terrain Studio: sculpting, painting and generating terrains.
    Terrain,
    /// What the game costs during Play (`profiler.rs`).
    Profiler,
    /// The SoundGroups with live levels, mute and solo (`mixer.rs`, Phase
    /// 52).
    Mixer,
    /// Missing files, invalid references, import failures, and broken
    /// dependencies (`asset_check.rs`). Starts closed; Check Assets opens it.
    AssetCheck,
    /// The selected MeshPart's levels of detail (`lod_inspector.rs`).
    /// Starts closed.
    LodInspector,
    /// A NavigationAgent's behavior tree in the graph editor
    /// (`behavior_tree.rs`). Starts closed; opening a tree opens it.
    BehaviorTree,
    /// A `.dialogue.json` in the graph editor (`dialogue_editor.rs`, Phase
    /// 77). Starts closed; opening a dialogue opens it.
    Dialogue,
    /// A data table as a grid (`data_table.rs`). Starts closed; opening a
    /// table opens it.
    DataTable,
    /// A material file's values, bases and preview (`material_editor.rs`).
    /// Starts closed; opening a material opens it.
    Material,
    /// Every tag in the world (`tags.rs`). Starts closed.
    Tags,
    /// The project's levels (`levels.rs`, Phase 48). Starts closed.
    Levels,
    /// A cutscene sequence on a timeline (`sequencer.rs`, Phase 51).
    /// Starts closed; opening a sequence opens it.
    Sequencer,
    /// The project's game tests and how they went (`game_tests.rs`,
    /// Phase 53). Starts closed.
    Tests,
    /// The editor's Lua plugins: on, off and their errors (`plugins.rs`,
    /// Phase 54). Starts closed.
    Plugins,
    /// Editing a model's meshes (`model_studio.rs`, Phase 55). Starts
    /// closed; opening a model in it opens it.
    ModelStudio,
    /// The Toolbox: assets from ambientCG, Poly Haven and Openverse, and
    /// the asset library (`toolbox_panel.rs`). Starts closed; the Assets
    /// panel's Toolbox button opens it beside Assets.
    Toolbox,
    /// Every asset downloaded into the asset library or imported into the
    /// project, by type (`asset_inventory.rs`). Starts closed; the Toolbox's
    /// view menu and the Assets panel's Source menu open it beside Assets.
    Inventory,
    /// The engine's versions: this build, its releases, features, known
    /// issues and what changed between two versions (`version_explorer.rs`).
    /// Starts closed; Help > Version Explorer opens it.
    VersionExplorer,
    /// Collaborate: sharing, joining, who's here, invites and roles
    /// (`app/team_panel.rs`). Starts closed; File > Share with Collaborate
    /// and the title bar chip open it.
    TeamCreate,
    /// An open script, stylesheet or block script, by its tab's id
    /// (`ScriptTab::id`).
    Script(u64),
    /// A plugin's dock tab, by its number (`WidgetView::pane`). Not saved:
    /// plugins open their tabs again when they start.
    Plugin(u64),
}

impl Pane {
    /// The panels that can be closed and opened again, in menu order.
    pub const CLOSABLE: [Pane; 25] = [
        Pane::Explorer,
        Pane::Properties,
        Pane::Console,
        Pane::Assets,
        Pane::Animation,
        Pane::UiDesigner,
        Pane::Terrain,
        Pane::Profiler,
        Pane::Mixer,
        Pane::AssetCheck,
        Pane::LodInspector,
        Pane::BehaviorTree,
        Pane::Dialogue,
        Pane::DataTable,
        Pane::Material,
        Pane::Tags,
        Pane::Levels,
        Pane::Sequencer,
        Pane::Tests,
        Pane::Plugins,
        Pane::ModelStudio,
        Pane::Toolbox,
        Pane::Inventory,
        Pane::VersionExplorer,
        Pane::TeamCreate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Scene => "Scene View",
            Pane::Welcome => "Welcome",
            Pane::Explorer => "Explorer",
            Pane::Properties => "Properties",
            Pane::Console => "Console",
            Pane::Assets => "Assets",
            Pane::Animation => "Animation",
            Pane::UiDesigner => "UI Designer",
            Pane::Terrain => "Terrain",
            Pane::Profiler => "Profiler",
            Pane::Mixer => "Mixer",
            Pane::AssetCheck => "Asset Check",
            Pane::LodInspector => "LOD Inspector",
            Pane::BehaviorTree => "Behavior Tree",
            Pane::Dialogue => "Dialogue",
            Pane::DataTable => "Data Table",
            Pane::Material => "Material",
            Pane::Tags => "Tags",
            Pane::Levels => "Levels",
            Pane::Sequencer => "Sequencer",
            Pane::Tests => "Tests",
            Pane::Plugins => "Plugins",
            Pane::ModelStudio => "Model Studio",
            Pane::Toolbox => "Toolbox",
            Pane::Inventory => "Downloaded / Imported Assets",
            Pane::VersionExplorer => "Version Explorer",
            Pane::TeamCreate => "Collaborate",
            Pane::Script(_) => "Script",
            Pane::Plugin(_) => "Plugin",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Pane::Scene => icons::CUBE,
            Pane::Welcome => icons::HOUSE,
            Pane::Explorer => icons::TREE_STRUCTURE,
            Pane::Properties => icons::SLIDERS_HORIZONTAL,
            Pane::Console => icons::TERMINAL_WINDOW,
            Pane::Assets => icons::SHAPES,
            Pane::Animation => theme::AnimKind::Animator.icon(),
            Pane::UiDesigner => icons::LAYOUT,
            Pane::Terrain => icons::MOUNTAINS,
            Pane::Profiler => icons::GAUGE,
            Pane::Mixer => icons::FADERS,
            Pane::AssetCheck => icons::LIST,
            Pane::LodInspector => icons::MAGNIFYING_GLASS,
            Pane::BehaviorTree => icons::BRAIN,
            Pane::Dialogue => icons::CHAT_CIRCLE_TEXT,
            Pane::DataTable => icons::TABLE,
            Pane::Material => icons::SPHERE,
            Pane::Tags => icons::TAG,
            Pane::Levels => icons::STACK,
            Pane::Sequencer => icons::FILM_SLATE,
            Pane::Tests => icons::FLASK,
            Pane::Plugins => icons::PUZZLE_PIECE,
            Pane::ModelStudio => icons::POLYGON,
            Pane::Toolbox => icons::TOOLBOX,
            Pane::Inventory => icons::TRAY_ARROW_DOWN,
            Pane::VersionExplorer => icons::GIT_BRANCH,
            Pane::TeamCreate => icons::USERS_THREE,
            Pane::Script(_) => icons::CODE,
            Pane::Plugin(_) => icons::PUZZLE_PIECE,
        }
    }

    /// Whether the tab is a document (the Scene, a script or an editor of
    /// one file), the kind the welcome page stands in for. Animation and the
    /// UI Designer follow the selection, like panels, so they aren't.
    pub fn is_document(self) -> bool {
        matches!(
            self,
            Pane::Scene
                | Pane::Script(_)
                | Pane::BehaviorTree
                | Pane::Dialogue
                | Pane::DataTable
                | Pane::Material
                | Pane::Sequencer
                | Pane::ModelStudio
        )
    }

    /// The title shown on the panel's tab.
    pub fn title(self) -> egui::WidgetText {
        egui::RichText::new(theme::with_icon(self.icon(), self.label()))
            .family(theme::medium_family())
            .size(theme::type_scale::BODY - 0.5)
            .into()
    }
}

/// The panels and where they are.
pub struct Layout {
    pub dock: DockState<Pane>,
    /// Panels in OS windows of their own.
    pub windows: Vec<FloatingWindow>,
    /// The last [`FloatingWindow::id`] given out.
    next_window: u64,
    /// Goes up when a tab is torn off mid-drag; part of every dock area's id.
    pub dock_generation: u64,
    /// The script tab used last, which new script tabs open beside.
    last_script: Option<u64>,
    /// Where each panel was last docked in the main window.
    homes: HashMap<Pane, Home>,
    /// True from switching layouts until the scripts come back, which then
    /// go behind the tab the layout puts in front.
    switched: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            dock: default_dock(),
            windows: Vec::new(),
            next_window: 0,
            dock_generation: 0,
            last_script: None,
            homes: HashMap::new(),
            switched: false,
        }
    }
}

impl Layout {
    /// True if `pane` is open anywhere, docked or floating.
    pub fn is_open(&self, pane: Pane) -> bool {
        self.dock.find_tab(&pane).is_some() || self.windows.iter().any(|window| window.holds(pane))
    }

    /// Opens `pane` if it's closed, or closes it if it's open. The Scene
    /// and the welcome page aren't toggled: the Scene closes from its tab.
    pub fn toggle(&mut self, pane: Pane) {
        if matches!(pane, Pane::Scene | Pane::Welcome) {
            return;
        }
        if let Some(found) = self.dock.find_tab(&pane) {
            self.dock.remove_tab(found);
        } else if let Some(window) = self.windows.iter_mut().find(|w| w.holds(pane)) {
            if let Some(found) = window.dock.find_tab(&pane) {
                window.dock.remove_tab(found);
            }
            self.drop_empty_windows();
        } else {
            self.open(pane);
        }
    }

    /// Shows `pane`: makes it the active tab where it is, or opens it where
    /// it starts out (next to a panel that shares its place by default).
    pub fn open(&mut self, pane: Pane) {
        if let Some(found) = self.dock.find_tab(&pane) {
            // Fails only for a tab that isn't there, and this one was just found.
            let _ = self.dock.set_active_tab(found);
            return;
        }
        if let Some(window) = self.windows.iter_mut().find(|w| w.holds(pane)) {
            if let Some(found) = window.dock.find_tab(&pane) {
                let _ = window.dock.set_active_tab(found);
            }
            window.focus = true;
            return;
        }
        let neighbor = match pane {
            // Where the documents go (see `documents_place`).
            Pane::Scene | Pane::Welcome => Some(Pane::Scene),
            Pane::Explorer => Some(Pane::Properties),
            Pane::Properties => Some(Pane::Explorer),
            Pane::Console | Pane::Toolbox | Pane::Inventory => Some(Pane::Assets),
            Pane::Assets | Pane::Profiler | Pane::Mixer | Pane::AssetCheck | Pane::Tests => {
                Some(Pane::Console)
            }
            // A tab beside the Scene, with room for a preview and a graph.
            Pane::Animation
            | Pane::UiDesigner
            | Pane::BehaviorTree
            | Pane::Dialogue
            | Pane::DataTable
            | Pane::Material
            | Pane::Sequencer
            | Pane::ModelStudio => Some(Pane::Scene),
            // A tab beside Properties, down the right.
            Pane::Terrain
            | Pane::Tags
            | Pane::Levels
            | Pane::Plugins
            | Pane::LodInspector
            | Pane::VersionExplorer
            | Pane::TeamCreate
            | Pane::Plugin(_) => Some(Pane::Properties),
            Pane::Script(_) => Some(Pane::Scene),
        };
        let beside = match neighbor {
            Some(Pane::Scene) => self.documents_place(),
            Some(neighbor) => self
                .dock
                .find_main_surface_tab(&neighbor)
                .map(|(node, _)| node),
            None => None,
        };
        let tree = self.dock.main_surface_mut();
        match (pane, beside) {
            (_, Some(node)) => {
                tree.set_focused_node(node);
                tree.push_to_focused_leaf(pane);
            }
            // Its neighbor is closed too: split it off the Scene's side.
            (
                Pane::Explorer
                | Pane::Properties
                | Pane::Terrain
                | Pane::Tags
                | Pane::Levels
                | Pane::Plugins
                | Pane::VersionExplorer
                | Pane::TeamCreate
                | Pane::Plugin(_),
                None,
            ) => {
                tree.split_right(NodeIndex::root(), 0.78, vec![pane]);
            }
            _ => {
                tree.split_below(NodeIndex::root(), 0.78, vec![pane]);
            }
        }
    }

    /// Makes the script tabs match the open scripts: `open` lists their ids,
    /// `focus` is one to bring to the front, and `active` the one used last.
    ///
    /// A new tab opens beside the script tab used last (wherever it was
    /// dragged), or else beside the Scene. Tabs are added before closed
    /// ones are removed, so a script that switches between Code and Blocks
    /// keeps its place.
    pub fn sync_scripts(&mut self, open: &[u64], focus: Option<u64>, active: Option<u64>) {
        for &id in open {
            let pane = Pane::Script(id);
            if self.is_open(pane) {
                continue;
            }
            // Beside the last script when that one is in a window of its own.
            if let Some(window) = self.last_script.and_then(|last| {
                self.windows
                    .iter_mut()
                    .find(|w| w.holds(Pane::Script(last)))
            }) {
                window.dock.push_to_first_leaf(pane);
                continue;
            }
            let beside = self
                .last_script
                .and_then(|last| self.dock.find_tab(&Pane::Script(last)))
                .or_else(|| self.dock.find_tab(&Pane::Scene))
                .or_else(|| self.dock.find_tab(&Pane::Welcome));
            match beside {
                Some(egui_dock::TabPath { surface, node, .. }) => {
                    let front = match &self.dock[surface][node] {
                        egui_dock::Node::Leaf(leaf) if self.switched => Some(leaf.active),
                        _ => None,
                    };
                    self.dock
                        .set_focused_node_and_surface(egui_dock::NodePath::new(surface, node));
                    self.dock.push_to_focused_leaf(pane);
                    if let Some(front) = front {
                        let _ = self
                            .dock
                            .set_active_tab(egui_dock::TabPath::new(surface, node, front));
                    }
                }
                None => self.dock.push_to_first_leaf(pane),
            }
        }
        let keep = |pane: &mut Pane| !matches!(pane, Pane::Script(id) if !open.contains(id));
        self.dock.retain_tabs(keep);
        for window in &mut self.windows {
            window.dock.retain_tabs(keep);
        }
        self.drop_empty_windows();
        if let Some(id) = focus {
            let pane = Pane::Script(id);
            if let Some(found) = self.dock.find_tab(&pane) {
                let egui_dock::TabPath { surface, node, .. } = found;
                let _ = self.dock.set_active_tab(found);
                self.dock
                    .set_focused_node_and_surface(egui_dock::NodePath::new(surface, node));
            } else if self.is_open(pane) {
                self.open(pane);
            }
        }
        if let Some(id) = active.filter(|id| open.contains(id)) {
            self.last_script = Some(id);
        }
        self.switched = false;
    }

    /// Where documents open in the main window: beside the Scene, or the
    /// welcome page standing in for it, or else any open document.
    fn documents_place(&self) -> Option<NodeIndex> {
        let tree = self.dock.main_surface();
        [Pane::Scene, Pane::Welcome]
            .iter()
            .find_map(|pane| tree.find_tab(pane))
            .or_else(|| tree.find_tab_from(|pane| pane.is_document()))
            .map(|(node, _)| node)
    }

    /// Closes the Scene, leaving the welcome page in its place if no other
    /// document is open (see [`keep_a_document`](Self::keep_a_document)).
    pub fn close_scene(&mut self) {
        for (_, pane) in self.dock.iter_all_tabs_mut() {
            if *pane == Pane::Scene {
                *pane = Pane::Welcome;
            }
        }
        self.keep_a_document();
        if let Some(found) = self.dock.find_tab(&Pane::Welcome) {
            let _ = self.dock.set_active_tab(found);
        }
    }

    /// Shows the welcome page where the documents go while no document tab
    /// is open, and takes it away once one is. Called each frame after the
    /// script tabs are synced.
    pub fn keep_a_document(&mut self) {
        let documents = self
            .dock
            .iter_all_tabs()
            .any(|(_, pane)| pane.is_document())
            || self.windows.iter().any(|window| {
                window
                    .dock
                    .iter_all_tabs()
                    .any(|(_, pane)| pane.is_document())
            });
        let welcome = self.dock.find_tab(&Pane::Welcome);
        match (documents, welcome) {
            (true, Some(_)) => self.dock.retain_tabs(|pane| *pane != Pane::Welcome),
            (false, None) => {
                // The biggest place in the main window, as it was last drawn.
                let biggest = self
                    .dock
                    .main_surface()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, node)| match node {
                        egui_dock::Node::Leaf(leaf) => Some((index, leaf.rect.area())),
                        _ => None,
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| NodeIndex(index));
                let tree = self.dock.main_surface_mut();
                match biggest {
                    Some(node) => {
                        tree.set_focused_node(node);
                        tree.push_to_focused_leaf(Pane::Welcome);
                    }
                    None => tree.push_to_first_leaf(Pane::Welcome),
                }
            }
            _ => {}
        }
    }

    /// Makes the plugin dock tabs match `open`, the plugin widgets that
    /// show: new ones open beside Properties, and the rest close.
    pub fn sync_plugins(&mut self, open: &[u64]) {
        for &id in open {
            if !self.is_open(Pane::Plugin(id)) {
                self.open(Pane::Plugin(id));
            }
        }
        let keep = |pane: &mut Pane| !matches!(pane, Pane::Plugin(id) if !open.contains(id));
        self.dock.retain_tabs(keep);
        for window in &mut self.windows {
            window.dock.retain_tabs(keep);
        }
        self.drop_empty_windows();
    }

    /// Puts every panel back where it starts. Open scripts come back beside
    /// the Scene.
    pub fn reset(&mut self) {
        self.dock = default_dock();
        self.windows.clear();
        self.last_script = None;
        self.homes.clear();
    }

    /// The layout as text, for eframe's storage. Script tabs are left out:
    /// scripts aren't reopened between runs.
    pub fn to_text(&self) -> String {
        let keep = |pane: &Pane| !matches!(pane, Pane::Script(_) | Pane::Plugin(_));
        serde_json::to_string(&SavedLayout {
            dock: self.dock.filter_tabs(keep),
            windows: self.saved_windows(keep),
            homes: self
                .homes
                .iter()
                .filter(|(pane, _)| keep(pane))
                .map(|(pane, home)| (*pane, home.clone()))
                .collect(),
        })
        .unwrap_or_default()
    }

    /// A layout saved by [`to_text`](Self::to_text), or by a version from
    /// before panels had windows (then the text is just the dock). A damaged
    /// one, or one with neither the Scene nor the welcome page, gives the
    /// starting layout.
    pub fn from_text(text: &str) -> Self {
        let saved = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|mut json| {
                // Panels not drawn yet have infinite rectangles, which JSON
                // saves as null. egui_dock works every rectangle out again
                // each frame, so any number will do.
                zero_nulls(&mut json);
                if json.get("windows").is_some() {
                    serde_json::from_value::<SavedLayout>(json).ok()
                } else {
                    let dock = serde_json::from_value::<DockState<Pane>>(json).ok()?;
                    Some(SavedLayout {
                        dock,
                        windows: Vec::new(),
                        homes: Vec::new(),
                    })
                }
            })
            .filter(|saved| {
                [Pane::Scene, Pane::Welcome]
                    .iter()
                    .any(|pane| saved.dock.find_main_surface_tab(pane).is_some())
            })
            .map(|mut saved| {
                // The Animation and Animator tabs of older layouts both
                // load as Animation; keep the first.
                let mut seen = false;
                saved.dock.retain_tabs(|pane| {
                    if *pane != Pane::Animation {
                        return true;
                    }
                    let first = !seen;
                    seen = true;
                    first
                });
                saved
            });
        let mut layout = Self::default();
        if let Some(saved) = saved {
            layout.dock = saved.dock;
            for window in saved.windows {
                layout.restore_window(window);
            }
            layout.homes = saved.homes.into_iter().collect();
        }
        layout
    }
}

/// How the panels are written to eframe's storage.
#[derive(Serialize, Deserialize)]
struct SavedLayout {
    dock: DockState<Pane>,
    windows: Vec<SavedWindow>,
    /// A list, as a JSON object's keys could only be strings.
    #[serde(default)]
    homes: Vec<(Pane, Home)>,
}

/// Replaces null rectangle corners (`"x": null`, `"y": null`) in `json`
/// with 0. Other nulls are empty options and stay.
fn zero_nulls(json: &mut serde_json::Value) {
    match json {
        serde_json::Value::Array(items) => items.iter_mut().for_each(zero_nulls),
        serde_json::Value::Object(fields) => {
            for (name, value) in fields.iter_mut() {
                if value.is_null() && (name == "x" || name == "y") {
                    *value = serde_json::Value::from(0.0);
                } else {
                    zero_nulls(value);
                }
            }
        }
        _ => {}
    }
}

/// Like Roblox Studio: the Scene in the middle (with the Animation tab and
/// the UI Designer beside it, like Unity's), the Explorer with
/// Properties (and the Terrain Studio as a tab beside it) under it down the
/// right, and the Console, Assets and Profiler as tabs under the Scene.
fn default_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Scene, Pane::Animation, Pane::UiDesigner]);
    let _ = dock.set_active_tab(egui_dock::TabPath::new(
        egui_dock::SurfaceIndex::main(),
        NodeIndex::root(),
        egui_dock::TabIndex(0),
    ));
    let tree = dock.main_surface_mut();
    let [scene, explorer] = tree.split_right(NodeIndex::root(), 0.78, vec![Pane::Explorer]);
    tree.split_below(explorer, 0.45, vec![Pane::Properties, Pane::Terrain]);
    tree.split_below(
        scene,
        0.78,
        vec![Pane::Console, Pane::Assets, Pane::Profiler],
    );
    dock
}

/// The docking area's look, from the theme's tokens: quiet tab bars on the
/// app background, the active tab joined to its panel, thin dividers that
/// light up in the accent when hovered or dragged, and a minimum panel size.
pub fn style(
    style: &egui::Style,
    palette: &theme::Palette,
    pixels_per_point: f32,
) -> egui_dock::Style {
    let mut dock = egui_dock::Style::from_egui(style);
    let radius = egui::CornerRadius::same(theme::radius::NONE);
    dock.dock_area_padding = None;
    dock.main_surface_border_stroke = egui::Stroke::NONE;

    dock.tab_bar.bg_fill = palette.app_bg;
    dock.tab_bar.height = TAB_BAR_HEIGHT;
    dock.tab_bar.hline_color = palette.border;
    dock.tab_bar.corner_radius = egui::CornerRadius::same(theme::radius::NONE);

    let tab = |bg_fill, text_color, outline_color| egui_dock::TabInteractionStyle {
        bg_fill,
        text_color,
        outline_color,
        corner_radius: radius,
    };
    // The active tab shares its panel's colour, so it reads as part of it.
    dock.tab.active = tab(palette.panel, palette.text, palette.border);
    dock.tab.focused = tab(palette.panel, palette.text_strong, palette.border);
    dock.tab.inactive = tab(
        palette.app_bg,
        palette.text_weak,
        egui::Color32::TRANSPARENT,
    );
    dock.tab.hovered = tab(
        palette.control,
        palette.text_strong,
        egui::Color32::TRANSPARENT,
    );
    dock.tab.active_with_kb_focus = tab(palette.panel, palette.text_strong, palette.accent);
    dock.tab.focused_with_kb_focus = tab(palette.panel, palette.text_strong, palette.accent);
    dock.tab.inactive_with_kb_focus = tab(palette.app_bg, palette.text_strong, palette.accent);
    dock.tab.hline_below_active_tab_name = false;
    dock.tab.spacing = 0.0;
    dock.tab.minimum_width = Some(72.0);
    dock.tab.tab_body.bg_fill = palette.panel;
    dock.tab.tab_body.stroke = egui::Stroke::NONE;
    dock.tab.tab_body.corner_radius = egui::CornerRadius::same(theme::radius::NONE);
    dock.tab.tab_body.inner_margin = egui::Margin::symmetric(8, 6);

    dock.buttons.close_tab_color = palette.text_muted;
    dock.buttons.close_tab_active_color = palette.text_strong;
    dock.buttons.close_tab_bg_fill = palette.control_hover;
    dock.buttons.add_tab_bg_fill = palette.control_hover;

    // A hairline divider in the border colour, one pixel wide at any scale;
    // the accent shows where it can be dragged. Panels can't be squeezed
    // below `extra` points.
    dock.separator.width = 1.0 / pixels_per_point.max(1.0);
    dock.separator.extra_interact_width = 6.0;
    dock.separator.extra = 140.0;
    dock.separator.color_idle = palette.border;
    dock.separator.color_hovered = palette.accent;
    dock.separator.color_dragged = palette.accent;

    dock.overlay.selection_color = palette.accent.gamma_multiply(0.25);
    dock.overlay.button_color = palette.elevated;
    dock.overlay.button_border_stroke = egui::Stroke::new(1.0_f32, palette.border_strong);
    dock.overlay.hovered_leaf_highlight.color = palette.accent.gamma_multiply(0.08);
    dock.overlay.hovered_leaf_highlight.stroke = egui::Stroke::new(1.0_f32, palette.accent);
    dock.overlay.hovered_leaf_highlight.corner_radius =
        egui::CornerRadius::same(theme::radius::NONE);
    dock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_starts_open() {
        let layout = Layout::default();
        for pane in Pane::CLOSABLE.into_iter().chain([Pane::Scene]) {
            if matches!(
                pane,
                Pane::AssetCheck
                    | Pane::LodInspector
                    | Pane::BehaviorTree
                    | Pane::Dialogue
                    | Pane::DataTable
                    | Pane::Material
                    | Pane::Tags
                    | Pane::Levels
                    | Pane::Sequencer
                    | Pane::Mixer
                    | Pane::Tests
                    | Pane::Plugins
                    | Pane::ModelStudio
                    | Pane::Toolbox
                    | Pane::Inventory
                    | Pane::VersionExplorer
                    | Pane::TeamCreate
            ) {
                assert!(!layout.is_open(pane), "{pane:?} starts closed");
                continue;
            }
            assert!(layout.is_open(pane), "{pane:?}");
        }
    }

    #[test]
    fn closed_panels_open_again_and_the_scene_does_not_toggle() {
        let mut layout = Layout::default();
        for pane in Pane::CLOSABLE {
            if layout.is_open(pane) {
                layout.toggle(pane);
            }
            assert!(!layout.is_open(pane), "{pane:?}");
        }
        layout.toggle(Pane::Scene);
        assert!(layout.is_open(Pane::Scene));
        for pane in Pane::CLOSABLE {
            layout.toggle(pane);
            assert!(layout.is_open(pane), "{pane:?}");
        }
    }

    #[test]
    fn the_layout_survives_saving_and_bad_text_gives_the_default() {
        let mut layout = Layout::default();
        layout.toggle(Pane::Assets);
        let reloaded = Layout::from_text(&layout.to_text());
        assert!(!reloaded.is_open(Pane::Assets));
        assert!(reloaded.is_open(Pane::Console));
        // A layout saved when the Console was called Output.
        let old = layout.to_text().replace("\"Console\"", "\"Output\"");
        assert!(old.contains("\"Output\""));
        assert!(Layout::from_text(&old).is_open(Pane::Console));
        assert!(Layout::from_text("not json").is_open(Pane::Assets));
    }

    #[test]
    fn a_layout_with_the_old_animator_tab_keeps_one_animation_tab() {
        let layout = Layout::default();
        let mut dock = layout.dock.clone();
        let egui_dock::TabPath { surface, node, .. } = dock.find_tab(&Pane::Animation).unwrap();
        dock.set_focused_node_and_surface(egui_dock::NodePath::new(surface, node));
        dock.push_to_focused_leaf(Pane::Console);
        // Saved before v3 Phase 8: the second tab was the Animator.
        let text = serde_json::to_string(&dock)
            .unwrap()
            .replacen("\"Console\"", "\"Animator\"", 1);
        let reloaded = Layout::from_text(&text);
        let mut count = 0;
        for (_, pane) in reloaded.dock.iter_all_tabs() {
            if *pane == Pane::Animation {
                count += 1;
            }
        }
        assert_eq!(count, 1);
        assert!(reloaded.is_open(Pane::Console));
    }

    /// The leaf (surface and node) a panel is in.
    fn leaf_of(layout: &Layout, pane: Pane) -> (egui_dock::SurfaceIndex, NodeIndex) {
        let egui_dock::TabPath { surface, node, .. } = layout.dock.find_tab(&pane).unwrap();
        (surface, node)
    }

    #[test]
    fn script_tabs_follow_the_open_scripts_and_open_beside_the_last_one() {
        let mut layout = Layout::default();
        layout.sync_scripts(&[0], Some(0), Some(0));
        assert_eq!(
            leaf_of(&layout, Pane::Script(0)),
            leaf_of(&layout, Pane::Scene)
        );

        // Dragged off beside the Explorer, the next script joins it there.
        let found = layout.dock.find_tab(&Pane::Script(0)).unwrap();
        layout.dock.remove_tab(found);
        let explorer = leaf_of(&layout, Pane::Explorer);
        layout
            .dock
            .set_focused_node_and_surface(egui_dock::NodePath::new(explorer.0, explorer.1));
        layout.dock.push_to_focused_leaf(Pane::Script(0));
        layout.sync_scripts(&[0, 1], Some(1), Some(1));
        assert_eq!(leaf_of(&layout, Pane::Script(1)), explorer);

        // Closed scripts lose their tabs, and they're never saved.
        assert!(!layout.to_text().contains("Script"));
        layout.sync_scripts(&[1], None, Some(1));
        assert!(!layout.is_open(Pane::Script(0)));
        assert!(layout.is_open(Pane::Script(1)));
        assert!(!Layout::from_text(&layout.to_text()).is_open(Pane::Script(1)));
    }

    fn front_of(layout: &Layout, pane: Pane) -> bool {
        layout
            .dock
            .iter_leaves()
            .any(|(_, leaf)| leaf.tabs.get(leaf.active.0) == Some(&pane))
    }

    #[test]
    fn closing_the_scene_leaves_the_welcome_page_in_its_place() {
        let mut layout = Layout::default();
        let scene = layout.dock.find_tab(&Pane::Scene).unwrap();
        layout.close_scene();
        assert!(!layout.is_open(Pane::Scene));
        let welcome = layout.dock.find_tab(&Pane::Welcome).expect("welcome");
        assert_eq!(welcome, scene, "in the Scene's own place");
        assert!(front_of(&layout, Pane::Welcome));

        // Show Scene puts the Scene back there, and the page goes.
        layout.open(Pane::Scene);
        layout.keep_a_document();
        assert_eq!(
            layout.dock.find_tab(&Pane::Scene).map(|at| at.node),
            Some(scene.node)
        );
        assert!(!layout.is_open(Pane::Welcome));
    }

    #[test]
    fn the_welcome_page_shows_only_while_no_document_is_open() {
        let mut layout = Layout::default();
        layout.close_scene();
        // A script opens where the page was, and the page goes.
        layout.sync_scripts(&[7], Some(7), Some(7));
        layout.keep_a_document();
        assert!(layout.is_open(Pane::Script(7)));
        assert!(!layout.is_open(Pane::Welcome));
        // Closing the last document brings it back.
        layout.sync_scripts(&[], None, None);
        layout.keep_a_document();
        assert!(layout.is_open(Pane::Welcome));
        // With the Scene open it never shows.
        let mut layout = Layout::default();
        layout.keep_a_document();
        assert!(!layout.is_open(Pane::Welcome));
        // And it isn't toggled from the View menu.
        layout.toggle(Pane::Welcome);
        assert!(!layout.is_open(Pane::Welcome));
    }

    #[test]
    fn the_scene_comes_back_beside_open_scripts() {
        let mut layout = Layout::default();
        layout.sync_scripts(&[3], Some(3), Some(3));
        layout.close_scene();
        assert!(!layout.is_open(Pane::Welcome), "a script is open");
        layout.open(Pane::Scene);
        let scene = layout.dock.find_tab(&Pane::Scene).unwrap();
        let script = layout.dock.find_tab(&Pane::Script(3)).unwrap();
        assert_eq!(scene.node, script.node, "beside the script, not split off");
    }

    #[test]
    fn a_saved_layout_with_the_scene_closed_loads() {
        let mut layout = Layout::default();
        layout.close_scene();
        let loaded = Layout::from_text(&layout.to_text());
        assert!(loaded.is_open(Pane::Welcome));
        assert!(!loaded.is_open(Pane::Scene));
    }
}
