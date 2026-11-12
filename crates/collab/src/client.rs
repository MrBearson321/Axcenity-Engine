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
use crate::live::LiveDoc;
use crate::message::{Have, MemberId, ToClient, ToHost};
use crate::op::{self, Op, Transaction};
use crate::presence::{self, ChatLine, Collaborator, Presence, TextCursor};
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
    /// The host didn't take a change to the live document at `path`; the
    /// copy here was opened again from the host's (C7).
    DocRefused { path: String, reason: String },
    /// About a checkpoint or restore this member asked for (C9).
    History(String),
}

/// A live document this member has open (C7).
struct ClientDoc {
    doc: LiveDoc,
    /// Has had the host's state.
    ready: bool,
    /// The host document's origin this copy merges with.
    origin: Option<u64>,
    /// This member typed something the file may not have yet.
    typed: bool,
}

impl ClientDoc {
    fn new() -> ClientDoc {
        let mut doc = LiveDoc::new();
        doc.track_own_edits();
        ClientDoc {
            doc,
            ready: false,
            origin: None,
            typed: false,
        }
    }
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
    /// Open live scripts and stylesheets, by path (C7).
    live: BTreeMap<String, ClientDoc>,
    /// Live documents others changed since [`Client::take_doc_changes`].
    live_changed: BTreeSet<String>,
    /// The session's chat, oldest first (C8).
    chat: VecDeque<ChatLine>,
    /// Others' chat lines that arrived since [`Client::take_new_chat`].
    new_chat: usize,
    /// When this member's last chat lines went, for the rate limit.
    chatted: VecDeque<std::time::Instant>,
    /// The session's checkpoints, newest first (C9).
    checkpoints: Vec<crate::history::Checkpoint>,
    /// The host's audit log as last sent (C10; the Owner's).
    audit: Vec<crate::audit::AuditEntry>,
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
            live: BTreeMap::new(),
            live_changed: BTreeSet::new(),
            chat: VecDeque::new(),
            new_chat: 0,
            chatted: VecDeque::new(),
            checkpoints: Vec::new(),
            audit: Vec::new(),
        }
    }

    /// Opens the live document of the script or stylesheet at `path`: its
    /// text arrives from the host ([`Client::doc_text`] once ready).
    pub fn open_doc(&mut self, path: &str) {
        if self.live.contains_key(path) {
            return;
        }
        self.live.insert(path.to_owned(), ClientDoc::new());
        if self.is_online() {
            self.outbox.push(ToHost::DocOpen {
                path: path.to_owned(),
                state_vector: Vec::new(),
            });
        }
    }

    /// Stops following the live document at `path`.
    pub fn close_doc(&mut self, path: &str) {
        self.live.remove(path);
        self.live_changed.remove(path);
    }

    /// The live document's text, once the host's has arrived.
    pub fn doc_text(&self, path: &str) -> Option<String> {
        let open = self.live.get(path)?;
        open.ready.then(|| open.doc.text())
    }

    /// This member typed: the document's text is now `text`. Sent at once
    /// when online; offline, it's kept and merged when the connection is
    /// back. False when the document isn't open and ready.
    pub fn edit_doc(&mut self, path: &str, text: &str) -> bool {
        let online = self.is_online();
        let Some(open) = self.live.get_mut(path) else {
            return false;
        };
        if !open.ready {
            return false;
        }
        if let Some(update) = open.doc.set_text(text) {
            open.typed = true;
            if online {
                self.outbox.push(ToHost::DocUpdate {
                    path: path.to_owned(),
                    update,
                });
            }
        }
        true
    }

    /// Undoes this member's last change to the live document at `path`
    /// (others' changes stay). False when there was nothing to undo.
    pub fn undo_doc(&mut self, path: &str) -> bool {
        self.undo_or_redo_doc(path, true)
    }

    /// Does again what [`Client::undo_doc`] last undid.
    pub fn redo_doc(&mut self, path: &str) -> bool {
        self.undo_or_redo_doc(path, false)
    }

    fn undo_or_redo_doc(&mut self, path: &str, undo: bool) -> bool {
        let online = self.is_online();
        let Some(open) = self.live.get_mut(path).filter(|open| open.ready) else {
            return false;
        };
        let update = if undo {
            open.doc.undo()
        } else {
            open.doc.redo()
        };
        let Some(update) = update else {
            return false;
        };
        open.typed = true;
        if online {
            self.outbox.push(ToHost::DocUpdate {
                path: path.to_owned(),
                update,
            });
        }
        true
    }

    /// This member's cursor in the live document at `path`, as presence
    /// carries it: the selection's ends as character indexes (C8).
    pub fn doc_cursor(&self, path: &str, anchor: usize, head: usize) -> Option<TextCursor> {
        let open = self.live.get(path).filter(|open| open.ready)?;
        Some(TextCursor {
            path: path.to_owned(),
            anchor: open.doc.sticky(anchor)?,
            head: open.doc.sticky(head)?,
        })
    }

    /// Where another member's `cursor` is in this copy of its document, as
    /// character indexes (anchor, head).
    pub fn resolve_cursor(&self, cursor: &TextCursor) -> Option<(usize, usize)> {
        let open = self.live.get(&cursor.path).filter(|open| open.ready)?;
        Some((
            open.doc.resolve(&cursor.anchor)?,
            open.doc.resolve(&cursor.head)?,
        ))
    }

    /// Sends a line to the session's chat. It shows (for everyone, this
    /// member too) once the host passes it on. Refused when offline, blank,
    /// or more than [`presence::CHAT_PER_SECOND`] lines a second.
    pub fn send_chat(&mut self, text: &str) -> Result<(), String> {
        if !self.is_online() {
            return Err("Not connected to the session".to_owned());
        }
        let Some(text) = presence::tidy_chat(text) else {
            return Err("Nothing to send".to_owned());
        };
        let now = std::time::Instant::now();
        while self
            .chatted
            .front()
            .is_some_and(|at| now.duration_since(*at) >= std::time::Duration::from_secs(1))
        {
            self.chatted.pop_front();
        }
        if self.chatted.len() >= presence::CHAT_PER_SECOND {
            return Err("Slow down a little".to_owned());
        }
        self.chatted.push_back(now);
        self.outbox.push(ToHost::Chat(text));
        Ok(())
    }

    /// The session's checkpoints, newest first (C9).
    pub fn checkpoints(&self) -> &[crate::history::Checkpoint] {
        &self.checkpoints
    }

    /// Asks the host to keep a checkpoint now. Refused when offline or for
    /// a Viewer.
    pub fn save_checkpoint(&mut self, label: &str) -> Result<(), String> {
        if !self.is_online() {
            return Err("Not connected to the session".to_owned());
        }
        if !self.role.can(crate::role::Capability::SaveCheckpoint) {
            return Err("Viewers can't save checkpoints".to_owned());
        }
        self.outbox.push(ToHost::Checkpoint(label.to_owned()));
        Ok(())
    }

    /// Asks the host to put the session back as checkpoint `id` was, for
    /// everyone. Only the Owner can.
    pub fn restore_checkpoint(&mut self, id: u64) -> Result<(), String> {
        if !self.is_online() {
            return Err("Not connected to the session".to_owned());
        }
        if self.role != Role::Owner {
            return Err("Only the owner can restore a checkpoint".to_owned());
        }
        self.outbox.push(ToHost::Restore(id));
        Ok(())
    }

    /// Asks the host for its audit log (the Owner only); it comes back
    /// through [`Client::audit_log`].
    pub fn ask_for_audit_log(&mut self) -> bool {
        if !self.is_online() || self.role != Role::Owner {
            return false;
        }
        self.outbox.push(ToHost::AuditLog);
        true
    }

    /// The host's audit log as last asked for, oldest first.
    pub fn audit_log(&self) -> &[crate::audit::AuditEntry] {
        &self.audit
    }

    /// The session's chat, oldest first.
    pub fn chat(&self) -> &VecDeque<ChatLine> {
        &self.chat
    }

    /// How many of others' chat lines arrived since last asked, for an
    /// unread count.
    pub fn take_new_chat(&mut self) -> usize {
        std::mem::take(&mut self.new_chat)
    }

    /// Live documents others changed since last asked.
    pub fn take_doc_changes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.live_changed).into_iter().collect()
    }

    /// Asks the host for each open live document again, with what this
    /// copy has (after joining).
    fn reopen_docs(&mut self) {
        for (path, open) in &self.live {
            self.outbox.push(ToHost::DocOpen {
                path: path.clone(),
                state_vector: open.doc.state_vector(),
            });
        }
    }

    fn doc_state(&mut self, path: String, update: &[u8], host_has: &[u8], origin: u64) {
        let Some(open) = self.live.get_mut(&path) else {
            return;
        };
        // The host lost the document this copy grew from (its saved state
        // was gone when it started again): the two can't merge, so this
        // copy starts over from the host's, with this member's text made
        // again on top if they typed something the file may lack.
        let mut retype = None;
        if open.origin.is_some_and(|had| had != origin) {
            retype = (open.ready && open.typed).then(|| open.doc.text());
            *open = ClientDoc::new();
        }
        if let Err(error) = open.doc.apply(update) {
            log::warn!("Collaborate: the host's copy of `{path}` didn't apply: {error}");
            return;
        }
        open.ready = true;
        open.origin = Some(origin);
        let retyped = retype.and_then(|text| open.doc.set_text(&text));
        if retyped.is_some() {
            open.typed = true;
        }
        // What this copy has that the host lacks (typed offline, or while
        // the host was away).
        let send = retyped.or_else(|| {
            open.doc
                .is_ahead_of(host_has)
                .then(|| open.doc.update_since(host_has))
        });
        if let Some(update) = send {
            self.outbox.push(ToHost::DocUpdate {
                path: path.clone(),
                update,
            });
        }
        self.live_changed.insert(path);
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
            ToClient::DocState {
                path,
                update,
                state_vector,
                origin,
            } => self.doc_state(path, &update, &state_vector, origin),
            ToClient::DocUpdate { path, update, .. } => {
                if let Some(open) = self.live.get_mut(&path) {
                    match open.doc.apply(&update) {
                        Ok(()) => {
                            self.live_changed.insert(path);
                        }
                        Err(error) => {
                            log::warn!("Collaborate: a change to `{path}` didn't apply: {error}")
                        }
                    }
                }
            }
            ToClient::Chat(line) => {
                if line.member != self.member {
                    self.new_chat += 1;
                }
                self.chat.push_back(line);
                while self.chat.len() > presence::CHAT_KEPT {
                    self.chat.pop_front();
                }
            }
            ToClient::ChatHistory(lines) => {
                let before = self.chat.len();
                self.chat = lines.into_iter().collect();
                let me = self.member;
                self.new_chat += self
                    .chat
                    .iter()
                    .skip(before)
                    .filter(|line| line.member != me)
                    .count();
            }
            ToClient::Checkpoints(list) => self.checkpoints = list,
            ToClient::HistoryNotice(text) => self.notices.push(Notice::History(text)),
            ToClient::AuditLog(entries) => self.audit = entries,
            ToClient::DocRefused { path, reason } => {
                if self.live.contains_key(&path) {
                    self.live.insert(path.clone(), ClientDoc::new());
                    if self.is_online() {
                        self.outbox.push(ToHost::DocOpen {
                            path: path.clone(),
                            state_vector: Vec::new(),
                        });
                    }
                }
                self.notices.push(Notice::DocRefused { path, reason });
            }
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
        // A file written with everything typed in its live document here.
        for op in &ops {
            if let Op::FileVersion {
                path,
                new: Some(new),
                ..
            } = op
                && let Some(open) = self.live.get_mut(path)
                && FileHash::of(open.doc.text().as_bytes()) == *new
            {
                open.typed = false;
            }
        }
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
        self.reopen_docs();
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
