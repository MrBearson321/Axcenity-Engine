//! Phase 79: IKControls posing the `hello-character` example's Remy.

use std::path::Path;

use axcenity_assets::{Assets, Rig};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::Vec3;

use crate::{Animators, ragdoll};

const MODEL: &str = "assets/models/Walking/Walking.fbx";
const STATE_MACHINE: &str = "assets/animations/Dummy/Dummy.statemachine.json";
const FRAME: f64 = 1.0 / 60.0;

fn assets() -> Assets {
    Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-character"))
}

/// A standing character: a skinned MeshPart at the origin, facing +Z as the
/// model does, with an Animator playing the example's idle.
fn character(world: &mut World) -> InstanceId {
    let part = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(part, "MeshId", Value::String(MODEL.into()))
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(3.89, 3.78, 0.72)))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world
        .set_property(
            animator,
            "StateMachine",
            Value::String(STATE_MACHINE.into()),
        )
        .unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    part
}

/// A small part in the Workspace at `at`.
fn marker(world: &mut World, at: Vector3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world
        .set_property(part, "Position", Value::Vector3(at))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

/// An IKControl in `part`.
fn control(
    world: &mut World,
    part: InstanceId,
    kind: &str,
    end: &str,
    root: &str,
    target: InstanceId,
) -> InstanceId {
    let ik = world.create(ClassName::IKControl).unwrap();
    for (name, value) in [
        ("Type", Value::String(kind.into())),
        ("EndEffector", Value::String(end.into())),
        ("ChainRoot", Value::String(root.into())),
        ("Target", Value::Instance(Some(target))),
        ("SmoothTime", Value::Number(0.0)),
    ] {
        world.set_property(ik, name, value).unwrap();
    }
    world.set_parent(ik, Some(part)).unwrap();
    ik
}

fn rig_of(assets: &Assets) -> std::sync::Arc<Rig> {
    assets.model_blocking(MODEL).unwrap().rig.clone().unwrap()
}

fn bone(rig: &Rig, name: &str) -> usize {
    rig.bone(&format!("mixamorig:{name}")).unwrap()
}

/// Where the bone is in the world, and which way the character's forward
/// (+Z in the model) points from it, as `animators` posed `part`.
fn head_of(
    world: &World,
    animators: &Animators,
    rig: &Rig,
    part: InstanceId,
    index: usize,
) -> (Vec3, Vec3) {
    let posed = animators.poses().get(&part).expect("posed");
    let matrix = ragdoll::bone_in_world(world, part, rig, Some(posed), index).unwrap();
    let rest = rig.world(&rig.rest_pose())[index]
        .to_scale_rotation_translation()
        .1;
    let (_, rotation, position) = matrix.to_scale_rotation_translation();
    let forward = rotation * (rest.inverse() * Vec3::Z);
    (position, forward.normalize())
}

fn step(animators: &mut Animators, world: &mut World, assets: &Assets, frames: usize) {
    for _ in 0..frames {
        animators.update(world, assets, FRAME);
    }
}

#[test]
fn an_npc_with_a_look_at_ikcontrol_turns_its_head_to_follow_the_player_within_its_limits() {
    let assets = assets();
    let rig = rig_of(&assets);
    let head = bone(&rig, "Head");
    let mut world = World::new();
    let npc = character(&mut world);
    let player = marker(&mut world, Vector3::new(4.0, 2.0, 8.0));
    control(&mut world, npc, "LookAt", "Head", "Neck", player);
    let mut animators = Animators::new();
    step(&mut animators, &mut world, &assets, 5);
    assert!(animators.take_errors().is_empty());

    let (at, facing) = head_of(&world, &animators, &rig, npc, head);
    let to_player = (Vec3::new(4.0, 2.0, 8.0) - at).normalize();
    assert!(
        facing.angle_between(to_player) < 0.05,
        "it looks at the player: {facing} toward {to_player}"
    );

    // The player walks to the other side; the head follows.
    let to = Vector3::new(-6.0, 2.0, 4.0);
    world
        .set_property(player, "Position", Value::Vector3(to))
        .unwrap();
    step(&mut animators, &mut world, &assets, 5);
    let (at, facing) = head_of(&world, &animators, &rig, npc, head);
    let to_player = (Vec3::new(to.x, to.y, to.z) - at).normalize();
    assert!(
        facing.angle_between(to_player) < 0.05,
        "the head follows the player: {facing} toward {to_player}"
    );

    // Behind it, the head goes as far as its limit and no further.
    world
        .set_property(
            player,
            "Position",
            Value::Vector3(Vector3::new(1.0, 2.0, -10.0)),
        )
        .unwrap();
    step(&mut animators, &mut world, &assets, 5);
    let (_, facing) = head_of(&world, &animators, &rig, npc, head);
    let turned = facing.angle_between(Vec3::Z).to_degrees();
    assert!(
        (turned - 75.0).abs() < 2.0,
        "held at MaxAngle (75), turned {turned}"
    );
}

#[test]
fn turning_ik_off_returns_to_the_plain_animation_within_smooth_time() {
    let assets = assets();
    let mut world = World::new();
    let plain = character(&mut world);
    let npc = character(&mut world);
    let target = marker(&mut world, Vector3::new(6.0, 2.0, 2.0));
    let ik = control(&mut world, npc, "LookAt", "Head", "Spine2", target);
    world
        .set_property(ik, "SmoothTime", Value::Number(0.25))
        .unwrap();
    let mut animators = Animators::new();
    step(&mut animators, &mut world, &assets, 20);
    let off_by = |animators: &Animators| -> f32 {
        let joints = |part| animators.poses()[&part].joints().to_vec();
        let (a, b) = (joints(plain), joints(npc));
        a.iter()
            .zip(&b)
            .flat_map(|(a, b)| {
                a.to_cols_array()
                    .into_iter()
                    .zip(b.to_cols_array())
                    .map(|(a, b)| (a - b).abs())
                    .collect::<Vec<f32>>()
            })
            .fold(0.0, f32::max)
    };
    let solved = off_by(&animators);
    assert!(solved > 0.01, "the IK moved the head: off by {solved}");

    world
        .set_property(ik, "Enabled", Value::Bool(false))
        .unwrap();
    step(&mut animators, &mut world, &assets, 6);
    let partway = off_by(&animators);
    assert!(
        partway > 1e-3 && partway < solved,
        "it fades: {partway} of {solved}"
    );
    // SmoothTime is 15 frames; one more for the frame it was turned off in.
    step(&mut animators, &mut world, &assets, 10);
    let done = off_by(&animators);
    assert!(done < 1e-4, "the plain animation again: off by {done}");

    // Weight 0 fades the same way when it's turned back on and off.
    world
        .set_property(ik, "Enabled", Value::Bool(true))
        .unwrap();
    step(&mut animators, &mut world, &assets, 20);
    assert!(off_by(&animators) > 0.01, "on again");
    world
        .set_property(ik, "Weight", Value::Number(0.0))
        .unwrap();
    step(&mut animators, &mut world, &assets, 16);
    assert!(off_by(&animators) < 1e-4, "Weight 0 is the plain animation");
}

#[test]
fn a_position_ikcontrol_puts_the_hand_on_its_target_bending_toward_its_pole() {
    let assets = assets();
    let rig = rig_of(&assets);
    let hand = bone(&rig, "RightHand");
    let forearm = bone(&rig, "RightForeArm");
    let mut world = World::new();
    let part = character(&mut world);
    let target = marker(&mut world, Vector3::new(-0.8, 1.2, 0.8));
    let pole = marker(&mut world, Vector3::new(-1.0, -4.0, 0.0));
    let ik = control(&mut world, part, "Position", "RightHand", "", target);
    world
        .set_property(ik, "Pole", Value::Instance(Some(pole)))
        .unwrap();
    let mut animators = Animators::new();
    step(&mut animators, &mut world, &assets, 3);
    assert!(animators.take_errors().is_empty());
    let (at, _) = head_of(&world, &animators, &rig, part, hand);
    assert!(
        at.distance(Vec3::new(-0.8, 1.2, 0.8)) < 0.02,
        "the hand reached it: {at}"
    );
    let (elbow, _) = head_of(&world, &animators, &rig, part, forearm);
    assert!(elbow.y < at.y, "the elbow bends down, to the pole: {elbow}");
}

#[test]
fn rotation_and_transform_turn_the_bone_as_the_target_is_turned() {
    let assets = assets();
    let rig = rig_of(&assets);
    let head = bone(&rig, "Head");
    let mut world = World::new();
    let part = character(&mut world);
    // Turned a quarter around Y, so what faced +Z now faces +X.
    let target = marker(&mut world, Vector3::new(0.0, 3.0, 0.0));
    world
        .set_property(
            target,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    let ik = control(&mut world, part, "Rotation", "Head", "", target);
    let mut animators = Animators::new();
    step(&mut animators, &mut world, &assets, 2);
    let (at_rest, facing) = head_of(&world, &animators, &rig, part, head);
    assert!(facing.angle_between(Vec3::X) < 0.01, "{facing}");

    // Transform also moves it there, with the bones above it.
    world
        .set_property(ik, "Type", Value::String("Transform".into()))
        .unwrap();
    let goal = at_rest + Vec3::new(0.2, -0.1, 0.1);
    world
        .set_property(
            target,
            "Position",
            Value::Vector3(Vector3::new(goal.x, goal.y, goal.z)),
        )
        .unwrap();
    step(&mut animators, &mut world, &assets, 2);
    let (at, facing) = head_of(&world, &animators, &rig, part, head);
    assert!(at.distance(goal) < 0.02, "{at} toward {goal}");
    assert!(facing.angle_between(Vec3::X) < 0.01, "{facing}");
}

#[test]
fn an_ikcontrol_naming_a_bone_the_skeleton_lacks_is_reported_once_and_leaves_the_pose() {
    let assets = assets();
    let mut world = World::new();
    let part = character(&mut world);
    let target = marker(&mut world, Vector3::new(1.0, 1.0, 1.0));
    control(&mut world, part, "LookAt", "Tail", "", target);
    let mut animators = Animators::new();
    step(&mut animators, &mut world, &assets, 3);
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("EndEffector `Tail`"), "{errors:?}");
    assert!(animators.poses().contains_key(&part));
}
