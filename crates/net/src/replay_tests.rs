//! Phase 81: replays of a multiplayer game, and a kill cam, with a
//! dedicated server and players' devices in this process.

use std::collections::BTreeMap;

use axcenity_core::{Vector3, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;
use crate::replication_tests::{Device, castle, frames, info, player, server};

/// The match: crates that slide and spin, a pillar that's made and
/// destroyed every few seconds and changes colour, and a scoreboard
/// value.
const MATCH: &str = r#"
local crates = {}
for i = 1, 6 do
    local crate = Instance.new("Part")
    crate.Name = "Crate" .. i
    crate.Anchored = true
    crate.Position = Vector3.new(i * 6, 2, 0)
    crate.Parent = workspace
    crates[i] = crate
end
local score = Instance.new("IntValue")
score.Name = "Score"
score.Parent = game.ReplicatedStorage
local t = 0
local pillar
game:GetService("RunService").Stepped:Connect(function() end)
task.spawn(function()
    while true do
        t = t + 1 / 60
        for i, crate in crates do
            crate.Position = Vector3.new(i * 6 + math.sin(t * i) * 4, 2 + i * 0.1 * t % 3, math.cos(t) * i)
            crate.Orientation = Vector3.new(0, (t * 40 * i) % 360 - 180, 0)
        end
        if math.floor(t * 60) % 240 == 0 then
            if pillar then
                pillar:Destroy()
                pillar = nil
            else
                pillar = Instance.new("Part")
                pillar.Name = "Pillar"
                pillar.Anchored = true
                pillar.Size = Vector3.new(1, 8, 1)
                pillar.Position = Vector3.new(0, 4, t % 10)
                pillar.Parent = workspace
            end
            score.Value = score.Value + 1
        end
        if pillar then
            pillar.Color = Color3.new((t % 1), 0.5, 0.2)
        end
        task.wait()
    end
end)
"#;

/// The replicated Instances of `world` as replication shows them: the
/// Workspace and ReplicatedStorage, with every property but velocities
/// (replays keep where parts are, not how fast they go).
fn shown(world: &World) -> BTreeMap<u64, String> {
    let mut seen = BTreeMap::new();
    for service in [world.workspace(), world.replicated_storage()] {
        for id in world.descendants(service) {
            let instance = world.get(id).unwrap();
            let mut properties: Vec<String> = instance
                .properties()
                .filter(|(name, _)| !name.starts_with("Assembly"))
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
    }
    seen
}

#[test]
fn a_two_minute_match_plays_back_as_it_was_and_the_same_each_time() {
    let (mut server, first) = server();
    let mut players = vec![player(first, 101.0)];
    frames(&mut server, &mut players, 5);
    let link = server.network.connect_loopback().unwrap();
    players.push(player(link, 102.0));
    frames(&mut server, &mut players, 5);
    server
        .scripts
        .run_command(&mut server.world, MATCH)
        .unwrap();
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"game:GetService("ReplayService"):StartRecording()"#,
        )
        .unwrap();
    // Two minutes at 60 frames a second, noting how the server's world
    // looked at a few moments (counted from the recording's first frame).
    let moments = [1usize, 600, 3601, 5999, 7200];
    let mut expected = Vec::new();
    for frame in 1..=7200 {
        frames(&mut server, &mut players, 1);
        if moments.contains(&frame) {
            expected.push((frame, shown(&server.world)));
        }
    }
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"game:GetService("ReplayService"):StopRecording("match")"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    let mut replays = server.network.take_replays();
    assert_eq!(replays.len(), 1);
    let (name, replay) = replays.pop().unwrap();
    assert_eq!(name, "match");
    // Saved and read back.
    let bytes = replay.to_bytes();
    let replay = Replay::from_bytes(&bytes).unwrap();
    let (start, end) = replay.span();
    assert!(
        (end - start - 7199.0 * replication_tests::SECONDS).abs() < 1e-6,
        "{start}..{end}"
    );
    let keyframes = replay
        .frames
        .iter()
        .filter(|f| f.snapshot.is_some())
        .count();
    assert_eq!(keyframes, 12, "one every 10 seconds");

    let time_of = |frame: usize| start + (frame - 1) as f64 * replication_tests::SECONDS;
    // Played straight through, each moment is the server's world then.
    let mut straight = Playback::new(replay.clone(), &server.world).unwrap();
    for (frame, world) in &expected {
        straight.seek(time_of(*frame));
        assert_eq!(
            &shown(straight.world()),
            world,
            "frame {frame}, played through"
        );
    }
    assert!(straight.at_end());
    // Jumping about (back to keyframes, and forward past them) shows the
    // same, every time.
    let mut jumping = Playback::new(replay, &server.world).unwrap();
    for (frame, world) in expected.iter().rev().chain(&expected) {
        jumping.seek(time_of(*frame));
        assert_eq!(&shown(jumping.world()), world, "frame {frame}, jumped to");
    }
    // Played at double speed from the start, in steps.
    jumping.seek(start);
    jumping.speed = 2.0;
    while jumping.time() < time_of(3601) - 1e-9 {
        jumping.step(replication_tests::SECONDS / 2.0);
    }
    assert_eq!(&shown(jumping.world()), &expected[2].1);
}

/// A shooter's character: a Head that walks along X turning as it goes.
const ARENA: &str = r#"
game.Players.PlayerAdded:Connect(function(player)
    local character = Instance.new("Model")
    character.Name = player.Name
    local head = Instance.new("Part")
    head.Name = "Head"
    head.Anchored = true
    head.Position = Vector3.new(0, 5, 0)
    head.Parent = character
    character.Parent = workspace
    player.Character = character
    local t = 0
    task.spawn(function()
        while head.Parent do
            t = t + 1 / 60
            head.Position = Vector3.new(t * 2, 5, 0)
            head.Orientation = Vector3.new(0, (t * 9) % 360 - 180, 0)
            task.wait()
        end
    end)
end)
"#;

/// The victim keeps five seconds, and after ten shows the kill cam from
/// the first player's (the killer's) eyes.
const VICTIM: &str = r#"
local ReplayService = game:GetService("ReplayService")
local me = game.Players.LocalPlayer
if me.UserId == 102 then
    ReplayService.BufferSeconds = 5
    task.wait(10)
    local killer
    for _, player in game.Players:GetPlayers() do
        if player.UserId == 101 then
            killer = player
        end
    end
    ReplayService:PlayBuffer(killer)
    print("kill cam", killer.Name)
end
"#;

#[test]
fn a_kill_cam_shows_the_last_five_seconds_from_the_killers_eyes() {
    let arena = castle();
    let mut world = arena.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(
        &mut world,
        arena.server_script,
        "scripts/main.server.lua",
        ARENA,
    );
    let mut network = Network::new(info()).with_discovery_port(47936);
    let first = network.host_in_memory(&mut world, &mut scripts, false);
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    let join = |link, user_id| {
        let castle = castle();
        let mut world = castle.world;
        let mut scripts = ScriptRuntime::new();
        scripts.set_user_id(user_id);
        let mut network = Network::new(info());
        network.set_client_scripts(vec![ScriptCode {
            instance: castle.client_script,
            path: "scripts/hud.client.lua".to_owned(),
            source: VICTIM.to_owned(),
        }]);
        network.join_through(&mut world, &mut scripts, link, None);
        Device {
            world,
            scripts,
            network,
            output: Vec::new(),
        }
    };
    let mut players = vec![join(first, 101.0)];
    frames(&mut server, &mut players, 5);
    let link = server.network.connect_loopback().unwrap();
    players.push(join(link, 102.0));

    // The killer's eyes as the victim's device showed them, each frame.
    let mut seen: Vec<(f64, (Vector3, Vector3))> = Vec::new();
    let mut clock = 0.0;
    let mut play = None;
    for _ in 0..1200 {
        frames(&mut server, &mut players, 1);
        clock += replication_tests::SECONDS;
        let victim = &mut players[1];
        let killer = victim.scripts.local_player().and_then(|me| {
            let players = victim.world.players();
            victim
                .world
                .get(players)?
                .children()
                .iter()
                .copied()
                .find(|&p| p != me)
        });
        if let Some(eyes) = killer.and_then(|k| eyes_of(&victim.world, k)) {
            seen.push((clock, eyes));
        }
        if let Some(found) = victim.network.take_buffer_play() {
            play = Some((clock, found));
            break;
        }
    }
    let victim = &players[1];
    assert!(
        victim.said("kill cam Player"),
        "{:?}",
        &victim.output[victim.output.len().saturating_sub(3)..]
    );
    let (asked_at, play) = play.expect("the kill cam was asked for");
    let killer = play.follow.expect("it follows the killer");
    assert_eq!(victim.world.get(killer).unwrap().name(), "Player");

    let (first, end) = play.replay.span();
    assert!(first <= play.start + 1e-9);
    assert!(
        (end - play.start - 5.0).abs() < 1e-6,
        "five seconds: {} to {end}",
        play.start
    );
    let mut playback = Playback::new(play.replay, &victim.world).unwrap();
    playback.seek(play.start);
    // The buffer's clock started when BufferSeconds was set; line it up
    // with the frames noted here by its end, the frame it was asked in.
    let offset = asked_at - end;
    let mut checked = 0;
    while !playback.at_end() {
        let shown = eyes_of(playback.world(), killer).expect("the killer is in the kill cam");
        let at = playback.time() + offset;
        let (_, then) = seen
            .iter()
            .find(|(time, _)| (time - at).abs() < 1e-6)
            .unwrap_or_else(|| panic!("a frame at {at}"));
        assert_eq!(shown, *then, "the killer's view at {at}");
        checked += 1;
        playback.step(replication_tests::SECONDS);
    }
    assert!(checked >= 299, "five seconds of frames: {checked}");
    // The head walked 2 studs a second: the kill cam's view moves with it.
    playback.seek(play.start);
    let from = eyes_of(playback.world(), killer).unwrap().0;
    playback.seek(end);
    let to = eyes_of(playback.world(), killer).unwrap().0;
    assert!((to.x - from.x - 10.0).abs() < 0.2, "{from:?} to {to:?}");
}

#[test]
fn a_replay_from_another_format_or_protocol_is_refused_with_why() {
    let replay = Replay {
        game: "Castle".to_owned(),
        game_version: "1.0".to_owned(),
        protocol: PROTOCOL_VERSION + 1,
        frames: vec![ReplayFrame {
            snapshot: Some(Update::default()),
            ..ReplayFrame::default()
        }],
    };
    let err = Replay::from_bytes(&replay.to_bytes()).unwrap_err();
    assert!(err.contains("network protocol"), "{err}");
    assert!(
        Replay::from_bytes(b"hello")
            .unwrap_err()
            .contains("isn't an Axcenity replay")
    );
    let mut bytes = replay.to_bytes();
    bytes[8] = 99;
    assert!(
        Replay::from_bytes(&bytes)
            .unwrap_err()
            .contains("replay format 99")
    );
}

#[test]
fn replay_service_checks_what_scripts_give_it() {
    let mut scripts = ScriptRuntime::new();
    let mut world = World::new();
    let errors = [
        (
            r#"game:GetService("ReplayService"):StopRecording("x")"#,
            "nothing is being recorded",
        ),
        (
            r#"game:GetService("ReplayService"):PlayBuffer()"#,
            "there's no buffer",
        ),
        (
            r#"game:GetService("ReplayService").BufferSeconds = 90"#,
            "from 0 (off) to 60",
        ),
        (
            r#"local r = game:GetService("ReplayService") r:StartRecording() r:StopRecording("../x")"#,
            "must be 1 to 64 letters",
        ),
        (
            r#"local r = game:GetService("ReplayService") r.BufferSeconds = 3 r:PlayBuffer(workspace)"#,
            "a Player or nil",
        ),
    ];
    let mut errors_of = |scripts: &mut ScriptRuntime, source: &str| {
        scripts.run_command(&mut world, source).unwrap();
        scripts
            .take_output()
            .into_iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text)
            .collect::<Vec<_>>()
    };
    for (source, want) in errors {
        let errors = errors_of(&mut scripts, source);
        assert!(
            errors.iter().any(|e| e.contains(want)),
            "{source}: {errors:?}"
        );
    }
    let errors = errors_of(
        &mut scripts,
        r#"local r = game:GetService("ReplayService")
-- Still going: the bad name above stopped nothing.
assert(r.IsRecording)
r:StopRecording("first try")
assert(not r.IsRecording)
r:StartRecording()
assert(r.IsRecording and r.BufferSeconds == 3)"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}
