//! Who gets into a session this editor hosts (cohesion pass CW7.2–7.3):
//! the settings are kept, a host that can't listen on its address listens
//! everywhere, and the person hosting is asked about a new joiner.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_collab::access::Invite;
use axcenity_collab::handshake::Trust;

use super::collab_trust::host_binds;
use super::team_panel::TeamPanel;
use super::tests::copy_of_example;
use super::*;

#[test]
fn trust_and_listening_are_kept_between_runs() {
    let mut panel = TeamPanel::default();
    assert_eq!(panel.trust, Trust::InviteHolders);
    assert!(!panel.listen_everywhere);
    panel.trust = Trust::AskNewMembers;
    panel.listen_everywhere = true;
    assert_eq!(panel.to_text(), "editor week ask-new listen-all");
    let mut read = TeamPanel::default();
    read.read_text(&panel.to_text());
    assert_eq!(read.trust, Trust::AskNewMembers);
    assert!(read.listen_everywhere);

    panel.trust = Trust::AskEveryone;
    read.read_text(&panel.to_text());
    assert_eq!(read.trust, Trust::AskEveryone);
}

#[test]
fn a_host_that_cant_listen_on_its_address_listens_everywhere() {
    // A free port, then an address this computer doesn't have.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let elsewhere = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5));
    let mut output = crate::output::Output::default();
    let binds = host_binds(port, &[elsewhere], false, &mut output);
    assert_eq!(binds, [SocketAddr::from(([0, 0, 0, 0], port))]);
    let said = output.lines().last().expect("a warning").text.clone();
    assert!(said.contains("listening on every network"), "{said}");

    // Loopback alone is fine, and Listen on every network is asked for.
    let mut output = crate::output::Output::default();
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let binds = host_binds(port, &[loopback], false, &mut output);
    assert_eq!(binds, [SocketAddr::from(([127, 0, 0, 1], port))]);
    assert!(output.lines().is_empty());
    let binds = host_binds(port, &[loopback], true, &mut output);
    assert_eq!(binds, [SocketAddr::from(([0, 0, 0, 0], port))]);
}

fn data_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-collab-trust-{name}-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

/// Frames for both editors until `done`, or fails after 30 s.
fn settle(
    a: &mut EditorApp,
    b: &mut EditorApp,
    what: &str,
    done: impl Fn(&EditorApp, &EditorApp) -> bool,
) {
    let start = Instant::now();
    while !done(a, b) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
        a.team_frame();
        b.team_frame();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn asking(app: &EditorApp) -> usize {
    app.team
        .as_ref()
        .and_then(|t| t.hosting())
        .map_or(0, |hosting| hosting.lock().join_requests().len())
}

fn joined(app: &EditorApp) -> bool {
    app.team
        .as_ref()
        .and_then(|t| t.client())
        .is_some_and(|c| c.is_online() && c.is_joined())
}

#[test]
fn the_host_is_asked_about_a_new_joiner_and_lets_them_in() {
    let (data_a, data_b) = (data_folder("paul"), data_folder("sam"));
    let mut a = EditorApp::new(Some(&copy_of_example("hello-part", "collab-trust-host")));
    a.team_panel.trust = Trust::AskNewMembers;
    let invite = a.host_network_session(Some(&data_a), 0).expect("hosting");
    let port = invite.host.rsplit(':').next().unwrap().to_owned();
    let invite = Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite
    };

    let mut b = EditorApp::new(None);
    b.join_network_session(&invite, Some(&data_b))
        .expect("joining");
    settle(&mut a, &mut b, "the host to be asked", |a, _| {
        asking(a) == 1
    });
    assert!(!joined(&b), "Sam waits to be let in");

    // The window asks, with who it is and where they're from.
    let ctx = theme::test_context();
    for _ in 0..2 {
        let _ = theme::run(&ctx, Default::default(), |ui| {
            a.join_requests_window(&ui.ctx().clone());
        });
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join("collaborate-join-request.png"),
            egui::vec2(480.0, 320.0),
            vec![Vec::new(); 3],
            &mut |ui: &mut egui::Ui| a.join_requests_window(&ui.ctx().clone()),
        );
    }

    let id = {
        let hosting = a.team.as_ref().unwrap().hosting().unwrap();
        let session = hosting.lock();
        let request = &session.join_requests()[0];
        assert!(!request.asking.member);
        assert!(request.address.is_loopback());
        request.id
    };
    a.team
        .as_ref()
        .unwrap()
        .hosting()
        .unwrap()
        .lock()
        .answer_join(id, true);
    settle(&mut a, &mut b, "Sam let in", |_, b| joined(b));
    assert_eq!(asking(&a), 0);

    b.leave_team_session();
    a.leave_team_session();
    let _ = std::fs::remove_dir_all(&data_a);
    let _ = std::fs::remove_dir_all(&data_b);
}
