//! What clients and the host say to each other. TC3 puts these on the wire
//! (postcard over WebSocket and TLS); here they're plain values.

use axcenity_core::World;
use rustc_hash::FxHashMap;

use axcenity_core::InstanceId;

use crate::apply::{Note, Place, Refusal};
use crate::ids::IdBlock;
use crate::op::{Op, Transaction};
use crate::role::Role;

/// Who a member is within one hosted project. TC3 ties it to a signed-in
/// identity; here the caller assigns it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct MemberId(pub u32);

/// A whole World sent to a joining member. Its own type so `Debug` prints
/// only its size.
#[derive(Clone)]
pub struct Snapshot(pub Box<World>);

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snapshot({} instances)", self.0.len())
    }
}

/// From a client to the host.
#[derive(Clone, Debug)]
pub enum ToHost {
    /// The first message on a connection. `have` is the last host sequence
    /// number the client has, if it still has the session's World; `None`
    /// asks for a fresh snapshot.
    Hello { have: Option<u64> },
    /// One undo step.
    Submit(Transaction),
    /// The client is running low on IDs.
    NeedIds,
}

/// From the host to a client.
#[derive(Clone, Debug)]
pub enum ToClient {
    /// The whole World (and where its deleted Instances were) as of `seq`.
    /// Transactions the member sent up to `last_client_seq` were handled
    /// (accepted ones are in the snapshot).
    Welcome {
        world: Snapshot,
        graveyard: FxHashMap<InstanceId, Place>,
        seq: u64,
        last_client_seq: u64,
        role: Role,
        ids: Option<IdBlock>,
        /// The member's recently refused transactions, so a client that
        /// missed a `Rejected` while away can say which edits were refused.
        rejected: Vec<(u64, Refusal)>,
    },
    /// After a `Hello` whose `have` the host could catch up from: the
    /// missed transactions came just before this, and transactions the
    /// member sent up to `last_client_seq` were handled.
    Resumed {
        last_client_seq: u64,
        role: Role,
        rejected: Vec<(u64, Refusal)>,
    },
    /// A transaction the host applied, number `seq`, as applied.
    Accepted {
        seq: u64,
        author: MemberId,
        client_seq: u64,
        label: String,
        ops: Vec<Op>,
        notes: Vec<Note>,
    },
    /// The member's transaction `client_seq` changed nothing, and why.
    Rejected { client_seq: u64, reason: Refusal },
    /// More IDs for the member.
    Ids(IdBlock),
    /// The member's role changed.
    RoleChanged(Role),
}
