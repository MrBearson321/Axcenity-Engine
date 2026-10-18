//! Axcenity project folders: reading the manifest and world file, building
//! the Instance tree from them, and saving the tree back.
//!
//! A project folder looks like this:
//!
//! ```text
//! my-game/
//!   axcenity.json   the manifest: project name and format version
//!   world.json      the Instances in the world
//! ```

mod world_file;

use std::fs;
use std::path::{Path, PathBuf};

use axcenity_core::World;
use serde::Deserialize;

pub use world_file::{WorldError, parse_world, write_world};

pub const MANIFEST_FILE: &str = "axcenity.json";
pub const WORLD_FILE: &str = "world.json";

/// The newest project format this version of the engine understands.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub format_version: u32,
}

pub struct Project {
    pub folder: PathBuf,
    pub manifest: Manifest,
    pub world: World,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read `{}`", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{}` is not a valid project manifest", path.display())]
    Manifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("`{}` uses project format {found}, but this engine only understands up to {FORMAT_VERSION}; update Axcenity to open it", path.display())]
    TooNew { path: PathBuf, found: u32 },
    #[error("`{}` is not a valid world file", path.display())]
    World {
        path: PathBuf,
        #[source]
        source: WorldError,
    },
}

/// Loads the project in `folder`.
pub fn load(folder: &Path) -> Result<Project, LoadError> {
    let manifest_path = folder.join(MANIFEST_FILE);
    let manifest_text = read(&manifest_path)?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_text).map_err(|source| LoadError::Manifest {
            path: manifest_path.clone(),
            source,
        })?;
    if manifest.format_version > FORMAT_VERSION {
        return Err(LoadError::TooNew {
            path: manifest_path,
            found: manifest.format_version,
        });
    }

    let world_path = folder.join(WORLD_FILE);
    let world_text = read(&world_path)?;
    let world = parse_world(&world_text).map_err(|source| LoadError::World {
        path: world_path,
        source,
    })?;

    Ok(Project {
        folder: folder.to_owned(),
        manifest,
        world,
    })
}

#[derive(Debug, thiserror::Error)]
#[error("could not write `{}`", path.display())]
pub struct SaveError {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

/// Writes `world` to the world file in `folder`.
///
/// The text goes to a temporary file first, which then replaces the old
/// world file in one step. If saving fails partway, the old file is untouched.
pub fn save_world(folder: &Path, world: &World) -> Result<(), SaveError> {
    let path = folder.join(WORLD_FILE);
    let temporary = folder.join(format!("{WORLD_FILE}.saving"));
    let error = |source| SaveError {
        path: path.clone(),
        source,
    };

    fs::write(&temporary, write_world(world)).map_err(error)?;
    // `rename` replaces an existing file on Windows, macOS and Linux alike.
    fs::rename(&temporary, &path).map_err(error)
}

fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Material, PartType, Value};

    fn examples_folder() -> PathBuf {
        // CARGO_MANIFEST_DIR is this crate's folder: crates/project.
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
    }

    #[test]
    fn hello_part_example_loads() {
        let project = load(&examples_folder().join("hello-part")).unwrap();
        assert_eq!(project.manifest.name, "Hello Part");

        let world = &project.world;
        let parts: Vec<_> = world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| world.get(id).unwrap().class() == ClassName::Part)
            .collect();
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn missing_folder_names_the_manifest() {
        let err = load(&examples_folder().join("does-not-exist"))
            .err()
            .unwrap();
        assert!(matches!(err, LoadError::Read { .. }));
        assert!(err.to_string().contains(MANIFEST_FILE));
    }

    /// Every folder in `examples/` must load.
    fn every_example() -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = fs::read_dir(examples_folder())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        assert!(!folders.is_empty());
        folders
    }

    #[test]
    fn every_example_round_trips_through_saving() {
        for folder in every_example() {
            let first = write_world(&load(&folder).unwrap().world);
            let second = write_world(&parse_world(&first).unwrap());
            assert_eq!(first, second, "{} changed when saved", folder.display());
        }
    }

    /// The primitives example doubles as a reference for the saved layout.
    #[test]
    fn primitives_example_is_already_in_saved_form() {
        let folder = examples_folder().join("primitives");
        // Git may check text files out with Windows line endings.
        let text = fs::read_to_string(folder.join(WORLD_FILE))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            write_world(&load(&folder).unwrap().world),
            text,
            "re-save examples/primitives/world.json with `project::save_world`"
        );
    }

    #[test]
    fn primitives_example_has_every_shape_and_material() {
        let world = load(&examples_folder().join("primitives")).unwrap().world;
        let parts: Vec<_> = world
            .descendants(world.workspace())
            .into_iter()
            .map(|id| world.get(id).unwrap())
            .collect();
        for shape in PartType::ALL {
            let value = Value::PartType(shape);
            assert!(parts.iter().any(|p| p.property("Shape") == Some(&value)));
        }
        for material in Material::ALL {
            let value = Value::Material(material);
            assert!(parts.iter().any(|p| p.property("Material") == Some(&value)));
        }
    }

    #[test]
    fn save_then_load_gives_the_same_world_file() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-save-test-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let world = load(&examples_folder().join("primitives")).unwrap().world;

        save_world(&folder, &world).unwrap();
        let saved = fs::read_to_string(folder.join(WORLD_FILE)).unwrap();
        let leftover = folder.join(format!("{WORLD_FILE}.saving")).exists();
        fs::remove_dir_all(&folder).unwrap();

        assert_eq!(saved, write_world(&world));
        assert_eq!(write_world(&parse_world(&saved).unwrap()), saved);
        assert!(!leftover, "the temporary file was left behind");
    }
}
