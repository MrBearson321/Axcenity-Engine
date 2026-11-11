//! The release history: every release record, the unreleased change
//! entries, the feature registry and the known issues, loaded from
//! `releases/`, checked, and queried. This is the Version Explorer's data
//! model; the editor, the release tooling and the MCP toolkit all read it
//! through these functions, never the files.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::records::{
    Category, Change, Evidence, Feature, FeatureFile, FeatureStatus, FormatsFile, Introduced,
    IssueFile, IssueStatus, KnownIssue, ReleaseRecord, Subsystem, VersionRef,
};
use crate::version::Version;

/// The folders and files of `releases/`.
pub const HISTORY_DIR: &str = "history";
pub const UNRELEASED_DIR: &str = "unreleased";
pub const FEATURES_DIR: &str = "features";
pub const ISSUES_FILE: &str = "issues.toml";
/// The formats the code writes now (`releases/formats.toml`); `prepare`
/// copies them into the release record.
pub const FORMATS_FILE: &str = "formats.toml";

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("could not read `{}`", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{}` is not valid: {message}", path.display())]
    Parse { path: PathBuf, message: String },
}

/// Something wrong in the release files, with the file it's in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub file: String,
    pub message: String,
}

/// A query that couldn't be answered. Each case is its own value so a
/// caller (the MCP toolkit, the Version Explorer) can offer "did you mean".
#[derive(Clone, Debug, PartialEq, Eq, Error, Serialize)]
#[serde(tag = "error", rename_all = "kebab-case")]
pub enum QueryError {
    #[error("no feature `{id}`{}", did_you_mean(suggestions))]
    UnknownFeature {
        id: String,
        suggestions: Vec<String>,
    },
    #[error("no known issue `{id}`{}", did_you_mean(suggestions))]
    UnknownIssue {
        id: String,
        suggestions: Vec<String>,
    },
    #[error("no release `{version}` in the history{}", did_you_mean(suggestions))]
    UnknownVersion {
        version: String,
        suggestions: Vec<String>,
    },
    #[error("`{version}` is not a version: {message}")]
    InvalidVersion { version: String, message: String },
    #[error("comparisons go from an older release to a newer one (or `unreleased`)")]
    BackwardsRange,
    #[error("this page token belongs to another search, or is damaged")]
    InvalidCursor,
}

fn did_you_mean(suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        String::new()
    } else {
        format!("; did you mean {}?", suggestions.join(", "))
    }
}

/// Where version *V* stands with a known issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum IssueState {
    /// Fixed in `fixed_in`, which is not newer than *V*.
    Fixed { fixed_in: VersionRef },
    /// Someone checked *V* and found it affected.
    ConfirmedAffected,
    /// *V* is in the range from the issue's introduction to its fix, but
    /// nobody checked *V* itself.
    AffectedByRange,
    /// The issue was introduced after *V*.
    BeforeIntroduced { introduced: VersionRef },
    /// Not established either way. Never read this as "not affected".
    NotInvestigated,
}

impl IssueState {
    pub fn affects(&self) -> bool {
        matches!(
            self,
            IssueState::ConfirmedAffected | IssueState::AffectedByRange
        )
    }
}

/// One row of a feature's timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TimelineEntry {
    pub version: VersionRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    pub change: String,
    pub evidence: Evidence,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// A change as a query returns it: with the version it belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeRef {
    pub version: VersionRef,
    pub category: Category,
    pub subsystem: Subsystem,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub details: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
    pub evidence: Evidence,
    pub experimental: bool,
}

/// A format whose version differs between the two ends of a comparison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FormatChange {
    pub format: String,
    pub from: Option<u32>,
    pub to: Option<u32>,
}

/// What changed after `from`, up to and including `to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Comparison {
    pub from: VersionRef,
    pub to: VersionRef,
    /// Changes by category, breaking ones first.
    pub changes: BTreeMap<Category, Vec<ChangeRef>>,
    pub formats: Vec<FormatChange>,
    pub features_introduced: Vec<String>,
    pub features_deprecated: Vec<String>,
    pub features_removed: Vec<String>,
    pub issues_introduced: Vec<String>,
    pub issues_fixed: Vec<String>,
}

/// What a search looks through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HitKind {
    Feature,
    Issue,
    Change,
    Release,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// Words that must all appear (any case). Empty lists everything the
    /// filters allow.
    pub text: String,
    pub subsystem: Option<Subsystem>,
    pub kinds: Vec<HitKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub kind: HitKind,
    /// Feature or issue ID, or the version for changes and releases.
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<Subsystem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionRef>,
    #[serde(skip)]
    score: u32,
}

/// One page of answers. `next` goes back into the same query for the next
/// page; it is `None` on the last one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub next: Option<String>,
}

/// The whole history, releases oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    releases: Vec<ReleaseRecord>,
    /// File name (without `.toml`) and entry.
    unreleased: Vec<(String, Change)>,
    features: Vec<Feature>,
    issues: Vec<KnownIssue>,
    /// Problems found while loading (a record whose file name isn't its
    /// version), reported by [`History::validate`].
    load_problems: Vec<Problem>,
    /// From [`FORMATS_FILE`]; empty without one.
    formats: BTreeMap<String, u32>,
}

impl History {
    pub fn new(
        mut releases: Vec<ReleaseRecord>,
        mut unreleased: Vec<(String, Change)>,
        features: Vec<Feature>,
        issues: Vec<KnownIssue>,
    ) -> History {
        releases.sort_by(|a, b| a.version.cmp(&b.version));
        unreleased.sort_by(|a, b| a.0.cmp(&b.0));
        History {
            releases,
            unreleased,
            features,
            issues,
            load_problems: Vec::new(),
            formats: BTreeMap::new(),
        }
    }

    /// The file formats the code writes now, by name (`project`, `save`, …).
    pub fn formats(&self) -> &BTreeMap<String, u32> {
        &self.formats
    }

    /// Reads a `releases/` folder. Missing parts are empty.
    pub fn load(dir: &Path) -> Result<History, LoadError> {
        let mut sources = Vec::new();
        for sub in [HISTORY_DIR, UNRELEASED_DIR, FEATURES_DIR] {
            for path in toml_files(&dir.join(sub))? {
                sources.push((display(dir, &path), read(&path)?));
            }
        }
        for file in [ISSUES_FILE, FORMATS_FILE] {
            let path = dir.join(file);
            if path.exists() {
                sources.push((file.to_owned(), read(&path)?));
            }
        }
        History::from_sources(sources)
    }

    /// The history from the release files' texts, each with its path
    /// relative to `releases/` (`history/0.2.0-alpha.1.toml`, `issues.toml`,
    /// …), as [`History::load`] reads them from disk. The editor builds the
    /// repository's files in this way, so the Version Explorer works offline.
    pub fn from_sources<P, T>(
        sources: impl IntoIterator<Item = (P, T)>,
    ) -> Result<History, LoadError>
    where
        P: AsRef<str>,
        T: AsRef<str>,
    {
        let mut sources: Vec<(String, String)> = sources
            .into_iter()
            .map(|(p, t)| (p.as_ref().replace('\\', "/"), t.as_ref().to_owned()))
            .collect();
        sources.sort();
        let mut releases = Vec::new();
        let mut problems = Vec::new();
        let mut unreleased = Vec::new();
        let mut features = Vec::new();
        let mut issues = Vec::new();
        let mut formats = BTreeMap::new();
        for (path, text) in &sources {
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            let Some(stem) = name.strip_suffix(".toml") else {
                continue;
            };
            match folder {
                HISTORY_DIR => {
                    let record: ReleaseRecord = parse_text(path, text)?;
                    if stem != record.version.to_string() {
                        problems.push(Problem {
                            file: path.clone(),
                            message: format!(
                                "the file is named `{stem}` but records version {}",
                                record.version
                            ),
                        });
                    }
                    releases.push(record);
                }
                UNRELEASED_DIR => {
                    unreleased.push((stem.to_owned(), parse_text::<Change>(path, text)?))
                }
                FEATURES_DIR => features.extend(parse_text::<FeatureFile>(path, text)?.feature),
                "" if name == ISSUES_FILE => issues = parse_text::<IssueFile>(path, text)?.issue,
                "" if name == FORMATS_FILE => {
                    formats = parse_text::<FormatsFile>(path, text)?.formats;
                }
                _ => {}
            }
        }
        let mut history = History::new(releases, unreleased, features, issues);
        history.load_problems = problems;
        history.formats = formats;
        Ok(history)
    }

    pub fn releases(&self) -> &[ReleaseRecord] {
        &self.releases
    }

    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    pub fn issues(&self) -> &[KnownIssue] {
        &self.issues
    }

    /// The unreleased change entries, with their file names.
    pub fn unreleased(&self) -> &[(String, Change)] {
        &self.unreleased
    }

    pub fn release(&self, version: &Version) -> Option<&ReleaseRecord> {
        self.releases.iter().find(|r| &r.version == version)
    }

    /// The newest release that was published (not a tag that never was).
    pub fn latest_published(&self) -> Option<&ReleaseRecord> {
        self.releases.iter().rev().find(|r| r.published)
    }

    /// The newest release record of any kind.
    pub fn latest(&self) -> Option<&ReleaseRecord> {
        self.releases.last()
    }

    /// Only what may be published: internal changes, features and issues
    /// are dropped, and public entries stop naming non-public ones.
    pub fn public(&self) -> History {
        let features: Vec<Feature> = self.features.iter().filter(|f| f.public).cloned().collect();
        let issues: Vec<KnownIssue> = self.issues.iter().filter(|i| i.public).cloned().collect();
        let feature_ids: BTreeSet<&str> = features.iter().map(|f| f.id.as_str()).collect();
        let issue_ids: BTreeSet<&str> = issues.iter().map(|i| i.id.as_str()).collect();
        let clean = |change: &Change| -> Option<Change> {
            change.is_public().then(|| {
                let mut change = change.clone();
                change
                    .features
                    .retain(|id| feature_ids.contains(id.as_str()));
                change.fixes.retain(|id| issue_ids.contains(id.as_str()));
                change
                    .introduces_issues
                    .retain(|id| issue_ids.contains(id.as_str()));
                change
            })
        };
        let releases = self
            .releases
            .iter()
            .map(|record| {
                let mut record = record.clone();
                record.changes = record.changes.iter().filter_map(clean).collect();
                record
            })
            .collect();
        let unreleased = self
            .unreleased
            .iter()
            .filter_map(|(name, change)| clean(change).map(|c| (name.clone(), c)))
            .collect();
        let mut public = History::new(releases, unreleased, features, issues);
        public.formats = self.formats.clone();
        public
    }

    /// Every change with the version it belongs to, oldest release first,
    /// unreleased last.
    pub fn changes(&self) -> impl Iterator<Item = (VersionRef, &Change)> {
        self.releases
            .iter()
            .flat_map(|r| {
                r.changes
                    .iter()
                    .map(|c| (VersionRef::Release(r.version.clone()), c))
            })
            .chain(
                self.unreleased
                    .iter()
                    .map(|(_, c)| (VersionRef::Unreleased, c)),
            )
    }

    // ---- Checks ----

    /// Everything wrong with the history; empty when it's consistent.
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = self.load_problems.clone();
        let mut push = |file: String, message: String| problems.push(Problem { file, message });
        // A format the next release changes needs a breaking change saying so.
        if let Some(latest) = self.latest() {
            let changed: Vec<String> = self
                .formats
                .iter()
                .filter_map(|(name, &now)| {
                    let then = *latest.formats.get(name)?;
                    (then != now).then(|| format!("`{name}` {then} → {now}"))
                })
                .collect();
            let said = self
                .unreleased
                .iter()
                .any(|(_, c)| c.category == Category::Breaking);
            if !changed.is_empty() && !said {
                push(
                    FORMATS_FILE.to_owned(),
                    format!(
                        "{} changed since {}, but no unreleased change is `breaking`; add one \
                         that says what an older Axcenity can no longer open",
                        changed.join(", "),
                        latest.version
                    ),
                );
            }
        }
        let versions: BTreeSet<&Version> = self.releases.iter().map(|r| &r.version).collect();
        let feature_ids: BTreeSet<&str> = self.features.iter().map(|f| f.id.as_str()).collect();
        let issue_ids: BTreeSet<&str> = self.issues.iter().map(|i| i.id.as_str()).collect();
        let known = |v: &VersionRef| match v {
            VersionRef::Release(v) => versions.contains(v),
            _ => true,
        };

        let mut seen = BTreeSet::new();
        for record in &self.releases {
            let file = format!("{HISTORY_DIR}/{}.toml", record.version);
            if !seen.insert(&record.version) {
                push(file.clone(), "this version is recorded twice".into());
            }
            if !is_date(&record.date) {
                push(
                    file.clone(),
                    format!("date `{}` is not YYYY-MM-DD", record.date),
                );
            }
            if record.title.trim().is_empty() {
                push(file.clone(), "write a title for this release".into());
            }
            if !record.commit.is_empty() && !is_commit(&record.commit) {
                push(
                    file.clone(),
                    format!("commit `{}` is not a 40-character hash", record.commit),
                );
            }
            for correction in &record.corrections {
                if correction.release >= record.version || !versions.contains(&correction.release) {
                    push(
                        file.clone(),
                        format!(
                            "a correction can only be for an earlier recorded release, not {}",
                            correction.release
                        ),
                    );
                }
            }
            for change in &record.changes {
                check_change(change, &file, &feature_ids, &issue_ids, &mut push);
            }
        }
        for (name, change) in &self.unreleased {
            let file = format!("{UNRELEASED_DIR}/{name}.toml");
            check_change(change, &file, &feature_ids, &issue_ids, &mut push);
        }

        let mut seen = BTreeSet::new();
        for feature in &self.features {
            let file = format!("{FEATURES_DIR}/{}.toml", feature.subsystem);
            let what = format!("feature `{}`", feature.id);
            if !seen.insert(feature.id.as_str()) {
                push(file.clone(), format!("{what} is defined twice"));
            }
            if !is_id(&feature.id) {
                push(
                    file.clone(),
                    format!(
                        "{what}: IDs are lowercase words joined by `.` and `-`, such as `terrain.studio`"
                    ),
                );
            }
            check_introduced(&feature.introduced, &what, &file, &known, &mut push);
            match (feature.status, &feature.deprecated_in, &feature.removed_in) {
                (FeatureStatus::Deprecated, None, _) => {
                    push(
                        file.clone(),
                        format!("{what} is deprecated: say `deprecated_in`"),
                    );
                }
                (FeatureStatus::Removed, _, None) => {
                    push(file.clone(), format!("{what} is removed: say `removed_in`"));
                }
                _ => {}
            }
            for version in [&feature.deprecated_in, &feature.removed_in]
                .into_iter()
                .flatten()
                .chain(feature.history.iter().map(|e| &e.version))
            {
                if !known(version) {
                    push(
                        file.clone(),
                        format!("{what} names {version}, which has no release record"),
                    );
                }
            }
            for event in &feature.history {
                if event.evidence != Evidence::Unknown && event.source.trim().is_empty() {
                    push(
                        file.clone(),
                        format!("{what}: history row \"{}\" needs a `source`", event.change),
                    );
                }
            }
        }

        let fixes = self.fixes_by_issue();
        let mut seen = BTreeSet::new();
        for issue in &self.issues {
            let file = ISSUES_FILE.to_owned();
            let what = format!("issue `{}`", issue.id);
            if !seen.insert(issue.id.as_str()) {
                push(file.clone(), format!("{what} is defined twice"));
            }
            for version in std::iter::once(&issue.introduced).chain(issue.fixed_in.iter()) {
                if !known(version) {
                    push(
                        file.clone(),
                        format!("{what} names {version}, which has no release record"),
                    );
                }
            }
            for version in &issue.confirmed_affected {
                if !versions.contains(version) {
                    push(
                        file.clone(),
                        format!("{what} names {version}, which has no release record"),
                    );
                }
            }
            let fixed_by_change = fixes.get(issue.id.as_str());
            if fixed_by_change.is_some() && issue.fixed_in.is_some() {
                push(
                    file.clone(),
                    format!("{what}: a change entry already records its fix, so remove `fixed_in`"),
                );
            }
            let fixed = self.fixed_in(issue);
            match (issue.status, &fixed) {
                (IssueStatus::Fixed, None) => push(
                    file.clone(),
                    format!(
                        "{what} is marked fixed, but no change fixes it and it has no `fixed_in`"
                    ),
                ),
                // An unreleased fix leaves the issue open until it ships.
                (
                    IssueStatus::Open | IssueStatus::WontFix,
                    Some(version @ VersionRef::Release(_)),
                ) => push(
                    file.clone(),
                    format!(
                        "{what} is marked {:?} but is fixed in {version}",
                        issue.status
                    ),
                ),
                _ => {}
            }
            if let Some(VersionRef::Release(fixed)) = &fixed {
                for version in &issue.confirmed_affected {
                    if version >= fixed {
                        push(
                            file.clone(),
                            format!("{what} is confirmed in {version}, but fixed in {fixed}"),
                        );
                    }
                }
            }
        }
        problems
    }

    // ---- Queries ----

    pub fn feature(&self, id: &str) -> Result<&Feature, QueryError> {
        self.features
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| QueryError::UnknownFeature {
                id: id.to_owned(),
                suggestions: suggest(
                    id,
                    self.features.iter().flat_map(|f| {
                        [
                            (f.id.as_str(), f.id.as_str()),
                            (f.name.as_str(), f.id.as_str()),
                        ]
                    }),
                    self.features.iter().map(|f| f.id.as_str()),
                ),
            })
    }

    pub fn issue(&self, id: &str) -> Result<&KnownIssue, QueryError> {
        self.issues
            .iter()
            .find(|i| i.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| QueryError::UnknownIssue {
                id: id.to_owned(),
                suggestions: suggest(
                    id,
                    self.issues.iter().map(|i| (i.id.as_str(), i.id.as_str())),
                    std::iter::empty(),
                ),
            })
    }

    /// A version that must be in the history (`unreleased` is allowed when
    /// `allow_unreleased`).
    pub fn version(&self, text: &str, allow_unreleased: bool) -> Result<VersionRef, QueryError> {
        let text = text.trim().trim_start_matches('v');
        if allow_unreleased && text == "unreleased" {
            return Ok(VersionRef::Unreleased);
        }
        let version = Version::parse(text).map_err(|err| QueryError::InvalidVersion {
            version: text.to_owned(),
            message: err.to_string(),
        })?;
        if self.release(&version).is_some() {
            Ok(VersionRef::Release(version))
        } else {
            let names: Vec<String> = self
                .releases
                .iter()
                .map(|r| r.version.to_string())
                .collect();
            Err(QueryError::UnknownVersion {
                version: text.to_owned(),
                suggestions: suggest(
                    text,
                    names.iter().map(|s| (s.as_str(), s.as_str())),
                    std::iter::empty(),
                ),
            })
        }
    }

    /// When a feature arrived.
    pub fn introduced_in(&self, id: &str) -> Result<&Introduced, QueryError> {
        Ok(&self.feature(id)?.introduced)
    }

    /// Every recorded step of a feature, oldest first: its introduction,
    /// the rows written in the registry, and each change entry naming it.
    pub fn feature_timeline(&self, id: &str) -> Result<Vec<TimelineEntry>, QueryError> {
        let feature = self.feature(id)?;
        let mut rows = vec![TimelineEntry {
            version: feature.introduced.version.clone(),
            category: Some(Category::Feature),
            change: format!("{} introduced", feature.name),
            evidence: feature.introduced.evidence,
            prs: Vec::new(),
            source: feature.introduced.source.clone(),
        }];
        rows.extend(feature.history.iter().map(|e| TimelineEntry {
            version: e.version.clone(),
            category: None,
            change: e.change.clone(),
            evidence: e.evidence,
            prs: Vec::new(),
            source: e.source.clone(),
        }));
        for (version, change) in self.changes() {
            if change.features.iter().any(|f| f == id) {
                rows.push(TimelineEntry {
                    version,
                    category: Some(change.category),
                    change: change.title.clone(),
                    evidence: change.evidence,
                    prs: change.prs.clone(),
                    source: String::new(),
                });
            }
        }
        if let Some(version) = &feature.deprecated_in {
            rows.push(TimelineEntry {
                version: version.clone(),
                category: Some(Category::Deprecation),
                change: format!("{} deprecated", feature.name),
                evidence: Evidence::Verified,
                prs: Vec::new(),
                source: feature.replacement.clone(),
            });
        }
        if let Some(version) = &feature.removed_in {
            rows.push(TimelineEntry {
                version: version.clone(),
                category: Some(Category::Breaking),
                change: format!("{} removed", feature.name),
                evidence: Evidence::Verified,
                prs: Vec::new(),
                source: feature.replacement.clone(),
            });
        }
        // Stable sort: rows of one version keep the order above.
        rows.sort_by(|a, b| version_order(&a.version).cmp(&version_order(&b.version)));
        Ok(rows)
    }

    /// The version an issue was fixed in: from the change entry that fixes
    /// it, else its own `fixed_in`.
    pub fn fixed_in(&self, issue: &KnownIssue) -> Option<VersionRef> {
        self.fixes_by_issue()
            .get(issue.id.as_str())
            .cloned()
            .or_else(|| issue.fixed_in.clone())
    }

    fn fixes_by_issue(&self) -> HashMap<&str, VersionRef> {
        let mut map = HashMap::new();
        for (version, change) in self.changes() {
            for id in &change.fixes {
                // The earliest fix wins: changes come oldest first.
                map.entry(id.as_str()).or_insert_with(|| version.clone());
            }
        }
        map
    }

    /// Whether `version` has an issue (rules in `schemas.md`, "Known issues").
    pub fn issue_status(&self, id: &str, version: &Version) -> Result<IssueState, QueryError> {
        let issue = self.issue(id)?;
        Ok(self.state_of(issue, version))
    }

    fn state_of(&self, issue: &KnownIssue, version: &Version) -> IssueState {
        let fixed = self.fixed_in(issue);
        let fixed_by = match &fixed {
            Some(VersionRef::Release(fixed_version)) => version >= fixed_version,
            // A version newer than every release is a build of the coming
            // one, which carries the unreleased changes.
            Some(VersionRef::Unreleased) => self.latest().is_none_or(|r| *version > r.version),
            _ => false,
        };
        if fixed_by {
            return IssueState::Fixed {
                fixed_in: fixed.unwrap(),
            };
        }
        if issue.confirmed_affected.contains(version) {
            return IssueState::ConfirmedAffected;
        }
        match &issue.introduced {
            VersionRef::Release(introduced) if version >= introduced => IssueState::AffectedByRange,
            VersionRef::Release(_) => IssueState::BeforeIntroduced {
                introduced: issue.introduced.clone(),
            },
            _ => IssueState::NotInvestigated,
        }
    }

    /// The issues known to affect `version` (confirmed, or by range).
    pub fn issues_affecting(&self, version: &Version) -> Vec<(&KnownIssue, IssueState)> {
        self.issues
            .iter()
            .map(|issue| (issue, self.state_of(issue, version)))
            .filter(|(_, state)| state.affects())
            .collect()
    }

    /// The first release whose record says it reads format `format` at
    /// version `n` or later.
    pub fn minimum_engine_for(&self, format: &str, n: u32) -> Option<&Version> {
        self.releases
            .iter()
            .find(|r| r.formats.get(format).is_some_and(|&have| have >= n))
            .map(|r| &r.version)
    }

    /// What changed after `from` up to and including `to` (`unreleased`
    /// for the work not released yet).
    pub fn compare(&self, from: &VersionRef, to: &VersionRef) -> Result<Comparison, QueryError> {
        let VersionRef::Release(start) = from else {
            return Err(QueryError::BackwardsRange);
        };
        let in_range = |version: &VersionRef| match (version, to) {
            (VersionRef::Release(v), VersionRef::Release(end)) => v > start && v <= end,
            (VersionRef::Release(v), VersionRef::Unreleased) => v > start,
            (VersionRef::Unreleased, VersionRef::Unreleased) => true,
            _ => false,
        };
        if let VersionRef::Release(end) = to
            && end <= start
        {
            return Err(QueryError::BackwardsRange);
        }
        if matches!(to, VersionRef::Unknown) {
            return Err(QueryError::BackwardsRange);
        }

        let mut changes: BTreeMap<Category, Vec<ChangeRef>> = BTreeMap::new();
        let mut issues_introduced = BTreeSet::new();
        let mut issues_fixed = BTreeSet::new();
        for (version, change) in self.changes() {
            if !in_range(&version) {
                continue;
            }
            issues_introduced.extend(change.introduces_issues.iter().cloned());
            issues_fixed.extend(change.fixes.iter().cloned());
            changes.entry(change.category).or_default().push(ChangeRef {
                version,
                category: change.category,
                subsystem: change.subsystem,
                title: change.title.clone(),
                details: change.details.clone(),
                prs: change.prs.clone(),
                evidence: change.evidence,
                experimental: change.stability == crate::records::Stability::Experimental,
            });
        }
        for issue in &self.issues {
            if in_range(&issue.introduced) {
                issues_introduced.insert(issue.id.clone());
            }
            if let Some(fixed) = &issue.fixed_in
                && in_range(fixed)
            {
                issues_fixed.insert(issue.id.clone());
            }
        }
        let pick = |of: fn(&Feature) -> Option<&VersionRef>| -> Vec<String> {
            self.features
                .iter()
                .filter(|f| of(f).is_some_and(&in_range))
                .map(|f| f.id.clone())
                .collect()
        };

        let formats_at = |v: &VersionRef| -> BTreeMap<String, u32> {
            match v {
                VersionRef::Release(v) => self
                    .release(v)
                    .map(|r| r.formats.clone())
                    .unwrap_or_default(),
                _ => BTreeMap::new(),
            }
        };
        let (before, after) = (formats_at(from), formats_at(to));
        let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
        let formats = if after.is_empty() {
            Vec::new()
        } else {
            names
                .into_iter()
                .filter(|name| before.get(*name) != after.get(*name))
                .map(|name| FormatChange {
                    format: name.clone(),
                    from: before.get(name).copied(),
                    to: after.get(name).copied(),
                })
                .collect()
        };

        Ok(Comparison {
            from: from.clone(),
            to: to.clone(),
            changes,
            formats,
            features_introduced: pick(|f| Some(&f.introduced.version)),
            features_deprecated: pick(|f| f.deprecated_in.as_ref()),
            features_removed: pick(|f| f.removed_in.as_ref()),
            issues_introduced: issues_introduced.into_iter().collect(),
            issues_fixed: issues_fixed.into_iter().collect(),
        })
    }

    /// Searches features, issues, changes and releases. Pass the previous
    /// page's `next` as `cursor` with the same query for the next page.
    pub fn search(
        &self,
        query: &SearchQuery,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Page<SearchHit>, QueryError> {
        let fingerprint = query_fingerprint(query);
        let offset = match cursor {
            None => 0,
            Some(token) => parse_cursor(token, fingerprint)?,
        };
        let words: Vec<String> = query
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let wanted = |kind: HitKind| query.kinds.is_empty() || query.kinds.contains(&kind);
        let subsystem_ok = |s: Option<Subsystem>| query.subsystem.is_none() || s == query.subsystem;
        let mut hits = Vec::new();
        let mut consider = |kind, id: String, title: String, subsystem, version, other: &[&str]| {
            if !wanted(kind) || !subsystem_ok(subsystem) {
                return;
            }
            if let Some(score) = score(&words, &id, &title, other) {
                hits.push(SearchHit {
                    kind,
                    id,
                    title,
                    subsystem,
                    version,
                    score,
                });
            }
        };
        for f in &self.features {
            consider(
                HitKind::Feature,
                f.id.clone(),
                f.name.clone(),
                Some(f.subsystem),
                Some(f.introduced.version.clone()),
                &[f.description.as_str(), f.subsystem.key()],
            );
        }
        for i in &self.issues {
            consider(
                HitKind::Issue,
                i.id.clone(),
                i.title.clone(),
                Some(i.subsystem),
                Some(i.introduced.clone()),
                &[i.details.as_str(), i.workaround.as_str(), i.subsystem.key()],
            );
        }
        for (version, change) in self.changes() {
            consider(
                HitKind::Change,
                version.to_string(),
                change.title.clone(),
                Some(change.subsystem),
                Some(version.clone()),
                &[
                    change.details.as_str(),
                    change.subsystem.key(),
                    category_word(change.category),
                ],
            );
        }
        for r in &self.releases {
            consider(
                HitKind::Release,
                r.version.to_string(),
                r.title.clone(),
                None,
                Some(VersionRef::Release(r.version.clone())),
                &[r.summary.as_str(), r.version.channel().key()],
            );
        }
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.kind.cmp(&b.kind))
                .then(version_order_opt(&b.version).cmp(&version_order_opt(&a.version)))
                .then(a.id.cmp(&b.id))
                .then(a.title.cmp(&b.title))
        });
        let total = hits.len();
        let limit = limit.max(1);
        let items: Vec<SearchHit> = hits.into_iter().skip(offset).take(limit).collect();
        let end = offset + items.len();
        Ok(Page {
            items,
            total,
            next: (end < total).then(|| make_cursor(end, fingerprint)),
        })
    }
}

fn check_change(
    change: &Change,
    file: &str,
    features: &BTreeSet<&str>,
    issues: &BTreeSet<&str>,
    push: &mut impl FnMut(String, String),
) {
    let what = format!("change \"{}\"", change.title);
    if change.title.trim().is_empty() {
        push(file.to_owned(), "a change needs a title".into());
    }
    if change.category == Category::Breaking && change.details.trim().is_empty() {
        push(
            file.to_owned(),
            format!("{what} is breaking: say in `details` what breaks and what to do"),
        );
    }
    if change.evidence == Evidence::Reconstructed
        && change.prs.is_empty()
        && change.commits.is_empty()
    {
        push(
            file.to_owned(),
            format!("{what} is reconstructed: name the `prs` or `commits` it comes from"),
        );
    }
    for commit in &change.commits {
        if commit.len() < 7 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            push(
                file.to_owned(),
                format!("{what}: `{commit}` is not a commit hash"),
            );
        }
    }
    for id in &change.features {
        if !features.contains(id.as_str()) {
            push(
                file.to_owned(),
                format!("{what} names feature `{id}`, which isn't in releases/features/"),
            );
        }
    }
    for id in change.fixes.iter().chain(&change.introduces_issues) {
        if !issues.contains(id.as_str()) {
            push(
                file.to_owned(),
                format!("{what} names issue `{id}`, which isn't in releases/issues.toml"),
            );
        }
    }
}

fn check_introduced(
    introduced: &Introduced,
    what: &str,
    file: &str,
    known: &impl Fn(&VersionRef) -> bool,
    push: &mut impl FnMut(String, String),
) {
    if !known(&introduced.version) {
        push(
            file.to_owned(),
            format!(
                "{what} was introduced in {}, which has no release record",
                introduced.version
            ),
        );
    }
    let unknown_version = introduced.version == VersionRef::Unknown;
    if unknown_version != (introduced.evidence == Evidence::Unknown) {
        push(
            file.to_owned(),
            format!("{what}: an unknown introduction has evidence `unknown`, and only then"),
        );
    }
    if !unknown_version && introduced.source.trim().is_empty() {
        push(
            file.to_owned(),
            format!("{what}: say what shows its introduction in `introduced.source`"),
        );
    }
}

/// Sort key: releases in order, then unreleased; unknown first.
fn version_order(version: &VersionRef) -> (u8, Option<&Version>) {
    match version {
        VersionRef::Unknown => (0, None),
        VersionRef::Release(v) => (1, Some(v)),
        VersionRef::Unreleased => (2, None),
    }
}

fn version_order_opt(version: &Option<VersionRef>) -> (u8, Option<&Version>) {
    version.as_ref().map_or((0, None), version_order)
}

fn category_word(category: Category) -> &'static str {
    match category {
        Category::Feature => "feature",
        Category::Improvement => "improvement",
        Category::Performance => "performance",
        Category::Fix => "fix bug",
        Category::Breaking => "breaking",
        Category::Deprecation => "deprecated",
        Category::Security => "security",
        Category::Internal => "internal",
    }
}

/// Every word must appear somewhere; the title and ID count more.
fn score(words: &[String], id: &str, title: &str, other: &[&str]) -> Option<u32> {
    let id = id.to_lowercase();
    let title = title.to_lowercase();
    let other: Vec<String> = other.iter().map(|s| s.to_lowercase()).collect();
    let mut total = 0;
    for word in words {
        let mut points = 0;
        if id == *word {
            points += 10;
        } else if id.contains(word.as_str()) {
            points += 3;
        }
        if title.contains(word.as_str()) {
            points += 3;
        }
        if other.iter().any(|s| s.contains(word.as_str())) {
            points += 1;
        }
        if points == 0 {
            return None;
        }
        total += points;
    }
    Some(total)
}

/// Up to three names close to `wanted`: containing it, or a few edits away.
/// `prefer` maps a matched name to what to suggest (a feature's ID for its
/// name). `extra` adds IDs that contain `wanted`.
fn suggest<'a>(
    wanted: &str,
    names: impl Iterator<Item = (&'a str, &'a str)>,
    extra: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let wanted_lower = wanted.to_lowercase();
    let mut scored: Vec<(usize, String)> = Vec::new();
    for (name, id) in names {
        let lower = name.to_lowercase();
        let distance = if lower.contains(&wanted_lower) || wanted_lower.contains(&lower) {
            0
        } else {
            edit_distance(&wanted_lower, &lower)
        };
        if distance <= (wanted_lower.chars().count() / 3).max(2) {
            scored.push((distance, id.to_owned()));
        }
    }
    for id in extra {
        if id.to_lowercase().contains(&wanted_lower) {
            scored.push((0, id.to_owned()));
        }
    }
    scored.sort();
    let mut out: Vec<String> = Vec::new();
    for (_, id) in scored {
        if !out.contains(&id) {
            out.push(id);
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// FNV-1a over the query, so a page token only works with its own query,
/// and gives the same token in every build.
fn query_fingerprint(query: &SearchQuery) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= u32::from(b);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash ^= 0xff;
    };
    eat(query.text.trim().to_lowercase().as_bytes());
    eat(query.subsystem.map_or("", |s| s.key()).as_bytes());
    let mut kinds = query.kinds.clone();
    kinds.sort();
    for kind in kinds {
        eat(&[kind as u8]);
    }
    hash
}

fn make_cursor(offset: usize, fingerprint: u32) -> String {
    format!("{offset}-{fingerprint:08x}")
}

fn parse_cursor(token: &str, fingerprint: u32) -> Result<usize, QueryError> {
    let (offset, print) = token.split_once('-').ok_or(QueryError::InvalidCursor)?;
    let offset: usize = offset.parse().map_err(|_| QueryError::InvalidCursor)?;
    let print = u32::from_str_radix(print, 16).map_err(|_| QueryError::InvalidCursor)?;
    if print != fingerprint {
        return Err(QueryError::InvalidCursor);
    }
    Ok(offset)
}

fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && matches!(text[5..7].parse::<u8>(), Ok(1..=12))
        && matches!(text[8..10].parse::<u8>(), Ok(1..=31))
}

fn is_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn toml_files(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(LoadError::Read {
                path: dir.to_owned(),
                source,
            });
        }
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| LoadError::Read {
                path: dir.to_owned(),
                source,
            })?
            .path();
        if path.extension().is_some_and(|e| e == "toml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })
}

fn parse_text<T: serde::de::DeserializeOwned>(path: &str, text: &str) -> Result<T, LoadError> {
    toml::from_str(text).map_err(|err| LoadError::Parse {
        path: PathBuf::from(path),
        message: err.to_string(),
    })
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
