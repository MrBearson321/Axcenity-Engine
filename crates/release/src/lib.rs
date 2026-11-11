//! Axcenity's versions and release history.
//!
//! - [`Version`] and [`Channel`]: the one engine version (`Cargo.toml`'s
//!   workspace version) and the channel that follows from it.
//! - [`BuildInfo`]: this build's version, commit, toolchain and whether the
//!   release workflow made it; `--version` on every program.
//! - [`History`]: the release records, change entries, feature registry and
//!   known issues in `releases/`, checked by [`History::validate`] and asked
//!   questions by its query methods (the Version Explorer's data model, for
//!   the editor and the MCP toolkit alike).
//! - [`prepare`]: `axcenity-release check` and `prepare` for the repository.
//! - [`publish`]: what a release attaches: release notes, the public history
//!   file and the manifest the updater trusts.
//! - [`install`]: `install.json`, how this copy was installed.
//!
//! Design: `docs/research/release-system/`. Writing release files:
//! `releases/README.md`.

pub mod build_info;
pub mod history;
pub mod install;
pub mod prepare;
pub mod publish;
pub mod records;
pub mod version;

pub use build_info::{BuildInfo, BuildKind, version_flag};
pub use history::{
    ChangeRef, Comparison, FormatChange, History, HitKind, IssueState, LoadError, Page, Problem,
    QueryError, SearchHit, SearchQuery, TimelineEntry,
};
pub use install::InstallKind;
pub use publish::{Manifest, Notes};
pub use records::{
    Category, Change, Evidence, Feature, FeatureEvent, FeatureStatus, Introduced, IssueStatus,
    KnownIssue, ReleaseRecord, Stability, Subsystem, VersionRef,
};
pub use version::{Channel, Version, VersionError};

#[cfg(test)]
mod tests;
