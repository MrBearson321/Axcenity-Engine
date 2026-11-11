//! Repository-level release steps: checking that `Cargo.toml`, the tag and
//! `releases/` agree, and preparing a release record from the unreleased
//! change entries (`axcenity-release check` and `prepare`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::history::{HISTORY_DIR, History, LoadError, Problem, UNRELEASED_DIR};
use crate::records::{Evidence, ReleaseRecord};
use crate::version::Version;

/// The folder of release files, at the repository root.
pub const RELEASES_DIR: &str = "releases";

#[derive(Debug, Error)]
pub enum PrepareError {
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error("could not {action} `{}`", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`Cargo.toml` has no `version = \"…\"` under `[workspace.package]`")]
    NoWorkspaceVersion,
    #[error("Cargo.toml's version: {0}")]
    BadWorkspaceVersion(String),
    #[error("{version} is not newer than the latest recorded release, {latest}")]
    NotNewer { version: Version, latest: Version },
    #[error("there is already a record for {0}")]
    AlreadyRecorded(Version),
    #[error("there are no unreleased change entries to release")]
    NothingToRelease,
    #[error("the release files have problems; run `axcenity-release check`:\n{}", list(.0))]
    Invalid(Vec<Problem>),
}

fn list(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|p| format!("  {}: {}", p.file, p.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `[workspace.package] version` in a root `Cargo.toml`'s text.
pub fn workspace_version(cargo_toml: &str) -> Result<Version, PrepareError> {
    let (_, line) = find_workspace_version(cargo_toml).ok_or(PrepareError::NoWorkspaceVersion)?;
    let text = quoted(line).ok_or(PrepareError::NoWorkspaceVersion)?;
    Version::parse(text).map_err(|err| PrepareError::BadWorkspaceVersion(err.to_string()))
}

/// The text with `[workspace.package] version` replaced, all else kept.
pub fn set_workspace_version(cargo_toml: &str, version: &Version) -> Result<String, PrepareError> {
    let (index, _) = find_workspace_version(cargo_toml).ok_or(PrepareError::NoWorkspaceVersion)?;
    Ok(replace_line(
        cargo_toml,
        index,
        &format!("version = \"{version}\""),
    ))
}

/// `Cargo.lock` with every engine crate's version set: the packages named
/// `axcenity-…` that have no `source` (they come from this workspace).
pub fn set_lock_versions(lock: &str, version: &Version) -> String {
    let mut out = String::with_capacity(lock.len());
    for block in split_keep(lock, "[[package]]") {
        let ours = block.lines().any(|l| l.starts_with("name = \"axcenity-"))
            && !block.lines().any(|l| l.starts_with("source = "));
        if ours {
            for line in block.split_inclusive('\n') {
                if line.starts_with("version = ") {
                    out.push_str(&format!("version = \"{version}\""));
                    if line.ends_with('\n') {
                        out.push('\n');
                    }
                } else {
                    out.push_str(line);
                }
            }
        } else {
            out.push_str(block);
        }
    }
    out
}

/// Everything wrong for the repository at `repo`: the history's own
/// problems, a `Cargo.toml` version that is older than a release, and, for
/// `tag`, a tag that isn't Cargo.toml's version or has no record.
pub fn check(repo: &Path, tag: Option<&str>) -> Result<Vec<Problem>, PrepareError> {
    let history = History::load(&repo.join(RELEASES_DIR))?;
    let mut problems = history.validate();
    let cargo_path = repo.join("Cargo.toml");
    let cargo = read(&cargo_path)?;
    let version = match workspace_version(&cargo) {
        Ok(version) => version,
        Err(err) => {
            problems.push(Problem {
                file: "Cargo.toml".into(),
                message: err.to_string(),
            });
            return Ok(problems);
        }
    };
    let recorded = history.release(&version).is_some();
    if let Some(latest) = history.latest()
        && !recorded
        && version <= latest.version
    {
        problems.push(Problem {
            file: "Cargo.toml".into(),
            message: format!(
                "version {version} is not newer than the latest recorded release, {}",
                latest.version
            ),
        });
    }
    if let Some(tag) = tag {
        match Version::from_tag(tag) {
            Err(err) => problems.push(Problem {
                file: "tag".into(),
                message: err.to_string(),
            }),
            Ok(tagged) => {
                if tagged != version {
                    problems.push(Problem {
                        file: "tag".into(),
                        message: format!(
                            "{tag} doesn't match Cargo.toml's version {version}; tag v{version}, or prepare {tagged}"
                        ),
                    });
                } else if !recorded {
                    problems.push(Problem {
                        file: "tag".into(),
                        message: format!(
                            "{version} has no record in {RELEASES_DIR}/{HISTORY_DIR}/: run `axcenity-release prepare {version}` first"
                        ),
                    });
                }
            }
        }
    }
    Ok(problems)
}

/// Starts the release record for `version`: moves every unreleased entry
/// into `releases/history/<version>.toml` (title and summary left for a
/// person), sets the version in `Cargo.toml` and `Cargo.lock`, and deletes
/// the moved entries. Returns the new record's path.
pub fn prepare(repo: &Path, version: &Version, date: &str) -> Result<PathBuf, PrepareError> {
    let releases = repo.join(RELEASES_DIR);
    let history = History::load(&releases)?;
    let problems = history.validate();
    if !problems.is_empty() {
        return Err(PrepareError::Invalid(problems));
    }
    if history.release(version).is_some() {
        return Err(PrepareError::AlreadyRecorded(version.clone()));
    }
    if let Some(latest) = history.latest()
        && version <= &latest.version
    {
        return Err(PrepareError::NotNewer {
            version: version.clone(),
            latest: latest.version.clone(),
        });
    }
    if history.unreleased().is_empty() {
        return Err(PrepareError::NothingToRelease);
    }

    let record = ReleaseRecord {
        version: version.clone(),
        date: date.to_owned(),
        title: String::new(),
        summary: String::new(),
        evidence: Evidence::Verified,
        published: true,
        note: String::new(),
        commit: String::new(),
        formats: Default::default(),
        verification: Default::default(),
        changes: history
            .unreleased()
            .iter()
            .map(|(_, c)| c.clone())
            .collect(),
        corrections: Vec::new(),
    };
    let text = format!(
        "# Write `title` and `summary`, then run `axcenity-release check`.\n{}",
        toml::to_string(&record).expect("a release record always serializes")
    );
    let record_path = releases.join(HISTORY_DIR).join(format!("{version}.toml"));
    write(&record_path, &text)?;

    let cargo_path = repo.join("Cargo.toml");
    let cargo = read(&cargo_path)?;
    write(&cargo_path, &set_workspace_version(&cargo, version)?)?;
    let lock_path = repo.join("Cargo.lock");
    if lock_path.exists() {
        let lock = read(&lock_path)?;
        write(&lock_path, &set_lock_versions(&lock, version))?;
    }
    for (name, _) in history.unreleased() {
        let path = releases.join(UNRELEASED_DIR).join(format!("{name}.toml"));
        fs::remove_file(&path).map_err(|source| PrepareError::Io {
            action: "delete",
            path,
            source,
        })?;
    }
    Ok(record_path)
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::build_info::day_of(seconds)
}

/// The line index and text of `version = …` in `[workspace.package]`.
fn find_workspace_version(text: &str) -> Option<(usize, &str)> {
    let mut in_section = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == "[workspace.package]";
        } else if in_section && trimmed.starts_with("version") {
            let rest = trimmed["version".len()..].trim_start();
            if rest.starts_with('=') {
                return Some((index, trimmed));
            }
        }
    }
    None
}

fn quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    Some(&line[start..end])
}

fn replace_line(text: &str, index: usize, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i == index {
            out.push_str(with);
            if line.ends_with("\r\n") {
                out.push_str("\r\n");
            } else if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Splits before each `marker` that starts a line, keeping every byte.
fn split_keep<'a>(text: &'a str, marker: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut search = 0;
    while let Some(found) = text[search..].find(marker) {
        let at = search + found;
        if at > start && (at == 0 || text.as_bytes()[at - 1] == b'\n') {
            parts.push(&text[start..at]);
            start = at;
        }
        search = at + marker.len();
    }
    parts.push(&text[start..]);
    parts
}

fn read(path: &Path) -> Result<String, PrepareError> {
    fs::read_to_string(path).map_err(|source| PrepareError::Io {
        action: "read",
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, text: &str) -> Result<(), PrepareError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| PrepareError::Io {
            action: "create",
            path: parent.to_owned(),
            source,
        })?;
    }
    fs::write(path, text).map_err(|source| PrepareError::Io {
        action: "write",
        path: path.to_owned(),
        source,
    })
}
