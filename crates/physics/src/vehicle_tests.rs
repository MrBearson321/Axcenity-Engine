//! Phase 40: cars on VehicleSeats and VehicleWheels.

use axcenity_core::{ClassName, InstanceId, PartType, TerrainData, Value, Vector3, World};

use super::*;

fn part(world: &mut World, position: Vector3, size: Vector3, anchored: bool) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(anchored));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn run(physics: &mut Physics, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.step(world, 1.0 / 60.0);
    }
}

/// A 40×1×200 floor whose top is at Y = 0, reaching far toward -Z.
fn floor(world: &mut World) -> InstanceId {
    part(
        world,
        Vector3::new(0.0, -0.5, -80.0),
        Vector3::new(40.0, 1.0, 200.0),
        true,
    )
}

/// An empty car facing -Z: a 6×1×10 chassis with its middle at `at`, a
/// VehicleSeat welded on top, four wheels a stud below the chassis's middle
/// (front ones steer) and an anchored, uncolliding cylinder for each.
struct Car {
    seat: InstanceId,
    chassis: InstanceId,
    /// Front left, front right, back left, back right.
    wheels: [InstanceId; 4],
    wheel_parts: [InstanceId; 4],
}

fn car(world: &mut World, at: Vector3) -> Car {
    let chassis = part(world, at, Vector3::new(6.0, 1.0, 10.0), false);
    let seat = world.create(ClassName::VehicleSeat).unwrap();
    world.set_parent(seat, Some(world.workspace())).unwrap();
    let on_top = Vector3::new(at.x, at.y + 1.0, at.z + 1.0);
    set(world, seat, "Position", Value::Vector3(on_top));
    set(
        world,
        seat,
        "Size",
        Value::Vector3(Vector3::new(2.0, 1.0, 2.0)),
    );
    set(world, seat, "Anchored", Value::Bool(false));
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(chassis)).unwrap();
    set(world, weld, "Part0", Value::Instance(Some(chassis)));
    set(world, weld, "Part1", Value::Instance(Some(seat)));
    let corners = [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)];
    let mut wheels = Vec::new();
    let mut wheel_parts = Vec::new();
    for (x, z) in corners {
        let wheel = world.create(ClassName::VehicleWheel).unwrap();
        world.set_parent(wheel, Some(chassis)).unwrap();
        set(
            world,
            wheel,
            "Position",
            Value::Vector3(Vector3::new(x, -1.0, z)),
        );
        set(world, wheel, "Steers", Value::Bool(z < 0.0));
        let middle = Vector3::new(at.x + x, at.y - 1.0, at.z + z);
        let shown = part(world, middle, Vector3::new(0.8, 2.0, 2.0), true);
        set(world, shown, "Shape", Value::PartType(PartType::Cylinder));
        set(world, shown, "CanCollide", Value::Bool(false));
        set(world, wheel, "WheelPart", Value::Instance(Some(shown)));
        wheels.push(wheel);
        wheel_parts.push(shown);
    }
    Car {
        seat,
        chassis,
        wheels: wheels.try_into().unwrap(),
        wheel_parts: wheel_parts.try_into().unwrap(),
    }
}

/// The car's speed along the ground, in studs a second.
fn speed(world: &World, car: &Car) -> f32 {
    let v = vector(world, car.chassis, "AssemblyLinearVelocity");
    v.x.hypot(v.z)
}

#[test]
fn an_empty_car_settles_on_its_springs_and_stays_still() {
    let mut world = World::new();
    floor(&mut world);
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0);
    // The wheels' bottoms (a stud and a radius below the chassis's middle at
    // rest length) are on the floor, with the springs squashed half a stud.
    let rest = vector(&world, car.chassis, "Position");
    assert!((rest.y - 1.51).abs() < 0.1, "{rest:?}");
    assert!(rest.x.abs() < 0.05 && rest.z.abs() < 0.05, "{rest:?}");
}

#[test]
fn throttle_drives_an_empty_car_forward_up_to_its_max_speed_and_back_in_reverse() {
    let mut world = World::new();
    floor(&mut world);
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    set(&mut world, car.seat, "MaxSpeed", Value::Number(30.0));
    set(&mut world, car.seat, "Throttle", Value::Number(1.0));
    run(&mut physics, &mut world, 3.0);
    let at = vector(&world, car.chassis, "Position");
    assert!(at.z < -40.0, "forward is -Z: {at:?}");
    assert!(at.x.abs() < 0.5, "straight on: {at:?}");
    let fast = speed(&world, &car);
    assert!(fast > 24.0 && fast <= 30.5, "{fast}");

    // Reverse slows it, then backs it up.
    set(&mut world, car.seat, "Throttle", Value::Number(-1.0));
    run(&mut physics, &mut world, 3.0);
    let velocity = vector(&world, car.chassis, "AssemblyLinearVelocity");
    assert!(velocity.z > 10.0, "{velocity:?}");
}

#[test]
fn steer_turns_the_front_wheels_and_the_car_and_the_wheel_parts_follow() {
    let mut world = World::new();
    floor(&mut world);
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    set(&mut world, car.seat, "Throttle", Value::Number(0.25));
    set(&mut world, car.seat, "Steer", Value::Number(1.0));
    run(&mut physics, &mut world, 1.5);
    let at = vector(&world, car.chassis, "Position");
    assert!(at.x > 3.0, "steering right turns toward +X: {at:?}");
    // The chassis turned clockwise from above (Orientation Y goes down).
    let heading = vector(&world, car.chassis, "Orientation").y;
    assert!(heading < -20.0, "{heading}");

    // Each wheel part is at its wheel: beside the chassis, a stud or so
    // below its middle, and the front ones are turned 30° further right
    // than the back ones.
    for (wheel, shown) in car.wheels.iter().zip(car.wheel_parts) {
        let local = vector(&world, *wheel, "Position");
        let middle = vector(&world, shown, "Position");
        let (sy, cy) = heading.to_radians().sin_cos();
        let beside = Vector3::new(
            at.x + local.x * cy + local.z * sy,
            middle.y,
            at.z - local.x * sy + local.z * cy,
        );
        // The chassis leans out of the turn a little, which this leaves out.
        let off = (middle.x - beside.x).hypot(middle.z - beside.z);
        assert!(off < 0.75, "{middle:?} vs {beside:?}");
        assert!(middle.y < at.y - 0.5 && middle.y > at.y - 1.6, "{middle:?}");
    }
    // The axle (a cylinder's X) doesn't change as the wheel spins.
    let axle = |shown| {
        let axle = rotate(
            vector(&world, shown, "Orientation"),
            Vector3::new(1.0, 0.0, 0.0),
        );
        axle.z.atan2(axle.x).to_degrees()
    };
    let (front, back) = (axle(car.wheel_parts[0]), axle(car.wheel_parts[2]));
    let turned = (front - back + 180.0).rem_euclid(360.0) - 180.0;
    assert!((turned - 30.0).abs() < 1.0, "front {front}, back {back}");
}

/// `v` turned by an Orientation in degrees: Z first, then X, then Y.
fn rotate(orientation: Vector3, v: Vector3) -> Vector3 {
    let (sx, cx) = orientation.x.to_radians().sin_cos();
    let (sy, cy) = orientation.y.to_radians().sin_cos();
    let (sz, cz) = orientation.z.to_radians().sin_cos();
    let (x, y, z) = (v.x * cz - v.y * sz, v.x * sz + v.y * cz, v.z);
    let (y, z) = (y * cx - z * sx, y * sx + z * cx);
    Vector3::new(x * cy + z * sy, y, -x * sy + z * cy)
}

#[test]
fn wheel_parts_spin_as_the_car_rolls() {
    let mut world = World::new();
    floor(&mut world);
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    set(&mut world, car.seat, "Throttle", Value::Number(0.3));
    let mut turns = Vec::new();
    for _ in 0..30 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        turns.push(vector(&world, car.wheel_parts[2], "Orientation"));
    }
    // Rolling toward -Z, the top of the wheel goes forward: about -X.
    let first = turns[0];
    let moved = turns
        .iter()
        .any(|turn| (turn.x - first.x).abs() > 5.0 || (turn.z - first.z).abs() > 5.0);
    assert!(moved, "{turns:?}");
}

#[test]
fn the_hand_brake_stops_a_moving_car_and_an_empty_car_with_no_throttle_holds() {
    let mut world = World::new();
    floor(&mut world);
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    // Someone sits on it, so taking the throttle off only coasts.
    let driver = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(driver, Some(world.workspace())).unwrap();
    set(
        &mut world,
        car.seat,
        "Occupant",
        Value::Instance(Some(driver)),
    );
    set(&mut world, car.seat, "Throttle", Value::Number(1.0));
    run(&mut physics, &mut world, 1.5);
    let fast = speed(&world, &car);
    assert!(fast > 25.0, "{fast}");
    set(&mut world, car.seat, "Throttle", Value::Number(0.0));
    run(&mut physics, &mut world, 0.5);
    let coasting = speed(&world, &car);
    assert!(coasting > fast - 10.0, "{fast} {coasting}");

    set(&mut world, car.seat, "HandBrake", Value::Bool(true));
    run(&mut physics, &mut world, 1.0);
    assert!(speed(&world, &car) < 0.5, "{}", speed(&world, &car));

    // Off the hand brake, an empty car still holds.
    set(&mut world, car.seat, "HandBrake", Value::Bool(false));
    set(&mut world, car.seat, "Occupant", Value::Instance(None));
    let stopped = vector(&world, car.chassis, "Position");
    run(&mut physics, &mut world, 1.0);
    let now = vector(&world, car.chassis, "Position");
    assert!((now.z - stopped.z).abs() < 0.2, "{stopped:?} {now:?}");
}

#[test]
fn suspension_settles_the_car_on_uneven_ground_without_jitter() {
    let mut world = World::new();
    floor(&mut world);
    // A 0.4-stud bump under the front left wheel, and a dip under the back
    // right one (the floor there is lower).
    part(
        &mut world,
        Vector3::new(-3.5, 0.2, -3.5),
        Vector3::new(3.0, 0.4, 3.0),
        true,
    );
    let car = car(&mut world, Vector3::new(0.0, 2.5, 0.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 3.0);
    let tilt = vector(&world, car.chassis, "Orientation");
    assert!(tilt.x.abs() + tilt.z.abs() > 0.5, "it leans: {tilt:?}");
    let first = (
        vector(&world, car.chassis, "Position"),
        vector(&world, car.chassis, "Orientation"),
    );
    let mut most = (0.0f32, 0.0f32);
    for _ in 0..120 {
        run(&mut physics, &mut world, 1.0 / 60.0);
        let at = vector(&world, car.chassis, "Position");
        let turn = vector(&world, car.chassis, "Orientation");
        let moved = (at.x - first.0.x)
            .abs()
            .max((at.y - first.0.y).abs())
            .max((at.z - first.0.z).abs());
        let turned = (turn.x - first.1.x)
            .abs()
            .max((turn.y - first.1.y).abs())
            .max((turn.z - first.1.z).abs());
        most = (most.0.max(moved), most.1.max(turned));
    }
    assert!(most.0 < 0.01 && most.1 < 0.1, "{most:?}");
    for shown in car.wheel_parts {
        let wheel = vector(&world, shown, "Position");
        assert!(wheel.y > 0.9 && wheel.y < 1.5, "{wheel:?}");
    }
}

#[test]
fn a_car_climbs_a_gentle_terrain_slope() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    set(
        &mut world,
        terrain,
        "Position",
        Value::Vector3(Vector3::default()),
    );
    let size = Vector3::new(128.0, 64.0, 128.0);
    set(&mut world, terrain, "Size", Value::Vector3(size));
    // Rising 10° toward -Z: the ground is at 6.4 studs at z = 64.
    let rise = 10.0_f32.to_radians().tan();
    let ground = |z: f32| 6.4 + (64.0 - z) * rise;
    let mut data = TerrainData::flat(33, 33, 0.0).unwrap();
    let all = axcenity_core::SampleRect::new(0, 0, 33, 33);
    let heights = data.heights_mut_in(all);
    for z in 0..33 {
        let world_z = z as f32 * 4.0 - 64.0;
        for x in 0..33 {
            heights[z * 33 + x] = ground(world_z) / size.y;
        }
    }
    world.set_terrain(terrain, std::sync::Arc::new(data));
    let car = car(&mut world, Vector3::new(0.0, ground(50.0) + 2.8, 50.0));
    let mut physics = Physics::new();
    run(&mut physics, &mut world, 1.0);
    let start = vector(&world, car.chassis, "Position");
    set(&mut world, car.seat, "Throttle", Value::Number(1.0));
    run(&mut physics, &mut world, 2.5);
    let up = vector(&world, car.chassis, "Position");
    assert!(start.z - up.z > 40.0, "{start:?} {up:?}");
    assert!(up.y - start.y > 6.0, "{start:?} {up:?}");
    // Still on its wheels, nose up the slope.
    let pitch = vector(&world, car.chassis, "Orientation").x;
    assert!((pitch - 10.0).abs() < 3.0, "{pitch}");
}
