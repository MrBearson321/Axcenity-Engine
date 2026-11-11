//! What opening a project folder would do, worked out from its manifest
//! before anything is written: open it, upgrade it (and how), or refuse it.
//! The editor asks before an upgrade; [`crate::load`] upgrades without
//! asking, for the command-line tools.

use std::fs;
use std::path::Path;

use crate::migrate::{UpgradeStep, upgrade_steps};
use crate::{FORMAT_VERSION, MANIFEST_FILE};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compatibility {
    /// This engine's format: it opens as it is.
    Current,
    /// An older format this engine upgrades, one step at a time.
    Upgrade { from: u32, steps: Vec<UpgradeStep> },
    /// Older than the oldest format this engine can upgrade.
    TooOld { found: u32 },
    /// A newer format; `saved_with` is the engine that saved it, when the
    /// manifest says.
    TooNew {
        found: u32,
        saved_with: Option<String>,
    },
    /// No manifest, or one without a format version. Loading reports why.
    Damaged(String),
}

/// How this engine would open the project in `folder`.
pub fn compatibility(folder: &Path) -> Compatibility {
    let path = folder.join(MANIFEST_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Compatibility::Damaged(format!("there's no {MANIFEST_FILE} in it"));
        }
        Err(err) => return Compatibility::Damaged(format!("couldn't read {MANIFEST_FILE}: {err}")),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(err) => return Compatibility::Damaged(format!("{MANIFEST_FILE} isn't valid: {err}")),
    };
    let Some(found) = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
    else {
        return Compatibility::Damaged(format!("{MANIFEST_FILE} has no format version"));
    };
    if found == FORMAT_VERSION {
        Compatibility::Current
    } else if found > FORMAT_VERSION {
        let saved_with = value
            .pointer("/engine/saved_with")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        Compatibility::TooNew { found, saved_with }
    } else {
        match upgrade_steps(found) {
            Some(steps) => Compatibility::Upgrade { from: found, steps },
            None => Compatibility::TooOld { found },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn folder(name: &str, manifest: Option<&str>) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-compat-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        if let Some(manifest) = manifest {
            fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
        }
        folder
    }

    #[test]
    fn each_kind_of_project_is_told_apart_without_writing() {
        let current = format!(r#"{{"name":"A","format_version":{FORMAT_VERSION}}}"#);
        let cases = [
            ("current", Some(current.as_str())),
            ("old", Some(r#"{"name":"A","format_version":2}"#)),
            ("ancient", Some(r#"{"name":"A","format_version":0}"#)),
            (
                "new",
                Some(r#"{"name":"A","format_version":99,"engine":{"saved_with":"0.9.0"}}"#),
            ),
            ("none", None),
            ("broken", Some("{ not json")),
            ("unversioned", Some(r#"{"name":"A"}"#)),
        ];
        let mut seen = Vec::new();
        for (name, manifest) in cases {
            let dir = folder(name, manifest);
            seen.push(compatibility(&dir));
            assert!(!dir.join(".axcenity").exists(), "{name} wrote a copy");
            if let Some(manifest) = manifest {
                assert_eq!(
                    fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap(),
                    manifest
                );
            }
            fs::remove_dir_all(&dir).unwrap();
        }
        assert_eq!(seen[0], Compatibility::Current);
        let Compatibility::Upgrade { from: 2, steps } = &seen[1] else {
            panic!("{:?}", seen[1]);
        };
        assert_eq!(
            steps.iter().map(|s| s.to).collect::<Vec<_>>(),
            (3..=FORMAT_VERSION).collect::<Vec<_>>()
        );
        assert!(steps.iter().all(|s| !s.summary.is_empty()));
        assert_eq!(seen[2], Compatibility::TooOld { found: 0 });
        assert_eq!(
            seen[3],
            Compatibility::TooNew {
                found: 99,
                saved_with: Some("0.9.0".to_owned())
            }
        );
        for damaged in &seen[4..] {
            assert!(matches!(damaged, Compatibility::Damaged(_)), "{damaged:?}");
        }
    }
}
