use super::*;
use crate::enums::Material;
use crate::value::Vector3;

fn part_in(world: &mut World, parent: InstanceId, name: &str) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(parent)).unwrap();
    part
}

#[test]
fn a_cloned_world_has_the_same_ids_and_changes_to_it_stay_in_it() {
    let mut world = World::new();
    let workspace = world.workspace();
    let pad = part_in(&mut world, workspace, "Pad");
    let removed = part_in(&mut world, workspace, "Gone");
    world.destroy(removed).unwrap();

    let mut copy = world.clone();
    assert_eq!(copy.find_first_child(workspace, "Pad"), Some(pad));
    assert!(
        !copy.contains(removed),
        "a destroyed ID stays dead in the copy"
    );
    copy.set_name(pad, "Changed").unwrap();
    copy.destroy(pad).unwrap();
    part_in(&mut copy, workspace, "New");

    assert_eq!(world.get(pad).unwrap().name(), "Pad");
    assert_eq!(world.get(workspace).unwrap().children(), &[pad]);
    // Something made only in the copy isn't in the original.
    let new = copy.find_first_child(workspace, "New").unwrap();
    assert!(!world.contains(new));
}

#[test]
fn new_world_has_game_and_its_services() {
    let world = World::new();
    let game = world.get(world.root()).unwrap();
    assert_eq!(game.class(), ClassName::DataModel);
    let starter_player = world.service(ClassName::StarterPlayer).unwrap();
    assert_eq!(world.starter_player(), starter_player);
    assert_eq!(
        game.children(),
        &[
            world.workspace(),
            world.players(),
            world.lighting(),
            world.replicated_storage(),
            world.server_script_service(),
            world.server_storage(),
            starter_player,
            world.teams(),
            world.sound_service(),
        ]
    );
    assert_eq!(
        world.get(starter_player).unwrap().children(),
        &[world.starter_player_scripts()]
    );
    for class in ClassName::ALL {
        let service = world.service(class);
        assert_eq!(service.is_some(), class.is_service(), "{class}");
        if let Some(id) = service {
            assert_eq!(world.get(id).unwrap().class(), class);
        }
    }
    let workspace = world.get(world.workspace()).unwrap();
    assert_eq!(workspace.parent(), Some(world.root()));
    let scripts = world.get(world.server_script_service()).unwrap();
    assert_eq!(scripts.class(), ClassName::ServerScriptService);
}

#[test]
fn create_gives_defaults_and_no_parent() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let instance = world.get(part).unwrap();
    assert_eq!(instance.name(), "Part");
    assert_eq!(instance.parent(), None);
    assert_eq!(
        instance.property("Size"),
        Some(&Value::Vector3(Vector3::new(4.0, 1.0, 2.0)))
    );
}

#[test]
fn services_cannot_be_created_moved_or_destroyed() {
    let mut world = World::new();
    let workspace = world.workspace();
    assert_eq!(
        world.create(ClassName::Workspace),
        Err(TreeError::Service(ClassName::Workspace))
    );
    assert!(world.set_parent(workspace, None).is_err());
    assert!(world.destroy(workspace).is_err());
    assert!(world.destroy(world.root()).is_err());
    assert!(world.destroy(world.server_script_service()).is_err());
}

#[test]
fn find_first_descendant_searches_the_whole_subtree() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let deep = part_in(&mut world, a, "Target");
    assert_eq!(world.find_first_child(workspace, "Target"), None);
    assert_eq!(world.find_first_descendant(workspace, "Target"), Some(deep));
}

#[test]
fn parent_links_both_ways() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "Block");
    assert_eq!(world.get(part).unwrap().parent(), Some(workspace));
    assert_eq!(world.get(workspace).unwrap().children(), &[part]);
    assert_eq!(world.find_first_child(workspace, "Block"), Some(part));
    assert_eq!(world.full_name(part).as_deref(), Some("Workspace.Block"));
}

#[test]
fn reparent_moves_between_parents() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, workspace, "B");

    world.set_parent(b, Some(a)).unwrap();
    assert_eq!(world.get(workspace).unwrap().children(), &[a]);
    assert_eq!(world.get(a).unwrap().children(), &[b]);
    assert_eq!(world.get(b).unwrap().parent(), Some(a));

    world.set_parent(b, None).unwrap();
    assert!(world.get(a).unwrap().children().is_empty());
    assert_eq!(world.get(b).unwrap().parent(), None);
}

#[test]
fn reparent_into_own_descendant_is_rejected() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, a, "B");
    assert_eq!(world.set_parent(a, Some(b)), Err(TreeError::Cycle));
    assert_eq!(world.set_parent(a, Some(a)), Err(TreeError::Cycle));
    // Nothing changed.
    assert_eq!(world.get(a).unwrap().parent(), Some(workspace));
}

#[test]
fn set_parent_at_puts_it_at_that_index() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, workspace, "B");
    let c = part_in(&mut world, workspace, "C");

    // Within the same parent: C moves to the front.
    world.set_parent_at(c, workspace, 0).unwrap();
    assert_eq!(world.get(workspace).unwrap().children(), &[c, a, b]);
    // Into another parent, past the end: it goes last.
    world.set_parent_at(b, a, 99).unwrap();
    assert_eq!(world.get(a).unwrap().children(), &[b]);
    // Detached, then back in the middle.
    world.set_parent(c, None).unwrap();
    world.set_parent_at(c, workspace, 1).unwrap();
    assert_eq!(world.get(workspace).unwrap().children(), &[a, c]);
    assert_eq!(world.set_parent_at(a, b, 0), Err(TreeError::Cycle));
}

#[test]
fn duplicate_copies_the_whole_subtree_without_a_parent() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Car").unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let wheel = part_in(&mut world, model, "Wheel");
    let hub = part_in(&mut world, wheel, "Hub");
    world
        .set_property(wheel, "Metalness", Value::Number(0.7))
        .unwrap();

    let copy = world.duplicate(model).unwrap();
    let instance = world.get(copy).unwrap();
    assert_eq!(
        (instance.class(), instance.name()),
        (ClassName::Model, "Car")
    );
    assert_eq!(instance.parent(), None);
    let copied_wheel = instance.children()[0];
    assert_ne!(copied_wheel, wheel);
    let copied_wheel = world.get(copied_wheel).unwrap();
    assert_eq!(copied_wheel.name(), "Wheel");
    assert_eq!(copied_wheel.parent(), Some(copy));
    assert_eq!(
        copied_wheel.property("Metalness"),
        Some(&Value::Number(0.7))
    );
    let copied_hub = world.get(copied_wheel.children()[0]).unwrap();
    assert_eq!(copied_hub.name(), "Hub");
    // The original is untouched.
    assert_eq!(world.get(wheel).unwrap().children(), &[hub]);
    assert_eq!(world.get(workspace).unwrap().children(), &[model]);
    assert!(world.duplicate(workspace).is_err());
}

#[test]
fn is_descendant_of_follows_parents_up() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, a, "B");
    assert!(world.is_descendant_of(b, world.root()));
    assert!(world.is_descendant_of(b, a));
    assert!(!world.is_descendant_of(a, b));
    assert!(!world.is_descendant_of(a, a));
    world.set_parent(a, None).unwrap();
    assert!(!world.is_descendant_of(b, world.root()));
}

#[test]
fn destroy_removes_descendants_and_makes_ids_stale() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, a, "B");
    let c = part_in(&mut world, b, "C");
    let before = world.len();

    world.destroy(a).unwrap();
    assert_eq!(world.len(), before - 3);
    assert!(world.get(workspace).unwrap().children().is_empty());
    for id in [a, b, c] {
        assert!(world.get(id).is_none());
        assert_eq!(world.set_parent(id, None), Err(TreeError::Stale));
        assert_eq!(world.destroy(id), Err(TreeError::Stale));
        assert_eq!(
            world.set_property(id, "Metalness", Value::Number(1.0)),
            Err(PropertyError::Stale)
        );
    }
}

#[test]
fn stale_id_stays_stale_after_its_slot_is_reused() {
    let mut world = World::new();
    let old = world.create(ClassName::Part).unwrap();
    world.destroy(old).unwrap();
    let new = world.create(ClassName::Part).unwrap();
    assert_ne!(old, new);
    assert!(world.get(old).is_none());
    assert!(world.get(new).is_some());
}

#[test]
fn descendants_are_parents_first_in_child_order() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let a1 = part_in(&mut world, a, "A1");
    let b = part_in(&mut world, workspace, "B");
    assert_eq!(world.descendants(workspace), vec![a, a1, b]);
}

#[test]
fn set_property_checks_name_and_type() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();

    world
        .set_property(part, "Metalness", Value::Number(1.0))
        .unwrap();
    assert_eq!(
        world.get(part).unwrap().property("Metalness"),
        Some(&Value::Number(1.0))
    );

    assert!(matches!(
        world.set_property(part, "Shininess", Value::Number(1.0)),
        Err(PropertyError::Unknown { .. })
    ));
    assert!(matches!(
        world.set_property(part, "Metalness", Value::Bool(true)),
        Err(PropertyError::WrongType { .. })
    ));
}

#[test]
fn setting_material_applies_its_preset_which_can_then_be_adjusted() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let number = |world: &World, name| world.get(part).unwrap().property(name).cloned();

    world
        .set_property(part, "Material", Value::Material(Material::Metal))
        .unwrap();
    assert_eq!(number(&world, "Metalness"), Some(Value::Number(1.0)));
    assert_eq!(number(&world, "Roughness"), Some(Value::Number(0.3)));

    world
        .set_property(part, "Roughness", Value::Number(0.8))
        .unwrap();
    assert_eq!(number(&world, "Roughness"), Some(Value::Number(0.8)));
    assert_eq!(
        number(&world, "Material"),
        Some(Value::Material(Material::Metal))
    );
}

#[test]
fn default_part_matches_the_plastic_preset() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let instance = world.get(part).unwrap();
    let preset = Material::Plastic.preset();
    assert_eq!(
        instance.property("Material"),
        Some(&Value::Material(Material::Plastic))
    );
    assert_eq!(
        instance.property("Metalness"),
        Some(&Value::Number(preset.metalness))
    );
    assert_eq!(
        instance.property("Roughness"),
        Some(&Value::Number(preset.roughness))
    );
}

fn changes(world: &mut World) -> Vec<Change> {
    world
        .take_changes()
        .into_iter()
        .map(|recorded| recorded.change)
        .collect()
}

#[test]
fn changes_are_only_recorded_once_asked() {
    let mut world = World::new();
    let workspace = world.workspace();
    part_in(&mut world, workspace, "Before");
    assert!(world.take_changes().is_empty());

    world.record_changes();
    let part = part_in(&mut world, workspace, "After");
    let property = |name| Change::Property {
        instance: part,
        name,
    };
    assert_eq!(
        changes(&mut world),
        [
            property("Name"),
            Change::ChildAdded {
                parent: workspace,
                child: part
            },
            property("Parent"),
        ]
    );
    // Taken, so the list starts again.
    assert!(world.take_changes().is_empty());
}

#[test]
fn only_real_changes_are_recorded_and_numbered_in_order() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "Pad");
    world.record_changes();
    let first = world.next_change_number();

    // The values it already has: no change.
    world.set_name(part, "Pad").unwrap();
    world
        .set_property(part, "Anchored", Value::Bool(false))
        .unwrap();
    world.set_parent(part, Some(workspace)).unwrap();
    assert!(world.take_changes().is_empty());
    assert_eq!(world.next_change_number(), first);

    world
        .set_property(part, "Anchored", Value::Bool(true))
        .unwrap();
    // Metal's preset differs in both, so all three change.
    world
        .set_property(part, "Material", Value::Material(Material::Metal))
        .unwrap();
    let recorded = world.take_changes();
    let names: Vec<_> = recorded
        .iter()
        .map(|recorded| match &recorded.change {
            Change::Property { name, .. } => *name,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(names, ["Anchored", "Material", "Metalness", "Roughness"]);
    let numbers: Vec<u64> = recorded.iter().map(|recorded| recorded.number).collect();
    assert_eq!(numbers, [first, first + 1, first + 2, first + 3]);
    assert_eq!(world.next_change_number(), first + 4);
}

#[test]
fn moving_records_removed_added_and_parent_but_reordering_does_not() {
    let mut world = World::new();
    let workspace = world.workspace();
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_parent(folder, Some(workspace)).unwrap();
    let part = part_in(&mut world, workspace, "Pad");
    world.record_changes();

    world.set_parent_at(part, workspace, 0).unwrap();
    assert!(world.take_changes().is_empty());

    world.set_parent(part, Some(folder)).unwrap();
    world.destroy(part).unwrap();
    assert_eq!(
        changes(&mut world),
        [
            Change::ChildRemoved {
                parent: workspace,
                child: part
            },
            Change::ChildAdded {
                parent: folder,
                child: part
            },
            Change::Property {
                instance: part,
                name: "Parent"
            },
            // Destroying takes it out of its parent first.
            Change::ChildRemoved {
                parent: folder,
                child: part
            },
            Change::Property {
                instance: part,
                name: "Parent"
            },
        ]
    );
}

#[test]
fn every_terrain_has_data_that_copies_share_until_one_changes() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let data = world.terrain(terrain).unwrap().clone();
    assert_eq!(data.size(), (257, 257));

    // Play's copy shares it; changing it there leaves the original alone.
    let mut played = world.clone();
    assert!(Arc::ptr_eq(played.terrain(terrain).unwrap(), &data));
    let all = data.bounds();
    played.terrain_mut(terrain).unwrap().heights_mut_in(all)[0] = 1.0;
    assert_eq!(world.terrain(terrain).unwrap().heights()[0], 0.25);
    assert_ne!(played.terrain(terrain).unwrap().uid(), data.uid());

    // A duplicate gets the same landscape; destroying drops it.
    let copy = world.duplicate(terrain).unwrap();
    assert_eq!(world.terrain(copy).unwrap().heights(), data.heights());
    world.destroy(copy).unwrap();
    assert!(world.terrain(copy).is_none());

    let part = world.create(ClassName::Part).unwrap();
    assert!(world.terrain(part).is_none());
    assert!(!world.set_terrain(part, data));
}

#[test]
fn attributes_are_set_changed_and_removed_with_change_records() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "Lava");
    world.record_changes();
    world
        .set_attribute(part, "Damage", Some(Value::Number(10.0)))
        .unwrap();
    // The same value again isn't a change.
    world
        .set_attribute(part, "Damage", Some(Value::Number(10.0)))
        .unwrap();
    world
        .set_attribute(part, "Kind", Some(Value::Material(Material::Neon)))
        .unwrap();
    assert_eq!(
        world.get(part).unwrap().attribute("Damage"),
        Some(&Value::Number(10.0))
    );
    // Property enums are kept as plain enum items.
    assert_eq!(
        world.get(part).unwrap().attribute("Kind"),
        Some(&Value::EnumItem(crate::EnumItem {
            enum_name: "Material".into(),
            item: "Neon".into()
        }))
    );
    world.set_attribute(part, "Damage", None).unwrap();
    assert_eq!(world.get(part).unwrap().attribute("Damage"), None);

    let names: Vec<String> = world
        .take_changes()
        .into_iter()
        .map(|recorded| match recorded.change {
            Change::Attribute { name, .. } => name,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(names, ["Damage", "Kind", "Damage"]);

    assert!(matches!(
        world.set_attribute(part, "has space", Some(Value::Bool(true))),
        Err(AttributeError::BadName(_))
    ));
    assert_eq!(
        world.set_attribute(part, "Target", Some(Value::Instance(Some(part)))),
        Err(AttributeError::BadValue)
    );
}

#[test]
fn tags_are_listed_in_order_and_only_found_inside_the_game() {
    let mut world = World::new();
    let workspace = world.workspace();
    let lava = part_in(&mut world, workspace, "Lava");
    let loose = world.create(ClassName::Part).unwrap();
    assert!(world.add_tag(lava, "Lava").unwrap());
    assert!(!world.add_tag(lava, "Lava").unwrap());
    world.add_tag(lava, "Hot").unwrap();
    world.add_tag(loose, "Lava").unwrap();
    assert_eq!(world.get(lava).unwrap().tags(), ["Lava", "Hot"]);
    assert_eq!(world.tagged("Lava"), [lava]);
    assert_eq!(
        world.all_tags().into_iter().collect::<Vec<_>>(),
        [("Hot".to_owned(), 1), ("Lava".to_owned(), 1)]
    );
    assert!(world.remove_tag(lava, "Lava").unwrap());
    assert!(!world.remove_tag(lava, "Lava").unwrap());
    assert!(world.tagged("Lava").is_empty());
}

#[test]
fn duplicate_copies_attributes_tags_and_points_references_inside_the_copy() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let door = part_in(&mut world, model, "Door");
    let outside = part_in(&mut world, workspace, "Outside");
    let to_door = world.create(ClassName::ObjectValue).unwrap();
    world.set_parent(to_door, Some(model)).unwrap();
    world
        .set_property(to_door, "Value", Value::Instance(Some(door)))
        .unwrap();
    let to_outside = world.create(ClassName::ObjectValue).unwrap();
    world.set_parent(to_outside, Some(model)).unwrap();
    world
        .set_property(to_outside, "Value", Value::Instance(Some(outside)))
        .unwrap();
    world.add_tag(door, "Door").unwrap();
    world
        .set_attribute(door, "Locked", Some(Value::Bool(true)))
        .unwrap();

    let copy = world.duplicate(model).unwrap();
    let children = world.get(copy).unwrap().children().to_vec();
    let [new_door, new_to_door, new_to_outside] = children[..] else {
        panic!("{children:?}");
    };
    assert_eq!(world.get(new_door).unwrap().tags(), ["Door"]);
    assert_eq!(
        world.get(new_door).unwrap().attribute("Locked"),
        Some(&Value::Bool(true))
    );
    let value = |id| world.get(id).unwrap().property("Value").unwrap().clone();
    assert_eq!(value(new_to_door), Value::Instance(Some(new_door)));
    assert_eq!(value(new_to_outside), Value::Instance(Some(outside)));
}

#[test]
fn an_int_value_rounds_to_a_whole_number() {
    let mut world = World::new();
    let int = world.create(ClassName::IntValue).unwrap();
    world
        .set_property(int, "Value", Value::Number(2.6))
        .unwrap();
    assert_eq!(
        world.get(int).unwrap().property("Value"),
        Some(&Value::Number(3.0))
    );
}

#[test]
fn copy_from_brings_a_tree_over_from_another_world_without_its_scripts() {
    let mut level = World::new();
    let workspace = level.workspace();
    let model = level.create(ClassName::Model).unwrap();
    level.set_name(model, "Dungeon").unwrap();
    level.set_parent(model, Some(workspace)).unwrap();
    let door = part_in(&mut level, model, "Door");
    let script = level.create(ClassName::Script).unwrap();
    level.set_parent(script, Some(model)).unwrap();
    let link = level.create(ClassName::ObjectValue).unwrap();
    level
        .set_property(link, "Value", Value::Instance(Some(door)))
        .unwrap();
    level.set_parent(link, Some(model)).unwrap();
    let outside = level.create(ClassName::ObjectValue).unwrap();
    level
        .set_property(outside, "Value", Value::Instance(Some(workspace)))
        .unwrap();
    level.set_parent(outside, Some(model)).unwrap();

    let mut world = World::new();
    let copy = world.copy_from(&level, model).unwrap();
    assert_eq!(world.get(copy).unwrap().parent(), None);
    let children: Vec<_> = world.get(copy).unwrap().children().to_vec();
    let classes: Vec<_> = children
        .iter()
        .map(|&id| world.get(id).unwrap().class())
        .collect();
    assert_eq!(
        classes,
        [
            ClassName::Part,
            ClassName::ObjectValue,
            ClassName::ObjectValue
        ]
    );
    // Points at the copied door, and at nothing outside the copy.
    assert_eq!(
        world.get(children[1]).unwrap().property("Value"),
        Some(&Value::Instance(Some(children[0])))
    );
    assert_eq!(
        world.get(children[2]).unwrap().property("Value"),
        Some(&Value::Instance(None))
    );
    assert!(level.contains(door), "the source is untouched");
}

#[test]
fn a_replica_holds_the_servers_instances_at_their_ids_and_its_own_apart() {
    let mut server = World::new();
    let workspace = server.workspace();
    let pad = part_in(&mut server, workspace, "Pad");
    let mut replica = server.empty_replica();
    assert_eq!(replica.workspace(), workspace, "the services match");
    replica.create_at(pad, ClassName::Part).unwrap();
    replica.set_parent(pad, Some(workspace)).unwrap();
    let own = part_in(&mut replica, workspace, "Mine");
    assert!(own.is_local() && !pad.is_local());
    // The server's next Instance fits beside the replica's own.
    let next = part_in(&mut server, workspace, "Next");
    replica.create_at(next, ClassName::Part).unwrap();
    assert_eq!(replica.get(own).unwrap().name(), "Mine");
    assert_eq!(InstanceId::from_bits(next.to_bits()), next);
    assert!(replica.create_at(pad, ClassName::Part).is_err(), "taken");
}

#[test]
fn server_storage_and_server_script_service_are_server_only() {
    let mut world = World::new();
    let storage = world.server_storage();
    let kept = part_in(&mut world, storage, "Kept");
    let workspace = world.workspace();
    let shown = part_in(&mut world, workspace, "Shown");
    assert!(world.is_server_only(kept));
    assert!(world.is_server_only(world.server_script_service()));
    assert!(!world.is_server_only(shown));
    assert_eq!(world.service(ClassName::ServerStorage), Some(storage));
}
