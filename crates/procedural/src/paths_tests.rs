//! Procedural worlds PW4: roads and paths on the terrain.

use std::sync::Arc;

use axcenity_core::spline::SplineCurve;
use axcenity_core::{ClassName, InstanceId, TerrainData, Value, Vector3, World};
use axcenity_terrain::Placement;
use glam::{Vec2, Vec3};

use crate::paths::{PathSettings, apply_path, keep_offs};
use crate::route::route;
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
            let (wx, wz) = (x as f32 * 2.0 - 128.0, z as f32 * 2.0 - 128.0);
            heights[z * 129 + x] = height(wx, wz) / 100.0;
        }
    }
    world.set_terrain(terrain, Arc::new(data));
    terrain
}

fn placement(world: &World, terrain: InstanceId) -> Placement {
    Placement::of(world.get(terrain).unwrap()).unwrap()
}

/// A Spline through `points` with a TerrainPath in it.
fn path(world: &mut World, points: &[(f32, f32, f32)]) -> (InstanceId, InstanceId) {
    let workspace = world.workspace();
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(workspace)).unwrap();
    for &(x, y, z) in points {
        let point = world.create(ClassName::SplinePoint).unwrap();
        set(
            world,
            point,
            "Position",
            Value::Vector3(Vector3::new(x, y, z)),
        );
        world.set_parent(point, Some(spline)).unwrap();
    }
    let path = world.create(ClassName::TerrainPath).unwrap();
    world.set_parent(path, Some(spline)).unwrap();
    (spline, path)
}

/// World X and Z of every generated plant.
fn plants(world: &World, terrain: InstanceId) -> Vec<Vec2> {
    let data = world.terrain(terrain).unwrap();
    let placement = placement(world, terrain);
    data.generated_foliage()
        .map(|i| placement.to_world_xz(data, Vec2::new(i.x, i.z)))
        .collect()
}

#[test]
fn scatter_rules_leave_a_paths_strip_empty_and_moving_it_remakes_only_chunks_near_it() {
    let mut world = World::new();
    let land = terrain(&mut world, |_, _| 10.0);
    let rule = world.create(ClassName::ScatterRule).unwrap();
    world.set_parent(rule, Some(land)).unwrap();
    set(
        &mut world,
        rule,
        "Models",
        Value::String("plant:Tree".into()),
    );
    set(&mut world, rule, "Spacing", Value::Number(3.0));
    set(&mut world, rule, "Density", Value::Number(100.0));
    let mut scatter = TerrainScatter::default();
    scatter.update(&mut world);
    let near_road = |plants: &[Vec2], reach: f32| {
        plants
            .iter()
            .filter(|p| p.y.abs() < reach && p.x.abs() < 100.0)
            .count()
    };
    let before = plants(&world, land);
    assert!(near_road(&before, 5.0) > 20, "{}", near_road(&before, 5.0));

    // A straight road along X at Z = 0: 8 wide, kept 1 stud past its edges.
    let (spline, road) = path(&mut world, &[(-100.0, 10.0, 0.0), (100.0, 10.0, 0.0)]);
    assert_eq!(keep_offs(&world).len(), 1);
    assert!(scatter.update(&mut world) > 0);
    let after = plants(&world, land);
    assert_eq!(near_road(&after, 5.0), 0, "plants on the road");
    // Away from it, the same plants as before.
    let far = |plants: &[Vec2]| {
        let mut far: Vec<_> = plants
            .iter()
            .filter(|p| p.y.abs() > 20.0)
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .collect();
        far.sort();
        far
    };
    assert_eq!(far(&before), far(&after));

    // Nothing changed: nothing made. Moving one end remakes only chunks
    // near the road (of 16 on this terrain).
    assert_eq!(scatter.update(&mut world), 0);
    let end = world.get(spline).unwrap().children()[1];
    set(
        &mut world,
        end,
        "Position",
        Value::Vector3(Vector3::new(100.0, 10.0, 6.0)),
    );
    let remade = scatter.update(&mut world);
    assert!((1..16).contains(&remade), "{remade} chunks remade");

    // Off, or not clearing scatter, the plants come back.
    set(&mut world, road, "ClearScatter", Value::Bool(false));
    scatter.update(&mut world);
    assert!(near_road(&plants(&world, land), 5.0) > 20);
}

#[test]
fn apply_flattens_and_paints_within_width_and_blends_over_its_falloff() {
    let mut world = World::new();
    // Ground sloping up along Z, a stud every 4.
    let land = terrain(&mut world, |_, z| 30.0 + z / 4.0);
    let (spline, road) = path(&mut world, &[(-60.0, 30.0, 0.0), (60.0, 30.0, 0.0)]);
    set(&mut world, road, "Width", Value::Number(10.0));
    set(&mut world, road, "Falloff", Value::Number(6.0));
    let settings = PathSettings::of(world.get(road).unwrap()).unwrap();
    let curve = SplineCurve::of(&world, spline).unwrap();
    let placement = placement(&world, land);
    let mut data = TerrainData::clone(world.terrain(land).unwrap());
    let changed = apply_path(&mut data, &placement, &curve, &settings, Some(2));
    assert!(changed > 300, "{changed}");
    let ground = |z: f32| placement.ground_y(&data, 0.0, z).unwrap();
    let paint = |z: f32| {
        let grid = placement.to_grid(&data, 0.0, z);
        data.paint()[data.index(grid.x as usize, grid.y as usize)][2]
    };
    // Within the width: level with the road, fully painted.
    for z in [-4.0, 0.0, 4.0] {
        assert!((ground(z) - 30.0).abs() < 0.05, "{z}: {}", ground(z));
        assert_eq!(paint(z), 255, "{z}");
    }
    // In the falloff: between the road and the slope, partly painted.
    let blend = ground(8.0);
    assert!(blend > 30.05 && blend < 32.0, "{blend}");
    assert!((1..255).contains(&paint(8.0)), "{}", paint(8.0));
    // Past it: untouched.
    for z in [-12.0, 12.0] {
        assert!((ground(z) - (30.0 + z / 4.0)).abs() < 0.05, "{z}");
        assert_eq!(paint(z), 0);
    }
    // Flatten off and no layer: nothing to do.
    set(&mut world, road, "Flatten", Value::Bool(false));
    let settings = PathSettings::of(world.get(road).unwrap()).unwrap();
    assert_eq!(
        apply_path(&mut data, &placement, &curve, &settings, None),
        0
    );
}

#[test]
fn route_winds_up_a_steep_hill_keeping_every_stretch_under_its_grade() {
    let mut world = World::new();
    // A cone about 22 degrees steep, 45 studs high, with a flat top.
    let land = terrain(&mut world, |x, z| {
        (50.0 - Vec2::new(x, z).length() * 0.4).clamp(5.0, 45.0)
    });
    let placement = placement(&world, land);
    let data = world.terrain(land).unwrap();
    let (from, to) = (Vec3::new(-110.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0));
    let grade = 10.0_f32;
    let points = route(data, &placement, (from, to), grade, 8.0).expect("a way up");
    assert!(points.len() > 4, "{points:?}");
    let flat = |p: Vec3| Vec2::new(p.x, p.z);
    assert!(flat(points[0]).distance(flat(from)) < 1e-3);
    assert!(flat(*points.last().unwrap()).distance(flat(to)) < 1e-3);
    let limit = grade.to_radians().tan();
    let mut length = 0.0;
    for pair in points.windows(2) {
        let run = flat(pair[0]).distance(flat(pair[1]));
        length += run;
        let climb = (pair[1].y - pair[0].y).abs();
        assert!(
            climb <= limit * run + 0.6,
            "{:?} to {:?}: {} degrees",
            pair[0],
            pair[1],
            (climb / run).atan().to_degrees()
        );
    }
    // 43 studs up at 10 degrees takes at least 244 studs: much further
    // than straight up the slope.
    assert!(length > 240.0, "{length}");
    // Each point is on the ground, or cut into it or built up over it by
    // at most half the road's width.
    for p in &points {
        let ground = placement.ground_y(data, p.x, p.z).unwrap();
        assert!((ground - p.y).abs() <= 4.01, "{p:?} vs {ground}");
    }
    // The curve through them climbs within the grade (a little over where
    // it might round a turn), measured over 4-stud stretches.
    let mut world = World::new();
    let (spline, _) = path(
        &mut world,
        &points.iter().map(|p| (p.x, p.y, p.z)).collect::<Vec<_>>(),
    );
    let curve = SplineCurve::of(&world, spline).unwrap();
    let line = curve.polyline(4.0);
    let steepest = line
        .windows(2)
        .map(|pair| {
            let (a, b) = (crate::v3(pair[0]), crate::v3(pair[1]));
            ((b.y - a.y).abs() / flat(a).distance(flat(b)).max(1e-3))
                .atan()
                .to_degrees()
        })
        .fold(0.0_f32, f32::max);
    assert!(steepest < grade + 1.0, "{steepest} degrees");
}

#[test]
fn route_gives_up_where_no_way_is_within_the_grade() {
    let mut world = World::new();
    // A wall: 60 studs up in 2.
    let land = terrain(&mut world, |x, _| if x < 0.0 { 0.0 } else { 60.0 });
    let placement = placement(&world, land);
    let data = world.terrain(land).unwrap();
    let points = route(
        data,
        &placement,
        (Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0)),
        10.0,
        8.0,
    );
    assert_eq!(points, None);
    // Off the terrain: none either.
    assert_eq!(
        route(
            data,
            &placement,
            (Vec3::new(-500.0, 0.0, 0.0), Vec3::ZERO),
            10.0,
            8.0
        ),
        None
    );
}
