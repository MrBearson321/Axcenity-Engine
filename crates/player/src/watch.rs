//! `axcenity-player --watch`: live asset reload while developing a game.
//!
//! With the flag and a project folder, the player watches the folder's
//! `assets/` (`project::Watcher`) and swaps each changed file in as the
//! editor does: models, textures, materials and fonts through `Assets`,
//! clips, state machines and linked takes through `Animators`, and sounds
//! through `GameAudio`. Without the flag, and for a built game (a
//! `.axgame` file, or one bundled next to the player), nothing is watched.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use axcenity_anim::Animators;
use axcenity_assets::{Assets, Reload};
use axcenity_audio::GameAudio;
use axcenity_project::{Watched, Watcher};

use axcenity_render::{GraphicsSettings, Quality};

use crate::USAGE;

/// The flag that turns live reload on.
pub const WATCH_FLAG: &str = "--watch";
/// The flag that turns the Developer Console on in a built game.
pub const DEV_FLAG: &str = "--dev";

/// What the player was started with.
#[derive(Debug, Default, PartialEq)]
pub struct Arguments {
    /// A project folder or a `.axgame` file; `None` for the game bundled
    /// next to the player.
    pub game: Option<PathBuf>,
    /// `--watch` was given.
    pub watch: bool,
    /// `--host [port]` or `--join address:port` (Phase 63).
    pub network: Option<NetworkStart>,
    /// `--password <text>`: the password a host asks for, or a join gives.
    pub password: Option<String>,
    /// `--lan` with `--host`: a LAN game, not secured with connect tokens
    /// (MP4).
    pub lan: bool,
    /// `--dev`: the F9 Developer Console, even in a built game whose
    /// project setting has it off (Phase 71).
    pub dev: bool,
    /// `--bench <frames>`: measure that many frames' steps after a warm-up,
    /// print them, and quit (roadmap I1, `docs/development/benchmarks.md`).
    pub bench: Option<usize>,
    /// `--size <width>x<height>`: the window's size in physical pixels,
    /// so benchmarks draw the same number of pixels on every monitor.
    pub size: Option<(u32, u32)>,
    /// `--quality <Low|Medium|High|Ultra>`: that preset with no overrides,
    /// for this run only (`graphics.json` is left as it is).
    pub quality: Option<Quality>,
    /// `--graphics <file.json>`: graphics settings read from that file (as
    /// `graphics.json` holds them; leaving a setting out keeps the preset's)
    /// for this run only, including whether the GPU is opened with
    /// hardware ray tracing. `--quality` with it replaces the file's preset.
    pub graphics: Option<PathBuf>,
    /// `--replay <file>`: watch a `.replay` of the game instead of
    /// playing it (Phase 81).
    pub replay: Option<PathBuf>,
}

/// Hosting or joining from the start.
#[derive(Debug, PartialEq, Eq)]
pub enum NetworkStart {
    /// A listen server on this port (`None`: the default port).
    Host(Option<u16>),
    /// Joins the game at this address.
    Join(String),
}

impl Arguments {
    /// Reads the arguments after the program's name. The options can come
    /// before or after the game.
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments> {
        let mut parsed = Arguments::default();
        // `peekable` lets `--host` look at the next argument without taking
        // it: a port number goes with it, a game folder doesn't.
        let mut arguments = arguments.into_iter().peekable();
        while let Some(argument) = arguments.next() {
            if argument == WATCH_FLAG {
                parsed.watch = true;
            } else if argument == DEV_FLAG {
                parsed.dev = true;
            } else if argument == "--host" || argument == "--join" {
                if parsed.network.is_some() {
                    bail!("a player can host or join, not both\n\n{USAGE}");
                }
                parsed.network = Some(if argument == "--host" {
                    let port = arguments
                        .peek()
                        .and_then(|next| next.to_str())
                        .and_then(|next| next.parse::<u16>().ok());
                    if port.is_some() {
                        arguments.next();
                    }
                    NetworkStart::Host(port)
                } else {
                    let Some(address) = arguments.next() else {
                        bail!(
                            "--join takes the address to join, such as 192.168.1.20:47800\n\n{USAGE}"
                        );
                    };
                    NetworkStart::Join(address.to_string_lossy().into_owned())
                });
            } else if argument == "--bench" {
                let frames = arguments
                    .next()
                    .and_then(|frames| frames.to_str()?.parse::<usize>().ok())
                    .filter(|&frames| frames > 0);
                let Some(frames) = frames else {
                    bail!("--bench takes how many frames to measure, such as 600\n\n{USAGE}");
                };
                parsed.bench = Some(frames);
            } else if argument == "--size" {
                let size = arguments.next().and_then(|size| {
                    let (width, height) = size.to_str()?.split_once(['x', 'X'])?;
                    Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?))
                });
                match size {
                    Some((width, height)) if width > 0 && height > 0 => {
                        parsed.size = Some((width, height));
                    }
                    _ => bail!(
                        "--size takes the window's size in pixels, such as 2560x1440

{USAGE}"
                    ),
                }
            } else if argument == "--quality" {
                let name = arguments
                    .next()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase());
                let quality = Quality::ALL
                    .into_iter()
                    .find(|quality| Some(quality.label().to_ascii_lowercase()) == name);
                let Some(quality) = quality else {
                    bail!(
                        "--quality takes Low, Medium, High or Ultra

{USAGE}"
                    );
                };
                parsed.quality = Some(quality);
            } else if argument == "--graphics" {
                let Some(file) = arguments.next() else {
                    bail!(
                        "--graphics takes a graphics settings file, such as rt-all.json\n\n{USAGE}"
                    );
                };
                parsed.graphics = Some(PathBuf::from(file));
            } else if argument == "--replay" {
                let Some(file) = arguments.next() else {
                    bail!("--replay takes the .replay file to watch\n\n{USAGE}");
                };
                parsed.replay = Some(PathBuf::from(file));
            } else if argument == "--lan" {
                parsed.lan = true;
            } else if argument == "--password" {
                let Some(password) = arguments.next() else {
                    bail!("--password takes the game's password\n\n{USAGE}");
                };
                parsed.password = Some(password.to_string_lossy().into_owned());
            } else if argument.to_string_lossy().starts_with("--") {
                bail!("unknown option `{}`\n\n{USAGE}", argument.to_string_lossy());
            } else if parsed.game.is_none() {
                parsed.game = Some(PathBuf::from(argument));
            } else {
                bail!("only one game can be played at a time\n\n{USAGE}");
            }
        }
        if parsed.lan && !matches!(parsed.network, Some(NetworkStart::Host(_))) {
            bail!("--lan goes with --host: it hosts a LAN game, not secured\n\n{USAGE}");
        }
        if parsed.replay.is_some() && parsed.network.is_some() {
            bail!("a replay is watched on its own, without --host or --join\n\n{USAGE}");
        }
        Ok(parsed)
    }

    /// The graphics settings this run is held to, whatever `graphics.json`
    /// or a LocalScript says: `--graphics`' file (with `--quality`'s preset,
    /// if given), or `--quality`'s preset alone. `None` when neither was
    /// given.
    pub fn fixed_graphics(&self) -> Result<Option<GraphicsSettings>> {
        let mut settings = match &self.graphics {
            Some(file) => {
                let text = std::fs::read_to_string(file).with_context(|| {
                    format!("could not read the graphics settings in {}", file.display())
                })?;
                serde_json::from_str::<GraphicsSettings>(&text).with_context(|| {
                    format!("{} doesn't read as graphics settings", file.display())
                })?
            }
            None => match self.quality {
                Some(quality) => return Ok(Some(GraphicsSettings::preset(quality))),
                None => return Ok(None),
            },
        };
        if let Some(quality) = self.quality {
            settings.quality = quality;
        }
        Ok(Some(settings))
    }

    /// The project folder to watch: the game's, if `--watch` was given and
    /// the game is a project folder. A built game is never watched, and
    /// asking for it says so.
    pub fn watched_folder(&self) -> Option<&Path> {
        if !self.watch {
            return None;
        }
        match &self.game {
            Some(folder) if folder.is_dir() => Some(folder),
            _ => {
                eprintln!(
                    "warning: {WATCH_FLAG} only works on a project folder; a built game is \
                     never watched"
                );
                None
            }
        }
    }
}

/// Starts watching `folder`'s assets.
pub fn start(folder: &Path) -> Watcher {
    eprintln!(
        "Watching `{}` for asset changes.",
        folder.join("assets").display()
    );
    Watcher::start(folder.to_owned(), Watched::Assets)
}

/// Swaps in the new versions of the changed asset files at `paths`.
/// Returns what to tell the developer: a line per file reloaded, and per
/// sound that can't be decoded any more. Other files that fail to load
/// keep their last good version and are reported by `Assets` and
/// `Animators` (once) with their other errors.
pub fn reload(
    paths: &[String],
    assets: &Assets,
    animators: &mut Animators,
    audio: &mut GameAudio,
) -> Vec<String> {
    let mut lines = Vec::new();
    for path in paths {
        let mut reloaded = assets.reload(path) == Reload::Reloaded;
        reloaded |= animators.reload(path);
        match audio.reload(path, assets) {
            Ok(heard) => reloaded |= heard,
            Err(err) => lines.push(format!(
                "error: could not reload `{path}`: {err}. The last good version stays."
            )),
        }
        if reloaded {
            lines.push(format!("Reloaded `{path}`."));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use axcenity_anim::files::{Motion, StateDef, StateMachineFile};
    use axcenity_core::{ClassName, Value, World};

    use super::*;

    fn parse(arguments: &[&str]) -> Result<Arguments> {
        Arguments::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn replay_takes_a_file_and_isnt_played_online() {
        let watched = parse(&["--replay", "final.replay", "examples/hello-part"]).unwrap();
        assert_eq!(watched.replay, Some(PathBuf::from("final.replay")));
        assert_eq!(watched.game, Some(PathBuf::from("examples/hello-part")));
        assert!(parse(&["--replay"]).is_err());
        assert!(parse(&["--replay", "a.replay", "--host", "examples/hello-part"]).is_err());
    }

    #[test]
    fn watch_comes_before_or_after_the_game_and_only_a_project_folder_is_watched() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-door");
        let folder = example.to_str().unwrap();
        for arguments in [[WATCH_FLAG, folder], [folder, WATCH_FLAG]] {
            let parsed = parse(&arguments).unwrap();
            assert!(parsed.watch);
            assert_eq!(parsed.watched_folder(), Some(example.as_path()));
        }
        // Without the flag, nothing is watched.
        let plain = parse(&[folder]).unwrap();
        assert_eq!(plain.watched_folder(), None);
        // Nor is a built game: a game file, or the one next to the player.
        let game =
            std::env::temp_dir().join(format!("axcenity-watch-{}.axgame", std::process::id()));
        fs::write(&game, b"packed").unwrap();
        assert_eq!(
            parse(&[WATCH_FLAG, game.to_str().unwrap()])
                .unwrap()
                .watched_folder(),
            None
        );
        assert_eq!(parse(&[WATCH_FLAG]).unwrap().watched_folder(), None);
        assert_eq!(parse(&[]).unwrap(), Arguments::default());
        let _ = fs::remove_file(game);

        assert!(parse(&["--wach", folder]).is_err());
        assert!(parse(&[folder, folder]).is_err());
    }

    #[test]
    fn bench_takes_a_frame_count() {
        let parsed = parse(&["--bench", "600", "games/s10k"]).unwrap();
        assert_eq!(parsed.bench, Some(600));
        assert_eq!(parsed.game, Some(PathBuf::from("games/s10k")));
        for bad in [&["--bench"][..], &["--bench", "0"], &["--bench", "lots"]] {
            let err = parse(bad).unwrap_err();
            assert!(err.to_string().contains("--bench takes"), "{err}");
        }
    }

    #[test]
    fn a_graphics_file_holds_the_run_to_its_settings() {
        use axcenity_render::RayTracedShadows;
        let folder =
            std::env::temp_dir().join(format!("axcenity-graphics-arg-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("rt.json");
        std::fs::write(
            &file,
            r#"{ "quality": "Ultra", "ray_tracing": true, "ray_traced_shadows": "High" }"#,
        )
        .unwrap();
        let file_arg = file.to_string_lossy().into_owned();

        let fixed = parse(&["--graphics", &file_arg, "games/showcase"])
            .unwrap()
            .fixed_graphics()
            .unwrap()
            .unwrap();
        assert_eq!(fixed.quality, Quality::Ultra);
        assert!(fixed.ray_tracing());
        assert_eq!(fixed.ray_traced_shadows, Some(RayTracedShadows::High));
        // Settings the file leaves out keep the preset's.
        assert_eq!(fixed.ray_traced_ambient_occlusion, None);

        // `--quality` replaces the file's preset and keeps the rest.
        let fixed = parse(&["--graphics", &file_arg, "--quality", "Low"])
            .unwrap()
            .fixed_graphics()
            .unwrap()
            .unwrap();
        assert_eq!(fixed.quality, Quality::Low);
        assert!(fixed.ray_tracing());

        // `--quality` alone is the preset; neither is no fixed settings.
        let preset = parse(&["--quality", "Medium"])
            .unwrap()
            .fixed_graphics()
            .unwrap();
        assert_eq!(preset, Some(GraphicsSettings::preset(Quality::Medium)));
        assert_eq!(
            parse(&["games/showcase"])
                .unwrap()
                .fixed_graphics()
                .unwrap(),
            None
        );

        // A missing or unreadable file is an error that names it.
        assert!(parse(&["--graphics"]).is_err());
        let missing = parse(&["--graphics", "no-such-settings.json"])
            .unwrap()
            .fixed_graphics();
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .contains("no-such-settings.json")
        );
        std::fs::write(&file, "not json").unwrap();
        assert!(
            parse(&["--graphics", &file_arg])
                .unwrap()
                .fixed_graphics()
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn size_and_quality_fix_what_a_benchmark_draws() {
        let parsed = parse(&[
            "--bench",
            "300",
            "--size",
            "2560x1440",
            "--quality",
            "ultra",
            "p",
        ])
        .unwrap();
        assert_eq!(parsed.size, Some((2560, 1440)));
        assert_eq!(parsed.quality, Some(Quality::Ultra));
        for bad in [&["--size", "2560"][..], &["--size", "0x10"], &["--size"]] {
            let err = parse(bad).unwrap_err();
            assert!(err.to_string().contains("--size takes"), "{err}");
        }
        let err = parse(&["--quality", "Epic"]).unwrap_err();
        assert!(err.to_string().contains("--quality takes"), "{err}");
    }

    #[test]
    fn lan_goes_with_host() {
        let lan = parse(&["--host", "--lan", "examples/hello-part"]).unwrap();
        assert!(lan.lan);
        assert_eq!(lan.network, Some(NetworkStart::Host(None)));
        assert!(!parse(&["--host", "examples/hello-part"]).unwrap().lan);
        assert!(parse(&["--lan", "examples/hello-part"]).is_err());
        assert!(parse(&["--join", "127.0.0.1:4000", "--lan"]).is_err());
    }

    #[test]
    fn host_takes_an_optional_port_and_join_an_address() {
        let host = parse(&["--host", "4000", "examples/hello-part"]).unwrap();
        assert_eq!(host.network, Some(NetworkStart::Host(Some(4000))));
        assert_eq!(host.game, Some(PathBuf::from("examples/hello-part")));
        // A game folder after `--host` is the game, not a port.
        let host = parse(&["--host", "examples/hello-part"]).unwrap();
        assert_eq!(host.network, Some(NetworkStart::Host(None)));
        assert_eq!(host.game, Some(PathBuf::from("examples/hello-part")));
        let join = parse(&[
            "examples/hello-part",
            "--join",
            "127.0.0.1:4000",
            "--password",
            "moat",
        ])
        .unwrap();
        assert_eq!(
            join.network,
            Some(NetworkStart::Join("127.0.0.1:4000".to_owned()))
        );
        assert_eq!(join.password.as_deref(), Some("moat"));
        assert!(parse(&["--host", "--join", "127.0.0.1:4000"]).is_err());
        assert!(parse(&["--join"]).is_err());
    }

    #[test]
    fn dev_turns_the_console_on_before_or_after_the_game() {
        assert!(!parse(&["game.axgame"]).unwrap().dev);
        let dev = parse(&["game.axgame", "--dev"]).unwrap();
        assert!(dev.dev);
        assert_eq!(dev.game, Some(PathBuf::from("game.axgame")));
        assert!(parse(&["--dev"]).unwrap().dev);
    }

    /// A one-triangle glTF `width` studs wide at `assets/models/tri.gltf`.
    fn write_triangle(folder: &Path, width: f32) {
        let corners: [f32; 9] = [0.0, 0.0, 0.0, width, 0.0, 0.0, 0.0, 1.0, 0.0];
        let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
        let models = folder.join("assets/models");
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join("tri.bin"), bytes).unwrap();
        let gltf = format!(
            r#"{{
                "asset": {{ "version": "2.0" }},
                "buffers": [ {{ "uri": "tri.bin", "byteLength": 36 }} ],
                "bufferViews": [ {{ "buffer": 0, "byteLength": 36 }} ],
                "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 3,
                                  "type": "VEC3", "min": [0, 0, 0], "max": [{width}, 1, 0] }} ],
                "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ]
            }}"#
        );
        fs::write(models.join("tri.gltf"), gltf).unwrap();
    }

    const TRIANGLE: &str = "assets/models/tri.gltf";
    const CLIP: &str = "assets/animations/Door/Door.anim.json";
    const MACHINE: &str = "assets/animations/Door/Door.statemachine.json";

    /// The changes the watcher reports within 3 seconds.
    fn changes(watcher: &Watcher) -> (Vec<String>, Duration) {
        let start = Instant::now();
        let mut paths = Vec::new();
        while paths.is_empty() && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
            paths = watcher.changes();
        }
        (paths, start.elapsed())
    }

    fn y(world: &World, part: axcenity_core::InstanceId) -> f32 {
        world
            .get(part)
            .and_then(|part| part.property("Position"))
            .and_then(Value::as_vector3)
            .unwrap()
            .y
    }

    #[test]
    fn with_watch_a_changed_model_and_clip_reload_while_the_game_runs() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-player-watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-door");
        fs::create_dir_all(folder.join("assets/animations/Door")).unwrap();
        fs::copy(example.join(CLIP), folder.join(CLIP)).unwrap();
        let machine = StateMachineFile {
            default_state: "Rise".into(),
            states: vec![StateDef::new("Rise", Motion::Clip(CLIP.into()), [0.0, 0.0])],
            ..StateMachineFile::default()
        };
        fs::write(folder.join(MACHINE), machine.to_json()).unwrap();
        write_triangle(&folder, 1.0);

        // A door rising by the clip, and a MeshPart drawn with the triangle.
        let mut world = World::new();
        let door = world.create(ClassName::Part).unwrap();
        world.set_parent(door, Some(world.workspace())).unwrap();
        let animator = world.create(ClassName::Animator).unwrap();
        world.set_parent(animator, Some(door)).unwrap();
        world
            .set_property(animator, "StateMachine", Value::String(MACHINE.into()))
            .unwrap();
        let assets = Assets::new(&folder);
        let mut animators = Animators::new();
        let mut audio = GameAudio::default();
        assert_eq!(assets.model_blocking(TRIANGLE).unwrap().size[0], 1.0);
        animators.update(&mut world, &assets, 0.0);
        animators.update(&mut world, &assets, 0.75);
        assert!((y(&world, door) - 5.0).abs() < 1e-3);

        let watcher = Watcher::start(folder.clone(), Watched::Assets);
        std::thread::sleep(Duration::from_millis(100));
        write_triangle(&folder, 2.5);
        let clip = fs::read_to_string(folder.join(CLIP)).unwrap();
        fs::write(folder.join(CLIP), clip.replace("8.0", "14.0")).unwrap();
        let (mut paths, took) = changes(&watcher);
        assert!(took < Duration::from_secs(1), "took {took:?}");
        // Both writes usually land in one look; wait for the other if not.
        if paths.len() < 3 {
            paths.extend(changes(&watcher).0);
            paths.sort();
            paths.dedup();
        }
        assert_eq!(paths, [CLIP, "assets/models/tri.bin", TRIANGLE]);

        let lines = reload(&paths, &assets, &mut animators, &mut audio);
        // The buffer comes first and reads the model again, so the model
        // file itself is then as it was read.
        assert_eq!(
            lines,
            [
                format!("Reloaded `{CLIP}`."),
                "Reloaded `assets/models/tri.bin`.".to_owned()
            ]
        );
        assert_eq!(assets.model_blocking(TRIANGLE).unwrap().size[0], 2.5);
        // Halfway through the clip, as before, but on the new one.
        animators.update(&mut world, &assets, 0.0);
        assert_eq!(world.animator(animator).unwrap().current_state, "Rise");
        assert!((y(&world, door) - 8.0).abs() < 1e-3, "{}", y(&world, door));
        drop(watcher);
        let _ = fs::remove_dir_all(&folder);
    }
}
