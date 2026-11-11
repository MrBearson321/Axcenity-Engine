//! Engine versions and release channels.
//!
//! The version is SemVer 2.0 with at most one pre-release tag of the form
//! `alpha.N`, `beta.N` or `rc.N`. The channel always follows from it, so
//! the two can't disagree: no suffix is Stable, `beta`/`rc` is Beta, and
//! `alpha` is Development.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Where a release is offered. A user on a channel sees releases of that
/// channel and the more stable ones ([`Channel::offers`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Pre-releases for testing and contributors (`-alpha.N`).
    Development,
    /// New features nearing completion (`-beta.N`, `-rc.N`).
    Beta,
    /// Fully validated public releases (no suffix).
    Stable,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Stable, Channel::Beta, Channel::Development];

    /// Whether someone following `self` is offered a release on `release`:
    /// Stable sees Stable, Beta sees Beta and Stable, Development sees all.
    pub fn offers(self, release: Channel) -> bool {
        release >= self
    }

    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
            Channel::Development => "Development",
        }
    }

    /// The lowercase name used in files and on the command line.
    pub fn key(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
            Channel::Development => "development",
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Channel {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Channel, VersionError> {
        match text.to_ascii_lowercase().as_str() {
            "stable" => Ok(Channel::Stable),
            "beta" => Ok(Channel::Beta),
            "development" | "dev" => Ok(Channel::Development),
            _ => Err(VersionError::UnknownChannel(text.to_owned())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VersionError {
    #[error("`{0}` is not a version like 0.2.0 or 0.2.0-alpha.1: {1}")]
    Syntax(String, String),
    #[error(
        "`{0}` has a pre-release tag Axcenity doesn't use; use -alpha.N, -beta.N or -rc.N with N from 1"
    )]
    PreRelease(String),
    #[error("`{0}` has build metadata (`+…`), which Axcenity versions don't use")]
    BuildMetadata(String),
    #[error("`{0}` is not a channel: use stable, beta or development")]
    UnknownChannel(String),
    #[error("`{0}` is not a release tag: tags are `v` and the version, such as v0.2.0-alpha.1")]
    Tag(String),
}

/// An Axcenity version: SemVer with an allowed pre-release tag.
/// Ordering is SemVer precedence (`0.2.0-alpha.1 < 0.2.0-beta.1 < 0.2.0-rc.1 < 0.2.0`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    semver: semver::Version,
    channel: Channel,
}

impl Version {
    pub fn parse(text: &str) -> Result<Version, VersionError> {
        let semver = semver::Version::parse(text)
            .map_err(|err| VersionError::Syntax(text.to_owned(), err.to_string()))?;
        if !semver.build.is_empty() {
            return Err(VersionError::BuildMetadata(text.to_owned()));
        }
        let channel = if semver.pre.is_empty() {
            Channel::Stable
        } else {
            let (tag, number) = semver
                .pre
                .as_str()
                .split_once('.')
                .ok_or_else(|| VersionError::PreRelease(text.to_owned()))?;
            let numbered = !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && !number.starts_with('0');
            match tag {
                "alpha" if numbered => Channel::Development,
                "beta" | "rc" if numbered => Channel::Beta,
                _ => return Err(VersionError::PreRelease(text.to_owned())),
            }
        };
        Ok(Version { semver, channel })
    }

    /// Parses a release tag, `v` followed by the version.
    pub fn from_tag(tag: &str) -> Result<Version, VersionError> {
        let rest = tag
            .strip_prefix('v')
            .ok_or_else(|| VersionError::Tag(tag.to_owned()))?;
        Version::parse(rest)
    }

    /// The version this program was built as (`Cargo.toml`'s workspace version).
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("the workspace version in Cargo.toml is a valid Axcenity version")
    }

    pub fn channel(&self) -> Channel {
        self.channel
    }

    pub fn is_prerelease(&self) -> bool {
        self.channel != Channel::Stable
    }

    pub fn tag(&self) -> String {
        format!("v{self}")
    }

    pub fn semver(&self) -> &semver::Version {
        &self.semver
    }

    pub fn major(&self) -> u64 {
        self.semver.major
    }

    pub fn minor(&self) -> u64 {
        self.semver.minor
    }

    pub fn patch(&self) -> u64 {
        self.semver.patch
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.semver.fmt(f)
    }
}

impl FromStr for Version {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Version, VersionError> {
        Version::parse(text)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.semver.cmp_precedence(&other.semver)
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Version, D::Error> {
        let text = String::deserialize(deserializer)?;
        Version::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_channel_follows_from_the_pre_release_tag() {
        assert_eq!(v("0.2.0").channel(), Channel::Stable);
        assert_eq!(v("1.0.0-rc.2").channel(), Channel::Beta);
        assert_eq!(v("0.3.0-beta.1").channel(), Channel::Beta);
        assert_eq!(v("0.2.0-alpha.1").channel(), Channel::Development);
        assert!(!v("0.2.0").is_prerelease());
        assert!(v("0.2.0-alpha.1").is_prerelease());
    }

    #[test]
    fn versions_order_by_semver_precedence() {
        let mut list = [
            v("0.2.0"),
            v("0.2.0-rc.1"),
            v("0.1.0-alpha.3"),
            v("0.2.0-alpha.10"),
            v("0.2.0-alpha.2"),
            v("0.2.0-beta.1"),
            v("0.10.0"),
        ];
        list.sort();
        let names: Vec<String> = list.iter().map(Version::to_string).collect();
        assert_eq!(
            names,
            [
                "0.1.0-alpha.3",
                "0.2.0-alpha.2",
                "0.2.0-alpha.10",
                "0.2.0-beta.1",
                "0.2.0-rc.1",
                "0.2.0",
                "0.10.0"
            ]
        );
    }

    #[test]
    fn unsupported_pre_release_tags_and_build_metadata_are_refused() {
        for bad in [
            "0.2.0-nightly.1",
            "0.2.0-alpha",
            "0.2.0-alpha.0",
            "0.2.0-alpha.1.2",
            "0.2.0-beta.x",
            "0.2.0-dev.1",
        ] {
            assert!(
                matches!(Version::parse(bad), Err(VersionError::PreRelease(_))),
                "{bad}"
            );
        }
        assert!(matches!(
            Version::parse("0.2.0+abc"),
            Err(VersionError::BuildMetadata(_))
        ));
        // SemVer itself forbids leading zeros.
        assert!(Version::parse("0.2.0-alpha.01").is_err());
        assert!(matches!(
            Version::parse("0.2"),
            Err(VersionError::Syntax(..))
        ));
    }

    #[test]
    fn tags_are_v_and_the_version() {
        assert_eq!(
            Version::from_tag("v0.2.0-alpha.1").unwrap(),
            v("0.2.0-alpha.1")
        );
        assert_eq!(v("0.2.0-alpha.1").tag(), "v0.2.0-alpha.1");
        assert!(matches!(
            Version::from_tag("0.2.0"),
            Err(VersionError::Tag(_))
        ));
    }

    #[test]
    fn each_channel_offers_itself_and_the_more_stable_ones() {
        use Channel::*;
        assert!(Stable.offers(Stable));
        assert!(!Stable.offers(Beta));
        assert!(!Stable.offers(Development));
        assert!(Beta.offers(Stable) && Beta.offers(Beta));
        assert!(!Beta.offers(Development));
        assert!(Development.offers(Stable) && Development.offers(Beta));
        assert!(Development.offers(Development));
    }

    #[test]
    fn the_workspace_version_is_a_valid_axcenity_version() {
        let current = Version::current();
        assert_eq!(current.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn versions_round_trip_through_serde_as_strings() {
        let json = serde_json::to_string(&v("0.2.0-beta.3")).unwrap();
        assert_eq!(json, "\"0.2.0-beta.3\"");
        let back: Version = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel(), Channel::Beta);
        assert!(serde_json::from_str::<Version>("\"0.2.0-nightly.1\"").is_err());
    }
}
