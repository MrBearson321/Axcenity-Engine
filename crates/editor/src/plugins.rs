//! The editor's Lua plugins (Phase 54): which are found and running, their
//! dock tabs' UI state, what's saved for them in the editor's data, and the
//! Plugins panel. Running them, undo and the ribbon are `app/plugins.rs`;
//! the plugin API is the `plugins` crate (`docs/editor/plugins.md`).
//!
//! The panel is also the creator toolkit: New Plugin… scaffolds a folder,
//! Export packs a `.axcenity-plugin` for sharing, and Import installs one
//! (the Toolbox Packs menu offers the same import).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use axcenity_core::InstanceId;
use axcenity_plugins::{DockWidget, Found, Origin, Plugin, Settings};
use axcenity_project::Watcher;
use axcenity_ui::{Stylesheet, UiInteraction};
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};
use crate::widgets;

/// The file in the editor's data folder that keeps which plugins are off
/// and every plugin's settings.
const SAVED_FILE: &str = "Plugins.json";
/// The folder in the editor's data folder for the user's own plugins,
/// which every project gets.
const USER_FOLDER: &str = "Plugins";

/// The editor's data folder for plugins: the user's plugins and the saved
/// file, beside the editor's settings.
pub fn default_folder() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID)
}

/// What's kept between runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Saved {
    /// Plugins turned off in the Plugins panel, by [`Found::key`].
    #[serde(default)]
    disabled: BTreeSet<String>,
    /// Each plugin's `plugin:SetSetting` values, by [`Found::key`].
    #[serde(default)]
    settings: BTreeMap<String, Settings>,
}

/// One plugin folder.
pub struct Entry {
    pub found: Found,
    /// Running, while it's on and started without a problem.
    pub running: Option<Plugin>,
    /// Its errors since it last (re)started, newest last.
    pub errors: Vec<String>,
    /// Its `.css` files, which style its dock tabs.
    pub stylesheets: Vec<Stylesheet>,
    /// Its dock tabs.
    pub widgets: Vec<WidgetView>,
    /// What it draws over the Scene View (`plugin:DrawLine` and the rest),
    /// as of its last call.
    pub drawings: Vec<axcenity_plugins::Drawing>,
    /// Its handles in the Scene View (`plugin:CreateHandle`), as of its
    /// last call.
    pub handles: Vec<axcenity_plugins::Handle>,
}

/// How many of a plugin's errors the panel keeps.
const MAX_ERRORS: usize = 20;

impl Entry {
    pub fn error(&mut self, text: String) {
        self.errors.push(text);
        if self.errors.len() > MAX_ERRORS {
            self.errors.remove(0);
        }
    }

    /// How its files are named in messages: `plugins/align` in the
    /// project, `Plugins/align` in the user's folder.
    pub fn shown(&self) -> String {
        let folder = self
            .found
            .folder
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        match self.found.origin {
            Origin::Project => format!("{}/{folder}", axcenity_plugins::PLUGINS_FOLDER),
            Origin::User => format!("{USER_FOLDER}/{folder}"),
        }
    }
}

/// A plugin's dock tab, shown as `Pane::Plugin(pane)`.
pub struct WidgetView {
    pub pane: u64,
    pub widget: DockWidget,
    pub ui: UiInteraction,
    /// The TextButton the left button went down on.
    pub pressed: Option<InstanceId>,
    /// Where the GPU draws it.
    pub canvas: crate::animation::preview::Canvas,
}

/// Every plugin, and what the app keeps for them.
#[derive(Default)]
pub struct Plugins {
    pub entries: Vec<Entry>,
    saved: Saved,
    /// The editor's data folder (see [`default_folder`]); `None` keeps
    /// nothing (tests).
    data: Option<PathBuf>,
    /// Watches the project's `plugins/` and the user's `Plugins` folder.
    pub watchers: Vec<(Origin, Watcher)>,
    /// The undo step plugins' edits merge into until a waypoint.
    pub open_step: Option<u64>,
    /// The selection plugins were last told about.
    pub told_selection: Vec<InstanceId>,
    /// Dock tab numbers, by plugin key and widget id, kept for the session
    /// so a reloaded plugin's tab stays where it was.
    pane_ids: BTreeMap<(String, String), u64>,
    /// The New Plugin… form, while it's open.
    new_plugin: Option<NewPlugin>,
    /// A status line under the toolbar (scaffold / import / export).
    pub notice: Option<String>,
}

/// What's typed in the New Plugin… form.
struct NewPlugin {
    name: String,
    /// Into the project's `plugins/` when a project is open; otherwise the
    /// user's `Plugins` folder.
    into_project: bool,
    error: Option<String>,
}

impl Plugins {
    /// Plugins whose saved state is in `data` (the editor's data folder).
    pub fn new(data: Option<PathBuf>) -> Self {
        let saved = data
            .as_ref()
            .and_then(|folder| std::fs::read_to_string(folder.join(SAVED_FILE)).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            saved,
            data,
            ..Self::default()
        }
    }

    /// Where the user's own plugins are.
    pub fn user_folder(&self) -> Option<PathBuf> {
        self.data.as_ref().map(|data| data.join(USER_FOLDER))
    }

    pub fn is_enabled(&self, key: &str) -> bool {
        !self.saved.disabled.contains(key)
    }

    pub fn set_enabled(&mut self, key: &str, enabled: bool) {
        if enabled {
            self.saved.disabled.remove(key);
        } else {
            self.saved.disabled.insert(key.to_owned());
        }
        self.write();
    }

    pub fn settings(&self, key: &str) -> Settings {
        self.saved.settings.get(key).cloned().unwrap_or_default()
    }

    pub fn set_settings(&mut self, key: &str, settings: Settings) {
        self.saved.settings.insert(key.to_owned(), settings);
        self.write();
    }

    fn write(&self) {
        let Some(folder) = &self.data else {
            return;
        };
        let text = serde_json::to_string_pretty(&self.saved).unwrap_or_default();
        if let Err(err) = std::fs::create_dir_all(folder)
            .and_then(|()| std::fs::write(folder.join(SAVED_FILE), text))
        {
            log::warn!("could not save the plugins' settings: {err}");
        }
    }

    /// The dock tab number for `widget` of the plugin `key`.
    pub fn pane_id(&mut self, key: &str, widget: &str) -> u64 {
        let next = self.pane_ids.len() as u64 + 1;
        *self
            .pane_ids
            .entry((key.to_owned(), widget.to_owned()))
            .or_insert(next)
    }

    /// The dock tab shown as `Pane::Plugin(pane)`: its plugin's index and
    /// the tab's index in it.
    pub fn find_pane(&self, pane: u64) -> Option<(usize, usize)> {
        self.entries.iter().enumerate().find_map(|(index, entry)| {
            entry
                .widgets
                .iter()
                .position(|view| view.pane == pane)
                .map(|widget| (index, widget))
        })
    }
}

/// What the Plugins panel asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum PluginAction {
    /// Turn the plugin at this index on or off.
    Enable(usize, bool),
    /// Start the plugin at this index again.
    Reload(usize),
    /// Look for plugins again, in both folders.
    Rescan,
    /// Scaffold a new plugin folder.
    Create { name: String, origin: Origin },
    /// Install a `.axcenity-plugin` into the project or user plugins folder.
    Install { archive: PathBuf, origin: Origin },
    /// Pack the plugin at this index into the given file.
    Export { index: usize, file: PathBuf },
}

/// Draws the Plugins panel: each plugin, whether it's on, and its errors.
pub fn show(
    ui: &mut egui::Ui,
    plugins: &mut Plugins,
    project: Option<&Path>,
) -> Option<PluginAction> {
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    // Wraps in a narrow column rather than running past it.
    crate::responsive::toolbar(ui, |ui| {
        if widgets::icon_button(ui, icons::ARROWS_CLOCKWISE, "Look for plugins again", false)
            .clicked()
        {
            action = Some(PluginAction::Rescan);
        }
        if widgets::icon_button(ui, icons::PLUS, "New Plugin…", false)
            .on_hover_text("Scaffold a plugin folder with plugin.json and init.lua")
            .clicked()
        {
            plugins.new_plugin = Some(NewPlugin {
                name: String::new(),
                into_project: project.is_some(),
                error: None,
            });
        }
        if widgets::icon_button(ui, icons::DOWNLOAD_SIMPLE, "Import plugin…", false)
            .on_hover_text("Install a .axcenity-plugin shared by another creator")
            .clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter("Axcenity plugin", &["axcenity-plugin"])
                .pick_file()
        {
            let origin = if project.is_some() {
                Origin::Project
            } else {
                Origin::User
            };
            action = Some(PluginAction::Install {
                archive: file,
                origin,
            });
        }
        ui.label(RichText::new("Lua plugins for the editor").color(palette.text_weak));
    });
    if let Some(notice) = &plugins.notice {
        ui.label(RichText::new(notice).small().color(palette.success));
    }
    widgets::divider(ui);

    if let Some(form) = &mut plugins.new_plugin {
        let mut create = false;
        let mut cancel = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong("New Plugin");
            ui.label(
                RichText::new("A folder with plugin.json and a starter init.lua.")
                    .small()
                    .color(palette.text_weak),
            );
            ui.horizontal(|ui| {
                ui.label("Name");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut form.name)
                        .desired_width(180.0)
                        .hint_text("Align to Grid"),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(project.is_some(), |ui| {
                    ui.radio_value(&mut form.into_project, true, "This project");
                });
                ui.radio_value(&mut form.into_project, false, "Your plugins");
            });
            if let Some(error) = &form.error {
                ui.label(RichText::new(error).small().color(palette.error));
            }
            ui.horizontal(|ui| {
                if ui
                    .add(widgets::button(
                        widgets::Role::Primary,
                        None,
                        "Create",
                        &palette,
                    ))
                    .clicked()
                {
                    create = true;
                }
                if ui
                    .add(widgets::button(
                        widgets::Role::Ghost,
                        None,
                        "Cancel",
                        &palette,
                    ))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
        if cancel {
            plugins.new_plugin = None;
        } else if create {
            let name = form.name.trim().to_owned();
            if name.is_empty() {
                form.error = Some("Give the plugin a name.".to_owned());
            } else if form.into_project && project.is_none() {
                form.error = Some("Open a project to put the plugin there.".to_owned());
            } else {
                let origin = if form.into_project {
                    Origin::Project
                } else {
                    Origin::User
                };
                plugins.new_plugin = None;
                action = Some(PluginAction::Create { name, origin });
            }
        }
        widgets::divider(ui);
    }

    if plugins.entries.is_empty() {
        ui.add_space(12.0);
        let mut places = Vec::new();
        if let Some(project) = project {
            places.push(
                project
                    .join(axcenity_plugins::PLUGINS_FOLDER)
                    .display()
                    .to_string(),
            );
        }
        if let Some(user) = plugins.user_folder() {
            places.push(user.display().to_string());
        }
        widgets::empty_state(
            ui,
            icons::PUZZLE_PIECE,
            "No plugins yet",
            &format!(
                "Create one with New Plugin…, import a .axcenity-plugin, or add a folder with plugin.json and init.lua in {}.",
                places.join(" or ")
            ),
            None,
        );
        return action;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            for (index, entry) in plugins.entries.iter().enumerate() {
                ui.push_id(&entry.found.key, |ui| {
                    let mut enabled = plugins.is_enabled(&entry.found.key);
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut enabled, "")
                            .on_hover_text("Run this plugin")
                            .changed()
                        {
                            action = Some(PluginAction::Enable(index, enabled));
                        }
                        ui.label(RichText::new(entry.found.name()).strong());
                        if let Ok(manifest) = &entry.found.manifest {
                            ui.label(
                                RichText::new(format!("v{}", manifest.version))
                                    .color(palette.text_weak),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(
                                ui,
                                icons::EXPORT,
                                "Export .axcenity-plugin…",
                                false,
                            )
                            .on_hover_text("Pack this plugin to share through the Toolbox")
                            .clicked()
                            {
                                let stem = entry
                                    .found
                                    .folder
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "plugin".to_owned());
                                if let Some(file) = rfd::FileDialog::new()
                                    .set_file_name(format!(
                                        "{stem}{}",
                                        axcenity_plugins::PLUGIN_PACKAGE_SUFFIX
                                    ))
                                    .add_filter("Axcenity plugin", &["axcenity-plugin"])
                                    .save_file()
                                {
                                    action = Some(PluginAction::Export { index, file });
                                }
                            }
                            if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Reload", false)
                                .clicked()
                            {
                                action = Some(PluginAction::Reload(index));
                            }
                        });
                    });
                    let origin = match entry.found.origin {
                        Origin::Project => "This project",
                        Origin::User => "Your plugins",
                    };
                    let mut about = format!("{origin} · {}", entry.shown());
                    if let Ok(manifest) = &entry.found.manifest
                        && manifest.allows(axcenity_plugins::Permission::Files)
                    {
                        about.push_str(" · project files");
                    }
                    ui.label(RichText::new(about).small().color(palette.text_weak));
                    let status = match (&entry.running, enabled) {
                        (Some(_), _) => None,
                        (None, false) => Some(("Off", palette.text_weak)),
                        (None, true) => Some(("Not running", palette.warning)),
                    };
                    if let Some((text, color)) = status {
                        ui.label(RichText::new(text).small().color(color));
                    }
                    for error in &entry.errors {
                        ui.label(
                            RichText::new(theme::with_icon(icons::WARNING_CIRCLE, error))
                                .small()
                                .color(palette.error),
                        );
                    }
                    widgets::divider(ui);
                });
            }
        });
    action
}
