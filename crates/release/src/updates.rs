//! Finding an update: the GitHub releases list, the best release for the
//! channel someone follows, and its signed manifest. No networking here:
//! [`check`] takes a `fetch` function, which the editor gives its HTTP
//! client and tests give canned answers. Design:
//! `docs/research/release-system/architecture.md` §6 (update checks) and §9.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::build_info::{BuildInfo, BuildKind};
use crate::publish::{MANIFEST_FILE, Manifest, SIGNATURE_FILE, UPDATE_PROTOCOL};
use crate::version::{Channel, Version};

/// The repository releases come from. Download addresses are built from it
/// and a release's tag and file names, never taken from a manifest.
pub const REPOSITORY: &str = "MrBearson321/Axcenity-Engine";

/// One request per check: the 20 newest releases.
pub fn releases_url() -> String {
    format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=20")
}

/// Where a release's file is downloaded from.
pub fn asset_url(tag: &str, file: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{file}")
}

/// The public half of the release signing key
/// (`packaging/axcenity-release.pub`), built in. `None` in a build made
/// before the key existed: such a build never checks for updates.
pub fn release_public_key() -> Option<&'static str> {
    let key = include_str!(concat!(env!("OUT_DIR"), "/release-key.pub"));
    (!key.trim().is_empty()).then_some(key)
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// No connection, a timeout, or an HTTP error: the message is for the
    /// Settings page ("couldn't check: …").
    #[error("couldn't reach GitHub: {0}")]
    Fetch(String),
    #[error("GitHub's release list wasn't understood: {0}")]
    BadReleaseList(String),
    #[error("the manifest of {0} isn't signed with Axcenity's release key")]
    BadSignature(String),
    #[error("the manifest of {tag} doesn't match its release: {reason}")]
    Mismatch { tag: String, reason: String },
    #[error("the manifest of {0} wasn't understood: {1}")]
    BadManifest(String, String),
}

/// What the GitHub API says about a release; other fields are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<GithubAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct GithubAsset {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

pub fn parse_releases(json: &[u8]) -> Result<Vec<GithubRelease>, UpdateError> {
    serde_json::from_slice(json).map_err(|e| UpdateError::BadReleaseList(e.to_string()))
}

/// A release worth offering, before its manifest is checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub version: Version,
    pub tag: String,
    pub page: String,
}

/// The newest release newer than `installed` that someone following
/// `follow` is offered. Drafts, tags that aren't Axcenity versions, releases
/// whose pre-release flag disagrees with their version, and releases
/// without a signed manifest are passed over.
pub fn best_update(
    releases: &[GithubRelease],
    installed: &Version,
    follow: Channel,
) -> Option<Candidate> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter(|r| {
            [MANIFEST_FILE, SIGNATURE_FILE]
                .iter()
                .all(|file| r.assets.iter().any(|a| a.name == *file))
        })
        .filter_map(|r| {
            let version = Version::from_tag(&r.tag_name).ok()?;
            (r.prerelease == version.is_prerelease()
                && follow.offers(version.channel())
                && &version > installed)
                .then(|| Candidate {
                    version,
                    tag: r.tag_name.clone(),
                    page: r.html_url.clone(),
                })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// The manifest of `candidate`, after checking its signature with
/// `public_key` (a minisign public key file's text) and that it describes
/// that release: same version and tag, the channel its version implies, a
/// trusted comment naming the tag, and an update protocol this build knows.
pub fn verify_manifest(
    json: &[u8],
    signature: &str,
    public_key: &str,
    candidate: &Candidate,
) -> Result<Manifest, UpdateError> {
    let tag = &candidate.tag;
    let bad = || UpdateError::BadSignature(tag.clone());
    let key = minisign_verify::PublicKey::decode(public_key.trim())
        .or_else(|_| minisign_verify::PublicKey::from_base64(public_key.trim()))
        .map_err(|_| bad())?;
    let signature = minisign_verify::Signature::decode(signature.trim()).map_err(|_| bad())?;
    key.verify(json, &signature, false).map_err(|_| bad())?;
    let mismatch = |reason: String| UpdateError::Mismatch {
        tag: tag.clone(),
        reason,
    };
    let expected_comment = format!("axcenity-engine {tag}");
    if signature.trusted_comment() != expected_comment {
        return Err(mismatch(format!(
            "its signature says `{}`",
            signature.trusted_comment()
        )));
    }
    let manifest: Manifest = serde_json::from_slice(json)
        .map_err(|e| UpdateError::BadManifest(tag.clone(), e.to_string()))?;
    if manifest.product != "axcenity-engine" {
        return Err(mismatch(format!("it is for `{}`", manifest.product)));
    }
    if manifest.version != candidate.version || &manifest.tag != tag {
        return Err(mismatch(format!(
            "it describes {} ({})",
            manifest.version, manifest.tag
        )));
    }
    if manifest.channel != manifest.version.channel() {
        return Err(mismatch(format!(
            "it says channel {}, but {} is {}",
            manifest.channel,
            manifest.version,
            manifest.version.channel()
        )));
    }
    if manifest.compatibility.min_update_protocol > UPDATE_PROTOCOL {
        return Err(mismatch(
            "it needs a newer updater; download it from the release page".to_owned(),
        ));
    }
    Ok(manifest)
}

/// A verified update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub candidate: Candidate,
    pub manifest: Manifest,
}

/// One whole check: the releases list, the best candidate, then its
/// manifest and signature. `Ok(None)` means this build is up to date for
/// `follow`. `fetch` gets a URL's body or an error message.
pub fn check(
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    installed: &Version,
    follow: Channel,
    public_key: &str,
) -> Result<Option<Update>, UpdateError> {
    let list = fetch(&releases_url()).map_err(UpdateError::Fetch)?;
    let releases = parse_releases(&list)?;
    let Some(candidate) = best_update(&releases, installed, follow) else {
        return Ok(None);
    };
    let json = fetch(&asset_url(&candidate.tag, MANIFEST_FILE)).map_err(UpdateError::Fetch)?;
    let signature =
        fetch(&asset_url(&candidate.tag, SIGNATURE_FILE)).map_err(UpdateError::Fetch)?;
    let signature = String::from_utf8_lossy(&signature);
    let manifest = verify_manifest(&json, &signature, public_key, &candidate)?;
    Ok(Some(Update {
        candidate,
        manifest,
    }))
}

/// How often the editor looks for updates on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckInterval {
    #[default]
    Daily,
    Weekly,
    Never,
}

impl CheckInterval {
    pub const ALL: [CheckInterval; 3] = [
        CheckInterval::Daily,
        CheckInterval::Weekly,
        CheckInterval::Never,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CheckInterval::Daily => "Every day",
            CheckInterval::Weekly => "Every week",
            CheckInterval::Never => "Never (only Check now)",
        }
    }

    fn seconds(self) -> Option<u64> {
        match self {
            CheckInterval::Daily => Some(24 * 60 * 60),
            CheckInterval::Weekly => Some(7 * 24 * 60 * 60),
            CheckInterval::Never => None,
        }
    }
}

/// The update settings, saved with the editor's other settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePrefs {
    /// The channel followed; `None` follows this build's own channel.
    pub channel: Option<Channel>,
    pub interval: CheckInterval,
    /// When the last check finished, in Unix seconds.
    pub last_check: Option<u64>,
    /// The version whose notice was dismissed; only a newer one shows again.
    pub dismissed: Option<Version>,
}

impl UpdatePrefs {
    pub fn channel(&self, build: &Version) -> Channel {
        self.channel.unwrap_or(build.channel())
    }

    /// Whether an automatic check is due at `now` (Unix seconds).
    pub fn due(&self, now: u64) -> bool {
        match (self.interval.seconds(), self.last_check) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(every), Some(last)) => now.saturating_sub(last) >= every || now < last,
        }
    }

    /// Whether `version` should get a notice: not dismissed, or newer than
    /// the one that was.
    pub fn notifies(&self, version: &Version) -> bool {
        self.dismissed.as_ref().is_none_or(|d| version > d)
    }
}

/// Why this build doesn't look for updates, if it doesn't.
pub fn checks_disabled(build: &BuildInfo) -> Option<&'static str> {
    if build.kind != BuildKind::Official {
        Some("This is a local build, so it doesn't check for updates.")
    } else if release_public_key().is_none() {
        Some("This build has no release key, so it can't check updates safely.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures signed with a throwaway test key pair (never used for
    // anything else): `minisign -Sm manifest.json -t "axcenity-engine
    // v0.2.0-alpha.2"`; `wrong-comment.minisig` names v0.2.0-alpha.1 and
    // `other-key.minisig` is signed by a second throwaway key. Remake them
    // with new keys if the manifest changes.
    const KEY: &str = include_str!("testdata/release-key.pub");
    const MANIFEST: &[u8] = include_bytes!("testdata/manifest.json");
    const SIGNATURE: &str = include_str!("testdata/manifest.json.minisig");

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn release(tag: &str, prerelease: bool, draft: bool) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag, "draft": draft, "prerelease": prerelease,
            "html_url": format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
            "assets": [{"name": MANIFEST_FILE, "size": 1}, {"name": SIGNATURE_FILE, "size": 1}],
            "body": "ignored", "author": {"login": "ignored"}
        })
    }

    fn list(items: &[serde_json::Value]) -> Vec<GithubRelease> {
        parse_releases(&serde_json::to_vec(items).unwrap()).unwrap()
    }

    fn candidate(tag: &str) -> Candidate {
        Candidate {
            version: Version::from_tag(tag).unwrap(),
            tag: tag.to_owned(),
            page: String::new(),
        }
    }

    #[test]
    fn the_built_in_release_key_is_a_minisign_public_key() {
        let key = release_public_key().expect("packaging/axcenity-release.pub is committed");
        assert!(minisign_verify::PublicKey::decode(key.trim()).is_ok());
    }

    #[test]
    fn the_best_update_is_the_newest_the_channel_offers() {
        let releases = list(&[
            release("v0.2.0-alpha.3", true, false),
            release("v0.2.0-beta.1", true, false),
            release("v0.2.0", false, true), // a draft nobody may see yet
            release("v0.1.9", false, false),
            release("nightly-2026", true, false),
        ]);
        let installed = v("0.2.0-alpha.1");
        let pick = |c| best_update(&releases, &installed, c).map(|c| c.tag);
        assert_eq!(pick(Channel::Development).as_deref(), Some("v0.2.0-beta.1"));
        assert_eq!(pick(Channel::Beta).as_deref(), Some("v0.2.0-beta.1"));
        // Stable sees only v0.1.9, which isn't newer.
        assert_eq!(pick(Channel::Stable), None);
        // Up to date.
        assert_eq!(
            best_update(&releases, &v("0.2.0-beta.1"), Channel::Development),
            None
        );
    }

    #[test]
    fn releases_that_disagree_with_themselves_or_lack_a_manifest_are_passed_over() {
        let mut unsigned = release("v0.3.0", false, false);
        unsigned["assets"] = serde_json::json!([{"name": MANIFEST_FILE}]);
        let releases = list(&[
            // A stable-looking version marked pre-release, and the reverse.
            release("v0.4.0", true, false),
            release("v0.4.0-beta.1", false, false),
            unsigned,
            release("v0.2.1", false, false),
        ]);
        let best = best_update(&releases, &v("0.2.0"), Channel::Development).unwrap();
        assert_eq!(best.tag, "v0.2.1");
        assert!(best.page.ends_with("/releases/tag/v0.2.1"));
    }

    #[test]
    fn a_manifest_signed_with_the_release_key_is_trusted() {
        let manifest =
            verify_manifest(MANIFEST, SIGNATURE, KEY, &candidate("v0.2.0-alpha.2")).unwrap();
        assert_eq!(manifest.title, "Faster startup");
        assert_eq!(
            manifest.notes.performance[0].title,
            "Projects open twice as fast"
        );
        assert_eq!(manifest.formats["project"], 5);
        // The base64 line alone works too.
        let line = KEY.lines().nth(1).unwrap();
        assert!(verify_manifest(MANIFEST, SIGNATURE, line, &candidate("v0.2.0-alpha.2")).is_ok());
    }

    #[test]
    fn anything_else_is_refused() {
        let c = candidate("v0.2.0-alpha.2");
        let refused = |json: &[u8], signature: &str, c: &Candidate| {
            verify_manifest(json, signature, KEY, c).unwrap_err()
        };
        // One byte changed.
        let mut altered = MANIFEST.to_vec();
        let at = altered.iter().position(|b| *b == b'5').unwrap();
        altered[at] = b'6';
        assert!(matches!(
            refused(&altered, SIGNATURE, &c),
            UpdateError::BadSignature(_)
        ));
        // Signed by another key, or not signed at all.
        let other = include_str!("testdata/other-key.minisig");
        assert!(matches!(
            refused(MANIFEST, other, &c),
            UpdateError::BadSignature(_)
        ));
        assert!(matches!(
            refused(MANIFEST, "", &c),
            UpdateError::BadSignature(_)
        ));
        // A good signature whose trusted comment names another tag.
        let wrong = include_str!("testdata/wrong-comment.minisig");
        assert!(matches!(
            refused(MANIFEST, wrong, &c),
            UpdateError::Mismatch { .. }
        ));
        // A good manifest offered as another release.
        assert!(matches!(
            refused(MANIFEST, SIGNATURE, &candidate("v0.2.0-alpha.3")),
            UpdateError::Mismatch { .. }
        ));
    }

    #[test]
    fn a_check_fetches_the_list_then_only_the_best_candidates_manifest() {
        let releases = serde_json::to_vec(&[
            release("v0.2.0-alpha.2", true, false),
            release("v0.1.0-alpha.3", true, false),
        ])
        .unwrap();
        let mut asked = Vec::new();
        let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
            asked.push(url.to_owned());
            if url == releases_url() {
                Ok(releases.clone())
            } else if url == asset_url("v0.2.0-alpha.2", MANIFEST_FILE) {
                Ok(MANIFEST.to_vec())
            } else if url == asset_url("v0.2.0-alpha.2", SIGNATURE_FILE) {
                Ok(SIGNATURE.as_bytes().to_vec())
            } else {
                Err(format!("unexpected {url}"))
            }
        };
        let update = check(&mut fetch, &v("0.2.0-alpha.1"), Channel::Development, KEY)
            .unwrap()
            .unwrap();
        assert_eq!(update.candidate.tag, "v0.2.0-alpha.2");
        assert_eq!(asked.len(), 3);
        assert!(asked[1].starts_with(
            "https://github.com/MrBearson321/Axcenity-Engine/releases/download/v0.2.0-alpha.2/"
        ));

        // Up to date: one request.
        let mut count = 0;
        let mut fetch = |_: &str| -> Result<Vec<u8>, String> {
            count += 1;
            Ok(releases.clone())
        };
        assert_eq!(
            check(&mut fetch, &v("0.2.0-alpha.2"), Channel::Development, KEY),
            Ok(None)
        );
        assert_eq!(count, 1);

        // Offline.
        let mut offline = |_: &str| -> Result<Vec<u8>, String> { Err("no connection".into()) };
        assert_eq!(
            check(&mut offline, &v("0.2.0-alpha.1"), Channel::Development, KEY),
            Err(UpdateError::Fetch("no connection".into()))
        );
    }

    #[test]
    fn checks_are_due_by_interval_and_notices_wait_for_a_newer_version() {
        let day = 24 * 60 * 60;
        let mut prefs = UpdatePrefs::default();
        assert!(prefs.due(1_000));
        prefs.last_check = Some(1_000);
        assert!(!prefs.due(1_000 + day - 1));
        assert!(prefs.due(1_000 + day));
        // A clock set back counts as due rather than never.
        assert!(prefs.due(10));
        prefs.interval = CheckInterval::Weekly;
        assert!(!prefs.due(1_000 + day));
        prefs.interval = CheckInterval::Never;
        assert!(!prefs.due(u64::MAX));

        assert_eq!(prefs.channel(&v("0.2.0-alpha.1")), Channel::Development);
        prefs.channel = Some(Channel::Stable);
        assert_eq!(prefs.channel(&v("0.2.0-alpha.1")), Channel::Stable);

        prefs.dismissed = Some(v("0.2.0-alpha.2"));
        assert!(!prefs.notifies(&v("0.2.0-alpha.2")));
        assert!(prefs.notifies(&v("0.2.0-alpha.3")));

        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<UpdatePrefs>(&json).unwrap(), prefs);
        assert_eq!(
            serde_json::from_str::<UpdatePrefs>("{}").unwrap(),
            UpdatePrefs::default()
        );
    }

    #[test]
    fn local_builds_never_check() {
        let build = BuildInfo::current();
        assert_eq!(build.kind, BuildKind::Local);
        assert!(checks_disabled(&build).is_some());
    }
}
