//! What the tests that run the real `axcenity-server` and `axcenity-bots`
//! share (multiplayer readiness MP1 and MP2).

#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Kills a process when the test ends, passing or not.
pub struct Running(pub Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn example(name: &str) -> String {
    format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

pub fn scratch(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-bots-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Copies the folder `from` to `to`, files and folders inside.
pub fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A running server, and every line it prints.
pub struct ServerRun {
    pub process: Running,
    pub port: u16,
    pub lines: mpsc::Receiver<String>,
}

impl ServerRun {
    /// Waits up to `seconds` for a line containing `text`; the lines read
    /// on the way are returned with it, or in the panic.
    pub fn wait_for(&self, text: &str, seconds: u64) -> Vec<String> {
        self.wait_for_all(&[text], seconds)
    }

    /// Waits up to `seconds` for lines containing each of `texts`, in any
    /// order.
    pub fn wait_for_all(&self, texts: &[&str], seconds: u64) -> Vec<String> {
        let start = Instant::now();
        let mut said: Vec<String> = Vec::new();
        loop {
            let missing: Vec<&&str> = texts
                .iter()
                .filter(|text| !said.iter().any(|line| line.contains(**text)))
                .collect();
            if missing.is_empty() {
                return said;
            }
            assert!(
                start.elapsed() < Duration::from_secs(seconds),
                "the server never said {missing:?}: {said:?}"
            );
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(50)) {
                said.push(line);
            }
        }
    }
}

/// Starts the server on `port` (0 for a free one) with `envs`; returns it
/// once it says which port it hosts on.
pub fn server_with(game: &str, port: u16, arguments: &[&str], envs: &[(&str, &Path)]) -> ServerRun {
    let mut command = Command::new(env!("CARGO_BIN_EXE_axcenity-server"));
    command
        .arg(game)
        .args(["--port", &port.to_string()])
        // Tests leave the router alone.
        .arg("--no-port-map")
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = Running(command.spawn().expect("the server starts"));
    let (sender, lines) = mpsc::channel();
    for stream in [
        Box::new(child.0.stderr.take().unwrap()) as Box<dyn std::io::Read + Send>,
        Box::new(child.0.stdout.take().unwrap()),
    ] {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = sender.send(line);
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
        if let Ok(line) = lines.recv_timeout(Duration::from_millis(50)) {
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
    ServerRun {
        process: child,
        port,
        lines,
    }
}

/// Starts the server on a free port; returns it, its port, and its lines.
pub fn server(game: &str, arguments: &[&str]) -> (Running, u16, mpsc::Receiver<String>) {
    let run = server_with(game, 0, arguments, &[]);
    (run.process, run.port, run.lines)
}

/// Starts `axcenity-bots` for `game` joining `port`, reporting each second
/// to `report`, with more `arguments`.
pub fn bots(game: &str, port: u16, report: &Path, arguments: &[&str]) -> Running {
    Running(
        Command::new(env!("CARGO_BIN_EXE_axcenity-bots"))
            .arg(game)
            .args(["--join", &format!("127.0.0.1:{port}")])
            .args(["--report", "1", "--report-file", report.to_str().unwrap()])
            .args(arguments)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the bots start"),
    )
}

/// Waits for bots to finish; whether they succeeded, and their summary.
pub fn finish(mut bots: Running, report: &Path) -> (bool, serde_json::Value) {
    let status = bots.0.wait().unwrap();
    let lines = json_lines(report);
    let summary = lines
        .iter()
        .rev()
        .find(|line| line["summary"] == true)
        .cloned()
        .unwrap_or_else(|| panic!("no summary: {lines:?}"));
    (status.success(), summary)
}

pub fn json_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
