//! The editor and the project folder: saving, script files (insert, open,
//! rename, delete), importing models, and reacting to files changed by
//! other programs.
//!
//! Each script is its `.server.lua` (Script), `.client.lua` (LocalScript)
//! or `.module.lua` (ModuleScript) file. Making, renaming or deleting a script changes its file straight
//! away and isn't an undo step; only the code typed in a tab waits for Save.
//!
//! These are more methods of [`EditorApp`]: a child module like this one can
//! use its private fields.

use std::io::ErrorKind;
use std::path::Path;

use axcenity_core::{ClassName, InstanceId, Value, Vector3};
use axcenity_project::{self as project, LoadError, Project, ScriptFile};
use eframe::egui;

use super::EditorApp;
use crate::output::Output;
use crate::properties::MIN_SIZE;
use crate::scripts::{Disk, new_script_source};
use crate::tree::{self, Insertable};

/// An error and its causes on one line, the way `{:#}` shows an `anyhow::Error`.
fn chain(err: &dyn std::error::Error) -> String {
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
    /// Writes the world file and every script tab with unsaved changes.
    /// Returns false (with the reason in Output) if nothing is open or a
    /// file couldn't be written.
    pub(super) fn save(&mut self) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        let name = project.name.clone();
        let scripts = self.tabs.unsaved_paths();
        // Save with nothing changed still writes the world file, as it
        // always has, but never over a world file the editor didn't load.
        let world_unsaved = !self.history.is_saved();
        let save_world = world_unsaved || (scripts.is_empty() && !project.keep_world_file);

        let mut saved = Vec::new();
        if save_world {
            if !self.save_world_file() {
                return false;
            }
            saved.push(project::WORLD_FILE.to_string());
        }
        for path in scripts {
            if !self.write_tab(&path) {
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
        }
        true
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
        if self.selected == Some(script.instance) {
            self.selected = None;
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

    /// File > Import Model… and the toolbar's Import: asks for a glTF model
    /// with the system's file picker, then imports it.
    pub(super) fn import_model_with_dialog(&mut self) {
        if self.project.is_none() || self.playing() || self.viewport.is_dragging() {
            return;
        }
        let dialog = rfd::FileDialog::new()
            .set_title("Import a 3D model")
            .add_filter("glTF model", &["gltf", "glb"]);
        // `None` means the dialog was cancelled.
        if let Some(file) = dialog.pick_file() {
            self.import_model(&file);
        }
    }

    /// Copies the model at `file` into the project's `assets/models` folder
    /// and inserts a MeshPart showing it, at its size in the file, inside
    /// the selected Model or Folder if there is one. The insert is an undo
    /// step; the copied file stays. A model that can't be loaded is reported
    /// in Output and not copied.
    pub(super) fn import_model(&mut self, file: &Path) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let (mesh_id, model) = match axcenity_assets::import_model(&project.folder, file) {
            Ok(imported) => imported,
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not import `{}`: {err}", file.display()));
                return;
            }
        };
        let model = self.assets.insert(&mesh_id, model);
        // A flat model (a plane) still needs some thickness to be seen and picked.
        let [x, y, z] = model.size.map(|size| size.max(MIN_SIZE));
        let name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim().to_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| ClassName::MeshPart.as_str().to_owned());
        self.output.info(format!(
            "Imported `{}` into the project as `{mesh_id}`.",
            file.display()
        ));
        let near = self.selected;
        self.insert_named(
            Insertable::MeshPart,
            near,
            &name,
            vec![
                ("MeshId", Value::String(mesh_id)),
                ("Size", Value::Vector3(Vector3::new(x, y, z))),
            ],
        );
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
        match project::rename_script(&project.folder, &old, name) {
            Ok(new) => {
                self.world.set_name(id, name).expect("it's a Script");
                if let Some(project) = &mut self.project
                    && let Some(script) = project.scripts.iter_mut().find(|s| s.instance == id)
                {
                    script.path = new.clone();
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
        let mut delete = None;
        let modal = egui::Modal::new(egui::Id::new("delete script")).show(ctx, |ui| {
            ui.heading(format!("Delete {}?", project::script_name(&path)));
            ui.label(format!(
                "This deletes `{path}` from the project folder. It can't be undone."
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    delete = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    delete = Some(false);
                }
            });
        });
        // Escape or a click outside the box means Cancel.
        if delete.is_none() && modal.should_close() {
            delete = Some(false);
        }
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
            } else {
                self.script_file_changed(path);
            }
        }
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
                // The new world has no Scripts yet: they come from the files.
                for script in &mut project.scripts {
                    script.instance = project::add_script_instance(&mut world, &script.path);
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
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use super::super::tests::{copy_of_example, last_line, part, play_for, printed};
    use super::*;
    use crate::app::{Answer, Discard};
    use crate::explorer::Action;
    use crate::output::OutputKind;
    use crate::properties::Edit;
    use crate::scripts::NEW_SCRIPT_SOURCE;

    fn script_names(app: &EditorApp) -> Vec<String> {
        let world = &app.world;
        world
            .get(world.server_script_service())
            .unwrap()
            .children()
            .iter()
            .map(|&id| world.get(id).unwrap().name().to_string())
            .collect()
    }

    fn script(app: &EditorApp, name: &str) -> InstanceId {
        app.world
            .find_first_child(app.world.server_script_service(), name)
            .unwrap()
    }

    fn example_model() -> std::path::PathBuf {
        super::super::tests::example("hello-mesh").join("assets/models/pyramid.gltf")
    }

    #[test]
    fn importing_a_model_copies_it_and_inserts_a_mesh_part_in_one_undo_step() {
        let folder = copy_of_example("hello-part", "import-model");
        let mut app = EditorApp::new(Some(&folder));
        app.import_model(&example_model());

        let id = app.selected.expect("the new MeshPart is selected");
        let mesh_part = app.world.get(id).unwrap();
        assert_eq!(mesh_part.class(), ClassName::MeshPart);
        assert_eq!(mesh_part.name(), "pyramid");
        let mesh_id = "assets/models/pyramid/pyramid.gltf";
        assert_eq!(
            mesh_part.property("MeshId"),
            Some(&Value::String(mesh_id.to_owned()))
        );
        assert_eq!(
            mesh_part.property("Size"),
            Some(&Value::Vector3(Vector3::new(4.0, 3.0, 4.0)))
        );
        assert!(folder.join(mesh_id).is_file());
        assert!(last_line(&app).text.contains(mesh_id));

        // It's drawn as its model, not as a stand-in box.
        let camera = axcenity_render::Camera {
            position: axcenity_render::glam::Vec3::new(0.0, 20.0, 30.0),
            look_at: axcenity_render::glam::Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
        assert!(
            list.items
                .iter()
                .any(|item| matches!(item.shape, axcenity_render::Shape::Mesh(_)))
        );

        // Saved and opened again, it's still there.
        assert!(app.save());
        let reopened = EditorApp::new(Some(&folder));
        let again = part(&reopened, "pyramid");
        assert_eq!(
            reopened.world.get(again).unwrap().property("MeshId"),
            Some(&Value::String(mesh_id.to_owned()))
        );

        // One undo takes it out again (the copied file stays).
        app.undo();
        let workspace = app.world.workspace();
        assert!(app.world.find_first_child(workspace, "pyramid").is_none());
        assert!(folder.join(mesh_id).is_file());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_broken_model_is_reported_and_nothing_is_inserted_or_copied() {
        let folder = copy_of_example("hello-part", "import-broken");
        let broken = folder.join("broken.glb");
        fs::write(&broken, "glTF, but not really").unwrap();
        let mut app = EditorApp::new(Some(&folder));
        let before = app.world.descendants(app.world.workspace()).len();

        app.import_model(&broken);
        assert_eq!(app.world.descendants(app.world.workspace()).len(), before);
        assert!(!folder.join(axcenity_assets::MODELS_FOLDER).exists());
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text.starts_with("Could not import")
                && line.text.contains("it isn't a valid glTF file"),
            "{}",
            line.text
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_mesh_part_whose_file_is_missing_is_reported_in_output() {
        let folder = copy_of_example("hello-mesh", "missing-model");
        fs::remove_file(folder.join("assets/models/pyramid.gltf")).unwrap();
        let app = EditorApp::new(Some(&folder));
        // Drawing asks for the model, which reports it once.
        let camera = axcenity_render::Camera {
            position: axcenity_render::glam::Vec3::new(0.0, 20.0, 30.0),
            look_at: axcenity_render::glam::Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
        let errors = app.assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("pyramid.gltf") && errors[0].contains("isn't there"));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn insert_script_makes_the_file_and_the_script_together_and_opens_it() {
        let folder = copy_of_example("hello-touched", "insert-script");
        let mut app = EditorApp::new(Some(&folder));
        app.act(Action::InsertScript(ClassName::Script));
        app.act(Action::InsertScript(ClassName::Script));

        assert_eq!(script_names(&app), ["Script", "Script2", "pad"]);
        let path = "scripts/Script2.server.lua";
        assert_eq!(
            fs::read_to_string(folder.join(path)).unwrap(),
            NEW_SCRIPT_SOURCE
        );
        assert_eq!(app.selected, Some(script(&app, "Script2")));
        assert_eq!(app.tabs.active().unwrap().path, path);
        // The file is already there, so there's nothing to save.
        assert!(!app.unsaved());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn editing_and_saving_a_script_writes_its_file() {
        let folder = copy_of_example("hello-touched", "edit-script");
        let mut app = EditorApp::new(Some(&folder));
        app.act(Action::InsertScript(ClassName::Script));
        let path = "scripts/Script.server.lua";
        app.tabs.get_mut(path).unwrap().text = "print('edited here')\n".to_string();
        assert!(app.unsaved());
        assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");

        assert!(app.save());
        assert!(!app.unsaved());
        assert_eq!(
            fs::read_to_string(folder.join(path)).unwrap(),
            "print('edited here')\n"
        );
        assert!(
            last_line(&app).text.contains(path),
            "{}",
            last_line(&app).text
        );
        // The editor's own save is not an outside change.
        app.disk_changed(&[path.to_string()]);
        assert_eq!(app.tabs.get(path).unwrap().disk, Disk::Same);

        // Play runs the saved code.
        app.play();
        play_for(&mut app, 0.1);
        assert!(printed(&app).contains(&"edited here"));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn play_saves_script_tabs_first() {
        let folder = copy_of_example("hello-touched", "play-saves-scripts");
        let mut app = EditorApp::new(Some(&folder));
        app.open_script_instance(script(&app, "pad"));
        app.tabs.get_mut("scripts/pad.server.lua").unwrap().text =
            "print('unsaved edit')".to_string();
        app.play();
        play_for(&mut app, 0.1);
        assert!(printed(&app).contains(&"unsaved edit"));
        assert_eq!(
            fs::read_to_string(folder.join("scripts/pad.server.lua")).unwrap(),
            "print('unsaved edit')"
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn closing_a_tab_with_unsaved_changes_asks_first() {
        let folder = copy_of_example("hello-touched", "close-tab");
        let mut app = EditorApp::new(Some(&folder));
        let path = "scripts/pad.server.lua".to_string();
        app.open_script(&path, None);
        let close = Discard::CloseTab(path.clone());
        assert_eq!(app.request(close.clone()), Some(close.clone()));

        app.tabs.get_mut(&path).unwrap().text.push_str("\n-- more");
        assert_eq!(app.request(close.clone()), None);
        // Saving in the prompt saves only that script, then closes it.
        app.apply(part(&app, "Floor"), Edit::Name("Ground2".to_string()));
        assert_eq!(app.answer(Answer::Save), Some(close));
        assert!(
            fs::read_to_string(folder.join(&path))
                .unwrap()
                .ends_with("-- more")
        );
        assert!(!app.history.is_saved(), "the world wasn't saved");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn renaming_a_script_renames_its_file_and_bad_names_are_refused() {
        let folder = copy_of_example("hello-touched", "rename-script");
        let mut app = EditorApp::new(Some(&folder));
        let pad = script(&app, "pad");
        app.open_script_instance(pad);
        app.act(Action::Rename(pad, "launcher".to_string()));

        assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
        assert!(folder.join("scripts/launcher.server.lua").exists());
        assert!(!folder.join("scripts/pad.server.lua").exists());
        assert_eq!(
            app.tabs.active().unwrap().path,
            "scripts/launcher.server.lua"
        );
        assert!(!app.unsaved());

        app.act(Action::Rename(pad, "a/b".to_string()));
        assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
        assert_eq!(last_line(&app).kind, OutputKind::Error);
        // The Properties panel's Name box doesn't rename Scripts.
        app.apply(pad, Edit::Name("other".to_string()));
        assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn deleting_a_script_asks_then_deletes_its_file_and_closes_its_tab() {
        let folder = copy_of_example("hello-touched", "delete-script");
        let mut app = EditorApp::new(Some(&folder));
        let pad = script(&app, "pad");
        app.open_script_instance(pad);
        app.act(Action::Delete(pad));
        // Nothing happens until "Delete script?" is answered.
        assert_eq!(app.confirm_delete, Some(pad));
        assert!(folder.join("scripts/pad.server.lua").exists());

        app.confirm_delete = None;
        app.delete_script(pad);
        assert!(!folder.join("scripts/pad.server.lua").exists());
        assert!(script_names(&app).is_empty());
        assert!(app.tabs.world_shown());
        assert!(app.project.as_ref().unwrap().scripts.is_empty());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn outside_edits_update_a_tab_or_keep_unsaved_changes() {
        let folder = copy_of_example("hello-touched", "outside-edit");
        let mut app = EditorApp::new(Some(&folder));
        let path = "scripts/pad.server.lua";
        app.open_script(path, None);

        fs::write(folder.join(path), "print('from VS Code')").unwrap();
        app.disk_changed(&[path.to_string()]);
        let tab = app.tabs.get(path).unwrap();
        assert_eq!(tab.text, "print('from VS Code')");
        assert!(!tab.unsaved());

        // With unsaved changes in the tab, they stay.
        app.tabs.get_mut(path).unwrap().text = "print('mine')".to_string();
        fs::write(folder.join(path), "print('theirs')").unwrap();
        app.disk_changed(&[path.to_string()]);
        let tab = app.tabs.get(path).unwrap();
        assert_eq!(
            (tab.text.as_str(), tab.disk),
            ("print('mine')", Disk::Changed)
        );
        assert_eq!(tab.saved, "print('theirs')");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn new_and_deleted_script_files_add_and_remove_scripts() {
        let folder = copy_of_example("hello-touched", "outside-files");
        let mut app = EditorApp::new(Some(&folder));
        fs::write(folder.join("scripts/door.server.lua"), "print('door')").unwrap();
        app.disk_changed(&["scripts/door.server.lua".to_string()]);
        assert_eq!(script_names(&app), ["door", "pad"]);

        app.open_script("scripts/door.server.lua", None);
        fs::remove_file(folder.join("scripts/door.server.lua")).unwrap();
        app.disk_changed(&["scripts/door.server.lua".to_string()]);
        assert_eq!(script_names(&app), ["pad"]);
        assert!(app.tabs.world_shown(), "its unchanged tab closed");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_outside_change_to_the_world_file_reloads_it_unless_there_are_unsaved_changes() {
        let folder = copy_of_example("hello-touched", "outside-world");
        let mut app = EditorApp::new(Some(&folder));
        let mut other = axcenity_project::load(&folder).unwrap().world;
        let floor = other.find_first_child(other.workspace(), "Floor").unwrap();
        other.set_name(floor, "Ground").unwrap();
        axcenity_project::save_world(&folder, &other).unwrap();

        app.disk_changed(&[axcenity_project::WORLD_FILE.to_string()]);
        assert!(
            app.world
                .find_first_child(app.world.workspace(), "Ground")
                .is_some()
        );
        // Scripts are still there, from their files.
        assert_eq!(script_names(&app), ["pad"]);

        // With unsaved changes, the world here stays, and saving keeps the other file.
        app.apply(part(&app, "Pad"), Edit::Name("Pad2".to_string()));
        other.set_name(floor, "Lava").unwrap();
        axcenity_project::save_world(&folder, &other).unwrap();
        app.disk_changed(&[axcenity_project::WORLD_FILE.to_string()]);
        assert!(
            app.world
                .find_first_child(app.world.workspace(), "Lava")
                .is_none()
        );
        assert!(app.save());
        let kept = fs::read_to_string(folder.join(axcenity_project::KEPT_WORLD_FILE)).unwrap();
        assert!(kept.contains("Lava"));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_corrupted_script_is_reported_and_the_rest_of_the_project_still_loads() {
        let folder = copy_of_example("hello-touched", "broken-script");
        fs::write(folder.join("scripts/broken.server.lua"), b"print('\xFF')").unwrap();
        let mut app = EditorApp::new(Some(&folder));

        let errors: Vec<&str> = app
            .output
            .lines()
            .iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("broken.server.lua"), "{}", errors[0]);
        assert_eq!(script_names(&app), ["pad"]);
        assert!(
            app.world
                .find_first_child(app.world.workspace(), "Pad")
                .is_some()
        );

        // Fixed in another program, it loads.
        fs::write(folder.join("scripts/broken.server.lua"), "print('fixed')").unwrap();
        app.disk_changed(&["scripts/broken.server.lua".to_string()]);
        assert_eq!(script_names(&app), ["broken", "pad"]);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn saving_over_a_damaged_world_file_keeps_it() {
        let folder = copy_of_example("hello-touched", "damaged-world");
        fs::write(folder.join("world.json"), "{ damaged").unwrap();
        let mut app = EditorApp::new(Some(&folder));
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        assert_eq!(script_names(&app), ["pad"]);
        // Nothing changed: Save doesn't touch the damaged file.
        assert!(app.save());
        assert_eq!(
            fs::read_to_string(folder.join("world.json")).unwrap(),
            "{ damaged"
        );

        app.insert(crate::tree::Insertable::Model, None);
        assert!(app.save());
        let kept = fs::read_to_string(folder.join(axcenity_project::KEPT_WORLD_FILE));
        assert_eq!(kept.unwrap(), "{ damaged");
        assert!(axcenity_project::load(&folder).unwrap().world_loaded);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn clicking_a_script_error_opens_the_script_at_that_line() {
        let folder = copy_of_example("hello-touched", "error-link");
        fs::write(
            folder.join("scripts/pad.server.lua"),
            "print('one')\nprint('two')\nerror('three')\n",
        )
        .unwrap();
        let mut app = EditorApp::new(Some(&folder));
        app.play();
        play_for(&mut app, 0.1);
        app.stop();
        let error = app
            .output
            .lines()
            .iter()
            .find(|line| line.kind == OutputKind::Error)
            .unwrap();
        assert_eq!(error.text, "scripts/pad.server.lua:3: three");
        let location = error.location.clone().unwrap();

        // What the Output panel's click does.
        app.open_script(&location.path, Some(location.line));
        let tab = app.tabs.active().unwrap();
        assert_eq!(tab.path, "scripts/pad.server.lua");
        assert_eq!(tab.go_to_line(), Some(3));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_script_edited_in_another_program_updates_its_tab_within_a_second() {
        let folder = copy_of_example("hello-touched", "watch-live");
        let mut app = EditorApp::new(Some(&folder));
        let path = "scripts/pad.server.lua";
        app.open_script(path, None);
        // Let the watcher take its first look.
        std::thread::sleep(Duration::from_millis(100));

        fs::write(folder.join(path), "print('saved in VS Code')").unwrap();
        let written = Instant::now();
        while app.tabs.get(path).unwrap().text != "print('saved in VS Code')"
            && written.elapsed() < Duration::from_secs(3)
        {
            std::thread::sleep(Duration::from_millis(20));
            app.check_disk();
        }
        let took = written.elapsed();
        let text = app.tabs.get(path).unwrap().text.clone();
        drop(app);
        let _ = fs::remove_dir_all(&folder);

        assert_eq!(text, "print('saved in VS Code')");
        assert!(took < Duration::from_secs(1), "took {took:?}");
    }

    #[test]
    fn script_changes_wait_while_playing() {
        let folder = copy_of_example("hello-touched", "watch-play");
        let mut app = EditorApp::new(Some(&folder));
        std::thread::sleep(Duration::from_millis(100));
        app.play();
        play_for(&mut app, 0.0);
        fs::write(folder.join("scripts/door.server.lua"), "print('door')").unwrap();
        // Long enough for the watcher to notice.
        std::thread::sleep(Duration::from_millis(1200));
        app.check_disk();
        assert_eq!(script_names(&app), ["pad"]);

        app.stop();
        let stopped = Instant::now();
        while script_names(&app).len() == 1 && stopped.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
            app.check_disk();
        }
        let names = script_names(&app);
        drop(app);
        let _ = fs::remove_dir_all(&folder);
        assert_eq!(names, ["door", "pad"]);
    }

    /// The names of the Instances in `service`, in order.
    fn names_in(app: &EditorApp, service: InstanceId) -> Vec<String> {
        let world = &app.world;
        world
            .get(service)
            .unwrap()
            .children()
            .iter()
            .map(|&id| world.get(id).unwrap().name().to_string())
            .collect()
    }

    #[test]
    fn insert_local_script_makes_a_client_file_in_starter_player_scripts() {
        let folder = copy_of_example("hello-touched", "insert-local-script");
        let mut app = EditorApp::new(Some(&folder));
        app.act(Action::InsertScript(ClassName::LocalScript));

        let path = "scripts/LocalScript.client.lua";
        assert_eq!(
            fs::read_to_string(folder.join(path)).unwrap(),
            NEW_SCRIPT_SOURCE
        );
        let service = app.world.starter_player_scripts();
        assert_eq!(names_in(&app, service), ["LocalScript"]);
        let id = app.world.find_first_child(service, "LocalScript").unwrap();
        assert_eq!(app.world.get(id).unwrap().class(), ClassName::LocalScript);
        assert_eq!(app.tabs.active().unwrap().path, path);
        // Server scripts still start first.
        let paths: Vec<&str> = app
            .project
            .as_ref()
            .unwrap()
            .scripts
            .iter()
            .map(|s| s.path.as_str())
            .collect();
        assert_eq!(paths, ["scripts/pad.server.lua", path]);

        // Renaming keeps it a LocalScript.
        app.act(Action::Rename(id, "Keys".to_string()));
        assert!(folder.join("scripts/Keys.client.lua").exists());
        // A client file made elsewhere becomes a LocalScript too.
        fs::write(folder.join("scripts/Camera.client.lua"), "print(1)").unwrap();
        app.disk_changed(&["scripts/Camera.client.lua".to_string()]);
        assert_eq!(names_in(&app, service), ["Camera", "Keys"]);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn insert_module_script_makes_a_module_file_in_replicated_storage() {
        let folder = copy_of_example("hello-touched", "insert-module-script");
        let mut app = EditorApp::new(Some(&folder));
        let what = crate::tree::Insertable::RemoteEvent;
        app.act(Action::Insert { what, near: None });
        app.act(Action::InsertScript(ClassName::ModuleScript));

        let path = "scripts/ModuleScript.module.lua";
        assert_eq!(
            fs::read_to_string(folder.join(path)).unwrap(),
            "local module = {}\n\nreturn module\n"
        );
        let service = app.world.replicated_storage();
        // After the saved things, as it would be after loading.
        assert_eq!(names_in(&app, service), ["RemoteEvent", "ModuleScript"]);
        let id = app.world.find_first_child(service, "ModuleScript").unwrap();
        assert_eq!(app.world.get(id).unwrap().class(), ClassName::ModuleScript);
        // Modules are ready before any script starts.
        let paths: Vec<&str> = app
            .project
            .as_ref()
            .unwrap()
            .scripts
            .iter()
            .map(|s| s.path.as_str())
            .collect();
        assert_eq!(paths, [path, "scripts/pad.server.lua"]);

        // Nothing can be put inside it: it couldn't be saved there.
        let part = app.world.create(ClassName::Part).unwrap();
        app.world
            .set_parent(part, Some(app.world.workspace()))
            .unwrap();
        assert!(tree::check_move(&app.world, part, id).is_err());

        // It's a file, not part of the world file.
        assert!(app.save());
        let world_file = fs::read_to_string(folder.join(project::WORLD_FILE)).unwrap();
        assert!(world_file.contains("RemoteEvent") && !world_file.contains("ModuleScript"));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn remote_events_are_inserted_into_replicated_storage_and_saved() {
        let folder = copy_of_example("hello-touched", "insert-remote");
        let mut app = EditorApp::new(Some(&folder));
        let what = crate::tree::Insertable::RemoteEvent;
        app.act(Action::Insert { what, near: None });
        let storage = app.world.replicated_storage();
        assert_eq!(names_in(&app, storage), ["RemoteEvent"]);
        assert!(app.save());
        let saved = axcenity_project::load(&folder).unwrap();
        let world = &saved.world;
        let remote = world
            .find_first_child(world.replicated_storage(), "RemoteEvent")
            .unwrap();
        assert_eq!(world.get(remote).unwrap().class(), ClassName::RemoteEvent);
        // It's an ordinary undo step.
        app.undo();
        assert_eq!(names_in(&app, storage), Vec::<String>::new());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_script_using_input_gets_a_warning_badge_until_it_is_fixed() {
        let folder = copy_of_example("hello-touched", "side-warning");
        let mut app = EditorApp::new(Some(&folder));
        let pad = script(&app, "pad");
        app.update_warnings();
        assert!(app.explorer.warnings.is_empty());

        // Typed in the tab, before saving: the badge shows straight away.
        app.open_script_instance(pad);
        let path = "scripts/pad.server.lua";
        app.tabs.get_mut(path).unwrap().text =
            "print('hi')\nlocal uis = game:GetService('UserInputService')\n".to_string();
        app.update_warnings();
        let badge = &app.explorer.warnings[&pad];
        assert!(
            badge.starts_with("Line 2: UserInputService only works in a LocalScript"),
            "{badge}"
        );
        assert_eq!(app.tabs.warnings[path][0].line, 2);

        app.tabs.get_mut(path).unwrap().text = "print('hi')\n".to_string();
        app.update_warnings();
        assert!(app.explorer.warnings.is_empty() && app.tabs.warnings.is_empty());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn play_runs_hello_remote_and_held_arrow_keys_move_the_block() {
        let folder = copy_of_example("hello-remote", "play-remote");
        let mut app = EditorApp::new(Some(&folder));
        let mover = part(&app, "Mover");
        let start = app.world.get(mover).unwrap().property("Position").cloned();
        app.play();
        let right = axcenity_core::KeyCode::Right;
        let key = |pressed| axcenity_script::KeyEvent {
            key: right,
            pressed,
        };
        app.play_frame(1.0 / 60.0, &[key(true)]);
        play_for(&mut app, 0.5);
        app.play_frame(1.0 / 60.0, &[key(false)]);

        let world = app.play.as_ref().unwrap().world();
        let Some(axcenity_core::Value::Vector3(moved)) =
            world.get(mover).unwrap().property("Position").cloned()
        else {
            panic!("the block has no Position");
        };
        assert!(moved.x > 5.0, "{moved:?}");
        assert!(
            printed(&app).contains(&"Hold the arrow keys to move the blue block (Space for help)."),
            "{:?}",
            printed(&app)
        );
        app.stop();
        assert_eq!(
            app.world.get(mover).unwrap().property("Position").cloned(),
            start,
            "Stop puts it back"
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
