//! `axcenity-server <game> [--port <port>] [--password <text>] [--bench <frames>]`: a
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
//! I1, `docs/development/benchmarks.md`).

use std::ffi::OsString;
use std::fs;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axcenity_assets::Assets;
use axcenity_core::bench::{BenchRun, Laps};
use axcenity_core::{ClassName, World};
use axcenity_net::{GameInfo, Network};
use axcenity_script::{OutputKind, ScriptRuntime};
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};

const USAGE: &str = "usage: axcenity-server <project-folder | game.axgame> [--port <port>] [--password <text>] [--bench <frames>]\n\
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
                "--help" | "-h" => bail!("{USAGE}"),
                _ if text.starts_with("--") => bail!("unknown option `{text}`\n\n{USAGE}"),
                _ if parsed.game.is_none() => parsed.game = Some(PathBuf::from(argument)),
                _ => bail!("only one game can be served at a time\n\n{USAGE}"),
            }
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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
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
    let assets = Assets::new(&project.folder);
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.set_assets(assets.clone());
    scripts.set_save_folder(axcenity_save::game_folder(&project.manifest.name));
    if let Err(err) = axcenity_project::load_starter_character(&project.folder, &mut world) {
        eprintln!("warning: {err}");
    }
    scripts.set_input_map(project.input_map.clone());
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

    let mut network = Network::new(GameInfo {
        name: project.manifest.name.clone(),
        version: project.manifest.version.clone(),
    });
    if let Err(reason) = network.host(
        &mut world,
        &mut scripts,
        arguments.port,
        arguments.password,
        false,
    ) {
        bail!("{reason}");
    }
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
            if bench.record(&laps) {
                let scene = axcenity_core::bench::scene_name(&project.folder);
                print!("{}", bench.finish_text("server", &scene));
                server
                    .network
                    .disconnect(&mut server.sim.world, &mut server.sim.scripts);
                return Ok(());
            }
            continue;
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
}
