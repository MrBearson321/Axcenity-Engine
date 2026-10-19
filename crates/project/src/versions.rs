//! Saved versions of a project, so a game can be rolled back.
//!
//! Each version is a copy of `world.json`, the `scripts` folder and every
//! `.css` file, under `.axcenity/history/<id>/`. Assets, the manifest and
//! the thumbnail stay as they are. The newest [`MAX_VERSIONS`] are kept.
//! Restoring copies those files back and removes scripts or styles the
//! version doesn't have. `.axcenity` is already left out of exports.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{WORLD_FILE, find_scripts, find_stylesheets};

/// Where versions live, relative to the project folder.
pub const HISTORY_DIR: &str = ".axcenity/history";
const META_FILE: &str = "meta.txt";
/// How many versions to keep. Older ones are deleted.
const MAX_VERSIONS: usize = 40;

/// One saved version of a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The folder name under [`HISTORY_DIR`]. Safe to pass to [`restore_version`].
    pub id: String,
    /// Why it was saved, such as "Save" or "Before restore".
    pub label: String,
    /// Seconds since the Unix epoch, UTC.
    pub saved_unix: u64,
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
        let label = fs::read_to_string(entry.path().join(META_FILE))
            .ok()
            .and_then(|text| text.lines().next().map(str::trim).map(str::to_owned))
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "Save".to_owned());
        let saved_unix = name
            .split('-')
            .next()
            .and_then(|secs| secs.parse().ok())
            .unwrap_or(0);
        versions.push(Version {
            id: name,
            label,
            saved_unix,
        });
    }
    versions.sort_by(|a, b| b.id.cmp(&a.id));
    versions
}

/// Copies the project's world, scripts and styles into a new version named
/// `label`. Does nothing when that is identical to the newest version.
/// Returns whether a new version was written.
pub fn record_version(folder: &Path, label: &str) -> io::Result<bool> {
    let files = tracked_files(folder);
    if let Some(newest) = list_versions(folder).into_iter().next()
        && let Some(dir) = version_dir(folder, &newest.id)
        && same_files(folder, &dir, &files)?
    {
        return Ok(false);
    }
    let id = fresh_id(folder)?;
    let dir = folder.join(HISTORY_DIR).join(&id);
    fs::create_dir_all(&dir)?;
    for path in &files {
        copy_in(folder, &dir, path)?;
    }
    let label = label.replace(['\n', '\r'], " ");
    let label = if label.trim().is_empty() {
        "Save".to_owned()
    } else {
        label
    };
    fs::write(dir.join(META_FILE), format!("{label}\n"))?;
    prune(folder)?;
    Ok(true)
}

/// Puts the version `id` back into `folder`: world, scripts and styles.
/// Scripts or styles that aren't in the version are removed. Assets and the
/// manifest are left as they are.
pub fn restore_version(folder: &Path, id: &str) -> io::Result<()> {
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
    let snapshot = snapshot_files(&dir)?;
    if !snapshot.iter().any(|path| path == WORLD_FILE) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "that version has no world file",
        ));
    }
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

/// `world.json`, every script and every stylesheet, in path order.
fn tracked_files(folder: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let mut files = Vec::new();
    if folder.join(WORLD_FILE).is_file() {
        files.push(WORLD_FILE.to_owned());
    }
    files.extend(find_scripts(folder, &mut problems));
    files.extend(find_stylesheets(folder, &mut problems));
    files.sort();
    files.dedup();
    files
}

fn snapshot_files(dir: &Path) -> io::Result<Vec<String>> {
    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    files.retain(|path| path != META_FILE && safe_relative(path));
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

fn fresh_id(folder: &Path) -> io::Result<String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for n in 0..100 {
        let id = format!("{secs:016}-{n:02}");
        if !folder.join(HISTORY_DIR).join(&id).exists() {
            return Ok(id);
        }
    }
    Err(io::Error::other("could not pick a new version name"))
}

fn prune(folder: &Path) -> io::Result<()> {
    let versions = list_versions(folder);
    for version in versions.into_iter().skip(MAX_VERSIONS) {
        if let Some(dir) = version_dir(folder, &version.id) {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

fn version_dir(folder: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| folder.join(HISTORY_DIR).join(id))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

fn safe_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// `secs` as `YYYY-MM-DD HH:MM:SS UTC`.
fn format_unix(secs: u64) -> String {
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
mod tests {
    use super::*;

    fn temp_folder() -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-versions-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("scripts")).unwrap();
        fs::write(folder.join(WORLD_FILE), "{\"format_version\":1}").unwrap();
        fs::write(folder.join("scripts/hello.server.lua"), "print(1)\n").unwrap();
        fs::write(folder.join("ui.css"), "Frame { }\n").unwrap();
        folder
    }

    #[test]
    fn a_save_can_be_listed_and_restored_and_a_repeat_is_skipped() {
        let folder = temp_folder();
        assert!(list_versions(&folder).is_empty());
        assert!(record_version(&folder, "Save").unwrap());
        assert!(!record_version(&folder, "Save again").unwrap());
        let versions = list_versions(&folder);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].label, "Save");
        assert!(versions[0].when().contains("UTC"));

        fs::write(
            folder.join(WORLD_FILE),
            "{\"format_version\":1,\"changed\":true}",
        )
        .unwrap();
        fs::write(folder.join("scripts/extra.client.lua"), "print(2)\n").unwrap();
        assert!(record_version(&folder, "Second").unwrap());

        restore_version(&folder, &versions[0].id).unwrap();
        assert_eq!(
            fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
            "{\"format_version\":1}"
        );
        assert!(!folder.join("scripts/extra.client.lua").exists());
        assert_eq!(
            fs::read_to_string(folder.join("scripts/hello.server.lua")).unwrap(),
            "print(1)\n"
        );
        assert!(folder.join("ui.css").is_file());
        let _ = fs::remove_dir_all(&folder);
    }
}
