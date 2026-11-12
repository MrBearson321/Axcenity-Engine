//! AutoRecover: every few minutes (Settings > Saving), unsaved work is
//! written to the project's `.axcenity/autorecover/` on a thread
//! (`axcenity_project::write_recovery`), so an editor crash or a power cut
//! loses at most those minutes. Saving, "Don't Save", or getting back to no
//! unsaved changes removes it. Opening a project that has one that no
//! running editor touched lately asks "Recover unsaved work?".
//!
//! What's kept: the world (Instances, with the open level), script and
//! stylesheet tabs, and block scripts. Other documents (UI graphs, behavior
//! trees, dialogues, data tables, materials) and terrain sculpting are not
//! in a snapshot yet. A Collaborate session keeps its own journal instead.

use std::time::{Duration, Instant};

use axcenity_project::{RecoveryInfo, RecoverySnapshot};

use super::*;

/// The editor setting's key, and the minutes between snapshots it starts at.
pub(super) const AUTORECOVER_KEY: &str = "autorecover minutes";
pub(super) const DEFAULT_MINUTES: u32 = 2;
/// The choices Settings offers (0 is off).
pub(super) const MINUTE_CHOICES: [u32; 5] = [0, 1, 2, 5, 10];
/// How often an open project's snapshot is marked as still in use
/// (well inside `axcenity_project::RECOVERY_STALE_AFTER`).
const TOUCH_EVERY: Duration = Duration::from_secs(20);

/// AutoRecover's state in the editor.
pub(super) struct AutoRecover {
    /// Minutes between snapshots; 0 turns AutoRecover off.
    pub minutes: u32,
    /// When the last snapshot was taken (or the project opened).
    pub(super) last: Option<Instant>,
    last_touch: Option<Instant>,
    /// What was unsaved at the last snapshot, so the same work isn't
    /// written again.
    pub(super) written: Option<u64>,
    /// The snapshot being written.
    pub(super) writing: Option<std::thread::JoinHandle<Result<(), String>>>,
    /// A left-behind snapshot found on open, until it's answered. Nothing
    /// is written over it meanwhile.
    pub offer: Option<RecoveryInfo>,
}

impl Default for AutoRecover {
    fn default() -> Self {
        Self {
            minutes: DEFAULT_MINUTES,
            last: None,
            last_touch: None,
            written: None,
            writing: None,
            offer: None,
        }
    }
}

impl AutoRecover {
    /// The setting as saved: the minutes.
    pub(super) fn to_text(&self) -> String {
        self.minutes.to_string()
    }

    /// The setting from [`Self::to_text`]; anything else keeps the default.
    pub(super) fn read(&mut self, text: &str) {
        if let Ok(minutes) = text.trim().parse::<u32>() {
            self.minutes = minutes.min(60);
        }
    }

    fn interval(&self) -> Option<Duration> {
        (self.minutes > 0).then(|| Duration::from_secs(u64::from(self.minutes) * 60))
    }

    /// Forgets the last project's timing (a project just opened).
    fn restart(&mut self) {
        self.last = Some(Instant::now());
        self.last_touch = None;
        self.written = None;
    }
}

/// A label for a choice of minutes.
pub(super) fn minutes_label(minutes: u32) -> String {
    match minutes {
        0 => "Off".to_owned(),
        1 => "Every minute".to_owned(),
        n => format!("Every {n} minutes"),
    }
}

impl EditorApp {
    /// After a project opens: looks for work a crashed editor left behind,
    /// and starts timing snapshots from now.
    pub(super) fn autorecover_opened(&mut self) {
        self.autorecover.restart();
        self.autorecover.offer = None;
        if self.team.is_some() {
            return;
        }
        if let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) {
            self.autorecover.offer = axcenity_project::left_behind_recovery(&folder);
        }
    }

    /// The snapshot is no longer needed: everything was saved, or the
    /// unsaved work was let go. A finished write is waited for first, so it
    /// can't put the snapshot back afterwards.
    pub(super) fn autorecover_done(&mut self) {
        if let Some(writing) = self.autorecover.writing.take() {
            let _ = writing.join();
        }
        self.autorecover.written = None;
        if self.autorecover.offer.is_some() {
            return;
        }
        if let Some(folder) = self.project.as_ref().map(|p| p.folder.clone())
            && let Err(err) = axcenity_project::discard_recovery(&folder)
        {
            self.output
                .warning(format!("Could not remove the AutoRecover copy: {err}"));
        }
    }

    /// Each frame: takes a snapshot when one is due and something unsaved
    /// changed since the last, and marks the snapshot as in use.
    pub(super) fn autorecover_frame(&mut self) {
        if let Some(writing) = &self.autorecover.writing
            && writing.is_finished()
        {
            let writing = self.autorecover.writing.take().expect("checked above");
            match writing.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => self.output.warning(format!(
                    "AutoRecover couldn't keep a copy of your work: {err}"
                )),
                Err(_) => self
                    .output
                    .warning("AutoRecover stopped while keeping a copy of your work."),
            }
        }
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.team.is_some() || self.autorecover.offer.is_some() {
            return;
        }
        let now = Instant::now();
        if self
            .autorecover
            .last_touch
            .is_none_or(|at| now.duration_since(at) >= TOUCH_EVERY)
        {
            self.autorecover.last_touch = Some(now);
            axcenity_project::touch_recovery(&folder);
        }
        let Some(interval) = self.autorecover.interval() else {
            return;
        };
        let due = self
            .autorecover
            .last
            .is_none_or(|at| now.duration_since(at) >= interval);
        if !due || self.autorecover.writing.is_some() {
            return;
        }
        self.autorecover.last = Some(now);
        self.write_autorecover(&folder);
    }

    /// Writes a snapshot of what's unsaved now on a thread, if it changed
    /// since the last; with nothing unsaved, removes the snapshot.
    pub(super) fn write_autorecover(&mut self, folder: &Path) {
        let snapshot = self.recovery_snapshot();
        let fingerprint = fingerprint(&self.history.revision(), &snapshot);
        if snapshot.is_empty() {
            if self.autorecover.written.is_some()
                || axcenity_project::recovery_info(folder).is_some()
            {
                self.autorecover_done();
            }
            return;
        }
        if self.autorecover.written == Some(fingerprint) {
            return;
        }
        self.autorecover.written = Some(fingerprint);
        let folder = folder.to_owned();
        self.autorecover.writing = Some(std::thread::spawn(move || {
            axcenity_project::write_recovery(&folder, &snapshot).map_err(|err| files::chain(&err))
        }));
    }

    /// What's unsaved now, as a snapshot: the world if it has changes (a
    /// copy, written on the thread), and the text of unsaved script tabs
    /// and block scripts.
    fn recovery_snapshot(&self) -> RecoverySnapshot {
        let Some(project) = &self.project else {
            return RecoverySnapshot::default();
        };
        let world = (!self.history.is_saved()).then(|| self.world.clone());
        let mut files: Vec<(String, String)> = self
            .tabs
            .unsaved_paths()
            .into_iter()
            .filter_map(|path| {
                let text = self.tabs.get(&path)?.text.clone();
                Some((path, text))
            })
            .collect();
        for document in &project.block_scripts {
            if document.script == document.saved
                || files.iter().any(|(path, _)| *path == document.path)
            {
                continue;
            }
            if let Ok(text) = serde_json::to_string_pretty(&document.script) {
                files.push((document.path.clone(), text));
            }
        }
        RecoverySnapshot {
            level: project.level.clone(),
            world,
            files,
        }
    }

    /// "Recover unsaved work?" while a left-behind snapshot waits for an answer.
    pub(super) fn autorecover_prompt(&mut self, ctx: &egui::Context) {
        let Some(info) = self.autorecover.offer.clone() else {
            return;
        };
        let mut items = Vec::new();
        if info.world {
            let world = match &info.level {
                Some(level) => format!("The world (level \"{level}\")"),
                None => "The world".to_owned(),
            };
            items.push((icons::GLOBE, world));
        }
        for path in &info.files {
            items.push((icons::FILE_CODE, path.clone()));
        }
        let message = format!(
            "The editor stopped without saving this project: it may have crashed, or the \
             computer lost power. AutoRecover kept a copy of the unsaved work {}. Recover \
             opens it as unsaved changes (save to keep them); Discard deletes the copy and \
             keeps the project as last saved.",
            ago(info.saved_at)
        );
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "autorecover",
                icon: icons::CLOCK_COUNTER_CLOCKWISE,
                tone: dialog::Tone::Warning,
                title: "Recover unsaved work?",
                message: &message,
                items,
            },
            &[
                dialog::Button::new(icons::TRASH, "Discard", dialog::Kind::Danger),
                dialog::Button::new(
                    icons::CLOCK_COUNTER_CLOCKWISE,
                    "Recover",
                    dialog::Kind::Main,
                ),
            ],
        );
        // Escape or a click outside asks again: the copy is never lost
        // without choosing Discard.
        match outcome {
            Some(dialog::Outcome::Picked(1)) => self.recover_unsaved_work(),
            Some(dialog::Outcome::Picked(0)) => self.discard_unsaved_work(),
            _ => {}
        }
    }

    /// Deletes a left-behind snapshot: the project stays as last saved.
    pub(super) fn discard_unsaved_work(&mut self) {
        self.autorecover.offer = None;
        self.autorecover_done();
        self.output
            .info("Discarded the AutoRecover copy; the project is as last saved.");
    }

    /// Opens the project again with the snapshot's work in it, as unsaved
    /// changes. The snapshot stays until the work is saved or let go.
    pub(super) fn recover_unsaved_work(&mut self) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        self.autorecover.offer = None;
        let recovered = match axcenity_project::load_recovered(&folder) {
            Ok(Some(recovered)) => recovered,
            Ok(None) => return,
            Err(err) => {
                self.output.error(format!(
                    "Could not read the AutoRecover copy: {:#}. It's kept in `{}`.",
                    anyhow::Error::new(err),
                    axcenity_project::RECOVERY_FOLDER
                ));
                return;
            }
        };
        let level = recovered.info.level.clone();
        let assets = Assets::new(&folder);
        self.open_loaded(recovered.project, assets, level);
        // Opening looked again; the answer is already given.
        self.autorecover.offer = None;
        if recovered.info.world {
            self.history.mark_unsaved();
        }
        let mut documents = 0;
        for (path, text) in recovered.files {
            documents += 1;
            if self.recover_block_script(&path, &text) {
                continue;
            }
            let saved = std::fs::read_to_string(folder.join(&path)).unwrap_or_default();
            self.tabs.open(&path, &saved, None);
            if let Some(tab) = self.tabs.get_mut(&path) {
                tab.text = text;
            }
        }
        let what = match (recovered.info.world, documents) {
            (true, 0) => "the world".to_owned(),
            (true, n) => format!("the world and {n} document(s)"),
            (false, n) => format!("{n} document(s)"),
        };
        self.output.warning(format!(
            "Recovered {what} from AutoRecover. Save to keep it; closing without saving \
             drops it."
        ));
        self.toasts
            .warning("Recovered your unsaved work. Save to keep it.");
    }

    /// Puts recovered blocks back into the block script at `path`, if it's
    /// one. Returns whether it was.
    fn recover_block_script(&mut self, path: &str, text: &str) -> bool {
        let Some(project) = &mut self.project else {
            return false;
        };
        let Some(document) = project
            .block_scripts
            .iter_mut()
            .find(|document| document.path == path)
        else {
            return false;
        };
        match serde_json::from_str(text) {
            Ok(script) => document.script = script,
            Err(err) => {
                self.output.warning(format!(
                    "Could not read the recovered blocks of `{path}`: {err}"
                ));
                return true;
            }
        }
        self.update_block_lua(path);
        true
    }
}

/// Stands for what's unsaved: the world's undo position and each
/// document's text.
fn fingerprint(revision: &(u64, bool), snapshot: &RecoverySnapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    revision.hash(&mut hasher);
    snapshot.world.is_some().hash(&mut hasher);
    snapshot.level.hash(&mut hasher);
    snapshot.files.hash(&mut hasher);
    hasher.finish()
}

/// "3 minutes ago" for a time in seconds since 1970.
fn ago(saved_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let seconds = now.saturating_sub(saved_at);
    match seconds {
        0..60 => "less than a minute ago".to_owned(),
        60..3600 => format!("about {} minute(s) ago", seconds / 60),
        3600..86_400 => format!("about {} hour(s) ago", seconds / 3600),
        _ => format!("about {} day(s) ago", seconds / 86_400),
    }
}
