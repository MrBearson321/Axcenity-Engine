//! The editor and the project folder: saving, script files (insert, open,
//! rename, delete), importing models and asset packages, and reacting to
//! files changed by other programs.
//!
//! Each script is its `.server.lua` (Script), `.client.lua` (LocalScript)
//! or `.module.lua` (ModuleScript) file, or for a Script or LocalScript
//! shown as blocks, its `.server.blocks.json` or `.client.blocks.json`
//! file. Making, renaming or deleting a script changes its file straight
//! away and isn't an undo step; only the code typed in a tab waits for Save.
//!
//! These are more methods of [`EditorApp`]: a child module like this one can
//! use its private fields.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use axcenity_assets::Pick;
use axcenity_blocks::{from_lua, translate};
use axcenity_core::{ClassName, InstanceId, Value, Vector3};
use axcenity_project::{self as project, LoadError, Project, ScriptFile};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::{BlockDocument, EditorApp};
use crate::anim_import::AnimationImport;
use crate::animation::store;
use crate::dialog;
use crate::history::{self, Move, Place};
use crate::import_dialog::{Answer, ImportDialog, Placement};
use crate::output::Output;
use crate::properties::MIN_SIZE;
use crate::scripts::{Disk, new_script_source};
use crate::surface::{self, MapFile};
use crate::theme::icons;
use crate::tree::{self, Insertable};

mod export;
mod import;
mod packages;
mod pickers;
mod script_files;
mod watch;

pub(super) use export::ExportTarget;
pub(super) use import::is_package;
pub(in crate::app) use import::{asset_files, files_made_since};

/// An error and its causes on one line, the way `{:#}` shows an `anyhow::Error`.
pub(super) fn chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// Says in Output which files of `project` couldn't be loaded.
pub(super) fn report_problems(output: &mut Output, project: &Project) {
    for problem in &project.problems {
        output.error(format!("Skipped a damaged file: {}", chain(problem)));
    }
    if !project.world_loaded {
        output.info(format!(
            "The Workspace is empty because `{world}` couldn't be loaded; the rest of the \
             project loaded without it. Fix the file (in VS Code, say) and the editor loads it \
             as soon as it's saved. If you save here first, the old file is kept as `{kept}`.",
            world = project::WORLD_FILE,
            kept = project::KEPT_WORLD_FILE,
        ));
    }
}

/// The world file's text, or `None` if it can't be read.
pub(super) fn read_world_text(folder: &Path) -> Option<String> {
    std::fs::read_to_string(folder.join(project::WORLD_FILE)).ok()
}

/// How many of `parent`'s children aren't scripts.
fn non_script_children(world: &axcenity_core::World, parent: InstanceId) -> usize {
    world.get(parent).map_or(0, |parent| {
        parent
            .children()
            .iter()
            .filter(|&&child| world.get(child).is_some_and(|c| !c.class().is_script()))
            .count()
    })
}

impl EditorApp {
    pub(super) fn new_stylesheet(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() {
            return;
        }
        let folder = project.folder.clone();
        for suffix in 1.. {
            let name = if suffix == 1 {
                "Stylesheet".to_owned()
            } else {
                format!("Stylesheet{suffix}")
            };
            match project::create_stylesheet(&folder, &name, project::NEW_STYLESHEET_SOURCE) {
                Ok(path) => {
                    let (stylesheet, warnings) =
                        axcenity_ui::parse_stylesheet(&path, project::NEW_STYLESHEET_SOURCE);
                    if let Some(project) = &mut self.project {
                        project
                            .stylesheet_texts
                            .insert(path.clone(), project::NEW_STYLESHEET_SOURCE.to_owned());
                        let index = project
                            .stylesheets
                            .partition_point(|existing| existing.path() < path.as_str());
                        project.stylesheets.insert(index, stylesheet);
                    }
                    self.tabs.open(&path, project::NEW_STYLESHEET_SOURCE, None);
                    for warning in warnings {
                        self.output.warning(format!(
                            "Stylesheet `{}`: {}",
                            warning.path, warning.message
                        ));
                    }
                    self.output
                        .info(format!("Made a new stylesheet, `{path}`."));
                    return;
                }
                Err(err) if err.already_exists() => continue,
                Err(err) => {
                    self.output
                        .error(format!("Could not make a stylesheet: {}", chain(&err)));
                    return;
                }
            }
        }
    }

    pub(super) fn open_stylesheet_with_dialog(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() {
            return;
        }
        let folder = project.folder.clone();
        let Some(file) = rfd::FileDialog::new()
            .set_title("Open a stylesheet")
            .add_filter("CSS stylesheet", &["css"])
            .set_directory(&folder)
            .pick_file()
        else {
            return;
        };
        let path = match (folder.canonicalize(), file.canonicalize()) {
            (Ok(folder), Ok(canonical_file)) => match canonical_file.strip_prefix(&folder) {
                Ok(relative) => relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
                Err(_) => {
                    self.output.error(
                        "Could not open stylesheet: choose a CSS file inside the project folder.",
                    );
                    return;
                }
            },
            (Err(err), _) | (_, Err(err)) => {
                self.output
                    .error(format!("Could not open `{}`: {err}", file.display()));
                return;
            }
        };
        if !project::is_stylesheet(&path) {
            self.output.error(format!(
                "Could not open `{}`: it isn't a CSS stylesheet.",
                file.display()
            ));
            return;
        }
        match std::fs::read_to_string(&file) {
            Ok(source) => self.tabs.open(&path, &source, None),
            Err(err) => self
                .output
                .error(format!("Could not read stylesheet `{path}`: {err}")),
        }
    }

    /// Writes the world file and every script tab with unsaved changes.
    /// Returns false (with the reason in Output) if nothing is open or a
    /// file couldn't be written.
    pub(super) fn save(&mut self) -> bool {
        if self.project.is_none() {
            return false;
        }
        // The Model Studio's model first: its save is an undo step, so the
        // world is marked saved after it.
        if self
            .model_studio
            .doc
            .as_ref()
            .is_some_and(|doc| doc.unsaved())
            && self.save_model_studio().is_none()
        {
            self.toasts.error("Could not save. Output says why.");
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        let _ = self.animation.write_all(&project.folder, &mut self.output);
        let name = project.name.clone();
        let mut scripts = self.tabs.unsaved_paths();
        if let Some(project) = &self.project {
            for document in &project.block_scripts {
                if document.script != document.saved && !scripts.contains(&document.path) {
                    scripts.push(document.path.clone());
                }
            }
        }
        // Save with nothing changed still writes the world file, as it
        // always has, but never over a world file the editor didn't load.
        let world_unsaved = !self.history.is_saved();
        let graphs_unsaved = self.ui_graphs_unsaved()
            || self.behavior.unsaved()
            || self.dialogues.unsaved()
            || self.data_tables.unsaved()
            || self.materials.unsaved();
        let save_world =
            world_unsaved || (scripts.is_empty() && !graphs_unsaved && !project.keep_world_file);

        let mut saved = Vec::new();
        if save_world {
            if !self.save_world_file() {
                self.toasts.error("Could not save. Output says why.");
                return false;
            }
            saved.push(project::WORLD_FILE.to_string());
        }
        for path in scripts {
            if !self.write_tab(&path) {
                self.toasts.error("Could not save. Output says why.");
                return false;
            }
            saved.push(path);
        }
        // After the world, so a graph's LocalScript is made against what's saved.
        if !self.save_ui_graphs(&mut saved)
            || !self.save_behavior_trees(&mut saved)
            || !self.save_dialogues(&mut saved)
            || !self.save_data_tables(&mut saved)
            || !self.save_materials(&mut saved)
        {
            self.toasts.error("Could not save. Output says why.");
            return false;
        }
        if saved.is_empty() {
            self.output
                .info("Nothing to save: the world has no changes.");
        } else {
            self.output
                .info(format!("Saved \"{name}\": {}.", saved.join(", ")));
            self.toasts.success(format!("Saved \"{name}\""));
            if let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) {
                // The manifest says which Axcenity last saved it (format 5).
                if let Err(err) = project::record_engine(&folder) {
                    self.output.warning(format!(
                        "Saved, but could not record this Axcenity's version: {err}"
                    ));
                }
                self.repo_changed();
                match project::record_version(&folder, "Save") {
                    Ok(_) => {}
                    Err(err) => self
                        .output
                        .warning(format!("Saved, but could not keep a version: {err}")),
                }
            }
            self.tests_saved();
        }
        // The project list shows the picture taken at the last save.
        self.thumbnail_wanted.get_or_insert(false);
        true
    }

    pub(super) fn set_show_splash(&mut self, show: bool) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match project::set_show_splash(&folder, show) {
            Ok(()) => {
                if let Some(project) = &mut self.project {
                    project.show_splash = show;
                }
                self.output.info(if show {
                    "Built games will show the Axcenity splash."
                } else {
                    "Built games will not show the Axcenity splash."
                });
            }
            Err(err) => self
                .output
                .error(format!("Could not update the splash setting: {err}")),
        }
    }

    /// Stores whether built games have the F9 Developer Console, and says so.
    pub(super) fn set_developer_console(&mut self, on: bool) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match project::set_developer_console(&folder, on) {
            Ok(()) => {
                if let Some(project) = &mut self.project {
                    project.developer_console = on;
                }
                self.output.info(if on {
                    "Built games will open the Developer Console with F9."
                } else {
                    "Built games will not have the Developer Console (start one with --dev to use it)."
                });
            }
            Err(err) => self.output.error(format!(
                "Could not update the Developer Console setting: {err}"
            )),
        }
    }

    /// Stores how built games' loading screen goes, and says so.
    pub(super) fn set_loading_screen(&mut self, loading: project::LoadingScreen) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match project::set_loading_screen(&folder, &loading) {
            Ok(()) => {
                let said = if loading.screen_gui.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Built games (and Play) will show the ScreenGui \"{}\" as their loading \
                         screen for at least {:.1}s{}.",
                        loading.screen_gui,
                        loading.min_seconds,
                        if loading.wait_for_script {
                            ", until a LocalScript calls \
                             ReplicatedFirst:RemoveDefaultLoadingScreen()"
                        } else {
                            ""
                        }
                    ))
                };
                if let Some(project) = &mut self.project {
                    project.loading_screen = loading;
                }
                if let Some(said) = said {
                    self.output.info(said);
                }
            }
            Err(err) => self.output.error(format!(
                "Could not update the loading screen setting: {err}"
            )),
        }
    }

    /// Saves just the script tab for `path`, as asked when closing it.
    pub(super) fn save_tab(&mut self, path: &str) -> bool {
        let saved = self.write_tab(path);
        if saved {
            self.output.info(format!("Saved `{path}`."));
            self.tests_saved();
        }
        saved
    }

    /// Writes the world file, first keeping the old one if the editor never
    /// loaded it. Returns false, with the reason in Output, if it failed.
    /// Writes the session's World before a checkpoint while hosting
    /// (`app/collab_repo.rs`).
    pub(super) fn save_world_for_checkpoint(&mut self) -> bool {
        self.save_world_file()
    }

    fn save_world_file(&mut self) -> bool {
        // Borrows only the `project` field, so the other fields can be used too.
        let Some(project) = &mut self.project else {
            return false;
        };
        if project.keep_world_file {
            // With a level open, its file is the one the editor never loaded.
            let (kept, file) = match &project.level {
                Some(name) => (
                    project::keep_level_file(&project.folder, name),
                    project::level_path(name),
                ),
                None => (
                    project::keep_world_file(&project.folder),
                    project::WORLD_FILE.to_owned(),
                ),
            };
            match kept {
                Ok(Some(kept)) => self.output.info(format!(
                    "Kept the old `{file}`, which the editor never loaded, as `{kept}`."
                )),
                Ok(None) => {}
                Err(err) => {
                    self.output
                        .error(format!("Could not save: {}", chain(&err)));
                    return false;
                }
            }
            project.keep_world_file = false;
        }
        // Terrains first: a duplicate may be given its own data folder.
        if !self.save_terrains() {
            return false;
        }
        let Some(project) = &mut self.project else {
            return false;
        };
        if let Err(err) = project::save_strings(&project.folder, self.world.strings()) {
            self.output
                .error(format!("Could not save: {}", chain(&err)));
            return false;
        }
        match project::save_level(&project.folder, project.level.as_deref(), &self.world) {
            Ok(()) => {
                // So the watcher's report of this change is known to be ours.
                project.world_text = match project.level {
                    None => Some(project::write_world(&self.world)),
                    Some(_) => read_world_text(&project.folder),
                };
                // The undo history stays: saving only marks this point as saved.
                self.history.mark_saved();
                self.saved_after_team_session();
                true
            }
            Err(err) => {
                self.output
                    .error(format!("Could not save: {}", chain(&err)));
                false
            }
        }
    }

    /// Writes the script tab for `path` to its file. If the file had been
    /// deleted elsewhere, this brings it back, and its Script with it.
    pub(in crate::app) fn write_tab(&mut self, path: &str) -> bool {
        if project::is_block_script(path) {
            return self.write_block_tab(path);
        }
        // A game test's spec is only a file: no Script runs it in the game.
        if axcenity_testing::is_spec_file(path) {
            let (Some(project), Some(tab)) = (&self.project, self.tabs.get_mut(path)) else {
                return false;
            };
            if let Err(err) = project::write_script(&project.folder, path, &tab.text) {
                self.output
                    .error(format!("Could not save: {}", chain(&err)));
                return false;
            }
            tab.mark_saved();
            return true;
        }
        if project::is_stylesheet(path) {
            let (Some(project), Some(tab)) = (&self.project, self.tabs.get(path)) else {
                return false;
            };
            let folder = project.folder.clone();
            let source = tab.text.clone();
            if let Err(err) = project::write_stylesheet(&folder, path, &source) {
                self.output
                    .error(format!("Could not save: {}", chain(&err)));
                return false;
            }
            if let Some(tab) = self.tabs.get_mut(path) {
                tab.mark_saved();
            }
            if let Some(project) = &mut self.project {
                project
                    .stylesheet_texts
                    .insert(path.to_owned(), source.clone());
            }
            self.refresh_stylesheet(path, &source);
            return true;
        }
        let (Some(project), Some(tab)) = (&mut self.project, self.tabs.get_mut(path)) else {
            return false;
        };
        if let Err(err) = project::write_script(&project.folder, path, &tab.text) {
            self.output
                .error(format!("Could not save: {}", chain(&err)));
            return false;
        }
        tab.mark_saved();
        let source = tab.text.clone();
        // A script inside an object's file: its copies come from the world.
        if let Some(known) = project.object_files.get_mut(path) {
            *known = source;
            self.reload_in_play(path);
            return true;
        }
        match project
            .scripts
            .iter_mut()
            .find(|script| script.path == path)
        {
            Some(script) => script.source = source,
            None => {
                self.add_script(path, source);
            }
        }
        // During Play the game runs the saved code from now on.
        self.reload_in_play(path);
        true
    }

    fn write_block_tab(&mut self, path: &str) -> bool {
        let Some((folder, script)) = self.project.as_ref().and_then(|project| {
            let document = project
                .block_scripts
                .iter()
                .find(|document| document.path == path)?;
            Some((project.folder.clone(), document.script.clone()))
        }) else {
            return false;
        };
        let translation = match translate(&script) {
            Ok(translation) => translation,
            Err(err) => {
                self.output.error(format!("Could not save `{path}`: {err}"));
                return false;
            }
        };
        if let Err(err) = project::write_block_script(&folder, path, &script) {
            self.output
                .error(format!("Could not save `{path}`: {}", chain(&err)));
            return false;
        }
        // Scripts inside objects take their code from the world's copies.
        let object = self.is_object_file(path);
        let missing_instance = !object
            && self
                .project
                .as_ref()
                .is_some_and(|project| project.scripts.iter().all(|file| file.path != path));
        let instance =
            missing_instance.then(|| project::add_script_instance(&mut self.world, path));
        if let Some(project) = &mut self.project {
            if let Some(instance) = instance {
                if let Some(document) = project
                    .block_scripts
                    .iter_mut()
                    .find(|document| document.path == path)
                {
                    document.instance = instance;
                }
                project.scripts.push(ScriptFile {
                    instance,
                    path: path.to_owned(),
                    source: translation.source.clone(),
                });
            }
            if let Some(document) = project
                .block_scripts
                .iter_mut()
                .find(|document| document.path == path)
            {
                document.saved = script;
            }
            if let Some(file) = project.scripts.iter_mut().find(|file| file.path == path) {
                file.source = translation.source.clone();
            }
            if object {
                project
                    .object_files
                    .insert(path.to_owned(), translation.source.clone());
            }
        }
        if missing_instance {
            self.sort_scripts();
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = translation.source;
            tab.mark_saved();
        }
        self.reload_in_play(path);
        true
    }
}

#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod reload_tests;
#[cfg(test)]
mod tests;
