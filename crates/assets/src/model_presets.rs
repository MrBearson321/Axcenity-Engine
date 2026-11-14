//! Mesh import presets (P7.3, `docs/research/import-formats-and-plugins.md`
//! D2): how a model comes in, picked in the Import dialog, so a team
//! importing fifty props makes the same choices once.
//!
//! They live beside the texture presets (R12) in `import-presets.json`, as
//! `"model_presets"`; one named like a built-in one takes its place:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "model_presets": {
//!     "Kitbash": { "bring_in": "model", "weld": true, "scale": 0.01, "up_axis": "z", "anchored": true }
//!   }
//! }
//! ```
//!
//! The preset last used is remembered per project in
//! `.axcenity/model-preset.json` (this computer's, not committed). Only the
//! editor acts on them; nothing here changes how a model loads.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ImportPresets;

/// The preset a model import starts with when none was used before.
pub const DEFAULT_MODEL_PRESET: &str = "Default";

/// Where the preset last used is remembered, in the project.
pub const LAST_MODEL_PRESET_FILE: &str = ".axcenity/model-preset.json";

/// How the objects of a file with more than one arrive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BringIn {
    /// As suits the file: one MeshPart for an animated file, else a Model.
    #[default]
    ByFile,
    /// A Model with a MeshPart per object, laid out as in the file.
    Model,
    /// A MeshPart per object, loose.
    Separate,
    /// One MeshPart drawing all of them.
    Merged,
}

impl BringIn {
    /// Its name in menus.
    pub fn label(self) -> &'static str {
        match self {
            BringIn::ByFile => "By file",
            BringIn::Model => "A Model",
            BringIn::Separate => "Separate MeshParts",
            BringIn::Merged => "One MeshPart",
        }
    }
}

/// Which way is up in the file. glTF, FBX and USD say so themselves (and
/// are turned Y up as they load); this is for a file exported wrongly, such
/// as a Z-up OBJ or glTF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpAxis {
    #[default]
    Y,
    /// Turned a quarter round X, so the file's Z points up.
    Z,
}

/// The `CollisionFidelity` the imported MeshParts get.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportCollision {
    /// The MeshPart's own default: the file's `_COL` pieces, else its hull.
    #[default]
    Default,
    Box,
    Hull,
    PreciseConvexDecomposition,
}

impl ImportCollision {
    /// Every choice, in menu order.
    pub const ALL: [ImportCollision; 4] = [
        ImportCollision::Default,
        ImportCollision::Box,
        ImportCollision::Hull,
        ImportCollision::PreciseConvexDecomposition,
    ];

    /// Its `CollisionFidelity` enum item.
    pub fn item(self) -> &'static str {
        match self {
            ImportCollision::Default => "Default",
            ImportCollision::Box => "Box",
            ImportCollision::Hull => "Hull",
            ImportCollision::PreciseConvexDecomposition => "PreciseConvexDecomposition",
        }
    }
}

/// What a model preset does: the Import dialog's choices.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSettings {
    #[serde(default)]
    pub bring_in: BringIn,
    /// Weld a Model's parts to its largest one.
    #[serde(default)]
    pub weld: bool,
    /// Give an animated file an Animator with its clips.
    #[serde(default = "yes")]
    pub animator: bool,
    /// Every size and distance times this (0.01 for a file in centimetres
    /// that doesn't say so).
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub up_axis: UpAxis,
    #[serde(default)]
    pub anchored: bool,
    #[serde(default)]
    pub collision: ImportCollision,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            bring_in: BringIn::ByFile,
            weld: false,
            animator: true,
            scale: 1.0,
            up_axis: UpAxis::Y,
            anchored: false,
            collision: ImportCollision::Default,
        }
    }
}

impl ModelSettings {
    /// The scale to use: 1 for one that isn't a positive number.
    pub fn scale(&self) -> f32 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    /// A few words on what it does, for a menu's tooltip.
    pub fn describe(&self) -> String {
        let mut parts = vec![self.bring_in.label().to_owned()];
        if self.weld {
            parts.push("welded".to_owned());
        }
        if !self.animator {
            parts.push("no Animator".to_owned());
        }
        if self.scale() != 1.0 {
            parts.push(format!("scaled ×{}", self.scale()));
        }
        if self.up_axis == UpAxis::Z {
            parts.push("Z up".to_owned());
        }
        if self.anchored {
            parts.push("Anchored".to_owned());
        }
        if self.collision != ImportCollision::Default {
            parts.push(format!("{} collision", self.collision.item()));
        }
        parts.join(", ") + "."
    }
}

/// The model presets every project has, in menu order.
pub fn built_in_model_presets() -> [(&'static str, ModelSettings); 4] {
    [
        (DEFAULT_MODEL_PRESET, ModelSettings::default()),
        (
            "Static prop",
            ModelSettings {
                bring_in: BringIn::Merged,
                anchored: true,
                collision: ImportCollision::Hull,
                ..ModelSettings::default()
            },
        ),
        (
            "Building kit",
            ModelSettings {
                bring_in: BringIn::Model,
                weld: true,
                anchored: true,
                collision: ImportCollision::PreciseConvexDecomposition,
                ..ModelSettings::default()
            },
        ),
        (
            "Character",
            ModelSettings {
                bring_in: BringIn::Merged,
                collision: ImportCollision::Box,
                ..ModelSettings::default()
            },
        ),
    ]
}

impl ImportPresets {
    /// Every model preset's name: the built-in ones, then the project's own.
    pub fn model_names(&self) -> Vec<String> {
        let mut names: Vec<String> = built_in_model_presets()
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        for name in self.model_presets.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// Every model preset by name, in menu order.
    pub fn model_presets_in_order(&self) -> Vec<(String, ModelSettings)> {
        self.model_names()
            .into_iter()
            .filter_map(|name| Some((name.clone(), self.model_settings(&name)?)))
            .collect()
    }

    /// What the model preset `name` does, if there's one by that name.
    pub fn model_settings(&self, name: &str) -> Option<ModelSettings> {
        self.model_presets.get(name).copied().or_else(|| {
            built_in_model_presets()
                .into_iter()
                .find(|(built_in, _)| *built_in == name)
                .map(|(_, settings)| settings)
        })
    }

    /// The file's bytes, as written to `import-presets.json`.
    pub fn encode(&self) -> Vec<u8> {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text.into_bytes()
    }
}

/// The bytes of `project`'s `import-presets.json` with the model preset
/// `name` saved as `settings` (taking the place of one by that name), the
/// rest of the file kept. An error says why a file that's there can't be
/// read, so it isn't written over.
pub fn presets_file_with_model_preset(
    project: &Path,
    name: &str,
    settings: ModelSettings,
) -> Result<Vec<u8>, String> {
    let mut presets = ImportPresets::read(project)?;
    presets.model_presets.insert(name.to_owned(), settings);
    Ok(presets.encode())
}

#[derive(Serialize, Deserialize)]
struct LastPreset {
    preset: String,
}

/// The model preset last used in `project`, if one was remembered.
pub fn last_model_preset(project: &Path) -> Option<String> {
    let bytes = fs::read(project.join(LAST_MODEL_PRESET_FILE)).ok()?;
    let last: LastPreset = serde_json::from_slice(&bytes).ok()?;
    Some(last.preset)
}

/// Remembers `name` as the model preset last used in `project`.
pub fn remember_model_preset(project: &Path, name: &str) -> io::Result<()> {
    if last_model_preset(project).as_deref() == Some(name) {
        return Ok(());
    }
    let path = project.join(LAST_MODEL_PRESET_FILE);
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string(&LastPreset {
        preset: name.to_owned(),
    })
    .expect("plain data");
    fs::write(path, text + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PRESETS_FILE;

    fn project(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-model-presets-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn built_in_presets_are_there_and_a_projects_own_join_or_replace_them() {
        let presets = ImportPresets::default();
        assert_eq!(
            presets.model_names(),
            ["Default", "Static prop", "Building kit", "Character"]
        );
        let prop = presets.model_settings("Static prop").unwrap();
        assert_eq!(prop.bring_in, BringIn::Merged);
        assert!(prop.anchored);
        assert_eq!(prop.collision, ImportCollision::Hull);
        assert_eq!(presets.model_settings("Nope"), None);

        let text = r#"{
            "format_version": 1,
            "model_presets": {
                "Kitbash": { "bring_in": "model", "weld": true, "scale": 0.01, "up_axis": "z" },
                "Character": { "bring_in": "separate" }
            }
        }"#;
        let presets: ImportPresets = serde_json::from_str(text).unwrap();
        assert_eq!(
            presets.model_names(),
            [
                "Default",
                "Static prop",
                "Building kit",
                "Character",
                "Kitbash"
            ]
        );
        let kitbash = presets.model_settings("Kitbash").unwrap();
        assert_eq!(kitbash.scale(), 0.01);
        assert_eq!(kitbash.up_axis, UpAxis::Z);
        assert!(kitbash.animator, "left out, it's on");
        assert_eq!(
            presets.model_settings("Character").unwrap().bring_in,
            BringIn::Separate
        );
        assert_eq!(kitbash.describe(), "A Model, welded, scaled ×0.01, Z up.");
        // A scale that isn't a positive number is 1.
        let bad = ModelSettings {
            scale: -2.0,
            ..ModelSettings::default()
        };
        assert_eq!(bad.scale(), 1.0);
    }

    #[test]
    fn saving_a_preset_keeps_the_texture_presets_and_reads_back() {
        let project = project("save");
        fs::write(
            project.join(PRESETS_FILE),
            r#"{ "format_version": 1, "folders": { "assets/ui": "UI and pixel art" } }"#,
        )
        .unwrap();
        let settings = ModelSettings {
            bring_in: BringIn::Separate,
            anchored: true,
            ..ModelSettings::default()
        };
        let bytes = presets_file_with_model_preset(&project, "Rocks", settings).unwrap();
        fs::write(project.join(PRESETS_FILE), &bytes).unwrap();
        let presets = ImportPresets::read(&project).unwrap();
        assert_eq!(presets.model_settings("Rocks"), Some(settings));
        assert_eq!(
            presets.folders.get("assets/ui").map(String::as_str),
            Some("UI and pixel art")
        );
        // A file that can't be read isn't written over.
        fs::write(project.join(PRESETS_FILE), b"{ not json").unwrap();
        assert!(presets_file_with_model_preset(&project, "Rocks", settings).is_err());
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn the_last_preset_used_is_remembered() {
        let project = project("last");
        assert_eq!(last_model_preset(&project), None);
        remember_model_preset(&project, "Static prop").unwrap();
        assert_eq!(last_model_preset(&project).as_deref(), Some("Static prop"));
        remember_model_preset(&project, "Default").unwrap();
        assert_eq!(last_model_preset(&project).as_deref(), Some("Default"));
        let _ = fs::remove_dir_all(&project);
    }
}
