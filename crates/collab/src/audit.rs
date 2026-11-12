//! The host's audit log (C10): who joined or was turned away, invites,
//! role changes and removals, restores, and members refused for their
//! role or for going over a [`limits`](crate::limits) budget. The Owner
//! reads it in the Collaborate panel ([`ToHost::AuditLog`](crate::message::ToHost::AuditLog)).
//!
//! The host keeps the newest [`KEPT`] entries in memory and, when it has
//! a members file, appends each to a file beside it in the host's data
//! folder (never the project's, which may be public): one JSON object a
//! line, the older half moved aside once it passes [`FILE_MOST`] bytes.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::message::MemberId;

/// Entries kept in memory and sent to the Owner.
pub const KEPT: usize = 500;

/// The file's size past which it's moved to `<name>.old` and started again.
pub const FILE_MOST: u64 = 1 << 20;

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditKind {
    /// A member was let in.
    Joined,
    /// A handshake was refused (not a member, a bad invite, a bad
    /// signature, other versions, too many tries).
    Refused,
    /// The Owner made an invite, or revoked those not used.
    Invite,
    /// A member's role changed.
    Role,
    /// A member was taken out of the project.
    Removed,
    /// The Owner restored a checkpoint.
    Restored,
    /// A member asked for something their role doesn't allow.
    Denied,
    /// A member went over a budget; what was over it was dropped.
    Limited,
    /// The session moved to another host, or a new host recovered a lost
    /// one's transactions (Collaborate 2.0).
    Handoff,
}

/// One line of the audit log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Seconds since 1970.
    pub at: u64,
    pub kind: AuditKind,
    /// The member it's about, if any.
    pub member: Option<MemberId>,
    /// What happened, in a sentence for the Owner.
    pub text: String,
}

/// The audit log.
#[derive(Debug, Default)]
pub struct Audit {
    entries: VecDeque<AuditEntry>,
    file: Option<PathBuf>,
}

impl Audit {
    /// Also writes each entry to `file`, after the newest [`KEPT`] already
    /// there are read back.
    pub fn keep_in(&mut self, file: PathBuf) {
        if let Ok(text) = std::fs::read_to_string(&file) {
            for line in text.lines() {
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(line) {
                    self.push(entry);
                }
            }
        }
        self.file = Some(file);
    }

    /// Records that `kind` happened, now.
    pub fn record(&mut self, kind: AuditKind, member: Option<MemberId>, text: impl Into<String>) {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let entry = AuditEntry {
            at,
            kind,
            member,
            text: text.into(),
        };
        if let Some(file) = &self.file
            && let Err(error) = append(file, &entry)
        {
            log::warn!("Collaborate: couldn't write the audit log: {error}");
        }
        self.push(entry);
    }

    fn push(&mut self, entry: AuditEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > KEPT {
            self.entries.pop_front();
        }
    }

    /// Oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &AuditEntry> {
        self.entries.iter()
    }
}

fn append(file: &Path, entry: &AuditEntry) -> std::io::Result<()> {
    if std::fs::metadata(file).is_ok_and(|m| m.len() > FILE_MOST) {
        std::fs::rename(file, file.with_extension("old"))?;
    }
    let mut line = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
    line.push(b'\n');
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)?.write_all(&line)
}

/// The audit file beside the members file `members`.
pub fn file_beside(members: &Path) -> PathBuf {
    members.with_extension("audit.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_kept_in_a_file_and_read_back() {
        let folder = std::env::temp_dir().join(format!("axcenity-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = file_beside(&folder.join("abc.json"));
        assert_eq!(file.file_name().unwrap(), "abc.audit.jsonl");

        let mut audit = Audit::default();
        audit.keep_in(file.clone());
        audit.record(AuditKind::Joined, Some(MemberId(2)), "Sam joined as Editor");
        audit.record(AuditKind::Refused, None, "Refused a joiner: not invited");
        let mut again = Audit::default();
        again.keep_in(file.clone());
        let texts: Vec<_> = again.entries().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Sam joined as Editor", "Refused a joiner: not invited"]
        );
        assert_eq!(again.entries().next().unwrap().member, Some(MemberId(2)));

        // Only the newest are kept in memory.
        for i in 0..KEPT + 10 {
            again.record(AuditKind::Limited, None, format!("{i}"));
        }
        assert_eq!(again.entries().count(), KEPT);
        assert_eq!(
            again.entries().last().unwrap().text,
            format!("{}", KEPT + 9)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
