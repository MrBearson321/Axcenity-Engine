//! The session going on in the editor when its host leaves or is lost
//! (Collaborate 2.0, P4): three editors over real sockets on 127.0.0.1.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_collab::Role;
use axcenity_collab::access::Invite;
use axcenity_collab::election::Timing;
use axcenity_core::PartType;

use super::tests::copy_of_example;
use super::*;
use crate::properties::Edit;

fn data_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-collab-handoff-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn settle(apps: &mut [&mut EditorApp], what: &str, done: impl Fn(&[&mut EditorApp]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        if start.elapsed() > Duration::from_secs(40) {
            for app in apps.iter() {
                eprintln!("---");
                for line in app.output.lines() {
                    eprintln!("{}", line.text);
                }
            }
            panic!("timed out waiting for {what}");
        }
        for app in apps.iter_mut() {
            app.team_frame();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn joined(app: &EditorApp) -> bool {
    app.team
        .as_ref()
        .and_then(|t| t.client())
        .is_some_and(|c| c.is_online() && c.is_joined() && c.pending() == 0)
}

fn hosting(app: &EditorApp) -> bool {
    app.team.as_ref().is_some_and(|t| t.hosting().is_some())
}

fn successors(app: &EditorApp) -> usize {
    app.team
        .as_ref()
        .and_then(|t| t.client())
        .map_or(0, |c| c.successors().len())
}

fn has_part(app: &EditorApp, name: &str) -> bool {
    let world = &app.world;
    world
        .descendants(world.root())
        .into_iter()
        .any(|id| world.get(id).is_some_and(|i| i.name() == name))
}

fn add_part(app: &mut EditorApp, name: &str) {
    app.insert(Insertable::Part(PartType::Block), None);
    let part = app.selected.expect("the new part is selected");
    app.change(
        part,
        Field::Name,
        format!("Rename to {name}"),
        vec![Edit::Name(name.into())],
    );
    app.history.seal();
}

fn loopback(invite: &Invite) -> Invite {
    let port = invite.host.rsplit(':').next().unwrap();
    Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite.clone()
    }
}

/// A member who may host offers a loopback port (set before its card is
/// sent, on its first frame), with short timings.
fn on_loopback(app: &mut EditorApp) {
    let team = app.team.as_mut().unwrap();
    team.card_address = Some(format!("127.0.0.1:{}", free_port()));
    team.timing = Timing {
        grace: Duration::from_millis(300),
        per_successor: Duration::from_secs(10),
    };
    team.recovery_wait = Duration::from_secs(2);
}

/// Paul hosts hello-part; Sam and Kim join, each offering to host on a
/// loopback port, with short timings.
fn session(name: &str) -> (Box<EditorApp>, Box<EditorApp>, Box<EditorApp>, Vec<PathBuf>) {
    let folders: Vec<PathBuf> = ["paul", "sam", "kim"]
        .iter()
        .map(|who| data_folder(&format!("{name}-{who}")))
        .collect();
    let project = copy_of_example("hello-part", &format!("handoff-{name}"));
    let mut paul = Box::new(EditorApp::new(Some(&project)));
    let invite = paul
        .host_network_session(Some(&folders[0]), 0)
        .expect("hosting");
    let mut sam = Box::new(EditorApp::new(None));
    sam.join_network_session(&loopback(&invite), Some(&folders[1]))
        .expect("Sam joins");
    on_loopback(&mut sam);
    // Sam is in (files and all) before Kim: connected longest, he's first
    // on the list.
    settle(&mut [&mut *paul, &mut *sam], "Sam in", |apps| {
        joined(apps[1]) && apps[1].project.is_some()
    });
    let invite = loopback(
        &paul
            .make_team_invite(Role::Editor)
            .map(|link| Invite::parse(&link).unwrap())
            .unwrap(),
    );
    let mut kim = Box::new(EditorApp::new(None));
    kim.join_network_session(&invite, Some(&folders[2]))
        .expect("Kim joins");
    on_loopback(&mut kim);
    settle(
        &mut [&mut *paul, &mut *sam, &mut *kim],
        "everyone in",
        |apps| apps.iter().all(|a| joined(a)) && apps.iter().all(|a| successors(a) == 2),
    );
    (paul, sam, kim, folders)
}

/// Scenario A: Paul leaves; Sam takes over and nobody's work is lost.
#[test]
fn when_the_host_leaves_the_next_member_hosts_and_everyone_carries_on() {
    let (mut paul, mut sam, mut kim, folders) = session("leave");
    add_part(&mut kim, "BeforeLeaving");
    settle(
        &mut [&mut *paul, &mut *sam, &mut *kim],
        "Kim's part",
        |apps| has_part(apps[0], "BeforeLeaving") && has_part(apps[1], "BeforeLeaving"),
    );

    paul.leave_team_session();
    assert!(paul.team.is_none());
    assert!(
        paul.output
            .lines()
            .iter()
            .any(|line| line.text.contains("is hosting the session now")),
        "Paul is told who hosts"
    );
    add_part(&mut kim, "DuringHandoff");
    settle(
        &mut [&mut *sam, &mut *kim],
        "Sam hosting, Kim with him",
        |apps| hosting(apps[0]) && joined(apps[0]) && joined(apps[1]),
    );
    add_part(&mut sam, "AfterSam");
    settle(&mut [&mut *sam, &mut *kim], "everyone's parts", |apps| {
        apps.iter().all(|a| {
            ["BeforeLeaving", "DuringHandoff", "AfterSam"]
                .iter()
                .all(|name| has_part(a, name))
        })
    });
    assert_eq!(
        kim.team.as_ref().unwrap().client().unwrap().role(),
        Role::Editor
    );
    sam.leave_team_session();
    kim.leave_team_session();
    for folder in folders {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// Scenario B: Paul's computer goes off. After the grace period Sam (first
/// on the list) hosts from his copy, and Kim finds him.
#[test]
fn when_the_host_is_lost_the_first_successor_takes_over_after_the_grace_period() {
    let (mut paul, mut sam, mut kim, folders) = session("lost");
    add_part(&mut sam, "BeforeCrash");
    settle(
        &mut [&mut *paul, &mut *sam, &mut *kim],
        "Sam's part",
        |apps| has_part(apps[2], "BeforeCrash"),
    );

    // The host stops without a word, as a crash would.
    let mut team = paul.team.take().unwrap();
    if let super::team::Link::Net { hosting, .. } = &mut team.link {
        hosting.take().unwrap().stop_without_a_word();
    }
    drop(team);
    add_part(&mut kim, "WhileLost");
    settle(
        &mut [&mut *sam, &mut *kim],
        "Sam hosting, Kim with him",
        |apps| hosting(apps[0]) && joined(apps[0]) && joined(apps[1]),
    );
    settle(&mut [&mut *sam, &mut *kim], "Kim's part at Sam's", |apps| {
        has_part(apps[0], "WhileLost") && has_part(apps[1], "BeforeCrash")
    });
    add_part(&mut sam, "AfterRecovery");
    settle(&mut [&mut *sam, &mut *kim], "Sam's new part", |apps| {
        has_part(apps[1], "AfterRecovery")
    });
    sam.leave_team_session();
    kim.leave_team_session();
    for folder in folders {
        let _ = std::fs::remove_dir_all(folder);
    }
}
