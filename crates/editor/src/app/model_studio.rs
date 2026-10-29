//! The Model Studio tab's actions (Phase 55; the tab is `model_studio.rs`):
//! opening a model file, and saving it as a `.glb` as one step of the
//! editor's history, which MeshParts using it reload at once. A model with
//! modifiers (Phase 57) saves its sidecar too, and opens from it.

use std::sync::Arc;

use axcenity_assets::ModelFormat;
use axcenity_mesh_edit::{ModelDoc, sidecar_path};
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
        let mut models: Vec<String> = self
            .asset_browser
            .entries()
            .into_iter()
            .filter(|entry| entry.kind == AssetKind::Model)
            .map(|entry| entry.path)
            .collect();
        models.sort();
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
            }
        }
    }

    /// Opens the model at `mesh_id` (a project path, maybe with a `#`
    /// pick, whose first object becomes the active one) in the Model
    /// Studio. An FBX saves as a `.glb` beside it.
    pub(in crate::app) fn open_in_model_studio(&mut self, mesh_id: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
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
        if same_file
            && let Some(sidecar_bytes) = axcenity_assets::safe_path(&sidecar)
                .and_then(|path| std::fs::read(folder.join(path)).ok())
            && let Err(why) = model.read_sidecar(&sidecar_bytes, &bytes)
        {
            self.output.warning(format!(
                "`{sidecar}` wasn't used, so modifiers are already applied: {why}."
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
        let bytes = doc.model.write_glb();
        // The sidecar goes with it, or an old one goes once nothing needs it.
        let sidecar = sidecar_path(&path);
        let sidecar_bytes = doc.model.write_sidecar(&bytes);
        let mut changes = Vec::new();
        for (file, after) in [(&path, Some(bytes)), (&sidecar, sidecar_bytes)] {
            let disk = folder.join(axcenity_assets::safe_path(file)?);
            let before = std::fs::read(&disk).ok();
            if before.is_none() && after.is_none() {
                continue;
            }
            let written = match &after {
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
                after: after.map(Arc::new),
            });
        }
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
}
