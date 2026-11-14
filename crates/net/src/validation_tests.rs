//! Multiplayer readiness MP3: what the host does with what a modified
//! client sends, at the session level.

use crate::handshake::Message;
use crate::rejoin_tests::{frames, game, host, host_client};
use crate::session::{Join, NOT_A_PLAYERS, SessionEvent, UNREADABLE};
use crate::transport::Channel;
use crate::wire_tests::nested_tables;

fn flags(events: &[SessionEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Flagged { reason, .. } => Some(*reason),
            _ => None,
        })
        .collect()
}

#[test]
fn tables_nested_200_000_deep_are_flagged_as_unreadable_and_the_host_carries_on() {
    let mut host = host(4);
    let mut join = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    frames(&mut host, &mut [&mut join]);
    assert!(join.you().is_some());
    // 600 KB, which overflowed the stack before MP3 (a million deep is
    // 3 MB, over the bytes budget, so dropped unread).
    join.send_bytes(Channel::ReliableOrdered, nested_tables(200_000));
    join.send(&Message::Chat {
        text: "still here".to_owned(),
    });
    // At renet's 60 KB a frame.
    let mut on_host = Vec::new();
    for _ in 0..20 {
        on_host.extend(frames(&mut host, &mut [&mut join]).0);
    }
    assert_eq!(flags(&on_host), [UNREADABLE]);
    assert!(
        on_host
            .iter()
            .any(|event| matches!(event, SessionEvent::Chat { text, .. } if text == "still here")),
        "{on_host:?}"
    );
}

#[test]
fn chat_past_its_budget_and_messages_only_a_server_sends_are_dropped_and_flagged_once_a_second() {
    let mut host = host(4);
    let mut join = Join::start(host_client(&mut host), &game(), "Ann", None, 42);
    frames(&mut host, &mut [&mut join]);
    for n in 0..20 {
        join.send(&Message::Chat {
            text: format!("hi {n}"),
        });
        join.send(&Message::Left { id: 7 });
    }
    let (on_host, _) = frames(&mut host, &mut [&mut join]);
    let chats = on_host
        .iter()
        .filter(|event| matches!(event, SessionEvent::Chat { .. }))
        .count();
    let burst = crate::limits::budget(crate::limits::Kind::Chat).burst as usize;
    assert_eq!(chats, burst);
    // Twelve chats and twenty Lefts dropped: one flag for each reason.
    let mut flagged = flags(&on_host);
    flagged.sort();
    let mut expected = vec![crate::limits::Kind::Chat.reason(), NOT_A_PLAYERS];
    expected.sort();
    assert_eq!(flagged, expected);
}
