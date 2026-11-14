//! Multiplayer readiness MP1: the real `axcenity-server` with `--stats`,
//! and `axcenity-bots` joining it over UDP as separate processes.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Kills a process when the test ends, passing or not.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn example(name: &str) -> String {
    format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-bots-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Starts the server on a free port; returns it, its port, and its lines.
fn server(game: &str, arguments: &[&str]) -> (Running, u16, mpsc::Receiver<String>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_axcenity-server"));
    command
        .arg(game)
        .args(["--port", "0"])
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Running(command.spawn().expect("the server starts"));
    let (lines, heard) = mpsc::channel();
    for stream in [
        Box::new(child.0.stderr.take().unwrap()) as Box<dyn std::io::Read + Send>,
        Box::new(child.0.stdout.take().unwrap()),
    ] {
        let lines = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = lines.send(line);
            }
        });
    }
    let start = Instant::now();
    let mut said = Vec::new();
    let port = loop {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the server never said its port: {said:?}"
        );
        if let Ok(line) = heard.recv_timeout(Duration::from_millis(50)) {
            if let Some(port) = line
                .split("on UDP port ")
                .nth(1)
                .and_then(|rest| rest.split(',').next())
                .and_then(|port| port.parse().ok())
            {
                break port;
            }
            said.push(line);
        }
    };
    (child, port, heard)
}

fn json_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

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
