//! Global assets: a library of asset files in the user's own folder that
//! every project can use, so a model or texture set needn't be downloaded
//! or copied by hand into each project again.
//!
//! The library mirrors a project: its files are under `assets/` at the
//! same paths they have in a project, such as
//! `assets/models/Walking/Walking.fbx`. So an asset is global when the
//! library has a file at its path, and using a global asset in a project
//! copies the files it needs to the same paths there. Projects stay
//! self-contained: the player and exported games never read the library.
//!
//! An asset's own files are what making it global copies in and making it
//! local again removes: an imported model's folder (and its
//! `assets/animations/<name>/` folder), a texture set's folder, or the one
//! file. The files it needs are those plus every file its JSON (a prefab,
//! state machine or glTF) names, so a global prefab brings its models.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MODELS: &str = "assets/models/";
const ANIMATIONS: &str = "assets/animations/";

/// Where the library is: the editor's data folder, beside its settings.
pub fn default_folder() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|data| data.join("Global Assets"))
}

/// True if the library in `library` has the asset at `path`.
pub fn is_global(library: &Path, path: &str) -> bool {
    has(library, path)
}

/// Copies the asset at `path` in `project`, with what it needs, into the
/// library, replacing older copies there. Returns how many files it copied.
pub fn make_global(project: &Path, library: &Path, path: &str) -> Result<usize, String> {
    if !has(project, path) {
        return Err(format!("`{path}` isn't in this project"));
    }
    copy(project, library, &needed_files(project, path), true)
}

/// Deletes the asset at `path`'s own files from the library (the files it
/// only needs stay, as global assets of their own). Projects that used it
/// keep their copies. Returns how many files it deleted.
pub fn remove(library: &Path, path: &str) -> Result<usize, String> {
    let files = own_files(library, path);
    for file in &files {
        std::fs::remove_file(library.join(file))
            .map_err(|err| format!("could not delete `{file}` from global assets: {err}"))?;
        prune(library, file);
    }
    Ok(files.len())
}

/// Copies what the global asset at `path` needs into `project`, leaving
/// files the project already has alone. Returns how many files it copied:
/// 0 when the project has them all, or `path` isn't global.
pub fn use_in(library: &Path, project: &Path, path: &str) -> Result<usize, String> {
    if !is_global(library, path) {
        return Ok(0);
    }
    copy(library, project, &needed_files(library, path), false)
}

/// The asset at `path`'s own files in `folder` (a project or the library),
/// each as a path from `folder`, sorted.
pub fn own_files(folder: &Path, path: &str) -> Vec<String> {
    if !has(folder, path) {
        return Vec::new();
    }
    let lower = path.to_lowercase();
    let mut files = BTreeSet::new();
    let is_model =
        crate::asset_browser::kind_of(path) == Some(crate::asset_browser::AssetKind::Model);
    if let Some(rest) = lower.strip_prefix(MODELS).filter(|_| is_model) {
        // Its folder, or a file on its own (with its glTF buffers and
        // images, from `needed_files`).
        let name = match rest.split_once('/') {
            Some((name, _)) => {
                let folder_path = &path[..MODELS.len() + name.len()];
                files.extend(files_under(folder, folder_path));
                name
            }
            None => {
                files.insert(path.to_owned());
                rest.rsplit_once('.').map_or(rest, |(stem, _)| stem)
            }
        };
        if let Some(animations) = folder_named(folder, ANIMATIONS, name) {
            files.extend(files_under(folder, &animations));
        }
    } else if lower.ends_with(axcenity_assets::TEXTURE_SET_SUFFIX)
        && let Some((set_folder, _)) = path.rsplit_once('/')
        && set_folder != "assets"
    {
        files.extend(files_under(folder, set_folder));
    } else {
        files.insert(path.to_owned());
    }
    files.into_iter().collect()
}

/// The asset at `path`'s own files and every file they name, followed
/// through the files that names in turn.
pub fn needed_files(folder: &Path, path: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    let mut waiting = vec![path.to_owned()];
    while let Some(next) = waiting.pop() {
        for file in own_files(folder, &next) {
            if found.insert(file.clone()) {
                waiting.extend(references(folder, &file));
            }
        }
    }
    found.into_iter().collect()
}

/// The project files the JSON file at `path` names: any string that is a
/// path under `assets/`, and a glTF's `uri`s (next to it). Not JSON, none.
fn references(folder: &Path, path: &str) -> Vec<String> {
    let lower = path.to_lowercase();
    if !lower.ends_with(".json") && !lower.ends_with(".gltf") {
        return Vec::new();
    }
    let Some(json) = std::fs::read_to_string(folder.join(path))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return Vec::new();
    };
    let beside = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let mut strings = Vec::new();
    collect_strings(&json, None, &mut strings);
    strings
        .into_iter()
        .filter_map(|(key, text)| {
            let text = text.replace('\\', "/");
            if text.starts_with("assets/") {
                Some(text)
            } else if key == Some("uri") && !text.starts_with("data:") && !beside.is_empty() {
                Some(format!("{beside}/{text}"))
            } else {
                None
            }
        })
        .filter(|file| safe(file) && folder.join(file).is_file())
        .collect()
}

/// Every string in `value`, with the key it's under in an object.
fn collect_strings<'a>(
    value: &'a serde_json::Value,
    key: Option<&'a str>,
    found: &mut Vec<(Option<&'a str>, &'a str)>,
) {
    match value {
        serde_json::Value::String(text) => found.push((key, text)),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_strings(item, key, found);
            }
        }
        serde_json::Value::Object(fields) => {
            for (name, item) in fields {
                collect_strings(item, Some(name), found);
            }
        }
        _ => {}
    }
}

/// Copies `files` from `from` to `to`, replacing ones already there only
/// when `replace`. Returns how many it copied.
fn copy(from: &Path, to: &Path, files: &[String], replace: bool) -> Result<usize, String> {
    let mut copied = 0;
    for file in files {
        let target = to.join(file);
        if !replace && target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not make a folder for `{file}`: {err}"))?;
        }
        std::fs::copy(from.join(file), &target)
            .map_err(|err| format!("could not copy `{file}`: {err}"))?;
        copied += 1;
    }
    Ok(copied)
}

/// Every file under `directory` (a path from `folder`), hidden ones
/// skipped, each as a path from `folder`.
fn files_under(folder: &Path, directory: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder.join(directory)) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{directory}/{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => found.extend(files_under(folder, &path)),
            Ok(_) => found.push(path),
            Err(_) => {}
        }
    }
    found
}

/// The folder in `parent` (such as `assets/animations/`) called `name`,
/// ignoring case, as a path from `folder`.
fn folder_named(folder: &Path, parent: &str, name: &str) -> Option<String> {
    std::fs::read_dir(folder.join(parent))
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|found| found.eq_ignore_ascii_case(name))
        .map(|found| format!("{parent}{found}"))
}

/// Removes the folders above `file` that are now empty, up to `assets`.
fn prune(folder: &Path, file: &str) {
    let mut path = file;
    while let Some((parent, _)) = path.rsplit_once('/') {
        if parent == "assets" || std::fs::remove_dir(folder.join(parent)).is_err() {
            return;
        }
        path = parent;
    }
}

/// True if `folder` has a file at `path`.
fn has(folder: &Path, path: &str) -> bool {
    safe(path) && folder.join(path).is_file()
}

/// True for a path inside a folder's `assets` folder: no `..`, not absolute.
fn safe(path: &str) -> bool {
    path.starts_with("assets/") && !path.split('/').any(|part| part == ".." || part.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder in the temp directory with these files in it.
    fn folder_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-global-{name}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (file, text) in files {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        folder
    }

    #[test]
    fn a_global_model_brings_its_folder_and_clips_to_another_project() {
        let first = folder_with(
            "first",
            &[
                ("assets/models/Walking/Walking.fbx", "model"),
                ("assets/models/Walking/skin.png", "texture"),
                ("assets/animations/walking/Walk.anim.json", "{}"),
                ("assets/models/Other/Other.fbx", "not this"),
            ],
        );
        let library = folder_with("library", &[]);
        let second = folder_with("second", &[]);
        let model = "assets/models/Walking/Walking.fbx";

        assert!(!is_global(&library, model), "assets start local");
        assert_eq!(make_global(&first, &library, model), Ok(3));
        assert!(is_global(&library, model));
        assert!(!is_global(&library, "assets/models/Other/Other.fbx"));

        assert_eq!(use_in(&library, &second, model), Ok(3));
        for file in [
            model,
            "assets/models/Walking/skin.png",
            "assets/animations/walking/Walk.anim.json",
        ] {
            assert!(second.join(file).is_file(), "{file} copied");
        }
        // Used again, nothing is copied, and the project's own edits stay.
        std::fs::write(second.join("assets/models/Walking/skin.png"), "edited").unwrap();
        assert_eq!(use_in(&library, &second, model), Ok(0));
        assert_eq!(
            std::fs::read_to_string(second.join("assets/models/Walking/skin.png")).unwrap(),
            "edited"
        );

        // Local again: gone from the library, kept in both projects.
        assert_eq!(remove(&library, model), Ok(3));
        assert!(!is_global(&library, model));
        assert!(!library.join("assets/models").exists(), "empty folders go");
        assert!(first.join(model).is_file() && second.join(model).is_file());
        for folder in [first, library, second] {
            std::fs::remove_dir_all(folder).unwrap();
        }
    }

    #[test]
    fn a_global_prefab_brings_the_files_it_names_and_nothing_outside_assets() {
        let project = folder_with(
            "prefab",
            &[
                (
                    "assets/prefabs/Hut.prefab.json",
                    r#"{"instances":[{"MeshId":"assets/models/hut.gltf"},
                        {"Image":"assets/../secret.txt"}]}"#,
                ),
                (
                    "assets/models/hut.gltf",
                    r#"{"buffers":[{"uri":"hut.bin"}]}"#,
                ),
                ("assets/models/hut.bin", "buffer"),
                ("secret.txt", "not an asset"),
            ],
        );
        let library = folder_with("prefab-library", &[]);
        let prefab = "assets/prefabs/Hut.prefab.json";
        assert_eq!(
            needed_files(&project, prefab),
            [
                "assets/models/hut.bin",
                "assets/models/hut.gltf",
                "assets/prefabs/Hut.prefab.json",
            ]
        );
        assert_eq!(make_global(&project, &library, prefab), Ok(3));
        assert!(!library.join("secret.txt").exists());
        // Making the prefab local again leaves its model global.
        assert_eq!(remove(&library, prefab), Ok(1));
        assert!(is_global(&library, "assets/models/hut.gltf"));
        for folder in [project, library] {
            std::fs::remove_dir_all(folder).unwrap();
        }
    }

    #[test]
    fn a_texture_set_is_its_whole_folder() {
        let project = folder_with(
            "set",
            &[
                ("assets/textures/Rock/Rock.texture.json", "{}"),
                ("assets/textures/Rock/Rock_Color.jpg", "color"),
                ("assets/textures/Rock/Rock_NormalGL.jpg", "normal"),
                ("assets/textures/Grass.png", "other"),
            ],
        );
        assert_eq!(
            own_files(&project, "assets/textures/Rock/Rock.texture.json"),
            [
                "assets/textures/Rock/Rock.texture.json",
                "assets/textures/Rock/Rock_Color.jpg",
                "assets/textures/Rock/Rock_NormalGL.jpg",
            ]
        );
        assert_eq!(
            own_files(&project, "assets/textures/Grass.png"),
            ["assets/textures/Grass.png"]
        );
        assert!(own_files(&project, "assets/../axcenity.json").is_empty());
        std::fs::remove_dir_all(project).unwrap();
    }
}
