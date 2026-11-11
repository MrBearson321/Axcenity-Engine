//! What `axcenity-updater` does once the editor has handed over: wait for
//! the editor to exit, swap the install folder for the staged one (keeping
//! the old one as `<install>.previous`), check the new editor runs and is
//! the expected version, and put the old one back if anything fails. A roll
//! back swaps `<install>.previous` in the same way.
//!
//! It only ever touches the install folder, its `.new`, `.previous`,
//! `.failed` and `.rollback` siblings, and the staged folder. It refuses an
//! install folder that is a root, the user's home, a link, or has no
//! `install.json`, and it never follows links.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axcenity_release::InstallKind;
use axcenity_release::install::{INSTALL_FILE, InstallMarker};
use axcenity_release::publish::UPDATE_PROTOCOL;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The editor's executable in an install folder.
pub const EDITOR_EXE: &str = if cfg!(windows) {
    "axcenity-editor.exe"
} else {
    "axcenity-editor"
};
/// The updater's executable in an install folder.
pub const UPDATER_EXE: &str = if cfg!(windows) {
    "axcenity-updater.exe"
} else {
    "axcenity-updater"
};
/// Files the installer adds that no update archive carries; they move to
/// the new folder.
const KEEP: &[&str] = &["Uninstall.exe"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Install,
    RollBack,
}

/// What the editor asks for (`instruction.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruction {
    pub schema: u32,
    /// The update protocol of the editor that wrote it; an updater that
    /// knows an older one refuses.
    pub protocol: u32,
    pub action: Action,
    pub install: PathBuf,
    /// The verified, unpacked update (for [`Action::Install`]).
    #[serde(default)]
    pub staged: Option<PathBuf>,
    /// What the new editor's `--version --json` must say.
    #[serde(default)]
    pub expect: Option<Expected>,
    pub editor_pid: u32,
    /// Arguments for the editor started afterwards (the project it had open).
    #[serde(default)]
    pub relaunch: Vec<String>,
    /// Where the log goes and the result is written.
    pub log: PathBuf,
    pub result: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub version: String,
    pub commit: String,
}

/// What happened, for the editor's next start (`result.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub action: Action,
    pub ok: bool,
    /// The version installed now.
    pub version: String,
    /// What failed, for the user; empty when it worked.
    pub message: String,
    /// Unix seconds.
    pub at: u64,
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("{0}")]
    Refused(String),
    #[error("{what}: {source}")]
    Io {
        what: String,
        #[source]
        source: io::Error,
    },
    #[error("the new editor didn't start correctly: {0}")]
    Verify(String),
    #[error("the editor didn't close, so nothing was changed")]
    StillRunning,
}

fn io(what: impl Into<String>) -> impl FnOnce(io::Error) -> ApplyError {
    let what = what.into();
    move |source| ApplyError::Io { what, source }
}

/// The outside world: the processes the updater waits for, starts and asks.
/// Tests give a fake one.
pub trait Host {
    /// Waits until process `pid` has exited; false after a timeout.
    fn wait_for_exit(&mut self, pid: u32) -> bool;
    /// The version and commit an editor's `--version --json` reports.
    fn version_of(&mut self, editor: &Path) -> Result<(String, String), String>;
    /// Starts the editor and leaves it running.
    fn launch(&mut self, editor: &Path, args: &[String]) -> Result<(), String>;
    /// After a swap: updates the OS's record of the install (the Windows
    /// uninstall entry's version). Failures are only logged.
    fn record_version(&mut self, _kind: InstallKind, _version: &str) -> Result<(), String> {
        Ok(())
    }
    /// Retries a rename that the OS refuses for a while (Windows keeps a
    /// closing program's files locked for a moment).
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

/// Appends timestamped lines to the update log.
pub struct Log {
    file: Option<fs::File>,
}

impl Log {
    pub fn open(path: &Path) -> Log {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Log {
            file: fs::OpenOptions::new().create(true).append(true).open(path).ok(),
        }
    }

    pub fn line(&mut self, text: &str) {
        eprintln!("{text}");
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "[{}] {text}", now());
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Carries out `instruction`, writes its result file, and starts an editor
/// (the new one, or the old one after a failure).
pub fn run(instruction: &Instruction, host: &mut dyn Host) -> Outcome {
    let mut log = Log::open(&instruction.log);
    log.line(&format!(
        "{:?} in {}",
        instruction.action,
        instruction.install.display()
    ));
    let result = match instruction.action {
        Action::Install => install(instruction, host, &mut log),
        Action::RollBack => roll_back(instruction, host, &mut log),
    };
    let editor = instruction.install.join(EDITOR_EXE);
    let version = host.version_of(&editor).map(|(v, _)| v).unwrap_or_default();
    let outcome = Outcome {
        action: instruction.action,
        ok: result.is_ok(),
        version,
        message: result.as_ref().err().map(ToString::to_string).unwrap_or_default(),
        at: now(),
    };
    match &result {
        Ok(()) => log.line("done"),
        Err(error) => log.line(&format!("failed: {error}")),
    }
    let text = serde_json::to_string_pretty(&outcome).expect("an outcome always serializes");
    if let Err(error) = fs::write(&instruction.result, text) {
        log.line(&format!("couldn't write the result: {error}"));
    }
    // Whatever happened, an editor comes back, unless it never closed.
    if !matches!(result, Err(ApplyError::StillRunning))
        && let Err(error) = host.launch(&editor, &instruction.relaunch)
    {
        log.line(&format!("couldn't start the editor: {error}"));
    }
    outcome
}

/// `<install>` with `suffix` added to its name, beside it.
pub fn sibling(install: &Path, suffix: &str) -> PathBuf {
    let mut name = install.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    install.with_file_name(name)
}

/// The kind recorded in `folder`'s `install.json`, if it has a readable one.
fn marker(folder: &Path) -> Option<InstallKind> {
    let text = fs::read_to_string(folder.join(INSTALL_FILE)).ok()?;
    serde_json::from_str::<InstallMarker>(&text)
        .ok()
        .map(|m| m.kind)
}

/// Whether the updater may replace `install`; the kind it was installed as.
pub fn check_install(install: &Path) -> Result<InstallKind, ApplyError> {
    let refuse = |why: &str| {
        Err(ApplyError::Refused(format!(
            "{} {why}, so the updater won't touch it",
            install.display()
        )))
    };
    if !install.is_absolute() || install.parent().is_none_or(|p| p.parent().is_none()) {
        return refuse("is not an install folder");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if home.is_some_and(|home| Path::new(&home) == install) {
        return refuse("is the home folder");
    }
    match fs::symlink_metadata(install) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return refuse("is a link or a file"),
        Err(_) => return refuse("doesn't exist"),
    }
    match marker(install) {
        Some(kind @ (InstallKind::WindowsSetup | InstallKind::LinuxTarball)) => Ok(kind),
        Some(kind) => Err(ApplyError::Refused(format!(
            "this copy was installed as {}, which updates through its own package",
            kind.label()
        ))),
        None => refuse("has no install.json"),
    }
}

/// Copies the plain files and folders of `from` into a new `to`.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        } else {
            return Err(io::Error::other(format!(
                "{} is a link or special file",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

/// Removes one of the install's own siblings (`.new`, `.failed`,
/// `.rollback`, or a `.previous` that is an install). Anything else is
/// refused.
fn remove_sibling(install: &Path, suffix: &str) -> Result<(), ApplyError> {
    let path = sibling(install, suffix);
    match fs::symlink_metadata(&path) {
        Err(_) => return Ok(()),
        Ok(meta) if !meta.is_dir() => {
            return Err(ApplyError::Refused(format!(
                "{} is in the way and isn't a folder",
                path.display()
            )));
        }
        Ok(_) => {}
    }
    if suffix == ".previous" && marker(&path).is_none() {
        return Err(ApplyError::Refused(format!(
            "{} is in the way and isn't an older install",
            path.display()
        )));
    }
    fs::remove_dir_all(&path).map_err(io(format!("couldn't remove {}", path.display())))
}

fn install(instruction: &Instruction, host: &mut dyn Host, log: &mut Log) -> Result<(), ApplyError> {
    if instruction.protocol > UPDATE_PROTOCOL {
        return Err(ApplyError::Refused(
            "this update needs a newer updater".to_owned(),
        ));
    }
    let install = &instruction.install;
    let kind = check_install(install)?;
    let (Some(staged), Some(expect)) = (&instruction.staged, &instruction.expect) else {
        return Err(ApplyError::Refused("nothing to install".to_owned()));
    };
    if marker(staged) != Some(kind) || !staged.join(EDITOR_EXE).is_file() {
        return Err(ApplyError::Refused(format!(
            "the update in {} isn't a {} install",
            staged.display(),
            kind.label()
        )));
    }

    // Build the new folder beside the old one, so the swap is two renames
    // on one volume.
    let new = sibling(install, ".new");
    remove_sibling(install, ".new")?;
    log.line(&format!("copying the update to {}", new.display()));
    copy_tree(staged, &new).map_err(io("couldn't copy the update"))?;
    for keep in KEEP {
        let old = install.join(keep);
        if old.is_file() && !new.join(keep).exists() {
            fs::copy(&old, new.join(keep)).map_err(io(format!("couldn't keep {keep}")))?;
        }
    }

    log.line("waiting for the editor to close");
    if !host.wait_for_exit(instruction.editor_pid) {
        let _ = remove_sibling(install, ".new");
        return Err(ApplyError::StillRunning);
    }

    let previous = sibling(install, ".previous");
    remove_sibling(install, ".previous")?;
    host.rename(install, &previous)
        .map_err(io("couldn't move the old version aside"))?;
    if let Err(source) = host.rename(&new, install) {
        let _ = host.rename(&previous, install);
        return Err(ApplyError::Io {
            what: "couldn't move the new version into place".to_owned(),
            source,
        });
    }
    log.line("swapped; checking the new editor");

    let editor = install.join(EDITOR_EXE);
    let checked = match host.version_of(&editor) {
        Ok((version, commit)) if version == expect.version && commit == expect.commit => Ok(()),
        Ok((version, commit)) => Err(format!(
            "it says {version} ({commit}), not {} ({})",
            expect.version, expect.commit
        )),
        Err(error) => Err(error),
    };
    if let Err(why) = checked {
        log.line(&format!("the new editor failed its check ({why}); putting the old one back"));
        let failed = sibling(install, ".failed");
        let _ = remove_sibling(install, ".failed");
        host.rename(install, &failed)
            .map_err(io("couldn't move the failed version aside"))?;
        host.rename(&previous, install)
            .map_err(io("couldn't put the old version back"))?;
        let _ = remove_sibling(install, ".failed");
        return Err(ApplyError::Verify(why));
    }
    if let Err(error) = host.record_version(kind, &expect.version) {
        log.line(&format!("couldn't update the uninstall entry: {error}"));
    }
    Ok(())
}

fn roll_back(instruction: &Instruction, host: &mut dyn Host, log: &mut Log) -> Result<(), ApplyError> {
    let install = &instruction.install;
    let kind = check_install(install)?;
    let previous = sibling(install, ".previous");
    if marker(&previous) != Some(kind) || !previous.join(EDITOR_EXE).is_file() {
        return Err(ApplyError::Refused(
            "there is no earlier version to go back to".to_owned(),
        ));
    }
    log.line("waiting for the editor to close");
    if !host.wait_for_exit(instruction.editor_pid) {
        return Err(ApplyError::StillRunning);
    }
    let aside = sibling(install, ".rollback");
    remove_sibling(install, ".rollback")?;
    host.rename(install, &aside)
        .map_err(io("couldn't move this version aside"))?;
    if let Err(source) = host.rename(&previous, install) {
        let _ = host.rename(&aside, install);
        return Err(ApplyError::Io {
            what: "couldn't put the earlier version back".to_owned(),
            source,
        });
    }
    match host.version_of(&install.join(EDITOR_EXE)) {
        Ok((version, _)) => {
            // The newer one becomes the one to go back to.
            host.rename(&aside, &previous)
                .map_err(io("couldn't keep the newer version"))?;
            if let Err(error) = host.record_version(kind, &version) {
                log.line(&format!("couldn't update the uninstall entry: {error}"));
            }
            Ok(())
        }
        Err(why) => {
            host.rename(install, &previous)
                .map_err(io("couldn't move the earlier version back"))?;
            host.rename(&aside, install)
                .map_err(io("couldn't put this version back"))?;
            Err(ApplyError::Verify(why))
        }
    }
}

/// The real world: processes, `--version --json`, the Windows registry.
pub struct System;

impl Host for System {
    fn wait_for_exit(&mut self, pid: u32) -> bool {
        for _ in 0..120 {
            if !running(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    fn version_of(&mut self, editor: &Path) -> Result<(String, String), String> {
        let mut child = std::process::Command::new(editor)
            .args(["--version", "--json"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start it: {e}"))?;
        for _ in 0..60 {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                let output = child.wait_with_output().map_err(|e| e.to_string())?;
                if !output.status.success() {
                    return Err(format!("it exited with {}", output.status));
                }
                let json: serde_json::Value = serde_json::from_slice(&output.stdout)
                    .map_err(|e| format!("its --version --json wasn't JSON: {e}"))?;
                let field = |name: &str| json[name].as_str().unwrap_or_default().to_owned();
                return Ok((field("version"), field("commit")));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = child.kill();
        Err("it didn't answer --version within 30 seconds".to_owned())
    }

    fn launch(&mut self, editor: &Path, args: &[String]) -> Result<(), String> {
        std::process::Command::new(editor)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }

    fn record_version(&mut self, kind: InstallKind, version: &str) -> Result<(), String> {
        if kind != InstallKind::WindowsSetup || !cfg!(windows) {
            return Ok(());
        }
        // The same key the installer writes (packaging/windows/installer.nsi).
        let status = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor",
                "/v",
                "DisplayVersion",
                "/t",
                "REG_SZ",
                "/d",
                version,
                "/f",
            ])
            .stdout(std::process::Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("reg exited with {status}"))
        }
    }

    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let mut tries = 0;
        loop {
            match fs::rename(from, to) {
                Err(error) if tries < 20 && error.kind() == io::ErrorKind::PermissionDenied => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(500));
                }
                other => return other,
            }
        }
    }
}

/// Whether process `pid` is still running.
fn running(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new(&format!("/proc/{pid}")).exists()
    } else if cfg!(windows) {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
    } else {
        // Other systems don't update themselves (`check_install`).
        false
    }
}
