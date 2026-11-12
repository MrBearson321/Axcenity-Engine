//! The host's side of handing the session on (Collaborate 2.0, P4 of
//! `docs/research/collaborate-persistent.md` §4.3–4.4, ADR-0022).
//!
//! - **Successors:** members who may host send their [`HostCard`]; the
//!   host ranks them ([`election::rank`]) and sends the list to everyone
//!   whenever it changes, with the next Instance ID it would hand out.
//! - **Leaving** ([`Host::hand_off`]): the host tells everyone who hosts
//!   next and the last transaction they must have, and takes no more edits
//!   (members keep them pending and send them to the new host).
//! - **Recovering** ([`Host::recover_from`]): a host that took over from a
//!   lost one asks members ahead of it for the old host's transactions it
//!   missed ([`ToClient::SendTail`]), replays them in order, and holds
//!   everyone's new edits until then (at most [`RECOVERY_WAIT`]).

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use super::{Host, Outgoing};
use crate::apply;
use crate::audit::AuditKind;
use crate::election::{self, Candidate};
use crate::identity::Fingerprint;
use crate::message::{Have, MemberId, TailEntry, ToClient};
use crate::op::Transaction;
use crate::project::HostCard;

/// How long a recovering host waits for members ahead of it before taking
/// edits anyway.
pub const RECOVERY_WAIT: Duration = Duration::from_secs(10);

/// Blocks of IDs a recovering host skips, for blocks the lost host handed
/// out after its last successor list.
const RESERVED_BLOCKS: u32 = 64;

/// The most a member's address may be, in bytes.
const ADDRESS_MOST: usize = 260;

/// What the host keeps for handing on.
#[derive(Default)]
pub(super) struct State {
    /// The member the Owner named to host next.
    backup: Option<MemberId>,
    /// The member whose editor runs this host (never its own successor).
    hosted_by: Option<MemberId>,
    /// Counts joins, for "connected longest".
    joins: u64,
    /// The list as last sent.
    sent: Option<(Vec<HostCard>, u32)>,
    /// [`Host::hand_off`] was called: no more edits.
    leaving: bool,
    recovery: Option<Recovery>,
    /// After recovery: the first old transaction nobody online had.
    unrecovered: Option<u64>,
}

/// A host that took over from a lost one, while it gathers what it missed.
struct Recovery {
    /// The lost host's epoch, and the newest of its transactions this host
    /// has (in the old numbering).
    old_epoch: u64,
    have: u64,
    /// Members asked for their tail, not answered yet.
    asked: BTreeSet<MemberId>,
    until: Instant,
    /// Edits that came meanwhile, in order.
    held: Vec<(MemberId, Transaction)>,
}

impl Host {
    /// Names the member whose editor runs this host: it's never on the
    /// successor list.
    pub fn set_hosted_by(&mut self, member: Option<MemberId>) -> Vec<Outgoing> {
        self.handoff.hosted_by = member;
        self.successors_if_changed()
    }

    /// The Owner names who hosts next (`None`: the usual order).
    pub fn set_backup(&mut self, member: Option<MemberId>) -> Vec<Outgoing> {
        self.handoff.backup = member;
        self.successors_if_changed()
    }

    pub fn backup(&self) -> Option<MemberId> {
        self.handoff.backup
    }

    pub(super) fn handoff_backup_is(&self, member: MemberId) -> bool {
        self.handoff.backup == Some(member)
    }

    /// Who hosts next, best first.
    pub fn successors(&self) -> Vec<HostCard> {
        let candidates: Vec<Candidate> = self
            .members
            .iter()
            .map(|(&member, m)| Candidate {
                member,
                role: m.role,
                connected: m.connected,
                card: m.card.clone(),
                since: m.since,
            })
            .collect();
        election::rank(&candidates, self.handoff.backup, self.handoff.hosted_by)
    }

    /// Counts a member's join, for "connected longest".
    pub(super) fn note_join(&mut self, member: MemberId) {
        self.handoff.joins += 1;
        let joins = self.handoff.joins;
        if let Some(m) = self.members.get_mut(&member) {
            m.since = joins;
        }
    }

    /// The successor list to everyone, when it changed since last sent.
    pub(super) fn successors_if_changed(&mut self) -> Vec<Outgoing> {
        let now = (self.successors(), self.ids.next_index());
        if self.handoff.sent.as_ref() == Some(&now) {
            return Vec::new();
        }
        self.handoff.sent = Some(now.clone());
        self.to_everyone(ToClient::Successors {
            hosts: now.0,
            next_id: now.1,
        })
    }

    /// The list as last sent, to one member who just joined.
    pub(super) fn successors_to(&mut self, member: MemberId) -> Vec<Outgoing> {
        let mut out = self.successors_if_changed();
        if out.is_empty()
            && let Some((hosts, next_id)) = self.handoff.sent.clone()
        {
            out.push(Outgoing {
                to: member,
                message: ToClient::Successors { hosts, next_id },
            });
        }
        out
    }

    /// `HostCard`: how to reach a member's computer if it hosts.
    pub(super) fn receive_card(
        &mut self,
        member: MemberId,
        address: String,
        cert: Fingerprint,
    ) -> Vec<Outgoing> {
        let Some(m) = self.members.get_mut(&member) else {
            return Vec::new();
        };
        if !m.connected || !m.role.may_host() {
            return Vec::new();
        }
        let address = address.trim();
        if address.is_empty() || address.len() > ADDRESS_MOST || address.contains(char::is_whitespace)
        {
            return Vec::new();
        }
        m.card = Some(HostCard {
            member: Some(member),
            address: address.to_owned(),
            cert,
        });
        self.successors_if_changed()
    }

    /// The host is leaving: everyone hears who hosts next and the last
    /// transaction to have, and no edit is taken from now. Returns the
    /// successor too.
    pub fn hand_off(&mut self) -> (Option<HostCard>, Vec<Outgoing>) {
        self.handoff.leaving = true;
        let to = self.successors().into_iter().next();
        let who = to
            .as_ref()
            .and_then(|card| card.member)
            .and_then(|m| self.members.get(&m))
            .map_or_else(|| "nobody".to_owned(), |m| m.profile.name.clone());
        self.audit.record(
            AuditKind::Handoff,
            None,
            format!("The host handed the session to {who} at transaction {}", self.seq),
        );
        let out = self.to_everyone(ToClient::HandingOff {
            to: to.clone(),
            last_seq: self.seq,
        });
        (to, out)
    }

    /// True once [`Host::hand_off`] was called.
    pub fn is_leaving(&self) -> bool {
        self.handoff.leaving
    }

    /// Starts this host as the successor of one that was lost without a
    /// word: it had the old host's (`old_epoch`) transactions up to `have`.
    /// Members ahead of it are asked for the rest; edits wait until `wait`
    /// is over ([`RECOVERY_WAIT`] in the editor), or sooner once every
    /// member asked has answered.
    pub fn recover_from(mut self, old_epoch: u64, have: u64, wait: Duration) -> Host {
        // The old host may have handed out blocks since its last list:
        // the replayed transactions use IDs from them.
        let reserve = self.ids.next_index().saturating_add(RESERVED_BLOCKS * crate::ids::BLOCK_SIZE);
        self.ids.skip_to(reserve);
        self.handoff.recovery = Some(Recovery {
            old_epoch,
            have,
            asked: BTreeSet::new(),
            until: Instant::now() + wait,
            held: Vec::new(),
        });
        self
    }

    /// Starts handing out Instance IDs at `next` or later (the old host's
    /// next, so a successor never hands out one in use).
    pub fn with_next_id(mut self, next: u32) -> Host {
        self.ids.skip_to(next);
        self
    }

    /// Where the World's deleted Instances were, for a host starting from
    /// a member's copy ([`crate::client::Confirmed`]).
    pub fn with_graveyard(mut self, graveyard: crate::apply::Graveyard) -> Host {
        self.doc.graveyard = graveyard;
        self
    }

    /// True while gathering a lost host's transactions.
    pub fn is_recovering(&self) -> bool {
        self.handoff.recovery.is_some()
    }

    /// The first of the old host's transactions nobody online had, once
    /// recovery ended without them.
    pub fn unrecovered(&self) -> Option<u64> {
        self.handoff.unrecovered
    }

    /// Holds `transaction` while recovering; false when not recovering
    /// (or leaving: then it's dropped, as the member sends it again to the
    /// next host).
    pub(super) fn hold(&mut self, member: MemberId, transaction: &Transaction) -> bool {
        if self.handoff.leaving {
            return true;
        }
        let Some(recovery) = &mut self.handoff.recovery else {
            return false;
        };
        recovery.held.push((member, transaction.clone()));
        true
    }

    /// After a member's `Hello`: one ahead of this host in the lost host's
    /// numbering is asked for what it has.
    pub(super) fn ask_for_tail(&mut self, member: MemberId, have: Option<Have>) -> Vec<Outgoing> {
        let Some(recovery) = &mut self.handoff.recovery else {
            return Vec::new();
        };
        let ahead = have.is_some_and(|h| h.epoch == recovery.old_epoch && h.seq > recovery.have);
        if !ahead || !recovery.asked.insert(member) {
            return Vec::new();
        }
        vec![Outgoing {
            to: member,
            message: ToClient::SendTail {
                after: recovery.have,
            },
        }]
    }

    /// `Tail`: the old host's transactions a member had and this host
    /// didn't, replayed in order as they were accepted. Only from a member
    /// that was asked.
    pub(super) fn receive_tail(&mut self, member: MemberId, entries: Vec<TailEntry>) -> Vec<Outgoing> {
        let Some(recovery) = &mut self.handoff.recovery else {
            return Vec::new();
        };
        if !recovery.asked.remove(&member) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for entry in entries {
            let recovery = self.handoff.recovery.as_mut().expect("checked above");
            if entry.seq != recovery.have + 1 {
                continue;
            }
            // The old host accepted it: it's replayed, not checked again.
            match apply::apply(&mut self.doc.view(), &entry.ops, &|_| true) {
                Ok(applied) => {
                    let recovery = self.handoff.recovery.as_mut().expect("checked above");
                    recovery.have = entry.seq;
                    if let Some(author) = self.members.get_mut(&entry.author) {
                        author.last_client_seq = author.last_client_seq.max(entry.client_seq);
                    }
                    out.extend(self.commit(entry.author, entry.client_seq, entry.label, applied));
                }
                Err(_) => break,
            }
        }
        if self
            .handoff
            .recovery
            .as_ref()
            .is_some_and(|r| r.asked.is_empty())
        {
            out.extend(self.finish_recovery(false));
        }
        out
    }

    /// Each tick: ends recovery once its wait is over.
    pub(super) fn recovery_tick(&mut self, now: Instant) -> Vec<Outgoing> {
        let over = self
            .handoff
            .recovery
            .as_ref()
            .is_some_and(|r| now >= r.until);
        if over {
            return self.finish_recovery(true);
        }
        Vec::new()
    }

    /// Takes edits again: the held ones first, in the order they came.
    fn finish_recovery(&mut self, timed_out: bool) -> Vec<Outgoing> {
        let Some(mut recovery) = self.handoff.recovery.take() else {
            return Vec::new();
        };
        if timed_out && !recovery.asked.is_empty() {
            self.handoff.unrecovered = Some(recovery.have + 1);
            self.audit.record(
                AuditKind::Handoff,
                None,
                format!(
                    "Transactions after {} of the old host couldn't be recovered: the members \
                     who had them didn't answer in time",
                    recovery.have
                ),
            );
        }
        let mut out = Vec::new();
        for (member, transaction) in std::mem::take(&mut recovery.held) {
            out.extend(self.receive(member, crate::message::ToHost::Submit(transaction)));
        }
        out
    }
}
