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
