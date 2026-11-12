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
//!   ui/             `<ScreenGui>.uigraph.json` UI Event Graphs
//!   *.css           project stylesheets, also found in subfolders
//!   strings.json    the string table: UI text in each language (optional)
//!   input.json      the input map: actions and their bindings (optional)
//!   collision-groups.json  the collision groups (optional)
//!   levels/         `<name>.world.json`: the levels other than the start
//!                   level, `world.json` (optional; see `levels.rs`)
//! ```
//!
//! One damaged file never stops the rest from loading: [`load`] leaves it
//! out and lists it in [`Project::problems`].
//!
//! `lib.rs` owns the manifest, templates, script paths and stylesheets.
//! `world_file.rs` reads and writes `world.json`. `migrate.rs` upgrades an
//! older manifest and keeps the previous files. `terrain_files.rs` reads and
//! writes Terrains' heightfields. `packed.rs` packs a game folder.
//! `versions.rs` keeps the copies made on save. `validate.rs` is the
//! editor's asset check ([`check_assets`]); it does not write the project.
//! `asset_fix.rs` works out fixes for what it finds ([`plan_fix`]).
//! `watch.rs` notices files changed by other programs ([`Watcher`]).

mod asset_fix;
mod asset_refs;
mod backup;
pub mod branding;
mod collaborate_file;
mod compat;
mod levels;
pub mod memory_budget;
mod migrate;
mod migrate_animation;
mod migrate_physics;
mod migrate_shapes;
mod object_scripts;
mod package;
mod packed;
mod prefabs;
mod starter_character;
mod terrain_files;
mod validate;
mod versions;
mod watch;
mod world_file;

mod collision_groups;
mod input_map;
mod scripts;
mod strings;
mod stylesheets;
mod templates;
mod ui_graphs;
mod user_templates;

pub use collision_groups::{
    COLLISION_GROUPS_FILE, parse_collision_groups, save_collision_groups, write_collision_groups,
};
pub use input_map::{INPUT_FILE, load_input_map, parse_input_map, save_input_map, write_input_map};
pub use scripts::*;
pub use strings::{STRINGS_FILE, parse_strings, save_strings, write_strings};
pub use stylesheets::*;
pub use templates::*;
pub use ui_graphs::*;
pub use user_templates::{
    TEMPLATE_EXTENSION, TemplateError, TemplateInfo, UserTemplate,
    create_project_from_template_file, list_templates, read_template, save_template,
    template_file_name,
};

#[cfg(test)]
use axcenity_blocks_runtime::{Block, BlockKind};
use axcenity_blocks_runtime::{BlockScript, reserve_block_ids, translate};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, InstanceId, World};
use serde::{Deserialize, Serialize};

pub use asset_fix::{
    FileWrite, Fix, FixPlan, IGNORED_FILE, Ignored, PropertyFix, files_of_kind, list_project_files,
    moved_suggestion, plan_fix, read_ignored, suggestion, write_ignored,
};
pub use asset_refs::{
    RefPlace, Reference, find_references, is_model_part, plan_rename, plan_switch, unused_assets,
};
pub use backup::{
    BACKUP_SETTINGS_FILE, BackupSettings, back_up_project, backup_settings, save_backup_settings,
};
pub use collaborate_file::{
    COLLABORATE_FILE, CollaborateFile, RepositoryLink, load_collaborate_file, save_collaborate_file,
};
pub use compat::{Compatibility, compatibility};
pub use levels::{
    DEFAULT_START_LEVEL, FileChange, LEVEL_SUFFIX, LEVELS_FOLDER, check_level_name,
    copy_workspace_into, keep_level_file, level_file, level_names, level_path, load_levels,
    load_with_level, new_level_text, read_level, read_start_level, replace_workspace, save_level,
    set_start_level, set_start_level_name, write_files, write_level,
};
pub use migrate::{FAILED_UPGRADE_DIR, PREVIOUS_FORMAT_DIR, UpgradeStep, upgrade_steps};
pub use object_scripts::{
    in_start_order, level_code, numbered_path, object_scripts, read_object_file, source_file_fits,
};
pub use package::{
    Clashes, Contents, ImportPlan, PACKAGE_EXTENSION, Package, PackageError, PackageManifest,
    package_contents, package_contents_with_scripts, package_scripts, plan_import, read_package,
    script_tables, without_scripts, write_package,
};
pub use packed::{PackError, PackOptions, pack_game, pack_game_with, unpack_game};
pub use prefabs::{PrefabError, Prefabs, applied_text, prefab_references, variant_text};
pub use starter_character::load_starter_character;
pub use terrain_files::{
    TERRAIN_FOLDER, data_folder, is_safe_folder, keep_terrain_folder, load_terrains,
    new_terrain_folder, save_terrain, terrain_folder_path,
};
pub use validate::{
    AssetKind, Finding, FindingKind, Report, Site, check_assets, check_assets_in_level,
};
pub use versions::{
    AssetChange, Change, ChangedFile, Changes, InstanceChange, Line, Version, around_changes,
    list_versions, record_checkpoint, record_version, restore_file, restore_version, set_pinned,
    version_changes,
};
pub use watch::{Watched, Watcher, is_asset_path};
pub use world_file::{
    WorldError, json_to_attribute, json_to_value, parse_prefab, parse_world, write_prefab,
    write_world,
};

pub const MANIFEST_FILE: &str = "axcenity.json";
pub const WORLD_FILE: &str = "world.json";
/// Where the editor keeps a world file it never loaded (it was damaged, or
/// changed outside the editor while the world had unsaved changes) before
/// saving over it, so nothing in it is lost. A number goes before `.json` if
/// this name is taken too.
pub const KEPT_WORLD_FILE: &str = "world.backup.json";
pub const SCRIPTS_FOLDER: &str = "scripts";
/// Where new files of scripts inside objects go (G2); the `scripts` scan
/// never reads it.
pub const OBJECTS_FOLDER: &str = "objects";
/// The editor's Lua plugins for this project (Phase 54; the `plugins`
/// crate). Editor-only: built games never have it, and its stylesheets
/// don't style the game.
pub const PLUGINS_FOLDER: &str = "plugins";
/// Files ending in this are server scripts. The part before it is the Script's name.
pub const SERVER_SCRIPT_SUFFIX: &str = ".server.lua";
/// Files ending in this are client scripts, which become LocalScripts.
pub const CLIENT_SCRIPT_SUFFIX: &str = ".client.lua";
/// Files ending in this are modules, which become ModuleScripts that other
/// scripts load with `require`.
pub const MODULE_SCRIPT_SUFFIX: &str = ".module.lua";
pub const SERVER_BLOCK_SCRIPT_SUFFIX: &str = ".server.blocks.json";
pub const CLIENT_BLOCK_SCRIPT_SUFFIX: &str = ".client.blocks.json";

/// The newest manifest (`axcenity.json`) this engine writes. Opening an older
/// one upgrades the project and keeps the previous files. `world.json` and
/// prefabs have their own version and are not this number. Format 3 (v3
/// Phase 7) is the one clip format and state machines; format 4 (Phase 34)
/// physical properties, which keep older Metal parts' physics; format 5
/// (release system Phase 7) the engine that created and last saved the
/// project ([`EngineRecord`]); format 6 (unified Phase 92) rewrites the
/// file shapes from before format 3 that the readers no longer take.
pub const FORMAT_VERSION: u32 = 6;

/// Which Axcenity versions made and last saved a project (`engine` in the
/// manifest, format 5). Only for telling people what a project needs; the
/// format number decides what opens.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineRecord {
    /// `None` for a project from before format 5: nobody wrote it down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_with: Option<String>,
    pub saved_with: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub format_version: u32,
    #[serde(default = "default_show_splash")]
    pub show_splash: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub loading_screen: LoadingScreen,
    /// The name of the start level, `world.json` (Phase 48). Other levels
    /// are `levels/<name>.world.json`.
    #[serde(default = "default_start_level")]
    pub start_level: String,
    /// The game's own version, such as `"1.2"` (Phase 63). Players can
    /// only join a host running the same one.
    #[serde(default = "default_game_version")]
    pub version: String,
    /// The built game's F9 Developer Console (Phase 71). A project folder
    /// run in the player, or a game started with `--dev`, has it anyway.
    #[serde(default)]
    pub developer_console: bool,
    /// The engine that made and last saved it (format 5; `None` before).
    #[serde(default)]
    pub engine: Option<EngineRecord>,
    /// The game's icon and banner (cohesion pass CW8.1, `branding.rs`).
    #[serde(default, skip_serializing_if = "branding::Branding::is_empty")]
    pub branding: branding::Branding,
    /// The memory budget the Profiler warns about (competitive roadmap
    /// item 5, `memory_budget.rs`).
    #[serde(default, skip_serializing_if = "memory_budget::MemoryBudget::is_empty")]
    pub memory_budget: memory_budget::MemoryBudget,
}

fn default_show_splash() -> bool {
    true
}

fn default_game_version() -> String {
    "1.0".to_owned()
}

fn default_start_level() -> String {
    levels::DEFAULT_START_LEVEL.to_owned()
}

/// How the player's loading screen goes (`loading_screen` in the manifest).
/// With no `screen_gui`, it's the Axcenity splash when `show_splash` is on,
/// or nothing.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoadingScreen {
    /// The `Name` of a ScreenGui in the Workspace, shown over all the other
    /// UI in place of the splash. Empty for none.
    pub screen_gui: String,
    /// How long that ScreenGui stays up at least, in seconds.
    pub min_seconds: f32,
    /// Also wait for a LocalScript to call
    /// `ReplicatedFirst:RemoveDefaultLoadingScreen()`.
    pub wait_for_script: bool,
}

impl Default for LoadingScreen {
    fn default() -> Self {
        Self {
            screen_gui: String::new(),
            min_seconds: 3.0,
            wait_for_script: false,
        }
    }
}

/// The picture of the project the editor saves for its project list.
pub const THUMBNAIL_FILE: &str = "thumbnail.png";

/// The manifest in `folder`, without loading anything else (for lists of
/// projects). `None` if there is no readable manifest.
pub fn read_manifest(folder: &Path) -> Option<Manifest> {
    let text = fs::read_to_string(folder.join(MANIFEST_FILE)).ok()?;
    serde_json::from_str::<Manifest>(&text).ok()
}

/// Changes the project's name in its manifest.
pub fn set_name(folder: &Path, name: &str) -> Result<(), SaveError> {
    update_manifest(folder, "name", serde_json::Value::from(name.trim()))
}

/// Changes the project's description in its manifest.
pub fn set_description(folder: &Path, description: &str) -> Result<(), SaveError> {
    update_manifest(folder, "description", serde_json::Value::from(description))
}

pub struct Project {
    pub folder: PathBuf,
    pub manifest: Manifest,
    pub world: World,
    /// Every script that could be read, in the order they start: modules
    /// first (so they can be required), then server scripts, then client
    /// scripts, each sorted by path (see [`script_order`]) and followed by
    /// the scripts inside objects of its kind, in tree order (G2,
    /// [`in_start_order`]). Copies of an object share their file, so its
    /// path can be here more than once.
    pub scripts: Vec<ScriptFile>,
    /// Parsed block documents corresponding to generated entries in `scripts`.
    pub block_scripts: Vec<BlockScriptFile>,
    /// The UI Event Graphs (`ui/*.uigraph.json`). Each that compiles has a
    /// LocalScript under its ScreenGui, at the end of `scripts`.
    pub ui_graphs: Vec<UiGraphFile>,
    /// CSS files, in stable path order, ready for the UI crate to parse.
    pub stylesheets: Vec<StylesheetFile>,
    /// The actions and their bindings, from `input.json` (the defaults
    /// without one).
    pub input_map: axcenity_core::input::InputMap,
    /// The files that couldn't be loaded. Each was left out, and the rest
    /// of the project loaded without it.
    pub problems: Vec<LoadError>,
    /// False when the world file couldn't be loaded, so the Workspace
    /// started empty. Saving would then replace a file that may only need a
    /// small fix, so the editor keeps it first ([`keep_world_file`]).
    pub world_loaded: bool,
    /// Terrains' data folders that couldn't be read (those terrains are
    /// flat). The editor keeps each ([`keep_terrain_folder`]) before saving
    /// over it.
    pub damaged_terrains: Vec<String>,
    /// The manifest version this open upgraded from. `None` when the project
    /// was already current, or the manifest could not be read.
    pub migrated_from: Option<u32>,
    /// The copy of the project files from before [`Self::migrated_from`].
    /// Later saves do not write it.
    pub migration_backup: Option<PathBuf>,
}

/// Stores whether exported players show the brief engine splash screen.
pub fn set_show_splash(folder: &Path, show: bool) -> Result<(), SaveError> {
    update_manifest(folder, "show_splash", serde_json::Value::Bool(show))
}

/// Stores whether the built game has the F9 Developer Console (Phase 71).
pub fn set_developer_console(folder: &Path, on: bool) -> Result<(), SaveError> {
    update_manifest(folder, "developer_console", serde_json::Value::Bool(on))
}

/// Stores how the player's loading screen goes.
pub fn set_loading_screen(folder: &Path, loading: &LoadingScreen) -> Result<(), SaveError> {
    let value = serde_json::to_value(loading).expect("the loading screen settings are plain data");
    update_manifest(folder, "loading_screen", value)
}

/// Sets one key of the manifest, keeping the rest of the file as it was.
pub(crate) fn update_manifest(
    folder: &Path,
    key: &str,
    value: serde_json::Value,
) -> Result<(), SaveError> {
    let path = folder.join(MANIFEST_FILE);
    let text = fs::read_to_string(&path).map_err(SaveError::new(&path))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
        SaveError::new(&path)(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("invalid project manifest: {error}"),
        ))
    })?;
    let Some(object) = manifest.as_object_mut() else {
        return Err(SaveError::new(&path)(std::io::Error::new(
            ErrorKind::InvalidData,
            "project manifest must be a JSON object",
        )));
    };
    object.insert(key.to_owned(), value);
    let serialized = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        SaveError::new(&path)(std::io::Error::new(ErrorKind::InvalidData, error))
    })?;
    let temporary = path.with_extension("json.saving");
    fs::write(&temporary, serialized).map_err(SaveError::new(&temporary))?;
    if let Err(source) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(SaveError { path, source });
    }
    Ok(())
}

/// A script file from the project's `scripts` folder, or a script inside an
/// object and the file its `SourceFile` names (G2).
#[derive(Clone)]
pub struct ScriptFile {
    /// The Script made for this file, in `ServerScriptService`, the
    /// LocalScript, in `StarterPlayer.StarterPlayerScripts`, or the
    /// ModuleScript, in `ReplicatedStorage`; or the script inside an
    /// object.
    pub instance: InstanceId,
    /// Where the file is, relative to the project folder and with `/` between
    /// folders on every OS, for example `scripts/doors/open.server.lua`.
    /// Error messages show this path.
    pub path: String,
    /// The Lua code.
    pub source: String,
}

pub struct BlockScriptFile {
    pub instance: InstanceId,
    pub path: String,
    pub script: BlockScript,
}

/// A CSS file from the project folder.
pub struct StylesheetFile {
    /// Relative path with `/` separators on every OS.
    pub path: String,
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
    #[error(
        "could not upgrade `{}` from project format {found}: {message}",
        path.display()
    )]
    Upgrade {
        path: PathBuf,
        found: u32,
        message: String,
    },
    #[error("`{}` is not a valid world file", path.display())]
    World {
        path: PathBuf,
        #[source]
        source: WorldError,
    },
    #[error("`{}` is not a valid block script: {message}", path.display())]
    BlockScript { path: PathBuf, message: String },
    #[error("the UI Event Graph `{}` doesn't run: {message}", path.display())]
    UiGraph { path: PathBuf, message: String },
    #[error("`{}` is not a valid string table, so UI text shows its source text", path.display())]
    Strings {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("`{}` is not a valid collision groups file, so every part collides: {message}", path.display())]
    CollisionGroups { path: PathBuf, message: String },
    #[error("`{}` is not a valid input map, so the default actions are used: {message}", path.display())]
    InputMap { path: PathBuf, message: String },
    #[error("the terrain in `{}` couldn't be loaded, so it's flat: {message}", path.display())]
    Terrain { path: PathBuf, message: String },
    /// A script inside an object (G2) that can't run; the Instance stays.
    #[error("`{script}` does nothing: {message}")]
    ObjectScript { script: String, message: String },
    /// A `SourceFile` no script can take its code from.
    #[error("the scripts using `{path}` do nothing: {message}")]
    SourceFile { path: String, message: String },
    #[error("{source}, so its copies are as they were last saved")]
    Prefab {
        #[source]
        source: PrefabError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BlockSaveError {
    #[error("could not encode block script: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error(transparent)]
    Write(#[from] SaveError),
}

impl BlockSaveError {
    pub fn already_exists(&self) -> bool {
        match self {
            Self::Write(error) => error.already_exists(),
            Self::Serialize(_) => false,
        }
    }
}

/// Loads the project in `folder`.
///
/// A manifest from an older engine is upgraded first: the previous files are
/// copied under [`PREVIOUS_FORMAT_DIR`], then the manifest version is
/// rewritten. That copy is left alone by a later save. Only a missing or
/// unreadable manifest (so this isn't a project folder), one from a newer
/// engine, or an upgrade that cannot finish stops loading. Any other file
/// that can't be loaded is left out and listed in [`Project::problems`]: a
/// broken manifest gives the project its folder's name, a broken world file
/// an empty Workspace, and a broken script no Script.
pub fn load(folder: &Path) -> Result<Project, LoadError> {
    load_with_progress(folder, &mut |_| {})
}

/// What [`load_with_progress`] is about to read, for a loading screen.
/// `done` of `total` counts the files of that kind already read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStep<'a> {
    Manifest,
    /// Upgrading a project saved by an older engine.
    Upgrade {
        from: u32,
    },
    World,
    Terrain {
        name: &'a str,
        done: usize,
        total: usize,
    },
    Script {
        path: &'a str,
        done: usize,
        total: usize,
    },
    Stylesheets,
}

/// [`load`], telling `progress` what it reads next. `&mut dyn FnMut` takes
/// any closure without making this function generic.
pub fn load_with_progress(
    folder: &Path,
    progress: &mut dyn FnMut(LoadStep),
) -> Result<Project, LoadError> {
    load_project(folder, None, progress)
}

/// [`load_with_progress`], with the level `level` in the Workspace in
/// place of the start level's (see [`levels::load_with_level`]).
pub(crate) fn load_project(
    folder: &Path,
    level: Option<&str>,
    progress: &mut dyn FnMut(LoadStep),
) -> Result<Project, LoadError> {
    progress(LoadStep::Manifest);
    let manifest_path = folder.join(MANIFEST_FILE);
    let manifest_text = read(&manifest_path)?;
    let mut problems = Vec::new();
    let mut manifest = match serde_json::from_str::<Manifest>(&manifest_text) {
        Ok(manifest) => manifest,
        Err(source) => {
            problems.push(LoadError::Manifest {
                path: manifest_path.clone(),
                source,
            });
            Manifest {
                name: folder_name(folder),
                format_version: FORMAT_VERSION,
                show_splash: default_show_splash(),
                description: String::new(),
                loading_screen: LoadingScreen::default(),
                start_level: default_start_level(),
                version: default_game_version(),
                developer_console: false,
                engine: None,
                branding: Default::default(),
                memory_budget: Default::default(),
            }
        }
    };
    // A manifest this engine can't read may be a newer format's: refuse it
    // rather than open it as a current one.
    if !problems.is_empty()
        && let Some(found) = format_of(&manifest_text).filter(|&found| found > FORMAT_VERSION)
    {
        return Err(LoadError::TooNew {
            path: manifest_path,
            found,
        });
    }
    if manifest.format_version > FORMAT_VERSION {
        return Err(LoadError::TooNew {
            path: manifest_path,
            found: manifest.format_version,
        });
    }
    // Whether the world read before an upgrade: only then is a world that
    // doesn't read afterwards the upgrade's fault.
    let mut world_read_before = false;
    let upgrade = if manifest.format_version < FORMAT_VERSION {
        progress(LoadStep::Upgrade {
            from: manifest.format_version,
        });
        world_read_before = load_world(folder).is_ok();
        let upgrade = migrate::upgrade(folder, manifest.format_version)?;
        // The steps rewrote the manifest (format 5 added `engine`).
        if let Some(upgraded) = read_manifest(folder) {
            manifest = upgraded;
        }
        manifest.format_version = FORMAT_VERSION;
        Some(upgrade)
    } else {
        None
    };
    let (migrated_from, migration_backup) = upgrade
        .as_ref()
        .map_or((None, None), |u| (Some(u.from), Some(u.backup.clone())));

    progress(LoadStep::World);
    let (mut world, mut world_loaded) = match load_world(folder) {
        Ok(world) => (world, true),
        // The upgrade left a world this engine can't read: put the files
        // back as they were rather than open an empty Workspace over them.
        Err(err) if world_read_before && upgrade.as_ref().is_some_and(|u| u.fresh) => {
            let upgrade = upgrade.expect("checked above");
            return Err(migrate::put_back(folder, &upgrade, &err));
        }
        Err(err) => {
            problems.push(err);
            (World::new(), false)
        }
    };
    // A level other than the start level: its Workspace, before terrains,
    // scripts and UI graphs look in it.
    if let Some(name) = level {
        let opened = std::fs::read_to_string(folder.join(levels::level_path(name)))
            .map_err(|source| LoadError::Read {
                path: folder.join(levels::level_path(name)),
                source,
            })
            .and_then(|text| {
                parse_world(&text).map_err(|source| LoadError::World {
                    path: folder.join(levels::level_path(name)),
                    source,
                })
            });
        match opened {
            Ok(level) => levels::replace_workspace(&mut world, &level),
            Err(err) => {
                problems.push(err);
                levels::replace_workspace(&mut world, &World::new());
                world_loaded = false;
            }
        }
    }
    // Copies of prefabs follow their files as they are now (Phase 49).
    let mut prefabs = Prefabs::in_folder(folder);
    let root = world.root();
    prefabs.refresh(&mut world, root);
    problems.extend(
        prefabs
            .take_problems()
            .into_iter()
            .map(|source| LoadError::Prefab { source }),
    );
    strings::load_strings(folder, &mut world, &mut problems);
    collision_groups::load_collision_groups(folder, &mut world, &mut problems);
    let damaged_terrains =
        terrain_files::load_terrains_reporting(folder, &mut world, &mut problems, progress);
    let (scripts, block_scripts) = load_scripts(folder, &mut world, &mut problems, progress);
    // Scripts inside objects (G2), from the tree as the world, level and
    // prefabs made it.
    let objects = object_scripts(folder, &world, &mut problems);
    let mut scripts = in_start_order(&world, scripts, objects);
    let ui_graphs = load_ui_graphs(folder, &mut world, &mut scripts, &mut problems);
    if level.is_some() {
        // A level without a graph's ScreenGui just doesn't use that graph.
        problems.retain(|problem| match problem {
            LoadError::UiGraph { path, .. } => path
                .file_name()
                .and_then(|file| file.to_str())
                .is_none_or(|file| {
                    screen_gui_named(
                        &world,
                        axcenity_blocks_runtime::ui_graph::ui_graph_gui_name(file),
                    )
                    .is_some()
                }),
            _ => true,
        });
    }
    progress(LoadStep::Stylesheets);
    let stylesheets = load_stylesheets(folder, &mut problems);
    let input_map = load_input_map(folder, &mut problems);

    Ok(Project {
        folder: folder.to_owned(),
        manifest,
        world,
        scripts,
        block_scripts,
        ui_graphs,
        stylesheets,
        input_map,
        problems,
        world_loaded,
        damaged_terrains,
        migrated_from,
        migration_backup,
    })
}

/// The `format_version` of a manifest that doesn't parse as a whole.
fn format_of(text: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    u32::try_from(value.get("format_version")?.as_u64()?).ok()
}

/// After a save: records this engine as the one that last saved the
/// project (`engine.saved_with`), writing the manifest only when that
/// changes. Returns whether it wrote.
pub fn record_engine(folder: &Path) -> Result<bool, SaveError> {
    let current = axcenity_release::Version::current().to_string();
    let engine = read_manifest(folder).and_then(|m| m.engine);
    if engine.as_ref().is_some_and(|e| e.saved_with == current) {
        return Ok(false);
    }
    let record = EngineRecord {
        created_with: engine.and_then(|e| e.created_with),
        saved_with: current,
    };
    update_manifest(
        folder,
        "engine",
        serde_json::to_value(record).expect("an engine record always serializes"),
    )?;
    Ok(true)
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

pub(crate) fn load_world(folder: &Path) -> Result<World, LoadError> {
    let path = folder.join(WORLD_FILE);
    let text = read(&path)?;
    parse_world(&text).map_err(|source| LoadError::World { path, source })
}

#[derive(Debug, thiserror::Error)]
#[error("could not write `{}`", path.display())]
pub struct SaveError {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

impl SaveError {
    pub(crate) fn new(path: &Path) -> impl FnOnce(std::io::Error) -> SaveError + '_ {
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
pub(crate) fn write_through_temporary(path: &Path, text: &str) -> Result<(), SaveError> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".saving");
    let error = SaveError::new(path);
    fs::write(&temporary, text).map_err(SaveError::new(path))?;
    // `rename` replaces an existing file on Windows, macOS and Linux alike.
    fs::rename(&temporary, path).map_err(error)
}

pub(crate) fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests;
