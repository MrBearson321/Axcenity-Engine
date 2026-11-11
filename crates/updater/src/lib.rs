//! Updating an installed Axcenity editor (release system Phase 6;
//! `docs/research/release-system/architecture.md` §7).
//!
//! - [`download`]: the payload, resumable and checked against the signed
//!   manifest's size and SHA-256.
//! - [`extract`]: unpacking it without letting anything out of its folder.
//! - [`stage`]: the editor's side: the payload this install needs, staging
//!   it, and handing over to the new release's `axcenity-updater`.
//! - [`apply`]: what `axcenity-updater` does after the editor exits: swap
//!   the install folder, check the new editor, roll back on failure.
//!
//! Only Windows setup installs and the Linux `.tar.gz` update themselves;
//! the others point to their new package.

pub mod apply;
pub mod download;
pub mod extract;
pub mod stage;

#[cfg(test)]
mod tests;
