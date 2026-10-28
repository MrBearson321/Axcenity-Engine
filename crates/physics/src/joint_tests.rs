//! Phase 36: hinges, motors, servos, springs, ropes and the other joints.

use axcenity_core::{ClassName, EnumItem, InstanceId, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, size: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn attachment(world: &mut World, part: InstanceId, at: Vector3, turn: Vector3) -> InstanceId {
    let id = world.create(ClassName::Attachment).unwrap();
    world.set_parent(id, Some(part)).unwrap();
    set(world, id, "Position", Value::Vector3(at));
    set(world, id, "Orientation", Value::Vector3(turn));
    id
}

fn constraint(
    world: &mut World,
    class: ClassName,
    attachment0: InstanceId,
    attachment1: InstanceId,
) -> InstanceId {
    let id = world.create(class).unwrap();
    let parent = world.get(attachment0).unwrap().parent().unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    set(world, id, "Attachment0", Value::Instance(Some(attachment0)));
    set(world, id, "Attachment1", Value::Instance(Some(attachment1)));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn number(world: &mut World, id: InstanceId, name: &str, n: f64) {
    set(world, id, name, Value::Number(n));
}

fn actuator(world: &mut World, id: InstanceId, name: &str, item: &str) {
    let item = EnumItem {
        enum_name: "ActuatorType".to_owned(),
        item: item.to_owned(),
    };
    set(world, id, name, Value::EnumItem(item));
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn current(world: &World, id: InstanceId, name: &str) -> f64 {
    joints::current(world, id, name).unwrap()
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
    }
}

/// Attachments turned this way point their X axis (the hinge's) up.
const UP: Vector3 = Vector3::new(0.0, 0.0, 90.0);

/// An anchored post and a door hinged to its side around a vertical axis,
/// limited to 0..110 degrees. Returns the door and the hinge.
fn door(world: &mut World) -> (InstanceId, InstanceId) {
    let post = part(world, v(0.0, 5.0, 0.0), v(1.0, 10.0, 1.0), true);
    let door = part(world, v(2.5, 5.0, 0.0), v(4.0, 8.0, 0.5), false);
    let a0 = attachment(world, post, v(0.5, 0.0, 0.0), UP);
    let a1 = attachment(world, door, v(-2.0, 0.0, 0.0), UP);
    let hinge = constraint(world, ClassName::HingeConstraint, a0, a1);
    set(world, hinge, "LimitsEnabled", Value::Bool(true));
    number(world, hinge, "LowerAngle", 0.0);
    number(world, hinge, "UpperAngle", 110.0);
    (door, hinge)
}

#[test]
fn a_hinged_door_swings_when_pushed_and_stops_at_its_limits() {
    let mut world = World::new();
    let (door, hinge) = door(&mut world);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    assert!(current(&world, hinge, "CurrentAngle").abs() < 1.0);
    // It hangs on its hinge without falling.
    assert!((vector(&world, door, "Position").y - 5.0).abs() < 0.1);

    // Pushed open hard, it swings to its upper limit and stays within it.
    set(
        &mut world,
        door,
        "AssemblyAngularVelocity",
        Value::Vector3(v(0.0, 6.0, 0.0)),
    );
    let mut widest: f64 = 0.0;
    for _ in 0..120 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        widest = widest.max(current(&world, hinge, "CurrentAngle"));
    }
    assert!(widest > 105.0, "it swung open to {widest}");
    assert!(widest < 113.0, "it went past its limit: {widest}");
    // The door still turns around the post's edge.
    let at = vector(&world, door, "Position");
    let from_hinge = ((at.x - 0.5).powi(2) + at.z.powi(2)).sqrt();
    assert!((from_hinge - 2.0).abs() < 0.1, "{at:?}");

    // Pushed shut, it stops at 0 rather than swinging through the post.
    set(
        &mut world,
        door,
        "AssemblyAngularVelocity",
        Value::Vector3(v(0.0, -6.0, 0.0)),
    );
    let mut narrowest: f64 = 180.0;
    for _ in 0..120 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        narrowest = narrowest.min(current(&world, hinge, "CurrentAngle"));
    }
    assert!(narrowest > -3.0, "it went past 0: {narrowest}");
    assert!(narrowest < 5.0, "it swung shut to {narrowest}");
}

/// An anchored post and an arm hinged to it around a horizontal axis (X),
/// sticking out sideways so gravity pulls it down. Returns the hinge.
fn arm(world: &mut World) -> InstanceId {
    let post = part(world, v(0.0, 10.0, 0.0), v(1.0, 1.0, 1.0), true);
    let arm = part(world, v(1.0, 10.0, 3.0), v(1.0, 1.0, 5.0), false);
    let a0 = attachment(world, post, v(0.5, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(world, arm, v(-0.5, 0.0, -3.0), v(0.0, 0.0, 0.0));
    constraint(world, ClassName::HingeConstraint, a0, a1)
}

#[test]
fn a_servo_hinge_turns_to_its_target_angle_and_holds_it_under_load() {
    let mut world = World::new();
    let hinge = arm(&mut world);
    actuator(&mut world, hinge, "ActuatorType", "Servo");
    number(&mut world, hinge, "TargetAngle", 30.0);
    number(&mut world, hinge, "AngularSpeed", 3.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0);
    let angle = current(&world, hinge, "CurrentAngle");
    assert!((angle - 30.0).abs() < 1.0, "it turned to {angle}");
    // Gravity keeps pulling on the arm, and it keeps holding.
    run(&mut physics, &mut world, 3.0);
    let angle = current(&world, hinge, "CurrentAngle");
    assert!((angle - 30.0).abs() < 1.0, "it sagged to {angle}");

    // A new target, the other way.
    number(&mut world, hinge, "TargetAngle", -60.0);
    run(&mut physics, &mut world, 0.2);
    let moving = current(&world, hinge, "CurrentAngle");
    assert!(moving < 29.0 && moving > -60.0, "on its way: {moving}");
    run(&mut physics, &mut world, 2.0);
    let angle = current(&world, hinge, "CurrentAngle");
    assert!((angle + 60.0).abs() < 1.0, "it turned to {angle}");
}

#[test]
fn a_servo_too_weak_for_its_load_sags() {
    let mut world = World::new();
    let hinge = arm(&mut world);
    actuator(&mut world, hinge, "ActuatorType", "Servo");
    number(&mut world, hinge, "TargetAngle", 30.0);
    number(&mut world, hinge, "ServoMaxTorque", 10.0);
    let mut physics = Physics::new();
    let mut lowest: f64 = 0.0;
    for _ in 0..120 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        // Gravity turns the arm the positive way, down.
        lowest = lowest.max(current(&world, hinge, "CurrentAngle"));
    }
    assert!(lowest > 80.0, "the arm swings down: {lowest}");
}

#[test]
fn a_motor_hinge_spins_a_wheel_at_its_angular_velocity() {
    let mut world = World::new();
    let axle = part(&mut world, v(0.0, 10.0, 0.0), v(1.0, 1.0, 1.0), true);
    let wheel = part(&mut world, v(1.0, 10.0, 0.0), v(1.0, 4.0, 4.0), false);
    set(
        &mut world,
        wheel,
        "Shape",
        Value::PartType(axcenity_core::PartType::Cylinder),
    );
    let a0 = attachment(&mut world, axle, v(0.5, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, wheel, v(-0.5, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let hinge = constraint(&mut world, ClassName::HingeConstraint, a0, a1);
    actuator(&mut world, hinge, "ActuatorType", "Motor");
    number(&mut world, hinge, "AngularVelocity", 4.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let spin = vector(&world, wheel, "AssemblyAngularVelocity");
    assert!((spin.x - 4.0).abs() < 0.1, "{spin:?}");
    assert!(spin.y.abs() < 0.1 && spin.z.abs() < 0.1, "{spin:?}");
    let at = vector(&world, wheel, "Position");
    assert!(
        (at.y - 10.0).abs() < 0.05,
        "the wheel stays on its axle: {at:?}"
    );

    // Turned off, the motor stops driving it.
    actuator(&mut world, hinge, "ActuatorType", "None");
    number(&mut world, hinge, "AngularVelocity", 0.0);
    run(&mut physics, &mut world, 0.5);
    let spin = vector(&world, wheel, "AssemblyAngularVelocity");
    assert!(spin.x > 2.0, "it coasts: {spin:?}");
}

#[test]
fn a_part_on_a_spring_bounces_and_settles_at_its_free_length() {
    let mut world = World::new();
    // No gravity, so it settles at the free length itself.
    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", Value::Number(0.0));
    let anchor = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), true);
    let ball = part(&mut world, v(9.0, 5.0, 0.0), v(1.0, 1.0, 1.0), false);
    let a0 = attachment(&mut world, anchor, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, ball, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let spring = constraint(&mut world, ClassName::SpringConstraint, a0, a1);
    number(&mut world, spring, "FreeLength", 5.0);
    number(&mut world, spring, "Stiffness", 100.0);
    number(&mut world, spring, "Damping", 2.0);
    let mut physics = Physics::new();
    let mut shortest: f64 = 9.0;
    for _ in 0..60 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        shortest = shortest.min(current(&world, spring, "CurrentLength"));
    }
    assert!(
        shortest < 4.5,
        "it bounces past its free length: {shortest}"
    );
    run(&mut physics, &mut world, 15.0);
    let length = current(&world, spring, "CurrentLength");
    assert!((length - 5.0).abs() < 0.05, "it settled at {length}");
}

#[test]
fn a_spring_never_stretches_past_its_max_length() {
    let mut world = World::new();
    let anchor = part(&mut world, v(0.0, 20.0, 0.0), v(1.0, 1.0, 1.0), true);
    let weight = part(&mut world, v(0.0, 15.0, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, anchor, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, weight, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let spring = constraint(&mut world, ClassName::SpringConstraint, a0, a1);
    number(&mut world, spring, "Stiffness", 1.0);
    number(&mut world, spring, "MaxLength", 8.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0);
    let length = current(&world, spring, "CurrentLength");
    assert!(
        (length - 8.0).abs() < 0.1,
        "it hangs at its max length: {length}"
    );
}

/// An anchored beam high up, and a crate `height` up (1 is on the floor)
/// tied to it by a rope of `length`. Returns the crate and the rope.
fn crane(world: &mut World, height: f32, length: f64) -> (InstanceId, InstanceId) {
    part(world, v(0.0, -0.5, 0.0), v(40.0, 1.0, 40.0), true);
    let beam = part(world, v(0.0, 20.0, 0.0), v(4.0, 1.0, 1.0), true);
    let crate_ = part(world, v(0.0, height, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(world, beam, v(0.0, -0.5, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(world, crate_, v(0.0, 1.0, 0.0), v(0.0, 0.0, 0.0));
    let rope = constraint(world, ClassName::RopeConstraint, a0, a1);
    number(world, rope, "Length", length);
    (crate_, rope)
}

#[test]
fn a_rope_only_keeps_parts_from_going_too_far_apart() {
    let mut world = World::new();
    // Slack: the crate sits on the floor.
    let (crate_, _) = crane(&mut world, 1.0, 25.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    assert!((vector(&world, crate_, "Position").y - 1.0).abs() < 0.05);

    // Dropped from higher up, the crate hangs from it.
    let mut world = World::new();
    let (crate_, rope) = crane(&mut world, 12.0, 10.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0);
    let length = current(&world, rope, "CurrentLength");
    assert!((length - 10.0).abs() < 0.1, "{length}");
    let y = vector(&world, crate_, "Position").y;
    assert!((y - 8.5).abs() < 0.2, "it hangs: {y}");
}

#[test]
fn a_rope_winch_lifts_a_crate() {
    let mut world = World::new();
    let (crate_, rope) = crane(&mut world, 1.0, 18.5);
    set(&mut world, rope, "WinchEnabled", Value::Bool(true));
    number(&mut world, rope, "WinchTarget", 4.0);
    number(&mut world, rope, "WinchSpeed", 4.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let rising = vector(&world, crate_, "Position").y;
    assert!(rising > 2.0 && rising < 10.0, "it's lifting: {rising}");
    run(&mut physics, &mut world, 4.0);
    let length = current(&world, rope, "CurrentLength");
    assert!((length - 4.0).abs() < 0.1, "reeled in to {length}");
    let y = vector(&world, crate_, "Position").y;
    assert!((y - 14.5).abs() < 0.2, "lifted to {y}");

    // A winch too weak for the crate can't lift it.
    let mut world = World::new();
    let (crate_, rope) = crane(&mut world, 1.0, 18.5);
    set(&mut world, rope, "WinchEnabled", Value::Bool(true));
    number(&mut world, rope, "WinchTarget", 4.0);
    number(&mut world, rope, "WinchForce", 100.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0);
    let y = vector(&world, crate_, "Position").y;
    assert!(y < 2.0, "it stays down: {y}");
}

#[test]
fn a_rod_holds_its_length_and_a_ball_socket_holds_its_point() {
    let mut world = World::new();
    let top = part(&mut world, v(0.0, 20.0, 0.0), v(1.0, 1.0, 1.0), true);
    let bob = part(&mut world, v(6.0, 20.0, 0.0), v(1.0, 1.0, 1.0), false);
    let a0 = attachment(&mut world, top, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, bob, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let rod = constraint(&mut world, ClassName::RodConstraint, a0, a1);
    number(&mut world, rod, "Length", 6.0);

    let pivot = part(&mut world, v(20.0, 20.0, 0.0), v(1.0, 1.0, 1.0), true);
    let pendulum = part(&mut world, v(23.0, 20.0, 0.0), v(5.0, 1.0, 1.0), false);
    let b0 = attachment(&mut world, pivot, v(0.5, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let b1 = attachment(&mut world, pendulum, v(-2.5, 0.0, 0.0), v(0.0, 0.0, 0.0));
    constraint(&mut world, ClassName::BallSocketConstraint, b0, b1);

    let mut physics = Physics::new();
    let (mut lowest, mut swung) = (20.0_f32, 20.0_f32);
    for _ in 0..90 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        let length = current(&world, rod, "CurrentLength");
        assert!((length - 6.0).abs() < 0.1, "{length}");
        lowest = lowest.min(vector(&world, bob, "Position").y);
        // The pendulum swings down from the pivot's side, its end on it.
        let at = vector(&world, pendulum, "Position");
        swung = swung.min(at.y);
        let reach = ((at.x - 20.5).powi(2) + (at.y - 20.0).powi(2) + at.z.powi(2)).sqrt();
        assert!((reach - 2.5).abs() < 0.1, "{reach}");
    }
    assert!(lowest < 15.0, "the rod swings down: {lowest}");
    assert!(swung < 18.0, "the pendulum swings down: {swung}");
}

#[test]
fn a_prismatic_servo_slides_to_its_target_within_its_limits() {
    let mut world = World::new();
    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", Value::Number(0.0));
    let rail = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), true);
    let slider = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), false);
    let a0 = attachment(&mut world, rail, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, slider, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let prismatic = constraint(&mut world, ClassName::PrismaticConstraint, a0, a1);
    actuator(&mut world, prismatic, "ActuatorType", "Servo");
    number(&mut world, prismatic, "TargetPosition", 3.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0);
    let at = vector(&world, slider, "Position");
    assert!(
        (at.x - 3.0).abs() < 0.05 && at.y == 5.0 && at.z == 0.0,
        "{at:?}"
    );
    assert!((current(&world, prismatic, "CurrentPosition") - 3.0).abs() < 0.05);

    // Past its upper limit, it stops there.
    set(&mut world, prismatic, "LimitsEnabled", Value::Bool(true));
    number(&mut world, prismatic, "UpperLimit", 4.0);
    number(&mut world, prismatic, "TargetPosition", 8.0);
    run(&mut physics, &mut world, 2.0);
    let position = current(&world, prismatic, "CurrentPosition");
    assert!((position - 4.0).abs() < 0.1, "{position}");
}

#[test]
fn a_cylindrical_constraint_slides_and_spins_on_one_axis() {
    let mut world = World::new();
    let workspace = world.workspace();
    set(&mut world, workspace, "Gravity", Value::Number(0.0));
    let rail = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), true);
    let drill = part(&mut world, v(0.0, 5.0, 0.0), v(1.0, 1.0, 1.0), false);
    let a0 = attachment(&mut world, rail, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let a1 = attachment(&mut world, drill, v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0));
    let cylindrical = constraint(&mut world, ClassName::CylindricalConstraint, a0, a1);
    actuator(&mut world, cylindrical, "ActuatorType", "Motor");
    number(&mut world, cylindrical, "Velocity", 2.0);
    actuator(&mut world, cylindrical, "AngularActuatorType", "Motor");
    number(&mut world, cylindrical, "AngularVelocity", 3.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let velocity = vector(&world, drill, "AssemblyLinearVelocity");
    let spin = vector(&world, drill, "AssemblyAngularVelocity");
    assert!((velocity.x - 2.0).abs() < 0.05, "{velocity:?}");
    assert!((spin.x - 3.0).abs() < 0.05, "{spin:?}");
    assert!(
        velocity.y.abs() < 0.01 && spin.y.abs() < 0.01,
        "{velocity:?} {spin:?}"
    );
    assert!((current(&world, cylindrical, "CurrentPosition") - 2.0).abs() < 0.1);
}

#[test]
fn a_disabled_constraint_holds_nothing_and_enabling_it_joins_again() {
    let mut world = World::new();
    let (crate_, rope) = crane(&mut world, 12.0, 10.0);
    // Lift it off the floor first: hanging from the rope.
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    assert!((vector(&world, crate_, "Position").y - 8.5).abs() < 0.2);
    set(&mut world, rope, "Enabled", Value::Bool(false));
    run(&mut physics, &mut world, 2.0);
    assert!((vector(&world, crate_, "Position").y - 1.0).abs() < 0.05);
    assert_eq!(physics.joints.len(), 0);
    // Enabled again, it pulls the crate back up. Rapier closes a joint's
    // gap at 3 studs a second, so that takes a moment.
    set(&mut world, rope, "Enabled", Value::Bool(true));
    run(&mut physics, &mut world, 4.0);
    assert!((vector(&world, crate_, "Position").y - 8.5).abs() < 0.2);
}

#[test]
fn a_joint_survives_its_part_being_welded_into_an_assembly() {
    let mut world = World::new();
    let (door, hinge) = door(&mut world);
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.2);
    // A handle welded on rebuilds the door's body; the hinge follows it.
    let handle = part(&mut world, v(4.0, 5.0, 0.5), v(0.3, 1.0, 0.5), false);
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(handle)).unwrap();
    set(&mut world, weld, "Part0", Value::Instance(Some(door)));
    set(&mut world, weld, "Part1", Value::Instance(Some(handle)));
    run(&mut physics, &mut world, 1.0);
    let at = vector(&world, door, "Position");
    assert!((at.y - 5.0).abs() < 0.1, "the door still hangs: {at:?}");
    assert!(current(&world, hinge, "CurrentAngle").abs() < 5.0);
}

#[test]
fn current_values_belong_to_their_classes() {
    let mut world = World::new();
    let (_, hinge) = door(&mut world);
    assert!(joints::current(&world, hinge, "CurrentAngle").is_some());
    assert_eq!(joints::current(&world, hinge, "CurrentLength"), None);
    let (_, rope) = crane(&mut world, 12.0, 10.0);
    let length = joints::current(&world, rope, "CurrentLength").unwrap();
    assert!((length - 6.5).abs() < 1e-4, "{length}");
}
