use axcenity_core::{ClassName, InstanceId, PartType, Value, Vector3, World};

use super::*;

/// A world with an anchored 40×1×40 floor whose top is at Y = 0.
fn world_with_floor() -> (World, InstanceId) {
    let mut world = World::new();
    let floor = part(&mut world, Vector3::new(0.0, -0.5, 0.0), true);
    set(&mut world, floor, "Size", vector(40.0, 1.0, 40.0));
    (world, floor)
}

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

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

/// Steps physics for `seconds` of game time at `fps` frames per second.
fn run(physics: &mut Physics, world: &mut World, seconds: f64, fps: f64) -> u32 {
    let frames = (seconds * fps).round() as u32;
    (0..frames).map(|_| physics.step(world, 1.0 / fps)).sum()
}

#[test]
fn unanchored_part_falls_and_comes_to_rest_on_an_anchored_floor() {
    let (mut world, floor) = world_with_floor();
    // A 4×1×2 block (the default size), dropped from 10 studs up.
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let mut physics = Physics::new();

    run(&mut physics, &mut world, 0.25, 60.0);
    assert!(position(&world, block).y < 9.0, "it should be falling");

    run(&mut physics, &mut world, 3.0, 60.0);
    let resting = position(&world, block);
    // Its bottom (half its 1-stud height below the center) is on the floor.
    assert!((resting.y - 0.5).abs() < 0.05, "{resting:?}");
    run(&mut physics, &mut world, 1.0, 60.0);
    assert_eq!(position(&world, block), resting, "it should stay at rest");
    assert_eq!(position(&world, floor), Vector3::new(0.0, -0.5, 0.0));
}

#[test]
fn moves_the_same_at_30_and_144_frames_per_second() {
    let simulate = |fps: f64| {
        let (mut world, _) = world_with_floor();
        let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
        set(&mut world, block, "Orientation", vector(20.0, 0.0, 35.0));
        let ball = part(&mut world, Vector3::new(1.0, 14.0, 0.5), false);
        set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
        set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
        let mut physics = Physics::new();
        let steps = run(&mut physics, &mut world, 2.0, fps);
        (steps, position(&world, block), position(&world, ball))
    };

    let at_30 = simulate(30.0);
    let at_144 = simulate(144.0);
    // 2 seconds is always 120 fixed steps, so the results match exactly.
    assert_eq!(at_30.0, 120);
    assert_eq!(at_30, at_144);
}

#[test]
fn runs_a_step_only_once_a_whole_step_of_time_has_passed() {
    let mut world = World::new();
    let mut physics = Physics::new();
    // At 144 fps most frames run no step, and some run one.
    let steps: Vec<u32> = (0..5)
        .map(|_| physics.step(&mut world, 1.0 / 144.0))
        .collect();
    assert_eq!(steps, [0, 0, 1, 0, 1]);
    // A slow frame runs several steps to catch up.
    assert_eq!(physics.step(&mut world, 0.1), 6);
    // Bad times are ignored.
    assert_eq!(physics.step(&mut world, -1.0), 0);
    assert_eq!(physics.step(&mut world, f64::NAN), 0);
}

#[test]
fn a_huge_frame_runs_at_most_the_step_limit() {
    let mut world = World::new();
    let mut physics = Physics::new();
    assert_eq!(physics.step(&mut world, 60.0), MAX_STEPS_PER_FRAME);
    // The rest was dropped, not saved up for later frames.
    assert_eq!(physics.step(&mut world, 0.0), 0);
}

#[test]
fn anchored_parts_do_not_fall() {
    let mut world = World::new();
    let floating = part(&mut world, Vector3::new(0.0, 10.0, 0.0), true);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0, 60.0);
    assert_eq!(position(&world, floating), Vector3::new(0.0, 10.0, 0.0));
}

#[test]
fn toggling_anchored_freezes_and_releases_a_part() {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 100.0, 0.0), true);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5, 60.0);
    assert_eq!(position(&world, block).y, 100.0);

    set(&mut world, block, "Anchored", Value::Bool(false));
    run(&mut physics, &mut world, 0.5, 60.0);
    let frozen_at = position(&world, block);
    assert!(
        frozen_at.y < 90.0,
        "released, it should fall: {frozen_at:?}"
    );

    set(&mut world, block, "Anchored", Value::Bool(true));
    run(&mut physics, &mut world, 0.5, 60.0);
    assert_eq!(
        position(&world, block),
        frozen_at,
        "anchored, it should stop"
    );

    // Released again, it starts falling from rest, not at its old speed.
    set(&mut world, block, "Anchored", Value::Bool(false));
    run(&mut physics, &mut world, 1.0 / 60.0, 60.0);
    let dropped = frozen_at.y - position(&world, block).y;
    assert!(dropped > 0.0 && dropped < 0.2, "{dropped}");
}

#[test]
fn a_destroyed_part_is_removed_from_the_physics_world() {
    let (mut world, floor) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 0.5, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0, 60.0);
    assert_eq!(physics.body_count(), 2);

    world.destroy(floor).unwrap();
    run(&mut physics, &mut world, 0.5, 60.0);
    assert!(!physics.has_body(floor));
    assert_eq!(physics.body_count(), 1);
    // The block was asleep on the floor; with the floor gone it falls.
    assert!(position(&world, block).y < -5.0);
}

#[test]
fn a_part_taken_out_of_the_workspace_leaves_physics_and_can_come_back() {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert!(physics.has_body(block));

    world.set_parent(block, None).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!physics.has_body(block));

    world.set_parent(block, Some(world.workspace())).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(physics.has_body(block));
}

#[test]
fn setting_position_teleports_a_falling_part_and_stops_it() {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 100.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0, 60.0);
    assert!(position(&world, block).y < 50.0);

    set(&mut world, block, "Position", vector(5.0, 100.0, 0.0));
    physics.step(&mut world, STEP_SECONDS);
    let after = position(&world, block);
    assert_eq!(after.x, 5.0);
    // One step of falling from rest, not at the speed it had before.
    assert!(after.y < 100.0 && after.y > 99.8, "{after:?}");
}

#[test]
fn resizing_a_part_changes_how_it_collides() {
    let (mut world, _) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 5.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0, 60.0);
    assert!((position(&world, block).y - 0.5).abs() < 0.05);

    set(&mut world, block, "Size", vector(4.0, 3.0, 2.0));
    set(&mut world, block, "Position", vector(0.0, 5.0, 0.0));
    run(&mut physics, &mut world, 2.0, 60.0);
    let y = position(&world, block).y;
    assert!((y - 1.5).abs() < 0.05, "{y}");
}

#[test]
fn balls_roll_down_a_wedge() {
    let (mut world, _) = world_with_floor();
    let ramp = part(&mut world, Vector3::new(0.0, 2.0, 0.0), true);
    set(&mut world, ramp, "Shape", Value::PartType(PartType::Wedge));
    set(&mut world, ramp, "Size", vector(4.0, 4.0, 8.0));
    let ball = part(&mut world, Vector3::new(0.0, 6.0, 0.0), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(1.0, 1.0, 1.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.5, 60.0);
    // The slope falls towards -Z, so the ball rolls off the front.
    assert!(
        position(&world, ball).z < -4.0,
        "{:?}",
        position(&world, ball)
    );
}

#[test]
fn parts_with_invalid_numbers_are_left_out() {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    set(&mut world, block, "Position", vector(f32::NAN, 0.0, 0.0));
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!physics.has_body(block));
}

#[test]
fn orientation_survives_the_trip_through_a_rotation() {
    for degrees in [
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(30.0, 45.0, 60.0),
        Vector3::new(-80.0, 170.0, -10.0),
    ] {
        let back = quat_to_orientation(orientation_to_quat(degrees));
        let close = (back.x - degrees.x).abs() < 1e-3
            && (back.y - degrees.y).abs() < 1e-3
            && (back.z - degrees.z).abs() < 1e-3;
        assert!(close, "{degrees:?} came back as {back:?}");
    }
}
