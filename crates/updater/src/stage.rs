//! The editor's side of an update: which payload this install needs, where
//! update files go (`<app data>/Updates/<version>/`), unpacking and checking
//! the download, and handing over to the new release's own updater, which
//! the editor starts just before it exits.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use axcenity_release::install::{INSTALL_FILE, InstallMarker};
use axcenity_release::publish::Artifact;
use axcenity_release::{InstallKind, Manifest};
use thiserror::Error;

use crate::apply::{EDITOR_EXE, Instruction, Outcome, UPDATER_EXE};
use crate::extract::{self, ExtractError, Format};

#[derive(Debug, Error)]
pub enum StageError {
    #[error("{0}")]
    Extract(#[from] ExtractError),
    #[error("couldn't prepare the update: {0}")]
    Io(#[from] io::Error),
    #[error("the update isn't what this install needs: {0}")]
    Wrong(String),
}

/// `<app data>/Updates`: downloads, staged updates, the log and results.
pub fn updates_dir(app_data: &Path) -> PathBuf {
    app_data.join("Updates")
}

/// The update payload in `manifest` for an install of `kind` on this
/// machine, if this kind updates itself and the release has one.
pub fn payload(manifest: &Manifest, kind: InstallKind) -> Option<&Artifact> {
    let platform = match kind {
        InstallKind::WindowsSetup if cfg!(all(windows, target_arch = "x86_64")) => "windows-x86_64",
        InstallKind::LinuxTarball if cfg!(all(target_os = "linux", target_arch = "x86_64")) => {
            "linux-x86_64"
        }
        _ => return None,
    };
    manifest.artifacts.get(platform)?.get("update")
}

/// Unpacks the downloaded `archive` (named `file`, as in the manifest) into
/// a fresh `staged` folder and checks it is a whole install of `kind`: its
/// `install.json`, the editor and the updater.
pub fn stage(
    archive: &Path,
    file: &str,
    kind: InstallKind,
    staged: &Path,
) -> Result<(), StageError> {
    let format =
        Format::of(file).ok_or_else(|| StageError::Wrong(format!("{file} isn't an archive")))?;
    if staged.exists() {
        fs::remove_dir_all(staged)?;
    }
    extract::extract(archive, format, staged)?;
    let marker = fs::read_to_string(staged.join(INSTALL_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<InstallMarker>(&text).ok());
    if marker.map(|m| m.kind) != Some(kind) {
        return Err(StageError::Wrong(format!(
            "it isn't a {} install",
            kind.label()
        )));
    }
    for exe in [EDITOR_EXE, UPDATER_EXE] {
        if !staged.join(exe).is_file() {
            return Err(StageError::Wrong(format!("it has no {exe}")));
        }
    }
    Ok(())
}

/// Copies the staged release's updater to `<version dir>/run/` and writes
/// `instruction.json` beside it. Returns the updater to start, with
/// `apply <instruction>`; the editor exits right after starting it. Running
/// a copy means no program replaces its own running file.
pub fn hand_over(
    version_dir: &Path,
    instruction: &Instruction,
) -> Result<(PathBuf, PathBuf), StageError> {
    let from = match &instruction.staged {
        Some(staged) => staged.join(UPDATER_EXE),
        // A roll back runs this install's own updater.
        None => instruction.install.join(UPDATER_EXE),
    };
    let run = version_dir.join("run");
    if run.exists() {
        fs::remove_dir_all(&run)?;
    }
    fs::create_dir_all(&run)?;
    let updater = run.join(UPDATER_EXE);
    fs::copy(&from, &updater)?;
    let path = version_dir.join("instruction.json");
    let text = serde_json::to_string_pretty(instruction).expect("an instruction always serializes");
    fs::write(&path, text)?;
    Ok((updater, path))
}

/// The result an updater left, if any.
pub fn read_outcome(path: &Path) -> Option<Outcome> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}
