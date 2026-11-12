//! Checkpoints: Axcenity's own commits (P2 of
//! `docs/research/collaborate-persistent.md`, the brief's
//! `CheckpointManager`).
//!
//! A checkpoint is an ordinary Git commit whose message starts with
//! `[Collaborate]` and ends with an `Axcenity-Checkpoint: <kind>` trailer,
//! so the history tells Axcenity's automatic and manual checkpoints from
//! people's own commits. Restoring one never rewrites history: the old
//! files come back as a new checkpoint on top, which can be restored away
//! in turn.

use std::path::Path;

use crate::git::{Author, Commit, Git, GitError};
use crate::repository::CHECKPOINT_TRAILER;

/// Why a checkpoint was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Changes were waiting for the checkpoint interval.
    Auto,
    /// A member left the session.
    Left,
    /// The session host handed the session to someone else.
    Handoff,
    /// The session ended.
    Ended,
    /// The editor was closing.
    Closing,
    /// Someone asked (Sync now, or a named checkpoint).
    Manual,
    /// An earlier checkpoint was put back.
    Restore,
    /// Changes made while the remote couldn't be reached, now integrated.
    Merge,
}

impl Kind {
    /// The trailer's value.
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Auto => "auto",
            Kind::Left => "left",
            Kind::Handoff => "handoff",
            Kind::Ended => "ended",
            Kind::Closing => "closing",
            Kind::Manual => "manual",
            Kind::Restore => "restore",
            Kind::Merge => "merge",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Kind> {
        Some(match tag {
            "auto" => Kind::Auto,
            "left" => Kind::Left,
            "handoff" => Kind::Handoff,
            "ended" => Kind::Ended,
            "closing" => Kind::Closing,
            "manual" => Kind::Manual,
            "restore" => Kind::Restore,
            "merge" => Kind::Merge,
            _ => return None,
        })
    }

    /// The message's subject, after `[Collaborate] `.
    pub fn subject(self) -> &'static str {
        match self {
            Kind::Auto => "Automatic checkpoint",
            Kind::Left => "User left session",
            Kind::Handoff => "Session handoff",
            Kind::Ended => "Session ended",
            Kind::Closing => "Editor closed",
            Kind::Manual => "Manual checkpoint",
            Kind::Restore => "Restored checkpoint",
            Kind::Merge => "Merge remote changes",
        }
    }

    /// Kinds that are worth a checkpoint even when one was made a moment
    /// ago (everything but the timer's).
    pub fn is_urgent(self) -> bool {
        self != Kind::Auto
    }
}

/// The commit message for a checkpoint of `kind`, with an optional label
/// (a manual checkpoint's name) and who took part.
pub fn message(kind: Kind, label: Option<&str>, participants: &[&str], changed: usize) -> String {
    let subject = match label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => format!("{}: {}", kind.subject(), one_line(label, 72)),
        None => kind.subject().to_owned(),
    };
    let detail = match changed {
        0 => String::new(),
        1 => "1 file changed.".to_owned(),
        n => format!("{n} files changed."),
    };
    crate::checkpoint_message(&subject, kind.tag(), participants, &detail)
}

fn one_line(text: &str, max: usize) -> String {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect();
    line.trim().to_owned()
}

/// One entry of the project's history, as the History list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub commit: Commit,
    /// `None`: someone's own commit, not one of Axcenity's.
    pub kind: Option<Kind>,
    pub participants: Vec<String>,
}

impl Entry {
    /// The subject without `[Collaborate] `.
    pub fn title(&self) -> &str {
        self.commit
            .subject
            .strip_prefix("[Collaborate] ")
            .unwrap_or(&self.commit.subject)
    }
}

/// The newest `count` entries of the history `HEAD` is on.
pub fn history(git: &Git, dir: &Path, count: usize) -> Result<Vec<Entry>, GitError> {
    Ok(git
        .log(dir, "HEAD", count)?
        .into_iter()
        .map(|commit| {
            let kind = commit.trailer(CHECKPOINT_TRAILER).and_then(Kind::from_tag);
            let participants = commit
                .trailer("Axcenity-Participants")
                .map(|p| {
                    p.split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            Entry {
                commit,
                kind,
                participants,
            }
        })
        .collect())
}

/// Puts the project's files back as they were at `commit`, as a new
/// checkpoint on top (outstanding changes are checkpointed first, so
/// nothing is lost). Per-computer files Git ignores (`.axcenity/`) are
/// left alone. Returns the new commit.
pub fn restore(git: &Git, dir: &Path, commit: &str, author: &Author) -> Result<String, GitError> {
    let target = git
        .rev(dir, commit)
        .ok_or_else(|| GitError::Failed(format!("there's no checkpoint {commit}")))?;
    let changed = git.changes(dir)?.len();
    if changed > 0 {
        git.commit_all(
            dir,
            &message(
                Kind::Manual,
                Some("Before restoring"),
                &[&author.name],
                changed,
            ),
            author,
        )?;
    }
    let subject = git
        .log(dir, &target, 1)?
        .first()
        .map(|c| c.subject.trim_start_matches("[Collaborate] ").to_owned())
        .unwrap_or_default();
    // The index and the files become the checkpoint's tree; files added
    // since are removed.
    git.run(dir, &["read-tree", "--reset", "-u", &target])?;
    let label = format!(
        "\u{201c}{}\u{201d} ({})",
        one_line(&subject, 60),
        &target[..7.min(target.len())]
    );
    let text = message(Kind::Restore, Some(&label), &[&author.name], 0);
    match git.commit_all(dir, &text, author)? {
        Some(commit) => Ok(commit),
        // Already as it was: nothing to add.
        None => git
            .head(dir)
            .ok_or_else(|| GitError::Failed("no history".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{author, git, temp, write};

    #[test]
    fn messages_carry_the_kind_and_who_took_part() {
        let text = message(Kind::Left, None, &["Paul", "Sam"], 3);
        assert!(
            text.starts_with("[Collaborate] User left session\n"),
            "{text}"
        );
        assert!(text.contains("3 files changed."));
        assert!(text.contains("Axcenity-Checkpoint: left\n"));
        assert!(text.contains("Axcenity-Participants: Paul, Sam\n"));
        let named = message(Kind::Manual, Some("Before the boss\nfight"), &[], 0);
        assert!(
            named.starts_with("[Collaborate] Manual checkpoint: Before the boss fight\n"),
            "{named}"
        );
        for kind in [
            Kind::Auto,
            Kind::Left,
            Kind::Handoff,
            Kind::Ended,
            Kind::Closing,
            Kind::Manual,
            Kind::Restore,
            Kind::Merge,
        ] {
            assert_eq!(Kind::from_tag(kind.tag()), Some(kind));
        }
    }

    #[test]
    fn history_tells_checkpoints_from_peoples_own_commits() {
        let git = git();
        let dir = temp("history");
        git.init(&dir, "main").unwrap();
        write(&dir, "a.txt", "1");
        git.commit_all(&dir, "My own commit", &author("Paul"))
            .unwrap();
        write(&dir, "a.txt", "2");
        git.commit_all(
            &dir,
            &message(Kind::Auto, None, &["Paul", "Sam"], 1),
            &author("Paul"),
        )
        .unwrap();
        let entries = history(&git, &dir, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, Some(Kind::Auto));
        assert_eq!(entries[0].title(), "Automatic checkpoint");
        assert_eq!(entries[0].participants, vec!["Paul", "Sam"]);
        assert_eq!(entries[1].kind, None);
        assert_eq!(entries[1].title(), "My own commit");
    }

    #[test]
    fn restoring_adds_a_checkpoint_and_can_be_undone() {
        let git = git();
        let dir = temp("restore");
        git.init(&dir, "main").unwrap();
        write(&dir, "a.txt", "first");
        write(&dir, ".axcenity/cache.bin", "local");
        std::fs::write(dir.join(".gitignore"), ".axcenity/\n").unwrap();
        let first = git
            .commit_all(
                &dir,
                &message(Kind::Manual, Some("First"), &[], 1),
                &author("Paul"),
            )
            .unwrap()
            .unwrap();
        write(&dir, "a.txt", "second");
        write(&dir, "b.txt", "new file");
        let second = git
            .commit_all(&dir, &message(Kind::Auto, None, &[], 2), &author("Paul"))
            .unwrap()
            .unwrap();
        // An unsaved change is checkpointed before the restore.
        write(&dir, "a.txt", "third, not checkpointed");
        let restored = restore(&git, &dir, &first, &author("Paul")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "first");
        assert!(!dir.join("b.txt").exists(), "added since: removed");
        assert!(
            dir.join(".axcenity/cache.bin").exists(),
            "ignored files stay"
        );
        let entries = history(&git, &dir, 10).unwrap();
        assert_eq!(entries[0].commit.sha, restored);
        assert_eq!(entries[0].kind, Some(Kind::Restore));
        assert!(
            entries[0].title().contains("First"),
            "{}",
            entries[0].title()
        );
        assert_eq!(entries[1].title(), "Manual checkpoint: Before restoring");
        assert_eq!(entries.len(), 4, "nothing rewritten");
        // The "Before restoring" checkpoint brings the unsaved change back.
        restore(&git, &dir, &entries[1].commit.sha, &author("Paul")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "third, not checkpointed"
        );
        assert!(dir.join("b.txt").exists());
        let _ = second;
    }
}
