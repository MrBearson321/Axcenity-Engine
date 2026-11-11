//! Phase 37: velocities, forces, torques and aligns.

use axcenity_assets::Assets;
use axcenity_core::{CFrame, ClassName, EnumItem, InstanceId, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, size: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn attachment(world: &mut World, part: InstanceId, at: Vector3) -> InstanceId {
    let id = world.create(ClassName::Attachment).unwrap();
    world.set_parent(id, Some(part)).unwrap();
    set(world, id, "Position", Value::Vector3(at));
    id
}

/// A mover of `class` on `attachment0`, inside its part.
fn mover(world: &mut World, class: ClassName, attachment0: InstanceId) -> InstanceId {
    let id = world.create(class).unwrap();
    let parent = world.get(attachment0).unwrap().parent().unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    set(world, id, "Attachment0", Value::Instance(Some(attachment0)));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn item(world: &mut World, id: InstanceId, name: &str, enum_name: &str, item: &str) {
    let item = EnumItem {
        enum_name: enum_name.to_owned(),
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

fn distance(a: Vector3, b: Vector3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
    }
}

/// The part's weight in the default gravity.
fn weight(world: &World, id: InstanceId) -> f32 {
    Mass::of(world, &Assets::default(), id).unwrap().mass * GRAVITY
}

/// How far, in degrees, the part's up axis leans from the world's.
fn tilt(world: &World, id: InstanceId) -> f32 {
    let up = orientation_to_quat(vector(world, id, "Orientation")) * Vec3::Y;
    up.dot(Vec3::Y).clamp(-1.0, 1.0).acos().to_degrees()
}

/// A loose 2-stud block at `at` with an AlignPosition on its middle going
/// to `Position` (OneAttachment). Returns the block and the align.
fn aligned_block(world: &mut World, at: Vector3) -> (InstanceId, InstanceId) {
    let block = part(world, at, v(2.0, 2.0, 2.0), false);
    let a0 = attachment(world, block, v(0.0, 0.0, 0.0));
    let align = mover(world, ClassName::AlignPosition, a0);
    item(
        world,
        align,
        "Mode",
        "PositionAlignmentMode",
        "OneAttachment",
    );
    set(world, align, "Position", Value::Vector3(at));
    (block, align)
}

#[test]
fn an_align_position_follows_a_moving_target_smoothly() {
    let mut world = World::new();
    // The target is an anchored part a script moves every frame, followed
    // through its attachment (TwoAttachment, the default Mode).
    let target = part(&mut world, v(0.0, 10.0, 0.0), v(1.0, 1.0, 1.0), true);
    set(&mut world, target, "CanCollide", Value::Bool(false));
    let a1 = attachment(&mut world, target, v(0.0, 0.0, 0.0));
    let block = part(&mut world, v(0.0, 10.0, 6.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let align = mover(&mut world, ClassName::AlignPosition, a0);
    set(&mut world, align, "Attachment1", Value::Instance(Some(a1)));
    let mut physics = Physics::new();

    // It flies 6 studs to the target without falling or overshooting.
    let mut closest = f32::MAX;
    for _ in 0..60 {
        physics.step(&mut world, 1.0 / 60.0);
        let at = vector(&world, block, "Position");
        closest = closest.min(at.z);
        assert!((at.y - 10.0).abs() < 0.01, "{at:?}");
    }
    assert!(closest > -0.01, "it overshot to {closest}");
    assert!(vector(&world, block, "Position").z < 0.1);

    // Then the target moves in a circle at 3 studs a second.
    let mut last = vector(&world, block, "AssemblyLinearVelocity");
    let mut roughest: f32 = 0.0;
    let mut farthest: f32 = 0.0;
    for frame in 0..360 {
        let t = frame as f32 / 60.0;
        let goal = v(6.0 * (t * 0.5).sin(), 10.0, 6.0 * (t * 0.5).cos() - 6.0);
        set(&mut world, target, "Position", Value::Vector3(goal));
        physics.step(&mut world, 1.0 / 60.0);
        let speed = vector(&world, block, "AssemblyLinearVelocity");
        roughest = roughest.max(distance(speed, last));
        last = speed;
        farthest = farthest.max(distance(vector(&world, block, "Position"), goal));
    }
    // Its speed never jumps (studs a second, from one step to the next)...
    assert!(roughest < 0.3, "its speed jumped by {roughest}");
    // ...and it stays close behind the target.
    assert!(farthest < 0.6, "it fell {farthest} studs behind");

    // When the target stops, it closes the gap and stays there.
    run(&mut physics, &mut world, 1.0);
    let goal = vector(&world, target, "Position");
    assert!(distance(vector(&world, block, "Position"), goal) < 0.01);
}

#[test]
fn an_align_position_reaches_its_target_faster_with_higher_responsiveness() {
    let mut world = World::new();
    let (slow, slow_align) = aligned_block(&mut world, v(0.0, 10.0, 0.0));
    let (fast, fast_align) = aligned_block(&mut world, v(20.0, 10.0, 0.0));
    set(
        &mut world,
        slow_align,
        "Responsiveness",
        Value::Number(10.0),
    );
    set(
        &mut world,
        fast_align,
        "Responsiveness",
        Value::Number(40.0),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    // Both hold still in the air where they are.
    assert!((vector(&world, slow, "Position").y - 10.0).abs() < 0.01);

    // Both targets jump 8 studs up.
    for (align, x) in [(slow_align, 0.0), (fast_align, 20.0)] {
        set(
            &mut world,
            align,
            "Position",
            Value::Vector3(v(x, 18.0, 0.0)),
        );
    }
    let mut arrived = [None, None];
    let mut highest = [0.0_f32; 2];
    for frame in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
        for (i, block) in [slow, fast].into_iter().enumerate() {
            let y = vector(&world, block, "Position").y;
            highest[i] = highest[i].max(y);
            if arrived[i].is_none() && (y - 18.0).abs() < 0.1 {
                arrived[i] = Some(frame);
            }
        }
    }
    let [Some(slow_frames), Some(fast_frames)] = arrived else {
        panic!("they didn't both arrive: {arrived:?}");
    };
    assert!(
        fast_frames * 3 < slow_frames,
        "{fast_frames} steps with 40, {slow_frames} with 10"
    );
    // Neither overshoots.
    assert!(highest.iter().all(|&y| y < 18.05), "{highest:?}");

    // RigidityEnabled gets there at once, as fast as MaxVelocity allows.
    set(&mut world, slow_align, "RigidityEnabled", Value::Bool(true));
    set(&mut world, slow_align, "MaxVelocity", Value::Number(30.0));
    set(
        &mut world,
        slow_align,
        "Position",
        Value::Vector3(v(0.0, 8.0, 0.0)),
    );
    run(&mut physics, &mut world, 2.0 / 60.0);
    let speed = vector(&world, slow, "AssemblyLinearVelocity").y;
    assert!((speed + 30.0).abs() < 0.5, "{speed}");
    run(&mut physics, &mut world, 0.5);
    assert!((vector(&world, slow, "Position").y - 8.0).abs() < 0.01);
}

#[test]
fn an_align_position_too_weak_for_its_part_sinks() {
    let mut world = World::new();
    let (block, align) = aligned_block(&mut world, v(0.0, 10.0, 0.0));
    let half = weight(&world, block) / 2.0;
    set(
        &mut world,
        align,
        "MaxForce",
        Value::Number(f64::from(half)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    assert!(vector(&world, block, "Position").y < 8.0);
}

#[test]
fn a_vector_force_equal_to_its_weight_makes_a_part_hover() {
    let mut world = World::new();
    let hovering = part(&mut world, v(0.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let falling = part(&mut world, v(10.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let weight = weight(&world, hovering);
    for (block, force) in [(hovering, weight), (falling, weight * 0.9)] {
        let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
        let push = mover(&mut world, ClassName::VectorForce, a0);
        set(
            &mut world,
            push,
            "Force",
            Value::Vector3(v(0.0, force, 0.0)),
        );
    }
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0);
    let at = vector(&world, hovering, "Position");
    assert!(distance(at, v(0.0, 10.0, 0.0)) < 0.01, "{at:?}");
    assert!(tilt(&world, hovering) < 0.01);
    // A little less, and it sinks.
    assert!(vector(&world, falling, "Position").y < 9.0);
}

#[test]
fn a_vector_force_off_the_center_turns_the_part_unless_applied_at_the_center() {
    let mut world = World::new();
    let mut pushed = Vec::new();
    for (x, at_center) in [(0.0, false), (10.0, true)] {
        let block = part(&mut world, v(x, 10.0, 0.0), v(4.0, 1.0, 4.0), false);
        let a0 = attachment(&mut world, block, v(1.5, 0.0, 0.0));
        let push = mover(&mut world, ClassName::VectorForce, a0);
        let force = weight(&world, block);
        set(
            &mut world,
            push,
            "Force",
            Value::Vector3(v(0.0, force, 0.0)),
        );
        item(
            &mut world,
            push,
            "RelativeTo",
            "ActuatorRelativeTo",
            "World",
        );
        set(
            &mut world,
            push,
            "ApplyAtCenterOfMass",
            Value::Bool(at_center),
        );
        pushed.push(block);
    }
    let mut physics = Physics::new();
    // A tenth of a second: it turns right over in a quarter, and so is
    // nearly level again by half a second.
    run(&mut physics, &mut world, 0.1);
    assert!(tilt(&world, pushed[0]) > 20.0);
    assert!(tilt(&world, pushed[1]) < 0.01);
}

/// A flat drone hovering at height 20, tilted, with a push up on one side
/// that tips it over, and a LinearVelocity flying it forward along X.
/// Returns the drone and its Attachment0.
fn drone(world: &mut World, z: f32) -> (InstanceId, InstanceId) {
    let drone = part(world, v(0.0, 20.0, z), v(4.0, 0.5, 4.0), false);
    set(
        world,
        drone,
        "Orientation",
        Value::Vector3(v(15.0, 0.0, 20.0)),
    );
    let middle = attachment(world, drone, v(0.0, 0.0, 0.0));
    let flight = mover(world, ClassName::LinearVelocity, middle);
    set(
        world,
        flight,
        "VectorVelocity",
        Value::Vector3(v(10.0, 0.0, 0.0)),
    );
    let rotor = attachment(world, drone, v(1.5, 0.0, 0.0));
    let gust = mover(world, ClassName::VectorForce, rotor);
    let force = weight(world, drone) * 0.3;
    set(world, gust, "Force", Value::Vector3(v(0.0, force, 0.0)));
    (drone, middle)
}

#[test]
fn an_align_orientation_keeps_a_drone_level_while_linear_velocity_flies_it_forward() {
    let mut world = World::new();
    let (level, middle) = drone(&mut world, 0.0);
    let keeper = mover(&mut world, ClassName::AlignOrientation, middle);
    item(
        &mut world,
        keeper,
        "Mode",
        "OrientationAlignmentMode",
        "OneAttachment",
    );
    set(
        &mut world,
        keeper,
        "CFrame",
        Value::CFrame(CFrame::IDENTITY),
    );
    let (tipping, _) = drone(&mut world, 20.0);
    let mut physics = Physics::new();

    // It levels out from its tilt within a second.
    run(&mut physics, &mut world, 1.0);
    assert!(tilt(&world, level) < 1.0, "{}", tilt(&world, level));
    // Then it stays level while flying forward at 10 studs a second at
    // the same height.
    let start = vector(&world, level, "Position");
    let mut worst: f32 = 0.0;
    for _ in 0..120 {
        physics.step(&mut world, 1.0 / 60.0);
        worst = worst.max(tilt(&world, level));
    }
    assert!(worst < 0.5, "it tilted {worst} degrees");
    let at = vector(&world, level, "Position");
    assert!((at.x - start.x - 20.0).abs() < 0.1, "{start:?} to {at:?}");
    assert!((at.y - 20.0).abs() < 0.05 && at.z.abs() < 0.05, "{at:?}");
    let speed = vector(&world, level, "AssemblyLinearVelocity");
    assert!(distance(speed, v(10.0, 0.0, 0.0)) < 0.01, "{speed:?}");

    // Without the AlignOrientation, the push on one side tips it over.
    assert!(tilt(&world, tipping) > 30.0, "{}", tilt(&world, tipping));
}

#[test]
fn an_align_orientation_turns_to_its_second_attachment() {
    let mut world = World::new();
    let post = part(&mut world, v(0.0, 2.0, 0.0), v(1.0, 1.0, 1.0), true);
    set(
        &mut world,
        post,
        "Orientation",
        Value::Vector3(v(0.0, 90.0, 0.0)),
    );
    let a1 = attachment(&mut world, post, v(0.0, 0.0, 0.0));
    let block = part(&mut world, v(10.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let turner = mover(&mut world, ClassName::AlignOrientation, a0);
    set(&mut world, turner, "Attachment1", Value::Instance(Some(a1)));
    // A LinearVelocity holds it in the air meanwhile.
    let hold = mover(&mut world, ClassName::LinearVelocity, a0);
    set(
        &mut world,
        hold,
        "VectorVelocity",
        Value::Vector3(v(0.0, 0.0, 0.0)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.5);
    let turned = vector(&world, block, "Orientation");
    assert!((turned.y - 90.0).abs() < 0.5, "{turned:?}");
    assert!(turned.x.abs() < 0.5 && turned.z.abs() < 0.5, "{turned:?}");
}

#[test]
fn velocities_hold_their_speed_in_their_relative_frame() {
    let mut world = World::new();
    // Turned 90 degrees around Y, its X axis points along world -Z.
    let block = part(&mut world, v(0.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    set(
        &mut world,
        block,
        "Orientation",
        Value::Vector3(v(0.0, 90.0, 0.0)),
    );
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let flight = mover(&mut world, ClassName::LinearVelocity, a0);
    set(
        &mut world,
        flight,
        "VectorVelocity",
        Value::Vector3(v(5.0, 0.0, 0.0)),
    );
    item(
        &mut world,
        flight,
        "RelativeTo",
        "ActuatorRelativeTo",
        "Attachment0",
    );
    let spinner = mover(&mut world, ClassName::AngularVelocity, a0);
    set(
        &mut world,
        spinner,
        "AngularVelocity",
        Value::Vector3(v(0.0, 0.0, 0.0)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let speed = vector(&world, block, "AssemblyLinearVelocity");
    assert!(distance(speed, v(0.0, 0.0, -5.0)) < 0.01, "{speed:?}");

    // An AngularVelocity spins it at its speed despite angular damping.
    set(
        &mut world,
        spinner,
        "AngularVelocity",
        Value::Vector3(v(0.0, 3.0, 0.0)),
    );
    set(
        &mut world,
        flight,
        "VectorVelocity",
        Value::Vector3(v(0.0, 0.0, 0.0)),
    );
    run(&mut physics, &mut world, 1.0);
    let spin = vector(&world, block, "AssemblyAngularVelocity");
    assert!(distance(spin, v(0.0, 3.0, 0.0)) < 0.01, "{spin:?}");
}

#[test]
fn a_linear_velocity_pushes_with_at_most_its_max_force() {
    let mut world = World::new();
    let block = part(&mut world, v(0.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let flight = mover(&mut world, ClassName::LinearVelocity, a0);
    let half = weight(&world, block) / 2.0;
    set(
        &mut world,
        flight,
        "MaxForce",
        Value::Number(f64::from(half)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    // It falls at half the pull of gravity.
    let speed = vector(&world, block, "AssemblyLinearVelocity").y;
    assert!((speed + GRAVITY / 2.0).abs() < 1.0, "{speed}");
}

#[test]
fn a_torque_turns_a_part_and_a_disabled_mover_does_nothing() {
    let mut world = World::new();
    let block = part(&mut world, v(0.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let twist = mover(&mut world, ClassName::Torque, a0);
    set(
        &mut world,
        twist,
        "Torque",
        Value::Vector3(v(0.0, 50.0, 0.0)),
    );
    let hold = mover(&mut world, ClassName::VectorForce, a0);
    let weight = weight(&world, block);
    set(
        &mut world,
        hold,
        "Force",
        Value::Vector3(v(0.0, weight, 0.0)),
    );
    set(&mut world, hold, "Enabled", Value::Bool(false));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    assert!(vector(&world, block, "AssemblyAngularVelocity").y > 1.0);
    // The disabled VectorForce didn't hold it up.
    assert!(vector(&world, block, "Position").y < 9.0);
}

#[test]
fn velocities_hold_their_speed_against_friction_on_the_floor() {
    let mut world = World::new();
    part(&mut world, v(0.0, -0.5, 0.0), v(200.0, 1.0, 200.0), true);
    let top = part(&mut world, v(0.0, 0.5, 0.0), v(3.0, 1.0, 3.0), false);
    let middle = attachment(&mut world, top, v(0.0, 0.0, 0.0));
    let spin = mover(&mut world, ClassName::AngularVelocity, middle);
    set(
        &mut world,
        spin,
        "AngularVelocity",
        Value::Vector3(v(0.0, 4.0, 0.0)),
    );
    let sled = part(&mut world, v(20.0, 0.5, 0.0), v(2.0, 1.0, 2.0), false);
    let middle = attachment(&mut world, sled, v(0.0, 0.0, 0.0));
    let slide = mover(&mut world, ClassName::LinearVelocity, middle);
    set(
        &mut world,
        slide,
        "VectorVelocity",
        Value::Vector3(v(5.0, 0.0, 0.0)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 2.0);
    let turning = vector(&world, top, "AssemblyAngularVelocity");
    assert!(distance(turning, v(0.0, 4.0, 0.0)) < 0.02, "{turning:?}");
    let moving = vector(&world, sled, "AssemblyLinearVelocity");
    assert!(distance(moving, v(5.0, 0.0, 0.0)) < 0.02, "{moving:?}");
    // The top still sits on the floor.
    assert!((vector(&world, top, "Position").y - 0.5).abs() < 0.05);
}

#[test]
fn a_torque_turns_as_hard_every_step_and_stops_when_disabled() {
    let mut world = World::new();
    world
        .set_property(world.workspace(), "Gravity", Value::Number(0.0))
        .unwrap();
    let block = part(&mut world, v(0.0, 10.0, 0.0), v(2.0, 2.0, 2.0), false);
    let a0 = attachment(&mut world, block, v(0.0, 0.0, 0.0));
    let twist = mover(&mut world, ClassName::Torque, a0);
    set(
        &mut world,
        twist,
        "Torque",
        Value::Vector3(v(0.0, 50.0, 0.0)),
    );
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 0.5);
    // A 2-stud cube of density 1: mass 8, inertia 8 * (4 + 4) / 12 about Y.
    let inertia = 8.0 * 8.0 / 12.0;
    let expected = 50.0 / inertia * 0.5;
    let spin = vector(&world, block, "AssemblyAngularVelocity").y;
    // Less a little for the angular damping.
    assert!(
        spin <= expected && spin > expected * 0.8,
        "{spin} vs {expected}"
    );

    // Disabled, nothing turns it harder any more: only damping slows it.
    set(&mut world, twist, "Enabled", Value::Bool(false));
    run(&mut physics, &mut world, 0.5);
    let later = vector(&world, block, "AssemblyAngularVelocity").y;
    assert!(later < spin, "{later} vs {spin}");
}
