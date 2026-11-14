//! Multiplayer readiness MP2: a player who lost their connection rejoins
//! with the key the host gave them, in place of the stale connection.

use std::time::Duration;

use crate::handshake::{GameInfo, Peer};
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::transport::Server;

const FRAME: Duration = Duration::from_millis(16);

fn game() -> GameInfo {
    GameInfo {
        name: "Castle".to_owned(),
        version: "1.0".to_owned(),
    }
}

/// A dedicated host for at most `max_players`.
fn host(max_players: usize) -> Host {
    Host::new(
        Server::in_memory(),
        HostSettings {
            game: game(),
            max_players,
            password: None,
            own_player: None,
        },
    )
}

/// Steps the host and every join for a few frames; returns what each saw.
fn frames(host: &mut Host, joins: &mut [&mut Join]) -> (Vec<SessionEvent>, Vec<Vec<SessionEvent>>) {
    let mut on_host = Vec::new();
    let mut on_joins = vec![Vec::new(); joins.len()];
    for _ in 0..10 {
        for (join, seen) in joins.iter_mut().zip(&mut on_joins) {
            join.flush();
            seen.extend(join.update(FRAME));
        }
        on_host.extend(host.update(FRAME));
        host.flush();
    }
    (on_host, on_joins)
}

fn welcomed(events: &[SessionEvent]) -> Option<&Peer> {
    events.iter().find_map(|event| match event {
        SessionEvent::Welcomed(peer) => Some(peer),
        _ => None,
    })
}

#[test]
fn a_player_who_rejoins_with_their_key_replaces_their_stale_connection() {
    // Room for one: the stale connection must not count, or Ann is refused.
    let mut host = host(1);
    let mut first = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    let (_, seen) = frames(&mut host, &mut [&mut first]);
    let was = welcomed(&seen[0]).expect("Ann joins").clone();
    let key = first.rejoin_key();
    assert_ne!(key, 0, "the Welcome carries a rejoin key");

    // Her device lost the connection; the host hasn't noticed yet.
    let mut again = Join::start(host_client(&mut host), &game(), "Ann", None, 42).rejoining(key);
    let (on_host, seen) = frames(&mut host, &mut [&mut first, &mut again]);
    let now = welcomed(&seen[1]).expect("Ann rejoins").clone();
    // The same name (not "Ann2"), a new player id, and a new key.
    assert_eq!(now.name, "Ann");
    assert_ne!(now.id, was.id);
    assert_ne!(again.rejoin_key(), key);
    assert!(on_host.contains(&SessionEvent::Left(was.id)), "{on_host:?}");
    assert!(
        on_host.contains(&SessionEvent::Joined(now.clone())),
        "{on_host:?}"
    );
    assert_eq!(host.players(), vec![now]);
    // The old connection is closed: had it still been there, it would
    // have heard it was lost (the device rejoins).
    assert!(
        seen[0]
            .iter()
            .any(|event| matches!(event, SessionEvent::Lost(_))),
        "{:?}",
        seen[0]
    );
}

#[test]
fn a_wrong_rejoin_key_is_a_new_player_and_counts_against_the_limit() {
    let mut host = host(1);
    let mut first = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    frames(&mut host, &mut [&mut first]);
    let key = first.rejoin_key();
    let mut guess = Join::start(host_client(&mut host), &game(), "Ann", None, 42)
        .rejoining(key.wrapping_add(1));
    let (_, seen) = frames(&mut host, &mut [&mut first, &mut guess]);
    assert!(
        seen[1]
            .iter()
            .any(|event| matches!(event, SessionEvent::Failed(reason) if reason.contains("full"))),
        "{:?}",
        seen[1]
    );
    // Ann is still in, and still connected.
    assert_eq!(host.players().len(), 1);
    assert!(first.you().is_some());
    assert!(
        !seen[0]
            .iter()
            .any(|event| matches!(event, SessionEvent::Lost(_)))
    );
}

#[test]
fn losing_the_connection_after_joining_is_lost_not_failed() {
    let mut host = host(4);
    let mut join = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    frames(&mut host, &mut [&mut join]);
    assert!(join.you().is_some());
    host.shut_down();
    let (_, seen) = frames(&mut host, &mut [&mut join]);
    assert!(
        matches!(seen[0].as_slice(), [SessionEvent::Lost(_)]),
        "{:?}",
        seen[0]
    );

    // Before the Welcome, it's a failed join.
    let mut host = self::host(4);
    let mut join = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    host.shut_down();
    let (_, seen) = frames(&mut host, &mut [&mut join]);
    assert!(
        seen[0]
            .iter()
            .any(|event| matches!(event, SessionEvent::Failed(_))),
        "{:?}",
        seen[0]
    );
}

fn host_client(host: &mut Host) -> crate::transport::Client {
    host.server_mut().connect_loopback()
}
