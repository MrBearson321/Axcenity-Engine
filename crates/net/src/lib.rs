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
//! - `secure`: MP4's connect tokens: a server's certificate and token
//!   port, and fetching a token over TLS checked against a fingerprint.
//! - `relayed` and `host_socket`: MP5's join codes through an
//!   `axcenity-relay`, hole punching, and relayed players on the host.
//! - `port_map`: MP5b's port mapping on the host's router (UPnP, PCP,
//!   NAT-PMP).
//! - `discovery`: LAN games found by broadcast.
//! - `wire`: values and remote messages as plain data, Instances as
//!   network IDs; `replicate`: the snapshot and each frame's changes.
//! - `game`: [`Network`], which the player and `axcenity-server` run: it
//!   does scripts' `NetworkService` calls, keeps `Players` in step, sends
//!   the world to players and carries their remote messages.

mod discovery;
mod game;
mod handshake;
mod host_socket;
pub mod limits;
mod motion;
mod movement;
mod ownership;
mod port_map;
mod predict;
mod relayed;
mod replay;
mod replicate;
mod secure;
mod session;
pub mod streaming;
mod transport;
mod wire;

pub use discovery::{HostInfo, LanAnswerer, LanSearch, SEARCH_TIME};
pub use game::{Network, PeerStats, ScriptCode, server_moves_characters};
pub use handshake::{GameInfo, Message, Peer, Refusal, class_defaults};
pub use motion::{
    DELAY, ENTRY_BYTES, MOTION_BUDGET, Motion, MotionSender, PartState, SETTLE_TIME, Smoother,
};
pub use ownership::{
    ASSEMBLY_SPEED, CharacterCheck, Owned, OwnedSender, Sender, TELEPORT_DISTANCE,
};
pub use port_map::{MapEvent, MapMethod, PortMapper, is_public};
pub use predict::{AHEAD, Move, Moved, PredictionStats, SNAP};
pub use relayed::{PUNCH_TIME, RelayEvent, Route};
pub use replay::{
    BufferPlay, KEYFRAME_SECONDS, MAX_BUFFER_SECONDS, Playback, REPLAY_VERSION, Recorder, Replay,
    ReplayFrame, Replays, WirePose, eyes_of,
};
pub use replicate::{
    Applied, LevelState, Op, Replicator, Update, WireAnimator, WireChange, WireInstance,
    WireParameter, apply, replicated_parts,
};
pub use secure::{
    FetchError, Secured, ServerCert, TokenRequest, TokenRules, Trust, fingerprint_of, join_link,
    short as short_fingerprint, split_join_link,
};
pub use session::{HANDSHAKE_TIMEOUT, Host, HostSettings, Join, SessionEvent};
pub use transport::{Channel, Client, Conditions, ConnectionId, LinkStats, Server, ServerEvent};
pub use wire::{WireArg, WireKind, WireMessage, WireValue};

/// The network protocol's version: games on different ones can't play
/// together, and the handshake says so. Raise it when a message changes.
pub const PROTOCOL_VERSION: u32 = 15;
/// A chat message as sent on: trimmed, at most 200 characters, and not
/// empty, whatever a modified client sends.
pub(crate) fn chat_text(text: &str) -> Option<String> {
    let text: String = text.trim().chars().take(200).collect();
    (!text.is_empty()).then_some(text)
}

/// The log filter hosts start with unless `RUST_LOG` says otherwise:
/// warnings, but none from `renetcode`, which logs an error for every
/// packet it can't read. Anyone can send those (a client of a server that
/// restarted does, many a second), so they'd flood the log (MP2); this
/// crate reports the reasons connections end.
pub const LOG_FILTER: &str = "warn,renetcode=off";

/// The UDP port games are hosted on unless told another.
pub const DEFAULT_PORT: u16 = 47800;
/// The UDP port hosts answer LAN searches on.
pub const DISCOVERY_PORT: u16 = 47801;

#[cfg(test)]
mod ability_tests;
#[cfg(test)]
mod character_tests;
#[cfg(test)]
mod data_layer_tests;
#[cfg(test)]
mod lag_tests;
#[cfg(test)]
mod movement_tests;
#[cfg(test)]
mod multiplayer_tests;
#[cfg(test)]
mod object_script_tests;
#[cfg(test)]
mod predict_tests;
#[cfg(test)]
mod rejoin_tests;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod replication_tests;
#[cfg(test)]
mod secure_tests;
#[cfg(test)]
mod smoothing_tests;
#[cfg(test)]
mod streaming_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;
#[cfg(test)]
mod wire_tests;
