//! The brief's scenarios end to end in the editor (Collaborate 2.0, P7):
//! sessions over real sockets on 127.0.0.1, the project's repository a
//! bare `file://` remote. Scenarios D and E alone are in
//! `collab_repo_tests.rs`; A and B alone in `collab_handoff_tests.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use axcenity_collab::Role;
use axcenity_collab::access::Invite;
use axcenity_project as project;
use axcenity_repo::Git;
use axcenity_repo::checkpoint::{self, Kind};
use axcenity_repo::repository;
use axcenity_repo::sync::State;

use super::collab_handoff_tests::{
    add_part, data_folder, has_part, hosting, joined, loopback, on_loopback, session, settle,
    successors,
};
use super::collab_repo_tests::{frames_until, linked_project, manager, remote_log};
use super::*;

/// Runs `app` (its session, its repository, the Project section's
/// background work) until a checkpoint of `kind` is in `folder`'s history
/// and uploaded.
fn uploaded(app: &mut EditorApp, folder: &Path, kind: Kind, ctx: &egui::Context) {
    let start = Instant::now();
    loop {
        app.team_frame();
        app.repo_frame();
        let _ = theme::run(ctx, Default::default(), |ui| app.repo_ui_frame(ui.ctx()));
        let made = checkpoint::history(&Git::system(), folder, 20)
            .is_ok_and(|entries| entries.iter().any(|e| e.kind == Some(kind)));
        if made && !app.repo.schedule.is_running() && app.repo.state() == Some(&State::Synced) {
            return;
        }
        if start.elapsed() > Duration::from_secs(60) {
            for line in app.output.lines() {
                eprintln!("{}", line.text);
            }
            panic!("no {kind:?} checkpoint uploaded; last {:?}", app.repo.last);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn tracked_files(folder: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files"])
        .current_dir(folder)
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn project_id(folder: &Path) -> Option<String> {
    project::load_collaborate_file(folder).unwrap().project
}

fn folder_of(app: &EditorApp) -> PathBuf {
    app.project.as_ref().unwrap().folder.clone()
}

/// Scenarios A and C with a repository: Paul hosts a linked project and
/// leaves; Sam hosts on from his copy, which takes up the project's
/// history, and then ends the session too. Later Kim gets the project from
/// the repository, with everything, and hosts it again as the same project.
#[test]
fn when_everyone_leaves_the_project_waits_in_its_repository() {
    let ctx = theme::test_context();
    let (folder, url) = linked_project("scenario-c");
    let data: Vec<PathBuf> = ["paul", "sam", "kim", "lee"]
        .iter()
        .map(|who| data_folder(&format!("scenario-c-{who}")))
        .collect();
    let mut paul = Box::new(EditorApp::new(Some(&folder)));
    paul.repo.manager = Some(manager());
    frames_until(&mut paul, &State::Synced);
    let invite = paul
        .host_network_session(Some(&data[0]), 0)
        .expect("hosting");
    let id = project_id(&folder).expect("hosting gives the project an ID");
    let mut sam = Box::new(EditorApp::new(None));
    sam.repo.manager = Some(manager());
    sam.join_network_session(&loopback(&invite), Some(&data[1]))
        .expect("Sam joins");
    on_loopback(&mut sam);
    settle(&mut [&mut *paul, &mut *sam], "Sam in", |apps| {
        joined(apps[1]) && apps[1].project.is_some() && successors(apps[1]) == 1
    });
    add_part(&mut sam, "FromSam");
    settle(&mut [&mut *paul, &mut *sam], "Sam's part", |apps| {
        has_part(apps[0], "FromSam")
    });

    // A: Paul leaves. His computer uploads the session as it was.
    paul.leave_team_session();
    assert!(paul.team.is_none());
    uploaded(&mut paul, &folder, Kind::Handoff, &ctx);
    let before = tracked_files(&folder);

    // Sam hosts on. His copy never had `.git` (nor `world.json`): it takes
    // up the project's history and uploads the session on top.
    settle(&mut [&mut *sam], "Sam hosting", |apps| {
        hosting(apps[0]) && joined(apps[0])
    });
    let sams = folder_of(&sam);
    uploaded(&mut sam, &sams, Kind::Handoff, &ctx);
    assert!(sams.join(".git").exists());
    assert!(
        sam.output
            .lines()
            .iter()
            .any(|line| line.text.contains("keeps the project's history"))
    );
    add_part(&mut sam, "WhileSamHosts");

    // C: Sam ends it; nobody is left. The repository has the last state.
    sam.end_team_session();
    assert!(sam.team.is_none());
    uploaded(&mut sam, &sams, Kind::Ended, &ctx);
    let kinds: Vec<Option<Kind>> = remote_log(&url, "scenario-c")
        .iter()
        .map(|e| e.kind)
        .collect();
    assert!(kinds.contains(&Some(Kind::Handoff)) && kinds.contains(&Some(Kind::Ended)));

    // Later, Kim gets the project from the repository.
    let kims = data_folder("scenario-c-kim-project");
    repository::clone_project(&Git::system(), &url, &kims, None).unwrap();
    let mut kim = Box::new(EditorApp::new(Some(&kims)));
    assert!(has_part(&kim, "FromSam") && has_part(&kim, "WhileSamHosts"));
    assert_eq!(project_id(&kims).as_ref(), Some(&id), "the same project");
    let after = tracked_files(&kims);
    for file in &before {
        assert!(
            after.contains(file),
            "`{file}` went missing (Sam's copy never had it)"
        );
    }

    // She hosts it again under the same ID, and Lee gets everything.
    kim.repo.manager = Some(manager());
    let invite = kim
        .host_network_session(Some(&data[2]), 0)
        .expect("Kim hosts");
    assert_eq!(project_id(&kims), Some(id));
    let mut lee = Box::new(EditorApp::new(None));
    lee.join_network_session(&loopback(&invite), Some(&data[3]))
        .expect("Lee joins");
    settle(&mut [&mut *kim, &mut *lee], "Lee has it all", |apps| {
        joined(apps[1]) && has_part(apps[1], "FromSam") && has_part(apps[1], "WhileSamHosts")
    });
    lee.leave_team_session();
    kim.end_team_session();
    for folder in data.iter().chain([&folder, &kims]) {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// Scenario A twice: Paul leaves, Sam hosts; Sam leaves, the next member
/// hosts. Nobody's part is lost on the way.
#[test]
fn hosting_passes_on_more_than_once() {
    let (mut paul, mut sam, mut kim, mut folders) = session("again");
    let lee_data = data_folder("again-lee");
    let invite = loopback(
        &paul
            .make_team_invite(Role::Editor)
            .map(|link| Invite::parse(&link).unwrap())
            .unwrap(),
    );
    let mut lee = Box::new(EditorApp::new(None));
    lee.join_network_session(&invite, Some(&lee_data))
        .expect("Lee joins");
    on_loopback(&mut lee);
    folders.push(lee_data);
    settle(
        &mut [&mut *paul, &mut *sam, &mut *kim, &mut *lee],
        "everyone in",
        |apps| apps.iter().all(|a| joined(a) && successors(a) == 3),
    );
    add_part(&mut lee, "First");
    settle(
        &mut [&mut *paul, &mut *sam, &mut *kim, &mut *lee],
        "Lee's part",
        |apps| apps.iter().all(|a| has_part(a, "First")),
    );

    paul.leave_team_session();
    settle(
        &mut [&mut *sam, &mut *kim, &mut *lee],
        "Sam hosting, with Kim and Lee offering to host next",
        |apps| hosting(apps[0]) && apps.iter().all(|a| joined(a) && successors(a) == 2),
    );
    add_part(&mut kim, "Second");
    settle(
        &mut [&mut *sam, &mut *kim, &mut *lee],
        "Kim's part",
        |apps| apps.iter().all(|a| has_part(a, "Second")),
    );

    sam.leave_team_session();
    settle(&mut [&mut *kim, &mut *lee], "the next host", |apps| {
        (hosting(apps[0]) || hosting(apps[1])) && apps.iter().all(|a| joined(a))
    });
    add_part(&mut lee, "Third");
    settle(&mut [&mut *kim, &mut *lee], "every part", |apps| {
        apps.iter()
            .all(|a| ["First", "Second", "Third"].iter().all(|n| has_part(a, n)))
    });
    kim.leave_team_session();
    lee.leave_team_session();
    for folder in folders {
        let _ = std::fs::remove_dir_all(folder);
    }
}
