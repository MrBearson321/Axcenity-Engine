//! Terrains' heightfields and paint, each in its own folder (the Terrain's
//! `DataFolder`, such as `assets/terrain/Terrain`). See `terrain::files`
//! for what's in it.
//!
//! A terrain whose files can't be read loads flat (and is reported), so the
//! rest of the project still opens. Before such a folder is saved over, it
//! is kept as `<folder>.backup` ([`keep_terrain_folder`]), so nothing in it
//! is lost.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, TerrainData, Value, World};
use axcenity_terrain::files;

use crate::{LoadError, LoadStep, SaveError};

/// Where new terrains' folders go.
pub const TERRAIN_FOLDER: &str = "assets/terrain";

/// The data folder a Terrain names, if it names one.
pub fn data_folder(world: &World, id: InstanceId) -> Option<String> {
    let instance = world.get(id)?;
    let folder = instance.property("DataFolder").and_then(Value::as_str)?;
    (!folder.is_empty()).then(|| folder.to_owned())
}

/// True for a folder path inside the project: relative, with `/` between
/// names and no `..`.
pub fn is_safe_folder(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Gives every Terrain in `world`'s saved services the landscape in its
/// data folder. Returns the folders that couldn't be read (each also in
/// `problems`): those terrains load flat.
pub fn load_terrains(
    folder: &Path,
    world: &mut World,
    problems: &mut Vec<LoadError>,
) -> Vec<String> {
    load_terrains_reporting(folder, world, problems, &mut |_| {})
}

/// [`load_terrains`], telling `progress` about each terrain before it's read.
pub(crate) fn load_terrains_reporting(
    folder: &Path,
    world: &mut World,
    problems: &mut Vec<LoadError>,
    progress: &mut dyn FnMut(LoadStep),
) -> Vec<String> {
    let terrains: Vec<InstanceId> = [world.workspace(), world.replicated_storage()]
        .into_iter()
        .flat_map(|service| world.descendants(service))
        .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Terrain))
        .collect();
    let mut damaged = Vec::new();
    let total = terrains.len();
    for (done, id) in terrains.into_iter().enumerate() {
        let Some(data_folder) = data_folder(world, id) else {
            continue;
        };
        progress(LoadStep::Terrain {
            name: world.get(id).map_or("", |i| i.name()),
            done,
            total,
        });
        let disk = folder.join(&data_folder);
        let result = if is_safe_folder(&data_folder) {
            files::decode(|name| fs::read(disk.join(name)).ok()).map_err(|error| error.to_string())
        } else {
            Err("its DataFolder must be a folder inside the project".to_owned())
        };
        match result {
            Ok(data) => {
                world.set_terrain(id, Arc::new(data));
            }
            Err(message) => {
                problems.push(LoadError::Terrain {
                    path: disk,
                    message,
                });
                damaged.push(data_folder);
            }
        }
    }
    damaged
}

/// Writes `data` to `data_folder` (made if it isn't there). Each file goes
/// through a temporary file, so a failed save leaves the old one whole.
pub fn save_terrain(folder: &Path, data_folder: &str, data: &TerrainData) -> Result<(), SaveError> {
    let disk = folder.join(data_folder);
    if !is_safe_folder(data_folder) {
        return Err(SaveError::new(&disk)(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a terrain's DataFolder must be a folder inside the project",
        )));
    }
    fs::create_dir_all(&disk).map_err(SaveError::new(&disk))?;
    let encoded = files::encode(data);
    for (name, bytes) in &encoded {
        let path = disk.join(name);
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".saving");
        fs::write(&temporary, bytes).map_err(SaveError::new(&path))?;
        fs::rename(&temporary, &path).map_err(SaveError::new(&path))?;
    }
    // Foliage is an optional extra file: drop a leftover when this
    // landscape has none, so undoing every plant and saving clears it.
    if !encoded.iter().any(|(name, _)| *name == files::FOLIAGE_FILE) {
        let leftover = disk.join(files::FOLIAGE_FILE);
        if leftover.exists() {
            fs::remove_file(&leftover).map_err(SaveError::new(&leftover))?;
        }
    }
    Ok(())
}

/// Renames `data_folder` to `<data_folder>.backup` (or `.backup2`…), so
/// files that couldn't be loaded aren't lost when the terrain is saved over
/// them. Returns the new folder's path, or `None` if there was no folder.
pub fn keep_terrain_folder(folder: &Path, data_folder: &str) -> Result<Option<String>, SaveError> {
    let disk = folder.join(data_folder);
    if !disk.exists() {
        return Ok(None);
    }
    let name = (1u32..)
        .map(|n| match n {
            1 => format!("{data_folder}.backup"),
            n => format!("{data_folder}.backup{n}"),
        })
        .find(|name| !folder.join(name).exists())
        .expect("some number is always free");
    let kept = folder.join(&name);
    fs::rename(&disk, &kept).map_err(SaveError::new(&kept))?;
    Ok(Some(name))
}

/// A data folder for a new terrain called `name`: `assets/terrain/<name>`,
/// with a number added if that's on disk or in `taken` already.
pub fn new_terrain_folder(folder: &Path, name: &str, taken: &[String]) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let clean = clean.trim();
    let base = if clean.is_empty() { "Terrain" } else { clean };
    (1u32..)
        .map(|n| match n {
            1 => format!("{TERRAIN_FOLDER}/{base}"),
            n => format!("{TERRAIN_FOLDER}/{base}{n}"),
        })
        .find(|path| {
            !folder.join(path).exists() && !taken.iter().any(|t| t.eq_ignore_ascii_case(path))
        })
        .expect("some number is always free")
}

/// The disk path of a terrain's data folder, for messages.
pub fn terrain_folder_path(folder: &Path, data_folder: &str) -> PathBuf {
    folder.join(data_folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-project-terrain-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn world_with_terrain(data_folder: &str) -> (World, InstanceId) {
        let mut world = World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        world
            .set_property(terrain, "DataFolder", Value::String(data_folder.to_owned()))
            .unwrap();
        (world, terrain)
    }

    #[test]
    fn a_terrain_saves_and_loads_with_its_world() {
        let folder = temp_folder("round-trip");
        let (mut world, terrain) = world_with_terrain("assets/terrain/Hills");
        let data = world.terrain_mut(terrain).unwrap();
        let all = data.bounds();
        data.heights_mut_in(all)[1000] = 0.75;
        data.paint_mut_in(all)[1000] = [0, 200, 0, 0, 0, 0, 0, 55];
        let saved = world.terrain(terrain).unwrap().clone();
        save_terrain(&folder, "assets/terrain/Hills", &saved).unwrap();
        assert!(folder.join("assets/terrain/Hills/heights.png").exists());

        let mut loaded = crate::parse_world(&crate::write_world(&world)).unwrap();
        let id = loaded.descendants(loaded.workspace())[0];
        let mut problems = Vec::new();
        let damaged = load_terrains(&folder, &mut loaded, &mut problems);
        assert!(problems.is_empty() && damaged.is_empty());
        let data = loaded.terrain(id).unwrap();
        assert!((data.heights()[1000] - 0.75).abs() < 1e-4);
        assert_eq!(data.paint()[1000], [0, 200, 0, 0, 0, 0, 0, 55]);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn damaged_terrain_files_load_flat_and_are_kept_before_saving_over() {
        let folder = temp_folder("damaged");
        let (mut world, terrain) = world_with_terrain("assets/terrain/Broken");
        fs::create_dir_all(folder.join("assets/terrain/Broken")).unwrap();
        fs::write(folder.join("assets/terrain/Broken/terrain.json"), "{ oops").unwrap();
        let mut problems = Vec::new();
        let damaged = load_terrains(&folder, &mut world, &mut problems);
        assert_eq!(damaged, vec!["assets/terrain/Broken".to_owned()]);
        assert!(
            problems[0].to_string().contains("Broken"),
            "{}",
            problems[0]
        );
        assert_eq!(world.terrain(terrain).unwrap().size(), (257, 257));

        let kept = keep_terrain_folder(&folder, "assets/terrain/Broken").unwrap();
        assert_eq!(kept.as_deref(), Some("assets/terrain/Broken.backup"));
        assert!(
            folder
                .join("assets/terrain/Broken.backup/terrain.json")
                .exists()
        );
        assert_eq!(
            keep_terrain_folder(&folder, "assets/terrain/Broken").unwrap(),
            None
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn folders_must_stay_inside_the_project_and_new_ones_are_unique() {
        assert!(is_safe_folder("assets/terrain/Hills"));
        for bad in ["", "../outside", "/etc", "C:/x", "a\\b", "assets/../.."] {
            assert!(!is_safe_folder(bad), "{bad}");
        }
        let folder = temp_folder("names");
        fs::create_dir_all(folder.join("assets/terrain/Terrain")).unwrap();
        let taken = vec!["assets/terrain/Terrain2".to_owned()];
        assert_eq!(
            new_terrain_folder(&folder, "Terrain", &taken),
            "assets/terrain/Terrain3"
        );
        assert_eq!(
            new_terrain_folder(&folder, "My Island!", &[]),
            "assets/terrain/My Island"
        );
        let (world, terrain) = world_with_terrain("assets/terrain/Hills");
        assert_eq!(
            data_folder(&world, terrain).as_deref(),
            Some("assets/terrain/Hills")
        );
        assert!(save_terrain(&folder, "../escape", &TerrainData::default()).is_err());
        fs::remove_dir_all(folder).unwrap();
    }
}
