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
