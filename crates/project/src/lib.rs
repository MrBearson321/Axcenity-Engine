//! Axcenity project folders: reading the manifest and world file, building
//! the Instance tree from them, and saving the tree back.
//!
//! A project folder looks like this:
//!
//! ```text
//! my-game/
//!   axcenity.json   the manifest: project name and format version
//!   world.json      the Instances in the world
//!   scripts/        `*.server.lua` files, in subfolders if you like
//! ```

mod world_file;

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, World};
use serde::Deserialize;

pub use world_file::{WorldError, parse_world, write_world};

pub const MANIFEST_FILE: &str = "axcenity.json";
pub const WORLD_FILE: &str = "world.json";
pub const SCRIPTS_FOLDER: &str = "scripts";
/// Files ending in this are server scripts. The part before it is the Script's name.
pub const SERVER_SCRIPT_SUFFIX: &str = ".server.lua";

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
    /// Every server script, sorted by path.
    pub scripts: Vec<ScriptFile>,
}

/// A script file from the project's `scripts` folder.
pub struct ScriptFile {
    /// The Script Instance made for this file, in `ServerScriptService`.
    pub instance: InstanceId,
    /// Where the file is, relative to the project folder and with `/` between
    /// folders on every OS, for example `scripts/doors/open.server.lua`.
    /// Error messages show this path.
    pub path: String,
    /// The Lua code.
    pub source: String,
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
    let mut world = parse_world(&world_text).map_err(|source| LoadError::World {
        path: world_path,
        source,
    })?;
    let scripts = load_scripts(folder, &mut world)?;

    Ok(Project {
        folder: folder.to_owned(),
        manifest,
        world,
        scripts,
    })
}

/// Reads every server script and gives each one a Script Instance in
/// `ServerScriptService`. Subfolders only organize the files: every Script
/// goes directly into `ServerScriptService` (there is no Folder class yet).
fn load_scripts(folder: &Path, world: &mut World) -> Result<Vec<ScriptFile>, LoadError> {
    let mut found = Vec::new();
    find_server_scripts(&folder.join(SCRIPTS_FOLDER), SCRIPTS_FOLDER, &mut found)?;
    // Folder listings come back in a different order on each OS; sorting makes
    // scripts start in the same order everywhere.
    found.sort();

    let mut scripts = Vec::new();
    for (path, disk_path) in found {
        let source = read(&disk_path)?;
        let file_name = path.rsplit('/').next().unwrap_or(&path);
        let name = file_name
            .strip_suffix(SERVER_SCRIPT_SUFFIX)
            .unwrap_or(file_name);

        let instance = world
            .create(ClassName::Script)
            .expect("Script is not a service");
        world.set_name(instance, name).expect("just created");
        world
            .set_parent(instance, Some(world.server_script_service()))
            .expect("both exist");
        scripts.push(ScriptFile {
            instance,
            path,
            source,
        });
    }
    Ok(scripts)
}

/// Adds every `*.server.lua` file in `dir` and its subfolders to `found`, as
/// (path to show, path on disk). `shown` is how `dir` itself is shown.
fn find_server_scripts(
    dir: &Path,
    shown: &str,
    found: &mut Vec<(String, PathBuf)>,
) -> Result<(), LoadError> {
    let read_error = |source| LoadError::Read {
        path: dir.to_owned(),
        source,
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // A project without a `scripts` folder simply has no scripts.
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(read_error(err)),
    };

    for entry in entries {
        let entry = entry.map_err(read_error)?;
        let disk_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let shown = format!("{shown}/{file_name}");
        if disk_path.is_dir() {
            find_server_scripts(&disk_path, &shown, found)?;
        } else if file_name.ends_with(SERVER_SCRIPT_SUFFIX) {
            found.push((shown, disk_path));
        }
    }
    Ok(())
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
    fn project_without_a_scripts_folder_has_no_scripts() {
        let project = load(&examples_folder().join("hello-part")).unwrap();
        assert!(project.scripts.is_empty());
    }

    #[test]
    fn server_scripts_become_script_instances_sorted_by_path() {
        let folder = temp_folder("scripts");
        fs::write(
            folder.join(MANIFEST_FILE),
            r#"{"name": "T", "format_version": 1}"#,
        )
        .unwrap();
        fs::write(folder.join(WORLD_FILE), r#"{"format_version": 1}"#).unwrap();
        let scripts = folder.join(SCRIPTS_FOLDER);
        fs::create_dir_all(scripts.join("doors")).unwrap();
        fs::write(scripts.join("main.server.lua"), "print('main')").unwrap();
        fs::write(
            scripts.join("doors").join("open.server.lua"),
            "print('door')",
        )
        .unwrap();
        fs::write(scripts.join("notes.txt"), "not a script").unwrap();
        fs::write(
            scripts.join("camera.client.lua"),
            "-- client scripts come later",
        )
        .unwrap();

        let project = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();

        let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            ["scripts/doors/open.server.lua", "scripts/main.server.lua"]
        );
        assert_eq!(project.scripts[1].source, "print('main')");

        let world = &project.world;
        let service = world.server_script_service();
        let names: Vec<&str> = world
            .get(service)
            .unwrap()
            .children()
            .iter()
            .map(|&id| world.get(id).unwrap().name())
            .collect();
        assert_eq!(names, ["open", "main"]);
        for script in &project.scripts {
            let instance = world.get(script.instance).unwrap();
            assert_eq!(instance.class(), ClassName::Script);
            assert_eq!(instance.parent(), Some(service));
        }
    }

    /// An empty folder in the system's temporary folder, unique to this test run.
    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-{name}-test-{}", std::process::id()));
        // Left over from an earlier run that failed partway, if it exists.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
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
