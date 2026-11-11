//! Which build this program is: version, channel, official or local, the
//! commit and toolchain it came from. Recorded at compile time by
//! `build.rs`; shown by `--version`, the About box and Copy diagnostics.

use std::fmt;

use serde::Serialize;

use crate::version::{Channel, Version};

/// How a build was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildKind {
    /// Made by the release workflow from a release tag.
    Official,
    /// Built on a developer's machine (`cargo build`, `cargo run`).
    Local,
}

/// This program's build. [`BuildInfo::current`] is the running one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    pub version: Version,
    pub channel: Channel,
    pub kind: BuildKind,
    /// The full commit hash, or empty when it couldn't be read (a source
    /// copy without git).
    pub commit: String,
    /// The release tag, for official builds.
    pub tag: Option<String>,
    /// Seconds since 1970 (UTC) of the source: `SOURCE_DATE_EPOCH` or the
    /// commit's time. `None` when neither was known.
    pub source_date: Option<u64>,
    pub rustc: String,
    pub target: String,
    pub profile: String,
}

impl BuildInfo {
    /// The running program's build.
    pub fn current() -> BuildInfo {
        let version = Version::current();
        BuildInfo {
            channel: version.channel(),
            version,
            kind: if env!("AXCENITY_BUILD_KIND") == "official" {
                BuildKind::Official
            } else {
                BuildKind::Local
            },
            commit: env!("AXCENITY_BUILD_COMMIT").to_owned(),
            tag: Some(env!("AXCENITY_BUILD_TAG"))
                .filter(|t| !t.is_empty())
                .map(str::to_owned),
            source_date: env!("AXCENITY_BUILD_SOURCE_DATE").parse().ok(),
            rustc: env!("AXCENITY_BUILD_RUSTC").to_owned(),
            target: env!("AXCENITY_BUILD_TARGET").to_owned(),
            profile: env!("AXCENITY_BUILD_PROFILE").to_owned(),
        }
    }

    /// The first 9 characters of the commit, or "unknown commit".
    pub fn short_commit(&self) -> &str {
        if self.commit.is_empty() {
            "unknown commit"
        } else {
            &self.commit[..self.commit.len().min(9)]
        }
    }

    /// One line for an About box or a log: "0.2.0-alpha.1 · Development ·
    /// local build of 1a2b3c4d5".
    pub fn summary(&self) -> String {
        let how = match self.kind {
            BuildKind::Official => "official build".to_owned(),
            BuildKind::Local => format!("local build of {}", self.short_commit()),
        };
        format!("{} · {} · {how}", self.version, self.channel)
    }

    /// The source date as `YYYY-MM-DD` (UTC).
    pub fn source_day(&self) -> Option<String> {
        self.source_date.map(day_of)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("build info always serializes")
    }
}

/// `--version`'s text, for any Axcenity program: its name and the build.
impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.summary())?;
        writeln!(
            f,
            "commit: {}",
            if self.commit.is_empty() {
                "unknown"
            } else {
                &self.commit
            }
        )?;
        if let Some(tag) = &self.tag {
            writeln!(f, "tag: {tag}")?;
        }
        if let Some(day) = self.source_day() {
            writeln!(f, "source date: {day}")?;
        }
        writeln!(f, "rustc: {}", self.rustc)?;
        write!(f, "target: {} ({} profile)", self.target, self.profile)
    }
}

/// `--version` and `--version --json` for a program's command line: returns
/// the text to print when `args` ask for it.
pub fn version_flag(program: &str, args: &[String]) -> Option<String> {
    if !args.iter().any(|a| a == "--version" || a == "-V") {
        return None;
    }
    let info = BuildInfo::current();
    Some(if args.iter().any(|a| a == "--json") {
        info.to_json()
    } else {
        format!("{program} {info}")
    })
}

/// Days since 1970 to a civil date (proleptic Gregorian), so no date crate
/// is needed. Howard Hinnant's `civil_from_days`.
pub(crate) fn day_of(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_build_knows_its_version_target_and_profile() {
        let info = BuildInfo::current();
        assert_eq!(info.version, Version::current());
        assert_eq!(info.channel, info.version.channel());
        assert!(!info.target.is_empty());
        assert!(!info.profile.is_empty());
        assert!(info.rustc.starts_with("rustc "), "{}", info.rustc);
        // Tests are never made by the release workflow.
        assert_eq!(info.kind, BuildKind::Local);
        assert!(info.tag.is_none());
    }

    #[test]
    fn a_local_build_names_its_commit_in_the_summary() {
        let mut info = BuildInfo::current();
        info.commit = "0123456789abcdef0123456789abcdef01234567".to_owned();
        info.kind = BuildKind::Local;
        assert!(info.summary().ends_with("local build of 012345678"));
        info.commit.clear();
        assert!(info.summary().ends_with("local build of unknown commit"));
        info.kind = BuildKind::Official;
        assert!(info.summary().ends_with("official build"));
    }

    #[test]
    fn version_flag_answers_only_when_asked() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(version_flag("axcenity-player", &args(&["examples/hello-part"])).is_none());
        let text = version_flag("axcenity-player", &args(&["--version"])).unwrap();
        assert!(text.starts_with(&format!("axcenity-player {}", Version::current())));
        let json = version_flag("axcenity-player", &args(&["--version", "--json"])).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["kind"], "local");
    }

    #[test]
    fn source_dates_print_as_utc_days() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(951_782_400), "2000-02-29");
        // alpha.3's commit: 2026-10-21T04:11:55Z.
        assert_eq!(day_of(1_792_555_915), "2026-10-21");
    }
}
