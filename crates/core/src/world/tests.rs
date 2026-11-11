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
            world.text_chat_service(),
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

/// The walk `descendants` did before `descendants_iter`: a stack of every
/// child still to visit. The iterator must give exactly this order.
fn descendants_by_stack(world: &World, id: InstanceId) -> Vec<InstanceId> {
    let mut found = Vec::new();
    let mut stack: Vec<InstanceId> = match world.get(id) {
        Some(instance) => instance.children().iter().rev().copied().collect(),
        None => return found,
    };
    while let Some(next) = stack.pop() {
        if let Some(instance) = world.get(next) {
            found.push(next);
            stack.extend(instance.children().iter().rev().copied());
        }
    }
    found
}

#[test]
fn descendants_iter_matches_the_stack_walk() {
    let mut world = World::new();
    let workspace = world.workspace();
    // Nested Models of different shapes: some with children after a nested
    // one, some whose last child has children, and an empty one.
    let mut models = vec![workspace];
    for i in 0..60 {
        let parent = models[(i * 7) % models.len()];
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(parent)).unwrap();
        for j in 0..(i % 4) {
            part_in(&mut world, model, &format!("P{i}.{j}"));
        }
        models.push(model);
    }
    for id in std::iter::once(world.root()).chain(models.iter().copied()) {
        let walked: Vec<InstanceId> = world.descendants_iter(id).collect();
        assert_eq!(walked, descendants_by_stack(&world, id));
        assert_eq!(walked, world.descendants(id));
    }
    // A leaf and a stale id have none.
    let leaf = part_in(&mut world, workspace, "Leaf");
    assert_eq!(world.descendants_iter(leaf).count(), 0);
    world.destroy(leaf).unwrap();
    assert_eq!(world.descendants_iter(leaf).count(), 0);
}

#[test]
fn descendants_iter_walks_a_deep_chain_in_order() {
    let mut world = World::new();
    let mut chain = vec![world.workspace()];
    for i in 0..5_000 {
        let parent = *chain.last().unwrap();
        chain.push(part_in(&mut world, parent, &format!("C{i}")));
    }
    let walked: Vec<InstanceId> = world.descendants_iter(world.workspace()).collect();
    assert_eq!(walked, chain[1..]);
}

#[test]
fn descendants_iter_keeps_no_positions_for_flat_or_last_child_trees() {
    let mut world = World::new();
    let workspace = world.workspace();
    for i in 0..100 {
        part_in(&mut world, workspace, &format!("Loose{i}"));
    }
    // Each level's only (so last) child has children of its own.
    let mut parent = workspace;
    for i in 0..10 {
        parent = part_in(&mut world, parent, &format!("Nested{i}"));
    }
    let mut walk = world.descendants_iter(workspace);
    assert_eq!(walk.by_ref().count(), 110);
    assert_eq!(walk.parents.capacity(), 0, "nothing was allocated");
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
fn a_copy_from_another_world_keeps_scripts_inside_objects() {
    let mut level = World::new();
    let workspace = level.workspace();
    let model = level.create(ClassName::Model).unwrap();
    level.set_parent(model, Some(workspace)).unwrap();
    let open = level.create(ClassName::Script).unwrap();
    level.set_name(open, "Open").unwrap();
    level
        .set_property(
            open,
            "SourceFile",
            Value::String("objects/Door/Open.server.lua".into()),
        )
        .unwrap();
    level.set_parent(open, Some(model)).unwrap();
    let inside = part_in(&mut level, open, "Inside");
    let service_script = level.create(ClassName::Script).unwrap();
    level.set_parent(service_script, Some(model)).unwrap();

    let mut world = World::new();
    let copy = world.copy_from(&level, model).unwrap();
    let children = world.get(copy).unwrap().children().to_vec();
    assert_eq!(children.len(), 1, "a script with no SourceFile is left out");
    let script = world.get(children[0]).unwrap();
    assert_eq!(script.name(), "Open");
    assert_eq!(script.source_file(), Some("objects/Door/Open.server.lua"));
    assert_eq!(script.children().len(), 1, "with what's inside it");
    assert!(level.get(inside).is_some());
    assert_eq!(level.get(service_script).unwrap().source_file(), None);
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

fn listed(world: &World, class: ClassName) -> Vec<InstanceId> {
    world.all_of(class).collect()
}

fn tagged_by_id(world: &World, tag: &str) -> Vec<InstanceId> {
    world.tagged_iter(tag).collect()
}

/// What a walk from `game` finds: the index must agree.
fn walked(world: &World, class: ClassName) -> Vec<InstanceId> {
    world
        .descendants_iter(world.root())
        .filter(|&id| world.get(id).unwrap().class() == class)
        .collect()
}

#[test]
fn every_class_has_a_slot_in_the_index() {
    // The index keeps a list per `ClassName as usize`, `ALL.len()` of them.
    let mut slots: Vec<usize> = ClassName::ALL.iter().map(|&class| class as usize).collect();
    slots.sort_unstable();
    assert_eq!(slots, (0..ClassName::ALL.len()).collect::<Vec<_>>());
}

#[test]
fn only_instances_in_the_game_are_indexed() {
    let mut world = World::new();
    let model = world.create(ClassName::Model).unwrap();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(model)).unwrap();
    world.add_tag(part, "Lava").unwrap();
    assert!(
        listed(&world, ClassName::Part).is_empty(),
        "not in the game"
    );
    assert!(tagged_by_id(&world, "Lava").is_empty());
    world.check_index().unwrap();

    // Putting the Model in the game brings in everything inside it.
    world.set_parent(model, Some(world.workspace())).unwrap();
    assert_eq!(listed(&world, ClassName::Model), [model]);
    assert_eq!(listed(&world, ClassName::Part), [part]);
    assert_eq!(tagged_by_id(&world, "Lava"), [part]);
    world.check_index().unwrap();

    // Taking it out takes them out, and moving inside the game keeps them.
    world
        .set_parent(model, Some(world.replicated_storage()))
        .unwrap();
    assert_eq!(listed(&world, ClassName::Part), [part], "still in the game");
    world.check_index().unwrap();
    world.set_parent(model, None).unwrap();
    assert!(listed(&world, ClassName::Part).is_empty());
    assert!(tagged_by_id(&world, "Lava").is_empty());
    world.check_index().unwrap();
}

#[test]
fn destroying_takes_out_everything_inside() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let part = part_in(&mut world, model, "Inside");
    world.add_tag(part, "Lava").unwrap();
    world.destroy(model).unwrap();
    assert!(listed(&world, ClassName::Model).is_empty());
    assert!(listed(&world, ClassName::Part).is_empty());
    assert!(world.all_tags().is_empty(), "no tag left listed");
    world.check_index().unwrap();
    // A destroyed Instance stays out if its slot is used again.
    let new = world.create(ClassName::Part).unwrap();
    assert_eq!(listed(&world, ClassName::Part), []);
    world.set_parent(new, Some(workspace)).unwrap();
    assert_eq!(listed(&world, ClassName::Part), [new]);
    world.check_index().unwrap();
}

#[test]
fn tags_follow_the_instance_and_never_game_itself() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "Pad");
    world.add_tag(part, "Lava").unwrap();
    world.add_tag(workspace, "Lava").unwrap();
    // `game` isn't one of its own descendants, so it never was tagged in a
    // walk from it.
    world.add_tag(world.root(), "Lava").unwrap();
    assert_eq!(tagged_by_id(&world, "Lava").len(), 2);
    assert_eq!(world.all_tags()["Lava"], 2);
    world.check_index().unwrap();
    world.remove_tag(part, "Lava").unwrap();
    world.remove_tag(workspace, "Lava").unwrap();
    assert!(world.all_tags().is_empty());
    world.check_index().unwrap();
}

#[test]
fn copies_are_indexed_once_they_are_in_the_game() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let part = part_in(&mut world, model, "Inside");
    world.add_tag(part, "Lava").unwrap();

    let copy = world.duplicate(model).unwrap();
    assert_eq!(listed(&world, ClassName::Part), [part], "no parent yet");
    world.check_index().unwrap();
    world.set_parent(copy, Some(workspace)).unwrap();
    assert_eq!(listed(&world, ClassName::Part).len(), 2);
    assert_eq!(tagged_by_id(&world, "Lava").len(), 2);
    world.check_index().unwrap();

    let mut other = World::new();
    let pasted = other.copy_from(&world, copy).unwrap();
    other.check_index().unwrap();
    other.set_parent(pasted, Some(other.workspace())).unwrap();
    assert_eq!(listed(&other, ClassName::Part).len(), 1);
    assert_eq!(tagged_by_id(&other, "Lava").len(), 1);
    other.check_index().unwrap();
}

#[test]
fn a_world_clone_keeps_its_index_and_moves_back_like_undo() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, workspace, "B");
    world.add_tag(a, "T").unwrap();

    let saved = world.clone();
    world.destroy(a).unwrap();
    world.destroy(b).unwrap();
    assert!(listed(&world, ClassName::Part).is_empty());
    // A whole-world undo swaps the saved copy back in.
    world = saved;
    assert_eq!(listed(&world, ClassName::Part), [a, b]);
    assert_eq!(tagged_by_id(&world, "T"), [a]);
    world.check_index().unwrap();

    // A move undone puts the Instance back at its old index among the
    // same parent's children: tree order, not the index, sees that.
    world.set_parent(a, None).unwrap();
    world.set_parent_at(a, workspace, 1).unwrap();
    world.check_index().unwrap();
    assert_eq!(world.descendants(workspace), [b, a]);
    assert_eq!(listed(&world, ClassName::Part), [a, b], "by ID");
    world.add_tag(b, "T").unwrap();
    assert_eq!(world.tagged("T"), [b, a], "in tree order");
    assert_eq!(tagged_by_id(&world, "T"), [a, b], "by ID");
    world.check_index().unwrap();
}

#[test]
fn a_replica_indexes_what_it_follows() {
    let mut server = World::new();
    let workspace = server.workspace();
    let part = part_in(&mut server, workspace, "Part");

    let mut replica = server.empty_replica();
    replica.create_at(part, ClassName::Part).unwrap();
    assert!(listed(&replica, ClassName::Part).is_empty());
    replica.set_parent(part, Some(replica.workspace())).unwrap();
    assert_eq!(listed(&replica, ClassName::Part), [part]);
    let own = replica.create(ClassName::Part).unwrap();
    replica.set_parent(own, Some(replica.workspace())).unwrap();
    assert_eq!(listed(&replica, ClassName::Part), [part, own]);
    replica.check_index().unwrap();
}

#[test]
fn the_workspace_remembers_what_moved() {
    let mut world = World::new();
    let workspace = world.workspace();
    let start = world.structure_version_of(workspace);
    assert_eq!(world.workspace_moves_since(start), Some(Vec::new()));

    let model = world.create(ClassName::Model).unwrap();
    let inside = part_in(&mut world, model, "Inside");
    world.set_parent(model, Some(workspace)).unwrap();
    let storage = world.replicated_storage();
    let elsewhere = part_in(&mut world, storage, "Elsewhere");
    assert_eq!(world.workspace_moves_since(start), Some(vec![model]));

    // Leaving the Workspace counts; moving outside it doesn't.
    world.set_parent(inside, Some(storage)).unwrap();
    world.set_parent(elsewhere, Some(model)).unwrap();
    let middle = world.structure_version_of(workspace);
    assert_eq!(
        world.workspace_moves_since(start),
        Some(vec![model, inside, elsewhere]),
    );
    world.destroy(model).unwrap();
    assert_eq!(world.workspace_moves_since(middle), Some(vec![model]));

    // A version the world never had is no help.
    assert_eq!(world.workspace_moves_since(middle + 100), None);
    // Nor is one too long ago, or a change to everything.
    let mut flood = World::new();
    let flood_start = flood.structure_version_of(flood.workspace());
    for _ in 0..=WORKSPACE_MOVES_KEPT {
        let workspace = flood.workspace();
        part_in(&mut flood, workspace, "Part");
    }
    assert_eq!(flood.workspace_moves_since(flood_start), None);
    let version = world.structure_version_of(workspace);
    world.set_collision_groups(CollisionGroups::default());
    assert_eq!(world.workspace_moves_since(version), None);
}

#[test]
fn a_service_version_changes_only_with_its_own_tree() {
    let mut world = World::new();
    let (workspace, storage) = (world.workspace(), world.replicated_storage());

    let model = world.create(ClassName::Model).unwrap();
    let part = world.create(ClassName::Part).unwrap();
    let total = world.structure_version();
    world.set_parent(part, Some(model)).unwrap();
    assert_eq!(world.structure_version(), total + 1, "the tree changed");
    assert_eq!(world.structure_version_of(workspace), 0, "no service did");

    // Into a service, within it, and between two.
    world.set_parent(model, Some(storage)).unwrap();
    assert_eq!(world.structure_version_of(storage), 1);
    assert_eq!(world.structure_version_of(workspace), 0);
    let inner = world.create(ClassName::Model).unwrap();
    world.set_parent(inner, Some(model)).unwrap();
    world.set_parent(part, Some(inner)).unwrap();
    assert_eq!(world.structure_version_of(storage), 3);
    assert_eq!(world.structure_version_of(workspace), 0);
    world.set_parent(model, Some(workspace)).unwrap();
    assert_eq!(world.structure_version_of(storage), 4);
    assert_eq!(world.structure_version_of(workspace), 1);

    // Reordering among the same parent's children isn't a move.
    let other = part_in(&mut world, workspace, "Other");
    let before = world.structure_version_of(workspace);
    world.set_parent_at(other, workspace, 0).unwrap();
    assert_eq!(world.structure_version_of(workspace), before);

    // A nested service's contents count as its parent service's.
    let scripts = world.starter_player_scripts();
    let local = world.create(ClassName::LocalScript).unwrap();
    world.set_parent(local, Some(scripts)).unwrap();
    assert_eq!(world.structure_version_of(world.starter_player()), 1);

    // Collision groups are read with the parts.
    let before = world.structure_version_of(workspace);
    world.collision_groups_mut();
    assert_eq!(world.structure_version_of(workspace), before + 1);
    assert_eq!(world.structure_version_of(storage), 4);
}

#[test]
fn all_below_is_the_same_list_as_a_filtered_walk() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let later = part_in(&mut world, workspace, "Later");
    let inside = part_in(&mut world, model, "Inside");
    // Made after `later` but before it in the tree.
    let first = world.create(ClassName::Part).unwrap();
    world.set_parent_at(first, workspace, 0).unwrap();
    let storage = world.replicated_storage();
    let stored = part_in(&mut world, storage, "Stored");
    let is_part = |class| class == ClassName::Part;

    let walk: Vec<_> = world
        .descendants_iter(workspace)
        .filter(|&id| world.get(id).unwrap().class() == ClassName::Part)
        .collect();
    assert_eq!(walk, [first, inside, later]);
    assert_eq!(world.all_below(workspace, is_part), walk);
    let mut everywhere = walk.clone();
    everywhere.push(stored);
    assert_eq!(world.all_below(world.root(), is_part), everywhere);
    assert_eq!(world.all_below(model, is_part), [inside]);
}

#[test]
fn in_tree_order_leaves_out_what_is_not_in_the_game() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    let b = part_in(&mut world, workspace, "B");
    let loose = world.create(ClassName::Part).unwrap();
    assert_eq!(world.in_tree_order(vec![b, loose, a, b]), [a, b]);
    assert!(world.in_tree_order(vec![]).is_empty());
    assert_eq!(world.in_tree_order(vec![loose]), [loose], "too few to sort");
}

/// A small repeatable random number generator, so a failing run can be
/// reproduced from its seed.
struct Dice(u64);

impl Dice {
    fn roll(&mut self, sides: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % sides
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> Option<T> {
        (!from.is_empty()).then(|| from[self.roll(from.len())])
    }
}

/// Runs random operations on a world and returns the order the index
/// listed each class and tag in at the end, checking the index after every
/// few.
fn random_operations(seed: u64, steps: usize) -> Vec<Vec<InstanceId>> {
    let mut dice = Dice(seed);
    let mut world = World::new();
    let mut alive = Vec::new();
    let classes = [
        ClassName::Part,
        ClassName::Model,
        ClassName::Folder,
        ClassName::Sound,
        ClassName::WeldConstraint,
    ];
    let tags = ["A", "B", "C"];
    let services = [
        world.workspace(),
        world.replicated_storage(),
        world.server_storage(),
        world.lighting(),
    ];
    for step in 0..steps {
        let target = dice.pick(&alive);
        match dice.roll(8) {
            0 | 1 => {
                let id = world.create(dice.pick(&classes).unwrap()).unwrap();
                alive.push(id);
            }
            2 => {
                // Anywhere, or nowhere: a cycle is simply refused.
                if let Some(id) = target {
                    let parent = match dice.roll(3) {
                        0 => None,
                        1 => Some(dice.pick(&services).unwrap()),
                        _ => dice.pick(&alive),
                    };
                    let _ = world.set_parent(id, parent);
                }
            }
            3 => {
                if let (Some(id), Some(parent)) = (target, dice.pick(&alive)) {
                    let _ = world.set_parent_at(id, parent, dice.roll(4));
                }
            }
            4 => {
                if let Some(id) = target {
                    world.add_tag(id, dice.pick(&tags).unwrap()).unwrap();
                }
            }
            5 => {
                if let Some(id) = target {
                    world.remove_tag(id, dice.pick(&tags).unwrap()).unwrap();
                }
            }
            6 => {
                if let Some(id) = target
                    && let Ok(copy) = world.duplicate(id)
                {
                    alive.push(copy);
                    alive.extend(world.descendants(copy));
                }
            }
            _ => {
                if let Some(id) = target {
                    let gone: Vec<_> = std::iter::once(id)
                        .chain(world.descendants_iter(id))
                        .collect();
                    world.destroy(id).unwrap();
                    alive.retain(|id| !gone.contains(id));
                }
            }
        }
        if step % 7 == 0 {
            world.check_index().unwrap();
        }
    }
    world.check_index().unwrap();
    // Whatever the mix, the lists match a walk (the index is by ID, the walk
    // by tree, so sets), and `tagged` is in tree order.
    let mut orders = Vec::new();
    for class in classes {
        let mut walk = walked(&world, class);
        walk.sort();
        assert_eq!(listed(&world, class), walk, "{class}");
        orders.push(listed(&world, class));
    }
    for tag in tags {
        let walk: Vec<_> = world
            .descendants_iter(world.root())
            .filter(|&id| world.get(id).unwrap().has_tag(tag))
            .collect();
        assert_eq!(world.tagged(tag), walk, "{tag}");
        orders.push(tagged_by_id(&world, tag));
    }
    orders
}

#[test]
fn random_operations_never_desync_the_index() {
    for seed in 0..40 {
        random_operations(seed, 300);
    }
}

#[test]
fn the_index_order_is_deterministic() {
    for seed in [3, 17, 91] {
        assert_eq!(random_operations(seed, 500), random_operations(seed, 500));
    }
}

#[test]
fn a_parts_motion_changes_its_revision_but_not_its_revision_without_motion() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "Crate");
    let start = world.get(part).unwrap().revision_without_motion();
    for name in MOTION_PROPERTIES {
        let revision = world.get(part).unwrap().revision();
        world
            .set_property(part, name, Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
            .unwrap();
        let instance = world.get(part).unwrap();
        assert_eq!(instance.revision(), revision + 1, "{name}");
        assert_eq!(instance.revision_without_motion(), start, "{name}");
    }
    // Setting the value it has is no change at all.
    world
        .set_property(
            part,
            "Position",
            Value::Vector3(Vector3::new(1.0, 2.0, 3.0)),
        )
        .unwrap();
    assert_eq!(world.get(part).unwrap().revision_without_motion(), start);
    // Anything else counts.
    world
        .set_property(part, "Transparency", Value::Number(0.5))
        .unwrap();
    assert_eq!(
        world.get(part).unwrap().revision_without_motion(),
        start + 1
    );
    world.set_name(part, "Box").unwrap();
    assert_eq!(
        world.get(part).unwrap().revision_without_motion(),
        start + 2
    );

    // Only a BasePart has motion: an Attachment's Position is just a
    // property.
    let attachment = world.create(ClassName::Attachment).unwrap();
    world.set_parent(attachment, Some(part)).unwrap();
    let before = world.get(attachment).unwrap().revision_without_motion();
    world
        .set_property(
            attachment,
            "Position",
            Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
        )
        .unwrap();
    assert_eq!(
        world.get(attachment).unwrap().revision_without_motion(),
        before + 1
    );
}

#[test]
fn the_edit_journal_is_off_until_asked_and_keeps_what_each_edit_replaced() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    assert!(!world.has_edits(), "off by default");
    world.record_edits();
    let b = part_in(&mut world, workspace, "B");
    world.set_name(a, "Door").unwrap();
    world.set_parent_at(b, workspace, 0).unwrap();
    world
        .set_attribute(a, "Damage", Some(Value::Number(2.0)))
        .unwrap();
    world.add_tag(a, "Hot").unwrap();
    world.destroy(b).unwrap();
    let edits = world.take_edits();
    assert_eq!(
        edits,
        [
            Edit::Created(b),
            Edit::Name {
                id: b,
                before: "Part".into()
            },
            Edit::Parent {
                id: b,
                before: None
            },
            Edit::Name {
                id: a,
                before: "A".into()
            },
            Edit::Parent {
                id: b,
                before: Some((workspace, 1))
            },
            Edit::Attribute {
                id: a,
                name: "Damage".into(),
                before: None
            },
            Edit::Tags {
                id: a,
                before: Vec::new()
            },
            // Destroying takes it out of the tree first.
            Edit::Parent {
                id: b,
                before: Some((workspace, 0))
            },
            Edit::Destroyed(b),
        ]
    );
    assert!(!world.has_edits(), "taking empties it");
    // A move to where it already is isn't an edit.
    world.set_parent_at(a, workspace, 0).unwrap();
    assert!(!world.has_edits());
}

#[test]
fn undoing_journalled_property_edits_newest_first_puts_metalness_back_after_material() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = part_in(&mut world, workspace, "A");
    world
        .set_known_property(a, "Metalness", Value::Number(0.25))
        .unwrap();
    world.record_edits();
    world
        .set_known_property(a, "Material", Value::Material(Material::Granite))
        .unwrap();
    for edit in world.take_edits().into_iter().rev() {
        if let Edit::Property { id, name, before } = edit {
            world.set_known_property(id, name, before).unwrap();
        }
    }
    let part = world.get(a).unwrap();
    assert_eq!(
        part.property("Material"),
        Some(&Value::Material(Material::Plastic))
    );
    assert_eq!(part.property("Metalness"), Some(&Value::Number(0.25)));
}

#[test]
fn a_clone_of_a_recording_world_records_its_own_edits_only() {
    let mut world = World::new();
    world.record_edits();
    let workspace = world.workspace();
    part_in(&mut world, workspace, "A");
    let mut copy = world.clone();
    assert!(copy.is_recording_edits() && !copy.has_edits());
    copy.stop_recording_edits();
    part_in(&mut copy, workspace, "B");
    assert!(!copy.has_edits());
    assert!(world.has_edits());
}

#[test]
fn a_replica_mode_world_makes_instances_at_ids_from_its_blocks_first() {
    let mut world = World::new();
    world.make_replica();
    let start = world.next_unused_index() + 10;
    world.add_id_block(start, 2);
    assert_eq!(world.ids_left(), 2);
    let a = world.create(ClassName::Part).unwrap();
    assert_eq!(a.to_bits(), u64::from(start));
    let taken = world.take_id().unwrap();
    assert_eq!(taken.to_bits(), u64::from(start + 1));
    assert_eq!(world.id_blocks_left(), []);
    // Out of blocks: IDs only this computer uses.
    assert!(world.create(ClassName::Part).unwrap().is_local());
    world.create_at(taken, ClassName::Part).unwrap();
    assert!(world.contains(taken));
}
