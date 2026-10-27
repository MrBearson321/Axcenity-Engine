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

use anyhow::{Result, bail};
use axcenity_anim::Animators;
use axcenity_assets::{Assets, Reload};
use axcenity_audio::GameAudio;
use axcenity_project::{Watched, Watcher};

use crate::USAGE;

/// The flag that turns live reload on.
pub const WATCH_FLAG: &str = "--watch";

/// What the player was started with.
#[derive(Debug, Default, PartialEq)]
pub struct Arguments {
    /// A project folder or a `.axgame` file; `None` for the game bundled
    /// next to the player.
    pub game: Option<PathBuf>,
    /// `--watch` was given.
    pub watch: bool,
}

impl Arguments {
    /// Reads the arguments after the program's name. `--watch` can come
    /// before or after the game.
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments> {
        let mut parsed = Arguments::default();
        for argument in arguments {
            if argument == WATCH_FLAG {
                parsed.watch = true;
            } else if argument.to_string_lossy().starts_with("--") {
                bail!("unknown option `{}`\n\n{USAGE}", argument.to_string_lossy());
            } else if parsed.game.is_none() {
                parsed.game = Some(PathBuf::from(argument));
            } else {
                bail!("only one game can be played at a time\n\n{USAGE}");
            }
        }
        Ok(parsed)
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
        assert_eq!(assets.model(TRIANGLE).unwrap().size[0], 1.0);
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
        assert_eq!(assets.model(TRIANGLE).unwrap().size[0], 2.5);
        // Halfway through the clip, as before, but on the new one.
        animators.update(&mut world, &assets, 0.0);
        assert_eq!(world.animator(animator).unwrap().current_state, "Rise");
        assert!((y(&world, door) - 8.0).abs() < 1e-3, "{}", y(&world, door));
        drop(watcher);
        let _ = fs::remove_dir_all(&folder);
    }
}
