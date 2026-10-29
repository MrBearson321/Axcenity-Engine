//! Runs the real `axcenity-server` binary, as on a machine with no display,
//! and joins it as a player would.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, World};
use axcenity_net::{GameInfo, Network};
use axcenity_script::ScriptRuntime;

/// Kills the server when the test ends, passing or not.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_server_runs_headless_and_a_player_joins_it() {
    let example = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello-part");
    let mut command = Command::new(env!("CARGO_BIN_EXE_axcenity-server"));
    command
        .args([example, "--port", "0"])
        // No display, as on a headless Linux server.
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut server = Running(command.spawn().expect("the server starts"));
    let (lines, heard) = mpsc::channel();
    let stderr = server.0.stderr.take().unwrap();
    let stdout = server.0.stdout.take().unwrap();
    for stream in [
        Box::new(stderr) as Box<dyn std::io::Read + Send>,
        Box::new(stdout),
    ] {
        let lines = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = lines.send(line);
            }
        });
    }
    let mut said = Vec::new();
    let wait_for = |said: &mut Vec<String>, text: &str| -> String {
        let start = Instant::now();
        loop {
            if let Some(line) = said.iter().find(|line| line.contains(text)) {
                return line.clone();
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "the server never said \"{text}\": {said:?}"
            );
            if let Ok(line) = heard.recv_timeout(Duration::from_millis(50)) {
                said.push(line);
            }
        }
    };
    // The example's Script runs on the server.
    wait_for(&mut said, "Hello world!");
    let hosting = wait_for(&mut said, "on UDP port");
    let port: u16 = hosting
        .split("on UDP port ")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|port| port.parse().ok())
        .unwrap_or_else(|| panic!("no port in {hosting:?}"));

    let mut world = World::new();
    let mut scripts = ScriptRuntime::new();
    let script = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(script, Some(world.starter_player_scripts()))
        .unwrap();
    scripts.start_script(&mut world, script, "scripts/start.client.lua", "");
    let mut network = Network::new(GameInfo {
        name: "Hello Part".to_owned(),
        version: "1.0".to_owned(),
    })
    .with_discovery_port(47931);
    network
        .join(&mut world, &mut scripts, &format!("127.0.0.1:{port}"), None)
        .unwrap();
    let start = Instant::now();
    // Joined, this device follows the server's world (Phase 64).
    while !(network.is_joined() && scripts.is_client_only()) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "never joined: {:?}",
            network.take_log()
        );
        network.receive(&mut world, &mut scripts, 1.0 / 60.0);
        network.send(&world, &mut scripts);
        std::thread::sleep(Duration::from_millis(10));
    }
    // A dedicated server has no player of its own.
    let players = world.get(world.players()).unwrap().children().to_vec();
    assert_eq!(
        players,
        [scripts.local_player().unwrap()],
        "only this player"
    );
    assert!(
        !world.get(world.workspace()).unwrap().children().is_empty(),
        "the server's Workspace arrived"
    );
    wait_for(&mut said, "Player joined.");
}
