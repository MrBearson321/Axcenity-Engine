//! Procedural worlds PW5: terrain and objects. Scatter rules keep off
//! anchored parts, lean with the slope and sink into it; Blend into
//! Terrain levels the ground under a footprint.

use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, TerrainData, Value, Vector3, World};
use axcenity_terrain::Placement;
use glam::{Vec2, Vec3};

use crate::blend_footprint;
use crate::obstacles::Obstacle;
use crate::terrain_scatter::TerrainScatter;

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

/// A 256-stud terrain round the origin (129 samples, 2 studs apart, a
/// 100-stud height range) with these heights in studs, by world X and Z.
fn terrain(world: &mut World, height: impl Fn(f32, f32) -> f32) -> InstanceId {
    let workspace = world.workspace();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(workspace)).unwrap();
    set(
        world,
        terrain,
        "Position",
        Value::Vector3(Vector3::default()),
    );
    set(
        world,
        terrain,
        "Size",
        Value::Vector3(Vector3::new(256.0, 100.0, 256.0)),
    );
    let mut data = TerrainData::flat(129, 129, 0.0).unwrap();
    let all = data.bounds();
    let heights = data.heights_mut_in(all);
    for z in 0..129 {
        for x in 0..129 {
            heights[z * 129 + x] = height(x as f32 * 2.0 - 128.0, z as f32 * 2.0 - 128.0) / 100.0;
        }
    }
    world.set_terrain(terrain, Arc::new(data));
    terrain
}

fn trees(world: &mut World, terrain: InstanceId) -> InstanceId {
    let rule = world.create(ClassName::ScatterRule).unwrap();
    world.set_parent(rule, Some(terrain)).unwrap();
    set(world, rule, "Models", Value::String("plant:Tree".into()));
    set(world, rule, "Spacing", Value::Number(3.0));
    set(world, rule, "Density", Value::Number(100.0));
    rule
}

/// An anchored part `size` big standing with its middle at `at`.
fn part(world: &mut World, at: Vec3, size: Vec3, turn: f32) -> InstanceId {
    let workspace = world.workspace();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(workspace)).unwrap();
    set(world, part, "Anchored", Value::Bool(true));
    set(
        world,
        part,
        "Position",
        Value::Vector3(Vector3::new(at.x, at.y, at.z)),
    );
    set(
        world,
        part,
        "Size",
        Value::Vector3(Vector3::new(size.x, size.y, size.z)),
    );
    set(
        world,
        part,
        "Orientation",
        Value::Vector3(Vector3::new(0.0, turn, 0.0)),
    );
    part
}

fn plants(world: &World, terrain: InstanceId) -> Vec<Vec2> {
    let data = world.terrain(terrain).unwrap();
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    data.generated_foliage()
        .map(|i| placement.to_world_xz(data, Vec2::new(i.x, i.z)))
        .collect()
}

#[test]
fn nothing_generated_stands_inside_an_anchored_part_and_moving_it_remakes_only_nearby_chunks() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 10.0);
    let rule = trees(&mut world, land);
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    // A 20 by 12 house floor turned 30 degrees, standing on the ground, and
    // a platform high above it.
    let house = part(
        &mut world,
        Vec3::new(30.0, 13.0, -20.0),
        Vec3::new(20.0, 6.0, 12.0),
        30.0,
    );
    let high = part(
        &mut world,
        Vec3::new(-50.0, 60.0, 40.0),
        Vec3::new(20.0, 1.0, 20.0),
        0.0,
    );
    assert!(scatter.update(&mut world) > 0);
    let inside = |world: &World, id: InstanceId, margin: f32| {
        let obstacle = Obstacle::part_box(world.get(id).unwrap()).unwrap();
        plants(world, land)
            .into_iter()
            .filter(|&p| obstacle.distance_from_above(p) <= margin)
            .count()
    };
    // None under the house or within its 1-stud margin; plenty under the
    // platform, which is far above the ground.
    assert_eq!(inside(&world, house, 0.9), 0);
    assert!(
        inside(&world, high, 0.0) > 10,
        "{}",
        inside(&world, high, 0.0)
    );
    // Nothing changed: nothing made. Moving the house remakes only the
    // chunks near it (of 16).
    assert_eq!(scatter.update(&mut world), 0);
    set(
        &mut world,
        house,
        "Position",
        Value::Vector3(Vector3::new(34.0, 13.0, -20.0)),
    );
    let remade = scatter.update(&mut world);
    assert!((1..16).contains(&remade), "{remade}");
    assert_eq!(inside(&world, house, 0.9), 0);
    // The rule not avoiding parts: plants grow there again.
    set(&mut world, rule, "AvoidParts", Value::Bool(false));
    scatter.update(&mut world);
    assert!(inside(&world, house, 0.0) > 5);
}

#[test]
fn rules_lean_what_they_place_with_the_slope_and_sink_it() {
    let mut world = World::new();
    // Ground rising one stud in two along X: about 27 degrees.
    let land = terrain(&mut world, |x, _| 50.0 + x / 2.0);
    let rule = trees(&mut world, land);
    set(&mut world, rule, "MaxSlope", Value::Number(60.0));
    set(&mut world, rule, "AlignToSlope", Value::Number(1.0));
    set(&mut world, rule, "SinkDepth", Value::Number(0.5));
    set(
        &mut world,
        rule,
        "ScaleRange",
        Value::Vector2(axcenity_core::Vector2::new(2.0, 2.0)),
    );
    TerrainScatter::default().update(&mut world);
    let data = world.terrain(land).unwrap();
    let placement = Placement::of(world.get(land).unwrap()).unwrap();
    let cell = placement.cell_size(data);
    let instance = data
        .generated_foliage()
        .find(|i| i.x > 10.0 && i.x < 118.0 && i.z > 10.0 && i.z < 118.0)
        .expect("trees");
    assert!((instance.lean - 1.0).abs() < 1e-6);
    assert!((instance.sink - 1.0).abs() < 1e-6, "sinks 0.5 at scale 1");
    let up = instance.up(data, (cell.x, cell.y), placement.size.y);
    let up = Vec3::new(up.x, up.y, up.z);
    // Square to the ground: leaning back down the slope (toward -X).
    let normal = Vec3::new(-0.5, 1.0, 0.0).normalize();
    assert!(up.dot(normal) > 0.999, "{up}");
    // Half way, half the tilt.
    let mut half = instance.clone();
    half.lean = 0.5;
    let up = half.up(data, (cell.x, cell.y), placement.size.y);
    let angle = Vec3::new(up.x, up.y, up.z)
        .angle_between(Vec3::Y)
        .to_degrees();
    assert!((angle - 13.3).abs() < 1.0, "{angle}");
}

#[test]
fn blend_levels_and_paints_the_ground_under_a_footprint_and_fades_out_around_it() {
    let mut world = World::new();
    let land = terrain(&mut world, |x, z| 20.0 + x / 8.0 + z / 10.0);
    let house = part(
        &mut world,
        Vec3::new(0.0, 24.0, 0.0),
        Vec3::new(16.0, 8.0, 10.0),
        45.0,
    );
    let obstacle = Obstacle::part_box(world.get(house).unwrap()).unwrap();
    assert!((obstacle.bottom() - 20.0).abs() < 1e-4);
    let placement = Placement::of(world.get(land).unwrap()).unwrap();
    let mut data = TerrainData::clone(world.terrain(land).unwrap());
    let changed = blend_footprint(
        &mut data,
        &placement,
        std::slice::from_ref(&obstacle),
        20.0,
        6.0,
        Some(1),
    );
    assert!(changed > 50, "{changed}");
    let ground = |x: f32, z: f32| placement.ground_y(&data, x, z).unwrap();
    let paint = |x: f32, z: f32| {
        let grid = placement.to_grid(&data, x, z).round();
        data.paint()[data.index(grid.x as usize, grid.y as usize)][1]
    };
    // Under the house (turned 45 degrees): level, painted.
    for (x, z) in [
        (0.0, 0.0),
        (2.0, 2.0),
        (-2.0, -2.0),
        (4.0, -4.0),
        (-4.0, 4.0),
    ] {
        assert!(
            (ground(x, z) - 20.0).abs() < 0.05,
            "{x},{z}: {}",
            ground(x, z)
        );
        assert_eq!(paint(x, z), 255);
    }
    // Far outside: as it was.
    let was = |x: f32, z: f32| 20.0 + x / 8.0 + z / 10.0;
    for (x, z) in [(30.0, 0.0), (0.0, -30.0)] {
        assert!((ground(x, z) - was(x, z)).abs() < 0.05);
        assert_eq!(paint(x, z), 0);
    }
}
