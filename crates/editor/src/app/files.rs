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

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use axcenity_assets::Pick;
use axcenity_core::{ClassName, InstanceId, Value, Vector3};
use axcenity_project::{self as project, LoadError, Project, ScriptFile};
use eframe::egui;

use super::EditorApp;
use crate::history::{self, Move, Place};
use crate::import_dialog::{Answer, ImportDialog, Placement};
use crate::output::Output;
use crate::properties::MIN_SIZE;
use crate::scripts::{Disk, new_script_source};
use crate::surface::{self, MapFile};
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
    pub(super) fn pick_texture(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let file = rfd::FileDialog::new()
            .set_title("Choose a texture")
            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
            .set_directory(&project.folder)
            .pick_file()?;
        match axcenity_assets::import_texture(
            &project.folder,
            &file,
            axcenity_assets::PROJECT_TEXTURES_FOLDER,
        ) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
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
        let (objects, model) = match read {
            Ok(read) => read,
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not import `{}`: {err}", file.display()));
                return;
            }
        };
        let materials = surface::materials_maps(model.primitives.iter().map(|p| &p.material));
        let dialog = ImportDialog::new(file.to_owned(), objects, materials);
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
                self.import_chosen(&dialog);
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
    fn new_mesh_part(
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
    fn place_new(&mut self, parent: InstanceId, ids: Vec<InstanceId>, label: String) {
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
    use crate::surface::Slot;
    use axcenity_assets::Channel;

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
        // It has a texture, so its maps are shown first.
        let dialog = app.importing.take().expect("the dialog is open");
        assert!(!dialog.pick_objects);
        app.import_chosen(&dialog);

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

    /// A glTF file in `folder` holding three copies of the example pyramid
    /// (4 wide): `Pyramid` at the origin, `Street/Big` 10 along X inside
    /// an empty `Street` node, and `Small` 10 along -Z.
    fn write_props(folder: &Path) -> std::path::PathBuf {
        let text = fs::read_to_string(example_model()).unwrap();
        let mut gltf: serde_json::Value = serde_json::from_str(&text).unwrap();
        gltf["scenes"][0]["nodes"] = serde_json::json!([0, 1, 3]);
        gltf["nodes"] = serde_json::json!([
            { "name": "Pyramid", "mesh": 0 },
            { "name": "Street", "children": [2] },
            { "name": "Big", "mesh": 0, "translation": [10, 0, 0] },
            { "name": "Small", "mesh": 0, "translation": [0, 0, -10] },
        ]);
        let file = folder.join("props.gltf");
        fs::write(&file, gltf.to_string()).unwrap();
        file
    }

    /// Opens the Import Model dialog for `file`, unticks `Small`, and
    /// imports the rest as `placement` says.
    fn import_without_small(app: &mut EditorApp, file: &Path, placement: Placement) {
        app.import_model(file);
        let mut dialog = app.importing.take().expect("several objects ask first");
        let small = dialog
            .objects
            .iter()
            .position(|object| object.name == "Small")
            .unwrap();
        dialog.set(small, false);
        dialog.placement = placement;
        app.import_chosen(&dialog);
    }

    fn position(app: &EditorApp, id: InstanceId) -> Vector3 {
        match app.world.get(id).unwrap().property("Position") {
            Some(Value::Vector3(position)) => *position,
            other => panic!("{other:?}"),
        }
    }

    fn mesh_id(app: &EditorApp, id: InstanceId) -> String {
        match app.world.get(id).unwrap().property("MeshId") {
            Some(Value::String(mesh_id)) => mesh_id.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn each_material_gets_a_surface_appearance_with_its_maps_as_changed_in_the_dialog() {
        let folder = copy_of_example("hello-part", "import-surface");
        let mut app = EditorApp::new(Some(&folder));
        app.import_model(&example_model());
        let mut dialog = app.importing.take().expect("the dialog is open");
        let maps = &mut dialog.materials[0];
        assert_eq!(maps.material, "Sandstone");
        // A roughness map picked from outside the project, read from green.
        let picked = folder.join("..").join("import-surface-rough.png");
        fs::write(&picked, b"not checked until drawn").unwrap();
        maps.set_file(Slot::Roughness, Some(MapFile::Picked(picked.clone())));
        *maps.channel_mut(Slot::Roughness).unwrap() = Channel::Green;
        app.import_chosen(&dialog);

        let part = app.selected.unwrap();
        let surface = app.world.find_first_child(part, "Sandstone").unwrap();
        let surface = app.world.get(surface).unwrap();
        assert_eq!(surface.class(), ClassName::SurfaceAppearance);
        let path = |name| {
            surface
                .property(name)
                .and_then(Value::as_str)
                .unwrap()
                .to_owned()
        };
        assert_eq!(path("MaterialName"), "Sandstone");
        // The model's own texture, copied out of the file into its folder.
        let color = path("ColorMap");
        assert!(
            color.starts_with("assets/models/pyramid/textures/"),
            "{color}"
        );
        assert!(folder.join(&color).is_file());
        // The picked one, copied in beside it.
        assert_eq!(
            path("RoughnessMap"),
            "assets/models/pyramid/textures/import-surface-rough.png"
        );
        assert!(folder.join(path("RoughnessMap")).is_file());
        assert_eq!(
            surface.property("RoughnessChannel"),
            Some(&Value::TextureChannel(axcenity_core::TextureChannel::Green))
        );
        assert_eq!(path("NormalMap"), "");

        // Saved and opened again, it's still there; undo takes it out with the part.
        assert!(app.save());
        let reopened = EditorApp::new(Some(&folder));
        let again = part_named(&reopened, "pyramid");
        assert!(
            reopened
                .world
                .find_first_child(again, "Sandstone")
                .is_some()
        );
        app.undo();
        assert!(
            app.world
                .get(app.world.workspace())
                .unwrap()
                .children()
                .iter()
                .all(|&id| { app.world.get(id).unwrap().name() != "pyramid" })
        );
        let _ = fs::remove_file(&picked);
        let _ = fs::remove_dir_all(&folder);
    }

    fn part_named(app: &EditorApp, name: &str) -> InstanceId {
        app.world
            .find_first_child(app.world.workspace(), name)
            .unwrap()
    }

    #[test]
    fn a_file_with_several_objects_asks_first_and_copies_nothing_yet() {
        let folder = copy_of_example("hello-part", "import-ask");
        let file = write_props(&folder);
        let mut app = EditorApp::new(Some(&folder));
        app.import_model(&file);
        let dialog = app.importing.as_ref().expect("the dialog is open");
        let names: Vec<&str> = dialog.objects.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Pyramid", "Street", "Big", "Small"]);
        assert!(!folder.join(axcenity_assets::MODELS_FOLDER).exists());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn chosen_objects_import_as_a_model_laid_out_as_in_the_file() {
        let folder = copy_of_example("hello-part", "import-grouped");
        let file = write_props(&folder);
        let mut app = EditorApp::new(Some(&folder));
        import_without_small(&mut app, &file, Placement::Grouped);

        let workspace = app.world.workspace();
        let model = app.world.find_first_child(workspace, "props").unwrap();
        assert_eq!(app.selected, Some(model));
        assert_eq!(app.world.get(model).unwrap().class(), ClassName::Model);
        let pyramid = app.world.find_first_child(model, "Pyramid").unwrap();
        let street = app.world.find_first_child(model, "Street").unwrap();
        assert_eq!(app.world.get(street).unwrap().class(), ClassName::Model);
        let big = app.world.find_first_child(street, "Big").unwrap();
        assert!(app.world.find_first_child(model, "Small").is_none());
        assert_eq!(
            mesh_id(&app, big),
            "assets/models/props/props.gltf#Street/Big"
        );
        assert_eq!(
            mesh_id(&app, pyramid),
            "assets/models/props/props.gltf#Pyramid"
        );
        let (a, b) = (position(&app, pyramid), position(&app, big));
        assert!((b.x - a.x - 10.0).abs() < 1e-4 && (b.z - a.z).abs() < 1e-4);
        assert_eq!(
            app.world.get(big).unwrap().property("Size"),
            Some(&Value::Vector3(Vector3::new(4.0, 3.0, 4.0)))
        );

        // Saved and opened again, each part loads its own object.
        assert!(app.save());
        let reopened = EditorApp::new(Some(&folder));
        let model = reopened.world.find_first_child(workspace, "props").unwrap();
        let street = reopened.world.find_first_child(model, "Street").unwrap();
        let big = reopened.world.find_first_child(street, "Big").unwrap();
        let loaded = reopened.assets.model(&mesh_id(&reopened, big)).unwrap();
        assert!((loaded.center[0] - 10.0).abs() < 1e-4);
        assert!(reopened.assets.take_errors().is_empty());

        // One undo takes it all out.
        app.undo();
        assert!(app.world.find_first_child(workspace, "props").is_none());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn chosen_objects_can_import_as_separate_parts_or_one_merged_part() {
        let folder = copy_of_example("hello-part", "import-separate");
        let file = write_props(&folder);
        let mut app = EditorApp::new(Some(&folder));
        let workspace = app.world.workspace();
        let before = app.world.get(workspace).unwrap().children().len();

        import_without_small(&mut app, &file, Placement::Separate);
        let pyramid = app.world.find_first_child(workspace, "Pyramid").unwrap();
        let big = app.world.find_first_child(workspace, "Big").unwrap();
        assert_eq!(position(&app, pyramid), position(&app, big));
        assert_eq!(
            app.world.get(workspace).unwrap().children().len(),
            before + 2
        );
        app.undo();
        assert_eq!(app.world.get(workspace).unwrap().children().len(), before);

        import_without_small(&mut app, &file, Placement::Merged);
        let merged = app.world.find_first_child(workspace, "props").unwrap();
        // The second import's copy: the first is still in the project.
        assert_eq!(
            mesh_id(&app, merged),
            "assets/models/props2/props2.gltf#Pyramid,Street/Big"
        );
        // From the Pyramid's left edge (-2) to the Big one's right (12).
        assert_eq!(
            app.world.get(merged).unwrap().property("Size"),
            Some(&Value::Vector3(Vector3::new(14.0, 3.0, 4.0)))
        );
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
