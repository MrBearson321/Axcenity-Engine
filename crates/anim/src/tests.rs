//! Animators running the `hello-animation` example's noodle.

use std::path::{Path, PathBuf};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, PlayRequest, Value, Vector3, World};

use crate::{AnimatorEvent, Animators, ClipFile, ControllerFile};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation")
}

/// A world with the example's noodle: a MeshPart with an Animator.
fn noodle(controller: &str) -> (World, InstanceId, InstanceId) {
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
        .set_property(animator, "Controller", Value::String(controller.into()))
        .unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    (world, part, animator)
}

const CONTROLLER: &str = "assets/animations/noodle.controller.json";

#[test]
fn an_animator_starts_in_its_default_state_and_poses_its_part() {
    let (mut world, part, animator) = noodle(CONTROLLER);
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
        first.primitives, later.primitives,
        "Idle (Speed 0) sways the noodle"
    );
}

#[test]
fn play_switches_state_and_reports_the_change() {
    let (mut world, _, animator) = noodle(CONTROLLER);
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
    let (mut world, _, animator) = noodle(CONTROLLER);
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
fn a_parameter_the_controller_lacks_is_reported() {
    let (mut world, _, animator) = noodle(CONTROLLER);
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
fn a_missing_controller_is_reported_once_and_leaves_the_model_still() {
    let (mut world, part, _) = noodle("assets/animations/nothing.controller.json");
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
    let (mut world, part, animator) = noodle(CONTROLLER);
    let assets = Assets::new(example());
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.1);
    assert!(animators.poses().contains_key(&part));
    world.destroy(animator).unwrap();
    animators.update(&mut world, &assets, 0.1);
    assert!(!animators.poses().contains_key(&part));
}

#[test]
fn the_example_files_are_saved_as_the_engine_writes_them() {
    let folder = example().join("assets/animations");
    let read = |path: PathBuf| std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let text = read(folder.join("noodle.controller.json"));
    assert_eq!(ControllerFile::from_json(&text).unwrap().to_json(), text);
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
fn walk(root_motion: bool) -> (crate::BoundClip, std::sync::Arc<axcenity_assets::Model>) {
    let assets = Assets::new(example());
    let model = assets.model("assets/models/noodle.fbx").unwrap();
    let (mut clip, takes) =
        crate::load_clip(&assets, "assets/animations/noodle/Walk.anim.json").unwrap();
    clip.root_motion = root_motion;
    let bound = crate::BoundClip::bind(clip, takes, model.rig.as_ref().unwrap()).unwrap();
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
    let model = assets.model("assets/models/noodle.fbx").unwrap();
    let file = ControllerFile {
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
        ..ControllerFile::default()
    };
    let (compiled, problems) = crate::Compiled::new(file, model.rig.as_ref().unwrap(), |path| {
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
    let file = ControllerFile {
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
        ..ControllerFile::default()
    };
    let rig = axcenity_assets::Rig {
        bones: Vec::new(),
        joints: Vec::new(),
        fit: glam::Mat4::IDENTITY,
    };
    let (compiled, problems) = crate::Compiled::new(file, &rig, |_| Err("none".into()));
    assert!(
        problems[0].contains("needs a Number parameter"),
        "{problems:?}"
    );
    // With nothing to play it holds still and lasts a second.
    assert_eq!(compiled.duration(0, &speed(1.0)), 1.0);
}

#[test]
fn events_fire_once_per_pass_in_order() {
    let (mut clip, model) = walk(false);
    clip.file.events = vec![
        crate::ClipEvent {
            time: 0.75,
            name: "Right".into(),
        },
        crate::ClipEvent {
            time: 0.25,
            name: "Left".into(),
        },
    ];
    let clip = crate::BoundClip::bind(
        clip.file.clone(),
        std::sync::Arc::new(vec![clip.take().clone()]),
        model.rig.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(clip.events_between(0.0, 1.5), ["Left", "Right", "Left"]);
    assert_eq!(clip.events_between(0.25, 0.5), ["Left"]);
    assert!(clip.events_between(0.3, 0.7).is_empty());
    let mut once = clip.file.clone();
    once.looping = false;
    let once = crate::BoundClip::bind(
        once,
        std::sync::Arc::new(vec![clip.take().clone()]),
        model.rig.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(once.events_between(0.0, 3.0), ["Left", "Right"]);
    assert!(once.events_between(1.0, 3.0).is_empty());
}
