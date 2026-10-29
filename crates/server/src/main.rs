//! `axcenity-server <game> [--port <port>] [--password <text>]`: a
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

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axcenity_assets::Assets;
use axcenity_core::{ClassName, World};
use axcenity_net::{GameInfo, Network};
use axcenity_physics::Physics;
use axcenity_script::{OutputKind, ScriptRuntime};

const USAGE: &str = "usage: axcenity-server <project-folder | game.axgame> [--port <port>] [--password <text>]\n\
                     example: cargo run -p axcenity-server -- examples/hello-part --port 47800";

/// Each frame is this long.
const FRAME: Duration = Duration::from_micros(16_667);

/// What the server was started with.
#[derive(Debug, Default, PartialEq)]
struct Arguments {
    game: Option<PathBuf>,
    port: Option<u16>,
    password: Option<String>,
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

    let mut physics = Physics::with_assets(assets.clone());
    let mut last = Instant::now();
    loop {
        let started = Instant::now();
        let seconds = started.duration_since(last).as_secs_f64().min(0.1);
        last = started;
        frame(
            &mut world,
            &mut scripts,
            &mut physics,
            &mut network,
            seconds,
        );
        for error in assets.take_errors() {
            eprintln!("error: {error}");
        }
        if let Some(rest) = FRAME.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

/// One frame, in the frame order: network receive, players (their
/// characters and respawns), scripts, physics, touches and Heartbeat,
/// network send. Each player's device moves their character (Phase 66),
/// which physics holds still here, as it does what they own; agents,
/// animation and everything drawn belong to players' devices.
fn frame(
    world: &mut World,
    scripts: &mut ScriptRuntime,
    physics: &mut Physics,
    network: &mut Network,
    seconds: f64,
) {
    network.receive(world, scripts, seconds);
    print_log(network);
    scripts.step_players(world, seconds);
    scripts.step(world, seconds);
    scripts.stepped(world, seconds);
    scripts.apply_shatters(world, physics);
    physics.set_held(network.held_parts(world, scripts));
    physics.step(world, seconds);
    let touches = physics.take_touches();
    scripts.fire_touched(world, &touches);
    let ends = physics.take_touch_ends();
    scripts.fire_touch_ended(world, &ends);
    scripts.fire_breaks(world, physics);
    scripts.step_health(world, seconds);
    scripts.heartbeat(world, seconds);
    print_output(scripts);
    network.send(world, scripts);
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
            }
        );
    }

    #[test]
    fn a_bad_port_says_what_it_takes() {
        let err = parse(&["castle", "--port", "lots"]).unwrap_err();
        assert!(err.to_string().contains("--port takes a number"), "{err}");
    }
}
