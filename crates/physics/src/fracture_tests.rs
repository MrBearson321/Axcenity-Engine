//! Phase 42: parts with a Fracture shattering into their pieces.

use std::path::PathBuf;

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vec3 {
    to_vec3(
        world
            .get(id)
            .unwrap()
            .property(name)
            .and_then(Value::as_vector3)
            .unwrap(),
    )
}

fn part(world: &mut World, name: &str, position: Vector3, size: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    id
}

/// An empty project folder for this test.
fn project(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-physics-fracture-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A world with an anchored floor and a 4×0.4×6 slab named `Slab` (with
/// `pieces` pieces generated in its Fracture), and the project's assets.
fn slab(name: &str, pieces: f64, at: Vector3) -> (World, Assets, InstanceId, InstanceId) {
    let folder = project(name);
    let mut world = World::new();
    let floor = part(&mut world, "Floor", v(0.0, -0.5, 0.0), v(40.0, 1.0, 40.0));
    set(&mut world, floor, "Anchored", Value::Bool(true));
    let slab = part(&mut world, "Slab", at, v(4.0, 0.4, 6.0));
    let fracture = world.create(ClassName::Fracture).unwrap();
    world.set_parent(fracture, Some(slab)).unwrap();
    set(&mut world, fracture, "Pieces", Value::Number(pieces));
    set(&mut world, fracture, "Seed", Value::Number(3.0));
    let assets = Assets::new(&folder);
    let path = axcenity_mesh_edit::generate(&world, &assets, &folder, fracture).unwrap();
    set(&mut world, fracture, "PiecesId", Value::String(path));
    (world, assets, slab, fracture)
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
    }
}

#[test]
fn pieces_shattered_at_rest_fill_the_part_without_gaps_or_overlaps() {
    let (mut world, assets, slab, fracture) = slab("rest", 30.0, v(0.0, 0.2, 0.0));
    let mut physics = Physics::with_assets(assets);
    run(&mut physics, &mut world, 0.5);
    let resting = vector(&world, slab, "Position");
    assert!(physics.shatter(&mut world, fracture, None, 0.0));
    assert_eq!(physics.take_shattered(), vec![fracture]);
    assert_eq!(world.get(slab).unwrap().parent(), None);
    let (_, pieces) = physics.debris().remove(0);
    assert_eq!(pieces.len(), 30);
    let started: Vec<Vec3> = pieces
        .iter()
        .map(|&id| vector(&world, id, "Position"))
        .collect();
    // Their boxes put together are the slab's box.
    let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for (&id, &at) in pieces.iter().zip(&started) {
        let half = vector(&world, id, "Size") / 2.0;
        (low, high) = (low.min(at - half), high.max(at + half));
    }
    assert!((low - (resting - Vec3::new(2.0, 0.2, 3.0))).length() < 1e-3);
    assert!((high - (resting + Vec3::new(2.0, 0.2, 3.0))).length() < 1e-3);
    // Overlapping pieces would push each other apart; touching ones lie still.
    run(&mut physics, &mut world, 1.0);
    for (&id, &at) in pieces.iter().zip(&started) {
        let moved = (vector(&world, id, "Position") - at).length();
        assert!(
            moved < 0.05,
            "{} moved {moved}",
            world.get(id).unwrap().name()
        );
    }
}

#[test]
fn a_slow_bump_leaves_the_part_and_a_fast_hit_shatters_it_where_it_hit() {
    let hit = |speed: f32| {
        let (mut world, assets, slab, fracture) = slab("hit", 8.0, v(0.0, 5.0, 0.0));
        set(&mut world, slab, "Anchored", Value::Bool(true));
        set(&mut world, fracture, "Threshold", Value::Number(40.0));
        let ball = part(&mut world, "Ball", v(1.0, 6.5, 1.0), v(1.0, 1.0, 1.0));
        set(
            &mut world,
            ball,
            "AssemblyLinearVelocity",
            Value::Vector3(v(0.0, -speed, 0.0)),
        );
        let mut physics = Physics::with_assets(assets);
        run(&mut physics, &mut world, 0.5);
        (world, physics, ball, fracture)
    };
    let (world, mut physics, ball, _) = hit(10.0);
    assert!(physics.take_shattered().is_empty());
    assert!(world.find_first_child(world.workspace(), "Slab").is_some());
    // It landed on the slab.
    assert!((vector(&world, ball, "Position").y - 5.7).abs() < 0.1);

    let (world, mut physics, ball, fracture) = hit(80.0);
    assert_eq!(physics.take_shattered(), vec![fracture]);
    assert!(world.find_first_child(world.workspace(), "Slab").is_none());
    // The ball went on through, below where the slab was.
    assert!(vector(&world, ball, "Position").y < 4.0);
}

#[test]
fn shattered_pieces_fly_away_from_the_point() {
    let (mut world, assets, slab, fracture) = slab("fly", 12.0, v(0.0, 10.0, 0.0));
    set(&mut world, slab, "Anchored", Value::Bool(true));
    let mut physics = Physics::with_assets(assets);
    physics.step(&mut world, 1.0 / 60.0);
    let point = v(-2.0, 10.0, -3.0);
    assert!(physics.shatter(&mut world, fracture, Some(point), 200.0));
    let (_, pieces) = physics.debris().remove(0);
    for id in pieces {
        let away = vector(&world, id, "Position") - to_vec3(point);
        let velocity = vector(&world, id, "AssemblyLinearVelocity");
        assert!(velocity.dot(away) > 0.0);
        // The slab weighs 4 × 0.4 × 6 = 9.6: up to 200 ÷ 9.6 studs a second.
        assert!(velocity.length() <= 200.0 / 9.6 + 1e-3);
        assert!(velocity.length() >= 100.0 / 9.6 - 1e-3);
    }
}

#[test]
fn a_fracture_without_pieces_does_not_shatter() {
    let mut world = World::new();
    let pane = part(&mut world, "Pane", v(0.0, 5.0, 0.0), v(4.0, 4.0, 0.2));
    let fracture = world.create(ClassName::Fracture).unwrap();
    world.set_parent(fracture, Some(pane)).unwrap();
    let mut physics = Physics::new();
    assert!(!physics.shatter(&mut world, fracture, None, 10.0));
    assert!(physics.take_shattered().is_empty());
    assert_eq!(world.get(pane).unwrap().parent(), Some(world.workspace()));
}
