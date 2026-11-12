//! The Model Studio tab's actions (Phase 55; the tab is `model_studio.rs`):
//! opening a model file, and saving it as a `.glb` as one step of the
//! editor's history, which MeshParts using it reload at once. A model with
//! modifiers (Phase 57) saves its sidecar too, and opens from it. Painted
//! maps (Phase 59) save as PNGs in `textures/` beside it, and their layers
//! in its sidecar folder.

use std::sync::Arc;

use axcenity_assets::ModelFormat;
use axcenity_mesh_edit::{ModelDoc, paint_folder, sidecar_path};
use eframe::egui_wgpu::RenderState;

use super::*;
use crate::asset_browser::AssetKind;
use crate::history::BlobChange;
use crate::model_studio::{StudioAction, StudioDoc};

impl EditorApp {
    pub(in crate::app) fn model_studio_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) {
        let entries = self.asset_browser.entries();
        let of_kind = |kinds: &[AssetKind]| -> Vec<String> {
            let mut paths: Vec<String> = entries
                .iter()
                .filter(|entry| kinds.contains(&entry.kind))
                .map(|entry| entry.path.clone())
                .collect();
            paths.sort();
            paths
        };
        let models = of_kind(&[AssetKind::Model]);
        self.model_studio.images = of_kind(&[AssetKind::Image, AssetKind::Texture]);
        let editable = self.project.is_some() && !self.playing();
        let actions = self
            .model_studio
            .show(ui, render_state, &models, self.snap, editable);
        for notice in std::mem::take(&mut self.model_studio.notices) {
            self.output.info(notice);
        }
        for action in actions {
            match action {
                StudioAction::Open(path) => self.open_in_model_studio(&path),
                StudioAction::Save => {
                    self.save_model_studio();
                }
                StudioAction::BrushImage(path) => self.load_brush_image(&path),
            }
        }
    }

    /// Opens the model at `mesh_id` (a project path or asset ID, maybe
    /// with a `#` pick, whose first object becomes the active one) in the
    /// Model Studio. An FBX saves as a `.glb` beside it.
    pub(in crate::app) fn open_in_model_studio(&mut self, mesh_id: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let mesh_id = &*self.assets.resolve(mesh_id);
        let (file, pick) = mesh_id.split_once('#').unwrap_or((mesh_id, ""));
        let Some(format) = ModelFormat::of(std::path::Path::new(file)) else {
            self.output.error(format!("`{file}` isn't a model file."));
            return;
        };
        let Some(relative) = axcenity_assets::safe_path(file) else {
            return;
        };
        let disk = folder.join(&relative);
        let opened = std::fs::read(&disk)
            .map_err(|err| err.to_string())
            .and_then(|bytes| {
                let parent = disk.parent().unwrap_or(&folder);
                let model =
                    ModelDoc::open(&bytes, parent, format).map_err(|err| err.to_string())?;
                Ok((model, bytes))
            });
        let (mut model, bytes) = match opened {
            Ok(opened) => opened,
            Err(why) => {
                self.output.error(format!(
                    "Could not open `{file}` in the Model Studio: {why}"
                ));
                return;
            }
        };
        // A `.glb` saves over itself; an FBX or `.gltf` as a `.glb` beside it.
        let glb = format!(
            "{}.glb",
            file.rsplit_once('.').map_or(file, |(stem, _)| stem)
        );
        let same_file = glb == file;
        // Each object's own mesh and modifiers, from the sidecar.
        let sidecar = sidecar_path(file);
        // Paint layers are files in the sidecar folder.
        let layers =
            folder.join(axcenity_assets::safe_path(&paint_folder(file)).unwrap_or_default());
        let mut read_layer = |name: &str| {
            let name = axcenity_assets::safe_file_name(name)?;
            std::fs::read(layers.join(name)).ok()
        };
        if same_file
            && let Some(sidecar_bytes) = axcenity_assets::safe_path(&sidecar)
                .and_then(|path| std::fs::read(folder.join(path)).ok())
            && let Err(why) = model.read_sidecar_with(&sidecar_bytes, &bytes, &mut read_layer)
        {
            self.output.warning(format!(
                "`{sidecar}` wasn't used, so modifiers are already applied (and seams, pins and second uvs are gone): {why}."
            ));
        }
        let mut doc = StudioDoc::new(glb.clone(), model, same_file);
        // The picked object, by the last name in its path.
        if let Some(first) = pick.split(',').next().filter(|first| !first.is_empty()) {
            let name = first.rsplit('/').next().unwrap_or(first);
            if let Some(index) = doc
                .model
                .objects
                .iter()
                .position(|o| o.name == name && o.mesh.is_some())
            {
                doc.click_object(Some(index), false);
            }
        }
        if !same_file {
            self.output
                .info(format!("`{file}` will be saved as `{glb}`."));
        }
        self.model_studio.open(doc);
        self.layout.open(Pane::ModelStudio);
    }

    /// Writes the Studio's model to its `.glb` (a new model goes in
    /// `assets/models`), as one undo step of the editor, and reloads it in
    /// every MeshPart using it. Returns the path written.
    pub(in crate::app) fn save_model_studio(&mut self) -> Option<String> {
        let (folder, path) = self.model_studio_path()?;
        let doc = self.model_studio.doc.as_mut()?;
        // Painted maps flattened to PNGs, which the materials then use.
        let textures = doc.model.bake_paint(&path);
        let bytes = doc.model.write_glb();
        // The sidecar goes with it, or an old one goes once nothing needs it.
        let sidecar = sidecar_path(&path);
        let sidecar_bytes = doc.model.write_sidecar(&bytes);
        let mut files: Vec<(String, Option<Vec<u8>>)> = vec![
            (path.clone(), Some(bytes)),
            (sidecar.clone(), sidecar_bytes),
        ];
        files.extend(textures.into_iter().map(|(file, png)| (file, Some(png))));
        // The paint layers, in place of the ones there (a deleted layer's
        // file goes).
        let layers = paint_folder(&path);
        let mut layer_files: Vec<(String, Option<Vec<u8>>)> = doc
            .model
            .paint_files()
            .into_iter()
            .map(|(name, png)| (format!("{layers}/{name}"), Some(png)))
            .collect();
        if let Some(disk) = axcenity_assets::safe_path(&layers).map(|p| folder.join(p))
            && let Ok(entries) = std::fs::read_dir(disk)
        {
            for entry in entries.flatten() {
                let old = format!("{layers}/{}", entry.file_name().to_string_lossy());
                if !layer_files.iter().any(|(file, _)| *file == old) {
                    layer_files.push((old, None));
                }
            }
        }
        files.extend(layer_files);
        let changes = self.write_studio_files(&folder, &files)?;
        // An emptied sidecar folder goes too.
        if let Some(disk) = axcenity_assets::safe_path(&layers).map(|p| folder.join(p)) {
            let _ = std::fs::remove_dir(disk);
        }
        let doc = self.model_studio.doc.as_mut()?;
        doc.mark_saved();
        let name = doc.name().to_owned();
        self.history
            .record_blobs(&mut self.world, format!("Save {name}"), changes);
        // Taken in now, not when the watcher notices: MeshParts using it
        // draw the new version this frame.
        self.asset_file_changed(&path);
        self.asset_browser.refresh();
        self.output.info(format!("Saved `{path}`."));
        Some(path)
    }

    /// The project folder and the path the Studio's model saves at, which a
    /// new model is given now (in `assets/models`, named after its first
    /// object).
    fn model_studio_path(&mut self) -> Option<(std::path::PathBuf, String)> {
        let folder = self
            .project
            .as_ref()
            .map(|project| project.folder.clone())?;
        let doc = self.model_studio.doc.as_mut()?;
        if doc.path.is_empty() {
            let name = doc
                .model
                .objects
                .first()
                .map_or("Model".to_owned(), |o| o.name.clone());
            let name = axcenity_assets::safe_file_name(&name).unwrap_or_else(|| "Model".to_owned());
            let models = folder.join(axcenity_assets::MODELS_FOLDER);
            let file = axcenity_assets::unique_file_name(&format!("{name}.glb"), |taken| {
                models.join(taken).exists()
            });
            doc.path = format!("{}/{file}", axcenity_assets::MODELS_FOLDER);
        }
        let path = doc.path.clone();
        Some((folder, path))
    }

    /// Writes (or, for `None`, deletes) each project file, and returns the
    /// changes for one history step. `None`, with the error in the Output,
    /// if one couldn't be written.
    fn write_studio_files(
        &mut self,
        folder: &std::path::Path,
        files: &[(String, Option<Vec<u8>>)],
    ) -> Option<Vec<BlobChange>> {
        let mut changes = Vec::new();
        for (file, after) in files {
            let disk = folder.join(axcenity_assets::safe_path(file)?);
            let before = std::fs::read(&disk).ok();
            if before.is_none() && after.is_none() {
                continue;
            }
            let written = match after {
                Some(after) => disk
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&disk, after)),
                None => std::fs::remove_file(&disk),
            };
            if let Err(err) = written {
                self.output.error(format!("Could not save `{file}`: {err}"));
                return None;
            }
            changes.push(BlobChange {
                path: file.clone(),
                before: before.map(Arc::new),
                after: after.clone().map(Arc::new),
            });
        }
        Some(changes)
    }

    /// Reads the project image at `path` as the Model Studio's brush image.
    fn load_brush_image(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let read = axcenity_assets::safe_path(path)
            .ok_or_else(|| "it isn't inside the project".to_owned())
            .and_then(|relative| std::fs::read(folder.join(relative)).map_err(|e| e.to_string()))
            .and_then(|bytes| crate::model_studio::paint::brush_image(path, &bytes));
        match read {
            Ok(image) => self.model_studio.paint.brush.image = Some(image),
            Err(why) => self
                .output
                .error(format!("Could not use `{path}` as a brush image: {why}")),
        }
    }
}
