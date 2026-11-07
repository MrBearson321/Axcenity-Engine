use axcenity_core::{ClassName, PartType, Value, Vector3, World};
use axcenity_physics::Physics;

use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn pos(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .unwrap()
        .as_vector3()
        .unwrap()
}

fn orientation(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Orientation")
        .unwrap()
        .as_vector3()
        .unwrap()
}

/// An anchored part in the Workspace.
fn solid(world: &mut World, shape: PartType, size: Vector3, position: Vector3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    world
        .set_property(part, "Anchored", Value::Bool(true))
        .unwrap();
    world
        .set_property(part, "Shape", Value::PartType(shape))
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(size))
        .unwrap();
    world
        .set_property(part, "Position", Value::Vector3(position))
        .unwrap();
    part
}

/// A wide floor and a character standing on it: a Model, a 2×2×2 body
/// whose feet are at y = 0, and a third-person controller on the model.
fn standing() -> (World, InstanceId, InstanceId) {
    standing_as(ClassName::ThirdPersonController)
}

/// [`standing`], with a controller of `class`.
fn standing_as(class: ClassName) -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(80.0, 1.0, 80.0),
        Vector3::new(0.0, -0.5, 0.0),
    );
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Character").unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_name(body, "Body").unwrap();
    world.set_parent(body, Some(model)).unwrap();
    world
        .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0)))
        .unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
        )
        .unwrap();
    let controller = world.create(class).unwrap();
    world.set_parent(controller, Some(model)).unwrap();
    (world, controller, body)
}

fn forward() -> MoveInput {
    MoveInput {
        forward: true,
        ..MoveInput::default()
    }
}

/// Toward +Z, up a wedge, whose slope rises toward +Z.
fn back() -> MoveInput {
    MoveInput {
        back: true,
        ..MoveInput::default()
    }
}

/// Steps the character `frames` times at 60 frames a second.
fn run(world: &mut World, controllers: &mut Controllers, input: MoveInput, frames: usize) {
    let assets = Assets::default();
    for _ in 0..frames {
        controllers.step(world, &assets, input, FRAME);
    }
}

/// A slope `angle` degrees steep, 5 studs high, whose foot is at z = 2.
fn ramp(world: &mut World, angle: f32) {
    let length = 5.0 / angle.to_radians().tan();
    solid(
        world,
        PartType::Wedge,
        Vector3::new(8.0, 5.0, length),
        Vector3::new(0.0, 2.5, 2.0 + length * 0.5),
    );
}

/// A box `height` studs tall whose near face is at z = 2, deep enough that
/// the character is still on it after half a second.
fn ledge(world: &mut World, height: f32) {
    solid(
        world,
        PartType::Block,
        Vector3::new(8.0, height, 20.0),
        Vector3::new(0.0, height * 0.5, 12.0),
    );
}

fn set(world: &mut World, id: InstanceId, name: &str, value: f64) {
    world.set_property(id, name, Value::Number(value)).unwrap();
}

#[test]
fn walk_and_run_move_at_different_speeds_and_stay_upright() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "WalkSpeed", 16.0);
    set(&mut world, controller, "RunSpeed", 32.0);
    let mut controllers = Controllers::default();
    controllers.step(&mut world, &assets, forward(), 1.0);
    let walked = pos(&world, body);
    assert!((walked.z + 16.0).abs() < 1e-3, "{walked:?}");
    assert!((walked.y - 1.0).abs() < 1e-3, "{walked:?}");
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));

    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
        )
        .unwrap();
    world
        .set_property(controller, "Movement", Value::String("Run".to_owned()))
        .unwrap();
    controllers.step(&mut world, &assets, forward(), 1.0);
    let ran = pos(&world, body);
    assert!((ran.z + 32.0).abs() < 1e-3, "{ran:?}");
    assert!((ran.y - 1.0).abs() < 1e-3, "{ran:?}");
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));
}

#[test]
fn the_camera_starts_behind_the_character() {
    let (world, _, _) = standing();
    let camera = Controllers::default().view(&world).unwrap();
    assert!(camera.position.z > 8.0, "{:?}", camera.position);
    assert!(camera.position.y > 4.0, "{:?}", camera.position);
    assert!(camera.position.x.abs() < 1e-3, "{:?}", camera.position);
}

#[test]
fn a_wall_stops_the_character_and_a_crate_still_falls() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "WalkSpeed", 16.0);
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(6.0, 4.0, 1.0),
        Vector3::new(0.0, 1.0, -4.0),
    );
    let crate_id = world.create(ClassName::Part).unwrap();
    world.set_parent(crate_id, Some(world.workspace())).unwrap();
    world
        .set_property(
            crate_id,
            "Position",
            Value::Vector3(Vector3::new(0.0, 10.0, 20.0)),
        )
        .unwrap();
    world
        .set_property(
            crate_id,
            "Size",
            Value::Vector3(Vector3::new(2.0, 2.0, 2.0)),
        )
        .unwrap();

    let mut controllers = Controllers::default();
    controllers.step(&mut world, &assets, forward(), 1.0);
    let stopped = pos(&world, body);
    // The wall's near face is at z = -3.5. The body is 2 studs deep, so
    // its center stops short of that.
    assert!(stopped.z < -1.0 && stopped.z > -3.2, "{stopped:?}");
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));

    controllers.keep_kinematic(&mut world);
    assert_eq!(
        world
            .get(body)
            .unwrap()
            .property("Anchored")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut physics = Physics::with_assets(assets);
    // One call runs at most ten fixed steps, so repeat until a second
    // of falling has been simulated.
    for _ in 0..6 {
        physics.step(&mut world, 0.5);
    }
    let landed = pos(&world, crate_id);
    assert!(landed.y < 5.0, "{landed:?}");
    assert!((pos(&world, body).y - stopped.y).abs() < 1e-3);
    assert_eq!(axcenity_physics::GRAVITY, 196.2);
}

#[test]
fn noclip_flies_through_a_wall_and_up_and_teleport_puts_the_feet_where_asked() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "WalkSpeed", 16.0);
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(6.0, 4.0, 1.0),
        Vector3::new(0.0, 1.0, -4.0),
    );
    let mut controllers = Controllers::default();
    controllers.set_noclip(true);
    assert!(controllers.noclip());
    controllers.step(&mut world, &assets, forward(), 1.0);
    let through = pos(&world, body);
    assert!(through.z < -10.0, "past the wall: {through:?}");
    assert!((through.y - 1.0).abs() < 1e-3, "no gravity: {through:?}");
    let up = MoveInput {
        jump: true,
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, up, 30);
    assert!(pos(&world, body).y > 5.0, "{:?}", pos(&world, body));

    assert!(controllers.teleport(&mut world, Vector3::new(10.0, 0.0, 5.0)));
    let there = pos(&world, body);
    assert!(
        (there.x - 10.0).abs() < 1e-3
            && (there.y - 1.0).abs() < 1e-3
            && (there.z - 5.0).abs() < 1e-3,
        "{there:?}"
    );
    // Off again, it stands on the floor.
    controllers.set_noclip(false);
    run(&mut world, &mut controllers, MoveInput::default(), 30);
    assert!(
        (pos(&world, body).y - 1.0).abs() < 0.05,
        "{:?}",
        pos(&world, body)
    );
    assert!(!Controllers::default().teleport(&mut World::new(), Vector3::new(0.0, 0.0, 0.0)));
}

/// Where a crate dropped next to the character is after half a second of
/// physics, with the controller's properties set by `tune` first.
fn fallen_crate(tune: impl Fn(&mut World, InstanceId)) -> Vector3 {
    let assets = Assets::default();
    let (mut world, controller, _) = standing();
    tune(&mut world, controller);
    let crate_id = world.create(ClassName::Part).unwrap();
    world.set_parent(crate_id, Some(world.workspace())).unwrap();
    world
        .set_property(
            crate_id,
            "Position",
            Value::Vector3(Vector3::new(0.0, 10.0, 20.0)),
        )
        .unwrap();
    let mut controllers = Controllers::default();
    controllers.step(&mut world, &assets, MoveInput::default(), FRAME);
    controllers.keep_kinematic(&mut world);
    let mut physics = Physics::with_assets(assets);
    physics.step(&mut world, 0.5);
    pos(&world, crate_id)
}

#[test]
fn walk_speed_does_not_change_how_a_crate_falls() {
    let with_speed = |speed| move |world: &mut World, id| set(world, id, "WalkSpeed", speed);
    assert_eq!(
        fallen_crate(with_speed(16.0)),
        fallen_crate(with_speed(200.0))
    );
}

#[test]
fn a_jump_rises_and_lands_and_a_second_jump_in_the_air_does_nothing() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    let jump = MoveInput {
        jump: true,
        ..MoveInput::default()
    };
    controllers.step(&mut world, &assets, jump, FRAME);
    let mut peak: f32 = 0.0;
    for frame in 0..90 {
        if frame == 10 {
            // Still rising: a second jump from a script is used up and ignored.
            world
                .set_property(controller, "Jump", Value::Bool(true))
                .unwrap();
        }
        controllers.step(&mut world, &assets, MoveInput::default(), FRAME);
        if frame == 10 {
            let jump = world.get(controller).unwrap().property("Jump");
            assert_eq!(jump, Some(&Value::Bool(false)));
        }
        peak = peak.max(pos(&world, body).y - 1.0);
    }
    assert!((peak - 7.2).abs() < 0.1, "peak {peak}");
    assert!(
        (pos(&world, body).y - 1.0).abs() < 1e-3,
        "{:?}",
        pos(&world, body)
    );
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));
}

#[test]
fn jump_height_sets_how_high_a_jump_rises() {
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "JumpHeight", 2.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    world
        .set_property(controller, "Jump", Value::Bool(true))
        .unwrap();
    let mut peak: f32 = 0.0;
    for _ in 0..60 {
        run(&mut world, &mut controllers, MoveInput::default(), 1);
        peak = peak.max(pos(&world, body).y - 1.0);
    }
    assert!((peak - 2.0).abs() < 0.1, "peak {peak}");
}

/// How high a character with a controller of `class` jumps under the
/// Workspace's `gravity`.
fn jump_peak(class: ClassName, gravity: f64) -> f32 {
    let (mut world, controller, body) = standing_as(class);
    let workspace = world.workspace();
    world
        .set_property(workspace, "Gravity", Value::Number(gravity))
        .unwrap();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    world
        .set_property(controller, "Jump", Value::Bool(true))
        .unwrap();
    let mut peak: f32 = 0.0;
    for _ in 0..180 {
        run(&mut world, &mut controllers, MoveInput::default(), 1);
        peak = peak.max(pos(&world, body).y - 1.0);
    }
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3, "it lands again");
    peak
}

#[test]
fn halving_workspace_gravity_makes_both_characters_jump_higher() {
    for class in [
        ClassName::ThirdPersonController,
        ClassName::FirstPersonController,
    ] {
        let full = jump_peak(class, 196.2);
        let half = jump_peak(class, 98.1);
        // The same launch speed: half the gravity reaches twice as high.
        assert!((full - 7.2).abs() < 0.1, "{class}: {full}");
        assert!((half - 14.4).abs() < 0.2, "{class}: {half}");
    }
}

#[test]
fn a_jump_stops_under_a_ceiling() {
    let (mut world, controller, body) = standing();
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(8.0, 1.0, 8.0),
        Vector3::new(0.0, 4.5, 0.0),
    );
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    world
        .set_property(controller, "Jump", Value::Bool(true))
        .unwrap();
    let mut top: f32 = 0.0;
    for _ in 0..60 {
        run(&mut world, &mut controllers, MoveInput::default(), 1);
        top = top.max(pos(&world, body).y + 1.0);
    }
    // The ceiling's underside is at y = 4.
    assert!(top <= 4.0 && top > 3.8, "top {top}");
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3);
}

#[test]
fn walking_off_a_ledge_falls_to_the_floor_below() {
    let (mut world, _, body) = standing();
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(6.0, 6.0, 6.0),
        Vector3::new(0.0, 3.0, 0.0),
    );
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 7.0, 0.0)),
        )
        .unwrap();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!((pos(&world, body).y - 7.0).abs() < 1e-3, "stands on top");
    run(&mut world, &mut controllers, forward(), 60);
    let landed = pos(&world, body);
    assert!(landed.z < -6.0, "{landed:?}");
    assert!((landed.y - 1.0).abs() < 1e-3, "{landed:?}");
}

#[test]
fn a_step_under_the_limit_is_climbed_and_a_taller_one_stops_the_character() {
    let (mut world, _, body) = standing();
    ledge(&mut world, 0.8);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 30);
    let climbed = pos(&world, body);
    assert!(climbed.z > 4.0, "{climbed:?}");
    assert!((climbed.y - 1.8).abs() < 1e-3, "{climbed:?}");

    let (mut world, _, body) = standing();
    ledge(&mut world, 1.5);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 30);
    let stopped = pos(&world, body);
    assert!(stopped.z < 1.0 && stopped.z > 0.8, "{stopped:?}");
    assert!((stopped.y - 1.0).abs() < 1e-3, "{stopped:?}");
}

#[test]
fn a_slope_under_the_limit_is_climbed_and_a_steeper_one_stops_the_character() {
    let (mut world, _, body) = standing();
    ramp(&mut world, 30.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 30);
    let climbed = pos(&world, body);
    assert!(climbed.z > 7.0, "{climbed:?}");
    assert!(climbed.y > 3.5, "{climbed:?}");

    let (mut world, _, body) = standing();
    ramp(&mut world, 60.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 60);
    let stopped = pos(&world, body);
    // Stopped at the foot of the slope, not launched up it.
    assert!(stopped.z < 1.2, "{stopped:?}");
    assert!((stopped.y - 1.0).abs() < 0.05, "{stopped:?}");
}

#[test]
fn raising_the_limits_lets_the_character_cross_and_a_crate_falls_the_same() {
    let (mut world, controller, body) = standing();
    ledge(&mut world, 1.5);
    set(&mut world, controller, "MaxStepHeight", 2.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 30);
    let stepped = pos(&world, body);
    assert!((stepped.y - 2.5).abs() < 1e-3, "{stepped:?}");

    let (mut world, controller, body) = standing();
    ramp(&mut world, 60.0);
    set(&mut world, controller, "MaxSlopeAngle", 70.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 20);
    let climbed = pos(&world, body);
    assert!(climbed.y > 3.0, "{climbed:?}");

    // Lowering the limits stops what the defaults cross.
    let (mut world, controller, body) = standing();
    ledge(&mut world, 0.8);
    set(&mut world, controller, "MaxStepHeight", 0.5);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, back(), 30);
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3);

    let tuned = |world: &mut World, id| {
        set(world, id, "MaxStepHeight", 4.0);
        set(world, id, "MaxSlopeAngle", 80.0);
        set(world, id, "JumpHeight", 30.0);
    };
    assert_eq!(fallen_crate(|_, _| {}), fallen_crate(tuned));
}

fn size(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Size")
        .unwrap()
        .as_vector3()
        .unwrap()
}

fn crouch(world: &mut World, controller: InstanceId, crouched: bool) {
    world
        .set_property(controller, "Crouch", Value::Bool(crouched))
        .unwrap();
}

#[test]
fn crouching_lowers_the_character_and_the_camera_and_standing_restores_both() {
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "CrouchHeight", 1.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    let camera = controllers.view(&world).unwrap().position;

    crouch(&mut world, controller, true);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    // Squashed toward the feet: 1 stud tall, feet still on the floor.
    assert_eq!(size(&world, body), Vector3::new(2.0, 1.0, 2.0));
    assert!(
        (pos(&world, body).y - 0.5).abs() < 1e-3,
        "{:?}",
        pos(&world, body)
    );
    let low = controllers.view(&world).unwrap().position;
    assert!(
        (camera.y - low.y - 1.0).abs() < 1e-3,
        "{camera:?} -> {low:?}"
    );
    assert!((camera.z - low.z).abs() < 1e-3);

    crouch(&mut world, controller, false);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert_eq!(size(&world, body), Vector3::new(2.0, 2.0, 2.0));
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3);
    let back = controllers.view(&world).unwrap().position;
    assert!((back.y - camera.y).abs() < 1e-3, "{camera:?} -> {back:?}");

    // Holding C crouches too, and letting go stands.
    let held = MoveInput {
        crouch: true,
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, held, 1);
    assert_eq!(size(&world, body).y, 1.0);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert_eq!(size(&world, body).y, 2.0);
}

#[test]
fn standing_under_a_low_ceiling_waits_until_the_character_is_clear() {
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "CrouchHeight", 1.0);
    // Underside at y = 1.5, from z = -2 to z = -8.
    solid(
        &mut world,
        PartType::Block,
        Vector3::new(8.0, 2.0, 6.0),
        Vector3::new(0.0, 2.5, -5.0),
    );
    let mut controllers = Controllers::default();
    // Standing, the ceiling stops the character like a wall.
    run(&mut world, &mut controllers, forward(), 30);
    assert!(pos(&world, body).z > -1.1, "{:?}", pos(&world, body));

    // Crouched, it goes under, and asking to stand there is refused.
    crouch(&mut world, controller, true);
    run(&mut world, &mut controllers, forward(), 30);
    assert!(pos(&world, body).z < -3.5, "{:?}", pos(&world, body));
    crouch(&mut world, controller, false);
    run(&mut world, &mut controllers, MoveInput::default(), 30);
    assert_eq!(size(&world, body).y, 1.0);
    assert!((pos(&world, body).y - 0.5).abs() < 1e-3);

    // Out the far side, it stands up by itself.
    let mut stood_at = None;
    for _ in 0..90 {
        run(&mut world, &mut controllers, forward(), 1);
        if stood_at.is_none() && size(&world, body).y == 2.0 {
            stood_at = Some(pos(&world, body).z);
        }
    }
    let stood_at = stood_at.expect("stands once clear");
    // Its 1-stud footprint has to be past the ceiling's far edge at z = -8.
    assert!(stood_at < -8.9, "stood at z = {stood_at}");
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3);
}

#[test]
fn crouched_walk_and_run_are_slower_than_walking() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    crouch(&mut world, controller, true);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    controllers.step(&mut world, &assets, forward(), 1.0);
    let walked = pos(&world, body).z;
    assert!((walked + 8.0).abs() < 1e-3, "crouched walk {walked}");

    world
        .set_property(controller, "Movement", Value::String("Run".to_owned()))
        .unwrap();
    controllers.step(&mut world, &assets, forward(), 1.0);
    let ran = pos(&world, body).z - walked;
    assert!((ran + 16.0).abs() < 1e-3, "crouched run {ran}");
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));
}

/// The body's place and size after each frame of `inputs`, for a character
/// driven by a `class` controller, after `build` adds to the world.
fn course(
    class: ClassName,
    build: fn(&mut World, InstanceId),
    inputs: &[MoveInput],
) -> Vec<(Vector3, Vector3)> {
    let (mut world, controller, body) = standing_as(class);
    build(&mut world, controller);
    let mut controllers = Controllers::default();
    inputs
        .iter()
        .map(|&input| {
            run(&mut world, &mut controllers, input, 1);
            (pos(&world, body), size(&world, body))
        })
        .collect()
}

type Course = (
    &'static str,
    fn(&mut World, InstanceId),
    Vec<MoveInput>,
    fn(&[(Vector3, Vector3)]) -> bool,
);

#[test]
fn a_first_person_controller_moves_exactly_like_a_third_person_one() {
    let still = MoveInput::default();
    let jump = MoveInput {
        jump: true,
        ..still
    };
    let last = |path: &[(Vector3, Vector3)]| path.last().unwrap().0;
    let courses: Vec<Course> = vec![
        (
            "walk",
            |_, _| {},
            vec![forward(); 60],
            |path| (path.last().unwrap().0.z + 16.0).abs() < 0.1,
        ),
        (
            "run",
            |world, id| {
                world
                    .set_property(id, "Movement", Value::String("Run".to_owned()))
                    .unwrap();
            },
            vec![forward(); 60],
            |path| (path.last().unwrap().0.z + 32.0).abs() < 0.1,
        ),
        (
            "jump",
            |_, _| {},
            [vec![still, jump], vec![still; 60]].concat(),
            |path| path.iter().any(|(p, _)| p.y - 1.0 > 7.0),
        ),
        (
            "crouch",
            |world, id| {
                set(world, id, "CrouchHeight", 1.0);
                crouch(world, id, true);
            },
            vec![forward(); 60],
            |path| {
                let (p, size) = path.last().unwrap();
                size.y == 1.0 && (p.z + 8.0).abs() < 0.2
            },
        ),
        (
            "low step",
            |world, _| ledge(world, 0.8),
            vec![back(); 30],
            |path| (path.last().unwrap().0.y - 1.8).abs() < 1e-3,
        ),
        (
            "tall step",
            |world, _| ledge(world, 1.5),
            vec![back(); 30],
            |path| (path.last().unwrap().0.y - 1.0).abs() < 1e-3,
        ),
        (
            "gentle slope",
            |world, _| ramp(world, 30.0),
            vec![back(); 30],
            |path| path.last().unwrap().0.y > 3.5,
        ),
        (
            "steep slope",
            |world, _| ramp(world, 60.0),
            vec![back(); 60],
            |path| (path.last().unwrap().0.y - 1.0).abs() < 0.05,
        ),
    ];
    for (name, build, inputs, crossed) in courses {
        let third = course(ClassName::ThirdPersonController, build, &inputs);
        let first = course(ClassName::FirstPersonController, build, &inputs);
        assert_eq!(first, third, "{name}");
        assert!(crossed(&first), "{name}: ended at {:?}", last(&first));
    }
}

#[test]
fn the_first_person_camera_is_at_the_eyes_and_looks_where_the_player_looks() {
    let (mut world, controller, body) = standing_as(ClassName::FirstPersonController);
    set(&mut world, controller, "EyeHeight", 1.5);
    let mut controllers = Controllers::default();
    let start = controllers.view(&world).unwrap();
    assert_eq!(start.position, Vector3::new(0.0, 1.5, 0.0));
    assert_eq!(start.orientation, Vector3::new(0.0, 0.0, 0.0));

    // Looking left and up turns the camera where it is.
    let look = MoveInput {
        look_dx: -100.0,
        look_dy: -50.0,
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, look, 1);
    let turned = controllers.view(&world).unwrap();
    assert_eq!(turned.position, start.position);
    assert!((turned.orientation.y - 0.3_f32.to_degrees()).abs() < 1e-3);
    assert!((turned.orientation.x - 0.15_f32.to_degrees()).abs() < 1e-3);

    // Walking goes where it looks, and the camera stays at the eyes. The
    // wheel doesn't move it.
    let walk = MoveInput {
        forward: true,
        zoom: 5.0,
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, walk, 30);
    let at = pos(&world, body);
    assert!(at.x < -1.0 && at.z < -4.0, "{at:?}");
    let eyes = controllers.view(&world).unwrap().position;
    assert!((eyes.x - at.x).abs() < 1e-4 && (eyes.z - at.z).abs() < 1e-4);
    assert!((eyes.y - 1.5).abs() < 1e-4, "{eyes:?}");

    // Crouched, the eyes drop as far as the top of the body.
    set(&mut world, controller, "CrouchHeight", 1.0);
    crouch(&mut world, controller, true);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    let low = controllers.view(&world).unwrap().position;
    assert!((low.y - 0.5).abs() < 1e-4, "{low:?}");
}

/// How far the third-person camera is from the point it looks at, 4 studs
/// above the feet of a [`standing`] character.
fn zoomed(world: &World, controllers: &Controllers) -> f32 {
    let at = controllers.view(world).unwrap().position;
    let feet = Vector3::new(0.0, 4.0, 0.0);
    ((at.x - feet.x).powi(2) + (at.y - feet.y).powi(2) + (at.z - feet.z).powi(2)).sqrt()
}

fn wheel(notches: f32) -> MoveInput {
    MoveInput {
        zoom: notches,
        ..MoveInput::default()
    }
}

fn camera_distance_property(world: &World, controller: InstanceId) -> f64 {
    world
        .get(controller)
        .unwrap()
        .property("CameraDistance")
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn the_wheel_zooms_the_third_person_camera_between_its_limits() {
    let (mut world, controller, _) = standing();
    set(&mut world, controller, "CameraMinZoomDistance", 4.0);
    set(&mut world, controller, "CameraMaxZoomDistance", 20.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!((zoomed(&world, &controllers) - 12.0).abs() < 1e-3);

    // One notch in is a fifth of the distance; scripts see it.
    run(&mut world, &mut controllers, wheel(1.0), 1);
    assert!((zoomed(&world, &controllers) - 9.6).abs() < 1e-3);
    assert!((camera_distance_property(&world, controller) - 9.6).abs() < 1e-3);
    run(&mut world, &mut controllers, wheel(-1.0), 1);
    assert!(zoomed(&world, &controllers) > 11.0);

    // It stops at the limits.
    run(&mut world, &mut controllers, wheel(50.0), 1);
    assert!((zoomed(&world, &controllers) - 4.0).abs() < 1e-3);
    run(&mut world, &mut controllers, wheel(-50.0), 1);
    assert!((zoomed(&world, &controllers) - 20.0).abs() < 1e-3);

    // The same limit twice locks the distance.
    set(&mut world, controller, "CameraMinZoomDistance", 8.0);
    set(&mut world, controller, "CameraMaxZoomDistance", 8.0);
    for notches in [3.0, -3.0, 50.0] {
        run(&mut world, &mut controllers, wheel(notches), 1);
        assert!((zoomed(&world, &controllers) - 8.0).abs() < 1e-3);
    }

    // Fully in, the camera comes to half a stud from where it looks.
    set(&mut world, controller, "CameraMinZoomDistance", 0.0);
    set(&mut world, controller, "CameraMaxZoomDistance", 128.0);
    run(&mut world, &mut controllers, wheel(100.0), 1);
    assert!((zoomed(&world, &controllers) - 0.5).abs() < 1e-3);
}

fn set_movement(world: &mut World, controller: InstanceId, name: &str) {
    world
        .set_property(controller, "Movement", Value::String(name.to_owned()))
        .unwrap();
}

fn movement_of(world: &World, controller: InstanceId) -> String {
    world
        .get(controller)
        .unwrap()
        .property("Movement")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

#[test]
fn a_custom_mode_stops_built_in_motion_and_a_script_moves_the_character() {
    for class in [
        ClassName::ThirdPersonController,
        ClassName::FirstPersonController,
    ] {
        let (mut world, controller, body) = standing_as(class);
        let mut controllers = Controllers::default();
        run(&mut world, &mut controllers, forward(), 1);
        set_movement(&mut world, controller, "Swim");
        let start = pos(&world, body);
        run(&mut world, &mut controllers, forward(), 60);
        assert_eq!(pos(&world, body), start, "{class:?} walked in Swim");
        assert_eq!(movement_of(&world, controller), "Swim");

        // A jump does nothing and is used up.
        world
            .set_property(controller, "Jump", Value::Bool(true))
            .unwrap();
        run(&mut world, &mut controllers, MoveInput::default(), 30);
        assert_eq!(pos(&world, body), start);
        let jump = world.get(controller).unwrap().property("Jump");
        assert_eq!(jump, Some(&Value::Bool(false)));

        // A script lifts it 5 studs a second, and gravity doesn't pull it
        // back; the camera follows.
        let assets = Assets::default();
        let before = controllers.view(&world).unwrap().position;
        for frame in 1..=60 {
            let lifted = Vector3::new(start.x, start.y + 5.0 * frame as f32 * FRAME, start.z);
            world
                .set_property(body, "Position", Value::Vector3(lifted))
                .unwrap();
            controllers.step(&mut world, &assets, forward(), FRAME);
        }
        let lifted = pos(&world, body);
        assert!((lifted.y - start.y - 5.0).abs() < 1e-3, "{lifted:?}");
        assert_eq!((lifted.x, lifted.z), (start.x, start.z));
        let after = controllers.view(&world).unwrap().position;
        assert!((after.y - before.y - 5.0).abs() < 1e-3);
    }
}

#[test]
fn clearing_a_custom_mode_restores_run_with_jumping_and_stepping() {
    let (mut world, controller, body) = standing();
    ledge(&mut world, 0.8);
    let mut controllers = Controllers::default();
    set_movement(&mut world, controller, "Run");
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    set_movement(&mut world, controller, "Climb");
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    // A script left the character in the air.
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 6.0, 0.0)),
        )
        .unwrap();
    set_movement(&mut world, controller, "");
    run(&mut world, &mut controllers, MoveInput::default(), 60);
    assert_eq!(movement_of(&world, controller), "Run");
    assert!((pos(&world, body).y - 1.0).abs() < 1e-3, "fell and landed");

    // It runs, back toward +Z and onto the 0.8-stud step.
    let start = pos(&world, body);
    run(&mut world, &mut controllers, back(), 30);
    let ran = pos(&world, body);
    assert!((ran.z - start.z - 16.0).abs() < 0.5, "{start:?} -> {ran:?}");
    assert!((ran.y - 1.8).abs() < 1e-3, "on the step at {ran:?}");

    // And jumps.
    world
        .set_property(controller, "Jump", Value::Bool(true))
        .unwrap();
    let mut peak = ran.y;
    for _ in 0..60 {
        run(&mut world, &mut controllers, MoveInput::default(), 1);
        peak = peak.max(pos(&world, body).y);
    }
    assert!(peak - ran.y > 6.0, "rose {}", peak - ran.y);
    assert!((pos(&world, body).y - ran.y).abs() < 1e-3);
}

#[test]
fn clearing_a_custom_mode_restores_walk_or_crouch() {
    let (mut world, controller, body) = standing();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    // Never set: clearing walks.
    set_movement(&mut world, controller, "Glide");
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    set_movement(&mut world, controller, "");
    let start = pos(&world, body);
    run(&mut world, &mut controllers, forward(), 60);
    assert_eq!(movement_of(&world, controller), "Walk");
    let walked = start.z - pos(&world, body).z;
    assert!((walked - 16.0).abs() < 0.5, "walked {walked}");

    // Movement "Crouch" walks crouched, and comes back after a custom mode.
    set(&mut world, controller, "CrouchHeight", 1.0);
    set_movement(&mut world, controller, "crouch");
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!((size(&world, body).y - 1.0).abs() < 1e-3);
    set_movement(&mut world, controller, "Glide");
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    set_movement(&mut world, controller, "");
    let start = pos(&world, body);
    run(&mut world, &mut controllers, forward(), 60);
    assert_eq!(movement_of(&world, controller), "crouch");
    assert!((size(&world, body).y - 1.0).abs() < 1e-3);
    let crept = start.z - pos(&world, body).z;
    assert!((crept - 8.0).abs() < 0.5, "crept {crept}");
}

#[test]
fn a_gamepads_left_stick_walks_the_character_and_its_a_button_jumps() {
    use axcenity_core::KeyCode;
    use axcenity_core::input::{InputKind, InputMap, InputState};

    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "WalkSpeed", 16.0);
    let map = InputMap::default();
    let mut pad = InputState::default();
    pad.apply(&InputKind::PadConnected(0));
    pad.apply(&InputKind::PadStick {
        pad: 0,
        stick: KeyCode::Thumbstick1,
        position: [0.0, 1.0],
    });
    let mut controllers = Controllers::default();
    let input = MoveInput::from_actions(&map, &pad, [0.0, 0.0], 0.0, 1.0);
    controllers.step(&mut world, &assets, input, 1.0);
    let walked = pos(&world, body);
    assert!(
        (walked.z + 16.0).abs() < 1e-3,
        "pushed all the way: {walked:?}"
    );

    // Half way past the dead zone walks at half speed.
    let half =
        axcenity_core::input::STICK_DEAD_ZONE + (1.0 - axcenity_core::input::STICK_DEAD_ZONE) * 0.5;
    pad.apply(&InputKind::PadStick {
        pad: 0,
        stick: KeyCode::Thumbstick1,
        position: [half, 0.0],
    });
    let input = MoveInput::from_actions(&map, &pad, [0.0, 0.0], 0.0, 1.0);
    controllers.step(&mut world, &assets, input, 1.0);
    let sideways = pos(&world, body);
    assert!((sideways.x - 8.0).abs() < 1e-2, "half way: {sideways:?}");

    pad.apply(&InputKind::PadStick {
        pad: 0,
        stick: KeyCode::Thumbstick1,
        position: [0.0, 0.0],
    });
    pad.apply(&InputKind::PadButton {
        pad: 0,
        key: KeyCode::ButtonA,
        pressed: true,
    });
    let input = MoveInput::from_actions(&map, &pad, [0.0, 0.0], 0.0, FRAME);
    assert!(input.jump);
    run(&mut world, &mut controllers, input, 10);
    assert!(pos(&world, body).y > 1.5, "{:?}", pos(&world, body));
}

#[test]
fn holding_run_runs_a_walking_character() {
    let assets = Assets::default();
    let (mut world, controller, body) = standing();
    set(&mut world, controller, "WalkSpeed", 16.0);
    set(&mut world, controller, "RunSpeed", 32.0);
    let mut controllers = Controllers::default();
    let input = MoveInput {
        run: true,
        ..forward()
    };
    controllers.step(&mut world, &assets, input, 1.0);
    let ran = pos(&world, body);
    assert!((ran.z + 32.0).abs() < 1e-3, "{ran:?}");
}

#[test]
fn a_ragdoll_s_bodies_are_not_the_character_and_a_ragdoll_that_s_on_holds_it() {
    let (mut world, controller, body) = standing();
    let ragdoll = world.create(ClassName::Ragdoll).unwrap();
    world.set_parent(ragdoll, Some(body)).unwrap();
    let capsule = world.create(ClassName::Part).unwrap();
    world.set_parent(capsule, Some(ragdoll)).unwrap();
    let model = world.get(body).unwrap().parent().unwrap();
    assert_eq!(body_parts(&world, model), vec![body]);

    // Off, it walks; on, it stays where physics has it.
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, forward(), 30);
    let walked = pos(&world, body);
    assert!(walked.z < -1.0, "{walked:?}");
    world
        .set_property(ragdoll, "Enabled", Value::Bool(true))
        .unwrap();
    run(&mut world, &mut controllers, forward(), 30);
    assert_eq!(pos(&world, body), walked);
    let _ = controller;
}

/// Phase 46: a second character, after the first, that gets no input.
fn npc(world: &mut World, x: f32) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_parent(body, Some(model)).unwrap();
    world
        .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0)))
        .unwrap();
    world
        .set_property(body, "Position", Value::Vector3(Vector3::new(x, 1.0, 0.0)))
        .unwrap();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(model)).unwrap();
    (controller, body)
}

fn direct(world: &mut World, controller: InstanceId, x: f32, z: f32) {
    world
        .set_property(
            controller,
            "MoveDirection",
            Value::Vector3(Vector3::new(x, 0.0, z)),
        )
        .unwrap();
}

#[test]
fn a_character_without_input_walks_its_move_direction_and_turns_to_face_it() {
    let (mut world, player_controller, player) = standing();
    let (npc, body) = npc(&mut world, 10.0);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 10);
    assert_eq!(pos(&world, body), Vector3::new(10.0, 1.0, 0.0), "it waits");
    direct(&mut world, npc, 2.0, 0.0);
    run(&mut world, &mut controllers, MoveInput::default(), 60);
    let walked = pos(&world, body);
    // Its length is kept to 1: WalkSpeed across.
    assert!((walked.x - 26.0).abs() < 0.05, "{walked:?}");
    assert!((orientation(&world, body).y + 90.0).abs() < 1e-3);
    // The player's character stood still and didn't turn.
    assert_eq!(pos(&world, player), Vector3::new(0.0, 1.0, 0.0));
    assert_eq!(orientation(&world, player), Vector3::new(0.0, 0.0, 0.0));
    // With no key held, the player's character walks its MoveDirection too.
    direct(&mut world, npc, 0.0, 0.0);
    direct(&mut world, player_controller, 0.0, 0.5);
    run(&mut world, &mut controllers, MoveInput::default(), 60);
    assert!((pos(&world, player).z - 8.0).abs() < 0.05);
    assert!((pos(&world, body).x - walked.x).abs() < 1e-3, "it stopped");
    // A key held wins over it.
    run(&mut world, &mut controllers, forward(), 60);
    assert!((pos(&world, player).z + 8.0).abs() < 0.05);
}

#[test]
fn auto_rotate_off_or_platform_stand_on_keeps_the_character_as_it_is() {
    let (mut world, controller, body) = standing();
    world
        .set_property(controller, "AutoRotate", Value::Bool(false))
        .unwrap();
    let right = MoveInput {
        right: true,
        ..MoveInput::default()
    };
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, right, 30);
    assert!(pos(&world, body).x > 7.0);
    assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));
    world
        .set_property(controller, "PlatformStand", Value::Bool(true))
        .unwrap();
    world
        .set_property(controller, "Jump", Value::Bool(true))
        .unwrap();
    let at = pos(&world, body);
    run(&mut world, &mut controllers, right, 30);
    assert_eq!(pos(&world, body), at);
}

#[test]
fn a_dead_character_is_left_to_physics() {
    let (mut world, controller, body) = standing();
    world
        .set_property(body, "Anchored", Value::Bool(false))
        .unwrap();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    controllers.keep_kinematic(&mut world);
    assert_eq!(
        world.get(body).unwrap().property("Anchored"),
        Some(&Value::Bool(true))
    );
    world
        .set_property(body, "Anchored", Value::Bool(false))
        .unwrap();
    set_movement(&mut world, controller, DEAD);
    run(&mut world, &mut controllers, forward(), 30);
    controllers.keep_kinematic(&mut world);
    assert_eq!(pos(&world, body), Vector3::new(0.0, 1.0, 0.0), "not walked");
    assert_eq!(
        world.get(body).unwrap().property("Anchored"),
        Some(&Value::Bool(false))
    );
}

mod seat;

#[test]
fn a_players_character_comes_before_the_first_controller() {
    let (mut world, controller, _) = standing();
    let first = world.get(controller).unwrap().parent();
    assert_eq!(player_character(&world), first);
    let second = rig::character(&mut world, Vector3::new(10.0, 0.0, 0.0), "");
    world.set_parent(second, Some(world.workspace())).unwrap();
    assert_eq!(player_character(&world), first);

    let player = world.create(ClassName::Player).unwrap();
    world.set_parent(player, Some(world.players())).unwrap();
    world
        .set_property(player, "Character", Value::Instance(Some(second)))
        .unwrap();
    assert_eq!(player_character(&world), Some(second));
}

/// A 10×1×10 platform under the standing character (its top at y = 0),
/// with the floor far below.
fn on_a_platform() -> (World, InstanceId, InstanceId, InstanceId) {
    let (mut world, controller, body) = standing();
    let floor = world.descendants(world.workspace())[0];
    world
        .set_property(
            floor,
            "Position",
            Value::Vector3(Vector3::new(0.0, -40.0, 0.0)),
        )
        .unwrap();
    let platform = solid(
        &mut world,
        PartType::Block,
        Vector3::new(10.0, 1.0, 10.0),
        Vector3::new(0.0, -0.5, 0.0),
    );
    (world, controller, body, platform)
}

#[test]
fn a_standing_character_rides_a_platform_that_moves_and_rises_under_it() {
    let (mut world, mut controllers, body, platform) = {
        let (world, _, body, platform) = on_a_platform();
        (world, Controllers::default(), body, platform)
    };
    run(&mut world, &mut controllers, MoveInput::default(), 5);
    let start = pos(&world, body);
    // The platform goes 6 studs across and 3 up over two seconds; each
    // step the character keeps its place on it, its feet on its top.
    for frame in 1..=120 {
        let share = frame as f32 / 120.0;
        world
            .set_property(
                platform,
                "Position",
                Value::Vector3(Vector3::new(6.0 * share, -0.5 + 3.0 * share, 0.0)),
            )
            .unwrap();
        run(&mut world, &mut controllers, MoveInput::default(), 1);
        let here = pos(&world, body);
        assert!(
            (here.x - (start.x + 6.0 * share)).abs() < 1e-3,
            "{frame}: {here:?}"
        );
        assert!(
            (here.y - (start.y + 3.0 * share)).abs() < 1e-3,
            "{frame}: {here:?}"
        );
    }
    // And walking on it still walks.
    run(&mut world, &mut controllers, forward(), 10);
    assert!(pos(&world, body).z < start.z - 1.0);
}

#[test]
fn a_platform_that_turns_carries_the_character_round_with_it() {
    let (mut world, _, body, platform) = on_a_platform();
    let mut controllers = Controllers::default();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(3.0, 1.0, 0.0)),
        )
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 5);
    for frame in 1..=90 {
        world
            .set_property(
                platform,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, frame as f32, 0.0)),
            )
            .unwrap();
        run(&mut world, &mut controllers, MoveInput::default(), 1);
    }
    // A quarter turn round Y takes +X to -Z.
    let here = pos(&world, body);
    assert!(
        here.x.abs() < 0.01 && (here.z + 3.0).abs() < 0.01,
        "{here:?}"
    );
}

#[test]
fn a_platform_put_far_away_leaves_the_character_to_fall() {
    let (mut world, _, body, platform) = on_a_platform();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 5);
    world
        .set_property(
            platform,
            "Position",
            Value::Vector3(Vector3::new(100.0, -0.5, 0.0)),
        )
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 10);
    let here = pos(&world, body);
    assert!(here.x.abs() < 1e-3 && here.y < 0.5, "{here:?}");
}
