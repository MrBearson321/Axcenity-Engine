//! What a release publishes, made from its record: the release notes, the
//! public history file and the signed manifest (`axcenity-release notes`,
//! `history` and `manifest`, run by `.github/workflows/release.yml`).
//! Shapes: `docs/research/release-system/schemas.md`.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::history::History;
use crate::records::{Category, Change, Feature, KnownIssue, ReleaseRecord, Stability, Subsystem};
use crate::version::{Channel, Version};

/// The manifest's file name on a GitHub release. The only file the engine
/// trusts directly; every other download is checked against it.
pub const MANIFEST_FILE: &str = "axcenity-release.json";
/// Its minisign signature.
pub const SIGNATURE_FILE: &str = "axcenity-release.json.minisig";
/// The public history, attached to every release.
pub const HISTORY_FILE: &str = "axcenity-history.json";
/// Goes up only when a manifest field changes meaning; readers ignore
/// fields they don't know.
pub const MANIFEST_SCHEMA: u32 = 1;
/// The same rule for the history file.
pub const HISTORY_SCHEMA: u32 = 1;
/// How an update is handed to the updater. An editor offers a release only
/// if its own protocol is at least the release's `min_update_protocol`.
pub const UPDATE_PROTOCOL: u32 = 1;
/// Every package's file name starts with this, then the version.
pub const PACKAGE_PREFIX: &str = "AxcenityEditor-";

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("there is no record for {0} in releases/history/")]
    NoRecord(Version),
    #[error("the record for {0} has no {1}: write it before tagging")]
    Unwritten(Version, &'static str),
    #[error("`{0}` is a package for another version, or not one Axcenity makes")]
    UnknownPackage(String),
    #[error("no packages for {0} in `{1}`")]
    NoPackages(Version, String),
    #[error("could not read `{}`", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The public history: every release record, feature and known issue,
/// without internal entries. Unreleased entries aren't part of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryFile {
    pub schema: u32,
    pub releases: Vec<ReleaseRecord>,
    pub features: Vec<Feature>,
    pub issues: Vec<KnownIssue>,
}

impl HistoryFile {
    pub fn from_history(history: &History) -> HistoryFile {
        let public = history.public();
        HistoryFile {
            schema: HISTORY_SCHEMA,
            releases: public.releases().to_vec(),
            features: public.features().to_vec(),
            issues: public.issues().to_vec(),
        }
    }
}

/// One line of release notes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub subsystem: Subsystem,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub details: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
}

impl Note {
    fn of(change: &Change) -> Note {
        Note {
            subsystem: change.subsystem,
            title: change.title.clone(),
            details: change.details.clone(),
            prs: change.prs.clone(),
        }
    }
}

/// A release's public changes by kind. Experimental work is listed apart,
/// never as a finished feature.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notes {
    pub breaking: Vec<Note>,
    pub features: Vec<Note>,
    pub improvements: Vec<Note>,
    pub performance: Vec<Note>,
    pub fixes: Vec<Note>,
    pub deprecations: Vec<Note>,
    pub security: Vec<Note>,
    pub experimental: Vec<Note>,
}

impl Notes {
    pub fn of(record: &ReleaseRecord) -> Notes {
        let mut notes = Notes::default();
        for change in record.changes.iter().filter(|c| c.is_public()) {
            let list = if change.stability == Stability::Experimental {
                &mut notes.experimental
            } else {
                match change.category {
                    Category::Breaking => &mut notes.breaking,
                    Category::Feature => &mut notes.features,
                    Category::Improvement => &mut notes.improvements,
                    Category::Performance => &mut notes.performance,
                    Category::Fix => &mut notes.fixes,
                    Category::Deprecation => &mut notes.deprecations,
                    Category::Security => &mut notes.security,
                    Category::Internal => continue,
                }
            };
            list.push(Note::of(change));
        }
        notes
    }

    /// Each list with its heading, in release-notes order (breaking first).
    pub fn sections(&self) -> [(&'static str, &[Note]); 8] {
        [
            (Category::Breaking.heading(), &self.breaking),
            (Category::Feature.heading(), &self.features),
            (Category::Improvement.heading(), &self.improvements),
            (Category::Performance.heading(), &self.performance),
            (Category::Fix.heading(), &self.fixes),
            (Category::Deprecation.heading(), &self.deprecations),
            (Category::Security.heading(), &self.security),
            ("Experimental", &self.experimental),
        ]
    }
}

/// A file attached to the release, named rather than linked: the engine
/// builds its download address itself, so a manifest can't send it to
/// another host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compatibility {
    pub min_update_protocol: u32,
}

/// How and where the release was built. `reproducible` stays
/// "not verified" until a second build has been compared byte for byte.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Provenance {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub workflow_run: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rustc: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub profile: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub runner_images: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_date_epoch: Option<u64>,
    pub reproducible: String,
}

/// `axcenity-release.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub product: String,
    pub version: Version,
    /// For readers; the engine recomputes it from `version` and refuses a
    /// manifest where the two differ.
    pub channel: Channel,
    pub tag: String,
    pub commit: String,
    /// RFC 3339, UTC.
    pub published: String,
    pub title: String,
    pub summary: String,
    pub notes: Notes,
    /// Public known issues that affect this version.
    pub known_issues: Vec<String>,
    pub formats: BTreeMap<String, u32>,
    pub compatibility: Compatibility,
    /// Platform (`windows-x86_64`) → role (`update`, `installer`, …) → file.
    pub artifacts: BTreeMap<String, BTreeMap<String, Artifact>>,
    pub history: Artifact,
    pub provenance: Provenance,
}

/// What the workflow knows that the record doesn't.
#[derive(Clone, Debug, Default)]
pub struct BuildFacts {
    pub commit: String,
    pub published: String,
    pub provenance: Provenance,
}

impl Manifest {
    /// The manifest for `version` from its record and the packages in
    /// `dist` (which must also hold [`HISTORY_FILE`]). Every file there
    /// named like a package must be one Axcenity makes for this version.
    pub fn build(
        history: &History,
        version: &Version,
        dist: &Path,
        facts: BuildFacts,
    ) -> Result<Manifest, PublishError> {
        let record = written_record(history, version)?;
        let mut artifacts: BTreeMap<String, BTreeMap<String, Artifact>> = BTreeMap::new();
        for name in file_names(dist)? {
            if !name.starts_with(PACKAGE_PREFIX) {
                continue;
            }
            let (platform, role) = package_kind(&name, version)
                .ok_or_else(|| PublishError::UnknownPackage(name.clone()))?;
            let artifact = describe(dist, &name)?;
            artifacts
                .entry(platform.to_owned())
                .or_default()
                .insert(role.to_owned(), artifact);
        }
        if artifacts.is_empty() {
            return Err(PublishError::NoPackages(
                version.clone(),
                dist.display().to_string(),
            ));
        }
        let public = history.public();
        let known_issues = public
            .issues_affecting(version)
            .into_iter()
            .map(|(issue, _)| issue.id.clone())
            .collect();
        Ok(Manifest {
            schema: MANIFEST_SCHEMA,
            product: "axcenity-engine".to_owned(),
            version: version.clone(),
            channel: version.channel(),
            tag: version.tag(),
            commit: facts.commit,
            published: facts.published,
            title: record.title.clone(),
            summary: record.summary.clone(),
            notes: Notes::of(record),
            known_issues,
            formats: record.formats.clone(),
            compatibility: Compatibility {
                min_update_protocol: UPDATE_PROTOCOL,
            },
            artifacts,
            history: describe(dist, HISTORY_FILE)?,
            provenance: Provenance {
                reproducible: "not verified".to_owned(),
                ..facts.provenance
            },
        })
    }
}

/// The record for `version`, with its title and summary written.
pub fn written_record<'a>(
    history: &'a History,
    version: &Version,
) -> Result<&'a ReleaseRecord, PublishError> {
    let record = history
        .release(version)
        .ok_or_else(|| PublishError::NoRecord(version.clone()))?;
    if record.title.trim().is_empty() {
        return Err(PublishError::Unwritten(version.clone(), "title"));
    }
    if record.summary.trim().is_empty() {
        return Err(PublishError::Unwritten(version.clone(), "summary"));
    }
    Ok(record)
}

/// The platform and role of a package file for `version`, from its name:
/// `AxcenityEditor-<version>-<platform>[-update].<ext>`.
pub fn package_kind(file: &str, version: &Version) -> Option<(&'static str, &'static str)> {
    let rest = file
        .strip_prefix(PACKAGE_PREFIX)?
        .strip_prefix(&version.to_string())?
        .strip_prefix('-')?;
    Some(match rest {
        "windows-x86_64-update.zip" => ("windows-x86_64", "update"),
        "windows-x86_64.exe" => ("windows-x86_64", "installer"),
        // The portable archive is also what an in-app update installs.
        "linux-x86_64.tar.gz" => ("linux-x86_64", "update"),
        "linux-x86_64.AppImage" => ("linux-x86_64", "appimage"),
        "linux-x86_64.deb" => ("linux-x86_64", "deb"),
        "linux-x86_64.rpm" => ("linux-x86_64", "rpm"),
        "linux-x86_64.pkg.tar.zst" => ("linux-x86_64", "pacman"),
        "macos-universal.dmg" => ("macos-universal", "dmg"),
        "macos-arm64.dmg" => ("macos-arm64", "dmg"),
        _ => return None,
    })
}

/// Release notes in Markdown for the GitHub release: the summary, the
/// public changes by kind, known issues and corrections, then `install`
/// (the install table, with `__VERSION__` filled in).
pub fn release_notes(
    history: &History,
    version: &Version,
    install: &str,
) -> Result<String, PublishError> {
    let record = written_record(history, version)?;
    let public = history.public();
    // The title is the GitHub release's name (`release.yml`), so the body
    // starts with the summary.
    let mut out = record.summary.trim().to_owned();
    out.push_str("\n\n");
    out.push_str(match version.channel() {
        Channel::Development => {
            "This is a **Development** (alpha) release, for testing: expect bugs and breaking changes.\n\n"
        }
        Channel::Beta => {
            "This is a **Beta** release: features are nearly finished, and some bugs remain.\n\n"
        }
        Channel::Stable => "This is a **Stable** release.\n\n",
    });
    let notes = Notes::of(record);
    for (heading, list) in notes.sections() {
        if list.is_empty() {
            continue;
        }
        out.push_str(&format!("## {heading}\n\n"));
        for note in list {
            out.push_str(&format!(
                "- **{}**: {}",
                note.subsystem.name(),
                note.title.trim()
            ));
            if !note.prs.is_empty() {
                let prs: Vec<String> = note.prs.iter().map(|n| format!("#{n}")).collect();
                out.push_str(&format!(" ({})", prs.join(", ")));
            }
            out.push('\n');
            for line in note.details.trim().lines() {
                out.push_str(&format!("  {line}\n"));
            }
        }
        out.push('\n');
    }
    let issues = public.issues_affecting(version);
    if !issues.is_empty() {
        out.push_str("## Known issues\n\n");
        for (issue, _) in issues {
            out.push_str(&format!("- {}: {}", issue.id, issue.title.trim()));
            if !issue.workaround.trim().is_empty() {
                out.push_str(&format!(". Workaround: {}", issue.workaround.trim()));
            }
            out.push('\n');
        }
        out.push('\n');
    }
    if !record.corrections.is_empty() {
        out.push_str("## Corrections to earlier releases\n\n");
        for correction in &record.corrections {
            out.push_str(&format!(
                "- {}: {}\n",
                correction.release,
                correction.text.trim()
            ));
        }
        out.push('\n');
    }
    if !install.trim().is_empty() {
        out.push_str(&install.replace("__VERSION__", &version.to_string()));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// SHA-256 of a file as lowercase hex, read in pieces.
pub fn sha256_file(path: &Path) -> Result<(String, u64), PublishError> {
    let io_err = |source| PublishError::Io {
        path: path.to_owned(),
        source,
    };
    let mut file = fs::File::open(path).map_err(io_err)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(io_err)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((hex, size))
}

fn describe(dist: &Path, name: &str) -> Result<Artifact, PublishError> {
    let (sha256, size) = sha256_file(&dist.join(name))?;
    Ok(Artifact {
        file: name.to_owned(),
        sha256,
        size,
    })
}

fn file_names(dir: &Path) -> Result<Vec<String>, PublishError> {
    let io_err = |source| PublishError::Io {
        path: dir.to_owned(),
        source,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        if entry.file_type().map_err(io_err)?.is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}
