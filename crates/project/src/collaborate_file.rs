//! `collaborate.json`: what makes a folder a Collaborate project that
//! outlives its sessions (Collaborate 2.0, `docs/research/collaborate-persistent.md`
//! §4.1): its persistent ID, its name, and the remote repository it's
//! linked to, if any.
//!
//! Nothing secret goes here: no tokens, keys or members (the repository
//! may be public). It syncs to members like any settings file and is
//! committed with the project, so a fresh clone knows its ID and remote.
//! A project without the file works as before.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{LoadError, SaveError};

/// The file's name, at the project's root.
pub const COLLABORATE_FILE: &str = "collaborate.json";

/// The contents of `collaborate.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborateFile {
    /// The project's persistent ID (16 hex digits): every host hosts
    /// under it, every member keeps one folder for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The name sessions show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The linked remote, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryLink>,
}

/// A remote repository a project is linked to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryLink {
    /// `github`, or `git` for any other Git host.
    pub provider: String,
    /// The HTTPS URL to clone (never with a user name or token in it).
    pub url: String,
    /// The branch checkpoints go to.
    pub branch: String,
}

/// The project's `collaborate.json`; the default when there's none.
pub fn load_collaborate_file(folder: &Path) -> Result<CollaborateFile, LoadError> {
    let path = folder.join(COLLABORATE_FILE);
    if !path.exists() {
        return Ok(CollaborateFile::default());
    }
    let text = crate::read(&path)?;
    serde_json::from_str(&text).map_err(|e| LoadError::Read {
        path,
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })
}

/// Writes the project's `collaborate.json` (removing it when empty).
pub fn save_collaborate_file(folder: &Path, file: &CollaborateFile) -> Result<(), SaveError> {
    let path = folder.join(COLLABORATE_FILE);
    if *file == CollaborateFile::default() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(SaveError::new(&path))?;
        }
        return Ok(());
    }
    if let Some(link) = &file.repository
        && link.url.contains('@')
        && link.url.contains("://")
    {
        // A URL with credentials in it must never be written.
        return Err(SaveError::new(&path)(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a repository URL with a user name or password in it isn't kept",
        )));
    }
    let mut text = serde_json::to_string_pretty(file)
        .map_err(|e| SaveError::new(&path)(std::io::Error::other(e)))?;
    text.push('\n');
    crate::write_through_temporary(&path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axcenity-collab-file-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_project_without_the_file_has_none_of_it() {
        let dir = temp("none");
        assert_eq!(
            load_collaborate_file(&dir).unwrap(),
            CollaborateFile::default()
        );
    }

    #[test]
    fn the_file_round_trips_and_empties_away() {
        let dir = temp("round");
        let file = CollaborateFile {
            project: Some("0123456789abcdef".into()),
            name: Some("Castle".into()),
            repository: Some(RepositoryLink {
                provider: "github".into(),
                url: "https://github.com/paul/castle.git".into(),
                branch: "main".into(),
            }),
        };
        save_collaborate_file(&dir, &file).unwrap();
        assert_eq!(load_collaborate_file(&dir).unwrap(), file);
        let text = std::fs::read_to_string(dir.join(COLLABORATE_FILE)).unwrap();
        assert!(text.contains("\"branch\": \"main\""), "{text}");
        save_collaborate_file(&dir, &CollaborateFile::default()).unwrap();
        assert!(!dir.join(COLLABORATE_FILE).exists());
    }

    #[test]
    fn a_url_with_a_token_is_never_written() {
        let dir = temp("token");
        let file = CollaborateFile {
            repository: Some(RepositoryLink {
                provider: "github".into(),
                url: "https://x-access-token:gho_abc@github.com/paul/castle.git".into(),
                branch: "main".into(),
            }),
            ..CollaborateFile::default()
        };
        assert!(save_collaborate_file(&dir, &file).is_err());
        assert!(!dir.join(COLLABORATE_FILE).exists());
    }
}
