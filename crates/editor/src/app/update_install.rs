//! Installing an update from Settings > Updates (release system Phase 6;
//! `crates/updater`): download the payload this install needs (resumable,
//! checked against the signed manifest's size and SHA-256) and unpack it on
//! a thread, then, once asked, save or discard like closing, hand over to the
//! new release's `axcenity-updater` and close. The updater swaps the install
//! folder, checks the new editor, rolls back if it fails, and opens the
//! editor again; the next start says how it went. Roll back works the same
//! way with the install's own updater. Only Windows setup installs and the
//! Linux `.tar.gz` update themselves (`InstallKind::updates_itself`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

use axcenity_release::publish::UPDATE_PROTOCOL;
use axcenity_release::updates::{self, Update};
use axcenity_release::{InstallKind, Version};
use axcenity_updater::apply::{self as updater, Action, Expected, Instruction};
use axcenity_updater::download::{self, Fetch};
use axcenity_updater::stage;

use super::*;

/// Sent with download requests: names the program, nothing about the user.
const USER_AGENT: &str = concat!("AxcenityEditor/", env!("CARGO_PKG_VERSION"), " (updater)");

/// An update ready to install: unpacked and checked.
#[derive(Clone, Debug)]
pub(super) struct Staged {
    pub version: Version,
    version_dir: PathBuf,
    staged: PathBuf,
    expect: Expected,
}

/// Where installing the found update has got to.
#[derive(Default)]
pub(super) enum Install {
    #[default]
    Idle,
    Working {
        version: Version,
        done: Arc<AtomicU64>,
        total: u64,
        cancel: Arc<AtomicBool>,
        result: mpsc::Receiver<Result<Staged, String>>,
    },
    Ready(Staged),
    Failed(String),
}

impl Install {
    /// Takes a finished download's result.
    pub fn poll(&mut self) {
        let Install::Working { result, .. } = self else {
            return;
        };
        let next = match result.try_recv() {
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Install::Failed("the download stopped unexpectedly".to_owned())
            }
            Ok(Ok(staged)) => Install::Ready(staged),
            Ok(Err(error)) if error == download::DownloadError::Cancelled.to_string() => {
                Install::Idle
            }
            Ok(Err(error)) => Install::Failed(error),
        };
        *self = next;
    }

    /// Downloads and stages `update` for an install of `kind`, keeping its
    /// files in `updates_dir/<version>/`, on a thread.
    pub fn start(
        &mut self,
        update: &Update,
        kind: InstallKind,
        updates_dir: &Path,
        fetch: Box<dyn Fetch + Send>,
    ) {
        if matches!(self, Install::Working { .. }) {
            return;
        }
        let Some(artifact) = stage::payload(&update.manifest, kind).cloned() else {
            *self = Install::Failed("this release has no update for this install".to_owned());
            return;
        };
        let version = update.candidate.version.clone();
        let version_dir = updates_dir.join(version.to_string());
        let url = updates::asset_url(&update.candidate.tag, &artifact.file);
        let expect = Expected {
            version: version.to_string(),
            commit: update.manifest.commit.clone(),
        };
        let total = artifact.size;
        let done = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, result) = mpsc::channel();
        let (thread_done, thread_cancel, thread_version) =
            (done.clone(), cancel.clone(), version.clone());
        std::thread::Builder::new()
            .name("update download".to_owned())
            .spawn(move || {
                let archive = version_dir.join(&artifact.file);
                let staged = version_dir.join("staged");
                let outcome = download::download(
                    fetch.as_ref(),
                    &url,
                    &artifact,
                    &archive,
                    &mut |have, _| thread_done.store(have, Ordering::Relaxed),
                    &thread_cancel,
                )
                .map_err(|e| e.to_string())
                .and_then(|archive| {
                    stage::stage(&archive, &artifact.file, kind, &staged).map_err(|e| e.to_string())
                })
                .map(|()| Staged {
                    version: thread_version,
                    version_dir,
                    staged,
                    expect,
                });
                let _ = send.send(outcome);
            })
            .expect("a thread can be started");
        *self = Install::Working {
            version,
            done,
            total,
            cancel,
            result,
        };
    }
}

/// The folder this editor runs from.
fn install_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_owned)
}

/// `<app data>/Updates`.
fn updates_dir() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|data| stage::updates_dir(&data))
}

/// Whether there's an earlier version kept to go back to.
fn can_roll_back(kind: InstallKind) -> bool {
    kind.updates_itself()
        && install_dir().is_some_and(|install| {
            let previous = updater::sibling(&install, ".previous");
            InstallKind::of_folder(&previous) == kind
        })
}

impl EditorApp {
    /// Each frame: follows the download.
    pub(super) fn update_install_frame(&mut self) {
        self.updates.install.poll();
    }

    /// Download and install: starts downloading the found update.
    pub(super) fn download_update(&mut self) {
        let Some(update) = self.updates.available_or_dismissed().cloned() else {
            return;
        };
        let Some(dir) = updates_dir() else {
            self.updates.install = Install::Failed("there's no folder for app data".to_owned());
            return;
        };
        self.updates.install.start(
            &update,
            InstallKind::current(),
            &dir,
            Box::new(download::HttpFetch::new(USER_AGENT)),
        );
    }

    /// Hands over to the updater and closes, once unsaved work was saved
    /// or discarded (`Discard::Update`).
    pub(super) fn hand_over_update(&mut self, ctx: &egui::Context, action: Action) {
        match self.write_hand_over(action) {
            Ok((updater, instruction)) => {
                match std::process::Command::new(&updater)
                    .arg("apply")
                    .arg(&instruction)
                    .spawn()
                {
                    Ok(_) => {
                        self.closing = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => self
                        .toasts
                        .error(format!("Couldn't start the updater: {error}")),
                }
            }
            Err(error) => self.toasts.error(error),
        }
    }

    fn write_hand_over(&self, action: Action) -> Result<(PathBuf, PathBuf), String> {
        let install = install_dir().ok_or("couldn't find the editor's folder")?;
        let dir = updates_dir().ok_or("there's no folder for app data")?;
        let (version_dir, staged, expect) = match (action, &self.updates.install) {
            (Action::Install, Install::Ready(staged)) => (
                staged.version_dir.clone(),
                Some(staged.staged.clone()),
                Some(staged.expect.clone()),
            ),
            (Action::Install, _) => return Err("the update isn't downloaded yet".to_owned()),
            (Action::RollBack, _) => (dir.join("rollback"), None, None),
        };
        let relaunch = self
            .project
            .as_ref()
            .map(|project| vec![project.folder.display().to_string()])
            .unwrap_or_default();
        let instruction = Instruction {
            schema: 1,
            protocol: UPDATE_PROTOCOL,
            action,
            install,
            staged,
            expect,
            editor_pid: std::process::id(),
            relaunch,
            log: dir.join("update.log"),
            result: dir.join("result.json"),
        };
        stage::hand_over(&version_dir, &instruction).map_err(|e| e.to_string())
    }

    /// On start: says how the last update or roll back went, and removes
    /// update files for versions up to this one.
    pub(super) fn update_results_on_start(&mut self) {
        let Some(dir) = updates_dir() else {
            return;
        };
        let result = dir.join("result.json");
        if let Some(outcome) = stage::read_outcome(&result) {
            let _ = std::fs::remove_file(&result);
            match (outcome.ok, outcome.action) {
                (true, Action::Install) => {
                    self.toasts
                        .success(format!("Updated to Axcenity {}", outcome.version));
                }
                (true, Action::RollBack) => {
                    self.toasts
                        .success(format!("Back on Axcenity {}", outcome.version));
                }
                (false, _) => {
                    let text = format!(
                        "The update didn't install: {}. This is still Axcenity {}; the details \
                         are in {}.",
                        outcome.message,
                        outcome.version,
                        dir.join("update.log").display()
                    );
                    self.output.warning(text);
                    self.toasts.warning("The update didn't install");
                }
            }
        }
        remove_old_updates(&dir, &Version::current());
    }

    /// The install part of Settings > Updates, under the found update.
    pub(super) fn install_controls(&mut self, ui: &mut egui::Ui, update: &Update) {
        let palette = theme::palette(ui.ctx());
        let kind = InstallKind::current();
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(egui::RichText::new(text).color(palette.text_weak)).wrap());
        };
        let Some(artifact) = stage::payload(&update.manifest, kind) else {
            note(ui, &package_hint(kind));
            return;
        };
        match &self.updates.install {
            Install::Idle | Install::Failed(_) => {
                if let Install::Failed(error) = &self.updates.install {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("The download failed: {error}"))
                                .color(palette.warning),
                        )
                        .wrap(),
                    );
                }
                let label = format!("Download and install ({})", megabytes(artifact.size));
                if ui
                    .add(widgets::button(
                        widgets::Role::Primary,
                        Some(icons::DOWNLOAD_SIMPLE),
                        &label,
                        &palette,
                    ))
                    .on_hover_text(
                        "Downloads in the background; you choose when to restart and install.",
                    )
                    .clicked()
                {
                    self.download_update();
                }
            }
            Install::Working {
                done,
                total,
                cancel,
                version,
                ..
            } => {
                let have = done.load(Ordering::Relaxed);
                let fraction = if *total == 0 {
                    0.0
                } else {
                    have as f32 / *total as f32
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(format!(
                            "Downloading {version}: {} of {}",
                            megabytes(have),
                            megabytes(*total)
                        ))
                        .desired_width(ui.available_width().min(320.0)),
                );
                if ui.button("Cancel").clicked() {
                    cancel.store(true, Ordering::Relaxed);
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
            Install::Ready(staged) => {
                let version = staged.version.clone();
                let restart = widgets::button(
                    widgets::Role::Primary,
                    Some(icons::ARROWS_CLOCKWISE),
                    "Restart and install",
                    &palette,
                );
                if ui
                    .add_enabled(!self.playing(), restart)
                    .on_disabled_hover_text("Stop Play first.")
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::Update(Action::Install));
                }
                note(
                    ui,
                    &format!(
                        "Axcenity closes (asking to save first), installs {version} and opens \
                         again. If the new version doesn't start, this one comes back."
                    ),
                );
            }
        }
    }

    /// Roll back, at the bottom of Settings > Updates, when there's an
    /// earlier version kept.
    pub(super) fn roll_back_controls(&mut self, ui: &mut egui::Ui) {
        if !can_roll_back(InstallKind::current()) {
            return;
        }
        let palette = theme::palette(ui.ctx());
        settings::section(ui, "Roll back");
        if ui
            .add_enabled(
                !self.playing(),
                widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::ARROW_COUNTER_CLOCKWISE),
                    "Roll back to the previous version",
                    &palette,
                ),
            )
            .on_hover_text(
                "Puts back the version this one replaced; this one is kept, so you can \
                 update again.",
            )
            .clicked()
        {
            self.request_and_go_ahead(ui.ctx(), Discard::Update(Action::RollBack));
        }
    }
}

/// How to update a copy that doesn't update itself.
fn package_hint(kind: InstallKind) -> String {
    match kind {
        InstallKind::Deb => {
            "Installed as a Debian package: download the new .deb from the release \
                             page and install it with `sudo apt install ./<file>.deb`."
                .to_owned()
        }
        InstallKind::Rpm => "Installed as an RPM package: download the new .rpm from the release \
                             page and install it with `sudo dnf install ./<file>.rpm`."
            .to_owned(),
        InstallKind::Pacman => "Installed as an Arch package: download the new package from the \
                                release page and install it with `sudo pacman -U <file>`."
            .to_owned(),
        InstallKind::Source => {
            "Built from source: pull the new version's tag and build it.".to_owned()
        }
        other => format!(
            "Installed as {}: download the new one from the release page.",
            other.label()
        ),
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Removes `Updates/<version>` folders for versions up to `installed`, and
/// the roll back's hand-over folder: they're done with.
fn remove_old_updates(dir: &Path, installed: &Version) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = name == "rollback" || Version::parse(&name).is_ok_and(|v| v <= *installed);
        if old && entry.path().is_dir() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};

    /// Serves one archive from memory.
    struct Memory(Vec<u8>);

    impl Fetch for Memory {
        fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read>, bool), String> {
            assert!(url.starts_with("https://github.com/"), "{url}");
            Ok((
                Box::new(io::Cursor::new(self.0[from as usize..].to_vec())),
                from > 0,
            ))
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("axcenity-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A Linux update archive of `files`, and a found update offering it.
    fn release(dir: &Path, files: &[(&str, &str)]) -> (Vec<u8>, Update) {
        let archive = dir.join("archive.tar.gz");
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gz);
        for (name, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o755);
            tar.append_data(&mut header, format!("top/{name}"), text.as_bytes())
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        let bytes = std::fs::read(&archive).unwrap();
        let hash = download::hash_of(&archive).unwrap();
        let mut manifest: axcenity_release::Manifest = serde_json::from_slice(include_bytes!(
            "../../../release/src/testdata/manifest.json"
        ))
        .unwrap();
        let file = "AxcenityEditor-0.2.0-alpha.2-linux-x86_64.tar.gz".to_owned();
        manifest.artifacts.insert(
            "linux-x86_64".to_owned(),
            [(
                "update".to_owned(),
                axcenity_release::publish::Artifact {
                    file,
                    sha256: hash,
                    size: bytes.len() as u64,
                },
            )]
            .into(),
        );
        let update = Update {
            candidate: updates::Candidate {
                version: manifest.version.clone(),
                tag: manifest.tag.clone(),
                page: String::new(),
            },
            manifest,
        };
        (bytes, update)
    }

    fn wait(install: &mut Install) {
        for _ in 0..2000 {
            install.poll();
            if !matches!(install, Install::Working { .. }) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the download never finished");
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn an_update_downloads_unpacks_and_is_ready_to_install() {
        let dir = scratch("ready");
        let (bytes, update) = release(
            &dir,
            &[
                ("install.json", r#"{"schema":1,"kind":"linux-tarball"}"#),
                (updater::EDITOR_EXE, "editor"),
                (updater::UPDATER_EXE, "updater"),
            ],
        );
        let mut install = Install::default();
        let updates = dir.join("Updates");
        install.start(
            &update,
            InstallKind::LinuxTarball,
            &updates,
            Box::new(Memory(bytes)),
        );
        wait(&mut install);
        let Install::Ready(staged) = &install else {
            panic!("not ready")
        };
        assert_eq!(staged.expect.version, "0.2.0-alpha.2");
        assert_eq!(staged.expect.commit, update.manifest.commit);
        assert!(staged.staged.join(updater::UPDATER_EXE).is_file());
        assert_eq!(staged.version_dir, updates.join("0.2.0-alpha.2"));
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn an_update_that_isnt_a_whole_install_fails_with_a_reason() {
        let dir = scratch("broken");
        let (bytes, update) = release(
            &dir,
            &[("install.json", r#"{"schema":1,"kind":"linux-tarball"}"#)],
        );
        let mut install = Install::default();
        install.start(
            &update,
            InstallKind::LinuxTarball,
            &dir.join("Updates"),
            Box::new(Memory(bytes)),
        );
        wait(&mut install);
        let Install::Failed(error) = &install else {
            panic!("didn't fail")
        };
        assert!(error.contains("axcenity-editor"), "{error}");
    }

    #[test]
    fn packages_and_source_builds_get_told_how_to_update() {
        let dir = scratch("package");
        let (bytes, update) = release(&dir, &[]);
        let mut install = Install::default();
        install.start(
            &update,
            InstallKind::Deb,
            &dir.join("Updates"),
            Box::new(Memory(bytes)),
        );
        assert!(matches!(install, Install::Failed(_)));
        assert!(package_hint(InstallKind::Deb).contains("apt install"));
        assert!(package_hint(InstallKind::Source).contains("from source"));
    }

    #[test]
    fn old_update_folders_are_removed_and_newer_ones_kept() {
        let dir = scratch("old");
        for name in [
            "0.1.0-alpha.3",
            "0.2.0-alpha.1",
            "0.2.0-alpha.2",
            "rollback",
            "notes",
        ] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        std::fs::write(dir.join("update.log"), "log").unwrap();
        remove_old_updates(&dir, &Version::parse("0.2.0-alpha.1").unwrap());
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["0.2.0-alpha.2", "notes", "update.log"]);
    }
}
