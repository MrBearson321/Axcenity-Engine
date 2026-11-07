use super::*;
use crate::files::PropertyKeyframe;

fn key(id: &str, time: f64, value: &[f32]) -> PropertyKeyframe {
    PropertyKeyframe::new(id, time, value.to_vec())
}

fn track(
    id: &str,
    target: &str,
    property: PropertyName,
    keys: Vec<PropertyKeyframe>,
) -> PropertyTrack {
    PropertyTrack {
        id: id.to_owned(),
        target: target.to_owned(),
        property,
        keys,
    }
}

/// A world with a Door part, a Camera named Shot, a tagged Hero Model with
/// an Animator, and a Sound.
fn world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    let make = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    make(&mut world, ClassName::Part, "Door", workspace);
    let shot = make(&mut world, ClassName::Camera, "Shot", workspace);
    world
        .set_property(
            shot,
            "Position",
            Value::Vector3(Vector3::new(1.0, 2.0, 3.0)),
        )
        .unwrap();
    world
        .set_property(shot, "FieldOfView", Value::Number(30.0))
        .unwrap();
    let hero = make(&mut world, ClassName::Model, "Hero", workspace);
    world.add_tag(hero, "Hero").unwrap();
    make(&mut world, ClassName::Animator, "Animator", hero);
    make(&mut world, ClassName::Sound, "Music", workspace);
    world
}

fn sequence() -> SequenceFile {
    let mut file = SequenceFile::new(10.0);
    file.tracks.push(track(
        "door",
        "Workspace/Door",
        PropertyName::Orientation,
        vec![
            key("a", 2.0, &[0.0, 0.0, 0.0]),
            key("b", 4.0, &[0.0, 90.0, 0.0]),
        ],
    ));
    file.tracks.push(track(
        "camera",
        CAMERA_TARGET,
        PropertyName::Position,
        vec![
            key("a", 5.0, &[0.0, 10.0, 0.0]),
            key("b", 10.0, &[0.0, 20.0, 0.0]),
        ],
    ));
    file.camera_cuts = vec![
        CameraCut {
            id: "c1".into(),
            time: 0.0,
            camera: "Workspace/Shot".into(),
        },
        CameraCut {
            id: "c2".into(),
            time: 5.0,
            camera: String::new(),
        },
    ];
    file.animations.push(AnimationSection {
        id: "walk".into(),
        start: 1.0,
        end: 9.0,
        target: "tag:Hero".into(),
        clip: "assets/animations/Walk.anim.json".into(),
        blend_in: 1.0,
        blend_out: 0.0,
    });
    file.audio.push(AudioSection {
        id: "music".into(),
        start: 0.0,
        end: 8.0,
        sound: "Workspace/Music".into(),
    });
    file.events.push(ClipEvent {
        time: 3.0,
        name: "Knock".into(),
    });
    file.fades.push(FadeSection {
        id: "in".into(),
        start: 0.0,
        end: 1.0,
        from: 1.0,
        to: 0.0,
        color: black(),
    });
    file.letterbox.push(LetterboxSection {
        id: "bars".into(),
        start: 0.0,
        end: 10.0,
        size: 0.1,
        ramp: 1.0,
    });
    file.subtitles.push(SubtitleSection {
        id: "line".into(),
        start: 2.0,
        end: 4.0,
        key: "Intro.Line1".into(),
        text: "Hello".into(),
    });
    file.levels.push(LevelSection {
        id: "cellar".into(),
        start: 6.0,
        end: 10.0,
        level: "Cellar".into(),
    });
    file
}

fn property(world: &World, path: &str, name: &str) -> Value {
    let id = resolve_binding(world, path).unwrap();
    world.get(id).unwrap().property(name).unwrap().clone()
}

#[test]
fn a_sequence_file_round_trips_and_bad_sections_say_why() {
    let file = sequence();
    let text = file.to_json();
    assert_eq!(SequenceFile::from_json(&text).unwrap(), file);
    assert_eq!(SequenceFile::from_json(&text).unwrap().to_json(), text);

    let mut late = sequence();
    late.subtitles[0].end = 12.0;
    let err = SequenceFile::from_json(&late.to_json()).unwrap_err();
    assert!(err.0.contains("subtitle `line`"), "{}", err.0);
    let err =
        SequenceFile::from_json(&text.replace("\"format_version\": 1", "\"format_version\": 9"))
            .unwrap_err();
    assert!(err.0.contains("format 9"), "{}", err.0);
}

#[test]
fn bindings_find_instances_by_path_or_tag() {
    let world = world();
    let hero = resolve_binding(&world, "tag:Hero").unwrap();
    assert_eq!(world.get(hero).unwrap().name(), "Hero");
    assert_eq!(resolve_binding(&world, "Workspace/Hero"), Some(hero));
    assert_eq!(
        binding_path(&world, hero).as_deref(),
        Some("Workspace/Hero")
    );
    assert_eq!(resolve_binding(&world, "tag:Nobody"), None);
    assert_eq!(resolve_binding(&world, CAMERA_TARGET), None);
}

#[test]
fn applying_a_time_sets_properties_clips_overlay_camera_and_levels() {
    let mut world = world();
    let mut run = SequenceRun::new(sequence()).unwrap();

    let at_three = run.apply(&mut world, 3.0);
    assert_eq!(
        property(&world, "Workspace/Door", "Orientation")
            .as_vector3()
            .unwrap()
            .y
            .round(),
        45.0
    );
    // The first cut is the Shot camera's.
    let shot = at_three.camera.unwrap();
    assert_eq!(shot.position, Vector3::new(1.0, 2.0, 3.0));
    assert_eq!(shot.field_of_view, 30.0);
    assert_eq!(
        at_three.sounds,
        vec![SoundCue::Start(
            resolve_binding(&world, "Workspace/Music").unwrap()
        )]
    );
    assert!(at_three.levels.is_empty());
    // The walk is loaded on the Hero's Animator, held at the sequence's time.
    let animator = world
        .find_first_child(resolve_binding(&world, "tag:Hero").unwrap(), "Animator")
        .unwrap();
    let track = *world.animator(animator).unwrap().tracks.last().unwrap();
    let state = world.animation_track(track).unwrap();
    assert!(state.playing);
    assert_eq!((state.time, state.speed, state.weight), (2.0, 0.0, 1.0));
    // The overlay shows the subtitle and the bars, and the fade is over.
    let overlay = world
        .find_first_child(world.workspace(), OVERLAY_NAME)
        .unwrap();
    let subtitle = world.find_first_child(overlay, "Subtitle").unwrap();
    assert_eq!(
        world.get(subtitle).unwrap().property("TextKey"),
        Some(&Value::String("Intro.Line1".into()))
    );
    assert_eq!(
        world.get(subtitle).unwrap().property("Visible"),
        Some(&Value::Bool(true))
    );
    let fade = world.find_first_child(overlay, "Fade").unwrap();
    assert_eq!(
        world.get(fade).unwrap().property("Visible"),
        Some(&Value::Bool(false))
    );

    // At 7.5 the sequence's own camera keys are in charge, the Cellar is
    // added, and nothing new starts.
    let later = run.apply(&mut world, 7.5);
    let own = later.camera.unwrap();
    assert_eq!(own.position, Vector3::new(0.0, 15.0, 0.0));
    assert_eq!(own.field_of_view, 50.0);
    assert_eq!(later.levels, vec!["Cellar".to_owned()]);
    assert!(later.sounds.is_empty());
    // Past the music's end it stops; the subtitle is gone.
    let end = run.apply(&mut world, 10.0);
    assert_eq!(
        end.sounds,
        vec![SoundCue::Stop(
            resolve_binding(&world, "Workspace/Music").unwrap()
        )]
    );
    assert_eq!(
        end.levels,
        vec!["Cellar".to_owned()],
        "a level section to the end stays"
    );
    assert_eq!(
        world.get(subtitle).unwrap().property("Visible"),
        Some(&Value::Bool(false))
    );
    assert!(run.missing.is_empty(), "{:?}", run.missing);

    // Going back is the same as getting there first.
    run.apply(&mut world, 3.0);
    assert_eq!(
        property(&world, "Workspace/Door", "Orientation")
            .as_vector3()
            .unwrap()
            .y
            .round(),
        45.0
    );

    run.finish(&mut world);
    assert!(
        world
            .find_first_child(world.workspace(), OVERLAY_NAME)
            .is_none()
    );
    assert!(!world.animation_track(track).unwrap().playing);
}

#[test]
fn a_clip_blends_in_over_its_blend_time() {
    let mut world = world();
    let mut run = SequenceRun::new(sequence()).unwrap();
    run.apply(&mut world, 1.5);
    let animator = world
        .find_first_child(resolve_binding(&world, "tag:Hero").unwrap(), "Animator")
        .unwrap();
    let track = *world.animator(animator).unwrap().tracks.last().unwrap();
    assert!((world.animation_track(track).unwrap().weight - 0.5).abs() < 1e-9);
    // Before the section nothing was loaded; after it the track lets go.
    run.apply(&mut world, 9.5);
    assert!(!world.animation_track(track).unwrap().playing);
}

#[test]
fn fades_hold_their_end_and_events_pass_in_order() {
    let mut file = sequence();
    file.fades.push(FadeSection {
        id: "out".into(),
        start: 8.0,
        end: 9.0,
        from: 0.0,
        to: 1.0,
        color: [1.0, 1.0, 1.0],
    });
    assert_eq!(file.fade_at(0.5).0, 0.5);
    assert_eq!(file.fade_at(5.0).0, 0.0);
    assert_eq!(file.fade_at(9.5), (1.0, [1.0, 1.0, 1.0]));
    assert!((file.letterbox_at(0.5) - 0.05).abs() < 1e-6);
    assert_eq!(file.letterbox_at(5.0), 0.1);

    file.events.push(ClipEvent {
        time: 10.0,
        name: "End".into(),
    });
    assert_eq!(file.events_between(0.0, 3.0), Vec::<String>::new());
    assert_eq!(file.events_between(3.0, 10.0), vec!["Knock", "End"]);
}

#[test]
fn missing_bindings_are_reported() {
    let mut world = World::new();
    let mut run = SequenceRun::new(sequence()).unwrap();
    let applied = run.apply(&mut world, 3.0);
    assert!(applied.camera.is_none());
    for binding in [
        "Workspace/Door",
        "tag:Hero",
        "Workspace/Music",
        "Workspace/Shot",
    ] {
        assert!(
            run.missing.contains(binding),
            "{binding}: {:?}",
            run.missing
        );
    }
}

#[test]
fn the_example_sequence_and_walk_clip_are_canonical() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-cutscene/assets");
    let text = std::fs::read_to_string(example.join("sequences/Intro.sequence.json"))
        .unwrap()
        .replace("\r\n", "\n");
    let file = SequenceFile::from_json(&text).unwrap();
    assert_eq!(file.to_json(), text);
    assert_eq!(file.duration, 20.0);
    assert_eq!(file.camera_cuts.len(), 3);
    assert_eq!(file.subtitles.len(), 2);
    let clip = std::fs::read_to_string(example.join("animations/Hero/Walk.anim.json"))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(ClipFile::from_json(&clip).unwrap().to_json(), clip);
}

#[test]
fn a_camera_cut_bound_to_a_spline_rides_it_from_the_cut_to_the_next() {
    let mut world = World::new();
    let rail = world.create(ClassName::Spline).unwrap();
    world.set_name(rail, "Rail").unwrap();
    world.set_parent(rail, Some(world.workspace())).unwrap();
    for x in [0.0, 20.0] {
        let point = world.create(ClassName::SplinePoint).unwrap();
        world.set_parent(point, Some(rail)).unwrap();
        world
            .set_property(point, "Position", Value::Vector3(Vector3::new(x, 5.0, 0.0)))
            .unwrap();
    }
    let mut file = SequenceFile::new(10.0);
    file.camera_cuts = vec![
        CameraCut {
            id: "ride".into(),
            time: 2.0,
            camera: "Workspace/Rail".into(),
        },
        CameraCut {
            id: "own".into(),
            time: 6.0,
            camera: String::new(),
        },
    ];
    let mut run = SequenceRun::new(file).unwrap();
    // From 2 to 6 it rides the 20 studs: halfway along at 4.
    let shot = |run: &mut SequenceRun, world: &mut World, time| {
        run.apply(world, time).camera.expect("a shot")
    };
    let start = shot(&mut run, &mut world, 2.0);
    assert!((start.position.x - 0.0).abs() < 1e-3);
    let middle = shot(&mut run, &mut world, 4.0);
    assert!((middle.position.x - 10.0).abs() < 1e-2, "{middle:?}");
    assert_eq!(middle.position.y, 5.0);
    // Facing +X, along the rail.
    assert!((middle.orientation.y + 90.0).abs() < 1e-2, "{middle:?}");
    let end = shot(&mut run, &mut world, 5.999);
    assert!((end.position.x - 20.0).abs() < 0.05, "{end:?}");
}
