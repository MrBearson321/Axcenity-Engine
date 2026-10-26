//! Behavior trees: checking a tree file, and agents running them.

use std::path::PathBuf;

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

/// A Model with a 2×5×1 body standing with its feet at `feet`, and `class`
/// in it. Returns that and the body.
fn body_with(world: &mut World, class: ClassName, feet: Vector3) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_parent(body, Some(model)).unwrap();
    set(world, body, "Size", Value::Vector3(at(2.0, 5.0, 1.0)));
    put(world, body, feet);
    let holder = world.create(class).unwrap();
    world.set_parent(holder, Some(model)).unwrap();
    (holder, body)
}

fn put(world: &mut World, body: InstanceId, feet: Vector3) {
    let position = at(feet.x, feet.y + 2.5, feet.z);
    set(world, body, "Position", Value::Vector3(position));
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn feet(world: &World, body: InstanceId) -> Vector3 {
    let position = vector(world, body, "Position");
    at(position.x, position.y - 2.5, position.z)
}

fn across(a: Vector3, b: Vector3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// A tree file: nodes as (id, kind, values) and wires as (parent, pin,
/// child).
/// A node for [`tree`]: its id, kind and values.
type Node<'a> = (u32, &'a str, &'a [(&'a str, &'a str)]);

fn tree(nodes: &[Node], wires: &[(u32, &str, u32)]) -> String {
    let nodes: Vec<_> = nodes
        .iter()
        .map(|(id, kind, values)| {
            let values: serde_json::Map<_, _> = values
                .iter()
                .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
                .collect();
            serde_json::json!({ "id": id, "kind": kind, "position": [0.0, 0.0], "values": values })
        })
        .collect();
    let wires: Vec<_> = wires
        .iter()
        .map(|(from, pin, to)| {
            serde_json::json!({ "from": { "node": from, "pin": pin }, "to": { "node": to, "pin": "" } })
        })
        .collect();
    serde_json::json!({ "format_version": 1, "nodes": nodes, "wires": wires }).to_string()
}

/// A project folder of its own, with `files` written into it.
fn project(name: &str, files: &[(&str, &str)]) -> (PathBuf, Assets) {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-nav-behavior-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    for (path, text) in files {
        let file = folder.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let assets = Assets::new(&folder);
    (folder, assets)
}

/// Steps the agents for `seconds`, a sixtieth of a second at a time.
fn run(agents: &mut Agents, world: &mut World, assets: &Assets, seconds: f32) {
    for _ in 0..(seconds * 60.0).round() as usize {
        agents.step(world, assets, 1.0 / 60.0);
    }
}

fn messages(errors: &[TreeError]) -> Vec<(Option<u32>, &str)> {
    errors
        .iter()
        .map(|error| (error.node, error.message.as_str()))
        .collect()
}

#[test]
fn a_tree_file_is_checked_and_each_problem_names_its_node() {
    // Fine: a Root, a Selector under it, and two leaves.
    let good = tree(
        &[
            (1, "root", &[]),
            (2, "selector", &[]),
            (3, "sees_player", &[]),
            (4, "wait", &[("Seconds", "2")]),
        ],
        &[(1, "", 2), (2, "1", 3), (2, "2", 4)],
    );
    let checked = BehaviorTree::from_json(&good).unwrap();
    assert_eq!(checked.len(), 3);

    // No Root.
    let errors =
        BehaviorTree::from_json(&tree(&[(1, "wait", &[("Seconds", "1")])], &[])).unwrap_err();
    assert_eq!(
        messages(&errors),
        vec![(None, "A behavior tree needs a Root node: it starts there.")]
    );

    // A Root with nothing under it, a Sequence with no children, a number
    // that isn't one, an unknown kind and a second Root.
    let errors = BehaviorTree::from_json(&tree(
        &[
            (1, "root", &[]),
            (2, "sequence", &[]),
            (3, "move_to", &[("X", "ten"), ("Y", "0"), ("Z", "0")]),
            (4, "fly", &[]),
            (5, "root", &[]),
        ],
        &[(5, "", 2)],
    ))
    .unwrap_err();
    let nodes: Vec<Option<u32>> = errors.iter().map(|error| error.node).collect();
    assert!(nodes.contains(&Some(1)), "{errors:?}");
    assert!(nodes.contains(&Some(3)), "{errors:?}");
    assert!(nodes.contains(&Some(4)), "{errors:?}");
    assert!(nodes.contains(&Some(5)), "{errors:?}");
    let x = errors.iter().find(|error| error.node == Some(3)).unwrap();
    assert_eq!(x.message, "X needs a number, not \"ten\".");

    // A loop that a damaged file could hold.
    let errors = BehaviorTree::from_json(&tree(
        &[(1, "root", &[]), (2, "sequence", &[]), (3, "selector", &[])],
        &[(1, "", 2), (2, "1", 3), (3, "1", 2)],
    ))
    .unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains("loops")),
        "{errors:?}"
    );

    // Not a tree at all, or from a newer engine.
    assert!(BehaviorTree::from_json("{ \"states\": [] }").is_err());
    assert!(BehaviorTree::from_json("{ \"format_version\": 9 }").is_err());
}

/// Chase the player while it's seen, else walk between (10, 0, 0) and
/// (-10, 0, 0), waiting half a second at each.
fn guard_tree() -> String {
    tree(
        &[
            (1, "root", &[]),
            (2, "selector", &[]),
            (3, "sequence", &[]),
            (4, "sees_player", &[]),
            (5, "chase_player", &[]),
            (6, "sequence", &[]),
            (7, "move_to", &[("X", "10"), ("Y", "0"), ("Z", "0")]),
            (8, "wait", &[("Seconds", "0.5")]),
            (9, "move_to", &[("X", "-10"), ("Y", "0"), ("Z", "0")]),
            (10, "wait", &[("Seconds", "0.5")]),
        ],
        &[
            (1, "", 2),
            (2, "1", 3),
            (3, "1", 4),
            (3, "2", 5),
            (2, "2", 6),
            (6, "1", 7),
            (6, "2", 8),
            (6, "3", 9),
            (6, "4", 10),
        ],
    )
}

#[test]
fn a_guard_with_a_tree_patrols_and_chases_the_player_while_it_sees_them() {
    let (_folder, assets) = project(
        "chase",
        &[("assets/ai/Guard.behaviortree.json", &guard_tree())],
    );
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(100.0, 1.0, 100.0));
    let (agent, guard) = body_with(&mut world, ClassName::NavigationAgent, at(0.0, 0.0, 0.0));
    set(&mut world, agent, "SightRange", Value::Number(30.0));
    set(&mut world, agent, "SightAngle", Value::Number(360.0));
    set(
        &mut world,
        agent,
        "BehaviorTree",
        Value::String("assets/ai/Guard.behaviortree.json".to_owned()),
    );
    // Out of sight: 40 studs or more from anywhere on the patrol.
    let (_controller, player) = body_with(
        &mut world,
        ClassName::ThirdPersonController,
        at(0.0, 0.0, 40.0),
    );
    let mut agents = Agents::default();

    // It patrols: to (10, 0, 0), then over to (-10, 0, 0).
    let mut reached_east = false;
    let mut reached_west = false;
    for _ in 0..(4 * 60) {
        agents.step(&mut world, &assets, 1.0 / 60.0);
        let x = feet(&world, guard).x;
        reached_east |= x > 9.5;
        reached_west |= reached_east && x < -9.5;
    }
    assert!(agents.take_warnings().is_empty());
    assert!(reached_east && reached_west, "{:?}", feet(&world, guard));
    assert!(
        !world
            .get(agent)
            .unwrap()
            .property("SeesPlayer")
            .unwrap()
            .as_bool()
            .unwrap()
    );

    // The player walks into view: the guard goes to them and stops beside
    // them.
    put(&mut world, player, at(0.0, 0.0, 15.0));
    run(&mut agents, &mut world, &assets, 3.0);
    let here = feet(&world, guard);
    assert!(across(here, at(0.0, 0.0, 15.0)) <= 2.6, "{here:?}");
    run(&mut agents, &mut world, &assets, 0.5);
    assert!(
        across(feet(&world, guard), here) < 0.01,
        "it stays beside them"
    );

    // They move on, still in sight, and it follows.
    put(&mut world, player, at(15.0, 0.0, 25.0));
    run(&mut agents, &mut world, &assets, 2.0);
    assert!(across(feet(&world, guard), at(15.0, 0.0, 25.0)) <= 2.6);

    // They get away, out of sight: it goes back to its patrol.
    put(&mut world, player, at(-45.0, 0.0, -45.0));
    run(&mut agents, &mut world, &assets, 0.2);
    let destination = vector(&world, agent, "Destination");
    assert!(
        across(destination, at(10.0, 0.0, 0.0)) < 0.01
            || across(destination, at(-10.0, 0.0, 0.0)) < 0.01,
        "{destination:?}"
    );
}

#[test]
fn a_wait_starts_again_when_an_earlier_branch_takes_over() {
    // While it hears the player, go to (5, 0, 0); otherwise wait a second,
    // then go to (-5, 0, 0).
    let text = tree(
        &[
            (1, "root", &[]),
            (2, "selector", &[]),
            (3, "sequence", &[]),
            (4, "hears_player", &[]),
            (5, "set_destination", &[("X", "5"), ("Y", "0"), ("Z", "0")]),
            (6, "sequence", &[]),
            (7, "wait", &[("Seconds", "1")]),
            (8, "set_destination", &[("X", "-5"), ("Y", "0"), ("Z", "0")]),
        ],
        &[
            (1, "", 2),
            (2, "1", 3),
            (3, "1", 4),
            (3, "2", 5),
            (2, "2", 6),
            (6, "1", 7),
            (6, "2", 8),
        ],
    );
    let (_folder, assets) = project("wait", &[("assets/ai/Ears.behaviortree.json", &text)]);
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(100.0, 1.0, 100.0));
    let (agent, _guard) = body_with(&mut world, ClassName::NavigationAgent, at(0.0, 0.0, 0.0));
    set(
        &mut world,
        agent,
        "BehaviorTree",
        Value::String("assets/ai/Ears.behaviortree.json".to_owned()),
    );
    let (_controller, player) = body_with(
        &mut world,
        ClassName::ThirdPersonController,
        at(0.0, 0.0, 10.0),
    );
    let model = world.get(player).unwrap().parent().unwrap();
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_parent(sound, Some(model)).unwrap();
    let mut agents = Agents::default();

    // Half the wait, then a sound: it goes for (5, 0, 0).
    run(&mut agents, &mut world, &assets, 0.5);
    set(&mut world, sound, "Playing", Value::Bool(true));
    run(&mut agents, &mut world, &assets, 0.1);
    assert!(across(vector(&world, agent, "Destination"), at(5.0, 0.0, 0.0)) < 0.01);

    // Quiet again: the wait starts over, so after 0.7 s it's still going for
    // (5, 0, 0), and after a whole second it sets (-5, 0, 0).
    set(&mut world, sound, "Playing", Value::Bool(false));
    run(&mut agents, &mut world, &assets, 0.7);
    assert!(across(vector(&world, agent, "Destination"), at(5.0, 0.0, 0.0)) < 0.01);
    run(&mut agents, &mut world, &assets, 0.4);
    assert!(across(vector(&world, agent, "Destination"), at(-5.0, 0.0, 0.0)) < 0.01);
}

#[test]
fn a_state_machine_or_a_broken_tree_named_as_an_agents_tree_doesnt_run() {
    let broken = tree(&[(1, "root", &[])], &[]);
    let (_folder, assets) = project(
        "not-a-tree",
        &[
            (
                "assets/animations/Hero.statemachine.json",
                "{ \"format_version\": 1, \"states\": [] }",
            ),
            ("assets/ai/Broken.behaviortree.json", &broken),
        ],
    );
    for (path, why) in [
        (
            "assets/animations/Hero.statemachine.json",
            "it's a state machine",
        ),
        (
            "assets/ai/Broken.behaviortree.json",
            "Wire what the tree does to Root's pin",
        ),
    ] {
        let mut world = World::new();
        part(&mut world, at(0.0, -0.5, 0.0), at(100.0, 1.0, 100.0));
        let (agent, guard) = body_with(&mut world, ClassName::NavigationAgent, at(0.0, 0.0, 0.0));
        set(
            &mut world,
            agent,
            "BehaviorTree",
            Value::String(path.to_owned()),
        );
        let mut agents = Agents::default();
        run(&mut agents, &mut world, &assets, 1.0);

        // It warns once and stays put, as an agent with no tree would.
        let warnings = agents.take_warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains(why), "{warnings:?}");
        assert!(across(feet(&world, guard), at(0.0, 0.0, 0.0)) < 0.01);

        // A script can still send it somewhere.
        set(
            &mut world,
            agent,
            "Destination",
            Value::Vector3(at(6.0, 0.0, 0.0)),
        );
        run(&mut agents, &mut world, &assets, 1.0);
        assert!(across(feet(&world, guard), at(6.0, 0.0, 0.0)) < 0.01);
        assert!(agents.take_warnings().is_empty());
    }
}
