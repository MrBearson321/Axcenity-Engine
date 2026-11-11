//! Humanoids' `MoveTo` plans on the game's one navigation mesh (M13).

use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_character::Controllers;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_nav::{Agents, SharedNavMesh};

use crate::humanoid::{Humanoids, Request};

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn child(world: &mut World, class: ClassName, parent: InstanceId) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A floor and a character with a Humanoid standing on it at (-10, 0, 0).
fn floor_and_character() -> (World, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let floor = child(&mut world, ClassName::Part, workspace);
    set(
        &mut world,
        floor,
        "Size",
        Value::Vector3(Vector3::new(40.0, 1.0, 40.0)),
    );
    set(
        &mut world,
        floor,
        "Position",
        Value::Vector3(Vector3::new(0.0, -0.5, 0.0)),
    );
    set(&mut world, floor, "Anchored", Value::Bool(true));
    let model = child(&mut world, ClassName::Model, workspace);
    let body = child(&mut world, ClassName::Part, model);
    set(
        &mut world,
        body,
        "Size",
        Value::Vector3(Vector3::new(2.0, 5.0, 1.0)),
    );
    set(
        &mut world,
        body,
        "Position",
        Value::Vector3(Vector3::new(-10.0, 2.5, 0.0)),
    );
    let humanoid = child(&mut world, ClassName::Humanoid, model);
    (world, humanoid)
}

fn walk_east(humanoids: &mut Humanoids, world: &mut World, humanoid: InstanceId) {
    humanoids.requests.0.borrow_mut().push(Request::MoveTo {
        humanoid,
        target: Vector3::new(10.0, 0.0, 0.0),
        part: None,
    });
    humanoids.step(
        world,
        &Assets::default(),
        &Controllers::default(),
        1.0 / 60.0,
    );
}

#[test]
fn move_to_plans_on_the_mesh_the_agents_walk_on() {
    let (mut world, humanoid) = floor_and_character();
    let agents = Agents::default();
    let shared = agents.nav_mesh();
    let mut humanoids = Humanoids::default();
    humanoids.share_nav_mesh(agents.nav_mesh());
    walk_east(&mut humanoids, &mut world, humanoid);
    // No agents stepped, so the Humanoid built the shared mesh.
    let mesh = shared.current().expect("built for the MoveTo").mesh;
    assert!(mesh.cell_at(Vector3::new(0.0, 0.0, 0.0)).is_some());

    // Planning again uses the same mesh; nothing is built twice.
    walk_east(&mut humanoids, &mut world, humanoid);
    assert!(Arc::ptr_eq(&shared.current().unwrap().mesh, &mesh));
}

#[test]
fn without_a_shared_mesh_move_to_builds_its_own() {
    let (mut world, humanoid) = floor_and_character();
    let unrelated = SharedNavMesh::default();
    let mut humanoids = Humanoids::default();
    walk_east(&mut humanoids, &mut world, humanoid);
    assert!(unrelated.current().is_none());
    // It found a path on a mesh of its own and set off east.
    let model = world.get(humanoid).unwrap().parent().unwrap();
    let controller = world
        .find_first_child(model, "ThirdPersonController")
        .unwrap();
    let direction = world.get(controller).unwrap().property("MoveDirection");
    match direction {
        Some(Value::Vector3(direction)) => assert!(direction.x > 0.9, "{direction:?}"),
        other => panic!("{other:?}"),
    }
}
