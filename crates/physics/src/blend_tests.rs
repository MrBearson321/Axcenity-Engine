//! Render interpolation (roadmap G3): see `blend.rs`.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    world
        .set_property(id, "Anchored", Value::Bool(anchored))
        .unwrap();
    id
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

/// A block dropped from 50 studs, with a second of game time stepped at
/// 60 frames a second so it's falling fast.
fn falling_block() -> (World, InstanceId, Physics) {
    let mut world = World::new();
    let block = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    for _ in 0..30 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    (world, block, physics)
}

#[test]
fn a_falling_part_blends_from_before_the_last_step_to_where_it_was_written() {
    let (mut world, block, mut physics) = falling_block();
    let before = position(&world, block);
    assert_eq!(physics.step(&mut world, 1.0 / 60.0), 1);
    let after = position(&world, block);
    assert!(
        after.y < before.y,
        "it should be falling: {before:?} {after:?}"
    );

    let blends = physics.blends(&[]);
    assert_eq!(blends.len(), 1);
    let blend = blends[0];
    assert_eq!(blend.part, block);
    assert_eq!(blend.to_position, after);
    assert!(
        (blend.from_position.y - before.y).abs() < 1e-4,
        "{blend:?} {before:?}"
    );
}

#[test]
fn the_share_of_a_step_left_over_says_how_far_along_it_is_drawn() {
    let (mut world, _, mut physics) = falling_block();
    // At 240 frames a second a step runs every fourth frame.
    let mut alphas = Vec::new();
    let mut steps = Vec::new();
    for _ in 0..8 {
        steps.push(physics.step(&mut world, 1.0 / 240.0));
        alphas.push(physics.blend_alpha());
    }
    assert_eq!(steps.iter().sum::<u32>(), 2, "{steps:?}");
    for (alpha, stepped) in alphas.iter().zip(&steps) {
        assert!((0.0..1.0).contains(alpha), "{alphas:?}");
        if *stepped == 1 {
            assert!(*alpha < 0.01, "a step leaves nothing over: {alphas:?}");
        }
    }
    // Between steps the share grows by a quarter a frame.
    let first = steps.iter().position(|&s| s == 1).unwrap();
    assert!((alphas[first + 1] - 0.25).abs() < 0.01, "{alphas:?}");
    assert!((alphas[first + 2] - 0.5).abs() < 0.01, "{alphas:?}");
}

#[test]
fn blends_are_kept_through_frames_without_a_step() {
    let (mut world, block, mut physics) = falling_block();
    physics.step(&mut world, 1.0 / 60.0);
    let kept = physics.blends(&[]);
    assert_eq!(physics.step(&mut world, 1.0 / 240.0), 0);
    assert_eq!(physics.blends(&[]), kept);
    assert_eq!(kept[0].part, block);
}

#[test]
fn with_several_steps_in_a_frame_the_blend_spans_only_the_last() {
    let (mut world, block, mut physics) = falling_block();
    // Two steps' worth of time, one step at a time, for the reference.
    let (mut reference, _, mut reference_physics) = falling_block();
    reference_physics.step(&mut reference, 1.0 / 60.0);
    let after_one = position(&reference, block);

    assert_eq!(physics.step(&mut world, 2.0 / 60.0), 2);
    let blend = physics.blends(&[])[0];
    assert!(
        (blend.from_position.y - after_one.y).abs() < 1e-4,
        "{blend:?} {after_one:?}"
    );
    assert_eq!(blend.to_position, position(&world, block));
}

#[test]
fn anchored_and_resting_parts_have_no_blend() {
    let mut world = World::new();
    let floor = part(&mut world, Vector3::new(0.0, -0.5, 0.0), true);
    world
        .set_property(floor, "Size", Value::Vector3(Vector3::new(40.0, 1.0, 40.0)))
        .unwrap();
    let block = part(&mut world, Vector3::new(0.0, 0.5, 0.0), false);
    let mut physics = Physics::new();
    for _ in 0..600 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let blends = physics.blends(&[]);
    assert!(
        blends
            .iter()
            .all(|blend| blend.part != floor && blend.part != block),
        "{blends:?}"
    );
}

#[test]
fn a_part_the_camera_follows_keeps_its_whole_assembly_stepped() {
    let mut world = World::new();
    let chassis = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let seat = part(&mut world, Vector3::new(0.0, 51.0, 0.0), false);
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(chassis)).unwrap();
    world
        .set_property(weld, "Part0", Value::Instance(Some(chassis)))
        .unwrap();
    world
        .set_property(weld, "Part1", Value::Instance(Some(seat)))
        .unwrap();
    let loose = part(&mut world, Vector3::new(20.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    for _ in 0..10 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let all: Vec<InstanceId> = physics.blends(&[]).iter().map(|b| b.part).collect();
    assert_eq!(all.len(), 3, "{all:?}");
    let kept: Vec<InstanceId> = physics.blends(&[seat]).iter().map(|b| b.part).collect();
    assert_eq!(kept, vec![loose]);
}

#[test]
fn turning_it_off_on_the_workspace_leaves_nothing_to_blend_and_moves_the_same() {
    let (mut world, block, mut physics) = falling_block();
    let (mut off, _, mut off_physics) = falling_block();
    let workspace = off.workspace();
    off.set_property(workspace, "PhysicsInterpolation", Value::Bool(false))
        .unwrap();
    for _ in 0..20 {
        physics.step(&mut world, 1.0 / 144.0);
        off_physics.step(&mut off, 1.0 / 144.0);
    }
    assert!(!physics.blends(&[]).is_empty());
    assert!(off_physics.blends(&[]).is_empty());
    assert_eq!(position(&world, block), position(&off, block));
}

#[test]
fn a_list_nobody_read_before_a_script_moved_the_part_is_forgotten() {
    let (mut world, block, mut physics) = falling_block();
    assert_eq!(physics.step(&mut world, 1.0 / 60.0), 1);
    world
        .set_property(
            block,
            "Position",
            Value::Vector3(Vector3::new(9.0, 80.0, 0.0)),
        )
        .unwrap();
    assert_eq!(physics.step(&mut world, 1.0 / 240.0), 0);
    assert!(physics.blends(&[]).is_empty());
}

/// How long listing the blends takes with 1,000 parts falling (S10k's
/// drops). Run with
/// `cargo test -p axcenity-physics --release blend_list_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn blend_list_cost() {
    use std::time::Instant;
    let mut world = World::new();
    for i in 0..1000 {
        let at = Vector3::new((i % 32) as f32 * 3.0, 50.0, (i / 32) as f32 * 3.0);
        part(&mut world, at, false);
    }
    let mut physics = Physics::new();
    physics.step(&mut world, 1.0 / 60.0);
    let (mut listing, frames) = (0.0, 60);
    for _ in 0..frames {
        physics.step(&mut world, 1.0 / 60.0);
        let started = Instant::now();
        let blends = physics.blends(&[]);
        listing += started.elapsed().as_secs_f64();
        assert_eq!(blends.len(), 1000);
    }
    println!(
        "listing 1,000 blends: {:.3} ms",
        listing * 1000.0 / frames as f64
    );
}
