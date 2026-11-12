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

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use axcenity_core::{InstanceId, World};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::apply::{self, DocMut, Graveyard, Note, Refusal, Undo};
use crate::capture;
use crate::files::{self, Assembly, Bytes, FileHash, Manifest, Piece};
use crate::ids::BLOCK_SIZE;
use crate::message::{Have, MemberId, ToClient, ToHost};
use crate::op::{self, Op, Transaction};
use crate::presence::{Collaborator, Presence};
use crate::role::{Capability, Role};

/// Something the editor should tell the user about, or act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The World was replaced with the session's (on joining, or after a
    /// resync): selections and the editor's own undo steps are stale.
    Joined,
    /// Their transaction applied, but not entirely as made (someone deleted
    /// what it edited, or the parent it used). `recovered` as for
    /// [`Notice::Refused`].
    Changed {
        client_seq: u64,
        label: String,
        notes: Vec<Note>,
        recovered: bool,
    },
    /// The host refused their transaction (or their role doesn't allow
    /// edits); it was taken off. `recovered` says it was sent again after
    /// the connection or the host came back, or from an editor's unsent
    /// edits ([`Client::recover_unsent`]): the editor lists those in its
    /// Recovered Changes window, as they're from a while ago.
    Refused {
        client_seq: u64,
        label: String,
        reason: Refusal,
        recovered: bool,
    },
    /// The copy disagreed with the host's, so it was replaced by a fresh
    /// snapshot (a bug if it ever happens; edits not yet confirmed are kept
    /// and sent again).
    Resynced,
    /// Another member connected.
    Arrived { member: MemberId, name: String },
    /// Another member's connection closed, or they were removed.
    Left { member: MemberId, name: String },
    /// The owner ended the session or quit: this copy stays as it is, read
    /// only, and edits not confirmed yet are in [`Client::unsent`] (TC6).
    HostLeft,
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
    /// Sent again after a gap ([`Notice::Refused`]'s `recovered`).
    recovered: bool,
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
    /// The newest of this member's transactions the host says it handled
    /// (from the last join message).
    host_handled: u64,
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
    /// The project's files as the host's order says, with this member's
    /// unconfirmed saves on top (as the World is).
    files: Manifest,
    /// The bytes of files this member's pending transactions publish, to
    /// upload again after a reconnect.
    uploads: HashMap<FileHash, Bytes>,
    /// Fetched files arriving in pieces.
    downloads: HashMap<FileHash, Assembly>,
    /// Fetched files that have arrived, for [`Client::take_files`].
    arrived: Vec<(FileHash, Bytes)>,
    /// Fetched files the host didn't have.
    missing: Vec<FileHash>,
    /// Who is editing which document.
    locks: BTreeMap<String, MemberId>,
    /// Documents this member asked to edit.
    my_locks: BTreeSet<String>,
    /// This member's transactions the host refused lately, newest last.
    refused: VecDeque<u64>,
    /// Names the session's project, once joined.
    session: Option<String>,
    /// The run of the host this copy's World and numbers are from.
    epoch: Option<u64>,
    /// The host said the session is over.
    host_left: bool,
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
            host_handled: 0,
            asked_for_ids: false,
            awaiting_snapshot: false,
            connection: Connection::Offline,
            outbox: Vec::new(),
            notices: Vec::new(),
            roster: Vec::new(),
            presences: FxHashMap::default(),
            presence: Presence::default(),
            files: Manifest::new(),
            uploads: HashMap::new(),
            downloads: HashMap::new(),
            arrived: Vec::new(),
            missing: Vec::new(),
            locks: BTreeMap::new(),
            my_locks: BTreeSet::new(),
            refused: VecDeque::new(),
            session: None,
            epoch: None,
            host_left: false,
        }
    }

    /// Names the session's project, once joined (empty if the host didn't
    /// name it).
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// True if the host refused transaction `client_seq` (among the last
    /// few hundred).
    pub fn was_refused(&self, client_seq: u64) -> bool {
        self.refused.contains(&client_seq)
    }

    fn note_refused(&mut self, client_seq: u64) {
        self.refused.push_back(client_seq);
        while self.refused.len() > 256 {
            self.refused.pop_front();
        }
    }

    /// The project's files, with this member's unconfirmed saves.
    pub fn files(&self) -> &Manifest {
        &self.files
    }

    /// True while transaction `client_seq` waits for the host.
    pub fn is_pending(&self, client_seq: u64) -> bool {
        self.pending
            .iter()
            .any(|p| p.transaction.client_seq == client_seq)
    }

    /// Asks the host for files by hash; they come back through
    /// [`Client::take_files`] (or [`Client::take_missing`]). False while
    /// offline (ask again later).
    pub fn fetch(&mut self, hashes: Vec<FileHash>) -> bool {
        if !self.is_online() || hashes.is_empty() {
            return false;
        }
        self.outbox.push(ToHost::Fetch(hashes));
        true
    }

    /// True when fetched files (or word of missing ones) wait for
    /// [`Client::take_files`].
    pub fn files_waiting(&self) -> bool {
        !self.arrived.is_empty() || !self.missing.is_empty()
    }

    /// The documents this member asked to edit.
    pub fn my_locks(&self) -> &BTreeSet<String> {
        &self.my_locks
    }

    /// Files fetched since the last call.
    pub fn take_files(&mut self) -> Vec<(FileHash, Bytes)> {
        std::mem::take(&mut self.arrived)
    }

    /// Fetched files the host no longer had, since the last call.
    pub fn take_missing(&mut self) -> Vec<FileHash> {
        std::mem::take(&mut self.missing)
    }

    /// Who is editing which document, as the host last said.
    pub fn locks(&self) -> &BTreeMap<String, MemberId> {
        &self.locks
    }

    /// Who is editing `path`, if anyone.
    pub fn lock_holder(&self, path: &str) -> Option<MemberId> {
        self.locks.get(path).copied()
    }

    /// True if this member may save `path` now: nobody else is editing it.
    pub fn may_save(&self, path: &str) -> bool {
        self.lock_holder(path).is_none_or(|m| m == self.member)
    }

    /// True if this member's role changes files like `path` and nobody
    /// else is editing it.
    pub fn can_change_file(&self, path: &str) -> bool {
        self.role.can(if files::is_script(path) {
            Capability::EditScripts
        } else {
            Capability::EditAssets
        }) && self.may_save(path)
    }

    /// Asks to be the one editing `path` (asked again after a reconnect
    /// until [`Client::unlock`]). [`Client::lock_holder`] says who got it.
    pub fn lock(&mut self, path: &str) {
        if self.my_locks.insert(path.to_owned()) && self.is_online() {
            self.outbox.push(ToHost::Lock {
                path: path.to_owned(),
            });
        }
    }

    /// Done editing `path`.
    pub fn unlock(&mut self, path: &str) {
        if self.my_locks.remove(path) && self.is_online() {
            self.outbox.push(ToHost::Unlock {
                path: path.to_owned(),
            });
        }
    }

    /// Publishes changed files (`None` deletes one), with any World edits
    /// not captured yet, as one transaction (one undo step): an import is
    /// its files and the model placed from them. Each file's base is its
    /// version in [`Client::files`]. Returns the transaction's number, or
    /// `None` if nothing changed.
    pub fn publish(
        &mut self,
        world: &mut World,
        label: impl Into<String>,
        changes: Vec<(String, Option<Bytes>)>,
    ) -> Result<Option<u64>, Refusal> {
        if !self.joined {
            return Err(Refusal::Invalid("not in a session yet".to_owned()));
        }
        if self.host_left {
            return Err(self.edit_refusal());
        }
        let mut ops = Vec::new();
        let mut uploads = Vec::new();
        for (path, bytes) in changes {
            files::check_path(&path).map_err(Refusal::Invalid)?;
            if !self.role.can(if files::is_script(&path) {
                Capability::EditScripts
            } else {
                Capability::EditAssets
            }) {
                return Err(Refusal::Permission);
            }
            if !self.may_save(&path) {
                return Err(Refusal::Locked {
                    by: self.lock_holder(&path).expect("may_save checked"),
                    path,
                });
            }
            let base = self.files.get(&path).copied();
            let new = bytes.as_ref().map(|b| FileHash::of(b));
            if base == new {
                continue;
            }
            if let (Some(hash), Some(bytes)) = (new, bytes) {
                uploads.push((hash, bytes));
            }
            ops.push(Op::FileVersion { path, base, new });
        }
        let label = label.into();
        if world.has_edits() {
            let captured = capture::capture(world, &self.graveyard, &mut self.unsent);
            if !captured.ops.is_empty() {
                if self.role.can(Capability::EditWorld) {
                    ops.extend(captured.ops);
                } else {
                    // `capture` put the World back as it was.
                    self.notices.push(Notice::Refused {
                        client_seq: 0,
                        label: label.clone(),
                        reason: Refusal::Permission,
                        recovered: false,
                    });
                }
            }
        }
        if ops.is_empty() {
            return Ok(None);
        }
        for (hash, bytes) in uploads {
            self.uploads.insert(hash, bytes);
            self.send_upload(hash);
        }
        let result = self.submit_ops(world, label, ops);
        self.ask_for_ids_if_low(world);
        if result.is_err() {
            self.forget_uploads();
        }
        result.map(Some)
    }

    fn send_upload(&mut self, hash: FileHash) {
        if !self.is_online() {
            return;
        }
        let Some(bytes) = self.uploads.get(&hash) else {
            return;
        };
        let total = bytes.len() as u64;
        for (at, piece) in files::pieces(bytes) {
            self.outbox.push(ToHost::Upload {
                hash,
                at,
                total,
                bytes: Piece(piece),
            });
        }
    }

    /// Keeps only the uploads pending transactions still publish.
    fn forget_uploads(&mut self) {
        let wanted: FxHashSet<FileHash> = self
            .pending
            .iter()
            .flat_map(|p| p.transaction.ops.iter())
            .filter_map(|op| match op {
                Op::FileVersion { new, .. } => *new,
                _ => None,
            })
            .collect();
        self.uploads.retain(|hash, _| wanted.contains(hash));
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

    /// True once the host said the session is over: edits are taken back.
    pub fn host_left(&self) -> bool {
        self.host_left
    }

    /// The edits the host hasn't confirmed, oldest first: what an editor
    /// keeps in its recovery file while offline (TC6).
    pub fn unsent(&self) -> Vec<Transaction> {
        self.pending.iter().map(|p| p.transaction.clone()).collect()
    }

    /// The newest edit not confirmed yet, if any: changes whenever
    /// [`Client::unsent`] does.
    pub fn newest_unsent(&self) -> Option<u64> {
        self.pending.back().map(|p| p.transaction.client_seq)
    }

    /// Takes in edits an earlier run of the editor made and never had
    /// confirmed (its recovery file): they apply here and go to the host
    /// like new ones, what they make with IDs from this member's blocks.
    /// One the host already handled is left out (C5); one that no longer
    /// applies is reported as a recovered refusal.
    /// Files are left out: the member's folder still has what they saved,
    /// and their [`Mirror`](crate::Mirror) sends it. Returns how many
    /// there were. Only once joined.
    pub fn recover_unsent(
        &mut self,
        world: &mut World,
        mut transactions: Vec<Transaction>,
    ) -> usize {
        for transaction in &mut transactions {
            transaction.ops.retain(Op::is_world);
        }
        // Ones the host already handled are in the session (C5).
        let handled = self.host_handled;
        transactions.retain(|t| !t.ops.is_empty() && t.client_seq > handled);
        if !self.joined || self.host_left || transactions.is_empty() {
            return 0;
        }
        self.capture(world, "Edit");
        let count = transactions.len();
        self.take_pending_off(world);
        self.adopt(world, transactions);
        self.put_pending_on(world);
        if self.is_online() {
            let start = self.pending.len() - count;
            for pending in self.pending.iter().skip(start) {
                self.outbox
                    .push(ToHost::Submit(pending.transaction.clone()));
            }
        }
        self.ask_for_ids_if_low(world);
        world.take_edits();
        count
    }

    /// Adds `transactions` as pending (off the copy), numbered after this
    /// client's own, with what they make moved to IDs from this member's
    /// blocks. They were made against IDs that aren't this member's now.
    fn adopt(&mut self, world: &mut World, transactions: Vec<Transaction>) {
        let mut map = FxHashMap::default();
        for transaction in &transactions {
            for op in &transaction.ops {
                if let Op::CreateTree { nodes } = op {
                    for node in nodes {
                        if !map.contains_key(&node.id)
                            && let Some(new) = world.take_id()
                        {
                            map.insert(node.id, new);
                        }
                    }
                }
            }
        }
        for mut transaction in transactions {
            op::remap_ids(&mut transaction.ops, &map);
            transaction.client_seq = self.next_client_seq;
            self.next_client_seq += 1;
            self.pending.push_back(Pending {
                transaction,
                undo: None,
                recovered: true,
            });
        }
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
        if self.host_left {
            return;
        }
        self.connection = Connection::Joining;
        self.asked_for_ids = false;
        self.awaiting_snapshot = false;
        let have = self.epoch.filter(|_| self.joined).map(|epoch| Have {
            epoch,
            seq: self.seq,
        });
        self.outbox.push(ToHost::Hello { have });
        if self.presence != Presence::default() {
            self.outbox.push(ToHost::Presence(self.presence.clone()));
        }
        for path in &self.my_locks {
            self.outbox.push(ToHost::Lock { path: path.clone() });
        }
    }

    /// The connection closed. Editing goes on; edits wait in `pending`
    /// until the next `connect`.
    pub fn disconnected(&mut self) {
        self.connection = Connection::Offline;
        self.outbox.clear();
        self.presences.clear();
        self.downloads.clear();
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
        if self.host_left || !self.role.can(Capability::EditWorld) {
            // `capture` already put the World back as it was.
            self.notices.push(Notice::Refused {
                client_seq: 0,
                label,
                reason: self.edit_refusal(),
                recovered: false,
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
                    recovered: false,
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
        if self.host_left || !self.role.can(Capability::EditWorld) {
            return Err(self.edit_refusal());
        }
        if !self.joined {
            return Err(Refusal::Invalid("not in a session yet".to_owned()));
        }
        self.capture(world, "Edit");
        self.submit_ops(world, label.into(), ops)
    }

    /// Why this member's edits don't go anywhere.
    fn edit_refusal(&self) -> Refusal {
        if self.host_left {
            Refusal::Invalid("the host left the session: this copy is read-only".to_owned())
        } else {
            Refusal::Permission
        }
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
            recovered: false,
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
                files,
                session,
                epoch,
            } => {
                self.files = files;
                self.session = Some(session);
                // The old copy is gone, and with it what undoes the pending
                // edits on it. Blocks of IDs not used yet stay this member's,
                // unless the host started again: its blocks start over.
                for pending in &mut self.pending {
                    pending.undo = None;
                }
                let restarted = self.epoch.is_some_and(|old| old != epoch);
                self.epoch = Some(epoch);
                let blocks = if restarted {
                    Vec::new()
                } else {
                    world.id_blocks_left()
                };
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
                if restarted {
                    // What this member made used IDs from blocks the new
                    // run of the host doesn't know: they get new ones, and
                    // every edit not confirmed is sent again, renumbered.
                    // Those the host already handled (it wrote them down
                    // just before it stopped, and the confirmation never
                    // left) are in the snapshot: sending them again would
                    // apply them twice (C5).
                    let unsent: Vec<Transaction> = self
                        .pending
                        .drain(..)
                        .map(|p| p.transaction)
                        .filter(|t| t.client_seq > last_client_seq)
                        .collect();
                    self.next_client_seq = self.next_client_seq.max(last_client_seq + 1);
                    self.adopt(world, unsent);
                }
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
                    self.note_refused(client_seq);
                    self.notices.push(Notice::Refused {
                        client_seq,
                        label: pending.transaction.label,
                        reason,
                        recovered: pending.recovered,
                    });
                }
                self.put_pending_on(world);
                self.forget_uploads();
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
            ToClient::File {
                hash,
                at,
                total,
                bytes,
            } => match Assembly::add(&mut self.downloads, hash, at, total, &bytes.0) {
                Ok(Some(file)) => self.arrived.push((hash, file)),
                Ok(None) => {}
                Err(()) => self.missing.push(hash),
            },
            ToClient::Missing(hash) => self.missing.push(hash),
            ToClient::Locks(locks) => self.locks = locks,
            ToClient::Ended => {
                self.host_left = true;
                self.connection = Connection::Offline;
                self.outbox.clear();
                self.presences.clear();
                self.downloads.clear();
                self.locks.clear();
                self.notices.push(Notice::HostLeft);
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
            let mut recovered = false;
            while let Some(front) = self.pending.front() {
                if front.transaction.client_seq > client_seq {
                    break;
                }
                recovered = front.recovered;
                self.pending.pop_front();
            }
            self.forget_uploads();
            if !notes.is_empty() {
                self.notices.push(Notice::Changed {
                    client_seq,
                    label,
                    notes,
                    recovered,
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
        self.host_handled = last_client_seq;
        while let Some(front) = self.pending.front() {
            if front.transaction.client_seq > last_client_seq {
                break;
            }
            let pending = self.pending.pop_front().expect("checked above");
            let client_seq = pending.transaction.client_seq;
            if let Some((_, reason)) = rejected.iter().find(|(seq, _)| *seq == client_seq) {
                self.note_refused(client_seq);
                self.notices.push(Notice::Refused {
                    client_seq,
                    label: pending.transaction.label,
                    reason: reason.clone(),
                    recovered: true,
                });
            }
        }
        // A new run of this editor starts counting at 1, but the host
        // drops numbers it has handled: carry on after them.
        self.next_client_seq = self.next_client_seq.max(last_client_seq + 1);
        // What's still pending was made before the gap and goes again.
        for pending in &mut self.pending {
            pending.recovered = true;
        }
        self.put_pending_on(world);
        self.connection = Connection::Online;
        // The files they publish go first: the host takes a save only once
        // it has the file.
        self.forget_uploads();
        let hashes: Vec<FileHash> = self.uploads.keys().copied().collect();
        for hash in hashes {
            self.send_upload(hash);
        }
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
            files: &mut self.files,
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
            files: &mut self.files,
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
            files: &mut self.files,
        };
        for pending in &mut self.pending {
            pending.undo = apply::apply(&mut doc, &pending.transaction.ops, &|_| true)
                .ok()
                .map(|applied| applied.undo);
        }
    }
}
