//! Vehicles (Phase 40): a VehicleSeat drives the body it's part of (its
//! assembly, the chassis) on the VehicleWheels parented to that body's
//! parts, with rapier's ray-cast vehicle controller, which stays stable at
//! speed. Each wheel is a spring along its -Y and a ray to the ground; no
//! wheel collider touches the ground.
//!
//! Like movers, vehicles are read from the world once a frame, after the
//! bodies are synced, and act before every fixed step. The controller is
//! made again each frame from the world, so a car that gains or loses a
//! part, or a driver, keeps driving; only how far each wheel has rolled and
//! steered is kept between frames ([`Rolling`]).
//!
//! The seat's `Throttle` pushes the Driven wheels, scaled by mass so
//! `Torque` is studs a second squared whatever the car weighs, and less as
//! the car nears `MaxSpeed`. `Steer` turns the Steers wheels toward
//! `SteerAngle` at `TurnSpeed`. `HandBrake`, or an empty seat with no
//! throttle, holds the car with the Brakes wheels; otherwise a coasting car
//! slows by rolling resistance. After each step the wheel's `WheelPart` is
//! moved to the wheel, spinning as it rolls and turning as it steers.

use std::collections::{BTreeMap, HashSet};

use axcenity_core::{ClassName, InstanceId, Value, World};
use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::glamx::{Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::pipeline::{PhysicsWorld, QueryFilter};
use rapier3d::prelude::{RigidBodyHandle, RigidBodySet};

use crate::joints::local_pose;
use crate::{Body, Groups, STEP_SECONDS, quat_to_orientation, to_vector3};

/// How hard the braking wheels hold a car on its HandBrake, or an empty
/// one, in studs a second squared: it stops from full speed in under a
/// second and stays put on a slope.
const HOLD_DECELERATION: f32 = 120.0;
/// Below this speed along the ground, in studs a second, a held car stops.
const HOLD_SPEED: f32 = 1.0;
/// How fast a coasting car slows by itself, in studs a second squared.
const ROLLING_DECELERATION: f32 = 8.0;
/// How much of the sideways slip rapier's tyres take out each step. Its
/// full amount overshoots on four wheels at these sizes, and the car
/// wobbles side to side more each step; half still grips in a turn.
const SIDE_GRIP: f32 = 0.5;
/// The most a wheel's spring pushes, in car weights: a hard landing is
/// caught without the spring launching the car.
const MAX_SPRING_WEIGHTS: f32 = 4.0;

/// One VehicleSeat's car this frame.
pub(crate) struct Vehicle {
    chassis: RigidBodyHandle,
    controller: DynamicRayCastVehicleController,
    /// The controller's wheels, in the same order.
    wheels: Vec<Wheel>,
    /// The bodies of the wheels' parts, which the rays pass through.
    wheel_bodies: HashSet<RigidBodyHandle>,
    groups: Groups,
    drive: Drive,
}

/// A VehicleSeat's settings, read once a frame.
#[derive(Clone, Copy, Debug)]
struct Drive {
    throttle: f32,
    /// Radians the steering wheels head for (positive turns left).
    steering: f32,
    /// Radians a second they turn.
    turn_speed: f32,
    max_speed: f32,
    torque: f32,
    hold: bool,
}

/// One VehicleWheel.
struct Wheel {
    id: InstanceId,
    /// Its frame on the chassis body, at its middle with the spring at
    /// rest length.
    frame: Pose,
    steers: bool,
    driven: bool,
    brakes: bool,
    /// Its `SuspensionDamping`, before [`drive`] limits it.
    damping: f32,
    /// The part moved to it, and that part's body.
    part: Option<(InstanceId, RigidBodyHandle)>,
}

/// How far a wheel has rolled and steered, in radians. Not saved.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rolling {
    rotation: f32,
    steering: f32,
}

/// Every VehicleSeat in the Workspace whose part is in a moving body, with
/// the wheels on that body. A second seat in the same car does nothing.
pub(crate) fn read(
    world: &World,
    parts: &BTreeMap<InstanceId, Body>,
    bodies: &RigidBodySet,
    rolling: &BTreeMap<InstanceId, Rolling>,
) -> Vec<Vehicle> {
    let wheels: Vec<InstanceId> =
        world.all_below(world.workspace(), |class| class == ClassName::VehicleWheel);
    let mut vehicles: Vec<Vehicle> = Vec::new();
    for seat in world.all_below(world.workspace(), |class| class == ClassName::VehicleSeat) {
        let Some(instance) = world.get(seat) else {
            continue;
        };
        let Some(body) = parts.get(&seat) else {
            continue;
        };
        let chassis = body.body;
        let moving = bodies.get(chassis).is_some_and(|b| b.is_dynamic());
        if !moving || vehicles.iter().any(|v| v.chassis == chassis) {
            continue;
        }
        let mut vehicle = Vehicle {
            chassis,
            controller: DynamicRayCastVehicleController::new(chassis),
            wheels: Vec::new(),
            wheel_bodies: HashSet::new(),
            groups: body.seen.groups,
            drive: Drive::read(instance),
        };
        for &id in &wheels {
            if let Some(wheel) = read_wheel(world, parts, chassis, id) {
                vehicle.add(world, wheel, rolling.get(&id).copied());
            }
        }
        vehicles.push(vehicle);
    }
    vehicles
}

impl Drive {
    fn read(seat: &axcenity_core::Instance) -> Self {
        let number = |name: &str| {
            seat.property(name)
                .and_then(Value::as_number)
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
                .unwrap_or(0.0)
        };
        let occupied = seat
            .property("Occupant")
            .and_then(Value::as_instance)
            .is_some();
        let throttle = number("Throttle").clamp(-1.0, 1.0);
        let steer = number("Steer").clamp(-1.0, 1.0);
        let hand_brake = seat.property("HandBrake").and_then(Value::as_bool) == Some(true);
        Self {
            throttle,
            // Steer right (positive) turns the wheels clockwise from above.
            steering: -steer * number("SteerAngle").clamp(0.0, 89.0).to_radians(),
            turn_speed: number("TurnSpeed").max(0.0).to_radians(),
            max_speed: number("MaxSpeed").max(0.0),
            torque: number("Torque").max(0.0),
            hold: hand_brake || (!occupied && throttle == 0.0),
        }
    }
}

/// A VehicleWheel on a part of `chassis`.
fn read_wheel(
    world: &World,
    parts: &BTreeMap<InstanceId, Body>,
    chassis: RigidBodyHandle,
    id: InstanceId,
) -> Option<Wheel> {
    let wheel = world.get(id)?;
    let on = parts.get(&wheel.parent()?)?;
    if on.body != chassis {
        return None;
    }
    let flag = |name| wheel.property(name).and_then(Value::as_bool) == Some(true);
    let part = wheel
        .property("WheelPart")
        .and_then(Value::as_instance)
        // A part alone in its body: one welded into the car moves with it
        // already, and one welded to others can't be placed by itself.
        .and_then(|part| Some((part, parts.get(&part).filter(|body| body.root)?.body)))
        .filter(|(_, body)| *body != chassis);
    Some(Wheel {
        id,
        frame: on.offset * local_pose(wheel)?,
        steers: flag("Steers"),
        driven: flag("Driven"),
        brakes: flag("Brakes"),
        damping: wheel
            .property("SuspensionDamping")
            .and_then(Value::as_number)
            .map(|n| n as f32)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0)
            .max(0.0),
        part,
    })
}

impl Vehicle {
    fn add(&mut self, world: &World, wheel: Wheel, rolling: Option<Rolling>) {
        let Some(instance) = world.get(wheel.id) else {
            return;
        };
        let number = |name: &str, min: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
                .unwrap_or(min)
                .max(min)
        };
        let radius = number("Radius", 0.05);
        let rest = number("SuspensionRestLength", 0.0);
        let damping = wheel.damping;
        let tuning = WheelTuning {
            suspension_stiffness: number("SuspensionStiffness", 0.0),
            suspension_compression: damping,
            suspension_damping: damping,
            // It may squash all the way up to where it's fixed on.
            max_suspension_travel: rest,
            side_friction_stiffness: SIDE_GRIP,
            friction_slip: number("Friction", 0.0),
            // Set from the car's weight before each step.
            max_suspension_force: 0.0,
        };
        let turn = wheel.frame.rotation;
        let up = turn * Vec3::Y;
        let added = self.controller.add_wheel(
            wheel.frame.translation + up * rest,
            -up,
            turn * Vec3::X,
            rest,
            radius,
            &tuning,
        );
        let rolling = rolling.unwrap_or_default();
        added.rotation = rolling.rotation;
        added.steering = rolling.steering;
        if let Some((_, body)) = wheel.part {
            self.wheel_bodies.insert(body);
        }
        self.wheels.push(wheel);
    }
}

/// Before a fixed step: pushes, steers and brakes each car's wheels and
/// lets the controller add their springs' and tyres' impulses to its body.
pub(crate) fn drive(
    rapier: &mut PhysicsWorld,
    vehicles: &mut [Vehicle],
    rolling: &mut BTreeMap<InstanceId, Rolling>,
) {
    let dt = STEP_SECONDS as f32;
    let gravity = rapier.gravity.length();
    let pull = rapier.gravity * dt;
    for vehicle in vehicles {
        let Some(body) = rapier.bodies.get_mut(vehicle.chassis) else {
            continue;
        };
        let drive = vehicle.drive;
        let steering = |wheel: &Wheel| if wheel.steers { drive.steering } else { 0.0 };
        let turning = vehicle
            .controller
            .wheels()
            .iter()
            .zip(&vehicle.wheels)
            .any(|(state, wheel)| state.steering != steering(wheel));
        if drive.throttle != 0.0 || turning {
            body.wake_up(true);
        } else if body.is_sleeping() {
            // A car at rest stays asleep: its springs add nothing then.
            continue;
        }
        let mass = body.mass();
        let rotation = body.position().rotation;
        let velocity = body.linvel();
        // The tyres hold against the slope gravity is about to pull the
        // car down this step, not only against how fast it moves now, or a
        // braked car would creep downhill a step's pull at a time.
        let up = vehicle
            .wheels
            .first()
            .map_or(Vec3::Y, |wheel| rotation * (wheel.frame.rotation * Vec3::Y));
        let downhill = pull - up * pull.dot(up);
        body.set_linvel(velocity + downhill, false);
        let count = |pick: fn(&Wheel) -> bool| vehicle.wheels.iter().filter(|w| pick(w)).count();
        let (driven, braking) = (count(|w| w.driven), count(|w| w.brakes));
        let all = vehicle.wheels.len();
        let (pose, center) = (*body.position(), body.center_of_mass());
        let inverse_inertia = body.mass_properties().effective_world_inv_inertia;
        for (state, wheel) in vehicle
            .controller
            .wheels_mut()
            .iter_mut()
            .zip(&vehicle.wheels)
        {
            // The springs' damping works a step at a time, so too much
            // overshoots and the car rocks harder each step. Each wheel
            // takes out at most its share of how fast the car moves along
            // its spring there: how much depends on how easily the car
            // turns about that point (a narrow car rolls easily).
            let along = rotation * state.direction_cs;
            let arm = (pose * state.chassis_connection_point_cs - center).cross(along);
            let give = 1.0 / mass + arm.dot(inverse_inertia * arm);
            let most = 1.0 / (all as f32 * mass * dt * give);
            state.damping_compression = wheel.damping.min(most);
            state.damping_relaxation = wheel.damping.min(most);
            let target = steering(wheel);
            let step = drive.turn_speed * dt;
            state.steering += (target - state.steering).clamp(-step, step);
            state.max_suspension_force = mass * gravity * MAX_SPRING_WEIGHTS;
            state.engine_force = 0.0;
            if wheel.driven && drive.throttle != 0.0 {
                let forward = rotation * (wheel.frame.rotation * Vec3::NEG_Z);
                let speed = forward.dot(velocity);
                // Pushing the way it rolls gives less near MaxSpeed.
                let room = if speed * drive.throttle > 0.0 && drive.max_speed > 0.0 {
                    (1.0 - speed.abs() / drive.max_speed).max(0.0)
                } else if drive.max_speed > 0.0 {
                    1.0
                } else {
                    0.0
                };
                state.engine_force = drive.torque * mass * drive.throttle * room / driven as f32;
            }
            state.brake = if drive.hold && wheel.brakes {
                mass * HOLD_DECELERATION * dt / braking as f32
            } else {
                mass * ROLLING_DECELERATION * dt / all as f32
            };
        }
        let wheel_bodies = &vehicle.wheel_bodies;
        let predicate = |_, collider: &rapier3d::prelude::Collider| {
            collider
                .parent()
                .is_none_or(|body| !wheel_bodies.contains(&body))
        };
        let filter = QueryFilter::default()
            .exclude_rigid_body(vehicle.chassis)
            .exclude_sensors()
            .groups(vehicle.groups.interaction())
            .predicate(&predicate);
        let queries = rapier.broad_phase.as_query_pipeline_mut(
            rapier.narrow_phase.query_dispatcher(),
            &mut rapier.bodies,
            &mut rapier.colliders,
            filter,
        );
        vehicle.controller.update_vehicle(dt, queries);
        let grounded = vehicle
            .controller
            .wheels()
            .iter()
            .filter(|wheel| wheel.raycast_info().is_in_contact)
            .count();
        if let Some(body) = rapier.bodies.get_mut(vehicle.chassis) {
            let mut velocity = body.linvel();
            // The tyres only slow sliding; a held car that has all but
            // stopped with most of its wheels down stays where it is.
            let along_ground = velocity - up * velocity.dot(up);
            if drive.hold && grounded * 2 > all && along_ground.length() < HOLD_SPEED {
                velocity -= along_ground;
            }
            body.set_linvel(velocity - downhill, false);
        }
        for (state, wheel) in vehicle.controller.wheels().iter().zip(&vehicle.wheels) {
            rolling.insert(
                wheel.id,
                Rolling {
                    rotation: state.rotation,
                    steering: state.steering,
                },
            );
        }
    }
}

/// After a fixed step: moves each wheel's body to where its wheel is now,
/// and notes the pose in `placed` (the last step's pose wins) for
/// [`write_wheels`] to copy into the world once the frame's steps are done.
pub(crate) fn place_wheels(
    bodies: &mut RigidBodySet,
    vehicles: &[Vehicle],
    placed: &mut BTreeMap<InstanceId, Pose>,
) {
    for vehicle in vehicles {
        let Some(chassis) = bodies.get(vehicle.chassis) else {
            continue;
        };
        let chassis = *chassis.position();
        let turn = chassis.rotation;
        for (state, wheel) in vehicle.controller.wheels().iter().zip(&vehicle.wheels) {
            let Some((part, handle)) = wheel.part else {
                continue;
            };
            // How long its spring is is only known once the controller has
            // run this frame; a sleeping car's wheels stay where they are.
            if state.raycast_info().hard_point_ws == Vec3::ZERO {
                continue;
            }
            // Where the car is after the step, not before it.
            let center = chassis * state.chassis_connection_point_cs
                + turn * state.direction_cs * state.raycast_info().suspension_length;
            // Rolling forward (-Z) turns the top of the wheel forward.
            let rotation = turn
                * wheel.frame.rotation
                * Quat::from_rotation_y(state.steering)
                * Quat::from_rotation_x(-state.rotation);
            let pose = Pose::from_parts(center, rotation);
            if let Some(body) = bodies.get_mut(handle)
                && *body.position() != pose
            {
                body.set_position(pose, false);
            }
            placed.insert(part, pose);
        }
    }
}

/// Once per frame, after the write-back: moves each wheel's part to the
/// pose [`place_wheels`] last gave it, and remembers what it wrote so the
/// move isn't taken for a script's.
pub(crate) fn write_wheels(
    world: &mut World,
    parts: &mut BTreeMap<InstanceId, Body>,
    placed: &BTreeMap<InstanceId, Pose>,
) {
    for (&part, pose) in placed {
        let Some(wheel_part) = parts.get_mut(&part) else {
            continue;
        };
        let (position, orientation) = (
            to_vector3(pose.translation),
            quat_to_orientation(pose.rotation),
        );
        let seen = &mut wheel_part.seen;
        if seen.position == position && seen.orientation == orientation {
            continue;
        }
        seen.position = position;
        seen.orientation = orientation;
        crate::own_write(world, part, &mut wheel_part.revision, |world| {
            let _ = world.set_property(part, "Position", Value::Vector3(position));
            let _ = world.set_property(part, "Orientation", Value::Vector3(orientation));
        });
    }
}
