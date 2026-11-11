//! The Axcenity Toolbox: the editor's one way to find and fetch assets
//! from online sources.
//!
//! This first stage holds what every source needs, moved out of the
//! editor's Get texture dialog: HTTPS requests with size limits
//! ([`http`]), the rules for untrusted downloads ([`guard`]), swapping a
//! finished folder into a project ([`files`]) and errors that are safe to
//! show ([`SourceError`]). The plan for the rest is in
//! `docs/research/asset-sources/provider-interface.md`.
//!
//! Editor only: the player, server and test runner must never depend on it.

mod error;
pub mod files;
pub mod guard;
pub mod http;

pub use error::SourceError;
