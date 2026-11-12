//! What clients and the host say to each other. TC3 puts these on the wire
//! (postcard over WebSocket and TLS); here they're plain values.

use std::collections::BTreeMap;

use axcenity_core::World;
use rustc_hash::FxHashMap;

use axcenity_core::InstanceId;

use crate::apply::{Note, Place, Refusal};
use crate::files::{FileHash, Manifest, Piece};
use crate::ids::IdBlock;
use crate::op::{Op, Transaction};
use crate::presence::{ChatLine, Collaborator, Presence};
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

/// What a client's copy of the session has: the World as of transaction
/// `seq` of the host that started as `epoch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Have {
    /// The host's [`ToClient::Welcome`] `epoch`: a host started again (after
    /// a crash, say) has another, and its numbers start over.
    pub epoch: u64,
    pub seq: u64,
}

/// From a client to the host.
#[derive(Clone, Debug)]
pub enum ToHost {
    /// The first message on a connection. `have` is what the client's copy
    /// has, if it still has the session's World; `None` asks for a fresh
    /// snapshot.
    Hello { have: Option<Have> },
    /// One undo step.
    Submit(Transaction),
    /// The client is running low on IDs.
    NeedIds,
    /// What the member is doing now (sent when it changes).
    Presence(Presence),
    /// A piece of a file the member is about to publish with a
    /// `FileVersion`: `bytes` from offset `at` of `total` (TC5).
    Upload {
        hash: FileHash,
        at: u64,
        total: u64,
        bytes: Piece,
    },
    /// Asks for files by hash.
    Fetch(Vec<FileHash>),
    /// Asks to be the one member editing a document (a script tab with
    /// unsaved changes, say).
    Lock { path: String },
    /// Done editing it.
    Unlock { path: String },
    /// Opens the live document of a script or stylesheet (C7): the host
    /// answers with what a copy that has seen `state_vector` lacks (an
    /// empty one for a copy with nothing yet).
    DocOpen { path: String, state_vector: Vec<u8> },
    /// A change to an open live document: a `yrs` v1 update.
    DocUpdate { path: String, update: Vec<u8> },
    /// A line for the session's chat (C8).
    Chat(String),
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
        /// The project's files as of `seq`.
        files: Manifest,
        /// Names the session's project ([`crate::Host::with_session`]).
        session: String,
        /// Names this run of the host: one started again has another, so a
        /// member knows its old ID blocks and numbers are gone (TC6).
        epoch: u64,
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
    /// Everyone in the project, whenever someone joins, leaves, or changes
    /// name or role.
    Roster(Vec<Collaborator>),
    /// What another member is doing now.
    Presence {
        member: MemberId,
        presence: Presence,
    },
    /// A piece of a file the member fetched.
    File {
        hash: FileHash,
        at: u64,
        total: u64,
        bytes: Piece,
    },
    /// The host doesn't have a fetched file (any more): a newer version
    /// replaced it.
    Missing(FileHash),
    /// Who is editing which document, whenever that changes.
    Locks(BTreeMap<String, MemberId>),
    /// The owner ended the session (or quit): nothing more comes (TC6).
    Ended,
    /// Answers `DocOpen`: what the member's copy lacked, and what the
    /// host's has seen (so the member sends what the host lacks).
    DocState {
        path: String,
        update: Vec<u8>,
        state_vector: Vec<u8>,
        /// The document's origin: a copy from another can't merge with it.
        origin: u64,
    },
    /// Another member's change to a live document, as the host applied it.
    DocUpdate {
        path: String,
        update: Vec<u8>,
        author: MemberId,
    },
    /// The host didn't take the member's change to a live document, and
    /// why. Their copy should open the document again.
    DocRefused { path: String, reason: String },
    /// A line of the session's chat, to everyone, the sender included
    /// (C8).
    Chat(ChatLine),
    /// The chat so far, to someone joining.
    ChatHistory(Vec<ChatLine>),
}
