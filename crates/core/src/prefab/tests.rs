use super::*;
use crate::class::ClassName;
use crate::enums::Material;
use crate::value::Color3;

fn red() -> Value {
    Value::Color3(Color3::new(1.0, 0.0, 0.0))
}

/// A source holding one Model named "Car" with two Parts named "Wheel".
fn car_source() -> Arc<PrefabSource> {
    let mut world = World::new();
    let workspace = world.workspace();
    let car = world.create(ClassName::Model).unwrap();
    world.set_name(car, "Car").unwrap();
    world.set_parent(car, Some(workspace)).unwrap();
    for _ in 0..2 {
        let wheel = world.create(ClassName::Part).unwrap();
        world.set_name(wheel, "Wheel").unwrap();
        world.set_parent(wheel, Some(car)).unwrap();
    }
    Arc::new(PrefabSource {
        world,
        roots: vec![car],
    })
}

/// A linked copy of `source` in `world`'s Workspace.
fn place_copy(world: &mut World, source: &Arc<PrefabSource>) -> InstanceId {
    let root = world.copy_from(&source.world, source.roots[0]).unwrap();
    world.set_parent(root, Some(world.workspace())).unwrap();
    world
        .set_prefab(
            root,
            Some(PrefabLink::new("assets/prefabs/Car.prefab.json", 0)),
        )
        .unwrap();
    sync(world, root, source, 0, &Overrides::default());
    root
}

fn wheels(world: &World, car: InstanceId) -> Vec<InstanceId> {
    world.get(car).unwrap().children().to_vec()
}

#[test]
fn a_fresh_copy_has_no_overrides() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    assert!(overrides(&world, car).is_empty());
}

#[test]
fn a_changed_property_is_an_override_on_the_second_wheel() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    let second = wheels(&world, car)[1];
    world.set_property(second, "Color", red()).unwrap();
    let found = overrides(&world, car);
    let wheel = found.at(&[Segment::new("Wheel", 1)]).unwrap();
    assert!(wheel.properties.contains("Color"));
    assert_eq!(found.count(), 1);
    let place = place(&world, second).unwrap();
    assert_eq!(place.root, car);
    assert!(place.here().properties.contains("Color"));
}

#[test]
fn syncing_takes_the_new_source_but_keeps_overrides() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    let [first, second] = wheels(&world, car)[..] else {
        panic!()
    };
    world.set_property(second, "Color", red()).unwrap();
    world
        .set_property(first, "Transparency", Value::Number(0.5))
        .unwrap();

    // The prefab changes every wheel's colour and gains a third wheel.
    let mut changed = (*source).clone();
    let blue = Value::Color3(Color3::new(0.0, 0.0, 1.0));
    for wheel in changed
        .world
        .get(changed.roots[0])
        .unwrap()
        .children()
        .to_vec()
    {
        changed
            .world
            .set_property(wheel, "Color", blue.clone())
            .unwrap();
    }
    let third = changed.world.create(ClassName::Part).unwrap();
    changed.world.set_name(third, "Wheel").unwrap();
    changed
        .world
        .set_parent(third, Some(changed.roots[0]))
        .unwrap();
    let changed = Arc::new(changed);

    let kept = overrides(&world, car);
    sync(&mut world, car, &changed, 0, &kept);
    let now = wheels(&world, car);
    assert_eq!(now.len(), 3);
    assert_eq!(now[0], first, "matched Instances keep their IDs");
    assert_eq!(world.get(first).unwrap().property("Color"), Some(&blue));
    assert_eq!(
        world.get(first).unwrap().property("Transparency"),
        Some(&Value::Number(0.5))
    );
    assert_eq!(world.get(second).unwrap().property("Color"), Some(&red()));
    assert_eq!(overrides(&world, car), kept);
}

#[test]
fn added_and_removed_children_survive_a_sync() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    world.destroy(wheels(&world, car)[1]).unwrap();
    let spoiler = world.create(ClassName::Part).unwrap();
    world.set_name(spoiler, "Spoiler").unwrap();
    world.set_parent(spoiler, Some(car)).unwrap();

    let found = overrides(&world, car);
    assert_eq!(found.removed, vec![Segment::new("Wheel", 1)]);
    assert_eq!(found.added, vec![Segment::new("Spoiler", 0)]);
    assert_eq!(added_children(&world, car), vec![spoiler]);
    assert!(
        place(&world, spoiler).is_none(),
        "added things aren't the copy's"
    );

    sync(&mut world, car, &source, 0, &found);
    let names: Vec<_> = wheels(&world, car)
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, ["Wheel", "Spoiler"]);
    // Reverting everything brings the wheel back and drops the spoiler.
    sync(&mut world, car, &source, 0, &Overrides::default());
    assert!(world.get(spoiler).is_none());
    assert_eq!(wheels(&world, car).len(), 2);
}

#[test]
fn an_overridden_metalness_survives_the_source_changing_material() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    let wheel = wheels(&world, car)[0];
    world
        .set_property(wheel, "Metalness", Value::Number(0.25))
        .unwrap();
    let kept = overrides(&world, car);

    let mut changed = (*source).clone();
    let theirs = changed.world.get(changed.roots[0]).unwrap().children()[0];
    changed
        .world
        .set_property(theirs, "Material", Value::Material(Material::Metal))
        .unwrap();
    sync(&mut world, car, &Arc::new(changed), 0, &kept);
    let wheel = world.get(wheel).unwrap();
    assert_eq!(
        wheel.property("Material"),
        Some(&Value::Material(Material::Metal))
    );
    assert_eq!(wheel.property("Metalness"), Some(&Value::Number(0.25)));
}

#[test]
fn revert_all_keeps_where_parts_were_placed() {
    let source = car_source();
    let mut world = World::new();
    let car = place_copy(&mut world, &source);
    let wheel = wheels(&world, car)[0];
    let moved = Value::Vector3(crate::value::Vector3::new(5.0, 1.0, 0.0));
    world
        .set_property(wheel, "Position", moved.clone())
        .unwrap();
    world.set_property(wheel, "Color", red()).unwrap();
    let kept = overrides(&world, car).placement_only();
    sync(&mut world, car, &source, 0, &kept);
    assert_eq!(world.get(wheel).unwrap().property("Position"), Some(&moved));
    assert_ne!(world.get(wheel).unwrap().property("Color"), Some(&red()));
}
