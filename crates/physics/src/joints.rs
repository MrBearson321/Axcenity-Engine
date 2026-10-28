//! Joints (Phase 36): HingeConstraint, BallSocketConstraint,
//! PrismaticConstraint, CylindricalConstraint, SpringConstraint,
//! RopeConstraint and RodConstraint each join `Attachment0` to
//! `Attachment1` with one rapier impulse joint between their parts' bodies.
//!
//! The joint's frames are the attachments' frames on their bodies, so a
//! hinge turns and a prismatic slides along `Attachment0`'s X axis (its
//! `Axis`), rapier's free axis. A spring, rope or rod couples the three
//! linear axes, so its limits and motor act on the distance between the
//! attachments.
//!
//! A joint is made again when its bodies change (an assembly was rebuilt)
//! and updated in place when only its properties change. Servos and winches
//! are driven before every step: a servo holds a setpoint that moves toward
//! its target at its speed, and a winch pays the rope out at its speed, or
//! reels it in with a motor that pulls the load at its speed (with at most
//! its force) while the rope shortens behind it.

use std::collections::BTreeMap;

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use rapier3d::dynamics::{
    GenericJoint, GenericJointBuilder, ImpulseJointHandle, JointAxesMask, JointAxis, MotorModel,
};
use rapier3d::glamx::Vec3;
use rapier3d::math::Pose;
use rapier3d::pipeline::PhysicsWorld;
use rapier3d::prelude::RigidBodyHandle;

use crate::{Body, STEP_SECONDS, orientation_to_quat, to_vec3};

/// How hard a motor holds its speed: rapier's velocity "damping". High
/// enough that a motor keeps its speed under load, up to its max torque.
const MOTOR_DAMPING: f32 = 1.0e8;

/// How stiffly a servo holds its setpoint, in force (or torque) per stud
/// (or radian): stiff enough to be a hard constraint, up to its max force.
/// With no damping it closes the whole gap each step, so a servo holding a
/// load sags by only a tenth of a degree or so.
const SERVO_STIFFNESS: f32 = 1.0e9;
const SERVO_DAMPING: f32 = 0.0;

/// How much shorter than its rope a winch's load can be and still count as
/// hanging on it, in studs.
const WINCH_SLACK: f32 = 0.25;

/// One constraint's joint in the simulation.
pub(crate) struct Joint {
    handle: ImpulseJointHandle,
    bodies: (RigidBodyHandle, RigidBodyHandle),
    /// The joint as its properties describe it, before servos and winches
    /// change it each step. When the properties give something else, the
    /// joint is updated.
    built: GenericJoint,
    /// What to drive before each step.
    drive: Drive,
    /// Where its servos and winch are now.
    setpoints: Setpoints,
}

/// Where a joint's servos and winch have got to. A servo's setpoint moves
/// toward its target at its speed, and the joint holds it there; a winch's
/// rope length is reeled toward its target.
#[derive(Clone, Copy, Debug, Default)]
struct Setpoints {
    /// Radians, from where the joint was when the servo started.
    turn: Option<f32>,
    /// Studs, likewise.
    slide: Option<f32>,
    /// Studs, from the rope's `Length` when the winch started.
    reeled: Option<f32>,
}

/// What a joint's servos and winch want, read from its properties.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Drive {
    /// Turning to an angle (radians) at up to a speed (radians a second).
    turn: Option<Servo>,
    /// Sliding to a position (studs) at up to a speed (studs a second).
    slide: Option<Servo>,
    winch: Option<Winch>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Servo {
    target: f32,
    speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Winch {
    /// The rope's `Length`, where the winch starts.
    length: f32,
    target: f32,
    speed: f32,
    force: f32,
}

/// The joint a constraint describes, and what to drive on it.
struct Described {
    data: GenericJoint,
    drive: Drive,
}

/// The constraints in `world` that hold two parts with bodies, by ID.
fn constraints(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_joint()))
        .collect()
}

/// Makes, updates and removes joints to match the enabled constraints.
/// Returns true if anything changed.
pub(crate) fn sync(
    world: &World,
    rapier: &mut PhysicsWorld,
    parts: &BTreeMap<InstanceId, Body>,
    joints: &mut BTreeMap<InstanceId, Joint>,
) -> bool {
    let mut changed = false;
    let mut wanted = BTreeMap::new();
    for id in constraints(world) {
        let constraint = world.get(id).expect("listed");
        let Some((bodies, frames)) = ends(world, constraint, parts) else {
            continue;
        };
        if let Some(described) = describe(constraint, frames) {
            wanted.insert(id, (bodies, described));
        }
    }
    // Joints whose constraint is gone or disabled, or whose bodies changed.
    joints.retain(|id, joint| {
        let keep = wanted
            .get(id)
            .is_some_and(|(bodies, _)| *bodies == joint.bodies)
            && rapier.impulse_joints.get(joint.handle).is_some();
        if !keep {
            // Its body may be gone already, taking the joint with it.
            rapier.impulse_joints.remove(joint.handle, true);
            changed = true;
        }
        keep
    });
    for (id, (bodies, described)) in wanted {
        match joints.get_mut(&id) {
            Some(joint) => {
                if joint.built != described.data {
                    if let Some(live) = rapier.impulse_joints.get_mut(joint.handle, true) {
                        live.data = described.data;
                    }
                    joint.built = described.data;
                    changed = true;
                }
                // A servo or winch that starts (again) starts from here.
                let (old, new) = (joint.drive, described.drive);
                let points = &mut joint.setpoints;
                if old.turn.is_some() != new.turn.is_some() {
                    points.turn = None;
                }
                if old.slide.is_some() != new.slide.is_some() {
                    points.slide = None;
                }
                if old.winch.map(|w| w.length) != new.winch.map(|w| w.length) {
                    points.reeled = new.winch.map(|winch| winch.length);
                }
                joint.drive = new;
            }
            None => {
                let handle = rapier
                    .impulse_joints
                    .insert(bodies.0, bodies.1, described.data, true);
                joints.insert(
                    id,
                    Joint {
                        handle,
                        bodies,
                        built: described.data,
                        drive: described.drive,
                        setpoints: Setpoints {
                            reeled: described.drive.winch.map(|winch| winch.length),
                            ..Setpoints::default()
                        },
                    },
                );
                changed = true;
            }
        }
    }
    changed
}

/// Moves each servo's setpoint and each winch's rope length on by a step,
/// and sets the joints to hold them.
pub(crate) fn drive(rapier: &mut PhysicsWorld, joints: &mut BTreeMap<InstanceId, Joint>) {
    let dt = STEP_SECONDS as f32;
    for joint in joints.values_mut() {
        let drive = joint.drive;
        if drive == Drive::default() {
            continue;
        }
        let (Some(body1), Some(body2)) = (
            rapier.bodies.get(joint.bodies.0),
            rapier.bodies.get(joint.bodies.1),
        ) else {
            continue;
        };
        let Some(live) = rapier.impulse_joints.get(joint.handle) else {
            continue;
        };
        let frame1 = body1.position() * live.data.local_frame1;
        let frame2 = body2.position() * live.data.local_frame2;
        let mut data = live.data;
        let points = &mut joint.setpoints;
        if let Some(servo) = drive.turn {
            let at = points.turn.unwrap_or_else(|| angle(frame1, frame2));
            let step = wrap(servo.target - at).clamp(-servo.speed * dt, servo.speed * dt);
            let at = wrap(at + step);
            points.turn = Some(at);
            let motor = &mut data.motors[JointAxis::AngX as usize];
            (motor.target_pos, motor.target_vel) = (at, step / dt);
        }
        if let Some(servo) = drive.slide {
            let at = points.slide.unwrap_or_else(|| offset(frame1, frame2));
            let step = (servo.target - at).clamp(-servo.speed * dt, servo.speed * dt);
            let at = at + step;
            points.slide = Some(at);
            let motor = &mut data.motors[JointAxis::LinX as usize];
            (motor.target_pos, motor.target_vel) = (at, step / dt);
        }
        if let (Some(winch), Some(reeled)) = (drive.winch, points.reeled.as_mut()) {
            let length = (frame2.translation - frame1.translation).length();
            let step = winch.speed * dt;
            let taut = length >= *reeled - WINCH_SLACK;
            let pulling = winch.target < *reeled && taut;
            if winch.target > *reeled {
                // Paying out.
                *reeled = (*reeled + step).min(winch.target);
            } else if taut {
                // Reeling in a load: the motor below pulls it, with at most
                // the winch's force, and the rope shortens as far as the
                // load has come, so it holds the load where it got to.
                *reeled = reeled.min(length.max(winch.target));
            } else {
                // Reeling in slack.
                *reeled = (*reeled - step).max(winch.target);
            }
            data.limits[JointAxis::LinX as usize].max = reeled.max(0.0);
            let motor = &mut data.motors[JointAxis::LinX as usize];
            if pulling {
                data.motor_axes |= JointAxesMask::LIN_X;
                (motor.target_vel, motor.damping) = (-winch.speed, MOTOR_DAMPING);
                (motor.max_force, motor.model) = (max_force(winch.force), MotorModel::ForceBased);
            } else {
                data.motor_axes.remove(JointAxesMask::LIN_X);
            }
        }
        if data != live.data
            && let Some(live) = rapier.impulse_joints.get_mut(joint.handle, true)
        {
            live.data = data;
        }
    }
}

/// The bodies an enabled constraint joins, and its attachments' frames on
/// them. `None` if it's disabled, an attachment isn't on a part with a
/// body, or both are on the same body (welded together).
fn ends(
    world: &World,
    constraint: &Instance,
    parts: &BTreeMap<InstanceId, Body>,
) -> Option<((RigidBodyHandle, RigidBodyHandle), (Pose, Pose))> {
    if constraint.property("Enabled") != Some(&Value::Bool(true)) {
        return None;
    }
    let end = |name| {
        let attachment = constraint.property(name)?.as_instance()?;
        let (part, local) = attachment_on_part(world, attachment)?;
        let body = parts.get(&part)?;
        Some((body.body, body.offset * local))
    };
    let (body0, frame0) = end("Attachment0")?;
    let (body1, frame1) = end("Attachment1")?;
    (body0 != body1).then_some(((body0, body1), (frame0, frame1)))
}

/// The part an Attachment is on, and its pose on the part.
pub(crate) fn attachment_on_part(world: &World, id: InstanceId) -> Option<(InstanceId, Pose)> {
    let attachment = world.get(id)?;
    if attachment.class() != ClassName::Attachment {
        return None;
    }
    let part = attachment.parent()?;
    let class = world.get(part)?.class();
    if !matches!(class, ClassName::Part | ClassName::MeshPart) {
        return None;
    }
    Some((part, local_pose(attachment)?))
}

/// An Attachment's `Position` and `Orientation` on its part, if finite.
fn local_pose(attachment: &Instance) -> Option<Pose> {
    let vector = |name| attachment.property(name).and_then(Value::as_vector3);
    let (position, orientation) = (vector("Position")?, vector("Orientation")?);
    crate::finite(position).then_some(())?;
    crate::finite(orientation).then_some(())?;
    Some(Pose::from_parts(
        to_vec3(position),
        orientation_to_quat(orientation),
    ))
}

/// The joint a constraint's properties describe between `frames`, or
/// `None` for a class that isn't a joint.
fn describe(constraint: &Instance, (frame0, frame1): (Pose, Pose)) -> Option<Described> {
    let number = |name: &str| {
        constraint
            .property(name)
            .and_then(Value::as_number)
            .map(|n| n as f32)
            .filter(|n| !n.is_nan())
            .unwrap_or(0.0)
    };
    let flag = |name: &str| constraint.property(name).and_then(Value::as_bool) == Some(true);
    let actuator = |name: &str| match constraint.property(name) {
        Some(Value::EnumItem(item)) => item.item.as_str(),
        _ => "None",
    };
    let force = |name: &str| max_force(number(name));
    let locked = match constraint.class() {
        ClassName::HingeConstraint => JointAxesMask::LOCKED_REVOLUTE_AXES,
        ClassName::BallSocketConstraint => JointAxesMask::LOCKED_SPHERICAL_AXES,
        ClassName::PrismaticConstraint => JointAxesMask::LOCKED_PRISMATIC_AXES,
        ClassName::CylindricalConstraint => {
            JointAxesMask::LIN_Y
                | JointAxesMask::LIN_Z
                | JointAxesMask::ANG_Y
                | JointAxesMask::ANG_Z
        }
        ClassName::SpringConstraint | ClassName::RopeConstraint | ClassName::RodConstraint => {
            JointAxesMask::empty()
        }
        _ => return None,
    };
    let mut joint = GenericJointBuilder::new(locked)
        .local_frame1(frame0)
        .local_frame2(frame1)
        .build();
    let mut drive = Drive::default();
    let degrees = |name: &str| number(name).to_radians();

    // Turning around the axis: hinges, and cylindrical constraints.
    let turn_actuator = match constraint.class() {
        ClassName::HingeConstraint => Some(("LimitsEnabled", "ActuatorType")),
        ClassName::CylindricalConstraint => Some(("AngularLimitsEnabled", "AngularActuatorType")),
        _ => None,
    };
    if let Some((limits, actuator_name)) = turn_actuator {
        if flag(limits) {
            joint.set_limits(
                JointAxis::AngX,
                ordered(degrees("LowerAngle"), degrees("UpperAngle")),
            );
        }
        match actuator(actuator_name) {
            "Motor" => {
                joint
                    .set_motor_velocity(JointAxis::AngX, number("AngularVelocity"), MOTOR_DAMPING)
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased)
                    .set_motor_max_force(JointAxis::AngX, force("MotorMaxTorque"));
            }
            "Servo" => {
                joint
                    .set_motor(JointAxis::AngX, 0.0, 0.0, SERVO_STIFFNESS, SERVO_DAMPING)
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased)
                    .set_motor_max_force(JointAxis::AngX, force("ServoMaxTorque"));
                drive.turn = Some(Servo {
                    target: wrap(degrees("TargetAngle")),
                    speed: number("AngularSpeed").max(0.0),
                });
            }
            _ => {}
        }
    }

    // Sliding along the axis: prismatic and cylindrical constraints.
    if matches!(
        constraint.class(),
        ClassName::PrismaticConstraint | ClassName::CylindricalConstraint
    ) {
        if flag("LimitsEnabled") {
            joint.set_limits(
                JointAxis::LinX,
                ordered(number("LowerLimit"), number("UpperLimit")),
            );
        }
        match actuator("ActuatorType") {
            "Motor" => {
                joint
                    .set_motor_velocity(JointAxis::LinX, number("Velocity"), MOTOR_DAMPING)
                    .set_motor_model(JointAxis::LinX, MotorModel::ForceBased)
                    .set_motor_max_force(JointAxis::LinX, force("MotorMaxForce"));
            }
            "Servo" => {
                joint
                    .set_motor(JointAxis::LinX, 0.0, 0.0, SERVO_STIFFNESS, SERVO_DAMPING)
                    .set_motor_model(JointAxis::LinX, MotorModel::ForceBased)
                    .set_motor_max_force(JointAxis::LinX, force("ServoMaxForce"));
                drive.slide = Some(Servo {
                    target: number("TargetPosition"),
                    speed: number("Speed").max(0.0),
                });
            }
            _ => {}
        }
    }

    match constraint.class() {
        ClassName::BallSocketConstraint => {
            if flag("LimitsEnabled") {
                let lean = degrees("UpperAngle").abs();
                for axis in [JointAxis::AngY, JointAxis::AngZ] {
                    joint.set_limits(axis, [-lean, lean]);
                }
            }
            if flag("TwistLimitsEnabled") {
                joint.set_limits(
                    JointAxis::AngX,
                    ordered(degrees("TwistLowerAngle"), degrees("TwistUpperAngle")),
                );
            }
        }
        ClassName::SpringConstraint => {
            joint.coupled_axes = JointAxesMask::LIN_AXES;
            joint
                .set_motor_position(
                    JointAxis::LinX,
                    number("FreeLength").max(0.0),
                    number("Stiffness").max(0.0),
                    number("Damping").max(0.0),
                )
                .set_motor_model(JointAxis::LinX, MotorModel::ForceBased);
            let min = number("MinLength").max(0.0);
            joint.set_limits(JointAxis::LinX, ordered(min, number("MaxLength").max(min)));
        }
        ClassName::RopeConstraint => {
            joint.coupled_axes = JointAxesMask::LIN_AXES;
            let length = number("Length").max(0.0);
            joint.set_limits(JointAxis::LinX, [0.0, length]);
            if flag("WinchEnabled") {
                drive.winch = Some(Winch {
                    length,
                    target: number("WinchTarget").max(0.0),
                    speed: number("WinchSpeed").max(0.0),
                    force: number("WinchForce").max(0.0),
                });
            }
        }
        ClassName::RodConstraint => {
            joint.coupled_axes = JointAxesMask::LIN_AXES;
            let length = number("Length").max(0.0);
            joint.set_limits(JointAxis::LinX, [length, length]);
        }
        _ => {}
    }
    // Parts on a hinge, ball socket, prismatic or cylindrical constraint
    // often overlap (an axle in a wheel, a door in its frame), so they
    // don't collide with each other. Ropes, rods and springs still do.
    joint.set_contacts_enabled(locked.is_empty());
    Some(Described { data: joint, drive })
}

/// `[a, b]` smallest first.
fn ordered(a: f32, b: f32) -> [f32; 2] {
    [a.min(b), a.max(b)]
}

/// A max force or torque: infinite (`math.huge`) is as much as there is,
/// and less than nothing is nothing.
fn max_force(n: f32) -> f32 {
    n.clamp(0.0, f32::MAX)
}

/// An angle in radians brought into -π..π.
fn wrap(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
}

/// How far `frame2` is turned from `frame1` around `frame1`'s X axis, in
/// radians from -π to π.
fn angle(frame1: Pose, frame2: Pose) -> f32 {
    let turn = frame1.rotation.inverse() * frame2.rotation;
    // The twist around X of the relative turn.
    wrap(2.0 * turn.x.atan2(turn.w))
}

/// How far `frame2` is from `frame1` along `frame1`'s X axis, in studs.
fn offset(frame1: Pose, frame2: Pose) -> f32 {
    (frame2.translation - frame1.translation).dot(frame1.rotation * Vec3::X)
}

/// `CurrentAngle` (degrees), `CurrentPosition` and `CurrentLength` (studs)
/// of the constraint `id`, worked out from where its attachments are now:
/// the name is one of [`CURRENT`], and the class has it. `None` otherwise,
/// and 0 while an attachment isn't on a part.
pub fn current(world: &World, id: InstanceId, name: &str) -> Option<f64> {
    let constraint = world.get(id)?;
    if !has_current(constraint.class(), name) {
        return None;
    }
    let frame = |name| {
        let attachment = constraint.property(name)?.as_instance()?;
        let (part, local) = attachment_on_part(world, attachment)?;
        let part = world.get(part)?;
        let vector = |name| part.property(name).and_then(Value::as_vector3);
        let pose = Pose::from_parts(
            to_vec3(vector("Position")?),
            orientation_to_quat(vector("Orientation")?),
        );
        Some(pose * local)
    };
    let (Some(frame0), Some(frame1)) = (frame("Attachment0"), frame("Attachment1")) else {
        return Some(0.0);
    };
    let value = match name {
        "CurrentAngle" => angle(frame0, frame1).to_degrees(),
        "CurrentPosition" => offset(frame0, frame1),
        _ => (frame1.translation - frame0.translation).length(),
    };
    Some(f64::from(value))
}

/// The read-only values scripts can read from joints (see [`current`]).
pub const CURRENT: [&str; 3] = ["CurrentAngle", "CurrentPosition", "CurrentLength"];

/// Whether constraints of `class` have the read-only value `name`.
pub fn has_current(class: ClassName, name: &str) -> bool {
    matches!(
        (class, name),
        (
            ClassName::HingeConstraint | ClassName::CylindricalConstraint,
            "CurrentAngle"
        ) | (
            ClassName::PrismaticConstraint | ClassName::CylindricalConstraint,
            "CurrentPosition"
        ) | (
            ClassName::SpringConstraint | ClassName::RopeConstraint | ClassName::RodConstraint,
            "CurrentLength"
        )
    )
}

/// Where each attachment of a constraint is in the world, for drawing:
/// `None` unless both are on parts.
pub fn attachment_points(world: &World, id: InstanceId) -> Option<(Vector3, Vector3)> {
    let constraint = world.get(id)?;
    let point = |name| {
        let attachment = constraint.property(name)?.as_instance()?;
        let (part, local) = attachment_on_part(world, attachment)?;
        let part = world.get(part)?;
        let vector = |name| part.property(name).and_then(Value::as_vector3);
        let pose = Pose::from_parts(
            to_vec3(vector("Position")?),
            orientation_to_quat(vector("Orientation")?),
        );
        Some(crate::to_vector3((pose * local).translation))
    };
    Some((point("Attachment0")?, point("Attachment1")?))
}
