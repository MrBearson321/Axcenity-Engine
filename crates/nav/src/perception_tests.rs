//! Agents seeing and hearing the player character.

use axcenity_core::{ClassName, InstanceId, Value};

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
/// (a controller or an agent) in it. Returns that and the body.
fn body_with(world: &mut World, class: ClassName, feet: Vector3) -> (InstanceId, InstanceId) {
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
    let holder = world.create(class).unwrap();
    world.set_parent(holder, Some(model)).unwrap();
    (holder, body)
}

struct Scene {
    world: World,
    wall: InstanceId,
    agent: InstanceId,
    guard: InstanceId,
    controller: InstanceId,
    /// The player character's body.
    player: InstanceId,
}

/// A 100×100 floor with its top at Y = 0, a wall 8 wide, 8 tall and 2 thick
/// across Z = -10, a Guard agent standing at the origin facing -Z (sight 30
/// studs and 90° across, hearing 25 studs), and the player character behind
/// the wall at (0, 0, -20).
fn guard_and_player() -> Scene {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(100.0, 1.0, 100.0));
    let wall = part(&mut world, at(0.0, 4.0, -10.0), at(8.0, 8.0, 2.0));
    let (agent, guard) = body_with(&mut world, ClassName::NavigationAgent, at(0.0, 0.0, 0.0));
    set(&mut world, agent, "SightRange", Value::Number(30.0));
    set(&mut world, agent, "SightAngle", Value::Number(90.0));
    set(&mut world, agent, "HearingRange", Value::Number(25.0));
    let (controller, player) = body_with(
        &mut world,
        ClassName::ThirdPersonController,
        at(0.0, 0.0, -20.0),
    );
    Scene {
        world,
        wall,
        agent,
        guard,
        controller,
        player,
    }
}

fn flag(world: &World, agent: InstanceId, name: &str) -> bool {
    world
        .get(agent)
        .unwrap()
        .property(name)
        .and_then(Value::as_bool)
        .unwrap()
}

/// Puts the player's feet at `feet`.
fn put(world: &mut World, player: InstanceId, feet: Vector3) {
    set(
        world,
        player,
        "Position",
        Value::Vector3(at(feet.x, feet.y + 2.5, feet.z)),
    );
}

/// Steps the agents once, a sixtieth of a second, and returns the senses
/// that started.
fn step(agents: &mut Agents, world: &mut World) -> Vec<Sense> {
    agents.step(world, &Assets::default(), 1.0 / 60.0);
    agents
        .take_sensed()
        .into_iter()
        .map(|sensed| sensed.sense)
        .collect()
}

/// Gives the player character a Sound, playing or not.
fn sound(world: &mut World, player: InstanceId, playing: bool) -> InstanceId {
    let model = world.get(player).unwrap().parent().unwrap();
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_parent(sound, Some(model)).unwrap();
    set(world, sound, "Playing", Value::Bool(playing));
    sound
}

#[test]
fn an_agent_sees_the_player_walk_into_view_but_not_behind_a_wall_or_outside_its_angle() {
    let Scene {
        mut world,
        agent,
        player,
        ..
    } = guard_and_player();
    let mut agents = Agents::default();

    // Behind the wall, in range and inside the angle: hidden.
    assert_eq!(step(&mut agents, &mut world), vec![]);
    assert!(!flag(&world, agent, "SeesPlayer"));

    // Walking out from behind the wall toward -X, it comes into view once,
    // and stays seen.
    let mut seen_at = None;
    for i in 1..=60 {
        let x = -0.25 * i as f32;
        put(&mut world, player, at(x, 0.0, -20.0));
        let started = step(&mut agents, &mut world);
        if seen_at.is_none() && !started.is_empty() {
            assert_eq!(started, vec![Sense::Sight]);
            seen_at = Some(x);
        } else {
            assert_eq!(started, vec![], "once only, at {x}");
        }
    }
    let seen_at = seen_at.expect("seen once clear of the wall");
    // The wall's edge is at X = -4, half way: from the eyes, the line to the
    // player clears it a little past X = -8.
    assert!((-10.0..-6.0).contains(&seen_at), "seen at X = {seen_at}");
    assert!(flag(&world, agent, "SeesPlayer"));

    // Outside the angle (beside it, 90° off its front): not seen.
    put(&mut world, player, at(-20.0, 0.0, 0.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));
    // Behind it: not seen.
    put(&mut world, player, at(0.0, 0.0, 15.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));

    // Back in front, in the open: seen again, and reported again.
    put(&mut world, player, at(-15.0, 0.0, -20.0));
    assert_eq!(step(&mut agents, &mut world), vec![Sense::Sight]);
}

#[test]
fn a_part_that_does_not_collide_does_not_block_sight() {
    let Scene {
        mut world,
        wall,
        agent,
        ..
    } = guard_and_player();
    set(&mut world, wall, "CanCollide", Value::Bool(false));
    let mut agents = Agents::default();
    assert_eq!(step(&mut agents, &mut world), vec![Sense::Sight]);
    assert!(flag(&world, agent, "SeesPlayer"));
}

#[test]
fn an_agent_hears_a_sound_playing_in_the_player_through_a_wall_and_only_within_range() {
    let Scene {
        mut world,
        agent,
        player,
        ..
    } = guard_and_player();
    let mut agents = Agents::default();
    let footsteps = sound(&mut world, player, false);

    // Behind the wall, 20 studs away: a Sound that isn't playing is silent.
    assert_eq!(step(&mut agents, &mut world), vec![]);
    assert!(!flag(&world, agent, "HearsPlayer"));

    // Playing, it's heard through the wall, once, and not seen.
    set(&mut world, footsteps, "Playing", Value::Bool(true));
    assert_eq!(step(&mut agents, &mut world), vec![Sense::Hearing]);
    assert_eq!(step(&mut agents, &mut world), vec![]);
    assert!(flag(&world, agent, "HearsPlayer"));
    assert!(!flag(&world, agent, "SeesPlayer"));

    // Behind the agent, where it can't see: still heard.
    put(&mut world, player, at(0.0, 0.0, 15.0));
    step(&mut agents, &mut world);
    assert!(flag(&world, agent, "HearsPlayer"));
    assert!(!flag(&world, agent, "SeesPlayer"));

    // Out of range: not heard.
    put(&mut world, player, at(0.0, 0.0, 40.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "HearsPlayer"));

    // A sound playing somewhere else isn't the player making it.
    put(&mut world, player, at(0.0, 0.0, -20.0));
    set(&mut world, footsteps, "Playing", Value::Bool(false));
    let elsewhere = world.create(ClassName::Sound).unwrap();
    world
        .set_parent(elsewhere, Some(world.workspace()))
        .unwrap();
    set(&mut world, elsewhere, "Playing", Value::Bool(true));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "HearsPlayer"));
}

#[test]
fn sight_range_sight_angle_and_hearing_range_change_what_it_notices() {
    let Scene {
        mut world,
        agent,
        player,
        ..
    } = guard_and_player();
    let mut agents = Agents::default();
    sound(&mut world, player, true);
    // In the open, 15 studs out and 30° off its front.
    put(&mut world, player, at(-8.66, 0.0, -15.0));
    step(&mut agents, &mut world);
    assert!(flag(&world, agent, "SeesPlayer"));
    assert!(flag(&world, agent, "HearsPlayer"));

    // A narrower angle (40° across, 20° each side) leaves it out.
    set(&mut world, agent, "SightAngle", Value::Number(40.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));
    // 360° sees all round, even behind.
    set(&mut world, agent, "SightAngle", Value::Number(360.0));
    put(&mut world, player, at(0.0, 0.0, 15.0));
    step(&mut agents, &mut world);
    assert!(flag(&world, agent, "SeesPlayer"));

    // Shorter ranges than the player is away from its eyes.
    set(&mut world, agent, "SightRange", Value::Number(10.0));
    set(&mut world, agent, "HearingRange", Value::Number(10.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));
    assert!(!flag(&world, agent, "HearsPlayer"));

    // Zero senses nothing, even right beside it.
    set(&mut world, agent, "SightRange", Value::Number(0.0));
    set(&mut world, agent, "HearingRange", Value::Number(0.0));
    put(&mut world, player, at(0.0, 0.0, -3.0));
    assert_eq!(step(&mut agents, &mut world), vec![]);
    assert!(!flag(&world, agent, "SeesPlayer"));
    assert!(!flag(&world, agent, "HearsPlayer"));
    set(&mut world, agent, "SightRange", Value::Number(30.0));
    set(&mut world, agent, "SightAngle", Value::Number(0.0));
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));
}

#[test]
fn an_agent_faces_its_body_front_and_then_the_way_it_walks() {
    let Scene {
        mut world,
        wall,
        agent,
        guard,
        ..
    } = guard_and_player();
    world.destroy(wall).unwrap();
    let mut agents = Agents::default();
    // Turned 180°, its front is +Z: the player at -Z is behind it.
    set(
        &mut world,
        guard,
        "Orientation",
        Value::Vector3(at(0.0, 180.0, 0.0)),
    );
    step(&mut agents, &mut world);
    assert!(!flag(&world, agent, "SeesPlayer"));

    // It walks toward -Z, so it faces -Z, and stays facing that way when it
    // stops.
    set(
        &mut world,
        agent,
        "Destination",
        Value::Vector3(at(0.0, 0.0, -2.0)),
    );
    for _ in 0..60 {
        step(&mut agents, &mut world);
    }
    assert!(flag(&world, agent, "SeesPlayer"));
}

#[test]
fn with_no_player_character_an_agent_senses_nothing() {
    let Scene {
        mut world,
        agent,
        controller,
        player,
        ..
    } = guard_and_player();
    // A model without a controller isn't the player character.
    world.destroy(controller).unwrap();
    put(&mut world, player, at(-15.0, 0.0, -20.0));
    let mut agents = Agents::default();
    assert_eq!(step(&mut agents, &mut world), vec![]);
    assert!(!flag(&world, agent, "SeesPlayer"));
}
