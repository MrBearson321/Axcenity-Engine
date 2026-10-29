//! Axcenity multiplayer (Milestone 14). Phase 63 is the transport, hosting
//! and joining; Phase 64 gives a joined player a replica of the server's
//! world and carries RemoteEvents and RemoteFunctions between devices.
//!
//! - `transport`: renet's reliable-ordered and unreliable channels over
//!   UDP (netcode's connections, timeouts and disconnect reasons), or in
//!   memory through the loopback, with the same [`Server`] and [`Client`].
//! - `handshake`: the join request with the network protocol and the
//!   game's version, and the host's answer: a welcome, or a refusal that
//!   says why on both ends.
//! - `session`: a [`Host`] admitting players and keeping everyone's roster,
//!   and a [`Join`].
//! - `discovery`: LAN games found by broadcast.
//! - `wire`: values and remote messages as plain data, Instances as
//!   network IDs; `replicate`: the snapshot and each frame's changes.
//! - `game`: [`Network`], which the player and `axcenity-server` run: it
//!   does scripts' `NetworkService` calls, keeps `Players` in step, sends
//!   the world to players and carries their remote messages.

mod discovery;
mod game;
mod handshake;
mod replicate;
mod session;
mod transport;
mod wire;

pub use discovery::{HostInfo, LanAnswerer, LanSearch, SEARCH_TIME};
pub use game::{Network, ScriptCode};
pub use handshake::{GameInfo, Message, Peer, Refusal};
pub use replicate::{Applied, Op, Replicator, Update, WireChange, WireInstance, apply};
pub use session::{HANDSHAKE_TIMEOUT, Host, HostSettings, Join, SessionEvent};
pub use transport::{Channel, Client, ConnectionId, Server, ServerEvent};
pub use wire::{WireArg, WireKind, WireMessage, WireValue};

/// The network protocol's version: games on different ones can't play
/// together, and the handshake says so. Raise it when a message changes.
pub const PROTOCOL_VERSION: u32 = 2;
/// The UDP port games are hosted on unless told another.
pub const DEFAULT_PORT: u16 = 47800;
/// The UDP port hosts answer LAN searches on.
pub const DISCOVERY_PORT: u16 = 47801;

#[cfg(test)]
mod replication_tests;
#[cfg(test)]
mod tests;
