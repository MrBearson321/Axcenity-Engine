//! The open project's repository in the editor (Collaborate 2.0, P2):
//! saves become checkpoints that reach the remote, and a member's copy
//! never commits.

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_repo::checkpoint::{self, Kind};
use axcenity_repo::credentials::MemoryCredentials;
use axcenity_repo::github::GitHub;
use axcenity_repo::repository::{self, RemoteSpec};
use axcenity_repo::sync::State;
use axcenity_repo::{Author, Git, RepositoryManager};

use super::tests::copy_of_example;
use super::*;
use axcenity_core::PartType;
use axcenity_project as project;

fn bare_remote(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "axcenity-editor-remote-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ok = Command::new("git")
        .args(["init", "--quiet", "--bare", "--initial-branch", "main"])
        .current_dir(&dir)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    format!("file://{}", dir.to_string_lossy().replace('\\', "/"))
}

/// A copy of hello-part linked to a new bare remote, as connecting it in
/// the panel would leave it.
fn linked_project(name: &str) -> (std::path::PathBuf, String) {
    let folder = copy_of_example("hello-part", name);
    let url = bare_remote(name);
    let spec = RemoteSpec {
        url: url.clone(),
        branch: Some("main".into()),
        replace_remote: false,
    };
    let author = Author {
        name: "Tester".into(),
        email: "tester@example.com".into(),
    };
    repository::link(&Git::system(), &folder, &spec, &author).unwrap();
    project::save_collaborate_file(
        &folder,
        &project::CollaborateFile {
            project: None,
            name: Some("Hello".into()),
            repository: Some(project::RepositoryLink {
                provider: "git".into(),
                url: url.clone(),
                branch: "main".into(),
            }),
        },
    )
    .unwrap();
    (folder, url)
}

fn manager() -> Arc<RepositoryManager> {
    Arc::new(RepositoryManager::new(
        Git::system(),
        GitHub::default(),
        Arc::new(MemoryCredentials::default()),
    ))
}

/// Runs frames until the sync state is `want` (or panics after 60 s).
fn frames_until(app: &mut EditorApp, want: &State) {
    let start = Instant::now();
    loop {
        app.repo_frame();
        if app.repo.state() == Some(want) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "still {:?}, last {:?}",
            app.repo.state(),
            app.repo.last
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn remote_log(url: &str, test: &str) -> Vec<checkpoint::Entry> {
    let dest = std::env::temp_dir().join(format!(
        "axcenity-editor-check-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dest);
    Git::system()
        .clone_repository(url, &dest, None)
        .unwrap();
    checkpoint::history(&Git::system(), &dest, 20).unwrap()
}

#[test]
fn saves_become_checkpoints_that_reach_the_repository() {
    let (folder, url) = linked_project("repo-saves");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    // Opening a linked project uploads what's waiting (collaborate.json).
    frames_until(&mut app, &State::Synced);
    assert!(app.repo.link.is_some());

    // A save marks it changed; it waits for the interval…
    app.insert(Insertable::Part(PartType::Block), None);
    let part = app.selected.expect("the new part is selected");
    app.change(
        part,
        Field::Name,
        "Rename to Crate".into(),
        vec![crate::properties::Edit::Name("Crate".into())],
    );
    app.history.seal();
    assert!(app.save());
    app.repo_frame();
    assert_eq!(app.repo.state(), Some(&State::Pending));
    // …unless someone asks.
    app.repo_checkpoint(Kind::Manual, Some("Before lunch".into()));
    frames_until(&mut app, &State::Synced);
    let log = remote_log(&url, "repo-saves");
    assert_eq!(log[0].kind, Some(Kind::Manual));
    assert_eq!(log[0].title(), "Manual checkpoint: Before lunch");
    assert!(log.iter().any(|e| e.title() == "First checkpoint"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_project_without_a_repository_never_runs_git() {
    let folder = copy_of_example("hello-part", "repo-none");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    for _ in 0..3 {
        app.repo_frame();
    }
    assert_eq!(app.repo.state(), None);
    assert!(!folder.join(".git").exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn closing_the_editor_uploads_what_was_saved() {
    let (folder, url) = linked_project("repo-close");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    frames_until(&mut app, &State::Synced);
    std::fs::create_dir_all(folder.join("scripts")).unwrap();
    std::fs::write(folder.join("scripts").join("Extra.server.lua"), "print('x')\n").unwrap();
    app.repo_changed();
    app.repo_close();
    let log = remote_log(&url, "repo-close");
    assert_eq!(log[0].kind, Some(Kind::Closing), "{:?}", log[0]);
    let _ = std::fs::remove_dir_all(&folder);
}

