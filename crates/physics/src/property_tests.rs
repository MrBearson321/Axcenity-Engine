//! The per-part properties that change how a body moves (cohesion pass
//! CW6.2), and what stepping costs with many bodies.

use std::time::Instant;

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

fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn number(value: f64) -> Value {
    Value::Number(value)
}

fn get(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

/// A world without gravity, so only drag changes how things move.
fn weightless_world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", number(0.0));
    world
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds / STEP_SECONDS).round() as usize {
        physics.step(world, STEP_SECONDS);
    }
}

#[test]
fn linear_drag_slows_a_thrown_part_and_none_keeps_its_speed() {
    let mut world = weightless_world();
    let free = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let dragged = part(&mut world, Vector3::new(0.0, 10.0, 20.0), false);
    for id in [free, dragged] {
        set(
            &mut world,
            id,
            "AssemblyLinearVelocity",
            vector(20.0, 0.0, 0.0),
        );
    }
    set(&mut world, dragged, "LinearDrag", number(1.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let free_speed = get(&world, free, "AssemblyLinearVelocity").x;
    let dragged_speed = get(&world, dragged, "AssemblyLinearVelocity").x;
    assert!((free_speed - 20.0).abs() < 0.01, "{free_speed}");
    // Divided by 1 + 1/60 sixty times: about 37% of it is left.
    assert!((dragged_speed - 7.4).abs() < 0.3, "{dragged_speed}");
}

#[test]
fn angular_drag_slows_spin_and_zero_lets_it_spin_on() {
    let mut world = weightless_world();
    let default = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let none = part(&mut world, Vector3::new(0.0, 10.0, 20.0), false);
    for id in [default, none] {
        set(
            &mut world,
            id,
            "AssemblyAngularVelocity",
            vector(0.0, 5.0, 0.0),
        );
    }
    set(&mut world, none, "AngularDrag", number(0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let slowed = get(&world, default, "AssemblyAngularVelocity").y;
    let spinning = get(&world, none, "AssemblyAngularVelocity").y;
    assert!((spinning - 5.0).abs() < 0.01, "{spinning}");
    // The default 0.5, as every body had before it was a property.
    assert!((slowed - 3.03).abs() < 0.1, "{slowed}");
}

#[test]
fn drag_set_while_running_takes_effect_and_bad_numbers_are_ignored() {
    let mut world = weightless_world();
    let id = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    set(
        &mut world,
        id,
        "AssemblyLinearVelocity",
        vector(20.0, 0.0, 0.0),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    set(&mut world, id, "LinearDrag", number(f64::NAN));
    run(&mut physics, &mut world, 0.5);
    assert!((get(&world, id, "AssemblyLinearVelocity").x - 20.0).abs() < 0.01);
    set(&mut world, id, "LinearDrag", number(3.0));
    run(&mut physics, &mut world, 0.5);
    assert!(get(&world, id, "AssemblyLinearVelocity").x < 5.0);
}

/// A ball rolling along a floor, after `seconds`: how far it went.
fn roll_ball(rolling: f64, seconds: f64) -> f32 {
    let mut world = World::new();
    let floor = part(&mut world, Vector3::new(0.0, -0.5, 0.0), true);
    set(&mut world, floor, "Size", vector(400.0, 1.0, 400.0));
    let ball = part(&mut world, Vector3::new(-150.0, 1.0, 0.0), false);
    set(
        &mut world,
        ball,
        "Shape",
        Value::PartType(axcenity_core::PartType::Ball),
    );
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    set(&mut world, ball, "AngularDrag", number(0.0));
    set(&mut world, ball, "RollingFriction", number(rolling));
    set(
        &mut world,
        ball,
        "AssemblyLinearVelocity",
        vector(30.0, 0.0, 0.0),
    );
    set(
        &mut world,
        ball,
        "AssemblyAngularVelocity",
        vector(0.0, 0.0, -30.0),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, seconds);
    get(&world, ball, "Position").x + 150.0
}

#[test]
fn rolling_friction_stops_a_ball_on_the_ground() {
    let free = roll_ball(0.0, 3.0);
    let braked = roll_ball(3.0, 3.0);
    assert!(free > 60.0, "a free ball rolls on: {free}");
    assert!(braked < free * 0.6, "{braked} against {free}");
}

#[test]
fn rolling_friction_does_nothing_in_the_air() {
    let mut world = weightless_world();
    let id = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    set(&mut world, id, "AngularDrag", number(0.0));
    set(&mut world, id, "RollingFriction", number(5.0));
    set(
        &mut world,
        id,
        "AssemblyAngularVelocity",
        vector(0.0, 5.0, 0.0),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    assert!((get(&world, id, "AssemblyAngularVelocity").y - 5.0).abs() < 0.01);
}

/// How long a step takes with `count` blocks dropped in a grid onto a
/// floor: the median of 240 frames after 60 to settle in. Run with
/// `cargo test -p axcenity-physics --release step_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn step_cost() {
    for count in [1_000, 5_000, 10_000] {
        let mut world = World::new();
        let side = (count as f32).sqrt().ceil() as usize;
        let floor = part(&mut world, Vector3::new(0.0, -0.5, 0.0), true);
        let width = side as f32 * 3.0 + 10.0;
        set(&mut world, floor, "Size", vector(width, 1.0, width));
        for i in 0..count {
            let at = Vector3::new(
                (i % side) as f32 * 3.0 - width / 2.0 + 5.0,
                2.0 + (i % 7) as f32 * 3.0,
                (i / side) as f32 * 3.0 - width / 2.0 + 5.0,
            );
            part(&mut world, at, false);
        }
        let mut physics = Physics::new();
        for _ in 0..60 {
            physics.step(&mut world, STEP_SECONDS);
        }
        let mut times: Vec<f64> = (0..240)
            .map(|_| {
                let started = Instant::now();
                physics.step(&mut world, STEP_SECONDS);
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        times.sort_by(f64::total_cmp);
        println!(
            "{count} bodies: median {:.2} ms, 95th percentile {:.2} ms",
            times[times.len() / 2],
            times[times.len() * 95 / 100]
        );
    }
}
