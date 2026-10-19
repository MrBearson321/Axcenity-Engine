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
}

impl Pane {
    /// The panels that can be closed and opened again, in menu order.
    pub const CLOSABLE: [Pane; 4] = [Pane::Explorer, Pane::Properties, Pane::Output, Pane::Assets];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Scene => "Scene",
            Pane::Explorer => "Explorer",
            Pane::Properties => "Properties",
            Pane::Output => "Output",
            Pane::Assets => "Assets",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Pane::Scene => icons::CUBE,
            Pane::Explorer => icons::TREE_STRUCTURE,
            Pane::Properties => icons::SLIDERS_HORIZONTAL,
            Pane::Output => icons::TERMINAL_WINDOW,
            Pane::Assets => icons::SHAPES,
        }
    }

    /// The title shown on the panel's tab.
    pub fn title(self) -> String {
        theme::with_icon(self.icon(), self.label())
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
            (Pane::Explorer | Pane::Properties, None) => {
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

/// Like Roblox Studio: the Scene in the middle, the Explorer with
/// Properties under it down the right, and Output and Assets as tabs under
/// the Scene.
fn default_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Scene]);
    let tree = dock.main_surface_mut();
    let [scene, explorer] = tree.split_right(NodeIndex::root(), 0.78, vec![Pane::Explorer]);
    tree.split_below(explorer, 0.45, vec![Pane::Properties]);
    tree.split_below(scene, 0.78, vec![Pane::Output, Pane::Assets]);
    dock
}

/// The docking area's look, taken from the editor's theme so tabs match the
/// rest of the window.
pub fn style(style: &egui::Style) -> egui_dock::Style {
    let mut dock = egui_dock::Style::from_egui(style);
    dock.tab_bar.height = 26.0;
    dock.tab.tab_body.inner_margin = egui::Margin::same(6);
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
