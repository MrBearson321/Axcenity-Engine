//! World presets in the editor (procedural worlds PW3): generating with
//! one (Apply puts in its layers, biomes, rules and water in the same undo
//! step), using only its objects on the terrain as it is, and saving the
//! terrain's setup to the project's `presets/` folder.

use std::path::PathBuf;

use axcenity_project::{WORLD_PRESETS_FOLDER, WorldPreset};

use super::*;
use crate::terrain_studio::{Preview, built_in_world_presets};

/// The world presets the Terrain Studio lists: the built-in ones, then the
/// project's, read again when the project changes or one is saved.
#[derive(Default)]
pub(in crate::app) struct WorldPresets {
    /// The project folder `list` was read for (`Some(None)`: no project).
    read_for: Option<Option<PathBuf>>,
    pub list: Vec<WorldPreset>,
}

impl EditorApp {
    /// Reads the presets again if the project changed since they were read.
    pub(super) fn refresh_world_presets(&mut self) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        if self.world_presets.read_for.as_ref() == Some(&folder) {
            return;
        }
        let mut list = built_in_world_presets();
        if let Some(folder) = &folder {
            let (found, problems) =
                axcenity_project::list_world_presets(&folder.join(WORLD_PRESETS_FOLDER));
            for (file, problem) in problems {
                self.output.warning(format!(
                    "Couldn't read the world preset {}: {problem}",
                    file.display()
                ));
            }
            list.extend(found.into_iter().map(|(_, preset)| preset));
        }
        self.world_presets = WorldPresets {
            read_for: Some(folder),
            list,
        };
    }

    /// Generate the landscape of preset `index`; Apply on the preview then
    /// also sets up its objects and water.
    pub(super) fn generate_world_preset(&mut self, index: usize) {
        let Some(preset) = self.world_presets.list.get(index).cloned() else {
            return;
        };
        self.terrain.generator = GeneratorSettings {
            seed: self.terrain.generator.seed,
            ..preset.generator.clone()
        };
        self.terrain.pending_world = Some(preset);
        self.start_generate();
    }

    /// Applies a generated preview made from a world preset: the new
    /// landscape, the preset's objects and its water, as one undo step.
    /// False (nothing done) if there's no such preset or preview.
    pub(super) fn apply_world_preview(&mut self, preview: &Preview) -> bool {
        let Some(preset) = self.terrain.pending_world.take() else {
            return false;
        };
        if preview.erosion {
            return false;
        }
        let terrain = preview.terrain;
        let before = self.world.clone();
        if let Some(size) = preview.size
            && self
                .world
                .set_property(terrain, "Size", Value::Vector3(size))
                .is_err()
        {
            return false;
        }
        if let Some(current) = self.world.terrain_mut(terrain) {
            current.replace_with(&preview.data);
        }
        if let Err(problem) = preset.apply_objects(&mut self.world, terrain) {
            self.world = before;
            self.output.error(format!(
                "Could not use the world preset {}: {problem}",
                preset.name
            ));
            return true;
        }
        self.history
            .record_world(&mut self.world, format!("Generate {}", preset.name), before);
        self.generate_frame();
        true
    }

    /// Replaces `terrain`'s layers, biomes, rules and water with preset
    /// `index`'s, keeping its shape, as one undo step.
    pub(super) fn apply_world_preset(&mut self, terrain: InstanceId, index: usize) {
        let Some(preset) = self.world_presets.list.get(index).cloned() else {
            return;
        };
        let before = self.world.clone();
        match preset.apply_objects(&mut self.world, terrain) {
            Ok(_) => {
                self.history.record_world(
                    &mut self.world,
                    format!("Use {} Materials and Rules", preset.name),
                    before,
                );
                self.generate_frame();
            }
            Err(problem) => {
                self.world = before;
                self.output.error(format!(
                    "Could not use the world preset {}: {problem}",
                    preset.name
                ));
            }
        }
    }

    /// Saves `terrain`'s setup as the world preset `name` in the project's
    /// presets folder: its layers, biomes, rules and water, and the
    /// settings its landscape was generated with (or the Generate tool's).
    pub(super) fn save_world_preset(&mut self, terrain: InstanceId, name: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let generator = self
            .world
            .terrain(terrain)
            .and_then(|data| data.generation.as_deref())
            .and_then(|text| GeneratorSettings::from_json(text).ok())
            .unwrap_or_else(|| self.terrain.generator.clone());
        let saved =
            WorldPreset::capture(&self.world, terrain, name, "", generator).and_then(|preset| {
                axcenity_project::save_world_preset(&folder.join(WORLD_PRESETS_FOLDER), &preset)
            });
        match saved {
            Ok(file) => {
                self.output.info(format!(
                    "Saved the world preset {name} ({}).",
                    file.strip_prefix(&folder).unwrap_or(&file).display()
                ));
                // Listed again, and picked.
                self.world_presets.read_for = None;
                self.refresh_world_presets();
                if let Some(index) = self.world_presets.list.iter().rposition(|p| p.name == name) {
                    self.terrain.world_preset = index;
                }
                self.terrain.preset_name.clear();
            }
            Err(problem) => self
                .output
                .error(format!("Could not save the world preset {name}: {problem}")),
        }
    }
}
