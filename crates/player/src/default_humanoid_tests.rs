//! The default humanoid (`assets/characters/humanoid`, shown by
//! `examples/hello-humanoid`): its glTF animations play on its model, its
//! skeleton maps to every humanoid role with nothing to fix, and its head
//! carries the facial profile's shapes.

use axcenity_anim::{
    Digit, HumanBone, ROLE_COUNT, Segment, Severity, Side, check_pair, check_rig, rest_arm_angle,
};
use axcenity_core::Value;

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

/// A new Baseplate project's player spawns as the default humanoid, which
/// walks on its root part and animates through the Humanoid's states.
#[test]
fn a_new_baseplate_projects_player_is_the_default_humanoid_and_walks_animated() {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-player-default-humanoid-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let folder = axcenity_project::create_project_from(
        &parent,
        "Humanoid Test",
        axcenity_project::Template::Baseplate,
    )
    .unwrap();
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    axcenity_project::load_starter_character(&folder, &mut world)
        .unwrap()
        .expect("the template names the Character prefab");
    let assets = Assets::new(&folder);
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_assets(assets.clone());
    let (mut characters, mut physics) =
        (Controllers::default(), Physics::with_assets(assets.clone()));
    let mut animators = Animators::new();
    let walk = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    let mut frame = |world: &mut World, animators: &mut Animators, input: MoveInput| {
        let seconds = 1.0 / 60.0;
        characters.step(world, &assets, input, seconds as f32);
        axcenity_gameplay::step_humanoids(&mut scripts, world, &characters, seconds);
        scripts.step_players(world, seconds);
        scripts.step(world, seconds);
        characters.keep_kinematic(world);
        physics.step(world, seconds);
        animators.update(world, &assets, seconds);
    };
    for _ in 0..30 {
        frame(&mut world, &mut animators, MoveInput::default());
    }
    let player = world
        .find_first_child(world.players(), "Player")
        .unwrap_or_else(|| world.get(world.players()).unwrap().children()[0]);
    let character = world
        .get(player)
        .unwrap()
        .property("Character")
        .and_then(Value::as_instance)
        .unwrap();
    let root = world
        .find_first_child(character, "HumanoidRootPart")
        .unwrap();
    let body = world.find_first_child(character, "Body").unwrap();
    let position = |world: &World, id| {
        world
            .get(id)
            .unwrap()
            .property("Position")
            .unwrap()
            .as_vector3()
            .unwrap()
    };
    let start = position(&world, root);
    assert!(
        start.y > 2.0 && start.y < 3.0,
        "standing on the baseplate: {start:?}"
    );
    let animator = world.find_first_child(body, "Animator").unwrap();
    let state = |world: &World| world.animator(animator).unwrap().current_state.clone();
    assert_eq!(state(&world), "Idle");
    let idle = animators.poses()[&body].joints().to_vec();
    for _ in 0..30 {
        frame(&mut world, &mut animators, walk);
    }
    let moved = position(&world, root);
    assert!(
        moved.z < start.z - 4.0,
        "walked forward: {start:?} → {moved:?}"
    );
    let body_at = position(&world, body);
    assert!((body_at.z - moved.z).abs() < 0.5, "the mesh came along");
    assert_eq!(state(&world), "Walk");
    assert_ne!(animators.poses()[&body].joints(), idle.as_slice());
    // Crouched, the root squashes (the mesh doesn't) and it plays
    // CrouchWalk, then Crouch standing still.
    let crouch = MoveInput {
        crouch: true,
        ..walk
    };
    for _ in 0..10 {
        frame(&mut world, &mut animators, crouch);
    }
    assert_eq!(state(&world), "CrouchWalk");
    let size = |world: &World, id| {
        world
            .get(id)
            .unwrap()
            .property("Size")
            .unwrap()
            .as_vector3()
            .unwrap()
    };
    assert!(size(&world, root).y < 4.0, "{:?}", size(&world, root));
    assert!((size(&world, body).y - 5.0).abs() < 0.1);
    for _ in 0..10 {
        frame(
            &mut world,
            &mut animators,
            MoveInput {
                forward: false,
                ..crouch
            },
        );
    }
    assert_eq!(state(&world), "Crouch");
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    assert!(
        assets.take_errors().is_empty(),
        "{:?}",
        assets.take_errors()
    );
    let _ = std::fs::remove_dir_all(&parent);
}
