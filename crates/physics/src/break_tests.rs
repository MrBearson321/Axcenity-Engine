//! Phase 42: joints and welds that break past their BreakForce or
//! BreakTorque.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, size: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

fn enabled(world: &World, id: InstanceId) -> bool {
    world.get(id).unwrap().property("Enabled") == Some(&Value::Bool(true))
}

/// Steps for `seconds`, collecting what broke.
fn run(physics: &mut Physics, world: &mut World, seconds: f64) -> Vec<InstanceId> {
    let mut broken = Vec::new();
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
        broken.extend(physics.take_broken());
    }
    broken
}

fn weld(world: &mut World, part0: InstanceId, part1: InstanceId, break_force: f64) -> InstanceId {
    let id = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(id, Some(part0)).unwrap();
    set(world, id, "Part0", Value::Instance(Some(part0)));
    set(world, id, "Part1", Value::Instance(Some(part1)));
    set(world, id, "BreakForce", Value::Number(break_force));
    id
}

/// A 1×1×1 block (mass 1, so it weighs 196.2) hanging from an anchored
/// beam on a ball socket that breaks past `break_force`.
fn hanging(world: &mut World, break_force: f64) -> (InstanceId, InstanceId) {
    let beam = part(world, v(0.0, 20.0, 0.0), v(4.0, 1.0, 1.0), true);
    let weight = part(world, v(0.0, 18.0, 0.0), v(1.0, 1.0, 1.0), false);
    let top = world.create(ClassName::Attachment).unwrap();
    world.set_parent(top, Some(beam)).unwrap();
    set(world, top, "Position", Value::Vector3(v(0.0, -0.5, 0.0)));
    let bottom = world.create(ClassName::Attachment).unwrap();
    world.set_parent(bottom, Some(weight)).unwrap();
    set(world, bottom, "Position", Value::Vector3(v(0.0, 1.5, 0.0)));
    let socket = world.create(ClassName::BallSocketConstraint).unwrap();
    world.set_parent(socket, Some(beam)).unwrap();
    set(world, socket, "Attachment0", Value::Instance(Some(top)));
    set(world, socket, "Attachment1", Value::Instance(Some(bottom)));
    set(world, socket, "BreakForce", Value::Number(break_force));
    (socket, weight)
}

#[test]
fn a_joint_holds_a_weight_below_its_break_force() {
    let mut world = World::new();
    let (socket, weight) = hanging(&mut world, 260.0);
    let mut physics = Physics::new();
    let broken = run(&mut physics, &mut world, 2.0);
    assert!(broken.is_empty(), "{broken:?}");
    assert!(enabled(&world, socket));
    assert!((position(&world, weight).y - 18.0).abs() < 0.2);
}

#[test]
fn a_joint_breaks_under_a_weight_past_its_break_force_once() {
    let mut world = World::new();
    let (socket, weight) = hanging(&mut world, 150.0);
    let mut physics = Physics::new();
    let broken = run(&mut physics, &mut world, 2.0);
    assert_eq!(broken, vec![socket]);
    assert!(!enabled(&world, socket));
    // It fell.
    assert!(position(&world, weight).y < 10.0);
}

/// An anchored wall with a 4×0.4×3 shelf (weight 941.76) welded to its
/// side by a weld that breaks past 2500.
fn shelf(world: &mut World) -> (InstanceId, InstanceId) {
    let wall = part(world, v(0.0, 4.0, 0.0), v(1.0, 8.0, 8.0), true);
    let shelf = part(world, v(2.5, 4.0, 0.0), v(4.0, 0.4, 3.0), false);
    (weld(world, wall, shelf, 2500.0), shelf)
}

/// A crate of `size` dropped onto the shelf from a stud above it.
fn drop_crate(world: &mut World, size: f32) -> InstanceId {
    part(
        world,
        v(2.5, 4.2 + size / 2.0 + 1.0, 0.0),
        v(size, size, size),
        false,
    )
}

#[test]
fn a_welded_shelf_holds_a_light_crate_dropped_on_it() {
    let mut world = World::new();
    let (weld, shelf) = shelf(&mut world);
    let mut physics = Physics::new();
    // Weighs 196.2: shelf and crate are well under 2500.
    let light = drop_crate(&mut world, 1.0);
    let broken = run(&mut physics, &mut world, 3.0);
    assert!(broken.is_empty(), "{broken:?}");
    assert!(enabled(&world, weld));
    assert!((position(&world, shelf).y - 4.0).abs() < 0.1);
    assert!(position(&world, light).y > 4.0);
}

#[test]
fn a_welded_shelf_breaks_under_a_heavy_crate_once() {
    let mut world = World::new();
    let (weld, shelf) = shelf(&mut world);
    let mut physics = Physics::new();
    assert!(run(&mut physics, &mut world, 1.0).is_empty());
    // Weighs 3 × 3 × 3 × 196.2 = 5297.4.
    drop_crate(&mut world, 3.0);
    let broken = run(&mut physics, &mut world, 3.0);
    assert_eq!(broken, vec![weld]);
    assert!(!enabled(&world, weld));
    assert!(position(&world, shelf).y < 2.0);
}

#[test]
fn a_weld_breaks_past_its_break_torque_measured_between_its_parts() {
    // About the weld's middle (x = 1.25) the shelf's weight turns it by
    // 941.76 × 1.25 = 1177; a light crate on it adds 245, a heavy one 6622.
    let shelf_with = |size: f32| {
        let mut world = World::new();
        let (weld, _) = shelf(&mut world);
        set(&mut world, weld, "BreakForce", Value::Number(0.0));
        set(&mut world, weld, "BreakTorque", Value::Number(6000.0));
        drop_crate(&mut world, size);
        let mut physics = Physics::new();
        run(&mut physics, &mut world, 3.0)
    };
    assert!(shelf_with(1.0).is_empty());
    assert_eq!(shelf_with(3.0).len(), 1);
}

#[test]
fn a_weld_that_can_break_is_not_part_of_an_assembly() {
    let mut world = World::new();
    let (weld, shelf) = shelf(&mut world);
    let wall = world.get(weld).unwrap().parent().unwrap();
    let mut physics = Physics::new();
    physics.step(&mut world, 1.0 / 60.0);
    assert_ne!(physics.parts[&wall].body, physics.parts[&shelf].body);
    assert!(assembly::welds(&world).is_empty());
    // With no BreakForce it's an ordinary weld again.
    set(&mut world, weld, "BreakForce", Value::Number(0.0));
    physics.step(&mut world, 1.0 / 60.0);
    assert_eq!(physics.parts[&wall].body, physics.parts[&shelf].body);
}
