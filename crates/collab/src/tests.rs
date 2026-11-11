//! One test per row of the conflict table in `docs/research/team-create.md`
//! (§8), plus reconnection, roles and IDs.

use axcenity_core::{ClassName, InstanceId, Material, Value, World};

use crate::apply::{self, DropReason, Note, Refusal};
use crate::client::Notice;
use crate::host::Host;
use crate::message::{ToClient, ToHost};
use crate::op::Transaction;
use crate::role::Role;
use crate::testing::*;

/// A world with a Folder "Shed" holding a Model "Bench", and Parts "A" and
/// "B", all in the Workspace.
fn sample() -> (World, Ids) {
    let mut world = World::new();
    let workspace = world.workspace();
    let mut make = |class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let shed = make(ClassName::Folder, "Shed", workspace);
    let bench = make(ClassName::Model, "Bench", shed);
    let a = make(ClassName::Part, "A", workspace);
    let b = make(ClassName::Part, "B", workspace);
    (
        world,
        Ids {
            workspace,
            shed,
            bench,
            a,
            b,
        },
    )
}

struct Ids {
    workspace: InstanceId,
    shed: InstanceId,
    bench: InstanceId,
    a: InstanceId,
    b: InstanceId,
}

fn two_editors() -> (Net, Ids) {
    let (world, ids) = sample();
    (Net::new(world, &[Role::Owner, Role::Editor]), ids)
}

fn transparency(net: &Net, i: usize, id: InstanceId) -> Value {
    net.worlds[i]
        .get(id)
        .unwrap()
        .property("Transparency")
        .unwrap()
        .clone()
}

#[test]
fn an_edit_shows_at_once_for_its_author_and_reaches_everyone() {
    let (mut net, ids) = two_editors();
    let id = net.new_id(0);
    net.submit(
        0,
        "Insert Part",
        vec![create(vec![part(id, ids.workspace, "Crate")])],
    )
    .unwrap();
    assert_eq!(find(&net, 0, "Crate"), Some(id), "no waiting for the host");
    assert_eq!(find(&net, 1, "Crate"), None);
    net.settle();
    assert_eq!(find(&net, 1, "Crate"), Some(id), "the same ID everywhere");
    net.assert_converged();
}

#[test]
fn two_writes_to_one_property_end_with_the_later_one_in_host_order() {
    let (mut net, ids) = two_editors();
    net.submit(
        0,
        "Set Transparency",
        vec![set(ids.a, "Transparency", Value::Number(0.25))],
    )
    .unwrap();
    net.submit(
        1,
        "Set Transparency",
        vec![set(ids.a, "Transparency", Value::Number(0.75))],
    )
    .unwrap();
    // The host hears client 1 first, then client 0.
    net.deliver_up(1);
    net.deliver_down(0);
    // Client 0's own value stays on top of client 1's until the host
    // confirms it, so what they see doesn't flicker back.
    assert_eq!(transparency(&net, 0, ids.a), Value::Number(0.25));
    net.deliver_up(0);
    net.settle();
    assert_eq!(transparency(&net, 1, ids.a), Value::Number(0.25));
    net.assert_converged();
}

#[test]
fn writes_to_different_properties_of_one_instance_both_apply() {
    let (mut net, ids) = two_editors();
    net.submit(
        0,
        "Set Transparency",
        vec![set(ids.a, "Transparency", Value::Number(0.5))],
    )
    .unwrap();
    net.submit(1, "Rename", vec![rename(ids.a, "Door")])
        .unwrap();
    net.settle();
    let world = net.host.world();
    assert_eq!(world.get(ids.a).unwrap().name(), "Door");
    assert_eq!(
        world.get(ids.a).unwrap().property("Transparency"),
        Some(&Value::Number(0.5))
    );
    net.assert_converged();
}

#[test]
fn an_edit_of_something_deleted_meanwhile_is_dropped_and_its_author_told() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Delete A", vec![delete(ids.a)]).unwrap();
    net.submit(1, "Rename A", vec![rename(ids.a, "Door")])
        .unwrap();
    net.deliver_up(0);
    net.settle();
    assert!(!net.host.doc().in_tree(ids.a), "delete wins");
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "A");
    let notices = net.clients[1].take_notices();
    assert!(
        matches!(
            &notices[..],
            [Notice::Changed { notes, .. }] if notes == &[Note::Dropped { id: ids.a, reason: DropReason::Deleted }]
        ),
        "{notices:?}"
    );
    net.assert_converged();
}

#[test]
fn a_move_of_something_deleted_meanwhile_is_dropped() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Delete A", vec![delete(ids.a)]).unwrap();
    net.submit(1, "Move A", vec![move_to(ids.a, ids.shed)])
        .unwrap();
    net.deliver_up(0);
    net.settle();
    assert!(!net.host.doc().in_tree(ids.a));
    net.assert_converged();
}

#[test]
fn something_made_inside_a_parent_deleted_meanwhile_goes_to_its_nearest_surviving_ancestor() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Delete Shed", vec![delete(ids.shed)])
        .unwrap();
    let id = net.new_id(1);
    net.submit(
        1,
        "Insert Part",
        vec![create(vec![part(id, ids.bench, "Leg")])],
    )
    .unwrap();
    net.deliver_up(0);
    net.settle();
    let world = net.host.world();
    assert_eq!(
        world.get(id).unwrap().parent(),
        Some(ids.workspace),
        "nothing is lost"
    );
    let notices = net.clients[1].take_notices();
    assert!(
        matches!(&notices[..], [Notice::Changed { notes, .. }] if notes == &[Note::Rehomed { id, to: ids.workspace }]),
        "{notices:?}"
    );
    net.assert_converged();
}

#[test]
fn a_move_into_a_parent_deleted_meanwhile_goes_to_its_nearest_surviving_ancestor() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Delete Bench", vec![delete(ids.bench)])
        .unwrap();
    net.submit(1, "Move A", vec![move_to(ids.a, ids.bench)])
        .unwrap();
    net.deliver_up(0);
    net.settle();
    assert_eq!(
        net.host.world().get(ids.a).unwrap().parent(),
        Some(ids.shed)
    );
    net.assert_converged();
}

#[test]
fn two_moves_that_would_make_a_cycle_refuse_the_second_and_take_it_off() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Move A into B", vec![move_to(ids.a, ids.b)])
        .unwrap();
    net.submit(1, "Move B into A", vec![move_to(ids.b, ids.a)])
        .unwrap();
    net.deliver_up(0);
    net.settle();
    let world = net.host.world();
    assert_eq!(world.get(ids.a).unwrap().parent(), Some(ids.b));
    assert_eq!(world.get(ids.b).unwrap().parent(), Some(ids.workspace));
    let notices = net.clients[1].take_notices();
    assert!(
        matches!(
            &notices[..],
            [Notice::Refused {
                reason: Refusal::Cycle { .. },
                ..
            }]
        ),
        "{notices:?}"
    );
    net.assert_converged();
}

#[test]
fn a_member_can_restore_what_someone_else_deleted() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Delete Shed", vec![delete(ids.shed)])
        .unwrap();
    net.settle();
    net.submit(1, "Restore Shed", vec![restore(ids.shed, ids.workspace)])
        .unwrap();
    net.settle();
    let world = net.host.world();
    assert_eq!(world.get(ids.shed).unwrap().parent(), Some(ids.workspace));
    assert_eq!(
        world.get(ids.bench).unwrap().parent(),
        Some(ids.shed),
        "with what was inside"
    );
    assert!(net.host.doc().graveyard.is_empty());
    net.assert_converged();
}

#[test]
fn a_transaction_applies_whole_or_not_at_all() {
    let (mut net, ids) = two_editors();
    let before = net.host_copy();
    // The second op names a property Parts don't have.
    let refused = net.submit(
        0,
        "Bad edit",
        vec![
            rename(ids.a, "Door"),
            set(ids.a, "Wheels", Value::Number(4.0)),
        ],
    );
    assert!(matches!(refused, Err(Refusal::Invalid(_))));
    assert_eq!(net.copy(0), before, "nothing changed locally");
    // A forged one reaching the host changes nothing there either.
    let out = net.host.receive(
        member(0),
        ToHost::Submit(Transaction {
            client_seq: 99,
            label: "Bad edit".into(),
            ops: vec![
                rename(ids.a, "Door"),
                set(ids.a, "Wheels", Value::Number(4.0)),
            ],
        }),
    );
    assert!(matches!(
        out[0].message,
        ToClient::Rejected {
            reason: Refusal::Invalid(_),
            ..
        }
    ));
    assert_eq!(net.host_copy(), before);
}

#[test]
fn a_transaction_the_host_already_applied_is_not_applied_again_after_a_reconnect() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Rename A", vec![rename(ids.a, "Door")])
        .unwrap();
    net.deliver_up(0);
    // The connection drops before the confirmation arrives.
    net.disconnect(0);
    assert_eq!(net.clients[0].pending(), 1);
    net.connect(0);
    net.settle();
    let mine: Vec<_> = net.host.log().filter(|l| l.author == member(0)).collect();
    assert_eq!(mine.len(), 1, "applied once");
    net.assert_converged();
}

#[test]
fn edits_made_while_offline_are_sent_after_reconnecting() {
    let (mut net, ids) = two_editors();
    net.disconnect(1);
    net.submit(1, "Rename A", vec![rename(ids.a, "Door")])
        .unwrap();
    net.submit(
        0,
        "Set Transparency",
        vec![set(ids.a, "Transparency", Value::Number(0.5))],
    )
    .unwrap();
    net.settle();
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "A");
    net.connect(1);
    net.settle();
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "Door");
    net.assert_converged();
}

#[test]
fn a_client_too_far_behind_gets_a_snapshot_and_keeps_its_unsent_edits() {
    let (world, ids) = sample();
    let mut net = Net::with_host(
        Host::new(world).with_log_kept(2),
        &[Role::Owner, Role::Editor],
    );
    net.disconnect(1);
    for n in 0..5 {
        net.submit(
            0,
            "Set Transparency",
            vec![set(ids.a, "Transparency", Value::Number(n as f64 / 10.0))],
        )
        .unwrap();
    }
    net.settle();
    net.submit(1, "Rename B", vec![rename(ids.b, "Wall")])
        .unwrap();
    net.connect(1);
    net.settle();
    assert_eq!(net.host.world().get(ids.b).unwrap().name(), "Wall");
    net.assert_converged();
}

#[test]
fn an_edit_refused_while_its_author_was_away_is_reported_after_reconnecting() {
    let (mut net, ids) = two_editors();
    net.submit(0, "Move A into B", vec![move_to(ids.a, ids.b)])
        .unwrap();
    net.submit(1, "Move B into A", vec![move_to(ids.b, ids.a)])
        .unwrap();
    net.deliver_up(0);
    net.deliver_up(1);
    // Client 1 drops off before hearing about either.
    net.disconnect(1);
    net.connect(1);
    net.settle();
    let notices = net.clients[1].take_notices();
    assert!(
        matches!(
            &notices[..],
            [Notice::Refused {
                reason: Refusal::Cycle { .. },
                ..
            }]
        ),
        "{notices:?}"
    );
    net.assert_converged();
}

#[test]
fn a_viewer_cannot_change_anything() {
    let (world, ids) = sample();
    let mut net = Net::new(world, &[Role::Owner, Role::Viewer]);
    assert_eq!(
        net.submit(1, "Rename A", vec![rename(ids.a, "Door")]),
        Err(Refusal::Permission)
    );
    // A modified client sending it anyway is refused by the host.
    let out = net.host.receive(
        member(1),
        ToHost::Submit(Transaction {
            client_seq: 1,
            label: "Rename A".into(),
            ops: vec![rename(ids.a, "Door")],
        }),
    );
    assert!(matches!(
        out[0].message,
        ToClient::Rejected {
            reason: Refusal::Permission,
            ..
        }
    ));
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "A");
}

#[test]
fn a_member_made_viewer_mid_session_has_their_unconfirmed_edit_taken_off() {
    let (mut net, ids) = two_editors();
    net.submit(1, "Rename A", vec![rename(ids.a, "Door")])
        .unwrap();
    net.set_role(1, Role::Viewer);
    net.settle();
    assert_eq!(net.clients[1].role(), Role::Viewer);
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "A");
    net.assert_converged();
}

#[test]
fn a_member_can_only_make_instances_at_ids_from_their_own_blocks() {
    let (mut net, ids) = two_editors();
    let theirs = net.new_id(1);
    let out = net.host.receive(
        member(0),
        ToHost::Submit(Transaction {
            client_seq: 1,
            label: "Insert Part".into(),
            ops: vec![create(vec![part(theirs, ids.workspace, "Crate")])],
        }),
    );
    assert!(matches!(
        out[0].message,
        ToClient::Rejected {
            reason: Refusal::NotYourId { .. },
            ..
        }
    ));
}

#[test]
fn undo_puts_material_back_first_so_metalness_comes_back_too() {
    let (world, ids) = sample();
    let mut doc = crate::apply::Doc::new(world);
    let all = |_| true;
    apply::apply(
        &mut doc.view(),
        &[set(ids.a, "Metalness", Value::Number(0.9))],
        &all,
    )
    .unwrap();
    let before = dump_doc(&doc);
    let applied = apply::apply(
        &mut doc.view(),
        &[set(ids.a, "Material", Value::Material(Material::Wood))],
        &all,
    )
    .unwrap();
    assert_ne!(dump_doc(&doc), before);
    apply::undo(&mut doc.view(), applied.undo);
    assert_eq!(dump_doc(&doc), before);
}

#[test]
fn taking_an_edit_off_and_on_again_keeps_tag_and_child_order() {
    let (mut net, ids) = two_editors();
    let tag = |id, tag: &str, added| crate::op::Op::Tag {
        id,
        tag: tag.into(),
        added,
    };
    net.submit(
        0,
        "Tag",
        vec![tag(ids.a, "Lava", true), tag(ids.a, "Hot", true)],
    )
    .unwrap();
    net.settle();
    // Client 1's untag and move stay pending while client 0's edits arrive,
    // so they come off and go back on several times.
    net.submit(
        1,
        "Untag",
        vec![tag(ids.a, "Lava", false), move_to(ids.b, ids.shed)],
    )
    .unwrap();
    net.submit(0, "Rename", vec![rename(ids.a, "Door")])
        .unwrap();
    net.submit(0, "Tag", vec![tag(ids.a, "Big", true)]).unwrap();
    net.deliver_up(0);
    net.deliver_up(0);
    net.deliver_down(1);
    net.deliver_down(1);
    net.settle();
    let a = net.host.world().get(ids.a).unwrap();
    assert_eq!(a.tags(), ["Hot".to_owned(), "Big".to_owned()]);
    net.assert_converged();
}

#[test]
fn new_ids_never_clash_and_more_come_before_a_writer_runs_out() {
    let (mut net, ids) = two_editors();
    let mut made = Vec::new();
    for n in 0..600 {
        let i = n % 2;
        let id = net.new_id(i);
        net.submit(
            i,
            "Insert Part",
            vec![create(vec![part(id, ids.workspace, "P")])],
        )
        .unwrap();
        made.push(id);
        net.settle();
    }
    made.sort_by_key(|id| id.to_bits());
    made.dedup();
    assert_eq!(made.len(), 600);
    net.assert_converged();
}

#[test]
fn undo_puts_metalness_back_even_when_it_ended_where_it_started() {
    // Found by the convergence test (seed 9): putting Material back resets
    // Metalness to the material's preset, so undo must set it again.
    let (world, ids) = sample();
    let mut doc = crate::apply::Doc::new(world);
    let all = |_| true;
    apply::apply(
        &mut doc.view(),
        &[set(ids.a, "Metalness", Value::Number(1.0))],
        &all,
    )
    .unwrap();
    let before = dump_doc(&doc);
    let both = crate::op::Op::SetProps {
        id: ids.a,
        properties: vec![
            ("Material".into(), Value::Material(Material::Granite)),
            ("Metalness".into(), Value::Number(1.0)),
        ],
    };
    let applied = apply::apply(&mut doc.view(), &[both], &all).unwrap();
    apply::undo(&mut doc.view(), applied.undo);
    assert_eq!(dump_doc(&doc), before);
}
