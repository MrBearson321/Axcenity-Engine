//! Levels (Phase 48): `world.json` is the start level, and each other
//! level is `levels/<name>.world.json`, a world file of its own.
//!
//! A level is its Workspace. The services outside it (ReplicatedStorage,
//! Lighting, Players, StarterPlayer, Teams) are the project's, saved in
//! `world.json`, and stay the same from level to level, so a level file
//! only has a `workspace` section ([`write_level`]); any other section in
//! one is left unused. The start level's name is the manifest's
//! `start_level` ([`DEFAULT_START_LEVEL`] when it has none).
//!
//! The editor opens a level as the project's world with that level's
//! Workspace ([`load_with_level`]) and saves it back to both files
//! ([`save_level`]). Games get every level from [`load_levels`].

use std::fs;
use std::path::Path;

use axcenity_core::{InstanceId, World};

use crate::world_file::{parse_world, write_workspace_only, write_world};
use crate::{
    LoadError, LoadStep, Project, SaveError, WORLD_FILE, check_script_name, load_project,
    load_terrains, read, update_manifest, write_through_temporary,
};

/// The folder the levels other than the start level are in.
pub const LEVELS_FOLDER: &str = "levels";
/// A level's file ends in this; the part before it is its name.
pub const LEVEL_SUFFIX: &str = ".world.json";
/// The start level's name when the manifest doesn't give one.
pub const DEFAULT_START_LEVEL: &str = "Start";

/// The file of the level `name`, relative to the project folder with `/`
/// between folders: `levels/Dungeon.world.json`.
pub fn level_path(name: &str) -> String {
    format!("{LEVELS_FOLDER}/{name}{LEVEL_SUFFIX}")
}

/// The file the level `name` is saved in: [`WORLD_FILE`] for the start
/// level, else [`level_path`].
pub fn level_file(start: &str, name: &str) -> String {
    if name == start {
        WORLD_FILE.to_owned()
    } else {
        level_path(name)
    }
}

/// The names of the levels in `levels/`, sorted. The start level isn't
/// among them.
pub fn level_names(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder.join(LEVELS_FOLDER)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            let name = file.strip_suffix(LEVEL_SUFFIX)?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect();
    names.sort();
    names
}

/// Whether `name` can be a new level's name beside `start` and `existing`,
/// or why not. Names become file names, with a script's rules.
pub fn check_level_name(name: &str, start: &str, existing: &[String]) -> Result<(), String> {
    check_script_name(name).map_err(|why| why.replace("script", "level"))?;
    let taken = |other: &str| other.eq_ignore_ascii_case(name);
    if taken(start) || existing.iter().any(|other| taken(other)) {
        return Err(format!("there's already a level called \"{name}\""));
    }
    Ok(())
}

/// The text of a level file for `world`: only its Workspace.
pub fn write_level(world: &World) -> String {
    write_workspace_only(world)
}

/// The text of a new level: the Baseplate template's Workspace, a grey
/// baseplate to build on.
pub fn new_level_text() -> String {
    let world = parse_world(crate::templates::BASEPLATE_WORLD).expect("the template loads");
    write_level(&world)
}

/// Reads the level `name` (not the start level) as a world of its own,
/// with its terrains' landscapes.
pub fn read_level(folder: &Path, name: &str) -> Result<World, LoadError> {
    let path = folder.join(level_path(name));
    let text = read(&path)?;
    let mut world = parse_world(&text).map_err(|source| LoadError::World { path, source })?;
    // A prefab that can't be read leaves its copies as saved.
    let root = world.root();
    crate::Prefabs::in_folder(folder).refresh(&mut world, root);
    let mut problems = Vec::new();
    load_terrains(folder, &mut world, &mut problems);
    match problems.into_iter().next() {
        Some(problem) => Err(problem),
        None => Ok(world),
    }
}

/// The start level, `world.json`, as a world of its own with its terrains'
/// landscapes (for the editor's Play while another level is open).
pub fn read_start_level(folder: &Path) -> Result<World, LoadError> {
    let path = folder.join(WORLD_FILE);
    let text = read(&path)?;
    let mut world = parse_world(&text).map_err(|source| LoadError::World { path, source })?;
    // A prefab that can't be read leaves its copies as saved.
    let root = world.root();
    crate::Prefabs::in_folder(folder).refresh(&mut world, root);
    let mut problems = Vec::new();
    load_terrains(folder, &mut world, &mut problems);
    match problems.into_iter().next() {
        Some(problem) => Err(problem),
        None => Ok(world),
    }
}

/// Every level in `levels/`, by name, each a world of its own. One that
/// can't be read is left out and added to `problems`.
pub fn load_levels(folder: &Path, problems: &mut Vec<LoadError>) -> Vec<(String, World)> {
    level_names(folder)
        .into_iter()
        .filter_map(|name| match read_level(folder, &name) {
            Ok(world) => Some((name, world)),
            Err(problem) => {
                problems.push(problem);
                None
            }
        })
        .collect()
}

/// Puts the Workspace of `level` in place of `world`'s: its children (not
/// scripts) and its own properties. Nothing else in `world` changes.
pub fn replace_workspace(world: &mut World, level: &World) {
    let workspace = world.workspace();
    let old: Vec<InstanceId> = world
        .get(workspace)
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default();
    for child in old {
        let _ = world.destroy(child);
    }
    copy_workspace_into(world, level, workspace);
}

/// Copies `level`'s Workspace children into `parent`, and its Workspace
/// properties onto `world`'s Workspace when `parent` is it.
pub fn copy_workspace_into(world: &mut World, level: &World, parent: InstanceId) {
    let from = level.workspace();
    if parent == world.workspace()
        && let Some(instance) = level.get(from)
    {
        let properties: Vec<_> = instance
            .properties()
            .map(|(name, value)| (name, value.clone()))
            .collect();
        for (name, value) in properties {
            let _ = world.set_property(parent, name, value);
        }
    }
    let children = level
        .get(from)
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default();
    for child in children {
        if let Ok(copy) = world.copy_from(level, child) {
            let _ = world.set_parent(copy, Some(parent));
        }
    }
}

/// [`crate::load`], with the level `level` (or the start level for
/// `None`) in the Workspace. The rest of the project (services, scripts,
/// UI graphs of the level's ScreenGuis) loads as always; a level that can't
/// be read leaves the Workspace empty and is listed in `problems`, like a
/// damaged `world.json`.
pub fn load_with_level(
    folder: &Path,
    level: Option<&str>,
    progress: &mut dyn FnMut(LoadStep),
) -> Result<Project, LoadError> {
    load_project(folder, level, progress)
}

/// Saves `world` with the level `level` open (`None` is the start level):
/// its Workspace to that level's file, and the project's services to
/// `world.json`, whose Workspace (the start level's) stays as it is there.
/// A `world.json` that can't be read is left alone.
pub fn save_level(folder: &Path, level: Option<&str>, world: &World) -> Result<(), SaveError> {
    let Some(name) = level else {
        return crate::save_world(folder, world);
    };
    let levels = folder.join(LEVELS_FOLDER);
    fs::create_dir_all(&levels).map_err(SaveError::new(&levels))?;
    write_through_temporary(&folder.join(level_path(name)), &write_level(world))?;
    if let Some(start) = fs::read_to_string(folder.join(WORLD_FILE))
        .ok()
        .and_then(|text| parse_world(&text).ok())
    {
        let mut services = world.clone();
        replace_workspace(&mut services, &start);
        write_through_temporary(&folder.join(WORLD_FILE), &write_world(&services))?;
    }
    Ok(())
}

/// A project file's text before and after a change, by its path from the
/// project folder; `None` is no file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Makes `name` the start level: its Workspace moves into `world.json`, and
/// the old start level's goes to `levels/<old start>.world.json`. The
/// manifest's `start_level` becomes `name`. Returns every file this
/// changes, with its text before and after (`None` for no file), so the
/// editor can undo it; nothing is written when it fails.
pub fn set_start_level(
    folder: &Path,
    start: &str,
    name: &str,
) -> Result<Vec<FileChange>, LoadError> {
    let world_path = folder.join(WORLD_FILE);
    let world_text = read(&world_path)?;
    let start_world = parse_world(&world_text).map_err(|source| LoadError::World {
        path: world_path.clone(),
        source,
    })?;
    let level_text = read(&folder.join(level_path(name)))?;
    let level = parse_world(&level_text).map_err(|source| LoadError::World {
        path: folder.join(level_path(name)),
        source,
    })?;
    let mut new_start = start_world.clone();
    replace_workspace(&mut new_start, &level);
    let manifest_path = folder.join(crate::MANIFEST_FILE);
    let manifest = read(&manifest_path)?;
    let change = |path: String, before: Option<String>, after: Option<String>| FileChange {
        path,
        before,
        after,
    };
    let mut changes = vec![
        change(
            WORLD_FILE.to_owned(),
            Some(world_text),
            Some(write_world(&new_start)),
        ),
        change(level_path(name), Some(level_text), None),
        change(
            level_path(start),
            fs::read_to_string(folder.join(level_path(start))).ok(),
            Some(write_level(&start_world)),
        ),
    ];
    let mut json: serde_json::Value =
        serde_json::from_str(&manifest).map_err(|source| LoadError::Manifest {
            path: manifest_path.clone(),
            source,
        })?;
    if let Some(object) = json.as_object_mut() {
        object.insert("start_level".to_owned(), serde_json::Value::from(name));
    }
    let after = serde_json::to_string_pretty(&json).expect("a manifest is plain JSON");
    changes.push(change(
        crate::MANIFEST_FILE.to_owned(),
        Some(manifest),
        Some(after),
    ));
    Ok(changes)
}

/// Writes (or, for an `after` of `None`, removes) each file of `changes`,
/// as [`set_start_level`] lists them.
pub fn write_files(folder: &Path, changes: &[FileChange]) -> Result<(), SaveError> {
    for change in changes {
        let disk = folder.join(&change.path);
        match &change.after {
            Some(text) => {
                if let Some(parent) = disk.parent() {
                    fs::create_dir_all(parent).map_err(SaveError::new(parent))?;
                }
                write_through_temporary(&disk, text)?;
            }
            None => match fs::remove_file(&disk) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(SaveError::new(&disk)(err)),
            },
        }
    }
    Ok(())
}

/// Renames the file of the level `name` to `levels/<name>.backup.json` (or
/// `<name>.backup2.json` and so on), as [`crate::keep_world_file`] does for
/// `world.json`. Returns the new name, or `None` if there was no file.
pub fn keep_level_file(folder: &Path, name: &str) -> Result<Option<String>, SaveError> {
    let path = folder.join(level_path(name));
    if !path.exists() {
        return Ok(None);
    }
    let kept = (1u32..)
        .map(|n| match n {
            1 => format!("{LEVELS_FOLDER}/{name}.backup.json"),
            n => format!("{LEVELS_FOLDER}/{name}.backup{n}.json"),
        })
        .find(|kept| !folder.join(kept).exists())
        .expect("some number is always free");
    let disk = folder.join(&kept);
    fs::rename(&path, &disk).map_err(SaveError::new(&disk))?;
    Ok(Some(kept))
}

/// Changes the start level's name in the manifest.
pub fn set_start_level_name(folder: &Path, name: &str) -> Result<(), SaveError> {
    update_manifest(folder, "start_level", serde_json::Value::from(name))
}

#[cfg(test)]
mod tests;
