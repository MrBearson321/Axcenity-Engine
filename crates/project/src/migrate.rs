//! Upgrades a project whose manifest is from an older engine.
//!
//! [`upgrade`] copies the project files into [`PREVIOUS_FORMAT_DIR`] first,
//! then runs one step per format version until the manifest is current.
//! A later save writes the project files and `.axcenity/history`; it does not
//! write that copy. A later phase that changes the on-disk shape adds a step
//! in [`step`] and bumps [`crate::FORMAT_VERSION`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::{FORMAT_VERSION, LoadError, MANIFEST_FILE};

/// Where an upgrade keeps the files from before it, relative to the project.
/// The version those files used is a subfolder (`1`, `2`, …).
pub const PREVIOUS_FORMAT_DIR: &str = ".axcenity/previous";

/// Directories at the project root that are not part of the game. The backup
/// lives under `.axcenity`, so that directory is skipped to avoid copying the
/// copy into itself.
const SKIP_ROOT_DIRS: &[&str] = &[".axcenity", ".git", "target", "node_modules"];

pub(crate) struct Upgrade {
    pub from: u32,
    /// The copy of the files from `from`, taken before any step ran.
    pub backup: PathBuf,
    /// Whether this upgrade made that copy. An older copy for the same
    /// format is kept as it was, so it may not match what was upgraded.
    pub fresh: bool,
}

/// One step of an upgrade, for telling people what it will change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeStep {
    /// The format this step upgrades to.
    pub to: u32,
    /// What it changes, in a line.
    pub summary: &'static str,
}

/// The steps from format `from` to [`FORMAT_VERSION`], or `None` when this
/// engine can't upgrade from `from`.
pub fn upgrade_steps(from: u32) -> Option<Vec<UpgradeStep>> {
    (from..FORMAT_VERSION)
        .map(|version| {
            step(version).map(|(summary, _)| UpgradeStep {
                to: version + 1,
                summary,
            })
        })
        .collect()
}

/// Copies `folder` as format `from` and rewrites it up to [`FORMAT_VERSION`].
///
/// `from` must be older than [`FORMAT_VERSION`]. A version this engine has no
/// step for fails before anything is written. A copy already stored for a
/// version is left as it is.
pub(crate) fn upgrade(folder: &Path, from: u32) -> Result<Upgrade, LoadError> {
    if from >= FORMAT_VERSION {
        return Err(upgrade_error(
            folder,
            from,
            "this project is already current".to_owned(),
        ));
    }
    let mut version = from;
    while version < FORMAT_VERSION {
        if step(version).is_none() {
            return Err(upgrade_error(
                folder,
                from,
                format!("project format {version} is older than this engine can upgrade"),
            ));
        }
        version += 1;
    }

    let fresh = !previous_dir(folder, from).exists();
    let backup = keep_previous(folder, from)?;
    let mut version = from;
    while version < FORMAT_VERSION {
        if version != from {
            keep_previous(folder, version)?;
        }
        let (_, migrate) = step(version).expect("the version was checked above");
        migrate(folder).map_err(|message| upgrade_error(folder, from, message))?;
        version += 1;
    }
    Ok(Upgrade {
        from,
        backup,
        fresh,
    })
}

/// One format step. A later phase adds an arm here.
type Migration = fn(&Path) -> Result<(), String>;

/// The migration from `from` to `from + 1` and what it changes, if this
/// engine knows it.
fn step(from: u32) -> Option<(&'static str, Migration)> {
    match from {
        1 => Some((
            "The manifest is marked as format 2; nothing else changes.",
            migrate_v1_to_v2,
        )),
        2 => Some((
            "Animation clips move to the one clip format, and Animators get state machines.",
            migrate_v2_to_v3,
        )),
        3 => Some((
            "Parts get physical properties; older Metal parts keep the physics they had.",
            migrate_v3_to_v4,
        )),
        4 => Some((
            "The manifest records the Axcenity version that saved the project.",
            migrate_v4_to_v5,
        )),
        5 => Some((
            "Old clips and block-script conditions are rewritten in the current shape.",
            migrate_v5_to_v6,
        )),
        _ => None,
    }
}

/// Format 1 and format 2 describe the same world. The step rewrites the
/// manifest version so opening an older project takes the real upgrade path.
fn migrate_v1_to_v2(folder: &Path) -> Result<(), String> {
    write_manifest_version(folder, 2)
}

/// One clip format and state machines (v3 Phase 7): see
/// `migrate_animation.rs`.
fn migrate_v2_to_v3(folder: &Path) -> Result<(), String> {
    crate::migrate_animation::migrate(folder)?;
    write_manifest_version(folder, 3)
}

/// Physical properties (Phase 34): see `migrate_physics.rs`.
fn migrate_v3_to_v4(folder: &Path) -> Result<(), String> {
    crate::migrate_physics::migrate(folder)?;
    write_manifest_version(folder, 4)
}

/// The engine that saved the project (release system Phase 7). Who
/// created it isn't known, so `created_with` stays out.
fn migrate_v4_to_v5(folder: &Path) -> Result<(), String> {
    let engine =
        serde_json::json!({ "saved_with": axcenity_release::Version::current().to_string() });
    write_manifest(folder, 5, Some(("engine", engine)))
}

/// File shapes from before format 3 that the readers dropped (unified
/// Phase 92): see `migrate_shapes.rs`.
fn migrate_v5_to_v6(folder: &Path) -> Result<(), String> {
    crate::migrate_shapes::migrate(folder)?;
    write_manifest_version(folder, 6)
}

fn write_manifest_version(folder: &Path, version: u32) -> Result<(), String> {
    write_manifest(folder, version, None)
}

/// Sets the manifest's `format_version`, and `field` too when given.
fn write_manifest(
    folder: &Path,
    version: u32,
    field: Option<(&str, serde_json::Value)>,
) -> Result<(), String> {
    let path = folder.join(MANIFEST_FILE);
    let text =
        fs::read_to_string(&path).map_err(|err| format!("could not read the manifest: {err}"))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("the manifest is not valid JSON: {err}"))?;
    let Some(object) = manifest.as_object_mut() else {
        return Err("the manifest is not a JSON object".to_owned());
    };
    object.insert(
        "format_version".to_owned(),
        serde_json::Value::from(version),
    );
    if let Some((key, value)) = field {
        object.insert(key.to_owned(), value);
    }
    let serialized = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("could not write the manifest: {err}"))?;
    let temporary = path.with_extension("json.saving");
    if let Err(err) = fs::write(&temporary, serialized) {
        return Err(format!("could not write the manifest: {err}"));
    }
    if let Err(err) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not replace the manifest: {err}"));
    }
    Ok(())
}

fn previous_dir(folder: &Path, version: u32) -> PathBuf {
    folder.join(PREVIOUS_FORMAT_DIR).join(version.to_string())
}

/// Where [`put_back`] moves the files of an upgrade that didn't load.
pub const FAILED_UPGRADE_DIR: &str = ".axcenity/failed-upgrade";

/// After an upgrade whose world didn't load (`why`): moves the upgraded
/// files to [`FAILED_UPGRADE_DIR`] and copies the ones from before back, so
/// the project is as it was and an older engine still opens it. Returns the
/// error to show.
pub(crate) fn put_back(folder: &Path, upgrade: &Upgrade, why: &LoadError) -> LoadError {
    let failed = folder.join(FAILED_UPGRADE_DIR);
    let moved = (|| -> Result<(), String> {
        if failed.exists() {
            fs::remove_dir_all(&failed)
                .map_err(|err| format!("could not clear `{}`: {err}", failed.display()))?;
        }
        fs::create_dir_all(&failed)
            .map_err(|err| format!("could not create `{}`: {err}", failed.display()))?;
        let entries = fs::read_dir(folder)
            .map_err(|err| format!("could not read `{}`: {err}", folder.display()))?;
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("could not read `{}`: {err}", folder.display()))?;
            let name = entry.file_name();
            if SKIP_ROOT_DIRS.contains(&name.to_string_lossy().as_ref())
                || entry.file_type().is_ok_and(|t| t.is_symlink())
            {
                continue;
            }
            fs::rename(entry.path(), failed.join(&name)).map_err(|err| {
                format!("could not move `{}` aside: {err}", entry.path().display())
            })?;
        }
        copy_project(&upgrade.backup, folder)
    })();
    let why = ErrorChain(why).to_string();
    let message = match moved {
        Ok(()) => format!(
            "the upgraded project didn't load ({why}). Its files are back as they were; \
             the upgraded ones are in `{FAILED_UPGRADE_DIR}`"
        ),
        Err(err) => format!(
            "the upgraded project didn't load ({why}), and putting the files back failed \
             ({err}). The files from before are in `{}`",
            upgrade.backup.display()
        ),
    };
    upgrade_error(folder, upgrade.from, message)
}

/// An error and its causes on one line.
struct ErrorChain<'a>(&'a LoadError);

impl std::fmt::Display for ErrorChain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = std::error::Error::source(self.0);
        while let Some(cause) = source {
            write!(f, ": {cause}")?;
            source = cause.source();
        }
        Ok(())
    }
}

/// The folder holding format `version`, creating it from `folder` when it
/// is not there yet. An existing copy is returned unchanged.
fn keep_previous(folder: &Path, version: u32) -> Result<PathBuf, LoadError> {
    let root = folder.join(".axcenity").join("previous");
    let dest = root.join(version.to_string());
    if dest.exists() {
        if dest.is_dir() {
            return Ok(dest);
        }
        return Err(upgrade_error(
            folder,
            version,
            format!(
                "`{}` is in the way of the previous-format copy",
                dest.display()
            ),
        ));
    }
    if let Err(err) = fs::create_dir_all(&root) {
        return Err(upgrade_error(
            folder,
            version,
            format!("could not create `{}`: {err}", root.display()),
        ));
    }
    let partial = root.join(format!("{version}.partial"));
    if partial.exists()
        && let Err(err) = fs::remove_dir_all(&partial)
    {
        return Err(upgrade_error(
            folder,
            version,
            format!("could not replace an unfinished copy: {err}"),
        ));
    }
    if let Err(message) = copy_project(folder, &partial) {
        let _ = fs::remove_dir_all(&partial);
        return Err(upgrade_error(folder, version, message));
    }
    if let Err(err) = fs::rename(&partial, &dest) {
        let _ = fs::remove_dir_all(&partial);
        return Err(upgrade_error(
            folder,
            version,
            format!("could not keep the previous files: {err}"),
        ));
    }
    Ok(dest)
}

fn copy_project(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("could not create `{}`: {err}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|err| format!("could not read `{}`: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("could not read `{}`: {err}", from.display()))?;
        let name = entry.file_name();
        let shown = name.to_string_lossy();
        if SKIP_ROOT_DIRS.contains(&shown.as_ref()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| format!("could not read `{}`: {err}", entry.path().display()))?;
        // A link could point outside the project. The copy only takes real files.
        if file_type.is_symlink() {
            continue;
        }
        let dest = to.join(&name);
        if file_type.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|err| format!("could not copy `{}`: {err}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("could not create `{}`: {err}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|err| format!("could not read `{}`: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("could not read `{}`: {err}", from.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("could not read `{}`: {err}", entry.path().display()))?;
        if file_type.is_symlink() {
            continue;
        }
        let dest = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|err| format!("could not copy `{}`: {err}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn upgrade_error(folder: &Path, found: u32, message: String) -> LoadError {
    LoadError::Upgrade {
        path: folder.to_owned(),
        found,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WORLD_FILE, load, read_manifest, record_version, save_world, set_description};
    use axcenity_core::ClassName;

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-migrate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    struct OldProject {
        folder: PathBuf,
        manifest: String,
        world: String,
        script: &'static str,
        asset: Vec<u8>,
    }

    fn old_project() -> OldProject {
        let folder = temp_folder("v1");
        let manifest = "{\n  \"name\": \"Old\",\n  \"format_version\": 1,\n  \"description\": \"Keep me\",\n  \"show_splash\": false\n}\n";
        let world = "{\n  \"format_version\": 1,\n  \"workspace\": {\n    \"children\": [\n      { \"class\": \"Part\", \"name\": \"Block\" }\n    ]\n  }\n}\n";
        let script = "print(\"hi\")\n";
        let asset = vec![9, 8, 7];
        fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
        fs::write(folder.join(WORLD_FILE), world).unwrap();
        fs::create_dir_all(folder.join("scripts")).unwrap();
        fs::write(folder.join("scripts/hello.server.lua"), script).unwrap();
        fs::create_dir_all(folder.join("assets/audio")).unwrap();
        fs::write(folder.join("assets/audio/beep.bin"), &asset).unwrap();
        fs::write(folder.join("notes.txt"), "a project file").unwrap();
        fs::create_dir_all(folder.join("target")).unwrap();
        fs::write(folder.join("target/junk"), "not part of the game").unwrap();
        fs::create_dir_all(folder.join(".axcenity/history")).unwrap();
        fs::write(folder.join(".axcenity/history/note.txt"), "old history").unwrap();
        OldProject {
            folder,
            manifest: manifest.to_owned(),
            world: world.to_owned(),
            script,
            asset,
        }
    }

    #[test]
    fn opening_an_older_project_upgrades_it_and_keeps_the_previous_files() {
        let old = old_project();
        let project = load(&old.folder).unwrap();
        let backup = old.folder.join(PREVIOUS_FORMAT_DIR).join("1");
        let backup_manifest = fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap();
        let backup_world = fs::read_to_string(backup.join(WORLD_FILE)).unwrap();
        let backup_script = fs::read_to_string(backup.join("scripts/hello.server.lua")).unwrap();
        let backup_asset = fs::read(backup.join("assets/audio/beep.bin")).unwrap();
        let backup_notes = fs::read_to_string(backup.join("notes.txt")).unwrap();
        let project_world = fs::read_to_string(old.folder.join(WORLD_FILE)).unwrap();
        let project_script =
            fs::read_to_string(old.folder.join("scripts/hello.server.lua")).unwrap();
        let project_asset = fs::read(old.folder.join("assets/audio/beep.bin")).unwrap();
        let on_disk = read_manifest(&old.folder).unwrap();
        let block = project
            .world
            .find_first_child(project.world.workspace(), "Block");

        assert_eq!(project.migrated_from, Some(1));
        assert_eq!(project.migration_backup.as_deref(), Some(backup.as_path()));
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        assert_eq!(project.manifest.format_version, FORMAT_VERSION);
        assert_eq!(project.manifest.name, "Old");
        assert_eq!(project.manifest.description, "Keep me");
        assert!(!project.manifest.show_splash);
        assert_eq!(on_disk.format_version, FORMAT_VERSION);
        assert_eq!(on_disk.description, "Keep me");
        assert!(!on_disk.show_splash);
        assert!(block.is_some());
        assert_eq!(
            project.world.get(block.unwrap()).unwrap().class(),
            ClassName::Part
        );
        assert_eq!(project.scripts.len(), 1);
        assert_eq!(project.scripts[0].source, old.script);
        assert_eq!(project_world, old.world);
        assert_eq!(project_script, old.script);
        assert_eq!(project_asset, old.asset);
        assert_eq!(backup_manifest, old.manifest);
        assert_eq!(backup_world, old.world);
        assert_eq!(backup_script, old.script);
        assert_eq!(backup_asset, old.asset);
        assert_eq!(backup_notes, "a project file");
        assert!(!backup.join("target").exists());
        assert!(!backup.join(".axcenity").exists());
        assert!(!backup.join(".git").exists());

        // A later save, including version history, leaves the copy alone.
        set_description(&old.folder, "Edited").unwrap();
        save_world(&old.folder, &project.world).unwrap();
        record_version(&old.folder, "Save").unwrap();
        fs::write(
            old.folder.join("scripts/hello.server.lua"),
            "print(\"edited\")\n",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap(),
            old.manifest
        );
        assert_eq!(
            fs::read_to_string(backup.join(WORLD_FILE)).unwrap(),
            old.world
        );
        assert_eq!(
            fs::read_to_string(backup.join("scripts/hello.server.lua")).unwrap(),
            old.script
        );
        assert_eq!(read_manifest(&old.folder).unwrap().description, "Edited");
        assert_eq!(
            read_manifest(&old.folder).unwrap().format_version,
            FORMAT_VERSION
        );

        // Opening the upgraded project does not migrate again or replace the copy.
        let again = load(&old.folder).unwrap();
        assert_eq!(again.migrated_from, None);
        assert!(again.migration_backup.is_none());
        assert_eq!(
            fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap(),
            old.manifest
        );

        // Putting the old manifest back upgrades again, and still keeps the first copy.
        fs::write(old.folder.join(MANIFEST_FILE), &old.manifest).unwrap();
        fs::write(
            old.folder.join(WORLD_FILE),
            "{\n  \"format_version\": 1\n}\n",
        )
        .unwrap();
        let restored = load(&old.folder).unwrap();
        assert_eq!(restored.migrated_from, Some(1));
        assert_eq!(
            fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap(),
            old.manifest
        );
        assert_eq!(
            fs::read_to_string(backup.join(WORLD_FILE)).unwrap(),
            old.world
        );
        assert_eq!(
            read_manifest(&old.folder).unwrap().format_version,
            FORMAT_VERSION
        );

        fs::remove_dir_all(&old.folder).unwrap();
    }

    #[test]
    fn a_newer_project_is_refused_and_the_folder_stays_unchanged() {
        let folder = temp_folder("too-new");
        let manifest = "{\"name\": \"Future\", \"format_version\": 99}\n";
        fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
        fs::write(folder.join(WORLD_FILE), "{\"format_version\": 1}\n").unwrap();
        fs::write(folder.join("notes.txt"), "leave me").unwrap();
        let world_before = fs::read(folder.join(WORLD_FILE)).unwrap();

        let result = load(&folder);
        let manifest_after = fs::read_to_string(folder.join(MANIFEST_FILE)).unwrap();
        let world_after = fs::read(folder.join(WORLD_FILE)).unwrap();
        let notes = fs::read_to_string(folder.join("notes.txt")).unwrap();
        let copied = folder.join(PREVIOUS_FORMAT_DIR).exists();
        fs::remove_dir_all(&folder).unwrap();

        let Err(err) = result else {
            panic!("a newer project opened");
        };
        assert!(matches!(err, LoadError::TooNew { found: 99, .. }));
        let message = err.to_string();
        assert!(message.contains("update Axcenity"), "{message}");
        assert_eq!(manifest_after, manifest);
        assert_eq!(world_after, world_before);
        assert_eq!(notes, "leave me");
        assert!(!copied);
    }

    #[test]
    fn an_unsupported_old_format_is_refused_without_changes() {
        let folder = temp_folder("v0");
        let manifest = "{\"name\": \"Ancient\", \"format_version\": 0}\n";
        fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
        fs::write(folder.join(WORLD_FILE), "{\"format_version\": 1}\n").unwrap();

        let result = load(&folder);
        let manifest_after = fs::read_to_string(folder.join(MANIFEST_FILE)).unwrap();
        let copied = folder.join(".axcenity").exists();
        fs::remove_dir_all(&folder).unwrap();

        let Err(err) = result else {
            panic!("an unknown old format opened");
        };
        let message = err.to_string();
        assert!(message.contains("format 0"), "{message}");
        assert_eq!(manifest_after, manifest);
        assert!(!copied);
    }
    fn v4_project(name: &str, world: &str) -> PathBuf {
        let folder = temp_folder(name);
        fs::write(
            folder.join(MANIFEST_FILE),
            "{\n  \"name\": \"Four\",\n  \"format_version\": 4\n}\n",
        )
        .unwrap();
        fs::write(folder.join(WORLD_FILE), world).unwrap();
        fs::write(folder.join("notes.txt"), "mine").unwrap();
        folder
    }

    #[test]
    fn format_5_records_the_engine_that_saved_it_but_not_who_made_it() {
        let folder = v4_project("v4", "{\"format_version\": 1}\n");
        let project = load(&folder).unwrap();
        assert_eq!(project.migrated_from, Some(4));
        let engine = project.manifest.engine.clone().unwrap();
        assert_eq!(engine.created_with, None);
        assert_eq!(
            engine.saved_with,
            axcenity_release::Version::current().to_string()
        );
        let on_disk = read_manifest(&folder).unwrap();
        assert_eq!(on_disk.format_version, FORMAT_VERSION);
        assert_eq!(on_disk.engine, Some(engine));

        // Saving again with the same engine doesn't touch the manifest.
        assert!(!crate::record_engine(&folder).unwrap());
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn every_step_says_what_it_changes() {
        let steps = upgrade_steps(1).unwrap();
        assert_eq!(steps.len() as u32, FORMAT_VERSION - 1);
        assert!(steps.iter().all(|step| step.summary.ends_with('.')));
        assert_eq!(upgrade_steps(FORMAT_VERSION), Some(Vec::new()));
        assert_eq!(upgrade_steps(0), None);
    }

    #[test]
    fn an_upgrade_whose_world_wont_read_is_put_back() {
        let world = "{\"format_version\": 1}\n";
        let folder = v4_project("put-back", world);
        let manifest = fs::read_to_string(folder.join(MANIFEST_FILE)).unwrap();
        let upgrade = upgrade(&folder, 4).unwrap();
        assert!(upgrade.fresh);
        // As if a step had written a world this engine can't read.
        fs::write(folder.join(WORLD_FILE), "{ broken").unwrap();
        fs::write(folder.join("made-by-the-step.json"), "{}").unwrap();
        let Err(why) = crate::load_world(&folder) else {
            panic!("the broken world read");
        };
        let err = put_back(&folder, &upgrade, &why).to_string();
        assert!(err.contains("back as they were"), "{err}");

        assert_eq!(
            fs::read_to_string(folder.join(MANIFEST_FILE)).unwrap(),
            manifest
        );
        assert_eq!(fs::read_to_string(folder.join(WORLD_FILE)).unwrap(), world);
        assert_eq!(
            fs::read_to_string(folder.join("notes.txt")).unwrap(),
            "mine"
        );
        assert!(!folder.join("made-by-the-step.json").exists());
        let failed = folder.join(FAILED_UPGRADE_DIR);
        assert_eq!(
            fs::read_to_string(failed.join(WORLD_FILE)).unwrap(),
            "{ broken"
        );
        assert!(failed.join("made-by-the-step.json").is_file());
        // The copy from before is still there too.
        assert_eq!(
            fs::read_to_string(upgrade.backup.join(WORLD_FILE)).unwrap(),
            world
        );
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_world_that_was_already_damaged_opens_upgraded_as_before() {
        let folder = v4_project("damaged", "{ was already broken");
        let project = load(&folder).unwrap();
        assert_eq!(project.migrated_from, Some(4));
        assert!(!project.world_loaded);
        assert_eq!(
            read_manifest(&folder).unwrap().format_version,
            FORMAT_VERSION
        );
        assert!(!folder.join(FAILED_UPGRADE_DIR).exists());
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_newer_manifest_this_engine_cant_parse_is_refused_not_opened() {
        let folder = temp_folder("newer-fields");
        let newer = FORMAT_VERSION + 1;
        let manifest =
            format!("{{\"name\": \"Later\", \"format_version\": {newer}, \"something_new\": 1}}\n");
        fs::write(folder.join(MANIFEST_FILE), &manifest).unwrap();
        fs::write(folder.join(WORLD_FILE), "{\"format_version\": 1}\n").unwrap();
        let result = load(&folder);
        assert!(matches!(result, Err(LoadError::TooNew { found, .. }) if found == newer));
        assert_eq!(
            fs::read_to_string(folder.join(MANIFEST_FILE)).unwrap(),
            manifest
        );
        fs::remove_dir_all(&folder).unwrap();
    }
}
