//! Phase 35: welded parts move as one assembly.

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

fn weld(world: &mut World, part0: InstanceId, part1: InstanceId) -> InstanceId {
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(part1)).unwrap();
    set(world, weld, "Part0", Value::Instance(Some(part0)));
    set(world, weld, "Part1", Value::Instance(Some(part1)));
    weld
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn property(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

/// Where `part` is, seen from `from`'s own frame.
fn relative(world: &World, from: InstanceId, part: InstanceId) -> (Vec3, Quat) {
    let pose = |id| {
        (
            to_vec3(property(world, id, "Position")),
            orientation_to_quat(property(world, id, "Orientation")),
        )
    };
    let ((at, turn), (other_at, other_turn)) = (pose(from), pose(part));
    (
        turn.inverse() * (other_at - at),
        turn.inverse() * other_turn,
    )
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
    }
}

/// A floor whose top is at Y = 0, and a table of five parts welded to its
/// top, held `height` up. The top first, then the legs.
fn table(world: &mut World, height: f32) -> Vec<InstanceId> {
    part(world, v(0.0, -0.5, 0.0), v(60.0, 1.0, 60.0), true);
    let top = part(world, v(0.0, height + 3.25, 0.0), v(6.0, 0.5, 4.0), false);
    let mut parts = vec![top];
    for (x, z) in [(-2.5, -1.5), (2.5, -1.5), (-2.5, 1.5), (2.5, 1.5)] {
        let leg = part(world, v(x, height + 1.5, z), v(0.5, 3.0, 0.5), false);
        weld(world, top, leg);
        parts.push(leg);
    }
    parts
}

#[test]
fn a_table_of_five_welded_parts_falls_and_lands_as_one_piece() {
    let mut world = World::new();
    let parts = table(&mut world, 8.0);
    let top = parts[0];
    let legs_before: Vec<_> = parts[1..]
        .iter()
        .map(|&leg| relative(&world, top, leg))
        .collect();
    let mut physics = Physics::new();

    run(&mut physics, &mut world, 0.15);
    assert!(
        property(&world, top, "Position").y < 10.5,
        "the table should be falling"
    );
    // Falling, every part moves at the assembly's speed.
    let speed = property(&world, top, "AssemblyLinearVelocity");
    assert!(speed.y < -20.0, "{speed:?}");
    for &leg in &parts[1..] {
        assert_eq!(property(&world, leg, "AssemblyLinearVelocity"), speed);
    }

    run(&mut physics, &mut world, 3.0);
    // It stands on its four legs: their bottoms on the floor, the top
    // 3 studs up.
    for &leg in &parts[1..] {
        let at = property(&world, leg, "Position");
        assert!((at.y - 1.5).abs() < 0.05, "a leg is at {at:?}");
    }
    let at = property(&world, top, "Position");
    assert!((at.y - 3.25).abs() < 0.05, "the top is at {at:?}");
    // And no part moved against the others on the way.
    for (&leg, (offset, turn)) in parts[1..].iter().zip(legs_before) {
        let (now_offset, now_turn) = relative(&world, top, leg);
        assert!(
            now_offset.abs_diff_eq(offset, 0.01),
            "{now_offset} vs {offset}"
        );
        assert!(now_turn.angle_between(turn) < 0.01);
    }
    // Five parts, one body.
    let bodies: std::collections::HashSet<_> =
        parts.iter().map(|id| physics.parts[id].body).collect();
    assert_eq!(bodies.len(), 1);
}

#[test]
fn disabling_a_weld_lets_the_parts_fall_apart_and_enabling_welds_them_where_they_are() {
    let mut world = World::new();
    part(&mut world, v(0.0, -0.5, 0.0), v(60.0, 1.0, 60.0), true);
    // An anchored beam with a block welded under it.
    let beam = part(&mut world, v(0.0, 20.0, 0.0), v(8.0, 1.0, 2.0), true);
    let block = part(&mut world, v(0.0, 18.5, 0.0), v(2.0, 2.0, 2.0), false);
    let weld = weld(&mut world, beam, block);
    let mut physics = Physics::new();

    run(&mut physics, &mut world, 1.0);
    assert_eq!(property(&world, block, "Position"), v(0.0, 18.5, 0.0));

    set(&mut world, weld, "Enabled", Value::Bool(false));
    run(&mut physics, &mut world, 0.4);
    let falling = property(&world, block, "Position");
    assert!(
        falling.y < 15.0,
        "it should fall once unwelded: {falling:?}"
    );
    assert_eq!(property(&world, beam, "Position"), v(0.0, 20.0, 0.0));

    // Welded again in mid-air, to the anchored beam: it stops there.
    set(&mut world, weld, "Enabled", Value::Bool(true));
    run(&mut physics, &mut world, 1.0 / 60.0);
    let held = property(&world, block, "Position");
    assert!((held.y - falling.y).abs() < 0.01, "{held:?} vs {falling:?}");
    run(&mut physics, &mut world, 1.0);
    assert_eq!(property(&world, block, "Position"), held);
    assert_eq!(
        property(&world, block, "AssemblyLinearVelocity"),
        Vector3::default()
    );
}

#[test]
fn anchoring_one_part_of_a_welded_assembly_holds_the_whole_assembly_still() {
    let mut world = World::new();
    let parts = table(&mut world, 8.0);
    let leg = parts[2];
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.2);

    set(&mut world, leg, "Anchored", Value::Bool(true));
    run(&mut physics, &mut world, 1.0 / 60.0);
    let held: Vec<_> = parts
        .iter()
        .map(|&id| property(&world, id, "Position"))
        .collect();
    run(&mut physics, &mut world, 1.0);
    for (&id, at) in parts.iter().zip(&held) {
        assert_eq!(property(&world, id, "Position"), *at);
        assert_eq!(
            property(&world, id, "AssemblyLinearVelocity"),
            Vector3::default()
        );
    }

    // Released, the whole table falls again.
    set(&mut world, leg, "Anchored", Value::Bool(false));
    run(&mut physics, &mut world, 0.3);
    for (&id, at) in parts.iter().zip(&held) {
        assert!(property(&world, id, "Position").y < at.y - 1.0);
    }
}

#[test]
fn welded_parts_never_touch_each_other() {
    let mut world = World::new();
    let parts = table(&mut world, 0.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    let touches = physics.take_touches();
    assert!(
        touches
            .iter()
            .all(|(a, b)| !(parts.contains(a) && parts.contains(b))),
        "{touches:?}"
    );
}

#[test]
fn moving_a_welded_part_moves_it_alone_and_the_weld_holds_it_there() {
    let mut world = World::new();
    let beam = part(&mut world, v(0.0, 20.0, 0.0), v(8.0, 1.0, 2.0), true);
    let block = part(&mut world, v(0.0, 18.5, 0.0), v(2.0, 2.0, 2.0), false);
    weld(&mut world, beam, block);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.1);

    set(
        &mut world,
        block,
        "Position",
        Value::Vector3(v(3.0, 18.5, 0.0)),
    );
    run(&mut physics, &mut world, 1.0);
    assert_eq!(property(&world, block, "Position"), v(3.0, 18.5, 0.0));
    assert_eq!(property(&world, beam, "Position"), v(0.0, 20.0, 0.0));
}

#[test]
fn the_root_is_the_anchored_part_then_the_largest() {
    let mut world = World::new();
    let small = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), false);
    let large = part(&mut world, v(0.0, 6.0, 0.0), v(4.0, 1.0, 4.0), false);
    let middle = part(&mut world, v(0.0, 7.0, 0.0), v(2.0, 1.0, 2.0), false);
    let lone = part(&mut world, v(9.0, 7.0, 0.0), v(2.0, 1.0, 2.0), false);
    weld(&mut world, small, large);
    let second = weld(&mut world, middle, large);

    assert_eq!(assembly::root_part(&world, small), large);
    assert_eq!(assembly::root_part(&world, lone), lone);
    set(&mut world, middle, "Anchored", Value::Bool(true));
    assert_eq!(assembly::root_part(&world, small), middle);
    // Massless parts are never the root unless they're anchored.
    set(&mut world, middle, "Anchored", Value::Bool(false));
    set(&mut world, large, "Massless", Value::Bool(true));
    assert_eq!(assembly::root_part(&world, small), middle);

    // Directly welded parts, or the whole assembly, the part itself first.
    assert_eq!(
        assembly::connected_parts(&world, small, false),
        vec![small, large]
    );
    assert_eq!(
        assembly::connected_parts(&world, small, true),
        vec![small, large, middle]
    );
    set(&mut world, second, "Enabled", Value::Bool(false));
    assert_eq!(
        assembly::connected_parts(&world, small, true),
        vec![small, large]
    );
    assert_eq!(assembly::connected_parts(&world, lone, true), vec![lone]);
}

#[test]
fn an_assembly_weighs_its_parts_but_not_massless_ones() {
    let mut world = World::new();
    let a = part(&mut world, v(0.0, 5.0, 0.0), v(2.0, 2.0, 2.0), false);
    let b = part(&mut world, v(4.0, 5.0, 0.0), v(2.0, 2.0, 2.0), false);
    let assets = Assets::default();
    let alone = Mass::of_assembly(&world, &assets, a).unwrap();
    assert_eq!(alone.mass, 8.0);

    weld(&mut world, a, b);
    let both = Mass::of_assembly(&world, &assets, b).unwrap();
    assert!((both.mass - 16.0).abs() < 1e-4);
    assert!((both.center.x - 2.0).abs() < 1e-4, "{:?}", both.center);
    assert_eq!(Mass::of(&world, &assets, b).unwrap().mass, 8.0);

    set(&mut world, b, "Massless", Value::Bool(true));
    let light = Mass::of_assembly(&world, &assets, b).unwrap();
    assert!((light.mass - 8.0).abs() < 1e-4);
}
