//! Layouts to switch between (design system V17): Default and five presets
//! for common tasks, plus layouts saved by name. A preset is an ordinary
//! layout, built here; a saved one is [`Layout::to_text`]. Switching keeps
//! open scripts and plugin tabs (they come back beside the Scene, as after
//! Reset Layout) and never touches the selection.

use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabIndex};
use serde::{Deserialize, Serialize};

use super::{Layout, Pane, default_dock};
use crate::theme::icons;

/// The layouts that are always there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    /// The starting layout; Reset Layout puts it back.
    Default,
    Scripting,
    Animation,
    UiDesign,
    Modeling,
    Lighting,
}

impl Preset {
    pub const ALL: [Preset; 6] = [
        Preset::Default,
        Preset::Scripting,
        Preset::Animation,
        Preset::UiDesign,
        Preset::Modeling,
        Preset::Lighting,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Default => "Default",
            Preset::Scripting => "Scripting",
            Preset::Animation => "Animation",
            Preset::UiDesign => "UI Design",
            Preset::Modeling => "Modeling & Assets",
            Preset::Lighting => "Lighting & Environment",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Preset::Default => icons::LAYOUT,
            Preset::Scripting => icons::CODE,
            Preset::Animation => icons::FILM_STRIP,
            Preset::UiDesign => icons::FRAME_CORNERS,
            Preset::Modeling => icons::POLYGON,
            Preset::Lighting => icons::SUN,
        }
    }

    /// What it's for, for its tooltip.
    pub fn about(self) -> &'static str {
        match self {
            Preset::Default => {
                "The Scene in the middle, the Explorer and Properties down the right, the \
                 Console along the bottom."
            }
            Preset::Scripting => {
                "Scripts wide beside the Scene, the Console tall next to them, the Explorer \
                 and Properties on the right."
            }
            Preset::Animation => {
                "The Animation tab large, the Scene beside it to preview, Properties and the \
                 Explorer under it."
            }
            Preset::UiDesign => {
                "The UI Designer large, with the Explorer and Properties down the right."
            }
            Preset::Modeling => {
                "The Model Studio over a wide Assets panel, with the Explorer and Properties \
                 on the right."
            }
            Preset::Lighting => {
                "The Scene large, Properties tall on the right with the Terrain Studio beside \
                 it, the Explorer above."
            }
        }
    }

    /// The panels, docked.
    pub fn dock(self) -> DockState<Pane> {
        match self {
            Preset::Default => default_dock(),
            Preset::Scripting => {
                let mut dock = DockState::new(vec![Pane::Scene]);
                let tree = dock.main_surface_mut();
                let [_, console] =
                    tree.split_right(NodeIndex::root(), 0.62, vec![Pane::Console, Pane::Assets]);
                let [_, explorer] = tree.split_right(console, 0.55, vec![Pane::Explorer]);
                tree.split_below(explorer, 0.45, vec![Pane::Properties]);
                dock
            }
            Preset::Animation => {
                let mut dock = DockState::new(vec![Pane::Animation]);
                let tree = dock.main_surface_mut();
                let [_, scene] = tree.split_right(NodeIndex::root(), 0.62, vec![Pane::Scene]);
                tree.split_below(scene, 0.5, vec![Pane::Properties, Pane::Explorer]);
                dock
            }
            Preset::UiDesign => {
                let mut dock = DockState::new(vec![Pane::UiDesigner, Pane::Scene]);
                let tree = dock.main_surface_mut();
                let [_, explorer] = tree.split_right(NodeIndex::root(), 0.75, vec![Pane::Explorer]);
                tree.split_below(explorer, 0.4, vec![Pane::Properties]);
                dock
            }
            Preset::Modeling => {
                let mut dock = DockState::new(vec![Pane::ModelStudio, Pane::Scene]);
                let tree = dock.main_surface_mut();
                let [main, explorer] =
                    tree.split_right(NodeIndex::root(), 0.78, vec![Pane::Explorer]);
                tree.split_below(explorer, 0.45, vec![Pane::Properties]);
                tree.split_below(main, 0.62, vec![Pane::Assets, Pane::Console]);
                dock
            }
            Preset::Lighting => {
                let mut dock = DockState::new(vec![Pane::Scene]);
                let tree = dock.main_surface_mut();
                let [_, explorer] = tree.split_right(NodeIndex::root(), 0.74, vec![Pane::Explorer]);
                tree.split_below(explorer, 0.3, vec![Pane::Properties, Pane::Terrain]);
                dock
            }
        }
    }

    /// Whether `name` is one of these, which a saved layout can't be called.
    pub fn is_name(name: &str) -> bool {
        Preset::ALL
            .iter()
            .any(|preset| preset.label().eq_ignore_ascii_case(name.trim()))
    }
}

/// Layouts saved by name, in the order they were saved. Kept in eframe's
/// storage as JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedLayouts(pub Vec<(String, String)>);

impl SavedLayouts {
    pub fn from_text(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Saves `layout` as `name`, replacing one already called that. A
    /// blank name or a preset's is refused, with why.
    pub fn save(&mut self, name: &str, layout: &Layout) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give it a name.");
        }
        if Preset::is_name(name) {
            return Err("That's a built-in layout's name.");
        }
        let text = layout.to_text();
        match self.0.iter_mut().find(|(saved, _)| saved == name) {
            Some((_, saved)) => *saved = text,
            None => self.0.push((name.to_owned(), text)),
        }
        Ok(())
    }

    pub fn delete(&mut self, name: &str) {
        self.0.retain(|(saved, _)| saved != name);
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(saved, _)| saved == name)
            .map(|(_, text)| text.as_str())
    }
}

impl Layout {
    /// Switches to `preset`. Open scripts and plugin tabs come back beside
    /// the Scene on the next frame.
    pub fn use_preset(&mut self, preset: Preset) {
        let mut dock = preset.dock();
        dock.set_active_tab((SurfaceIndex::main(), NodeIndex::root(), TabIndex(0)));
        self.replace_dock(dock);
        // Scripting is for the scripts: they come back in front.
        self.switched = preset != Preset::Scripting;
    }

    /// Switches to a layout saved with [`to_text`](Layout::to_text).
    pub fn use_saved(&mut self, text: &str) {
        let saved = Layout::from_text(text);
        self.replace_dock(saved.dock);
        for window in saved.windows {
            self.windows.push(window);
        }
        self.homes = saved.homes;
        self.next_window = self.next_window.max(saved.next_window);
        for window in &mut self.windows {
            self.next_window = self.next_window.max(window.id);
        }
    }

    fn replace_dock(&mut self, dock: DockState<Pane>) {
        self.dock = dock;
        self.windows.clear();
        self.homes.clear();
        self.last_script = None;
        self.switched = true;
        // New ids for the dock areas, so none keeps the old one's state.
        self.dock_generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pane in a dock, in order.
    fn panes(dock: &DockState<Pane>) -> Vec<Pane> {
        dock.iter_all_tabs().map(|(_, pane)| *pane).collect()
    }

    #[test]
    fn every_preset_has_the_scene_and_survives_saving() {
        for preset in Preset::ALL {
            let mut layout = Layout::default();
            layout.use_preset(preset);
            assert!(
                layout.dock.find_main_surface_tab(&Pane::Scene).is_some(),
                "{preset:?} has the Scene"
            );
            let again = Layout::from_text(&layout.to_text());
            assert_eq!(panes(&again.dock), panes(&layout.dock), "{preset:?}");
        }
        let mut layout = Layout::default();
        layout.use_preset(Preset::Default);
        assert_eq!(panes(&layout.dock), panes(&Layout::default().dock));
    }

    #[test]
    fn each_preset_brings_its_task_s_panels() {
        let shows = |preset: Preset, pane: Pane| {
            let mut layout = Layout::default();
            layout.use_preset(preset);
            layout.is_open(pane)
        };
        assert!(shows(Preset::Scripting, Pane::Console));
        assert!(shows(Preset::Animation, Pane::Animation));
        assert!(shows(Preset::UiDesign, Pane::UiDesigner));
        assert!(shows(Preset::Modeling, Pane::ModelStudio));
        assert!(shows(Preset::Modeling, Pane::Assets));
        assert!(shows(Preset::Lighting, Pane::Terrain));
        for preset in Preset::ALL {
            assert!(shows(preset, Pane::Explorer) && shows(preset, Pane::Properties));
        }
    }

    #[test]
    fn open_scripts_come_back_after_switching() {
        let mut layout = Layout::default();
        layout.sync_scripts(&[7], Some(7), Some(7));
        let front = |layout: &mut Layout| layout.dock.find_active_focused().map(|(_, pane)| *pane);
        layout.use_preset(Preset::Lighting);
        assert!(!layout.is_open(Pane::Script(7)));
        // The app keeps the scripts' tabs in step each frame.
        layout.sync_scripts(&[7], None, None);
        assert!(layout.is_open(Pane::Script(7)), "beside the Scene again");
        assert_eq!(front(&mut layout), Some(Pane::Scene), "behind the Scene");
        layout.use_preset(Preset::Scripting);
        layout.sync_scripts(&[7], None, None);
        assert_eq!(
            front(&mut layout),
            Some(Pane::Script(7)),
            "in front for Scripting"
        );
    }

    #[test]
    fn layouts_are_saved_by_name_but_never_as_a_preset() {
        let mut saved = SavedLayouts::default();
        let mut layout = Layout::default();
        layout.use_preset(Preset::Animation);
        assert_eq!(saved.save("  ", &layout), Err("Give it a name."));
        assert!(
            saved.save("default", &layout).is_err(),
            "Default can't be replaced"
        );
        assert!(saved.save("UI Design", &layout).is_err());
        saved.save("Mine", &layout).unwrap();
        layout.use_preset(Preset::Lighting);
        saved.save("Mine", &layout).unwrap();
        assert_eq!(saved.0.len(), 1, "the same name replaces it");

        let again = SavedLayouts::from_text(&saved.to_text());
        assert_eq!(again, saved);
        let mut other = Layout::default();
        other.use_saved(again.get("Mine").unwrap());
        assert!(
            other.is_open(Pane::Terrain),
            "the Lighting one, saved second"
        );
        saved.delete("Mine");
        assert!(saved.0.is_empty());
        assert_eq!(SavedLayouts::from_text("not json"), SavedLayouts::default());
    }
}
