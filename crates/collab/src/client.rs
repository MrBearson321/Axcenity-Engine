//! A collaborator's side of a session. The World is the editor's own (the
//! one its tools edit); the client keeps it as the host's order says, with
//! this member's unconfirmed edits on top.
//!
//! Tools edit the World directly. The World keeps an edit journal
//! ([`World::record_edits`], switched on when the session's World arrives),
//! and [`Client::capture`] turns what changed into a transaction at the end
//! of each undo step. Edits can also be sent as ops with
//! [`Client::submit`]. Every call that takes the World captures what's
//! outstanding first, so edits never get lost or out of order.

use std::collections::VecDeque;

use axcenity_core::{InstanceId, World};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::apply::{self, DocMut, Graveyard, Note, Refusal, Undo};
use crate::capture;
use crate::ids::BLOCK_SIZE;
use crate::message::{MemberId, ToClient, ToHost};
use crate::op::{Op, Transaction};
use crate::presence::{Collaborator, Presence};
use crate::role::{Capability, Role};

/// Something the editor should tell the user about, or act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The World was replaced with the session's (on joining, or after a
    /// resync): selections and the editor's own undo steps are stale.
    Joined,
    /// Their transaction applied, but not entirely as made (someone deleted
    /// what it edited, or the parent it used).
    Changed {
        client_seq: u64,
        label: String,
        notes: Vec<Note>,
    },
    /// The host refused their transaction (or their role doesn't allow
    /// edits); it was taken off.
    Refused {
        client_seq: u64,
        label: String,
        reason: Refusal,
    },
    /// The copy disagreed with the host's, so it was replaced by a fresh
    /// snapshot (a bug if it ever happens; edits not yet confirmed are kept
    /// and sent again).
    Resynced,
    /// Another member connected.
    Arrived { member: MemberId, name: String },
    /// Another member's connection closed, or they were removed.
    Left { member: MemberId, name: String },
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

/// One member's side of the session.
pub struct Client {
    member: MemberId,
    /// True once the session's World has arrived.
    joined: bool,
    graveyard: Graveyard,
    /// New Instances a tool made that aren't in the game yet (it may still
    /// be building them); they're sent once placed.
    unsent: FxHashSet<InstanceId>,
    /// The last host transaction applied to the World.
    seq: u64,
    role: Role,
    pending: VecDeque<Pending>,
    next_client_seq: u64,
    asked_for_ids: bool,
    /// A fresh snapshot was asked for: host transactions until it comes
    /// don't apply to this copy.
    awaiting_snapshot: bool,
    connection: Connection,
    outbox: Vec<ToHost>,
    notices: Vec<Notice>,
    /// Everyone in the project, as the host last said.
    roster: Vec<Collaborator>,
    /// What the other connected members are doing.
    presences: FxHashMap<MemberId, Presence>,
    /// What this member is doing, as last set.
    presence: Presence,
}

impl Client {
    pub fn new(member: MemberId) -> Client {
        Client {
            member,
            joined: false,
            graveyard: Graveyard::default(),
            unsent: FxHashSet::default(),
            seq: 0,
            role: Role::Viewer,
            pending: VecDeque::new(),
            next_client_seq: 1,
            asked_for_ids: false,
            awaiting_snapshot: false,
            connection: Connection::Offline,
            outbox: Vec::new(),
            notices: Vec::new(),
            roster: Vec::new(),
            presences: FxHashMap::default(),
            presence: Presence::default(),
        }
    }

    /// Everyone in the project, as the host last said.
    pub fn roster(&self) -> &[Collaborator] {
        &self.roster
    }

    /// What each other connected member is doing.
    pub fn presences(&self) -> &FxHashMap<MemberId, Presence> {
        &self.presences
    }

    /// Says what this member is doing now; sent if it changed.
    pub fn set_presence(&mut self, presence: Presence) {
        let presence = presence.tidied();
        if presence == self.presence {
            return;
        }
        self.presence = presence.clone();
        if self.connection != Connection::Offline {
            self.outbox.push(ToHost::Presence(presence));
        }
    }

    pub fn member(&self) -> MemberId {
        self.member
    }

    /// True once the session's World has arrived.
    pub fn is_joined(&self) -> bool {
        self.joined
    }

    /// Deleted Instances (kept detached in the World) and where they were.
    pub fn graveyard(&self) -> &Graveyard {
        &self.graveyard
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
        let have = self.joined.then_some(self.seq);
        self.outbox.push(ToHost::Hello { have });
        if self.presence != Presence::default() {
            self.outbox.push(ToHost::Presence(self.presence.clone()));
        }
    }

    /// The connection closed. Editing goes on; edits wait in `pending`
    /// until the next `connect`.
    pub fn disconnected(&mut self) {
        self.connection = Connection::Offline;
        self.outbox.clear();
        self.presences.clear();
    }

    /// An ID from this member's blocks, for a new Instance to make with
    /// [`World::create_at`] and send in a `CreateTree`. (`World::create`
    /// takes from the same blocks.) Asks the host for more when half a
    /// block is left. `None` when there are none left (only while offline
    /// for long).
    pub fn new_id(&mut self, world: &mut World) -> Option<InstanceId> {
        let id = world.take_id();
        self.ask_for_ids_if_low(world);
        id
    }

    /// Turns the World's edits since the last capture into one transaction
    /// (one undo step, named `label`), applies it as the host will and sends
    /// it. Returns its number, or `None` if nothing shared changed. Call it
    /// when a tool finishes an undo step.
    ///
    /// If this member may not edit, the edits are taken back and a
    /// `Refused` notice says so.
    pub fn capture(&mut self, world: &mut World, label: impl Into<String>) -> Option<u64> {
        if !self.joined {
            world.take_edits();
            return None;
        }
        if !world.has_edits() {
            return None;
        }
        let captured = capture::capture(world, &self.graveyard, &mut self.unsent);
        if captured.ops.is_empty() {
            return None;
        }
        let label = label.into();
        if !self.role.can(Capability::EditWorld) {
            // `capture` already put the World back as it was.
            self.notices.push(Notice::Refused {
                client_seq: 0,
                label,
                reason: Refusal::Permission,
            });
            return None;
        }
        let result = self.submit_ops(world, label.clone(), captured.ops);
        self.ask_for_ids_if_low(world);
        match result {
            Ok(seq) => Some(seq),
            Err(reason) => {
                self.notices.push(Notice::Refused {
                    client_seq: 0,
                    label,
                    reason,
                });
                None
            }
        }
    }

    /// Makes an edit from ops: applies it to the World at once and sends it
    /// to the host (or keeps it to send after reconnecting). Returns its
    /// number. An edit that can't apply here (a move into itself) is
    /// refused at once and never sent.
    pub fn submit(
        &mut self,
        world: &mut World,
        label: impl Into<String>,
        ops: Vec<Op>,
    ) -> Result<u64, Refusal> {
        if !self.role.can(Capability::EditWorld) {
            return Err(Refusal::Permission);
        }
        if !self.joined {
            return Err(Refusal::Invalid("not in a session yet".to_owned()));
        }
        self.capture(world, "Edit");
        self.submit_ops(world, label.into(), ops)
    }

    fn submit_ops(
        &mut self,
        world: &mut World,
        label: String,
        ops: Vec<Op>,
    ) -> Result<u64, Refusal> {
        let applied = apply::apply(&mut self.view(world), &ops, &|_| true);
        world.take_edits();
        let applied = applied?;
        let transaction = Transaction {
            client_seq: self.next_client_seq,
            label,
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

    /// Handles one message from the host. Edits not captured yet are
    /// captured first (as one step labelled "Edit"); the editor should hold
    /// messages while a tool is mid-step, such as during a drag.
    pub fn receive(&mut self, world: &mut World, message: ToClient) {
        if self.joined && world.has_edits() {
            self.capture(world, "Edit");
        }
        match message {
            ToClient::Welcome {
                world: snapshot,
                graveyard,
                seq,
                last_client_seq,
                role,
                ids,
                rejected,
            } => {
                // The old copy is gone, and with it what undoes the pending
                // edits on it. Blocks of IDs not used yet stay this member's.
                for pending in &mut self.pending {
                    pending.undo = None;
                }
                let blocks = world.id_blocks_left();
                *world = *snapshot.0;
                world.make_replica();
                for (start, len) in blocks {
                    world.add_id_block(start, len);
                }
                if let Some(block) = ids {
                    world.add_id_block(block.start, block.len);
                }
                world.record_edits();
                self.graveyard = graveyard;
                self.unsent.clear();
                self.joined = true;
                self.awaiting_snapshot = false;
                self.seq = seq;
                self.role = role;
                self.notices.push(Notice::Joined);
                self.finish_joining(world, last_client_seq, &rejected);
            }
            ToClient::Resumed {
                last_client_seq,
                role,
                rejected,
            } => {
                self.role = role;
                self.take_pending_off(world);
                self.finish_joining(world, last_client_seq, &rejected);
            }
            ToClient::Accepted {
                seq,
                author,
                client_seq,
                label,
                ops,
                notes,
            } => self.accepted(world, seq, author, client_seq, label, ops, notes),
            ToClient::Rejected { client_seq, reason } => {
                self.take_pending_off(world);
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
                self.put_pending_on(world);
            }
            ToClient::Ids(block) => {
                world.add_id_block(block.start, block.len);
                self.asked_for_ids = false;
            }
            ToClient::RoleChanged(role) => self.role = role,
            ToClient::Roster(roster) => self.new_roster(roster),
            ToClient::Presence { member, presence } => {
                if member != self.member {
                    self.presences.insert(member, presence);
                }
            }
        }
        // What applying and undoing did isn't the user's edit.
        world.take_edits();
    }

    /// A new roster: says who came and went (not on the first, which lists
    /// who was already here).
    fn new_roster(&mut self, roster: Vec<Collaborator>) {
        let online =
            |list: &[Collaborator], member| list.iter().any(|c| c.member == member && c.online);
        if !self.roster.is_empty() {
            for c in &roster {
                if c.member != self.member && c.online && !online(&self.roster, c.member) {
                    self.notices.push(Notice::Arrived {
                        member: c.member,
                        name: c.profile.name.clone(),
                    });
                }
            }
            for c in &self.roster {
                if c.member != self.member && c.online && !online(&roster, c.member) {
                    self.notices.push(Notice::Left {
                        member: c.member,
                        name: c.profile.name.clone(),
                    });
                }
            }
        }
        self.presences.retain(|&member, _| online(&roster, member));
        self.roster = roster;
    }

    #[allow(clippy::too_many_arguments)]
    fn accepted(
        &mut self,
        world: &mut World,
        seq: u64,
        author: MemberId,
        client_seq: u64,
        label: String,
        ops: Vec<Op>,
        notes: Vec<Note>,
    ) {
        if !self.joined || self.awaiting_snapshot || seq <= self.seq {
            return;
        }
        if seq != self.seq + 1 {
            self.resync();
            return;
        }
        self.take_pending_off(world);
        let mut doc = self.view(world);
        match apply::apply(&mut doc, &ops, &|_| true) {
            Ok(applied) if applied.notes.is_empty() => {}
            Ok(applied) => {
                apply::undo(&mut doc, applied.undo);
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
        self.put_pending_on(world);
    }

    /// The end of `Welcome` or `Resumed`: forget pending edits the host
    /// already handled (accepted ones are in the copy now; refused ones are
    /// reported), put the rest back on, and send them.
    fn finish_joining(
        &mut self,
        world: &mut World,
        last_client_seq: u64,
        rejected: &[(u64, Refusal)],
    ) {
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
        self.put_pending_on(world);
        self.connection = Connection::Online;
        for pending in &self.pending {
            self.outbox
                .push(ToHost::Submit(pending.transaction.clone()));
        }
        self.ask_for_ids_if_low(world);
    }

    fn ask_for_ids_if_low(&mut self, world: &World) {
        if world.ids_left() < BLOCK_SIZE / 2 && !self.asked_for_ids && self.is_online() {
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

    fn view<'a>(&'a mut self, world: &'a mut World) -> DocMut<'a> {
        DocMut {
            world,
            graveyard: &mut self.graveyard,
        }
    }

    /// Takes every pending edit off the copy, newest first.
    fn take_pending_off(&mut self, world: &mut World) {
        if !self.joined {
            return;
        }
        let mut doc = DocMut {
            world,
            graveyard: &mut self.graveyard,
        };
        for pending in self.pending.iter_mut().rev() {
            if let Some(undo) = pending.undo.take() {
                apply::undo(&mut doc, undo);
            }
        }
    }

    /// Puts every pending edit back on, oldest first, with the same rules
    /// the host will use. One that no longer applies stays pending, off the
    /// copy, for the host to decide.
    fn put_pending_on(&mut self, world: &mut World) {
        if !self.joined {
            return;
        }
        let mut doc = DocMut {
            world,
            graveyard: &mut self.graveyard,
        };
        for pending in &mut self.pending {
            pending.undo = apply::apply(&mut doc, &pending.transaction.ops, &|_| true)
                .ok()
                .map(|applied| applied.undo);
        }
    }
}
