//! The roster and presence (TC4): who's in the session, and what each
//! member has selected and is looking at.

use axcenity_core::{ClassName, InstanceId, World};

use crate::client::Notice;
use crate::message::MemberId;
use crate::presence::{Camera, Presence, Profile, SELECTION_SHOWN};
use crate::role::Role;
use crate::testing::*;

/// Parts "A" and "B" in the Workspace, with an owner, an editor and a
/// viewer joined.
fn session() -> (Net, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let mut make = |name: &str| {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    };
    let a = make("A");
    let b = make("B");
    let net = Net::new(world, &[Role::Owner, Role::Editor, Role::Viewer]);
    (net, a, b)
}

fn selecting(ids: &[InstanceId]) -> Presence {
    Presence {
        selection: ids.to_vec(),
        ..Presence::default()
    }
}

fn notices(net: &mut Net, i: usize) -> Vec<Notice> {
    net.clients[i].take_notices()
}

#[test]
fn everyone_sees_who_is_in_the_session_with_their_role() {
    let (mut net, _, _) = session();
    let out = net.host.set_profile(
        member(1),
        Profile {
            name: "Sam".into(),
            color: [10, 200, 30],
        },
    );
    net.route_out(out);
    net.settle();
    for i in 0..3 {
        let roster = net.clients[i].roster();
        assert_eq!(roster.len(), 3, "client {i}");
        assert!(roster.iter().all(|c| c.online));
        let sam = roster.iter().find(|c| c.member == member(1)).unwrap();
        assert_eq!(sam.profile.name, "Sam");
        assert_eq!(sam.profile.color, [10, 200, 30]);
        assert_eq!(sam.role, Role::Editor);
    }
    let roles: Vec<Role> = net.clients[0].roster().iter().map(|c| c.role).collect();
    assert_eq!(roles, [Role::Owner, Role::Editor, Role::Viewer]);
}

#[test]
fn leaving_and_coming_back_are_told_to_the_others() {
    let (mut net, _, _) = session();
    net.disconnect(2);
    net.settle();
    assert_eq!(
        notices(&mut net, 0),
        [Notice::Left {
            member: member(2),
            name: "Member 3".into()
        }]
    );
    let viewer = net.clients[1]
        .roster()
        .iter()
        .find(|c| c.member == member(2))
        .unwrap()
        .clone();
    assert!(!viewer.online, "still listed, offline");
    notices(&mut net, 1);
    net.connect(2);
    net.settle();
    assert_eq!(
        notices(&mut net, 1),
        [Notice::Arrived {
            member: member(2),
            name: "Member 3".into()
        }]
    );
    // The one who came back isn't told about everyone already there.
    assert!(
        !notices(&mut net, 2)
            .iter()
            .any(|n| matches!(n, Notice::Arrived { .. }))
    );
}

#[test]
fn a_selection_reaches_the_others_and_late_joiners() {
    let (mut net, a, b) = session();
    let looking = Presence {
        selection: vec![a, b],
        camera: Some(Camera {
            position: [1.0, 2.0, 3.0],
            look: [0.0, 0.0, -1.0],
        }),
        editing: Some("Transparency".into()),
        script: None,
    };
    net.set_presence(0, looking.clone());
    net.settle();
    for i in [1, 2] {
        assert_eq!(net.clients[i].presences().get(&member(0)), Some(&looking));
    }
    assert!(net.clients[0].presences().is_empty(), "not their own");
    assert_eq!(net.host.presence(member(0)), Some(&looking));

    // Someone who reconnects is told at once.
    net.disconnect(2);
    net.settle();
    assert!(net.clients[2].presences().is_empty());
    net.connect(2);
    net.settle();
    assert_eq!(net.clients[2].presences().get(&member(0)), Some(&looking));

    // A member who leaves takes their presence with them.
    net.disconnect(0);
    net.settle();
    assert!(net.clients[1].presences().get(&member(0)).is_none());
}

#[test]
fn a_presence_set_while_offline_is_sent_on_reconnecting() {
    let (mut net, a, _) = session();
    net.disconnect(1);
    net.set_presence(1, selecting(&[a]));
    net.connect(1);
    net.settle();
    assert_eq!(
        net.clients[0].presences().get(&member(1)),
        Some(&selecting(&[a]))
    );
}

#[test]
fn an_unchanged_presence_isnt_sent_again() {
    let (mut net, a, _) = session();
    net.set_presence(0, selecting(&[a]));
    assert_eq!(net.waiting_up(0), 1);
    net.set_presence(0, selecting(&[a]));
    assert_eq!(net.waiting_up(0), 1);
}

#[test]
fn a_huge_selection_is_cut_and_local_instances_are_left_out() {
    let (mut net, a, _) = session();
    let local = net
        .world_mut(0)
        .unrecorded(|w| w.create(ClassName::Part).unwrap());
    assert!(local.is_local());
    let mut many = vec![local, a];
    many.extend(std::iter::repeat_n(a, SELECTION_SHOWN * 2));
    net.set_presence(0, selecting(&many));
    net.settle();
    let seen = &net.clients[1].presences()[&member(0)];
    assert!(seen.selection.len() < SELECTION_SHOWN);
    assert!(!seen.selection.contains(&local));
    assert_eq!(seen.selection[0], a);
}

#[test]
fn a_role_change_updates_the_roster_for_everyone() {
    let (mut net, _, _) = session();
    net.set_role(2, Role::Editor);
    net.settle();
    for i in 0..3 {
        let viewer = net.clients[i]
            .roster()
            .iter()
            .find(|c| c.member == member(2))
            .unwrap();
        assert_eq!(viewer.role, Role::Editor);
    }
}

#[test]
fn a_removed_member_leaves_the_roster_and_cant_edit() {
    let (mut net, _, _) = session();
    let out = net.host.remove_member(member(1));
    net.route_out(out);
    net.settle();
    assert!(
        net.clients[0]
            .roster()
            .iter()
            .all(|c| c.member != MemberId(2))
    );
    let id = net.new_id(1);
    let _ = net.submit(
        1,
        "Insert",
        vec![create(vec![part(id, net.host.world().workspace(), "X")])],
    );
    net.settle();
    assert!(net.host.world().get(id).is_none());
}

// Chat (C8).

fn chat_texts(net: &Net, i: usize) -> Vec<(u32, String)> {
    net.clients[i]
        .chat()
        .iter()
        .map(|line| (line.member.0, line.text.clone()))
        .collect()
}

#[test]
fn chat_reaches_everyone_in_the_hosts_order() {
    let (mut net, _, _) = session();
    net.with_client(1, |client, _| client.send_chat("hello").unwrap());
    net.with_client(0, |client, _| client.send_chat("hi there").unwrap());
    // Viewers can talk too.
    net.with_client(2, |client, _| client.send_chat("  can I look?  ").unwrap());
    net.settle();
    let first = chat_texts(&net, 0);
    assert_eq!(first.len(), 3);
    assert!(first.contains(&(3, "can I look?".to_owned())));
    for i in 1..3 {
        assert_eq!(chat_texts(&net, i), first, "member {i}");
        // Unread: the two lines the others sent.
        assert_eq!(net.clients[i].take_new_chat(), 2);
    }
}

#[test]
fn someone_joining_gets_the_last_two_hundred_lines() {
    use crate::presence::CHAT_KEPT;
    let (mut net, _, _) = session();
    let start = std::time::Instant::now();
    for n in 0..CHAT_KEPT + 5 {
        net.tick(start + std::time::Duration::from_secs(n as u64));
        let out = net
            .host
            .receive(member(0), crate::message::ToHost::Chat(format!("line {n}")));
        net.route_out(out);
    }
    net.settle();
    net.restart_editor(2);
    net.connect(2);
    net.settle();
    let joined = chat_texts(&net, 2);
    assert_eq!(joined.len(), CHAT_KEPT);
    assert_eq!(joined[0].1, "line 5");
    assert_eq!(joined.last().unwrap().1, format!("line {}", CHAT_KEPT + 4));
}

#[test]
fn chat_is_plain_short_and_not_too_fast() {
    use crate::message::ToHost;
    use crate::presence::{CHAT_MOST, CHAT_PER_SECOND};
    let (mut net, _, _) = session();
    // Control characters go; line breaks stay; long lines are cut.
    let out = net
        .host
        .receive(member(1), ToHost::Chat("a\u{1b}[31mb\nc\u{0}".into()));
    net.route_out(out);
    let out = net
        .host
        .receive(member(1), ToHost::Chat("x".repeat(CHAT_MOST * 2)));
    net.route_out(out);
    // Blank lines aren't sent at all.
    let out = net.host.receive(member(1), ToHost::Chat(" \n\t ".into()));
    assert!(out.is_empty());
    net.settle();
    let lines = chat_texts(&net, 0);
    assert_eq!(lines[0].1, "a[31mb\nc");
    assert_eq!(lines[1].1.chars().count(), CHAT_MOST);

    // More than five a second: the host drops the rest, and the client
    // says so before sending.
    for n in 0..CHAT_PER_SECOND + 3 {
        let out = net
            .host
            .receive(member(2), ToHost::Chat(format!("spam {n}")));
        net.route_out(out);
    }
    net.settle();
    let spam = chat_texts(&net, 0)
        .into_iter()
        .filter(|(_, text)| text.starts_with("spam"))
        .count();
    assert_eq!(spam, CHAT_PER_SECOND);
    for _ in 0..CHAT_PER_SECOND {
        net.with_client(0, |client, _| client.send_chat("quick").unwrap());
    }
    assert!(
        net.with_client(0, |client, _| client.send_chat("too quick"))
            .is_err()
    );
    assert!(
        net.with_client(0, |client, _| client.send_chat("   "))
            .is_err()
    );
}
