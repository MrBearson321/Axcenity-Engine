//! The session host: the one place that orders, checks and applies every
//! member's transactions.

use std::collections::{BTreeMap, VecDeque};

use axcenity_core::World;

use crate::apply::{self, Doc, Note, Refusal};
use crate::ids::{IdAllocator, IdBlock};
use crate::message::{MemberId, Snapshot, ToClient, ToHost};
use crate::op::Op;
use crate::role::{Capability, Role};

/// How many applied transactions the host keeps to catch a reconnecting
/// member up; one further behind gets a fresh snapshot instead.
pub const LOG_KEPT: usize = 10_000;

/// How many of a member's refusals the host remembers for when they
/// reconnect.
const REFUSALS_KEPT: usize = 64;

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
}

/// The authority of one Team Create session.
pub struct Host {
    doc: Doc,
    seq: u64,
    log: VecDeque<Logged>,
    log_kept: usize,
    ids: IdAllocator,
    members: BTreeMap<MemberId, Member>,
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
        }
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
            },
        );
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
        if m.connected {
            vec![Outgoing {
                to: member,
                message: ToClient::RoleChanged(role),
            }]
        } else {
            Vec::new()
        }
    }

    /// The member's connection closed. What they send after reconnecting
    /// starts with `Hello`.
    pub fn disconnected(&mut self, member: MemberId) {
        if let Some(m) = self.members.get_mut(&member) {
            m.connected = false;
        }
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
                if !m.role.can(Capability::EditWorld) {
                    return self.refuse(member, transaction.client_seq, Refusal::Permission);
                }
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
                        self.log.push_back(logged);
                        while self.log.len() > self.log_kept {
                            self.log.pop_front();
                        }
                        self.to_everyone(message)
                    }
                    Err(reason) => self.refuse(member, transaction.client_seq, reason),
                }
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

    fn hello(&mut self, member: MemberId, have: Option<u64>) -> Vec<Outgoing> {
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
