//! The open project's Git remote in the editor (Collaborate 2.0, P2 of
//! `docs/research/collaborate-persistent.md`): automatic checkpoints and
//! uploads, run off the frame by `axcenity_repo::sync::Worker`.
//!
//! A project is linked when its `collaborate.json` names a repository.
//! Then, when this computer is the one that commits (not in a session, or
//! hosting it; a member's folder is a copy and never commits):
//!
//! - every save marks the project changed, and changes are checkpointed
//!   and uploaded after the checkpoint interval (batched, not per save);
//! - **Sync now**, a member leaving, a handoff, the session ending and the
//!   editor closing checkpoint at once (closing waits up to 10 s);
//! - a failure retries with backoff, and the chip never says Synced
//!   unless the remote confirmed our commit.
//!
//! While hosting, the session's World is written to `world.json` before
//! each checkpoint, so what's uploaded is what everyone sees (the journal
//! still covers a crash in between). Outside a session only what the
//! person saved is checkpointed.
//!
//! What the remote's changes bring into the folder is read again by the
//! file watcher like any file another program changed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_project as project;
use axcenity_repo::checkpoint::Kind;
use axcenity_repo::sync::{Job, Outcome, Schedule, State, Worker};
use axcenity_repo::{Author, RepositoryManager};

use super::*;

/// How often the link in `collaborate.json` is read again.
const RELOAD_LINK: Duration = Duration::from_secs(5);
/// How long closing the editor waits for its last upload.
const CLOSING_WAIT: Duration = Duration::from_secs(10);

/// The open project's repository, if it has one.
#[derive(Default)]
pub(crate) struct ProjectRepo {
    /// The project folder the rest is about.
    folder: Option<PathBuf>,
    pub(crate) link: Option<project::RepositoryLink>,
    pub(crate) schedule: Schedule,
    worker: Option<Worker>,
    /// GitHub, the token and Git (made on first need; tests set their own).
    pub(crate) manager: Option<Arc<RepositoryManager>>,
    read_at: Option<Instant>,
    /// The last pass's outcome, for the panel.
    pub(crate) last: Option<Outcome>,
    author: Option<Author>,
}

impl ProjectRepo {
    /// The sync state for the chip and panel; `None` when not linked.
    pub(crate) fn state(&self) -> Option<&State> {
        self.link.as_ref().map(|_| self.schedule.state())
    }

    pub(crate) fn manager(&mut self) -> Arc<RepositoryManager> {
        self.manager
            .get_or_insert_with(|| Arc::new(RepositoryManager::system()))
            .clone()
    }
}

impl EditorApp {
    /// Each frame: notices the open project's link, starts a pass when one
    /// is due, and takes in finished ones.
    pub(crate) fn repo_frame(&mut self) {
        let folder = self.project.as_ref().map(|p| p.folder.clone());
        let now = Instant::now();
        if self.repo.folder != folder {
            self.repo = ProjectRepo {
                manager: self.repo.manager.take(),
                ..ProjectRepo::default()
            };
            self.repo.folder = folder.clone();
        }
        let Some(folder) = folder else {
            return;
        };
        if self
            .repo
            .read_at
            .is_none_or(|at| now.duration_since(at) >= RELOAD_LINK)
        {
            self.repo.read_at = Some(now);
            let link = project::load_collaborate_file(&folder)
                .ok()
                .and_then(|file| file.repository);
            if link.is_some() && self.repo.link.is_none() {
                // Newly linked (or just opened): see what's waiting.
                self.repo.schedule.ask(Kind::Auto, None);
            }
            self.repo.link = link;
        }
        if let Some(worker) = &self.repo.worker
            && let Some(outcome) = worker.poll()
        {
            self.repo_finished(outcome, now);
        }
        if self.repo.link.is_none() || !self.repo_commits_here() {
            return;
        }
        if let Some((kind, label)) = self.repo.schedule.due(now) {
            self.repo_run(&folder, kind, label);
        }
    }

    /// True when this computer's folder is the one that commits: no
    /// session, or hosting one. A member's folder is a copy.
    pub(crate) fn repo_commits_here(&self) -> bool {
        match &self.team {
            None => true,
            Some(team) => team.hosting().is_some(),
        }
    }

    /// The project changed on disk (a save, an import).
    pub(crate) fn repo_changed(&mut self) {
        if self.repo.link.is_some() {
            self.repo.schedule.changed(Instant::now());
        }
    }

    /// Asks for a checkpoint now (Sync now, someone leaving, a handoff).
    pub(crate) fn repo_checkpoint(&mut self, kind: Kind, label: Option<String>) {
        if self.repo.link.is_some() {
            self.repo.schedule.ask(kind, label);
        }
    }

    /// The Console's `checkpoint [label]` (Sync now, until the panel has
    /// a button): asks for a manual checkpoint and says where sync stands.
    pub(super) fn checkpoint_command(&mut self, label: &str) {
        let Some(state) = self.repo.state().cloned() else {
            self.output.info(
                "This project isn't linked to a repository, so there's nothing to upload \
                 checkpoints to.",
            );
            return;
        };
        if !self.repo_commits_here() {
            self.output.info(
                "In a session the host makes the checkpoints: ask them, or use the \
                 session's own checkpoints.",
            );
            return;
        }
        let label = (!label.is_empty()).then(|| label.to_owned());
        self.repo_checkpoint(Kind::Manual, label);
        self.output.info(format!(
            "Checkpoint asked for. Sync is {}: {}",
            state.word(),
            state.explain()
        ));
    }

    fn repo_run(&mut self, folder: &Path, kind: Kind, label: Option<String>) {
        let Some(link) = self.repo.link.clone() else {
            return;
        };
        // What everyone sees is what's uploaded.
        if self.team.as_ref().is_some_and(|t| t.hosting().is_some()) && self.unsaved() {
            self.save_world_for_checkpoint();
        }
        let participants = self.repo_participants();
        let author = self.repo_author(folder);
        let manager = self.repo.manager();
        if self.repo.worker.is_none() {
            self.repo.worker = Some(Worker::start(manager.git_with_token()));
        }
        let job = Job {
            dir: folder.to_owned(),
            branch: link.branch.clone(),
            kind,
            label,
            author,
            participants,
        };
        if let Some(worker) = &self.repo.worker {
            worker.run(job);
        }
    }

    fn repo_finished(&mut self, outcome: Outcome, now: Instant) {
        let before = self.repo.schedule.state().clone();
        self.repo.schedule.finished(&outcome, now);
        match &outcome.state {
            State::Synced if before != State::Synced => {}
            State::Offline if !matches!(before, State::Offline) => self.output.warning(
                "Collaborate: the project's repository can't be reached. Changes are kept on \
                 this computer and will upload when it can be.",
            ),
            State::Conflict(files) => self.output.warning(format!(
                "Collaborate: {} changed both here and in the repository; nothing was \
                 overwritten. Open the Collaborate panel to choose what to keep.",
                files.join(", ")
            )),
            State::Error(why) => self
                .output
                .warning(format!("Collaborate: uploading failed: {why}")),
            _ => {}
        }
        self.repo.last = Some(outcome);
    }

    /// Who took part in what's being checkpointed: everyone in the session,
    /// or just this person.
    fn repo_participants(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .team
            .as_ref()
            .and_then(|t| t.client())
            .map(|client| {
                client
                    .roster()
                    .iter()
                    .map(|c| c.profile.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            names.push(self.repo_author_name());
        }
        names.sort();
        names.dedup();
        names
    }

    fn repo_author_name(&self) -> String {
        team::load_own_identity()
            .map(|identity| identity.name)
            .unwrap_or_else(|_| "Axcenity".to_owned())
    }

    /// The commit author: the Collaborate name, with the email from the
    /// person's own Git settings when there is one.
    fn repo_author(&mut self, folder: &Path) -> Author {
        if let Some(author) = &self.repo.author {
            return author.clone();
        }
        let name = self.repo_author_name();
        let email = self
            .repo
            .manager()
            .git
            .config_value(folder, "user.email")
            .unwrap_or_else(|| "collaborate@axcenity.invalid".to_owned());
        let author = Author { name, email };
        self.repo.author = Some(author.clone());
        author
    }

    /// The editor is closing: a last checkpoint, waiting a little for it.
    pub(crate) fn repo_close(&mut self) {
        let Some(folder) = self.repo.folder.clone() else {
            return;
        };
        if self.repo.link.is_none() || !self.repo_commits_here() {
            return;
        }
        // A pass already running finishes first.
        if let Some(worker) = &self.repo.worker
            && self.repo.schedule.state() == &State::Syncing
            && let Some(outcome) = worker.wait(CLOSING_WAIT)
        {
            self.repo.schedule.finished(&outcome, Instant::now());
        }
        let kind = if self.team.is_some() {
            Kind::Ended
        } else {
            Kind::Closing
        };
        self.repo.schedule.ask(kind, None);
        if let Some((kind, label)) = self.repo.schedule.due(Instant::now()) {
            self.repo_run(&folder, kind, label);
            if let Some(worker) = &self.repo.worker {
                match worker.wait(CLOSING_WAIT) {
                    Some(outcome) => self.repo_finished(outcome, Instant::now()),
                    None => log::warn!(
                        "Collaborate: the last upload didn't finish before closing; it's kept \
                         on this computer and uploads next time"
                    ),
                }
            }
        }
    }
}
