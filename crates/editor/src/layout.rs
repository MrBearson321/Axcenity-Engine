//! The editor's dockable panels, like Unity's: every panel is a tab that can
//! be dragged by its title onto another panel's tabs, to any side of one to
//! split it, or out into a floating window. Panels other than the Scene can
//! be closed and opened again from View > Panels or the View ribbon tab.
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
    /// The viewport and the open scripts, with their own tab bar. Always
    /// there: it can be moved but not closed or floated.
    Scene,
    Explorer,
    Properties,
    Output,
    Assets,
    /// The Animation Studio: clips, controllers and their preview.
    Animation,
    /// The Terrain Studio: sculpting, painting and generating terrains.
    Terrain,
}

impl Pane {
    /// The panels that can be closed and opened again, in menu order.
    pub const CLOSABLE: [Pane; 6] = [
        Pane::Explorer,
        Pane::Properties,
        Pane::Output,
        Pane::Assets,
        Pane::Animation,
        Pane::Terrain,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Scene => "Scene",
            Pane::Explorer => "Explorer",
            Pane::Properties => "Properties",
            Pane::Output => "Output",
            Pane::Assets => "Assets",
            Pane::Animation => "Animation",
            Pane::Terrain => "Terrain",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Pane::Scene => icons::CUBE,
            Pane::Explorer => icons::TREE_STRUCTURE,
            Pane::Properties => icons::SLIDERS_HORIZONTAL,
            Pane::Output => icons::TERMINAL_WINDOW,
            Pane::Assets => icons::SHAPES,
            Pane::Animation => icons::PERSON_SIMPLE_RUN,
            Pane::Terrain => icons::MOUNTAINS,
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
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            dock: default_dock(),
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
            Pane::Output => Some(Pane::Assets),
            Pane::Assets => Some(Pane::Output),
            // A tab beside the Scene, with room for its graph and preview.
            Pane::Animation => Some(Pane::Scene),
            // A tab beside Properties, down the right.
            Pane::Terrain => Some(Pane::Properties),
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
            (Pane::Explorer | Pane::Properties | Pane::Terrain, None) => {
                tree.split_right(NodeIndex::root(), 0.78, vec![pane]);
            }
            _ => {
                tree.split_below(NodeIndex::root(), 0.78, vec![pane]);
            }
        }
    }

    /// Puts every panel back where it starts.
    pub fn reset(&mut self) {
        self.dock = default_dock();
    }

    /// The layout as text, for eframe's storage.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&self.dock).unwrap_or_default()
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
            .filter(|dock| dock.find_main_surface_tab(&Pane::Scene).is_some());
        Self {
            dock: dock.unwrap_or_else(default_dock),
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

/// Like Roblox Studio: the Scene in the middle (with the Animation Studio as
/// a tab beside it, like Unity's Animator window), the Explorer with
/// Properties (and the Terrain Studio as a tab beside it) under it down the
/// right, and Output and Assets as tabs under the Scene.
fn default_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Scene, Pane::Animation]);
    dock.set_active_tab((
        egui_dock::SurfaceIndex::main(),
        NodeIndex::root(),
        egui_dock::TabIndex(0),
    ));
    let tree = dock.main_surface_mut();
    let [scene, explorer] = tree.split_right(NodeIndex::root(), 0.78, vec![Pane::Explorer]);
    tree.split_below(explorer, 0.45, vec![Pane::Properties, Pane::Terrain]);
    tree.split_below(scene, 0.78, vec![Pane::Output, Pane::Assets]);
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
            assert!(layout.is_open(pane), "{pane:?}");
        }
    }

    #[test]
    fn closed_panels_open_again_and_the_scene_never_closes() {
        let mut layout = Layout::default();
        for pane in Pane::CLOSABLE {
            layout.toggle(pane);
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
        assert!(reloaded.is_open(Pane::Output));
        assert!(Layout::from_text("not json").is_open(Pane::Assets));
    }
}
