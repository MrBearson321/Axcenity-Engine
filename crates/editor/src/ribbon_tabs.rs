//! The tabs above the ribbon, like Roblox Studio's Home, Model and Test:
//! each tab shows some of the ribbon's groups. Besides the built-in tabs,
//! people can make their own ("custom tabs") from any groups, in any
//! order. Custom tabs are saved between runs, like the theme.
//!
//! This module only knows which groups a tab has; the app draws the groups
//! themselves (`EditorApp::ribbon_group`).

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};

/// One captioned group of ribbon buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RibbonGroup {
    /// Play and Stop.
    Game,
    /// Select, Move, Scale, Rotate and the building tools.
    Tools,
    Snapping,
    /// Block, Ball, Cylinder and Wedge.
    Parts,
    /// Sounds, particle emitters and lights.
    Effects,
    /// Import a model.
    Import,
    /// ScreenGui, Frame, TextLabel and the other UI elements.
    Ui,
    /// Script, LocalScript and ModuleScript.
    Scripts,
    /// Open and close the dockable panels, and reset the layout.
    Panels,
}

impl RibbonGroup {
    pub const ALL: [RibbonGroup; 9] = [
        RibbonGroup::Game,
        RibbonGroup::Tools,
        RibbonGroup::Snapping,
        RibbonGroup::Parts,
        RibbonGroup::Effects,
        RibbonGroup::Import,
        RibbonGroup::Ui,
        RibbonGroup::Scripts,
        RibbonGroup::Panels,
    ];

    /// The caption under the group.
    pub fn label(self) -> &'static str {
        match self {
            RibbonGroup::Game => "Game",
            RibbonGroup::Tools => "Tools",
            RibbonGroup::Snapping => "Snapping",
            RibbonGroup::Parts => "Parts",
            RibbonGroup::Effects => "Effects",
            RibbonGroup::Import => "Import",
            RibbonGroup::Ui => "UI",
            RibbonGroup::Scripts => "Scripts",
            RibbonGroup::Panels => "Panels",
        }
    }

    fn description(self) -> &'static str {
        match self {
            RibbonGroup::Game => "Play and Stop",
            RibbonGroup::Tools => "Select, Move, Scale, Rotate and the building tools",
            RibbonGroup::Snapping => "Move and rotate steps",
            RibbonGroup::Parts => "Insert blocks, balls, cylinders and wedges",
            RibbonGroup::Effects => "Insert sounds, particle emitters and lights",
            RibbonGroup::Import => "Import a 3D model",
            RibbonGroup::Ui => "Insert screens, frames, text, buttons and other UI",
            RibbonGroup::Scripts => "Insert scripts",
            RibbonGroup::Panels => "Show or hide the Explorer, Properties, Output and Assets",
        }
    }
}

/// The tabs every editor has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltInTab {
    Home,
    Model,
    Ui,
    Script,
    Test,
    View,
}

impl BuiltInTab {
    pub const ALL: [BuiltInTab; 6] = [
        BuiltInTab::Home,
        BuiltInTab::Model,
        BuiltInTab::Ui,
        BuiltInTab::Script,
        BuiltInTab::Test,
        BuiltInTab::View,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BuiltInTab::Home => "Home",
            BuiltInTab::Model => "Model",
            BuiltInTab::Ui => "UI",
            BuiltInTab::Script => "Script",
            BuiltInTab::Test => "Test",
            BuiltInTab::View => "View",
        }
    }

    pub fn groups(self) -> &'static [RibbonGroup] {
        use RibbonGroup::*;
        match self {
            BuiltInTab::Home => &[Game, Tools, Snapping, Parts],
            BuiltInTab::Model => &[Tools, Snapping, Parts, Effects, Import],
            BuiltInTab::Ui => &[Ui],
            BuiltInTab::Script => &[Game, Scripts],
            BuiltInTab::Test => &[Game],
            BuiltInTab::View => &[Panels],
        }
    }
}

/// A tab someone made: a name and the groups it shows, in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomTab {
    pub name: String,
    pub groups: Vec<RibbonGroup>,
}

/// Which tab is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveTab {
    BuiltIn(BuiltInTab),
    /// An index into [`RibbonTabs::custom`].
    Custom(usize),
}

/// What's saved between runs.
#[derive(Serialize, Deserialize)]
struct Saved {
    active: ActiveTab,
    custom: Vec<CustomTab>,
}

/// The ribbon's tabs, the custom ones, and the window that makes and
/// changes custom tabs.
pub struct RibbonTabs {
    active: ActiveTab,
    pub custom: Vec<CustomTab>,
    editing: Option<TabEditor>,
}

/// A custom tab being made (`index` `None`) or changed.
struct TabEditor {
    index: Option<usize>,
    name: String,
    groups: Vec<RibbonGroup>,
}

impl Default for RibbonTabs {
    fn default() -> Self {
        Self {
            active: ActiveTab::BuiltIn(BuiltInTab::Home),
            custom: Vec::new(),
            editing: None,
        }
    }
}

impl RibbonTabs {
    pub fn set_active(&mut self, tab: ActiveTab) {
        if let ActiveTab::Custom(index) = tab
            && index >= self.custom.len()
        {
            return;
        }
        self.active = tab;
    }

    /// The groups the ribbon shows now, in order.
    pub fn groups(&self) -> Vec<RibbonGroup> {
        match self.active {
            ActiveTab::BuiltIn(tab) => tab.groups().to_vec(),
            ActiveTab::Custom(index) => self
                .custom
                .get(index)
                .map_or_else(Vec::new, |tab| tab.groups.clone()),
        }
    }

    /// Adds a custom tab and shows it. Groups listed twice are kept once.
    pub fn add_custom(&mut self, name: &str, groups: &[RibbonGroup]) {
        self.custom.push(CustomTab {
            name: name.trim().to_owned(),
            groups: unique(groups),
        });
        self.active = ActiveTab::Custom(self.custom.len() - 1);
    }

    /// Removes a custom tab. If it was showing, Home shows instead.
    pub fn remove_custom(&mut self, index: usize) {
        if index >= self.custom.len() {
            return;
        }
        self.custom.remove(index);
        self.active = match self.active {
            ActiveTab::Custom(active) if active == index => ActiveTab::BuiltIn(BuiltInTab::Home),
            ActiveTab::Custom(active) if active > index => ActiveTab::Custom(active - 1),
            other => other,
        };
    }

    /// Draws the row of tabs: the built-in ones, the custom ones (right-click
    /// to change or delete), and + to make a new one.
    pub fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for tab in BuiltInTab::ALL {
                let active = ActiveTab::BuiltIn(tab);
                if ui
                    .selectable_label(self.active == active, tab.label())
                    .clicked()
                {
                    self.active = active;
                }
            }
            if !self.custom.is_empty() {
                ui.separator();
            }
            let mut remove = None;
            for index in 0..self.custom.len() {
                let active = ActiveTab::Custom(index);
                let label = theme::with_icon(icons::STAR, &self.custom[index].name);
                let response = ui
                    .selectable_label(self.active == active, label)
                    .on_hover_text("Your tab. Right-click to change or delete it.");
                if response.clicked() {
                    self.active = active;
                }
                response.context_menu(|ui| {
                    if ui
                        .button(theme::with_icon(icons::PENCIL_SIMPLE, "Edit Tab…"))
                        .clicked()
                    {
                        self.edit(Some(index));
                        ui.close();
                    }
                    if ui
                        .button(theme::with_icon(icons::TRASH, "Delete Tab"))
                        .clicked()
                    {
                        remove = Some(index);
                        ui.close();
                    }
                });
            }
            if let Some(index) = remove {
                self.remove_custom(index);
            }
            if ui
                .small_button(icons::PLUS)
                .on_hover_text("Make your own tab from any of the ribbon's groups.")
                .clicked()
            {
                self.edit(None);
            }
        });
    }

    /// Opens the window that makes a custom tab (`None`) or changes one.
    pub fn edit(&mut self, index: Option<usize>) {
        let (name, groups) = match index.and_then(|index| self.custom.get(index)) {
            Some(tab) => (tab.name.clone(), tab.groups.clone()),
            None => (format!("My Tab {}", self.custom.len() + 1), Vec::new()),
        };
        self.editing = Some(TabEditor {
            index,
            name,
            groups,
        });
    }

    /// The custom tab window, while it's open.
    pub fn editor_window(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.editing else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let title = if editor.index.is_some() {
            "Edit Tab"
        } else {
            "New Tab"
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut editor.name);
                });
                ui.add_space(6.0);
                ui.strong("In this tab");
                if editor.groups.is_empty() {
                    ui.weak("Nothing yet. Add groups from the list below.");
                }
                let mut action = None;
                let count = editor.groups.len();
                for (index, group) in editor.groups.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(group.label());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                                action = Some(Reorder::Remove(index));
                            }
                            if ui
                                .add_enabled(
                                    index + 1 < count,
                                    egui::Button::new(icons::ARROW_DOWN).small(),
                                )
                                .on_hover_text("Move right in the ribbon")
                                .clicked()
                            {
                                action = Some(Reorder::Swap(index, index + 1));
                            }
                            if ui
                                .add_enabled(index > 0, egui::Button::new(icons::ARROW_UP).small())
                                .on_hover_text("Move left in the ribbon")
                                .clicked()
                            {
                                action = Some(Reorder::Swap(index - 1, index));
                            }
                        });
                    });
                }
                match action {
                    Some(Reorder::Remove(index)) => {
                        editor.groups.remove(index);
                    }
                    Some(Reorder::Swap(a, b)) => editor.groups.swap(a, b),
                    None => {}
                }
                ui.add_space(6.0);
                ui.strong("Add a group");
                for group in RibbonGroup::ALL {
                    if editor.groups.contains(&group) {
                        continue;
                    }
                    if ui
                        .button(theme::with_icon(icons::PLUS, group.label()))
                        .on_hover_text(group.description())
                        .clicked()
                    {
                        editor.groups.push(group);
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = !editor.name.trim().is_empty() && !editor.groups.is_empty();
                    save = ui
                        .add_enabled(ready, egui::Button::new("Save"))
                        .on_disabled_hover_text("Give the tab a name and at least one group.")
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if save {
            let editor = self.editing.take().expect("checked above");
            match editor.index.filter(|&index| index < self.custom.len()) {
                Some(index) => {
                    self.custom[index] = CustomTab {
                        name: editor.name.trim().to_owned(),
                        groups: unique(&editor.groups),
                    };
                    self.active = ActiveTab::Custom(index);
                }
                None => self.add_custom(&editor.name, &editor.groups),
            }
        } else if cancel || !open {
            self.editing = None;
        }
    }

    /// The custom tabs and the tab showing, as text for eframe's storage.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&Saved {
            active: self.active,
            custom: self.custom.clone(),
        })
        .unwrap_or_default()
    }

    /// Tabs saved by [`to_text`](Self::to_text). Damaged text gives the
    /// built-in tabs only.
    pub fn from_text(text: &str) -> Self {
        let Ok(saved) = serde_json::from_str::<Saved>(text) else {
            return Self::default();
        };
        let mut tabs = Self {
            custom: saved.custom,
            ..Self::default()
        };
        tabs.set_active(saved.active);
        tabs
    }
}

enum Reorder {
    Remove(usize),
    Swap(usize, usize),
}

/// `groups` with any repeats left out, in order.
fn unique(groups: &[RibbonGroup]) -> Vec<RibbonGroup> {
    let mut kept = Vec::new();
    for &group in groups {
        if !kept.contains(&group) {
            kept.push(group);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_is_on_some_built_in_tab() {
        for group in RibbonGroup::ALL {
            assert!(
                BuiltInTab::ALL
                    .iter()
                    .any(|tab| tab.groups().contains(&group)),
                "{group:?}"
            );
        }
    }

    #[test]
    fn a_custom_tab_shows_its_groups_in_its_order() {
        let mut tabs = RibbonTabs::default();
        assert_eq!(tabs.groups(), BuiltInTab::Home.groups());
        tabs.add_custom(
            " Building ",
            &[RibbonGroup::Parts, RibbonGroup::Tools, RibbonGroup::Parts],
        );
        assert_eq!(tabs.active, ActiveTab::Custom(0));
        assert_eq!(tabs.custom[0].name, "Building");
        assert_eq!(tabs.groups(), [RibbonGroup::Parts, RibbonGroup::Tools]);
    }

    #[test]
    fn deleting_the_showing_custom_tab_goes_back_home() {
        let mut tabs = RibbonTabs::default();
        tabs.add_custom("One", &[RibbonGroup::Game]);
        tabs.add_custom("Two", &[RibbonGroup::Ui]);
        tabs.remove_custom(0);
        assert_eq!(tabs.active, ActiveTab::Custom(0));
        assert_eq!(tabs.groups(), [RibbonGroup::Ui]);
        tabs.remove_custom(0);
        assert_eq!(tabs.active, ActiveTab::BuiltIn(BuiltInTab::Home));
    }

    #[test]
    fn custom_tabs_survive_saving_and_bad_text_is_ignored() {
        let mut tabs = RibbonTabs::default();
        tabs.add_custom("Lights", &[RibbonGroup::Effects, RibbonGroup::Game]);
        let reloaded = RibbonTabs::from_text(&tabs.to_text());
        assert_eq!(reloaded.custom, tabs.custom);
        assert_eq!(reloaded.active, ActiveTab::Custom(0));
        let broken = RibbonTabs::from_text("{");
        assert!(broken.custom.is_empty());
        assert_eq!(broken.active, ActiveTab::BuiltIn(BuiltInTab::Home));
    }
}
