//! How much a step reads from the world, and what it leaves alone.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn height(world: &World, id: InstanceId) -> f32 {
    world
        .get(id)
        .and_then(|part| part.property("Position"))
        .and_then(Value::as_vector3)
        .unwrap()
        .y
}

#[test]
fn bodies_physics_moves_are_not_read_again() {
    let mut world = World::new();
    let falling = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(
        physics.sync_stats().parts_read,
        1,
        "the first step reads it"
    );

    for _ in 0..10 {
        let before = height(&world, falling);
        physics.step(&mut world, STEP_SECONDS);
        assert!(height(&world, falling) < before, "it should be falling");
        assert_eq!(physics.sync_stats().parts_read, 0);
    }
}

#[test]
fn a_script_moving_a_falling_part_is_still_noticed() {
    let mut world = World::new();
    let falling = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    for _ in 0..5 {
        physics.step(&mut world, STEP_SECONDS);
    }

    set(
        &mut world,
        falling,
        "Position",
        Value::Vector3(Vector3::new(0.0, 200.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(physics.sync_stats().parts_read, 1);
    assert!(height(&world, falling) > 190.0, "it was moved up");
}

#[test]
fn a_change_physics_does_not_care_about_is_read_once() {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 50.0, 0.0), true);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);

    world.set_name(block, "Renamed").unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(physics.sync_stats().parts_read, 1);
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(physics.sync_stats().parts_read, 0);
}

#[test]
fn a_mesh_part_is_read_only_when_it_or_its_model_changes() {
    let mut world = World::new();
    let mesh = world.create(ClassName::MeshPart).unwrap();
    world.set_parent(mesh, Some(world.workspace())).unwrap();
    set(&mut world, mesh, "Anchored", Value::Bool(true));
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(physics.sync_stats().parts_read, 0);
    set(
        &mut world,
        mesh,
        "Position",
        Value::Vector3(Vector3::new(1.0, 0.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(physics.sync_stats().parts_read, 1);
}

/// `count` anchored parts in a row, none touching.
fn row(world: &mut World, count: usize) -> Vec<InstanceId> {
    (0..count)
        .map(|i| part(world, Vector3::new(i as f32 * 10.0, 0.0, 0.0), true))
        .collect()
}

fn weld(world: &mut World, a: InstanceId, b: InstanceId) -> InstanceId {
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(world.workspace())).unwrap();
    set(world, weld, "Part0", Value::Instance(Some(a)));
    set(world, weld, "Part1", Value::Instance(Some(b)));
    weld
}

#[test]
fn reparenting_one_part_does_not_read_the_whole_world() {
    let mut world = World::new();
    let parts = row(&mut world, 200);
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert!(
        physics.sync_stats().full_sync,
        "the first step reads it all"
    );
    assert_eq!(physics.sync_stats().bodies_built, 200);

    // Within the Workspace: nothing about it changes.
    world.set_parent(parts[7], Some(model)).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (1, 0));
    assert_eq!(physics.body_count(), 200);

    // Out of it, with the Model it's in: its body goes.
    let storage = world.replicated_storage();
    world.set_parent(model, Some(storage)).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!physics.sync_stats().full_sync);
    assert!(!physics.has_body(parts[7]));
    assert_eq!(physics.body_count(), 199);

    // And back again.
    world.set_parent(model, Some(world.workspace())).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (1, 1));
    assert!(physics.has_body(parts[7]));
}

#[test]
fn a_new_part_and_a_destroyed_part_cost_only_themselves() {
    let mut world = World::new();
    let parts = row(&mut world, 200);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);

    let added = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    world.destroy(parts[3]).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (1, 1));
    assert!(physics.has_body(added));
    assert!(!physics.has_body(parts[3]));
    assert_eq!(physics.body_count(), 200);
}

#[test]
fn destroying_a_model_removes_the_parts_inside_it() {
    let mut world = World::new();
    let parts = row(&mut world, 20);
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let inner = world.create(ClassName::Model).unwrap();
    world.set_parent(inner, Some(model)).unwrap();
    world.set_parent(parts[0], Some(inner)).unwrap();
    world.set_parent(parts[1], Some(model)).unwrap();
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);

    world.destroy(model).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!physics.sync_stats().full_sync);
    assert!(!physics.has_body(parts[0]));
    assert!(!physics.has_body(parts[1]));
    assert_eq!(physics.body_count(), 18);
}

#[test]
fn a_weld_rebuilds_only_its_own_assembly() {
    let mut world = World::new();
    let parts = row(&mut world, 200);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);

    let joined = weld(&mut world, parts[1], parts[2]);
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (2, 1));
    assert_eq!(physics.parts[&parts[1]].body, physics.parts[&parts[2]].body);

    // A script moving a welded part builds that assembly again, alone.
    set(
        &mut world,
        parts[2],
        "Position",
        Value::Vector3(Vector3::new(20.0, 3.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (3, 1));

    world.destroy(joined).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    let stats = physics.sync_stats();
    assert!(!stats.full_sync);
    assert_eq!((stats.parts_read, stats.bodies_built), (2, 2));
    assert_ne!(physics.parts[&parts[1]].body, physics.parts[&parts[2]].body);
}

#[test]
fn taking_a_part_out_of_an_assembly_keeps_the_other_assemblies() {
    let mut world = World::new();
    let parts = row(&mut world, 10);
    weld(&mut world, parts[0], parts[1]);
    weld(&mut world, parts[1], parts[2]);
    weld(&mut world, parts[5], parts[6]);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    let other = physics.parts[&parts[5]].body;

    world.destroy(parts[1]).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!physics.sync_stats().full_sync);
    assert_ne!(physics.parts[&parts[0]].body, physics.parts[&parts[2]].body);
    assert_eq!(physics.parts[&parts[5]].body, other, "the other weld stays");
}

#[test]
fn too_many_moves_at_once_read_everything_again() {
    let mut world = World::new();
    row(&mut world, 3);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    row(&mut world, axcenity_core::WORKSPACE_MOVES_KEPT + 1);
    physics.step(&mut world, STEP_SECONDS);
    assert!(physics.sync_stats().full_sync);
    assert_eq!(
        physics.body_count(),
        axcenity_core::WORKSPACE_MOVES_KEPT + 4
    );
}

#[test]
fn a_new_collision_group_table_is_applied_to_every_part() {
    let mut world = World::new();
    row(&mut world, 3);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    world.set_collision_groups(axcenity_core::CollisionGroups::default());
    physics.step(&mut world, STEP_SECONDS);
    assert!(physics.sync_stats().full_sync);
}
