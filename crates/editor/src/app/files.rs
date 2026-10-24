//! The editor and the project folder: saving, script files (insert, open,
//! rename, delete), importing models, and reacting to files changed by
//! other programs.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExportTarget {
    Windows,
    MacOS,
    Linux,
}

fn current_export_target() -> ExportTarget {
    if cfg!(windows) {
        ExportTarget::Windows
    } else if cfg!(target_os = "macos") {
        ExportTarget::MacOS
    } else {
        ExportTarget::Linux
    }
}

impl ExportTarget {
    fn directory(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::Linux => "linux",
        }
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Windows => "axcenity-player.exe",
            Self::MacOS | Self::Linux => "axcenity-player",
        }
    }

    fn output_name(self) -> &'static str {
        match self {
            Self::Windows => "AxcenityPlayer.exe",
            Self::MacOS | Self::Linux => "AxcenityPlayer",
        }
    }

    fn app_bundle(self) -> bool {
        self == Self::MacOS
    }
}

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

/// The properties of a MeshPart showing `model` from `mesh_id`: it starts
/// at the model's size in the file. A flat model (a plane) still gets some
/// thickness, to be seen and picked.
fn mesh_part_properties(
    mesh_id: String,
    model: &axcenity_assets::Model,
) -> Vec<(&'static str, Value)> {
    let [x, y, z] = model.size.map(|size| size.max(MIN_SIZE));
    vec![
        ("MeshId", Value::String(mesh_id)),
        ("Size", Value::Vector3(Vector3::new(x, y, z))),
    ]
}

/// An object's name from a model file as an Instance name.
fn object_name(name: &str) -> String {
    match name.trim() {
        "" => ClassName::MeshPart.as_str().to_owned(),
        name => name.to_owned(),
    }
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
        let save_world = world_unsaved || (scripts.is_empty() && !project.keep_world_file);

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
        if saved.is_empty() {
            self.output
                .info("Nothing to save: the world has no changes.");
        } else {
            self.output
                .info(format!("Saved \"{name}\": {}.", saved.join(", ")));
            self.toasts.success(format!("Saved \"{name}\""));
            if let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) {
                match project::record_version(&folder, "Save") {
                    Ok(_) => {}
                    Err(err) => self
                        .output
                        .warning(format!("Saved, but could not keep a version: {err}")),
                }
            }
        }
        // The project list shows the picture taken at the last save.
        self.thumbnail_wanted.get_or_insert(false);
        true
    }

    pub(super) fn export_game_with_dialog(&mut self) {
        let target = if cfg!(windows) {
            ExportTarget::Windows
        } else if cfg!(target_os = "macos") {
            ExportTarget::MacOS
        } else {
            ExportTarget::Linux
        };
        self.export_game_for_with_dialog(target);
    }

    pub(super) fn export_game_for_with_dialog(&mut self, target: ExportTarget) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() {
            return;
        }
        let folder = project.folder.clone();
        if self.unsaved() && !self.save() {
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Export game")
            .set_directory(&folder)
            .pick_folder()
        else {
            return;
        };
        self.export_game_to(&destination, target);
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
                    "Exported games will show the Axcenity splash."
                } else {
                    "Exported games will not show the Axcenity splash."
                });
            }
            Err(err) => self
                .output
                .error(format!("Could not update the splash setting: {err}")),
        }
    }

    fn export_game_to(&mut self, destination: &Path, target: ExportTarget) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let player_name = target.executable();
        let player_source = match std::env::current_exe() {
            Ok(path) if target == current_export_target() => path.with_file_name(player_name),
            Ok(path) => path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("players")
                .join(target.directory())
                .join(player_name),
            Err(err) => {
                self.output
                    .error(format!("Could not locate the editor executable: {err}"));
                return;
            }
        };
        if !player_source.is_file() {
            let location = if target == current_export_target() {
                "beside the editor".to_owned()
            } else {
                format!("in `players/{}` beside the editor", target.directory())
            };
            self.output.error(format!(
                "Could not export: the player binary `{}` is missing. Build `axcenity-player` \
                 for {} and put it {}.",
                player_source.display(),
                target.directory(),
                location
            ));
            return;
        }
        let game = match project::pack_game(&folder) {
            Ok(game) => game,
            Err(err) => {
                self.output
                    .error(format!("Could not pack the project: {err}"));
                return;
            }
        };
        if let Err(err) = fs::create_dir_all(destination) {
            self.output.error(format!(
                "Could not create export folder `{}`: {err}",
                destination.display()
            ));
            return;
        }
        let bundle = if target.app_bundle() {
            destination
                .join("AxcenityPlayer.app")
                .join("Contents")
                .join("MacOS")
        } else {
            destination.to_owned()
        };
        if let Err(err) = fs::create_dir_all(&bundle) {
            self.output.error(format!(
                "Could not create export folder `{}`: {err}",
                bundle.display()
            ));
            return;
        }
        let output_name = target.output_name();
        let player_destination = bundle.join(output_name);
        if let Err(err) = fs::copy(&player_source, &player_destination) {
            self.output.error(format!(
                "Could not copy player to `{}`: {err}",
                player_destination.display()
            ));
            return;
        }
        let game_destination = bundle.join("game.axgame");
        if let Err(err) = fs::write(&game_destination, game) {
            self.output.error(format!(
                "Could not write game file `{}`: {err}",
                game_destination.display()
            ));
            return;
        }
        if target.app_bundle() {
            let plist = bundle
                .parent()
                .expect("MacOS folder is inside Contents")
                .join("Info.plist");
            if let Err(err) = fs::write(
                &plist,
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                 \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\"><dict>\n\
                 <key>CFBundleExecutable</key><string>AxcenityPlayer</string>\n\
                 <key>CFBundleIdentifier</key><string>com.axcenity.player</string>\n\
                 <key>CFBundleName</key><string>Axcenity Player</string>\n\
                 <key>CFBundlePackageType</key><string>APPL</string>\n\
                 <key>CFBundleVersion</key><string>1</string>\n\
                 </dict></plist>\n",
            ) {
                self.output.error(format!(
                    "Could not write macOS app metadata `{}`: {err}",
                    plist.display()
                ));
                return;
            }
        } else if target == ExportTarget::Linux {
            let desktop = destination.join("AxcenityPlayer.desktop");
            if let Err(err) = fs::write(
                &desktop,
                "[Desktop Entry]\nType=Application\nName=Axcenity Player\n\
                 Exec=./AxcenityPlayer\nTerminal=true\nCategories=Game;\n",
            ) {
                self.output.error(format!(
                    "Could not write Linux desktop entry `{}`: {err}",
                    desktop.display()
                ));
                return;
            }
        }
        let credits_folder = if target.app_bundle() {
            destination
                .join("AxcenityPlayer.app")
                .join("Contents")
                .join("Resources")
        } else {
            destination.to_owned()
        };
        if let Err(err) = fs::create_dir_all(&credits_folder) {
            self.output.error(format!(
                "Could not create credits folder `{}`: {err}",
                credits_folder.display()
            ));
            return;
        }
        let credits_destination = credits_folder.join("LICENSES.txt");
        if let Err(err) = fs::write(
            &credits_destination,
            format!(
                "Axcenity Engine and third-party license notices\n\n{}",
                include_str!("../../../../NOTICE")
            ),
        ) {
            self.output.error(format!(
                "Could not write credits file `{}`: {err}",
                credits_destination.display()
            ));
            return;
        }
        self.output.info(format!(
            "Exported the game for {} to `{}` (run `{output_name}`).",
            target.directory(),
            destination.display()
        ));
    }

    /// Saves just the script tab for `path`, as asked when closing it.
    pub(super) fn save_tab(&mut self, path: &str) -> bool {
        let saved = self.write_tab(path);
        if saved {
            self.output.info(format!("Saved `{path}`."));
        }
        saved
    }

    /// Writes the world file, first keeping the old one if the editor never
    /// loaded it. Returns false, with the reason in Output, if it failed.
    fn save_world_file(&mut self) -> bool {
        // Borrows only the `project` field, so the other fields can be used too.
        let Some(project) = &mut self.project else {
            return false;
        };
        if project.keep_world_file {
            match project::keep_world_file(&project.folder) {
                Ok(Some(kept)) => self.output.info(format!(
                    "Kept the old `{}`, which the editor never loaded, as `{kept}`.",
                    project::WORLD_FILE
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
        match project::save_world(&project.folder, &self.world) {
            Ok(()) => {
                // So the watcher's report of this change is known to be ours.
                project.world_text = Some(project::write_world(&self.world));
                // The undo history stays: saving only marks this point as saved.
                self.history.mark_saved();
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
    fn write_tab(&mut self, path: &str) -> bool {
        if project::is_block_script(path) {
            return self.write_block_tab(path);
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
        let missing_instance = self
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
        }
        if missing_instance {
            self.sort_scripts();
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = translation.source;
            tab.mark_saved();
        }
        true
    }

    /// The path of the file of the Script `id`.
    fn script_path(&self, id: InstanceId) -> Option<String> {
        let project = self.project.as_ref()?;
        let script = project
            .scripts
            .iter()
            .find(|script| script.instance == id)?;
        Some(script.path.clone())
    }

    /// Adds the script at `path` (whose file has `source` in it) to the
    /// project, with a Script or LocalScript for it, and returns that.
    fn add_script(&mut self, path: &str, source: String) -> InstanceId {
        let instance = project::add_script_instance(&mut self.world, path);
        if let Some(project) = &mut self.project {
            project.scripts.push(ScriptFile {
                instance,
                path: path.to_string(),
                source,
            });
        }
        self.sort_scripts();
        instance
    }

    /// Takes the script at `path` out of the project, and its Script out of
    /// the world. Its tab stays (the caller decides).
    fn remove_script(&mut self, path: &str) {
        self.remove_script_instance(path);
        if let Some(project) = &mut self.project {
            project
                .block_scripts
                .retain(|document| document.path != path);
        }
    }

    fn remove_script_instance(&mut self, path: &str) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(index) = project
            .scripts
            .iter()
            .position(|script| script.path == path)
        else {
            return;
        };
        let script = project.scripts.remove(index);
        // Scripts are never in the undo history, so nothing refers to it there.
        let _ = self.world.destroy(script.instance);
        self.selection.retain(|id| *id != script.instance);
        if self.selected == Some(script.instance) {
            self.selected = self.selection.last().copied();
        }
    }

    /// Keeps the scripts in the order they start in (and load in next
    /// time): ModuleScripts, Scripts, then LocalScripts, each by path. Their
    /// Instances in ReplicatedStorage, ServerScriptService and
    /// StarterPlayerScripts follow the same order.
    fn sort_scripts(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        project
            .scripts
            .sort_by(|a, b| project::script_order(&a.path).cmp(&project::script_order(&b.path)));
        // Each service's scripts go after its other children (only
        // ReplicatedStorage has any: RemoteEvents, Folders and so on), as
        // they are after loading, where the world file comes first.
        let mut next_index = std::collections::HashMap::new();
        for script in &project.scripts {
            let service = project::script_service(&self.world, project::script_class(&script.path));
            let index = next_index
                .entry(service)
                .or_insert_with(|| non_script_children(&self.world, service));
            let _ = self.world.set_parent_at(script.instance, service, *index);
            *index += 1;
        }
    }

    /// Opens the Script `id` in a tab.
    pub(super) fn open_script_instance(&mut self, id: InstanceId) {
        if let Some(path) = self.script_path(id) {
            self.open_script(&path, None);
        }
    }

    /// Opens the script at `path` in a tab (or shows its tab), with the
    /// cursor on `line` if there is one.
    pub(super) fn open_script(&mut self, path: &str, line: Option<usize>) {
        let known = self
            .project
            .as_ref()
            .and_then(|project| project.scripts.iter().find(|script| script.path == path))
            .map(|script| script.source.clone());
        if project::is_block_script(path)
            && let Some(document) = self.project.as_mut().and_then(|project| {
                project
                    .block_scripts
                    .iter_mut()
                    .find(|document| document.path == path)
            })
        {
            document.highlighted =
                line.and_then(|line| translate(&document.script).ok()?.block_at(line));
        }
        match known {
            Some(source) => self.tabs.open(path, &source, line),
            // Its file was deleted, but its tab is still open.
            None if self.tabs.get(path).is_some() => self.tabs.open(path, "", line),
            None => self.output.error(format!(
                "Can't open `{path}`: this project has no such script now (it may have \
                 been renamed or deleted)."
            )),
        }
    }

    /// Makes a new script file for a script of `class`:
    /// `scripts/Script.server.lua` for a Script, or
    /// `scripts/LocalScript.client.lua` for a LocalScript (or `Script2` and
    /// so on), with its Instance, then selects it and opens it.
    pub(super) fn insert_script(&mut self, class: ClassName) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let service = project::script_service(&self.world, class);
        for name in tree::numbered_names(class.as_str()) {
            if self.world.find_first_child(service, &name).is_some() {
                continue;
            }
            let source = new_script_source(class);
            match project::create_script(&folder, &name, class, source) {
                Ok(path) => {
                    let id = self.add_script(&path, source.to_string());
                    // Selects it and shows it in the Explorer.
                    self.select_from_viewport(Some(id));
                    self.tabs.open(&path, source, None);
                    self.output.info(format!("Made a new script, `{path}`."));
                    return;
                }
                // A file with that name is there already (one that couldn't
                // be read, or one that only differs in capitals): next name.
                Err(err) if err.already_exists() => continue,
                Err(err) => {
                    let err = chain(&err);
                    self.output
                        .error(format!("Could not make a new script: {err}"));
                    return;
                }
            }
        }
    }

    /// Switches the Script or LocalScript at `path` between Code and Blocks,
    /// keeping its Instance. Blocks become the Lua they translate to; Lua
    /// becomes blocks, with code that has no blocks kept in grey Lua blocks.
    /// The file is replaced by one with the other ending (`.server.lua` ↔
    /// `.server.blocks.json`), so switching saves the script. Like a rename,
    /// it isn't an undo step. Blocks switched to Code and back, with the code
    /// unchanged, come back exactly as they were, loose blocks and all.
    pub(super) fn switch_script_mode(&mut self, path: &str) {
        if self.playing() {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let name = project::script_name(path).to_owned();
        let Some(instance) = project
            .scripts
            .iter()
            .find(|script| script.path == path)
            .map(|script| script.instance)
        else {
            return;
        };
        let to_blocks = !project::is_block_script(path);
        let (new_path, source, blocks) = if to_blocks {
            let source = self.tabs.get(path).map_or_else(
                || {
                    project
                        .scripts
                        .iter()
                        .find(|script| script.path == path)
                        .map(|script| script.source.clone())
                        .unwrap_or_default()
                },
                |tab| tab.text.clone(),
            );
            let remembered = project
                .remembered_blocks
                .get(path)
                .filter(|(code, _)| *code == source)
                .map(|(_, blocks)| blocks.clone());
            let blocks = match remembered.map_or_else(|| from_lua(&source), Ok) {
                Ok(blocks) => blocks,
                Err(err) => {
                    self.output.error(format!(
                        "Can't show {name} as blocks: {err}. Fix the code first, then switch."
                    ));
                    return;
                }
            };
            let source = match translate(&blocks) {
                Ok(translation) => translation.source,
                Err(err) => {
                    self.output
                        .error(format!("Can't show {name} as blocks: {err}."));
                    return;
                }
            };
            match project::switch_to_blocks(&folder, path, &blocks) {
                Ok(new_path) => (new_path, source, Some(blocks)),
                Err(err) => {
                    self.output.error(format!(
                        "Could not switch {name} to Blocks: {}",
                        chain(&err)
                    ));
                    return;
                }
            }
        } else {
            let Some(document) = project
                .block_scripts
                .iter()
                .find(|document| document.path == path)
            else {
                return;
            };
            let blocks = document.script.clone();
            let source = match translate(&blocks) {
                Ok(translation) => translation.source,
                Err(err) => {
                    self.output
                        .error(format!("Can't switch {name} to Code: {err}."));
                    return;
                }
            };
            match project::switch_to_code(&folder, path, &source) {
                Ok(new_path) => {
                    if let Some(project) = &mut self.project {
                        project
                            .remembered_blocks
                            .insert(new_path.clone(), (source.clone(), blocks));
                    }
                    (new_path, source, None)
                }
                Err(err) => {
                    self.output
                        .error(format!("Could not switch {name} to Code: {}", chain(&err)));
                    return;
                }
            }
        };

        if let Some(project) = &mut self.project {
            project
                .block_scripts
                .retain(|document| document.path != path);
            if let Some(blocks) = blocks {
                project.block_scripts.push(BlockDocument {
                    instance,
                    path: new_path.clone(),
                    script: blocks.clone(),
                    saved: blocks,
                    highlighted: None,
                });
            }
            if let Some(script) = project.scripts.iter_mut().find(|s| s.path == path) {
                script.path = new_path.clone();
                script.source = source.clone();
            }
        }
        self.tabs.rename(path, &new_path);
        if let Some(tab) = self.tabs.get_mut(&new_path) {
            tab.text = source;
            tab.mark_saved();
        }
        self.sort_scripts();
        let mode = if to_blocks { "Blocks" } else { "Code" };
        self.output
            .info(format!("Switched {name} to {mode} (`{new_path}`)."));
    }

    /// File > Import Model… and the toolbar's Import: asks for a glTF or
    /// FBX model with the system's file picker, then imports it.
    pub(super) fn import_model_with_dialog(&mut self) {
        if self.project.is_none() || self.playing() || self.viewport.is_dragging() {
            return;
        }
        let dialog = rfd::FileDialog::new()
            .set_title("Import a 3D model")
            .add_filter("3D model", &["gltf", "glb", "fbx"])
            .add_filter("glTF model", &["gltf", "glb"])
            .add_filter("FBX model", &["fbx"]);
        // `None` means the dialog was cancelled.
        if let Some(file) = dialog.pick_file() {
            self.import_model(&file);
        }
    }

    /// Asks for a texture file with the system's file picker, for a map
    /// in Properties, and returns its path in the project: a file outside
    /// the project is copied into `assets/textures` first. `None` if
    /// cancelled or it can't be used (said in Output).
    pub(super) fn pick_texture(&mut self, into: &str) -> Option<String> {
        let project = self.project.as_ref()?;
        let file = rfd::FileDialog::new()
            .set_title("Choose a texture")
            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
            .set_directory(&project.folder)
            .pick_file()?;
        match axcenity_assets::import_texture(&project.folder, &file, into) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Asks for a sound file, and returns its `SoundId`: its path in the
    /// project, copied into `assets/audio` first if it was outside.
    pub(super) fn pick_audio(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let file = rfd::FileDialog::new()
            .set_title("Choose a sound")
            .add_filter("Sound", &axcenity_assets::AUDIO_EXTENSIONS)
            .set_directory(&project.folder)
            .pick_file()?;
        match axcenity_assets::import_audio(&project.folder, &file) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Asks for an animator controller file inside the project, and returns
    /// its path there for an Animator's `Controller`.
    pub(super) fn pick_controller(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let animations = project.folder.join(axcenity_anim::ANIMATIONS_FOLDER);
        let start = if animations.is_dir() {
            animations
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a state machine")
            .add_filter("State machine", &["json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION));
        if relative.is_none() {
            self.output.error(format!(
                "Could not use `{}`: choose a `.statemachine.json` file inside the project folder.",
                file.display()
            ));
        }
        relative
    }

    /// Asks for a property clip (`.anim.json` with `"kind": "property"`).
    pub(super) fn pick_property_clip(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let animations = project.folder.join(axcenity_anim::ANIMATIONS_FOLDER);
        let start = if animations.is_dir() {
            animations
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a property clip")
            .add_filter("Animation clip", &["json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| path.ends_with(axcenity_anim::CLIP_EXTENSION));
        let Some(path) = relative else {
            self.output.error(format!(
                "Could not use `{}`: choose a `.anim.json` file inside the project folder.",
                file.display()
            ));
            return None;
        };
        match store::read_any_clip(&project.folder, &path) {
            Ok(_) => Some(path),
            Err(err) => {
                self.output.error(format!("Could not read `{path}`: {err}"));
                None
            }
        }
    }

    /// Plays the Sound `id` in the editor (or stops it), with its own
    /// settings. Starting Play stops it too.
    pub(super) fn preview_sound(&mut self, id: InstanceId, play: bool) {
        if !play {
            self.sound_preview = None;
            return;
        }
        let Some(sound) = self.world.get(id) else {
            return;
        };
        let text = |name| {
            sound
                .property(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let action = axcenity_audio::SoundAction::Play {
            instance: id,
            sound_id: text("SoundId"),
            volume: sound
                .property("Volume")
                .and_then(Value::as_number)
                .unwrap_or(1.0),
            looped: sound.property("Looped").and_then(Value::as_bool) == Some(true),
        };
        if self.sound_preview.is_none() {
            match axcenity_audio::AudioEngine::new() {
                Ok(engine) => self.sound_preview = Some(engine),
                Err(err) => {
                    self.output.error(format!("Audio is unavailable: {err}"));
                    return;
                }
            }
        }
        let (position, orientation) = self.viewport.camera_view();
        let listener = axcenity_audio::Listener::from_view(position, orientation);
        if let Some(engine) = &mut self.sound_preview {
            engine.set_listener(listener);
            if let Err(err) = engine.apply(action, &self.world, &self.assets) {
                self.output.error(format!("Audio: {err}"));
            }
        }
    }

    /// Keeps a Properties preview in the place the camera and the Sound's
    /// part are now. A spatial preview pans as the viewport camera moves.
    pub(super) fn follow_sound_preview(&mut self) {
        if self.playing() {
            return;
        }
        let Some(engine) = &mut self.sound_preview else {
            return;
        };
        let (position, orientation) = self.viewport.camera_view();
        let listener = axcenity_audio::Listener::from_view(position, orientation);
        for err in engine.follow(&self.world, &self.assets, listener) {
            self.output.error(format!("Audio: {err}"));
        }
    }

    /// Imports the model at `file`. A file with more than one object, or
    /// with texture maps, asks first which objects to import and shows its
    /// materials' maps for changing ([`ImportDialog`]); any other goes
    /// straight in as a MeshPart. A file that can't be read is reported in
    /// Output.
    pub(super) fn import_model(&mut self, file: &Path) {
        if self.project.is_none() || self.playing() || self.viewport.is_dragging() {
            return;
        }
        let read = axcenity_assets::list_model_objects(file).and_then(|objects| {
            let model = axcenity_assets::load_model_file(file)?;
            Ok((objects, model))
        });
        // A file's takes, if it has any. One that can't be read just has none.
        let takes = axcenity_assets::load_takes_file(file).unwrap_or_default();
        let model_name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (objects, model) = match read {
            Ok(read) => read,
            // Nothing to draw, but takes: an animation-only file.
            Err(axcenity_assets::ImportError::Model(axcenity_assets::ModelError::NoTriangles))
                if let Some(animation) = AnimationImport::new(None, &takes, &model_name) =>
            {
                self.importing = Some(ImportDialog::animation_only(file.to_owned(), animation));
                return;
            }
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not import `{}`: {err}", file.display()));
                return;
            }
        };
        let materials = surface::materials_maps(model.primitives.iter().map(|p| &p.material));
        let animation = AnimationImport::new(model.rig.as_deref(), &takes, &model_name);
        let dialog =
            ImportDialog::new(file.to_owned(), objects, materials).with_animation(animation);
        if dialog.worth_asking() {
            self.importing = Some(dialog);
        } else {
            self.import_chosen(&dialog);
        }
    }

    /// Shows the Import Model dialog while a file is waiting for it, and
    /// imports what it says.
    pub(super) fn import_prompt(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.importing else {
            return;
        };
        match dialog.show(ctx) {
            Some(Answer::Import) => {
                let dialog = self.importing.take().expect("checked above");
                if dialog.animation_only {
                    self.import_animation_only(&dialog);
                } else {
                    self.import_chosen(&dialog);
                }
            }
            Some(Answer::Cancel) => self.importing = None,
            None => {}
        }
    }

    /// Copies the dialog's model file into the project's `assets/models`
    /// folder and inserts the objects ticked in it, as its placement says,
    /// inside the selected Model or Folder if there is one. Each MeshPart
    /// starts at its object's size in the file. The insert is one undo
    /// step; the copied file stays. A model that can't be loaded is
    /// reported in Output and not copied; problems that didn't stop it
    /// loading (an FBX texture that isn't there) are reported after it's
    /// imported.
    pub(super) fn import_chosen(&mut self, dialog: &ImportDialog) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let file = &dialog.file;
        let chosen = dialog.chosen();
        let path = |i: usize| dialog.objects[i].path.clone();
        let picks: Vec<Pick> = match dialog.placement {
            Placement::Merged if dialog.all_chosen() => vec![Pick::Whole],
            Placement::Merged => vec![Pick::Objects(chosen.iter().map(|&i| path(i)).collect())],
            Placement::Grouped | Placement::Separate => chosen
                .iter()
                .map(|&i| Pick::Objects(vec![path(i)]))
                .collect(),
        };
        let imported = match axcenity_assets::import_picks(&project.folder, file, &picks) {
            Ok(imported) => imported,
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not import `{}`: {err}", file.display()));
                return;
            }
        };
        let (copy, _) = Pick::of_mesh_id(&imported[0].0);
        self.output.info(format!(
            "Imported `{}` into the project as `{copy}`.",
            file.display()
        ));
        let animation = dialog
            .animation
            .as_ref()
            .and_then(|animation| self.write_import_animation(animation, copy));
        // The copy's folder in the project, which its maps' paths start from.
        let folder = copy
            .rsplit_once('/')
            .map_or("", |(folder, _)| folder)
            .to_owned();
        let mut copied = HashMap::new();
        let mut parts = Vec::new();
        for (mesh_id, model) in imported {
            let model = self.assets.insert(&mesh_id, model);
            for warning in &model.warnings {
                self.output
                    .error(format!("The model `{mesh_id}` {warning}."));
            }
            parts.push((mesh_id, model));
        }

        let file_name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim().to_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| ClassName::MeshPart.as_str().to_owned());
        let near = self.selected;
        let position = self.insert_position();
        match dialog.placement {
            Placement::Merged => {
                let parent = tree::insert_parent(&self.world, near, Insertable::MeshPart);
                let (mesh_id, model) = parts.remove(0);
                let id = self.new_mesh_part(mesh_id, &model, position);
                self.world.set_name(id, &file_name).expect("just made");
                self.add_surfaces(id, &model, dialog, &folder, &mut copied);
                self.add_animator(id, &model, animation.as_ref());
                self.place_new(parent, vec![id], format!("Import {file_name}"));
            }
            Placement::Separate => {
                let parent = tree::insert_parent(&self.world, near, Insertable::MeshPart);
                let ids: Vec<InstanceId> = chosen
                    .iter()
                    .zip(parts)
                    .map(|(&i, (mesh_id, model))| {
                        let id = self.new_mesh_part(mesh_id, &model, position);
                        self.world
                            .set_name(id, object_name(&dialog.objects[i].name))
                            .expect("just made");
                        self.add_surfaces(id, &model, dialog, &folder, &mut copied);
                        self.add_animator(id, &model, animation.as_ref());
                        id
                    })
                    .collect();
                self.place_new(parent, ids, format!("Import {file_name}"));
            }
            Placement::Grouped => {
                let parent = tree::insert_parent(&self.world, near, Insertable::Model);
                let root = tree::create(&mut self.world, Insertable::Model, position);
                self.world.set_name(root, &file_name).expect("just made");
                // The parts keep their places around the middle of them all.
                let mut min = [f32::INFINITY; 3];
                let mut max = [f32::NEG_INFINITY; 3];
                for (_, model) in &parts {
                    for axis in 0..3 {
                        let half = model.size[axis] / 2.0;
                        min[axis] = min[axis].min(model.center[axis] - half);
                        max[axis] = max[axis].max(model.center[axis] + half);
                    }
                }
                let middle = [0, 1, 2].map(|axis| (min[axis] + max[axis]) / 2.0);
                let mut groups = HashMap::new();
                for (&i, (mesh_id, model)) in chosen.iter().zip(parts) {
                    let at = Vector3::new(
                        position.x + model.center[0] - middle[0],
                        position.y + model.center[1] - middle[1],
                        position.z + model.center[2] - middle[2],
                    );
                    let id = self.new_mesh_part(mesh_id, &model, at);
                    self.add_surfaces(id, &model, dialog, &folder, &mut copied);
                    self.add_animator(id, &model, animation.as_ref());
                    let container = self.group_of(dialog, i, root, &mut groups);
                    let name = tree::unique_name(
                        &self.world,
                        container,
                        &object_name(&dialog.objects[i].name),
                    );
                    self.world.set_name(id, name).expect("just made");
                    self.world
                        .set_parent(id, Some(container))
                        .expect("a new part can go in a new Model");
                }
                self.place_new(parent, vec![root], format!("Import {file_name}"));
            }
        }
    }

    /// Writes the clips `animation` makes, from the model copied to
    /// `copy` in the project, and their state machine if an Animator was
    /// asked for. Returns the state machine's path and the clips', for the
    /// Animator. Files aren't undo steps; a problem is reported in Output.
    fn write_import_animation(
        &mut self,
        animation: &AnimationImport,
        copy: &str,
    ) -> Option<(String, Vec<String>)> {
        let project = self.project.as_ref()?.folder.clone();
        if animation.clips.is_empty() {
            return None;
        }
        let model = copy
            .rsplit('/')
            .next()
            .and_then(|file| file.rsplit_once('.'))
            .map_or(copy, |(stem, _)| stem);
        let clips = animation.clip_files(copy);
        match crate::anim_import::write_animation(&project, model, &clips, animation.add_animator) {
            Ok((paths, controller)) => {
                self.output.info(format!(
                    "Made {} {} in `{}/{model}`{}.",
                    paths.len(),
                    if paths.len() == 1 { "clip" } else { "clips" },
                    axcenity_anim::ANIMATIONS_FOLDER,
                    controller.as_ref().map_or_else(String::new, |path| format!(
                        " and the state machine `{path}`"
                    ))
                ));
                controller.map(|controller| (controller, paths))
            }
            Err(err) => {
                self.output
                    .error(format!("Could not write the animation clips: {err}"));
                None
            }
        }
    }

    /// Puts an Animator playing the state machine in the new MeshPart
    /// `part`, with an Animation for each clip under it, if there's a state
    /// machine (`animation`: its path and its clips') and the part's
    /// `model` has a skeleton to move.
    fn add_animator(
        &mut self,
        part: InstanceId,
        model: &axcenity_assets::Model,
        animation: Option<&(String, Vec<String>)>,
    ) {
        let Some((controller, clips)) = animation else {
            return;
        };
        if model.rig.is_none() {
            return;
        }
        let id = self
            .world
            .create(ClassName::Animator)
            .expect("not a service");
        self.world
            .set_property(id, "StateMachine", Value::String(controller.to_owned()))
            .expect("every Animator has a StateMachine");
        self.world
            .set_parent(id, Some(part))
            .expect("an Animator can go in a new MeshPart");
        for clip in clips {
            let animation = self
                .world
                .create(ClassName::Animation)
                .expect("not a service");
            let name = tree::unique_name(&self.world, id, axcenity_anim::files::clip_name(clip));
            self.world.set_name(animation, name).expect("just made");
            self.world
                .set_property(animation, "Clip", Value::String(clip.clone()))
                .expect("every Animation has a Clip");
            self.world
                .set_parent(animation, Some(id))
                .expect("an Animation can go in an Animator");
        }
    }

    /// Imports a file with only animation: copies it into the project and
    /// writes the clips the dialog made from its takes.
    pub(super) fn import_animation_only(&mut self, dialog: &ImportDialog) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(animation) = &dialog.animation else {
            return;
        };
        match axcenity_assets::import_animation_file(&project.folder, &dialog.file) {
            Ok(copy) => {
                self.output.info(format!(
                    "Imported the animation `{}` into the project as `{copy}`.",
                    dialog.file.display()
                ));
                let mut clips = animation.clone();
                clips.add_animator = false;
                self.write_import_animation(&clips, &copy);
            }
            Err(err) => {
                let err = chain(&err);
                self.output.error(format!(
                    "Could not import `{}`: {err}",
                    dialog.file.display()
                ));
            }
        }
    }

    /// Puts a SurfaceAppearance in the new MeshPart `part` for each
    /// material its `model` uses, with the maps `dialog` has for it (as
    /// found, or as changed there). `folder` is the model's copy's folder
    /// in the project. Files picked in the dialog are copied into its
    /// `textures` folder, each once (`copied` remembers where to).
    fn add_surfaces(
        &mut self,
        part: InstanceId,
        model: &axcenity_assets::Model,
        dialog: &ImportDialog,
        folder: &str,
        copied: &mut HashMap<std::path::PathBuf, Option<String>>,
    ) {
        let Some(project) = &self.project else {
            return;
        };
        let project = project.folder.clone();
        let textures = format!("{folder}/{}", axcenity_assets::TEXTURES_FOLDER);
        let in_project = |path: &str| match folder {
            "" => path.to_owned(),
            folder => format!("{folder}/{path}"),
        };
        for maps in surface::materials_maps(model.primitives.iter().map(|p| &p.material)) {
            let maps = dialog
                .materials
                .iter()
                .find(|changed| changed.material == maps.material)
                .cloned()
                .unwrap_or(maps);
            let mut errors = Vec::new();
            let properties = maps.properties(|file| match file {
                MapFile::Model(path) => Some(in_project(path)),
                MapFile::Picked(path) => copied
                    .entry(path.clone())
                    .or_insert_with(|| {
                        match axcenity_assets::import_texture(&project, path, &textures) {
                            Ok(copy) => Some(copy),
                            Err(err) => {
                                errors.push(format!(
                                    "Could not use `{}`: {}",
                                    path.display(),
                                    chain(&err)
                                ));
                                None
                            }
                        }
                    })
                    .clone(),
            });
            for error in errors {
                self.output.error(error);
            }
            let id = self
                .world
                .create(ClassName::SurfaceAppearance)
                .expect("not a service");
            for (property, value) in properties {
                self.world
                    .set_property(id, property, value)
                    .expect("every SurfaceAppearance has these");
            }
            let wanted = match maps.material.as_str() {
                "" => ClassName::SurfaceAppearance.as_str(),
                name => name,
            };
            let name = tree::unique_name(&self.world, part, wanted);
            self.world.set_name(id, name).expect("just made");
            self.world
                .set_parent(id, Some(part))
                .expect("a SurfaceAppearance can go in a new MeshPart");
        }
    }

    /// A new MeshPart showing `model` (loaded from `mesh_id`), centered on
    /// `position`, with no parent yet.
    pub(super) fn new_mesh_part(
        &mut self,
        mesh_id: String,
        model: &axcenity_assets::Model,
        position: Vector3,
    ) -> InstanceId {
        let id = tree::create(&mut self.world, Insertable::MeshPart, position);
        for (property, value) in mesh_part_properties(mesh_id, model) {
            self.world
                .set_property(id, property, value)
                .expect("every MeshPart has these");
        }
        id
    }

    /// The Model that object `i` of `dialog` goes in when importing it as
    /// a Model: one made for the group (an object without a mesh) it's
    /// inside, or `root` if it isn't inside one. Groups' Models are made
    /// the first time, and remembered in `groups` by object.
    fn group_of(
        &mut self,
        dialog: &ImportDialog,
        i: usize,
        root: InstanceId,
        groups: &mut HashMap<usize, InstanceId>,
    ) -> InstanceId {
        // An object inside one with a mesh goes next to it: parts don't
        // hold other parts.
        let mut parent = dialog.parent(i);
        while let Some(p) = parent
            && dialog.objects[p].has_mesh()
        {
            parent = dialog.parent(p);
        }
        let Some(group) = parent else {
            return root;
        };
        if let Some(&model) = groups.get(&group) {
            return model;
        }
        let container = self.group_of(dialog, group, root, groups);
        let model = tree::create(
            &mut self.world,
            Insertable::Model,
            Vector3::new(0.0, 0.0, 0.0),
        );
        let name = tree::unique_name(
            &self.world,
            container,
            &object_name(&dialog.objects[group].name),
        );
        self.world.set_name(model, name).expect("just made");
        self.world
            .set_parent(model, Some(container))
            .expect("a new Model can go in a new Model");
        groups.insert(group, model);
        model
    }

    /// Puts the new Instances `ids` at the end of `parent`, each with a
    /// name none of its siblings has, as one undo step named `label`, and
    /// selects the last of them.
    pub(super) fn place_new(&mut self, parent: InstanceId, ids: Vec<InstanceId>, label: String) {
        let mut moves = Vec::new();
        for &id in &ids {
            let wanted = self.world.get(id).map(|i| i.name().to_string());
            let name = tree::unique_name(&self.world, parent, &wanted.unwrap_or_default());
            self.world.set_name(id, name).expect("just made");
            let end = self.world.get(parent).map_or(0, |p| p.children().len());
            let place = Place { parent, index: end };
            if let Err(err) = history::put(&mut self.world, id, Some(place)) {
                self.output.error(format!("Could not do that: {err}"));
                break;
            }
            moves.push(Move {
                id,
                before: None,
                after: Some(place),
            });
        }
        let last = moves.last().map(|step| step.id);
        self.history.record_tree(&mut self.world, label, moves);
        if last.is_some() {
            self.select_from_viewport(last);
        }
    }

    /// Renames the Script `id` and its file to `name`. Not an undo step.
    pub(super) fn rename_script(&mut self, id: InstanceId, name: &str) {
        let name = name.trim();
        let (Some(project), Some(old)) = (&self.project, self.script_path(id)) else {
            return;
        };
        let old_name = project::script_name(&old).to_string();
        if old_name == name {
            return;
        }
        if let Err(reason) = project::check_script_name(name) {
            self.output
                .error(format!("Can't rename {old_name} to \"{name}\": {reason}."));
            return;
        }
        let rename = if project::is_block_script(&old) {
            project::rename_block_script(&project.folder, &old, name)
        } else {
            project::rename_script(&project.folder, &old, name)
        };
        match rename {
            Ok(new) => {
                self.world.set_name(id, name).expect("it's a Script");
                if let Some(project) = &mut self.project
                    && let Some(script) = project.scripts.iter_mut().find(|s| s.instance == id)
                {
                    script.path = new.clone();
                }
                if let Some(document) = self
                    .project
                    .as_mut()
                    .and_then(|project| project.block_scripts.iter_mut().find(|d| d.instance == id))
                {
                    document.path = new.clone();
                }
                self.tabs.rename(&old, &new);
                self.sort_scripts();
                self.output.info(format!("Renamed `{old}` to `{new}`."));
            }
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not rename {old_name}: {err}"));
            }
        }
    }

    /// Shows "Delete script?" while a Script is waiting to be deleted.
    pub(super) fn delete_script_prompt(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else {
            return;
        };
        let Some(path) = self.script_path(id) else {
            // It went away meanwhile (deleted in another program, say).
            self.confirm_delete = None;
            return;
        };
        let title = format!("Delete {}?", project::script_name(&path));
        let message = format!("This deletes `{path}` from the project folder. It can't be undone.");
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "delete script",
                icon: icons::TRASH,
                tone: dialog::Tone::Danger,
                title: &title,
                message: &message,
                items: Vec::new(),
            },
            &[
                dialog::Button {
                    icon: icons::X,
                    label: "Cancel",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::TRASH,
                    label: "Delete",
                    kind: dialog::Kind::Danger,
                },
            ],
        );
        // Escape or a click outside the box means Cancel.
        let delete = outcome.map(|outcome| outcome == dialog::Outcome::Picked(1));
        if let Some(delete) = delete {
            self.confirm_delete = None;
            if delete {
                self.delete_script(id);
            }
        }
    }

    /// Deletes the Script `id` and its file, and closes its tab.
    pub(super) fn delete_script(&mut self, id: InstanceId) {
        let (Some(project), Some(path)) = (&self.project, self.script_path(id)) else {
            return;
        };
        match project::delete_script(&project.folder, &path) {
            Ok(()) => {}
            // Already gone is as good as deleted.
            Err(err) if err.source.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                self.output
                    .error(format!("Could not delete it: {}", chain(&err)));
                return;
            }
        }
        self.remove_script(&path);
        self.tabs.close(&path);
        self.output.info(format!("Deleted `{path}`."));
    }

    /// Takes in the files other programs changed since the last frame.
    pub(super) fn check_disk(&mut self) {
        // During Play, changes wait until Stop: the game runs on a copy of
        // the edit world, and the two mustn't get mixed up.
        if self.playing() {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let paths = project.watcher.changes();
        self.disk_changed(&paths);
    }

    /// Takes in changes to the files at `paths` (as the watcher names them).
    /// Files that are the same as the editor has them (its own saves) are
    /// left alone.
    pub(super) fn disk_changed(&mut self, paths: &[String]) {
        for path in paths {
            if path == project::WORLD_FILE {
                self.world_file_changed();
            } else if project::is_stylesheet(path) {
                self.stylesheet_file_changed(path);
            } else {
                self.script_file_changed(path);
            }
        }
    }

    /// A stylesheet changed on disk, such as one edited in VS Code: restyle
    /// the UI with it, unless its tab has unsaved changes (they're kept, and
    /// saving replaces the other version). A deleted one stops applying.
    fn stylesheet_file_changed(&mut self, path: &str) {
        let Some(project) = &mut self.project else {
            return;
        };
        let source = std::fs::read_to_string(project.folder.join(path)).ok();
        if source.as_ref() == project.stylesheet_texts.get(path) {
            // The editor's own save, or touched without a change.
            return;
        }
        let Some(source) = source else {
            project.stylesheet_texts.remove(path);
            project.stylesheets.retain(|sheet| sheet.path() != path);
            self.output
                .info(format!("`{path}` was deleted outside the editor."));
            self.sync_tab(path, None);
            return;
        };
        project
            .stylesheet_texts
            .insert(path.to_owned(), source.clone());
        let unsaved = self.tabs.get(path).is_some_and(|tab| tab.unsaved());
        if !unsaved {
            self.refresh_stylesheet(path, &source);
            self.output
                .info(format!("`{path}` was changed outside the editor."));
        }
        self.sync_tab(path, Some(&source));
    }

    /// The world file changed: load it, unless the world has unsaved changes.
    fn world_file_changed(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        let text = read_world_text(&project.folder);
        if text == project.world_text {
            return;
        }
        project.world_text = text.clone();
        let world_file = project::WORLD_FILE;
        if !self.history.is_saved() {
            project.keep_world_file = true;
            self.output.error(format!(
                "`{world_file}` was changed outside the editor, but the world has unsaved \
                 changes here, so they stay. Saving keeps the other version as `{}`.",
                project::KEPT_WORLD_FILE
            ));
            return;
        }
        let path = project.folder.join(world_file);
        let parsed = match text {
            Some(text) => {
                project::parse_world(&text).map_err(|source| LoadError::World { path, source })
            }
            None => Err(LoadError::Read {
                path,
                source: std::io::Error::new(ErrorKind::NotFound, "it's gone"),
            }),
        };
        match parsed {
            Ok(mut world) => {
                let mut problems = Vec::new();
                let damaged = project::load_terrains(&project.folder, &mut world, &mut problems);
                for problem in &problems {
                    self.output
                        .error(format!("Skipped a damaged file: {}", chain(problem)));
                }
                project.damaged_terrains = damaged;
                // The new world has no Scripts yet: they come from the files.
                for script in &mut project.scripts {
                    script.instance = project::add_script_instance(&mut world, &script.path);
                }
                for document in &mut project.block_scripts {
                    if let Some(script) = project
                        .scripts
                        .iter()
                        .find(|script| script.path == document.path)
                    {
                        document.instance = script.instance;
                    }
                }
                project.keep_world_file = false;
                self.replace_world(world);
                self.output.info(format!(
                    "Reloaded `{world_file}`, which was changed outside the editor."
                ));
            }
            Err(err) => {
                project.keep_world_file = true;
                self.output.error(format!(
                    "`{world_file}` was changed outside the editor, but can't be loaded: {}. \
                     The world stays as it is here; saving keeps that file as `{}`.",
                    chain(&err),
                    project::KEPT_WORLD_FILE
                ));
            }
        }
    }

    /// The script file at `path` was made, changed or deleted.
    fn script_file_changed(&mut self, path: &str) {
        if project::is_block_script(path) {
            self.block_script_file_changed(path);
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let known = project
            .scripts
            .iter()
            .find(|script| script.path == path)
            .map(|script| script.source.clone());
        if !project.folder.join(path).exists() {
            if known.is_some() {
                self.remove_script(path);
                self.output
                    .info(format!("`{path}` was deleted outside the editor."));
                self.sync_tab(path, None);
            }
            return;
        }
        let source = match project::read_script(&project.folder, path) {
            Ok(source) => source,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged file: {}", chain(&err)));
                return;
            }
        };
        match known {
            // Saved by the editor itself, or touched without a change.
            Some(known) if known == source => return,
            Some(_) => {
                if let Some(project) = &mut self.project
                    && let Some(script) = project.scripts.iter_mut().find(|s| s.path == path)
                {
                    script.source = source.clone();
                }
                self.output
                    .info(format!("`{path}` was changed outside the editor."));
            }
            None => {
                self.add_script(path, source.clone());
                self.output.info(format!("Found a new script, `{path}`."));
            }
        }
        self.sync_tab(path, Some(&source));
    }

    fn block_script_file_changed(&mut self, path: &str) {
        let Some((folder, old_script)) = self.project.as_ref().map(|project| {
            (
                project.folder.clone(),
                project
                    .block_scripts
                    .iter()
                    .find(|document| document.path == path)
                    .map(|document| (document.script.clone(), document.saved.clone())),
            )
        }) else {
            return;
        };
        if !folder.join(path).exists() {
            if let Some((script, saved)) = old_script {
                if script != saved {
                    self.remove_script_instance(path);
                } else {
                    self.remove_script(path);
                }
                self.output
                    .info(format!("`{path}` was deleted outside the editor."));
                self.sync_tab(path, None);
            }
            return;
        }

        let script = match project::read_block_script(&folder, path) {
            Ok(script) => script,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged file: {}", chain(&err)));
                return;
            }
        };
        let translation = match translate(&script) {
            Ok(translation) => translation,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged block script `{path}`: {err}"));
                return;
            }
        };
        if old_script
            .as_ref()
            .is_some_and(|(_, saved)| *saved == script)
        {
            return;
        }

        let source = translation.source;
        let was_dirty = old_script
            .as_ref()
            .is_some_and(|(current, saved)| current != saved);
        match old_script {
            Some(_) if was_dirty => {
                if let Some(project) = &mut self.project {
                    if let Some(document) = project
                        .block_scripts
                        .iter_mut()
                        .find(|document| document.path == path)
                    {
                        document.saved = script;
                    }
                    if let Some(file) = project.scripts.iter_mut().find(|file| file.path == path) {
                        file.source = source.clone();
                    }
                }
                self.output.info(format!(
                    "`{path}` changed outside the editor; your unsaved blocks are kept."
                ));
            }
            Some(_) => {
                if let Some(project) = &mut self.project {
                    if let Some(document) = project
                        .block_scripts
                        .iter_mut()
                        .find(|document| document.path == path)
                    {
                        document.script = script.clone();
                        document.saved = script;
                    }
                    if let Some(file) = project.scripts.iter_mut().find(|file| file.path == path) {
                        file.source = source.clone();
                    }
                }
                self.output
                    .info(format!("`{path}` was changed outside the editor."));
            }
            None => {
                let id = self.add_script(path, source.clone());
                if let Some(project) = &mut self.project {
                    project.block_scripts.push(BlockDocument {
                        instance: id,
                        path: path.to_owned(),
                        script: script.clone(),
                        saved: script,
                        highlighted: None,
                    });
                }
                self.output
                    .info(format!("Found a new block script, `{path}`."));
            }
        }
        self.sync_tab(path, Some(&source));
    }

    /// Brings the tab for `path`, if open, up to date with its file, which
    /// now has `on_disk` in it (`None`: deleted). A tab with unsaved changes
    /// keeps them, and says the file changed.
    fn sync_tab(&mut self, path: &str, on_disk: Option<&str>) {
        let Some(tab) = self.tabs.get_mut(path) else {
            return;
        };
        let had_changes = tab.unsaved();
        match on_disk {
            None if had_changes => tab.disk = Disk::Deleted,
            None => self.tabs.close(path),
            Some(source) => {
                tab.saved = source.to_string();
                if had_changes && tab.text != source {
                    tab.disk = Disk::Changed;
                    self.output.error(format!(
                        "Your unsaved changes to `{path}` are kept in its tab. Saving replaces \
                         the other version; Reload from disk in the tab gets it instead."
                    ));
                } else {
                    tab.text = source.to_string();
                    tab.disk = Disk::Same;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
