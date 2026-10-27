//! The editor and the Assets panel: placing a dragged (or double-clicked)
//! asset in the world, saving the selection as a prefab, and importing
//! files into the project's `assets` folder. A global asset the project
//! hasn't got is copied into it first (see `global_assets.rs`).
//!
//! Every placement is one undo step. A model, prefab or prop is moved as a
//! whole so its bounding box rests on the surface it's dropped on. Saving,
//! renaming and duplicating a prefab is one undo step too, and so is
//! deleting any asset.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_project as project;
use axcenity_render::glam::Vec3;

use super::EditorApp;
use crate::asset_browser::{
    self, AssetEntry, AssetKind, PREFAB_SUFFIX, PREFABS_FOLDER, display_name, prefab_file_stem,
};
use crate::gizmo::Pose;
use crate::history::{BlobChange, Field};
use crate::layout::Pane;
use crate::properties::Edit;
use crate::tree::{self, Insertable};
use crate::viewport::DropTarget;

/// Where something is dropped: a point on a surface and the way it faces,
/// and what's there.
#[derive(Clone, Copy, Debug)]
pub(super) struct Spot {
    point: Vec3,
    normal: Vec3,
    part: Option<InstanceId>,
    ui: Option<InstanceId>,
}

impl From<DropTarget> for Spot {
    fn from(target: DropTarget) -> Self {
        Spot {
            point: target.point,
            normal: target.normal,
            part: target.part,
            ui: target.ui,
        }
    }
}

impl EditorApp {
    /// Uses the global assets in `library` in every project, and lists
    /// them on the start screen.
    pub fn set_global_assets(&mut self, library: Option<PathBuf>) {
        self.start.set_library(library.clone());
        self.asset_browser.set_library(library);
    }

    /// Copies the global asset at `path`, with what it needs, into the
    /// project if it isn't there yet. False (told in Output) if that fails.
    pub(super) fn use_global_asset(&mut self, path: &str) -> bool {
        match self.asset_browser.use_here(path) {
            Ok(0) => true,
            Ok(copied) => {
                let files = if copied == 1 { "file" } else { "files" };
                self.output.info(format!(
                    "Copied the global asset {} into this project ({copied} {files}).",
                    display_name(path)
                ));
                true
            }
            Err(problem) => {
                self.output
                    .error(format!("Could not copy the global asset: {problem}"));
                false
            }
        }
    }

    /// Places `entry` at `target`, or at the camera's focus point for `None`.
    pub(super) fn drop_asset(&mut self, entry: &AssetEntry, target: Option<DropTarget>) {
        if self.playing() || self.viewport.is_dragging() || self.project.is_none() {
            return;
        }
        if !self.use_global_asset(&entry.path) {
            return;
        }
        let spot = target.map(Spot::from).unwrap_or_else(|| {
            let focus = self.insert_position();
            Spot {
                point: Vec3::new(focus.x, focus.y, focus.z),
                normal: Vec3::Y,
                part: None,
                ui: None,
            }
        });
        match entry.kind {
            AssetKind::Prop | AssetKind::Prefab => self.drop_prefab(entry, spot),
            AssetKind::Model => self.drop_model(entry, spot),
            AssetKind::Audio => self.drop_sound(entry, spot),
            AssetKind::Image => self.drop_image(entry, spot),
            AssetKind::Texture => self.drop_texture(entry, spot),
            AssetKind::TextureSet => self.apply_texture_set(entry, spot.part),
            AssetKind::Material => self.apply_material(entry, spot.part),
            AssetKind::Particle => self.drop_particle_texture(entry, spot),
            AssetKind::Animation => self.open_animation_clip(&entry.path, true),
            AssetKind::Controller => self.drop_controller(entry, spot),
            AssetKind::BehaviorTree => self.drop_behavior_tree(&entry.path, spot.part),
            AssetKind::DataTable => self.open_data_table(&entry.path),
        }
    }

    /// Opens the clip at `path` in the Animation tab, and shows the tab.
    /// `alone` (from the Assets panel) opens it on its own, following no
    /// object; otherwise it stays with the object the tab follows.
    pub(super) fn open_animation_clip(&mut self, path: &str, alone: bool) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let (assets, world, output) = (&self.assets, &mut self.world, &mut self.output);
        if alone {
            self.animation
                .open_clip_alone(&folder, assets, path, world, output);
        } else {
            self.animation
                .open_clip(&folder, assets, path, world, output);
        }
        self.layout.open(Pane::Animation);
    }

    /// Opens the state machine of the Animator `animator` in the Animation
    /// tab, previewed on its MeshPart's model, and shows the tab.
    pub(super) fn open_animator_controller(&mut self, animator: InstanceId) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let world = super::shown(&self.world, &self.play);
        let Some(instance) = world.get(animator) else {
            return;
        };
        let path = instance
            .property("StateMachine")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let model = instance
            .parent()
            .and_then(|part| world.get(part))
            .and_then(|part| part.property("MeshId").and_then(Value::as_str))
            .map(str::to_owned);
        if path.is_empty() {
            return;
        }
        self.animation
            .open_controller(&folder, &self.assets, &path, model, &mut self.output);
        self.layout.open(Pane::Animation);
    }

    /// Gives the Animator of the part under the drop this state machine
    /// (adding an Animator if it has none), as one undo step.
    fn drop_controller(&mut self, entry: &AssetEntry, spot: Spot) {
        let part = spot.part.filter(|&part| {
            self.world.get(part).is_some_and(|instance| {
                matches!(instance.class(), ClassName::Part | ClassName::MeshPart)
            })
        });
        let Some(part) = part else {
            self.output
                .warning("Drop a state machine onto a part to animate it.");
            return;
        };
        let animator = self.world.get(part).and_then(|instance| {
            instance.children().iter().copied().find(|&child| {
                self.world
                    .get(child)
                    .is_some_and(|c| c.class() == ClassName::Animator)
            })
        });
        let controller = Value::String(entry.path.clone());
        match animator {
            Some(animator) => {
                let label = format!(
                    "Set State Machine of {}",
                    self.world.get(animator).map_or("Animator", |a| a.name())
                );
                self.change(
                    animator,
                    Field::Property("StateMachine"),
                    label,
                    vec![Edit::Property("StateMachine", controller)],
                );
                self.history.seal();
                self.select_from_viewport(Some(animator));
            }
            None => {
                let id = self
                    .world
                    .create(ClassName::Animator)
                    .expect("not a service");
                self.world
                    .set_property(id, "StateMachine", controller)
                    .expect("every Animator has a StateMachine");
                self.place_new(part, vec![id], format!("Animate {}", entry.name));
            }
        }
    }

    fn drop_prefab(&mut self, entry: &AssetEntry, spot: Spot) {
        let text = match entry.prop() {
            Some(prop) => prop.prefab.to_owned(),
            None => match self.read_project_file(&entry.path) {
                Some(text) => text,
                None => return,
            },
        };
        let workspace = self.world.workspace();
        let ids = match project::parse_prefab(&text, &mut self.world, workspace) {
            Ok(ids) => ids,
            Err(err) => {
                self.output
                    .error(format!("Could not place `{}`: {err}", entry.path));
                return;
            }
        };
        if ids.is_empty() {
            self.output
                .warning(format!("The prefab `{}` has nothing in it.", entry.path));
            return;
        }
        // Taken out again, so placing them is one undo step that puts them back.
        for &id in &ids {
            let _ = self.world.set_parent(id, None);
        }
        self.rest_on(&ids, spot);
        self.place_new(workspace, ids, format!("Place {}", entry.name));
    }

    fn drop_model(&mut self, entry: &AssetEntry, spot: Spot) {
        // A model that can't be loaded is reported by `Assets`, once.
        let Some(model) = self.assets.model(&entry.path) else {
            return;
        };
        let position = Vector3::new(spot.point.x, spot.point.y, spot.point.z);
        let id = self.new_mesh_part(entry.path.clone(), &model, position);
        let _ = self.world.set_name(id, display_name(&entry.path));
        self.rest_on(&[id], spot);
        let workspace = self.world.workspace();
        self.place_new(workspace, vec![id], format!("Place {}", entry.name));
    }

    fn drop_sound(&mut self, entry: &AssetEntry, spot: Spot) {
        let parent = spot
            .part
            .filter(|&id| is_part(&self.world, id))
            .unwrap_or(self.world.workspace());
        let id = self.world.create(ClassName::Sound).expect("not a service");
        let _ = self
            .world
            .set_property(id, "SoundId", Value::String(entry.path.clone()));
        let _ = self.world.set_name(id, display_name(&entry.path));
        self.place_new(parent, vec![id], format!("Add sound {}", entry.name));
    }

    fn drop_image(&mut self, entry: &AssetEntry, spot: Spot) {
        let image = Value::String(entry.path.clone());
        let has_image = |world: &World, id: InstanceId| {
            world
                .get(id)
                .is_some_and(|instance| instance.property("Image").is_some())
        };
        if let Some(id) = spot.ui.filter(|&id| has_image(&self.world, id)) {
            let name = self
                .world
                .get(id)
                .map(|i| i.name().to_owned())
                .unwrap_or_default();
            self.change(
                id,
                Field::Property("Image"),
                format!("Change Image of {name}"),
                vec![Edit::Property("Image", image)],
            );
            self.history.seal();
            return;
        }
        self.insert(Insertable::ImageLabel, None);
        // The new label is selected. It's the same Instance that undo takes
        // out and redo puts back, so its image comes back with it.
        if let Some(id) = self.selected.filter(|&id| has_image(&self.world, id)) {
            let _ = self.world.set_property(id, "Image", image);
            let parent = self
                .world
                .get(id)
                .and_then(|i| i.parent())
                .unwrap_or(self.world.root());
            let name = tree::unique_name(&self.world, parent, &display_name(&entry.path));
            let _ = self.world.set_name(id, name);
        }
    }

    fn drop_texture(&mut self, entry: &AssetEntry, spot: Spot) {
        let Some(part) = spot.part else {
            self.output.info(
                "Drop a texture onto a MeshPart to use it as its color map, or set it as a map \
                 of a SurfaceAppearance in Properties.",
            );
            return;
        };
        let Some(instance) = self.world.get(part) else {
            return;
        };
        if instance.class() != ClassName::MeshPart {
            self.output.info(
                "Textures show on MeshParts (imported models). A part's look comes from its \
                 Color and Material.",
            );
            return;
        }
        let name = instance.name().to_owned();
        let surfaces: Vec<InstanceId> = instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                self.world
                    .get(child)
                    .is_some_and(|c| c.class() == ClassName::SurfaceAppearance)
            })
            .collect();
        if surfaces.is_empty() {
            self.output.warning(format!(
                "{name} has no SurfaceAppearance to put the texture in. Importing a model makes \
                 one for each of its materials."
            ));
            return;
        }
        let edits = surfaces
            .into_iter()
            .map(|id| {
                let edit = Edit::Property("ColorMap", Value::String(entry.path.clone()));
                (id, vec![edit])
            })
            .collect();
        self.change_many(
            Field::Property("ColorMap"),
            format!("Texture {name}"),
            edits,
        );
        self.history.seal();
    }

    /// A particle texture dropped on a part: its emitters use it, or it
    /// gets a new emitter with it if it has none.
    fn drop_particle_texture(&mut self, entry: &AssetEntry, spot: Spot) {
        let Some(part) = spot.part.filter(|&id| is_part(&self.world, id)) else {
            self.output
                .info("Drop a particle texture onto a part to give it a ParticleEmitter.");
            return;
        };
        let texture = Value::String(entry.path.clone());
        let emitters =
            crate::build_tools::lights_in(&self.world, &[part], ClassName::ParticleEmitter);
        if emitters.is_empty() {
            let name = self
                .world
                .get(part)
                .map(|i| i.name().to_owned())
                .unwrap_or_default();
            self.add_children(
                &[part],
                ClassName::ParticleEmitter,
                &[("Texture", texture)],
                format!("Add particles to {name}"),
            );
            return;
        }
        let edits = emitters
            .into_iter()
            .map(|id| (id, vec![Edit::Property("Texture", texture.clone())]))
            .collect();
        self.change_many(
            Field::Property("Texture"),
            format!("Change particles to {}", entry.name),
            edits,
        );
        self.history.seal();
    }

    /// Moves `ids` (detached, and everything inside them) as a whole so
    /// their bounding box rests against the surface at `spot`, on the move
    /// grid across the surface when snapping is on.
    fn rest_on(&mut self, ids: &[InstanceId], spot: Spot) {
        let Some((min, max)) = bounds(&self.world, ids) else {
            return;
        };
        let center = (min + max) / 2.0;
        let half = (max - min) / 2.0;
        let normal = spot.normal.normalize_or(Vec3::Y);
        let reach = normal.abs().dot(half);
        let mut wanted = spot.point + normal * reach;
        // Snap across the surface, never off it.
        for axis in 0..3 {
            if normal[axis].abs() < 0.5 {
                wanted[axis] = self.snap.studs(wanted[axis]);
            }
        }
        shift(&mut self.world, ids, wanted - center);
    }

    /// Saves the selected parts and Models (and what's inside them) as a
    /// new prefab file in `assets/prefabs`.
    pub(super) fn save_prefab(&mut self) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let roots: Vec<InstanceId> = self
            .selection_roots()
            .into_iter()
            .filter(|&id| tree::can_be_prefab(&self.world, id))
            .collect();
        if roots.is_empty() {
            self.output
                .error("Select parts, Models or Folders in the Workspace to save as a prefab.");
            return;
        }
        let wanted = match roots.as_slice() {
            [only] => self
                .world
                .get(*only)
                .map(|i| i.name().to_owned())
                .unwrap_or_default(),
            _ => "Prefab".to_owned(),
        };
        let stem = prefab_file_stem(&wanted);
        let path = tree::numbered_names(&stem)
            .map(|stem| format!("{PREFABS_FOLDER}/{stem}{PREFAB_SUFFIX}"))
            .find(|path| !folder.join(path).exists())
            .expect("numbered names never run out");
        let text = project::write_prefab(&self.world, &roots);
        match project::write_script(&folder, &path, &text) {
            Ok(()) => {
                self.toasts.success(format!("Saved the prefab `{path}`"));
                self.output.info(format!("Saved the prefab `{path}`."));
                let name = display_name(&path);
                self.record_prefab(
                    format!("Create {name}"),
                    vec![prefab_diff(&path, None, Some(text))],
                );
            }
            Err(err) => self
                .output
                .error(format!("Could not save the prefab `{path}`: {err}")),
        }
    }

    /// Renames the prefab file at `path` to `name`, in the same folder.
    pub(super) fn rename_prefab(&mut self, path: &str, name: &str) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let parent = path
            .rsplit_once('/')
            .map_or(PREFABS_FOLDER, |(parent, _)| parent);
        let new_path = format!("{parent}/{name}{PREFAB_SUFFIX}");
        if new_path == path {
            return;
        }
        let Some(text) = std::fs::read_to_string(folder.join(path)).ok() else {
            self.output
                .error(format!("Could not rename `{path}`: the file isn't there"));
            return;
        };
        let differs_in_case_only = new_path.eq_ignore_ascii_case(path);
        if !differs_in_case_only && folder.join(&new_path).exists() {
            self.output.error(format!(
                "Could not rename `{path}`: `{name}` is already a prefab"
            ));
            return;
        }
        if let Err(err) = std::fs::rename(folder.join(path), folder.join(&new_path)) {
            self.output
                .error(format!("Could not rename `{path}`: {err}"));
            return;
        }
        self.output
            .info(format!("Renamed the prefab `{path}` to `{new_path}`."));
        self.record_prefab(
            format!("Rename {name}"),
            vec![
                prefab_diff(path, Some(text.clone()), None),
                prefab_diff(&new_path, None, Some(text)),
            ],
        );
    }

    /// Copies the prefab at `path` beside it, as "<name> copy".
    pub(super) fn duplicate_prefab(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let Some(text) = std::fs::read_to_string(folder.join(path)).ok() else {
            self.output
                .error(format!("Could not copy `{path}`: the file isn't there"));
            return;
        };
        let stem = prefab_file_stem(&format!("{} copy", display_name(path)));
        let copy = tree::numbered_names(&stem)
            .map(|stem| format!("{PREFABS_FOLDER}/{stem}{PREFAB_SUFFIX}"))
            .find(|candidate| !folder.join(candidate).exists())
            .expect("numbered names never run out");
        if let Err(err) = project::write_script(&folder, &copy, &text) {
            self.output.error(format!("Could not copy `{path}`: {err}"));
            return;
        }
        self.output
            .info(format!("Copied the prefab `{path}` as `{copy}`."));
        self.record_prefab(
            format!("Duplicate {}", display_name(path)),
            vec![prefab_diff(&copy, None, Some(text))],
        );
    }

    /// Deletes `files` (paths from the project folder), the own files of
    /// the assets called `name`, as one undo step. The step keeps each
    /// file's bytes, so undo puts models and pictures back too. Folders
    /// left empty go.
    pub(super) fn delete_assets(&mut self, name: &str, files: &[String]) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        let mut blobs = Vec::new();
        for path in files {
            match std::fs::read(folder.join(path)) {
                Ok(bytes) => blobs.push(BlobChange {
                    path: path.clone(),
                    before: Some(Arc::new(bytes)),
                    after: None,
                }),
                Err(err) => {
                    self.output
                        .error(format!("Could not delete {name}: `{path}`: {err}"));
                    return;
                }
            }
        }
        let deleted = self.write_package_files(&blobs);
        if deleted == 0 {
            return;
        }
        let what = if deleted == 1 {
            format!("`{}`", files[0])
        } else {
            format!("{deleted} files")
        };
        self.output.info(format!("Deleted {name} ({what})."));
        let blobs = blobs.into_iter().take(deleted).collect();
        self.history
            .record_blobs(&mut self.world, format!("Delete {name}"), blobs);
    }

    fn record_prefab(&mut self, label: String, files: Vec<crate::animation::store::FileDiff>) {
        self.record_asset_files(label, files, None);
    }

    /// Imports one file into the project (in `folder`) by its kind, for
    /// Import…: a model or an asset package through the Import dialog, the rest copied
    /// straight into `assets` (images into `image_folder`) and reported in
    /// the Console.
    pub(in crate::app) fn import_file(&mut self, folder: &Path, file: &Path, image_folder: &str) {
        if super::files::is_package(file) {
            self.import_package(file);
            return;
        }
        let name = file.to_string_lossy().replace('\\', "/");
        let result = match asset_browser::kind_of(&name) {
            Some(AssetKind::Model) => {
                self.import_model(file);
                return;
            }
            Some(AssetKind::Audio) => {
                axcenity_assets::import_audio(folder, file).map_err(|err| err.to_string())
            }
            Some(AssetKind::Image | AssetKind::Texture | AssetKind::Particle) => {
                axcenity_assets::import_texture(folder, file, image_folder)
                    .map_err(|err| err.to_string())
            }
            Some(AssetKind::Prefab) => copy_prefab(folder, file),
            // Clips and controllers name files in their own project, so
            // they come with an imported FBX instead.
            Some(AssetKind::Animation | AssetKind::Controller) => Err(
                "animation clips and state machines come from importing an animated FBX model"
                    .to_owned(),
            ),
            Some(AssetKind::BehaviorTree) => {
                Err("behavior trees are made with New tree in the Behavior Tree tab".to_owned())
            }
            Some(AssetKind::DataTable) => {
                Err("data tables are made with New table in the Data Table tab".to_owned())
            }
            Some(AssetKind::TextureSet) => {
                Err("texture sets are saved with Get texture in the Assets panel".to_owned())
            }
            Some(AssetKind::Material) => Err(
                "material files name textures in their own project, so they're made with Save                  as Material on a part's Explorer row"
                    .to_owned(),
            ),
            Some(AssetKind::Prop) | None => {
                Err("it isn't a model, image, sound or prefab (`.prefab.json`) file".to_owned())
            }
        };
        match result {
            Ok(path) => {
                self.toasts.success(format!("Imported `{path}`"));
                self.output.info(format!("Imported `{path}`."));
            }
            Err(err) => {
                self.toasts
                    .error("Could not import that file. Output says why.");
                self.output
                    .error(format!("Could not import `{}`: {err}", file.display()));
            }
        }
    }

    /// The text of a file in the project, or `None` with the reason in Output.
    fn read_project_file(&mut self, path: &str) -> Option<String> {
        let bytes = match self.assets.read_file(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.output.error(format!("Could not read `{path}`: {err}"));
                return None;
            }
        };
        match String::from_utf8(bytes) {
            Ok(text) => Some(text),
            Err(_) => {
                self.output
                    .error(format!("Could not read `{path}`: it isn't a text file"));
                None
            }
        }
    }
}

/// Copies a prefab file into the project's prefabs folder (checking it
/// first), under a name not yet taken there.
fn copy_prefab(folder: &Path, file: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(file).map_err(|err| err.to_string())?;
    let mut check = World::new();
    let workspace = check.workspace();
    project::parse_prefab(&text, &mut check, workspace).map_err(|err| err.to_string())?;
    let name = file.to_string_lossy().replace('\\', "/");
    let stem = prefab_file_stem(&display_name(&name));
    let path = tree::numbered_names(&stem)
        .map(|stem| format!("{PREFABS_FOLDER}/{stem}{PREFAB_SUFFIX}"))
        .find(|path| !folder.join(path).exists())
        .expect("numbered names never run out");
    project::write_script(folder, &path, &text).map_err(|err| err.to_string())?;
    Ok(path)
}

fn prefab_diff(
    path: &str,
    before: Option<String>,
    after: Option<String>,
) -> crate::animation::store::FileDiff {
    crate::animation::store::FileDiff {
        path: path.to_owned(),
        before,
        after,
    }
}

fn is_part(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|i| matches!(i.class(), ClassName::Part | ClassName::MeshPart))
}

/// True if the selection has something [`can_be_prefab`].
pub(super) fn any_prefab(world: &World, ids: &[InstanceId]) -> bool {
    ids.iter().any(|&id| tree::can_be_prefab(world, id))
}

/// `ids` and everything inside them.
fn with_descendants(world: &World, ids: &[InstanceId]) -> Vec<InstanceId> {
    let mut all = Vec::new();
    for &id in ids {
        all.push(id);
        all.extend(world.descendants(id));
    }
    all
}

/// The world-aligned box around every part in `ids` (and inside them).
pub(super) fn bounds(world: &World, ids: &[InstanceId]) -> Option<(Vec3, Vec3)> {
    let mut found: Option<(Vec3, Vec3)> = None;
    for id in with_descendants(world, ids) {
        let Some(pose) = world.get(id).and_then(Pose::of) else {
            continue;
        };
        // The box's half size along each world axis, however it's turned.
        let axes = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| pose.rotation * axis);
        let half = pose.size / 2.0;
        let reach = Vec3::new(
            axes[0].x.abs() * half.x + axes[1].x.abs() * half.y + axes[2].x.abs() * half.z,
            axes[0].y.abs() * half.x + axes[1].y.abs() * half.y + axes[2].y.abs() * half.z,
            axes[0].z.abs() * half.x + axes[1].z.abs() * half.y + axes[2].z.abs() * half.z,
        );
        let (min, max) = (pose.position - reach, pose.position + reach);
        found = Some(match found {
            Some((a, b)) => (a.min(min), b.max(max)),
            None => (min, max),
        });
    }
    found
}

/// Moves every part in `ids` (and inside them) by `delta`.
pub(super) fn shift(world: &mut World, ids: &[InstanceId], delta: Vec3) {
    for id in with_descendants(world, ids) {
        let Some(position) = world
            .get(id)
            .filter(|instance| Pose::of(instance).is_some())
            .and_then(|instance| instance.property("Position"))
            .and_then(Value::as_vector3)
        else {
            continue;
        };
        let moved = Vector3::new(
            position.x + delta.x,
            position.y + delta.y,
            position.z + delta.z,
        );
        let _ = world.set_property(id, "Position", Value::Vector3(moved));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::PROPS;

    #[test]
    fn a_dropped_prop_rests_on_the_surface_it_is_dropped_on() {
        let mut world = World::new();
        let workspace = world.workspace();
        let lamp = PROPS.iter().find(|prop| prop.name == "Lamp").unwrap();
        let ids = project::parse_prefab(lamp.prefab, &mut world, workspace).unwrap();
        let (min, max) = bounds(&world, &ids).unwrap();
        // Dropped on a floor whose top is at Y = 3.
        let target = Vec3::new(10.0, 3.0, -4.0);
        let center = (min + max) / 2.0;
        let lift = (max.y - min.y) / 2.0;
        shift(&mut world, &ids, target + Vec3::Y * lift - center);
        let (min, max) = bounds(&world, &ids).unwrap();
        assert!((min.y - 3.0).abs() < 1e-4, "bottom at {}", min.y);
        assert!(((min.x + max.x) / 2.0 - 10.0).abs() < 1e-4);
        assert!(((min.z + max.z) / 2.0 + 4.0).abs() < 1e-4);
    }

    #[test]
    fn dropping_a_prop_places_it_where_it_lands_as_one_undo_step() {
        let mut app = EditorApp::new(Some(&crate::app::tests::example("hello-touched")));
        let workspace = app.world.workspace();
        let before = app.world.get(workspace).unwrap().children().len();
        let entry = AssetEntry {
            kind: AssetKind::Prop,
            path: "Table".to_owned(),
            name: "Table".to_owned(),
        };
        let target = DropTarget {
            point: Vec3::new(5.0, 2.0, 0.0),
            normal: Vec3::Y,
            part: None,
            ui: None,
            released: true,
        };
        app.drop_asset(&entry, Some(target));
        let table = app.selected.expect("the placed prop is selected");
        assert_eq!(app.world.get(table).unwrap().name(), "Table");
        assert_eq!(
            app.world.get(workspace).unwrap().children().len(),
            before + 1
        );
        let (min, max) = bounds(&app.world, &[table]).unwrap();
        assert!((min.y - 2.0).abs() < 1e-4);
        assert!(((min.x + max.x) / 2.0 - 5.0).abs() < 1e-4);
        assert_eq!(app.history.undo_label(), Some("Place Table"));
        app.undo();
        assert_eq!(app.world.get(workspace).unwrap().children().len(), before);
    }

    #[test]
    fn dropping_a_global_asset_copies_it_into_the_project_first() {
        let temp =
            std::env::temp_dir().join(format!("axcenity-drop-global-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let library = temp.join("library");
        std::fs::create_dir_all(library.join("assets/audio")).unwrap();
        std::fs::write(library.join("assets/audio/ding.wav"), b"sound").unwrap();
        let folder = project::create_project(&temp, "Uses Global").unwrap();
        let mut app = EditorApp::new(Some(&folder));
        app.set_global_assets(Some(library));
        let entry = AssetEntry {
            kind: AssetKind::Audio,
            path: "assets/audio/ding.wav".to_owned(),
            name: "ding".to_owned(),
        };
        assert!(app.asset_browser.entries().contains(&entry), "listed");

        app.drop_asset(&entry, None);
        assert_eq!(
            std::fs::read(folder.join("assets/audio/ding.wav")).unwrap(),
            b"sound"
        );
        let sound = app.selected.expect("the sound is selected");
        assert_eq!(
            app.world.get(sound).unwrap().property("SoundId"),
            Some(&Value::String(entry.path.clone()))
        );
        assert!(
            app.output
                .lines()
                .iter()
                .any(|line| line.text.contains("Copied the global asset ding")),
            "said in Output"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn deleting_a_model_folder_removes_its_files_and_undo_puts_them_back() {
        let temp =
            std::env::temp_dir().join(format!("axcenity-delete-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let folder = project::create_project(&temp, "Deletes").unwrap();
        let skin = [0_u8, 159, 146, 150];
        for (file, bytes) in [
            ("assets/models/Walker/Walker.fbx", &b"model"[..]),
            ("assets/models/Walker/skin.png", &skin[..]),
            ("assets/animations/Walker/Walk.anim.json", &b"{}"[..]),
            ("assets/audio/ding.wav", &b"sound"[..]),
        ] {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let mut app = EditorApp::new(Some(&folder));
        let model = "assets/models/Walker/Walker.fbx".to_owned();
        let files = app.asset_browser.files_to_delete(&[model]);
        assert_eq!(
            files,
            [
                "assets/animations/Walker/Walk.anim.json",
                "assets/models/Walker/Walker.fbx",
                "assets/models/Walker/skin.png",
            ],
            "the folder and its clips"
        );

        app.delete_assets("Walker", &files);
        assert_eq!(app.history.undo_label(), Some("Delete Walker"));
        assert!(
            !folder.join("assets/models/Walker").exists(),
            "empty folders go"
        );
        assert!(!folder.join("assets/animations/Walker").exists());
        assert!(
            folder.join("assets/models").is_dir(),
            "the kind's folder stays"
        );
        assert!(
            folder.join("assets/audio/ding.wav").is_file(),
            "others stay"
        );
        assert!(
            !app.asset_browser
                .entries()
                .iter()
                .any(|entry| entry.path.starts_with("assets/models/Walker")),
            "gone from the panel"
        );

        app.undo();
        assert_eq!(
            std::fs::read(folder.join("assets/models/Walker/skin.png")).unwrap(),
            skin,
            "the bytes come back"
        );
        assert!(
            folder
                .join("assets/animations/Walker/Walk.anim.json")
                .is_file()
        );
        app.redo();
        assert!(!folder.join("assets/models/Walker/Walker.fbx").exists());
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn a_turned_part_is_bounded_by_its_turned_box() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(4.0, 1.0, 2.0)))
            .unwrap();
        world
            .set_property(
                part,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
            )
            .unwrap();
        let (min, max) = bounds(&world, &[part]).unwrap();
        let size = max - min;
        assert!(
            (size.x - 2.0).abs() < 1e-4 && (size.z - 4.0).abs() < 1e-4,
            "{size}"
        );
    }
}
