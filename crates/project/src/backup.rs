//! Back Up Project (unified Phase 72): a `.zip` of the project folder,
//! without the `.axcenity/` cache, written to a folder the creator picks,
//! such as a cloud-synced one. Unzipped anywhere it is the project again.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Where backups go and when, relative to the project folder. It's under
/// `.axcenity/` because the folder is this computer's, not the project's.
pub const BACKUP_SETTINGS_FILE: &str = ".axcenity/backup.json";
/// The cache folder, left out of backups.
const CACHE_DIR: &str = ".axcenity";

/// A project's backup folder, and whether every checkpoint backs up.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSettings {
    #[serde(default)]
    pub folder: Option<PathBuf>,
    #[serde(default)]
    pub on_checkpoint: bool,
}

/// The project's backup settings, or the defaults when it has none.
pub fn backup_settings(folder: &Path) -> BackupSettings {
    fs::read_to_string(folder.join(BACKUP_SETTINGS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_backup_settings(folder: &Path, settings: &BackupSettings) -> io::Result<()> {
    let path = folder.join(BACKUP_SETTINGS_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    fs::write(path, text + "\n")
}

/// Writes `<project folder name>-<YYYY-MM-DD-HHMMSS>.zip` into `destination`
/// and returns its path. Inside, every file sits under a folder with the
/// project folder's name, with `/` between folders. `.axcenity/` is left
/// out, and so is `destination` when it is inside the project.
pub fn back_up_project(folder: &Path, destination: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(destination)?;
    let name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Project".to_owned());
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // `2026-10-30 12:04:05 UTC` as `2026-10-30-120405`.
    let stamp: String = crate::versions::format_unix(secs)
        .trim_end_matches(" UTC")
        .replace(' ', "-")
        .replace(':', "");
    let mut path = destination.join(format!("{name}-{stamp}.zip"));
    let mut n = 2;
    while path.exists() {
        path = destination.join(format!("{name}-{stamp}-{n}.zip"));
        n += 1;
    }
    let skip = fs::canonicalize(destination).ok();
    let mut files = Vec::new();
    collect(folder, "", skip.as_deref(), &mut files)?;
    files.sort();

    let zip_error = |err: zip::result::ZipError| io::Error::other(err.to_string());
    let written = (|| -> io::Result<()> {
        let mut writer = zip::ZipWriter::new(fs::File::create(&path)?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for relative in &files {
            writer
                .start_file(format!("{name}/{relative}"), options)
                .map_err(zip_error)?;
            writer.write_all(&fs::read(folder.join(relative))?)?;
        }
        writer.finish().map_err(zip_error)?;
        Ok(())
    })();
    if let Err(err) = written {
        let _ = fs::remove_file(&path);
        return Err(err);
    }
    Ok(path)
}

/// Every file under `dir`, relative to the project with `/` between folders.
fn collect(
    dir: &Path,
    prefix: &str,
    skip: Option<&Path>,
    files: &mut Vec<String>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if prefix.is_empty() && name == CACHE_DIR {
            continue;
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if skip.is_some() && fs::canonicalize(&path).ok().as_deref() == skip {
                continue;
            }
            collect(&path, &relative, skip, files)?;
        } else if kind.is_file() {
            files.push(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-backup-{name}-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            if entry.file_name() == CACHE_DIR {
                continue;
            }
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// Unpacks `zip` into `into` the way any unzip tool would.
    fn unzip(zip: &Path, into: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().to_owned();
            assert!(!name.contains('\\'), "{name} uses a backslash");
            let relative = entry.enclosed_name().expect("a path inside the folder");
            let target = into.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            fs::write(target, bytes).unwrap();
            names.push(name);
        }
        names
    }

    #[test]
    fn a_backup_zip_opens_as_the_project_somewhere_else() {
        let root = temp("open");
        let project = root.join("Hello Part");
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-part");
        copy_dir(&example, &project);
        crate::record_version(&project, "Save").unwrap();
        assert!(project.join(crate::versions::HISTORY_DIR).is_dir());
        // A backup folder inside the project isn't backed up into itself.
        let backups = project.join("backups");
        save_backup_settings(
            &project,
            &BackupSettings {
                folder: Some(backups.clone()),
                on_checkpoint: true,
            },
        )
        .unwrap();
        assert_eq!(backup_settings(&project).folder, Some(backups.clone()));
        let first = back_up_project(&project, &backups).unwrap();
        let zip = back_up_project(&project, &backups).unwrap();
        assert_ne!(first, zip, "a second backup gets its own name");
        assert!(
            zip.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Hello Part-")
        );

        let elsewhere = root.join("other computer");
        let names = unzip(&zip, &elsewhere);
        assert!(names.iter().all(|name| name.starts_with("Hello Part/")));
        assert!(
            !names.iter().any(|name| name.contains(".axcenity")),
            "{names:?}"
        );
        assert!(
            !names.iter().any(|name| name.contains("backups")),
            "{names:?}"
        );
        assert!(names.contains(&"Hello Part/world.json".to_owned()));

        let opened = crate::load(&elsewhere.join("Hello Part")).unwrap();
        let original = crate::load(&project).unwrap();
        assert_eq!(opened.manifest.name, original.manifest.name);
        let workspace = opened.world.workspace();
        assert!(opened.world.find_first_child(workspace, "Block").is_some());
        assert_eq!(
            opened.world.descendants(workspace).len(),
            original.world.descendants(original.world.workspace()).len()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
