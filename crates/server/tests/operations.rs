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
fn junk_and_spam_dont_keep_players_out_or_slow_the_server() {
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

    // Two connections that join, then send all the junk they can.
    let spam = folder.join("spam.jsonl");
    let spamming = bots(
        &game,
        server.port,
        &spam,
        &["--flood", "spam", "--count", "2", "--seconds", "12"],
    );
    std::thread::sleep(Duration::from_secs(2));
    let report = folder.join("bots.jsonl");
    let running = bots(
        &game,
        server.port,
        &report,
        &["--count", "2", "--seconds", "6", "--user-id", "9300"],
    );
    let (ok, summary) = finish(running, &report);
    assert!(ok, "{summary}");
    assert_eq!(summary["joined"], 2, "{summary}");
    assert_eq!(summary["connected"], 2, "{summary}");
    // Not yet: no corrections and no loss. The junk fills the server's
    // socket between frames, so about a fifth of the players' packets are
    // lost (and the odd correction follows) until MP3's budgets drop a
    // connection that sends this much.
    assert!(summary["join_max_s"].as_f64().unwrap() < 2.0, "{summary}");
    let (_, spammed) = finish(spamming, &spam);
    assert_eq!(spammed["joined"], 2, "{spammed}");
    // The server took the junk in and kept its frame rate.
    let lines = json_lines(&stats);
    let busiest = lines
        .iter()
        .max_by(|a, b| {
            let received = |l: &serde_json::Value| l["received_bytes_per_s"].as_f64().unwrap();
            received(a).total_cmp(&received(b))
        })
        .unwrap();
    assert!(
        busiest["received_bytes_per_s"].as_f64().unwrap() > 500_000.0,
        "{busiest}"
    );
    for line in &lines[1..] {
        assert!(line["fps"].as_f64().unwrap() > 50.0, "{line}");
    }
    let _ = std::fs::remove_dir_all(&folder);
}
