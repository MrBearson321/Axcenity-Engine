//! Animators running the `hello-animation` example's noodle, and doors,
//! swords and tracks on top.

use std::path::{Path, PathBuf};

use axcenity_assets::{Assets, Posed, PosedPrimitive};
use axcenity_core::{ClassName, InstanceId, PlayRequest, Value, Vector3, World};

use crate::{AnimatorEvent, Animators, ClipFile, StateMachineFile};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation")
}

/// The noodle's corners in `posed`, at full detail.
fn corners(assets: &Assets, posed: &Posed) -> Vec<PosedPrimitive> {
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    posed.primitives(&model).into_iter().cloned().collect()
}

/// A world with the example's noodle: a MeshPart with an Animator.
fn noodle(state_machine: &str) -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let part = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(
            part,
            "MeshId",
            Value::String("assets/models/noodle.fbx".into()),
        )
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 3.8, 1.0)))
        .unwrap();
    let workspace = world.workspace();
    world.set_parent(part, Some(workspace)).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world
        .set_property(
            animator,
            "StateMachine",
            Value::String(state_machine.into()),
        )
        .unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    (world, part, animator)
}

const STATE_MACHINE: &str = "assets/animations/noodle/noodle.statemachine.json";

#[test]
fn an_animator_starts_in_its_default_state_and_poses_its_part() {
    let (mut world, part, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    let events = animators.update(&mut world, &assets, 0.0);
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    assert_eq!(
        world.animator(animator).unwrap().current_state,
        "Locomotion"
    );
    assert_eq!(
        events,
        [AnimatorEvent::StateChanged {
            animator,
            state: "Locomotion".into(),
            previous: String::new(),
        }]
    );
    let first = animators.poses()[&part].clone();
    animators.update(&mut world, &assets, 0.5);
    let later = animators.poses()[&part].clone();
    assert_ne!(first.version, later.version);
    assert_ne!(
        corners(&assets, &first),
        corners(&assets, &later),
        "Idle (Speed 0) sways the noodle"
    );
}

#[test]
fn play_switches_state_and_reports_the_change() {
    let (mut world, _, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    world.animator_mut(animator).unwrap().play = Some(PlayRequest {
        state: "Wave".into(),
        blend: Some(0.2),
    });
    let events = animators.update(&mut world, &assets, 1.0 / 60.0);
    assert_eq!(
        events,
        [AnimatorEvent::StateChanged {
            animator,
            state: "Wave".into(),
            previous: "Locomotion".into(),
        }]
    );
    world.animator_mut(animator).unwrap().play = Some(PlayRequest {
        state: "Dance".into(),
        blend: None,
    });
    animators.update(&mut world, &assets, 1.0 / 60.0);
    let errors = animators.take_errors();
    assert!(errors[0].contains("no state with that name"), "{errors:?}");
    assert_eq!(world.animator(animator).unwrap().current_state, "Wave");
}

#[test]
fn parameters_drive_the_example_through_its_transitions() {
    use axcenity_core::Parameter;
    let (mut world, _, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    let state = |world: &World| world.animator(animator).unwrap().current_state.clone();
    // The controller's parameters arrive at their defaults.
    assert_eq!(
        world.animator(animator).unwrap().parameters["Speed"],
        Parameter::Number(0.0)
    );
    let set = |world: &mut World, name: &str, value| {
        world
            .animator_mut(animator)
            .unwrap()
            .parameters
            .insert(name.into(), value);
    };
    // Speed moves the Locomotion blend tree, not the state.
    set(&mut world, "Speed", Parameter::Number(4.0));
    animators.update(&mut world, &assets, 0.1);
    assert_eq!(state(&world), "Locomotion");
    set(&mut world, "Wave", Parameter::Trigger(true));
    animators.update(&mut world, &assets, 0.1);
    assert_eq!(state(&world), "Wave");
    assert_eq!(
        world.animator(animator).unwrap().parameters["Wave"],
        Parameter::Trigger(false),
        "used up"
    );
    // Wave (1 s) goes back to Locomotion by itself.
    for _ in 0..12 {
        animators.update(&mut world, &assets, 0.1);
    }
    assert_eq!(state(&world), "Locomotion");
    assert_eq!(animators.take_errors(), Vec::<String>::new());
}

#[test]
fn a_parameter_the_state_machine_lacks_is_reported() {
    let (mut world, _, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    world
        .animator_mut(animator)
        .unwrap()
        .parameters
        .insert("IsFlying".into(), axcenity_core::Parameter::Bool(true));
    animators.update(&mut world, &assets, 0.1);
    animators.update(&mut world, &assets, 0.1);
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("IsFlying"));
}

#[test]
fn a_missing_state_machine_is_reported_once_and_leaves_the_model_still() {
    let (mut world, part, _) = noodle("assets/animations/nothing.statemachine.json");
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.1);
    animators.update(&mut world, &assets, 0.1);
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(!animators.poses().contains_key(&part));
}

#[test]
fn removing_the_animator_puts_the_model_back() {
    let (mut world, part, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.1);
    assert!(animators.poses().contains_key(&part));
    world.destroy(animator).unwrap();
    animators.update(&mut world, &assets, 0.1);
    assert!(!animators.poses().contains_key(&part));
}

#[test]
fn hello_doors_clip_is_saved_as_the_engine_writes_it() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-door/assets/animations/Door/Door.anim.json");
    let text = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    assert_eq!(ClipFile::from_json(&text).unwrap().to_json(), text);
}

#[test]
fn the_example_files_are_saved_as_the_engine_writes_them() {
    let folder = example().join("assets/animations");
    let read = |path: PathBuf| std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let text = read(folder.join("noodle/noodle.statemachine.json"));
    assert_eq!(StateMachineFile::from_json(&text).unwrap().to_json(), text);
    for clip in ["Idle", "Wave", "Hop", "Walk", "Run"] {
        let text = read(folder.join(format!("noodle/{clip}.anim.json")));
        assert_eq!(
            ClipFile::from_json(&text).unwrap().to_json(),
            text,
            "{clip}"
        );
    }
}

/// The example's Walk clip (Root moves 2 m along +Z in 1 s), bound to the
/// noodle, with root motion on or off.
fn walk(root_motion: bool) -> (crate::Clip, std::sync::Arc<axcenity_assets::Model>) {
    let assets = Assets::new(example());
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let crate::LoadedClip {
        file: mut clip,
        takes,
        ..
    } = crate::load_clip(&assets, "assets/animations/noodle/Walk.anim.json").unwrap();
    clip.link.as_mut().unwrap().root_motion = root_motion;
    let bound = crate::Clip::bind(clip, takes, model.rig.as_deref()).unwrap();
    (bound, model)
}

#[test]
fn root_motion_moves_the_model_instead_of_the_root_bone() {
    let (clip, model) = walk(true);
    let rig = model.rig.as_ref().unwrap();
    assert!(clip.has_root_motion());
    let motion = clip.root_motion(rig, 0.0, 0.5);
    assert!(
        (motion.travel - glam::Vec3::new(0.0, 0.0, 1.0)).length() < 1e-3,
        "{motion:?}"
    );
    assert!(motion.yaw.abs() < 1e-5);
    // Across the loop, each pass adds its whole length, with no slide back.
    let looped = clip.root_motion(rig, 0.5, 1.5);
    assert!((looped.travel.z - 2.0).abs() < 1e-3, "{looped:?}");
    // The pose keeps the root over its start.
    let mut pose = rig.rest_pose();
    clip.sample(0.5, rig, &mut pose);
    let root = rig.bone("Root").unwrap();
    let position = rig.world(&pose)[root].transform_point3(glam::Vec3::ZERO);
    assert!(position.z.abs() < 1e-3, "{position}");
    // Off, the bone moves and the model doesn't.
    let (still, _) = walk(false);
    assert_eq!(
        still.root_motion(rig, 0.0, 0.5),
        crate::RootMotion::default()
    );
}

/// A controller with one blend tree state: the example's Idle (2 s) at
/// Speed 0 and Walk (1 s) at Speed 2.
fn locomotion() -> (crate::Compiled, std::sync::Arc<axcenity_assets::Model>) {
    use crate::files::{BlendChild, BlendTree, Motion, ParameterDef, StateDef};
    let assets = Assets::new(example());
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let file = StateMachineFile {
        parameters: vec![ParameterDef::Number {
            name: "Speed".into(),
            default: 0.0,
        }],
        default_state: "Move".into(),
        states: vec![StateDef::new(
            "Move",
            Motion::BlendTree(BlendTree {
                parameter: "Speed".into(),
                children: vec![
                    BlendChild {
                        clip: "assets/animations/noodle/Idle.anim.json".into(),
                        threshold: 0.0,
                    },
                    BlendChild {
                        clip: "assets/animations/noodle/Walk.anim.json".into(),
                        threshold: 2.0,
                    },
                ],
            }),
            [0.0, 0.0],
        )],
        ..StateMachineFile::default()
    };
    let (compiled, problems) = crate::Compiled::new(file, model.rig.as_deref(), |path| {
        crate::load_clip(&assets, path)
    });
    assert_eq!(problems, Vec::<String>::new());
    (compiled, model)
}

fn speed(value: f64) -> crate::Parameters {
    [("Speed".to_owned(), axcenity_core::Parameter::Number(value))].into()
}

#[test]
fn a_blend_tree_mixes_its_clips_by_the_parameter() {
    let (compiled, model) = locomotion();
    let rig = model.rig.as_ref().unwrap();
    assert_eq!(
        compiled.tree_weights(0, &speed(0.5)),
        Some(vec![0.75, 0.25])
    );
    assert_eq!(compiled.tree_weights(0, &speed(9.0)), Some(vec![0.0, 1.0]));
    // Cycles stay in step: the pass lasts the clips' lengths by their shares.
    assert!((compiled.duration(0, &speed(0.0)) - 2.0).abs() < 1e-9);
    assert!((compiled.duration(0, &speed(1.0)) - 1.5).abs() < 1e-9);
    assert!((compiled.duration(0, &speed(2.0)) - 1.0).abs() < 1e-9);
    // Halfway, the root is halfway between Idle's (still) and Walk's place.
    let root = rig.bone("Root").unwrap();
    let z = |value: f64| {
        let mut pose = rig.rest_pose();
        compiled.sample(0, 0.5, rig, &speed(value), &mut pose);
        rig.world(&pose)[root].transform_point3(glam::Vec3::ZERO).z
    };
    assert!(z(0.0).abs() < 1e-4);
    assert!((z(2.0) - 1.0).abs() < 1e-3, "{}", z(2.0));
    assert!((z(1.0) - 0.5).abs() < 1e-3, "{}", z(1.0));
}

#[test]
fn a_blend_tree_with_a_missing_or_wrong_parameter_is_reported() {
    use crate::files::{BlendTree, Motion, ParameterDef, StateDef};
    let file = StateMachineFile {
        parameters: vec![ParameterDef::Bool {
            name: "Speed".into(),
            default: false,
        }],
        states: vec![StateDef::new(
            "Move",
            Motion::BlendTree(BlendTree {
                parameter: "Speed".into(),
                children: Vec::new(),
            }),
            [0.0, 0.0],
        )],
        ..StateMachineFile::default()
    };
    let rig = axcenity_assets::Rig {
        bones: Vec::new(),
        joints: Vec::new(),
        fit: glam::Mat4::IDENTITY,
    };
    let (compiled, problems) = crate::Compiled::new(file, Some(&rig), |_| Err("none".into()));
    assert!(
        problems[0].contains("needs a Number parameter"),
        "{problems:?}"
    );
    // With nothing to play it holds still and lasts a second.
    assert_eq!(compiled.duration(0, &speed(1.0)), 1.0);
}

#[test]
fn events_fire_once_per_pass_in_order() {
    let (clip, model) = walk(false);
    let mut file = clip.file().clone();
    file.events = vec![
        crate::ClipEvent {
            time: 0.75,
            name: "Right".into(),
        },
        crate::ClipEvent {
            time: 0.25,
            name: "Left".into(),
        },
    ];
    let takes = Some(std::sync::Arc::new(vec![clip.take().unwrap().clone()]));
    let clip = crate::Clip::bind(file, takes.clone(), model.rig.as_deref()).unwrap();
    assert_eq!(clip.events_between(0.0, 1.5), ["Left", "Right", "Left"]);
    assert_eq!(clip.events_between(0.25, 0.5), ["Left"]);
    assert!(clip.events_between(0.3, 0.7).is_empty());
    let mut once = clip.file().clone();
    once.looping = false;
    let once = crate::Clip::bind(once, takes, model.rig.as_deref()).unwrap();
    assert_eq!(once.events_between(0.0, 3.0), ["Left", "Right"]);
    assert!(once.events_between(1.0, 3.0).is_empty());
}

/// A temporary project with the example's noodle and `clips` (path, file).
fn project(name: &str, clips: &[(&str, ClipFile)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("axcenity-anim-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::copy(
        example().join("assets/models/noodle.fbx"),
        root.join("assets/models/noodle.fbx"),
    )
    .unwrap();
    for (path, clip) in clips {
        crate::files::write_file(&root.join(path), &clip.to_json()).unwrap();
    }
    root
}

/// A door clip: up from y 0 to y 10 over a second, with an event halfway.
fn door_clip() -> ClipFile {
    use crate::files::{ClipEvent, Interp, PropertyKeyframe, PropertyName, PropertyTrack};
    let key = |id: &str, time: f64, y: f32| PropertyKeyframe {
        id: id.into(),
        time,
        value: vec![0.0, y, 0.0],
        interp: Interp::Linear,
        easing: None,
        bezier: None,
    };
    let mut clip = ClipFile::new(1.0);
    clip.tracks.push(PropertyTrack {
        id: "position".into(),
        target: ".".into(),
        property: PropertyName::Position,
        keys: vec![key("a", 0.0, 0.0), key("b", 1.0, 10.0)],
    });
    clip.events.push(ClipEvent {
        time: 0.5,
        name: "Halfway".into(),
    });
    clip
}

const DOOR: &str = "assets/animations/Door/Door.anim.json";

/// A world with a Part and an Animation of the door clip in it, and an
/// Animator beside it if `animator`.
fn door_world(animator: bool) -> (World, InstanceId, Option<InstanceId>, InstanceId) {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let animator = animator.then(|| {
        let id = world.create(ClassName::Animator).unwrap();
        world.set_parent(id, Some(part)).unwrap();
        id
    });
    let animation = world.create(ClassName::Animation).unwrap();
    world.set_parent(animation, Some(part)).unwrap();
    world
        .set_property(animation, "Clip", Value::String(DOOR.into()))
        .unwrap();
    (world, part, animator, animation)
}

fn y(world: &World, part: InstanceId) -> f32 {
    world
        .get(part)
        .and_then(|p| p.property("Position"))
        .and_then(Value::as_vector3)
        .unwrap()
        .y
}

#[test]
fn a_track_from_load_animation_plays_fires_its_events_and_stops_at_its_end() {
    let root = project("door-track", &[(DOOR, door_clip())]);
    let assets = Assets::new(&root);
    let (mut world, part, animator, animation) = door_world(true);
    let animator = animator.unwrap();
    let track = world.load_animation(animator, animation).unwrap();
    let mut animators = Animators::new();
    // Loaded, not played: nothing moves.
    animators.update(&mut world, &assets, 0.1);
    assert_eq!(y(&world, part), 0.0);
    assert_eq!(world.animation_track(track).unwrap().length, 1.0);
    let state = world.animation_track_mut(track).unwrap();
    state.playing = true;
    state.weight = 1.0;
    state.order = 1;
    animators.update(&mut world, &assets, 0.0);
    let events = animators.update(&mut world, &assets, 0.6);
    assert!((y(&world, part) - 6.0).abs() < 1e-3, "{}", y(&world, part));
    assert_eq!(
        events,
        [AnimatorEvent::TrackMarker {
            track,
            name: "Halfway".into()
        }]
    );
    // At its end it stops, keeps the door at the top, and says so.
    let events = animators.update(&mut world, &assets, 0.6);
    assert_eq!(events, [AnimatorEvent::TrackStopped { track }]);
    assert_eq!(y(&world, part), 10.0);
    assert!(!world.animation_track(track).unwrap().playing);
    animators.update(&mut world, &assets, 0.5);
    assert_eq!(y(&world, part), 10.0, "properties keep their last value");
    assert_eq!(animators.take_errors(), Vec::<String>::new());
}

#[test]
fn animation_play_works_through_the_animator_and_auto_play_starts_by_itself() {
    let root = project("door-play", &[(DOOR, door_clip())]);
    let assets = Assets::new(&root);
    let (mut world, part, _, animation) = door_world(true);
    let mut animators = Animators::new();
    world.property_animation_mut(animation).unwrap().play = true;
    animators.update(&mut world, &assets, 0.0);
    let events = animators.update(&mut world, &assets, 0.51);
    assert!(
        events.contains(&AnimatorEvent::AnimationMarker {
            animation,
            name: "Halfway".into()
        }),
        "{events:?}"
    );
    assert!((y(&world, part) - 5.1).abs() < 1e-3);
    let events = animators.update(&mut world, &assets, 0.6);
    assert!(events.contains(&AnimatorEvent::AnimationCompleted { animation }));
    assert_eq!(y(&world, part), 10.0);

    // AutoPlay: played by itself the first frame, with no script.
    let (mut world, part, _, animation) = door_world(true);
    world
        .set_property(animation, "AutoPlay", Value::Bool(true))
        .unwrap();
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    animators.update(&mut world, &assets, 0.25);
    assert!((y(&world, part) - 2.5).abs() < 1e-3);
    // Two Animations are independent.
    assert!(world.property_animation(animation).unwrap().playing);
}

#[test]
fn a_linked_clip_that_also_keys_a_sword_plays_both() {
    use crate::files::{Interp, PropertyKeyframe, PropertyName, PropertyTrack};
    let mut clip = ClipFile::linked("assets/models/noodle.fbx", "Moves", 0.0, 1.0);
    let key = |id: &str, time: f64, value: f32| PropertyKeyframe {
        id: id.into(),
        time,
        value: vec![value],
        interp: Interp::Linear,
        easing: None,
        bezier: None,
    };
    clip.tracks.push(PropertyTrack {
        id: "sword".into(),
        target: "Sword".into(),
        property: PropertyName::Transparency,
        keys: vec![key("a", 0.0, 0.0), key("b", 1.0, 1.0)],
    });
    let path = "assets/animations/noodle/WaveSword.anim.json";
    let root = project("sword", &[(path, clip)]);
    let assets = Assets::new(&root);
    let (mut world, part, animator) = noodle("");
    let sword = world.create(ClassName::Part).unwrap();
    world.set_name(sword, "Sword").unwrap();
    world.set_parent(sword, Some(part)).unwrap();
    let animation = world.create(ClassName::Animation).unwrap();
    world
        .set_property(animation, "Clip", Value::String(path.into()))
        .unwrap();
    world.set_parent(animation, Some(animator)).unwrap();
    let track = world.load_animation(animator, animation).unwrap();
    let state = world.animation_track_mut(track).unwrap();
    state.playing = true;
    state.weight = 1.0;
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    let first = animators.poses()[&part].clone();
    animators.update(&mut world, &assets, 0.5);
    // The take moves the bones…
    assert_ne!(
        corners(&assets, &first),
        corners(&assets, &animators.poses()[&part])
    );
    // …and the clip's own track fades the sword.
    let transparency = world
        .get(sword)
        .and_then(|s| s.property("Transparency"))
        .and_then(Value::as_number)
        .unwrap();
    assert!((transparency - 0.5).abs() < 1e-3, "{transparency}");
}

#[test]
fn a_track_plays_over_the_state_machine_and_stop_fades_it_out() {
    let (mut world, part, animator) = noodle(STATE_MACHINE);
    let assets = Assets::new(example());
    let animation = world.create(ClassName::Animation).unwrap();
    world
        .set_property(
            animation,
            "Clip",
            Value::String("assets/animations/noodle/Wave.anim.json".into()),
        )
        .unwrap();
    world.set_parent(animation, Some(animator)).unwrap();
    // A second noodle with only the state machine, to compare with.
    let (mut alone, alone_part, _) = noodle(STATE_MACHINE);
    let mut animators = Animators::new();
    let mut alone_animators = Animators::new();
    let primitives = |animators: &Animators, part| corners(&assets, &animators.poses()[&part]);
    let step = |world: &mut World, alone: &mut World, a: &mut Animators, b: &mut Animators, dt| {
        let events = a.update(world, &assets, dt);
        b.update(alone, &assets, dt);
        events
    };
    step(
        &mut world,
        &mut alone,
        &mut animators,
        &mut alone_animators,
        0.1,
    );
    assert_eq!(
        primitives(&animators, part),
        primitives(&alone_animators, alone_part)
    );
    let track = world.load_animation(animator, animation).unwrap();
    let state = world.animation_track_mut(track).unwrap();
    state.playing = true;
    state.weight = 1.0;
    state.order = 1;
    step(
        &mut world,
        &mut alone,
        &mut animators,
        &mut alone_animators,
        0.2,
    );
    assert_ne!(
        primitives(&animators, part),
        primitives(&alone_animators, alone_part),
        "Wave plays over Locomotion"
    );
    assert_eq!(
        world.animator(animator).unwrap().current_state,
        "Locomotion"
    );
    // Stop(0.3): half faded after 0.15 s, gone (and Stopped) after 0.3 s.
    let state = world.animation_track_mut(track).unwrap();
    state.playing = false;
    state.stopping = true;
    state.target_weight = 0.0;
    state.fade = 0.3;
    step(
        &mut world,
        &mut alone,
        &mut animators,
        &mut alone_animators,
        0.15,
    );
    let weight = world.animation_track(track).unwrap().weight;
    assert!((weight - 0.5).abs() < 1e-6, "{weight}");
    let events = step(
        &mut world,
        &mut alone,
        &mut animators,
        &mut alone_animators,
        0.16,
    );
    assert!(
        events.contains(&AnimatorEvent::TrackStopped { track }),
        "{events:?}"
    );
    assert_eq!(
        primitives(&animators, part),
        primitives(&alone_animators, alone_part),
        "back to the state machine alone"
    );
}

/// The door clip with its first key eased `easing` (or linear for `None`),
/// played by an Animation for half a second: where the door is then.
fn door_halfway(name: &str, easing: Option<crate::Easing>) -> f32 {
    let mut clip = door_clip();
    if let Some(easing) = easing {
        let key = &mut clip.tracks[0].keys[0];
        key.interp = crate::Interp::Ease;
        key.easing = Some(easing);
    }
    let root = project(name, &[(DOOR, clip)]);
    let assets = Assets::new(&root);
    let (mut world, part, _, animation) = door_world(true);
    world.property_animation_mut(animation).unwrap().play = true;
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    animators.update(&mut world, &assets, 0.5);
    assert!(animators.take_errors().is_empty());
    y(&world, part)
}

#[test]
fn a_keys_easing_changes_how_it_plays() {
    use crate::{Easing, EasingDirection, EasingStyle};
    let linear = door_halfway("ease-linear", None);
    let quad_out = door_halfway(
        "ease-quad-out",
        Some(Easing::new(EasingStyle::Quad, EasingDirection::Out)),
    );
    let quad_in = door_halfway(
        "ease-quad-in",
        Some(Easing::new(EasingStyle::Quad, EasingDirection::In)),
    );
    assert!((linear - 5.0).abs() < 1e-3, "{linear}");
    assert!((quad_out - 7.5).abs() < 1e-3, "{quad_out}");
    assert!((quad_in - 2.5).abs() < 1e-3, "{quad_in}");
}

#[test]
fn eased_and_bezier_keys_save_and_read_back() {
    use crate::{Easing, EasingDirection, EasingStyle, Interp};
    let mut clip = door_clip();
    let keys = &mut clip.tracks[0].keys;
    keys[0].interp = Interp::Bezier;
    keys[0].bezier = Some([0.25, 0.1, 0.25, 1.0]);
    keys[1].interp = Interp::Ease;
    keys[1].easing = Some(Easing::new(EasingStyle::Elastic, EasingDirection::InOut));
    let text = clip.to_json();
    assert!(text.contains("\"bezier\""), "{text}");
    assert!(text.contains("\"style\": \"Elastic\""), "{text}");
    let read = ClipFile::from_json(&text).unwrap();
    assert_eq!(read, clip);
    assert_eq!(read.to_json(), text);
    // A linear key writes neither.
    let plain = door_clip().to_json();
    assert!(!plain.contains("easing") && !plain.contains("bezier"));
}

/// A clip written before keys could be eased: Step and Linear keys, no
/// easing or handles.
const BEFORE_EASING: &str = r#"{
  "format_version": 2,
  "duration": 2.0,
  "loop": false,
  "tracks": [
    {
      "id": "position",
      "target": ".",
      "property": "Position",
      "keys": [
        {
          "id": "k1",
          "time": 0.0,
          "value": [
            0.0,
            0.0,
            0.0
          ],
          "interp": "Linear"
        },
        {
          "id": "k2",
          "time": 1.0,
          "value": [
            0.0,
            4.0,
            0.0
          ],
          "interp": "Step"
        },
        {
          "id": "k3",
          "time": 2.0,
          "value": [
            0.0,
            10.0,
            0.0
          ],
          "interp": "Linear"
        }
      ]
    }
  ],
  "events": []
}
"#;

#[test]
fn a_clip_saved_before_easing_opens_and_plays_the_same() {
    let clip = ClipFile::from_json(BEFORE_EASING).unwrap();
    assert_eq!(clip.to_json(), BEFORE_EASING, "it's saved unchanged");
    assert!(
        clip.tracks[0]
            .keys
            .iter()
            .all(|k| k.easing.is_none() && k.bezier.is_none())
    );
    let root = project("before-easing", &[(DOOR, clip)]);
    let assets = Assets::new(&root);
    let (mut world, part, _, animation) = door_world(true);
    world.property_animation_mut(animation).unwrap().play = true;
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    // Linear to the Step key, then held at it until the last.
    let mut seen = Vec::new();
    for _ in 0..4 {
        animators.update(&mut world, &assets, 0.5);
        seen.push(y(&world, part));
    }
    assert_eq!(seen, [2.0, 4.0, 4.0, 10.0]);
    assert!(animators.take_errors().is_empty());
}

/// The door clip going up to `top` instead of 10.
fn door_clip_to(top: f32) -> ClipFile {
    let mut clip = door_clip();
    clip.tracks[0].keys[1].value = vec![0.0, top, 0.0];
    clip
}

const DOOR_MACHINE: &str = "assets/animations/Door/Door.statemachine.json";

/// A state machine that starts Shut (playing nothing) and has `Swing`
/// playing the door clip, with `extra` states after them.
fn door_machine(extra: &[&str]) -> String {
    use crate::files::{Motion, StateDef};
    let mut states = vec![
        StateDef::new("Shut", Motion::Clip(String::new()), [0.0, 0.0]),
        StateDef::new("Swing", Motion::Clip(DOOR.into()), [0.0, 0.0]),
    ];
    for name in extra {
        states.push(StateDef::new(name, Motion::Clip(String::new()), [0.0, 0.0]));
    }
    StateMachineFile {
        default_state: "Shut".into(),
        states,
        ..StateMachineFile::default()
    }
    .to_json()
}

/// The door world with its Animator playing [`door_machine`], swung a
/// quarter of the way.
fn swinging_door(root: &Path, assets: &Assets) -> (World, Animators, InstanceId, InstanceId) {
    crate::files::write_file(&root.join(DOOR_MACHINE), &door_machine(&[])).unwrap();
    let (mut world, part, animator, animation) = door_world(true);
    world.destroy(animation).unwrap();
    let animator = animator.unwrap();
    world
        .set_property(animator, "StateMachine", Value::String(DOOR_MACHINE.into()))
        .unwrap();
    let mut animators = Animators::new();
    animators.update(&mut world, assets, 0.0);
    world.animator_mut(animator).unwrap().play = Some(PlayRequest {
        state: "Swing".into(),
        blend: None,
    });
    animators.update(&mut world, assets, 0.0);
    animators.update(&mut world, assets, 0.25);
    assert!((y(&world, part) - 2.5).abs() < 1e-3, "{}", y(&world, part));
    (world, animators, part, animator)
}

#[test]
fn an_edited_clip_changes_the_motion_and_the_animator_stays_in_its_state() {
    let root = project("reload-clip", &[(DOOR, door_clip())]);
    let assets = Assets::new(&root);
    let (mut world, mut animators, part, animator) = swinging_door(&root, &assets);

    crate::files::write_file(&root.join(DOOR), &door_clip_to(20.0).to_json()).unwrap();
    assert!(animators.reload(DOOR), "the Swing state plays it");
    assert!(!animators.reload("assets/animations/Other.anim.json"));
    animators.update(&mut world, &assets, 0.0);
    assert_eq!(world.animator(animator).unwrap().current_state, "Swing");
    assert!((y(&world, part) - 5.0).abs() < 1e-3, "{}", y(&world, part));
    animators.update(&mut world, &assets, 0.25);
    assert!((y(&world, part) - 10.0).abs() < 1e-3, "{}", y(&world, part));

    // An edited state machine keeps the state it's in, by name, even when
    // states were added before it.
    let mut file = StateMachineFile::from_json(&door_machine(&[])).unwrap();
    file.states.insert(0, file.states[0].clone());
    file.states[0].name = "Ajar".into();
    crate::files::write_file(&root.join(DOOR_MACHINE), &file.to_json()).unwrap();
    assert!(animators.reload(DOOR_MACHINE));
    animators.update(&mut world, &assets, 0.0);
    assert_eq!(world.animator(animator).unwrap().current_state, "Swing");
    assert!((y(&world, part) - 10.0).abs() < 1e-3, "{}", y(&world, part));
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_clip_saved_broken_keeps_its_last_good_motion_and_is_reported_once() {
    let root = project("reload-broken", &[(DOOR, door_clip())]);
    let assets = Assets::new(&root);
    let (mut world, mut animators, part, animator) = swinging_door(&root, &assets);

    std::fs::write(root.join(DOOR), "{ broken").unwrap();
    animators.reload(DOOR);
    animators.update(&mut world, &assets, 0.25);
    animators.update(&mut world, &assets, 0.0);
    assert_eq!(world.animator(animator).unwrap().current_state, "Swing");
    assert!((y(&world, part) - 5.0).abs() < 1e-3, "{}", y(&world, part));
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with(&format!("Could not reload `{DOOR}`")));
    let _ = std::fs::remove_dir_all(root);
}

/// A fade: a UI element's BackgroundTransparency from 1 to 0 over a
/// second, with an event halfway.
fn fade_clip() -> ClipFile {
    use crate::files::{ClipEvent, Interp, PropertyKeyframe, PropertyName, PropertyTrack};
    let key = |id: &str, time: f64, value: f32| PropertyKeyframe {
        id: id.into(),
        time,
        value: vec![value],
        interp: Interp::Linear,
        easing: None,
        bezier: None,
    };
    let mut clip = ClipFile::new(1.0);
    clip.tracks.push(PropertyTrack {
        id: "fade".into(),
        target: ".".into(),
        property: PropertyName::BackgroundTransparency,
        keys: vec![key("a", 0.0, 1.0), key("b", 1.0, 0.0)],
    });
    clip.events.push(ClipEvent {
        time: 0.5,
        name: "Halfway".into(),
    });
    clip
}

#[test]
fn reduced_motion_ends_a_ui_elements_clips_at_once_and_leaves_parts_moving() {
    const FADE: &str = "assets/animations/Fade/Fade.anim.json";
    let root = project(
        "reduced-motion",
        &[(DOOR, door_clip()), (FADE, fade_clip())],
    );
    let assets = Assets::new(&root);
    let (mut world, part, _, door) = door_world(true);
    let mut settings = world.ui_settings();
    settings.reduced_motion = true;
    world.set_ui_settings(settings);
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let panel = world.create(ClassName::Frame).unwrap();
    world.set_parent(panel, Some(gui)).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(panel)).unwrap();
    let fade = world.create(ClassName::Animation).unwrap();
    world.set_parent(fade, Some(animator)).unwrap();
    world
        .set_property(fade, "Clip", Value::String(FADE.into()))
        .unwrap();
    let transparency = |world: &World| {
        world
            .get(panel)
            .and_then(|p| p.property("BackgroundTransparency"))
            .and_then(Value::as_number)
            .unwrap()
    };
    let mut animators = Animators::new();

    // A played Animation jumps to its end, with its events.
    world.property_animation_mut(fade).unwrap().play = true;
    world.property_animation_mut(door).unwrap().play = true;
    let events = animators.update(&mut world, &assets, 0.1);
    assert_eq!(transparency(&world), 0.0);
    assert!(
        events.contains(&AnimatorEvent::AnimationMarker {
            animation: fade,
            name: "Halfway".into()
        }),
        "{events:?}"
    );
    assert!(events.contains(&AnimatorEvent::AnimationCompleted { animation: fade }));
    // The 3D world still moves.
    assert!((y(&world, part) - 1.0).abs() < 1e-3, "{}", y(&world, part));

    // An AnimationTrack ends at once too, and says so. (The stopped
    // Animation under it would hold the panel at its start.)
    world.property_animation_mut(fade).unwrap().stop = true;
    animators.update(&mut world, &assets, 0.016);
    assert_eq!(transparency(&world), 1.0);
    let track = world.load_animation(animator, fade).unwrap();
    let state = world.animation_track_mut(track).unwrap();
    state.playing = true;
    state.target_weight = 1.0;
    state.fade = 0.3;
    state.order = 5;
    let events = animators.update(&mut world, &assets, 0.016);
    assert_eq!(transparency(&world), 0.0);
    assert!(
        events.contains(&AnimatorEvent::TrackStopped { track }),
        "{events:?}"
    );

    // A looping one holds where it is.
    world
        .set_property(panel, "BackgroundTransparency", Value::Number(1.0))
        .unwrap();
    let looped = world.load_animation(animator, fade).unwrap();
    let state = world.animation_track_mut(looped).unwrap();
    state.playing = true;
    state.looped = Some(true);
    state.target_weight = 1.0;
    state.order = 6;
    for _ in 0..5 {
        animators.update(&mut world, &assets, 0.2);
    }
    assert_eq!(world.animation_track(looped).unwrap().time, 0.0);
    assert_eq!(transparency(&world), 1.0);
    assert_eq!(animators.take_errors(), Vec::<String>::new());
}

/// Phase 45: where a Tool's hand is comes from the bone's place in the
/// posed model, worked back out of the joint matrices the pose keeps.
#[test]
fn a_bone_in_the_world_follows_the_pose_the_animators_made() {
    let (mut world, part, _) = noodle(STATE_MACHINE);
    world
        .set_property(
            part,
            "Position",
            Value::Vector3(Vector3::new(4.0, 2.0, -3.0)),
        )
        .unwrap();
    world
        .set_property(
            part,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.4);
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let rig = model.rig.clone().unwrap();
    let posed = animators.poses().get(&part).cloned().unwrap();
    let (clip, _) = walk(false);
    let mut pose = rig.rest_pose();
    clip.sample(0.4, &rig, &mut pose);
    let place = crate::ragdoll::placement(&world, part, &rig).unwrap();
    let posed_here = model.pose(&pose).unwrap();
    let mut checked = 0;
    for joint in &rig.joints {
        let expected = place * rig.world(&pose)[joint.bone];
        let found =
            crate::ragdoll::bone_in_world(&world, part, &rig, Some(&posed_here), joint.bone)
                .unwrap();
        assert!(
            found.abs_diff_eq(expected, 1e-3),
            "bone {}: {found} != {expected}",
            rig.bones[joint.bone].name
        );
        // The Animators' own pose gives a place too.
        assert!(
            crate::ragdoll::bone_in_world(&world, part, &rig, Some(&posed), joint.bone).is_some()
        );
        checked += 1;
    }
    assert!(checked > 1);
    // No pose: the bone at rest.
    let rest = place * rig.world(&rig.rest_pose())[rig.joints[0].bone];
    let found = crate::ragdoll::bone_in_world(&world, part, &rig, None, rig.joints[0].bone);
    assert!(found.unwrap().abs_diff_eq(rest, 1e-4));
}

/// `count` noodles side by side, each with an Animator playing the example's
/// state machine, at Speeds that differ so their poses differ.
fn crowd(count: usize) -> (World, Vec<InstanceId>) {
    use axcenity_core::Parameter;
    let mut world = World::new();
    let workspace = world.workspace();
    let mut parts = Vec::new();
    for index in 0..count {
        let part = world.create(ClassName::MeshPart).unwrap();
        world
            .set_property(
                part,
                "MeshId",
                Value::String("assets/models/noodle.fbx".into()),
            )
            .unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(index as f32 * 2.0, 0.0, 0.0)),
            )
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        let animator = world.create(ClassName::Animator).unwrap();
        world
            .set_property(
                animator,
                "StateMachine",
                Value::String(STATE_MACHINE.into()),
            )
            .unwrap();
        world.set_parent(animator, Some(part)).unwrap();
        parts.push(part);
    }
    // Their parameters arrive with the first update.
    let assets = Assets::new(example());
    Animators::with_threads(1).update(&mut world, &assets, 0.0);
    for (index, &part) in parts.iter().enumerate() {
        let animator = world.find_first_child(part, "Animator").unwrap();
        world
            .animator_mut(animator)
            .unwrap()
            .parameters
            .insert("Speed".into(), Parameter::Number(index as f64 / 4.0));
    }
    (world, parts)
}

#[test]
fn a_crowd_is_posed_the_same_on_one_thread_and_on_eight() {
    let assets = Assets::new(example());
    let mut poses = Vec::new();
    for threads in [1, 8] {
        let (mut world, parts) = crowd(24);
        let mut animators = Animators::with_threads(threads);
        for _ in 0..30 {
            animators.update(&mut world, &assets, 1.0 / 60.0);
        }
        assert_eq!(animators.take_errors(), Vec::<String>::new());
        let times = animators.thread_times();
        if threads == 1 {
            assert_eq!(times.len(), 1, "{times:?}");
            assert_eq!(times[0].0, "main");
        } else {
            assert!(!times.is_empty(), "{times:?}");
        }
        let made: Vec<_> = parts
            .iter()
            .map(|part| corners(&assets, &animators.poses()[part]))
            .collect();
        poses.push(made);
    }
    assert_eq!(poses[0], poses[1]);
    // And the noodles at different speeds aren't all posed alike.
    assert_ne!(poses[0][0], poses[0][23]);
}
