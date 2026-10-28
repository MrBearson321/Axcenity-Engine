//! The editor's dockable panels, like Unity's: every panel is a tab that can
//! be dragged by its title onto another panel's tabs, to any side of one to
//! split it, or out into a floating window. Panels other than the Scene can
//! be closed and opened again from View > Panels or the View ribbon tab.
//!
//! Each open script is a tab too ([`Pane::Script`]), opened beside the Scene
//! or the script used last, so code can sit next to the viewport. Script
//! tabs follow `ScriptTabs` (see [`Layout::sync_scripts`]) and aren't saved.
//!
//! [`egui_dock`] does the docking. This module only says which panels exist,
//! how they start out, and how the layout is saved between runs.

use eframe::egui;
use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};

/// One dockable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    /// The viewport. Always there: it can be moved but not closed or floated.
    Scene,
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
    /// A NavigationAgent's behavior tree in the graph editor
    /// (`behavior_tree.rs`). Starts closed; opening a tree opens it.
    BehaviorTree,
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
    /// An open script, stylesheet or block script, by its tab's id
    /// (`ScriptTab::id`).
    Script(u64),
}

impl Pane {
    /// The panels that can be closed and opened again, in menu order.
    pub const CLOSABLE: [Pane; 16] = [
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
        Pane::BehaviorTree,
        Pane::DataTable,
        Pane::Material,
        Pane::Tags,
        Pane::Levels,
        Pane::Sequencer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Scene => "Scene View",
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
            Pane::BehaviorTree => "Behavior Tree",
            Pane::DataTable => "Data Table",
            Pane::Material => "Material",
            Pane::Tags => "Tags",
            Pane::Levels => "Levels",
            Pane::Sequencer => "Sequencer",
            Pane::Script(_) => "Script",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Pane::Scene => icons::CUBE,
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
            Pane::BehaviorTree => icons::BRAIN,
            Pane::DataTable => icons::TABLE,
            Pane::Material => icons::SPHERE,
            Pane::Tags => icons::TAG,
            Pane::Levels => icons::STACK,
            Pane::Sequencer => icons::FILM_SLATE,
            Pane::Script(_) => icons::CODE,
        }
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
    /// The script tab used last, which new script tabs open beside.
    last_script: Option<u64>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            dock: default_dock(),
            last_script: None,
        }
    }
}

impl Layout {
    /// True if `pane` is open anywhere, docked or floating.
    pub fn is_open(&self, pane: Pane) -> bool {
        self.dock.find_tab(&pane).is_some()
    }

    /// Opens `pane` if it's closed, or closes it if it's open. The Scene
    /// always stays.
    pub fn toggle(&mut self, pane: Pane) {
        if pane == Pane::Scene {
            return;
        }
        match self.dock.find_tab(&pane) {
            Some(found) => {
                self.dock.remove_tab(found);
            }
            None => self.open(pane),
        }
    }

    /// Shows `pane`: makes it the active tab where it is, or opens it where
    /// it starts out (next to a panel that shares its place by default).
    pub fn open(&mut self, pane: Pane) {
        if let Some((surface, node, tab)) = self.dock.find_tab(&pane) {
            self.dock.set_active_tab((surface, node, tab));
            return;
        }
        let neighbor = match pane {
            Pane::Scene => None,
            Pane::Explorer => Some(Pane::Properties),
            Pane::Properties => Some(Pane::Explorer),
            Pane::Console => Some(Pane::Assets),
            Pane::Assets | Pane::Profiler | Pane::Mixer | Pane::AssetCheck => Some(Pane::Console),
            // A tab beside the Scene, with room for a preview and a graph.
            Pane::Animation
            | Pane::UiDesigner
            | Pane::BehaviorTree
            | Pane::DataTable
            | Pane::Material
            | Pane::Sequencer => Some(Pane::Scene),
            // A tab beside Properties, down the right.
            Pane::Terrain | Pane::Tags | Pane::Levels => Some(Pane::Properties),
            Pane::Script(_) => Some(Pane::Scene),
        };
        let beside = neighbor
            .and_then(|neighbor| self.dock.find_main_surface_tab(&neighbor))
            .map(|(node, _)| node);
        let tree = self.dock.main_surface_mut();
        match (pane, beside) {
            (_, Some(node)) => {
                tree.set_focused_node(node);
                tree.push_to_focused_leaf(pane);
            }
            // Its neighbor is closed too: split it off the Scene's side.
            (
                Pane::Explorer | Pane::Properties | Pane::Terrain | Pane::Tags | Pane::Levels,
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
            if self.dock.find_tab(&pane).is_some() {
                continue;
            }
            let beside = self
                .last_script
                .and_then(|last| self.dock.find_tab(&Pane::Script(last)))
                .or_else(|| self.dock.find_tab(&Pane::Scene));
            match beside {
                Some((surface, node, _)) => {
                    self.dock.set_focused_node_and_surface((surface, node));
                    self.dock.push_to_focused_leaf(pane);
                }
                None => self.dock.push_to_first_leaf(pane),
            }
        }
        self.dock
            .retain_tabs(|pane| !matches!(pane, Pane::Script(id) if !open.contains(id)));
        if let Some(found) = focus.and_then(|id| self.dock.find_tab(&Pane::Script(id))) {
            let (surface, node, _) = found;
            self.dock.set_active_tab(found);
            self.dock.set_focused_node_and_surface((surface, node));
        }
        if let Some(id) = active.filter(|id| open.contains(id)) {
            self.last_script = Some(id);
        }
    }

    /// Puts every panel back where it starts. Open scripts come back beside
    /// the Scene.
    pub fn reset(&mut self) {
        self.dock = default_dock();
        self.last_script = None;
    }

    /// The layout as text, for eframe's storage. Script tabs are left out:
    /// scripts aren't reopened between runs.
    pub fn to_text(&self) -> String {
        let dock = self
            .dock
            .filter_tabs(|pane| !matches!(pane, Pane::Script(_)));
        serde_json::to_string(&dock).unwrap_or_default()
    }

    /// A layout saved by [`to_text`](Self::to_text). A damaged one, or one
    /// that somehow lost the Scene, gives the starting layout instead.
    pub fn from_text(text: &str) -> Self {
        let dock = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|mut json| {
                // Panels not drawn yet have infinite rectangles, which JSON
                // saves as null. egui_dock works every rectangle out again
                // each frame, so any number will do.
                zero_nulls(&mut json);
                serde_json::from_value::<DockState<Pane>>(json).ok()
            })
            .filter(|dock| dock.find_main_surface_tab(&Pane::Scene).is_some())
            .map(|mut dock| {
                // The Animation and Animator tabs of older layouts both
                // load as Animation; keep the first.
                let mut seen = false;
                dock.retain_tabs(|pane| {
                    if *pane != Pane::Animation {
                        return true;
                    }
                    let first = !seen;
                    seen = true;
                    first
                });
                dock
            });
        Self {
            dock: dock.unwrap_or_else(default_dock),
            last_script: None,
        }
    }
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
    dock.set_active_tab((
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
pub fn style(style: &egui::Style, palette: &theme::Palette) -> egui_dock::Style {
    let mut dock = egui_dock::Style::from_egui(style);
    let radius = egui::CornerRadius {
        nw: 6,
        ne: 6,
        sw: 0,
        se: 0,
    };
    dock.dock_area_padding = None;
    dock.main_surface_border_stroke = egui::Stroke::NONE;

    dock.tab_bar.bg_fill = palette.app_bg;
    dock.tab_bar.height = 30.0;
    dock.tab_bar.hline_color = palette.border;
    dock.tab_bar.corner_radius = egui::CornerRadius::ZERO;

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
    dock.tab.spacing = 2.0;
    dock.tab.minimum_width = Some(72.0);
    dock.tab.tab_body.bg_fill = palette.panel;
    dock.tab.tab_body.stroke = egui::Stroke::NONE;
    dock.tab.tab_body.corner_radius = egui::CornerRadius::ZERO;
    dock.tab.tab_body.inner_margin = egui::Margin::symmetric(8, 6);

    dock.buttons.close_tab_color = palette.text_muted;
    dock.buttons.close_tab_active_color = palette.text_strong;
    dock.buttons.close_tab_bg_fill = palette.control_hover;
    dock.buttons.add_tab_bg_fill = palette.control_hover;

    // A 1-point divider in the border colour; the accent shows where it can
    // be dragged. Panels can't be squeezed below `extra` points.
    dock.separator.width = 1.0;
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
    dock.overlay.hovered_leaf_highlight.corner_radius = egui::CornerRadius::same(6);
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
                    | Pane::BehaviorTree
                    | Pane::DataTable
                    | Pane::Material
                    | Pane::Tags
                    | Pane::Levels
                    | Pane::Sequencer
                    | Pane::Mixer
            ) {
                assert!(!layout.is_open(pane), "{pane:?} starts closed");
                continue;
            }
            assert!(layout.is_open(pane), "{pane:?}");
        }
    }

    #[test]
    fn closed_panels_open_again_and_the_scene_never_closes() {
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
        let (surface, node, _) = dock.find_tab(&Pane::Animation).unwrap();
        dock.set_focused_node_and_surface((surface, node));
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
        let (surface, node, _) = layout.dock.find_tab(&pane).unwrap();
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
        layout.dock.set_focused_node_and_surface(explorer);
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
}
