//! Collaborate projects that outlive their sessions (Collaborate 2.0, P3):
//! one project ID wherever it's hosted, each computer's record of it, and
//! rejoining or hosting for the members without an invite.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axcenity_collab::Role;
use axcenity_collab::access::Invite;
use axcenity_collab::project::{HostCard, ProjectRecord, SessionState};
use axcenity_core::PartType;

use super::tests::copy_of_example;
use super::*;
use crate::properties::Edit;

/// One computer's Collaborate data folder.
fn data_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-collab-project-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

/// Frames for every editor until `done`, or fails after 30 s.
fn settle(apps: &mut [&mut EditorApp], what: &str, done: impl Fn(&[&mut EditorApp]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
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

fn role(app: &EditorApp) -> Option<Role> {
    app.team.as_ref().and_then(|t| t.client()).map(|c| c.role())
}

fn session(app: &EditorApp) -> Option<String> {
    let client = app.team.as_ref()?.client()?;
    client.session().map(str::to_owned)
}

/// The invite, naming the loopback address (the host names its LAN one).
fn loopback(invite: Invite) -> Invite {
    let port = invite.host.rsplit(':').next().unwrap().to_owned();
    Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite
    }
}

fn loopback_address(address: &str) -> String {
    format!("127.0.0.1:{}", address.rsplit(':').next().unwrap())
}

fn folder_of(app: &EditorApp) -> PathBuf {
    app.project.as_ref().unwrap().folder.clone()
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

fn has_part(app: &EditorApp, name: &str) -> bool {
    let world = &app.world;
    world
        .descendants(world.root())
        .into_iter()
        .any(|id| world.get(id).is_some_and(|i| i.name() == name))
}

fn record(data: &Path, id: &str) -> ProjectRecord {
    ProjectRecord::load(data, id).expect("a record of the project")
}

#[test]
fn a_project_keeps_its_members_and_id_across_hosts_and_members_rejoin_without_an_invite() {
    let (data_a, data_b) = (data_folder("paul"), data_folder("sam"));
    let project = copy_of_example("hello-part", "collab-project-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = a.host_network_session(Some(&data_a), 0).expect("hosting");
    // The project's ID is written into collaborate.json, and is the
    // session's.
    let file = axcenity_project::load_collaborate_file(&project).unwrap();
    let id = file.project.expect("a project ID");
    let hosted = record(&data_a, &id);
    assert_eq!(hosted.folder, project);
    assert_eq!(hosted.state, SessionState::Active);
    assert_eq!(hosted.members.as_ref().unwrap().members.len(), 1);

    let mut b = EditorApp::new(None);
    b.join_network_session(&loopback(invite), Some(&data_b))
        .expect("joining");
    settle(&mut [&mut a, &mut b], "Sam's copy", |apps| {
        joined(apps[0])
            && joined(apps[1])
            && apps[1]
                .project
                .as_ref()
                .is_some_and(|p| p.folder.ends_with(&id))
    });
    assert_eq!(session(&b).as_deref(), Some(id.as_str()));
    // Sam's computer keeps the project: its folder, the members (Sam was
    // sent them as an Editor), and where it's hosted.
    settle(&mut [&mut a, &mut b], "Sam's record", |_| {
        ProjectRecord::load(&data_b, &id)
            .and_then(|r| r.members)
            .is_some_and(|m| m.members.len() == 2)
    });
    let kept = record(&data_b, &id);
    assert_eq!(kept.folder, folder_of(&b));
    assert_eq!(kept.state, SessionState::Active);
    assert_eq!(
        kept.members.as_ref().unwrap().owner().unwrap().role,
        Role::Owner
    );
    let host = &kept.hosts[0];
    assert!(host.address.starts_with("127.0.0.1:"), "{host:?}");

    // Sam leaves, then comes back from the record: no invite.
    b.leave_team_session();
    assert_eq!(record(&data_b, &id).state, SessionState::Offline);
    b.rejoin_project(&record(&data_b, &id), Some(&data_b))
        .expect("rejoining");
    settle(&mut [&mut a, &mut b], "Sam back", |apps| {
        joined(apps[0]) && joined(apps[1])
    });
    assert_eq!(role(&b), Some(Role::Editor));
    add_part(&mut b, "FromSam");
    settle(&mut [&mut a, &mut b], "Sam's part", |apps| {
        has_part(apps[0], "FromSam")
    });
    assert!(b.save(), "Sam's copy saves");

    // Paul ends the session. Sam's copy can't invite anyone (only the
    // Owner does), but it can host for the members, under the same ID.
    a.leave_team_session();
    b.leave_team_session();
    let refused = b.host_network_session(Some(&data_b), 0).err().unwrap();
    assert!(refused.contains("Owner"), "{refused}");
    let address = b.host_for_members(Some(&data_b), 0).expect("Sam hosts");
    let sam_hosts = record(&data_b, &id);
    assert_eq!(sam_hosts.hosts[0].address, address);
    let sam_cert = sam_hosts.hosts[0].cert;

    // Paul rejoins Sam's session by key, still the Owner, and gets Sam's
    // copy, with the part Sam made.
    let mut paul = record(&data_a, &id);
    paul.hosts.insert(
        0,
        HostCard {
            member: None,
            address: loopback_address(&address),
            cert: sam_cert,
        },
    );
    a.rejoin_project(&paul, Some(&data_a))
        .expect("Paul rejoins");
    settle(&mut [&mut a, &mut b], "Paul in Sam's session", |apps| {
        joined(apps[0]) && joined(apps[1]) && has_part(apps[0], "FromSam")
    });
    assert_eq!(role(&a), Some(Role::Owner));
    assert_eq!(role(&b), Some(Role::Editor));
    assert_eq!(session(&a).as_deref(), Some(id.as_str()));

    a.leave_team_session();
    b.leave_team_session();
    for folder in [&data_a, &data_b, &project] {
        let _ = std::fs::remove_dir_all(folder);
    }
}

#[test]
fn someone_no_longer_a_member_cant_rejoin_or_host() {
    let data = data_folder("removed");
    let project = copy_of_example("hello-part", "collab-project-removed");
    let mut app = EditorApp::new(Some(&project));
    // A record whose members list doesn't have this computer's key.
    let mut kept = ProjectRecord::new("0123456789abcdef", "Castle", &project);
    let other = axcenity_collab::identity::Identity::new("Paul", [1, 2, 3]);
    kept.members =
        Some(axcenity_collab::access::Access::owned_by(other.public_key(), "Paul").members_list());
    kept.hosts.push(HostCard {
        member: None,
        address: "127.0.0.1:9".into(),
        cert: axcenity_collab::identity::Fingerprint([0; 32]),
    });
    let refused = app.rejoin_project(&kept, Some(&data)).err().unwrap();
    assert!(refused.contains("no longer a member"), "{refused}");
    assert!(app.team.is_none());

    // The same list as this computer's record of the project it opens:
    // no members file here, so the list is used, and it isn't in it.
    std::fs::create_dir_all(&data).unwrap();
    axcenity_project::save_collaborate_file(
        &project,
        &axcenity_project::CollaborateFile {
            project: Some("0123456789abcdef".into()),
            ..Default::default()
        },
    )
    .unwrap();
    kept.save(&data).unwrap();
    let refused = app.host_for_members(Some(&data), 0).err().unwrap();
    assert!(refused.contains("no longer a member"), "{refused}");
    for folder in [&data, &project] {
        let _ = std::fs::remove_dir_all(folder);
    }
}
