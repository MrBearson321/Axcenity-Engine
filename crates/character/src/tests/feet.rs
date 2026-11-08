//! Phase 79: `FootPlacement`, with the real character controller and rays.

use std::path::Path;

use axcenity_anim::{Animators, ragdoll};
use axcenity_assets::{Assets, Rig};
use axcenity_core::{ClassName, InstanceId, PartType, Value, Vector3, World};
use glam::{Quat, Vec3};

use super::{FRAME, solid};
use crate::{Controllers, MoveInput, foot_ground};

const MODEL: &str = "assets/models/Walking/Walking.fbx";
const HEIGHT: f32 = 3.78;

fn assets() -> Assets {
    Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-character"))
}

/// A skinned character whose feet are at `feet`, with an Animator (playing
/// nothing, so it holds its rest pose) and a third-person controller.
fn character(world: &mut World, feet: Vector3, placement: bool) -> InstanceId {
    let part = world.create(ClassName::MeshPart).unwrap();
    for (name, value) in [
        ("MeshId", Value::String(MODEL.into())),
        ("Size", Value::Vector3(Vector3::new(3.89, HEIGHT, 0.72))),
        (
            "Position",
            Value::Vector3(Vector3::new(feet.x, feet.y + HEIGHT * 0.5, feet.z)),
        ),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(part, name, value).unwrap();
    }
    world.set_parent(part, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world
        .set_property(controller, "FootPlacement", Value::Bool(placement))
        .unwrap();
    world.set_parent(controller, Some(part)).unwrap();
    part
}

fn rig_of(assets: &Assets) -> std::sync::Arc<Rig> {
    assets.model(MODEL).unwrap().rig.clone().unwrap()
}

fn bone(rig: &Rig, name: &str) -> usize {
    rig.bone(&format!("mixamorig:{name}")).unwrap()
}

/// Where each of these bones is in the world, as `animators` posed `part`.
fn joint(world: &World, animators: &Animators, rig: &Rig, part: InstanceId, index: usize) -> Vec3 {
    // A character nothing poses stands in its rest pose.
    let posed = animators.poses().get(&part).map(|posed| &**posed);
    ragdoll::bone_in_world(world, part, rig, posed, index)
        .unwrap()
        .to_scale_rotation_translation()
        .2
}

/// Where a bone rests in the world for a character at the origin.
fn resting(world: &World, rig: &Rig, part: InstanceId, index: usize) -> Vec3 {
    ragdoll::bone_in_world(world, part, rig, None, index)
        .unwrap()
        .to_scale_rotation_translation()
        .2
}

/// Steps the controller until it has settled, then the Animators.
fn settle(world: &mut World, assets: &Assets, animators: &mut Animators) {
    let mut controllers = Controllers::default();
    for _ in 0..30 {
        controllers.step(world, assets, MoveInput::default(), FRAME);
    }
    for _ in 0..30 {
        animators.update(world, assets, f64::from(FRAME));
    }
    assert!(animators.take_errors().is_empty());
}

#[test]
fn a_character_standing_on_stairs_has_both_feet_on_a_step_with_no_knee_bending_backwards() {
    let assets = assets();
    let rig = rig_of(&assets);
    let mut world = World::new();
    // Two steps, side by side: the right of the character (+X) is on the
    // upper one, with its middle, and the left of it is on one 0.4 lower.
    let tread = |world: &mut World, x: f32, top: f32| {
        solid(
            world,
            PartType::Block,
            Vector3::new(10.0, 2.0, 10.0),
            Vector3::new(x, top - 1.0, 0.0),
        );
    };
    tread(&mut world, 4.9, 0.0);
    tread(&mut world, -5.1, -0.4);
    let part = character(&mut world, Vector3::new(0.0, 0.0, 0.0), true);
    let (left, right) = (bone(&rig, "LeftFoot"), bone(&rig, "RightFoot"));
    let (upper, lower) = if resting(&world, &rig, part, left).x > 0.0 {
        (left, right)
    } else {
        (right, left)
    };
    assert!(
        resting(&world, &rig, part, lower).x < -0.1,
        "the feet are apart, on either side of the seam"
    );
    let rest_lower = resting(&world, &rig, part, lower);
    let rest_upper = resting(&world, &rig, part, upper);
    let rest_hips = resting(&world, &rig, part, bone(&rig, "Hips"));

    let mut animators = Animators::with_ground(foot_ground);
    settle(&mut world, &assets, &mut animators);

    let body_y = world
        .get(part)
        .unwrap()
        .property("Position")
        .unwrap()
        .as_vector3()
        .unwrap()
        .y;
    assert!(
        (body_y - HEIGHT * 0.5).abs() < 0.05,
        "the body stands on the upper step: {body_y}"
    );
    // Each ankle is as high over the step under it as it rests over flat
    // ground.
    let on_step = |foot: usize, rest: Vec3, top: f32| {
        let at = joint(&world, &animators, &rig, part, foot);
        assert!(
            (at.y - (rest.y + top)).abs() < 0.03,
            "the foot is on its step: {at} over {top} (rests at {rest})"
        );
    };
    on_step(upper, rest_upper, 0.0);
    on_step(lower, rest_lower, -0.4);
    let hips = joint(&world, &animators, &rig, part, bone(&rig, "Hips"));
    assert!(
        hips.y < rest_hips.y - 0.3,
        "the hips went down to match: {hips} from {rest_hips}"
    );

    // The knees point the way the toes do (+Z), not behind the leg.
    for (side, foot) in [("Left", left), ("Right", right)] {
        let top = joint(
            &world,
            &animators,
            &rig,
            part,
            bone(&rig, &format!("{side}UpLeg")),
        );
        let knee = joint(
            &world,
            &animators,
            &rig,
            part,
            bone(&rig, &format!("{side}Leg")),
        );
        let ankle = joint(&world, &animators, &rig, part, foot);
        let line = (ankle - top).normalize();
        let bend = (knee - top) - line * (knee - top).dot(line);
        assert!(
            bend.z > 0.0,
            "the {side} knee bends forward: {bend} (hip {top}, knee {knee}, ankle {ankle})"
        );
    }
}

#[test]
fn on_a_slope_each_foot_follows_the_ground_and_lies_along_it() {
    let assets = assets();
    let rig = rig_of(&assets);
    let mut world = World::new();
    let slope = solid(
        &mut world,
        PartType::Block,
        Vector3::new(20.0, 2.0, 20.0),
        Vector3::new(0.0, -1.0, 0.0),
    );
    world
        .set_property(
            slope,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 0.0, 15.0)),
        )
        .unwrap();
    let part = character(&mut world, Vector3::new(0.0, 0.0, 0.0), true);
    let foot = bone(&rig, "LeftFoot");
    let rest = resting(&world, &rig, part, foot);
    let rest_turn = ragdoll::bone_in_world(&world, part, &rig, None, foot)
        .unwrap()
        .to_scale_rotation_translation()
        .1;
    let mut animators = Animators::with_ground(foot_ground);
    settle(&mut world, &assets, &mut animators);

    // The slope's top: through the block's top centre, rising toward +X.
    let tilt = 15f32.to_radians();
    let top_at = |x: f32| -1.0 + tilt.cos() + (x + tilt.sin()) * tilt.tan();
    let at = joint(&world, &animators, &rig, part, foot);
    let expected = rest.y + top_at(at.x);
    assert!(
        (at.y - expected).abs() < 0.03,
        "the ankle follows the slope: {at}, expected height {expected}"
    );
    // It leans with the ground, about the 15 degrees of the slope.
    let turn = ragdoll::bone_in_world(
        &world,
        part,
        &rig,
        animators.poses().get(&part).map(|posed| &**posed),
        foot,
    )
    .unwrap()
    .to_scale_rotation_translation()
    .1;
    let lean: Quat = turn * rest_turn.inverse();
    let tilted = (lean * Vec3::Y).angle_between(Vec3::Y).to_degrees();
    assert!(
        (tilted - 15.0).abs() < 1.5,
        "the foot lies along the slope: {tilted} degrees"
    );
}

#[test]
fn feet_stay_with_the_animation_with_foot_placement_off_and_in_the_air() {
    let assets = assets();
    let rig = rig_of(&assets);
    let mut world = World::new();
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(10.0, 2.0, 10.0),
        Vector3::new(4.9, -1.0, 0.0),
    );
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(10.0, 2.0, 10.0),
        Vector3::new(-5.1, -1.4, 0.0),
    );
    // Off: the left foot hangs over the lower step.
    let off = character(&mut world, Vector3::new(0.0, 0.0, 0.0), false);
    let foot = bone(&rig, "LeftFoot");
    let rest = resting(&world, &rig, off, foot);
    let mut animators = Animators::with_ground(foot_ground);
    settle(&mut world, &assets, &mut animators);
    let at = joint(&world, &animators, &rig, off, foot);
    assert!((at - rest).length() < 1e-2, "off: {at} rests at {rest}");

    // In the air, too high for any foot to find the ground: on, but still.
    let air = 12.0;
    let mut world = World::new();
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(10.0, 2.0, 10.0),
        Vector3::new(0.0, -1.0, 0.0),
    );
    let part = character(&mut world, Vector3::new(0.0, air, 0.0), true);
    let rest = resting(&world, &rig, part, foot);
    let mut animators = Animators::with_ground(foot_ground);
    for _ in 0..3 {
        animators.update(&mut world, &assets, f64::from(FRAME));
    }
    let at = joint(&world, &animators, &rig, part, foot);
    assert!((at - rest).length() < 1e-2, "air: {at} rests at {rest}");
}
