//! Phase 74: SplineMesh pieces, SplineScatter layouts and Flatten Terrain
//! Along, with the hello-roads example's models.

use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_core::spline::SplineCurve;
use axcenity_core::{ClassName, InstanceId, TerrainData, Value, Vector2, Vector3, World};
use axcenity_terrain::Placement;
use glam::Vec3;

use super::*;

fn example_assets() -> Assets {
    Assets::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-roads"))
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

/// A Spline in the Workspace through `points`.
fn spline(world: &mut World, points: &[(f32, f32, f32)]) -> InstanceId {
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(world.workspace())).unwrap();
    for &(x, y, z) in points {
        let point = world.create(ClassName::SplinePoint).unwrap();
        world.set_parent(point, Some(spline)).unwrap();
        set(
            world,
            point,
            "Position",
            Value::Vector3(Vector3::new(x, y, z)),
        );
    }
    spline
}

fn add(world: &mut World, class: ClassName, parent: InstanceId) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A curving road: straight on, then bending right and down a little.
fn road(world: &mut World) -> (InstanceId, InstanceId) {
    let path = spline(
        world,
        &[
            (0.0, 0.0, 30.0),
            (0.0, 0.0, -20.0),
            (25.0, -2.0, -60.0),
            (70.0, -3.0, -75.0),
        ],
    );
    let road = add(world, ClassName::SplineMesh, path);
    set(
        world,
        road,
        "MeshId",
        Value::String("assets/models/road.glb".into()),
    );
    set(
        world,
        road,
        "Size",
        Value::Vector3(Vector3::new(12.0, 1.05, 8.0)),
    );
    (path, road)
}

/// Every corner of `part`'s model where it's drawn (it isn't turned).
fn corners(world: &World, assets: &Assets, part: InstanceId) -> Vec<Vec3> {
    let instance = world.get(part).unwrap();
    let property = |name| v3(instance.property(name).unwrap().as_vector3().unwrap());
    let (position, size) = (property("Position"), property("Size"));
    let mesh_id = instance.property("MeshId").unwrap().as_str().unwrap();
    let model = assets.model_blocking(mesh_id).unwrap();
    model
        .primitives
        .iter()
        .flat_map(|primitive| primitive.positions.iter())
        .map(|&p| position + Vec3::from(p) * size)
        .collect()
}

fn children(world: &World, id: InstanceId) -> Vec<InstanceId> {
    world.get(id).unwrap().children().to_vec()
}

#[test]
fn a_road_of_one_straight_segment_follows_a_curve_with_no_gaps() {
    let assets = example_assets();
    let mut world = World::new();
    let (path, road) = road(&mut world);
    let mut generators = Generators::default();
    assert!(generators.update(&mut world, &assets).is_empty());

    let curve = SplineCurve::of(&world, path).unwrap();
    let pieces = children(&world, road);
    let count = piece_count(&curve, 8.0);
    assert_eq!(pieces.len(), count);
    assert!(count > 10, "{count}");
    let length = curve.length() / count as f32;
    for (i, pair) in pieces.windows(2).enumerate() {
        // Where two pieces meet, the end of one is the start of the next:
        // the same corners, in the plane across the curve there.
        let (point, _, _, forward) = frame(&curve, (i + 1) as f32 * length);
        let on_seam = |part| -> Vec<Vec3> {
            corners(&world, &assets, part)
                .into_iter()
                .filter(|c| (*c - point).dot(forward).abs() < 2e-3)
                .collect()
        };
        let (end, start) = (on_seam(pair[0]), on_seam(pair[1]));
        assert!(end.len() >= 8, "piece {i} ends with {} corners", end.len());
        for corner in &end {
            assert!(
                start.iter().any(|other| other.distance(*corner) < 2e-3),
                "a gap after piece {i} at {corner}"
            );
        }
    }
    // Every corner stays within the road's half width and height of the
    // curve, so the straight segment bent to follow it.
    for &piece in &pieces {
        for corner in corners(&world, &assets, piece) {
            let nearest = v3(curve.position_at_distance(curve.closest_distance(vector3(corner))));
            assert!(corner.distance(nearest) < 6.2, "{corner} is off the curve");
        }
    }
    // The pieces are anchored and collide.
    let first = world.get(pieces[0]).unwrap();
    assert_eq!(first.property("Anchored"), Some(&Value::Bool(true)));
    assert_eq!(first.property("CanCollide"), Some(&Value::Bool(true)));
}

#[test]
fn nothing_is_made_again_until_the_spline_or_settings_change() {
    let assets = example_assets();
    let mut world = World::new();
    let (path, road) = road(&mut world);
    let mut generators = Generators::default();
    generators.update(&mut world, &assets);
    let before = children(&world, road);
    generators.update(&mut world, &assets);
    assert_eq!(children(&world, road), before, "unchanged, so kept");

    set(
        &mut world,
        road,
        "Size",
        Value::Vector3(Vector3::new(12.0, 1.05, 4.0)),
    );
    generators.update(&mut world, &assets);
    let shorter = children(&world, road);
    assert!(shorter.len() > before.len() + 5);
    assert!(before.iter().all(|&id| !world.contains(id)));

    // A piece deleted by hand comes back.
    world.destroy(shorter[0]).unwrap();
    generators.update(&mut world, &assets);
    assert_eq!(children(&world, road).len(), shorter.len());

    // Moving the spline away from the road's parent leaves nothing.
    world.set_parent(road, Some(world.workspace())).unwrap();
    generators.update(&mut world, &assets);
    assert!(children(&world, road).is_empty());
    let _ = path;
}

/// A scatter of trees and rocks along a closed loop on flat ground.
fn scattered(world: &mut World) -> (InstanceId, InstanceId) {
    let ground = add(world, ClassName::Part, world.workspace());
    set(world, ground, "Anchored", Value::Bool(true));
    set(
        world,
        ground,
        "Position",
        Value::Vector3(Vector3::new(0.0, -0.5, 0.0)),
    );
    set(
        world,
        ground,
        "Size",
        Value::Vector3(Vector3::new(200.0, 1.0, 200.0)),
    );
    let path = spline(
        world,
        &[
            (-30.0, 0.0, -30.0),
            (30.0, 0.0, -30.0),
            (30.0, 0.0, 30.0),
            (-30.0, 0.0, 30.0),
        ],
    );
    set(world, path, "Closed", Value::Bool(true));
    let scatter = add(world, ClassName::SplineScatter, path);
    set(
        world,
        scatter,
        "Models",
        Value::String("assets/models/tree.glb; assets/models/rock.glb".into()),
    );
    set(world, scatter, "Seed", Value::Number(7.0));
    set(world, scatter, "Offset", Value::Number(3.0));
    set(
        world,
        scatter,
        "ScaleRange",
        Value::Vector2(Vector2::new(0.8, 1.4)),
    );
    (path, scatter)
}

/// Each placed model's file, position, orientation and size.
fn placements(world: &World, scatter: InstanceId) -> Vec<(String, Vector3, Vector3, Vector3)> {
    children(world, scatter)
        .into_iter()
        .map(|id| {
            let instance = world.get(id).unwrap();
            let vector = |name| instance.property(name).unwrap().as_vector3().unwrap();
            (
                instance
                    .property("MeshId")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                vector("Position"),
                vector("Orientation"),
                vector("Size"),
            )
        })
        .collect()
}

#[test]
fn moving_a_spline_point_makes_the_scatter_again_and_the_same_seed_gives_the_same_layout() {
    let assets = example_assets();
    let mut world = World::new();
    let (path, scatter) = scattered(&mut world);
    let mut generators = Generators::default();
    assert!(generators.update(&mut world, &assets).is_empty());
    let first = placements(&world, scatter);
    // Every 8 studs round a loop about 240 long; both models used.
    assert!(first.len() >= 25, "{}", first.len());
    assert!(first.iter().any(|p| p.0.ends_with("tree.glb")));
    assert!(first.iter().any(|p| p.0.ends_with("rock.glb")));
    // Each stands on the ground (its box's bottom at Y = 0).
    for (_, position, _, size) in &first {
        assert!(
            (position.y - size.y / 2.0).abs() < 1e-3,
            "{position:?} {size:?}"
        );
    }

    // A fresh world with the same settings (a project opened again) has the
    // same layout.
    let mut again = World::new();
    let (_, scatter_again) = scattered(&mut again);
    Generators::default().update(&mut again, &assets);
    assert_eq!(placements(&again, scatter_again), first);

    // Moving a point makes it again, along the new curve.
    let corner = children(&world, path)[2];
    set(
        &mut world,
        corner,
        "Position",
        Value::Vector3(Vector3::new(60.0, 0.0, 60.0)),
    );
    generators.update(&mut world, &assets);
    let moved = placements(&world, scatter);
    assert_ne!(moved, first);
    assert!(moved.len() > first.len());
    assert!(moved.iter().any(|p| p.1.x > 35.0));

    // Another seed is another layout.
    set(&mut world, scatter, "Seed", Value::Number(8.0));
    generators.update(&mut world, &assets);
    assert_ne!(placements(&world, scatter), moved);
}

#[test]
fn a_scatter_fills_a_closed_spline_and_leaves_out_steep_ground() {
    let assets = example_assets();
    let mut world = World::new();
    let (path, scatter) = scattered(&mut world);
    // A square: straight sides between its corners.
    for point in children(&world, path) {
        if world.get(point).unwrap().class() != ClassName::SplinePoint {
            continue;
        }
        let linear = Value::EnumItem(axcenity_core::EnumItem {
            enum_name: "SplineTangentMode".to_owned(),
            item: "Linear".to_owned(),
        });
        set(&mut world, point, "TangentMode", linear);
    }
    set(&mut world, scatter, "FillInside", Value::Bool(true));
    set(&mut world, scatter, "Density", Value::Number(1.0));
    set(&mut world, scatter, "Offset", Value::Number(0.0));
    // A steep ramp (50°) over the middle of the square, clear of the ground.
    let workspace = world.workspace();
    let ramp = add(&mut world, ClassName::Part, workspace);
    set(&mut world, ramp, "Anchored", Value::Bool(true));
    set(
        &mut world,
        ramp,
        "Position",
        Value::Vector3(Vector3::new(0.0, 8.5, 0.0)),
    );
    set(
        &mut world,
        ramp,
        "Size",
        Value::Vector3(Vector3::new(20.0, 1.0, 60.0)),
    );
    set(
        &mut world,
        ramp,
        "Orientation",
        Value::Vector3(Vector3::new(0.0, 0.0, 50.0)),
    );
    Generators::default().update(&mut world, &assets);
    let placed = placements(&world, scatter);
    // 3600 square studs at 1 per 100, less those over the ramp.
    assert!(placed.len() > 20 && placed.len() < 36, "{}", placed.len());
    for (_, position, orientation, size) in &placed {
        assert!(
            position.x.abs() < 30.0 && position.z.abs() < 30.0,
            "{position:?}"
        );
        // The ramp reaches 6.5 studs either side of the middle: nothing on it.
        assert!(position.x.abs() > 6.0, "{position:?} is on the ramp");
        // Upright on the flat ground.
        assert!(
            orientation.x.abs() < 0.01 && orientation.z.abs() < 0.01,
            "{orientation:?}"
        );
        assert!((position.y - size.y / 2.0).abs() < 1e-3, "{position:?}");
    }
}

#[test]
fn flatten_along_levels_a_road_wide_strip_to_the_curve() {
    let mut world = World::new();
    let workspace = world.workspace();
    let terrain = add(&mut world, ClassName::Terrain, workspace);
    set(
        &mut world,
        terrain,
        "Size",
        Value::Vector3(Vector3::new(128.0, 64.0, 128.0)),
    );
    set(
        &mut world,
        terrain,
        "Position",
        Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
    );
    let mut data = TerrainData::flat(129, 129, 0.25).unwrap();
    // Bumpy ground.
    for z in 0..129 {
        for x in 0..129 {
            let rect = axcenity_core::SampleRect::new(x, z, x + 1, z + 1);
            data.heights_mut_in(rect)[z * 129 + x] = 0.25 + 0.05 * ((x as f32 * 0.3).sin());
        }
    }
    let path = spline(
        &mut world,
        &[(-40.0, 20.0, 0.0), (0.0, 20.0, 10.0), (40.0, 20.0, 0.0)],
    );
    let curve = SplineCurve::of(&world, path).unwrap();
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    let set_count = flatten_along(&mut data, &placement, &curve, 12.0);
    assert!(set_count > 500, "{set_count}");
    world.set_terrain(terrain, Arc::new(data));
    let data = world.terrain(terrain).unwrap();
    // On the curve and 5 studs to its side, the ground is at its height.
    for d in [10.0, curve.length() / 2.0, curve.length() - 10.0] {
        let (point, right, _, _) = frame(&curve, d);
        for side in [-5.0, 0.0, 5.0] {
            let at = point + right * side;
            let ground = placement.ground_y(data, at.x, at.z).unwrap();
            assert!((ground - point.y).abs() < 0.3, "{ground} vs {}", point.y);
        }
    }
    // Far from it, the bumps are still there.
    let away = placement.ground_y(data, 0.0, -40.0).unwrap();
    assert!((away - 20.0).abs() > 1.0, "{away}");
}
