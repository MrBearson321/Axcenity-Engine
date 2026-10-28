//! Phase 38: collision groups, CanTouch, TouchEnded and scripts' queries.

use axcenity_core::{CFrame, ClassName, InstanceId, Value, Vector3, World};

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

fn group(world: &mut World, id: InstanceId, name: &str) {
    set(world, id, "CollisionGroup", Value::String(name.to_owned()));
}

fn y(world: &World, id: InstanceId) -> f32 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
        .y
}

fn floor(world: &mut World) -> InstanceId {
    part(
        world,
        Vector3::new(0.0, -0.5, 0.0),
        Vector3::new(40.0, 1.0, 40.0),
        true,
    )
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

/// Pairs of parts, as physics reports touches.
type Pairs = Vec<(InstanceId, InstanceId)>;

/// Runs `seconds` at 60 frames a second, collecting touches and ends.
fn run(physics: &mut Physics, world: &mut World, seconds: f64) -> (Pairs, Pairs) {
    let (mut touches, mut ends) = (Vec::new(), Vec::new());
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
        touches.extend(physics.take_touches());
        ends.extend(physics.take_touch_ends());
    }
    (touches, ends)
}

#[test]
fn parts_in_groups_set_not_to_collide_fall_through_each_other_but_not_the_floor() {
    let mut world = World::new();
    floor(&mut world);
    // A crate resting on a ledge in "NPCs"; a block in "Players" dropped on it.
    let ledge = part(&mut world, v(0.0, 2.0, 0.0), v(4.0, 1.0, 4.0), true);
    let block = part(&mut world, v(0.0, 6.0, 0.0), v(2.0, 1.0, 2.0), false);
    let groups = world.collision_groups_mut();
    groups.register("Players").unwrap();
    groups.register("NPCs").unwrap();
    groups.set_collidable("Players", "NPCs", false).unwrap();
    group(&mut world, ledge, "NPCs");
    group(&mut world, block, "Players");
    let mut physics = Physics::new();
    let (touches, _) = run(&mut physics, &mut world, 2.0);
    // It passed the ledge and landed on the floor (Default collides).
    assert!(
        (y(&world, block) - 0.5).abs() < 0.05,
        "{}",
        y(&world, block)
    );
    assert!(!touches.iter().any(|&(a, b)| a == ledge || b == ledge));

    // Collidable again, a block dropped onto the ledge stays on it.
    world
        .collision_groups_mut()
        .set_collidable("Players", "NPCs", true)
        .unwrap();
    let second = part(&mut world, v(0.0, 6.0, 0.0), v(2.0, 1.0, 2.0), false);
    group(&mut world, second, "Players");
    run(&mut physics, &mut world, 2.0);
    assert!(
        (y(&world, second) - 3.0).abs() < 0.05,
        "{}",
        y(&world, second)
    );
}

#[test]
fn touch_ended_fires_once_when_a_part_leaves_another() {
    let mut world = World::new();
    let floor = floor(&mut world);
    let block = part(&mut world, v(0.0, 3.0, 0.0), v(2.0, 1.0, 2.0), false);
    let mut physics = Physics::new();
    let (touches, ends) = run(&mut physics, &mut world, 1.0);
    assert_eq!(touches, vec![(floor.min(block), floor.max(block))]);
    assert!(ends.is_empty(), "{ends:?}");

    // Thrown up, it leaves the floor: one TouchEnded, then it lands again.
    set(
        &mut world,
        block,
        "AssemblyLinearVelocity",
        Value::Vector3(v(0.0, 60.0, 0.0)),
    );
    let (touches, ends) = run(&mut physics, &mut world, 0.3);
    assert_eq!(ends, vec![(floor.min(block), floor.max(block))]);
    assert!(touches.is_empty(), "{touches:?}");
    // It lands again, maybe bouncing: each touch but the last one ends once.
    let (touches, ends) = run(&mut physics, &mut world, 2.0);
    assert!(!touches.is_empty());
    assert_eq!(ends.len(), touches.len() - 1, "{touches:?} {ends:?}");
}

#[test]
fn a_part_with_can_touch_off_touches_nothing_and_ends_its_touches() {
    let mut world = World::new();
    floor(&mut world);
    let block = part(&mut world, v(0.0, 3.0, 0.0), v(2.0, 1.0, 2.0), false);
    let mut physics = Physics::new();
    let (touches, _) = run(&mut physics, &mut world, 1.0);
    assert_eq!(touches.len(), 1);
    set(&mut world, block, "CanTouch", Value::Bool(false));
    let (touches, ends) = run(&mut physics, &mut world, 0.1);
    assert_eq!(ends.len(), 1, "it stops touching at once");
    assert!(touches.is_empty());
    // It still rests on the floor: CanTouch isn't CanCollide.
    assert!((y(&world, block) - 0.5).abs() < 0.05);

    let falling = part(&mut world, v(5.0, 3.0, 0.0), v(2.0, 1.0, 2.0), false);
    set(&mut world, falling, "CanTouch", Value::Bool(false));
    let (touches, _) = run(&mut physics, &mut world, 1.0);
    assert!(touches.is_empty(), "{touches:?}");
}

/// A character's body, a wall behind it and the floor: a ray from the
/// shooter's eyes through its own body.
#[test]
fn a_raycast_that_excludes_the_shooter_hits_the_wall_behind_it() {
    let mut world = World::new();
    floor(&mut world);
    let character = world.create(ClassName::Model).unwrap();
    world
        .set_parent(character, Some(world.workspace()))
        .unwrap();
    let torso = part(&mut world, v(0.0, 3.0, 0.0), v(2.0, 2.0, 1.0), true);
    world.set_parent(torso, Some(character)).unwrap();
    let wall = part(&mut world, v(0.0, 3.0, -10.0), v(10.0, 6.0, 1.0), true);
    let origin = v(0.0, 3.0, 2.0);
    let direction = v(0.0, 0.0, -50.0);

    let unfiltered = raycast(
        &world,
        &Assets::default(),
        origin,
        direction,
        &QueryParams::default(),
    );
    assert_eq!(unfiltered.unwrap().instance, torso);

    let params = QueryParams {
        filter: vec![character],
        ..QueryParams::default()
    };
    let hit = raycast(&world, &Assets::default(), origin, direction, &params).unwrap();
    assert_eq!(hit.instance, wall);
    assert!((hit.position.z + 9.5).abs() < 1e-3, "{hit:?}");
    assert_eq!(hit.normal, v(0.0, 0.0, 1.0));

    // Include finds only what's listed.
    let only_wall = QueryParams {
        filter: vec![wall],
        include: true,
        ..QueryParams::default()
    };
    let hit = raycast(&world, &Assets::default(), origin, direction, &only_wall).unwrap();
    assert_eq!(hit.instance, wall);
}

#[test]
fn queries_respect_can_collide_can_query_and_collision_groups() {
    let mut world = World::new();
    let ghost = part(&mut world, v(0.0, 0.0, -3.0), v(4.0, 4.0, 1.0), true);
    let glass = part(&mut world, v(0.0, 0.0, -6.0), v(4.0, 4.0, 1.0), true);
    let wall = part(&mut world, v(0.0, 0.0, -9.0), v(4.0, 4.0, 1.0), true);
    set(&mut world, ghost, "CanCollide", Value::Bool(false));
    set(&mut world, glass, "CanQuery", Value::Bool(false));
    world.collision_groups_mut().register("Bullets").unwrap();
    world.collision_groups_mut().register("Walls").unwrap();
    world
        .collision_groups_mut()
        .set_collidable("Bullets", "Walls", false)
        .unwrap();
    let assets = Assets::default();
    let cast = |world: &World, params: &QueryParams| {
        raycast(world, &assets, v(0.0, 0.0, 0.0), v(0.0, 0.0, -20.0), params)
            .map(|hit| hit.instance)
    };
    let scripted = QueryParams {
        respect_can_query: true,
        collision_group: Some("Bullets".to_owned()),
        ..QueryParams::default()
    };
    assert_eq!(cast(&world, &scripted), Some(ghost));
    let solid = QueryParams {
        respect_can_collide: true,
        ..scripted.clone()
    };
    assert_eq!(cast(&world, &solid), Some(wall), "glass can't be queried");
    group(&mut world, wall, "Walls");
    assert_eq!(
        cast(&world, &solid),
        None,
        "bullets don't collide with walls"
    );
    // The characters' probes don't respect CanQuery.
    let probe = QueryParams {
        respect_can_collide: true,
        ..QueryParams::default()
    };
    assert_eq!(cast(&world, &probe), Some(glass));
}

#[test]
fn get_parts_in_part_lists_exactly_the_parts_inside_a_trigger_zone() {
    let mut world = World::new();
    let zone = part(&mut world, v(0.0, 5.0, 0.0), v(10.0, 10.0, 10.0), true);
    set(&mut world, zone, "CanCollide", Value::Bool(false));
    let inside = part(&mut world, v(2.0, 2.0, 2.0), v(1.0, 1.0, 1.0), true);
    let crossing = part(&mut world, v(5.0, 5.0, 0.0), v(2.0, 2.0, 2.0), true);
    let hidden = part(&mut world, v(-2.0, 5.0, 0.0), v(1.0, 1.0, 1.0), true);
    set(&mut world, hidden, "CanQuery", Value::Bool(false));
    let _outside = part(&mut world, v(20.0, 5.0, 0.0), v(1.0, 1.0, 1.0), true);
    // A ball whose bounding box reaches the zone's corner, but not the ball.
    let ball = part(&mut world, v(5.9, 10.9, 5.9), v(2.0, 2.0, 2.0), true);
    set(
        &mut world,
        ball,
        "Shape",
        Value::PartType(axcenity_core::PartType::Ball),
    );
    let assets = Assets::default();
    let params = QueryParams {
        respect_can_query: true,
        ..QueryParams::default()
    };
    assert_eq!(
        parts_in_part(&world, &assets, zone, &params),
        vec![inside, crossing]
    );

    // Bounds queries count the ball by its box.
    let cframe = CFrame {
        position: v(0.0, 5.0, 0.0),
        ..CFrame::IDENTITY
    };
    let in_box = parts_in_box(&world, &assets, cframe, v(10.0, 10.0, 10.0), &params);
    assert_eq!(in_box, vec![zone, inside, crossing, ball]);
    let near = parts_in_radius(&world, &assets, v(20.0, 5.0, 0.0), 1.0, &params);
    assert_eq!(near, vec![_outside]);
    let limited = QueryParams {
        max_parts: 2,
        ..params.clone()
    };
    assert_eq!(
        parts_in_box(&world, &assets, cframe, v(10.0, 10.0, 10.0), &limited).len(),
        2
    );
}

#[test]
fn shape_casts_find_the_first_part_they_sweep_into() {
    let mut world = World::new();
    let wall = part(&mut world, v(0.0, 0.0, -10.0), v(10.0, 10.0, 1.0), true);
    // Off to the side: a ray misses it, a wide ball doesn't.
    let post = part(&mut world, v(2.5, 0.0, -5.0), v(1.0, 10.0, 1.0), true);
    let assets = Assets::default();
    let params = QueryParams::default();
    let ray = raycast(
        &world,
        &assets,
        v(0.0, 0.0, 0.0),
        v(0.0, 0.0, -20.0),
        &params,
    );
    assert_eq!(ray.unwrap().instance, wall);

    let ball = CastShape::Ball {
        center: v(0.0, 0.0, 0.0),
        radius: 2.5,
    };
    let hit = shapecast(&world, &assets, ball, v(0.0, 0.0, -20.0), &params).unwrap();
    assert_eq!(hit.instance, post);
    let small = CastShape::Ball {
        center: v(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let hit = shapecast(&world, &assets, small, v(0.0, 0.0, -20.0), &params).unwrap();
    assert_eq!(hit.instance, wall);
    // The ball's front reaches the wall's face (z = -9.5) after 8.5 studs.
    assert!((hit.distance - 8.5).abs() < 1e-3, "{hit:?}");
    assert!((hit.position.z + 9.5).abs() < 1e-3, "{hit:?}");

    let block = CastShape::Block {
        cframe: CFrame::IDENTITY,
        size: v(6.0, 1.0, 1.0),
    };
    let hit = shapecast(&world, &assets, block, v(0.0, 0.0, -20.0), &params).unwrap();
    assert_eq!(hit.instance, post);

    // A part sweeps as its own shape and never hits itself.
    let bullet = part(&mut world, v(2.5, 0.0, 0.0), v(1.0, 1.0, 1.0), true);
    let hit = shapecast(
        &world,
        &assets,
        CastShape::Part(bullet),
        v(0.0, 0.0, -20.0),
        &params,
    );
    assert_eq!(hit.unwrap().instance, post);
    // A shape that starts inside a part passes it by.
    let inside = CastShape::Ball {
        center: v(2.5, 0.0, -5.0),
        radius: 0.25,
    };
    let hit = shapecast(&world, &assets, inside, v(0.0, 0.0, -20.0), &params);
    assert_eq!(hit.unwrap().instance, wall);
}
