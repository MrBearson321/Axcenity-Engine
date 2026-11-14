//! Model import presets in the editor (P7.3): the Import dialog starts
//! with the preset last used in the project, Save as preset… writes
//! `import-presets.json` as an undo step, and importing applies the
//! dialog's scale, up axis, Anchored and collision to the parts it makes.
//! The presets: `axcenity_assets::model_presets`.

use std::sync::Arc;

use axcenity_assets::{ImportCollision, ImportPresets, ModelSettings, UpAxis};
use axcenity_core::EnumItem;
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::{orientation_from_rotation, rotation_from_orientation};

use super::*;
use crate::history::BlobChange;

impl EditorApp {
    /// Gives `dialog` the project's presets and starts it with the one
    /// last used (Default the first time, or when that one is gone).
    pub(super) fn start_with_preset(&mut self, dialog: &mut ImportDialog) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let presets = match ImportPresets::read(&folder) {
            Ok(presets) => presets,
            Err(err) => {
                self.output.error(format!(
                    "Could not read `{}`, so only the built-in import presets are offered: {err}",
                    axcenity_assets::PRESETS_FILE
                ));
                ImportPresets::default()
            }
        };
        dialog.presets = presets.model_presets_in_order();
        let last = axcenity_assets::last_model_preset(&folder)
            .and_then(|name| Some((presets.model_settings(&name)?, name)));
        let (settings, name) = last.unwrap_or_else(|| {
            (
                ModelSettings::default(),
                axcenity_assets::DEFAULT_MODEL_PRESET.to_owned(),
            )
        });
        dialog.apply_preset(&name, settings);
    }

    /// Remembers the preset `dialog` was imported with, for the next
    /// import (this computer's, in `.axcenity/`).
    pub(super) fn remember_preset(&mut self, dialog: &ImportDialog) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if dialog.presets.is_empty() {
            return;
        }
        // Not worth a message: the next import just starts from Default.
        let _ = axcenity_assets::remember_model_preset(&folder, &dialog.preset);
    }

    /// Saves the model preset `name` as `settings` in the project's
    /// `import-presets.json`, as one undo step.
    pub(in crate::app) fn save_model_preset(&mut self, name: &str, settings: ModelSettings) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let path = axcenity_assets::PRESETS_FILE;
        let before = std::fs::read(folder.join(path)).ok();
        let after = match axcenity_assets::presets_file_with_model_preset(&folder, name, settings) {
            Ok(after) => after,
            Err(err) => {
                self.output
                    .error(format!("Could not save the preset {name}: `{path}` {err}"));
                return;
            }
        };
        if before.as_deref() == Some(after.as_slice()) {
            return;
        }
        let blobs = vec![BlobChange {
            path: path.to_owned(),
            before: before.map(Arc::new),
            after: Some(Arc::new(after)),
        }];
        if self.write_package_files(&blobs) == 0 {
            return;
        }
        let label = format!("Save import preset {name}");
        self.output.info(format!(
            "Saved the import preset {name} in `{path}`: {}",
            settings.describe()
        ));
        self.history
            .record_blobs_and_properties(&mut self.world, label, blobs, Vec::new());
    }

    /// Scales and turns the new MeshParts `ids` about `origin` (where the
    /// import puts them), and anchors them and sets their collision, as
    /// `settings` says.
    pub(super) fn apply_model_settings(
        &mut self,
        ids: &[InstanceId],
        origin: Vector3,
        settings: &ModelSettings,
    ) {
        let scale = settings.scale();
        let turn = match settings.up_axis {
            UpAxis::Y => Quat::IDENTITY,
            // The file's Z up becomes Y up: a quarter turn back round X.
            UpAxis::Z => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        };
        let origin = Vec3::new(origin.x, origin.y, origin.z);
        for &id in ids {
            if scale != 1.0 || turn != Quat::IDENTITY {
                let part = self.world.get(id).expect("just made");
                if let Some(Value::Vector3(size)) = part.property("Size").cloned() {
                    let size = Vector3::new(
                        (size.x * scale).max(MIN_SIZE),
                        (size.y * scale).max(MIN_SIZE),
                        (size.z * scale).max(MIN_SIZE),
                    );
                    self.world
                        .set_property(id, "Size", Value::Vector3(size))
                        .expect("every MeshPart has a Size");
                }
                let part = self.world.get(id).expect("just made");
                let read = |name| match part.property(name) {
                    Some(Value::Vector3(value)) => Some(*value),
                    _ => None,
                };
                if let (Some(at), Some(orientation)) = (read("Position"), read("Orientation")) {
                    let at = Vec3::new(at.x, at.y, at.z);
                    let at = origin + turn * ((at - origin) * scale);
                    let rotation = turn * rotation_from_orientation(orientation);
                    for (name, value) in [
                        ("Position", Vector3::new(at.x, at.y, at.z)),
                        ("Orientation", orientation_from_rotation(rotation)),
                    ] {
                        self.world
                            .set_property(id, name, Value::Vector3(value))
                            .expect("every MeshPart has these");
                    }
                }
            }
            if settings.anchored {
                self.world
                    .set_property(id, "Anchored", Value::Bool(true))
                    .expect("every MeshPart has Anchored");
            }
            if settings.collision != ImportCollision::Default {
                self.world
                    .set_property(
                        id,
                        "CollisionFidelity",
                        Value::EnumItem(EnumItem {
                            enum_name: "CollisionFidelity".to_owned(),
                            item: settings.collision.item().to_owned(),
                        }),
                    )
                    .expect("every MeshPart has a CollisionFidelity");
            }
        }
    }
}
