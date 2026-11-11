//! Coming back (TC6): a member's editor or the host started again, edits
//! sent again after a gap, and the host ending the session.

use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::apply::Refusal;
use crate::client::Notice;
use crate::message::ToClient;
use crate::role::Role;
use crate::testing::*;

/// A world with Parts "A" and "B" in the Workspace.
fn sample() -> (World, InstanceId, InstanceId) {
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
    (world, a, b)
}

fn two_editors() -> (Net, InstanceId, InstanceId) {
    let (world, a, b) = sample();
    (Net::new(world, &[Role::Owner, Role::Editor]), a, b)
}

fn name(net: &Net, id: InstanceId) -> String {
    net.host.world().get(id).unwrap().name().to_owned()
}

/// Member `i` adds a Part named `label` to the Workspace.
fn add_part(net: &mut Net, i: usize, label: &str) -> InstanceId {
    let id = net.new_id(i);
    let workspace = net.worlds[i].workspace();
    net.submit(i, label, vec![create(vec![part(id, workspace, label)])])
        .unwrap();
    id
}

fn refusals(notices: &[Notice]) -> Vec<(&str, bool)> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Refused {
                label, recovered, ..
            } => Some((label.as_str(), *recovered)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_member_whose_editor_started_again_is_heard() {
    let (mut net, a, _) = two_editors();
    for n in 0..3 {
        net.submit(1, "Rename A", vec![rename(a, &format!("A{n}"))])
            .unwrap();
    }
    net.settle();
    // The host has handled their transactions 1 to 3; the new run of the
    // editor counts from 1 again.
    net.restart_editor(1);
    net.connect(1);
    net.settle();
    net.submit(1, "Rename A", vec![rename(a, "Door")]).unwrap();
    net.settle();
    assert_eq!(name(&net, a), "Door");
    net.assert_converged();
}

#[test]
fn a_copy_from_before_the_host_started_again_gets_a_snapshot() {
    let (mut net, a, b) = two_editors();
    net.submit(0, "Rename A", vec![rename(a, "One")]).unwrap();
    net.settle();
    net.disconnect(1);
    net.submit(0, "Rename A", vec![rename(a, "Two")]).unwrap();
    net.settle();
    // Member 1's copy is at transaction 1 of the old run; the new run
    // numbers its own history from 0, and soon passes 1.
    net.restart_host();
    net.connect(0);
    net.settle();
    for n in 0..3 {
        net.submit(0, "Rename B", vec![rename(b, &format!("B{n}"))])
            .unwrap();
    }
    net.settle();
    let snapshots = net.snapshots;
    net.connect(1);
    net.settle();
    assert_eq!(net.snapshots, snapshots + 1, "not caught up from the log");
    net.assert_converged();
}

#[test]
fn edits_made_while_the_host_started_again_reach_the_new_host() {
    let (mut net, a, _) = two_editors();
    net.disconnect(1);
    // Made with an ID from a block of the old run of the host, then
    // pointed at by another edit.
    let lamp = add_part(&mut net, 1, "Lamp");
    net.submit(
        1,
        "Light",
        vec![set(lamp, "Transparency", Value::Number(0.5))],
    )
    .unwrap();
    net.submit(
        1,
        "Link",
        vec![set(a, "Name", Value::String("Switch".into()))],
    )
    .unwrap();
    net.restart_host();
    net.connect(0);
    net.connect(1);
    net.settle();
    let notices = net.clients[1].take_notices();
    assert_eq!(refusals(&notices), [], "{notices:?}");
    let lamp = find(&net, 1, "Lamp").expect("the Lamp is in member 1's copy");
    let host_lamp = net.host.world().get(lamp).expect("and in the host's");
    assert_eq!(
        host_lamp.property("Transparency"),
        Some(&Value::Number(0.5))
    );
    assert_eq!(name(&net, a), "Switch");
    // Both make more; their new blocks don't overlap.
    add_part(&mut net, 0, "Chair");
    add_part(&mut net, 1, "Table");
    net.settle();
    net.assert_converged();
}

#[test]
fn an_edit_sent_again_that_no_longer_applies_is_reported_as_recovered() {
    let (mut net, a, b) = two_editors();
    net.disconnect(1);
    net.submit(1, "Move A into B", vec![move_to(a, b)]).unwrap();
    net.submit(0, "Move B into A", vec![move_to(b, a)]).unwrap();
    net.settle();
    net.connect(1);
    net.settle();
    let notices = net.clients[1].take_notices();
    assert_eq!(refusals(&notices), [("Move A into B", true)], "{notices:?}");
    net.assert_converged();
    // A refusal of an edit made while connected isn't from a gap.
    let (mut net, a, b) = two_editors();
    net.submit(0, "Move A into B", vec![move_to(a, b)]).unwrap();
    net.submit(1, "Move B into A", vec![move_to(b, a)]).unwrap();
    net.deliver_up(0);
    net.deliver_up(1);
    net.settle();
    let notices = net.clients[1].take_notices();
    assert!(
        refusals(&notices).iter().all(|(_, recovered)| !recovered),
        "{notices:?}"
    );
    net.assert_converged();
}

#[test]
fn when_the_host_ends_the_session_copies_stay_read_only() {
    let (mut net, a, _) = two_editors();
    net.disconnect(1);
    net.submit(1, "Rename A", vec![rename(a, "Door")]).unwrap();
    net.connect(1);
    net.deliver_up(1);
    // The host ends before member 1's edit arrives.
    let out = net.host.end();
    assert!(out.iter().all(|o| matches!(o.message, ToClient::Ended)));
    net.route_out(out);
    while net.deliver_down(1) {}
    let client = &mut net.clients[1];
    assert!(client.host_left());
    assert!(client.take_notices().contains(&Notice::HostLeft));
    // What wasn't confirmed is kept for the recovery file.
    assert_eq!(client.unsent().len(), 1);
    // Edits are taken back.
    let world = &mut net.worlds[1];
    let workspace = world.workspace();
    let made = world.create(ClassName::Part).unwrap();
    world.set_parent(made, Some(workspace)).unwrap();
    assert_eq!(net.capture(1, "Add Part"), None);
    assert!(net.worlds[1].get(made).is_none_or(|i| i.parent().is_none()));
    let notices = net.clients[1].take_notices();
    assert!(
        matches!(
            &notices[..],
            [Notice::Refused {
                reason: Refusal::Invalid(_),
                ..
            }]
        ),
        "{notices:?}"
    );
    // It doesn't try to join again.
    net.clients[1].connect();
    assert!(net.clients[1].take_outbox().is_empty());
}
