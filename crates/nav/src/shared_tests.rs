//! The shared mesh (M13): rebuilt meshes are put together on a worker and
//! come in at a fixed game time, the same as without the worker.

use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, Value};

use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn part(world: &mut World, position: Vector3, size: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(true));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn at(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn build(world: &World) -> NavMesh {
    NavMesh::build(world, &Assets::default(), NavSettings::default())
}

/// One frame: the clock moves on, then the mesh is brought up to date.
fn frame(nav: &SharedNavMesh, world: &World) -> MeshState {
    nav.advance(FRAME);
    nav.refresh(world, &Assets::default())
}

#[test]
fn a_rebuilt_mesh_comes_in_apply_seconds_after_the_look_with_or_without_the_worker() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let wall = part(&mut world, at(0.0, 3.0, 20.0), at(4.0, 6.0, 4.0));
    let threaded = SharedNavMesh::default();
    let inline = SharedNavMesh::default().without_worker();
    let first = build(&world);
    for nav in [&threaded, &inline] {
        let state = nav.refresh(&world, &Assets::default());
        assert_eq!(*state.mesh, first, "the first mesh is built at once");
        assert_eq!(state.generation, 0);
    }

    // Moved into the middle once the first look is due.
    let frames_to_look = (REBUILD_SECONDS / FRAME).round() as usize;
    for _ in 1..frames_to_look {
        for nav in [&threaded, &inline] {
            assert_eq!(frame(nav, &world).generation, 0);
            assert!(!nav.is_rebuilding());
        }
    }
    set(
        &mut world,
        wall,
        "Position",
        Value::Vector3(at(0.0, 3.0, 0.0)),
    );
    let moved = build(&world);
    assert_ne!(moved, first);
    for nav in [&threaded, &inline] {
        let state = frame(nav, &world);
        assert!(nav.is_rebuilding(), "the look found the change");
        assert_eq!(*state.mesh, first, "the old mesh while it's rebuilt");
    }

    // The old mesh until APPLY_SECONDS have passed, then the new one, on
    // the same frame whichever way it was put together.
    let frames_to_apply = (APPLY_SECONDS / FRAME).round() as usize;
    for step in 1..=frames_to_apply {
        let states = [&threaded, &inline].map(|nav| frame(nav, &world));
        let expected = if step < frames_to_apply {
            &first
        } else {
            &moved
        };
        for state in &states {
            assert_eq!(*state.mesh, *expected, "frame {step}");
        }
        assert_eq!(states[0].generation, states[1].generation);
    }
    for nav in [&threaded, &inline] {
        assert!(!nav.is_rebuilding());
        assert_eq!(nav.current().unwrap().generation, 1);
    }
}

#[test]
fn nothing_changed_rebuilds_nothing() {
    let mut world = World::new();
    let floor = part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let nav = SharedNavMesh::default();
    let first = nav.refresh(&world, &Assets::default()).mesh;
    // A color isn't something the mesh is made from.
    set(
        &mut world,
        floor,
        "Color",
        Value::Color3(axcenity_core::Color3::new(1.0, 0.0, 0.0)),
    );
    for _ in 0..120 {
        let state = frame(&nav, &world);
        assert!(!nav.is_rebuilding());
        assert!(Arc::ptr_eq(&state.mesh, &first));
    }
}

#[test]
fn clones_share_one_mesh_and_only_the_owner_moves_the_clock() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let wall = part(&mut world, at(0.0, 3.0, 20.0), at(4.0, 6.0, 4.0));
    let owner = SharedNavMesh::default();
    let other = owner.clone();
    let built = other.refresh(&world, &Assets::default()).mesh;
    assert!(Arc::ptr_eq(&owner.current().unwrap().mesh, &built));

    set(
        &mut world,
        wall,
        "Position",
        Value::Vector3(at(0.0, 3.0, 0.0)),
    );
    // Both read it every frame; only the owner moves the clock, so the
    // look comes REBUILD_SECONDS in, not half that.
    let frames_to_look = (REBUILD_SECONDS / FRAME).round() as usize;
    for _ in 1..frames_to_look {
        frame(&owner, &world);
        other.refresh(&world, &Assets::default());
        assert!(!owner.is_rebuilding());
    }
    frame(&owner, &world);
    assert!(other.is_rebuilding());
    for _ in 0..(APPLY_SECONDS / FRAME).round() as usize {
        frame(&owner, &world);
    }
    let state = other.refresh(&world, &Assets::default());
    assert_eq!(state.generation, 1);
    assert_eq!(*state.mesh, build(&world));
}

#[test]
fn agents_walk_on_the_mesh_they_share() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_parent(body, Some(model)).unwrap();
    set(&mut world, body, "Size", Value::Vector3(at(2.0, 5.0, 1.0)));
    set(
        &mut world,
        body,
        "Position",
        Value::Vector3(at(-20.0, 2.5, 0.0)),
    );
    let agent = world.create(ClassName::NavigationAgent).unwrap();
    world.set_parent(agent, Some(model)).unwrap();
    let mut agents = Agents::default();
    let shared = agents.nav_mesh();
    assert!(shared.current().is_none(), "nothing built before a step");
    agents.step(&mut world, &Assets::default(), FRAME);
    let mesh = shared.current().unwrap().mesh;
    assert!(std::ptr::eq(agents.mesh().unwrap(), &*mesh));
}
