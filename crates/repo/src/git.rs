//! The installed `git`, run as a child process (P1 of
//! `docs/research/collaborate-persistent.md`).
//!
//! Axcenity doesn't carry its own Git: users' credential helpers, Git LFS
//! and hooks all work with the `git` they already have. Everything goes
//! through [`Runner`], so tests can record what would run.
//!
//! **Tokens** are never put on a command line, in `.git/config` or in a
//! remote URL. [`Git::with_token`] makes every command it runs use a
//! one-off credential helper (`-c credential.helper=!…`) that prints the
//! token from an environment variable of that child process only. Output
//! is scrubbed with [`redact`] before it goes into an error.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::secret::{Secret, redact};

/// The environment variable the one-off credential helper reads.
const TOKEN_VAR: &str = "AXCENITY_GIT_TOKEN";

/// The helper: answers Git's `get` with the token, ignores `store` and
/// `erase` (the token is kept by [`crate::credentials`], not by this).
/// Git runs helpers starting with `!` through its shell, Git for Windows
/// included.
const TOKEN_HELPER: &str = "!f() { test \"$1\" = get && printf 'username=x-access-token\\npassword=%s\\n' \"$AXCENITY_GIT_TOKEN\"; }; f";

/// How long a local command may take.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a command that talks to the remote may take.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);

/// What a finished command said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// One run of `git`.
#[derive(Clone, Debug)]
pub struct Invocation<'a> {
    /// Where it runs (`None`: anywhere).
    pub dir: Option<&'a Path>,
    pub args: Vec<String>,
    /// Extra environment for this child only.
    pub env: Vec<(String, Secret)>,
    pub stdin: Option<String>,
    pub timeout: Duration,
}

/// Runs `git`. [`SystemRunner`] runs the real one.
pub trait Runner: Send + Sync {
    fn run(&self, call: &Invocation) -> std::io::Result<Output>;
}

/// Runs the `git` on the `PATH` (or the one given).
pub struct SystemRunner {
    pub program: PathBuf,
}

impl Default for SystemRunner {
    fn default() -> SystemRunner {
        SystemRunner {
            program: PathBuf::from("git"),
        }
    }
}

impl Runner for SystemRunner {
    fn run(&self, call: &Invocation) -> std::io::Result<Output> {
        let mut command = Command::new(&self.program);
        command
            .args(&call.args)
            .stdin(if call.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Never stop to ask in a terminal or a window: Axcenity runs
            // these in the background.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", "")
            .env("LC_ALL", "C");
        if let Some(dir) = call.dir {
            command.current_dir(dir);
        }
        for (name, value) in &call.env {
            command.env(name, value.expose());
        }
        hide_console(&mut command);
        let mut child = command.spawn()?;
        if let (Some(text), Some(mut stdin)) = (&call.stdin, child.stdin.take()) {
            stdin.write_all(text.as_bytes())?;
        }
        // Read both pipes on threads, so a chatty command never blocks on
        // a full pipe while we wait for it.
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let out = std::thread::spawn(move || read_all(out.as_mut()));
        let err = std::thread::spawn(move || read_all(err.as_mut()));
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if started.elapsed() > call.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = out.join().unwrap_or_default();
        let mut stderr = err.join().unwrap_or_default();
        if status.is_none() {
            stderr.push_str("\n(git took too long and was stopped)");
        }
        Ok(Output {
            success: status.is_some_and(|s| s.success()),
            stdout,
            stderr,
        })
    }
}

fn read_all(pipe: Option<&mut impl Read>) -> String {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe {
        let _ = pipe.read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

/// Why a Git step failed, in words for the person.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("Git isn't installed (or isn't on the PATH). Install Git from https://git-scm.com.")]
    Missing,
    #[error("couldn't reach the remote repository: {0}")]
    Network(String),
    #[error("the remote repository refused access: {0}")]
    Auth(String),
    #[error("{0}")]
    Failed(String),
}

impl GitError {
    /// True for errors that may go away by themselves (no network now).
    pub fn is_offline(&self) -> bool {
        matches!(self, GitError::Network(_))
    }
}

/// One changed path in the work tree (`git status`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The two-letter status (`" M"`, `"??"`, `"UU"`…).
    pub code: String,
    pub path: String,
}

/// A commit, as listed by [`Git::log`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub author: String,
    pub email: String,
    /// Seconds since 1970.
    pub time: u64,
    pub subject: String,
    pub body: String,
}

impl Commit {
    /// The value of trailer `key` (`Key: value` lines at the end of the
    /// message), if any.
    pub fn trailer(&self, key: &str) -> Option<&str> {
        self.body.lines().rev().find_map(|line| {
            let (k, v) = line.split_once(':')?;
            (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim())
        })
    }
}

/// Whether two commits can be merged without help (`git merge-tree`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeCheck {
    /// They merge cleanly.
    Clean,
    /// These paths conflict.
    Conflicts(Vec<String>),
}

/// The person a commit is from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// What a push did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// The remote branch is at our commit now.
    Done,
    /// The remote has commits we haven't got: fetch and integrate first.
    /// (Axcenity never forces.)
    Behind,
}

/// The installed `git`, with or without a token for the remote.
#[derive(Clone)]
pub struct Git {
    runner: Arc<dyn Runner>,
    token: Option<Secret>,
}

impl Default for Git {
    fn default() -> Git {
        Git::system()
    }
}

impl Git {
    /// The `git` on the `PATH`.
    pub fn system() -> Git {
        Git::with_runner(Arc::new(SystemRunner::default()))
    }

    pub fn with_runner(runner: Arc<dyn Runner>) -> Git {
        Git {
            runner,
            token: None,
        }
    }

    /// The same `git`, giving `token` to the remote when it asks (through
    /// an environment variable, never an argument).
    pub fn with_token(&self, token: Option<Secret>) -> Git {
        Git {
            runner: self.runner.clone(),
            token: token.filter(|t| !t.is_empty()),
        }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn call(
        &self,
        dir: Option<&Path>,
        args: &[&str],
        stdin: Option<String>,
        network: bool,
    ) -> Result<Output, GitError> {
        let mut all: Vec<String> = Vec::new();
        let mut env = Vec::new();
        if network && let Some(token) = &self.token {
            // Clear the user's helpers for this command, then add ours.
            all.extend(["-c", "credential.helper="].map(String::from));
            all.push("-c".into());
            all.push(format!("credential.helper={TOKEN_HELPER}"));
            env.push((TOKEN_VAR.to_owned(), token.clone()));
        }
        all.extend(args.iter().map(|a| a.to_string()));
        let call = Invocation {
            dir,
            args: all,
            env,
            stdin,
            timeout: if network {
                NETWORK_TIMEOUT
            } else {
                LOCAL_TIMEOUT
            },
        };
        let output = self.runner.run(&call).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitError::Missing
            } else {
                GitError::Failed(format!("couldn't run git: {error}"))
            }
        })?;
        Ok(Output {
            success: output.success,
            stdout: output.stdout,
            stderr: self.scrub(&output.stderr),
        })
    }

    fn scrub(&self, text: &str) -> String {
        match &self.token {
            Some(token) => redact(text, &[token]),
            None => redact(text, &[]),
        }
    }

    /// Runs a command that must succeed; its standard output.
    fn ok(&self, dir: Option<&Path>, args: &[&str]) -> Result<String, GitError> {
        let output = self.call(dir, args, None, false)?;
        if output.success {
            Ok(output.stdout)
        } else {
            Err(GitError::Failed(failure(args, &output.stderr)))
        }
    }

    /// Runs a command that talks to the remote.
    fn net(&self, dir: Option<&Path>, args: &[&str]) -> Result<Output, GitError> {
        let output = self.call(dir, args, None, true)?;
        if output.success {
            return Ok(output);
        }
        Err(classify(args, &output.stderr))
    }

    /// `git --version`, as (major, minor).
    pub fn version(&self) -> Result<(u32, u32), GitError> {
        let text = self.ok(None, &["--version"])?;
        parse_version(&text).ok_or_else(|| GitError::Failed(format!("unexpected `{text}`")))
    }

    /// True when Git is 2.38 or newer (`merge-tree --write-tree`).
    pub fn is_recent_enough(&self) -> Result<bool, GitError> {
        Ok(self.version()? >= (2, 38))
    }

    /// True if Git LFS is installed.
    pub fn has_lfs(&self) -> bool {
        self.call(None, &["lfs", "version"], None, false)
            .is_ok_and(|o| o.success)
    }

    /// The top folder of the work tree `dir` is in, if any.
    pub fn toplevel(&self, dir: &Path) -> Option<PathBuf> {
        let out = self
            .call(Some(dir), &["rev-parse", "--show-toplevel"], None, false)
            .ok()?;
        out.success.then(|| PathBuf::from(out.stdout.trim()))
    }

    /// Makes `dir` a repository whose first branch is `branch`.
    pub fn init(&self, dir: &Path, branch: &str) -> Result<(), GitError> {
        self.ok(Some(dir), &["init", "--quiet", "--initial-branch", branch])?;
        Ok(())
    }

    /// The commit `HEAD` is at (`None` before the first commit).
    pub fn head(&self, dir: &Path) -> Option<String> {
        self.rev(dir, "HEAD")
    }

    /// The commit `rev` names, if it exists.
    pub fn rev(&self, dir: &Path, rev: &str) -> Option<String> {
        let arg = format!("{rev}^{{commit}}");
        let out = self
            .call(
                Some(dir),
                &["rev-parse", "--verify", "--quiet", &arg],
                None,
                false,
            )
            .ok()?;
        out.success.then(|| out.stdout.trim().to_owned())
    }

    /// The branch checked out (also before its first commit).
    pub fn branch(&self, dir: &Path) -> Option<String> {
        let out = self
            .call(
                Some(dir),
                &["symbolic-ref", "--quiet", "--short", "HEAD"],
                None,
                false,
            )
            .ok()?;
        out.success.then(|| out.stdout.trim().to_owned())
    }

    /// The URL of remote `name`.
    pub fn remote_url(&self, dir: &Path, name: &str) -> Option<String> {
        let key = format!("remote.{name}.url");
        let out = self
            .call(Some(dir), &["config", "--get", &key], None, false)
            .ok()?;
        out.success.then(|| out.stdout.trim().to_owned())
    }

    /// Points remote `name` at `url`, adding it if there's none.
    pub fn set_remote(&self, dir: &Path, name: &str, url: &str) -> Result<(), GitError> {
        if self.remote_url(dir, name).is_some() {
            self.ok(Some(dir), &["remote", "set-url", name, url])?;
        } else {
            self.ok(Some(dir), &["remote", "add", name, url])?;
        }
        Ok(())
    }

    /// Removes remote `name` (and its remote-tracking branches); the files
    /// and local history stay.
    pub fn remove_remote(&self, dir: &Path, name: &str) -> Result<(), GitError> {
        if self.remote_url(dir, name).is_some() {
            self.ok(Some(dir), &["remote", "remove", name])?;
        }
        Ok(())
    }

    /// The remote's default branch (`HEAD`), and whether it has any
    /// commits at all. `url` may be a remote's name or a URL.
    pub fn remote_default_branch(
        &self,
        dir: Option<&Path>,
        url: &str,
    ) -> Result<RemoteHead, GitError> {
        let out = self.net(dir, &["ls-remote", "--symref", url, "HEAD"])?;
        let mut branch = None;
        let mut any = false;
        for line in out.stdout.lines() {
            if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
                branch = rest.split_whitespace().next().map(str::to_owned);
            } else if !line.trim().is_empty() {
                any = true;
            }
        }
        if !any {
            // An empty repository says nothing for HEAD; see if it has
            // any branch at all.
            let heads = self.net(dir, &["ls-remote", "--heads", url])?;
            any = !heads.stdout.trim().is_empty();
        }
        Ok(RemoteHead {
            branch,
            empty: !any,
        })
    }

    /// The commit the remote's `branch` is at, asked of the remote itself.
    pub fn remote_branch_sha(
        &self,
        dir: &Path,
        remote: &str,
        branch: &str,
    ) -> Result<Option<String>, GitError> {
        let reference = format!("refs/heads/{branch}");
        let out = self.net(Some(dir), &["ls-remote", remote, &reference])?;
        Ok(out
            .stdout
            .lines()
            .find(|l| l.ends_with(&reference))
            .and_then(|l| l.split_whitespace().next())
            .map(str::to_owned))
    }

    /// Fetches `branch` from `remote` into `refs/remotes/<remote>/<branch>`.
    pub fn fetch(&self, dir: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
        let spec = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
        self.net(Some(dir), &["fetch", "--quiet", "--no-tags", remote, &spec])?;
        Ok(())
    }

    /// What's changed in the work tree, untracked files included.
    pub fn changes(&self, dir: &Path) -> Result<Vec<Change>, GitError> {
        let out = self.ok(
            Some(dir),
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--no-renames",
            ],
        )?;
        Ok(out
            .split('\0')
            .filter(|e| e.len() > 3)
            .map(|entry| Change {
                code: entry[..2].to_owned(),
                path: entry[3..].to_owned(),
            })
            .collect())
    }

    /// Commits everything in the work tree with `message`, as `author`.
    /// `None` when there was nothing to commit (no empty commits).
    pub fn commit_all(
        &self,
        dir: &Path,
        message: &str,
        author: &Author,
    ) -> Result<Option<String>, GitError> {
        self.ok(Some(dir), &["add", "--all"])?;
        let staged = self.call(
            Some(dir),
            &["diff", "--cached", "--quiet", "--exit-code"],
            None,
            false,
        )?;
        let unborn = self.head(dir).is_none();
        if staged.success && !unborn {
            return Ok(None);
        }
        let name = format!("user.name={}", author.name);
        let email = format!("user.email={}", author.email);
        let mut args = vec![
            "-c",
            &name,
            "-c",
            &email,
            "commit",
            "--quiet",
            "--no-verify",
            "--file",
            "-",
        ];
        if unborn && staged.success {
            args.push("--allow-empty");
        }
        let output = self.call(Some(dir), &args, Some(message.to_owned()), false)?;
        if !output.success {
            return Err(GitError::Failed(failure(&args, &output.stderr)));
        }
        Ok(self.head(dir))
    }

    /// Pushes the checked-out commit to `remote`'s `branch` (never forced),
    /// whatever the local branch is called.
    pub fn push(&self, dir: &Path, remote: &str, branch: &str) -> Result<Pushed, GitError> {
        let spec = format!("HEAD:refs/heads/{branch}");
        match self.net(
            Some(dir),
            &["push", "--quiet", "--porcelain", remote, &spec],
        ) {
            Ok(_) => Ok(Pushed::Done),
            Err(GitError::Failed(text))
                if text.contains("non-fast-forward")
                    || text.contains("[rejected]")
                    || text.contains("fetch first") =>
            {
                Ok(Pushed::Behind)
            }
            Err(error) => Err(error),
        }
    }

    /// How many commits `HEAD` has that `other` hasn't, and the reverse.
    pub fn ahead_behind(&self, dir: &Path, other: &str) -> Result<(u32, u32), GitError> {
        let range = format!("HEAD...{other}");
        let out = self.ok(Some(dir), &["rev-list", "--left-right", "--count", &range])?;
        let mut parts = out.split_whitespace().map(|n| n.parse().unwrap_or(0));
        Ok((parts.next().unwrap_or(0), parts.next().unwrap_or(0)))
    }

    /// True if `ancestor` is `rev` or one of its ancestors.
    pub fn is_ancestor(&self, dir: &Path, ancestor: &str, rev: &str) -> bool {
        self.call(
            Some(dir),
            &["merge-base", "--is-ancestor", ancestor, rev],
            None,
            false,
        )
        .is_ok_and(|o| o.success)
    }

    /// Whether `ours` and `theirs` merge without conflicts, worked out
    /// without touching the work tree or the index.
    pub fn merge_check(
        &self,
        dir: &Path,
        ours: &str,
        theirs: &str,
    ) -> Result<MergeCheck, GitError> {
        let output = self.call(
            Some(dir),
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                ours,
                theirs,
            ],
            None,
            false,
        )?;
        if output.success {
            return Ok(MergeCheck::Clean);
        }
        // Exit 1: conflicts. The tree's ID, then one conflicting path a line.
        let mut lines = output.stdout.lines();
        let tree = lines.next().unwrap_or("");
        if tree.len() < 40 || !tree.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GitError::Failed(failure(&["merge-tree"], &output.stderr)));
        }
        let mut paths: Vec<String> = lines
            .take_while(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        paths.dedup();
        Ok(MergeCheck::Conflicts(paths))
    }

    /// Merges `theirs` into the checked-out branch. Only call after
    /// [`Self::merge_check`] said it's clean.
    pub fn merge(
        &self,
        dir: &Path,
        theirs: &str,
        message: &str,
        author: &Author,
    ) -> Result<(), GitError> {
        let name = format!("user.name={}", author.name);
        let email = format!("user.email={}", author.email);
        let args = [
            "-c",
            &name,
            "-c",
            &email,
            "merge",
            "--no-edit",
            "--no-verify",
            "-m",
            message,
            theirs,
        ];
        let output = self.call(Some(dir), &args, None, false)?;
        if output.success {
            return Ok(());
        }
        // Never leave a half-made merge behind.
        let _ = self.call(Some(dir), &["merge", "--abort"], None, false);
        Err(GitError::Failed(failure(&args, &output.stderr)))
    }

    /// Moves the checked-out branch to `rev` without touching the files:
    /// they then show as changes against it (adopting a folder onto a
    /// remote branch).
    pub fn reset_keep_files(&self, dir: &Path, rev: &str) -> Result<(), GitError> {
        self.ok(Some(dir), &["reset", "--quiet", "--mixed", rev])?;
        Ok(())
    }

    /// The newest `count` commits reachable from `rev`, newest first.
    pub fn log(&self, dir: &Path, rev: &str, count: usize) -> Result<Vec<Commit>, GitError> {
        if self.rev(dir, rev).is_none() {
            return Ok(Vec::new());
        }
        let max = format!("--max-count={count}");
        let out = self.ok(
            Some(dir),
            &[
                "log",
                &max,
                "--format=%H%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b%x1e",
                rev,
            ],
        )?;
        Ok(out
            .split('\u{1e}')
            .filter_map(|record| {
                let record = record.trim_start_matches('\n');
                let mut f = record.split('\u{1f}');
                Some(Commit {
                    sha: f.next()?.to_owned(),
                    author: f.next()?.to_owned(),
                    email: f.next()?.to_owned(),
                    time: f.next()?.parse().ok()?,
                    subject: f.next()?.to_owned(),
                    body: f.next().unwrap_or("").trim().to_owned(),
                })
            })
            .collect())
    }

    /// Clones `url`'s `branch` into `dest` (a new folder).
    pub fn clone_repository(
        &self,
        url: &str,
        dest: &Path,
        branch: Option<&str>,
    ) -> Result<(), GitError> {
        let dest = dest.to_string_lossy();
        let mut args = vec!["clone", "--quiet"];
        if let Some(branch) = branch {
            args.extend(["--branch", branch]);
        }
        args.extend(["--", url, &dest]);
        self.net(None, &args)?;
        Ok(())
    }

    /// The file at `path` as it is in `rev`, or `None` if it isn't there.
    pub fn show(&self, dir: &Path, rev: &str, path: &str) -> Option<Vec<u8>> {
        let spec = format!("{rev}:{path}");
        let out = self.call(Some(dir), &["show", &spec], None, false).ok()?;
        out.success.then(|| out.stdout.into_bytes())
    }

    /// Runs any other command that must succeed (used by sync steps here).
    pub(crate) fn run(&self, dir: &Path, args: &[&str]) -> Result<String, GitError> {
        self.ok(Some(dir), args)
    }

    /// Runs any command; its output whatever the result.
    pub(crate) fn try_run(
        &self,
        dir: &Path,
        args: &[&str],
        stdin: Option<String>,
    ) -> Result<Output, GitError> {
        self.call(Some(dir), args, stdin, false)
    }
}

/// What a remote says about its default branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteHead {
    /// The branch `HEAD` points at, when the remote says.
    pub branch: Option<String>,
    /// The remote has no commits at all.
    pub empty: bool,
}

fn failure(args: &[&str], stderr: &str) -> String {
    let what = args
        .iter()
        .find(|a| !a.starts_with('-') && !a.contains('='))
        .copied()
        .unwrap_or("git");
    let text = stderr.trim();
    if text.is_empty() {
        format!("git {what} failed")
    } else {
        format!("git {what} failed: {text}")
    }
}

/// Tells a network or sign-in problem from any other failure.
fn classify(args: &[&str], stderr: &str) -> GitError {
    let lower = stderr.to_ascii_lowercase();
    let auth = [
        "authentication failed",
        "could not read username",
        "terminal prompts disabled",
        "permission denied",
        "403",
        "invalid username or password",
        "repository not found",
    ];
    let network = [
        "could not resolve host",
        "failed to connect",
        "connection timed out",
        "connection refused",
        "network is unreachable",
        "operation timed out",
        "took too long",
        "unable to access",
        "temporary failure in name resolution",
        "could not read from remote repository",
    ];
    if auth.iter().any(|s| lower.contains(s)) {
        GitError::Auth(stderr.trim().to_owned())
    } else if network.iter().any(|s| lower.contains(s)) {
        GitError::Network(stderr.trim().to_owned())
    } else {
        GitError::Failed(failure(args, stderr))
    }
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(2)?;
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("git version 2.43.0\n"), Some((2, 43)));
        assert_eq!(parse_version("git version 2.45.1.windows.1"), Some((2, 45)));
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn errors_say_whether_its_the_network_or_access() {
        assert!(
            classify(
                &["fetch"],
                "fatal: unable to access 'https://x/': Could not resolve host: x"
            )
            .is_offline()
        );
        assert!(matches!(
            classify(
                &["push"],
                "remote: Repository not found.\nfatal: repository 'https://github.com/o/r.git/' not found"
            ),
            GitError::Auth(_)
        ));
        assert!(matches!(
            classify(&["push"], "fatal: something else"),
            GitError::Failed(_)
        ));
    }

    /// Records each call and answers with a fixed output.
    type Calls = Mutex<Vec<(Vec<String>, Vec<(String, String)>)>>;
    struct Recorder(Calls);

    impl Runner for Recorder {
        fn run(&self, call: &Invocation) -> std::io::Result<Output> {
            self.0.lock().unwrap().push((
                call.args.clone(),
                call.env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.expose().to_owned()))
                    .collect(),
            ));
            Ok(Output {
                success: false,
                stdout: String::new(),
                stderr: "fatal: unable to access 'https://x-access-token:gho_secrettoken123456@github.com/o/r.git/': Could not resolve host".into(),
            })
        }
    }

    #[test]
    fn a_token_goes_only_through_the_childs_environment() {
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let git = Git::with_runner(recorder.clone())
            .with_token(Some(Secret::new("gho_secrettoken123456")));
        let error = git.fetch(Path::new("."), "origin", "main").unwrap_err();
        // Errors are scrubbed.
        assert!(
            !error.to_string().contains("gho_secrettoken123456"),
            "{error}"
        );
        let calls = recorder.0.lock().unwrap();
        let (args, env) = &calls[0];
        assert!(
            args.iter().all(|a| !a.contains("gho_secrettoken123456")),
            "{args:?}"
        );
        assert!(args.iter().any(|a| a.starts_with("credential.helper=!")));
        assert_eq!(
            env,
            &vec![(TOKEN_VAR.to_owned(), "gho_secrettoken123456".to_owned())]
        );
        drop(calls);
        // Local commands don't get it at all.
        let _ = git.changes(Path::new("."));
        let calls = recorder.0.lock().unwrap();
        assert!(calls[1].1.is_empty());
        assert!(!calls[1].0.iter().any(|a| a.contains("credential")));
    }

    #[test]
    fn trailers_are_read_from_the_end_of_the_message() {
        let commit = Commit {
            sha: "a".into(),
            author: "Paul".into(),
            email: "p@x".into(),
            time: 0,
            subject: "[Collaborate] Automatic checkpoint".into(),
            body: "Changed 3 files.\n\nAxcenity-Checkpoint: auto\nAxcenity-Participants: Paul, Sam"
                .into(),
        };
        assert_eq!(commit.trailer("Axcenity-Checkpoint"), Some("auto"));
        assert_eq!(commit.trailer("axcenity-participants"), Some("Paul, Sam"));
        assert_eq!(commit.trailer("Missing"), None);
    }
}
