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
    Git::system().clone_repository(url, &dest, None).unwrap();
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
    // …unless someone asks (the Console's `checkpoint`, for now).
    app.run_console_command("checkpoint Before lunch");
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
    std::fs::write(
        folder.join("scripts").join("Extra.server.lua"),
        "print('x')\n",
    )
    .unwrap();
    app.repo_changed();
    app.repo_close();
    let log = remote_log(&url, "repo-close");
    assert_eq!(log[0].kind, Some(Kind::Closing), "{:?}", log[0]);
    let _ = std::fs::remove_dir_all(&folder);
}

/// Runs `git` in `dir` as Sam.
fn sam_git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=Sam", "-c", "user.email=sam@example.com"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn a_file_changed_on_both_sides_keeps_both_until_someone_chooses() {
    let (folder, url) = linked_project("repo-conflict");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    frames_until(&mut app, &State::Synced);
    // Sam changes the script elsewhere and uploads first.
    let sam = std::env::temp_dir().join(format!(
        "axcenity-editor-sam-repo-conflict-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&sam);
    Git::system().clone_repository(&url, &sam, None).unwrap();
    std::fs::create_dir_all(sam.join("scripts")).unwrap();
    std::fs::write(sam.join("scripts/Note.lua"), "return 'sam'\n").unwrap();
    std::fs::write(sam.join("notes.txt"), "sam\n").unwrap();
    sam_git(&sam, &["add", "--all"]);
    sam_git(&sam, &["commit", "--quiet", "-m", "Sam's"]);
    sam_git(&sam, &["push", "--quiet", "origin", "HEAD:main"]);
    // Here, the same files differently.
    std::fs::create_dir_all(folder.join("scripts")).unwrap();
    std::fs::write(folder.join("scripts/Note.lua"), "return 'paul'\n").unwrap();
    std::fs::write(folder.join("notes.txt"), "paul\n").unwrap();
    app.run_console_command("checkpoint");
    let start = Instant::now();
    while !matches!(app.repo.state(), Some(State::Conflict(_))) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{:?}",
            app.repo.last
        );
        app.repo_frame();
        std::thread::sleep(Duration::from_millis(20));
    }
    // Nothing overwritten; theirs is beside each.
    let found = app.repo_conflicts().unwrap();
    assert_eq!(found.len(), 2);
    let note = found.iter().find(|c| c.path == "scripts/Note.lua").unwrap();
    let copy = note.copy.clone().unwrap();
    assert!(copy.starts_with("scripts/Note.remote-"), "{copy}");
    assert_eq!(
        std::fs::read_to_string(folder.join("scripts/Note.lua")).unwrap(),
        "return 'paul'\n"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join(&copy)).unwrap(),
        "return 'sam'\n"
    );
    app.run_console_command("conflicts");
    assert!(
        app.output
            .lines()
            .iter()
            .any(|l| l.text.contains("2 files changed both here"))
    );

    // One choice at a time: the merge waits for the last.
    app.run_console_command("resolve both scripts/Note.lua");
    assert!(
        app.output
            .lines()
            .iter()
            .any(|l| l.text.contains("1 file still to choose"))
    );
    app.run_console_command("resolve theirs notes.txt");
    frames_until(&mut app, &State::Synced);
    assert_eq!(
        std::fs::read_to_string(folder.join("notes.txt")).unwrap(),
        "sam\n"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("scripts/Note.lua")).unwrap(),
        "return 'paul'\n"
    );
    let log = remote_log(&url, "repo-conflict");
    assert_eq!(log[0].kind, Some(Kind::Merge), "{:?}", log[0]);
    let check = std::env::temp_dir().join(format!(
        "axcenity-editor-check-repo-conflict-{}",
        std::process::id()
    ));
    assert_eq!(
        std::fs::read_to_string(check.join(&copy)).unwrap(),
        "return 'sam'\n"
    );
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_dir_all(&sam);
    let _ = std::fs::remove_dir_all(&check);
}

/// Draws the Collaborate panel and the windows; what was drawn, as text.
fn draw_collaborate(app: &mut EditorApp, ctx: &egui::Context) -> String {
    let mut drawn = String::new();
    for _ in 0..2 {
        let output = theme::run(ctx, Default::default(), |ui| {
            app.team_create_pane(ui);
            app.repo_ui_frame(ui.ctx());
        });
        drawn = format!("{:?}", output.shapes);
    }
    drawn
}

/// Waits for the Project section's background task.
fn task_done(app: &mut EditorApp, ctx: &egui::Context) {
    let start = Instant::now();
    while app.repo.ui.busy_for_test() {
        assert!(start.elapsed() < Duration::from_secs(60));
        let _ = theme::run(ctx, Default::default(), |ui| app.repo_ui_frame(ui.ctx()));
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_project_section_shows_the_repository_and_its_history() {
    let (folder, _url) = linked_project("repo-panel");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    frames_until(&mut app, &State::Synced);
    let ctx = theme::test_context();
    let drawn = draw_collaborate(&mut app, &ctx);
    for words in [
        "Project",
        "Repository",
        "Synced",
        "Sync now",
        "History…",
        "Manage…",
    ] {
        assert!(drawn.contains(words), "{words} isn't drawn");
    }
    // History lists the checkpoints.
    app.repo.ui.history_open = true;
    app.load_repo_history();
    task_done(&mut app, &ctx);
    let entries = app.repo.ui.history.clone().expect("the history was read");
    assert!(entries.iter().any(|e| e.title() == "First checkpoint"));
    let drawn = draw_collaborate(&mut app, &ctx);
    assert!(drawn.contains("First checkpoint"));
    assert!(drawn.contains("Restore…"));
    // Manage: not signed in to GitHub, and the link shown.
    app.open_manage_repository();
    task_done(&mut app, &ctx);
    assert!(matches!(app.repo.ui.account, Some(Ok(None))));
    let drawn = draw_collaborate(&mut app, &ctx);
    assert!(drawn.contains("Not signed in"));
    assert!(drawn.contains("Disconnect…"));
    if let Some(dir) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        crate::snapshot::snapshot(
            &std::path::Path::new(&dir).join("collaborate-project.png"),
            egui::vec2(260.0, 640.0),
            vec![Vec::new(); 3],
            &mut |ui: &mut egui::Ui| app.team_create_pane(ui),
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_project_without_a_repository_offers_to_connect_one_and_the_host_offer_is_kept() {
    let folder = copy_of_example("hello-part", "repo-panel-none");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    app.repo_frame();
    let ctx = theme::test_context();
    let drawn = draw_collaborate(&mut app, &ctx);
    assert!(drawn.contains("Connect a repository…"));
    // "Let others host from my computer" off is kept between runs.
    app.set_host_offer(false);
    assert!(app.team_panel.to_text().contains("nohost"));
    let mut panel = super::team_panel::TeamPanel::default();
    panel.read_text(&app.team_panel.to_text());
    assert!(panel.no_host_offer);
    app.set_host_offer(true);
    assert!(!app.team_panel.to_text().contains("nohost"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_resolve_window_merges_with_each_files_choice() {
    let (folder, url) = linked_project("repo-resolve-window");
    let mut app = EditorApp::new(Some(&folder));
    app.repo.manager = Some(manager());
    frames_until(&mut app, &State::Synced);
    let sam = std::env::temp_dir().join(format!(
        "axcenity-editor-sam-repo-resolve-window-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&sam);
    Git::system().clone_repository(&url, &sam, None).unwrap();
    std::fs::write(sam.join("notes.txt"), "sam\n").unwrap();
    sam_git(&sam, &["add", "--all"]);
    sam_git(&sam, &["commit", "--quiet", "-m", "Sam's"]);
    sam_git(&sam, &["push", "--quiet", "origin", "HEAD:main"]);
    std::fs::write(folder.join("notes.txt"), "paul\n").unwrap();
    app.run_console_command("checkpoint");
    let start = Instant::now();
    while !matches!(app.repo.state(), Some(State::Conflict(_))) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{:?}",
            app.repo.last
        );
        app.repo_frame();
        std::thread::sleep(Duration::from_millis(20));
    }
    let ctx = theme::test_context();
    let drawn = draw_collaborate(&mut app, &ctx);
    assert!(drawn.contains("Conflict"));
    assert!(drawn.contains("Resolve…"));
    app.open_conflicts_window();
    let drawn = draw_collaborate(&mut app, &ctx);
    assert!(drawn.contains("notes.txt"));
    assert!(drawn.contains("0 of 1 chosen"));
    app.repo
        .ui
        .choices
        .insert("notes.txt".into(), axcenity_repo::conflicts::Choice::Mine);
    app.resolve_chosen();
    assert!(!app.repo.ui.conflicts_open);
    frames_until(&mut app, &State::Synced);
    assert_eq!(
        std::fs::read_to_string(folder.join("notes.txt")).unwrap(),
        "paul\n"
    );
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_dir_all(&sam);
}
