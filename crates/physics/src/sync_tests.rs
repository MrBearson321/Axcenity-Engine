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

fn asleep(physics: &Physics, id: InstanceId) -> bool {
    physics.rapier.bodies[physics.parts[&id].body].is_sleeping()
}

/// A floor, and a block resting on it at `x`. Returns them, floor first.
fn block_on_a_floor(world: &mut World, x: f32) -> (InstanceId, InstanceId) {
    let floor = part(world, Vector3::new(x, -0.5, 0.0), true);
    set(
        world,
        floor,
        "Size",
        Value::Vector3(Vector3::new(10.0, 1.0, 10.0)),
    );
    let block = part(world, Vector3::new(x, 0.5, 0.0), false);
    (floor, block)
}

/// Steps until nothing moves any more.
fn settle(physics: &mut Physics, world: &mut World) {
    for _ in 0..300 {
        physics.step(world, STEP_SECONDS);
    }
}

#[test]
fn a_resting_body_stays_asleep_while_nothing_touches_it() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block), "it came to rest");
    for _ in 0..120 {
        physics.step(&mut world, STEP_SECONDS);
        assert!(asleep(&physics, block));
        assert_eq!(physics.sync_stats().parts_read, 0);
    }
}

#[test]
fn a_change_elsewhere_does_not_wake_a_resting_body() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let far = part(&mut world, Vector3::new(500.0, 0.0, 0.0), true);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block));

    for i in 0..30 {
        set(
            &mut world,
            far,
            "Position",
            Value::Vector3(Vector3::new(500.0 + i as f32, 0.0, 0.0)),
        );
        part(
            &mut world,
            Vector3::new(800.0 + 5.0 * i as f32, 0.0, 0.0),
            true,
        );
        physics.step(&mut world, STEP_SECONDS);
        assert!(asleep(&physics, block));
    }
}

#[test]
fn moving_a_part_wakes_what_rests_on_it_but_not_a_distant_stack() {
    let mut world = World::new();
    let (floor, block) = block_on_a_floor(&mut world, 0.0);
    let (_, distant) = block_on_a_floor(&mut world, 200.0);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block) && asleep(&physics, distant));

    // The floor drops away from under the block.
    set(
        &mut world,
        floor,
        "Position",
        Value::Vector3(Vector3::new(0.0, -50.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, block), "it lost what it rested on");
    assert!(asleep(&physics, distant));
    let (before, resting) = (height(&world, block), height(&world, distant));
    for _ in 0..10 {
        physics.step(&mut world, STEP_SECONDS);
    }
    assert!(height(&world, block) < before, "it falls");
    assert_eq!(height(&world, distant), resting);
}

#[test]
fn removing_a_part_wakes_what_rests_on_it_but_not_a_distant_stack() {
    let mut world = World::new();
    let (floor, block) = block_on_a_floor(&mut world, 0.0);
    let (_, distant) = block_on_a_floor(&mut world, 200.0);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);

    world.destroy(floor).unwrap();
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, block));
    assert!(asleep(&physics, distant));
}

#[test]
fn an_anchored_part_put_inside_a_sleeping_body_wakes_it() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let wall = part(&mut world, Vector3::new(100.0, 0.5, 0.0), true);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block));

    set(
        &mut world,
        wall,
        "Position",
        Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, block));
}

#[test]
fn a_new_anchored_part_inside_a_sleeping_body_wakes_it() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);

    part(&mut world, Vector3::new(0.0, 1.0, 0.0), true);
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, block));
}

#[test]
fn a_joint_to_a_moved_body_wakes_the_body_on_its_other_end() {
    let mut world = World::new();
    let (floor_a, a) = block_on_a_floor(&mut world, 0.0);
    let (_, b) = block_on_a_floor(&mut world, 20.0);
    let attachment = |world: &mut World, parent: InstanceId| {
        let id = world.create(ClassName::Attachment).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let (one, two) = (attachment(&mut world, a), attachment(&mut world, b));
    let rope = world.create(ClassName::RopeConstraint).unwrap();
    world.set_parent(rope, Some(world.workspace())).unwrap();
    set(&mut world, rope, "Attachment0", Value::Instance(Some(one)));
    set(&mut world, rope, "Attachment1", Value::Instance(Some(two)));
    set(&mut world, rope, "Enabled", Value::Bool(true));
    set(&mut world, rope, "Length", Value::Number(30.0));
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, a) && asleep(&physics, b));

    set(
        &mut world,
        floor_a,
        "Position",
        Value::Vector3(Vector3::new(0.0, -50.0, 0.0)),
    );
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, a) && !asleep(&physics, b));
}

#[test]
fn changing_gravity_wakes_what_slept_under_the_old_one() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let mut physics = Physics::new();
    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block));

    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", Value::Number(-196.2));
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, block));
    for _ in 0..30 {
        physics.step(&mut world, STEP_SECONDS);
    }
    assert!(height(&world, block) > 1.0, "gravity pulls it up now");
}

fn revision(world: &World, id: InstanceId) -> u64 {
    world.get(id).map_or(0, Instance::revision)
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .and_then(|part| part.property("Position"))
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn a_frame_of_many_steps_writes_each_part_once() {
    let mut world = World::new();
    let falling = part(&mut world, Vector3::new(0.0, 50.0, 0.0), false);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);

    let before = (revision(&world, falling), height(&world, falling));
    let steps = physics.step(&mut world, 3.0 * STEP_SECONDS);
    assert_eq!(steps, 3);
    assert!(height(&world, falling) < before.1, "it should be falling");
    // Its Position and AssemblyLinearVelocity change, once each: not three
    // times, once per step.
    assert_eq!(revision(&world, falling) - before.0, 2);
}

#[test]
fn one_long_frame_lands_where_short_frames_do() {
    let make = || {
        let mut world = World::new();
        let (_, block) = block_on_a_floor(&mut world, 0.0);
        let falling = part(&mut world, Vector3::new(0.3, 6.0, 0.2), false);
        (world, block, falling)
    };
    let (mut short, block, falling) = make();
    let (mut long, _, _) = make();
    let (mut short_physics, mut long_physics) = (Physics::new(), Physics::new());
    for _ in 0..12 {
        for _ in 0..4 {
            short_physics.step(&mut short, STEP_SECONDS);
        }
        long_physics.step(&mut long, 4.0 * STEP_SECONDS);
        for id in [block, falling] {
            assert_eq!(position(&short, id), position(&long, id));
        }
    }
}

#[test]
fn a_body_that_falls_asleep_in_a_long_frame_is_written_where_it_rests() {
    let mut world = World::new();
    let (_, block) = block_on_a_floor(&mut world, 0.0);
    let falling = part(&mut world, Vector3::new(0.3, 3.0, 0.2), false);
    let mut physics = Physics::new();
    for _ in 0..100 {
        physics.step(&mut world, 10.0 * STEP_SECONDS);
        if asleep(&physics, falling) && asleep(&physics, block) {
            break;
        }
    }
    assert!(asleep(&physics, falling), "it came to rest");
    for id in [block, falling] {
        let body = &physics.rapier.bodies[physics.parts[&id].body];
        let rest = to_vector3(body.position().translation);
        assert_eq!(position(&world, id), rest);
    }
}

#[test]
fn the_write_back_looks_only_at_awake_bodies() {
    let mut world = World::new();
    for i in 0..200 {
        part(&mut world, Vector3::new(i as f32 * 3.0, 0.0, 0.0), true);
    }
    let (_, block) = block_on_a_floor(&mut world, -50.0);
    let falling = part(&mut world, Vector3::new(0.0, 50.0, 30.0), false);
    let mut physics = Physics::new();
    physics.step(&mut world, STEP_SECONDS);
    assert!(
        physics.sync_stats().parts_written >= 200,
        "the first step writes all"
    );

    settle(&mut physics, &mut world);
    assert!(asleep(&physics, block));
    physics.step(&mut world, STEP_SECONDS);
    assert!(!asleep(&physics, falling), "nothing is under it");
    assert_eq!(
        physics.sync_stats().parts_written,
        1,
        "only the falling part"
    );
}

/// A pile of parts lands the same way every time, also with the
/// `parallel` feature (rapier's multi-threaded solver), whatever the
/// number of threads.
#[test]
fn a_pile_lands_the_same_way_every_time() {
    let first = pile(None);
    assert_eq!(first, pile(None));
}

/// Physics' own pool (`threads`) of any size lands it the same way.
#[cfg(feature = "parallel")]
#[test]
fn a_pile_lands_the_same_way_on_any_number_of_threads() {
    let one = pile(Some(1));
    assert_eq!(one, pile(Some(3)));
    assert_eq!(one, pile(Some(8)));
}

/// Worlds stepping at the same time on the same threads, more of them than
/// there are threads, as tests do. rapier's step waits on a task it hands
/// the pool, so if a step took every thread, each would wait forever.
#[cfg(feature = "parallel")]
#[test]
fn more_worlds_than_threads_can_step_at_once() {
    let pool = threads::make(2);
    let worlds = 6;
    let (done, finished) = std::sync::mpsc::channel();
    for _ in 0..worlds {
        let (pool, done) = (pool.clone(), done.clone());
        std::thread::spawn(move || {
            let mut world = World::new();
            block_on_a_floor(&mut world, 0.0);
            for i in 0..40 {
                let (x, z) = ((i % 5) as f32 * 0.9 - 2.0, (i / 5) as f32 * 0.7 - 2.5);
                part(&mut world, Vector3::new(x, 2.0 + (i / 10) as f32, z), false);
            }
            let mut physics = Physics::new();
            physics.pool = pool;
            for _ in 0..120 {
                physics.step(&mut world, STEP_SECONDS);
            }
            done.send(()).unwrap();
        });
    }
    for _ in 0..worlds {
        finished
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("every world finishes its steps");
    }
}

/// Where 150 parts dropped on a block on a floor are after 90 steps, with
/// rapier's step on `threads` threads of a pool of its own, or the shared
/// pool.
#[cfg_attr(not(feature = "parallel"), allow(unused_variables))]
fn pile(threads: Option<usize>) -> Vec<Vector3> {
    let run = || {
        let mut world = World::new();
        block_on_a_floor(&mut world, 0.0);
        let parts: Vec<InstanceId> = (0..150)
            .map(|i| {
                let (x, z) = ((i % 5) as f32 * 0.9 - 2.0, (i / 5 % 5) as f32 * 0.7 - 1.5);
                part(
                    &mut world,
                    Vector3::new(x, 2.0 + (i / 25) as f32 * 1.5, z),
                    false,
                )
            })
            .collect();
        let mut physics = Physics::new();
        #[cfg(feature = "parallel")]
        if let Some(threads) = threads {
            physics.use_threads(threads);
        }
        for _ in 0..90 {
            physics.step(&mut world, STEP_SECONDS);
        }
        parts
            .iter()
            .map(|&id| position(&world, id))
            .collect::<Vec<_>>()
    };
    run()
}

#[test]
fn a_part_moved_every_frame_still_takes_its_other_changes() {
    let mut world = World::new();
    // An anchored platform a script slides along every frame, with a block
    // resting on it.
    let platform = part(&mut world, Vector3::new(0.0, 0.0, 0.0), true);
    set(
        &mut world,
        platform,
        "Size",
        Value::Vector3(Vector3::new(8.0, 1.0, 8.0)),
    );
    let block = part(&mut world, Vector3::new(0.0, 1.0, 0.0), false);
    let mut physics = Physics::new();
    for frame in 0..30 {
        let x = frame as f32 * 0.01;
        set(
            &mut world,
            platform,
            "Position",
            Value::Vector3(Vector3::new(x, 0.0, 0.0)),
        );
        physics.step(&mut world, STEP_SECONDS);
    }
    assert!((height(&world, block) - 1.0).abs() < 0.05, "it rests on it");

    // Moved and, in the same frame, turned into a sensor: the block falls
    // through, so the change wasn't lost to only the move being read.
    set(
        &mut world,
        platform,
        "Position",
        Value::Vector3(Vector3::new(0.5, 0.0, 0.0)),
    );
    set(&mut world, platform, "CanCollide", Value::Bool(false));
    for _ in 0..30 {
        physics.step(&mut world, STEP_SECONDS);
    }
    assert!(height(&world, block) < 0.0, "it falls through");

    // Solid again while moving every frame: a new block lands on it.
    set(&mut world, platform, "CanCollide", Value::Bool(true));
    let second = part(&mut world, Vector3::new(0.5, 3.0, 0.0), false);
    for frame in 0..60 {
        let x = 0.5 + frame as f32 * 0.01;
        set(
            &mut world,
            platform,
            "Position",
            Value::Vector3(Vector3::new(x, 0.0, 0.0)),
        );
        physics.step(&mut world, STEP_SECONDS);
    }
    assert!(
        (height(&world, second) - 1.0).abs() < 0.05,
        "it lands on it"
    );
}
