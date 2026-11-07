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

/// How far `p` is, across, from the line through `points`.
fn off_path(points: &[Vector3], p: Vector3) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let (dx, dz) = (b.x - a.x, b.z - a.z);
            let length = dx * dx + dz * dz;
            let t = if length > 0.0 {
                (((p.x - a.x) * dx + (p.z - a.z) * dz) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            horizontal(p, at(a.x + dx * t, 0.0, a.z + dz * t))
        })
        .fold(f32::INFINITY, f32::min)
}

/// An agent at (-20, 0, 0) on its way to (20, 0, 0) across `world`, after
/// 0.6 s of walking, so the mesh is old enough to rebuild at once.
fn walking_east(world: &mut World) -> (InstanceId, InstanceId, Agents) {
    let (agent, body) = agent_at(world, at(-20.0, 0.0, 0.0));
    set(
        world,
        agent,
        "Destination",
        Value::Vector3(at(20.0, 0.0, 0.0)),
    );
    let mut agents = Agents::default();
    run(&mut agents, world, 0.6);
    assert_eq!(status(world, agent), "Moving");
    assert!(feet(world, body).x < -8.0);
    (agent, body, agents)
}

#[test]
fn an_agent_steers_round_a_part_pushed_into_its_path_and_rejoins_it() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    // A crate beside the path to start with.
    let crate_ = part(&mut world, at(0.0, 3.0, 14.0), at(4.0, 6.0, 4.0));
    let (agent, body, mut agents) = walking_east(&mut world);
    let path = agents.path(agent).unwrap();
    assert_eq!(path.len(), 2, "straight across: {path:?}");

    // Pushed into the path, 10 studs ahead of the agent.
    set(
        &mut world,
        crate_,
        "Position",
        Value::Vector3(at(0.0, 3.0, 0.0)),
    );
    agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
    assert_eq!(status(&world, agent), "Moving");
    let detour = agents.path(agent).unwrap();
    assert!(detour.iter().any(|p| p.z.abs() >= 2.0), "{detour:?}");
    assert_eq!(detour.last(), Some(&at(20.0, 0.0, 0.0)));
    assert!(
        walkable_all_along(agents.mesh().unwrap(), &detour),
        "{detour:?}"
    );

    let mut widest: f32 = 0.0;
    for _ in 0..(4.0 * 60.0) as usize {
        agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
        let p = feet(&world, body);
        assert!(
            p.x.abs() >= 2.0 || p.z.abs() >= 2.0,
            "walked into it: {p:?}"
        );
        widest = widest.max(p.z.abs());
        // Past the crate, it's back on its path.
        if p.x > 5.0 {
            assert!(p.z.abs() < 1e-3, "{p:?}");
        }
    }
    assert!(widest > 2.0, "it went round");
    assert!(horizontal(feet(&world, body), at(20.0, 0.0, 0.0)) < 1e-3);
    assert_eq!(status(&world, agent), "Reached");
    assert!(agents.take_warnings().is_empty());
}

#[test]
fn an_agent_stops_and_reports_a_blockage_it_cant_go_round_until_it_clears() {
    let mut world = World::new();
    // A walkway 6 studs wide, with drops either side.
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 6.0));
    let (agent, body, mut agents) = walking_east(&mut world);

    // A crate across the whole walkway.
    let crate_ = part(&mut world, at(0.0, 3.0, 0.0), at(4.0, 6.0, 10.0));
    run(&mut agents, &mut world, 0.1);
    let stopped = feet(&world, body);
    assert_eq!(status(&world, agent), "Blocked");
    run(&mut agents, &mut world, 2.0);
    assert_eq!(feet(&world, body), stopped, "it stays stopped");
    assert!(stopped.x < -2.0, "{stopped:?}");
    assert_eq!(status(&world, agent), "Blocked");
    assert_eq!(
        agents.take_warnings(),
        vec![
            "Workspace.Model.NavigationAgent is blocked on its way to (20, 0, 0): nothing \
             walkable goes round what's in the way."
                .to_owned()
        ]
    );
    // The path it was following still shows.
    let path = agents.path(agent).unwrap();
    assert_eq!(path.last(), Some(&at(20.0, 0.0, 0.0)));

    // Cleared, it carries on.
    set(
        &mut world,
        crate_,
        "Position",
        Value::Vector3(at(0.0, 3.0, 40.0)),
    );
    run(&mut agents, &mut world, 0.1);
    assert_eq!(status(&world, agent), "Moving");
    run(&mut agents, &mut world, 3.0);
    assert_eq!(status(&world, agent), "Reached");
    assert!(horizontal(feet(&world, body), at(20.0, 0.0, 0.0)) < 1e-3);
    assert!(agents.take_warnings().is_empty());
}

#[test]
fn a_way_round_far_out_of_the_way_is_a_blockage_not_a_detour() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    let (agent, body, mut agents) = walking_east(&mut world);
    // A wall across the floor, with a gap only at its far +Z end.
    part(&mut world, at(0.0, 3.0, -3.0), at(4.0, 6.0, 54.0));
    run(&mut agents, &mut world, 0.5);
    assert_eq!(status(&world, agent), "Blocked");
    assert!(feet(&world, body).x < -2.0);
    // A new destination finds the long way round.
    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(at(20.0, 0.0, 1.0)),
    );
    run(&mut agents, &mut world, 0.1);
    assert_eq!(status(&world, agent), "Moving");
    assert!(agents.path(agent).unwrap().iter().any(|p| p.z > 24.0));
}

#[test]
fn with_nothing_in_the_way_an_agent_keeps_to_the_path_it_found() {
    let mut world = floor_and_wall();
    // A part far from the path that keeps moving, so the mesh keeps
    // being rebuilt.
    let fidget = part(&mut world, at(25.0, 0.5, 25.0), at(2.0, 1.0, 2.0));
    let (start, destination) = (at(-10.0, 0.0, -10.0), at(10.0, 0.0, -10.0));
    let (agent, body) = agent_at(&mut world, start);
    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(destination),
    );
    // The path Phase 13 found.
    let expected = vec![
        start,
        at(-2.5, 0.0, 10.5),
        at(-0.5, 0.0, 11.5),
        at(1.5, 0.0, 11.5),
        at(2.5, 0.0, 9.5),
        at(9.5, 0.0, -9.5),
        destination,
    ];
    assert_eq!(
        build(&world).find_path(start, destination),
        Ok(expected.clone())
    );

    let mut agents = Agents::default();
    for frame in 0..(5.0 * 60.0) as usize {
        let x = 25.0 + (frame % 2) as f32 * 0.5;
        set(
            &mut world,
            fidget,
            "Position",
            Value::Vector3(at(x, 0.5, 25.0)),
        );
        agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
        let p = feet(&world, body);
        assert!(off_path(&expected, p) < 1e-3, "frame {frame}: {p:?}");
        if let Some(path) = agents.path(agent) {
            let ahead = &path[1..];
            assert_eq!(ahead, &expected[expected.len() - ahead.len()..]);
        }
    }
    assert_eq!(status(&world, agent), "Reached");
    assert!(agents.take_warnings().is_empty());
}

/// Phase 38: a player character (a Model with a ThirdPersonController) at
/// the origin walking toward -Z, an NPC standing in its way at z = -8 and
/// a wall behind the NPC whose near face is at z = -15.5.
fn player_npc_and_wall(collide: bool) -> (World, InstanceId, InstanceId, InstanceId) {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(60.0, 1.0, 60.0));
    part(&mut world, at(0.0, 4.0, -16.0), at(20.0, 8.0, 1.0));
    let groups = world.collision_groups_mut();
    groups.register("Players").unwrap();
    groups.register("NPCs").unwrap();
    groups.set_collidable("Players", "NPCs", collide).unwrap();

    let player = world.create(ClassName::Model).unwrap();
    world.set_parent(player, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_parent(body, Some(player)).unwrap();
    set(&mut world, body, "Size", Value::Vector3(at(2.0, 5.0, 1.0)));
    set(
        &mut world,
        body,
        "Position",
        Value::Vector3(at(0.0, 2.5, 0.0)),
    );
    let players = Value::String("Players".to_owned());
    set(&mut world, body, "CollisionGroup", players);
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(player)).unwrap();

    let (npc, npc_body) = agent_at(&mut world, at(0.0, 0.0, -8.0));
    let npcs = Value::String("NPCs".to_owned());
    set(&mut world, npc_body, "CollisionGroup", npcs);
    (world, body, npc, npc_body)
}

/// Runs the player (holding `forward`), the agents and physics together,
/// as a host's frame does.
fn play(world: &mut World, forward: bool, seconds: f32) {
    let assets = Assets::default();
    let input = axcenity_character::MoveInput {
        forward,
        ..Default::default()
    };
    let mut characters = axcenity_character::Controllers::default();
    let mut agents = Agents::default();
    let mut physics = axcenity_physics::Physics::new();
    for _ in 0..(seconds * 60.0).round() as usize {
        characters.step(world, &assets, input, 1.0 / 60.0);
        agents.step(world, &assets, 1.0 / 60.0);
        characters.keep_kinematic(world);
        agents.keep_kinematic(world);
        physics.step(world, 1.0 / 60.0);
    }
}

#[test]
fn players_and_npcs_set_as_non_colliding_walk_through_each_other_and_still_collide_with_walls() {
    // Collidable, the NPC stops the player.
    let (mut world, body, _, _) = player_npc_and_wall(true);
    play(&mut world, true, 2.0);
    let blocked = feet(&world, body);
    assert!(blocked.z > -7.5 && blocked.z < -5.0, "{blocked:?}");

    // Set not to collide, the player walks through the NPC to the wall,
    // which stops it (its body is 1 deep, so its middle is 0.5 from it).
    let (mut world, body, _, _) = player_npc_and_wall(false);
    play(&mut world, true, 2.0);
    let through = feet(&world, body);
    assert!(through.z < -14.0 && through.z > -15.1, "{through:?}");

    // And the NPC walks through the player standing at the origin, to z = 10.
    let (mut world, body, npc, npc_body) = player_npc_and_wall(false);
    set(
        &mut world,
        npc,
        "Destination",
        Value::Vector3(at(0.0, 0.0, 10.0)),
    );
    play(&mut world, false, 3.0);
    assert_eq!(status(&world, npc), "Reached");
    assert!(horizontal(feet(&world, npc_body), at(0.0, 0.0, 10.0)) < 0.1);
    assert!(
        horizontal(feet(&world, body), at(0.0, 0.0, 0.0)) < 0.01,
        "the player stays"
    );
}

/// A Spline in the Workspace through `points`, at floor height.
fn spline(world: &mut World, points: &[(f32, f32)], closed: bool) -> InstanceId {
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(world.workspace())).unwrap();
    set(world, spline, "Closed", Value::Bool(closed));
    for &(x, z) in points {
        let point = world.create(ClassName::SplinePoint).unwrap();
        world.set_parent(point, Some(spline)).unwrap();
        set(world, point, "Position", Value::Vector3(at(x, 0.0, z)));
    }
    spline
}

#[test]
fn an_agent_patrols_a_closed_spline_round_and_round_with_no_script() {
    let mut world = floor_and_wall();
    let corners = [(10.0, 15.0), (20.0, 15.0), (20.0, 25.0), (10.0, 25.0)];
    let route = spline(&mut world, &corners, true);
    let (agent, body) = agent_at(&mut world, at(9.0, 0.0, 14.0));
    set(
        &mut world,
        agent,
        "PatrolSpline",
        Value::Instance(Some(route)),
    );
    let mut agents = Agents::default();
    // Each side is 10 studs at 16 a second: it visits every corner in
    // order, nearest first, and comes round to the first again.
    let mut visited = Vec::new();
    for _ in 0..(6.0 * 60.0) as usize {
        agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
        let here = feet(&world, body);
        if let Some(i) = corners
            .iter()
            .position(|&(x, z)| horizontal(here, at(x, 0.0, z)) < 0.3)
            && visited.last() != Some(&i)
        {
            visited.push(i);
        }
    }
    assert!(visited.len() >= 5, "{visited:?}");
    for pair in visited.windows(2) {
        assert_eq!(pair[1], (pair[0] + 1) % 4, "{visited:?}");
    }
    assert_eq!(visited[0], 0);
}

#[test]
fn an_agent_walks_an_open_spline_back_and_forth() {
    let mut world = floor_and_wall();
    let points = [(10.0, 15.0), (16.0, 15.0), (22.0, 15.0)];
    let route = spline(&mut world, &points, false);
    let (agent, body) = agent_at(&mut world, at(10.0, 0.0, 15.0));
    set(
        &mut world,
        agent,
        "PatrolSpline",
        Value::Instance(Some(route)),
    );
    let mut agents = Agents::default();
    let mut visited = Vec::new();
    for _ in 0..(4.0 * 60.0) as usize {
        agents.step(&mut world, &Assets::default(), 1.0 / 60.0);
        let here = feet(&world, body);
        if let Some(i) = points
            .iter()
            .position(|&(x, z)| horizontal(here, at(x, 0.0, z)) < 0.3)
            && visited.last() != Some(&i)
        {
            visited.push(i);
        }
    }
    assert!(visited.starts_with(&[0, 1, 2, 1, 0]), "{visited:?}");
}
