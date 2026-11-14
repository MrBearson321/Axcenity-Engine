//! MP6a: characters the server moves from players' input, predicted on
//! their devices, with a dedicated server and devices in this process
//! over the loopback with emulated latency, jitter and loss.

use std::time::Duration;

use axcenity_character::MoveInput;
use axcenity_core::{InstanceId, Value, Vector3, World};

use super::*;
use crate::character_tests::{Device, arena, forward, frames, game_with, position};
use crate::predict::{Move, moves_to_bytes};

/// The arena, with `Players.CharacterAuthority` "Server".
fn server_arena() -> World {
    let mut world = arena();
    let players = world.players();
    world
        .set_property(
            players,
            "CharacterAuthority",
            Value::String("Server".to_owned()),
        )
        .unwrap();
    world
}

fn conditions(latency_ms: u64, jitter_ms: u64, loss: f64) -> Conditions {
    Conditions {
        latency: Duration::from_millis(latency_ms),
        jitter: Duration::from_millis(jitter_ms),
        loss,
    }
}

fn gap(a: Vector3, b: Vector3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn right() -> MoveInput {
    MoveInput {
        right: true,
        ..MoveInput::default()
    }
}

fn jump_forward() -> MoveInput {
    MoveInput {
        forward: true,
        jump: true,
        ..MoveInput::default()
    }
}

/// The first `walkers` players walk, turn, run and jump for a few seconds,
/// then everyone stands still long enough for every answer to arrive.
fn walk_about(server: &mut Device, players: &mut [Device], walkers: usize) {
    let run = MoveInput {
        forward: true,
        run: true,
        ..MoveInput::default()
    };
    for (input, count) in [
        (forward(), 60),
        (right(), 30),
        (jump_forward(), 40),
        (run, 40),
        (MoveInput::default(), 90),
    ] {
        let inputs = vec![input; walkers];
        frames(server, players, count, &inputs);
    }
}

#[test]
fn moves_and_answers_survive_the_wire() {
    let moves = vec![
        Move {
            seq: 7,
            seconds: 1.0 / 60.0,
            yaw: 1.25,
            keys: 1 | 16,
            stick: None,
        },
        Move {
            seq: 8,
            seconds: 0.02,
            yaw: -0.5,
            keys: 0,
            stick: Some([0.5, -1.0]),
        },
    ];
    let bytes = moves_to_bytes(&moves);
    assert_eq!(
        crate::predict::moves_from_bytes(&bytes),
        Some(moves.clone())
    );
    let drive = moves[0].drive();
    assert!(drive.input.forward && drive.input.jump && !drive.input.back);
    let moved = Moved {
        seq: 8,
        feet: [1.0, 2.0, 3.0],
        facing: 90.0,
        rising: -4.0,
        flags: 1 | 4,
    };
    assert_eq!(Moved::from_bytes(&moved.to_bytes()), Some(moved));
    let state = moved.state();
    assert!(state.grounded && state.crouched && !state.airborne);
}

#[test]
fn a_server_moved_character_moves_at_once_and_ends_where_the_server_has_it() {
    let (mut server, mut players) = game_with(server_arena, 2, conditions(150, 0, 0.0));
    let me = players[0].scripts.local_player().unwrap();
    let start = players[0].character_at(me).expect("a character");
    // The first frame W is held the character moves on the device: no
    // round trip first.
    frames(&mut server, &mut players, 1, &[forward()]);
    let moved = players[0].character_at(me).unwrap();
    assert!(gap(moved, start) > 0.1, "{start:?} to {moved:?}");
    // The server doesn't have it there yet: it moves it when the moves
    // arrive.
    assert!(gap(server.character_at(me).unwrap(), start) < 0.01);

    // The other player stands still, so both machines have them in the
    // same place (see `walking_into_a_moving_player_is_corrected`).
    walk_about(&mut server, &mut players, 1);
    let device = players[0].character_at(me).unwrap();
    let authority = server.character_at(me).unwrap();
    assert!(
        gap(device, start) > 20.0,
        "it walked: {start:?} to {device:?}"
    );
    assert!(
        gap(device, authority) < 0.05,
        "device {device:?}, server {authority:?}"
    );
    // The other player sees it there too.
    let seen = players[1].character_at(me).unwrap();
    assert!(gap(seen, authority) < 0.1, "seen {seen:?}");
    let stats = players[0].network.prediction();
    assert!(stats.moves > 200, "{stats:?}");
    // Same code, same input, same world: the server agrees with every
    // prediction, so nothing is taken again.
    assert_eq!(stats.corrections, 0, "{stats:?}");
    assert!(
        !players[0]
            .log
            .iter()
            .any(|line| line.contains("moved your character back")),
        "{:?}",
        players[0].log
    );
}

#[test]
fn with_jitter_and_loss_the_prediction_still_lands_where_the_server_has_it() {
    for (latency, jitter, loss) in [(100, 20, 0.05), (200, 40, 0.1)] {
        let (mut server, mut players) =
            game_with(server_arena, 1, conditions(latency, jitter, loss));
        let me = players[0].scripts.local_player().unwrap();
        let start = players[0].character_at(me).unwrap();
        walk_about(&mut server, &mut players, 1);
        // A little longer for late answers under loss.
        frames(&mut server, &mut players, 60, &[]);
        let device = players[0].character_at(me).unwrap();
        let authority = server.character_at(me).unwrap();
        let stats = players[0].network.prediction();
        println!("{latency} ms, {jitter} ms jitter, {loss} loss: {stats:?}");
        assert!(gap(device, start) > 20.0, "{latency} ms: it walked");
        assert!(
            gap(device, authority) < 0.05,
            "{latency} ms: device {device:?}, server {authority:?}, {stats:?}"
        );
        // Every move went in more than one packet, so lost packets lose no
        // moves: the server took the same steps.
        assert_eq!(stats.snaps, 0, "{latency} ms: {stats:?}");
        assert!(stats.largest < 0.5, "{latency} ms: {stats:?}");
    }
}

/// Two players walking side by side: each device has the other where the
/// server had them a round trip and the smoothing delay ago, so one
/// bumps into the other on its device where it doesn't on the server. The
/// server's answer puts it right.
#[test]
fn walking_into_a_moving_player_is_corrected() {
    let (mut server, mut players) = game_with(server_arena, 2, conditions(150, 0, 0.0));
    walk_about(&mut server, &mut players, 2);
    for player in &players {
        let me = player.scripts.local_player().unwrap();
        let device = player.character_at(me).unwrap();
        let authority = server.character_at(me).unwrap();
        assert!(
            gap(device, authority) < 0.05,
            "device {device:?}, server {authority:?}, {:?}",
            player.network.prediction()
        );
    }
    let corrected: u64 = players
        .iter()
        .map(|player| player.network.prediction().corrections)
        .sum();
    println!("corrections: {corrected}");
}

#[test]
fn a_device_claiming_more_time_than_passed_moves_no_further_than_real_time_allows() {
    let (mut server, mut players) = game_with(server_arena, 1, conditions(0, 0, 0.0));
    let me = players[0].scripts.local_player().unwrap();
    let start = server.character_at(me).unwrap();
    // A modified client: each frame a move of a quarter second (fifteen
    // times the frame) holding W.
    for seq in 10_001..=10_060 {
        let bytes = moves_to_bytes(&[Move {
            seq,
            seconds: 0.25,
            yaw: 0.0,
            keys: 1,
            stick: None,
        }]);
        players[0].network.send_raw(bytes);
        frames(&mut server, &mut players, 1, &[]);
    }
    let went = gap(server.character_at(me).unwrap(), start);
    // One second passed: WalkSpeed 16, plus the half second a player may
    // save up (AHEAD). Fifteen seconds' worth was claimed.
    assert!(
        went <= 16.0 * (1.0 + AHEAD as f32) + 1.0,
        "the server let it walk {went} studs"
    );
    assert!(went > 8.0, "it did walk: {went}");
}

#[test]
fn where_a_device_says_its_server_moved_character_is_counts_for_nothing() {
    let (mut server, mut players) = game_with(server_arena, 1, conditions(50, 0, 0.0));
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
    // The modified client puts its character 20 studs away.
    for &part in &parts {
        let world = &mut players[0].world;
        let p = position(world, part).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(p.x, p.y, p.z - 20.0)),
            )
            .unwrap();
    }
    frames(&mut server, &mut players, 60, &[]);
    let authority = server.character_at(me).unwrap();
    assert!(gap(authority, start) < 0.5, "the server didn't move it");
    let device = players[0].character_at(me).unwrap();
    assert!(
        gap(device, authority) < 0.05,
        "the device was put back: {device:?} vs {authority:?}"
    );
    let stats = players[0].network.prediction();
    assert!(stats.snaps >= 1, "{stats:?}");
}

#[test]
fn a_server_script_teleport_reaches_the_predicting_device() {
    let (mut server, mut players) = game_with(server_arena, 1, conditions(100, 0, 0.0));
    let me = players[0].scripts.local_player().unwrap();
    frames(&mut server, &mut players, 30, &[forward()]);
    // A server Script moves the character 30 studs (a respawn point, a
    // teleporter): the server's world has it there.
    let character = server.character(me).unwrap();
    for part in server.world.descendants(character) {
        if let Some(p) = position(&server.world, part) {
            server
                .world
                .set_property(
                    part,
                    "Position",
                    Value::Vector3(Vector3::new(p.x + 30.0, p.y, p.z)),
                )
                .unwrap();
        }
    }
    frames(&mut server, &mut players, 60, &[forward()]);
    frames(&mut server, &mut players, 60, &[]);
    let authority = server.character_at(me).unwrap();
    let device = players[0].character_at(me).unwrap();
    assert!(
        gap(device, authority) < 0.05,
        "device {device:?}, server {authority:?}"
    );
}

#[test]
fn client_authority_is_unchanged_and_sends_no_moves() {
    let (mut server, mut players) = game_with(arena, 1, conditions(50, 0, 0.0));
    let me = players[0].scripts.local_player().unwrap();
    frames(&mut server, &mut players, 60, &[forward()]);
    frames(&mut server, &mut players, 30, &[]);
    assert_eq!(players[0].network.prediction().moves, 0);
    let device = players[0].character_at(me).unwrap();
    let authority = server.character_at(me).unwrap();
    assert!(gap(device, authority) < 0.05);
}

/// Moves each player's character on the server 12 studs apart across
/// the floor, and lets the devices hear it.
fn spread_out(server: &mut Device, players: &mut [Device]) {
    for (n, player) in players.iter().enumerate() {
        let me = player.scripts.local_player().unwrap();
        let character = server.character(me).unwrap();
        let x = -90.0 + 12.0 * n as f32;
        let root = server.character_at(me).unwrap();
        for part in server.world.descendants(character) {
            if let Some(p) = position(&server.world, part) {
                server
                    .world
                    .set_property(
                        part,
                        "Position",
                        Value::Vector3(Vector3::new(p.x - root.x + x, p.y, p.z)),
                    )
                    .unwrap();
            }
        }
    }
    frames(server, players, 60, &[]);
}

/// Prediction's cost and corrections under 100 and 200 ms with loss,
/// eight players walking about: apart, then in a crowd that jumps onto
/// each other's heads (`cargo test -p axcenity-net --release --
/// --ignored --nocapture prediction_cost`).
#[test]
#[ignore = "a measurement: prints numbers"]
fn prediction_cost() {
    for (latency, jitter, loss) in [(100, 20, 0.02), (200, 40, 0.05)] {
        for crowd in [false, true] {
            let (mut server, mut players) =
                game_with(server_arena, 8, conditions(latency, jitter, loss));
            if !crowd {
                spread_out(&mut server, &mut players);
            }
            for player in &mut players {
                player.network.reset_prediction();
            }
            let started = std::time::Instant::now();
            walk_about(&mut server, &mut players, 8);
            let seconds = started.elapsed().as_secs_f64();
            let how = if crowd { "crowd" } else { "apart" };
            for (n, player) in players.iter().enumerate() {
                let me = player.scripts.local_player().unwrap();
                let stats = player.network.prediction();
                let off = gap(
                    player.character_at(me).unwrap(),
                    server.character_at(me).unwrap(),
                );
                println!(
                    "{latency} ms ±{jitter}, {loss} loss, {how}, player {n}: {} moves, {} corrections \
                     (largest {:.3}, mean {:.3} studs, {} snapped), {} replayed in {:.3} ms \
                     (longest frame {:.3} ms), ends {off:.4} studs from the server",
                    stats.moves,
                    stats.corrections,
                    stats.largest,
                    stats.total / stats.corrections.max(1) as f64,
                    stats.snaps,
                    stats.replayed,
                    stats.replay_seconds * 1000.0,
                    stats.longest_replay * 1000.0,
                );
            }
            println!("{latency} ms, {how}: 9 devices' frames took {seconds:.2} s");
        }
    }
}
