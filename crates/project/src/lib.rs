//! Axcenity project folders: reading the manifest and world file and
//! building the Instance tree from them.
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

pub use world_file::{WorldError, parse_world};

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

fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

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
}
