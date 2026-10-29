//! `plugin:ReadProjectFile` and `WriteProjectFile`: a plugin with the
//! `files` permission reads and writes text files inside the open project,
//! and nowhere else.
//!
//! A path is relative to the project folder, with `/` between folders. It
//! can't be absolute, go up a folder (`..`), or lead out of the project
//! through a link: the folder it ends up in, with links followed, must be
//! inside the project folder with links followed.

use std::path::{Component, Path, PathBuf};

/// Where `path` (as a plugin gave it) is on disk, if it's inside `project`.
pub fn project_file(project: &Path, path: &str) -> Result<PathBuf, String> {
    let refused = || format!("\"{path}\" is outside the project");
    if path.is_empty() || path.contains('\\') || path.contains(':') {
        return Err(refused());
    }
    let relative = Path::new(path);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(refused());
    }
    let root = project
        .canonicalize()
        .map_err(|err| format!("the project folder can't be read: {err}"))?;
    let full = root.join(relative);
    // The deepest folder (or the file) that exists, with links followed,
    // must still be inside the project.
    let mut existing = full.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or_else(refused)?;
    }
    let resolved = existing.canonicalize().map_err(|_| refused())?;
    if !resolved.starts_with(&root) {
        return Err(refused());
    }
    Ok(full)
}

/// The text of the project file `path`.
pub fn read(project: &Path, path: &str) -> Result<String, String> {
    let file = project_file(project, path)?;
    std::fs::read_to_string(&file).map_err(|err| format!("couldn't read \"{path}\": {err}"))
}

/// Writes `text` to the project file `path`, making its folders.
pub fn write(project: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = project_file(project, path)?;
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|err| format!("couldn't write \"{path}\": {err}"))?;
    }
    std::fs::write(&file, text).map_err(|err| format!("couldn't write \"{path}\": {err}"))
}
