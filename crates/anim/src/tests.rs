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
    assert_eq!(world.animator(animator).unwrap().current_state, "Idle");
    assert_eq!(
        events,
        [AnimatorEvent::StateChanged {
            animator,
            state: "Idle".into(),
            previous: String::new(),
        }]
    );
    let first = animators.poses()[&part].clone();
    animators.update(&mut world, &assets, 0.5);
    let later = animators.poses()[&part].clone();
    assert_ne!(first.version, later.version);
    assert_ne!(first.primitives, later.primitives, "Idle sways the noodle");
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
            previous: "Idle".into(),
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
    for clip in ["Idle", "Wave", "Hop", "Walk"] {
        let text = read(folder.join(format!("noodle/{clip}.anim.json")));
        assert_eq!(
            ClipFile::from_json(&text).unwrap().to_json(),
            text,
            "{clip}"
        );
    }
}
