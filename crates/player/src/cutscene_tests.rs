//! Phase 51: the hello-cutscene example in the player's frame order, from a
//! built game: its 20-second Intro sequence plays as the editor's preview
//! shows it (the same `SequenceRun`, applied to a copy of the world and
//! posed), skipping jumps to its end, and input is locked while it plays.

use std::path::{Path, PathBuf};

use axcenity_anim::sequence::{OVERLAY_NAME, SequenceRun, Shot, load_sequence};
use axcenity_core::{InstanceId, KeyCode, Value, Vector3};

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;
const SEQUENCE: &str = "assets/sequences/Intro.sequence.json";

/// hello-cutscene built with Build Game and unpacked, opened as the
/// player opens a game.
fn built_game(test: &str) -> (App, World, PathBuf) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-cutscene");
    let temp = |what: &str| {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-player-cutscene-{test}-{what}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        folder
    };
    let source = temp("source");
    copy_folder(&example, &source);
    let packed = axcenity_project::pack_game(&source).unwrap();
    let folder = temp("built");
    axcenity_project::unpack_game(&packed, &folder).unwrap();
    let _ = fs::remove_dir_all(&source);
    let project = axcenity_project::load(&folder).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let start = project.world.clone();
    let mut world = project.world;
    let assets = Assets::new(&folder);
    let mut scripts = ScriptRuntime::new();
    scripts.set_assets(assets.clone());
    scripts.set_input_map(project.input_map.clone());
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let mut app = App::new(world, scripts, Vec::new(), assets, Animators::new(), camera);
    app.physics = Physics::with_assets(app.assets.clone());
    (app, start, folder)
}

impl App {
    /// One frame in the frame loop's order: input and characters, scripts
    /// (and the sequences after them), physics, animation, then the camera.
    fn cutscene_frame(&mut self) {
        let seconds = f64::from(FRAME);
        let motion = self.controller_input(FRAME);
        self.characters
            .step(&mut self.world, &self.assets, motion, FRAME);
        self.scripts
            .step_humanoids(&mut self.world, &self.characters, seconds);
        self.scripts.step_players(&mut self.world, seconds);
        let events = self.input.take_events(self.pointer);
        self.scripts.input_events(&mut self.world, &events);
        self.scripts.step_prompts(&mut self.world, seconds);
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        self.scripts.heartbeat(&mut self.world, seconds);
        let _ = self
            .animators
            .update(&mut self.world, &self.assets, seconds);
        assert!(self.animators.take_errors().is_empty());
        self.game_camera = self.scripts.step_camera(&self.world, seconds);
    }

    fn printed(&mut self) -> Vec<String> {
        self.scripts
            .take_output()
            .into_iter()
            .inspect(|line| {
                assert!(!matches!(line.kind, OutputKind::Error), "{}", line.text);
            })
            .map(|line| line.text)
            .collect()
    }

    fn character_spot(&self) -> Vector3 {
        let player = self.scripts.local_player().expect("the player joined");
        let character = match self.world.get(player).unwrap().property("Character") {
            Some(Value::Instance(Some(character))) => *character,
            _ => panic!("the player has no character"),
        };
        let parts = axcenity_character::body_parts(&self.world, character);
        vector(&self.world, parts[0], "Position")
    }
}

/// What the editor's preview shows at `time`: a copy of the world at the
/// start, with the sequence applied and posed.
fn preview_at(start: &World, assets: &Assets, time: f64) -> (World, Option<Shot>) {
    let mut world = start.clone();
    let mut run = SequenceRun::new(load_sequence(assets, SEQUENCE).unwrap()).unwrap();
    let mut animators = Animators::new();
    let applied = run.apply(&mut world, time);
    animators.update(&mut world, assets, 0.0);
    assert!(run.missing.is_empty(), "{:?}", run.missing);
    (world, applied.camera)
}

fn find(world: &World, path: &[&str]) -> InstanceId {
    let mut at = world.workspace();
    for name in path {
        at = world
            .find_first_child(at, name)
            .unwrap_or_else(|| panic!("no {name}"));
    }
    at
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn close(a: Vector3, b: Vector3, what: &str) {
    let gap = (a.x - b.x)
        .abs()
        .max((a.y - b.y).abs())
        .max((a.z - b.z).abs());
    assert!(gap < 0.01, "{what}: {a:?} vs {b:?}");
}

/// What's shown of the overlay: the subtitle's key, if one shows, and
/// whether the bars and the fade show.
fn overlay(world: &World) -> Option<(String, bool, bool)> {
    let gui = world.find_first_child(world.workspace(), OVERLAY_NAME)?;
    let shown = |name| {
        let id = world.find_first_child(gui, name).unwrap();
        world.get(id).unwrap().property("Visible") == Some(&Value::Bool(true))
    };
    let label = world.find_first_child(gui, "Subtitle").unwrap();
    let key = match world.get(label).unwrap().property("TextKey") {
        Some(Value::String(key)) if shown("Subtitle") => key.clone(),
        _ => String::new(),
    };
    Some((key, shown("TopBar"), shown("Fade")))
}

fn playing(world: &World, path: &[&str]) -> bool {
    world.get(find(world, path)).unwrap().property("Playing") == Some(&Value::Bool(true))
}

#[test]
fn the_cutscene_plays_in_a_built_game_as_the_preview_shows_it() {
    let (mut app, start, folder) = built_game("plays");
    let mut printed = Vec::new();
    // Frames at each stretch: fading in with the wide shot, the first
    // subtitle, the door shot as the door opens and the second subtitle,
    // the sequence's own camera, and the fade out.
    let samples = [30, 240, 450, 600, 840, 1000, 1150];
    let mut torso = Vec::new();
    let mut swing: f32 = 0.0;
    for frame in 1..=1200 {
        app.cutscene_frame();
        printed.extend(app.printed());
        if !samples.contains(&frame) {
            continue;
        }
        let time = f64::from(frame) * f64::from(FRAME);
        let (preview, shot) = preview_at(&start, &app.assets, time);
        torso.push(vector(
            &app.world,
            find(&app.world, &["Hero", "Torso"]),
            "Position",
        ));
        let leg = vector(
            &app.world,
            find(&app.world, &["Hero", "LeftLeg"]),
            "Orientation",
        );
        swing = swing.max(leg.x.abs());
        for path in [
            &["Door"][..],
            &["Hero", "Torso"],
            &["Hero", "Head"],
            &["Hero", "LeftLeg"],
        ] {
            for property in ["Position", "Orientation"] {
                close(
                    vector(&app.world, find(&app.world, path), property),
                    vector(&preview, find(&preview, path), property),
                    &format!("{path:?} {property} at {time:.2}s"),
                );
            }
        }
        // The same camera cut, from the same place, with the same lens.
        let shot = shot.expect("a camera cut is in charge");
        let held = app
            .game_camera
            .scripted
            .expect("the sequence holds the camera");
        close(held.position, shot.position, "camera position");
        close(held.orientation, shot.orientation, "camera orientation");
        assert!((app.game_camera.field_of_view - shot.field_of_view).abs() < 0.01);
        assert_eq!(overlay(&app.world), overlay(&preview), "at {time:.2}s");
        assert!(playing(&app.world, &["Music"]), "the music plays");
    }
    // The hero walked from its start to the door, swinging its legs.
    // Its pivot moves from (-10, 2.6, 4) to (0, 2.6, -6) over 12 s.
    close(
        torso[0],
        Vector3::new(-9.583, 3.0, 3.583),
        "the hero at 0.5 s",
    );
    close(
        torso[3],
        Vector3::new(-1.667, 3.0, -4.333),
        "the hero at 10 s",
    );
    assert!(swing > 10.0, "the walk clip swings the legs: {swing}");
    // Two subtitles, three cuts and the door all showed along the way.
    let (_, wide) = preview_at(&start, &app.assets, 1.0);
    let (_, door) = preview_at(&start, &app.assets, 10.0);
    let (_, own) = preview_at(&start, &app.assets, 15.0);
    assert_eq!(wide.unwrap().position, Vector3::new(0.0, 8.0, 20.0));
    assert_eq!(door.unwrap().position, Vector3::new(10.0, 5.0, 4.0));
    assert!(own.unwrap().position.z < 6.0 && own.unwrap().position.x > -7.0);
    let (at_four, _) = preview_at(&start, &app.assets, 4.0);
    let (at_ten, _) = preview_at(&start, &app.assets, 10.0);
    assert_eq!(overlay(&at_four).unwrap().0, "intro.line1");
    assert_eq!(overlay(&at_ten).unwrap().0, "intro.line2");

    assert_eq!(
        printed,
        [
            "cutscene event\tDoorOpens",
            "cutscene event\tArrived",
            "cutscene finished: walk around"
        ]
    );
    // Over: the door is open, the overlay and the music gone, and the
    // camera is the character's again.
    close(
        vector(&app.world, find(&app.world, &["Door"]), "Position"),
        Vector3::new(4.2, 3.0, -9.4),
        "the open door",
    );
    assert!(overlay(&app.world).is_none());
    assert!(!playing(&app.world, &["Music"]));
    assert!(app.game_camera.scripted.is_none());
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn skipping_jumps_to_the_end_state_and_completed_fires_once() {
    let (mut app, start, folder) = built_game("skip");
    for _ in 0..120 {
        app.cutscene_frame();
    }
    assert!(app.printed().is_empty());
    app.push_key(KeyCode::Escape, true, false);
    app.cutscene_frame();
    app.push_key(KeyCode::Escape, false, false);
    for _ in 0..120 {
        app.cutscene_frame();
    }
    assert_eq!(app.printed(), ["cutscene finished: walk around"]);
    // Everything it keys is where the sequence leaves it at its end.
    let (end, _) = preview_at(&start, &app.assets, 20.0);
    for path in [&["Door"][..], &["Hero", "Torso"], &["Hero", "Head"]] {
        close(
            vector(&app.world, find(&app.world, path), "Position"),
            vector(&end, find(&end, path), "Position"),
            &format!("{path:?} at the end"),
        );
    }
    assert!(overlay(&app.world).is_none());
    assert!(!playing(&app.world, &["Music"]));
    assert!(app.game_camera.scripted.is_none());
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn input_is_locked_while_it_plays_and_control_comes_back_after() {
    let (mut app, _, folder) = built_game("lock");
    // Let the character land on the spawn, then hold W through it all.
    for _ in 0..60 {
        app.cutscene_frame();
    }
    let landed = app.character_spot();
    app.push_key(KeyCode::W, true, false);
    for _ in 0..600 {
        app.cutscene_frame();
        assert!(app.scripts.input_locked(&app.world));
    }
    let held = app.character_spot();
    assert!(
        (held.x - landed.x).abs() < 0.05 && (held.z - landed.z).abs() < 0.05,
        "the character stays put while it plays: {landed:?} → {held:?}"
    );
    for _ in 0..560 {
        app.cutscene_frame();
    }
    assert!(!app.scripts.input_locked(&app.world), "it has ended");
    for _ in 0..60 {
        app.cutscene_frame();
    }
    let walked = app.character_spot();
    let moved = ((walked.x - held.x).powi(2) + (walked.z - held.z).powi(2)).sqrt();
    assert!(moved > 2.0, "W walks it again after: {held:?} → {walked:?}");
    let _ = fs::remove_dir_all(folder);
}
