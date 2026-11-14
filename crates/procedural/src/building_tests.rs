//! Procedural worlds PW6: a BuildingGenerator makes the same building from
//! the same seed, stacks its floors, and follows a Spline's corners.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{Vec2, Vec3};

use crate::building::{BuildingSettings, Footprint, MAX_BLOCKS, plan};
use crate::{Generators, rotation_from_orientation, v3};

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn assets() -> Assets {
    Assets::new(std::env::temp_dir())
}

fn building(world: &mut World) -> InstanceId {
    let id = world.create(ClassName::BuildingGenerator).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    id
}

/// Each made part: its name, middle, size and shape.
fn parts(world: &World, generator: InstanceId) -> Vec<(String, Vec3, Vec3, String)> {
    world
        .get(generator)
        .unwrap()
        .children()
        .iter()
        .map(|&id| {
            let part = world.get(id).unwrap();
            let vector = |name| v3(part.property(name).unwrap().as_vector3().unwrap());
            (
                part.name().to_owned(),
                vector("Position"),
                vector("Size"),
                format!("{:?}", part.property("Shape")),
            )
        })
        .collect()
}

fn count(parts: &[(String, Vec3, Vec3, String)], name: &str) -> usize {
    parts.iter().filter(|p| p.0 == name).count()
}

#[test]
fn the_same_seed_gives_the_same_building_and_another_changes_its_windows() {
    let make = |seed: f64| {
        let mut world = World::new();
        let id = building(&mut world);
        set(&mut world, id, "Seed", Value::Number(seed));
        set(&mut world, id, "WindowChance", Value::Number(0.5));
        Generators::default().update(&mut world, &assets());
        parts(&world, id)
    };
    let (a, b, c) = (make(1.0), make(1.0), make(2.0));
    assert!(!a.is_empty());
    assert_eq!(a, b);
    assert_ne!(a, c);
    // The default: 24 by 16 on a 1-stud foundation, 2 floors, a gable roof.
    assert_eq!(count(&a, "Foundation"), 1);
    assert_eq!(count(&a, "Floor"), 2);
    assert_eq!(count(&a, "Corner"), 8);
    assert_eq!(count(&a, "Roof"), 2);
    // One doorway (sill-less: lintel and two piers).
    assert_eq!(count(&a, "Door"), 3);
    // Every bay is a wall, a window or the door: 22 studs of front and
    // back make 3 bays each, 14 of the sides 2 each: 10 a floor.
    let windows = a.iter().filter(|p| p.0 == "Glass").count();
    let walls = count(&a, "Wall");
    assert_eq!(windows + walls + 1, 20, "{windows} {walls}");
}

#[test]
fn floors_stack_on_the_foundation_under_the_roof() {
    let mut world = World::new();
    let id = building(&mut world);
    set(&mut world, id, "Floors", Value::Number(3.0));
    set(&mut world, id, "FloorHeight", Value::Number(12.0));
    set(
        &mut world,
        id,
        "Position",
        Value::Vector3(Vector3::new(100.0, 4.0, -50.0)),
    );
    let mut generators = Generators::default();
    generators.update(&mut world, &assets());
    let made = parts(&world, id);
    // The foundation's top is 4.5; three floors of 12 reach 40.5.
    let top = |p: &(String, Vec3, Vec3, String)| p.1.y + p.2.y / 2.0;
    let walls_top = made
        .iter()
        .filter(|p| p.0 != "Roof")
        .map(top)
        .fold(f32::MIN, f32::max);
    assert!((walls_top - 40.5).abs() < 1e-3, "{walls_top}");
    let floors: Vec<f32> = made.iter().filter(|p| p.0 == "Floor").map(top).collect();
    assert_eq!(floors.len(), 3);
    for (i, y) in floors.iter().enumerate() {
        assert!(
            (y - (4.5 + 12.0 * (i + 1) as f32)).abs() < 1e-3,
            "{floors:?}"
        );
    }
    // The gable's two wedges sit on top, the ridge 6 studs up.
    let roof: Vec<_> = made.iter().filter(|p| p.0 == "Roof").collect();
    assert!(roof.iter().all(|p| p.3.contains("Wedge")));
    assert!(
        roof.iter()
            .all(|p| (p.1.y - p.2.y / 2.0 - 40.5).abs() < 1e-3)
    );
    assert!(roof.iter().all(|p| (top(p) - 46.5).abs() < 1e-3));
    // Everything stands round its middle.
    assert!(
        made.iter()
            .all(|p| Vec2::new(p.1.x - 100.0, p.1.z + 50.0).length() < 16.0)
    );
    // Moving it makes it again there; unchanged, nothing is remade.
    let before = world.get(id).unwrap().children().to_vec();
    generators.update(&mut world, &assets());
    assert_eq!(world.get(id).unwrap().children(), before.as_slice());
    set(
        &mut world,
        id,
        "Position",
        Value::Vector3(Vector3::new(0.0, 4.0, 0.0)),
    );
    generators.update(&mut world, &assets());
    let moved = parts(&world, id);
    assert_eq!(moved.len(), made.len());
    assert!(moved.iter().all(|p| p.1.x.abs() < 16.0));
}

#[test]
fn a_turned_building_faces_its_door_the_way_it_turned() {
    let mut world = World::new();
    let id = building(&mut world);
    set(
        &mut world,
        id,
        "Orientation",
        Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
    );
    Generators::default().update(&mut world, &assets());
    // Turned 90 degrees, its front (-Z) faces -X: the door is on that side.
    let door: Vec<Vec3> = parts(&world, id)
        .into_iter()
        .filter(|p| p.0 == "Door")
        .map(|p| p.1)
        .collect();
    let front = rotation_from_orientation(Vec3::new(0.0, 90.0, 0.0)) * Vec3::NEG_Z;
    assert!(front.x < -0.99);
    assert!(door.iter().all(|p| (p.x - -7.5).abs() < 1e-3), "{door:?}");
}

#[test]
fn inside_a_spline_its_points_are_the_corners_and_any_shape_gets_a_flat_roof() {
    let mut world = World::new();
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(world.workspace())).unwrap();
    // An L, 300 square studs, its lowest point at 2.
    for (x, y, z) in [
        (0.0, 2.0, 0.0),
        (20.0, 3.0, 0.0),
        (20.0, 3.0, 10.0),
        (10.0, 3.0, 10.0),
        (10.0, 3.0, 20.0),
        (0.0, 3.0, 20.0),
    ] {
        let point = world.create(ClassName::SplinePoint).unwrap();
        world.set_parent(point, Some(spline)).unwrap();
        set(
            &mut world,
            point,
            "Position",
            Value::Vector3(Vector3::new(x, y, z)),
        );
    }
    let id = world.create(ClassName::BuildingGenerator).unwrap();
    world.set_parent(id, Some(spline)).unwrap();
    let footprint = Footprint::of(&world, id).unwrap();
    assert_eq!(footprint.corners.len(), 6);
    assert!((footprint.base - 3.0).abs() < 1e-4);
    assert!(!footprint.is_rectangle());
    Generators::default().update(&mut world, &assets());
    let made = parts(&world, id);
    // Slabs over an L are wedges; its roof is flat even though Gable is
    // set, and covers the L and its overhang.
    let roof: Vec<_> = made.iter().filter(|p| p.0 == "Roof").collect();
    assert!(!roof.is_empty() && roof.iter().all(|p| p.3.contains("Wedge")));
    let covered: f32 = roof.iter().map(|p| p.2.y * p.2.z / 2.0).sum();
    assert!(covered > 300.0 && covered < 360.0, "{covered}");
    let foundation: f32 = made
        .iter()
        .filter(|p| p.0 == "Foundation")
        .map(|p| p.2.y * p.2.z / 2.0)
        .sum();
    assert!((foundation - 300.0).abs() < 1e-2, "{foundation}");
    // Walls stand inside the L, along its sides.
    for p in made.iter().filter(|p| p.0 == "Wall" || p.0 == "Window") {
        let (x, z) = (p.1.x, p.1.z);
        let inside =
            (0.0..=20.0).contains(&x) && (0.0..=20.0).contains(&z) && !(x > 10.0 && z > 10.0);
        assert!(inside, "{p:?}");
    }
}

#[test]
fn module_models_stand_in_for_bays_and_a_huge_building_stops_at_the_limit() {
    let mut world = World::new();
    let id = building(&mut world);
    set(
        &mut world,
        id,
        "WindowModel",
        Value::String("assets/models/window.glb".into()),
    );
    let settings = BuildingSettings::of(world.get(id).unwrap());
    let footprint = Footprint::of(&world, id).unwrap();
    let blocks = plan(&footprint, &settings);
    assert!(blocks.iter().all(|b| b.name != "Glass"));
    let windows = blocks.iter().filter(|b| b.name == "Window").count();
    assert!(windows > 5, "{windows}");
    // A missing model is reported, and the rest is made anyway.
    let problems = Generators::default().update(&mut world, &assets());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("window.glb"));
    assert!(!parts(&world, id).is_empty());
    set(
        &mut world,
        id,
        "Size",
        Value::Vector3(Vector3::new(2000.0, 1.0, 2000.0)),
    );
    set(&mut world, id, "Floors", Value::Number(50.0));
    let settings = BuildingSettings::of(world.get(id).unwrap());
    let footprint = Footprint::of(&world, id).unwrap();
    assert_eq!(plan(&footprint, &settings).len(), MAX_BLOCKS);
}
