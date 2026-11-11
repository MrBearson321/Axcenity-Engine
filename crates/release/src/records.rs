//! The hand-written release files in `releases/`, as they are on disk
//! (`docs/research/release-system/schemas.md`). Unknown fields are refused,
//! so a typo can't silently drop information.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::version::Version;

/// What kind of change an entry is. `Internal` is never published.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Listed first everywhere: what breaks matters most to someone updating.
    Breaking,
    Feature,
    Improvement,
    Performance,
    Fix,
    Deprecation,
    Security,
    Internal,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Breaking,
        Category::Feature,
        Category::Improvement,
        Category::Performance,
        Category::Fix,
        Category::Deprecation,
        Category::Security,
        Category::Internal,
    ];

    /// The heading in release notes.
    pub fn heading(self) -> &'static str {
        match self {
            Category::Feature => "New features",
            Category::Improvement => "Improvements",
            Category::Performance => "Performance",
            Category::Fix => "Bug fixes",
            Category::Breaking => "Breaking changes",
            Category::Deprecation => "Deprecated",
            Category::Security => "Security and stability",
            Category::Internal => "Internal",
        }
    }
}

/// An engine area, the Version Explorer's filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Subsystem {
    Renderer,
    Physics,
    Animation,
    Terrain,
    Materials,
    Assets,
    Audio,
    Ui,
    Scripting,
    Editor,
    Project,
    Networking,
    Player,
    Gameplay,
    Navigation,
    Build,
    Toolbox,
    Release,
}

impl Subsystem {
    pub const ALL: [Subsystem; 18] = [
        Subsystem::Renderer,
        Subsystem::Physics,
        Subsystem::Animation,
        Subsystem::Terrain,
        Subsystem::Materials,
        Subsystem::Assets,
        Subsystem::Audio,
        Subsystem::Ui,
        Subsystem::Scripting,
        Subsystem::Editor,
        Subsystem::Project,
        Subsystem::Networking,
        Subsystem::Player,
        Subsystem::Gameplay,
        Subsystem::Navigation,
        Subsystem::Build,
        Subsystem::Toolbox,
        Subsystem::Release,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Subsystem::Renderer => "renderer",
            Subsystem::Physics => "physics",
            Subsystem::Animation => "animation",
            Subsystem::Terrain => "terrain",
            Subsystem::Materials => "materials",
            Subsystem::Assets => "assets",
            Subsystem::Audio => "audio",
            Subsystem::Ui => "ui",
            Subsystem::Scripting => "scripting",
            Subsystem::Editor => "editor",
            Subsystem::Project => "project",
            Subsystem::Networking => "networking",
            Subsystem::Player => "player",
            Subsystem::Gameplay => "gameplay",
            Subsystem::Navigation => "navigation",
            Subsystem::Build => "build",
            Subsystem::Toolbox => "toolbox",
            Subsystem::Release => "release",
        }
    }
}

impl fmt::Display for Subsystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

impl FromStr for Subsystem {
    type Err = String;

    fn from_str(text: &str) -> Result<Subsystem, String> {
        Subsystem::ALL
            .into_iter()
            .find(|s| s.key().eq_ignore_ascii_case(text))
            .ok_or_else(|| {
                let names: Vec<_> = Subsystem::ALL.iter().map(|s| s.key()).collect();
                format!(
                    "`{text}` is not a subsystem: use one of {}",
                    names.join(", ")
                )
            })
    }
}

/// How sure a historical fact is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    /// Recorded when it happened, or checked against the tagged source.
    #[default]
    Verified,
    /// Written afterwards from pull requests, commits and roadmap documents.
    Reconstructed,
    /// Not established. Never shown as a guess.
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stability {
    #[default]
    Stable,
    /// Listed under "Experimental", never as a finished feature.
    Experimental,
}

/// A version, the not-yet-released work, or not known.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VersionRef {
    Release(Version),
    Unreleased,
    Unknown,
}

impl VersionRef {
    pub fn release(&self) -> Option<&Version> {
        match self {
            VersionRef::Release(version) => Some(version),
            _ => None,
        }
    }
}

impl fmt::Display for VersionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionRef::Release(version) => version.fmt(f),
            VersionRef::Unreleased => f.write_str("unreleased"),
            VersionRef::Unknown => f.write_str("unknown"),
        }
    }
}

impl FromStr for VersionRef {
    type Err = crate::version::VersionError;

    fn from_str(text: &str) -> Result<VersionRef, Self::Err> {
        match text {
            "unreleased" => Ok(VersionRef::Unreleased),
            "unknown" | "" => Ok(VersionRef::Unknown),
            _ => Version::parse(text).map(VersionRef::Release),
        }
    }
}

impl Serialize for VersionRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for VersionRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<VersionRef, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// One change: a file in `releases/unreleased/`, or a `[[changes]]` entry
/// of a release record once released.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub category: Category,
    pub subsystem: Subsystem,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub details: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub stability: Stability,
    /// Feature IDs this change touches; each gets a row in its timeline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Issue IDs this change fixes; their fixed version follows from it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
    /// Issue IDs this change is known to have introduced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub introduces_issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
    /// Commit hashes, for work that didn't go through a pull request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,
    /// `false` keeps it out of everything published.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub public: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub evidence: Evidence,
}

impl Change {
    /// Whether it may appear in release notes and the published history.
    pub fn is_public(&self) -> bool {
        self.public && self.category != Category::Internal
    }
}

/// `releases/history/<version>.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRecord {
    pub version: Version,
    /// `YYYY-MM-DD`: the day it was published, or tagged when it never was.
    pub date: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub evidence: Evidence,
    /// `false` for a tag that never became a GitHub release.
    #[serde(default = "yes")]
    pub published: bool,
    /// Anything a reader should know about this release's record itself.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The tagged commit, once known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// Format versions in this release: `project`, `save`, `network_protocol`, …
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub formats: BTreeMap<String, u32>,
    /// What was run on real machines before tagging, by OS.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub verification: BTreeMap<String, String>,
    #[serde(default)]
    pub changes: Vec<Change>,
    /// Corrections to earlier releases' records, which never change once
    /// published.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<Correction>,
}

/// A correction to an earlier, published release's notes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    pub release: Version,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureStatus {
    Active,
    Experimental,
    Deprecated,
    Removed,
}

/// When a feature arrived, and how sure that is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Introduced {
    pub version: VersionRef,
    pub evidence: Evidence,
    /// What shows it: "crates/terrain in the tree at v0.1.0-alpha.3", "PR #45".
    #[serde(default)]
    pub source: String,
}

/// A row of a feature's timeline that no change entry covers (history
/// from before this system).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureEvent {
    pub version: VersionRef,
    pub change: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub source: String,
}

/// An entry of `releases/features/*.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub subsystem: Subsystem,
    pub description: String,
    pub status: FeatureStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub docs: String,
    pub introduced: Introduced,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_in: Option<VersionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_in: Option<VersionRef>,
    /// What to use instead, for deprecated and removed features.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub replacement: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<FeatureEvent>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub public: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueStatus {
    Open,
    Fixed,
    WontFix,
}

/// An entry of `releases/issues.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownIssue {
    pub id: String,
    pub title: String,
    pub subsystem: Subsystem,
    pub status: IssueStatus,
    pub introduced: VersionRef,
    /// Versions someone actually checked and found affected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmed_affected: Vec<Version>,
    /// Only for fixes no change entry records (before this system); otherwise
    /// the fixed version follows from the change whose `fixes` names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_in: Option<VersionRef>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workaround: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub details: String,
    /// The GitHub issue number, once there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub public: bool,
}

/// `releases/features/<subsystem>.toml`: `[[feature]]` entries.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeatureFile {
    #[serde(default)]
    pub feature: Vec<Feature>,
}

/// `releases/issues.toml`: `[[issue]]` entries.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueFile {
    #[serde(default)]
    pub issue: Vec<KnownIssue>,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
