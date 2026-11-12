//! Conflicts between a project folder and its repository, over the real
//! `git` with a bare `file://` remote.

use std::path::{Path, PathBuf};

use super::*;
use crate::checkpoint::Kind;
use crate::repository::{self, Linked, RemoteSpec};
use crate::sync::{Job, State, sync_once};
use crate::testing::{author, bare_remote, git, temp, write};

fn linked_copy(git: &Git, url: &str, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = temp(name);
    for (path, text) in files {
        write(&dir, path, text);
    }
    let spec = RemoteSpec {
        url: url.to_owned(),
        branch: Some("main".into()),
        replace_remote: false,
    };
    let (_, linked) = repository::link(git, &dir, &spec, &author(name)).unwrap();
    if let Linked::Adopted { differences } = linked
        && !differences.is_empty()
    {
        repository::take_remote_version(git, &dir, "main").unwrap();
    }
    dir
}

fn sync(git: &Git, dir: &Path, who: &str) -> State {
    sync_once(
        git,
        &Job {
            dir: dir.to_owned(),
            branch: "main".into(),
            kind: Kind::Auto,
            label: None,
            author: author(who),
            participants: vec![who.to_owned()],
        },
    )
    .state
}

fn read(dir: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(path)).ok()
}

/// Paul and Sam start from the same files; Paul changes `paul` and
/// uploads, Sam changes `sam` and syncs into a conflict.
fn conflict(
    files: &[(&str, &str)],
    paul: &[(&str, Option<&str>)],
    sam: &[(&str, Option<&str>)],
) -> (Git, String, PathBuf, PathBuf) {
    let git = git();
    let (_remote, url) = bare_remote(&git);
    let a = linked_copy(&git, &url, "Paul", files);
    let b = linked_copy(&git, &url, "Sam", &[]);
    for (dir, changes) in [(&a, paul), (&b, sam)] {
        for (path, text) in changes {
            match text {
                Some(text) => write(dir, path, text),
                None => std::fs::remove_file(dir.join(path)).unwrap(),
            }
        }
    }
    assert_eq!(sync(&git, &a, "Paul"), State::Synced);
    assert!(matches!(sync(&git, &b, "Sam"), State::Conflict(_)));
    (git, url, a, b)
}

#[test]
fn a_copy_is_named_beside_the_file_before_its_extension() {
    assert_eq!(
        copy_path("scripts/Door.server.lua", "1a2b3c4d5e"),
        "scripts/Door.server.remote-1a2b3c4.lua"
    );
    assert_eq!(
        copy_path("world.json", "abcdef0123"),
        "world.remote-abcdef0.json"
    );
    assert_eq!(copy_path("README", "abcdef0123"), "README.remote-abcdef0");
    assert_eq!(copy_path(".env", "abcdef0123"), ".env.remote-abcdef0");
    assert_eq!(Choice::parse(" Theirs "), Some(Choice::Theirs));
    assert_eq!(Choice::parse("maybe"), None);
}

#[test]
fn a_conflict_writes_the_repositorys_version_beside_mine_and_never_uploads_it() {
    let (git, url, _a, b) = conflict(
        &[("world.json", "{\n\"x\": 0\n}\n")],
        &[("world.json", Some("{\n\"x\": 1\n}\n"))],
        &[("world.json", Some("{\n\"x\": 2\n}\n"))],
    );
    let found = list(&git, &b, "main").unwrap();
    assert_eq!(found.len(), 1);
    let conflict = &found[0];
    assert_eq!(conflict.path, "world.json");
    assert!(conflict.text);
    assert_eq!(conflict.mine.author.as_deref(), Some("Sam"));
    assert_eq!(conflict.theirs.author.as_deref(), Some("Paul"));
    assert!(!conflict.mine.deleted && !conflict.theirs.deleted);
    let copy = conflict
        .copy
        .clone()
        .expect("the repository's version is beside mine");
    assert!(copy.starts_with("world.remote-") && copy.ends_with(".json"));
    // Mine is untouched; theirs is in the copy.
    assert_eq!(read(&b, "world.json").unwrap(), "{\n\"x\": 2\n}\n");
    assert_eq!(read(&b, &copy).unwrap(), "{\n\"x\": 1\n}\n");
    // The copy isn't a change to checkpoint, and syncing again leaves it be.
    assert!(git.changes(&b).unwrap().is_empty());
    assert!(matches!(sync(&git, &b, "Sam"), State::Conflict(_)));
    let check = temp("check").join("c");
    git.clone_repository(&url, &check, None).unwrap();
    assert!(!check.join(&copy).exists());
    assert_eq!(read(&check, "world.json").unwrap(), "{\n\"x\": 1\n}\n");
}

#[test]
fn each_file_is_resolved_as_chosen_and_the_merge_uploads() {
    let base = [
        ("a.txt", "a\n"),
        ("b.txt", "b\n"),
        ("c.txt", "c\n"),
        ("d.txt", "d\n"),
        ("same.txt", "1\n"),
    ];
    let (git, url, a, b) = conflict(
        &base,
        &[
            ("a.txt", Some("paul a\n")),
            ("b.txt", Some("paul b\n")),
            ("c.txt", Some("paul c\n")),
            ("d.txt", None),
            ("paul-only.txt", Some("new\n")),
        ],
        &[
            ("a.txt", Some("sam a\n")),
            ("b.txt", Some("sam b\n")),
            ("c.txt", Some("sam c\n")),
            ("d.txt", Some("sam d\n")),
        ],
    );
    let found = list(&git, &b, "main").unwrap();
    let paths: Vec<&str> = found.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "b.txt", "c.txt", "d.txt"]);
    let d = found.iter().find(|c| c.path == "d.txt").unwrap();
    assert!(d.theirs.deleted && d.copy.is_none());
    // Sam keeps editing before choosing: that's kept too.
    write(&b, "notes.txt", "sam's notes\n");
    // Not every file chosen: nothing happens.
    let some = vec![("a.txt".to_owned(), Choice::Mine)];
    assert!(matches!(
        resolve(&git, &b, "main", &some, &author("Sam")),
        Err(ResolveError::Undecided(_))
    ));
    assert_eq!(read(&b, "a.txt").unwrap(), "sam a\n");
    assert!(!repository::status(&git, &b, "main").merge_in_progress);

    let choices = vec![
        ("a.txt".to_owned(), Choice::Mine),
        ("b.txt".to_owned(), Choice::Theirs),
        ("c.txt".to_owned(), Choice::Both),
        ("d.txt".to_owned(), Choice::Theirs),
    ];
    let resolved = resolve(&git, &b, "main", &choices, &author("Sam")).unwrap();
    assert_eq!(resolved.entries.len(), 4);
    let copy = found
        .iter()
        .find(|c| c.path == "c.txt")
        .unwrap()
        .copy
        .clone()
        .unwrap();
    assert_eq!(read(&b, "a.txt").unwrap(), "sam a\n");
    assert_eq!(read(&b, "b.txt").unwrap(), "paul b\n");
    assert_eq!(read(&b, "c.txt").unwrap(), "sam c\n");
    assert_eq!(read(&b, &copy).unwrap(), "paul c\n");
    assert_eq!(read(&b, "d.txt"), None);
    assert_eq!(read(&b, "paul-only.txt").unwrap(), "new\n");
    assert_eq!(read(&b, "notes.txt").unwrap(), "sam's notes\n");
    // The only copy left is the one kept on purpose.
    for other in ["a.txt", "b.txt"] {
        let theirs = found.iter().find(|c| c.path == other).unwrap();
        assert!(!b.join(theirs.copy.clone().unwrap()).exists());
    }
    // The checkpoint says what was chosen, and the record is kept.
    let head = git.log(&b, "HEAD", 1).unwrap().remove(0);
    assert!(
        head.subject.contains("conflicts resolved"),
        "{}",
        head.subject
    );
    assert!(head.body.contains("Kept mine: a.txt"), "{}", head.body);
    assert!(head.body.contains(&format!("the repository's as {copy}")));
    let kept = history(&git, &b);
    assert_eq!(kept.len(), 4);
    assert_eq!(kept[2].choice, Choice::Both);
    assert_eq!(kept[0].by, "Sam");
    // Sam's version of everything stays in the history.
    assert!(git.is_ancestor(&b, &kept[0].mine, "HEAD"));

    // Uploaded, and Paul gets it.
    assert_eq!(sync(&git, &b, "Sam"), State::Synced);
    assert_eq!(sync(&git, &a, "Paul"), State::Synced);
    assert_eq!(read(&a, "a.txt").unwrap(), "sam a\n");
    assert_eq!(read(&a, &copy).unwrap(), "paul c\n");
    let check = temp("check").join("c");
    git.clone_repository(&url, &check, None).unwrap();
    assert_eq!(read(&check, "notes.txt").unwrap(), "sam's notes\n");
}

#[test]
fn a_binary_file_is_never_merged_and_both_are_kept() {
    let (git, _url, _a, b) = conflict(
        &[("texture.png", "PNG\0one")],
        &[("texture.png", Some("PNG\0paul"))],
        &[("texture.png", Some("PNG\0sam"))],
    );
    let found = list(&git, &b, "main").unwrap();
    assert!(!found[0].text);
    let copy = found[0].copy.clone().unwrap();
    assert_eq!(std::fs::read(b.join(&copy)).unwrap(), b"PNG\0paul");
    let choices = vec![("texture.png".to_owned(), Choice::Both)];
    resolve(&git, &b, "main", &choices, &author("Sam")).unwrap();
    assert_eq!(std::fs::read(b.join("texture.png")).unwrap(), b"PNG\0sam");
    assert_eq!(std::fs::read(b.join(&copy)).unwrap(), b"PNG\0paul");
}

#[test]
fn a_resolve_the_editor_never_finished_is_undone_and_asked_again() {
    let (git, _url, _a, b) = conflict(
        &[("a.txt", "1\n")],
        &[("a.txt", Some("paul\n"))],
        &[("a.txt", Some("sam\n"))],
    );
    // A merge started and never finished, as a crash mid-resolve leaves.
    let _ = git.try_run(
        &b,
        &[
            "-c",
            "user.name=Sam",
            "-c",
            "user.email=sam@example.com",
            "merge",
            "--no-commit",
            "origin/main",
        ],
        None,
    );
    assert!(repository::status(&git, &b, "main").merge_in_progress);
    assert_eq!(sync(&git, &b, "Sam"), State::Conflict(vec!["a.txt".into()]));
    assert!(!repository::status(&git, &b, "main").merge_in_progress);
    assert_eq!(read(&b, "a.txt").unwrap(), "sam\n");
    let found = list(&git, &b, "main").unwrap();
    assert_eq!(read(&b, &found[0].copy.clone().unwrap()).unwrap(), "paul\n");
    let choices = vec![("a.txt".to_owned(), Choice::Theirs)];
    resolve(&git, &b, "main", &choices, &author("Sam")).unwrap();
    assert_eq!(read(&b, "a.txt").unwrap(), "paul\n");
    assert!(matches!(
        resolve(&git, &b, "main", &choices, &author("Sam")),
        Err(ResolveError::Nothing)
    ));
}
