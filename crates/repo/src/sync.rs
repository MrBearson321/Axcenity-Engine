//! Automatic synchronization with the remote (P2 of
//! `docs/research/collaborate-persistent.md`, the brief's
//! `ProjectSyncManager`).
//!
//! Three parts:
//!
//! - [`sync_once`]: one safe pass. Checkpoint what changed, fetch, bring
//!   the remote's changes in when they merge cleanly, push (never forced),
//!   and confirm the remote's branch is at our commit. Conflicts stop it
//!   with nothing overwritten; no network leaves the checkpoint local.
//! - [`Schedule`]: when to run a pass (no I/O, so it's tested with a fake
//!   clock): changes wait for the checkpoint interval and are batched;
//!   a member leaving, a handoff, the session ending, the editor closing
//!   and **Sync now** run one at once; failures retry with backoff.
//! - [`Worker`]: runs passes on a thread of their own, so the editor never
//!   waits for the network.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime};

use crate::checkpoint::{self, Kind};
use crate::git::{Author, Git, GitError, MergeCheck, Pushed};
use crate::repository::REMOTE;

/// How often waiting changes are checkpointed by default.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// The first wait before trying again after a failure.
pub const FIRST_RETRY: Duration = Duration::from_secs(30);
/// The longest wait between tries.
pub const LONGEST_RETRY: Duration = Duration::from_secs(10 * 60);
/// How many times a push that lost a race goes back to fetch.
const PUSH_TRIES: usize = 3;

/// The sync indicator, in the brief's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Local and remote match (the remote's commit was checked).
    Synced,
    /// A pass is running.
    Syncing,
    /// Changes haven't been uploaded yet.
    Pending,
    /// The remote can't be reached; changes are kept here and retried.
    Offline,
    /// These files need someone to choose; nothing was overwritten.
    Conflict(Vec<String>),
    /// Something else failed (sign-in, a refused push), in words.
    Error(String),
}

impl State {
    /// A short word for the chip.
    pub fn word(&self) -> &'static str {
        match self {
            State::Synced => "Synced",
            State::Syncing => "Syncing",
            State::Pending => "Pending",
            State::Offline => "Offline",
            State::Conflict(_) => "Conflict",
            State::Error(_) => "Error",
        }
    }

    /// A sentence for the panel, without Git words.
    pub fn explain(&self) -> String {
        match self {
            State::Synced => "Everything is uploaded.".into(),
            State::Syncing => "Uploading changes…".into(),
            State::Pending => "Some changes haven't been uploaded yet.".into(),
            State::Offline => {
                "Saved on this computer, not uploaded yet: the repository can't be reached. Axcenity will try again.".into()
            }
            State::Conflict(files) => format!(
                "{} changed both here and in the repository. Nothing was overwritten: choose which version to keep.",
                match files.len() {
                    1 => format!("`{}`", files[0]),
                    n => format!("{n} files"),
                }
            ),
            State::Error(why) => format!("Uploading failed: {why}"),
        }
    }
}

/// What one pass needs.
#[derive(Clone, Debug)]
pub struct Job {
    pub dir: PathBuf,
    pub branch: String,
    pub kind: Kind,
    /// A manual checkpoint's name.
    pub label: Option<String>,
    pub author: Author,
    pub participants: Vec<String>,
}

/// What one pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub state: State,
    /// The checkpoint it made, if anything had changed.
    pub committed: Option<String>,
    /// The remote's changes were brought into the folder: files on disk
    /// changed, so the editor should read them again.
    pub pulled: bool,
    /// The commit the remote's branch was confirmed at.
    pub remote: Option<String>,
}

/// One pass; see the module notes. Never forces, never discards.
pub fn sync_once(git: &Git, job: &Job) -> Outcome {
    let mut outcome = Outcome {
        state: State::Pending,
        committed: None,
        pulled: false,
        remote: None,
    };
    match pass(git, job, &mut outcome) {
        Ok(state) => outcome.state = state,
        Err(error) if error.is_offline() => outcome.state = State::Offline,
        Err(error) => outcome.state = State::Error(error.to_string()),
    }
    outcome
}

fn pass(git: &Git, job: &Job, outcome: &mut Outcome) -> Result<State, GitError> {
    let dir = job.dir.as_path();
    recover_interrupted(git, dir);
    // 1. Checkpoint what changed (nothing if nothing did).
    let changed = git.changes(dir)?.len();
    if changed > 0 {
        let participants: Vec<&str> = job.participants.iter().map(String::as_str).collect();
        let message = checkpoint::message(job.kind, job.label.as_deref(), &participants, changed);
        outcome.committed = git.commit_all(dir, &message, &job.author)?;
    }
    if git.head(dir).is_none() {
        return Ok(State::Synced);
    }
    let theirs = format!("{REMOTE}/{}", job.branch);
    for _ in 0..PUSH_TRIES {
        // 2. What has the remote got?
        let remote_has_branch = match git.fetch(dir, REMOTE, &job.branch) {
            Ok(()) => true,
            Err(GitError::Failed(text)) if text.contains("couldn't find remote ref") => false,
            Err(error) => return Err(error),
        };
        // 3. Bring its changes in when that's safe.
        if remote_has_branch && !git.is_ancestor(dir, &theirs, "HEAD") {
            let ours = git.head(dir).unwrap_or_default();
            match git.merge_check(dir, &ours, &theirs)? {
                MergeCheck::Conflicts(files) => return Ok(State::Conflict(files)),
                MergeCheck::Clean => {
                    let message = checkpoint::message(Kind::Merge, None, &[&job.author.name], 0);
                    git.merge(dir, &theirs, &message, &job.author)?;
                    outcome.pulled = true;
                }
            }
        }
        // 4. Upload, never forced.
        match git.push(dir, REMOTE, &job.branch)? {
            Pushed::Behind => continue,
            Pushed::Done => {}
        }
        // 5. Only the remote's own answer counts as uploaded.
        let head = git.head(dir);
        let remote = git.remote_branch_sha(dir, REMOTE, &job.branch)?;
        outcome.remote = remote.clone();
        return Ok(if remote.is_some() && remote == head {
            State::Synced
        } else {
            State::Pending
        });
    }
    Err(GitError::Failed(
        "the repository kept changing while uploading; trying again later".into(),
    ))
}

/// A merge left half-made by an editor that stopped during a pass is
/// undone (the local checkpoint before it stays), so the pass can start
/// again from a clean state.
pub fn recover_interrupted(git: &Git, dir: &Path) -> bool {
    let Some(git_dir) = crate::repository::git_dir(git, dir) else {
        return false;
    };
    if git_dir.join("MERGE_HEAD").exists() {
        log::warn!(
            "Collaborate: finishing a sync that was interrupted (undoing its half-made merge)"
        );
        let _ = git.run(dir, &["merge", "--abort"]);
        return true;
    }
    // A commit interrupted before it finished leaves only an index lock.
    let lock = git_dir.join("index.lock");
    if lock.exists()
        && std::fs::metadata(&lock)
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(120))
    {
        let _ = std::fs::remove_file(lock);
        return true;
    }
    false
}

/// When to run a pass. Feed it what happens; ask [`Schedule::due`].
#[derive(Clone, Debug)]
pub struct Schedule {
    pub interval: Duration,
    /// When the oldest change not yet checkpointed was noticed.
    dirty_since: Option<Instant>,
    /// A checkpoint someone or something asked for, to run at once.
    asked: Option<(Kind, Option<String>)>,
    running: bool,
    /// After a failure: not before this.
    retry_at: Option<Instant>,
    backoff: Duration,
    state: State,
    /// When the remote last confirmed it had everything.
    pub last_synced: Option<SystemTime>,
    pub last_remote: Option<String>,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule::new(DEFAULT_INTERVAL)
    }
}

impl Schedule {
    pub fn new(interval: Duration) -> Schedule {
        Schedule {
            interval,
            dirty_since: None,
            asked: None,
            running: false,
            retry_at: None,
            backoff: FIRST_RETRY,
            state: State::Synced,
            last_synced: None,
            last_remote: None,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// The project changed on disk (a save, an import, a file arriving).
    pub fn changed(&mut self, now: Instant) {
        self.dirty_since.get_or_insert(now);
        if self.state == State::Synced {
            self.state = State::Pending;
        }
    }

    /// Asks for a checkpoint now (`Kind::Auto` only waits for a running
    /// pass). A more important kind replaces a waiting one.
    pub fn ask(&mut self, kind: Kind, label: Option<String>) {
        let replace = match &self.asked {
            None => true,
            Some((waiting, _)) => *waiting == Kind::Auto || kind != Kind::Auto,
        };
        if replace {
            self.asked = Some((kind, label));
        }
        // Someone asking means now, even while waiting to retry.
        self.retry_at = None;
    }

    /// The network came back: try again at once.
    pub fn online_again(&mut self) {
        if matches!(self.state, State::Offline) {
            self.retry_at = None;
            self.asked.get_or_insert((Kind::Auto, None));
        }
    }

    /// The pass to start now, if any.
    pub fn due(&mut self, now: Instant) -> Option<(Kind, Option<String>)> {
        if self.running || matches!(self.state, State::Conflict(_)) && self.asked.is_none() {
            return None;
        }
        if let Some(at) = self.retry_at
            && now < at
        {
            return None;
        }
        let due = if let Some(asked) = self.asked.take() {
            Some(asked)
        } else if self
            .dirty_since
            .is_some_and(|since| now.duration_since(since) >= self.interval)
        {
            Some((Kind::Auto, None))
        } else if self.retry_at.is_some() {
            // A failed pass's changes, tried again.
            Some((Kind::Auto, None))
        } else {
            None
        };
        if due.is_some() {
            self.running = true;
            self.dirty_since = None;
            self.state = State::Syncing;
        }
        due
    }

    /// A pass finished.
    pub fn finished(&mut self, outcome: &Outcome, now: Instant) {
        self.running = false;
        self.state = outcome.state.clone();
        match &outcome.state {
            State::Synced => {
                self.retry_at = None;
                self.backoff = FIRST_RETRY;
                self.last_synced = Some(SystemTime::now());
                self.last_remote = outcome.remote.clone();
                if self.dirty_since.is_some() {
                    // Changed again while the pass ran.
                    self.state = State::Pending;
                }
            }
            State::Offline | State::Error(_) | State::Pending => {
                self.retry_at = Some(now + self.backoff);
                self.backoff = (self.backoff * 2).min(LONGEST_RETRY);
            }
            // Waits for someone to choose; a new ask runs again.
            State::Conflict(_) => self.retry_at = None,
            State::Syncing => {}
        }
    }

    /// True while changes exist that the remote hasn't confirmed.
    pub fn has_unuploaded(&self) -> bool {
        !matches!(self.state, State::Synced) || self.dirty_since.is_some()
    }
}

/// Runs passes on a thread of its own.
pub struct Worker {
    jobs: Sender<Job>,
    results: Receiver<Outcome>,
}

impl Worker {
    pub fn start(git: Git) -> Worker {
        let (jobs, job_rx) = channel::<Job>();
        let (result_tx, results) = channel();
        std::thread::Builder::new()
            .name("collaborate-sync".into())
            .spawn(move || {
                for job in job_rx {
                    let outcome = sync_once(&git, &job);
                    if result_tx.send(outcome).is_err() {
                        break;
                    }
                }
            })
            .expect("the sync thread starts");
        Worker { jobs, results }
    }

    pub fn run(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    /// A finished pass, if one is ready.
    pub fn poll(&self) -> Option<Outcome> {
        self.results.try_recv().ok()
    }

    /// Waits up to `timeout` for a pass (leaving or quitting).
    pub fn wait(&self, timeout: Duration) -> Option<Outcome> {
        self.results.recv_timeout(timeout).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{self, Linked, RemoteSpec};
    use crate::testing::{author, bare_remote, git, temp, write};

    fn linked_copy(git: &Git, url: &str, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = temp(name);
        for (path, text) in files {
            write(&dir, path, text);
        }
        let spec = RemoteSpec {
            url: url.to_owned(),
            branch: Some("main".into()),
            replace_remote: false,
        };
        let (_, linked) = repository::link(git, &dir, &spec, &author(name)).unwrap();
        if let Linked::Adopted { differences } = linked
            && !differences.is_empty()
        {
            repository::take_remote_version(git, &dir, "main").unwrap();
        }
        dir
    }

    fn job(dir: &Path, kind: Kind, who: &str) -> Job {
        Job {
            dir: dir.to_owned(),
            branch: "main".into(),
            kind,
            label: None,
            author: author(who),
            participants: vec![who.to_owned()],
        }
    }

    #[test]
    fn changes_are_checkpointed_uploaded_and_confirmed() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("world.json", "{}")]);
        write(&a, "world.json", "{\"parts\":1}");
        let outcome = sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        assert_eq!(outcome.state, State::Synced);
        let commit = outcome.committed.clone().unwrap();
        assert_eq!(outcome.remote, Some(commit.clone()));
        let entries = checkpoint::history(&git, &a, 5).unwrap();
        assert_eq!(entries[0].kind, Some(Kind::Auto));
        assert_eq!(entries[0].commit.author, "Paul");
        // Nothing changed: no empty checkpoint.
        let again = sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        assert_eq!(again.state, State::Synced);
        assert_eq!(again.committed, None);
        assert_eq!(
            checkpoint::history(&git, &a, 5).unwrap().len(),
            entries.len()
        );
    }

    #[test]
    fn someone_elses_upload_is_brought_in_before_uploading() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("a.txt", "a"), ("b.txt", "b")]);
        let b = linked_copy(&git, &url, "Sam", &[]);
        // Separate files on each side.
        write(&a, "a.txt", "paul's");
        write(&b, "b.txt", "sam's");
        assert_eq!(
            sync_once(&git, &job(&a, Kind::Auto, "Paul")).state,
            State::Synced
        );
        let outcome = sync_once(&git, &job(&b, Kind::Auto, "Sam"));
        assert_eq!(outcome.state, State::Synced);
        assert!(outcome.pulled);
        assert_eq!(std::fs::read_to_string(b.join("a.txt")).unwrap(), "paul's");
        assert_eq!(std::fs::read_to_string(b.join("b.txt")).unwrap(), "sam's");
        // And back to Paul, by fast-forward.
        let back = sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        assert_eq!(back.state, State::Synced);
        assert!(back.pulled);
        assert_eq!(std::fs::read_to_string(a.join("b.txt")).unwrap(), "sam's");
    }

    #[test]
    fn a_conflict_stops_with_both_versions_kept() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("world.json", "{\n\"x\": 0\n}\n")]);
        let b = linked_copy(&git, &url, "Sam", &[]);
        write(&a, "world.json", "{\n\"x\": 1\n}\n");
        write(&b, "world.json", "{\n\"x\": 2\n}\n");
        assert_eq!(
            sync_once(&git, &job(&a, Kind::Auto, "Paul")).state,
            State::Synced
        );
        let outcome = sync_once(&git, &job(&b, Kind::Auto, "Sam"));
        assert_eq!(outcome.state, State::Conflict(vec!["world.json".into()]));
        // Sam's file is untouched and Sam's checkpoint is kept locally.
        assert_eq!(
            std::fs::read_to_string(b.join("world.json")).unwrap(),
            "{\n\"x\": 2\n}\n"
        );
        assert!(outcome.committed.is_some());
        assert!(!crate::repository::status(&git, &b, "main").merge_in_progress);
        // The remote still has Paul's.
        let check = temp("check").join("c");
        git.clone_repository(&url, &check, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(check.join("world.json")).unwrap(),
            "{\n\"x\": 1\n}\n"
        );
    }

    #[test]
    fn offline_keeps_the_checkpoint_and_uploads_it_later() {
        let git = git();
        let (remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("a.txt", "a")]);
        // The remote goes away.
        let away = remote.with_extension("away");
        std::fs::rename(&remote, &away).unwrap();
        write(&a, "a.txt", "changed offline");
        let outcome = sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        assert!(
            matches!(outcome.state, State::Offline | State::Error(_)),
            "{:?}",
            outcome.state
        );
        assert_ne!(outcome.state, State::Synced);
        assert!(outcome.committed.is_some(), "kept as a local checkpoint");
        // Back again: uploaded.
        std::fs::rename(&away, &remote).unwrap();
        let outcome = sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        assert_eq!(outcome.state, State::Synced);
        let check = temp("check").join("c");
        git.clone_repository(&url, &check, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(check.join("a.txt")).unwrap(),
            "changed offline"
        );
    }

    #[test]
    fn a_half_made_merge_from_a_crash_is_undone_first() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("a.txt", "1\n")]);
        let b = linked_copy(&git, &url, "Sam", &[]);
        write(&a, "a.txt", "paul\n");
        sync_once(&git, &job(&a, Kind::Auto, "Paul"));
        write(&b, "a.txt", "sam\n");
        git.commit_all(&b, "sam", &author("Sam")).unwrap();
        git.fetch(&b, REMOTE, "main").unwrap();
        // A merge the editor started and never finished.
        let _ = git.try_run(
            &b,
            &[
                "-c",
                "user.name=Sam",
                "-c",
                "user.email=sam@example.com",
                "merge",
                "--no-edit",
                "origin/main",
            ],
            None,
        );
        assert!(crate::repository::status(&git, &b, "main").merge_in_progress);
        let outcome = sync_once(&git, &job(&b, Kind::Auto, "Sam"));
        assert_eq!(outcome.state, State::Conflict(vec!["a.txt".into()]));
        assert!(!crate::repository::status(&git, &b, "main").merge_in_progress);
        assert_eq!(std::fs::read_to_string(b.join("a.txt")).unwrap(), "sam\n");
    }

    #[test]
    fn the_schedule_batches_changes_until_the_interval() {
        let start = Instant::now();
        let mut schedule = Schedule::new(Duration::from_secs(600));
        assert_eq!(schedule.due(start), None);
        schedule.changed(start);
        schedule.changed(start + Duration::from_secs(100));
        assert_eq!(schedule.state(), &State::Pending);
        assert_eq!(schedule.due(start + Duration::from_secs(599)), None);
        assert_eq!(
            schedule.due(start + Duration::from_secs(600)),
            Some((Kind::Auto, None))
        );
        assert_eq!(schedule.state(), &State::Syncing);
        // One at a time.
        schedule.ask(Kind::Manual, None);
        assert_eq!(schedule.due(start + Duration::from_secs(601)), None);
        let synced = Outcome {
            state: State::Synced,
            committed: None,
            pulled: false,
            remote: Some("abc".into()),
        };
        schedule.finished(&synced, start + Duration::from_secs(602));
        assert_eq!(
            schedule.due(start + Duration::from_secs(602)),
            Some((Kind::Manual, None))
        );
    }

    #[test]
    fn leaving_and_handoffs_go_at_once_and_win_over_the_timer() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.changed(now);
        schedule.ask(Kind::Auto, None);
        schedule.ask(Kind::Handoff, None);
        schedule.ask(Kind::Auto, None);
        assert_eq!(schedule.due(now), Some((Kind::Handoff, None)));
    }

    #[test]
    fn failures_retry_with_backoff_and_never_say_synced() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.ask(Kind::Manual, None);
        schedule.due(now).unwrap();
        let offline = Outcome {
            state: State::Offline,
            committed: Some("c".into()),
            pulled: false,
            remote: None,
        };
        schedule.finished(&offline, now);
        assert_eq!(schedule.state(), &State::Offline);
        assert!(schedule.has_unuploaded());
        assert_eq!(schedule.due(now + Duration::from_secs(29)), None);
        assert!(schedule.due(now + FIRST_RETRY).is_some());
        schedule.finished(&offline, now + FIRST_RETRY);
        // Twice as long the next time.
        assert_eq!(schedule.due(now + FIRST_RETRY * 2), None);
        assert!(schedule.due(now + FIRST_RETRY * 3).is_some());
        schedule.finished(&offline, now + FIRST_RETRY * 3);
        // The network coming back tries at once.
        schedule.online_again();
        assert!(schedule.due(now + FIRST_RETRY * 3).is_some());
    }

    #[test]
    fn a_conflict_waits_for_someone_to_choose() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.ask(Kind::Manual, None);
        schedule.due(now);
        schedule.finished(
            &Outcome {
                state: State::Conflict(vec!["world.json".into()]),
                committed: None,
                pulled: false,
                remote: None,
            },
            now,
        );
        schedule.changed(now);
        assert_eq!(schedule.due(now + Duration::from_secs(3600)), None);
        schedule.ask(Kind::Manual, None);
        assert!(schedule.due(now + Duration::from_secs(3600)).is_some());
    }

    #[test]
    fn the_worker_runs_passes_off_the_callers_thread() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let a = linked_copy(&git, &url, "Paul", &[("a.txt", "a")]);
        write(&a, "a.txt", "b");
        let worker = Worker::start(git.clone());
        worker.run(job(&a, Kind::Closing, "Paul"));
        let outcome = worker.wait(Duration::from_secs(60)).unwrap();
        assert_eq!(outcome.state, State::Synced);
        assert_eq!(
            checkpoint::history(&git, &a, 1).unwrap()[0].kind,
            Some(Kind::Closing)
        );
    }
}
