//! Stable asset IDs (roadmap L2): each imported file under `assets/` (a
//! model, texture, sky, sound or font) gets an ID that stays with it when
//! it's renamed or moved, kept in a sidecar next to it
//! (`ship.glb` has `ship.glb.import.json`).
//!
//! References still name paths, so a project without sidecars loads as it
//! always has. The IDs are how the editor notices a file that was renamed
//! or moved outside it: `.axcenity/asset-ids.json` remembers each path's
//! ID, and a path that's gone whose ID is now somewhere else was moved
//! there ([`moved_assets`]), which Asset Check offers as the fix.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::cooked::write_whole;
use crate::{AUDIO_EXTENSIONS, HDR_SKY_EXTENSIONS, MESH_EDIT_FOLDER_SUFFIX, TEXTURE_EXTENSIONS};

/// What a sidecar's name adds to its file's.
pub const IMPORT_SUFFIX: &str = ".import.json";

/// Where the editor remembers which ID each path had, inside the project.
pub const KNOWN_IDS_FILE: &str = ".axcenity/asset-ids.json";

const SCHEME: &str = "ax://";

/// Crockford's base 32, in lower case: no `i`, `l`, `o` or `u`.
const DIGITS: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// 128 bits as 26 digits of base 32.
const DIGIT_COUNT: usize = 26;

const FORMAT_VERSION: u32 = 1;

/// An asset's stable ID, written `ax://` and 26 digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(u128);

impl AssetId {
    /// A new ID, different from every other one made.
    pub fn new() -> AssetId {
        // `RandomState` is seeded from the system's randomness; the time
        // and a count keep two made at once apart even so.
        static MADE: AtomicU64 = AtomicU64::new(0);
        let made = MADE.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let half = |salt: u64| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u128(now);
            hasher.write_u64(made);
            hasher.write_u64(salt);
            hasher.write_u32(std::process::id());
            hasher.finish()
        };
        AssetId(u128::from(half(1)) << 64 | u128::from(half(2)))
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut digits = [0u8; DIGIT_COUNT];
        for (place, digit) in digits.iter_mut().rev().enumerate() {
            *digit = DIGITS[((self.0 >> (place * 5)) & 31) as usize];
        }
        write!(
            f,
            "{SCHEME}{}",
            std::str::from_utf8(&digits).expect("ASCII")
        )
    }
}

impl FromStr for AssetId {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || format!("`{text}` isn't an asset ID (ax:// and 26 letters and digits)");
        let digits = text.strip_prefix(SCHEME).ok_or_else(bad)?;
        if digits.len() != DIGIT_COUNT {
            return Err(bad());
        }
        let mut value: u128 = 0;
        for (index, byte) in digits.bytes().enumerate() {
            let digit = DIGITS
                .iter()
                .position(|&known| known == byte.to_ascii_lowercase())
                .ok_or_else(bad)?;
            // The first digit holds only the top 3 bits (26 × 5 = 130).
            if index == 0 && digit > 7 {
                return Err(bad());
            }
            value = value << 5 | digit as u128;
        }
        Ok(AssetId(value))
    }
}

impl Serialize for AssetId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A sidecar's contents.
#[derive(Serialize, Deserialize)]
struct ImportFile {
    format_version: u32,
    uid: AssetId,
}

/// The project path of the sidecar of the file at `relative`.
pub fn import_path(relative: &str) -> String {
    format!("{relative}{IMPORT_SUFFIX}")
}

/// Whether `path` is a sidecar.
pub fn is_import_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(IMPORT_SUFFIX)
}

/// Whether the file at `path` is one that gets an ID: a model, texture,
/// sky, sound or font file. The project's own JSON files (materials,
/// prefabs, clips, tables) don't.
pub fn takes_asset_id(path: &str) -> bool {
    let Some((_, extension)) = path.rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    let extension = extension.as_str();
    ["gltf", "glb", "fbx", "ttf", "otf"].contains(&extension)
        || TEXTURE_EXTENSIONS.contains(&extension)
        || HDR_SKY_EXTENSIONS.contains(&extension)
        || AUDIO_EXTENSIONS.contains(&extension)
}

/// A sidecar's bytes for `id`.
pub fn encode_import_file(id: AssetId) -> Vec<u8> {
    let file = ImportFile {
        format_version: FORMAT_VERSION,
        uid: id,
    };
    let mut text = serde_json::to_string_pretty(&file).expect("plain data");
    text.push('\n');
    text.into_bytes()
}

/// The ID of the project file `relative`, if it has a sidecar that can be
/// read.
pub fn read_asset_id(project: &Path, relative: &str) -> Option<AssetId> {
    let bytes = fs::read(project.join(import_path(relative))).ok()?;
    let file: ImportFile = serde_json::from_slice(&bytes).ok()?;
    Some(file.uid)
}

/// Every file under the project's `assets/` that takes an ID, as project
/// paths with `/` between folders, in order.
pub fn id_files(project: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![("assets".to_owned(), project.join("assets"))];
    while let Some((relative, folder)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{relative}/{name}");
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !name.ends_with(MESH_EDIT_FOLDER_SUFFIX) {
                    folders.push((path, entry.path()));
                }
            } else if takes_asset_id(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Each file under `assets/` that has an ID, with it.
pub fn asset_ids(project: &Path) -> BTreeMap<String, AssetId> {
    id_files(project)
        .into_iter()
        .filter_map(|path| Some((read_asset_id(project, &path)?, path)))
        .map(|(id, path)| (path, id))
        .collect()
}

/// Gives every file under `assets/` that takes an ID one, writing its
/// sidecar, and remembers each path's ID in [`KNOWN_IDS_FILE`]. A file
/// that has another's ID (a copy made outside the editor, sidecar and all)
/// gets a new one; the file that had it last time keeps it. Returns how
/// many sidecars were written.
pub fn give_asset_ids(project: &Path) -> io::Result<usize> {
    let known = read_known(project);
    let mut ids: BTreeMap<String, Option<AssetId>> = id_files(project)
        .into_iter()
        .map(|path| {
            let id = read_asset_id(project, &path);
            (path, id)
        })
        .collect();

    // One file keeps each ID: the one that had it before, else the first.
    let mut holders: BTreeMap<AssetId, Vec<String>> = BTreeMap::new();
    for (path, id) in &ids {
        if let Some(id) = id {
            holders.entry(*id).or_default().push(path.clone());
        }
    }
    for (id, paths) in holders {
        let keeper = paths
            .iter()
            .find(|path| known.get(*path) == Some(&id))
            .unwrap_or(&paths[0])
            .clone();
        for path in paths {
            if path != keeper {
                ids.insert(path, None);
            }
        }
    }

    let mut written = 0;
    let mut current = BTreeMap::new();
    for (path, id) in ids {
        let id = match id {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                write_whole(&project.join(import_path(&path)), &encode_import_file(id))?;
                written += 1;
                id
            }
        };
        current.insert(path, id);
    }
    remember(project, &known, current)?;
    Ok(written)
}

/// Keeps `current` as the known IDs, with the paths known before that are
/// gone but whose IDs are somewhere now: those were moved, and Asset Check
/// still has to point their references at the new place.
fn remember(
    project: &Path,
    known: &BTreeMap<String, AssetId>,
    mut current: BTreeMap<String, AssetId>,
) -> io::Result<()> {
    let live: BTreeSet<AssetId> = current.values().copied().collect();
    for (path, id) in known {
        if !current.contains_key(path) && !project.join(path).exists() && live.contains(id) {
            current.insert(path.clone(), *id);
        }
    }
    if &current == known {
        return Ok(());
    }
    let mut text = serde_json::to_string_pretty(&current).expect("plain data");
    text.push('\n');
    write_whole(&project.join(KNOWN_IDS_FILE), text.as_bytes())
}

/// The IDs the editor last saw, by path.
fn read_known(project: &Path) -> BTreeMap<String, AssetId> {
    fs::read(project.join(KNOWN_IDS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Files that were renamed or moved outside the editor, sidecar and all:
/// each known path that's gone, with where its ID is now. Reads only.
pub fn moved_assets(project: &Path) -> BTreeMap<String, String> {
    let known = read_known(project);
    let gone: Vec<(&String, &AssetId)> = known
        .iter()
        .filter(|(path, _)| !project.join(path).exists())
        .collect();
    if gone.is_empty() {
        return BTreeMap::new();
    }
    let now: BTreeMap<AssetId, String> = asset_ids(project)
        .into_iter()
        .map(|(path, id)| (id, path))
        .collect();
    gone.into_iter()
        .filter_map(|(path, id)| Some((path.clone(), now.get(id)?.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-asset-ids-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/models")).unwrap();
        fs::create_dir_all(folder.join("assets/textures")).unwrap();
        folder
    }

    #[test]
    fn an_id_reads_back_from_its_text() {
        let id = AssetId::new();
        let text = id.to_string();
        assert!(text.starts_with("ax://"));
        assert_eq!(text.len(), 5 + DIGIT_COUNT);
        assert_eq!(text.parse::<AssetId>(), Ok(id));
        assert_eq!(
            text.to_uppercase().replace("AX://", "ax://").parse(),
            Ok(id)
        );
        assert_ne!(AssetId::new(), id);
        assert_eq!(
            AssetId(u128::MAX).to_string().parse(),
            Ok(AssetId(u128::MAX))
        );
        for bad in [
            "",
            "ax://",
            "ax://0",
            "rbx://00000000000000000000000000",
            "ax://8zzzzzzzzzzzzzzzzzzzzzzzzz",
            "ax://0000000000000000000000000u",
        ] {
            assert!(bad.parse::<AssetId>().is_err(), "{bad}");
        }
    }

    #[test]
    fn imported_files_get_ids_once_and_project_files_none() {
        let project = project("give");
        for file in [
            "assets/models/ship.glb",
            "assets/textures/wood.png",
            "assets/data/items.data.json",
            "assets/materials/wood.material.json",
        ] {
            fs::create_dir_all(project.join(file).parent().unwrap()).unwrap();
            fs::write(project.join(file), file).unwrap();
        }
        assert_eq!(give_asset_ids(&project).unwrap(), 2);
        let ids = asset_ids(&project);
        assert_eq!(
            ids.keys().collect::<Vec<_>>(),
            ["assets/models/ship.glb", "assets/textures/wood.png"]
        );
        assert!(project.join("assets/models/ship.glb.import.json").is_file());
        // A second pass keeps them.
        assert_eq!(give_asset_ids(&project).unwrap(), 0);
        assert_eq!(asset_ids(&project), ids);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_file_moved_outside_the_editor_is_found_by_its_id() {
        let project = project("moved");
        fs::write(project.join("assets/models/ship.glb"), "ship").unwrap();
        give_asset_ids(&project).unwrap();
        let id = read_asset_id(&project, "assets/models/ship.glb").unwrap();

        // Renamed and moved, sidecar and all, in a file manager.
        fs::create_dir_all(project.join("assets/models/boats")).unwrap();
        for (from, to) in [
            ("assets/models/ship.glb", "assets/models/boats/schooner.glb"),
            (
                "assets/models/ship.glb.import.json",
                "assets/models/boats/schooner.glb.import.json",
            ),
        ] {
            fs::rename(project.join(from), project.join(to)).unwrap();
        }
        let moved = BTreeMap::from([(
            "assets/models/ship.glb".to_owned(),
            "assets/models/boats/schooner.glb".to_owned(),
        )]);
        assert_eq!(moved_assets(&project), moved);
        // Another pass remembers the new path and still knows the old one.
        assert_eq!(give_asset_ids(&project).unwrap(), 0);
        assert_eq!(
            read_asset_id(&project, "assets/models/boats/schooner.glb"),
            Some(id)
        );
        assert_eq!(moved_assets(&project), moved);

        // Once it's deleted, nothing was moved.
        fs::remove_file(project.join("assets/models/boats/schooner.glb")).unwrap();
        give_asset_ids(&project).unwrap();
        assert!(moved_assets(&project).is_empty());
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_copy_with_its_sidecar_gets_a_new_id_and_the_original_keeps_its_own() {
        let project = project("copied");
        fs::write(project.join("assets/textures/b.png"), "b").unwrap();
        give_asset_ids(&project).unwrap();
        let id = read_asset_id(&project, "assets/textures/b.png").unwrap();
        // Copied before itself in name order, so "first" would pick the copy.
        for (from, to) in [
            ("assets/textures/b.png", "assets/textures/a.png"),
            (
                "assets/textures/b.png.import.json",
                "assets/textures/a.png.import.json",
            ),
        ] {
            fs::copy(project.join(from), project.join(to)).unwrap();
        }
        assert_eq!(give_asset_ids(&project).unwrap(), 1);
        assert_eq!(read_asset_id(&project, "assets/textures/b.png"), Some(id));
        let copy = read_asset_id(&project, "assets/textures/a.png").unwrap();
        assert_ne!(copy, id);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_damaged_sidecar_is_replaced() {
        let project = project("damaged");
        fs::write(project.join("assets/textures/a.png"), "a").unwrap();
        fs::write(project.join("assets/textures/a.png.import.json"), "{").unwrap();
        assert_eq!(read_asset_id(&project, "assets/textures/a.png"), None);
        assert_eq!(give_asset_ids(&project).unwrap(), 1);
        assert!(read_asset_id(&project, "assets/textures/a.png").is_some());
        let _ = fs::remove_dir_all(project);
    }
}
