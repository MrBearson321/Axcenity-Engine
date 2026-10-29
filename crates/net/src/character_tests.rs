//! Phase 66: players' own characters and owned assemblies, with a
//! dedicated server and players' devices in this process, each with its
//! character controllers and physics as the player runs them.

use std::time::Duration;

use axcenity_assets::Assets;
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::Physics;
use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{SECONDS, info};

/// A floor reaching far toward -Z, and a car facing -Z on it: a chassis,
/// a VehicleSeat welded on top and four wheels (the front ones steer).
fn arena() -> World {
    let mut world = World::new();
    let part = |world: &mut World, name: &str, position: Vector3, size: Vector3, anchored: bool| {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(world.workspace())).unwrap();
        world
            .set_property(id, "Position", Value::Vector3(position))
            .unwrap();
        world
            .set_property(id, "Size", Value::Vector3(size))
            .unwrap();
        world
            .set_property(id, "Anchored", Value::Bool(anchored))
            .unwrap();
        id
    };
    part(
        &mut world,
        "Floor",
        Vector3::new(0.0, -0.5, -80.0),
        Vector3::new(200.0, 1.0, 400.0),
        true,
    );
    let at = Vector3::new(40.0, 2.0, 0.0);
    let chassis = part(
        &mut world,
        "Chassis",
        at,
        Vector3::new(6.0, 1.0, 10.0),
        false,
    );
    let seat = world.create(ClassName::VehicleSeat).unwrap();
    world.set_name(seat, "DriverSeat").unwrap();
    world.set_parent(seat, Some(world.workspace())).unwrap();
    for (name, value) in [
        (
            "Position",
            Value::Vector3(Vector3::new(at.x, at.y + 1.0, at.z + 1.0)),
        ),
        ("Size", Value::Vector3(Vector3::new(2.0, 1.0, 2.0))),
        ("Anchored", Value::Bool(false)),
    ] {
        world.set_property(seat, name, value).unwrap();
    }
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(chassis)).unwrap();
    world
        .set_property(weld, "Part0", Value::Instance(Some(chassis)))
        .unwrap();
    world
        .set_property(weld, "Part1", Value::Instance(Some(seat)))
        .unwrap();
    for (x, z) in [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
        let wheel = world.create(ClassName::VehicleWheel).unwrap();
        world.set_parent(wheel, Some(chassis)).unwrap();
        world
            .set_property(wheel, "Position", Value::Vector3(Vector3::new(x, -1.0, z)))
            .unwrap();
        world
            .set_property(wheel, "Steers", Value::Bool(z < 0.0))
            .unwrap();
    }
    world
}

/// One device, run as the player runs it.
struct Device {
    world: World,
    scripts: ScriptRuntime,
    network: Network,
    characters: Controllers,
    physics: Physics,
    assets: Assets,
    log: Vec<String>,
}

impl Device {
    fn frame(&mut self, input: MoveInput) {
        self.network
            .receive(&mut self.world, &mut self.scripts, SECONDS);
        self.log.extend(self.network.take_log());
        let following = self.network.is_following();
        let others = self.network.other_characters(&self.world, &self.scripts);
        self.characters
            .set_players(self.scripts.local_player(), others);
        self.characters
            .step(&mut self.world, &self.assets, input, SECONDS as f32);
        if !following {
            self.scripts.step_players(&mut self.world, SECONDS);
        }
        self.scripts.step(&mut self.world, SECONDS);
        if !following || self.network.simulates_physics() {
            let held = self.network.held_parts(&self.world, &self.scripts);
            self.physics.set_held(held);
            self.characters.keep_kinematic(&mut self.world);
            self.physics.step(&mut self.world, SECONDS);
        }
        self.scripts.heartbeat(&mut self.world, SECONDS);
        self.network.set_running(input.run);
        self.network.send(&self.world, &mut self.scripts);
        self.log.extend(self.network.take_log());
    }

    fn character(&self, player: InstanceId) -> Option<InstanceId> {
        self.world.get(player)?.property("Character")?.as_instance()
    }

    /// The first part of `player`'s character, where this device has it.
    fn character_at(&self, player: InstanceId) -> Option<Vector3> {
        let character = self.character(player)?;
        let root = self
            .world
            .descendants(character)
            .into_iter()
            .find(|&id| self.world.get(id).is_some_and(|i| i.class().is_base_part()))?;
        position(&self.world, root)
    }
}

fn position(world: &World, id: InstanceId) -> Option<Vector3> {
    world.get(id)?.property("Position")?.as_vector3()
}

fn device(world: World, scripts: ScriptRuntime, network: Network) -> Device {
    Device {
        world,
        scripts,
        network,
        characters: Controllers::default(),
        physics: Physics::new(),
        assets: Assets::default(),
        log: Vec::new(),
    }
}

/// `axcenity-server` hosting the arena in memory, and `players` devices
/// joined to it, with a little time for each to get their character.
fn game(players: usize, latency: Duration) -> (Device, Vec<Device>) {
    let mut world = arena();
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    let mut network = Network::new(info()).with_discovery_port(47933);
    let first = network.host_in_memory(&mut world, &mut scripts, false);
    network.emulate(Conditions {
        latency,
        ..Conditions::default()
    });
    let mut server = device(world, scripts, network);
    let mut devices = Vec::new();
    let mut first = Some(first);
    for n in 0..players {
        let client = match first.take() {
            Some(client) => client,
            None => server.network.connect_loopback().unwrap(),
        };
        let mut world = arena();
        let mut scripts = ScriptRuntime::new();
        scripts.set_user_id(500.0 + n as f64);
        let mut network = Network::new(info());
        network.join_through(&mut world, &mut scripts, client, None);
        devices.push(device(world, scripts, network));
    }
    frames(&mut server, &mut devices, 90, &[]);
    (server, devices)
}

/// `count` frames, with each player's input (none for those left out).
fn frames(server: &mut Device, players: &mut [Device], count: usize, inputs: &[MoveInput]) {
    for _ in 0..count {
        server.frame(MoveInput::default());
        for (n, player) in players.iter_mut().enumerate() {
            player.frame(inputs.get(n).copied().unwrap_or_default());
        }
    }
}

fn forward() -> MoveInput {
    MoveInput {
        forward: true,
        ..MoveInput::default()
    }
}

#[test]
fn with_150_ms_of_latency_a_players_own_character_moves_at_once_and_others_see_it_smoothly() {
    let (mut server, mut players) = game(2, Duration::from_millis(150));
    let a = players[0].scripts.local_player().unwrap();
    let start = players[0]
        .character_at(a)
        .expect("player 1 has their character");
    assert!(players[1].character_at(a).is_some(), "player 2 sees it");

    // The first frame player 1 holds W, their character moves on their
    // device: no round trip to the server first.
    frames(&mut server, &mut players, 1, &[forward()]);
    let moved = players[0].character_at(a).unwrap();
    assert!(
        (moved.z - start.z).abs() > 0.1 || (moved.x - start.x).abs() > 0.1,
        "it moved this frame: {start:?} to {moved:?}"
    );

    // Player 2 sees it a little later, smoothly.
    let mut seen = Vec::new();
    for _ in 0..90 {
        frames(&mut server, &mut players, 1, &[forward()]);
        seen.push(players[1].character_at(a).unwrap());
    }
    let walked = |p: Vector3| f64::from((p.x - start.x).hypot(p.z - start.z));
    let distances: Vec<f64> = seen.iter().map(|&p| walked(p)).collect();
    assert!(
        distances.last().unwrap() > &15.0,
        "player 2 saw it walk: {distances:?}"
    );
    for pair in distances.windows(2) {
        let step = pair[1] - pair[0];
        assert!(step >= -1e-3, "never back: {distances:?}");
        // WalkSpeed 16: about 0.27 studs a frame.
        assert!(step < 0.45, "no jumps ({step}): {distances:?}");
    }
    assert!(
        !players[0]
            .log
            .iter()
            .any(|line| line.contains("moved your character back")),
        "an honest player isn't corrected: {:?}",
        players[0].log
    );
}

#[test]
fn a_modified_client_that_moves_at_twice_walk_speed_is_pulled_back_by_the_server() {
    let (mut server, mut players) = game(1, Duration::ZERO);
    let me = players[0].scripts.local_player().unwrap();
    let start = server.character_at(me).unwrap();
    let character = players[0].character(me).unwrap();
    let parts: Vec<InstanceId> = players[0]
        .world
        .descendants(character)
        .into_iter()
        .filter(|&id| {
            players[0]
                .world
                .get(id)
                .is_some_and(|i| i.class().is_base_part())
        })
        .collect();
    // The modified client: its character goes 32 studs a second toward -Z,
    // twice its WalkSpeed, without Run.
    let step = (32.0 * SECONDS) as f32;
    let mut pulled_back = false;
    let mut last_z = start.z;
    for _ in 0..120 {
        for &part in &parts {
            let world = &mut players[0].world;
            let p = position(world, part).unwrap();
            world
                .set_property(
                    part,
                    "Position",
                    Value::Vector3(Vector3::new(p.x, p.y, p.z - step)),
                )
                .unwrap();
        }
        frames(&mut server, &mut players, 1, &[]);
        let z = players[0].character_at(me).unwrap().z;
        if z > last_z + 0.05 {
            pulled_back = true;
        }
        last_z = z;
    }
    let server_at = server.character_at(me).unwrap();
    let went = f64::from(start.z - server_at.z);
    // It tried 64 studs. The server allows two seconds at WalkSpeed, with
    // the leeway for late packets: 15% faster, and a quarter second saved.
    assert!(
        went < 16.0 * 1.15 * 2.25 + 3.0,
        "the server let it go {went} studs"
    );
    assert!(
        pulled_back,
        "the client was moved back (server let it go {went}): {:?}",
        players[0].log
    );
    assert!(
        players[0]
            .log
            .iter()
            .any(|line| line.contains("moved your character back")),
        "{:?}",
        players[0].log
    );
}

#[test]
fn a_car_owned_by_one_player_steers_at_once_for_them_and_the_others_see_it_drive() {
    let (mut server, mut players) = game(2, Duration::from_millis(100));
    let driver = players[0].scripts.local_player().unwrap();
    let name = server.world.get(driver).unwrap().name().to_owned();
    server
        .scripts
        .run_command(
            &mut server.world,
            &format!(
                "workspace.DriverSeat:SetNetworkOwner(game.Players.{name})\n\
                 assert(workspace.Chassis:GetNetworkOwner() == game.Players.{name})"
            ),
        )
        .unwrap();
    assert_eq!(
        server.world.get(driver).map(|p| p.class()),
        Some(ClassName::Player)
    );
    // The ownership arrives; then the driver puts their foot down.
    frames(&mut server, &mut players, 30, &[]);
    let seat = players[0]
        .world
        .find_first_child(players[0].world.workspace(), "DriverSeat")
        .unwrap();
    let chassis = players[0]
        .world
        .find_first_child(players[0].world.workspace(), "Chassis")
        .unwrap();
    let start = position(&players[0].world, chassis).unwrap();
    let watched = position(&players[1].world, chassis).unwrap();
    players[0]
        .world
        .set_property(seat, "Throttle", Value::Number(1.0))
        .unwrap();
    players[0]
        .world
        .set_property(seat, "Steer", Value::Number(1.0))
        .unwrap();
    frames(&mut server, &mut players, 3, &[]);
    let now = position(&players[0].world, chassis).unwrap();
    assert!(
        players[0].network.simulates_physics(),
        "the driver simulates the car"
    );
    assert!(
        crate::motion::distance(start, now) > 0.01,
        "it moves on the driver's device within three frames: {start:?} {now:?}"
    );
    let mut seen = Vec::new();
    let mut path = Vec::new();
    for _ in 0..120 {
        frames(&mut server, &mut players, 1, &[]);
        seen.push(position(&players[1].world, chassis).unwrap());
        path.push(position(&players[0].world, chassis).unwrap());
    }
    let theirs = *seen.last().unwrap();
    assert!(
        crate::motion::distance(watched, theirs) > 5.0,
        "player 2 saw it drive"
    );
    // Where the driver had it a moment ago: the latency both ways, and
    // the smoothing's delay.
    let near_path = |at: Vector3| {
        path[path.len() - 40..]
            .iter()
            .map(|&p| crate::motion::distance(p, at))
            .fold(f64::INFINITY, f64::min)
    };
    assert!(
        near_path(theirs) < 1.0,
        "on the driver's path: {theirs:?} {path:?}"
    );
    let server_has = position(&server.world, chassis).unwrap();
    assert!(near_path(server_has) < 1.0, "the server follows the driver");
    for pair in seen.windows(2) {
        assert!(
            crate::motion::distance(pair[0], pair[1]) < 1.5,
            "smooth: {seen:?}"
        );
    }
}
