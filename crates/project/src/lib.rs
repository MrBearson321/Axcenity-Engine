//! Axcenity project folders: reading the manifest and world file, building
//! the Instance tree from them, and saving the tree back.
//!
//! A project folder looks like this:
//!
//! ```text
//! my-game/
//!   axcenity.json   the manifest: project name and format version
//!   world.json      the Instances in the world
//!   scripts/        `*.server.lua`, `*.client.lua` and `*.module.lua`
//!                   files, in subfolders if you like
//! ```
//!
//! One damaged file never stops the rest from loading: [`load`] leaves it
//! out and lists it in [`Project::problems`].

mod world_file;

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, World};
use serde::Deserialize;

pub use world_file::{WorldError, parse_world, write_world};

pub const MANIFEST_FILE: &str = "axcenity.json";
pub const WORLD_FILE: &str = "world.json";
/// Where the editor keeps a world file it never loaded (it was damaged, or
/// changed outside the editor while the world had unsaved changes) before
/// saving over it, so nothing in it is lost. A number goes before `.json` if
/// this name is taken too.
pub const KEPT_WORLD_FILE: &str = "world.backup.json";
pub const SCRIPTS_FOLDER: &str = "scripts";
/// Files ending in this are server scripts. The part before it is the Script's name.
pub const SERVER_SCRIPT_SUFFIX: &str = ".server.lua";
/// Files ending in this are client scripts, which become LocalScripts.
pub const CLIENT_SCRIPT_SUFFIX: &str = ".client.lua";
/// Files ending in this are modules, which become ModuleScripts that other
/// scripts load with `require`.
pub const MODULE_SCRIPT_SUFFIX: &str = ".module.lua";

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
    /// Every script that could be read, in the order they start: modules
    /// first (so they can be required), then server scripts, then client
    /// scripts, each sorted by path (see [`script_order`]).
    pub scripts: Vec<ScriptFile>,
    /// The files that couldn't be loaded. Each was left out, and the rest
    /// of the project loaded without it.
    pub problems: Vec<LoadError>,
    /// False when the world file couldn't be loaded, so the Workspace
    /// started empty. Saving would then replace a file that may only need a
    /// small fix, so the editor keeps it first ([`keep_world_file`]).
    pub world_loaded: bool,
}

/// A script file from the project's `scripts` folder.
pub struct ScriptFile {
    /// The Script made for this file, in `ServerScriptService`, the
    /// LocalScript, in `StarterPlayer.StarterPlayerScripts`, or the
    /// ModuleScript, in `ReplicatedStorage`.
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
///
/// Only a missing or unreadable manifest (so this isn't a project folder),
/// or one from a newer engine, stops it. Any other file that can't be loaded
/// is left out and listed in [`Project::problems`]: a broken manifest gives
/// the project its folder's name, a broken world file an empty Workspace,
/// and a broken script no Script.
pub fn load(folder: &Path) -> Result<Project, LoadError> {
    let manifest_path = folder.join(MANIFEST_FILE);
    let manifest_text = read(&manifest_path)?;
    let mut problems = Vec::new();
    let manifest = match serde_json::from_str::<Manifest>(&manifest_text) {
        Ok(manifest) => manifest,
        Err(source) => {
            problems.push(LoadError::Manifest {
                path: manifest_path.clone(),
                source,
            });
            Manifest {
                name: folder_name(folder),
                format_version: FORMAT_VERSION,
            }
        }
    };
    if manifest.format_version > FORMAT_VERSION {
        return Err(LoadError::TooNew {
            path: manifest_path,
            found: manifest.format_version,
        });
    }

    let (mut world, world_loaded) = match load_world(folder) {
        Ok(world) => (world, true),
        Err(err) => {
            problems.push(err);
            (World::new(), false)
        }
    };
    let scripts = load_scripts(folder, &mut world, &mut problems);

    Ok(Project {
        folder: folder.to_owned(),
        manifest,
        world,
        scripts,
        problems,
        world_loaded,
    })
}

/// The name of the project in `folder`, from its manifest, without loading
/// anything else (for lists of projects). `None` if `folder` has no
/// readable manifest.
pub fn project_name(folder: &Path) -> Option<String> {
    let text = fs::read_to_string(folder.join(MANIFEST_FILE)).ok()?;
    let manifest = serde_json::from_str::<Manifest>(&text).ok()?;
    Some(manifest.name)
}

/// The last part of `folder`'s path, such as `my-game`.
fn folder_name(folder: &Path) -> String {
    // `canonicalize` turns `.` and `..` into real names first.
    let folder = fs::canonicalize(folder).unwrap_or_else(|_| folder.to_owned());
    folder.file_name().map_or_else(
        || "Untitled".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn load_world(folder: &Path) -> Result<World, LoadError> {
    let path = folder.join(WORLD_FILE);
    let text = read(&path)?;
    parse_world(&text).map_err(|source| LoadError::World { path, source })
}

/// Reads every script and gives each one its Script, LocalScript or
/// ModuleScript (see
/// [`add_script_instance`]). A file that can't be read is left out, with the
/// reason added to `problems`.
fn load_scripts(
    folder: &Path,
    world: &mut World,
    problems: &mut Vec<LoadError>,
) -> Vec<ScriptFile> {
    let mut scripts = Vec::new();
    for path in find_scripts(folder, problems) {
        match read_script(folder, &path) {
            Ok(source) => scripts.push(ScriptFile {
                instance: add_script_instance(world, &path),
                path,
                source,
            }),
            Err(err) => problems.push(err),
        }
    }
    scripts
}

/// The path of every `*.server.lua`, `*.client.lua` and `*.module.lua` file in the project
/// in `folder`, as [`ScriptFile::path`]s, in [`script_order`]. Folders that
/// can't be read are skipped and added to `problems`.
pub fn find_scripts(folder: &Path, problems: &mut Vec<LoadError>) -> Vec<String> {
    let mut found = Vec::new();
    find_script_files(
        &folder.join(SCRIPTS_FOLDER),
        SCRIPTS_FOLDER,
        &mut found,
        problems,
    );
    // Folder listings come back in a different order on each OS; sorting makes
    // scripts start in the same order everywhere.
    found.sort_by(|a, b| script_order(a).cmp(&script_order(b)));
    found
}

/// What scripts are sorted by: modules first (they only get ready to be
/// required, so they must be there before any script runs), then server
/// scripts before client scripts (a server is running before any player
/// joins), then by path. Sort with
/// `scripts.sort_by(|a, b| script_order(a).cmp(&script_order(b)))`.
pub fn script_order(path: &str) -> (u8, &str) {
    let rank = match script_class(path) {
        ClassName::ModuleScript => 0,
        ClassName::Script => 1,
        _ => 2,
    };
    (rank, path)
}

/// Adds the path of every script file in `dir` and its subfolders to
/// `found`. `shown` is how `dir` itself is shown.
fn find_script_files(
    dir: &Path,
    shown: &str,
    found: &mut Vec<String>,
    problems: &mut Vec<LoadError>,
) {
    let mut problem = |source| {
        problems.push(LoadError::Read {
            path: dir.to_owned(),
            source,
        })
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // A project without a `scripts` folder simply has no scripts.
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => return problem(err),
    };

    let mut folders = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                problem(err);
                continue;
            }
        };
        let disk_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let shown = format!("{shown}/{file_name}");
        if disk_path.is_dir() {
            // Visited after the loop, which still borrows `problems` through `problem`.
            folders.push((disk_path, shown));
        } else if [
            SERVER_SCRIPT_SUFFIX,
            CLIENT_SCRIPT_SUFFIX,
            MODULE_SCRIPT_SUFFIX,
        ]
        .iter()
        .any(|suffix| file_name.ends_with(suffix))
        {
            found.push(shown);
        }
    }
    for (disk_path, shown) in folders {
        find_script_files(&disk_path, &shown, found, problems);
    }
}

/// The class of the script in the file at `path`: LocalScript for a
/// `.client.lua` file, ModuleScript for a `.module.lua` file, otherwise Script.
pub fn script_class(path: &str) -> ClassName {
    if path.ends_with(CLIENT_SCRIPT_SUFFIX) {
        ClassName::LocalScript
    } else if path.ends_with(MODULE_SCRIPT_SUFFIX) {
        ClassName::ModuleScript
    } else {
        ClassName::Script
    }
}

/// The file ending for scripts of `class`: `.client.lua` for a LocalScript,
/// `.module.lua` for a ModuleScript, otherwise `.server.lua`.
pub fn script_suffix(class: ClassName) -> &'static str {
    match class {
        ClassName::LocalScript => CLIENT_SCRIPT_SUFFIX,
        ClassName::ModuleScript => MODULE_SCRIPT_SUFFIX,
        _ => SERVER_SCRIPT_SUFFIX,
    }
}

/// The name of the script for the file at `path`: its file name without
/// `.server.lua`, `.client.lua` or `.module.lua`, so
/// `scripts/doors/open.server.lua` is `open`.
pub fn script_name(path: &str) -> &str {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .strip_suffix(SERVER_SCRIPT_SUFFIX)
        .or_else(|| file_name.strip_suffix(CLIENT_SCRIPT_SUFFIX))
        .or_else(|| file_name.strip_suffix(MODULE_SCRIPT_SUFFIX))
        .unwrap_or(file_name)
}

/// The path of a new script of `class` (Script, LocalScript or ModuleScript) called
/// `name`, directly in the `scripts` folder.
pub fn script_path(name: &str, class: ClassName) -> String {
    format!("{SCRIPTS_FOLDER}/{name}{}", script_suffix(class))
}

/// The service the scripts of `class` go in: `ServerScriptService` for
/// Scripts, `StarterPlayer.StarterPlayerScripts` for LocalScripts, and
/// `ReplicatedStorage` for ModuleScripts, where both sides can require them.
pub fn script_service(world: &World, class: ClassName) -> InstanceId {
    match class {
        ClassName::LocalScript => world.starter_player_scripts(),
        ClassName::ModuleScript => world.replicated_storage(),
        _ => world.server_script_service(),
    }
}

/// Makes the Script (or, for a `.client.lua` file, the LocalScript, and for
/// a `.module.lua` file, the ModuleScript) for the
/// file at `path`, named after it, in its service (see [`script_service`]).
/// Subfolders only organize the files: every script goes directly into its
/// service.
pub fn add_script_instance(world: &mut World, path: &str) -> InstanceId {
    let class = script_class(path);
    let instance = world.create(class).expect("scripts are not services");
    world
        .set_name(instance, script_name(path))
        .expect("just created");
    world
        .set_parent(instance, Some(script_service(world, class)))
        .expect("both exist");
    instance
}

/// Reads the script file at `path` (a [`ScriptFile::path`]) in the project in
/// `folder` again, for the newest version of it.
pub fn read_script(folder: &Path, path: &str) -> Result<String, LoadError> {
    read(&folder.join(path))
}

/// Whether `name` can be a script's name, or why not, in words for the
/// Output panel. Script names become file names, so the rules are those of
/// file names on Windows, the strictest OS: then a project made on any OS
/// opens on every OS.
pub fn check_script_name(name: &str) -> Result<(), &'static str> {
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    if name.trim().is_empty() {
        return Err("a script needs a name");
    }
    if name.chars().count() > 100 {
        return Err("script names can be at most 100 characters long");
    }
    if let Some(bad) = name
        .chars()
        .find(|c| r#"/\:*?"<>|"#.contains(*c) || c.is_control())
    {
        return Err(if bad.is_control() {
            "script names can't contain control characters"
        } else {
            r#"script names can't contain any of / \ : * ? " < > |"#
        });
    }
    if name.ends_with('.') || name.ends_with(' ') || name.starts_with(' ') {
        return Err("script names can't start with a space or end with a dot or a space");
    }
    // Windows reserves device names such as `CON` and `COM1`, even with an
    // extension after them.
    let upper = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let numbered_device = (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.len() == 4
        && upper.ends_with(|c: char| c.is_ascii_digit() && c != '0');
    if RESERVED.contains(&upper.as_str()) || numbered_device {
        return Err("that name is reserved by Windows");
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

impl SaveError {
    fn new(path: &Path) -> impl FnOnce(std::io::Error) -> SaveError + '_ {
        move |source| SaveError {
            path: path.to_owned(),
            source,
        }
    }

    /// True if it failed because a file with that name is already there.
    pub fn already_exists(&self) -> bool {
        self.source.kind() == ErrorKind::AlreadyExists
    }
}

/// Makes a new script file called `name` (see [`check_script_name`]) for a
/// script of `class` (Script or LocalScript) with `source` in it, directly
/// in the `scripts` folder, and returns its path. Fails (see
/// [`SaveError::already_exists`]) if a file with that name is already there,
/// never replacing it. On Windows and macOS that includes a name that only
/// differs in capitals.
pub fn create_script(
    folder: &Path,
    name: &str,
    class: ClassName,
    source: &str,
) -> Result<String, SaveError> {
    let path = script_path(name, class);
    let disk_path = folder.join(&path);
    let error = SaveError::new(&disk_path);
    fs::create_dir_all(folder.join(SCRIPTS_FOLDER)).map_err(SaveError::new(&disk_path))?;
    // `create_new` checks and creates in one step, so nothing else can make
    // the file in between.
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&disk_path)
        .map_err(SaveError::new(&disk_path))?;
    file.write_all(source.as_bytes()).map_err(error)?;
    Ok(path)
}

/// Writes `source` to the script file at `path` (a [`ScriptFile::path`]),
/// making it if it isn't there. Like [`save_world`], a half-written file
/// never replaces the old one.
pub fn write_script(folder: &Path, path: &str, source: &str) -> Result<(), SaveError> {
    let disk_path = folder.join(path);
    if let Some(parent) = disk_path.parent() {
        fs::create_dir_all(parent).map_err(SaveError::new(&disk_path))?;
    }
    write_through_temporary(&disk_path, source)
}

/// Renames the script file at `path` to `new_name` (see
/// [`check_script_name`]), in the same folder and with the same ending
/// (`.server.lua` or `.client.lua`), and returns its new path. Fails rather
/// than replace another file.
pub fn rename_script(folder: &Path, path: &str, new_name: &str) -> Result<String, SaveError> {
    let new_file_name = format!("{new_name}{}", script_suffix(script_class(path)));
    let new_path = match path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{new_file_name}"),
        None => new_file_name.clone(),
    };
    if new_path == path {
        return Ok(new_path);
    }
    let (from, to) = (folder.join(path), folder.join(&new_path));
    // On Windows and macOS, `pad` and `Pad` are the same file, so `to`
    // "exists" when only capitals change. Listing the folder tells a second
    // file with exactly that name apart from the file itself.
    let only_capitals = new_path.to_lowercase() == path.to_lowercase();
    let listed = to
        .parent()
        .and_then(|dir| fs::read_dir(dir).ok())
        .is_some_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy() == new_file_name)
        });
    if listed || (to.exists() && !only_capitals) {
        return Err(SaveError {
            path: to,
            source: std::io::Error::new(
                ErrorKind::AlreadyExists,
                "a script with that name is already there",
            ),
        });
    }
    fs::rename(&from, &to).map_err(SaveError::new(&to))?;
    Ok(new_path)
}

/// Deletes the script file at `path` (a [`ScriptFile::path`]).
pub fn delete_script(folder: &Path, path: &str) -> Result<(), SaveError> {
    let disk_path = folder.join(path);
    fs::remove_file(&disk_path).map_err(SaveError::new(&disk_path))
}

/// Writes `world` to the world file in `folder`.
///
/// The text goes to a temporary file first, which then replaces the old
/// world file in one step. If saving fails partway, the old file is untouched.
pub fn save_world(folder: &Path, world: &World) -> Result<(), SaveError> {
    write_through_temporary(&folder.join(WORLD_FILE), &write_world(world))
}

/// Renames the world file in `folder` to [`KEPT_WORLD_FILE`] (or
/// `world.backup2.json` and so on, if that's taken), so a world file the
/// editor never loaded isn't lost when the world is saved over it. Returns
/// the new name, or `None` if there was no world file.
pub fn keep_world_file(folder: &Path) -> Result<Option<String>, SaveError> {
    let world_path = folder.join(WORLD_FILE);
    if !world_path.exists() {
        return Ok(None);
    }
    let stem = KEPT_WORLD_FILE.trim_end_matches(".json");
    let name = (1u32..)
        .map(|n| match n {
            1 => KEPT_WORLD_FILE.to_string(),
            n => format!("{stem}{n}.json"),
        })
        .find(|name| !folder.join(name).exists())
        .expect("some number is always free");
    let kept = folder.join(&name);
    fs::rename(&world_path, &kept).map_err(SaveError::new(&kept))?;
    Ok(Some(name))
}

/// Writes `text` to `path` through a temporary file next to it, which then
/// replaces `path` in one step.
fn write_through_temporary(path: &Path, text: &str) -> Result<(), SaveError> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".saving");
    let error = SaveError::new(path);
    fs::write(&temporary, text).map_err(SaveError::new(path))?;
    // `rename` replaces an existing file on Windows, macOS and Linux alike.
    fs::rename(&temporary, path).map_err(error)
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
        assert!(project.problems.is_empty() && project.world_loaded);

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

    /// A project with a manifest, an empty world and these script files.
    fn project_with_scripts(name: &str, scripts: &[(&str, &[u8])]) -> PathBuf {
        let folder = temp_folder(name);
        fs::write(
            folder.join(MANIFEST_FILE),
            r#"{"name": "T", "format_version": 1}"#,
        )
        .unwrap();
        fs::write(folder.join(WORLD_FILE), r#"{"format_version": 1}"#).unwrap();
        for (path, text) in scripts {
            let path = folder.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        folder
    }

    #[test]
    fn server_scripts_become_scripts_and_client_scripts_local_scripts() {
        let folder = project_with_scripts(
            "scripts",
            &[
                ("scripts/main.server.lua", b"print('main')"),
                ("scripts/doors/open.server.lua", b"print('door')"),
                ("scripts/notes.txt", b"not a script"),
                ("scripts/camera.client.lua", b"print('camera')"),
                ("scripts/a/keys.client.lua", b"print('keys')"),
            ],
        );
        let scripts = folder.join(SCRIPTS_FOLDER);

        let project = load(&folder);
        // Edited after loading: `read_script` gets the new text by its path.
        fs::write(scripts.join("main.server.lua"), "print('edited')").unwrap();
        let reread = read_script(&folder, "scripts/main.server.lua");
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();

        // Server scripts start first, then client scripts, each by path.
        let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "scripts/doors/open.server.lua",
                "scripts/main.server.lua",
                "scripts/a/keys.client.lua",
                "scripts/camera.client.lua",
            ]
        );
        assert_eq!(project.scripts[1].source, "print('main')");
        assert_eq!(reread.unwrap(), "print('edited')");

        let world = &project.world;
        let names = |service| -> Vec<&str> {
            world
                .get(service)
                .unwrap()
                .children()
                .iter()
                .map(|&id| world.get(id).unwrap().name())
                .collect()
        };
        assert_eq!(names(world.server_script_service()), ["open", "main"]);
        assert_eq!(names(world.starter_player_scripts()), ["keys", "camera"]);
        for script in &project.scripts {
            let instance = world.get(script.instance).unwrap();
            let class = script_class(&script.path);
            assert_eq!(instance.class(), class);
            assert_eq!(instance.parent(), Some(script_service(world, class)));
        }
    }

    #[test]
    fn module_files_become_module_scripts_in_replicated_storage_and_load_first() {
        let folder = project_with_scripts(
            "modules",
            &[
                ("scripts/main.server.lua", b"print('main')"),
                ("scripts/shared/utils.module.lua", b"return {}"),
                ("scripts/keys.client.lua", b"print('keys')"),
            ],
        );
        let project = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();

        let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "scripts/shared/utils.module.lua",
                "scripts/main.server.lua",
                "scripts/keys.client.lua",
            ]
        );
        let world = &project.world;
        let utils = world.get(project.scripts[0].instance).unwrap();
        assert_eq!(utils.class(), ClassName::ModuleScript);
        assert_eq!(utils.name(), "utils");
        assert_eq!(utils.parent(), Some(world.replicated_storage()));
        // ModuleScripts are files, so the world file doesn't save them.
        assert!(!write_world(world).contains("utils"));
        assert_eq!(
            script_path("utils", ClassName::ModuleScript),
            "scripts/utils.module.lua"
        );
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

    #[test]
    fn project_names_come_from_the_manifest() {
        let examples = examples_folder();
        assert_eq!(
            project_name(&examples.join("hello-touched")).as_deref(),
            Some("Hello Touched")
        );
        assert_eq!(project_name(&examples.join("does-not-exist")), None);
    }

    #[test]
    fn a_corrupted_world_file_is_reported_and_the_scripts_still_load() {
        let folder =
            project_with_scripts("broken-world", &[("scripts/a.server.lua", b"print('a')")]);
        fs::write(
            folder.join(WORLD_FILE),
            "{\n  \"format_version\": 1,\n  oops\n}",
        )
        .unwrap();
        let project = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();

        assert!(!project.world_loaded);
        assert_eq!(project.problems.len(), 1);
        let problem = format!("{:#}", anyhow_chain(&project.problems[0]));
        assert!(
            problem.contains("world.json") && problem.contains("line 3"),
            "{problem}"
        );
        // The Workspace is empty, and the script is there as usual.
        let world = &project.world;
        assert!(world.get(world.workspace()).unwrap().children().is_empty());
        assert_eq!(project.scripts.len(), 1);
        assert_eq!(project.scripts[0].source, "print('a')");
    }

    #[test]
    fn a_corrupted_script_is_reported_and_skipped_and_the_rest_still_load() {
        // 0xFF never appears in UTF-8 text, so this file can't be read as text.
        let folder = project_with_scripts(
            "broken-script",
            &[
                ("scripts/a.server.lua", b"print('a')"),
                ("scripts/broken.server.lua", b"print('\xFF\xFE')"),
                ("scripts/c.server.lua", b"print('c')"),
            ],
        );
        let project = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();

        assert!(project.world_loaded);
        let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["scripts/a.server.lua", "scripts/c.server.lua"]);
        assert_eq!(project.problems.len(), 1);
        let problem = format!("{:#}", anyhow_chain(&project.problems[0]));
        assert!(problem.contains("broken.server.lua"), "{problem}");
        let service = project.world.server_script_service();
        assert_eq!(project.world.get(service).unwrap().children().len(), 2);
    }

    #[test]
    fn a_corrupted_manifest_is_reported_and_the_project_takes_its_folders_name() {
        let folder = project_with_scripts("broken-manifest", &[]);
        fs::write(folder.join(MANIFEST_FILE), "{ \"name\": ").unwrap();
        let project = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        let project = project.unwrap();
        assert_eq!(
            project.manifest.name,
            folder.file_name().unwrap().to_string_lossy()
        );
        assert!(matches!(project.problems[..], [LoadError::Manifest { .. }]));
        assert!(project.world_loaded);
    }

    #[test]
    fn a_newer_project_still_refuses_to_load() {
        let folder = project_with_scripts("too-new", &[]);
        fs::write(
            folder.join(MANIFEST_FILE),
            r#"{"name": "T", "format_version": 99}"#,
        )
        .unwrap();
        let result = load(&folder);
        fs::remove_dir_all(&folder).unwrap();
        assert!(matches!(result, Err(LoadError::TooNew { found: 99, .. })));
    }

    /// The error with its causes, the way the editor and player print them.
    fn anyhow_chain(err: &LoadError) -> String {
        let mut text = err.to_string();
        let mut source = std::error::Error::source(err);
        while let Some(cause) = source {
            text.push_str(&format!(": {cause}"));
            source = cause.source();
        }
        text
    }

    #[test]
    fn scripts_are_created_renamed_and_deleted_as_files() {
        let folder = project_with_scripts("script-files", &[]);
        let server = ClassName::Script;
        let path = create_script(&folder, "Script", server, "print('hi')").unwrap();
        assert_eq!(path, "scripts/Script.server.lua");
        // Never replaces a file that's already there.
        let again = create_script(&folder, "Script", server, "print('other')").unwrap_err();
        assert!(again.already_exists());

        write_script(&folder, &path, "print('edited')").unwrap();
        assert_eq!(read_script(&folder, &path).unwrap(), "print('edited')");

        // A LocalScript's file keeps its `.client.lua` when renamed.
        let client = create_script(&folder, "Keys", ClassName::LocalScript, "").unwrap();
        assert_eq!(client, "scripts/Keys.client.lua");
        let client = rename_script(&folder, &client, "Input").unwrap();
        assert_eq!(client, "scripts/Input.client.lua");
        delete_script(&folder, &client).unwrap();

        let other = create_script(&folder, "Other", server, "").unwrap();
        assert!(
            rename_script(&folder, &path, "Other")
                .unwrap_err()
                .already_exists()
        );
        let renamed = rename_script(&folder, &path, "Door").unwrap();
        assert_eq!(renamed, "scripts/Door.server.lua");
        // Only the capitals change: that's the same file, so it's allowed.
        let capitals = rename_script(&folder, &renamed, "DOOR").unwrap();
        assert_eq!(capitals, "scripts/DOOR.server.lua");

        delete_script(&folder, &other).unwrap();
        let mut problems = Vec::new();
        let found = find_scripts(&folder, &mut problems);
        let text = read_script(&folder, &capitals);
        let leftover = folder.join("scripts/DOOR.server.lua.saving").exists();
        fs::remove_dir_all(&folder).unwrap();

        assert_eq!(found, ["scripts/DOOR.server.lua"]);
        assert_eq!(text.unwrap(), "print('edited')");
        assert!(problems.is_empty() && !leftover);
    }

    #[test]
    fn script_names_must_work_as_file_names_on_every_os() {
        for good in ["Script", "door opener", "Pad2", "über", "a.b"] {
            assert_eq!(check_script_name(good), Ok(()), "{good}");
        }
        for bad in [
            "",
            "  ",
            "a/b",
            r"a\b",
            "what?",
            "x:y",
            "end.",
            "end ",
            " start",
            "CON",
            "com1",
            "Lpt9.txt",
            "tab\there",
        ] {
            assert!(check_script_name(bad).is_err(), "{bad:?}");
        }
        assert_eq!(script_name("scripts/doors/open.server.lua"), "open");
        assert_eq!(script_name("scripts/keys.client.lua"), "keys");
        assert_eq!(
            script_path("open", ClassName::Script),
            "scripts/open.server.lua"
        );
        assert_eq!(
            script_path("keys", ClassName::LocalScript),
            "scripts/keys.client.lua"
        );
    }

    #[test]
    fn the_world_file_is_kept_under_a_free_name() {
        let folder = temp_folder("damaged-world");
        assert_eq!(keep_world_file(&folder).unwrap(), None);
        fs::write(folder.join(WORLD_FILE), "broken one").unwrap();
        let first = keep_world_file(&folder).unwrap();
        fs::write(folder.join(WORLD_FILE), "broken two").unwrap();
        let second = keep_world_file(&folder).unwrap();
        let texts = (
            fs::read_to_string(folder.join(KEPT_WORLD_FILE)),
            fs::read_to_string(folder.join("world.backup2.json")),
        );
        let world_left = folder.join(WORLD_FILE).exists();
        fs::remove_dir_all(&folder).unwrap();

        assert_eq!(first.as_deref(), Some(KEPT_WORLD_FILE));
        assert_eq!(second.as_deref(), Some("world.backup2.json"));
        assert_eq!(texts.0.unwrap(), "broken one");
        assert_eq!(texts.1.unwrap(), "broken two");
        assert!(!world_left);
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
    fn every_example_loads_without_problems() {
        for folder in every_example() {
            let project = load(&folder).unwrap();
            assert!(
                project.problems.is_empty(),
                "{}: {:?}",
                folder.display(),
                project.problems
            );
        }
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
