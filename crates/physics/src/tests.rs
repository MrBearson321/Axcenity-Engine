use axcenity_assets::Assets;
use axcenity_core::{
    ClassName, InstanceId, Material, PartType, TerrainData, Value, Vector3, World,
};

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
fn a_corner_wedge_collides_as_its_shape() {
    let (mut world, _) = world_with_floor();
    // 8 studs each way, its point over the back right corner (+X, +Z).
    let corner = part(&mut world, Vector3::new(0.0, 4.0, 0.0), true);
    set(
        &mut world,
        corner,
        "Shape",
        Value::PartType(PartType::CornerWedge),
    );
    set(&mut world, corner, "Size", vector(8.0, 8.0, 8.0));
    // Over the cut-away front left, a ball falls past the box's top and
    // lands on the slopes, then rolls off toward -X and -Z.
    let ball = part(&mut world, Vector3::new(-2.0, 10.0, -2.0), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(1.0, 1.0, 1.0));
    // Falling straight through it would leave it at (-2, -2) on the floor.
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.5, 60.0);
    let late = position(&world, ball);
    assert!(
        late.x < -4.0 || late.z < -4.0,
        "rolled off the front left: {late:?}"
    );
    // Its collider overlay has the bottom rectangle and the four edges up.
    let edges = crate::shapes::wireframe(
        &crate::shapes::Form::Primitive(PartType::CornerWedge),
        Vec3::new(8.0, 8.0, 8.0),
    );
    assert_eq!(edges.len(), 8);
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

/// Runs `seconds` at 60 fps and returns every touch reported, frame by frame.
fn touches_during(
    physics: &mut Physics,
    world: &mut World,
    seconds: f64,
) -> Vec<(InstanceId, InstanceId)> {
    let frames = (seconds * 60.0).round() as u32;
    let mut touches = Vec::new();
    for _ in 0..frames {
        physics.step(world, 1.0 / 60.0);
        touches.extend(physics.take_touches());
    }
    touches
}

/// The pair as physics reports it: smaller ID first.
fn pair(a: InstanceId, b: InstanceId) -> (InstanceId, InstanceId) {
    if a < b { (a, b) } else { (b, a) }
}

#[test]
fn a_landing_is_reported_once_not_every_step_while_resting() {
    let (mut world, floor) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 3.0, 0.0), false);
    let mut physics = Physics::new();

    let landing = touches_during(&mut physics, &mut world, 2.0);
    assert!(!landing.is_empty(), "the landing should be reported");
    assert!(
        landing.iter().all(|&t| t == pair(block, floor)),
        "{landing:?}"
    );

    // Resting on the floor for 5 more seconds reports nothing new.
    assert_eq!(touches_during(&mut physics, &mut world, 5.0), []);
}

#[test]
fn a_heavy_block_dropped_a_short_way_is_reported_exactly_once() {
    let (mut world, floor) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 1.0, 0.0), false);
    let mut physics = Physics::new();
    assert_eq!(
        touches_during(&mut physics, &mut world, 3.0),
        [pair(block, floor)]
    );
}

#[test]
fn parts_already_touching_at_the_start_are_not_reported() {
    let (mut world, _) = world_with_floor();
    // Resting exactly on the floor's top.
    part(&mut world, Vector3::new(0.0, 0.5, 0.0), false);
    let mut physics = Physics::new();
    assert_eq!(touches_during(&mut physics, &mut world, 2.0), []);
}

#[test]
fn touching_again_after_a_teleport_is_a_new_touch() {
    let (mut world, floor) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 1.0, 0.0), false);
    let mut physics = Physics::new();
    assert_eq!(touches_during(&mut physics, &mut world, 2.0).len(), 1);

    set(&mut world, block, "Position", vector(0.0, 1.0, 0.0));
    assert_eq!(
        touches_during(&mut physics, &mut world, 2.0),
        [pair(block, floor)]
    );
}

#[test]
fn two_falling_parts_landing_report_each_pair() {
    let (mut world, floor) = world_with_floor();
    let left = part(&mut world, Vector3::new(-5.0, 1.0, 0.0), false);
    let right = part(&mut world, Vector3::new(5.0, 2.0, 0.0), false);
    let mut physics = Physics::new();
    let touches = touches_during(&mut physics, &mut world, 3.0);
    assert_eq!(touches, [pair(left, floor), pair(right, floor)]);
}

#[test]
fn anchored_parts_never_touch_each_other() {
    let (mut world, _) = world_with_floor();
    // Overlapping the floor, but anchored, so physics doesn't move or test it.
    part(&mut world, Vector3::new(0.0, 0.0, 0.0), true);
    let mut physics = Physics::new();
    assert_eq!(touches_during(&mut physics, &mut world, 1.0), []);
}

#[test]
fn a_bouncing_ball_is_reported_for_each_real_landing() {
    let (mut world, floor) = world_with_floor();
    let ball = part(&mut world, Vector3::new(0.0, 20.0, 0.0), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    let mut physics = Physics::new();
    let touches = touches_during(&mut physics, &mut world, 5.0);
    eprintln!("bounce touches: {}", touches.len());
    assert!(!touches.is_empty());
    assert!(touches.iter().all(|&t| t == pair(ball, floor)));
    assert_eq!(touches_during(&mut physics, &mut world, 3.0), []);
}

/// A MeshPart showing the example project's pyramid model (4 wide, 3 tall).
fn pyramid(world: &mut World, position: Vector3, size: Value, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::MeshPart).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    let mesh_id = Value::String("assets/models/pyramid.gltf".to_owned());
    set(world, id, "MeshId", mesh_id);
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", size);
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn example_assets() -> Assets {
    Assets::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh"))
}

#[test]
fn a_mesh_part_falls_and_lands_on_the_floor() {
    let (mut world, floor) = world_with_floor();
    let model = pyramid(
        &mut world,
        Vector3::new(0.0, 6.0, 0.0),
        vector(4.0, 3.0, 4.0),
        false,
    );
    let assets = example_assets();
    let mut physics = Physics::with_assets(assets.clone());
    let touches = touches_during(&mut physics, &mut world, 3.0);
    assert_eq!(touches.first(), Some(&pair(model, floor)));
    // Its flat base is on the floor, so its center is half its height up.
    let resting = position(&world, model);
    assert!((resting.y - 1.5).abs() < 0.05, "{resting:?}");
    assert!(assets.take_errors().is_empty());
}

#[test]
fn parts_slide_off_a_mesh_parts_slopes() {
    let (mut world, _) = world_with_floor();
    // An anchored pyramid 8 wide and 6 tall, its tip at Y = 6.
    let model = pyramid(
        &mut world,
        Vector3::new(0.0, 3.0, 0.0),
        vector(8.0, 6.0, 8.0),
        true,
    );
    // A ball dropped just off the tip. On the pyramid's box it would rest on
    // top; on its hull it rolls down a side to the floor.
    let ball = part(&mut world, Vector3::new(0.5, 9.0, 0.3), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    let mut physics = Physics::with_assets(example_assets());
    let touches = touches_during(&mut physics, &mut world, 5.0);
    assert_eq!(touches.first(), Some(&pair(ball, model)));
    let rolled = position(&world, ball);
    assert!(
        rolled.y < 1.5,
        "the ball should reach the floor: {rolled:?}"
    );
}

#[test]
fn a_mesh_part_without_a_model_collides_as_its_box() {
    let (mut world, _) = world_with_floor();
    let model = pyramid(
        &mut world,
        Vector3::new(0.0, 5.0, 0.0),
        vector(4.0, 3.0, 4.0),
        false,
    );
    set(&mut world, model, "MeshId", Value::String(String::new()));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0, 60.0);
    let resting = position(&world, model);
    assert!((resting.y - 1.5).abs() < 0.05, "{resting:?}");
}

#[test]
fn a_part_that_cannot_collide_is_passed_through_but_still_touched() {
    let (mut world, _) = world_with_floor();
    // A ghostly anchored slab in the way, and a block dropped through it.
    let ghost = part(&mut world, Vector3::new(0.0, 4.0, 0.0), true);
    set(&mut world, ghost, "Size", vector(10.0, 1.0, 10.0));
    set(&mut world, ghost, "CanCollide", Value::Bool(false));
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let mut physics = Physics::new();
    let mut touches = Vec::new();
    for _ in 0..240 {
        physics.step(&mut world, 1.0 / 60.0);
        touches.extend(physics.take_touches());
    }
    // It fell through the slab onto the floor...
    assert!((position(&world, block).y - 0.5).abs() < 0.05);
    // ...and touched the slab on the way.
    let pair = if ghost < block {
        (ghost, block)
    } else {
        (block, ghost)
    };
    assert!(touches.contains(&pair), "{touches:?}");

    // Turned back on, it holds a dropped part up.
    set(&mut world, ghost, "CanCollide", Value::Bool(true));
    let second = part(&mut world, Vector3::new(3.0, 10.0, 0.0), false);
    run(&mut physics, &mut world, 3.0, 60.0);
    assert!((position(&world, second).y - 5.0).abs() < 0.05);
}

#[test]
fn wind_pushes_only_parts_that_are_wind_affected() {
    let mut world = World::new();
    let workspace = world.workspace();
    set(&mut world, workspace, "GlobalWind", vector(60.0, 0.0, 0.0));
    let blown = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    set(&mut world, blown, "WindAffected", Value::Bool(true));
    let still = part(&mut world, Vector3::new(0.0, 50.0, 10.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5, 60.0);
    assert!(
        position(&world, blown).x > 1.0,
        "{:?}",
        position(&world, blown)
    );
    assert!(position(&world, still).x.abs() < 1e-3);
}

#[test]
fn turning_wind_affected_off_stops_the_push() {
    let mut world = World::new();
    let workspace = world.workspace();
    set(&mut world, workspace, "GlobalWind", vector(60.0, 0.0, 0.0));
    let id = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    set(&mut world, id, "WindAffected", Value::Bool(true));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.25, 60.0);
    set(&mut world, id, "WindAffected", Value::Bool(false));
    run(&mut physics, &mut world, 1.0 / 60.0, 60.0);
    let before = position(&world, id).x;
    run(&mut physics, &mut world, 0.25, 60.0);
    let after = position(&world, id).x;
    // Still drifting at the speed it had, but no faster.
    let early = after - before;
    run(&mut physics, &mut world, 0.25, 60.0);
    let late = position(&world, id).x - after;
    assert!((late - early).abs() < 0.05, "{early} then {late}");
}

#[test]
fn parts_land_on_terrain_and_edits_to_it_are_felt() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    // The default terrain's ground is flat at Y = 0 (a quarter of 128
    // studs up from -32), with 2 studs between samples.
    let ball = part(&mut world, Vector3::new(10.0, 20.0, 10.0), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0, 60.0);
    let resting = position(&world, ball);
    assert!((resting.y - 1.0).abs() < 0.05, "{resting:?}");
    assert!(physics.has_body(terrain));

    // Raise a plateau 16 studs high under where the next block falls.
    let data = world.terrain_mut(terrain).unwrap();
    let rect = axcenity_core::SampleRect::new(90, 90, 120, 120);
    let heights = data.heights_mut_in(rect);
    for z in 90..120 {
        for x in 90..120 {
            heights[z * 257 + x] = 0.375;
        }
    }
    // Sample 105 is 210 studs from the edge: -46 in the world.
    let block = part(&mut world, Vector3::new(-46.0, 40.0, -46.0), false);
    run(&mut physics, &mut world, 3.0, 60.0);
    let landed = position(&world, block);
    assert!((landed.y - 16.5).abs() < 0.05, "{landed:?}");
    // The ball, elsewhere, didn't move.
    assert_eq!(position(&world, ball), resting);

    // Turning CanCollide off lets things fall through.
    set(&mut world, terrain, "CanCollide", Value::Bool(false));
    run(&mut physics, &mut world, 1.0, 60.0);
    assert!(position(&world, ball).y < 0.0);
}

#[test]
fn a_raycast_hits_the_shape_a_part_collides_as_including_a_sensor() {
    let (world, floor) = world_with_floor();
    let hit = super::cast_ray(
        &world,
        &Assets::default(),
        Vector3::new(0.0, 10.0, 0.0),
        Vector3::new(0.0, -20.0, 0.0),
    )
    .expect("the floor is under the ray");
    assert_eq!(hit.instance, floor);
    assert!((hit.position.y - 0.0).abs() < 0.05, "{:?}", hit.position);
    assert!(
        hit.distance > 9.0 && hit.distance < 11.0,
        "{}",
        hit.distance
    );

    let miss = super::cast_ray(
        &world,
        &Assets::default(),
        Vector3::new(100.0, 10.0, 0.0),
        Vector3::new(0.0, -20.0, 0.0),
    );
    assert_eq!(miss, None);

    let mut world = World::new();
    let ghost = part(&mut world, Vector3::new(0.0, 4.0, 0.0), true);
    set(&mut world, ghost, "Size", vector(10.0, 1.0, 10.0));
    set(&mut world, ghost, "CanCollide", Value::Bool(false));
    let hit = super::cast_ray(
        &world,
        &Assets::default(),
        Vector3::new(0.0, 10.0, 0.0),
        Vector3::new(0.0, -20.0, 0.0),
    )
    .expect("a sensor is still a collider");
    assert_eq!(hit.instance, ghost);
    assert_eq!(position(&world, ghost), Vector3::new(0.0, 4.0, 0.0));
}

#[test]
fn a_raycast_hits_terrain_and_does_not_move_it() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let data = TerrainData::flat(9, 9, 0.25).unwrap();
    world.set_terrain(terrain, std::sync::Arc::new(data));
    set(&mut world, terrain, "Size", vector(16.0, 16.0, 16.0));
    set(
        &mut world,
        terrain,
        "Position",
        Value::Vector3(Vector3::new(0.0, -4.0, 0.0)),
    );
    let before = position(&world, terrain);
    let hit = super::cast_ray(
        &world,
        &Assets::default(),
        Vector3::new(0.0, 20.0, 0.0),
        Vector3::new(0.0, -40.0, 0.0),
    )
    .expect("the terrain is under the ray");
    assert_eq!(hit.instance, terrain);
    assert_eq!(position(&world, terrain), before);
}

#[test]
fn debug_colliders_include_sensors_and_terrain_chunks() {
    let mut world = World::new();
    let ghost = part(&mut world, Vector3::new(0.0, 4.0, 0.0), true);
    set(&mut world, ghost, "CanCollide", Value::Bool(false));
    let ball = part(&mut world, Vector3::new(5.0, 1.0, 0.0), true);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    world.set_terrain(
        terrain,
        std::sync::Arc::new(TerrainData::flat(9, 9, 0.25).unwrap()),
    );

    let outlines = super::debug_colliders(&world, &Assets::default());
    let ghost_outline = outlines
        .iter()
        .find(|outline| outline.instance == ghost)
        .expect("the sensor still has a collider");
    assert!(!ghost_outline.can_collide);
    assert_eq!(ghost_outline.edges.len(), 12);

    let ball_outline = outlines
        .iter()
        .find(|outline| outline.instance == ball)
        .expect("the ball has a round collider");
    assert!(
        ball_outline.edges.len() > 12,
        "a ball is rings, not a box: {}",
        ball_outline.edges.len()
    );

    let chunks = outlines
        .iter()
        .filter(|outline| outline.instance == terrain)
        .count();
    assert!(chunks >= 1, "a terrain is drawn as its chunks");
}

#[test]
fn a_part_lands_on_colliding_foliage_and_falls_through_visual_foliage() {
    fn terrain_with_crate(collide: bool) -> (World, InstanceId) {
        let mut world = World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        set(&mut world, terrain, "Position", vector(0.0, 0.0, 0.0));
        set(&mut world, terrain, "Size", vector(32.0, 16.0, 32.0));
        let mut data = TerrainData::flat(9, 9, 0.0).unwrap();
        data.foliage_mut().push(axcenity_core::FoliageInstance {
            source: axcenity_core::FoliageSource::Prop(axcenity_core::FoliageProp::Crate),
            x: 4.0,
            z: 4.0,
            yaw: 0.0,
            scale: 1.0,
            collide,
            lean: 0.0,
            sink: 0.0,
        });
        world.set_terrain(terrain, std::sync::Arc::new(data));
        (world, terrain)
    }

    let (mut world, _) = terrain_with_crate(true);
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0, 60.0);
    let resting = position(&world, block);
    // Crate is 3 studs tall on the ground; the 1-stud-tall block rests on it.
    assert!(
        (resting.y - 3.5).abs() < 0.15,
        "should rest on the crate, got {resting:?}"
    );

    let (mut world, _) = terrain_with_crate(false);
    let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0, 60.0);
    let resting = position(&world, block);
    assert!(
        (resting.y - 0.5).abs() < 0.15,
        "should rest on the terrain, got {resting:?}"
    );
}

/// How far a 2×1×2 Plastic box thrown at 20 studs a second along X slides
/// across a floor of `material`, with no script: its velocity is set
/// before the game starts.
fn slide_on(material: Material) -> f32 {
    let (mut world, floor) = world_with_floor();
    set(&mut world, floor, "Material", Value::Material(material));
    let block = part(&mut world, Vector3::new(-15.0, 0.5, 0.0), false);
    set(&mut world, block, "Size", vector(2.0, 1.0, 2.0));
    set(
        &mut world,
        block,
        "AssemblyLinearVelocity",
        vector(20.0, 0.0, 0.0),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0, 60.0);
    position(&world, block).x + 15.0
}

#[test]
fn a_box_slides_further_on_ice_than_on_plastic() {
    let plastic = slide_on(Material::Plastic);
    let ice = slide_on(Material::Ice);
    // At friction 0.5 it stops after about v² / (2 μ g) = 2 studs; Ice
    // (weighted against Plastic) leaves about a quarter of that friction.
    assert!(plastic > 1.0 && plastic < 4.0, "{plastic}");
    assert!(ice > 2.5 * plastic, "ice {ice}, plastic {plastic}");
}

/// The highest a 2-stud ball of `material` gets after its first bounce,
/// dropped from 10 studs onto the Plastic floor, with no script.
fn bounce_of(material: Material) -> f32 {
    let (mut world, _) = world_with_floor();
    let ball = part(&mut world, Vector3::new(0.0, 10.0, 0.0), false);
    set(&mut world, ball, "Shape", Value::PartType(PartType::Ball));
    set(&mut world, ball, "Size", vector(2.0, 2.0, 2.0));
    set(&mut world, ball, "Material", Value::Material(material));
    let mut physics = Physics::new();
    let mut bounced = false;
    let mut highest = 0.0_f32;
    for _ in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
        let rising = world
            .get(ball)
            .unwrap()
            .property("AssemblyLinearVelocity")
            .and_then(Value::as_vector3)
            .unwrap()
            .y
            > 0.0;
        bounced |= rising;
        if bounced {
            highest = highest.max(position(&world, ball).y);
            if !rising {
                break;
            }
        }
    }
    highest
}

#[test]
fn a_rubber_ball_bounces_higher_than_a_plastic_one() {
    let plastic = bounce_of(Material::Plastic);
    let rubber = bounce_of(Material::Rubber);
    assert!(rubber > plastic + 2.0, "rubber {rubber}, plastic {plastic}");
    assert!(
        rubber < 10.0,
        "it never bounces higher than it fell: {rubber}"
    );
}

/// Where a crate dropped from 50 studs is after half a second, under the
/// Workspace's `gravity`.
fn drop_with_gravity(gravity: f64) -> f32 {
    let (mut world, _) = world_with_floor();
    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", Value::Number(gravity));
    let crate_id = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5, 60.0);
    50.0 - position(&world, crate_id).y
}

#[test]
fn halving_workspace_gravity_makes_crates_fall_slower() {
    assert_eq!(gravity(&World::new()), GRAVITY);
    let full = drop_with_gravity(f64::from(GRAVITY));
    let half = drop_with_gravity(f64::from(GRAVITY) / 2.0);
    // ½ g t² over half a second: about 24.5 and 12.3 studs.
    assert!((full - 24.5).abs() < 1.0, "{full}");
    assert!((half / full - 0.5).abs() < 0.03, "half {half}, full {full}");
}

#[test]
fn a_part_s_velocity_is_written_back_and_a_set_one_is_kept() {
    let (mut world, _) = world_with_floor();
    let block = part(&mut world, Vector3::new(0.0, 60.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5, 60.0);
    let falling = world
        .get(block)
        .unwrap()
        .property("AssemblyLinearVelocity")
        .and_then(Value::as_vector3)
        .unwrap();
    // v = g t after half a second.
    assert!((falling.y + 98.1).abs() < 2.0, "{falling:?}");
    // Teleporting and throwing it in the same frame keeps the throw.
    set(&mut world, block, "Position", vector(0.0, 30.0, 0.0));
    set(
        &mut world,
        block,
        "AssemblyLinearVelocity",
        vector(0.0, 50.0, 0.0),
    );
    physics.step(&mut world, 1.0 / 60.0);
    assert!(
        position(&world, block).y > 30.5,
        "{:?}",
        position(&world, block)
    );
}

/// Phase 45: a Tool in hand is held still and walked through; one put away
/// in an Inventory is out of the world.
#[test]
fn a_tool_in_hand_is_held_still_and_one_put_away_is_out_of_the_world() {
    let (mut world, _) = world_with_floor();
    let workspace = world.workspace();
    let character = world.create(ClassName::Model).unwrap();
    world.set_parent(character, Some(workspace)).unwrap();
    let inventory = world.create(ClassName::Inventory).unwrap();
    world.set_parent(inventory, Some(character)).unwrap();
    let tool = |world: &mut World, y: f32| {
        let tool = world.create(ClassName::Tool).unwrap();
        world.set_parent(tool, Some(inventory)).unwrap();
        let handle = part(world, Vector3::new(0.0, y, 0.0), false);
        world.set_name(handle, "Handle").unwrap();
        world.set_parent(handle, Some(tool)).unwrap();
        (tool, handle)
    };
    let (held, held_handle) = tool(&mut world, 5.0);
    let (_, stored_handle) = tool(&mut world, 10.0);
    set(
        &mut world,
        inventory,
        "Equipped",
        Value::Instance(Some(held)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0, 60.0);
    assert_eq!(position(&world, held_handle), Vector3::new(0.0, 5.0, 0.0));
    assert_eq!(
        position(&world, stored_handle),
        Vector3::new(0.0, 10.0, 0.0)
    );
    // A ray down from above meets the held one first, as a sensor (it
    // doesn't collide): the stored one above it isn't there.
    let assets = Assets::default();
    let down = Vector3::new(0.0, -20.0, 0.0);
    let hit = cast_ray(&world, &assets, Vector3::new(0.0, 15.0, 0.0), down).unwrap();
    assert_eq!(hit.instance, held_handle);
    assert!(!hit.can_collide);

    // Put away and back on the ground, it falls like any part.
    set(&mut world, inventory, "Equipped", Value::Instance(None));
    world.set_parent(held, Some(workspace)).unwrap();
    run(&mut physics, &mut world, 1.0, 60.0);
    assert!(
        position(&world, held_handle).y < 1.0,
        "it should have fallen"
    );
}

#[test]
fn far_parts_sleep_while_streaming_and_wake_when_something_comes_near() {
    use axcenity_core::{ClassName, Value, Vector3, World};

    let mut world = World::new();
    let block = |world: &mut World, x: f32| {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(part, "Anchored", Value::Bool(false))
            .unwrap();
        world
            .set_property(part, "Position", Value::Vector3(Vector3::new(x, 50.0, 0.0)))
            .unwrap();
        let workspace = world.workspace();
        world.set_parent(part, Some(workspace)).unwrap();
        part
    };
    let near = block(&mut world, 0.0);
    let far = block(&mut world, 1000.0);
    let height = |world: &World, part| {
        world
            .get(part)
            .and_then(|p| p.property("Position"))
            .and_then(|v| v.as_vector3())
            .unwrap()
            .y
    };
    let mut physics = Physics::new();
    physics.set_far_sleep(Some((vec![Vector3::default()], 100.0)));
    for _ in 0..30 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    assert!(height(&world, near) < 50.0, "the near part falls");
    assert_eq!(height(&world, far), 50.0, "the far part sleeps");

    // A focus comes near: it wakes and falls.
    physics.set_far_sleep(Some((vec![Vector3::new(1000.0, 0.0, 0.0)], 100.0)));
    for _ in 0..30 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    assert!(height(&world, far) < 50.0, "woken, it falls");

    // With streaming off every part simulates, whatever is far.
    physics.set_far_sleep(None);
    let before = height(&world, near);
    for _ in 0..10 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    assert!(height(&world, near) < before);
}

#[test]
fn a_weld_in_another_service_still_joins_and_parts_its_parts() {
    let mut world = World::new();
    let a = part(&mut world, Vector3::new(0.0, 5.0, 0.0), false);
    let b = part(&mut world, Vector3::new(1.0, 5.0, 0.0), false);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert_ne!(physics.parts[&a].body, physics.parts[&b].body);

    // Nothing in the Workspace changes parent, so only `watched` can notice.
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    let storage = world.replicated_storage();
    world.set_parent(weld, Some(storage)).unwrap();
    set(&mut world, weld, "Part0", Value::Instance(Some(a)));
    set(&mut world, weld, "Part1", Value::Instance(Some(b)));
    let workspace = world.structure_version_of(world.workspace());
    physics.step(&mut world, STEP_SECONDS);
    assert_eq!(world.structure_version_of(world.workspace()), workspace);
    assert_eq!(physics.parts[&a].body, physics.parts[&b].body);

    world.destroy(weld).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert_ne!(physics.parts[&a].body, physics.parts[&b].body);
}

/// A model that fills its box corner to corner: four faces, so not flat.
fn tetrahedron() -> std::sync::Arc<axcenity_assets::Model> {
    let primitive = axcenity_assets::Primitive::new(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![[0.0, 1.0, 0.0]; 4],
        Vec::new(),
        vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
        axcenity_assets::ModelMaterial::default(),
    );
    std::sync::Arc::new(axcenity_assets::Model::from_primitives(vec![primitive]).unwrap())
}

#[test]
fn a_hull_is_built_once_for_a_model_and_size() {
    use crate::shapes::{Fidelity, Form, collider};
    let model = tetrahedron();
    let form = Form::Mesh(Some(model), Fidelity::Hull);
    let size = rapier3d::glamx::Vec3::new(2.0, 4.0, 6.0);
    let (first, second) = (collider(&form, size), collider(&form, size));
    assert!(std::sync::Arc::ptr_eq(
        &first.shared_shape().0,
        &second.shared_shape().0
    ));

    // Another size has its own hull, stretched to it.
    let bigger = rapier3d::glamx::Vec3::new(4.0, 8.0, 12.0);
    let stretched = collider(&form, bigger);
    assert!(!std::sync::Arc::ptr_eq(
        &first.shared_shape().0,
        &stretched.shared_shape().0
    ));
    let extents = |built: &rapier3d::prelude::Collider| built.compute_aabb().half_extents();
    assert!((extents(&stretched) - extents(&first) * 2.0).length() < 1e-4);

    // A model loaded again is a new model.
    let reloaded = Form::Mesh(Some(tetrahedron()), Fidelity::Hull);
    assert!(!std::sync::Arc::ptr_eq(
        &first.shared_shape().0,
        &collider(&reloaded, size).shared_shape().0
    ));
}

/// The corners of the box from `min` to `max`.
fn box_corners(min: [f32; 3], max: [f32; 3]) -> Vec<[f32; 3]> {
    (0..8)
        .map(|i| {
            let pick = |axis: usize| {
                if i & (1 << axis) == 0 {
                    min[axis]
                } else {
                    max[axis]
                }
            };
            [pick(0), pick(1), pick(2)]
        })
        .collect()
}

/// A model filling its box, whose collision pieces (R15.2) make a doorway:
/// two posts and a lintel, open in the middle from the floor up.
fn doorway() -> std::sync::Arc<axcenity_assets::Model> {
    let corners = box_corners([0.0; 3], [1.0; 3]);
    let primitive = axcenity_assets::Primitive::new(
        corners,
        vec![[0.0, 1.0, 0.0]; 8],
        Vec::new(),
        vec![
            0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4,
            6, 1, 3, 5, 3, 7, 5,
        ],
        axcenity_assets::ModelMaterial::default(),
    );
    let mut model = axcenity_assets::Model::from_primitives(vec![primitive]).unwrap();
    model.collision = vec![
        box_corners([-0.5, -0.5, -0.5], [-0.3, 0.5, 0.5]),
        box_corners([0.3, -0.5, -0.5], [0.5, 0.5, 0.5]),
        box_corners([-0.5, 0.3, -0.5], [0.5, 0.5, 0.5]),
    ];
    std::sync::Arc::new(model)
}

#[test]
fn a_meshpart_with_collision_pieces_collides_as_them_by_default() {
    use crate::shapes::{Fidelity, Form, collider, wireframe};
    use rapier3d::glamx::Vec3;
    let size = Vec3::splat(10.0);
    let inside =
        |form: &Form, point: Vec3| collider(form, size).shape().contains_local_point(point);
    let doorway_form = Form::Mesh(Some(doorway()), Fidelity::Default);
    // The doorway is open; the posts and lintel are solid.
    assert!(!inside(&doorway_form, Vec3::new(0.0, -2.0, 0.0)));
    assert!(inside(&doorway_form, Vec3::new(-4.0, 0.0, 0.0)));
    assert!(inside(&doorway_form, Vec3::new(4.0, -4.0, 0.0)));
    assert!(inside(&doorway_form, Vec3::new(0.0, 4.0, 0.0)));
    assert_eq!(
        collider(&doorway_form, size)
            .shape()
            .as_compound()
            .map(|pieces| pieces.shapes().len()),
        Some(3)
    );
    // Asked for its hull, it fills the doorway as before.
    let hull = Form::Mesh(Some(doorway()), Fidelity::Hull);
    assert!(inside(&hull, Vec3::new(0.0, -2.0, 0.0)));
    // Without pieces, Default is its hull.
    let plain = Form::Mesh(Some(tetrahedron()), Fidelity::Default);
    assert!(
        collider(&plain, size)
            .shape()
            .as_convex_polyhedron()
            .is_some()
    );
    // The Collisions overlay draws the pieces.
    assert_ne!(
        wireframe(&doorway_form, size).len(),
        wireframe(&hull, size).len()
    );

    assert_eq!(Fidelity::named("Default"), Fidelity::Default);
    assert_eq!(Fidelity::named("Hull"), Fidelity::Hull);
}

/// A 2x2x2 block on a stack of five 1x1x1 blocks stays up for 10 s, built
/// six slightly different ways (`CONTACT_RECYCLE_DISTANCE`).
#[test]
fn a_heavy_block_on_a_short_stack_stays_up() {
    for build in 0..6 {
        let (mut world, _) = world_with_floor();
        // Each block a few thousandths off the line, differently per build.
        let nudge = |i: usize| build as f32 * 0.002 * (((i * 7 + build) % 3) as f32 - 1.0);
        for i in 0..5 {
            let block = part(
                &mut world,
                Vector3::new(nudge(i), 0.5 + i as f32, nudge(i + 1)),
                false,
            );
            set(&mut world, block, "Size", vector(1.0, 1.0, 1.0));
        }
        let heavy = part(&mut world, Vector3::new(0.0, 6.0, 0.0), false);
        set(&mut world, heavy, "Size", vector(2.0, 2.0, 2.0));
        let mut physics = Physics::new();
        run(&mut physics, &mut world, 10.0, 60.0);
        let at = position(&world, heavy);
        assert!(
            at.y > 5.0 && at.x.hypot(at.z) < 0.5,
            "build {build}: the heavy block fell to {at:?}"
        );
    }
}

/// Ten Ice blocks stack as well as ten Plastic ones. Ice's friction weight
/// turns on the contact hook, and rapier didn't keep hooked pairs' contacts
/// between steps, so the Ice stack toppled (vendor/README.md).
#[test]
fn a_stack_of_ice_blocks_stands_like_plastic() {
    let top_of = |material: &str| {
        let (mut world, _) = world_with_floor();
        let mut top = None;
        for i in 0..10 {
            let block = part(&mut world, Vector3::new(0.0, 0.5 + i as f32, 0.0), false);
            set(&mut world, block, "Size", vector(1.0, 1.0, 1.0));
            set(
                &mut world,
                block,
                "Material",
                Value::Material(Material::from_name(material).unwrap()),
            );
            top = Some(block);
        }
        let mut physics = Physics::new();
        run(&mut physics, &mut world, 10.0, 60.0);
        position(&world, top.unwrap())
    };
    let (ice, plastic) = (top_of("Ice"), top_of("Plastic"));
    assert!(
        ice.y > 8.5 && ice.x.hypot(ice.z) < 0.3,
        "the Ice stack stands: its top is at {ice:?}"
    );
    assert!(
        (ice.y - plastic.y).abs() < 0.05,
        "as high as Plastic's: {ice:?} and {plastic:?}"
    );
}

#[test]
fn the_debug_view_shows_contacts_centers_of_mass_and_sleep_without_changing_anything() {
    let (mut world, floor) = world_with_floor();
    // A 4×1×2 block (default size) dropped just above the floor.
    let block = part(&mut world, Vector3::new(3.0, 2.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.1, 60.0);
    // Still falling: awake, no contacts yet, its center where it is.
    let view = physics.debug_view();
    assert!(view.contacts.is_empty(), "{:?}", view.contacts);
    assert_eq!(view.bodies.len(), 1, "the anchored floor isn't listed");
    let body = view.bodies[0];
    assert_eq!(body.part, block);
    assert!(!body.sleeping);
    assert!(body.mass > 0.0);
    let at = position(&world, block);
    assert!(
        (body.center_of_mass.y - at.y).abs() < 1e-3,
        "{body:?} {at:?}"
    );

    // Landed and settled: contacts on the floor's top, pushing the block
    // up, and in time the block sleeps.
    run(&mut physics, &mut world, 5.0, 60.0);
    let view = physics.debug_view();
    assert!(!view.contacts.is_empty());
    for contact in &view.contacts {
        let (one, two) = contact.parts;
        assert!(
            (one, two) == (block, floor) || (one, two) == (floor, block),
            "{contact:?}"
        );
        assert!(
            contact.point.y.abs() < 0.05,
            "on the floor's top: {contact:?}"
        );
        let up = if one == block { 1.0 } else { -1.0 };
        assert!((contact.normal.y - up).abs() < 1e-3, "{contact:?}");
    }
    assert!(view.contacts.iter().any(|contact| contact.impulse > 0.0));
    assert!(view.bodies[0].sleeping, "{:?}", view.bodies[0]);
    // Reading it changed nothing.
    let before = position(&world, block);
    assert_eq!(physics.debug_view(), view);
    assert_eq!(position(&world, block), before);
}
