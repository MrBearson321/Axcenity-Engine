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

const HUD: &str = "assets/prefabs/Hud.prefab.json";
const MENU_SCRIPT: &str = "objects/Hud/Menu.client.lua";

/// A Hud prefab (G5): a ScreenGui holding a Frame with a TextLabel, and a
/// LocalScript inside an object.
fn hud_prefab(background: Value) -> String {
    let mut world = World::new();
    let workspace = world.workspace();
    let make = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let gui = make(&mut world, ClassName::ScreenGui, "Hud", workspace);
    let panel = make(&mut world, ClassName::Frame, "Panel", gui);
    let size = Value::UDim2(axcenity_core::UDim2::new(0.0, 300.0, 0.0, 80.0));
    world.set_property(panel, "Size", size).unwrap();
    world
        .set_property(panel, "BackgroundColor3", background)
        .unwrap();
    let label = make(&mut world, ClassName::TextLabel, "Score", panel);
    world
        .set_property(label, "Text", Value::String("Score: 0".to_owned()))
        .unwrap();
    let script = make(&mut world, ClassName::LocalScript, "Menu", gui);
    world
        .set_property(script, "SourceFile", Value::String(MENU_SCRIPT.to_owned()))
        .unwrap();
    write_prefab(&world, &[gui])
}

#[test]
fn a_ui_prefab_keeps_its_elements_script_and_each_copys_own_layout() {
    let dark = color(0.1, 0.1, 0.1);
    let mut files = Files::from([(HUD.to_owned(), hud_prefab(dark.clone()))]);
    let text = &files[HUD];
    assert!(text.contains("\"LocalScript\"") && text.contains(MENU_SCRIPT));

    // Two copies; the second's panel moved and its label's text changed.
    let mut world = World::new();
    let workspace = world.workspace();
    let huds: Vec<InstanceId> = (0..2)
        .map(|_| {
            prefabs(&files)
                .instantiate(HUD, &mut world, workspace)
                .unwrap()[0]
        })
        .collect();
    for &hud in &huds {
        assert_eq!(world.get(hud).unwrap().class(), ClassName::ScreenGui);
        let script = world.find_first_child(hud, "Menu").unwrap();
        assert_eq!(world.get(script).unwrap().source_file(), Some(MENU_SCRIPT));
    }
    let panel = world.find_first_child(huds[1], "Panel").unwrap();
    let corner = Value::UDim2(axcenity_core::UDim2::new(1.0, -300.0, 0.0, 0.0));
    world
        .set_property(panel, "Position", corner.clone())
        .unwrap();
    let label = world.find_first_child(panel, "Score").unwrap();
    let mine = Value::String("Points: 0".to_owned());
    world.set_property(label, "Text", mine.clone()).unwrap();
    let overrides = prefab::overrides(&world, huds[1]);
    assert!(prefab::overrides(&world, huds[0]).is_empty());
    assert_ne!(overrides, Overrides::default());
    let saved = write_world(&world);
    assert!(saved.contains("\"overrides\": [\"Position\"]"), "{saved}");

    // The prefab's panel colour changes; both copies follow and the
    // second keeps its own place and text, here and when opened again.
    let blue = color(0.1, 0.2, 0.8);
    files.insert(HUD.to_owned(), hud_prefab(blue.clone()));
    refresh(&files, &mut world);
    let mut opened = parse_world(&saved).unwrap();
    refresh(&files, &mut opened);
    for world in [&world, &opened] {
        let huds = world.get(world.workspace()).unwrap().children().to_vec();
        let panels: Vec<InstanceId> = huds
            .iter()
            .map(|&hud| world.find_first_child(hud, "Panel").unwrap())
            .collect();
        for &panel in &panels {
            assert_eq!(property(world, panel, "BackgroundColor3"), blue);
        }
        assert_eq!(property(world, panels[1], "Position"), corner);
        let label = world.find_first_child(panels[1], "Score").unwrap();
        assert_eq!(property(world, label, "Text"), mine);
        let first = world.find_first_child(panels[0], "Score").unwrap();
        assert_eq!(
            property(world, first, "Text"),
            Value::String("Score: 0".to_owned())
        );
        let script = world.find_first_child(huds[1], "Menu").unwrap();
        assert_eq!(world.get(script).unwrap().source_file(), Some(MENU_SCRIPT));
    }
}
