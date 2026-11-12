//! Who hosts next (Collaborate 2.0, P4 of
//! `docs/research/collaborate-persistent.md` §4.3–4.4, ADR-0022).
//!
//! The host keeps an ordered list of **successors** and sends it to every
//! member whenever it changes, so when the host goes, every member holds
//! the same list and picks the same computer: nobody votes. The order is
//! the brief's: the **backup host** the Owner named, then whoever has been
//! connected longest, then the lowest member number. Only members who may
//! change the project (Editors and the Owner) and whose editor sent a
//! [`HostCard`] (it can listen, and its person lets others host from it)
//! are on the list.
//!
//! When the connection to the host is lost without a word (a crash, a
//! power cut, the network), members first try the old host again for a
//! **grace period**; then they go down the list, giving each successor its
//! own time to come up ([`after_loss`]).

use std::time::Duration;

use crate::message::MemberId;
use crate::project::HostCard;
use crate::role::Role;

/// How long members keep trying the old host before moving on.
pub const GRACE: Duration = Duration::from_secs(20);

/// How long each successor gets to come up before the next is tried.
pub const PER_SUCCESSOR: Duration = Duration::from_secs(15);

/// The most successors a list holds.
pub const MOST_SUCCESSORS: usize = 8;

/// One member as the host sees them, for [`rank`].
#[derive(Clone, Debug)]
pub struct Candidate {
    pub member: MemberId,
    pub role: Role,
    pub connected: bool,
    /// Where they can be reached if they host, once their editor said.
    pub card: Option<HostCard>,
    /// When they connected, in the host's own count (smaller is longer
    /// ago).
    pub since: u64,
}

/// The successors, best first: the backup host, then the longest
/// connected, then the lowest member number. Connected members who may
/// host and have a card only; never `host` itself.
pub fn rank(
    candidates: &[Candidate],
    backup: Option<MemberId>,
    host: Option<MemberId>,
) -> Vec<HostCard> {
    let mut eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.connected && c.role.may_host() && c.card.is_some() && Some(c.member) != host)
        .collect();
    eligible.sort_by_key(|c| (Some(c.member) != backup, c.since, c.member.0));
    eligible
        .into_iter()
        .take(MOST_SUCCESSORS)
        .filter_map(|c| {
            let mut card = c.card.clone()?;
            card.member = Some(c.member);
            Some(card)
        })
        .collect()
}

/// The timings of [`after_loss`] (tests use short ones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub grace: Duration,
    pub per_successor: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            grace: GRACE,
            per_successor: PER_SUCCESSOR,
        }
    }
}

/// What a member does, `lost_for` after the host went quiet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Still within the grace period: keep trying the old host.
    WaitForHost,
    /// This member is the successor whose turn it is: host.
    Host,
    /// Join successor `index` of the list.
    Join(usize),
    /// Nobody on the list is left: keep working alone (offline).
    Alone,
}

/// Where `me` should be, `lost_for` after losing the host, with the list
/// of `successors` it last heard. Every member computes the same answer
/// from the same list.
pub fn after_loss(
    lost_for: Duration,
    timing: Timing,
    successors: &[HostCard],
    me: MemberId,
) -> Choice {
    let Some(past) = lost_for.checked_sub(timing.grace) else {
        return Choice::WaitForHost;
    };
    let turn = (past.as_millis() / timing.per_successor.as_millis().max(1)) as usize;
    // Once it's my turn I host, and stay hosting. (A member stops asking
    // once it has joined someone: only a member that couldn't reach the
    // successors before it gets this far.)
    if let Some(mine) = successors.iter().position(|c| c.member == Some(me))
        && mine <= turn
    {
        return Choice::Host;
    }
    match successors.get(turn) {
        Some(_) => Choice::Join(turn),
        None => Choice::Alone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Fingerprint;

    fn card(n: u8) -> HostCard {
        HostCard {
            member: None,
            address: format!("10.0.0.{n}:47810"),
            cert: Fingerprint([n; 32]),
        }
    }

    fn candidate(id: u32, role: Role, since: u64, card_: bool) -> Candidate {
        Candidate {
            member: MemberId(id),
            role,
            connected: true,
            card: card_.then(|| card(id as u8)),
            since,
        }
    }

    fn ids(list: &[HostCard]) -> Vec<u32> {
        list.iter().map(|c| c.member.unwrap().0).collect()
    }

    #[test]
    fn the_backup_comes_first_then_the_longest_connected_then_the_lowest_number() {
        let all = vec![
            candidate(1, Role::Owner, 1, true),
            candidate(2, Role::Editor, 5, true),
            candidate(3, Role::Editor, 3, true),
            candidate(4, Role::Editor, 3, true),
            candidate(5, Role::Viewer, 0, true),
            candidate(6, Role::Editor, 0, false),
        ];
        // The host (1) is never its own successor; a Viewer never hosts,
        // nor someone whose editor sent no card.
        assert_eq!(ids(&rank(&all, None, Some(MemberId(1)))), [3, 4, 2]);
        assert_eq!(
            ids(&rank(&all, Some(MemberId(2)), Some(MemberId(1)))),
            [2, 3, 4]
        );
        // A dedicated server hosts: the Owner is a successor too.
        assert_eq!(ids(&rank(&all, None, None)), [1, 3, 4, 2]);
        let mut gone = all.clone();
        gone[2].connected = false;
        assert_eq!(ids(&rank(&gone, None, Some(MemberId(1)))), [4, 2]);
    }

    #[test]
    fn everyone_picks_the_same_successor_and_moves_on_when_it_doesnt_come_up() {
        let timing = Timing {
            grace: Duration::from_secs(20),
            per_successor: Duration::from_secs(15),
        };
        let mut list = vec![card(2), card(3), card(4)];
        for (i, c) in list.iter_mut().enumerate() {
            c.member = Some(MemberId(i as u32 + 2));
        }
        let at = |secs| Duration::from_secs(secs);
        // The grace period: try the old host again.
        assert_eq!(
            after_loss(at(5), timing, &list, MemberId(3)),
            Choice::WaitForHost
        );
        // Then the first successor hosts, the others join it.
        assert_eq!(after_loss(at(21), timing, &list, MemberId(2)), Choice::Host);
        assert_eq!(
            after_loss(at(21), timing, &list, MemberId(3)),
            Choice::Join(0)
        );
        assert_eq!(
            after_loss(at(21), timing, &list, MemberId(9)),
            Choice::Join(0)
        );
        // It didn't come up: the next one hosts.
        assert_eq!(after_loss(at(36), timing, &list, MemberId(3)), Choice::Host);
        assert_eq!(
            after_loss(at(36), timing, &list, MemberId(4)),
            Choice::Join(1)
        );
        assert_eq!(
            after_loss(at(36), timing, &list, MemberId(9)),
            Choice::Join(1)
        );
        // A host stays a host.
        assert_eq!(after_loss(at(60), timing, &list, MemberId(2)), Choice::Host);
        assert_eq!(after_loss(at(60), timing, &list, MemberId(3)), Choice::Host);
        // Past the end of the list: alone.
        assert_eq!(
            after_loss(at(80), timing, &list, MemberId(9)),
            Choice::Alone
        );
        assert_eq!(after_loss(at(80), timing, &[], MemberId(9)), Choice::Alone);
    }
}
