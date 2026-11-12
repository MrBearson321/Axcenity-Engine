//! The session host: the one place that orders, checks and applies every
//! member's transactions.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

use axcenity_core::World;

use crate::apply::{self, Doc, Note, Refusal};
use crate::audit::{Audit, AuditEntry, AuditKind};
use crate::files::{self, Assembly, FileHash, FileStore, Manifest, MemoryStore, Piece};
use crate::ids::{IdAllocator, IdBlock};
use crate::journal::{self, Handled, Journal};
use crate::limits::{self, Kind, Limits};
use crate::live::{self, LiveDoc};
use crate::message::{Have, MemberId, Snapshot, ToClient, ToHost};
use crate::op::Op;
use crate::presence::{self, ChatLine, Collaborator, Presence, Profile};
use crate::role::{Capability, Role};

mod history;

/// How many applied transactions the host keeps to catch a reconnecting
/// member up; one further behind gets a fresh snapshot instead.
pub const LOG_KEPT: usize = 10_000;

/// How many of a member's refusals the host remembers for when they
/// reconnect.
const REFUSALS_KEPT: usize = 64;

/// How long a document stays locked to a member whose connection dropped,
/// so a short drop doesn't hand their unsaved work to someone else.
pub const LOCK_GRACE: Duration = Duration::from_secs(60);

/// Who is editing a document.
#[derive(Clone, Copy, Debug)]
struct Lock {
    member: MemberId,
    /// When their connection dropped, if it has.
    away_since: Option<Instant>,
}

/// A message for one member.
#[derive(Clone, Debug)]
pub struct Outgoing {
    pub to: MemberId,
    pub message: ToClient,
}

/// One applied transaction, as the host keeps it.
#[derive(Clone, Debug)]
pub struct Logged {
    pub seq: u64,
    pub author: MemberId,
    pub client_seq: u64,
    pub label: String,
    pub ops: Vec<Op>,
    pub notes: Vec<Note>,
}

#[derive(Debug)]
struct Member {
    role: Role,
    blocks: Vec<IdBlock>,
    /// The newest transaction of theirs handled, accepted or refused.
    last_client_seq: u64,
    refused: VecDeque<(u64, Refusal)>,
    connected: bool,
    profile: Profile,
    /// What they're doing, while connected.
    presence: Presence,
    /// Files they're uploading, by hash.
    uploads: HashMap<FileHash, Assembly>,
    /// What they may still send of each limited kind (C10).
    limits: Limits,
    /// Files they asked for that wait for their download budget, the
    /// first one read already.
    fetching: VecDeque<FileHash>,
    fetch_head: Option<(FileHash, files::Bytes)>,
    /// When each kind of trouble they caused was last written down, so a
    /// flood is one line a minute in the audit log.
    noted: BTreeMap<&'static str, Instant>,
}

/// How often the same trouble from one member goes in the audit log.
const NOTE_EVERY: Duration = Duration::from_secs(60);

/// The authority of one Collaborate session.
pub struct Host {
    doc: Doc,
    seq: u64,
    log: VecDeque<Logged>,
    log_kept: usize,
    ids: IdAllocator,
    members: BTreeMap<MemberId, Member>,
    store: Box<dyn FileStore>,
    locks: BTreeMap<String, Lock>,
    /// The time as of the last [`Host::tick`].
    clock: Instant,
    /// Names this session's project, so each member's folder for it is
    /// told apart from folders of other sessions.
    session: String,
    /// Where accepted transactions are written before they're sent (TC6).
    journal: Option<Journal>,
    /// When the journal's snapshot was last written.
    snapshot_at: Instant,
    /// Names this run of the host ([`ToClient::Welcome`]).
    epoch: u64,
    /// The newest transaction number earlier runs handled from each
    /// member, from the journal's folder.
    handled: Handled,
    /// Open live scripts and stylesheets, by path (C7).
    live: BTreeMap<String, HostDoc>,
    /// The chat's last [`presence::CHAT_KEPT`] lines, in memory only (C8).
    chat: VecDeque<ChatLine>,
    /// The project's checkpoints (C9), kept while the journal is.
    history: Option<crate::history::History>,
    /// When the newest checkpoint was made, and as of which transaction.
    checkpointed: (Instant, u64),
    /// Who changed the session since the newest checkpoint.
    edited_since: std::collections::BTreeSet<MemberId>,
    /// Joins, refusals, roles and trouble, for the Owner (C10).
    audit: Audit,
    /// Members whose connection should close ([`Host::take_closing`]).
    closing: Vec<MemberId>,
    /// Each limited kind's budget ([`limits::budget`]; tests use others).
    budgets: limits::Budgets,
}

/// A live document the host keeps.
struct HostDoc {
    doc: LiveDoc,
    /// Its origin number ([`live::Saved::origin`]).
    origin: u64,
    /// When it last changed without being written back to its file, and
    /// who changed it.
    unwritten: Option<(Instant, MemberId)>,
    /// Who changed it last.
    editor: MemberId,
    /// Its file's hash when last written to it or opened from it.
    file_at: Option<FileHash>,
    /// It changed since it was last kept beside the journal.
    unsaved: bool,
}

/// A number no earlier run of a host had (as near as random gets).
fn new_epoch() -> u64 {
    use ring::rand::SecureRandom;
    let mut bytes = [0; 8];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system's random numbers");
    u64::from_le_bytes(bytes)
}

impl Host {
    /// Hosts `world` (the project as saved, or the owner's edit World).
    pub fn new(world: World) -> Host {
        let doc = Doc::new(world);
        let ids = IdAllocator::new(doc.world.next_unused_index());
        Host {
            doc,
            seq: 0,
            log: VecDeque::new(),
            log_kept: LOG_KEPT,
            ids,
            members: BTreeMap::new(),
            store: Box::new(MemoryStore::default()),
            locks: BTreeMap::new(),
            clock: Instant::now(),
            session: String::new(),
            journal: None,
            snapshot_at: Instant::now(),
            epoch: new_epoch(),
            handled: Handled::new(),
            live: BTreeMap::new(),
            chat: VecDeque::new(),
            history: None,
            checkpointed: (Instant::now(), 0),
            edited_since: Default::default(),
            audit: Audit::default(),
            closing: Vec::new(),
            budgets: limits::budget,
        }
    }

    /// Keeps a journal in the project at `project` (TC6): a snapshot of
    /// the World now, then each accepted transaction, so a host that stops
    /// without ending the session can be recovered ([`journal::recover`]).
    ///
    /// What earlier runs handled from each member is read back first, so a
    /// member re-sending edits this project's host already has (it wrote
    /// them down just before it stopped) doesn't apply them twice (C5).
    pub fn start_journal(&mut self, project: &std::path::Path) -> std::io::Result<()> {
        for (member, seq) in journal::handled(project) {
            journal::note_handled(&mut self.handled, member, seq);
        }
        for (member, m) in &mut self.members {
            m.last_client_seq = m
                .last_client_seq
                .max(self.handled.get(member).copied().unwrap_or(0));
        }
        for saved in journal::saved_docs(project) {
            self.carry_on_doc(saved);
        }
        let handled = self.handled();
        self.journal = Some(Journal::start(
            project,
            &self.doc.world,
            &self.doc.graveyard,
            self.seq,
            &handled,
        )?);
        self.snapshot_at = self.clock;
        self.start_history(project);
        Ok(())
    }

    /// Whether this host keeps a journal.
    pub fn has_journal(&self) -> bool {
        self.journal.is_some()
    }

    /// Writes a fresh snapshot now, when the journal has entries.
    pub fn snapshot(&mut self) {
        self.snapshot_at = self.clock;
        self.save_docs();
        let Some(journal) = &mut self.journal else {
            return;
        };
        if journal.since_snapshot() == 0 {
            return;
        }
        let handled = handled_by(&self.handled, &self.members);
        if let Err(error) =
            journal.snapshot(&self.doc.world, &self.doc.graveyard, self.seq, &handled)
        {
            log::error!("Collaborate: couldn't write the session's snapshot: {error}");
        }
    }

    /// The session ended cleanly: the journal and its snapshot go, as
    /// there's nothing to recover.
    pub fn end_journal(&mut self) {
        let handled = self.handled();
        if let Some(journal) = self.journal.take()
            && let Err(error) = journal.end(&handled)
        {
            log::warn!("Collaborate: couldn't remove the session's journal: {error}");
        }
    }

    /// Names the session's project (the same each time it's hosted).
    pub fn with_session(mut self, session: impl Into<String>) -> Host {
        self.session = session.into();
        self
    }

    /// Hosts the project's files too: `files` names them, `store` has
    /// their bytes.
    pub fn with_files(mut self, files: Manifest, store: Box<dyn FileStore>) -> Host {
        self.doc.files = files;
        self.store = store;
        self
    }

    /// The project's files as the session has them now.
    pub fn files(&self) -> &Manifest {
        &self.doc.files
    }

    /// The bytes of a file the host has.
    pub fn read_file(&self, hash: FileHash) -> Option<files::Bytes> {
        self.store.read(hash)
    }

    /// Who is editing which document.
    pub fn locks(&self) -> BTreeMap<String, MemberId> {
        self.locks
            .iter()
            .map(|(path, lock)| (path.clone(), lock.member))
            .collect()
    }

    /// Lets time pass: a lock whose holder has been gone for
    /// [`LOCK_GRACE`] ends. Call it a few times a second.
    pub fn tick(&mut self, now: Instant) -> Vec<Outgoing> {
        self.clock = now;
        let mut out = self.write_back(now, false);
        let waiting: Vec<MemberId> = self
            .members
            .iter()
            .filter(|(_, m)| m.connected && !m.fetching.is_empty())
            .map(|(&member, _)| member)
            .collect();
        for member in waiting {
            out.extend(self.serve_fetches(member, now));
        }
        out.extend(self.hourly_checkpoint(now));
        if now.saturating_duration_since(self.snapshot_at) >= journal::SNAPSHOT_EVERY {
            self.snapshot();
        }
        let before = self.locks.len();
        self.locks.retain(|_, lock| {
            lock.away_since
                .is_none_or(|since| now.saturating_duration_since(since) < LOCK_GRACE)
        });
        if self.locks.len() != before {
            out.extend(self.locks_to_everyone());
        }
        out
    }

    /// Uses `budgets` for members added from now on (tests).
    #[cfg(test)]
    pub(crate) fn with_budgets(mut self, budgets: limits::Budgets) -> Host {
        self.budgets = budgets;
        self
    }

    /// Keeps only `kept` transactions for catching up (tests use a small one
    /// to exercise the snapshot path).
    pub fn with_log_kept(mut self, kept: usize) -> Host {
        self.log_kept = kept.max(1);
        self
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn world(&self) -> &World {
        &self.doc.world
    }

    /// The number of the last applied transaction.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Names this run of the host: a host started again has another.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The owner ended the session: tells everyone connected (TC6).
    pub fn end(&mut self) -> Vec<Outgoing> {
        // Typing not written yet goes to the files before everyone leaves.
        let mut out = self.write_back_now();
        out.extend(self.to_everyone(ToClient::Ended));
        out
    }

    /// The transactions still kept, oldest first.
    pub fn log(&self) -> impl Iterator<Item = &Logged> {
        self.log.iter()
    }

    /// The newest transaction number handled from each member, by this
    /// run and the ones before it whose journal this one read.
    pub fn handled(&self) -> Handled {
        handled_by(&self.handled, &self.members)
    }

    /// Lets `member` join with `role`. (TC3 does this when an invite is
    /// redeemed by a signed-in identity.)
    pub fn add_member(&mut self, member: MemberId, role: Role) {
        self.members.insert(
            member,
            Member {
                role,
                blocks: Vec::new(),
                last_client_seq: self.handled.get(&member).copied().unwrap_or(0),
                refused: VecDeque::new(),
                connected: false,
                profile: Profile::unnamed(member),
                presence: Presence::default(),
                uploads: HashMap::new(),
                limits: Limits::new(self.budgets),
                fetching: VecDeque::new(),
                fetch_head: None,
                noted: BTreeMap::new(),
            },
        );
    }

    /// Sets a member's name and colour (TC3 does this as they sign in).
    pub fn set_profile(&mut self, member: MemberId, profile: Profile) -> Vec<Outgoing> {
        let Some(m) = self.members.get_mut(&member) else {
            return Vec::new();
        };
        let profile = profile.tidied();
        if m.profile == profile {
            return Vec::new();
        }
        m.profile = profile;
        self.roster_to_everyone()
    }

    /// Takes a member out: their transactions are no longer taken.
    pub fn remove_member(&mut self, member: MemberId) -> Vec<Outgoing> {
        let Some(m) = self.members.remove(&member) else {
            return Vec::new();
        };
        let text = format!("{} was taken out of the project", m.profile.name);
        self.audit.record(AuditKind::Removed, Some(member), text);
        let mut out = self.roster_to_everyone();
        let before = self.locks.len();
        self.locks.retain(|_, lock| lock.member != member);
        if self.locks.len() != before {
            out.extend(self.locks_to_everyone());
        }
        out
    }

    /// Everyone in the session, by member ID.
    pub fn roster(&self) -> Vec<Collaborator> {
        self.members
            .iter()
            .map(|(&member, m)| Collaborator {
                member,
                profile: m.profile.clone(),
                role: m.role,
                online: m.connected,
            })
            .collect()
    }

    /// What `member` is doing now.
    pub fn presence(&self, member: MemberId) -> Option<&Presence> {
        self.members.get(&member).map(|m| &m.presence)
    }

    fn roster_to_everyone(&self) -> Vec<Outgoing> {
        self.to_everyone(ToClient::Roster(self.roster()))
    }

    pub fn role(&self, member: MemberId) -> Option<Role> {
        self.members.get(&member).map(|m| m.role)
    }

    /// Changes a member's role. It counts from their next transaction.
    pub fn set_role(&mut self, member: MemberId, role: Role) -> Vec<Outgoing> {
        let Some(m) = self.members.get_mut(&member) else {
            return Vec::new();
        };
        if m.role != role {
            let text = format!("{} is now {role:?} (was {:?})", m.profile.name, m.role);
            self.audit.record(AuditKind::Role, Some(member), text);
        }
        let m = self.members.get_mut(&member).expect("found above");
        m.role = role;
        let mut out = Vec::new();
        if m.connected {
            out.push(Outgoing {
                to: member,
                message: ToClient::RoleChanged(role),
            });
        }
        out.extend(self.roster_to_everyone());
        out
    }

    /// The member's connection closed. What they send after reconnecting
    /// starts with `Hello`.
    pub fn disconnected(&mut self, member: MemberId) -> Vec<Outgoing> {
        let Some(m) = self.members.get_mut(&member) else {
            return Vec::new();
        };
        if !m.connected {
            return Vec::new();
        }
        m.connected = false;
        m.presence = Presence::default();
        m.uploads.clear();
        m.fetching.clear();
        m.fetch_head = None;
        let now = self.clock;
        for lock in self.locks.values_mut() {
            if lock.member == member {
                lock.away_since = Some(now);
            }
        }
        self.roster_to_everyone()
    }

    /// Handles one message from `member`. Messages from someone who isn't
    /// a member are ignored.
    pub fn receive(&mut self, member: MemberId, message: ToHost) -> Vec<Outgoing> {
        if !self.members.contains_key(&member) {
            return Vec::new();
        }
        // The host's clock moves on with `tick`; budgets also follow the
        // real one, so a host that isn't ticked still refills them.
        let now = self.clock.max(Instant::now());
        let kind = match &message {
            ToHost::Hello { .. } => Some(Kind::Hello),
            ToHost::NeedIds => Some(Kind::Ids),
            ToHost::Presence(_) => Some(Kind::Presence),
            ToHost::Lock { .. } | ToHost::Unlock { .. } => Some(Kind::Lock),
            ToHost::DocOpen { .. } => Some(Kind::DocOpen),
            ToHost::Chat(_) => Some(Kind::Chat),
            ToHost::Checkpoint(_) => Some(Kind::Checkpoint),
            ToHost::Restore(_) => Some(Kind::Restore),
            _ => None,
        };
        if let Some(kind) = kind
            && !self.within(member, kind, 1.0, now)
        {
            return match kind {
                Kind::Checkpoint | Kind::Restore => self.history_notice(
                    member,
                    "That was asked for too often: wait a few seconds and try again",
                ),
                // A hello can't be dropped (the member would wait for an
                // answer forever): their connection closes instead, and
                // they reconnect after their back-off.
                Kind::Hello => {
                    self.closing.push(member);
                    Vec::new()
                }
                _ => Vec::new(),
            };
        }
        match message {
            ToHost::Hello { have } => self.hello(member, have),
            ToHost::Submit(transaction) => {
                let m = self.members.get_mut(&member).expect("checked above");
                if !m.connected || transaction.client_seq <= m.last_client_seq {
                    // Not joined yet, or a resend of one already handled.
                    return Vec::new();
                }
                m.last_client_seq = transaction.client_seq;
                let role = m.role;
                if let Err(reason) = self.check_files(member, role, &transaction.ops) {
                    if reason == Refusal::Permission {
                        self.denied(member, "an edit", now);
                    }
                    return self.refuse(member, transaction.client_seq, reason);
                }
                let m = self.members.get_mut(&member).expect("checked above");
                let blocks = m.blocks.clone();
                let owns = |id| blocks.iter().any(|block: &IdBlock| block.contains(id));
                match apply::apply(&mut self.doc.view(), &transaction.ops, &owns) {
                    Ok(applied) => {
                        let saved: Vec<String> = applied
                            .ops
                            .iter()
                            .filter_map(|op| match op {
                                Op::FileVersion { path, .. } => Some(path.clone()),
                                _ => None,
                            })
                            .collect();
                        let mut out =
                            self.commit(member, transaction.client_seq, transaction.label, applied);
                        // A live document's file saved some other way (an
                        // older editor, or an edit outside the session):
                        // the document takes the file's text as a change.
                        for path in saved {
                            out.extend(self.doc_follows_file(&path, member));
                        }
                        out
                    }
                    Err(reason) => self.refuse(member, transaction.client_seq, reason),
                }
            }
            ToHost::Presence(presence) => {
                let m = self.members.get_mut(&member).expect("checked above");
                if !m.connected {
                    return Vec::new();
                }
                let presence = presence.tidied();
                if m.presence == presence {
                    return Vec::new();
                }
                m.presence = presence.clone();
                self.members
                    .iter()
                    .filter(|&(&to, m)| m.connected && to != member)
                    .map(|(&to, _)| Outgoing {
                        to,
                        message: ToClient::Presence {
                            member,
                            presence: presence.clone(),
                        },
                    })
                    .collect()
            }
            ToHost::Upload {
                hash,
                at,
                total,
                bytes,
            } => {
                let m = self.members.get_mut(&member).expect("checked above");
                if !m.connected || self.store.has(hash) {
                    return Vec::new();
                }
                if !may_change_files(m.role) {
                    self.denied(member, "a file upload", now);
                    return Vec::new();
                }
                if at == 0 && m.uploads.len() >= limits::UPLOADS_AT_ONCE {
                    log::warn!(
                        "Collaborate: member {} started too many uploads at once; one was dropped",
                        member.0
                    );
                    return Vec::new();
                }
                match Assembly::add(&mut m.uploads, hash, at, total, &bytes.0) {
                    Ok(Some(file)) => self.store.keep(hash, file),
                    Ok(None) => {}
                    Err(()) => log::warn!(
                        "Collaborate: member {} sent a file that doesn't fit together",
                        member.0
                    ),
                }
                Vec::new()
            }
            ToHost::Fetch(hashes) => {
                let m = self.members.get_mut(&member).expect("checked above");
                if !m.connected {
                    return Vec::new();
                }
                let room = limits::FETCH_QUEUED_MOST.saturating_sub(m.fetching.len());
                if hashes.len() > room {
                    log::warn!(
                        "Collaborate: member {} asked for too many files at once; {} weren't sent",
                        member.0,
                        hashes.len() - room
                    );
                }
                m.fetching.extend(hashes.into_iter().take(room));
                self.serve_fetches(member, now)
            }
            ToHost::Lock { path } => {
                let m = &self.members[&member];
                if !m.connected || files::check_path(&path).is_err() {
                    return Vec::new();
                }
                if !may_edit_file(m.role, &path) {
                    self.denied(member, "a document lock", now);
                    return Vec::new();
                }
                match self.locks.get(&path) {
                    Some(lock) if lock.member == member => Vec::new(),
                    // Taken: tell the asker who has it.
                    Some(_) => vec![Outgoing {
                        to: member,
                        message: ToClient::Locks(self.locks()),
                    }],
                    None => {
                        self.locks.insert(
                            path,
                            Lock {
                                member,
                                away_since: None,
                            },
                        );
                        self.locks_to_everyone()
                    }
                }
            }
            ToHost::Unlock { path } => {
                if self
                    .locks
                    .get(&path)
                    .is_some_and(|lock| lock.member == member)
                {
                    self.locks.remove(&path);
                    return self.locks_to_everyone();
                }
                Vec::new()
            }
            ToHost::DocOpen { path, state_vector } => {
                if !self.members[&member].connected {
                    return Vec::new();
                }
                self.open_doc(member, path, &state_vector)
            }
            ToHost::DocUpdate { path, update } => {
                if !self.members[&member].connected {
                    return Vec::new();
                }
                self.update_doc(member, path, update)
            }
            ToHost::Chat(text) => self.chat(member, &text),
            ToHost::Checkpoint(label) => self.save_checkpoint(member, &label),
            ToHost::Restore(id) => self.restore(member, id),
            ToHost::NeedIds => {
                let m = &self.members[&member];
                if !m.connected {
                    return Vec::new();
                }
                if !m.role.can(Capability::EditWorld) {
                    self.denied(member, "new Instance IDs", now);
                    return Vec::new();
                }
                let Some(block) = self.ids.block() else {
                    log::error!("Collaborate: the session has run out of Instance IDs");
                    return Vec::new();
                };
                let m = self.members.get_mut(&member).expect("checked above");
                m.blocks.push(block);
                vec![Outgoing {
                    to: member,
                    message: ToClient::Ids(block),
                }]
            }
            ToHost::AuditLog => {
                let m = &self.members[&member];
                if !m.connected {
                    return Vec::new();
                }
                if m.role != Role::Owner {
                    self.denied(member, "the audit log", now);
                    return Vec::new();
                }
                vec![Outgoing {
                    to: member,
                    message: ToClient::AuditLog(self.audit_log()),
                }]
            }
        }
    }

    /// Whether `member` (with `role`) may make the transaction `ops`:
    /// each op needs its capability, each new file must have been uploaded,
    /// and nobody else may be editing a file it changes.
    fn check_files(&self, member: MemberId, role: Role, ops: &[Op]) -> Result<(), Refusal> {
        for op in ops {
            let Op::FileVersion { path, new, .. } = op else {
                if !role.can(Capability::EditWorld) {
                    return Err(Refusal::Permission);
                }
                continue;
            };
            if !may_edit_file(role, path) {
                return Err(Refusal::Permission);
            }
            if let Some(lock) = self.locks.get(path)
                && lock.member != member
            {
                return Err(Refusal::Locked {
                    path: path.clone(),
                    by: lock.member,
                });
            }
            if let Some(hash) = new
                && !self.store.has(*hash)
            {
                return Err(Refusal::Invalid(format!(
                    "`{path}` wasn't uploaded before it was saved"
                )));
            }
        }
        Ok(())
    }

    fn locks_to_everyone(&self) -> Vec<Outgoing> {
        self.to_everyone(ToClient::Locks(self.locks()))
    }

    fn hello(&mut self, member: MemberId, have: Option<Have>) -> Vec<Outgoing> {
        for lock in self.locks.values_mut() {
            if lock.member == member {
                lock.away_since = None;
            }
        }
        let mut out = self.catch_up(member, have);
        out.push(Outgoing {
            to: member,
            message: ToClient::Locks(self.locks()),
        });
        // Who's here, to everyone (the member is online now), and what the
        // others are doing, to the member.
        out.extend(self.roster_to_everyone());
        if !self.chat.is_empty() {
            out.push(Outgoing {
                to: member,
                message: ToClient::ChatHistory(self.chat.iter().cloned().collect()),
            });
        }
        if let Some(list) = self.checkpoints() {
            out.push(Outgoing {
                to: member,
                message: ToClient::Checkpoints(list),
            });
        }
        for (&other, m) in &self.members {
            if other != member && m.connected && m.presence != Presence::default() {
                out.push(Outgoing {
                    to: member,
                    message: ToClient::Presence {
                        member: other,
                        presence: m.presence.clone(),
                    },
                });
            }
        }
        out
    }

    fn catch_up(&mut self, member: MemberId, have: Option<Have>) -> Vec<Outgoing> {
        let first_kept = self.log.front().map_or(self.seq + 1, |logged| logged.seq);
        // A copy from another run of the host numbers another history.
        let have = have
            .filter(|have| have.epoch == self.epoch)
            .map(|have| have.seq);
        let can_catch_up = have.is_some_and(|have| have <= self.seq && have + 1 >= first_kept);
        let m = self.members.get_mut(&member).expect("checked by receive");
        m.connected = true;
        let role = m.role;
        let last_client_seq = m.last_client_seq;
        let rejected: Vec<_> = m.refused.iter().cloned().collect();
        if can_catch_up {
            let have = have.expect("checked above");
            let mut out: Vec<Outgoing> = self
                .log
                .iter()
                .filter(|logged| logged.seq > have)
                .map(|logged| Outgoing {
                    to: member,
                    message: accepted(logged),
                })
                .collect();
            out.push(Outgoing {
                to: member,
                message: ToClient::Resumed {
                    last_client_seq,
                    role,
                    rejected,
                },
            });
            return out;
        }
        let block = self.ids.block();
        let m = self.members.get_mut(&member).expect("checked by receive");
        m.blocks.extend(block);
        vec![Outgoing {
            to: member,
            message: ToClient::Welcome {
                world: Snapshot(Box::new(self.doc.world.clone())),
                graveyard: self.doc.graveyard.clone(),
                seq: self.seq,
                last_client_seq,
                role,
                ids: block,
                rejected,
                files: self.doc.files.clone(),
                session: self.session.clone(),
                epoch: self.epoch,
            },
        }]
    }

    /// `Chat`: any connected member, Viewers too, within the chat budget
    /// ([`limits::budget`]); tidied, kept and sent
    /// to everyone (the sender too, so everyone sees the host's order).
    fn chat(&mut self, member: MemberId, text: &str) -> Vec<Outgoing> {
        if !self.members[&member].connected {
            return Vec::new();
        }
        let Some(text) = presence::tidy_chat(text) else {
            return Vec::new();
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let line = ChatLine { member, at, text };
        self.chat.push_back(line.clone());
        while self.chat.len() > presence::CHAT_KEPT {
            self.chat.pop_front();
        }
        self.to_everyone(ToClient::Chat(line))
    }

    /// Numbers, journals and logs an applied transaction, and tells
    /// everyone.
    fn commit(
        &mut self,
        author: MemberId,
        client_seq: u64,
        label: String,
        applied: apply::Applied,
    ) -> Vec<Outgoing> {
        self.seq += 1;
        if !applied.ops.is_empty() {
            self.edited_since.insert(author);
        }
        let logged = Logged {
            seq: self.seq,
            author,
            client_seq,
            label,
            ops: applied.ops,
            notes: applied.notes,
        };
        let message = accepted(&logged);
        // Written down before anyone hears of it.
        if let Some(journal) = &mut self.journal
            && let Err(error) = journal.append(&logged)
        {
            log::error!("Collaborate: couldn't write the journal: {error}");
        }
        self.log.push_back(logged);
        while self.log.len() > self.log_kept {
            self.log.pop_front();
        }
        self.to_everyone(message)
    }

    /// The text of the project file at `path`, if it has one that reads as
    /// text.
    fn file_text(&self, path: &str) -> Option<String> {
        let hash = self.doc.files.get(path)?;
        let bytes = self.store.read(*hash)?;
        String::from_utf8(bytes.to_vec()).ok()
    }

    /// `DocOpen`: opens the document (from its file the first time) and
    /// sends the member what their copy lacks.
    fn open_doc(&mut self, member: MemberId, path: String, seen: &[u8]) -> Vec<Outgoing> {
        if let Err(reason) = files::check_path(&path).and_then(|()| {
            live::is_live(&path)
                .then_some(())
                .ok_or_else(|| format!("`{path}` isn't a script or stylesheet"))
        }) {
            return vec![Outgoing {
                to: member,
                message: ToClient::DocRefused { path, reason },
            }];
        }
        if !self.live.contains_key(&path) {
            let text = self.file_text(&path).unwrap_or_default();
            self.live.insert(
                path.clone(),
                HostDoc {
                    doc: LiveDoc::with_text(&text),
                    origin: new_epoch(),
                    unwritten: None,
                    editor: member,
                    file_at: self.doc.files.get(&path).copied(),
                    unsaved: true,
                },
            );
        }
        let open = &self.live[&path];
        vec![Outgoing {
            to: member,
            message: ToClient::DocState {
                update: open.doc.update_since(seen),
                state_vector: open.doc.state_vector(),
                origin: open.origin,
                path,
            },
        }]
    }

    /// `DocUpdate`: checks and applies a member's change, and passes it on.
    fn update_doc(&mut self, member: MemberId, path: String, update: Vec<u8>) -> Vec<Outgoing> {
        let role = self.members[&member].role;
        let refused = |reason: String| {
            vec![Outgoing {
                to: member,
                message: ToClient::DocRefused {
                    path: path.clone(),
                    reason,
                },
            }]
        };
        if !may_edit_file(role, &path) {
            self.denied(member, "a live document edit", Instant::now());
            return refused(Refusal::Permission.to_string());
        }
        let now = self.clock;
        let Some(open) = self.live.get_mut(&path) else {
            return refused("open the document first".to_owned());
        };
        if let Err(error) = open.doc.apply(&update) {
            return refused(error.to_string());
        }
        open.unwritten = Some((now, member));
        open.editor = member;
        open.unsaved = true;
        self.members
            .iter()
            .filter(|(to, m)| m.connected && **to != member)
            .map(|(&to, _)| Outgoing {
                to,
                message: ToClient::DocUpdate {
                    path: path.clone(),
                    update: update.clone(),
                    author: member,
                },
            })
            .collect()
    }

    /// After `path`'s file changed by a save: an open document whose text
    /// differs takes the file's text, as `author`'s change.
    fn doc_follows_file(&mut self, path: &str, author: MemberId) -> Vec<Outgoing> {
        if !self.live.contains_key(path) {
            return Vec::new();
        }
        let text = self.file_text(path).unwrap_or_default();
        let open = self.live.get_mut(path).expect("checked above");
        open.file_at = self.doc.files.get(path).copied();
        let Some(update) = open.doc.set_text(&text) else {
            return Vec::new();
        };
        open.unwritten = None;
        open.editor = author;
        open.unsaved = true;
        self.to_everyone(ToClient::DocUpdate {
            path: path.to_owned(),
            update,
            author,
        })
    }

    /// Writes live documents whose typing settled back to their files, as
    /// ordinary file versions by whoever changed them last.
    fn write_back(&mut self, now: Instant, all: bool) -> Vec<Outgoing> {
        let due: Vec<(String, MemberId)> = self
            .live
            .iter()
            .filter_map(|(path, open)| {
                let (since, who) = open.unwritten?;
                (all || now.saturating_duration_since(since) >= live::WRITE_BACK_AFTER)
                    .then(|| (path.clone(), who))
            })
            .collect();
        let mut out = Vec::new();
        let wrote = !due.is_empty();
        for (path, who) in due {
            let open = self.live.get_mut(&path).expect("listed above");
            open.unwritten = None;
            let bytes: crate::files::Bytes = open.doc.text().into_bytes().into();
            let hash = FileHash::of(&bytes);
            open.file_at = Some(hash);
            let base = self.doc.files.get(&path).copied();
            if base == Some(hash) {
                continue;
            }
            self.store.keep(hash, bytes);
            let ops = vec![Op::FileVersion {
                path: path.clone(),
                base,
                new: Some(hash),
            }];
            match apply::apply(&mut self.doc.view(), &ops, &|_| true) {
                Ok(applied) => {
                    let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                    // Number 0: not one of the member's own, so their
                    // editor doesn't take it for a confirmation.
                    out.extend(self.commit(who, 0, format!("Edit {name}"), applied));
                }
                Err(reason) => {
                    log::warn!("Collaborate: couldn't write `{path}` back: {reason}");
                }
            }
        }
        if wrote {
            self.save_docs();
        }
        out
    }

    /// Keeps each live document that changed beside the journal.
    fn save_docs(&mut self) {
        let Some(journal) = &self.journal else {
            return;
        };
        for (path, open) in &mut self.live {
            if !open.unsaved {
                continue;
            }
            let saved = live::Saved {
                path: path.clone(),
                origin: open.origin,
                file_at: open.file_at,
                editor: open.editor,
                state: open.doc.state(),
            };
            match journal.save_doc(&saved) {
                Ok(()) => open.unsaved = false,
                Err(error) => log::error!("Collaborate: couldn't keep `{path}`: {error}"),
            }
        }
    }

    /// A live document an earlier run kept: this run carries on with it.
    /// When its file is as it was, typing not written yet is written soon;
    /// when the file was saved some other way since, the document takes
    /// the file's text. One whose file is gone is dropped.
    fn carry_on_doc(&mut self, saved: live::Saved) {
        let Some(&file) = self.doc.files.get(&saved.path) else {
            return;
        };
        let doc = match LiveDoc::from_state(&saved.state) {
            Ok(doc) => doc,
            Err(error) => {
                log::warn!(
                    "Collaborate: `{}` kept from the last run didn't read: {error}",
                    saved.path
                );
                return;
            }
        };
        let mut open = HostDoc {
            doc,
            origin: saved.origin,
            unwritten: None,
            editor: saved.editor,
            file_at: Some(file),
            unsaved: false,
        };
        if saved.file_at == Some(file) {
            if FileHash::of(open.doc.text().as_bytes()) != file {
                open.unwritten = Some((self.clock, saved.editor));
            }
        } else if let Some(text) = self.file_text(&saved.path) {
            open.unsaved = open.doc.set_text(&text).is_some();
        }
        self.live.insert(saved.path, open);
    }

    /// Writes every live document with changes back to its file now (on
    /// save, and before the session ends).
    pub fn write_back_now(&mut self) -> Vec<Outgoing> {
        let now = self.clock;
        self.write_back(now, true)
    }

    /// The text of the open live document at `path` (tests, the server's
    /// status).
    pub fn doc_text(&self, path: &str) -> Option<String> {
        self.live.get(path).map(|open| open.doc.text())
    }

    /// Takes `cost` from `member`'s budget for `kind`; false, noted in the
    /// log and the audit log (once a minute), when it's spent.
    fn within(&mut self, member: MemberId, kind: Kind, cost: f64, now: Instant) -> bool {
        let m = self.members.get_mut(&member).expect("checked by receive");
        if m.limits.take(kind, cost, now) {
            return true;
        }
        let name = m.profile.name.clone();
        log::warn!(
            "Collaborate: {name} (member {}) is sending too much ({}); some was dropped",
            member.0,
            kind.name()
        );
        self.note(
            member,
            kind.name(),
            AuditKind::Limited,
            format!("{name} sent too much ({}); some was dropped", kind.name()),
            now,
        );
        false
    }

    /// `member` asked for `what`, which their role doesn't allow.
    fn denied(&mut self, member: MemberId, what: &'static str, now: Instant) {
        let m = &self.members[&member];
        let text = format!(
            "{} ({:?}) asked for {what}, which their role doesn't allow",
            m.profile.name, m.role
        );
        self.note(member, what, AuditKind::Denied, text, now);
    }

    /// Writes `text` in the audit log unless `member` caused the same
    /// trouble (`key`) less than a minute ago.
    fn note(
        &mut self,
        member: MemberId,
        key: &'static str,
        kind: AuditKind,
        text: String,
        now: Instant,
    ) {
        let m = self.members.get_mut(&member).expect("checked by receive");
        if m.noted
            .get(key)
            .is_some_and(|&at| now.saturating_duration_since(at) < NOTE_EVERY)
        {
            return;
        }
        m.noted.insert(key, now);
        self.audit.record(kind, Some(member), text);
    }

    /// Sends `member` the files they asked for, as far as their download
    /// budget goes; the rest wait for [`Host::tick`].
    fn serve_fetches(&mut self, member: MemberId, now: Instant) -> Vec<Outgoing> {
        let mut out = Vec::new();
        let m = self.members.get_mut(&member).expect("checked by caller");
        while let Some(&hash) = m.fetching.front() {
            let bytes = match m.fetch_head.take() {
                Some((head, bytes)) if head == hash => Some(bytes),
                _ => self.store.read(hash),
            };
            let Some(bytes) = bytes else {
                m.fetching.pop_front();
                out.push(Outgoing {
                    to: member,
                    message: ToClient::Missing(hash),
                });
                continue;
            };
            let cost = bytes.len().max(1) as f64;
            if !m.limits.take(Kind::FetchBytes, cost, now) {
                m.fetch_head = Some((hash, bytes));
                break;
            }
            m.fetching.pop_front();
            let total = bytes.len() as u64;
            out.extend(files::pieces(&bytes).map(|(at, piece)| Outgoing {
                to: member,
                message: ToClient::File {
                    hash,
                    at,
                    total,
                    bytes: Piece(piece),
                },
            }));
        }
        out
    }

    /// Members whose connections the session should close now (they went
    /// over a budget that can't just drop messages).
    pub fn take_closing(&mut self) -> Vec<MemberId> {
        std::mem::take(&mut self.closing)
    }

    /// The audit log, oldest first.
    pub fn audit_log(&self) -> Vec<AuditEntry> {
        self.audit.entries().cloned().collect()
    }

    /// Writes the audit log to `file` too (beside the members file).
    pub fn keep_audit_in(&mut self, file: std::path::PathBuf) {
        self.audit.keep_in(file);
    }

    /// Records something only the session knows (joins, refusals, invites).
    pub fn record(&mut self, kind: AuditKind, member: Option<MemberId>, text: impl Into<String>) {
        self.audit.record(kind, member, text);
    }

    fn refuse(&mut self, member: MemberId, client_seq: u64, reason: Refusal) -> Vec<Outgoing> {
        let m = self.members.get_mut(&member).expect("checked by receive");
        m.refused.push_back((client_seq, reason.clone()));
        while m.refused.len() > REFUSALS_KEPT {
            m.refused.pop_front();
        }
        vec![Outgoing {
            to: member,
            message: ToClient::Rejected { client_seq, reason },
        }]
    }

    fn to_everyone(&self, message: ToClient) -> Vec<Outgoing> {
        self.members
            .iter()
            .filter(|(_, m)| m.connected)
            .map(|(&to, _)| Outgoing {
                to,
                message: message.clone(),
            })
            .collect()
    }
}

fn handled_by(earlier: &Handled, members: &BTreeMap<MemberId, Member>) -> Handled {
    let mut handled = earlier.clone();
    for (&member, m) in members {
        journal::note_handled(&mut handled, member, m.last_client_seq);
    }
    handled
}

/// Whether `role` may change any file.
fn may_change_files(role: Role) -> bool {
    role.can(Capability::EditScripts) || role.can(Capability::EditAssets)
}

/// Whether `role` may change the file at `path`.
fn may_edit_file(role: Role, path: &str) -> bool {
    role.can(if files::is_script(path) {
        Capability::EditScripts
    } else {
        Capability::EditAssets
    })
}

fn accepted(logged: &Logged) -> ToClient {
    ToClient::Accepted {
        seq: logged.seq,
        author: logged.author,
        client_seq: logged.client_seq,
        label: logged.label.clone(),
        ops: logged.ops.clone(),
        notes: logged.notes.clone(),
    }
}
