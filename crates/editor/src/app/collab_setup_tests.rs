//! Collaborate's setup wizard and Start Session dialog (C4): each Tailscale
//! state leads to the right step, setup is timed, the checks find a busy
//! port, every page draws in the dialog, and Start Session hosts with the
//! session's name in the invite.

use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::time::{Duration, Instant};

use axcenity_collab::access::Invite;
use axcenity_collab::tailscale::{State, Tailnet};

use super::collab_setup::{Page, Purpose, Step, Timer, duration_text, host_checks};
use super::tests::copy_of_example;
use super::*;

fn tailnet(dns_name: Option<&str>) -> Tailnet {
    Tailnet {
        machine: "pauls-pc".into(),
        dns_name: dns_name.map(str::to_owned),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(100, 101, 102, 103))],
        name: None,
        login: None,
    }
}

#[test]
fn each_tailscale_state_asks_for_one_plain_step() {
    let states = [
        (State::Missing, "Install Tailscale"),
        (
            State::NotRunning {
                detail: "no daemon".into(),
            },
            "Open Tailscale",
        ),
        (State::NeedsLogin { auth_url: None }, "Sign in to Tailscale"),
        (State::NeedsApproval, "Waiting for approval"),
        (State::Stopped, "Turn Tailscale on"),
        (State::Starting, "Connecting…"),
        (State::Running(tailnet(None)), "Tailscale is connected"),
    ];
    for (state, title) in states {
        let (shown, text, _) = Step::of(&state).guidance();
        assert_eq!(shown, title);
        assert!(!text.is_empty());
        // No networking words the user must know.
        for word in ["port", "IP", "NAT", "firewall"] {
            assert!(!text.contains(word), "{title}: {text}");
        }
    }
    let (_, text, _) = Step::of(&State::Running(tailnet(Some("pauls-pc.tail1.ts.net")))).guidance();
    assert!(text.contains("pauls-pc.tail1.ts.net"));
}

#[test]
fn setup_is_timed_step_by_step_against_five_minutes() {
    let start = Instant::now();
    let mut timer = Timer::start(start);
    timer.at("installing Tailscale", start);
    timer.at("installing Tailscale", start + Duration::from_secs(30));
    // A blink of a step isn't worth naming.
    timer.at("connecting", start + Duration::from_secs(100));
    timer.at("signing in", start + Duration::from_secs(101));
    let summary = timer.summary(start + Duration::from_secs(130));
    assert_eq!(
        summary,
        "Setup took 2 min 10 s (installing Tailscale 1 min 40 s, signing in 29 s)."
    );

    let mut slow = Timer::start(start);
    assert!(
        slow.summary(start + Duration::from_secs(301))
            .contains("over the five-minute goal")
    );
    assert_eq!(duration_text(Duration::from_secs(5)), "5 s");
    assert_eq!(duration_text(Duration::from_secs(65)), "1 min 05 s");
}

#[test]
fn the_checks_name_the_address_and_find_a_busy_port() {
    let busy = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let checks = host_checks(&tailnet(None), port);
    assert_eq!(checks.len(), 2, "no name to look up without MagicDNS");
    assert_eq!(checks[0].outcome, Ok(format!("100.101.102.103:{port}")));
    assert!(checks[1].outcome.is_err() && checks[1].warning);

    drop(busy);
    let checks = host_checks(&tailnet(None), port);
    assert_eq!(checks[1].outcome, Ok("free".to_owned()));
}

#[test]
fn finished_setup_is_kept_between_runs() {
    let mut panel = super::team_panel::TeamPanel::default();
    panel.setup.done = true;
    assert_eq!(panel.to_text(), "editor week ready");
    let mut read = super::team_panel::TeamPanel::default();
    read.read_text(&panel.to_text());
    assert!(read.setup.done);
}

/// Draws the setup windows over an empty editor, and saves them as `name`
/// when `AXCENITY_SNAPSHOTS` names a folder.
fn draw(app: &mut EditorApp, name: &str) {
    let mut draw = |ui: &mut egui::Ui| {
        let ctx = ui.ctx().clone();
        app.collab_setup_windows(&ctx);
    };
    let ctx = theme::test_context();
    for _ in 0..3 {
        let _ = theme::run(&ctx, Default::default(), |ui| draw(ui));
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join(format!("{name}.png")),
            egui::vec2(640.0, 560.0),
            vec![Vec::new(); 4],
            &mut draw,
        );
    }
}

#[test]
fn every_wizard_page_draws_and_follows_tailscale() {
    let mut app = EditorApp::new(Some(&copy_of_example(
        "hello-touched",
        "collab-setup-pages",
    )));
    // Join opens the wizard the first time.
    app.join_team_create();
    assert_eq!(app.team_panel.setup.open, Some(Purpose::Join));
    draw(&mut app, "collaborate-setup-welcome");

    let setup = &mut app.team_panel.setup;
    setup.page = Some(Page::Tailscale);
    for (state, name) in [
        (State::Missing, "install"),
        (State::NeedsLogin { auth_url: None }, "sign-in"),
        (State::Stopped, "turn-on"),
    ] {
        app.team_panel.setup.state = Some(state);
        draw(&mut app, &format!("collaborate-setup-{name}"));
        assert_eq!(app.team_panel.setup.page, Some(Page::Tailscale));
    }

    // Connected moves on to the checks by itself.
    let net = tailnet(Some("pauls-pc.tail1.ts.net"));
    app.team_panel.setup.state = Some(State::Running(net.clone()));
    app.team_panel.setup.checks = Some(host_checks(&tailnet(None), 0));
    draw(&mut app, "collaborate-setup-checks");
    assert_eq!(app.team_panel.setup.page, Some(Page::Check));

    // Signing out goes back a page.
    app.team_panel.setup.state = Some(State::Stopped);
    draw(&mut app, "collaborate-setup-back");
    assert_eq!(app.team_panel.setup.page, Some(Page::Tailscale));

    app.team_panel.setup.state = Some(State::Running(net));
    app.team_panel.setup.page = Some(Page::Ready);
    draw(&mut app, "collaborate-setup-ready");
    assert!(app.team_panel.setup.open.is_some());
}

#[test]
fn start_session_hosts_with_the_name_in_the_invite() {
    let mut app = EditorApp::new(Some(&copy_of_example(
        "hello-touched",
        "collab-setup-start",
    )));
    // Once set up, Start goes straight to the dialog.
    app.team_panel.setup.done = true;
    app.share_with_team_create();
    assert!(app.team_panel.setup.open.is_none());
    let start = app.team_panel.setup.start.as_mut().expect("the dialog");
    assert!(!start.name.is_empty(), "named after the project folder");
    start.name = "Castle Game".into();
    start.port = 0;
    let folder = std::env::temp_dir().join(format!("axcenity-collab-setup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    start.folder = Some(folder.clone());
    draw(&mut app, "collaborate-start-session");

    app.start_session_from_dialog();
    let start = app.team_panel.setup.start.as_ref().unwrap();
    assert_eq!(start.problem, None);
    let (link, _) = start.hosted.clone().expect("hosting");
    let invite = Invite::parse(&link).unwrap();
    assert_eq!(invite.name.as_deref(), Some("Castle Game"));
    assert!(app.in_team_session());
    draw(&mut app, "collaborate-start-session-running");

    app.leave_team_session();
    let _ = std::fs::remove_dir_all(&folder);
}
