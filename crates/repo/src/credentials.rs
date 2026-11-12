//! Where a GitHub token is kept between runs (P1 of
//! `docs/research/collaborate-persistent.md`).
//!
//! Never in the project, never in Axcenity's settings files. Axcenity uses
//! **Git's own credential store** (`git credential fill`, `approve`,
//! `reject`): on a usual install that's the operating system's (Git
//! Credential Manager and Windows Credential Manager on Windows, the
//! Keychain on macOS, libsecret on Linux when set up), and `git` itself
//! uses the same token. Where Git has no credential helper, the token
//! stays in memory for this run only, and [`Stored::MemoryOnly`] says so.

use std::sync::{Arc, Mutex};

use crate::git::Git;
use crate::secret::Secret;

/// The host Axcenity keeps GitHub's token for.
pub const GITHUB_HOST: &str = "github.com";

/// A user name and token for a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub username: String,
    pub secret: Secret,
}

/// Where a stored credential ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    /// In Git's credential store: it's there next time.
    Kept,
    /// Git has no credential helper: only until Axcenity closes.
    MemoryOnly,
}

/// Keeps credentials by host.
pub trait CredentialStore: Send + Sync {
    fn get(&self, host: &str) -> Option<Credential>;
    fn put(&self, host: &str, credential: &Credential) -> Stored;
    fn remove(&self, host: &str);
}

/// Git's credential store, with a memory copy for when it keeps nothing.
pub struct GitCredentials {
    git: Git,
    memory: Mutex<Vec<(String, Credential)>>,
}

impl GitCredentials {
    pub fn new(git: Git) -> GitCredentials {
        GitCredentials {
            git,
            memory: Mutex::new(Vec::new()),
        }
    }

    fn ask(&self, verb: &str, host: &str, credential: Option<&Credential>) -> Option<String> {
        let mut input = format!("protocol=https\nhost={host}\n");
        if let Some(c) = credential {
            input.push_str(&format!(
                "username={}\npassword={}\n",
                c.username,
                c.secret.expose()
            ));
        }
        input.push('\n');
        // `fill` mustn't open a window or ask in a terminal: an empty
        // answer is better than a prompt in the middle of a sync.
        let dir = std::env::temp_dir();
        let output = self
            .git
            .try_run(
                &dir,
                &["-c", "core.askPass=", "credential", verb],
                Some(input),
            )
            .ok()?;
        output.success.then_some(output.stdout)
    }
}

impl CredentialStore for GitCredentials {
    fn get(&self, host: &str) -> Option<Credential> {
        if let Some((_, c)) = self.memory.lock().unwrap().iter().find(|(h, _)| h == host) {
            return Some(c.clone());
        }
        let answer = self.ask("fill", host, None)?;
        let mut username = None;
        let mut password = None;
        for line in answer.lines() {
            if let Some(v) = line.strip_prefix("username=") {
                username = Some(v.to_owned());
            } else if let Some(v) = line.strip_prefix("password=") {
                password = Some(v.to_owned());
            }
        }
        let password = password.filter(|p| !p.is_empty())?;
        Some(Credential {
            username: username.unwrap_or_default(),
            secret: Secret::new(password),
        })
    }

    fn put(&self, host: &str, credential: &Credential) -> Stored {
        {
            let mut memory = self.memory.lock().unwrap();
            memory.retain(|(h, _)| h != host);
            memory.push((host.to_owned(), credential.clone()));
        }
        let _ = self.ask("approve", host, Some(credential));
        // Whether a helper kept it: ask again, without the memory copy.
        let kept = self.ask("fill", host, None).is_some_and(|answer| {
            answer
                .lines()
                .any(|l| l.strip_prefix("password=") == Some(credential.secret.expose()))
        });
        if kept {
            Stored::Kept
        } else {
            Stored::MemoryOnly
        }
    }

    fn remove(&self, host: &str) {
        let removed = {
            let mut memory = self.memory.lock().unwrap();
            let found = memory
                .iter()
                .find(|(h, _)| h == host)
                .map(|(_, c)| c.clone());
            memory.retain(|(h, _)| h != host);
            found
        };
        let credential = removed.or_else(|| self.get(host));
        if let Some(c) = credential {
            let _ = self.ask("reject", host, Some(&c));
        }
    }
}

/// Keeps credentials in memory only (tests, and a fallback).
#[derive(Default, Clone)]
pub struct MemoryCredentials(Arc<Mutex<Vec<(String, Credential)>>>);

impl CredentialStore for MemoryCredentials {
    fn get(&self, host: &str) -> Option<Credential> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|(h, _)| h == host)
            .map(|(_, c)| c.clone())
    }

    fn put(&self, host: &str, credential: &Credential) -> Stored {
        let mut all = self.0.lock().unwrap();
        all.retain(|(h, _)| h != host);
        all.push((host.to_owned(), credential.clone()));
        Stored::MemoryOnly
    }

    fn remove(&self, host: &str) {
        self.0.lock().unwrap().retain(|(h, _)| h != host);
    }
}

/// The token of a signed-in GitHub CLI (`gh auth token`), if `gh` is
/// installed and signed in: another way to sign in without an OAuth App.
pub fn github_cli_token() -> Option<Secret> {
    let mut command = std::process::Command::new("gh");
    command
        .args(["auth", "token", "--hostname", GITHUB_HOST])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !token.is_empty()).then(|| Secret::new(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_credentials_keep_one_per_host() {
        let store = MemoryCredentials::default();
        let first = Credential {
            username: "paul".into(),
            secret: Secret::new("one"),
        };
        let second = Credential {
            username: "paul".into(),
            secret: Secret::new("two"),
        };
        assert_eq!(store.put(GITHUB_HOST, &first), Stored::MemoryOnly);
        store.put(GITHUB_HOST, &second);
        assert_eq!(store.get(GITHUB_HOST), Some(second));
        store.remove(GITHUB_HOST);
        assert_eq!(store.get(GITHUB_HOST), None);
    }

    /// With a credential helper set up only for this test's own config
    /// (Git's plain-file `store`, in a temp folder), a token put in comes
    /// back, and is gone after removing it.
    #[test]
    fn git_credentials_round_trip_through_a_helper() {
        let home = std::env::temp_dir().join(format!("axcenity-repo-cred-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let file = home.join("store");
        let runner = crate::testing::ConfigRunner::new(vec![(
            "credential.helper".into(),
            format!("store --file={}", file.display()),
        )]);
        let store = GitCredentials::new(Git::with_runner(std::sync::Arc::new(runner)));
        let credential = Credential {
            username: "x-access-token".into(),
            secret: Secret::new("gho_roundtriptoken12345"),
        };
        assert_eq!(store.put("example.com", &credential), Stored::Kept);
        // A fresh store (a later run) finds it.
        let runner = crate::testing::ConfigRunner::new(vec![(
            "credential.helper".into(),
            format!("store --file={}", file.display()),
        )]);
        let later = GitCredentials::new(Git::with_runner(std::sync::Arc::new(runner)));
        assert_eq!(later.get("example.com"), Some(credential));
        later.remove("example.com");
        assert_eq!(later.get("example.com"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// With no helper at all, the token is kept in memory and says so.
    #[test]
    fn without_a_helper_the_token_is_memory_only() {
        let runner =
            crate::testing::ConfigRunner::new(vec![("credential.helper".into(), String::new())]);
        let store = GitCredentials::new(Git::with_runner(std::sync::Arc::new(runner)));
        let credential = Credential {
            username: "x-access-token".into(),
            secret: Secret::new("gho_memoryonlytoken12345"),
        };
        assert_eq!(store.put("example.org", &credential), Stored::MemoryOnly);
        assert_eq!(store.get("example.org"), Some(credential));
    }
}
