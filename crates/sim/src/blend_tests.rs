//! Roadmap G3: which moving parts hosts draw between physics steps.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use crate::Sim;

fn part(world: &mut World, position: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    id
}

fn weld(world: &mut World, part0: InstanceId, part1: InstanceId) {
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(part0)).unwrap();
    world
        .set_property(weld, "Part0", Value::Instance(Some(part0)))
        .unwrap();
    world
        .set_property(weld, "Part1", Value::Instance(Some(part1)))
        .unwrap();
}

#[test]
fn the_car_the_player_sits_in_stays_stepped_and_the_rest_blends() {
    let mut world = World::new();
    // The player's character welded onto a falling cart, as a seat welds
    // it, and a crate falling beside them.
    let cart = part(&mut world, Vector3::new(0.0, 50.0, 0.0));
    let character =
        axcenity_character::rig::character(&mut world, Vector3::new(0.0, 50.5, 0.0), "");
    world
        .set_parent(character, Some(world.workspace()))
        .unwrap();
    let torso = world.find_first_child(character, "Torso").unwrap();
    weld(&mut world, cart, torso);
    let crate_ = part(&mut world, Vector3::new(20.0, 50.0, 0.0));

    let mut sim = Sim::new(world, crate::script_runtime(), Assets::default());
    // The controller steps first each frame, which is how the frame knows
    // whose character it is.
    let assets = sim.assets.clone();
    sim.characters
        .step(&mut sim.world, &assets, Default::default(), 0.0);
    for _ in 0..10 {
        sim.physics.step(&mut sim.world, 1.0 / 60.0);
    }
    let everything: Vec<InstanceId> = sim.physics.blends(&[]).iter().map(|b| b.part).collect();
    assert!(
        everything.contains(&cart) && everything.contains(&torso),
        "{everything:?}"
    );
    let drawn: Vec<InstanceId> = sim.blends().iter().map(|b| b.part).collect();
    assert_eq!(drawn, vec![crate_]);
}
