//! Finding the project's asset files and what kind each one is.

use axcenity_assets::{AUDIO_EXTENSIONS, TEXTURE_EXTENSIONS};
use axcenity_blocks::behavior_tree::BEHAVIOR_TREE_SUFFIX;

use super::*;

const MODEL_EXTENSIONS: [&str; 3] = ["gltf", "glb", "fbx"];

/// What kind of asset the file at `path` (in the project, with `/`
/// between folders) is, if it's one.
pub fn kind_of(path: &str) -> Option<AssetKind> {
    let lower = path.to_lowercase();
    if lower.ends_with(PREFAB_SUFFIX) {
        return Some(AssetKind::Prefab);
    }
    if lower.ends_with(axcenity_assets::TEXTURE_SET_SUFFIX) {
        return Some(AssetKind::TextureSet);
    }
    if lower.ends_with(axcenity_assets::MATERIAL_SUFFIX) {
        return Some(AssetKind::Material);
    }
    if lower.ends_with(axcenity_anim::CLIP_EXTENSION) {
        return Some(AssetKind::Animation);
    }
    if lower.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
        return Some(AssetKind::Controller);
    }
    if lower.ends_with(BEHAVIOR_TREE_SUFFIX) {
        return Some(AssetKind::BehaviorTree);
    }
    if lower.ends_with(axcenity_assets::DATA_TABLE_SUFFIX) {
        return Some(AssetKind::DataTable);
    }
    let extension = lower.rsplit_once('.')?.1;
    // PNG and JPEG skies work too, but look like any other image here.
    if axcenity_assets::HDR_SKY_EXTENSIONS.contains(&extension) {
        return Some(AssetKind::Sky);
    }
    if MODEL_EXTENSIONS.contains(&extension) {
        Some(AssetKind::Model)
    } else if AUDIO_EXTENSIONS.contains(&extension) {
        Some(AssetKind::Audio)
    } else if TEXTURE_EXTENSIONS.contains(&extension) {
        Some(if lower.starts_with(&format!("{PARTICLES_FOLDER}/")) {
            AssetKind::Particle
        } else if lower.starts_with("assets/textures/") || lower.starts_with("assets/models/") {
            AssetKind::Texture
        } else {
            AssetKind::Image
        })
    } else {
        None
    }
}

/// A file's name without its extension (or `.prefab.json`).
pub fn display_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let lower = file.to_lowercase();
    for suffix in [
        PREFAB_SUFFIX,
        axcenity_assets::TEXTURE_SET_SUFFIX,
        axcenity_assets::MATERIAL_SUFFIX,
        axcenity_anim::CLIP_EXTENSION,
        axcenity_anim::STATE_MACHINE_EXTENSION,
        BEHAVIOR_TREE_SUFFIX,
        axcenity_assets::DATA_TABLE_SUFFIX,
    ] {
        if lower.ends_with(suffix) {
            return file[..file.len() - suffix.len()].to_owned();
        }
    }
    file.rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_owned()
}

/// Every asset file in `folder`'s `assets` folder, sorted by path.
pub fn scan(folder: &Path) -> Vec<AssetEntry> {
    let mut found = Vec::new();
    walk(&folder.join("assets"), "assets", &mut found);
    let set_folders: Vec<String> = found
        .iter()
        .filter(|entry| entry.kind == AssetKind::TextureSet)
        .filter_map(|entry| {
            entry
                .path
                .rsplit_once('/')
                .map(|(folder, _)| folder.to_owned())
        })
        .collect();
    // The set is one tile. Its images are not listed again.
    found.retain(|entry| {
        if !matches!(
            entry.kind,
            AssetKind::Image | AssetKind::Texture | AssetKind::Particle
        ) {
            return true;
        }
        entry
            .path
            .rsplit_once('/')
            .is_none_or(|(folder, _)| !set_folders.iter().any(|set| set == folder))
    });
    found.sort_by_key(|entry| entry.path.to_lowercase());
    found
}

fn walk(directory: &Path, prefix: &str, found: &mut Vec<AssetEntry>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{prefix}/{name}");
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(&entry.path(), &path, found);
        } else if let Some(kind) = kind_of(&path) {
            found.push(AssetEntry {
                kind,
                name: display_name(&path),
                path,
            });
        }
    }
}
