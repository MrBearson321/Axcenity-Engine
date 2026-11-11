//! Team Create's sync core: how several editors keep one project's World in
//! step (phase TC1 of `docs/research/team-create.md`, ADR-0020).
//!
//! One **host** holds the authoritative World and decides the order of every
//! edit. Each collaborator's editor is a **client** with its own copy. An
//! edit is a [`Transaction`] of small [`Op`]s (one undo step). The client
//! applies it at once, so the user never waits, and sends it to the host.
//! The host checks the member's role, applies it with the conflict rules in
//! [`apply`], numbers it and sends it to everyone. Until then the client
//! keeps it *pending*: whenever the host's next transaction arrives, the
//! client takes its pending ones off ([`apply::undo`]), applies the host's,
//! and puts its pending ones back on top. So every copy ends as the host's
//! order says, and nobody's view jumps while they work.
//!
//! The conflict rules, in short: the later write to a property wins (in the
//! host's order); a delete wins over a concurrent edit or move; something
//! made or moved into a deleted parent goes to that parent's nearest
//! surviving ancestor; a move that would put an Instance inside itself is
//! refused. A deleted Instance is kept detached (as the editor's own undo
//! does), so Restore and undoing a delete are just moves back.
//!
//! New Instances get IDs from blocks the host hands each writer
//! ([`ids`]), so two people creating at once never clash and an ID never
//! has to be rewritten; every world here is in replica mode
//! ([`World::make_replica`]).
//!
//! This crate has no network, files or UI: [`Host`] and [`Client`] take
//! messages in and hand messages out, and whatever carries them (an
//! in-memory queue in the tests, WebSocket over TLS later) is the caller's.

pub mod apply;
pub mod client;
pub mod host;
pub mod ids;
pub mod message;
pub mod op;
pub mod role;

#[cfg(test)]
mod convergence_tests;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

pub use apply::{Doc, Note, Refusal};
pub use client::{Client, Notice};
pub use host::Host;
pub use ids::IdBlock;
pub use message::{MemberId, ToClient, ToHost};
pub use op::{NodeSpec, Op, Transaction};
pub use role::{Capability, Role};

pub use axcenity_core::World;
