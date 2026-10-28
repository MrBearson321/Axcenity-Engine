//! Phase 51: Sequence Instances on worlds made here (the hello-cutscene
//! example runs in the player's and the editor's tests).

use std::collections::BTreeMap;
use std::path::PathBuf;

use axcenity_anim::sequence::{
    AudioSection, CameraCut, FadeSection, LevelSection, SequenceFile, SubtitleSection,
};
use axcenity_anim::{ClipEvent, PropertyKeyframe, PropertyName, PropertyTrack};
use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, InstanceId, KeyCode, World};

use super::*;

const FRAME: f64 = 0.1;

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A 2-second sequence: the Door turns 90° over it, the Shot camera is in
/// charge, Music plays, "Knock" passes at 1 s, a fade and a subtitle show,
/// and the Cellar is added from 0.5 s to the end.
fn file() -> SequenceFile {
    let mut file = SequenceFile::new(2.0);
    file.tracks.push(PropertyTrack {
        id: "door".into(),
        target: "Workspace/Door".into(),
        property: PropertyName::Orientation,
        keys: vec![
            PropertyKeyframe::new("a", 0.0, vec![0.0, 0.0, 0.0]),
            PropertyKeyframe::new("b", 2.0, vec![0.0, 90.0, 0.0]),
        ],
    });
    file.camera_cuts.push(CameraCut {
        id: "shot".into(),
        time: 0.0,
        camera: "Workspace/Shot".into(),
    });
    file.audio.push(AudioSection {
        id: "music".into(),
        start: 0.0,
        end: 2.0,
        sound: "Workspace/Music".into(),
    });
    file.events.push(ClipEvent {
        time: 1.0,
        name: "Knock".into(),
    });
    file.fades.push(FadeSection {
        id: "in".into(),
        start: 0.0,
        end: 0.5,
        from: 1.0,
        to: 0.0,
        color: [0.0, 0.0, 0.0],
    });
    file.subtitles.push(SubtitleSection {
        id: "line".into(),
        start: 0.0,
        end: 2.0,
        key: String::new(),
        text: "Who's there?".into(),
    });
    file.levels.push(LevelSection {
        id: "cellar".into(),
        start: 0.5,
        end: 2.0,
        level: "Cellar".into(),
    });
    file
}

struct Game {
    world: World,
    runtime: ScriptRuntime,
    sequence: InstanceId,
    folder: PathBuf,
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn game(test: &str, file: SequenceFile) -> Game {
    let folder =
        std::env::temp_dir().join(format!("axcenity-sequence-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/sequences")).unwrap();
    std::fs::write(
        folder.join("assets/sequences/Intro.sequence.json"),
        file.to_json(),
    )
    .unwrap();
    let mut world = World::new();
    let workspace = world.workspace();
    add(&mut world, ClassName::Part, workspace, "Door");
    let shot = add(&mut world, ClassName::Camera, workspace, "Shot");
    world
        .set_property(
            shot,
            "Position",
            Value::Vector3(Vector3::new(0.0, 5.0, 10.0)),
        )
        .unwrap();
    let music = add(&mut world, ClassName::Sound, workspace, "Music");
    world
        .set_property(music, "SoundId", Value::String("assets/music.ogg".into()))
        .unwrap();
    let sequence = add(&mut world, ClassName::Sequence, workspace, "Intro");
    world
        .set_property(
            sequence,
            "SequenceId",
            Value::String("assets/sequences/Intro.sequence.json".into()),
        )
        .unwrap();
    let mut cellar = World::new();
    let cellar_workspace = cellar.workspace();
    add(&mut cellar, ClassName::Part, cellar_workspace, "Barrel");
    let mut runtime = ScriptRuntime::new();
    runtime.set_assets(Assets::new(&folder));
    runtime.set_levels(
        "Hall",
        "Hall",
        BTreeMap::from([
            ("Hall".to_owned(), world.clone()),
            ("Cellar".to_owned(), cellar),
        ]),
    );
    Game {
        world,
        runtime,
        sequence,
        folder,
    }
}

impl Game {
    fn command(&mut self, source: &str) {
        self.runtime.run_command(&mut self.world, source).unwrap();
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.runtime.input_events(&mut self.world, &[]);
            self.runtime.step(&mut self.world, FRAME);
        }
    }

    fn printed(&mut self) -> Vec<String> {
        self.runtime
            .take_output()
            .into_iter()
            .map(|line| match line.kind {
                OutputKind::Error => format!("error: {}", line.text),
                OutputKind::Warning => format!("warning: {}", line.text),
                OutputKind::Print => line.text,
            })
            .collect()
    }

    fn door_turn(&self) -> f32 {
        let door = self
            .world
            .find_first_child(self.world.workspace(), "Door")
            .unwrap();
        let instance = self.world.get(door).unwrap();
        instance
            .property("Orientation")
            .unwrap()
            .as_vector3()
            .unwrap()
            .y
    }

    fn has(&self, name: &str) -> bool {
        self.world
            .find_first_child(self.world.workspace(), name)
            .is_some()
    }

    fn press(&mut self, key: KeyCode, pressed: bool) {
        let event = InputEvent::new(InputKind::Key { key, pressed });
        self.runtime.input_events(&mut self.world, &[event]);
    }

    fn set(&mut self, name: &str, value: bool) {
        self.world
            .set_property(self.sequence, name, Value::Bool(value))
            .unwrap();
    }
}

const LISTEN: &str = "local intro = workspace.Intro\n\
    intro.EventReached:Connect(function(name) print('event', name, string.format('%.1f', intro.TimePosition)) end)\n\
    intro.Completed:Connect(function() print('completed', intro.TimePosition, intro.IsPlaying) end)\n";

#[test]
fn play_keys_the_world_holds_the_camera_and_fires_events_then_completed_once() {
    let mut game = game("play", file());
    game.command(&format!(
        "{LISTEN}print(intro.Length, intro.IsPlaying)\nintro:Play()"
    ));
    assert_eq!(game.printed(), ["2\tfalse"]);
    game.frames(5);
    assert!(
        (game.door_turn() - 22.5).abs() < 0.01,
        "{}",
        game.door_turn()
    );
    // The Shot camera holds the view; the music and the Cellar are in.
    assert!(game.runtime.camera_scripted());
    let camera = game.runtime.step_camera(&game.world, 0.0);
    assert_eq!(
        camera.scripted.unwrap().position,
        Vector3::new(0.0, 5.0, 10.0)
    );
    assert!(matches!(
        game.runtime.take_sound_actions()[..],
        [SoundAction::Play { .. }]
    ));
    assert!(game.has("Cellar"));
    assert!(game.has(axcenity_anim::sequence::OVERLAY_NAME));
    assert!(game.printed().is_empty());

    game.frames(6);
    assert_eq!(game.printed(), ["event\tKnock\t1.1"]);
    game.frames(20);
    assert_eq!(game.printed(), ["completed\t2\tfalse"]);
    assert_eq!(game.door_turn(), 90.0);
    // Let go of: the camera is the host's again, the music stopped, the
    // overlay gone; the Cellar, added to the end, stays.
    assert!(!game.runtime.camera_scripted());
    assert!(matches!(
        game.runtime.take_sound_actions()[..],
        [SoundAction::Stop(_)]
    ));
    assert!(!game.has(axcenity_anim::sequence::OVERLAY_NAME));
    assert!(game.has("Cellar"));
    game.frames(5);
    assert!(game.printed().is_empty(), "Completed fires once");
}

#[test]
fn the_menu_action_skips_to_the_end_state_and_completed_fires_once() {
    let mut game = game("skip", file());
    game.command(&format!("{LISTEN}intro:Play()"));
    game.frames(3);
    assert!(game.door_turn() < 20.0);
    game.press(KeyCode::Escape, true);
    assert_eq!(game.door_turn(), 90.0);
    assert_eq!(
        game.printed(),
        ["completed\t2\tfalse"],
        "no events skipped over"
    );
    assert!(!game.has(axcenity_anim::sequence::OVERLAY_NAME));
    assert!(!game.runtime.camera_scripted());
    game.press(KeyCode::Escape, false);
    game.frames(30);
    assert!(game.printed().is_empty(), "Completed fires once");
    assert_eq!(game.door_turn(), 90.0);
}

#[test]
fn a_sequence_that_isnt_skippable_ignores_the_menu_action() {
    let mut game = game("unskippable", file());
    game.set("Skippable", false);
    game.command(&format!("{LISTEN}intro:Play()"));
    game.frames(3);
    game.press(KeyCode::Escape, true);
    assert!(game.door_turn() < 20.0);
    assert!(game.printed().is_empty());
}

#[test]
fn input_is_locked_while_it_plays_and_comes_back_after() {
    let mut game = game("lock", file());
    let script = game.world.create(ClassName::LocalScript).unwrap();
    game.world
        .set_parent(script, Some(game.world.starter_player_scripts()))
        .unwrap();
    game.runtime.start_script(
        &mut game.world,
        script,
        "keys.client.lua",
        "game:GetService('UserInputService').InputBegan:Connect(function(input) print('began', input.KeyCode.Name) end)\n\
         game:GetService('UserInputService').InputEnded:Connect(function(input) print('ended', input.KeyCode.Name) end)",
    );
    game.press(KeyCode::E, true);
    assert_eq!(game.printed(), ["began\tE"]);
    game.command("workspace.Intro:Play()");
    game.frames(1);
    assert!(game.runtime.input_locked(&game.world));
    game.press(KeyCode::Q, true);
    assert!(game.printed().is_empty(), "presses don't get through");
    // A release does, so nothing stays held.
    game.press(KeyCode::E, false);
    assert_eq!(game.printed(), ["ended\tE"]);
    game.frames(25);
    assert!(!game.runtime.input_locked(&game.world));
    game.press(KeyCode::Q, true);
    assert_eq!(game.printed(), ["began\tQ"]);

    // Without LockPlayerInput, input gets through while it plays.
    game.set("LockPlayerInput", false);
    game.command("workspace.Intro:Play()");
    game.frames(1);
    assert!(!game.runtime.input_locked(&game.world));
}

#[test]
fn pause_stop_and_time_position() {
    let mut game = game("pause", file());
    game.command("workspace.Intro:Play()");
    game.frames(5);
    game.command("workspace.Intro:Pause() print(workspace.Intro.IsPlaying)");
    let turned = game.door_turn();
    game.frames(5);
    assert_eq!(game.door_turn(), turned);
    assert_eq!(game.printed(), ["false"]);
    assert!(
        game.runtime.input_locked(&game.world),
        "paused still holds input"
    );

    // Jumping applies at the next step, without the events between.
    game.command(&format!("{LISTEN}workspace.Intro.TimePosition = 1.5"));
    game.frames(1);
    assert!((game.door_turn() - 67.5).abs() < 0.01);
    assert!(game.printed().is_empty());

    // Stop lets go of everything it added (the Cellar too) and goes back.
    game.command("workspace.Intro:Stop() print(workspace.Intro.TimePosition)");
    game.frames(1);
    assert_eq!(game.printed(), ["0"]);
    assert!(!game.has("Cellar"));
    assert!(!game.has(axcenity_anim::sequence::OVERLAY_NAME));
    assert!(!game.runtime.camera_scripted());
    assert!(!game.runtime.input_locked(&game.world));
    // Playing again starts from the start.
    game.command("workspace.Intro:Play()");
    game.frames(1);
    assert!(game.door_turn() < 5.0);
}

#[test]
fn auto_play_starts_with_the_game_and_a_bad_file_says_why() {
    let mut game = game("auto", file());
    game.set("AutoPlay", true);
    game.frames(5);
    assert!(game.door_turn() > 0.0);

    game.world
        .set_property(
            game.sequence,
            "SequenceId",
            Value::String("assets/sequences/Missing.sequence.json".into()),
        )
        .unwrap();
    let id = game.sequence;
    let copy = game.world.copy_from(&game.world.clone(), id).unwrap();
    game.world
        .set_parent(copy, Some(game.world.workspace()))
        .unwrap();
    game.world.set_name(copy, "Broken").unwrap();
    game.printed();
    let _ = game
        .runtime
        .run_command(&mut game.world, "workspace.Broken:Play()");
    let printed = game.printed();
    assert!(
        printed
            .iter()
            .any(|line| line.starts_with("error") && line.contains("Missing.sequence.json")),
        "{printed:?}"
    );
}
