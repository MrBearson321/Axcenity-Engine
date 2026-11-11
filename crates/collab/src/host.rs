//! The session host: the one place that orders, checks and applies every
//! member's transactions.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

use axcenity_core::World;

use crate::apply::{self, Doc, Note, Refusal};
use crate::files::{self, Assembly, FileHash, FileStore, Manifest, MemoryStore, Piece};
use crate::ids::{IdAllocator, IdBlock};
use crate::journal::{self, Journal};
use crate::message::{MemberId, Snapshot, ToClient, ToHost};
use crate::op::Op;
use crate::presence::{Collaborator, Presence, Profile};
use crate::role::{Capability, Role};

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
}

/// The authority of one Team Create session.
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
        }
    }

    /// Keeps a journal in the project at `project` (TC6): a snapshot of
    /// the World now, then each accepted transaction, so a host that stops
    /// without ending the session can be recovered ([`journal::recover`]).
    pub fn start_journal(&mut self, project: &std::path::Path) -> std::io::Result<()> {
        self.journal = Some(Journal::start(
            project,
            &self.doc.world,
            &self.doc.graveyard,
            self.seq,
        )?);
        self.snapshot_at = self.clock;
        Ok(())
    }

    /// Whether this host keeps a journal.
    pub fn has_journal(&self) -> bool {
        self.journal.is_some()
    }

    /// Writes a fresh snapshot now, when the journal has entries.
    pub fn snapshot(&mut self) {
        self.snapshot_at = self.clock;
        let Some(journal) = &mut self.journal else {
            return;
        };
        if journal.since_snapshot() == 0 {
            return;
        }
        if let Err(error) = journal.snapshot(&self.doc.world, &self.doc.graveyard, self.seq) {
            log::error!("Team Create: couldn't write the session's snapshot: {error}");
        }
    }

    /// The session ended cleanly: the journal and its snapshot go, as
    /// there's nothing to recover.
    pub fn end_journal(&mut self) {
        if let Some(journal) = self.journal.take()
            && let Err(error) = journal.end()
        {
            log::warn!("Team Create: couldn't remove the session's journal: {error}");
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
        if now.saturating_duration_since(self.snapshot_at) >= journal::SNAPSHOT_EVERY {
            self.snapshot();
        }
        let before = self.locks.len();
        self.locks.retain(|_, lock| {
            lock.away_since
                .is_none_or(|since| now.saturating_duration_since(since) < LOCK_GRACE)
        });
        if self.locks.len() == before {
            return Vec::new();
        }
        self.locks_to_everyone()
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

    /// The transactions still kept, oldest first.
    pub fn log(&self) -> impl Iterator<Item = &Logged> {
        self.log.iter()
    }

    /// Lets `member` join with `role`. (TC3 does this when an invite is
    /// redeemed by a signed-in identity.)
    pub fn add_member(&mut self, member: MemberId, role: Role) {
        self.members.insert(
            member,
            Member {
                role,
                blocks: Vec::new(),
                last_client_seq: 0,
                refused: VecDeque::new(),
                connected: false,
                profile: Profile::unnamed(member),
                presence: Presence::default(),
                uploads: HashMap::new(),
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
        if self.members.remove(&member).is_none() {
            return Vec::new();
        }
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
                    return self.refuse(member, transaction.client_seq, reason);
                }
                let m = self.members.get_mut(&member).expect("checked above");
                let blocks = m.blocks.clone();
                let owns = |id| blocks.iter().any(|block: &IdBlock| block.contains(id));
                match apply::apply(&mut self.doc.view(), &transaction.ops, &owns) {
                    Ok(applied) => {
                        self.seq += 1;
                        let logged = Logged {
                            seq: self.seq,
                            author: member,
                            client_seq: transaction.client_seq,
                            label: transaction.label,
                            ops: applied.ops,
                            notes: applied.notes,
                        };
                        let message = accepted(&logged);
                        // Written down before anyone hears of it.
                        if let Some(journal) = &mut self.journal
                            && let Err(error) = journal.append(&logged)
                        {
                            log::error!("Team Create: couldn't write the journal: {error}");
                        }
                        self.log.push_back(logged);
                        while self.log.len() > self.log_kept {
                            self.log.pop_front();
                        }
                        self.to_everyone(message)
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
                if !m.connected || !may_change_files(m.role) || self.store.has(hash) {
                    return Vec::new();
                }
                match Assembly::add(&mut m.uploads, hash, at, total, &bytes.0) {
                    Ok(Some(file)) => self.store.keep(hash, file),
                    Ok(None) => {}
                    Err(()) => log::warn!(
                        "Team Create: member {} sent a file that doesn't fit together",
                        member.0
                    ),
                }
                Vec::new()
            }
            ToHost::Fetch(hashes) => {
                if !self.members[&member].connected {
                    return Vec::new();
                }
                let mut out = Vec::new();
                for hash in hashes {
                    match self.store.read(hash) {
                        Some(bytes) => {
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
                        None => out.push(Outgoing {
                            to: member,
                            message: ToClient::Missing(hash),
                        }),
                    }
                }
                out
            }
            ToHost::Lock { path } => {
                let m = &self.members[&member];
                if !m.connected
                    || !may_edit_file(m.role, &path)
                    || files::check_path(&path).is_err()
                {
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
            ToHost::NeedIds => {
                let block = self.ids.block();
                let m = self.members.get_mut(&member).expect("checked above");
                m.blocks.push(block);
                vec![Outgoing {
                    to: member,
                    message: ToClient::Ids(block),
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

    fn hello(&mut self, member: MemberId, have: Option<u64>) -> Vec<Outgoing> {
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

    fn catch_up(&mut self, member: MemberId, have: Option<u64>) -> Vec<Outgoing> {
        let first_kept = self.log.front().map_or(self.seq + 1, |logged| logged.seq);
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
        m.blocks.push(block);
        vec![Outgoing {
            to: member,
            message: ToClient::Welcome {
                world: Snapshot(Box::new(self.doc.world.clone())),
                graveyard: self.doc.graveyard.clone(),
                seq: self.seq,
                last_client_seq,
                role,
                ids: Some(block),
                rejected,
                files: self.doc.files.clone(),
                session: self.session.clone(),
            },
        }]
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
