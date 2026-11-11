//! Tests of the history, its checks and queries, and `check`/`prepare`,
//! against small release folders written to a scratch directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::history::{HitKind, IssueState, QueryError, SearchQuery};
use crate::prepare::{self, PrepareError};
use crate::records::{Category, Evidence, Subsystem, VersionRef};
use crate::{History, Version};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("axcenity-release-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// A small, consistent history: two releases, an unreleased fix, three
/// features and three issues.
fn sample(name: &str) -> PathBuf {
    let root = scratch(name);
    write(
        &root,
        "history/0.1.0-alpha.3.toml",
        r#"
version = "0.1.0-alpha.3"
date = "2026-10-21"
title = "First alpha"
summary = "The first packaged build."
commit = "a897631ffd3b27e5e491e521932aac1f2947c65a"
[formats]
project = 2

[[changes]]
category = "feature"
subsystem = "terrain"
title = "Terrain Studio"
features = ["terrain.studio"]
"#,
    );
    write(
        &root,
        "history/0.2.0-alpha.1.toml",
        r#"
version = "0.2.0-alpha.1"
date = "2026-11-20"
title = "Multiplayer"
summary = "Host and join games over the network."
[formats]
project = 5
network_protocol = 10

[[changes]]
category = "feature"
subsystem = "networking"
title = "Host and join games"
features = ["net.multiplayer"]
prs = [34]
evidence = "reconstructed"

[[changes]]
category = "breaking"
subsystem = "project"
title = "Project format 5"
details = "Projects saved by 0.2 can't be opened by 0.1. Keep the backup in .axcenity/previous."

[[changes]]
category = "fix"
subsystem = "terrain"
title = "Terrain layers load in the right order"
features = ["terrain.studio"]
fixes = ["AX-0001"]

[[changes]]
category = "internal"
subsystem = "build"
title = "Refactor the build script"
"#,
    );
    write(
        &root,
        "unreleased/0100-sun-shadows.toml",
        r#"
category = "fix"
subsystem = "renderer"
title = "Shadows no longer flicker at dawn"
fixes = ["AX-0002"]
features = ["renderer.shadows"]
"#,
    );
    write(
        &root,
        "unreleased/0101-secret.toml",
        r#"
category = "feature"
subsystem = "editor"
title = "Hidden experiment"
public = false
fixes = ["AX-0003"]
"#,
    );
    write(
        &root,
        "features/terrain.toml",
        r#"
[[feature]]
id = "terrain.studio"
name = "Terrain Studio"
subsystem = "terrain"
description = "Sculpt and paint terrain."
status = "active"
[feature.introduced]
version = "0.1.0-alpha.3"
evidence = "verified"
source = "crates/terrain at v0.1.0-alpha.3"
"#,
    );
    write(
        &root,
        "features/net.toml",
        r#"
[[feature]]
id = "net.multiplayer"
name = "Multiplayer"
subsystem = "networking"
description = "Host, join and replicate."
status = "active"
[feature.introduced]
version = "0.2.0-alpha.1"
evidence = "reconstructed"
source = "PR #34"

[[feature]]
id = "renderer.shadows"
name = "Shadows"
subsystem = "renderer"
description = "Sun and point light shadows."
status = "deprecated"
deprecated_in = "unreleased"
replacement = "Shadow quality settings"
[feature.introduced]
version = "unknown"
evidence = "unknown"
"#,
    );
    write(
        &root,
        "issues.toml",
        r#"
[[issue]]
id = "AX-0001"
title = "Terrain layers load out of order"
subsystem = "terrain"
status = "fixed"
introduced = "0.1.0-alpha.3"
confirmed_affected = ["0.1.0-alpha.3"]

[[issue]]
id = "AX-0002"
title = "Shadows flicker at dawn"
subsystem = "renderer"
status = "open"
introduced = "unknown"
confirmed_affected = ["0.2.0-alpha.1"]

[[issue]]
id = "AX-0003"
title = "Private crash"
subsystem = "editor"
status = "open"
introduced = "0.2.0-alpha.1"
public = false

[[issue]]
id = "AX-0004"
title = "Sound pops"
subsystem = "audio"
status = "open"
introduced = "unknown"
"#,
    );
    root
}

#[test]
fn a_consistent_history_has_no_problems() {
    let history = History::load(&sample("consistent")).unwrap();
    assert_eq!(history.validate(), vec![]);
    let versions: Vec<String> = history
        .releases()
        .iter()
        .map(|r| r.version.to_string())
        .collect();
    assert_eq!(versions, ["0.1.0-alpha.3", "0.2.0-alpha.1"]);
    assert_eq!(history.unreleased().len(), 2);
}

#[test]
fn an_empty_folder_is_an_empty_history() {
    let history = History::load(&scratch("empty")).unwrap();
    assert!(history.releases().is_empty());
    assert!(history.validate().is_empty());
}

#[test]
fn unknown_fields_are_refused_so_typos_are_not_lost() {
    let root = sample("typo");
    write(
        &root,
        "unreleased/0102-typo.toml",
        "category = \"fix\"\nsubsystem = \"audio\"\ntitle = \"x\"\nfixs = [\"AX-0004\"]\n",
    );
    let err = History::load(&root).unwrap_err().to_string();
    assert!(
        err.contains("0102-typo.toml") && err.contains("fixs"),
        "{err}"
    );
}

fn problems_with(name: &str, file: &str, text: &str) -> Vec<String> {
    let root = sample(name);
    write(&root, file, text);
    History::load(&root)
        .unwrap()
        .validate()
        .into_iter()
        .map(|p| format!("{}: {}", p.file, p.message))
        .collect()
}

#[test]
fn the_checks_catch_each_kind_of_mistake() {
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "breaking",
            "unreleased/x.toml",
            "category = \"breaking\"\nsubsystem = \"project\"\ntitle = \"Format 6\"\n",
            "say in `details` what breaks",
        ),
        (
            "reconstructed",
            "unreleased/x.toml",
            "category = \"feature\"\nsubsystem = \"audio\"\ntitle = \"Buses\"\nevidence = \"reconstructed\"\n",
            "name the `prs` or `commits`",
        ),
        (
            "unknown-feature",
            "unreleased/x.toml",
            "category = \"feature\"\nsubsystem = \"audio\"\ntitle = \"Buses\"\nfeatures = [\"audio.busses\"]\n",
            "names feature `audio.busses`",
        ),
        (
            "unknown-issue",
            "unreleased/x.toml",
            "category = \"fix\"\nsubsystem = \"audio\"\ntitle = \"Pops\"\nfixes = [\"AX-9\"]\n",
            "names issue `AX-9`",
        ),
        (
            "file-name",
            "history/0.3.0.toml",
            "version = \"0.3.1\"\ndate = \"2026-12-01\"\ntitle = \"T\"\n",
            "named `0.3.0` but records version 0.3.1",
        ),
        (
            "date",
            "history/0.3.0.toml",
            "version = \"0.3.0\"\ndate = \"01/12/2026\"\ntitle = \"T\"\n",
            "is not YYYY-MM-DD",
        ),
        (
            "title",
            "history/0.3.0.toml",
            "version = \"0.3.0\"\ndate = \"2026-12-01\"\ntitle = \"\"\n",
            "write a title",
        ),
        (
            "introduced-unrecorded",
            "features/audio.toml",
            "[[feature]]\nid = \"audio.buses\"\nname = \"Buses\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"active\"\n[feature.introduced]\nversion = \"0.1.5\"\nevidence = \"verified\"\nsource = \"x\"\n",
            "introduced in 0.1.5, which has no release record",
        ),
        (
            "introduced-source",
            "features/audio.toml",
            "[[feature]]\nid = \"audio.buses\"\nname = \"Buses\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"active\"\n[feature.introduced]\nversion = \"0.1.0-alpha.3\"\nevidence = \"verified\"\n",
            "say what shows its introduction",
        ),
        (
            "unknown-evidence",
            "features/audio.toml",
            "[[feature]]\nid = \"audio.buses\"\nname = \"Buses\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"active\"\n[feature.introduced]\nversion = \"unknown\"\nevidence = \"reconstructed\"\nsource = \"x\"\n",
            "evidence `unknown`, and only then",
        ),
        (
            "removed",
            "features/audio.toml",
            "[[feature]]\nid = \"audio.buses\"\nname = \"Buses\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"removed\"\n[feature.introduced]\nversion = \"unknown\"\nevidence = \"unknown\"\n",
            "say `removed_in`",
        ),
        (
            "id",
            "features/audio.toml",
            "[[feature]]\nid = \"Audio Buses\"\nname = \"Buses\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"active\"\n[feature.introduced]\nversion = \"unknown\"\nevidence = \"unknown\"\n",
            "IDs are lowercase words",
        ),
        (
            "duplicate",
            "features/audio.toml",
            "[[feature]]\nid = \"terrain.studio\"\nname = \"Again\"\nsubsystem = \"audio\"\ndescription = \"d\"\nstatus = \"active\"\n[feature.introduced]\nversion = \"unknown\"\nevidence = \"unknown\"\n",
            "`terrain.studio` is defined twice",
        ),
    ];
    for (name, file, text, expected) in cases {
        let problems = problems_with(name, file, text);
        assert!(
            problems.iter().any(|p| p.contains(expected)),
            "{name}: expected `{expected}` in {problems:#?}"
        );
    }
}

#[test]
fn an_issue_status_must_agree_with_its_fixes() {
    let root = sample("issue-status");
    let issues = fs::read_to_string(root.join("issues.toml")).unwrap();
    // AX-0002 is fixed by an unreleased change but still says open.
    let problems: Vec<String> = History::load(&root)
        .unwrap()
        .validate()
        .into_iter()
        .map(|p| p.message)
        .collect();
    assert!(problems.is_empty(), "{problems:?}");
    write(
        &root,
        "issues.toml",
        &issues.replacen("status = \"fixed\"", "status = \"open\"", 1),
    );
    let problems: Vec<String> = History::load(&root)
        .unwrap()
        .validate()
        .into_iter()
        .map(|p| p.message)
        .collect();
    assert!(
        problems
            .iter()
            .any(|p| p.contains("`AX-0001` is marked Open but is fixed in 0.2.0-alpha.1")),
        "{problems:?}"
    );
}

#[test]
fn an_unreleased_fix_leaves_an_issue_open_until_released() {
    // AX-0002's fix is unreleased: status "open" is right, and released
    // versions are still affected.
    let history = History::load(&sample("unreleased-fix")).unwrap();
    assert!(history.validate().is_empty());
    let issue = history.issue("AX-0002").unwrap();
    assert_eq!(history.fixed_in(issue), Some(VersionRef::Unreleased));
    assert_eq!(
        history
            .issue_status("AX-0002", &v("0.2.0-alpha.1"))
            .unwrap(),
        IssueState::ConfirmedAffected
    );
    // A build newer than every release carries the unreleased fix.
    assert_eq!(
        history
            .issue_status("AX-0002", &v("0.2.0-alpha.2"))
            .unwrap(),
        IssueState::Fixed {
            fixed_in: VersionRef::Unreleased
        }
    );
}

#[test]
fn issue_status_follows_the_four_rules() {
    let history = History::load(&sample("issue-rules")).unwrap();
    // Fixed in 0.2.0-alpha.1 by a change entry.
    assert_eq!(
        history
            .issue_status("AX-0001", &v("0.2.0-alpha.1"))
            .unwrap(),
        IssueState::Fixed {
            fixed_in: VersionRef::Release(v("0.2.0-alpha.1"))
        }
    );
    assert_eq!(
        history
            .issue_status("AX-0001", &v("0.1.0-alpha.3"))
            .unwrap(),
        IssueState::ConfirmedAffected
    );
    // Introduced in 0.2.0-alpha.1, never checked there: affected by range,
    // and not there before.
    assert_eq!(
        history
            .issue_status("AX-0003", &v("0.2.0-alpha.1"))
            .unwrap(),
        IssueState::AffectedByRange
    );
    assert!(matches!(
        history
            .issue_status("AX-0003", &v("0.1.0-alpha.3"))
            .unwrap(),
        IssueState::BeforeIntroduced { .. }
    ));
    // Unknown introduction, never checked: not investigated, never "not affected".
    assert_eq!(
        history
            .issue_status("ax-0004", &v("0.2.0-alpha.1"))
            .unwrap(),
        IssueState::NotInvestigated
    );
    let affecting: Vec<&str> = history
        .issues_affecting(&v("0.2.0-alpha.1"))
        .iter()
        .map(|(i, _)| i.id.as_str())
        .collect();
    assert_eq!(affecting, ["AX-0002", "AX-0003"]);
}

#[test]
fn a_feature_knows_where_it_came_from_and_its_timeline() {
    let history = History::load(&sample("timeline")).unwrap();
    let introduced = history.introduced_in("terrain.studio").unwrap();
    assert_eq!(introduced.version, VersionRef::Release(v("0.1.0-alpha.3")));
    assert_eq!(introduced.evidence, Evidence::Verified);
    let rows = history.feature_timeline("terrain.studio").unwrap();
    let summary: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.version.to_string(), r.change.clone()))
        .collect();
    assert_eq!(
        summary,
        [
            ("0.1.0-alpha.3".into(), "Terrain Studio introduced".into()),
            ("0.1.0-alpha.3".into(), "Terrain Studio".into()),
            (
                "0.2.0-alpha.1".into(),
                "Terrain layers load in the right order".into()
            ),
        ]
    );
    let shadows = history.feature_timeline("renderer.shadows").unwrap();
    assert_eq!(shadows.first().unwrap().version, VersionRef::Unknown);
    assert_eq!(shadows.last().unwrap().change, "Shadows deprecated");
}

#[test]
fn unknown_ids_and_versions_are_distinct_errors_with_suggestions() {
    let history = History::load(&sample("errors")).unwrap();
    match history.feature("terrain.stdio") {
        Err(QueryError::UnknownFeature { suggestions, .. }) => {
            assert_eq!(suggestions, ["terrain.studio"])
        }
        other => panic!("{other:?}"),
    }
    match history.feature("Multiplayer") {
        Err(QueryError::UnknownFeature { suggestions, .. }) => {
            assert!(
                suggestions.contains(&"net.multiplayer".to_owned()),
                "{suggestions:?}"
            )
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        history.issue("AX-0009"),
        Err(QueryError::UnknownIssue { .. })
    ));
    match history.version("0.2.0-alpha.2", false) {
        Err(QueryError::UnknownVersion { suggestions, .. }) => {
            assert_eq!(
                suggestions.first().map(String::as_str),
                Some("0.2.0-alpha.1")
            )
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        history.version("banana", false),
        Err(QueryError::InvalidVersion { .. })
    ));
    assert!(matches!(
        history.version("unreleased", false),
        Err(QueryError::InvalidVersion { .. })
    ));
    assert_eq!(
        history
            .version("v0.2.0-alpha.1", false)
            .unwrap()
            .to_string(),
        "0.2.0-alpha.1"
    );
    let json = serde_json::to_value(history.feature("nope").unwrap_err()).unwrap();
    assert_eq!(json["error"], "unknown-feature");
}

#[test]
fn compare_lists_what_changed_in_the_range() {
    let history = History::load(&sample("compare")).unwrap();
    let from = history.version("0.1.0-alpha.3", false).unwrap();
    let to = history.version("0.2.0-alpha.1", false).unwrap();
    let c = history.compare(&from, &to).unwrap();
    assert_eq!(c.changes[&Category::Breaking].len(), 1);
    assert_eq!(
        c.changes[&Category::Feature][0].title,
        "Host and join games"
    );
    assert_eq!(c.changes[&Category::Fix].len(), 1);
    // Breaking changes come first.
    assert_eq!(c.changes.keys().next(), Some(&Category::Breaking));
    assert_eq!(c.features_introduced, ["net.multiplayer"]);
    assert_eq!(c.issues_fixed, ["AX-0001"]);
    let formats: Vec<(String, Option<u32>, Option<u32>)> = c
        .formats
        .iter()
        .map(|f| (f.format.clone(), f.from, f.to))
        .collect();
    assert_eq!(
        formats,
        [
            ("network_protocol".into(), None, Some(10)),
            ("project".into(), Some(2), Some(5))
        ]
    );

    let unreleased = history.compare(&to, &VersionRef::Unreleased).unwrap();
    assert_eq!(unreleased.issues_fixed, ["AX-0002", "AX-0003"]);
    assert_eq!(unreleased.features_deprecated, ["renderer.shadows"]);
    assert!(unreleased.formats.is_empty());

    assert_eq!(history.compare(&to, &from), Err(QueryError::BackwardsRange));
    assert_eq!(history.compare(&to, &to), Err(QueryError::BackwardsRange));
}

#[test]
fn the_breaking_category_sorts_first() {
    // Comparison maps are ordered by Category's declaration order.
    assert!(Category::Breaking < Category::Feature);
    assert_eq!(Category::ALL[0], Category::Breaking);
}

#[test]
fn minimum_engine_reads_the_formats_of_each_release() {
    let history = History::load(&sample("formats")).unwrap();
    assert_eq!(
        history.minimum_engine_for("project", 2),
        Some(&v("0.1.0-alpha.3"))
    );
    assert_eq!(
        history.minimum_engine_for("project", 4),
        Some(&v("0.2.0-alpha.1"))
    );
    assert_eq!(history.minimum_engine_for("project", 6), None);
    assert_eq!(history.minimum_engine_for("save", 1), None);
}

#[test]
fn the_public_history_drops_everything_internal() {
    let history = History::load(&sample("public")).unwrap().public();
    let all_titles: Vec<&str> = history.changes().map(|(_, c)| c.title.as_str()).collect();
    assert!(!all_titles.contains(&"Refactor the build script"));
    assert!(!all_titles.contains(&"Hidden experiment"));
    assert!(history.issue("AX-0003").is_err());
    for (_, change) in history.changes() {
        assert!(change.is_public());
        assert!(!change.fixes.contains(&"AX-0003".to_owned()));
    }
    assert!(history.validate().is_empty());
}

#[test]
fn search_finds_ranks_filters_and_pages() {
    let history = History::load(&sample("search")).unwrap();
    let query = SearchQuery {
        text: "terrain".into(),
        ..Default::default()
    };
    let page = history.search(&query, None, 2).unwrap();
    assert_eq!(page.total, 4, "{page:#?}");
    assert_eq!(page.items[0].kind, HitKind::Feature);
    assert_eq!(page.items[0].id, "terrain.studio");
    let next = page.next.clone().unwrap();
    let rest = history.search(&query, Some(&next), 2).unwrap();
    assert_eq!(rest.items.len(), 2);
    assert!(rest.next.is_none());
    // The same token always means the same page.
    assert_eq!(history.search(&query, Some(&next), 2).unwrap(), rest);
    // A token from another search is refused.
    let other = SearchQuery {
        text: "shadows".into(),
        ..Default::default()
    };
    assert_eq!(
        history.search(&other, Some(&next), 2),
        Err(QueryError::InvalidCursor)
    );
    assert_eq!(
        history.search(&query, Some("garbage"), 2),
        Err(QueryError::InvalidCursor)
    );

    let issues_only = SearchQuery {
        text: String::new(),
        subsystem: Some(Subsystem::Renderer),
        kinds: vec![HitKind::Issue],
    };
    let page = history.search(&issues_only, None, 10).unwrap();
    let ids: Vec<&str> = page.items.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids, ["AX-0002"]);

    let nothing = SearchQuery {
        text: "terrain flicker".into(),
        ..Default::default()
    };
    assert_eq!(history.search(&nothing, None, 10).unwrap().total, 0);
}

// ---- check and prepare ----

fn repo(name: &str, version: &str) -> PathBuf {
    let root = scratch(name);
    let releases = sample(&format!("{name}-releases"));
    copy_dir(&releases, &root.join("releases"));
    write(
        &root,
        "Cargo.toml",
        &format!(
            "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n\n[workspace.dependencies]\nserde = {{ version = \"1\" }}\n"
        ),
    );
    write(
        &root,
        "Cargo.lock",
        &format!(
            "version = 4\n\n[[package]]\nname = \"axcenity-core\"\nversion = \"{version}\"\ndependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"axcenity-like\"\nversion = \"9.9.9\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
        ),
    );
    root
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn the_workspace_version_is_read_and_replaced_in_place() {
    let text = "[package]\nversion = \"9.0.0\"\n\n[workspace.package]\n# The product version.\nversion = \"0.1.0\"\nedition = \"2024\"\n";
    assert_eq!(prepare::workspace_version(text).unwrap(), v("0.1.0"));
    let changed = prepare::set_workspace_version(text, &v("0.2.0-alpha.1")).unwrap();
    assert_eq!(
        changed,
        text.replace("version = \"0.1.0\"", "version = \"0.2.0-alpha.1\"")
    );
    assert!(matches!(
        prepare::workspace_version("[workspace]\n"),
        Err(PrepareError::NoWorkspaceVersion)
    ));
}

#[test]
fn check_wants_cargo_toml_newer_than_every_release() {
    let ok = repo("check-ok", "0.3.0-alpha.1");
    assert_eq!(prepare::check(&ok, None).unwrap(), vec![]);
    let old = repo("check-old", "0.1.0");
    let problems = prepare::check(&old, None).unwrap();
    assert!(
        problems[0]
            .message
            .contains("not newer than the latest recorded release, 0.2.0-alpha.1")
    );
    // Equal to a recorded release: that release is being tagged.
    let tagged = repo("check-tagged", "0.2.0-alpha.1");
    assert_eq!(prepare::check(&tagged, None).unwrap(), vec![]);
}

#[test]
fn check_with_a_tag_needs_the_same_version_and_a_record() {
    let root = repo("check-tag", "0.2.0-alpha.1");
    assert_eq!(
        prepare::check(&root, Some("v0.2.0-alpha.1")).unwrap(),
        vec![]
    );
    let wrong = prepare::check(&root, Some("v0.2.0-alpha.2")).unwrap();
    assert!(
        wrong[0]
            .message
            .contains("doesn't match Cargo.toml's version"),
        "{wrong:?}"
    );
    let bad = prepare::check(&root, Some("0.2.0-alpha.1")).unwrap();
    assert!(bad[0].message.contains("not a release tag"), "{bad:?}");
    // The record being tagged must be written and published.
    let records = root.join("releases/history/0.2.0-alpha.1.toml");
    let text = fs::read_to_string(&records).unwrap();
    fs::write(
        &records,
        text.replace(
            "summary = \"Host and join games over the network.\"\n",
            "published = false\n",
        ),
    )
    .unwrap();
    let unwritten: Vec<String> = prepare::check(&root, Some("v0.2.0-alpha.1"))
        .unwrap()
        .into_iter()
        .map(|p| p.message)
        .collect();
    assert_eq!(
        unwritten,
        [
            "the record for 0.2.0-alpha.1 has no summary: write it before tagging",
            "`published = false`, but it is being tagged for release"
        ]
    );
    let unrecorded = repo("check-unrecorded", "0.3.0");
    let problems = prepare::check(&unrecorded, Some("v0.3.0")).unwrap();
    assert!(
        problems[0]
            .message
            .contains("run `axcenity-release prepare 0.3.0` first")
    );
}

#[test]
fn prepare_moves_unreleased_entries_into_a_new_record() {
    let root = repo("prepare", "0.2.0-alpha.1");
    let path = prepare::prepare(&root, &v("0.2.0-alpha.2"), "2026-12-01").unwrap();
    assert!(path.ends_with("releases/history/0.2.0-alpha.2.toml"));

    let history = History::load(&root.join("releases")).unwrap();
    assert!(history.unreleased().is_empty());
    let record = history.release(&v("0.2.0-alpha.2")).unwrap();
    assert_eq!(record.date, "2026-12-01");
    let titles: Vec<&str> = record.changes.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Shadows no longer flicker at dawn", "Hidden experiment"]
    );
    // The fix now has a released version.
    let issue = history.issue("AX-0002").unwrap();
    assert_eq!(
        history.fixed_in(issue),
        Some(VersionRef::Release(v("0.2.0-alpha.2")))
    );
    // ...and the issues it fixes are marked fixed in issues.toml.
    assert_eq!(issue.status, crate::IssueStatus::Fixed);
    // Until a person writes the title, check says so, and nothing else.
    let problems: Vec<String> = history.validate().into_iter().map(|p| p.message).collect();
    assert_eq!(problems, ["write a title for this release"]);

    let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert_eq!(
        prepare::workspace_version(&cargo).unwrap(),
        v("0.2.0-alpha.2")
    );
    let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap();
    assert!(lock.contains("name = \"axcenity-core\"\nversion = \"0.2.0-alpha.2\""));
    // A crates.io package with a similar name is left alone.
    assert!(lock.contains("name = \"axcenity-like\"\nversion = \"9.9.9\""));
    assert!(lock.contains("name = \"serde\"\nversion = \"1.0.0\""));
}

#[test]
fn prepare_refuses_old_versions_existing_records_and_empty_releases() {
    let root = repo("prepare-refuses", "0.2.0-alpha.1");
    assert!(matches!(
        prepare::prepare(&root, &v("0.2.0-alpha.1"), "2026-12-01"),
        Err(PrepareError::AlreadyRecorded(_))
    ));
    assert!(matches!(
        prepare::prepare(&root, &v("0.1.0"), "2026-12-01"),
        Err(PrepareError::NotNewer { .. })
    ));
    prepare::prepare(&root, &v("0.2.0-alpha.2"), "2026-12-01").unwrap();
    // Write the title so the history is valid again; then nothing is left.
    let record = root.join("releases/history/0.2.0-alpha.2.toml");
    let text = fs::read_to_string(&record).unwrap();
    fs::write(
        &record,
        text.replacen("title = \"\"", "title = \"Shadows\"", 1),
    )
    .unwrap();
    // The two issues those entries fixed are released now: mark them fixed.
    let issues = root.join("releases/issues.toml");
    let text = fs::read_to_string(&issues).unwrap();
    let text = text
        .replace(
            "title = \"Shadows flicker at dawn\"\nsubsystem = \"renderer\"\nstatus = \"open\"",
            "title = \"Shadows flicker at dawn\"\nsubsystem = \"renderer\"\nstatus = \"fixed\"",
        )
        .replace(
            "title = \"Private crash\"\nsubsystem = \"editor\"\nstatus = \"open\"",
            "title = \"Private crash\"\nsubsystem = \"editor\"\nstatus = \"fixed\"",
        );
    fs::write(&issues, text).unwrap();
    assert!(matches!(
        prepare::prepare(&root, &v("0.2.0-alpha.3"), "2026-12-02"),
        Err(PrepareError::NothingToRelease)
    ));
}

#[test]
fn prepare_marks_the_issues_it_releases_fixes_for_as_fixed() {
    let text = "# Known issues.\n\n[[issue]]\nid = \"AX-0001\"\nstatus = \"open\"  # until 0.2\r\n\n[[issue]]\nid = \"AX-0010\"\nstatus = \"open\"\n";
    assert_eq!(
        prepare::mark_issues_fixed(text, &["AX-0001"]),
        "# Known issues.\n\n[[issue]]\nid = \"AX-0001\"\nstatus = \"fixed\"\r\n\n[[issue]]\nid = \"AX-0010\"\nstatus = \"open\"\n"
    );
}

#[test]
fn prepare_refuses_an_invalid_history() {
    let root = repo("prepare-invalid", "0.2.0-alpha.1");
    write(
        &root,
        "releases/unreleased/bad.toml",
        "category = \"breaking\"\nsubsystem = \"project\"\ntitle = \"Oops\"\n",
    );
    assert!(matches!(
        prepare::prepare(&root, &v("0.2.0-alpha.2"), "2026-12-01"),
        Err(PrepareError::Invalid(_))
    ));
    // Nothing was written.
    assert!(!root.join("releases/history/0.2.0-alpha.2.toml").exists());
}

#[test]
fn a_format_change_needs_a_breaking_entry_and_prepare_records_the_formats() {
    let root = sample("formats");
    let history = History::load(&root).unwrap();
    let then = history.latest().unwrap().formats["project"];
    assert!(
        history
            .unreleased()
            .iter()
            .all(|(_, c)| c.category != crate::Category::Breaking)
    );
    write(
        &root,
        "formats.toml",
        &format!("[formats]\nproject = {}\nsave = 2\n", then + 1),
    );
    // No breaking entry: a problem naming the format. A format the last
    // release didn't record (`save`) isn't a change.
    let problems: Vec<String> = History::load(&root)
        .unwrap()
        .validate()
        .into_iter()
        .map(|p| format!("{}: {}", p.file, p.message))
        .collect();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("formats.toml: `project`"),
        "{problems:?}"
    );
    assert!(!problems[0].contains("save"));
    // With one: fine.
    write(
        &root,
        "unreleased/format.toml",
        "category = \"breaking\"\nsubsystem = \"project\"\ntitle = \"Format 6\"\ndetails = \"Older engines can't open it.\"\n",
    );
    let problems = History::load(&root).unwrap().validate();
    assert!(problems.is_empty(), "{problems:?}");

    // prepare copies the formats into the record.
    let root = repo("prepare-formats", "0.2.0-alpha.1");
    let then = History::load(&root.join("releases"))
        .unwrap()
        .latest()
        .unwrap()
        .formats["project"];
    write(
        &root.join("releases"),
        "formats.toml",
        &format!("[formats]\nproject = {then}\nsave = 2\n"),
    );
    prepare::prepare(&root, &v("0.2.0-alpha.2"), "2026-12-01").unwrap();
    let history = History::load(&root.join("releases")).unwrap();
    let record = history.release(&v("0.2.0-alpha.2")).unwrap();
    assert_eq!(record.formats["project"], then);
    assert_eq!(record.formats["save"], 2);
}

#[test]
fn the_repositorys_own_release_files_are_consistent() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let problems = prepare::check(&repo, None).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    let history = History::load(&repo.join(prepare::RELEASES_DIR)).unwrap();
    // The latest release: update this with each `prepare`.
    assert_eq!(
        history.latest_published().map(|r| r.version.to_string()),
        Some("0.2.0-alpha.1".into())
    );
    // Nothing internal survives into what would be published.
    for (_, change) in history.public().changes() {
        assert!(change.is_public(), "{}", change.title);
    }
}

// What a release publishes (`publish`).

use crate::publish::{self, BuildFacts, HistoryFile, Manifest, PublishError};

#[test]
fn release_notes_list_public_changes_by_kind_then_known_issues() {
    let history = History::load(&sample("notes")).unwrap();
    let install = "## Install\n\nDownload `AxcenityEditor-__VERSION__-windows-x86_64.exe`.\n";
    let notes = publish::release_notes(&history, &v("0.2.0-alpha.1"), install).unwrap();
    assert_eq!(
        notes,
        "Host and join games over the network.\n\n\
         This is a **Development** (alpha) release, for testing: expect bugs and breaking changes.\n\n\
         ## Breaking changes\n\n\
         - **Projects**: Project format 5\n  \
         Projects saved by 0.2 can't be opened by 0.1. Keep the backup in .axcenity/previous.\n\n\
         ## New features\n\n\
         - **Multiplayer**: Host and join games (#34)\n\n\
         ## Bug fixes\n\n\
         - **Terrain**: Terrain layers load in the right order\n\n\
         ## Known issues\n\n\
         - AX-0002: Shadows flicker at dawn\n\n\
         ## Install\n\nDownload `AxcenityEditor-0.2.0-alpha.1-windows-x86_64.exe`.\n"
    );
    // A record still waiting for its title or summary can't be published.
    let root = sample("notes-unwritten");
    let record = root.join("history/0.2.0-alpha.1.toml");
    let text = fs::read_to_string(&record).unwrap();
    fs::write(
        &record,
        text.replace("title = \"Multiplayer\"", "title = \"\""),
    )
    .unwrap();
    let history = History::load(&root).unwrap();
    assert!(matches!(
        publish::release_notes(&history, &v("0.2.0-alpha.1"), ""),
        Err(PublishError::Unwritten(_, "title"))
    ));
    assert!(matches!(
        publish::release_notes(&history, &v("9.0.0"), ""),
        Err(PublishError::NoRecord(_))
    ));
}

#[test]
fn experimental_changes_are_listed_apart_from_finished_features() {
    let root = sample("notes-experimental");
    let record = root.join("history/0.2.0-alpha.1.toml");
    let text = fs::read_to_string(&record).unwrap();
    fs::write(
        &record,
        text.replace("prs = [34]\n", "prs = [34]\nstability = \"experimental\"\n"),
    )
    .unwrap();
    let history = History::load(&root).unwrap();
    let notes = publish::Notes::of(history.release(&v("0.2.0-alpha.1")).unwrap());
    assert!(notes.features.is_empty());
    assert_eq!(notes.experimental[0].title, "Host and join games");
    let text = publish::release_notes(&history, &v("0.2.0-alpha.1"), "").unwrap();
    assert!(text.contains("## Experimental\n\n- **Multiplayer**: Host and join games (#34)\n"));
    assert!(!text.contains("## New features"));
}

#[test]
fn the_history_file_holds_only_public_released_entries() {
    let history = History::load(&sample("history-file")).unwrap();
    let file = HistoryFile::from_history(&history);
    let json = serde_json::to_string(&file).unwrap();
    assert_eq!(file.releases.len(), 2);
    assert!(json.contains("Host and join games"));
    for hidden in [
        "Refactor the build script",
        "Hidden experiment",
        "AX-0003",
        "Private crash",
        // Unreleased entries belong to the next release, not this file.
        "Shadows no longer flicker at dawn",
    ] {
        assert!(
            !json.contains(hidden),
            "{hidden} leaked into the history file"
        );
    }
    let back: HistoryFile = serde_json::from_str(&json).unwrap();
    assert_eq!(back, file);
}

/// The packages a release's `dist/` holds, and the history file.
fn dist(name: &str, version: &str) -> PathBuf {
    let dir = scratch(name);
    for (file, text) in [
        (
            format!("AxcenityEditor-{version}-windows-x86_64.exe"),
            "abc",
        ),
        (
            format!("AxcenityEditor-{version}-windows-x86_64-update.zip"),
            "zip",
        ),
        (
            format!("AxcenityEditor-{version}-linux-x86_64.tar.gz"),
            "tar",
        ),
        (
            format!("AxcenityEditor-{version}-linux-x86_64.AppImage"),
            "app",
        ),
        (format!("AxcenityEditor-{version}-macos-universal.dmg"), ""),
        ("axcenity-history.json".to_owned(), "{}"),
        ("SHA256SUMS.txt".to_owned(), "ignored"),
    ] {
        write(&dir, &file, text);
    }
    dir
}

#[test]
fn the_manifest_names_and_hashes_every_package() {
    let history = History::load(&sample("manifest")).unwrap();
    let dir = dist("manifest-dist", "0.2.0-alpha.1");
    let facts = BuildFacts {
        commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        published: "2026-11-20T18:04:11Z".to_owned(),
        ..BuildFacts::default()
    };
    let manifest = Manifest::build(&history, &v("0.2.0-alpha.1"), &dir, facts).unwrap();
    assert_eq!(manifest.tag, "v0.2.0-alpha.1");
    assert_eq!(manifest.channel, crate::Channel::Development);
    assert_eq!(manifest.known_issues, ["AX-0002"]);
    assert_eq!(manifest.formats["network_protocol"], 10);
    assert_eq!(
        manifest.compatibility.min_update_protocol,
        publish::UPDATE_PROTOCOL
    );
    assert_eq!(manifest.provenance.reproducible, "not verified");
    let windows = &manifest.artifacts["windows-x86_64"];
    // SHA-256 of "abc", from FIPS 180-2.
    assert_eq!(
        windows["installer"].sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(windows["installer"].size, 3);
    assert_eq!(
        windows["update"].file,
        "AxcenityEditor-0.2.0-alpha.1-windows-x86_64-update.zip"
    );
    let platforms: Vec<&str> = manifest.artifacts.keys().map(String::as_str).collect();
    assert_eq!(
        platforms,
        ["linux-x86_64", "macos-universal", "windows-x86_64"]
    );
    assert_eq!(
        manifest.artifacts["linux-x86_64"]
            .keys()
            .collect::<Vec<_>>(),
        ["appimage", "update"]
    );
    assert_eq!(manifest.history.file, "axcenity-history.json");
    assert_eq!(manifest.notes.breaking[0].title, "Project format 5");

    // The same manifest after a round trip, with a field a later schema adds.
    let mut json = serde_json::to_value(&manifest).unwrap();
    json["added_later"] = serde_json::json!(true);
    let back: Manifest = serde_json::from_value(json).unwrap();
    assert_eq!(back, manifest);
}

#[test]
fn the_manifest_refuses_packages_it_does_not_know() {
    let history = History::load(&sample("manifest-unknown")).unwrap();
    for stray in [
        "AxcenityEditor-0.2.0-alpha.1-windows-arm64.exe",
        // Left over from another version's build.
        "AxcenityEditor-0.1.0-alpha.3-windows-x86_64.exe",
    ] {
        let dir = dist(&format!("manifest-stray-{}", stray.len()), "0.2.0-alpha.1");
        write(&dir, stray, "x");
        let err = Manifest::build(&history, &v("0.2.0-alpha.1"), &dir, BuildFacts::default())
            .unwrap_err();
        assert!(
            matches!(err, PublishError::UnknownPackage(ref f) if f == stray),
            "{err}"
        );
    }
    let empty = scratch("manifest-empty");
    write(&empty, "axcenity-history.json", "{}");
    assert!(matches!(
        Manifest::build(&history, &v("0.2.0-alpha.1"), &empty, BuildFacts::default()),
        Err(PublishError::NoPackages(..))
    ));
}

#[test]
fn package_names_say_platform_and_role() {
    let version = v("1.0.0-rc.2");
    assert_eq!(
        publish::package_kind(
            "AxcenityEditor-1.0.0-rc.2-linux-x86_64.pkg.tar.zst",
            &version
        ),
        Some(("linux-x86_64", "pacman"))
    );
    assert_eq!(
        publish::package_kind("AxcenityEditor-1.0.0-rc.2-linux-x86_64.deb", &version),
        Some(("linux-x86_64", "deb"))
    );
    // A version that only starts the same way is a different version.
    assert_eq!(
        publish::package_kind("AxcenityEditor-1.0.0-rc.20-linux-x86_64.deb", &version),
        None
    );
}

#[test]
fn the_release_plan_follows_the_channel() {
    let alpha = prepare::ReleasePlan::for_version(&v("0.2.0-alpha.1"));
    assert_eq!((alpha.profile, alpha.prerelease), ("release", true));
    assert_eq!(alpha.tag, "v0.2.0-alpha.1");
    let beta = prepare::ReleasePlan::for_version(&v("0.2.0-rc.1"));
    assert_eq!((beta.profile, beta.prerelease), ("dist", true));
    let stable = prepare::ReleasePlan::for_version(&v("1.0.0"));
    assert_eq!((stable.profile, stable.prerelease), ("dist", false));
    assert_eq!(
        serde_json::to_value(&stable).unwrap(),
        serde_json::json!({
            "version": "1.0.0", "tag": "v1.0.0", "channel": "stable", "prerelease": false,
            "profile": "dist", "platforms": ["windows", "linux", "macos"]
        })
    );
}

#[test]
fn the_history_reads_the_same_from_texts_as_from_disk() {
    let root = sample("sources");
    let mut sources = Vec::new();
    for sub in ["history", "unreleased", "features"] {
        for entry in fs::read_dir(root.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            sources.push((format!("{sub}/{name}"), fs::read_to_string(&path).unwrap()));
        }
    }
    sources.push((
        "issues.toml".to_owned(),
        fs::read_to_string(root.join("issues.toml")).unwrap(),
    ));
    // Order and Windows separators don't matter; other files are ignored.
    sources.reverse();
    sources[0].0 = sources[0].0.replace('/', "\\");
    sources.push(("README.md".to_owned(), "# not a release file".to_owned()));
    let from_texts = History::from_sources(sources).unwrap();
    assert_eq!(from_texts, History::load(&root).unwrap());
    assert!(matches!(
        History::from_sources([("history/0.3.0.toml", "version = ")]),
        Err(crate::LoadError::Parse { .. })
    ));
}
