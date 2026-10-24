//! The tabs above the ribbon, like Roblox Studio's Home, Model and Test:
//! each tab shows some of the ribbon's groups. Besides the built-in tabs,
//! people can make their own ("custom tabs") from any groups, in any
//! order. Custom tabs are saved between runs, like the theme.
//!
//! This module only knows which groups a tab has; the app draws the groups
//! themselves (`EditorApp::ribbon_group`).

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons, type_scale};
use crate::widgets;

/// One captioned group of ribbon buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RibbonGroup {
    /// Play and Stop.
    Game,
    /// Select, Move, Scale and Rotate.
    Tools,
    /// Paint, Material, Anchor, Collision, Lighting, Emission and Angular.
    Build,
    Snapping,
    /// Block, Ball, Cylinder and Wedge.
    Parts,
    /// Sounds, particle emitters, lights, and a third-person character.
    Effects,
    /// Import a model.
    Import,
    /// ScreenGui, Frame, TextLabel and the other UI elements.
    Ui,
    /// Script, LocalScript and ModuleScript.
    Scripts,
    /// The Terrain Studio and its main tools.
    Terrain,
    /// Open and close the dockable panels, and reset the layout.
    Panels,
    /// Scan the open project for missing and broken assets.
    Check,
    /// Collider, raycast, bounds and emitter overlays.
    Overlays,
}

impl RibbonGroup {
    pub const ALL: [RibbonGroup; 13] = [
        RibbonGroup::Game,
        RibbonGroup::Tools,
        RibbonGroup::Build,
        RibbonGroup::Snapping,
        RibbonGroup::Parts,
        RibbonGroup::Effects,
        RibbonGroup::Import,
        RibbonGroup::Ui,
        RibbonGroup::Scripts,
        RibbonGroup::Terrain,
        RibbonGroup::Panels,
        RibbonGroup::Check,
        RibbonGroup::Overlays,
    ];

    /// The caption under the group.
    pub fn label(self) -> &'static str {
        match self {
            RibbonGroup::Game => "Game",
            RibbonGroup::Tools => "Tools",
            RibbonGroup::Build => "Build",
            RibbonGroup::Snapping => "Snapping",
            RibbonGroup::Parts => "Parts",
            RibbonGroup::Effects => "Effects",
            RibbonGroup::Import => "Import",
            RibbonGroup::Ui => "UI",
            RibbonGroup::Scripts => "Scripts",
            RibbonGroup::Terrain => "Terrain",
            RibbonGroup::Panels => "Panels",
            RibbonGroup::Check => "Check",
            RibbonGroup::Overlays => "Overlays",
        }
    }

    fn description(self) -> &'static str {
        match self {
            RibbonGroup::Game => "Play and Stop",
            RibbonGroup::Tools => "Select, Move, Scale and Rotate",
            RibbonGroup::Build => {
                "Paint, materials, anchor, collision, lights and the angular tool"
            }
            RibbonGroup::Snapping => "Move and rotate steps",
            RibbonGroup::Parts => "Insert blocks, balls, cylinders and wedges",
            RibbonGroup::Effects => {
                "Insert sounds, particle emitters, lights, and a third-person character"
            }
            RibbonGroup::Import => "Import a 3D model",
            RibbonGroup::Ui => "Insert screens, frames, text, buttons and other UI",
            RibbonGroup::Scripts => "Insert scripts",
            RibbonGroup::Terrain => "Sculpt, paint and generate terrain",
            RibbonGroup::Panels => "Show or hide the Explorer, Properties, Console and Assets",
            RibbonGroup::Check => "Scan the project for missing and broken assets",
            RibbonGroup::Overlays => "Collider, raycast, bounds and audio-emitter overlays",
        }
    }
}

/// The tabs every editor has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltInTab {
    Home,
    Model,
    Terrain,
    Ui,
    Script,
    Test,
    View,
}

impl BuiltInTab {
    pub const ALL: [BuiltInTab; 7] = [
        BuiltInTab::Home,
        BuiltInTab::Model,
        BuiltInTab::Terrain,
        BuiltInTab::Ui,
        BuiltInTab::Script,
        BuiltInTab::Test,
        BuiltInTab::View,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BuiltInTab::Home => "Home",
            BuiltInTab::Model => "Model",
            BuiltInTab::Terrain => "Terrain",
            BuiltInTab::Ui => "UI",
            BuiltInTab::Script => "Script",
            BuiltInTab::Test => "Test",
            BuiltInTab::View => "View",
        }
    }

    /// The icon in front of its name on the tab.
    pub fn icon(self) -> &'static str {
        match self {
            BuiltInTab::Home => icons::HOUSE,
            BuiltInTab::Model => icons::CUBE,
            BuiltInTab::Terrain => icons::MOUNTAINS,
            BuiltInTab::Ui => icons::LAYOUT,
            BuiltInTab::Script => icons::CODE,
            BuiltInTab::Test => icons::FLASK,
            BuiltInTab::View => icons::EYE,
        }
    }

    pub fn groups(self) -> &'static [RibbonGroup] {
        use RibbonGroup::*;
        match self {
            BuiltInTab::Home => &[Game, Tools, Snapping, Parts],
            BuiltInTab::Model => &[Build, Effects, Import],
            BuiltInTab::Terrain => &[Terrain],
            BuiltInTab::Ui => &[Ui],
            BuiltInTab::Script => &[Game, Scripts],
            BuiltInTab::Test => &[Game, Check],
            BuiltInTab::View => &[Panels, Overlays],
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

    /// Draws the row of tabs, centred: the built-in ones, the custom ones
    /// (right-click to change or delete), and + to make a new one. The
    /// active tab is filled like the ribbon under it and underlined in the
    /// accent colour, and the line slides to a newly picked tab. Returns
    /// where the active tab is, so the ribbon can join it.
    pub fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<egui::Rect> {
        let mut active_rect = None;
        // A place in the paint order behind the tabs, filled in once the
        // active tab's place is known.
        let background = ui.painter().add(egui::Shape::Noop);
        // A row is only as wide as what's in it once it's drawn, so it's
        // centred by last frame's width, and drawn again if that changed.
        let width_id = ui.id().with("ribbon tab row width");
        let last_width: f32 = ui.ctx().data(|data| data.get_temp(width_id)).unwrap_or(0.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(((ui.available_width() - last_width) / 2.0).max(0.0));
            let start = ui.cursor().left();
            for tab in BuiltInTab::ALL {
                let active = ActiveTab::BuiltIn(tab);
                let label = theme::with_icon(tab.icon(), tab.label());
                let response = tab_button(ui, &label, self.active == active);
                if self.active == active {
                    active_rect = Some(response.rect);
                }
                if response.clicked() {
                    self.active = active;
                }
            }
            if !self.custom.is_empty() {
                ui.add(egui::Separator::default().vertical().spacing(8.0));
            }
            let mut remove = None;
            for index in 0..self.custom.len() {
                let active = ActiveTab::Custom(index);
                let label = theme::with_icon(icons::STAR, &self.custom[index].name);
                let response = tab_button(ui, &label, self.active == active)
                    .on_hover_text("Your tab. Right-click to change or delete it.");
                if self.active == active {
                    active_rect = Some(response.rect);
                }
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
            ui.add_space(2.0);
            if widgets::icon_button(
                ui,
                icons::PLUS,
                "Make your own tab from any of the ribbon's groups.",
                false,
            )
            .clicked()
            {
                self.edit(None);
            }
            let width = ui.min_rect().right() - start;
            if (width - last_width).abs() > 0.5 {
                ui.ctx().data_mut(|data| data.insert_temp(width_id, width));
                ui.ctx().request_repaint();
            }
        });
        if let Some(rect) = active_rect {
            let palette = theme::palette(ui.ctx());
            let top_corners = egui::CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            };
            ui.painter().set(
                background,
                egui::epaint::RectShape::new(
                    rect,
                    top_corners,
                    palette.panel,
                    egui::Stroke::new(1.0_f32, palette.border),
                    egui::StrokeKind::Inside,
                ),
            );
        }
        // The accent line under the active tab, sliding from where it was.
        if let Some(rect) = active_rect {
            let ctx = ui.ctx();
            let time = ctx.style().animation_time * 1.4;
            let id = egui::Id::new("ribbon tab underline");
            let left = ctx.animate_value_with_time(id.with("left"), rect.left() + 8.0, time);
            let right = ctx.animate_value_with_time(id.with("right"), rect.right() - 8.0, time);
            let palette = theme::palette(ctx);
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, rect.bottom() - 2.0),
                    egui::pos2(right, rect.bottom()),
                ),
                1,
                palette.accent,
            );
        }
        active_rect
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

/// One ribbon tab: its name, brighter when hovered or active.
fn tab_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(type_scale::BODY, theme::medium_family()),
        egui::Color32::PLACEHOLDER,
    );
    let size = egui::vec2(galley.size().x + 16.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, label));
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    let color = if active {
        palette.text_strong
    } else {
        theme::mix(palette.text_weak, palette.text_strong, hover)
    };
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            5,
            egui::Stroke::new(1.0_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().galley(
        rect.center() - galley.size() / 2.0 - egui::vec2(0.0, 1.0),
        galley,
        color,
    );
    response
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
    fn home_is_only_the_everyday_groups() {
        assert_eq!(
            BuiltInTab::Home.groups(),
            &[
                RibbonGroup::Game,
                RibbonGroup::Tools,
                RibbonGroup::Snapping,
                RibbonGroup::Parts,
            ]
        );
    }

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
