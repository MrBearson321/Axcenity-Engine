//! Paths across the mesh, and agents walking them.

use axcenity_core::{ClassName, InstanceId};

use super::*;

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

/// A 60×60 floor with its top at Y = 0, and a wall 2 thick and 8 tall
/// across X = 0 from Z = -30 to Z = 10, so going from -X to +X means going
/// round its end at +Z.
fn floor_and_wall() -> World {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    part(&mut world, at(0.0, 4.0, -10.0), at(2.0, 8.0, 40.0));
    world
}

/// A Model with a 2×5×1 unanchored Body standing with its feet at `feet`,
/// and a NavigationAgent in it. Returns the agent and the body.
fn agent_at(world: &mut World, feet: Vector3) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_parent(body, Some(model)).unwrap();
    set(world, body, "Size", Value::Vector3(at(2.0, 5.0, 1.0)));
    set(
        world,
        body,
        "Position",
        Value::Vector3(at(feet.x, feet.y + 2.5, feet.z)),
    );
    let agent = world.create(ClassName::NavigationAgent).unwrap();
    world.set_parent(agent, Some(model)).unwrap();
    set(world, agent, "Destination", Value::Vector3(feet));
    (agent, body)
}

fn feet(world: &World, body: InstanceId) -> Vector3 {
    feet_of(world, &[body]).unwrap()
}

fn status(world: &World, agent: InstanceId) -> String {
    world
        .get(agent)
        .unwrap()
        .property("Status")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// Steps the agents for `seconds` at 60 frames a second.
fn run(agents: &mut Agents, world: &mut World, seconds: f32) {
    for _ in 0..(seconds * 60.0).round() as usize {
        agents.step(world, &Assets::default(), 1.0 / 60.0);
    }
}

fn horizontal(a: Vector3, b: Vector3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// True when every point a quarter of a stud apart along the path is on
/// walkable ground no more than a step from the path's height there.
fn walkable_all_along(mesh: &NavMesh, path: &[Vector3]) -> bool {
    path.windows(2).all(|pair| {
        let (a, b) = (pair[0], pair[1]);
        let samples = (horizontal(a, b) / 0.25).ceil().max(1.0) as usize;
        (0..=samples).all(|i| {
            let t = i as f32 / samples as f32;
            let p = at(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
            );
            mesh.ground_at(p).is_some_and(|y| (y - p.y).abs() <= 1.0)
        })
    })
}

#[test]
fn a_path_goes_round_a_wall_and_every_stretch_of_it_is_walkable() {
    let world = floor_and_wall();
    let mesh = build(&world);
    let (from, to) = (at(-10.0, 0.0, -10.0), at(10.0, 0.0, -10.0));
    let path = mesh.find_path(from, to).unwrap();

    assert_eq!(path.first(), Some(&from));
    assert_eq!(path.last(), Some(&to));
    assert!(walkable_all_along(&mesh, &path), "{path:?}");
    // It goes round the wall's end, past Z = 10, and no further than it needs.
    assert!(path.iter().any(|p| p.z > 10.0), "{path:?}");
    let length: f32 = path.windows(2).map(|w| horizontal(w[0], w[1])).sum();
    assert!(length < 70.0, "a roundabout path, {length} studs: {path:?}");
    // Straightened: a few corners, not one per cell.
    assert!(path.len() <= 8, "{path:?}");
}

#[test]
fn across_open_ground_a_path_is_one_straight_line() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let mesh = build(&world);
    let (from, to) = (at(-20.3, 0.0, 7.1), at(18.6, 0.0, -12.4));
    assert_eq!(mesh.find_path(from, to).unwrap(), vec![from, to]);
    // From a cell to itself.
    let near = at(-20.1, 0.0, 7.2);
    assert_eq!(mesh.find_path(from, near).unwrap(), vec![from, near]);
}

#[test]
fn a_path_climbs_a_step_and_follows_the_ground_up_it() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    // A platform 1 stud high, as tall as a step, from X = 0 to 20.
    part(&mut world, at(10.0, 0.5, 0.0), at(20.0, 1.0, 20.0));
    let mesh = build(&world);
    let path = mesh
        .find_path(at(-15.0, 0.0, 0.0), at(15.0, 1.0, 0.0))
        .unwrap();
    assert!((path.last().unwrap().y - 1.0).abs() < 1e-3, "{path:?}");
    assert!(walkable_all_along(&mesh, &path), "{path:?}");
}

#[test]
fn a_destination_off_the_mesh_or_across_a_gap_has_no_path() {
    let mut world = World::new();
    part(&mut world, at(-12.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    part(&mut world, at(12.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    let mesh = build(&world);
    let start = at(-12.0, 0.0, 0.0);
    assert_eq!(
        mesh.find_path(start, at(0.0, 0.0, 0.0)),
        Err(PathError::DestinationOffMesh),
        "the gap"
    );
    assert_eq!(
        mesh.find_path(start, at(-12.0, 30.0, 0.0)),
        Err(PathError::DestinationOffMesh),
        "high in the air"
    );
    assert_eq!(
        mesh.find_path(start, at(12.0, 0.0, 0.0)),
        Err(PathError::NotConnected)
    );
    assert_eq!(
        mesh.find_path(at(100.0, 0.0, 0.0), start),
        Err(PathError::StartOffMesh)
    );
}

#[test]
fn an_agent_walks_its_path_to_the_destination_at_walk_speed_and_stops() {
    let mut world = floor_and_wall();
    let (agent, body) = agent_at(&mut world, at(-10.0, 0.0, -10.0));
    let destination = at(10.0, 0.0, -10.0);
    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(destination),
    );
    let mut agents = Agents::default();

    agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
    assert_eq!(status(&world, agent), "Moving");
    let path = agents.path(agent).expect("walking a path");
    let length: f32 = path.windows(2).map(|w| horizontal(w[0], w[1])).sum();
    // The agent's own body isn't ground or an obstacle.
    assert!(agents.mesh().unwrap().is_walkable(feet(&world, body)));

    // Halfway there in time, it's still walking and nowhere near the wall.
    let seconds = length / 16.0;
    run(&mut agents, &mut world, seconds * 0.5);
    let halfway = feet(&world, body);
    assert_eq!(status(&world, agent), "Moving");
    assert!(halfway.x.abs() > 1.5 || halfway.z > 10.0, "{halfway:?}");

    run(&mut agents, &mut world, seconds * 0.5 + 0.2);
    let there = feet(&world, body);
    assert!(horizontal(there, destination) < 1e-3, "{there:?}");
    assert!(there.y.abs() < 1e-3, "its feet stay on the floor");
    assert_eq!(status(&world, agent), "Reached");
    assert_eq!(agents.path(agent), None);
    run(&mut agents, &mut world, 0.5);
    assert_eq!(feet(&world, body), there, "it stays");
    assert!(agents.take_warnings().is_empty());
}

#[test]
fn a_new_destination_starts_a_new_path() {
    let mut world = floor_and_wall();
    let (agent, body) = agent_at(&mut world, at(-10.0, 0.0, -20.0));
    let mut agents = Agents::default();
    run(&mut agents, &mut world, 0.1);
    assert_eq!(status(&world, agent), "Reached", "already there");

    let next = at(-20.0, 0.0, -20.0);
    set(&mut world, agent, "Destination", Value::Vector3(next));
    run(&mut agents, &mut world, 1.0);
    assert!(horizontal(feet(&world, body), next) < 1e-3);
    assert_eq!(status(&world, agent), "Reached");
}

#[test]
fn an_unreachable_destination_leaves_the_agent_where_it_is_with_one_warning() {
    let mut world = floor_and_wall();
    let (agent, body) = agent_at(&mut world, at(-10.0, 0.0, -10.0));
    let start = feet(&world, body);
    let mut agents = Agents::default();
    run(&mut agents, &mut world, 0.1);

    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(at(100.0, 0.0, 0.0)),
    );
    run(&mut agents, &mut world, 1.0);
    assert_eq!(feet(&world, body), start);
    assert_eq!(status(&world, agent), "Unreachable");
    assert_eq!(agents.path(agent), None);
    let warnings = agents.take_warnings();
    assert_eq!(
        warnings,
        vec![
            "Workspace.Model.NavigationAgent can't reach (100, 0, 0): that point isn't on \
             walkable ground."
                .to_owned()
        ]
    );
    // A reachable one afterwards walks.
    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(at(-10.0, 0.0, -5.0)),
    );
    run(&mut agents, &mut world, 1.0);
    assert_eq!(status(&world, agent), "Reached");
}

#[test]
fn velocity_moves_an_agent_without_a_path_and_it_stays_where_it_stops() {
    let mut world = floor_and_wall();
    let (agent, body) = agent_at(&mut world, at(-10.0, 0.0, -10.0));
    let mut agents = Agents::default();
    run(&mut agents, &mut world, 0.1);

    set(
        &mut world,
        agent,
        "Velocity",
        Value::Vector3(at(-5.0, 0.0, 0.0)),
    );
    run(&mut agents, &mut world, 1.0);
    let moved = feet(&world, body);
    assert!((moved.x + 15.0).abs() < 1e-3, "{moved:?}");
    assert_eq!(moved.z, -10.0);
    assert_eq!(status(&world, agent), "Moving");
    assert_eq!(agents.path(agent), None, "no path");
    let destination = world.get(agent).unwrap().property("Destination").cloned();
    assert_eq!(destination, Some(Value::Vector3(moved)), "follows it");

    set(
        &mut world,
        agent,
        "Velocity",
        Value::Vector3(at(0.0, 0.0, 0.0)),
    );
    run(&mut agents, &mut world, 1.0);
    assert_eq!(feet(&world, body), moved, "stops where it is");
    assert_eq!(status(&world, agent), "Idle");
}

#[test]
fn velocity_stops_at_the_edge_of_the_mesh_and_at_a_wall() {
    let mut world = floor_and_wall();
    let (agent, body) = agent_at(&mut world, at(-25.0, 0.0, -10.0));
    let mut agents = Agents::default();
    set(
        &mut world,
        agent,
        "Velocity",
        Value::Vector3(at(-10.0, 0.0, 0.0)),
    );
    run(&mut agents, &mut world, 2.0);
    let edge = feet(&world, body);
    assert!(edge.x > -30.0 && edge.x < -28.0, "{edge:?}");
    assert_eq!(edge.y, 0.0, "it doesn't fall off");

    set(
        &mut world,
        agent,
        "Velocity",
        Value::Vector3(at(10.0, 0.0, 0.0)),
    );
    run(&mut agents, &mut world, 4.0);
    let wall = feet(&world, body);
    assert!(wall.x < -1.0 && wall.x > -3.0, "{wall:?}");
}

#[test]
fn agents_are_held_kinematic_and_left_out_of_the_mesh() {
    let mut world = floor_and_wall();
    let (_, body) = agent_at(&mut world, at(-10.0, 0.0, -10.0));
    assert_eq!(agent_parts(&world), vec![body]);
    let before = fingerprint(&world);
    set(
        &mut world,
        body,
        "Position",
        Value::Vector3(at(5.0, 2.5, 5.0)),
    );
    assert_eq!(fingerprint(&world), before, "a moving agent isn't a change");
    assert!(build(&world).is_walkable(at(5.0, 0.0, 5.0)));

    Agents::default().keep_kinematic(&mut world);
    let anchored = world.get(body).unwrap().property("Anchored").cloned();
    assert_eq!(anchored, Some(Value::Bool(true)));
}
