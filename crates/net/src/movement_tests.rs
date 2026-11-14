//! Multiplayer readiness MP3b: the server's movement checks against
//! modified clients that walk through walls, fly or fling their parts,
//! and honest players who walk into walls, jump and stand on edges.

use std::collections::BTreeSet;
use std::time::Duration;

use axcenity_assets::Assets;
use axcenity_character::MoveInput;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;
use crate::character_tests::{Device, forward, frames, game_in, position, spawn_arena};
use crate::motion::{Motion, PartState};
use crate::replication_tests::SECONDS;

/// The spawn arena with a wall a stud thick across the way characters
/// face (-Z), 10 studs ahead.
fn walled() -> World {
    let mut world = spawn_arena();
    part(
        &mut world,
        "Wall",
        Vector3::new(0.0, 10.0, -10.0),
        Vector3::new(40.0, 20.0, 1.0),
        true,
    );
    world
}

fn part(world: &mut World, name: &str, at: Vector3, size: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Position", Value::Vector3(at)),
        ("Size", Value::Vector3(size)),
        ("Anchored", Value::Bool(anchored)),
    ] {
        world.set_property(id, name, value).unwrap();
    }
    id
}

/// The modified client: its controller gives up its character (a custom
/// `Movement` on its device only), and each frame `step` says where each
/// of the character's parts goes next.
fn cheat(
    server: &mut Device,
    players: &mut [Device],
    count: usize,
    step: impl Fn(Vector3) -> Vector3,
) {
    let me = players[0].scripts.local_player().unwrap();
    let character = players[0].character(me).unwrap();
    let world = &mut players[0].world;
    let controller = world
        .get(character)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|&id| world.get(id).unwrap().class().is_character_controller())
        .unwrap();
    world
        .set_property(controller, "Movement", Value::String("Cheat".into()))
        .unwrap();
    let parts: Vec<InstanceId> = world
        .descendants(character)
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .collect();
    for _ in 0..count {
        // Where the first part goes, and the rest with it.
        let first = position(&players[0].world, parts[0]).unwrap();
        let next = step(first);
        let (dx, dy, dz) = (next.x - first.x, next.y - first.y, next.z - first.z);
        for &part in &parts {
            let world = &mut players[0].world;
            let p = position(world, part).unwrap();
            let moved = Vector3::new(p.x + dx, p.y + dy, p.z + dz);
            world
                .set_property(part, "Position", Value::Vector3(moved))
                .unwrap();
        }
        frames(server, players, 1, &[]);
    }
}

fn flagged(server: &Device, reason: &str) -> bool {
    server
        .log
        .iter()
        .any(|line| line.contains("was flagged") && line.contains(reason))
}

#[test]
fn a_modified_client_that_walks_through_a_wall_is_held_on_its_side_and_flagged() {
    let (mut server, mut players) = game_in(walled, 1, Duration::ZERO);
    let me = players[0].scripts.local_player().unwrap();
    // At WalkSpeed, so only the wall is wrong.
    let step = (16.0 * SECONDS) as f32;
    cheat(&mut server, &mut players, 240, |at| {
        Vector3::new(at.x, at.y, at.z - step)
    });
    let server_at = server.character_at(me).unwrap();
    // The wall's face is at z = -9.5: a quarter stud in is allowed.
    assert!(
        server_at.z > -9.5 - 0.3,
        "the server never has it past the wall: {server_at:?}"
    );
    let device_at = players[0].character_at(me).unwrap();
    assert!(device_at.z > -10.5, "it was moved back: {device_at:?}");
    assert!(players[0].network.corrections() > 0);
    assert!(
        flagged(&server, crate::movement::THROUGH_WALL),
        "{:?}",
        server.log
    );
}

#[test]
fn a_modified_client_that_flies_is_held_to_a_jumps_height_and_flagged() {
    let (mut server, mut players) = game_in(spawn_arena, 1, Duration::ZERO);
    let me = players[0].scripts.local_player().unwrap();
    let start = server.character_at(me).unwrap();
    // Up at 24 studs a second (slower than a jump's launch) to 30 studs
    // over the ground, then hovering there.
    let top = start.y + 30.0;
    let step = (24.0 * SECONDS) as f32;
    let mut highest = start.y;
    for _ in 0..240 {
        cheat(&mut server, &mut players, 1, |at| {
            Vector3::new(at.x, (at.y + step).min(top), at.z)
        });
        highest = highest.max(server.character_at(me).unwrap().y);
    }
    // JumpHeight 7.2, with 15% and 2 studs to spare, from as high as it
    // still stood (its feet within 1.5 studs of the ground).
    assert!(
        highest < start.y + 1.5 + 7.2 * 1.15 + 2.5,
        "the server let it rise {} studs",
        highest - start.y
    );
    assert!(players[0].network.corrections() > 0);
    assert!(
        flagged(&server, crate::movement::IN_THE_AIR),
        "{:?}",
        server.log
    );
}

#[test]
fn an_honest_player_who_walks_into_a_wall_and_jumps_along_it_is_never_moved_back() {
    let (mut server, mut players) = game_in(walled, 1, Duration::from_millis(80));
    let me = players[0].scripts.local_player().unwrap();
    frames(&mut server, &mut players, 120, &[forward()]);
    let jump_forward = MoveInput {
        jump: true,
        ..forward()
    };
    frames(&mut server, &mut players, 240, &[jump_forward]);
    // Along the wall, jumping, then off the spawn's edge.
    let along = MoveInput {
        left: true,
        jump: true,
        ..MoveInput::default()
    };
    frames(&mut server, &mut players, 240, &[along]);
    let at = server.character_at(me).unwrap();
    assert!(at.z > -9.5, "{at:?}");
    assert_eq!(players[0].network.corrections(), 0, "{:?}", players[0].log);
    assert_eq!(server.network.flag_count(), 0, "{:?}", server.log);
}

#[test]
fn an_owned_part_may_go_as_fast_as_physics_takes_it_but_not_faster() {
    let mut world = World::new();
    let car = part(
        &mut world,
        "Car",
        Vector3::new(0.0, 2.0, 0.0),
        Vector3::new(4.0, 1.0, 8.0),
        false,
    );
    let assemblies = BTreeSet::from([car]);
    let assets = Assets::default();
    let sender = Sender {
        player: world.players(),
        assemblies: &assemblies,
        characters: &[],
    };
    let mut check = CharacterCheck::default();
    let send = |world: &mut World, check: &mut CharacterCheck, x: f32, now: f64| {
        let mut moved = world.clone();
        moved
            .set_property(car, "Position", Value::Vector3(Vector3::new(x, 2.0, 0.0)))
            .unwrap();
        let owned = Owned {
            motion: Motion {
                time: now,
                parts: vec![(car.to_bits(), PartState::of(&moved, car).unwrap())],
            },
            ..Owned::default()
        };
        check.receive(world, &assets, &sender, &owned, now);
        f64::from(position(world, car).unwrap().x)
    };
    // 300 studs a second: a fast car.
    assert_eq!(send(&mut world, &mut check, 5.0, 1.0 / 60.0), 5.0);
    assert_eq!(send(&mut world, &mut check, 10.0, 2.0 / 60.0), 10.0);
    // 1,200: not by physics.
    assert_eq!(send(&mut world, &mut check, 30.0, 3.0 / 60.0), 10.0);
    // Falling as far is.
    let mut fell = world.clone();
    fell.set_property(
        car,
        "Position",
        Value::Vector3(Vector3::new(10.0, -18.0, 0.0)),
    )
    .unwrap();
    let owned = Owned {
        motion: Motion {
            time: 0.0,
            parts: vec![(car.to_bits(), PartState::of(&fell, car).unwrap())],
        },
        ..Owned::default()
    };
    check.receive(&mut world, &assets, &sender, &owned, 4.0 / 60.0);
    assert_eq!(position(&world, car).unwrap().y, -18.0);
    // Flinging it for long enough flags the player.
    let flags = (5..200)
        .filter(|&n| {
            let x = 10.0 + 30.0 * n as f32;
            let mut moved = world.clone();
            moved
                .set_property(car, "Position", Value::Vector3(Vector3::new(x, -18.0, 0.0)))
                .unwrap();
            let owned = Owned {
                motion: Motion {
                    time: 0.0,
                    parts: vec![(car.to_bits(), PartState::of(&moved, car).unwrap())],
                },
                ..Owned::default()
            };
            check
                .receive(&mut world, &assets, &sender, &owned, n as f64 / 60.0)
                .is_some()
        })
        .count();
    assert!(flags >= 1, "{flags}");
}
