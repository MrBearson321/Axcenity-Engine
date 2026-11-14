//! The default humanoid (`assets/characters/humanoid`, shown by
//! `examples/hello-humanoid`): its glTF animations play on its model, its
//! skeleton maps to every humanoid role with nothing to fix, and its head
//! carries the facial profile's shapes.

use axcenity_anim::{
    Digit, HumanBone, ROLE_COUNT, Segment, Severity, Side, check_pair, check_rig, rest_arm_angle,
};

use super::tests::start_example_in;
use super::*;

const MODEL: &str = "assets/models/Humanoid/Humanoid.glb";
const ANIMATIONS: &str = "assets/models/Humanoid/HumanoidAnimations.glb";

#[test]
fn hello_humanoid_plays_its_glb_animations_on_each_humanoid() {
    let (mut world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    let mut joints_at = |world: &mut World, seconds: u32| {
        for _ in 0..seconds * 30 {
            animators.update(world, &assets, 1.0 / 60.0);
        }
        assert_eq!(animators.take_errors(), Vec::<String>::new());
        let walker = world.find_first_child(world.workspace(), "Walk").unwrap();
        assert_eq!(animators.poses().len(), 3, "Idle, Walk and Wave are posed");
        animators.poses()[&walker].joints().to_vec()
    };
    let first = joints_at(&mut world, 1);
    let later = joints_at(&mut world, 1);
    assert_ne!(first, later, "the walk moves on");
    assert!(
        assets.take_errors().is_empty(),
        "{:?}",
        assets.take_errors()
    );
}

#[test]
fn the_default_humanoid_skeleton_maps_every_role_with_no_problems() {
    let (_world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let model = assets.model_blocking(MODEL).unwrap();
    let bones = &model.rig.as_ref().unwrap().bones;
    let report = check_rig(bones, None, &assets.rig_definitions());
    let problems: Vec<_> = report
        .problems
        .iter()
        .filter(|problem| problem.severity != Severity::Note)
        .collect();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(report.map.roles().count(), ROLE_COUNT, "fingers included");
    assert!(
        report
            .map
            .get(HumanBone::Finger(Side::Left, Digit::Index, Segment::Distal))
            .is_some()
    );
    // A relaxed A-pose: the upper arms 45° below level.
    let angle = rest_arm_angle(bones, &report.map).unwrap();
    assert!((angle - 45.0).abs() < 1.0, "{angle}");
    // Every starter animation is a take for the same skeleton.
    let takes = assets.takes(ANIMATIONS).unwrap();
    assert_eq!(takes.len(), 32);
    for take in takes.iter() {
        let problems = check_pair(&take.skeleton, bones, &assets.rig_definitions());
        assert!(problems.is_empty(), "{}: {problems:?}", take.name);
    }
}

#[test]
fn the_default_humanoid_head_has_the_facial_profile_shapes_at_rest() {
    let (_world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let model = assets.model_blocking(MODEL).unwrap();
    let names: Vec<&str> = model
        .morph_targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    for name in [
        "Blink_Left",
        "Blink_Right",
        "Eye_Squint_Left",
        "Eye_Wide_Right",
        "Brow_Raise_Left",
        "Brow_Lower_Right",
        "Mouth_Smile_Left",
        "Mouth_Frown_Right",
        "Mouth_Open",
        "Jaw_Open",
        "Mouth_Pucker",
        "Mouth_Left",
        "Mouth_Right",
        "Cheek_Raise_Left",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert_eq!(names.len(), 41);
    assert!(model.morph_targets.iter().all(|t| t.default_weight == 0.0));
}
