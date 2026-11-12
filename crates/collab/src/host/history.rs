//! The host's side of checkpoints (C9): making them when the session
//! starts, every hour of changes and when a member asks, telling members
//! about them, and restoring one for everyone as one transaction.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axcenity_core::InstanceId;

use super::{Host, Outgoing};
use crate::apply;
use crate::history::{self, Checkpoint, CheckpointKind, History};
use crate::message::{MemberId, ToClient};
use crate::op::Op;
use crate::role::{Capability, Role};

impl Host {
    /// Starts keeping checkpoints of the project at `project`, with one of
    /// the session as it starts (unless the newest is the same).
    pub(super) fn start_history(&mut self, project: &std::path::Path) {
        self.history = Some(History::of(project));
        self.edited_since.clear();
        self.checkpoint(CheckpointKind::SessionStart, "Session started", None, true);
    }

    /// The checkpoints members are told about, if there are any.
    pub(super) fn checkpoints(&self) -> Option<Vec<Checkpoint>> {
        let mut list = self.history.as_ref()?.list();
        list.truncate(history::LISTED);
        (!list.is_empty()).then_some(list)
    }

    /// Every [`history::CHECKPOINT_EVERY`] with changes: a checkpoint.
    pub(super) fn hourly_checkpoint(&mut self, now: Instant) -> Vec<Outgoing> {
        let (at, seq) = self.checkpointed;
        if self.history.is_none()
            || self.seq == seq
            || now.saturating_duration_since(at) < history::CHECKPOINT_EVERY
        {
            return Vec::new();
        }
        if self
            .checkpoint(CheckpointKind::Hourly, "Hourly checkpoint", None, true)
            .is_some()
        {
            return self.checkpoints_to_everyone();
        }
        Vec::new()
    }

    /// `Checkpoint`: a member with the capability asks for one now.
    pub(super) fn save_checkpoint(&mut self, member: MemberId, label: &str) -> Vec<Outgoing> {
        let m = &self.members[&member];
        if !m.connected {
            return Vec::new();
        }
        if !m.role.can(Capability::SaveCheckpoint) {
            self.denied(member, "a checkpoint", Instant::now());
            return self.history_notice(member, "Viewers can't save checkpoints");
        }
        if self.history.is_none() {
            return self.history_notice(member, "This session doesn't keep checkpoints");
        }
        let label = tidy_label(label);
        let by = Some(m.profile.name.clone());
        match self.checkpoint(CheckpointKind::Saved, &label, by, false) {
            Some(_) => self.checkpoints_to_everyone(),
            None => self.history_notice(member, "The checkpoint couldn't be saved"),
        }
    }

    /// `Restore`: the Owner puts the session back as checkpoint `id` was,
    /// for everyone. A checkpoint of the session as it is comes first, so
    /// the restore can be taken back the same way.
    pub(super) fn restore(&mut self, member: MemberId, id: u64) -> Vec<Outgoing> {
        let m = &self.members[&member];
        if !m.connected {
            return Vec::new();
        }
        if m.role != Role::Owner {
            self.denied(member, "a restore", Instant::now());
            return self.history_notice(member, "Only the owner can restore a checkpoint");
        }
        let by = m.profile.name.clone();
        let Some(history) = self.history.clone() else {
            return self.history_notice(member, "This session doesn't keep checkpoints");
        };
        let Some(label) = history
            .list()
            .into_iter()
            .find(|checkpoint| checkpoint.id == id)
            .map(|checkpoint| checkpoint.label)
        else {
            return self.history_notice(member, "That checkpoint is gone");
        };
        let (then, then_files) = match history.load(id) {
            Ok(loaded) => loaded,
            Err(error) => {
                return self
                    .history_notice(member, &format!("The checkpoint doesn't read: {error}"));
            }
        };
        if self
            .checkpoint(
                CheckpointKind::BeforeRestore,
                &format!("Before restoring \u{201c}{label}\u{201d}"),
                Some(by.clone()),
                false,
            )
            .is_none()
        {
            return self.history_notice(
                member,
                "Couldn't keep a checkpoint of the session first, so nothing was restored",
            );
        }
        // New Instances get IDs from blocks of the host's own.
        let ids = &mut self.ids;
        let mut block = None;
        let mut next = 0;
        let mut fresh = || {
            let current = match block {
                Some(current) if next < crate::ids::BLOCK_SIZE => current,
                _ => {
                    // Out of IDs: the restore stops with an error below.
                    let new = ids.block()?;
                    block = Some(new);
                    next = 0;
                    new
                }
            };
            let crate::ids::IdBlock { start, .. } = current;
            next += 1;
            Some(InstanceId::from_bits(u64::from(start + next - 1)))
        };
        let store = &self.store;
        // Read, not just looked up: a file found in the folder at the start
        // may have changed there since.
        let has_file = |hash| history.read_file(hash).is_some() || store.read(hash).is_some();
        let restore =
            match history::restore_ops(&self.doc, &then, &then_files, &mut fresh, &has_file) {
                Ok(restore) => restore,
                Err(reason) => return self.history_notice(member, &reason),
            };
        for op in &restore.ops {
            if let Op::FileVersion {
                new: Some(hash), ..
            } = op
                && self.store.read(*hash).is_none()
                && let Some(bytes) = history.read_file(*hash)
            {
                self.store.keep(*hash, bytes);
            }
        }
        let applied = match apply::apply(&mut self.doc.view(), &restore.ops, &|_| true) {
            Ok(applied) => applied,
            Err(reason) => {
                return self.history_notice(member, &format!("The restore didn't apply: {reason}"));
            }
        };
        let files: Vec<String> = applied
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::FileVersion { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        // Number 0: the Owner's editor doesn't take it for one of its own.
        let mut out = self.commit(
            member,
            0,
            format!("Restore \u{201c}{label}\u{201d}"),
            applied,
        );
        for path in files {
            out.extend(self.doc_follows_file(&path, member));
        }
        out.extend(self.checkpoints_to_everyone());
        self.record(
            crate::audit::AuditKind::Restored,
            Some(member),
            format!("{by} restored \u{201c}{label}\u{201d} for everyone"),
        );
        if !restore.missing.is_empty() {
            out.extend(self.history_notice(
                member,
                &format!(
                    "Restored, but the checkpoint didn't keep {} of its files, so they were left as they are: {}",
                    restore.missing.len(),
                    restore.missing.join(", ")
                ),
            ));
        }
        out
    }

    /// Keeps a checkpoint of the session now. `None` when nothing was kept
    /// (unchanged since the newest with `skip_unchanged`, or it couldn't be
    /// written, which is logged).
    fn checkpoint(
        &mut self,
        kind: CheckpointKind,
        label: &str,
        by: Option<String>,
        skip_unchanged: bool,
    ) -> Option<Checkpoint> {
        let history = self.history.as_ref()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let edited_by = self
            .edited_since
            .iter()
            .filter_map(|member| self.members.get(member))
            .map(|m| m.profile.name.clone())
            .collect();
        let checkpoint = Checkpoint {
            id: now.as_millis() as u64,
            at: now.as_secs(),
            kind,
            label: label.to_owned(),
            by,
            edited_by,
            seq: self.seq,
        };
        let kept = match history.record(&self.doc, self.store.as_ref(), checkpoint, skip_unchanged)
        {
            Ok(kept) => kept,
            Err(error) => {
                log::error!("Collaborate: couldn't keep a checkpoint: {error}");
                return None;
            }
        };
        // Unchanged counts as checkpointed too: the hour starts again.
        self.checkpointed = (self.clock, self.seq);
        if kept.is_some() {
            self.edited_since.clear();
        }
        kept
    }

    fn checkpoints_to_everyone(&self) -> Vec<Outgoing> {
        self.checkpoints()
            .map(|list| self.to_everyone(ToClient::Checkpoints(list)))
            .unwrap_or_default()
    }

    pub(super) fn history_notice(&self, member: MemberId, text: &str) -> Vec<Outgoing> {
        vec![Outgoing {
            to: member,
            message: ToClient::HistoryNotice(text.to_owned()),
        }]
    }
}

/// A member's label: one line, control characters left out, at most
/// [`history::LABEL_MOST`] characters; "Checkpoint" when nothing's left.
fn tidy_label(label: &str) -> String {
    let line: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(history::LABEL_MOST)
        .collect();
    let line = line.trim();
    if line.is_empty() {
        "Checkpoint".to_owned()
    } else {
        line.to_owned()
    }
}
