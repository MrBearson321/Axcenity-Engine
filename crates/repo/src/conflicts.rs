//! Files changed both here and in the repository (Collaborate 2.0, P5 of
//! `docs/research/collaborate-persistent.md` §4.7).
//!
//! A sync pass that finds conflicts never merges and never overwrites:
//!
//! 1. [`keep_copies`]: the repository's version of each conflicting file is
//!    written beside this one as `<stem>.remote-<commit>.<ext>`
//!    (`Door.server.lua` → `Door.server.remote-1a2b3c4.lua`, which no
//!    loader takes for a script), so both can be opened and compared. The
//!    copies are listed in the repository's own `info/exclude`, so a
//!    checkpoint never uploads them by accident.
//! 2. [`list`]: what each side did to each file (who, when, how big, gone),
//!    and whether it's text, for the panel.
//! 3. [`resolve`]: once every file has a choice (keep **mine**, keep
//!    **theirs**, keep **both** with theirs as the copy), the merge is made
//!    with those choices as one checkpoint that says what was chosen; the
//!    copies go (or, for "both", become part of the project). Each choice is
//!    also written to the repository's own `axcenity/conflicts.jsonl`
//!    (never uploaded). Mine stays in the history either way: the local
//!    checkpoint is one of the merge's parents.
//!
//! A merge left half-made (the editor stopped during [`resolve`]) is undone
//! by the next pass ([`crate::sync::recover_interrupted`]): the conflicts
//! come back and nothing is lost.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::checkpoint::{self, Kind};
use crate::git::{Author, Git, GitError, MergeCheck};
use crate::repository::{REMOTE, git_dir};

/// Marks the block of `info/exclude` this module keeps.
const EXCLUDE_BEGIN: &str = "# Axcenity: conflict copies, until they're resolved (begin)";
const EXCLUDE_END: &str = "# Axcenity: conflict copies (end)";

/// What to keep of a file changed on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// This computer's version; the repository's is left in the history.
    Mine,
    /// The repository's version; this computer's is left in the history.
    Theirs,
    /// This computer's at the path, the repository's beside it as the copy.
    Both,
}

impl Choice {
    pub fn parse(word: &str) -> Option<Choice> {
        match word.trim().to_lowercase().as_str() {
            "mine" | "ours" | "local" => Some(Choice::Mine),
            "theirs" | "remote" | "repository" => Some(Choice::Theirs),
            "both" => Some(Choice::Both),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Choice::Mine => "mine",
            Choice::Theirs => "theirs",
            Choice::Both => "both",
        }
    }
}

/// One side's version of a conflicting file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// Gone on this side (deleted).
    pub deleted: bool,
    pub size: Option<u64>,
    /// The last commit on this side that changed the file.
    pub commit: Option<String>,
    pub author: Option<String>,
    /// Seconds since 1970.
    pub time: Option<i64>,
}

/// A file changed both here and in the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub mine: Version,
    pub theirs: Version,
    /// Both sides are text (a side-by-side compare makes sense).
    pub text: bool,
    /// Where the repository's version was written beside this one, when it
    /// has one.
    pub copy: Option<String>,
}

/// What [`resolve`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The merge checkpoint.
    pub commit: String,
    pub entries: Vec<Entry>,
}

/// One resolution, as kept in `axcenity/conflicts.jsonl`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Seconds since 1970.
    pub at: u64,
    pub path: String,
    pub choice: Choice,
    /// The two commits that disagreed.
    pub mine: String,
    pub theirs: String,
    /// The merge checkpoint that recorded it.
    pub commit: String,
    pub by: String,
}

/// Why [`resolve`] didn't merge.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("there's nothing to resolve: the project and the repository agree")]
    Nothing,
    #[error("choose what to keep of {0}")]
    Undecided(String),
    #[error(transparent)]
    Git(#[from] GitError),
}

/// `path` with `.remote-<short commit>` before its last extension.
pub fn copy_path(path: &str, theirs: &str) -> String {
    let short: String = theirs.chars().take(7).collect();
    let (folder, name) = match path.rfind('/') {
        Some(at) => (&path[..=at], &path[at + 1..]),
        None => ("", path),
    };
    let (stem, extension) = match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at..]),
        _ => (name, ""),
    };
    format!("{folder}{stem}.remote-{short}{extension}")
}

/// The repository's branch as last fetched, and its commit.
fn theirs(git: &Git, dir: &Path, branch: &str) -> Result<(String, String), GitError> {
    let name = format!("{REMOTE}/{branch}");
    let sha = git
        .rev(dir, &name)
        .ok_or_else(|| GitError::Failed(format!("there's no {name} yet")))?;
    Ok((name, sha))
}

/// The files that conflict between this copy and the repository's branch
/// as last fetched (none when they merge cleanly).
fn conflicting(git: &Git, dir: &Path, theirs: &str) -> Result<Vec<String>, GitError> {
    if git.is_ancestor(dir, theirs, "HEAD") {
        return Ok(Vec::new());
    }
    let ours = git.head(dir).unwrap_or_default();
    Ok(match git.merge_check(dir, &ours, theirs)? {
        MergeCheck::Clean => Vec::new(),
        MergeCheck::Conflicts(files) => files,
    })
}

fn version(git: &Git, dir: &Path, rev: &str, path: &str) -> Version {
    let spec = format!("{rev}:{path}");
    let size = git
        .run(dir, &["cat-file", "-s", &spec])
        .ok()
        .and_then(|s| s.trim().parse().ok());
    let last = git
        .run(
            dir,
            &["log", "-1", "--format=%H%x1f%an%x1f%at", rev, "--", path],
        )
        .unwrap_or_default();
    let mut fields = last.trim().split('\u{1f}');
    let commit = fields.next().filter(|s| !s.is_empty()).map(str::to_owned);
    let author = fields.next().map(str::to_owned);
    let time = fields.next().and_then(|t| t.parse().ok());
    Version {
        deleted: size.is_none(),
        size,
        commit,
        author,
        time,
    }
}

/// Text on both sides: not stored with Git LFS, and `git diff --numstat`
/// counts lines (a binary file shows `-`).
fn is_text(git: &Git, dir: &Path, theirs: &str, path: &str) -> bool {
    let lfs = git
        .run(dir, &["check-attr", "filter", "--", path])
        .is_ok_and(|out| out.trim_end().ends_with(": lfs"));
    !lfs && git
        .run(dir, &["diff", "--numstat", "HEAD", theirs, "--", path])
        .is_ok_and(|out| !out.starts_with('-'))
}

/// The conflicts between this copy and the repository, for the panel.
pub fn list(git: &Git, dir: &Path, branch: &str) -> Result<Vec<Conflict>, GitError> {
    let (name, sha) = theirs(git, dir, branch)?;
    Ok(conflicting(git, dir, &name)?
        .into_iter()
        .map(|path| {
            let theirs_version = version(git, dir, &name, &path);
            let copy = (!theirs_version.deleted)
                .then(|| copy_path(&path, &sha))
                .filter(|copy| dir.join(copy).exists());
            Conflict {
                mine: version(git, dir, "HEAD", &path),
                text: is_text(git, dir, &name, &path),
                theirs: theirs_version,
                copy,
                path,
            }
        })
        .collect())
}

/// Writes the repository's version of each of `paths` beside this one
/// (not when it deleted the file) and keeps the copies out of
/// checkpoints. Returns the copies written.
pub fn keep_copies(
    git: &Git,
    dir: &Path,
    branch: &str,
    paths: &[String],
) -> Result<Vec<String>, GitError> {
    let (name, sha) = theirs(git, dir, branch)?;
    let Some(git_dir) = git_dir(git, dir) else {
        return Ok(Vec::new());
    };
    // The repository's files are checked out, filters and all (line
    // endings, LFS), from a temporary index into a folder inside Git's
    // own, then moved beside this copy's.
    let work = git_dir.join("axcenity").join("copies");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(io)?;
    let index = git_dir.join("axcenity").join("copies.index");
    let env = [("GIT_INDEX_FILE", index.as_path())];
    git.run_env(dir, &["read-tree", &name], &env)?;
    let prefix = format!("{}/", work.to_string_lossy().replace('\\', "/"));
    let mut copies = Vec::new();
    for path in paths {
        let spec = format!("{name}:{path}");
        if git.run(dir, &["cat-file", "-e", &spec]).is_err() {
            continue;
        }
        let prefix_arg = format!("--prefix={prefix}");
        if git
            .run_env(
                dir,
                &["checkout-index", "--force", &prefix_arg, "--", path],
                &env,
            )
            .is_err()
        {
            continue;
        }
        let copy = copy_path(path, &sha);
        let target = dir.join(&copy);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::rename(work.join(path), &target).map_err(io)?;
        copies.push(copy);
    }
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&index);
    set_excluded(git, dir, &copies)?;
    Ok(copies)
}

fn io(error: std::io::Error) -> GitError {
    GitError::Failed(error.to_string())
}

/// Replaces this module's block of the repository's `info/exclude` with
/// `copies` (none: the block goes).
fn set_excluded(git: &Git, dir: &Path, copies: &[String]) -> Result<(), GitError> {
    let Some(git_dir) = git_dir(git, dir) else {
        return Ok(());
    };
    let path = git_dir.join("info").join("exclude");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut kept = Vec::new();
    let mut inside = false;
    for line in old.lines() {
        if line == EXCLUDE_BEGIN {
            inside = true;
        } else if line == EXCLUDE_END {
            inside = false;
        } else if !inside {
            kept.push(line.to_owned());
        }
    }
    if !copies.is_empty() {
        kept.push(EXCLUDE_BEGIN.to_owned());
        for copy in copies {
            // Anchored to the folder, with Git's pattern characters escaped.
            let escaped: String = copy
                .chars()
                .flat_map(|c| match c {
                    '*' | '?' | '[' | '\\' | '!' | '#' => vec!['\\', c],
                    c => vec![c],
                })
                .collect();
            kept.push(format!("/{escaped}"));
        }
        kept.push(EXCLUDE_END.to_owned());
    }
    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(&path, text).map_err(io)
}

/// Merges the repository's branch with a choice for every conflicting
/// file; see the module notes. Changes not checkpointed yet are
/// checkpointed first. On any failure the merge is undone and the folder
/// is as it was.
pub fn resolve(
    git: &Git,
    dir: &Path,
    branch: &str,
    choices: &[(String, Choice)],
    author: &Author,
) -> Result<Resolved, ResolveError> {
    crate::sync::recover_interrupted(git, dir);
    if !git.changes(dir)?.is_empty() {
        let message = checkpoint::message(Kind::Auto, None, &[&author.name], 0);
        git.commit_all(dir, &message, author)?;
    }
    let (name, sha) = theirs(git, dir, branch)?;
    let files = conflicting(git, dir, &name)?;
    if files.is_empty() {
        return Err(ResolveError::Nothing);
    }
    let choice_of = |path: &str| {
        choices
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, choice)| *choice)
    };
    if let Some(missing) = files.iter().find(|path| choice_of(path).is_none()) {
        return Err(ResolveError::Undecided(format!("`{missing}`")));
    }
    let mine = git.head(dir).unwrap_or_default();
    // Last time's copies go first: the merge writes over the folder.
    let old_copies: Vec<String> = files
        .iter()
        .map(|path| copy_path(path, &sha))
        .filter(|copy| dir.join(copy).exists())
        .collect();
    for copy in &old_copies {
        let _ = std::fs::remove_file(dir.join(copy));
    }
    let result = merge_with(git, dir, &name, &sha, &files, &choice_of, author);
    if result.is_err() {
        let _ = git.run(dir, &["merge", "--abort"]);
        // Put the copies back for the next try.
        let _ = keep_copies(git, dir, branch, &files);
        return Err(result.expect_err("checked").into());
    }
    let commit = git.head(dir).unwrap_or_default();
    set_excluded(git, dir, &[])?;
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let entries: Vec<Entry> = files
        .iter()
        .map(|path| Entry {
            at,
            path: path.clone(),
            choice: choice_of(path).expect("checked"),
            mine: mine.clone(),
            theirs: sha.clone(),
            commit: commit.clone(),
            by: author.name.clone(),
        })
        .collect();
    if let Err(error) = append_log(git, dir, &entries) {
        log::warn!("Collaborate: couldn't keep the record of the resolved conflicts: {error}");
    }
    Ok(Resolved { commit, entries })
}

fn merge_with(
    git: &Git,
    dir: &Path,
    theirs: &str,
    sha: &str,
    files: &[String],
    choice_of: &dyn Fn(&str) -> Option<Choice>,
    author: &Author,
) -> Result<(), GitError> {
    // The copies are about to be dealt with: no longer kept out.
    set_excluded(git, dir, &[])?;
    let name = format!("user.name={}", author.name);
    let email = format!("user.email={}", author.email);
    // Conflicts stop it before committing (exit 1): that's expected.
    git.try_run(
        dir,
        &[
            "-c",
            &name,
            "-c",
            &email,
            "merge",
            "--no-ff",
            "--no-commit",
            "--no-verify",
            theirs,
        ],
        None,
    )?;
    let mut detail = Vec::new();
    for path in files {
        let choice = choice_of(path).expect("checked by the caller");
        let has = |stage: u8| {
            let spec = format!(":{stage}:{path}");
            git.run(dir, &["cat-file", "-e", &spec]).is_ok()
        };
        let (ours, theirs_has) = (has(2), has(3));
        let take = |side: &str, present: bool| -> Result<(), GitError> {
            if present {
                git.run(dir, &["checkout", side, "--", path])?;
                git.run(dir, &["add", "--", path])?;
            } else {
                git.run(
                    dir,
                    &["rm", "--quiet", "--force", "--ignore-unmatch", "--", path],
                )?;
            }
            Ok(())
        };
        match choice {
            Choice::Mine => {
                take("--ours", ours)?;
                detail.push(format!("Kept mine: {path}"));
            }
            Choice::Theirs => {
                take("--theirs", theirs_has)?;
                detail.push(format!("Kept the repository's: {path}"));
            }
            Choice::Both => {
                if theirs_has {
                    let copy = copy_path(path, sha);
                    git.run(dir, &["checkout", "--theirs", "--", path])?;
                    let target = dir.join(&copy);
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(io)?;
                    }
                    std::fs::rename(dir.join(path), &target).map_err(io)?;
                    git.run(dir, &["add", "--", &copy])?;
                    detail.push(format!("Kept both: {path} (the repository's as {copy})"));
                } else {
                    detail.push(format!("Kept mine: {path} (the repository deleted it)"));
                }
                take("--ours", ours)?;
            }
        }
    }
    let unmerged = git.run(dir, &["diff", "--name-only", "--diff-filter=U"])?;
    if !unmerged.trim().is_empty() {
        return Err(GitError::Failed(format!(
            "still unresolved: {}",
            unmerged.trim().replace('\n', ", ")
        )));
    }
    let mut message =
        checkpoint::message(Kind::Merge, Some("conflicts resolved"), &[&author.name], 0);
    // The choices go in the body, before the trailers.
    if let Some(at) = message.find("\n\n") {
        message.insert_str(at + 1, &format!("\n{}\n", detail.join("\n")));
    }
    let args = [
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
    let output = git.try_run(dir, &args, Some(message))?;
    if !output.success {
        return Err(GitError::Failed(output.stderr));
    }
    Ok(())
}

/// Where this repository's resolutions are kept: inside its own Git
/// folder, so they're never uploaded.
pub fn log_path(git: &Git, dir: &Path) -> Option<PathBuf> {
    git_dir(git, dir).map(|d| d.join("axcenity").join("conflicts.jsonl"))
}

fn append_log(git: &Git, dir: &Path, entries: &[Entry]) -> std::io::Result<()> {
    let Some(path) = log_path(git, dir) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for entry in entries {
        let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}

/// The resolutions kept for this repository, oldest first.
pub fn history(git: &Git, dir: &Path) -> Vec<Entry> {
    log_path(git, dir)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "conflicts_tests.rs"]
mod tests;
