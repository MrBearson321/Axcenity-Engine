//! Edits made straight on a member's World (as the editor's tools, plugins
//! and AI clients make them) become transactions with `Client::capture`
//! and end the same on every copy.

use axcenity_core::{ClassName, InstanceId, Material, Value, Vector3, World};

use crate::apply::Refusal;
use crate::client::Notice;
use crate::role::Role;
use crate::testing::*;

struct Ids {
    workspace: InstanceId,
    shed: InstanceId,
    a: InstanceId,
    b: InstanceId,
}

/// A Folder "Shed" and Parts "A" and "B" in the Workspace, with an owner,
/// an editor and a viewer joined.
fn session() -> (Net, Ids) {
    let mut world = World::new();
    let workspace = world.workspace();
    let mut make = |class, name: &str| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    };
    let shed = make(ClassName::Folder, "Shed");
    let a = make(ClassName::Part, "A");
    let b = make(ClassName::Part, "B");
    let net = Net::new(world, &[Role::Owner, Role::Editor, Role::Viewer]);
    (
        net,
        Ids {
            workspace,
            shed,
            a,
            b,
        },
    )
}

#[test]
fn a_part_inserted_straight_on_the_world_reaches_everyone_at_the_same_id() {
    let (mut net, ids) = session();
    let world = net.world_mut(1);
    let id = world.create(ClassName::Part).unwrap();
    assert!(
        !id.is_local(),
        "a member's new Instances take IDs from its blocks"
    );
    world.set_name(id, "Wall").unwrap();
    world
        .set_known_property(id, "Transparency", Value::Number(0.5))
        .unwrap();
    world.set_parent(id, Some(ids.shed)).unwrap();
    assert!(net.capture(1, "Insert Part").is_some());
    assert_eq!(
        net.worlds[1].get(id).unwrap().name(),
        "Wall",
        "shows at once"
    );
    net.settle();
    net.assert_converged();
    let host = net.host.world().get(id).unwrap();
    assert_eq!(host.parent(), Some(ids.shed));
    assert_eq!(host.property("Transparency"), Some(&Value::Number(0.5)));
}

#[test]
fn a_duplicated_model_comes_with_its_children_in_order() {
    let (mut net, ids) = session();
    let world = net.world_mut(0);
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(ids.workspace)).unwrap();
    for name in ["Seat", "Back", "Leg"] {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, name).unwrap();
        world.set_parent(part, Some(model)).unwrap();
    }
    net.capture(0, "Insert Model");
    let world = net.world_mut(0);
    let copy = world.duplicate(model).unwrap();
    world.set_parent(copy, Some(ids.shed)).unwrap();
    net.capture(0, "Duplicate");
    net.settle();
    net.assert_converged();
    let host = net.host.world();
    let names: Vec<_> = host
        .get(copy)
        .unwrap()
        .children()
        .iter()
        .map(|&c| host.get(c).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, ["Seat", "Back", "Leg"]);
}

#[test]
fn something_built_outside_the_game_is_sent_once_placed() {
    let (mut net, ids) = session();
    let world = net.world_mut(0);
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Kit").unwrap();
    assert_eq!(net.capture(0, "Build"), None, "not in the game yet");
    let world = net.world_mut(0);
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(model)).unwrap();
    world.add_tag(model, "Prefab").unwrap();
    assert_eq!(net.capture(0, "Build"), None);
    net.world_mut(0)
        .set_parent(model, Some(ids.workspace))
        .unwrap();
    assert!(net.capture(0, "Place").is_some());
    net.settle();
    net.assert_converged();
    let host = net.host.world();
    assert_eq!(host.get(model).unwrap().tags(), ["Prefab"]);
    assert_eq!(host.get(part).unwrap().parent(), Some(model));
}

#[test]
fn moves_renames_attributes_and_tags_all_arrive() {
    let (mut net, ids) = session();
    let world = net.world_mut(1);
    world.set_parent_at(ids.b, ids.workspace, 0).unwrap();
    world.set_parent(ids.a, Some(ids.shed)).unwrap();
    world.set_name(ids.a, "Door").unwrap();
    world
        .set_attribute(ids.a, "Damage", Some(Value::Number(3.0)))
        .unwrap();
    world.add_tag(ids.a, "Hot").unwrap();
    world.add_tag(ids.a, "Lava").unwrap();
    net.capture(1, "Edit");
    let world = net.world_mut(1);
    world.remove_tag(ids.a, "Hot").unwrap();
    world.add_tag(ids.a, "Hot").unwrap();
    world.set_attribute(ids.a, "Damage", None).unwrap();
    net.capture(1, "Edit");
    net.settle();
    net.assert_converged();
    let host = net.host.world();
    let a = host.get(ids.a).unwrap();
    assert_eq!(a.name(), "Door");
    assert_eq!(a.parent(), Some(ids.shed));
    assert_eq!(a.tags(), ["Lava", "Hot"]);
    assert_eq!(a.attribute("Damage"), None);
    assert_eq!(host.get(ids.workspace).unwrap().children()[0], ids.b);
}

#[test]
fn an_edit_that_ends_where_it_started_sends_nothing() {
    let (mut net, ids) = session();
    let world = net.world_mut(0);
    world.set_name(ids.a, "Temp").unwrap();
    world.set_name(ids.a, "A").unwrap();
    world.set_parent(ids.a, Some(ids.shed)).unwrap();
    world.set_parent_at(ids.a, ids.workspace, 1).unwrap();
    assert_eq!(net.capture(0, "Nothing"), None);
    assert_eq!(net.waiting_up(0), 0);
}

#[test]
fn a_delete_by_detaching_can_be_restored() {
    let (mut net, ids) = session();
    net.world_mut(0).set_parent(ids.a, None).unwrap();
    net.capture(0, "Delete");
    net.settle();
    net.assert_converged();
    assert!(net.host.doc().graveyard.contains_key(&ids.a));
    assert!(!net.host.doc().in_tree(ids.a));
    // Someone else's editor restores it (as undoing their own delete
    // would, on the deleting member's).
    net.world_mut(1)
        .set_parent_at(ids.a, ids.workspace, 1)
        .unwrap();
    net.capture(1, "Restore");
    net.settle();
    net.assert_converged();
    assert!(net.host.doc().graveyard.is_empty());
    assert_eq!(
        net.host.world().get(ids.a).unwrap().parent(),
        Some(ids.workspace)
    );
}

#[test]
fn material_then_metalness_ends_the_same_everywhere() {
    let (mut net, ids) = session();
    let world = net.world_mut(0);
    world
        .set_known_property(ids.a, "Material", Value::Material(Material::Granite))
        .unwrap();
    world
        .set_known_property(ids.a, "Metalness", Value::Number(1.0))
        .unwrap();
    net.capture(0, "Paint");
    // A concurrent Material change by another member lands first.
    net.world_mut(1)
        .set_known_property(ids.a, "Material", Value::Material(Material::Wood))
        .unwrap();
    net.capture(1, "Paint");
    net.deliver_up(1);
    net.settle();
    net.assert_converged();
    let a = net.host.world().get(ids.a).unwrap();
    assert_eq!(
        a.property("Material"),
        Some(&Value::Material(Material::Granite))
    );
    assert_eq!(a.property("Metalness"), Some(&Value::Number(1.0)));
}

#[test]
fn two_members_moving_one_part_end_with_the_later_move_in_host_order() {
    let (mut net, ids) = session();
    let spot = |x| Value::Vector3(Vector3 { x, y: 1.0, z: 0.0 });
    net.world_mut(0)
        .set_known_property(ids.a, "Position", spot(1.0))
        .unwrap();
    net.world_mut(1)
        .set_known_property(ids.a, "Position", spot(2.0))
        .unwrap();
    net.capture(0, "Move");
    net.capture(1, "Move");
    net.deliver_up(1);
    net.deliver_up(0);
    net.settle();
    net.assert_converged();
    assert_eq!(
        net.host.world().get(ids.a).unwrap().property("Position"),
        Some(&spot(1.0))
    );
}

#[test]
fn edits_not_captured_yet_are_sent_before_a_host_message_applies() {
    let (mut net, ids) = session();
    net.submit(0, "Rename", vec![rename(ids.b, "Wall")])
        .unwrap();
    net.world_mut(1).set_name(ids.a, "Door").unwrap();
    // Member 1 receives member 0's edit while its own isn't captured.
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.world().get(ids.a).unwrap().name(), "Door");
    assert_eq!(net.host.world().get(ids.b).unwrap().name(), "Wall");
}

#[test]
fn a_viewers_edit_is_taken_back() {
    let (mut net, ids) = session();
    net.world_mut(2).set_name(ids.a, "Mine").unwrap();
    assert_eq!(net.capture(2, "Rename"), None);
    assert_eq!(net.worlds[2].get(ids.a).unwrap().name(), "A");
    assert!(matches!(
        &net.clients[2].take_notices()[..],
        [Notice::Refused {
            reason: Refusal::Permission,
            ..
        }]
    ));
    net.settle();
    net.assert_converged();
}

#[test]
fn edits_made_offline_are_sent_after_reconnecting() {
    let (mut net, ids) = session();
    net.disconnect(1);
    let world = net.world_mut(1);
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(ids.shed)).unwrap();
    net.capture(1, "Insert");
    net.world_mut(1).set_name(ids.b, "Offline").unwrap();
    net.capture(1, "Rename");
    net.submit(0, "Rename", vec![rename(ids.a, "Online")])
        .unwrap();
    net.settle();
    net.connect(1);
    net.settle();
    net.assert_converged();
    assert!(net.host.world().get(id).is_some());
    assert_eq!(net.host.world().get(ids.b).unwrap().name(), "Offline");
}

#[test]
fn ids_keep_coming_while_a_member_inserts_a_lot() {
    let (mut net, ids) = session();
    for n in 0..3000 {
        let world = net.world_mut(1);
        let id = world.create(ClassName::Part).unwrap();
        assert!(!id.is_local(), "insert {n} ran out of IDs");
        world.set_parent(id, Some(ids.shed)).unwrap();
        net.capture(1, "Insert");
        net.settle();
    }
    net.assert_converged();
}
