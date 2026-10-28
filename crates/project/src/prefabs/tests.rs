use std::collections::HashMap;

use axcenity_core::prefab::{Overrides, place};
use axcenity_core::{ClassName, Color3, Value, Vector3};

use super::*;
use crate::world_file::{parse_world, write_prefab, write_world};

const WHEEL: &str = "assets/prefabs/Wheel.prefab.json";
const CAR: &str = "assets/prefabs/Car.prefab.json";
const ENEMY: &str = "assets/prefabs/Enemy.prefab.json";
const RED_ENEMY: &str = "assets/prefabs/Red Enemy.prefab.json";

type Files = HashMap<String, String>;

fn prefabs(files: &Files) -> Prefabs<'_> {
    Prefabs::new(|path| {
        files
            .get(path)
            .cloned()
            .ok_or_else(|| "not found".to_owned())
    })
}

fn color(r: f32, g: f32, b: f32) -> Value {
    Value::Color3(Color3::new(r, g, b))
}

fn property(world: &World, id: InstanceId, name: &str) -> Value {
    world.get(id).unwrap().property(name).unwrap().clone()
}

/// The text of a prefab of one Part named `name` with `color`.
fn part_prefab(name: &str, color: Value) -> String {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    world.set_property(part, "Color", color).unwrap();
    write_prefab(&world, &[part])
}

/// A Car prefab: a Model holding a Body and four Wheel copies, each moved.
fn car_prefab(files: &Files) -> String {
    let mut world = World::new();
    let car = world.create(ClassName::Model).unwrap();
    world.set_name(car, "Car").unwrap();
    world.set_parent(car, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_name(body, "Body").unwrap();
    world.set_parent(body, Some(car)).unwrap();
    let mut prefabs = prefabs(files);
    for x in 0..4 {
        let wheel = prefabs.instantiate(WHEEL, &mut world, car).unwrap()[0];
        let at = Value::Vector3(Vector3::new(x as f32, 0.0, 0.0));
        world.set_property(wheel, "Position", at).unwrap();
    }
    write_prefab(&world, &[car])
}

/// Applies every override of the copy at `root` to its prefab's file.
fn apply(files: &mut Files, world: &World, root: InstanceId) {
    let path = world.get(root).unwrap().prefab().unwrap().path.clone();
    let text = applied_text(&mut prefabs(files), world, root).unwrap();
    files.insert(path, text);
}

/// What the editor does after a prefab file changes: every copy follows.
fn refresh(files: &Files, world: &mut World) {
    let mut prefabs = prefabs(files);
    let root = world.root();
    prefabs.refresh(world, root);
    assert_eq!(prefabs.take_problems(), []);
}

fn wheels_of(world: &World, car: InstanceId) -> Vec<InstanceId> {
    world
        .get(car)
        .unwrap()
        .children()
        .iter()
        .copied()
        .filter(|&id| world.get(id).unwrap().name() == "Wheel")
        .collect()
}

#[test]
fn changing_the_wheel_prefab_updates_the_wheels_in_every_car_copy() {
    let black = color(0.1, 0.1, 0.1);
    let mut files = Files::from([(WHEEL.to_owned(), part_prefab("Wheel", black.clone()))]);
    files.insert(CAR.to_owned(), car_prefab(&files));

    // Two Cars in the world, one moved.
    let mut world = World::new();
    let workspace = world.workspace();
    let mut cars = Vec::new();
    for _ in 0..2 {
        cars.push(
            prefabs(&files)
                .instantiate(CAR, &mut world, workspace)
                .unwrap()[0],
        );
    }
    let body = world.find_first_child(cars[1], "Body").unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 0.0, 9.0)),
        )
        .unwrap();
    let saved = write_world(&world);

    // Change the Wheel's colour through a placed Wheel and Apply.
    let lone = prefabs(&files)
        .instantiate(WHEEL, &mut world, workspace)
        .unwrap()[0];
    let orange = color(1.0, 0.5, 0.0);
    world.set_property(lone, "Color", orange.clone()).unwrap();
    apply(&mut files, &world, lone);
    refresh(&files, &mut world);

    for &car in &cars {
        let wheels = wheels_of(&world, car);
        assert_eq!(wheels.len(), 4);
        for (x, &wheel) in wheels.iter().enumerate() {
            assert_eq!(property(&world, wheel, "Color"), orange);
            let at = Value::Vector3(Vector3::new(x as f32, 0.0, 0.0));
            assert_eq!(
                property(&world, wheel, "Position"),
                at,
                "kept where the Car put it"
            );
        }
    }
    assert!(prefab::overrides(&world, lone).is_empty());

    // A world saved before the change picks it up when it's opened.
    let mut opened = parse_world(&saved).unwrap();
    refresh(&files, &mut opened);
    let car = opened.get(opened.workspace()).unwrap().children()[1];
    for wheel in wheels_of(&opened, car) {
        assert_eq!(property(&opened, wheel, "Color"), orange);
    }
    let body = opened.find_first_child(car, "Body").unwrap();
    assert_eq!(
        property(&opened, body, "Position"),
        Value::Vector3(Vector3::new(0.0, 0.0, 9.0)),
        "the copy's own change is kept"
    );
}

/// An Enemy prefab: a Model with a Body and a Health.
fn enemy_prefab(body: Value, max_health: f64) -> String {
    let mut world = World::new();
    let enemy = world.create(ClassName::Model).unwrap();
    world.set_name(enemy, "Enemy").unwrap();
    world.set_parent(enemy, Some(world.workspace())).unwrap();
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, "Body").unwrap();
    world.set_parent(part, Some(enemy)).unwrap();
    world.set_property(part, "Color", body).unwrap();
    let humanoid = world.create(ClassName::Health).unwrap();
    world.set_parent(humanoid, Some(enemy)).unwrap();
    world
        .set_property(humanoid, "MaxHealth", Value::Number(max_health))
        .unwrap();
    write_prefab(&world, &[enemy])
}

#[test]
fn a_variant_keeps_its_own_colour_but_takes_the_bases_new_max_health() {
    let grey = color(0.5, 0.5, 0.5);
    let red = color(1.0, 0.0, 0.0);
    let mut files = Files::from([(ENEMY.to_owned(), enemy_prefab(grey.clone(), 100.0))]);
    let variant = variant_text(&mut prefabs(&files), ENEMY).unwrap();
    files.insert(RED_ENEMY.to_owned(), variant);

    // Make the variant red: place it, paint it, apply.
    let mut world = World::new();
    let workspace = world.workspace();
    let red_enemy = prefabs(&files)
        .instantiate(RED_ENEMY, &mut world, workspace)
        .unwrap()[0];
    let body = world.find_first_child(red_enemy, "Body").unwrap();
    world.set_property(body, "Color", red.clone()).unwrap();
    apply(&mut files, &world, red_enemy);
    assert!(files[RED_ENEMY].contains(r#""prefab": "assets/prefabs/Enemy.prefab.json""#));

    // The base changes colour and MaxHealth.
    let enemy = prefabs(&files)
        .instantiate(ENEMY, &mut world, workspace)
        .unwrap()[0];
    let enemy_body = world.find_first_child(enemy, "Body").unwrap();
    let blue = color(0.0, 0.0, 1.0);
    world
        .set_property(enemy_body, "Color", blue.clone())
        .unwrap();
    let humanoid = world.find_first_child(enemy, "Health").unwrap();
    world
        .set_property(humanoid, "MaxHealth", Value::Number(250.0))
        .unwrap();
    apply(&mut files, &world, enemy);
    refresh(&files, &mut world);

    assert_eq!(property(&world, body, "Color"), red);
    let red_humanoid = world.find_first_child(red_enemy, "Health").unwrap();
    assert_eq!(
        property(&world, red_humanoid, "MaxHealth"),
        Value::Number(250.0)
    );
    // A Red Enemy placed afresh is the same.
    let fresh = prefabs(&files)
        .instantiate(RED_ENEMY, &mut world, workspace)
        .unwrap()[0];
    let fresh_body = world.find_first_child(fresh, "Body").unwrap();
    assert_eq!(property(&world, fresh_body, "Color"), red);
    let fresh_humanoid = world.find_first_child(fresh, "Health").unwrap();
    assert_eq!(
        property(&world, fresh_humanoid, "MaxHealth"),
        Value::Number(250.0)
    );
}

#[test]
fn applying_one_copys_change_reaches_the_other_copies() {
    let grey = color(0.5, 0.5, 0.5);
    let mut files = Files::from([(WHEEL.to_owned(), part_prefab("Wheel", grey.clone()))]);
    let mut world = World::new();
    let workspace = world.workspace();
    let first = prefabs(&files)
        .instantiate(WHEEL, &mut world, workspace)
        .unwrap()[0];
    let second = prefabs(&files)
        .instantiate(WHEEL, &mut world, workspace)
        .unwrap()[0];
    world
        .set_property(
            second,
            "Position",
            Value::Vector3(Vector3::new(4.0, 0.0, 0.0)),
        )
        .unwrap();
    let green = color(0.0, 1.0, 0.0);
    world.set_property(first, "Color", green.clone()).unwrap();
    let place = place(&world, first).unwrap();
    assert!(place.here().properties.contains("Color"));

    apply(&mut files, &world, first);
    refresh(&files, &mut world);
    assert_eq!(property(&world, second, "Color"), green);
    assert_eq!(
        property(&world, second, "Position"),
        Value::Vector3(Vector3::new(4.0, 0.0, 0.0))
    );
    assert!(prefab::overrides(&world, first).is_empty());
}

#[test]
fn a_prefab_saved_before_phase_49_loads_and_saves_unchanged() {
    // Written as before this phase: nothing in it is a linked copy.
    let old = enemy_prefab(color(0.2, 0.2, 0.2), 100.0);
    for field in ["\"prefab\"", "\"overrides\"", "\"added\"", "\"removed\""] {
        assert!(!old.contains(field));
    }
    let mut plain = World::new();
    let workspace = plain.workspace();
    let roots = parse_prefab(&old, &mut plain, workspace).unwrap();
    assert_eq!(write_prefab(&plain, &roots), old);

    // Placed through the loader, it's the same Instances, now linked.
    let files = Files::from([(ENEMY.to_owned(), old.clone())]);
    let mut linked = World::new();
    let workspace = linked.workspace();
    let placed = prefabs(&files)
        .instantiate(ENEMY, &mut linked, workspace)
        .unwrap();
    assert!(prefab::overrides(&linked, placed[0]).is_empty());
    linked.set_prefab(placed[0], None).unwrap();
    assert_eq!(write_prefab(&linked, &placed), old);
    assert!(prefab_references(&old).is_empty());
}

#[test]
fn copies_round_trip_through_a_world_file_without_their_prefabs() {
    let files = Files::from([(WHEEL.to_owned(), part_prefab("Wheel", color(0.5, 0.5, 0.5)))]);
    let files = {
        let mut files = files;
        let car = car_prefab(&files);
        files.insert(CAR.to_owned(), car);
        files
    };
    let mut world = World::new();
    let workspace = world.workspace();
    let car = prefabs(&files)
        .instantiate(CAR, &mut world, workspace)
        .unwrap()[0];
    let wheel = wheels_of(&world, car)[2];
    world
        .set_property(wheel, "Color", color(1.0, 0.0, 0.0))
        .unwrap();
    world.destroy(wheels_of(&world, car)[3]).unwrap();
    let spoiler = world.create(ClassName::Part).unwrap();
    world.set_name(spoiler, "Spoiler").unwrap();
    world.set_parent(spoiler, Some(car)).unwrap();

    let text = write_world(&world);
    assert!(text.contains(r#""overrides": ["Color"]"#), "{text}");
    assert!(text.contains(r#""removed": [["Wheel", 3]]"#), "{text}");
    assert!(text.contains(r#""added": true"#), "{text}");
    // Read with no prefab files at hand, the overrides are kept as read.
    let read = parse_world(&text).unwrap();
    assert_eq!(write_world(&read), text);
    let root = read.get(read.workspace()).unwrap().children()[0];
    assert_eq!(
        prefab::overrides(&read, root),
        prefab::overrides(&world, car)
    );
    // Synced, they're the same again.
    let mut synced = read;
    refresh(&files, &mut synced);
    assert_eq!(write_world(&synced), text);
}

#[test]
fn a_missing_or_self_containing_prefab_is_reported_and_its_copies_kept() {
    let files = Files::from([(
        CAR.to_owned(),
        r#"{ "format_version": 1, "instances": [
             { "class": "Model", "name": "Car", "prefab": "assets/prefabs/Car.prefab.json" } ] }"#
            .to_owned(),
    )]);
    let mut prefabs_ = prefabs(&files);
    let mut world = World::new();
    let workspace = world.workspace();
    prefabs_.instantiate(CAR, &mut world, workspace).unwrap();
    assert!(matches!(
        prefabs_.take_problems()[..],
        [PrefabError::Cycle { .. }]
    ));

    let text = r#"{ "format_version": 1, "workspace": { "children": [
        { "class": "Part", "name": "Wheel", "prefab": "assets/prefabs/Gone.prefab.json",
          "overrides": ["Color"], "properties": { "Color": [1, 0, 0] } } ] } }"#;
    let mut world = parse_world(text).unwrap();
    let mut prefabs_ = prefabs(&files);
    let root = world.root();
    prefabs_.refresh(&mut world, root);
    assert!(matches!(
        prefabs_.take_problems()[..],
        [PrefabError::Read { .. }]
    ));
    let wheel = world.get(world.workspace()).unwrap().children()[0];
    assert_eq!(property(&world, wheel, "Color"), color(1.0, 0.0, 0.0));
    let expected = Overrides {
        properties: ["Color".to_owned()].into(),
        ..Overrides::default()
    };
    assert_eq!(prefab::overrides(&world, wheel), expected);
    assert_eq!(prefab_references(text), ["assets/prefabs/Gone.prefab.json"]);
}
