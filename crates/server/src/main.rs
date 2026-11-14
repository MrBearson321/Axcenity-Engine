//! `axcenity-server <game> [--port <port>] [--password <text>] [--max-players <n>] [--stats <s>]
//! [--bench <frames> [--bench-clients <n>]]`: a
//! dedicated server (Phase 63). It loads a project folder or a `.axgame`,
//! runs the game's Scripts (never its LocalScripts: it has no player of
//! its own) and physics at 60 frames a second with no window, audio or
//! GPU, and hosts the game on UDP for players to `--join`.
//!
//! Players who join appear in the server's `Players` and fire `PlayerAdded`
//! in its Scripts; their devices follow its world and run their
//! LocalScripts there, and RemoteEvents and RemoteFunctions cross between
//! them (Phase 64). Nothing in ServerStorage or ServerScriptService leaves
//! the server.
//!
//! `--bench <frames>` runs frames back to back at a fixed 1/60 s, without
//! waiting, prints each step's times after a warm-up and quits (roadmap
//! I1, `docs/benchmarks.md`). `--bench-clients <n>` hosts in memory
//! instead and joins `n` loopback players that read everything sent to
//! them, so the bench measures replication to `n` players (roadmap M7);
//! measuring starts once they all have their snapshot.
//!
//! `--max-players <n>` takes up to n players whatever the game's
//! `Players.MaxPlayers` says (scale tests, multiplayer readiness MP1).
//! `--kick-flags <n>` kicks a player whose device is flagged n times in a
//! minute for sending what a game client doesn't (MP3; 5 unless told, 0
//! never).
//! `--stats <seconds>` prints a line that often while serving: frame times,
//! players, and their round trips, loss and bandwidth; `--stats-file
//! <path>` also writes each as a JSON line.

use std::ffi::OsString;
use std::fs;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axcenity_assets::Assets;
use axcenity_core::bench::{BenchRun, Laps};
use axcenity_core::{ClassName, World};
use axcenity_net::{Client, GameInfo, Join, Network, SessionEvent};
use axcenity_script::{OutputKind, ScriptRuntime};
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};

mod stats;

const USAGE: &str = "usage: axcenity-server <project-folder | game.axgame> [--port <port>] [--password <text>] [--max-players <n>] [--kick-flags <n>] [--stats <seconds> [--stats-file <path>]] [--bench <frames> [--bench-clients <n>]]  (or --version [--json])\n\
                     example: cargo run -p axcenity-server -- examples/hello-part --port 47800";

/// Each frame is this long.
const FRAME: Duration = Duration::from_micros(16_667);

/// What the server was started with.
#[derive(Debug, Default, PartialEq)]
struct Arguments {
    game: Option<PathBuf>,
    port: Option<u16>,
    password: Option<String>,
    /// `--bench <frames>`: measure that many frames, print them and quit.
    bench: Option<usize>,
    /// `--bench-clients <n>`: loopback players to replicate to while
    /// measuring.
    bench_clients: usize,
    /// `--max-players <n>`: instead of the game's `Players.MaxPlayers`.
    max_players: Option<u32>,
    /// `--kick-flags <n>`: instead of `net::KICK_FLAGS`.
    kick_flags: Option<usize>,
    /// `--stats <seconds>`: how often to print the server's numbers.
    stats: Option<f64>,
    /// `--stats-file <path>`: where to write them as JSON lines too.
    stats_file: Option<PathBuf>,
}

impl Arguments {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments> {
        let mut parsed = Arguments::default();
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let text = argument.to_string_lossy().into_owned();
            match text.as_str() {
                "--port" => {
                    let port = arguments
                        .next()
                        .map(|port| port.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let Ok(port) = port.parse::<u16>() else {
                        bail!("--port takes a number from 0 to 65535, not `{port}`\n\n{USAGE}");
                    };
                    parsed.port = Some(port);
                }
                "--password" => {
                    let Some(password) = arguments.next() else {
                        bail!("--password takes the password players join with\n\n{USAGE}");
                    };
                    parsed.password = Some(password.to_string_lossy().into_owned());
                }
                "--bench" => {
                    let frames = arguments
                        .next()
                        .and_then(|frames| frames.to_str()?.parse::<usize>().ok())
                        .filter(|&frames| frames > 0);
                    let Some(frames) = frames else {
                        bail!("--bench takes how many frames to measure, such as 600\n\n{USAGE}");
                    };
                    parsed.bench = Some(frames);
                }
                "--bench-clients" => {
                    let clients = arguments
                        .next()
                        .and_then(|clients| clients.to_str()?.parse::<usize>().ok());
                    let Some(clients) = clients else {
                        bail!(
                            "--bench-clients takes how many players to join, such as 8\n\n{USAGE}"
                        );
                    };
                    parsed.bench_clients = clients;
                }
                "--max-players" => {
                    let max = arguments
                        .next()
                        .and_then(|max| max.to_str()?.parse::<u32>().ok())
                        .filter(|&max| max > 0);
                    let Some(max) = max else {
                        bail!(
                            "--max-players takes how many players at most, such as 32\n\n{USAGE}"
                        );
                    };
                    parsed.max_players = Some(max);
                }
                "--kick-flags" => {
                    let flags = arguments
                        .next()
                        .and_then(|flags| flags.to_str()?.parse::<usize>().ok());
                    let Some(flags) = flags else {
                        bail!(
                            "--kick-flags takes how many flags in a minute kick a player, such as 5 (0 never)\n\n{USAGE}"
                        );
                    };
                    parsed.kick_flags = Some(flags);
                }
                "--stats" => {
                    let seconds = arguments
                        .next()
                        .and_then(|seconds| seconds.to_str()?.parse::<f64>().ok())
                        .filter(|&seconds| seconds > 0.0);
                    let Some(seconds) = seconds else {
                        bail!("--stats takes how often to print, in seconds, such as 5\n\n{USAGE}");
                    };
                    parsed.stats = Some(seconds);
                }
                "--stats-file" => {
                    let Some(path) = arguments.next() else {
                        bail!("--stats-file takes a path\n\n{USAGE}");
                    };
                    parsed.stats_file = Some(PathBuf::from(path));
                }
                "--help" | "-h" => bail!("{USAGE}"),
                _ if text.starts_with("--") => bail!("unknown option `{text}`\n\n{USAGE}"),
                _ if parsed.game.is_none() => parsed.game = Some(PathBuf::from(argument)),
                _ => bail!("only one game can be served at a time\n\n{USAGE}"),
            }
        }
        if parsed.stats_file.is_some() && parsed.stats.is_none() {
            bail!("--stats-file only goes with --stats\n\n{USAGE}");
        }
        if parsed.bench_clients > 0 && parsed.bench.is_none() {
            bail!("--bench-clients only goes with --bench\n\n{USAGE}");
        }
        Ok(parsed)
    }
}

/// A `.axgame` unpacked for the run, removed afterwards.
struct Unpacked(PathBuf);

impl Drop for Unpacked {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn main() -> Result<()> {
    // `--version` (and `--version --json`): this build's version, channel,
    // commit and toolchain, then exit.
    let all: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(text) = axcenity_release::version_flag("axcenity-server", &all) {
        println!("{text}");
        return Ok(());
    }
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(axcenity_net::LOG_FILTER),
    )
    .init();
    let arguments = Arguments::parse(std::env::args_os().skip(1))?;
    let Some(game) = arguments.game else {
        bail!("no game to serve\n\n{USAGE}");
    };
    let mut _unpacked = None;
    let folder = if game.is_file() {
        let folder = std::env::temp_dir().join(format!("axcenity-server-{}", std::process::id()));
        let bytes = fs::read(&game)
            .with_context(|| format!("could not read game file `{}`", game.display()))?;
        axcenity_project::unpack_game(&bytes, &folder)
            .with_context(|| format!("could not unpack game file `{}`", game.display()))?;
        _unpacked = Some(Unpacked(folder.clone()));
        folder
    } else {
        game
    };
    let project = axcenity_project::load(&folder)
        .with_context(|| format!("could not open the project in `{}`", folder.display()))?;
    let mut problems = project.problems;
    let start_level = project.manifest.start_level.clone();
    let mut levels: std::collections::BTreeMap<String, World> =
        axcenity_project::load_levels(&project.folder, &mut problems)
            .into_iter()
            .collect();
    levels.insert(start_level.clone(), project.world.clone());
    for problem in problems {
        eprintln!("error: {:#}", anyhow::Error::new(problem));
    }
    let mut world = project.world;
    // Switched-off data layers start set aside (R16.4).
    axcenity_core::data_layers::start(&mut world);
    let assets = Assets::new(&project.folder);
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_dedicated();
    scripts.set_assets(assets.clone());
    scripts.set_save_folder(axcenity_save::game_folder(&project.manifest.name));
    if let Err(err) = axcenity_project::load_starter_character(&project.folder, &mut world) {
        eprintln!("warning: {err}");
    }
    scripts.set_input_map(project.input_map.clone());
    // Scripts inside objects in levels LevelService loads later, and in
    // the StarterCharacter each player spawns as (G2).
    let worlds = levels.values().chain([&world]);
    for (path, source) in axcenity_project::level_code(&project.folder, worlds) {
        scripts.know_code(&path, &source);
    }
    scripts.set_levels(&start_level, &start_level, levels);
    // Scripts and the ModuleScripts they require; LocalScripts run on
    // players' devices, and this server has no player.
    for script in &project.scripts {
        let class = world.get(script.instance).map(|instance| instance.class());
        if class == Some(ClassName::LocalScript) {
            continue;
        }
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
        print_output(&mut scripts);
    }

    if let Some(max) = arguments.max_players {
        let players = world.players();
        let _ = world.set_property(
            players,
            "MaxPlayers",
            axcenity_core::Value::Number(f64::from(max)),
        );
    }

    let game_info = GameInfo {
        name: project.manifest.name.clone(),
        version: project.manifest.version.clone(),
    };
    let mut network = Network::new(game_info.clone());
    if let Some(flags) = arguments.kick_flags {
        network.set_kick_flags(flags);
    }
    // `ReplayService:StopRecording` saves beside the server's saves (Phase 81).
    network.set_replay_folder(
        axcenity_save::game_folder(&project.manifest.name).map(|data| data.join("replays")),
    );
    let mut bench_clients = if arguments.bench_clients > 0 {
        let first = network.host_in_memory(&mut world, &mut scripts, false);
        BenchClients::join(&mut network, first, &game_info, arguments.bench_clients)
    } else {
        if let Err(reason) = network.host(
            &mut world,
            &mut scripts,
            arguments.port,
            arguments.password,
            false,
        ) {
            bail!("{reason}");
        }
        BenchClients::default()
    };
    print_log(&mut network);

    // The physics, and the characters, agents and animators the server
    // leaves to players' devices, are made here with nothing to do yet.
    let mut server = Server {
        sim: Sim::new(world, scripts, assets),
        network,
    };
    let schedule = Schedule::for_host(Hosts::SERVER);
    let mut bench = arguments
        .bench
        .map(|frames| BenchRun::new(BenchRun::DEFAULT_WARM_UP, frames));
    let mut stats = match arguments.stats {
        Some(seconds) => Some(stats::ServerStats::new(
            seconds,
            arguments.stats_file.as_deref(),
        )?),
        None => None,
    };
    let mut last = Instant::now();
    loop {
        let started = Instant::now();
        let seconds = match bench {
            Some(_) => FRAME.as_secs_f64(),
            None => started.duration_since(last).as_secs_f64().min(0.1),
        };
        last = started;
        let mut laps = Laps::start();
        {
            profiling::scope!("frame");
            schedule.run(
                &mut FrameCtx::new(seconds, Hosts::SERVER, &mut server),
                &mut laps,
            );
        }
        profiling::finish_frame!();
        for error in server.sim.assets.take_errors() {
            eprintln!("error: {error}");
        }
        if let Some(bench) = &mut bench {
            let ready = bench_clients.frame(&server.network);
            if ready && bench.record(&laps) {
                let scene = axcenity_core::bench::scene_name(&project.folder);
                print!("{}", bench.finish_text("server", &scene));
                bench_clients.report(&server.network);
                server
                    .network
                    .disconnect(&mut server.sim.world, &mut server.sim.scripts);
                return Ok(());
            }
            continue;
        }
        if let Some(stats) = &mut stats {
            stats.frame(started.elapsed(), &server.network, &server.sim);
        }
        if let Some(rest) = FRAME.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

/// What the server does differently from the shared frame: it has the
/// network and prints to the console. Each player's device moves their
/// character (Phase 66), which physics holds still here, as it does what
/// they own; agents, animation and everything drawn belong to players'
/// devices.
struct Server {
    sim: Sim,
    network: Network,
}

impl HostIo for Server {
    fn sim(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn net_receive(&mut self, seconds: f64) -> ControlFlow<()> {
        let Sim { world, scripts, .. } = &mut self.sim;
        self.network.receive(world, scripts, seconds);
        // With no screen, a player here has nothing to load (Phase 80).
        for (request, _) in self.network.take_viewer_requests() {
            scripts.finish_stream_around(world, request);
        }
        print_log(&mut self.network);
        ControlFlow::Continue(())
    }

    fn flush_output(&mut self) {
        print_output(&mut self.sim.scripts);
    }

    fn prepare_physics(&mut self) {
        let Sim {
            world,
            scripts,
            physics,
            ..
        } = &mut self.sim;
        physics.set_held(self.network.held_parts(world, scripts));
        // Far from every player, physics sleeps (Phase 80).
        physics.set_far_sleep(
            axcenity_core::streaming::Settings::of(world)
                .map(|settings| (self.network.player_focuses(world), settings.unload_radius())),
        );
    }

    fn net_send(&mut self) {
        self.network.send(&self.sim.world, &mut self.sim.scripts);
    }
}

/// `--bench-clients`: loopback players, each reading everything the
/// server sends them as a player's device would, so the bench pays for
/// replicating to them. The bench's warm-up starts once every client has
/// its snapshot. Their own reading isn't in the frame's steps; it's
/// reported on its own.
#[derive(Default)]
struct BenchClients {
    joins: Vec<Join>,
    /// Which clients have their snapshot, or gave up.
    done: Vec<bool>,
    /// Frames until every client had its snapshot.
    joining_frames: Option<usize>,
    /// Frames since then.
    frames: usize,
    /// Bytes sent to every client by the end of the warm-up.
    warm_up_bytes: Option<u64>,
    /// The biggest snapshot message a client got, in bytes.
    snapshot_bytes: usize,
    /// Time spent receiving and decoding on the clients' side, after the
    /// warm-up.
    reading: Duration,
    /// Server frames so far.
    seen: usize,
}

impl BenchClients {
    fn join(network: &mut Network, first: Client, game: &GameInfo, count: usize) -> BenchClients {
        let mut clients = vec![first];
        clients.extend((1..count).filter_map(|_| network.connect_loopback()));
        let joins: Vec<Join> = clients
            .into_iter()
            .enumerate()
            .map(|(index, client)| {
                Join::start(
                    client,
                    game,
                    &format!("Bench{}", index + 1),
                    None,
                    index as u64 + 1,
                )
            })
            .collect();
        BenchClients {
            done: vec![false; joins.len()],
            joins,
            ..BenchClients::default()
        }
    }

    /// After each server frame: the clients take in what arrived. True once
    /// every client has its snapshot (always, with no clients).
    fn frame(&mut self, network: &Network) -> bool {
        if self.joins.is_empty() {
            return true;
        }
        self.seen += 1;
        let started = Instant::now();
        for (join, done) in self.joins.iter_mut().zip(&mut self.done) {
            for event in join.update(FRAME) {
                match event {
                    SessionEvent::Snapshot { update, .. } => {
                        self.snapshot_bytes = self.snapshot_bytes.max(update.encoded_len());
                        *done = true;
                    }
                    SessionEvent::Failed(reason) => {
                        eprintln!("bench client: {reason}");
                        *done = true;
                    }
                    _ => {}
                }
            }
            join.flush();
        }
        if self.joining_frames.is_none() {
            if !self.done.iter().all(|&done| done) {
                return false;
            }
            self.joining_frames = Some(self.seen);
        }
        self.frames += 1;
        if self.frames > BenchRun::DEFAULT_WARM_UP {
            self.reading += started.elapsed();
        } else if self.frames == BenchRun::DEFAULT_WARM_UP {
            self.warm_up_bytes = Some(bytes_sent(network));
        }
        true
    }

    fn report(&self, network: &Network) {
        if self.joins.is_empty() {
            return;
        }
        let measured = self.frames.saturating_sub(BenchRun::DEFAULT_WARM_UP).max(1);
        let warm_up = self.warm_up_bytes.unwrap_or(0);
        let after = bytes_sent(network).saturating_sub(warm_up);
        let joined = self
            .joins
            .iter()
            .filter(|join| join.you().is_some() && !join.is_over())
            .count();
        println!(
            "bench clients: {joined} of {} still in; snapshot {} bytes, all in after {} frames; \
             {} bytes sent by the end of the warm-up, then {:.0} bytes a frame to all; \
             clients read for {:.3} ms a frame",
            self.joins.len(),
            self.snapshot_bytes,
            self.joining_frames.unwrap_or(0),
            warm_up,
            after as f64 / measured as f64,
            self.reading.as_secs_f64() * 1000.0 / measured as f64,
        );
    }
}

/// Every byte the server has sent its loopback players.
fn bytes_sent(network: &Network) -> u64 {
    network
        .stats()
        .iter()
        .filter_map(|stats| stats.loopback_bytes_sent)
        .sum()
}

fn print_log(network: &mut Network) {
    for line in network.take_log() {
        eprintln!("{line}");
    }
}

/// Scripts' output: prints on stdout, warnings and errors on stderr.
fn print_output(scripts: &mut ScriptRuntime) {
    for line in scripts.take_output() {
        match line.kind {
            OutputKind::Print => println!("{}", line.text),
            OutputKind::Warning => eprintln!("warning: {}", line.text),
            OutputKind::Error => {
                eprintln!("error: {}", line.text);
                for frame in &line.trace {
                    eprintln!("    {frame}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Arguments> {
        Arguments::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn the_game_port_and_password_can_come_in_any_order() {
        let parsed = parse(&["--port", "4000", "games/castle", "--password", "moat"]).unwrap();
        assert_eq!(
            parsed,
            Arguments {
                game: Some(PathBuf::from("games/castle")),
                port: Some(4000),
                password: Some("moat".to_owned()),
                bench: None,
                bench_clients: 0,
                max_players: None,
                kick_flags: None,
                stats: None,
                stats_file: None,
            }
        );
    }

    #[test]
    fn a_bad_port_says_what_it_takes() {
        let err = parse(&["castle", "--port", "lots"]).unwrap_err();
        assert!(err.to_string().contains("--port takes a number"), "{err}");
    }

    #[test]
    fn bench_takes_a_frame_count() {
        assert_eq!(
            parse(&["castle", "--bench", "600"]).unwrap().bench,
            Some(600)
        );
        let err = parse(&["castle", "--bench", "0"]).unwrap_err();
        assert!(err.to_string().contains("--bench takes"), "{err}");
    }

    #[test]
    fn bench_clients_go_with_bench() {
        let parsed = parse(&["castle", "--bench", "600", "--bench-clients", "8"]).unwrap();
        assert_eq!(parsed.bench_clients, 8);
        let err = parse(&["castle", "--bench-clients", "8"]).unwrap_err();
        assert!(err.to_string().contains("only goes with --bench"), "{err}");
    }

    #[test]
    fn max_players_and_stats_take_numbers() {
        let parsed = parse(&["castle", "--max-players", "32", "--stats", "5"]).unwrap();
        assert_eq!((parsed.max_players, parsed.stats), (Some(32), Some(5.0)));
        let parsed = parse(&["castle", "--kick-flags", "0"]).unwrap();
        assert_eq!(parsed.kick_flags, Some(0));
        let err = parse(&["castle", "--kick-flags", "often"]).unwrap_err();
        assert!(err.to_string().contains("--kick-flags takes"), "{err}");
        let err = parse(&["castle", "--max-players", "0"]).unwrap_err();
        assert!(err.to_string().contains("--max-players takes"), "{err}");
        let err = parse(&["castle", "--stats-file", "s.jsonl"]).unwrap_err();
        assert!(err.to_string().contains("only goes with --stats"), "{err}");
    }
}
