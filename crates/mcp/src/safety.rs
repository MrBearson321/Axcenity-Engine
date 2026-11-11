//! What a session may do ([`Mode`]) and the confirm tokens a destructive
//! call needs ([`Confirms`]).
//!
//! A destructive call made without a token changes nothing: it returns a
//! preview and a token. The token is bound to that exact call (a key the
//! tool makes from its arguments and the world's revision), works once,
//! and expires after [`Confirms::LIFETIME`], so a model can't reuse it for
//! a different or stale request.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use crate::{Args, Class, Code, ToolError};

/// What the user lets AI clients do in this project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Only reads.
    Read,
    /// Reads and undoable edits; destructive ones need a confirm token.
    Edit,
    /// Everything, destructive calls too without a token (still undoable
    /// and logged).
    Full,
}

/// Whether a call of some [`Class`] may run in a [`Mode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allowed {
    Yes,
    /// Only with a confirm token.
    Confirm,
    No,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Read, Mode::Edit, Mode::Full];

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Read => "read",
            Mode::Edit => "edit",
            Mode::Full => "full",
        }
    }

    /// One line on what the mode allows, for `project_info`.
    pub fn about(self) -> &'static str {
        match self {
            Mode::Read => "reads only",
            Mode::Edit => "edits are undoable steps; deletes and saves ask first",
            Mode::Full => "edits are undoable steps; nothing asks first",
        }
    }

    pub fn allows(self, class: Class) -> Allowed {
        match (self, class) {
            (_, Class::Read) => Allowed::Yes,
            (Mode::Read, _) => Allowed::No,
            (Mode::Edit, Class::Destructive) => Allowed::Confirm,
            _ => Allowed::Yes,
        }
    }

    /// The error for a call this mode refuses.
    pub fn refusal(self, tool: &str) -> ToolError {
        ToolError::new(
            Code::NotAllowed,
            format!(
                "{tool} changes the project, and this server is in {} mode",
                self.name()
            ),
        )
        .with_fix("the user can restart the server with --mode edit to allow edits")
    }
}

/// A key for "this exact call": the tool, its arguments but `confirm`, and
/// `state` (such as the world's revision), hashed.
pub fn call_key(tool: &str, args: &Args, state: impl Hash) -> u64 {
    let mut arguments = args.0.clone();
    arguments.remove("confirm");
    // `serde_json::Map` keeps keys sorted, so equal arguments print alike.
    let text = serde_json::Value::Object(arguments).to_string();
    let mut hasher = std::hash::DefaultHasher::new();
    (tool, text, state).hash(&mut hasher);
    hasher.finish()
}

struct Issued {
    token: String,
    key: u64,
    at: Instant,
}

/// Confirm tokens handed out and not yet used.
pub struct Confirms {
    issued: Vec<Issued>,
    /// Seeds tokens, so they differ from run to run.
    random: RandomState,
    count: u64,
}

impl Default for Confirms {
    fn default() -> Self {
        Confirms {
            issued: Vec::new(),
            random: RandomState::new(),
            count: 0,
        }
    }
}

impl Confirms {
    /// How long a token stays good.
    pub const LIFETIME: Duration = Duration::from_secs(120);

    /// A new token for the call `key`.
    pub fn issue(&mut self, key: u64) -> String {
        self.forget_old(Instant::now());
        self.count += 1;
        let number = self.random.hash_one((key, self.count));
        // Six letters and digits: short to repeat, hard to guess.
        const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
        let token: String = (0..6)
            .map(|i| ALPHABET[((number >> (i * 8)) % ALPHABET.len() as u64) as usize] as char)
            .collect();
        self.issued.push(Issued {
            token: token.clone(),
            key,
            at: Instant::now(),
        });
        token
    }

    /// Uses up `token` for the call `key`, or says why it can't be used.
    pub fn redeem(&mut self, token: &str, key: u64) -> Result<(), ToolError> {
        self.redeem_at(token, key, Instant::now())
    }

    fn redeem_at(&mut self, token: &str, key: u64, now: Instant) -> Result<(), ToolError> {
        let found = self.issued.iter().position(|issued| issued.token == token);
        let stale = |message: &str| {
            ToolError::new(Code::NeedsConfirm, message.to_owned())
                .with_fix("call again without confirm for a new preview and token")
        };
        let Some(index) = found else {
            return Err(stale(
                "that confirm token isn't known (or was already used)",
            ));
        };
        let issued = self.issued.remove(index);
        if now.duration_since(issued.at) > Self::LIFETIME {
            return Err(stale("that confirm token expired"));
        }
        if issued.key != key {
            return Err(stale(
                "that confirm token was for a different call, or the project changed since",
            ));
        }
        Ok(())
    }

    fn forget_old(&mut self, now: Instant) {
        self.issued
            .retain(|issued| now.duration_since(issued.at) <= Self::LIFETIME);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn args(value: serde_json::Value) -> Args {
        Args::new(value.as_object().cloned().unwrap())
    }

    #[test]
    fn modes_allow_by_class() {
        assert_eq!(Mode::Read.allows(Class::Read), Allowed::Yes);
        assert_eq!(Mode::Read.allows(Class::Add), Allowed::No);
        assert_eq!(Mode::Edit.allows(Class::Modify), Allowed::Yes);
        assert_eq!(Mode::Edit.allows(Class::Destructive), Allowed::Confirm);
        assert_eq!(Mode::Full.allows(Class::Destructive), Allowed::Yes);
        assert_eq!(Mode::from_name("edit"), Some(Mode::Edit));
        assert_eq!(Mode::from_name("admin"), None);
    }

    #[test]
    fn a_key_ignores_confirm_and_order_but_not_arguments_or_state() {
        let a = call_key("object_delete", &args(json!({"ids": ["1"], "x": 2})), 7);
        let b = call_key(
            "object_delete",
            &args(json!({"x": 2, "ids": ["1"], "confirm": "abc"})),
            7,
        );
        assert_eq!(a, b);
        assert_ne!(
            a,
            call_key("object_delete", &args(json!({"ids": ["2"], "x": 2})), 7)
        );
        assert_ne!(
            a,
            call_key("object_delete", &args(json!({"ids": ["1"], "x": 2})), 8)
        );
        assert_ne!(
            a,
            call_key("project_save", &args(json!({"ids": ["1"], "x": 2})), 7)
        );
    }

    #[test]
    fn a_token_works_once_for_its_own_call() {
        let mut confirms = Confirms::default();
        let token = confirms.issue(1);
        assert_eq!(token.len(), 6);
        assert!(confirms.redeem(&token, 2).is_err(), "another call");
        // A wrong try uses it up too, so it can't be guessed at.
        assert!(confirms.redeem(&token, 1).is_err());
        let token = confirms.issue(1);
        assert!(confirms.redeem(&token, 1).is_ok());
        let error = confirms.redeem(&token, 1).unwrap_err();
        assert_eq!(error.code, Code::NeedsConfirm);
        assert!(error.message.contains("already used"), "{error}");
    }

    #[test]
    fn a_token_expires() {
        let mut confirms = Confirms::default();
        let token = confirms.issue(1);
        let later = Instant::now() + Confirms::LIFETIME + Duration::from_secs(1);
        let error = confirms.redeem_at(&token, 1, later).unwrap_err();
        assert!(error.message.contains("expired"), "{error}");
    }
}
