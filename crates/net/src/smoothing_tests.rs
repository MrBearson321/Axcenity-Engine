//! Phase 65: replication's motion, budgets and joining mid-game, with a
//! dedicated server and players' devices in this process.

use std::collections::BTreeMap;
use std::time::Duration;

use axcenity_core::{ClassName, Color3, InstanceId, Value, World};
use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{Device, castle, frames, info, player, server, three_players};

fn x_of(world: &World, id: InstanceId) -> Option<f32> {
    world
        .get(id)?
        .property("Position")?
        .as_vector3()
        .map(|p| p.x)
}

fn find(world: &World, name: &str) -> Option<InstanceId> {
    world.find_first_child(world.workspace(), name)
}

#[test]
fn a_part_a_server_script_makes_moves_smoothly_and_disappears_on_every_client() {
    let (mut server, mut players) = three_players();
    // A jittery network: motion arrives late and unevenly.
    server.network.emulate(Conditions {
        latency: Duration::from_millis(40),
        jitter: Duration::from_millis(15),
        loss: 0.0,
    });
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local crate = Instance.new("Part")
crate.Name = "Crate"
crate.Anchored = true
crate.Position = Vector3.new(0, 5, 0)
crate.Parent = workspace
_G.crate = crate
task.spawn(function()
    for i = 1, 120 do
        crate.Position = Vector3.new(i * 0.1, 5, 0)
        task.wait()
    end
end)
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    let crate_id = find(&server.world, "Crate").unwrap();
    let mut seen: Vec<Vec<f32>> = vec![Vec::new(); players.len()];
    for _ in 0..170 {
        frames(&mut server, &mut players, 1);
        for (player, xs) in players.iter().zip(&mut seen) {
            // It arrives a moment later, over the slow network.
            xs.extend(x_of(&player.world, crate_id));
        }
    }
    for xs in &seen {
        // It moved 6 studs a second, 0.1 a frame: shown smoothly, never
        // backward and never more than a little more than a frame's move.
        for pair in xs.windows(2) {
            let step = pair[1] - pair[0];
            assert!(step >= -1e-4, "it never goes back: {xs:?}");
            assert!(step <= 0.16, "no jumps, {step} in a frame: {xs:?}");
        }
        let moving = xs.windows(2).filter(|pair| pair[1] > pair[0]).count();
        assert!(moving > 100, "it moved most frames: {moving}");
        assert!((xs.last().unwrap() - 12.0).abs() < 1e-3, "{xs:?}");
    }
    server
        .scripts
        .run_command(&mut server.world, "_G.crate:Destroy()")
        .unwrap();
    frames(&mut server, &mut players, 10);
    for player in &players {
        assert!(!player.world.contains(crate_id));
    }
}

#[test]
fn a_colour_a_player_sets_on_their_replica_stays_theirs() {
    let (mut server, mut players) = three_players();
    let gate = find(&server.world, "Gate").unwrap();
    let red = Value::Color3(Color3::new(1.0, 0.0, 0.0));
    players[0]
        .world
        .set_property(gate, "Color", red.clone())
        .unwrap();
    // The server keeps changing something else of the gate's.
    for i in 0..30 {
        server
            .world
            .set_property(gate, "Transparency", Value::Number(f64::from(i) / 60.0))
            .unwrap();
        frames(&mut server, &mut players, 1);
    }
    let colour = |device: &Device| device.world.get(gate).unwrap().property("Color").cloned();
    assert_eq!(colour(&players[0]), Some(red.clone()));
    assert_ne!(colour(&server), Some(red.clone()));
    for player in &players[1..] {
        assert_eq!(colour(player), colour(&server));
    }
    assert_eq!(
        players[0].world.get(gate).unwrap().property("Transparency"),
        Some(&Value::Number(29.0 / 60.0)),
        "the server's own changes still arrive"
    );
}

/// The replicated Instances of `world` (everything outside ServerStorage
/// and ServerScriptService), with their class, name, parent and
/// properties, for comparing devices.
fn replicated_world(world: &World) -> BTreeMap<u64, String> {
    let mut seen = BTreeMap::new();
    for id in world.descendants(world.root()) {
        if world.is_server_only(id) || id.is_local() {
            continue;
        }
        let instance = world.get(id).unwrap();
        if instance.class() == ClassName::Player {
            continue;
        }
        let mut properties: Vec<String> = instance
            .properties()
            .map(|(name, value)| format!("{name}={value:?}"))
            .collect();
        properties.sort();
        seen.insert(
            id.to_bits(),
            format!(
                "{:?} {} in {:?}: {}",
                instance.class(),
                instance.name(),
                instance.parent(),
                properties.join(", ")
            ),
        );
    }
    seen
}

#[test]
fn a_player_who_joins_mid_game_sees_the_same_world_with_added_levels() {
    let castle = castle();
    let mut world = castle.world;
    let mut cave = World::new();
    let rock = cave.create(ClassName::Part).unwrap();
    cave.set_name(rock, "Stalactite").unwrap();
    cave.set_parent(rock, Some(cave.workspace())).unwrap();
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    let mut levels = BTreeMap::new();
    levels.insert("Start".to_owned(), world.clone());
    levels.insert("Cave".to_owned(), cave);
    scripts.set_levels("Start", "Start", levels);
    let mut network = Network::new(info()).with_discovery_port(47932);
    let first = network.host_in_memory(&mut world, &mut scripts, false);
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    let mut players = vec![player(first, 201.0)];
    frames(&mut server, &mut players, 5);

    // The game goes on: a level is added, parts are made and moved.
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
game.LevelService:AddLevel("Cave")
for i = 1, 5 do
    local part = Instance.new("Part")
    part.Name = "Block" .. i
    part.Position = Vector3.new(i, 1, 0)
    part.Parent = workspace
end
workspace.Gate.Position = Vector3.new(0, 20, 0)
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 30);
    let client = server.network.connect_loopback().unwrap();
    players.push(player(client, 202.0));
    frames(&mut server, &mut players, 30);

    let late = &players[1];
    assert!(
        find(&late.world, "Cave").is_some(),
        "the added level is there"
    );
    assert_eq!(late.scripts.levels().added().collect::<Vec<_>>(), ["Cave"]);
    assert_eq!(late.scripts.levels().current(), "Start");
    let expected = replicated_world(&server.world);
    for device in &players {
        let got = replicated_world(&device.world);
        for (id, instance) in &expected {
            assert_eq!(got.get(id), Some(instance), "the same Instance everywhere");
        }
    }
}

#[test]
fn two_hundred_moving_crates_stay_under_the_bandwidth_budget_per_client() {
    /// What each player may get, all told: motion's budget, plus room
    /// for acknowledgements and the reliable channel.
    const BUDGET: f64 = MOTION_BUDGET + 8_000.0;
    let (mut server, first) = server();
    let mut players = vec![player(first, 301.0)];
    frames(&mut server, &mut players, 5);
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local crates = {}
for i = 1, 200 do
    local crate = Instance.new("Part")
    crate.Name = "Crate" .. i
    crate.Anchored = true
    crate.Position = Vector3.new(i % 20 * 4, 1, i // 20 * 4)
    crate.Parent = workspace
    crates[i] = crate
end
local t = 0
game:GetService("RunService").Heartbeat:Connect(function(dt)
    t += dt
    for i, crate in crates do
        crate.Position = Vector3.new(i % 20 * 4 + math.sin(t + i), 1, i // 20 * 4)
    end
end)
"#,
        )
        .unwrap();
    // The crates arrive, then three seconds are measured.
    let sent = |server: &Device| server.network.stats()[0].loopback_bytes_sent.unwrap();
    for _ in 0..60 {
        server.frame_with_heartbeat();
        players[0].frame();
    }
    let crate_one = find(&server.world, "Crate1").unwrap();
    let before = sent(&server);
    let mut moves = 0;
    let mut last = x_of(&players[0].world, crate_one);
    for _ in 0..180 {
        server.frame_with_heartbeat();
        players[0].frame();
        let x = x_of(&players[0].world, crate_one);
        if x != last {
            moves += 1;
        }
        last = x;
    }
    let per_second = (sent(&server) - before) as f64 / 3.0;
    assert!(
        per_second < BUDGET,
        "{per_second} bytes a second, over the budget of {BUDGET}"
    );
    // Sent whole every frame, they'd take 200 × 32 × 60 bytes a second.
    assert!(per_second < (200 * ENTRY_BYTES * 60) as f64 / 10.0);
    // Every crate is still kept moving on the player's device.
    assert!(moves > 60, "crate 1 moved in {moves} of 180 frames");
    let positions_match = (1..=200).all(|i| {
        let id = find(&server.world, &format!("Crate{i}")).unwrap();
        let server_x = x_of(&server.world, id).unwrap();
        let player_x = x_of(&players[0].world, id).unwrap();
        (server_x - player_x).abs() < 0.6
    });
    assert!(
        positions_match,
        "every crate is near where the server has it"
    );
}
