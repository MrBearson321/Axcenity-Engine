//! Multiplayer readiness MP2: the real `axcenity-server` through what
//! goes wrong on a public server, with `axcenity-bots` as its players, as
//! separate processes over UDP. The server crashes and starts again (the
//! players rejoin, their saves kept); players come and go for a while (no
//! leak); connections misbehave (dropped, and real players still get in).

mod common;

use std::time::{Duration, Instant};

use common::{ServerRun, bots, copy_folder, example, finish, json_lines, scratch, server_with};

/// Each player's visits, kept in their saved data on the server.
const VISITS: &str = r#"local Players = game:GetService("Players")
local SaveService = game:GetService("SaveService")
Players.PlayerAdded:Connect(function(player)
    local visits = (SaveService:GetPlayerData(player, "Visits") or 0) + 1
    SaveService:SetPlayerData(player, "Visits", visits)
    print(player.Name .. " visit " .. visits)
end)
"#;

#[test]
fn bots_rejoin_a_server_that_crashed_and_started_again_with_their_saves() {
    let folder = scratch("crash");
    let game = folder.join("game");
    copy_folder(std::path::Path::new(&example("hello-spawn")), &game);
    std::fs::write(game.join("scripts/visits.server.lua"), VISITS).unwrap();
    let game = game.to_str().unwrap().to_owned();
    let saves = folder.join("saves");
    let envs = [("AXCENITY_SAVE_DIR", saves.as_path())];

    let first = server_with(&game, 0, &[], &envs);
    let port = first.port;
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        port,
        &report,
        &[
            "--count",
            "3",
            "--seconds",
            "50",
            "--idle",
            "--user-id",
            "9100",
        ],
    );
    first.wait_for_all(&["Bot1 visit 1", "Bot2 visit 1", "Bot3 visit 1"], 30);
    // Player data is written at most once a second while it changes.
    std::thread::sleep(Duration::from_secs(2));
    // A crash: no goodbye to anyone, nothing saved on the way out.
    drop(first);
    std::thread::sleep(Duration::from_secs(2));
    let again: ServerRun = server_with(&game, port, &[], &envs);
    // The bots notice the lost connection when netcode's 15 seconds pass,
    // then try again at 1, 2, 4 and 8 seconds.
    again.wait_for_all(&["Bot1 visit 2", "Bot2 visit 2", "Bot3 visit 2"], 45);
    let rejoined = Instant::now();
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    assert_eq!(summary["rejoins"], 3, "{summary}");
    assert_eq!(summary["rejoining"], 0, "{summary}");
    assert_eq!(summary["connected"], 3, "{summary}");
    assert_eq!(summary["corrections"], 0, "{summary}");
    assert_eq!(
        summary["ended"].as_object().map(|e| e.len()),
        Some(0),
        "{summary}"
    );
    assert!(rejoined.elapsed() < Duration::from_secs(60));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn bots_that_keep_coming_and_going_leave_nothing_behind_on_the_server() {
    let folder = scratch("churn");
    let game = example("hello-spawn");
    let stats = folder.join("server.jsonl");
    let server = server_with(
        &game,
        0,
        &["--stats", "1", "--stats-file", stats.to_str().unwrap()],
        &[],
    );
    // The server's numbers with no one in.
    let start = Instant::now();
    let baseline = loop {
        if let Some(first) = json_lines(&stats).first() {
            break first.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(100));
    };
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        server.port,
        &report,
        &["--count", "4", "--seconds", "16", "--churn", "3"],
    );
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    // Every place had a bot leave and another take it, several times.
    assert!(summary["churned"].as_u64().unwrap() >= 12, "{summary}");
    assert_eq!(summary["corrections"], 0, "{summary}");
    // Once they've all gone, the server is as it was.
    let seen = json_lines(&stats).len();
    let start = Instant::now();
    let end = loop {
        let lines = json_lines(&stats);
        let last = lines.last().cloned().unwrap();
        if lines.len() > seen + 1 && last["players"] == 0 {
            break last;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "{last}");
        std::thread::sleep(Duration::from_millis(200));
    };
    let peak = json_lines(&stats)
        .iter()
        .filter_map(|line| line["players"].as_u64())
        .max();
    assert!(peak >= Some(4), "{peak:?}");
    assert_eq!(end["instances"], baseline["instances"], "{baseline} {end}");
    let lua = |line: &serde_json::Value| line["lua_bytes"].as_f64().unwrap();
    assert!(
        lua(&end) < lua(&baseline) * 1.5 + 512.0 * 1024.0,
        "Lua grew: {baseline} {end}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn connections_that_never_say_who_they_are_are_dropped_and_players_get_in_after() {
    let folder = scratch("silent");
    let game = example("hello-spawn");
    // Two players and netcode's four spare connections: the flood takes
    // every one of them.
    let server = server_with(&game, 0, &["--max-players", "2"], &[]);
    let flood = folder.join("flood.jsonl");
    let running = bots(
        &game,
        server.port,
        &flood,
        &[
            "--flood",
            "silent",
            "--count",
            "6",
            "--seconds",
            "14",
            "--stagger",
            "10",
        ],
    );
    let (ok, summary) = finish(running, &flood);
    assert!(ok, "the server kept some: {summary}");
    let after = summary["ended_after_max_s"].as_f64().unwrap();
    // Dropped when the handshake's ten seconds pass.
    assert!((9.0..13.0).contains(&after), "{summary}");
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        server.port,
        &report,
        &["--count", "2", "--seconds", "3", "--idle"],
    );
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    assert_eq!(summary["joined"], 2, "{summary}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn junk_is_refused_and_spammers_are_kicked_while_players_play_on() {
    let folder = scratch("spam");
    let game = example("hello-spawn");
    let stats = folder.join("server.jsonl");
    let server = server_with(
        &game,
        0,
        &[
            "--max-players",
            "8",
            "--stats",
            "1",
            "--stats-file",
            stats.to_str().unwrap(),
        ],
        &[],
    );
    // Junk instead of a join request: refused and dropped.
    let junk = folder.join("junk.jsonl");
    let running = bots(
        &game,
        server.port,
        &junk,
        &["--flood", "junk", "--count", "3", "--seconds", "5"],
    );
    let (ok, summary) = finish(running, &junk);
    assert!(ok, "the server kept some: {summary}");
    server.wait_for(
        "refused a player whose first message wasn't a join request",
        5,
    );

    // Remote messages with tables nested 200,000 deep, which overflowed
    // the server's stack before MP3: unreadable, flagged, kicked.
    let deep = folder.join("deep.jsonl");
    let running = bots(
        &game,
        server.port,
        &deep,
        &["--flood", "deep", "--count", "1", "--seconds", "10"],
    );
    let (_, summary) = finish(running, &deep);
    assert_eq!(summary["joined"], 1, "{summary}");
    assert_eq!(
        summary["ended"]["The server closed the connection."], 1,
        "{summary}"
    );
    server.wait_for("Kicked Spam1", 5);

    // Two players walking about, then two connections that join and
    // send all the junk they can (MP3): the server drops what's over each
    // budget, flags them and kicks them, and the players don't notice.
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        server.port,
        &report,
        &["--count", "2", "--seconds", "10", "--user-id", "9300"],
    );
    std::thread::sleep(Duration::from_secs(3));
    let spam = folder.join("spam.jsonl");
    let spamming = bots(
        &game,
        server.port,
        &spam,
        &["--flood", "spam", "--count", "2", "--seconds", "12"],
    );
    let said = server.wait_for_all(&["Kicked Spam1", "Kicked Spam2"], 15);
    assert!(
        said.iter()
            .any(|line| line.contains("was flagged: their device")),
        "{said:?}"
    );
    let (_, spammed) = finish(spamming, &spam);
    assert_eq!(spammed["joined"], 2, "{spammed}");
    assert_eq!(
        spammed["ended"]["The server closed the connection."], 2,
        "{spammed}"
    );
    // Kicked within a few seconds of joining.
    assert!(
        spammed["ended_after_max_s"].as_f64().unwrap() < 8.0,
        "{spammed}"
    );
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    assert_eq!(summary["connected"], 2, "{summary}");
    // In the second or three before the spammers are kicked, their junk
    // fills the server's socket between frames and the players lose some
    // packets (renet's loss figure, a slow average, shows it for a while
    // after), so the odd correction can follow.
    assert!(summary["corrections"].as_u64().unwrap() <= 3, "{summary}");
    let lines = json_lines(&stats);
    assert!(
        lines.last().unwrap()["flags"].as_u64().unwrap() >= 2,
        "{:?}",
        lines.last()
    );
    for line in &lines[1..] {
        assert!(line["fps"].as_f64().unwrap() > 50.0, "{line}");
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// MP3b: modified clients that move their characters too fast, through
/// the floor or into the air are held back, flagged and kicked, each for
/// what it did, while two honest players walk and jump about untouched.
#[test]
fn cheating_clients_are_held_back_and_kicked_while_players_play_on() {
    let folder = scratch("cheats");
    let game = example("hello-spawn");
    let server = server_with(&game, 0, &["--max-players", "8"], &[]);
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        server.port,
        &report,
        &["--count", "2", "--seconds", "25", "--user-id", "9400"],
    );
    std::thread::sleep(Duration::from_secs(3));
    let cheats = [
        ("speed", "moved its character faster than it can go"),
        ("noclip", "moved its character through a wall"),
        ("fly", "kept its character in the air"),
    ];
    let cheating: Vec<_> = cheats
        .iter()
        .enumerate()
        .map(|(n, (cheat, _))| {
            let path = folder.join(format!("{cheat}.jsonl"));
            let user_id = (9500 + n).to_string();
            let running = bots(
                &game,
                server.port,
                &path,
                &[
                    "--cheat",
                    cheat,
                    "--count",
                    "1",
                    "--seconds",
                    "20",
                    "--user-id",
                    &user_id,
                ],
            );
            (running, path)
        })
        .collect();
    for ((running, path), (cheat, reason)) in cheating.into_iter().zip(cheats) {
        let (_, summary) = finish(running, &path);
        assert_eq!(summary["joined"], 1, "{cheat}: {summary}");
        let kicked = format!("Your device {reason}.");
        assert_eq!(summary["ended"][&kicked], 1, "{cheat}: {summary}");
        assert!(
            summary["corrections"].as_u64().unwrap() > 0,
            "{cheat}: {summary}"
        );
    }
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    assert_eq!(summary["connected"], 2, "{summary}");
    assert_eq!(summary["corrections"], 0, "{summary}");
    let _ = std::fs::remove_dir_all(&folder);
}
