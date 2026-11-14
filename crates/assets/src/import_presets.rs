//! Import presets (roadmap R12): how a texture file is cooked, picked per
//! file or per folder instead of only by its name.
//!
//! A preset says how a texture is compressed (by its name, as color, as a
//! normal map, or not at all) and the largest size its cooked copy keeps.
//! Four are built in; a project can add its own, or change a built-in one,
//! in `import-presets.json` at its top, and give a folder a preset there:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "presets": { "Terrain detail": { "compression": "color", "max_size": 2048 } },
//!   "folders": { "assets/images": "UI and pixel art" }
//! }
//! ```
//!
//! A file's own pick is kept in its sidecar (`"preset"`, see `asset_id.rs`),
//! so it moves with the file. A file without one takes the preset of the
//! deepest folder above it that has one, and otherwise Default. Only the
//! cooker (`axcenity-cook`) acts on presets; loading reads what it made.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{read_preset, takes_asset_id};

/// The project file that holds its own presets and folders' presets.
pub const PRESETS_FILE: &str = "import-presets.json";

/// The preset a file has when nothing picks one.
pub const DEFAULT_PRESET: &str = "Default";

const FORMAT_VERSION: u32 = 1;

/// How a texture's cooked copy is compressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    /// BC5 for a file whose name says it's a normal map, BC7 otherwise.
    #[default]
    Auto,
    /// BC7, whatever its name.
    Color,
    /// BC5 (red and green), whatever its name.
    NormalMap,
    /// Not cooked: the file is decoded and drawn as it is, every pixel
    /// kept exactly (UI, pixel art), at four times the GPU memory.
    None,
}

impl Compression {
    /// Its name in menus.
    pub fn label(self) -> &'static str {
        match self {
            Compression::Auto => "By name",
            Compression::Color => "Color (BC7)",
            Compression::NormalMap => "Normal map (BC5)",
            Compression::None => "None",
        }
    }
}

/// What a preset does to a texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSettings {
    #[serde(default)]
    pub compression: Compression,
    /// The largest width or height the cooked copy keeps: a bigger file is
    /// halved until it fits. Not used when it isn't compressed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
}

impl TextureSettings {
    /// A few words on what it does, for a menu's tooltip.
    pub fn describe(self) -> String {
        match (self.compression, self.max_size) {
            (Compression::None, _) => "Not compressed: every pixel exactly as drawn.".to_owned(),
            (compression, None) => format!("Compressed: {}.", compression.label()),
            (compression, Some(size)) => {
                format!(
                    "Compressed: {}, at most {size}×{size}.",
                    compression.label()
                )
            }
        }
    }
}

/// The presets every project has, in menu order.
pub fn built_in_presets() -> [(&'static str, TextureSettings); 4] {
    [
        (DEFAULT_PRESET, TextureSettings::default()),
        (
            "Normal map",
            TextureSettings {
                compression: Compression::NormalMap,
                max_size: None,
            },
        ),
        (
            "UI and pixel art",
            TextureSettings {
                compression: Compression::None,
                max_size: None,
            },
        ),
        (
            "Small (1024)",
            TextureSettings {
                compression: Compression::Auto,
                max_size: Some(1024),
            },
        ),
    ]
}

/// A project's `import-presets.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportPresets {
    #[serde(default = "format_version")]
    pub format_version: u32,
    /// The project's own presets by name; one named like a built-in one
    /// takes its place.
    #[serde(default)]
    pub presets: BTreeMap<String, TextureSettings>,
    /// Folders (project paths such as `assets/images`) and the preset of
    /// the files under them.
    #[serde(default)]
    pub folders: BTreeMap<String, String>,
    /// The project's own model presets by name (P7.3, `model_presets.rs`);
    /// one named like a built-in one takes its place.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_presets: BTreeMap<String, crate::ModelSettings>,
}

fn format_version() -> u32 {
    FORMAT_VERSION
}

impl ImportPresets {
    /// The project's presets file, or none (only the built-in presets) if
    /// it has none. An error says why a file that's there can't be read.
    pub fn read(project: &Path) -> Result<ImportPresets, String> {
        let bytes = match fs::read(project.join(PRESETS_FILE)) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ImportPresets::default());
            }
            Err(err) => return Err(err.to_string()),
        };
        let presets: ImportPresets =
            serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
        if presets.format_version > FORMAT_VERSION {
            return Err(format!(
                "it's format {}, newer than this editor reads",
                presets.format_version
            ));
        }
        Ok(presets)
    }

    /// [`read`](Self::read), with only the built-in presets when the file
    /// can't be read.
    pub fn read_or_default(project: &Path) -> ImportPresets {
        ImportPresets::read(project).unwrap_or_default()
    }

    /// Every preset's name: the built-in ones, then the project's own.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = built_in_presets()
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        for name in self.presets.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// What the preset `name` does, if there's one by that name.
    pub fn settings(&self, name: &str) -> Option<TextureSettings> {
        self.presets.get(name).copied().or_else(|| {
            built_in_presets()
                .into_iter()
                .find(|(built_in, _)| *built_in == name)
                .map(|(_, settings)| settings)
        })
    }

    /// The preset the project file `relative` has: its sidecar's pick, else
    /// that of the deepest folder above it given one, else Default.
    pub fn preset_of(&self, project: &Path, relative: &str) -> String {
        read_preset(project, relative)
            .or_else(|| self.folder_preset(relative))
            .unwrap_or_else(|| DEFAULT_PRESET.to_owned())
    }

    /// The preset of the deepest folder above `relative` given one.
    pub fn folder_preset(&self, relative: &str) -> Option<String> {
        self.folders
            .iter()
            .filter(|(folder, _)| {
                let folder = folder.trim_end_matches('/');
                relative.len() > folder.len()
                    && relative.starts_with(folder)
                    && relative.as_bytes()[folder.len()] == b'/'
            })
            .max_by_key(|(folder, _)| folder.trim_end_matches('/').len())
            .map(|(_, preset)| preset.clone())
    }

    /// What cooking does to the project file `relative`: its preset's
    /// settings, or Default's when the preset isn't there any more.
    pub fn texture_settings(&self, project: &Path, relative: &str) -> TextureSettings {
        self.settings(&self.preset_of(project, relative))
            .unwrap_or_default()
    }
}

/// The sidecar bytes that give the project file `relative` the preset
/// `preset` (`None`: no pick of its own), keeping its asset ID (a new one if
/// it has none yet). `None` for a file that takes no ID, so has no sidecar.
pub fn sidecar_with_preset(
    project: &Path,
    relative: &str,
    preset: Option<&str>,
) -> Option<Vec<u8>> {
    if !takes_asset_id(relative) {
        return None;
    }
    Some(crate::asset_id::encode_with_preset(
        project, relative, preset,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_asset_id;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-presets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/images/icons")).unwrap();
        folder
    }

    #[test]
    fn a_files_own_pick_beats_its_folders_which_beats_default() {
        let project = folder("pick");
        fs::write(
            project.join(PRESETS_FILE),
            r#"{
                "format_version": 1,
                "presets": { "Tiny": { "compression": "color", "max_size": 64 } },
                "folders": { "assets/images": "UI and pixel art", "assets/images/icons": "Tiny" }
            }"#,
        )
        .unwrap();
        let presets = ImportPresets::read(&project).unwrap();
        assert_eq!(presets.names().last().unwrap(), "Tiny");

        let plain = "assets/textures/rock.png";
        let ui = "assets/images/button.png";
        let icon = "assets/images/icons/star.png";
        assert_eq!(presets.preset_of(&project, plain), DEFAULT_PRESET);
        assert_eq!(presets.preset_of(&project, ui), "UI and pixel art");
        // The deepest folder wins, and `assets/images2` isn't under `assets/images`.
        assert_eq!(presets.preset_of(&project, icon), "Tiny");
        assert_eq!(presets.folder_preset("assets/images2/a.png"), None);
        assert_eq!(
            presets.texture_settings(&project, icon),
            TextureSettings {
                compression: Compression::Color,
                max_size: Some(64)
            }
        );

        // Its own pick, in its sidecar, wins and keeps its ID.
        fs::write(project.join(ui), b"png").unwrap();
        let id = crate::give_asset_id(&project, ui).unwrap().unwrap();
        let bytes = sidecar_with_preset(&project, ui, Some("Small (1024)")).unwrap();
        fs::write(project.join(crate::import_path(ui)), bytes).unwrap();
        assert_eq!(read_asset_id(&project, ui), Some(id));
        assert_eq!(presets.preset_of(&project, ui), "Small (1024)");
        assert_eq!(presets.texture_settings(&project, ui).max_size, Some(1024));
        // A preset that's gone is Default.
        let bytes = sidecar_with_preset(&project, ui, Some("Gone")).unwrap();
        fs::write(project.join(crate::import_path(ui)), bytes).unwrap();
        assert_eq!(
            presets.texture_settings(&project, ui),
            TextureSettings::default()
        );
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn a_missing_file_is_only_the_built_in_presets_and_a_bad_one_says_why() {
        let project = folder("bad");
        let none = ImportPresets::read(&project).unwrap();
        assert_eq!(none.names().len(), built_in_presets().len());
        fs::write(
            project.join(PRESETS_FILE),
            r#"{"presets": {"A": {"size": 3}}}"#,
        )
        .unwrap();
        assert!(ImportPresets::read(&project).is_err());
        assert_eq!(
            ImportPresets::read_or_default(&project),
            ImportPresets::default()
        );
        let _ = fs::remove_dir_all(&project);
    }
}
