//! Multiplayer readiness MP1: the real `axcenity-server` with `--stats`,
//! and `axcenity-bots` joining it over UDP as separate processes.

mod common;

use std::path::PathBuf;
use std::process::{Command, Stdio};

use common::{Running, example, json_lines, scratch, server};

#[test]
fn twelve_bots_in_two_processes_join_over_udp_walk_about_and_leave() {
    let folder = scratch("join");
    let game = example("hello-spawn");
    let stats = folder.join("server.jsonl");
    let (_server, port, _lines) = server(
        &game,
        &[
            "--max-players",
            "12",
            "--stats",
            "1",
            "--stats-file",
            stats.to_str().unwrap(),
        ],
    );
    let address = format!("127.0.0.1:{port}");
    let bots: Vec<(Running, PathBuf)> = (0..2)
        .map(|n| {
            let report = folder.join(format!("bots-{n}.jsonl"));
            let child = Command::new(env!("CARGO_BIN_EXE_axcenity-bots"))
                .args([
                    &game,
                    "--join",
                    &address,
                    "--count",
                    "6",
                    "--seconds",
                    "10",
                    "--report",
                    "1",
                    "--stagger",
                    "50",
                    "--threads",
                    "2",
                    "--user-id",
                    &format!("{}", 7000 + n * 100),
                    "--report-file",
                    report.to_str().unwrap(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("the bots start");
            (Running(child), report)
        })
        .collect();
    for (mut bot, report) in bots {
        let status = bot.0.wait().unwrap();
        let lines = json_lines(&report);
        assert!(status.success(), "the bots failed: {lines:?}");
        let summary = lines
            .iter()
            .rev()
            .find(|line| line["summary"] == true)
            .unwrap_or_else(|| panic!("no summary: {lines:?}"));
        assert_eq!(summary["joined"], 6, "{summary}");
        assert_eq!(summary["connected"], 6, "{summary}");
        assert_eq!(
            summary["ended"].as_object().map(|e| e.len()),
            Some(0),
            "{summary}"
        );
        assert!(
            summary["received_bytes_per_s"].as_f64().unwrap() > 0.0,
            "{summary}"
        );
    }
    // The server saw all twelve at once.
    let peak = json_lines(&stats)
        .iter()
        .filter_map(|line| line["players"].as_u64())
        .max();
    assert_eq!(peak, Some(12));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn bots_past_max_players_are_refused_with_the_reason() {
    let folder = scratch("full");
    let game = example("hello-spawn");
    let (_server, port, _lines) = server(&game, &["--max-players", "2"]);
    let report = folder.join("bots.jsonl");
    let status = Command::new(env!("CARGO_BIN_EXE_axcenity-bots"))
        .args([
            &game,
            "--join",
            &format!("127.0.0.1:{port}"),
            "--count",
            "3",
            "--seconds",
            "4",
            "--report",
            "1",
            "--idle",
            "--report-file",
            report.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    // One bot never joined, so the run fails.
    assert!(!status.success());
    let lines = json_lines(&report);
    let summary = lines
        .iter()
        .rev()
        .find(|line| line["summary"] == true)
        .unwrap();
    assert_eq!(summary["joined"], 2, "{summary}");
    let ended = summary["ended"].as_object().unwrap();
    assert!(
        ended.keys().any(|reason| reason.contains("full")),
        "{summary}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
