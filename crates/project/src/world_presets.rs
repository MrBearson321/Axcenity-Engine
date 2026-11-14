//! World presets (procedural worlds PW3): `.axworld` files holding a whole
//! landscape setup, so a world can be started again from it: the
//! generator's settings, the terrain's TerrainLayers, Biomes (with their
//! rules) and ScatterRules, and its water.
//!
//! A preset file is JSON ([`WorldPreset`]). The objects are a prefab file's
//! text (`write_prefab`), so every property any of those classes has is
//! kept, and heights are scaled to the terrain they're applied to (they're
//! saved with the height range they were made on). Built-in presets ship
//! with the editor; the project's own are in its `presets/` folder, and
//! the editor keeps people's own in its data folder, as templates are.

use std::io;
use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_terrain::Placement;
use axcenity_terrain::generate::GeneratorSettings;
use serde::{Deserialize, Serialize};

use crate::world_file::{WorldError, parse_prefab, write_prefab};

/// A world preset file's extension.
pub const WORLD_PRESET_EXTENSION: &str = "axworld";
/// The project folder its own world presets are saved in.
pub const WORLD_PRESETS_FOLDER: &str = "presets";
/// The newest preset format this reads, and the one it writes.
pub const WORLD_PRESET_FORMAT: u32 = 1;

/// The classes a preset holds, as children of the terrain.
const PRESET_CLASSES: [ClassName; 3] = [
    ClassName::TerrainLayer,
    ClassName::Biome,
    ClassName::ScatterRule,
];

/// Properties in studs of height, scaled to the terrain a preset is applied
/// to. 10000 (no top) and more stay as they are.
const HEIGHT_PROPERTIES: [&str; 3] = ["MinHeight", "MaxHeight", "HeightFade"];

/// A world preset: what a `.axworld` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldPreset {
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The landscape's generator settings.
    pub generator: GeneratorSettings,
    /// The terrain's height range (studs) the objects' heights were set for.
    pub height: f32,
    /// The water level, 0 (the terrain's bottom) to 1 (its top), if it has water.
    #[serde(default)]
    pub water: Option<f32>,
    /// The terrain's layers, biomes and rules, as a prefab file's JSON.
    pub objects: serde_json::Value,
}

/// Why a world preset couldn't be read or applied.
#[derive(Debug, thiserror::Error)]
pub enum WorldPresetError {
    #[error("not a world preset: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "world preset format {found} is newer than this engine understands; update Axcenity to use it"
    )]
    TooNew { found: u32 },
    #[error("its objects can't be read: {0}")]
    Objects(#[from] WorldError),
    #[error("the terrain isn't in the world")]
    NoTerrain,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The children of `terrain` a preset holds, in order.
pub fn preset_objects(world: &World, terrain: InstanceId) -> Vec<InstanceId> {
    world
        .get(terrain)
        .map(|t| t.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&child| {
            world
                .get(child)
                .is_some_and(|i| PRESET_CLASSES.contains(&i.class()))
        })
        .collect()
}

impl WorldPreset {
    /// A preset of `terrain` as it is: its layers, biomes and rules, its
    /// water, and `generator`.
    pub fn capture(
        world: &World,
        terrain: InstanceId,
        name: &str,
        description: &str,
        generator: GeneratorSettings,
    ) -> Result<WorldPreset, WorldPresetError> {
        let instance = world.get(terrain).ok_or(WorldPresetError::NoTerrain)?;
        let placement = Placement::of(instance).ok_or(WorldPresetError::NoTerrain)?;
        let water = (instance.property("Water") == Some(&Value::Bool(true)))
            .then(|| instance.property("WaterLevel").and_then(Value::as_number))
            .flatten()
            .map(|level| placement.height_of(level as f32));
        let objects = write_prefab(world, &preset_objects(world, terrain));
        Ok(WorldPreset {
            format: WORLD_PRESET_FORMAT,
            name: name.trim().to_owned(),
            description: description.trim().to_owned(),
            generator,
            height: placement.size.y,
            water,
            objects: serde_json::from_str(&objects)?,
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a preset is always JSON") + "\n"
    }

    pub fn from_json(text: &str) -> Result<WorldPreset, WorldPresetError> {
        let preset: WorldPreset = serde_json::from_str(text)?;
        if preset.format > WORLD_PRESET_FORMAT {
            return Err(WorldPresetError::TooNew {
                found: preset.format,
            });
        }
        Ok(preset)
    }

    /// Replaces `terrain`'s layers, biomes and rules with this preset's
    /// (heights scaled to its height range) and sets its water. Nothing
    /// changes if the objects can't be read. Returns the new objects.
    pub fn apply_objects(
        &self,
        world: &mut World,
        terrain: InstanceId,
    ) -> Result<Vec<InstanceId>, WorldPresetError> {
        let placement = world
            .get(terrain)
            .and_then(Placement::of)
            .ok_or(WorldPresetError::NoTerrain)?;
        // Checked in a world of its own first, so a bad preset changes nothing.
        let text = self.objects.to_string();
        let mut check = World::new();
        let workspace = check.workspace();
        parse_prefab(&text, &mut check, workspace)?;
        for old in preset_objects(world, terrain) {
            let _ = world.destroy(old);
        }
        let added = parse_prefab(&text, world, terrain)?;
        let scale = if self.height > 0.0 {
            f64::from(placement.size.y / self.height)
        } else {
            1.0
        };
        if (scale - 1.0).abs() > 1e-6 {
            let mut scaled = added.clone();
            for &id in &added {
                scaled.extend(world.descendants(id));
            }
            for id in scaled {
                for property in HEIGHT_PROPERTIES {
                    let Some(value) = world
                        .get(id)
                        .and_then(|i| i.property(property))
                        .and_then(Value::as_number)
                    else {
                        continue;
                    };
                    if value < 10000.0 {
                        let _ = world.set_property(id, property, Value::Number(value * scale));
                    }
                }
            }
        }
        let _ = world.set_property(terrain, "Water", Value::Bool(self.water.is_some()));
        if let Some(level) = self.water {
            let _ = world.set_property(
                terrain,
                "WaterLevel",
                Value::Number(f64::from(placement.world_y(level))),
            );
        }
        Ok(added)
    }
}

/// A preset file found on disk, and what it holds.
pub type FoundPreset = (PathBuf, WorldPreset);
/// A preset file that couldn't be read, and why.
pub type PresetProblem = (PathBuf, WorldPresetError);

/// A file name for a preset called `name`, such as `My Island.axworld`
/// (characters file systems don't allow become `-`).
pub fn world_preset_file_name(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim().trim_end_matches('.');
    let safe = if safe.is_empty() { "World" } else { safe };
    format!("{safe}.{WORLD_PRESET_EXTENSION}")
}

/// Saves `preset` in `folder` (made if missing), named after it, replacing
/// a preset file of the same name. Returns the file.
pub fn save_world_preset(folder: &Path, preset: &WorldPreset) -> Result<PathBuf, WorldPresetError> {
    std::fs::create_dir_all(folder)?;
    let file = folder.join(world_preset_file_name(&preset.name));
    std::fs::write(&file, preset.to_json())?;
    Ok(file)
}

/// The world presets in `folder`, sorted by name, and the files that
/// couldn't be read. A missing folder has none.
pub fn list_world_presets(folder: &Path) -> (Vec<FoundPreset>, Vec<PresetProblem>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return (found, problems);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(WORLD_PRESET_EXTENSION) {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(WorldPresetError::from)
            .and_then(|text| WorldPreset::from_json(&text))
        {
            Ok(preset) => found.push((path, preset)),
            Err(problem) => problems.push((path, problem)),
        }
    }
    found.sort_by_key(|(_, preset)| preset.name.to_lowercase());
    (found, problems)
}

#[cfg(test)]
#[path = "world_presets_tests.rs"]
mod tests;
