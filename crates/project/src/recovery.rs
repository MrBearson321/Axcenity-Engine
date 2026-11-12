//! AutoRecover: the editor's unsaved work, written every few minutes to
//! `.axcenity/autorecover/` so it survives an editor crash or a power cut.
//!
//! A snapshot is the world being edited (Instances, as `world.json` would
//! hold them, with the open level's Workspace) and the text of documents
//! with unsaved changes (script tabs, block scripts) by their project
//! paths. Saving, or closing without saving after being asked, removes it,
//! so a snapshot that's there when a project opens is work that was lost.
//!
//! Each snapshot is written whole into a folder beside the last one, its
//! files flushed to the disk, and then swapped in by renaming, so a crash
//! or a power cut part way through leaves the previous snapshot readable.
//! [`load_recovered`] reads the project as it is on disk with the
//! snapshot's world in place of the saved one; nothing here writes the
//! project's own files.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axcenity_core::World;
use serde::{Deserialize, Serialize};

use crate::{LoadError, Project, SaveError, WORLD_FILE, parse_world, write_world};

/// Where the snapshot is, in the project folder. It's under `.axcenity/`,
/// so backups, packages and built games leave it out.
pub const RECOVERY_FOLDER: &str = ".axcenity/autorecover";
/// A snapshot being written, before it's swapped in.
const WRITING_FOLDER: &str = ".axcenity/autorecover.writing";
/// The snapshot before the newest, while the newest is swapped in.
const PREVIOUS_FOLDER: &str = ".axcenity/autorecover.previous";
/// What the snapshot holds, inside it.
const INFO_FILE: &str = "recovery.json";
/// The documents' text, inside it, at their project paths.
const FILES_FOLDER: &str = "files";

/// How long after its editor last touched it a snapshot counts as left
/// behind. An editor with the project open touches it far more often
/// ([`touch_recovery`]), so a second editor opening the same project
/// doesn't offer the first one's live work as lost.
pub const RECOVERY_STALE_AFTER: Duration = Duration::from_secs(90);

/// What a snapshot holds, saved as `recovery.json` beside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryInfo {
    /// When it was written, in seconds since 1970 (UTC).
    pub saved_at: u64,
    /// The level open in the editor (`None` for the start level).
    #[serde(default)]
    pub level: Option<String>,
    /// The engine version that wrote it.
    #[serde(default)]
    pub engine: String,
    /// Whether it holds the world (it had unsaved changes).
    #[serde(default)]
    pub world: bool,
    /// The documents it holds, by project path.
    #[serde(default)]
    pub files: Vec<String>,
}

/// What to put in a snapshot.
#[derive(Clone, Default)]
pub struct RecoverySnapshot {
    pub level: Option<String>,
    /// The world, when it has unsaved changes.
    pub world: Option<World>,
    /// Documents with unsaved changes: (project path, text).
    pub files: Vec<(String, String)>,
}

impl RecoverySnapshot {
    /// Whether there's nothing unsaved to keep.
    pub fn is_empty(&self) -> bool {
        self.world.is_none() && self.files.is_empty()
    }
}

/// Writes `snapshot` as the project's recovery snapshot in place of the
/// last one, whole or not at all (see the module's notes). An empty one
/// removes the snapshot instead.
pub fn write_recovery(folder: &Path, snapshot: &RecoverySnapshot) -> Result<(), SaveError> {
    if snapshot.is_empty() {
        return discard_recovery(folder);
    }
    let writing = folder.join(WRITING_FOLDER);
    let _ = fs::remove_dir_all(&writing);
    fs::create_dir_all(&writing).map_err(SaveError::new(&writing))?;
    let mut files = Vec::new();
    for (path, text) in &snapshot.files {
        // Only paths inside the project, so a snapshot never writes outside
        // its folder (or reads outside it on recovery).
        let Some(relative) = safe_relative(path) else {
            continue;
        };
        write_synced(&writing.join(FILES_FOLDER).join(relative), text.as_bytes())?;
        files.push(path.clone());
    }
    if let Some(world) = &snapshot.world {
        write_synced(&writing.join(WORLD_FILE), write_world(world).as_bytes())?;
    }
    let info = RecoveryInfo {
        saved_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
        level: snapshot.level.clone(),
        engine: axcenity_release::Version::current().to_string(),
        world: snapshot.world.is_some(),
        files,
    };
    let text = serde_json::to_string_pretty(&info).expect("recovery info always serializes");
    // Written last: a folder without it is a snapshot that didn't finish.
    write_synced(&writing.join(INFO_FILE), text.as_bytes())?;

    // Swap it in: the newest becomes the previous, the new one the newest.
    let current = folder.join(RECOVERY_FOLDER);
    let previous = folder.join(PREVIOUS_FOLDER);
    let _ = fs::remove_dir_all(&previous);
    if current.exists() {
        fs::rename(&current, &previous).map_err(SaveError::new(&current))?;
    }
    fs::rename(&writing, &current).map_err(SaveError::new(&current))?;
    let _ = fs::remove_dir_all(&previous);
    Ok(())
}

/// The project's snapshot, if there is a whole one: the newest, or the
/// previous if the editor stopped while swapping a new one in.
fn snapshot_folder(folder: &Path) -> Option<PathBuf> {
    [RECOVERY_FOLDER, PREVIOUS_FOLDER]
        .into_iter()
        .map(|name| folder.join(name))
        .find(|snapshot| snapshot.join(INFO_FILE).is_file())
}

/// What the project's snapshot holds, if it has one.
pub fn recovery_info(folder: &Path) -> Option<RecoveryInfo> {
    let snapshot = snapshot_folder(folder)?;
    let text = fs::read_to_string(snapshot.join(INFO_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The project's snapshot if it was left behind: there, and not touched
/// by an editor for [`RECOVERY_STALE_AFTER`].
pub fn left_behind_recovery(folder: &Path) -> Option<RecoveryInfo> {
    let info = recovery_info(folder)?;
    let snapshot = snapshot_folder(folder)?;
    let touched = fs::metadata(snapshot.join(INFO_FILE))
        .and_then(|meta| meta.modified())
        .ok()?;
    let age = SystemTime::now()
        .duration_since(touched)
        .unwrap_or(Duration::ZERO);
    (age >= RECOVERY_STALE_AFTER).then_some(info)
}

/// Marks the project's snapshot as belonging to an editor that's still
/// running (see [`RECOVERY_STALE_AFTER`]). Does nothing without one.
pub fn touch_recovery(folder: &Path) {
    if let Some(snapshot) = snapshot_folder(folder)
        && let Ok(file) = File::options().append(true).open(snapshot.join(INFO_FILE))
    {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Removes the project's snapshot (after a save, or when recovering is
/// turned down).
pub fn discard_recovery(folder: &Path) -> Result<(), SaveError> {
    for name in [RECOVERY_FOLDER, PREVIOUS_FOLDER, WRITING_FOLDER] {
        let path = folder.join(name);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(SaveError::new(&path))?;
        }
    }
    Ok(())
}

/// A project read with its snapshot's work in it.
pub struct Recovered {
    /// The project as on disk, with the snapshot's world in place of the
    /// saved one (the saved one when the snapshot has no world).
    pub project: Project,
    pub info: RecoveryInfo,
    /// The documents' unsaved text: (project path, text).
    pub files: Vec<(String, String)>,
}

/// Reads the project in `folder` with its snapshot's work in it. `Ok(None)`
/// if it has no snapshot. Writes nothing.
pub fn load_recovered(folder: &Path) -> Result<Option<Recovered>, LoadError> {
    let Some(snapshot) = snapshot_folder(folder) else {
        return Ok(None);
    };
    let Some(info) = recovery_info(folder) else {
        return Ok(None);
    };
    let world = if info.world {
        let path = snapshot.join(WORLD_FILE);
        let text = crate::read(&path)?;
        Some(parse_world(&text).map_err(|source| LoadError::World { path, source })?)
    } else {
        None
    };
    let project = crate::load_project_with(folder, info.level.as_deref(), world, &mut |_| {})?;
    let mut files = Vec::new();
    for path in &info.files {
        let Some(relative) = safe_relative(path) else {
            continue;
        };
        let full = snapshot.join(FILES_FOLDER).join(relative);
        files.push((path.clone(), crate::read(&full)?));
    }
    Ok(Some(Recovered {
        project,
        info,
        files,
    }))
}

/// `path` as a relative path inside the project, or `None` if it's
/// absolute or climbs out with `..`.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    let inside = !path.is_empty()
        && relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    inside.then(|| relative.to_owned())
}

/// Writes `bytes` to `path` (making its folder) and flushes them to the
/// disk, so a power cut right after doesn't leave an empty file.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SaveError::new(parent))?;
    }
    let mut file = File::create(path).map_err(SaveError::new(path))?;
    file.write_all(bytes).map_err(SaveError::new(path))?;
    file.sync_all().map_err(SaveError::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_project, load};
    use axcenity_core::{ClassName, Value};

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-{name}-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn age(folder: &Path, by: Duration) {
        let info = folder.join(RECOVERY_FOLDER).join(INFO_FILE);
        let file = File::options().append(true).open(info).unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    /// A snapshot of an unsaved part and script text comes back with the
    /// project's scripts, without touching the saved files.
    #[test]
    fn a_snapshot_brings_back_unsaved_work_and_leaves_the_saved_files_alone() {
        let parent = temp_folder("snapshot");
        let folder = create_project(&parent, "Lost Work").unwrap();
        crate::write_script(&folder, "scripts/Hello.server.lua", "print('saved')").unwrap();
        let saved_world = fs::read_to_string(folder.join(WORLD_FILE)).unwrap();

        let mut world = load(&folder).unwrap().world;
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, "Unsaved").unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "Transparency", Value::Number(0.5))
            .unwrap();
        let snapshot = RecoverySnapshot {
            level: None,
            world: Some(world),
            files: vec![("scripts/Hello.server.lua".into(), "print('unsaved')".into())],
        };
        write_recovery(&folder, &snapshot).unwrap();
        assert_eq!(
            fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
            saved_world
        );

        let recovered = load_recovered(&folder).unwrap().unwrap();
        let world = &recovered.project.world;
        let part = world
            .find_first_child(world.workspace(), "Unsaved")
            .unwrap();
        assert_eq!(
            world.get(part).unwrap().property("Transparency"),
            Some(&Value::Number(0.5))
        );
        let paths: Vec<_> = recovered.project.scripts.iter().map(|s| &s.path).collect();
        assert!(
            paths.iter().any(|p| *p == "scripts/Hello.server.lua"),
            "{paths:?}"
        );
        assert_eq!(
            recovered.files,
            vec![(
                "scripts/Hello.server.lua".to_string(),
                "print('unsaved')".to_string()
            )]
        );
        assert!(recovered.info.world);
        assert!(recovered.info.saved_at > 0);
        fs::remove_dir_all(parent).unwrap();
    }

    /// The newest snapshot replaces the last; one being written when the
    /// editor stopped is ignored, and the previous one is still read if
    /// the swap didn't finish. Discarding removes them all.
    #[test]
    fn snapshots_swap_in_whole_and_a_half_written_one_is_ignored() {
        let parent = temp_folder("swap");
        let folder = create_project(&parent, "Swap").unwrap();
        let first = RecoverySnapshot {
            files: vec![("scripts/A.server.lua".into(), "1".into())],
            ..Default::default()
        };
        write_recovery(&folder, &first).unwrap();
        let second = RecoverySnapshot {
            files: vec![("scripts/A.server.lua".into(), "2".into())],
            ..Default::default()
        };
        write_recovery(&folder, &second).unwrap();
        let files = load_recovered(&folder).unwrap().unwrap().files;
        assert_eq!(files[0].1, "2");
        assert!(!folder.join(PREVIOUS_FOLDER).exists());

        // A power cut between the two renames: only the previous is left.
        fs::rename(folder.join(RECOVERY_FOLDER), folder.join(PREVIOUS_FOLDER)).unwrap();
        // And a new one that never got its info file.
        fs::create_dir_all(folder.join(WRITING_FOLDER).join(FILES_FOLDER)).unwrap();
        let files = load_recovered(&folder).unwrap().unwrap().files;
        assert_eq!(files[0].1, "2");

        discard_recovery(&folder).unwrap();
        assert!(recovery_info(&folder).is_none());
        assert!(load_recovered(&folder).unwrap().is_none());
        for name in [RECOVERY_FOLDER, PREVIOUS_FOLDER, WRITING_FOLDER] {
            assert!(!folder.join(name).exists(), "{name}");
        }
        fs::remove_dir_all(parent).unwrap();
    }

    /// Nothing unsaved means no snapshot; paths outside the project are
    /// never written.
    #[test]
    fn an_empty_snapshot_removes_it_and_paths_stay_inside() {
        let parent = temp_folder("empty");
        let folder = create_project(&parent, "Empty").unwrap();
        let snapshot = RecoverySnapshot {
            files: vec![
                ("../outside.lua".into(), "no".into()),
                ("scripts/In.server.lua".into(), "yes".into()),
            ],
            ..Default::default()
        };
        write_recovery(&folder, &snapshot).unwrap();
        assert!(!parent.join("outside.lua").exists());
        assert!(!folder.join(RECOVERY_FOLDER).join("outside.lua").exists());
        assert_eq!(
            recovery_info(&folder).unwrap().files,
            vec!["scripts/In.server.lua"]
        );
        write_recovery(&folder, &RecoverySnapshot::default()).unwrap();
        assert!(recovery_info(&folder).is_none());
        fs::remove_dir_all(parent).unwrap();
    }

    /// A snapshot an editor touched lately is that editor's live work, not
    /// left behind; one untouched for a while is.
    #[test]
    fn only_a_snapshot_nobody_touched_lately_counts_as_left_behind() {
        let parent = temp_folder("stale");
        let folder = create_project(&parent, "Stale").unwrap();
        let snapshot = RecoverySnapshot {
            files: vec![("scripts/A.server.lua".into(), "1".into())],
            ..Default::default()
        };
        write_recovery(&folder, &snapshot).unwrap();
        assert!(left_behind_recovery(&folder).is_none(), "just written");
        age(&folder, RECOVERY_STALE_AFTER * 2);
        assert!(left_behind_recovery(&folder).is_some());
        touch_recovery(&folder);
        assert!(left_behind_recovery(&folder).is_none(), "touched again");
        fs::remove_dir_all(parent).unwrap();
    }

    /// With a level open, the snapshot's world holds that level's
    /// Workspace, and recovering opens it in that level.
    #[test]
    fn a_snapshot_taken_in_a_level_comes_back_in_it() {
        let parent = temp_folder("level");
        let folder = create_project(&parent, "Levels").unwrap();
        let mut world = load(&folder).unwrap().world;
        let workspace = world.workspace();
        for child in world.get(workspace).unwrap().children().to_vec() {
            world.destroy(child).unwrap();
        }
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, "InTheCave").unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        let snapshot = RecoverySnapshot {
            level: Some("Cave".into()),
            world: Some(world),
            files: Vec::new(),
        };
        write_recovery(&folder, &snapshot).unwrap();
        let recovered = load_recovered(&folder).unwrap().unwrap();
        assert_eq!(recovered.info.level.as_deref(), Some("Cave"));
        let world = &recovered.project.world;
        assert!(
            world
                .find_first_child(world.workspace(), "InTheCave")
                .is_some()
        );
        assert!(
            world
                .find_first_child(world.workspace(), "Baseplate")
                .is_none()
        );
        fs::remove_dir_all(parent).unwrap();
    }
}
