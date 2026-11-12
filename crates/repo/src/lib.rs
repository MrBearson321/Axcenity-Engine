//! A Collaborate project's optional Git remote (Collaborate 2.0, P1–P5 of
//! `docs/research/collaborate-persistent.md`, ADR-0022).
//!
//! Git is how a project outlives every session: whoever hosts commits the
//! project as automatic checkpoints and uploads them, and anyone can pick
//! the project up from the remote later, even when nobody is online. It
//! never replaces live editing (that stays `collab`'s operations and live
//! documents), and nothing here is needed for a local-only project.
//!
//! - [`git`]: the installed `git`, behind a [`git::Runner`].
//! - [`github`]: GitHub sign-in (device flow) and repositories.
//! - [`credentials`]: where the token is kept (Git's credential store).
//! - [`repository`]: linking a folder to a remote, status, unlinking.
//! - [`secret`]: tokens that never print.
//! - [`checkpoint`], [`sync`]: automatic checkpoints and uploads;
//!   [`conflicts`]: files changed on both sides, kept and resolved.

pub mod checkpoint;
pub mod conflicts;
pub mod credentials;
pub mod git;
pub mod github;
pub mod manager;
pub mod repository;
pub mod secret;
pub mod sync;

#[cfg(test)]
mod testing;

pub use git::{Author, Git, GitError};
pub use manager::{ConnectError, Connected, RepositoryManager, SignIn};
pub use secret::Secret;

/// The commit message of an automatic or manual checkpoint:
/// `[Collaborate] <subject>`, a sentence, and the trailers that tell
/// Axcenity's checkpoints from people's own commits.
pub fn checkpoint_message(
    subject: &str,
    kind: &str,
    participants: &[&str],
    detail: &str,
) -> String {
    let mut message = format!("[Collaborate] {subject}\n");
    if !detail.is_empty() {
        message.push('\n');
        message.push_str(detail);
        message.push('\n');
    }
    message.push('\n');
    message.push_str(&format!("{}: {kind}\n", repository::CHECKPOINT_TRAILER));
    if !participants.is_empty() {
        message.push_str(&format!(
            "Axcenity-Participants: {}\n",
            participants.join(", ")
        ));
    }
    message
}
