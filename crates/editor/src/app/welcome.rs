//! The welcome page (`Pane::Welcome`): what the documents' place shows
//! while no document tab is open, like Visual Studio's start page. The
//! Axcenity mark as a faint watermark, ways to start (Show Scene, a new
//! script, search, another project), the tabs closed last in this project,
//! the recent projects and the shortcuts worth knowing. `Layout::keep_a_document`
//! decides when it shows; opening any document replaces it.

use std::collections::BTreeMap;
use std::path::Path;

use axcenity_core::ClassName;
use eframe::egui::{self, RichText, vec2};
use serde::{Deserialize, Serialize};

use super::{Discard, EditorApp};
use crate::explorer::Action;
use crate::layout::Pane;
use crate::shortcuts::Action as Shortcut;
use crate::start::Brand;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// How many closed tabs each project remembers, and how many projects.
const TABS_PER_PROJECT: usize = 8;
const PROJECTS_REMEMBERED: usize = 50;
/// How many recent projects the page lists.
const RECENT_PROJECTS: usize = 5;
/// Start and Recent sit side by side from this width.
const TWO_COLUMNS_FROM: f32 = 520.0;
/// The widest the page's content gets.
const MAX_WIDTH: f32 = 860.0;
/// The shortcuts the page lists.
const SHORTCUTS: [Shortcut; 7] = [
    Shortcut::Search,
    Shortcut::QuickOpen,
    Shortcut::Save,
    Shortcut::Play,
    Shortcut::Undo,
    Shortcut::FindInScripts,
    Shortcut::Settings,
];

/// The script tabs closed last, per project folder, newest first. Saved
/// with the editor's settings under [`RecentTabs::KEY`].
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct RecentTabs {
    projects: BTreeMap<String, Vec<String>>,
    /// The project folders, most recently used last, so the oldest go first.
    #[serde(default)]
    order: Vec<String>,
}

impl RecentTabs {
    pub(super) const KEY: &'static str = "recent tabs";

    /// Remembers that the tab of `path` closed in the project at `folder`.
    pub(super) fn closed(&mut self, folder: &Path, path: &str) {
        let key = folder.display().to_string();
        let tabs = self.projects.entry(key.clone()).or_default();
        tabs.retain(|tab| tab != path);
        tabs.insert(0, path.to_owned());
        tabs.truncate(TABS_PER_PROJECT);
        self.order.retain(|known| *known != key);
        self.order.push(key);
        while self.order.len() > PROJECTS_REMEMBERED {
            let oldest = self.order.remove(0);
            self.projects.remove(&oldest);
        }
    }

    /// The tabs closed last in the project at `folder`, newest first.
    pub(super) fn of(&self, folder: &Path) -> &[String] {
        self.projects
            .get(&folder.display().to_string())
            .map_or(&[], Vec::as_slice)
    }

    pub(super) fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Saved text, or nothing remembered if it doesn't read.
    pub(super) fn from_text(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
}

/// What was clicked on the page, done once it's drawn.
enum Picked {
    ShowScene,
    NewScript,
    Search,
    OpenAnother,
    Tab(String),
    Project(std::path::PathBuf),
}

impl EditorApp {
    /// Draws the welcome page.
    pub(super) fn welcome_pane(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let palette = theme::palette(&ctx);
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, palette.panel);
        // The watermark: the mark, large and faint, low in the page.
        if self.brand.is_none() {
            self.brand = Some(Brand::load(&ctx));
        }
        if let Some(brand) = &self.brand {
            let side = (rect.width().min(rect.height()) * 0.42).clamp(96.0, 320.0);
            let mark = egui::Rect::from_center_size(
                rect.center() + vec2(rect.width() * 0.22, rect.height() * 0.12),
                vec2(side, side),
            );
            let tint = palette
                .text
                .gamma_multiply(if palette.dark { 0.07 } else { 0.09 });
            ui.painter_at(rect).image(
                brand.mark().id(),
                mark,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
        }
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let name = self
            .project
            .as_ref()
            .map_or_else(String::new, |project| project.name.clone());
        let tabs: Vec<String> = folder
            .as_deref()
            .map(|folder| {
                self.recent_tabs
                    .of(folder)
                    .iter()
                    .filter(|path| folder.join(path).is_file())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let projects: Vec<(String, std::path::PathBuf)> = self
            .recent
            .iter()
            .filter(|link| Some(&link.folder) != folder.as_ref())
            .take(RECENT_PROJECTS)
            .map(|link| (link.name.clone(), link.folder.clone()))
            .collect();
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("welcome page")
            .auto_shrink(false)
            .show(ui, |ui| {
                let margin = ((ui.available_width() - MAX_WIDTH) / 2.0).max(theme::space::XL);
                let width = ui.available_width() - margin * 2.0;
                ui.add_space(theme::space::XL);
                ui.horizontal_top(|ui| {
                    ui.add_space(margin);
                    ui.vertical(|ui| {
                        ui.set_width(width.max(0.0));
                        ui.label(
                            RichText::new(&name)
                                .size(theme::type_scale::DISPLAY)
                                .family(theme::medium_family())
                                .color(palette.text),
                        );
                        ui.label(
                            RichText::new("Nothing is open. Pick up where you left off, or start something new.")
                                .color(palette.text_weak),
                        );
                        ui.add_space(theme::space::L);
                        // Side by side where there's room, else one above
                        // the other; only as tall as they are.
                        if ui.available_width() >= TWO_COLUMNS_FROM {
                            ui.horizontal_top(|ui| {
                                let left = ui.available_width() * 0.42;
                                ui.vertical(|ui| {
                                    ui.set_width(left);
                                    start_column(ui, &palette, &mut picked);
                                });
                                ui.add_space(theme::space::XL);
                                ui.vertical(|ui| {
                                    recent_column(ui, &palette, &tabs, &projects, &mut picked);
                                });
                            });
                        } else {
                            start_column(ui, &palette, &mut picked);
                            ui.add_space(theme::space::L);
                            recent_column(ui, &palette, &tabs, &projects, &mut picked);
                        }
                        ui.add_space(theme::space::L);
                        widgets::section_label(ui, "Shortcuts");
                        ui.add_space(theme::space::XS);
                        shortcut_grid(ui, &ctx, &palette);
                    });
                });
                ui.add_space(theme::space::XL);
            });
        match picked {
            Some(Picked::ShowScene) => self.layout.open(Pane::Scene),
            Some(Picked::NewScript) => self.act(Action::InsertScript(ClassName::Script)),
            Some(Picked::Search) => self.palette.toggle(),
            Some(Picked::OpenAnother) => self.request_and_go_ahead(&ctx, Discard::OpenAnother),
            Some(Picked::Tab(path)) => self.open_script(&path, None),
            Some(Picked::Project(folder)) => {
                self.request_and_go_ahead(&ctx, Discard::OpenSample(folder));
            }
            None => {}
        }
    }
}

/// Start: the ways in.
fn start_column(ui: &mut egui::Ui, palette: &theme::Palette, picked: &mut Option<Picked>) {
    widgets::section_label(ui, "Start");
    ui.add_space(theme::space::XS);
    let entries = [
        (
            icons::CUBE,
            "Show the Scene View",
            "Back to the world and its tools",
            Picked::ShowScene,
        ),
        (
            icons::FILE_CODE,
            "New script",
            "A Script in ServerScriptService",
            Picked::NewScript,
        ),
        (
            icons::MAGNIFYING_GLASS,
            "Search everything",
            "Objects, scripts, assets and commands",
            Picked::Search,
        ),
        (
            icons::FOLDER_OPEN,
            "Open another project…",
            "Asks about unsaved changes first",
            Picked::OpenAnother,
        ),
    ];
    for (icon, label, about, choice) in entries {
        if ui
            .add(widgets::button(Role::Ghost, Some(icon), label, palette))
            .on_hover_text(about)
            .clicked()
        {
            *picked = Some(choice);
        }
    }
}

/// Recent: the tabs closed last here, then the other projects opened last.
fn recent_column(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    tabs: &[String],
    projects: &[(String, std::path::PathBuf)],
    picked: &mut Option<Picked>,
) {
    widgets::section_label(ui, "Recent tabs");
    ui.add_space(theme::space::XS);
    if tabs.is_empty() {
        ui.label(RichText::new("Scripts you close show here.").color(palette.text_weak));
    }
    for path in tabs {
        let name = path.rsplit('/').next().unwrap_or(path);
        if ui
            .add(widgets::button(
                Role::Ghost,
                Some(icons::CODE),
                name,
                palette,
            ))
            .on_hover_text(path.as_str())
            .clicked()
        {
            *picked = Some(Picked::Tab(path.clone()));
        }
    }
    ui.add_space(theme::space::M);
    widgets::section_label(ui, "Recent projects");
    ui.add_space(theme::space::XS);
    if projects.is_empty() {
        ui.label(RichText::new("Other projects you open show here.").color(palette.text_weak));
    }
    for (name, folder) in projects {
        if ui
            .add(widgets::button(
                Role::Ghost,
                Some(icons::FOLDER),
                name,
                palette,
            ))
            .on_hover_text(folder.display().to_string())
            .clicked()
        {
            *picked = Some(Picked::Project(folder.clone()));
        }
    }
}

/// The shortcuts, two to a row where there's room.
fn shortcut_grid(ui: &mut egui::Ui, ctx: &egui::Context, palette: &theme::Palette) {
    let columns = if ui.available_width() >= 520.0 { 2 } else { 1 };
    egui::Grid::new("welcome shortcuts")
        .num_columns(columns * 2)
        .spacing(vec2(theme::space::L, theme::space::XS))
        .show(ui, |ui| {
            for (index, shortcut) in SHORTCUTS.iter().enumerate() {
                ui.label(RichText::new(shortcut.label()).color(palette.text_secondary));
                ui.label(
                    RichText::new(shortcut.menu_text(ctx))
                        .family(theme::medium_family())
                        .color(palette.text),
                );
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}
