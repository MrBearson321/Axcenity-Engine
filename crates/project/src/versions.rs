//! Saved versions of a project, so a game can be rolled back.
//!
//! Each version is a copy of `world.json`, the levels in `levels/`, the
//! `scripts` folder and every `.css` file, under `.axcenity/history/<id>/`,
//! with a list of the assets as they were (size and time, not their bytes).
//! Assets, the manifest and the thumbnail stay as they are on restore. The
//! newest [`MAX_VERSIONS`] unpinned versions are kept; pinned ones
//! (checkpoints, and what a restore replaced) are kept until unpinned.
//! Restoring copies those files back and removes scripts or styles the
//! version doesn't have. `.axcenity` is already left out of exports.

mod changes;

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{COLLISION_GROUPS_FILE, STRINGS_FILE, WORLD_FILE, find_scripts, find_stylesheets};

pub use changes::{
    AssetChange, Change, ChangedFile, Changes, InstanceChange, Line, around_changes,
};

/// Where versions live, relative to the project folder.
pub const HISTORY_DIR: &str = ".axcenity/history";
/// The label on its first line, then `pinned` when it is.
const META_FILE: &str = "meta.txt";
/// One line per file under `assets/`: size, modified time and path.
const ASSETS_FILE: &str = "assets.txt";
const ASSETS_DIR: &str = "assets";
const PINNED: &str = "pinned";
/// How many unpinned versions to keep. Older ones are deleted.
const MAX_VERSIONS: usize = 40;
/// The label of what a restore replaced.
const BEFORE_RESTORE: &str = "Before restore";

/// One saved version of a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The folder name under [`HISTORY_DIR`]. Safe to pass to [`restore_version`].
    pub id: String,
    /// Why it was saved, such as "Save", "Before restore" or a checkpoint's message.
    pub label: String,
    /// Seconds since the Unix epoch, UTC.
    pub saved_unix: u64,
    /// Kept however many saves follow (a checkpoint, or what a restore replaced).
    pub pinned: bool,
}

impl Version {
    /// `2026-10-18 23:17:00 UTC`.
    pub fn when(&self) -> String {
        format_unix(self.saved_unix)
    }
}

/// The versions in `folder`, newest first. A project with none gives an empty list.
pub fn list_versions(folder: &Path) -> Vec<Version> {
    let Ok(entries) = fs::read_dir(folder.join(HISTORY_DIR)) else {
        return Vec::new();
    };
    let mut versions = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&name) || !entry.path().is_dir() {
            continue;
        }
        let (label, pinned) = read_meta(&entry.path());
        let saved_unix = parse_id(&name).map_or(0, |(secs, _)| secs);
        versions.push(Version {
            id: name,
            label,
            saved_unix,
            pinned,
        });
    }
    versions.sort_by(|a, b| b.id.cmp(&a.id));
    versions
}

/// Copies the project's world, scripts and styles into a new version named
/// `label`. Does nothing when that is identical to the newest version.
/// Returns whether a new version was written.
pub fn record_version(folder: &Path, label: &str) -> io::Result<bool> {
    record(folder, label).map(|(_, new)| new)
}

/// File > Version History's Save Checkpoint: keeps the current files as a
/// pinned version labeled `message` ("Checkpoint" when it's empty). When the
/// newest version already has these files, that one is relabeled and pinned
/// instead. Returns the version's id.
pub fn record_checkpoint(folder: &Path, message: &str) -> io::Result<String> {
    let (id, _) = record(folder, message)?;
    let dir = folder.join(HISTORY_DIR).join(&id);
    write_meta(&dir, &clean_label(message, "Checkpoint"), true)?;
    Ok(id)
}

/// Pins or unpins version `id`. An unpinned version is dropped once
/// [`MAX_VERSIONS`] newer unpinned ones exist.
pub fn set_pinned(folder: &Path, id: &str, pinned: bool) -> io::Result<()> {
    let dir = existing_version(folder, id)?;
    let (label, _) = read_meta(&dir);
    write_meta(&dir, &label, pinned)?;
    if !pinned {
        prune(folder)?;
    }
    Ok(())
}

/// Puts the version `id` back into `folder`: world, string table, scripts
/// and styles.
/// Scripts or styles that aren't in the version are removed. Assets and the
/// manifest are left as they are. The current files are kept first, as a
/// pinned version labeled "Before restore", so the restore can be undone.
pub fn restore_version(folder: &Path, id: &str) -> io::Result<()> {
    let dir = existing_version(folder, id)?;
    let snapshot = snapshot_files(&dir)?;
    if !snapshot.iter().any(|path| path == WORLD_FILE) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "that version has no world file",
        ));
    }
    keep_before_restore(folder)?;
    for path in &snapshot {
        copy_in(&dir, folder, path)?;
    }
    for path in tracked_files(folder) {
        if !snapshot.iter().any(|saved| saved == &path) {
            let _ = fs::remove_file(folder.join(&path));
        }
    }
    Ok(())
}

/// Puts one file of version `id` back, such as `scripts/door.server.lua`.
/// A file the version doesn't have is removed from the project. Like
/// [`restore_version`], the current files are kept first.
pub fn restore_file(folder: &Path, id: &str, path: &str) -> io::Result<()> {
    let dir = existing_version(folder, id)?;
    if !safe_relative(path) || path == META_FILE || path == ASSETS_FILE {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "that isn't a file a version keeps",
        ));
    }
    let in_version = dir.join(path).is_file();
    let in_project = tracked_files(folder).iter().any(|tracked| tracked == path);
    if !in_version && !in_project {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!("`{path}` is in neither that version nor the project"),
        ));
    }
    keep_before_restore(folder)?;
    if in_version {
        copy_in(&dir, folder, path)
    } else {
        fs::remove_file(folder.join(path))
    }
}

/// What changed in version `id` since the version before it (everything is
/// "added" in the oldest): Instances in the world and levels, the lines of
/// scripts and other files, and which assets were added, removed or changed.
pub fn version_changes(folder: &Path, id: &str) -> io::Result<Changes> {
    let dir = existing_version(folder, id)?;
    let versions = list_versions(folder);
    let older = versions
        .iter()
        .position(|version| version.id == id)
        .and_then(|index| versions.get(index + 1))
        .and_then(|version| version_dir(folder, &version.id));
    let read_side = |dir: &Path| -> io::Result<changes::Side> {
        let mut files = Vec::new();
        for path in snapshot_files(dir)? {
            let bytes = fs::read(dir.join(&path))?;
            files.push((path, String::from_utf8_lossy(&bytes).into_owned()));
        }
        let assets = fs::read_to_string(dir.join(ASSETS_FILE)).ok();
        Ok(changes::Side { files, assets })
    };
    let new = read_side(&dir)?;
    let old = match &older {
        Some(dir) => read_side(dir)?,
        // Nothing before it: every file is new, and so is every asset.
        None => changes::Side {
            files: Vec::new(),
            assets: new.assets.as_ref().map(|_| String::new()),
        },
    };
    Ok(changes::compare(&old, &new))
}

/// `world.json`, `strings.json`, `collision-groups.json`, every level in
/// `levels/`, every script (`objects/`' too) and every stylesheet, in path
/// order.
fn tracked_files(folder: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let mut files = Vec::new();
    for file in [WORLD_FILE, STRINGS_FILE, COLLISION_GROUPS_FILE] {
        if folder.join(file).is_file() {
            files.push(file.to_owned());
        }
    }
    files.extend(
        crate::level_names(folder)
            .iter()
            .map(|name| crate::level_path(name)),
    );
    files.extend(find_scripts(folder, &mut problems));
    // The code of scripts inside objects (G2), which the world names.
    files.extend(
        crate::validate::files_under(folder, crate::OBJECTS_FOLDER)
            .into_iter()
            .filter(|file| file.ends_with(".lua") || crate::is_block_script(file)),
    );
    files.extend(find_stylesheets(folder, &mut problems));
    files.sort();
    files.dedup();
    files
}

/// Writes a version of the current files, unless the newest already has
/// them. Returns the id of the version that has them and whether it's new.
fn record(folder: &Path, label: &str) -> io::Result<(String, bool)> {
    let files = tracked_files(folder);
    let assets = asset_listing(folder);
    if let Some(newest) = list_versions(folder).into_iter().next()
        && let Some(dir) = version_dir(folder, &newest.id)
        && same_files(folder, &dir, &files)?
        && fs::read_to_string(dir.join(ASSETS_FILE)).ok().as_deref() == Some(assets.as_str())
    {
        return Ok((newest.id, false));
    }
    let id = fresh_id(folder)?;
    let dir = folder.join(HISTORY_DIR).join(&id);
    fs::create_dir_all(&dir)?;
    for path in &files {
        copy_in(folder, &dir, path)?;
    }
    fs::write(dir.join(ASSETS_FILE), assets)?;
    write_meta(&dir, &clean_label(label, "Save"), false)?;
    prune(folder)?;
    Ok((id, true))
}

/// Keeps the current files as a pinned version before a restore replaces
/// them. When the newest version already has them, that one is pinned, and
/// a plain "Save" is relabeled to say why.
fn keep_before_restore(folder: &Path) -> io::Result<()> {
    let (id, _) = record(folder, BEFORE_RESTORE)?;
    let dir = folder.join(HISTORY_DIR).join(&id);
    let (label, _) = read_meta(&dir);
    let label = if label == "Save" {
        BEFORE_RESTORE
    } else {
        &label
    };
    write_meta(&dir, label, true)
}

fn clean_label(label: &str, default: &str) -> String {
    let label = label.replace(['\n', '\r'], " ");
    let label = label.trim();
    if label.is_empty() {
        default.to_owned()
    } else {
        label.to_owned()
    }
}

fn read_meta(dir: &Path) -> (String, bool) {
    let text = fs::read_to_string(dir.join(META_FILE)).unwrap_or_default();
    let mut lines = text.lines().map(str::trim);
    let label = lines
        .next()
        .filter(|label| !label.is_empty())
        .unwrap_or("Save")
        .to_owned();
    let pinned = lines.any(|line| line == PINNED);
    (label, pinned)
}

fn write_meta(dir: &Path, label: &str, pinned: bool) -> io::Result<()> {
    let pin = if pinned { "pinned\n" } else { "" };
    fs::write(dir.join(META_FILE), format!("{label}\n{pin}"))
}

/// Every file under `assets/`, one per line: size, modified time and path,
/// in path order. Cheap to make, so it's taken on every save.
fn asset_listing(folder: &Path) -> String {
    let mut files = Vec::new();
    let _ = walk(&folder.join(ASSETS_DIR), ASSETS_DIR, &mut files);
    files.sort();
    let mut listing = String::new();
    for path in files {
        let Ok(meta) = fs::metadata(folder.join(&path)) else {
            continue;
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_millis());
        listing.push_str(&format!("{}\t{modified}\t{path}\n", meta.len()));
    }
    listing
}

fn snapshot_files(dir: &Path) -> io::Result<Vec<String>> {
    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    files.retain(|path| path != META_FILE && path != ASSETS_FILE && safe_relative(path));
    files.sort();
    Ok(files)
}

fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let shown = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        if path.is_dir() {
            walk(&path, &shown, files)?;
        } else if path.is_file() {
            files.push(shown);
        }
    }
    Ok(())
}

fn copy_in(from_root: &Path, to_root: &Path, relative: &str) -> io::Result<()> {
    if !safe_relative(relative) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "a version contains a path outside the project",
        ));
    }
    let from = from_root.join(relative);
    let to = to_root.join(relative);
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&from, &to)
        .map(|_| ())
        .map_err(|err| io::Error::new(err.kind(), format!("could not copy `{relative}`: {err}")))
}

fn same_files(project: &Path, snapshot: &Path, files: &[String]) -> io::Result<bool> {
    let saved = snapshot_files(snapshot)?;
    if saved != files {
        return Ok(false);
    }
    for path in files {
        if fs::read(project.join(path))? != fs::read(snapshot.join(path))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A name after every existing one, so the newest always sorts first even
/// with many saves in one second (or a clock that went back).
fn fresh_id(folder: &Path) -> io::Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (mut secs, mut n) = (now, 0);
    if let Some((newest, k)) = list_versions(folder)
        .first()
        .and_then(|version| parse_id(&version.id))
        && newest >= now
    {
        (secs, n) = (newest, k + 1);
    }
    for _ in 0..1000 {
        if n > 99 {
            (secs, n) = (secs + 1, 0);
        }
        let id = format!("{secs:016}-{n:02}");
        if !folder.join(HISTORY_DIR).join(&id).exists() {
            return Ok(id);
        }
        n += 1;
    }
    Err(io::Error::other("could not pick a new version name"))
}

/// `0000001760000000-03` as its seconds and its number within that second.
fn parse_id(id: &str) -> Option<(u64, u32)> {
    let (secs, n) = id.split_once('-')?;
    Some((secs.parse().ok()?, n.parse().ok()?))
}

fn prune(folder: &Path) -> io::Result<()> {
    let unpinned = list_versions(folder)
        .into_iter()
        .filter(|version| !version.pinned);
    for version in unpinned.skip(MAX_VERSIONS) {
        if let Some(dir) = version_dir(folder, &version.id) {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

fn version_dir(folder: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| folder.join(HISTORY_DIR).join(id))
}

/// The folder of version `id`, or an error saying why there isn't one.
fn existing_version(folder: &Path, id: &str) -> io::Result<PathBuf> {
    let Some(dir) = version_dir(folder, id) else {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "that version name isn't valid",
        ));
    };
    if !dir.is_dir() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "that version is not there",
        ));
    }
    Ok(dir)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

pub(crate) fn safe_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// `secs` as `YYYY-MM-DD HH:MM:SS UTC`.
pub(crate) fn format_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Days since 1970-01-01 to a civil date. Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests;
