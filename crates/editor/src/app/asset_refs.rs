//! Find References, the Unused filter and Rename or Move… for assets
//! (unified Phase 70; `project::find_references`, `project::plan_rename`).
//! A rename is one undo step that keeps the world's properties and every
//! file it moved or rewrote, as an Asset Check fix is.

use std::sync::Arc;

use axcenity_project::{RefPlace, Reference};

use super::*;
use crate::history::{BlobChange, PropertyEdit};
use axcenity_project as project;

/// The References window: what uses one asset.
pub(super) struct ReferencesWindow {
    pub asset: String,
    pub list: Vec<Reference>,
}

impl EditorApp {
    /// The open project's folder and open level, for the reference search.
    fn reference_scope(&self) -> Option<(PathBuf, Option<String>)> {
        self.project
            .as_ref()
            .map(|project| (project.folder.clone(), project.level.clone()))
    }

    /// Opens the References window for the asset at `path`.
    pub(super) fn find_asset_references(&mut self, path: &str) {
        let Some((folder, level)) = self.reference_scope() else {
            return;
        };
        let list = project::find_references(&folder, &self.world, level.as_deref(), path);
        self.references = Some(ReferencesWindow {
            asset: path.to_owned(),
            list,
        });
    }

    /// Tells the Assets panel which of `paths` nothing uses.
    pub(super) fn find_unused_assets(&mut self, paths: &[String]) {
        let Some((folder, level)) = self.reference_scope() else {
            return;
        };
        let unused = project::unused_assets(&folder, &self.world, level.as_deref(), paths);
        self.asset_browser.set_unused(unused);
    }

    /// Renames or moves the asset at `from` to `to`, pointing everything
    /// that uses it at the new path, as one undo step. Not during Play.
    pub(super) fn rename_asset(&mut self, from: &str, to: &str) {
        if self.playing() {
            self.output.error("Press Stop to rename assets.");
            return;
        }
        let Some((folder, level)) = self.reference_scope() else {
            return;
        };
        let name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
        let plan = match project::plan_rename(&folder, &self.world, level.as_deref(), from, to) {
            Ok(plan) => plan,
            Err(err) => {
                self.output
                    .error(format!("Could not rename `{}`: {err}.", name(from)));
                return;
            }
        };
        let mut properties = Vec::new();
        for change in &plan.properties {
            let Some(before) = self
                .world
                .get(change.id)
                .and_then(|instance| instance.property(change.property).cloned())
            else {
                continue;
            };
            let after = Value::String(change.value.clone());
            if self
                .world
                .set_property(change.id, change.property, after.clone())
                .is_ok()
            {
                properties.push(PropertyEdit {
                    id: change.id,
                    field: Field::Property(change.property),
                    before,
                    after,
                });
            }
        }
        let blobs: Vec<BlobChange> = plan
            .files
            .iter()
            .map(|write| BlobChange {
                path: write.path.clone(),
                before: write.before.clone().map(Arc::new),
                after: write.after.clone().map(Arc::new),
            })
            .collect();
        let written = self.write_package_files(&blobs);
        let blobs: Vec<BlobChange> = blobs.into_iter().take(written).collect();
        for blob in &blobs {
            self.assets.forget(&blob.path);
        }
        self.assets.forget_failures();
        let label = if name(from) == name(to) {
            format!("Move `{}` to `{to}`", name(from))
        } else {
            format!("Rename `{}` to `{}`", name(from), name(to))
        };
        let count = blobs.len().saturating_sub(2) + properties.len();
        self.output.info(format!(
            "{label}: {count} {} changed to match.",
            if count == 1 {
                "reference"
            } else {
                "references"
            }
        ));
        self.history
            .record_blobs_and_properties(&mut self.world, label, blobs, properties);
        if self
            .references
            .as_ref()
            .is_some_and(|open| open.asset == from)
        {
            self.find_asset_references(to);
        }
    }

    /// Goes to one reference: selects its Instance, opens its script at
    /// the line or block, or opens or shows its file.
    fn go_to_reference(&mut self, reference: &Reference) {
        if let Some(id) = reference.instance() {
            self.select_from_viewport(Some(id));
            self.layout.open(Pane::Explorer);
            return;
        }
        match &reference.place {
            RefPlace::Script { file, line } => self.open_script(file, Some(*line)),
            RefPlace::Blocks { file, block } => self.open_block(file, *block),
            _ => {
                let Some(file) = reference.file().map(str::to_owned) else {
                    return;
                };
                if axcenity_assets::is_material_file(&file) {
                    self.open_material(&file);
                } else if axcenity_assets::is_data_table(&file) {
                    self.open_data_table(&file);
                } else {
                    self.layout.open(Pane::Assets);
                    self.asset_browser.reveal(&file);
                }
            }
        }
    }

    /// The References window, while one is open.
    pub(super) fn references_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &self.references else {
            return;
        };
        let palette = theme::palette(ctx);
        let mut open = true;
        let mut go = None;
        let title = window.asset.rsplit('/').next().unwrap_or(&window.asset);
        widgets::tool_window(icons::MAGNIFYING_GLASS, &format!("References to {title}"))
            .id("asset references")
            .show(ctx, &mut open, |ui| {
                ui.label(egui::RichText::new(&window.asset).color(palette.text_weak));
                ui.add_space(4.0);
                if window.list.is_empty() {
                    ui.label("Nothing uses it.");
                    return;
                }
                ui.weak(format!(
                    "{} {}",
                    window.list.len(),
                    if window.list.len() == 1 {
                        "reference"
                    } else {
                        "references"
                    }
                ));
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for (index, reference) in window.list.iter().enumerate() {
                            let icon = match &reference.place {
                                RefPlace::Script { .. } => icons::FILE_CODE,
                                RefPlace::Blocks { .. } => icons::PUZZLE_PIECE,
                                RefPlace::Json { .. } => icons::SPHERE,
                                RefPlace::PrefabLink => icons::PACKAGE,
                                RefPlace::Foliage { .. } => icons::PLANT,
                                RefPlace::Site(..) => icons::CUBE,
                            };
                            let text =
                                format!("{icon}  {}  ·  {}", reference.owner, reference.what);
                            if ui
                                .add(egui::Button::new(text).frame(false).truncate())
                                .on_hover_text(&reference.path)
                                .clicked()
                            {
                                go = Some(index);
                            }
                        }
                    });
            });
        if let Some(index) = go
            && let Some(reference) = self
                .references
                .as_ref()
                .and_then(|window| window.list.get(index).cloned())
        {
            self.go_to_reference(&reference);
        }
        if !open {
            self.references = None;
        }
    }
}
