//! A collaborator's side of a session: their own copy of the World, kept as
//! the host's order says, with their unconfirmed edits on top.

use std::collections::VecDeque;

use axcenity_core::{InstanceId, World};

use crate::apply::{self, Doc, Note, Refusal, Undo};
use crate::ids::{BLOCK_SIZE, IdPool};
use crate::message::{MemberId, ToClient, ToHost};
use crate::op::{Op, Transaction};
use crate::role::{Capability, Role};

/// Something the editor should tell the user about their own edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Their transaction applied, but not entirely as made (someone deleted
    /// what it edited, or the parent it used).
    Changed {
        client_seq: u64,
        label: String,
        notes: Vec<Note>,
    },
    /// The host refused their transaction; it was taken off.
    Refused {
        client_seq: u64,
        label: String,
        reason: Refusal,
    },
    /// The copy disagreed with the host's, so it was replaced by a fresh
    /// snapshot (a bug if it ever happens; edits not yet confirmed are kept
    /// and sent again).
    Resynced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Connection {
    Offline,
    /// `Hello` sent, waiting for `Welcome` or `Resumed`.
    Joining,
    Online,
}

#[derive(Debug)]
struct Pending {
    transaction: Transaction,
    /// How to take it off the copy; `None` while it isn't on (it didn't
    /// apply locally, or it has just been taken off).
    undo: Option<Undo>,
}

/// One member's copy of the session.
pub struct Client {
    member: MemberId,
    doc: Option<Doc>,
    /// The last host transaction applied to `doc`.
    seq: u64,
    role: Role,
    pending: VecDeque<Pending>,
    next_client_seq: u64,
    ids: IdPool,
    asked_for_ids: bool,
    /// A fresh snapshot was asked for: host transactions until it comes
    /// don't apply to this copy.
    awaiting_snapshot: bool,
    connection: Connection,
    outbox: Vec<ToHost>,
    notices: Vec<Notice>,
}

impl Client {
    pub fn new(member: MemberId) -> Client {
        Client {
            member,
            doc: None,
            seq: 0,
            role: Role::Viewer,
            pending: VecDeque::new(),
            next_client_seq: 1,
            ids: IdPool::default(),
            asked_for_ids: false,
            awaiting_snapshot: false,
            connection: Connection::Offline,
            outbox: Vec::new(),
            notices: Vec::new(),
        }
    }

    pub fn member(&self) -> MemberId {
        self.member
    }

    /// The copy as the user sees it, once joined.
    pub fn world(&self) -> Option<&World> {
        self.doc.as_ref().map(|doc| &doc.world)
    }

    pub fn doc(&self) -> Option<&Doc> {
        self.doc.as_ref()
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn is_online(&self) -> bool {
        self.connection == Connection::Online
    }

    /// Edits not yet confirmed by the host.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The last host transaction this copy has.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Messages for the host, oldest first.
    pub fn take_outbox(&mut self) -> Vec<ToHost> {
        std::mem::take(&mut self.outbox)
    }

    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// A new connection is open: says hello, asking to catch up if this
    /// copy has the session already.
    pub fn connect(&mut self) {
        self.connection = Connection::Joining;
        self.asked_for_ids = false;
        self.awaiting_snapshot = false;
        let have = self.doc.as_ref().map(|_| self.seq);
        self.outbox.push(ToHost::Hello { have });
    }

    /// The connection closed. Editing goes on; edits wait in `pending`
    /// until the next `connect`.
    pub fn disconnected(&mut self) {
        self.connection = Connection::Offline;
        self.outbox.clear();
    }

    /// An ID for a new Instance, from this member's blocks. Asks the host
    /// for more when half a block is left. `None` when there are none left
    /// (only while offline for long).
    pub fn new_id(&mut self) -> Option<InstanceId> {
        let id = self.ids.take();
        if self.ids.left() < BLOCK_SIZE / 2 && !self.asked_for_ids && self.is_online() {
            self.asked_for_ids = true;
            self.outbox.push(ToHost::NeedIds);
        }
        id
    }

    /// Makes an edit: applies it to this copy at once and sends it to the
    /// host (or keeps it to send after reconnecting). Returns its number.
    /// An edit that can't apply here (a move into itself) is refused at
    /// once and never sent.
    pub fn submit(&mut self, label: impl Into<String>, ops: Vec<Op>) -> Result<u64, Refusal> {
        if !self.role.can(Capability::EditWorld) {
            return Err(Refusal::Permission);
        }
        let Some(doc) = self.doc.as_mut() else {
            return Err(Refusal::Invalid("not in a session yet".to_owned()));
        };
        let applied = apply::apply(doc, &ops, &|_| true)?;
        let transaction = Transaction {
            client_seq: self.next_client_seq,
            label: label.into(),
            ops,
        };
        self.next_client_seq += 1;
        if self.is_online() {
            self.outbox.push(ToHost::Submit(transaction.clone()));
        }
        let client_seq = transaction.client_seq;
        self.pending.push_back(Pending {
            transaction,
            undo: Some(applied.undo),
        });
        Ok(client_seq)
    }

    /// Handles one message from the host.
    pub fn receive(&mut self, message: ToClient) {
        match message {
            ToClient::Welcome {
                world,
                graveyard,
                seq,
                last_client_seq,
                role,
                ids,
                rejected,
            } => {
                // The old copy is gone, and with it what undoes the pending
                // edits on it.
                for pending in &mut self.pending {
                    pending.undo = None;
                }
                self.doc = Some(Doc {
                    world: *world.0,
                    graveyard,
                });
                self.awaiting_snapshot = false;
                self.seq = seq;
                self.role = role;
                if let Some(block) = ids {
                    self.ids.add(block);
                }
                self.finish_joining(last_client_seq, &rejected);
            }
            ToClient::Resumed {
                last_client_seq,
                role,
                rejected,
            } => {
                self.role = role;
                self.take_pending_off();
                self.finish_joining(last_client_seq, &rejected);
            }
            ToClient::Accepted {
                seq,
                author,
                client_seq,
                label,
                ops,
                notes,
            } => self.accepted(seq, author, client_seq, label, ops, notes),
            ToClient::Rejected { client_seq, reason } => {
                self.take_pending_off();
                if let Some(at) = self
                    .pending
                    .iter()
                    .position(|p| p.transaction.client_seq == client_seq)
                {
                    let pending = self.pending.remove(at).expect("found above");
                    self.notices.push(Notice::Refused {
                        client_seq,
                        label: pending.transaction.label,
                        reason,
                    });
                }
                self.put_pending_on();
            }
            ToClient::Ids(block) => {
                self.ids.add(block);
                self.asked_for_ids = false;
            }
            ToClient::RoleChanged(role) => self.role = role,
        }
    }

    fn accepted(
        &mut self,
        seq: u64,
        author: MemberId,
        client_seq: u64,
        label: String,
        ops: Vec<Op>,
        notes: Vec<Note>,
    ) {
        if self.doc.is_none() || self.awaiting_snapshot || seq <= self.seq {
            return;
        }
        if seq != self.seq + 1 {
            self.resync();
            return;
        }
        self.take_pending_off();
        let doc = self.doc.as_mut().expect("checked above");
        match apply::apply(doc, &ops, &|_| true) {
            Ok(applied) if applied.notes.is_empty() => {}
            Ok(applied) => {
                apply::undo(doc, applied.undo);
                self.resync();
                return;
            }
            Err(_) => {
                self.resync();
                return;
            }
        }
        self.seq = seq;
        if author == self.member {
            // The host handles one member's transactions in order and
            // refuses before accepting later ones, so this is the oldest.
            while let Some(front) = self.pending.front() {
                if front.transaction.client_seq > client_seq {
                    break;
                }
                self.pending.pop_front();
            }
            if !notes.is_empty() {
                self.notices.push(Notice::Changed {
                    client_seq,
                    label,
                    notes,
                });
            }
        }
        self.put_pending_on();
    }

    /// The end of `Welcome` or `Resumed`: forget pending edits the host
    /// already handled (accepted ones are in the copy now; refused ones are
    /// reported), put the rest back on, and send them.
    fn finish_joining(&mut self, last_client_seq: u64, rejected: &[(u64, Refusal)]) {
        while let Some(front) = self.pending.front() {
            if front.transaction.client_seq > last_client_seq {
                break;
            }
            let pending = self.pending.pop_front().expect("checked above");
            let client_seq = pending.transaction.client_seq;
            if let Some((_, reason)) = rejected.iter().find(|(seq, _)| *seq == client_seq) {
                self.notices.push(Notice::Refused {
                    client_seq,
                    label: pending.transaction.label,
                    reason: reason.clone(),
                });
            }
        }
        self.put_pending_on();
        self.connection = Connection::Online;
        for pending in &self.pending {
            self.outbox
                .push(ToHost::Submit(pending.transaction.clone()));
        }
        if self.ids.left() < BLOCK_SIZE / 2 {
            self.asked_for_ids = true;
            self.outbox.push(ToHost::NeedIds);
        }
    }

    /// Asks for a fresh snapshot after this copy disagreed with the host.
    fn resync(&mut self) {
        if self.awaiting_snapshot {
            return;
        }
        self.awaiting_snapshot = true;
        self.notices.push(Notice::Resynced);
        self.connection = Connection::Joining;
        self.outbox.push(ToHost::Hello { have: None });
    }

    /// Takes every pending edit off the copy, newest first.
    fn take_pending_off(&mut self) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        for pending in self.pending.iter_mut().rev() {
            if let Some(undo) = pending.undo.take() {
                apply::undo(doc, undo);
            }
        }
    }

    /// Puts every pending edit back on, oldest first, with the same rules
    /// the host will use. One that no longer applies stays pending, off the
    /// copy, for the host to decide.
    fn put_pending_on(&mut self) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        for pending in &mut self.pending {
            pending.undo = apply::apply(doc, &pending.transaction.ops, &|_| true)
                .ok()
                .map(|applied| applied.undo);
        }
    }
}
