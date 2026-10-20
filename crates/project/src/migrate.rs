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

    let backup = keep_previous(folder, from)?;
    let mut version = from;
    while version < FORMAT_VERSION {
        if version != from {
            keep_previous(folder, version)?;
        }
        let migrate = step(version).expect("the version was checked above");
        migrate(folder).map_err(|message| upgrade_error(folder, from, message))?;
        version += 1;
    }
    Ok(Upgrade { from, backup })
}

/// One format step. A later phase adds an arm here.
type Migration = fn(&Path) -> Result<(), String>;

/// The migration from `from` to `from + 1`, if this engine knows it.
fn step(from: u32) -> Option<Migration> {
    match from {
        1 => Some(migrate_v1_to_v2),
        _ => None,
    }
}

/// Format 1 and format 2 describe the same world. The step rewrites the
/// manifest version so opening an older project takes the real upgrade path.
fn migrate_v1_to_v2(folder: &Path) -> Result<(), String> {
    write_manifest_version(folder, 2)
}

fn write_manifest_version(folder: &Path, version: u32) -> Result<(), String> {
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
}
