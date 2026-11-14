//! Multiplayer readiness MP4: secured servers hand out connect tokens over
//! TLS, players check the certificate, and netcode encrypts the rest.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::rejoin_tests::game;
use crate::secure::{
    FetchError, Recent, Secured, ServerCert, TokenPort, TokenRequest, TokenRules, Trust,
    fetch_token, join_link, split_join_link,
};
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::transport::{Client, Server};

const FRAME: Duration = Duration::from_millis(16);

fn rules(password: Option<&str>) -> TokenRules {
    TokenRules {
        game: game(),
        password: password.map(str::to_owned),
    }
}

fn local(port: u16) -> SocketAddr {
    ([127, 0, 0, 1], port).into()
}

fn request(password: Option<&str>, port: u16) -> TokenRequest {
    TokenRequest::new(&game(), password.map(str::to_owned), local(port))
}

/// A token port on a free port, for `cert`.
fn token_port(cert: ServerCert, password: Option<&str>) -> TokenPort {
    TokenPort::start(0, &Secured::new(cert), rules(password)).expect("a free port")
}

fn refused(result: Result<renet_netcode::ConnectToken, FetchError>) -> String {
    match result {
        Err(FetchError::Refused(reason)) => reason,
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("expected a refusal, got a token"),
    }
}

fn temp_file(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-secure-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder.join("known-servers.json")
}

#[test]
fn the_token_port_checks_the_password_and_the_game_before_a_token() {
    let port = token_port(ServerCert::generate(), Some("swordfish"));
    let p = port.port();
    assert!(fetch_token(local(p), &request(Some("swordfish"), p), &Trust::Any).is_ok());

    let wrong = refused(fetch_token(
        local(p),
        &request(Some("guess"), p),
        &Trust::Any,
    ));
    assert!(wrong.contains("password"), "{wrong}");
    let mut other_version = request(Some("swordfish"), p);
    other_version.version = "2.0".to_owned();
    let version = refused(fetch_token(local(p), &other_version, &Trust::Any));
    assert!(
        version.contains("2.0") || version.contains("1.0"),
        "{version}"
    );
    assert_eq!(port.issued(), 1, "refusals get no token");
    let log = port.take_log();
    assert_eq!(log.len(), 2, "{log:?}");
}

#[test]
fn a_pinned_fingerprint_must_match() {
    let cert = ServerCert::generate();
    let fingerprint = cert.fingerprint();
    let port = token_port(cert, None);
    let p = port.port();
    let right = Trust::Pinned(fingerprint.to_ascii_uppercase());
    assert!(fetch_token(local(p), &request(None, p), &right).is_ok());

    let other = ServerCert::generate().fingerprint();
    let reason = refused(fetch_token(
        local(p),
        &request(None, p),
        &Trust::Pinned(other),
    ));
    assert!(reason.contains("isn't the one you expected"), "{reason}");
    assert_eq!(port.issued(), 1);
}

#[test]
fn a_remembered_server_with_a_new_certificate_is_refused() {
    let known = temp_file("remember");
    let first = token_port(ServerCert::generate(), None);
    let p = first.port();
    let trust = Trust::Remember(known.clone());
    assert!(fetch_token(local(p), &request(None, p), &trust).is_ok());
    assert!(crate::secure::is_known(&known, local(p)));
    // Again: the same certificate is still trusted.
    assert!(fetch_token(local(p), &request(None, p), &trust).is_ok());
    drop(first);

    // Another certificate on the same address: someone else, or a server
    // whose certificate was replaced. The password never goes out.
    let second = TokenPort::start(p, &Secured::new(ServerCert::generate()), rules(None))
        .expect("the same port again");
    let reason = refused(fetch_token(local(p), &request(None, p), &trust));
    assert!(reason.contains("isn't the one you expected"), "{reason}");
    assert_eq!(second.issued(), 0);
    let _ = std::fs::remove_dir_all(known.parent().unwrap());
}

#[test]
fn a_server_certificate_is_kept_in_its_folder() {
    let path = temp_file("cert").with_file_name("server-certificate.json");
    let made = ServerCert::load_or_make(&path).unwrap();
    let again = ServerCert::load_or_make(&path).unwrap();
    assert_eq!(made.fingerprint(), again.fingerprint());
    assert_eq!(made.fingerprint().len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "only its owner may read the key");
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn join_links_carry_the_fingerprint() {
    let link = join_link("203.0.113.5:47800", "ABCDEF");
    assert_eq!(link, "axcenity://join/203.0.113.5:47800#ABCDEF");
    assert_eq!(
        split_join_link(&link),
        ("203.0.113.5:47800".to_owned(), Some("abcdef".to_owned()))
    );
    assert_eq!(
        split_join_link(" 10.0.0.2:4000 "),
        ("10.0.0.2:4000".to_owned(), None)
    );
}

#[test]
fn one_address_gets_a_limited_number_of_tokens_a_minute() {
    let recent = Recent::default();
    let start = Instant::now();
    let stranger: IpAddr = [203, 0, 113, 7].into();
    let granted = (0..40).filter(|_| recent.allow(stranger, start)).count();
    assert_eq!(granted, 30);
    // Another address has its own; a minute later this one has again.
    assert!(recent.allow([203, 0, 113, 8].into(), start));
    assert!(recent.allow(stranger, start + Duration::from_secs(61)));
    // Bots and tests on this computer aren't held to it.
    assert!((0..100).all(|_| recent.allow([127, 0, 0, 1].into(), start)));
}

/// A dedicated host on a free UDP port asking for `password`, secured
/// with `cert` unless it's None.
fn udp_host(cert: Option<ServerCert>, password: Option<&str>) -> Host {
    let server = match cert {
        Some(cert) => {
            Server::listen_secure(0, 4, Secured::new(cert), rules(password)).expect("a free port")
        }
        None => Server::listen(0, 4).expect("a free port"),
    };
    Host::new(
        server,
        HostSettings {
            game: game(),
            max_players: 2,
            password: password.map(str::to_owned),
            own_player: None,
        },
    )
}

fn joining(host: &Host, password: Option<&str>, trust: Trust) -> Join {
    let address = format!("127.0.0.1:{}", host.port().unwrap());
    let owned = password.map(str::to_owned);
    let client = Client::join(
        &address,
        |to| TokenRequest::new(&game(), owned.clone(), to),
        trust,
    )
    .unwrap();
    Join::start(client, &game(), "Ann", password.map(str::to_owned), 1)
}

/// Runs real-time frames until the join is in or over (at most 10 s).
fn run(host: &mut Host, join: &mut Join) -> Vec<SessionEvent> {
    let start = Instant::now();
    let mut seen = Vec::new();
    while join.you().is_none() && !join.is_over() && start.elapsed() < Duration::from_secs(10) {
        join.flush();
        seen.extend(join.update(FRAME));
        host.update(FRAME);
        host.flush();
        std::thread::sleep(FRAME);
    }
    seen
}

#[test]
fn a_player_joins_a_secured_server_encrypted() {
    let cert = ServerCert::generate();
    let fingerprint = cert.fingerprint();
    let mut host = udp_host(Some(cert), None);
    assert_eq!(host.server().fingerprint(), Some(fingerprint.as_str()));
    let mut join = joining(&host, None, Trust::Pinned(fingerprint));
    let seen = run(&mut host, &mut join);
    assert!(join.you().is_some(), "not in: {seen:?}");
    assert!(join.is_secured());
    assert_eq!(host.server().tokens_issued(), 1);
}

#[test]
fn a_player_without_a_token_never_gets_in() {
    let mut host = udp_host(Some(ServerCert::generate()), None);
    let address = format!("127.0.0.1:{}", host.port().unwrap());
    let mut join = Join::start(Client::connect(&address).unwrap(), &game(), "Eve", None, 2);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        join.flush();
        join.update(FRAME);
        let events = host.update(FRAME);
        assert!(events.is_empty(), "{events:?}");
        host.flush();
        std::thread::sleep(FRAME);
    }
    assert!(join.you().is_none());
    assert_eq!(host.players().len(), 0);
}

#[test]
fn a_wrong_password_is_refused_at_the_token_port() {
    let mut host = udp_host(Some(ServerCert::generate()), Some("swordfish"));
    let mut join = joining(&host, Some("guess"), Trust::Any);
    let seen = run(&mut host, &mut join);
    assert!(join.is_over(), "{seen:?}");
    let failed = seen.iter().find_map(|event| match event {
        SessionEvent::Failed(reason) => Some(reason.clone()),
        _ => None,
    });
    assert!(
        failed.is_some_and(|reason| reason.contains("password")),
        "{seen:?}"
    );
    assert_eq!(host.server().tokens_issued(), 0);
}

#[test]
fn an_unsecured_server_is_joined_unsecured_unless_it_was_secured_before() {
    let mut host = udp_host(None, None);
    let mut join = joining(&host, None, Trust::Any);
    let seen = run(&mut host, &mut join);
    assert!(join.you().is_some(), "not in: {seen:?}");
    assert!(!join.is_secured());

    // A player with a fingerprint for it expects a secured server and
    // won't connect unsecured.
    let pinned = Trust::Pinned(ServerCert::generate().fingerprint());
    let mut join = joining(&host, None, pinned);
    let seen = run(&mut host, &mut join);
    assert!(join.is_over(), "{seen:?}");
    assert!(
        seen.iter().any(
            |event| matches!(event, SessionEvent::Failed(reason) if reason.contains("token port"))
        ),
        "{seen:?}"
    );
}
