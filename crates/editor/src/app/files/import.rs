use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use super::*;
use crate::history::BlobChange;

/// Every file under the project's `assets` folder, where imports copy and
/// write files, as paths in the project with `/`.
pub(super) fn asset_files(project: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut folders = vec![project.join("assets")];
    while let Some(at) = folders.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if let Ok(inside) = path.strip_prefix(project) {
                let parts: Vec<String> = inside
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.insert(parts.join("/"));
            }
        }
    }
    found
}

/// The files under the project's `assets` folder that aren't in `before`
/// (from [`asset_files`]), as made by an import, with their bytes so redo
/// can put them back. An import only makes new files (every copy and clip
/// gets a free name), so files that were there stay out of its step.
pub(super) fn files_made_since(project: &Path, before: &BTreeSet<String>) -> Vec<BlobChange> {
    asset_files(project)
        .difference(before)
        .filter_map(|path| {
            let bytes = fs::read(project.join(path)).ok()?;
            Some(BlobChange {
                path: path.clone(),
                before: None,
                after: Some(Arc::new(bytes)),
            })
        })
        .collect()
}

/// The properties of a MeshPart showing `model` from `mesh_id`: it starts
/// at the model's size in the file. A flat model (a plane) still gets some
/// thickness, to be seen and picked.
pub(super) fn mesh_part_properties(
    mesh_id: String,
    model: &axcenity_assets::Model,
) -> Vec<(&'static str, Value)> {
    let [x, y, z] = model.size.map(|size| size.max(MIN_SIZE));
    vec![
        ("MeshId", Value::String(mesh_id)),
        ("Size", Value::Vector3(Vector3::new(x, y, z))),
    ]
}

/// True if `file` is an asset package (`.axpkg`).
pub(in crate::app) fn is_package(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(project::PACKAGE_EXTENSION))
}

/// An object's name from a model file as an Instance name.
pub(super) fn object_name(name: &str) -> String {
    match name.trim() {
        "" => ClassName::MeshPart.as_str().to_owned(),
        name => name.to_owned(),
    }
}

impl EditorApp {
    /// Import…, the one way in from the File menu, the ribbon, the Assets
    /// panel and the Animation tab: asks for files of any kind it can
    /// import with the system's file picker, then imports them
    /// ([`import_files`](Self::import_files)). Images go in `image_folder`.
    pub(in crate::app) fn import_with_dialog(&mut self, image_folder: &str) {
        if self.project.is_none() || self.playing() || self.viewport.is_dragging() {
            return;
        }
        const MODELS: [&str; 3] = ["gltf", "glb", "fbx"];
        let mut everything: Vec<&str> = MODELS.to_vec();
        everything.extend(axcenity_assets::TEXTURE_EXTENSIONS);
        everything.extend(axcenity_assets::AUDIO_EXTENSIONS);
        everything.push("hdr");
        everything.push("json");
        everything.push(project::PACKAGE_EXTENSION);
        let dialog = rfd::FileDialog::new()
            .set_title("Import")
            .add_filter(
                "Models, images, skies, sounds, prefabs and packages",
                &everything,
            )
            .add_filter("3D model", &MODELS)
            .add_filter("Image", &axcenity_assets::TEXTURE_EXTENSIONS)
            .add_filter("Sky (HDR)", &["hdr"])
            .add_filter("Sound", &axcenity_assets::AUDIO_EXTENSIONS)
            .add_filter("Prefab", &["json"])
            .add_filter("Asset package", &[project::PACKAGE_EXTENSION]);
        // `None` means the dialog was cancelled.
        if let Some(files) = dialog.pick_files() {
            self.import_files(files, image_folder);
        }
    }

    /// Imports each of `files` by its kind: a model or an asset package
    /// through the Import dialog (one at a time, in order), and images (into `image_folder`),
    /// sounds and prefabs straight into the project, reported in the
    /// Console.
    pub(in crate::app) fn import_files(&mut self, files: Vec<PathBuf>, image_folder: &str) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        for file in files {
            self.import_file(&folder, &file, image_folder);
        }
        self.asset_browser.refresh();
    }

    /// Files dropped onto the window from the system: imported as with
    /// Import…, images into `assets/textures`.
    pub(in crate::app) fn import_dropped(&mut self, ctx: &egui::Context) {
        let files: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if files.is_empty() {
            return;
        }
        if self.project.is_none() {
            self.start
                .set_notice("Open a project, then drop files on it to import them.".to_owned());
        } else if self.playing() {
            self.output
                .error("Press Stop before importing files.".to_owned());
        } else {
            self.import_files(files, axcenity_assets::PROJECT_TEXTURES_FOLDER);
        }
    }

    /// Imports the model at `file`. A file with more than one object, or
    /// with texture maps, asks first which objects to import and shows its
    /// materials' maps for changing ([`ImportDialog`]); any other goes
    /// straight in as a MeshPart. A file that can't be read is reported in
    /// Output. While the dialog is open for another file, this one waits
    /// its turn.
    pub(in crate::app) fn import_model(&mut self, file: &Path) {
        if self.project.is_none() || self.playing() || self.viewport.is_dragging() {
            return;
        }
        if self.importing.is_some() {
            self.import_queue.push_back(file.to_owned());
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
            // The model already loaded is what the dialog's preview draws.
            self.importing = Some(dialog.with_preview(model, takes));
        } else {
            self.import_chosen(&dialog);
        }
    }

    /// Shows the Import dialog while a file is waiting for it, and imports
    /// what it says, then goes on to the next model waiting. `render_state`
    /// draws its preview.
    pub(in crate::app) fn import_prompt(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
    ) {
        let Some(dialog) = &mut self.importing else {
            return;
        };
        match dialog.show(ctx, render_state) {
            Some(Answer::Import) => {
                let dialog = self.importing.take().expect("checked above");
                if dialog.package.is_some() {
                    self.import_package_chosen(&dialog);
                } else if dialog.animation_only {
                    self.import_animation_only(&dialog);
                } else {
                    self.import_chosen(&dialog);
                }
            }
            Some(Answer::Cancel) => self.importing = None,
            None => return,
        }
        self.next_import();
    }

    /// Imports the models and packages waiting their turn, until one opens
    /// the dialog again (some models go straight in).
    pub(in crate::app) fn next_import(&mut self) {
        while self.importing.is_none()
            && let Some(file) = self.import_queue.pop_front()
        {
            if is_package(&file) {
                self.import_package(&file);
            } else {
                self.import_model(&file);
            }
        }
    }

    /// Copies the dialog's model file into the project's `assets/models`
    /// folder and inserts the objects ticked in it, as its placement says,
    /// inside the selected Model or Folder if there is one. Each MeshPart
    /// starts at its object's size in the file. The insert and the files
    /// it copied and wrote (the model, textures, clips and state machine)
    /// are one undo step. A model that can't be loaded is
    /// reported in Output and not copied; problems that didn't stop it
    /// loading (an FBX texture that isn't there) are reported after it's
    /// imported.
    pub(in crate::app) fn import_chosen(&mut self, dialog: &ImportDialog) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let before = asset_files(&project.folder);
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
                self.place_imported(parent, vec![id], format!("Import {file_name}"), &before);
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
                self.place_imported(parent, ids, format!("Import {file_name}"), &before);
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
                // Welded inside the Model, so they're in the same undo step.
                if dialog.weld {
                    crate::constraints::weld_model(&mut self.world, root);
                }
                self.place_imported(parent, vec![root], format!("Import {file_name}"), &before);
            }
        }
    }

    /// Writes the clips `animation` makes, from the model copied to
    /// `copy` in the project, and their state machine if an Animator was
    /// asked for. Returns the state machine's path and the clips', for the
    /// Animator. The import's undo step holds the files; a problem is
    /// reported in Output.
    pub(super) fn write_import_animation(
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
    pub(super) fn add_animator(
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
    /// writes the clips the dialog made from its takes, as one undo step.
    pub(in crate::app) fn import_animation_only(&mut self, dialog: &ImportDialog) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let Some(animation) = &dialog.animation else {
            return;
        };
        let before = asset_files(&folder);
        match axcenity_assets::import_animation_file(&folder, &dialog.file) {
            Ok(copy) => {
                self.output.info(format!(
                    "Imported the animation `{}` into the project as `{copy}`.",
                    dialog.file.display()
                ));
                let mut clips = animation.clone();
                clips.add_animator = false;
                self.write_import_animation(&clips, &copy);
                let name = dialog
                    .file
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let made = files_made_since(&folder, &before);
                self.history
                    .record_blobs(&mut self.world, format!("Import {name}"), made);
                self.animation.refresh();
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
    pub(super) fn add_surfaces(
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
    pub(in crate::app) fn new_mesh_part(
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
    pub(super) fn group_of(
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

    /// [`place_new`](Self::place_new) for an import: the files it made
    /// since `before` (from [`asset_files`]) go in the same undo step.
    fn place_imported(
        &mut self,
        parent: InstanceId,
        ids: Vec<InstanceId>,
        label: String,
        before: &BTreeSet<String>,
    ) {
        let made = self
            .project
            .as_ref()
            .map(|project| files_made_since(&project.folder, before))
            .unwrap_or_default();
        self.place_new_with_files(parent, ids, label, made);
        self.animation.refresh();
    }

    /// Puts the new Instances `ids` at the end of `parent`, each with a
    /// name none of its siblings has, as one undo step named `label`, and
    /// selects the last of them.
    pub(in crate::app) fn place_new(
        &mut self,
        parent: InstanceId,
        ids: Vec<InstanceId>,
        label: String,
    ) {
        self.place_new_with_files(parent, ids, label, Vec::new());
    }

    /// [`place_new`](Self::place_new), with files already written (`blobs`)
    /// in the same undo step.
    fn place_new_with_files(
        &mut self,
        parent: InstanceId,
        ids: Vec<InstanceId>,
        label: String,
        blobs: Vec<BlobChange>,
    ) {
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
        if blobs.is_empty() {
            self.history.record_tree(&mut self.world, label, moves);
        } else {
            self.history
                .record_blobs_and_tree(&mut self.world, label, blobs, moves);
        }
        if last.is_some() {
            self.select_from_viewport(last);
        }
    }
}
