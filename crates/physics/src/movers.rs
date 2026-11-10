//! Movers (Phase 37): LinearVelocity, AngularVelocity, VectorForce, Torque,
//! AlignPosition and AlignOrientation push or turn the body of the part
//! their `Attachment0` is on.
//!
//! They're read from the world once a frame, after the bodies and joints
//! are synced, and act before every fixed step by adding a force or a
//! torque to the body for that step. A VectorForce or Torque adds its own.
//! The others work out the force (or torque) that brings the body to the
//! speed they want by the end of the step, counting gravity and the forces
//! already added that step (the wind, VectorForces, Torques), and add it,
//! clamped to their max. So a LinearVelocity or AlignPosition holds its part
//! up, and an AlignOrientation holds a part level against a VectorForce off
//! its center of mass.
//!
//! Contacts and joints push in rapier's solver, after the movers, so those
//! can't be counted ahead. Instead each velocity or align mover compares
//! the speed its body ends a step with against what gravity and the forces
//! added would have given, and learns the difference as an extra force (or
//! torque) for the next steps, within its max ([`Learned`]). So an
//! AngularVelocity keeps a top spinning on the floor at its speed despite
//! the friction.
//!
//! An align closes the gap as a critically damped spring of natural
//! frequency `Responsiveness`, worked out implicitly so it's stable at any
//! Responsiveness: smooth, without overshooting. With `RigidityEnabled` it
//! closes the whole gap in a step instead.

use std::collections::{BTreeMap, HashSet};

use axcenity_core::{ClassName, Instance, InstanceId, Value, World};
use rapier3d::dynamics::{RigidBody, RigidBodySet};
use rapier3d::glamx::{Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::prelude::RigidBodyHandle;

use crate::joints::attachment_on_part;
use crate::{Body, STEP_SECONDS, to_vec3};

/// How far a mover's extra force moves toward what the last step showed
/// was missing. Half settles in a few steps without overreacting to a bump.
const LEARNING: f32 = 0.5;

/// One enabled mover with its part's body.
pub(crate) struct Mover {
    id: InstanceId,
    body: RigidBodyHandle,
    /// Attachment0's frame on the body.
    frame0: Pose,
    /// Attachment1's body and its frame on it, when it's on a part.
    end1: Option<(RigidBodyHandle, Pose)>,
    kind: Kind,
}

/// What a mover does, from its properties.
#[derive(Clone, Copy, Debug)]
enum Kind {
    LinearVelocity {
        velocity: Vec3,
        max_force: f32,
        relative: RelativeTo,
    },
    AngularVelocity {
        velocity: Vec3,
        max_torque: f32,
        relative: RelativeTo,
    },
    Force {
        force: Vec3,
        relative: RelativeTo,
        at_center: bool,
    },
    Torque {
        torque: Vec3,
        relative: RelativeTo,
    },
    AlignPosition {
        /// The world point in OneAttachment Mode, `None` for Attachment1.
        target: Option<Vec3>,
        max_force: f32,
        align: Align,
    },
    AlignOrientation {
        /// The world rotation in OneAttachment Mode, `None` for Attachment1.
        target: Option<Quat>,
        max_torque: f32,
        align: Align,
    },
}

impl Kind {
    /// VectorForces and Torques add their push first, so the others can
    /// count it.
    fn is_push(self) -> bool {
        matches!(self, Kind::Force { .. } | Kind::Torque { .. })
    }

    /// For a velocity or align: whether it pushes (rather than turns), and
    /// with at most how much.
    fn limit(self) -> Option<(bool, f32)> {
        match self {
            Kind::LinearVelocity { max_force, .. } | Kind::AlignPosition { max_force, .. } => {
                Some((true, max_force))
            }
            Kind::AngularVelocity { max_torque, .. }
            | Kind::AlignOrientation { max_torque, .. } => Some((false, max_torque)),
            Kind::Force { .. } | Kind::Torque { .. } => None,
        }
    }
}

/// What a velocity or align mover has learned about the pushes on its body
/// it isn't told about: contacts and joints (see the module's notes).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Learned {
    body: RigidBodyHandle,
    /// The speed and spin the body would have ended the last step with if
    /// only gravity and the forces added had acted on it.
    expected: (Vec3, Vec3),
    /// The force or torque that makes up for the rest.
    extra: Vec3,
}

/// Forgets what was learned about `bodies`, which a script just moved or
/// threw: the speed they end the next step with says nothing about
/// contacts.
pub(crate) fn forget(
    learned: &mut BTreeMap<InstanceId, Learned>,
    bodies: &HashSet<RigidBodyHandle>,
) {
    learned.retain(|_, learned| !bodies.contains(&learned.body));
}

/// How an align gets to its target.
#[derive(Clone, Copy, Debug)]
struct Align {
    /// The fastest it moves (studs or radians a second).
    max_speed: f32,
    responsiveness: f32,
    rigid: bool,
}

impl Align {
    /// The speed toward the target to have at the end of this step, from
    /// the gap `gap` and the speed `speed` now, both relative to the target.
    fn speed(self, gap: Vec3, speed: Vec3) -> Vec3 {
        let dt = STEP_SECONDS as f32;
        let wanted = if self.rigid {
            gap / dt
        } else {
            // Implicit Euler on x'' = r²·gap − 2r·x': stable for any r.
            let r = self.responsiveness;
            (speed + gap * (dt * r * r)) / (1.0 + 2.0 * r * dt + r * r * dt * dt)
        };
        wanted.clamp_length_max(self.max_speed)
    }
}

/// The axes a mover's vector is in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RelativeTo {
    World,
    Attachment0,
    Attachment1,
}

/// The enabled movers in `world` whose Attachment0 is on a part with a
/// body, in tree order.
pub(crate) fn read(world: &World, parts: &BTreeMap<InstanceId, Body>) -> Vec<Mover> {
    world
        .all_below(world.workspace(), ClassName::is_mover)
        .into_iter()
        .filter_map(|id| {
            let mover = world.get(id)?;
            if mover.property("Enabled") != Some(&Value::Bool(true)) {
                return None;
            }
            let end = |name| {
                let attachment = mover.property(name)?.as_instance()?;
                let (part, local) = attachment_on_part(world, attachment)?;
                let body = parts.get(&part)?;
                Some((body.body, body.offset * local))
            };
            let (body, frame0) = end("Attachment0")?;
            Some(Mover {
                id,
                body,
                frame0,
                end1: end("Attachment1"),
                kind: describe(mover)?,
            })
        })
        .collect()
}

/// What a mover's properties describe, or `None` for a class that isn't one.
fn describe(mover: &Instance) -> Option<Kind> {
    let number = |name: &str| {
        mover
            .property(name)
            .and_then(Value::as_number)
            .map(|n| n as f32)
            .filter(|n| !n.is_nan())
            .unwrap_or(0.0)
    };
    // Infinite (`math.huge`) is as much as there is.
    let max = |name: &str| number(name).clamp(0.0, f32::MAX);
    let vector = |name: &str| {
        mover
            .property(name)
            .and_then(Value::as_vector3)
            .filter(|v| crate::finite(*v))
            .map_or(Vec3::ZERO, to_vec3)
    };
    let item = |name: &str| match mover.property(name) {
        Some(Value::EnumItem(item)) => item.item.as_str(),
        _ => "",
    };
    let relative = match item("RelativeTo") {
        "Attachment0" => RelativeTo::Attachment0,
        "Attachment1" => RelativeTo::Attachment1,
        _ => RelativeTo::World,
    };
    let one_attachment = item("Mode") == "OneAttachment";
    let align = |max_speed| Align {
        max_speed: max(max_speed),
        responsiveness: number("Responsiveness").max(0.0),
        rigid: mover.property("RigidityEnabled") == Some(&Value::Bool(true)),
    };
    Some(match mover.class() {
        ClassName::LinearVelocity => Kind::LinearVelocity {
            velocity: vector("VectorVelocity"),
            max_force: max("MaxForce"),
            relative,
        },
        ClassName::AngularVelocity => Kind::AngularVelocity {
            velocity: vector("AngularVelocity"),
            max_torque: max("MaxTorque"),
            relative,
        },
        ClassName::VectorForce => Kind::Force {
            force: vector("Force"),
            relative,
            at_center: mover.property("ApplyAtCenterOfMass") == Some(&Value::Bool(true)),
        },
        ClassName::Torque => Kind::Torque {
            torque: vector("Torque"),
            relative,
        },
        ClassName::AlignPosition => Kind::AlignPosition {
            target: one_attachment.then(|| vector("Position")),
            max_force: max("MaxForce"),
            align: align("MaxVelocity"),
        },
        ClassName::AlignOrientation => Kind::AlignOrientation {
            target: one_attachment.then(|| {
                let cframe = mover
                    .property("CFrame")
                    .and_then(Value::as_cframe)
                    .unwrap_or_default();
                let rotation = Quat::from_array(cframe.rotation);
                if rotation.is_finite() && rotation.length_squared() > 0.0 {
                    rotation.normalize()
                } else {
                    Quat::IDENTITY
                }
            }),
            max_torque: max("MaxTorque"),
            align: align("MaxAngularVelocity"),
        },
        _ => return None,
    })
}

/// Adds each mover's force or torque to its body for the next step. Pushes
/// go first, so velocities and aligns can count them. Velocities and aligns
/// first learn from how the last step went, and afterwards note what they
/// expect of this one.
pub(crate) fn apply(
    bodies: &mut RigidBodySet,
    movers: &[Mover],
    gravity: Vec3,
    learned: &mut BTreeMap<InstanceId, Learned>,
) {
    let dt = STEP_SECONDS as f32;
    let others = || movers.iter().filter(|mover| !mover.kind.is_push());
    learned
        .retain(|id, learned| others().any(|mover| mover.id == *id && mover.body == learned.body));
    for mover in others() {
        let (Some(learned), Some(body), Some((pushes, max))) = (
            learned.get_mut(&mover.id),
            bodies.get(mover.body),
            mover.kind.limit(),
        ) else {
            continue;
        };
        // What the step took away from what was expected: the contacts'
        // and joints' push, the other way, which is what to add from now
        // on. The extra force was expected too, so a wrong one fades away.
        let (speed, spin) = learned.expected;
        let missing = if pushes {
            body.mass() * (speed - body.linvel()) / dt
        } else {
            let inertia = body.mass_properties().effective_angular_inertia();
            inertia.mul_vec((spin - body.angvel()) / dt)
        };
        learned.extra = learned.extra.lerp(missing, LEARNING).clamp_length_max(max);
    }

    let pushes = movers.iter().filter(|mover| mover.kind.is_push());
    for mover in pushes.chain(others()) {
        let extra = learned.get(&mover.id).map_or(Vec3::ZERO, |l| l.extra);
        apply_one(bodies, mover, gravity, extra);
    }

    for mover in others() {
        let Some(body) = bodies.get(mover.body).filter(|body| body.is_dynamic()) else {
            continue;
        };
        let speed =
            body.linvel() + (gravity * body.gravity_scale() + body.user_force() / body.mass()) * dt;
        let inverse = body.mass_properties().effective_world_inv_inertia;
        let spin = (body.angvel() + inverse.mul_vec(body.user_torque()) * dt)
            / (1.0 + dt * body.angular_damping());
        let expected = (speed, spin);
        learned
            .entry(mover.id)
            .and_modify(|learned| learned.expected = expected)
            .or_insert(Learned {
                body: mover.body,
                expected,
                extra: Vec3::ZERO,
            });
    }
}

/// Adds one mover's force or torque; a velocity or align adds `extra` too.
fn apply_one(bodies: &mut RigidBodySet, mover: &Mover, gravity: Vec3, extra: Vec3) {
    // Attachment1's pose in the world, and its body's.
    let end1 = mover.end1.and_then(|(handle, frame)| {
        let body = bodies.get(handle)?;
        Some((*body.position() * frame, handle))
    });
    let Some(body) = bodies.get_mut(mover.body) else {
        return;
    };
    if !body.is_dynamic() || body.mass() <= 0.0 {
        return;
    }
    let frame0 = *body.position() * mover.frame0;
    // A vector in the RelativeTo frame's axes, in the world's.
    let turned = |relative: RelativeTo, v: Vec3| match relative {
        RelativeTo::World => Some(v),
        RelativeTo::Attachment0 => Some(frame0.rotation * v),
        RelativeTo::Attachment1 => end1.map(|(frame1, _)| frame1.rotation * v),
    };
    match mover.kind {
        Kind::Force {
            force,
            relative,
            at_center,
        } => {
            let Some(force) = turned(relative, force).filter(|f| *f != Vec3::ZERO) else {
                return;
            };
            if at_center {
                body.add_force(force, true);
            } else {
                body.add_force_at_point(force, frame0.translation, true);
            }
        }
        Kind::Torque { torque, relative } => {
            if let Some(torque) = turned(relative, torque).filter(|t| *t != Vec3::ZERO) {
                body.add_torque(torque, true);
            }
        }
        Kind::LinearVelocity {
            velocity,
            max_force,
            relative,
        } => {
            if let Some(velocity) = turned(relative, velocity) {
                push_to(body, velocity - body.linvel(), max_force, gravity, extra);
            }
        }
        Kind::AngularVelocity {
            velocity,
            max_torque,
            relative,
        } => {
            if let Some(velocity) = turned(relative, velocity) {
                turn_to(body, velocity, max_torque, extra);
            }
        }
        Kind::AlignPosition {
            target,
            max_force,
            align,
        } => {
            let at = frame0.translation;
            // Where it's going, and how fast that's moving.
            let (goal, goal_speed) = match (target, end1) {
                (Some(point), _) => (point, Vec3::ZERO),
                (None, Some((frame1, handle))) if handle != mover.body => {
                    let point = frame1.translation;
                    (point, velocity_at(bodies, handle, point))
                }
                _ => return,
            };
            let body = bodies.get_mut(mover.body).expect("found above");
            let speed = body.velocity_at_point(at) - goal_speed;
            let wanted = align.speed(goal - at, speed);
            push_to(body, wanted - speed, max_force, gravity, extra);
        }
        Kind::AlignOrientation {
            target,
            max_torque,
            align,
        } => {
            let (goal, goal_spin) = match (target, end1) {
                (Some(rotation), _) => (rotation, Vec3::ZERO),
                (None, Some((frame1, handle))) if handle != mover.body => {
                    let spin = bodies.get(handle).map_or(Vec3::ZERO, |b| b.angvel());
                    (frame1.rotation, spin)
                }
                _ => return,
            };
            let body = bodies.get_mut(mover.body).expect("found above");
            // The turn from where Attachment0 faces to the goal, the short
            // way round, as an axis times radians.
            let mut turn = goal * frame0.rotation.inverse();
            if turn.w < 0.0 {
                turn = -turn;
            }
            let spin = body.angvel() - goal_spin;
            let wanted = align.speed(turn.to_scaled_axis(), spin);
            turn_to(body, wanted + goal_spin, max_torque, extra);
        }
    }
}

/// How fast the point `at` on the body `handle` moves.
fn velocity_at(bodies: &RigidBodySet, handle: RigidBodyHandle, at: Vec3) -> Vec3 {
    bodies
        .get(handle)
        .map_or(Vec3::ZERO, |body| body.velocity_at_point(at))
}

/// Pushes `body` through its center of mass to change its speed by
/// `change` this step, against gravity, the forces already on it and the
/// `extra` it learned, with at most `max_force`.
fn push_to(body: &mut RigidBody, change: Vec3, max_force: f32, gravity: Vec3, extra: Vec3) {
    let dt = STEP_SECONDS as f32;
    let mass = body.mass();
    let pull = gravity * body.gravity_scale();
    let force =
        (mass * (change / dt - pull) - body.user_force() + extra).clamp_length_max(max_force);
    if force != Vec3::ZERO && force.is_finite() {
        body.add_force(force, true);
    }
}

/// Turns `body` to spin at `spin` by the end of this step, against the
/// torques already on it and the `extra` it learned, with at most
/// `max_torque`.
fn turn_to(body: &mut RigidBody, spin: Vec3, max_torque: f32, extra: Vec3) {
    let dt = STEP_SECONDS as f32;
    // rapier slows the spin by its angular damping after the step, so aim
    // a little faster.
    let spin = spin * (1.0 + dt * body.angular_damping());
    let inertia = body.mass_properties().effective_angular_inertia();
    let torque = (inertia.mul_vec((spin - body.angvel()) / dt) - body.user_torque() + extra)
        .clamp_length_max(max_torque);
    if torque != Vec3::ZERO && torque.is_finite() {
        body.add_torque(torque, true);
    }
}
