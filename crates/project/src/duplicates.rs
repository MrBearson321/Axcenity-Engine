//! Asset files that are byte-for-byte the same (roadmap R12): the same
//! texture or model imported twice under different names. Asset Check
//! lists each extra copy, and Merge points everything at one copy and
//! deletes the other (`asset_refs::plan_merge`). Nothing here writes.

use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use crate::terrain_files::TERRAIN_FOLDER;
use axcenity_assets::{AUDIO_EXTENSIONS, TEXTURE_EXTENSIONS};

use crate::validate::files_under;

/// Whether `file` is a texture, a self-contained model or a sound.
fn heavy(file: &str) -> bool {
    let Some((_, extension)) = file.rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    ["glb", "fbx"].contains(&extension.as_str())
        || TEXTURE_EXTENSIONS.contains(&extension.as_str())
        || AUDIO_EXTENSIONS.contains(&extension.as_str())
}

/// Files of the same content: the copy to keep first (the shortest path,
/// then the first by name), then the others, and the size of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicates {
    pub keep: String,
    pub copies: Vec<String>,
    pub bytes: u64,
}

/// Every set of identical textures, models (`.glb`, `.fbx`; not `.gltf`,
/// which names files beside it) and sounds under `assets/`, biggest
/// first: the files a project can carry twice by importing the same thing
/// again. Left out: Terrains' data folders (each Terrain owns its own),
/// small JSON assets (clips, materials: Keep Both makes copies of those on
/// purpose) and empty files (the stand-ins a cooked-only build leaves).
pub fn find_duplicates(folder: &Path) -> Vec<Duplicates> {
    let terrain = format!("{TERRAIN_FOLDER}/");
    let mut by_size: HashMap<u64, Vec<String>> = HashMap::new();
    for file in files_under(folder, "assets") {
        if !heavy(&file) || file.starts_with(&terrain) {
            continue;
        }
        let Ok(meta) = fs::metadata(folder.join(&file)) else {
            continue;
        };
        if meta.is_file() && meta.len() > 0 {
            by_size.entry(meta.len()).or_default().push(file);
        }
    }
    let mut found = Vec::new();
    for (bytes, files) in by_size {
        if files.len() < 2 {
            continue;
        }
        // Same size: read each and group by content. Files are compared
        // whole, so a hash collision can't make two different files one.
        let mut groups: Vec<(u64, Vec<u8>, Vec<String>)> = Vec::new();
        for file in files {
            let Ok(content) = fs::read(folder.join(&file)) else {
                continue;
            };
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            let hash = hasher.finish();
            match groups
                .iter_mut()
                .find(|(other, first, _)| *other == hash && *first == content)
            {
                Some((_, _, same)) => same.push(file),
                None => groups.push((hash, content, vec![file])),
            }
        }
        for (_, _, mut same) in groups {
            if same.len() < 2 {
                continue;
            }
            same.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            let keep = same.remove(0);
            found.push(Duplicates {
                keep,
                copies: same,
                bytes,
            });
        }
    }
    found.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.keep.cmp(&b.keep)));
    found
}
