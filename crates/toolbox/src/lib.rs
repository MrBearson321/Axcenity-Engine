//! The Axcenity Toolbox: the editor's one way to find and fetch assets
//! from online sources.
//!
//! Every source implements [`AssetSource`] and describes its assets as
//! [`AssetRecord`]s, so nothing above it needs to know a provider's API:
//! ambientCG ([`ambientcg`]), Poly Haven ([`polyhaven`]) and Openverse
//! sounds ([`openverse`]), ported from the Django Toolbox. Underneath are
//! HTTPS requests with size limits ([`http`]), the rules for untrusted
//! downloads ([`guard`]), swapping a finished folder into a project
//! ([`files`]), licence codes ([`license`]) and errors that are safe to show
//! ([`SourceError`]). The design and what comes next:
//! `docs/research/asset-sources/provider-interface.md`.
//!
//! Editor only: the player, server and test runner must never depend on it.

pub mod ambientcg;
mod error;
pub mod files;
pub mod guard;
pub mod http;
mod json;
pub mod license;
pub mod openverse;
pub mod polyhaven;
pub mod record;
mod source;
pub mod variants;

pub use error::SourceError;
pub use record::{AssetRecord, Kind, Package, RemoteFile, Variant};
pub use source::{
    AssetSource, Licenses, Query, SearchPage, SourceInfo, source, sources, sources_for,
};
