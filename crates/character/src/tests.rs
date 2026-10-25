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
/// whose feet are at y = 0, and a controller on the model.
fn standing() -> (World, InstanceId, InstanceId) {
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
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
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
