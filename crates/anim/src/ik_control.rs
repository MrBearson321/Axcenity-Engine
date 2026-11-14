//! `IKControl`s (Phase 79): inverse kinematics during the game, after a
//! skinned MeshPart's Animator and before drawing.
//!
//! An IKControl in the MeshPart (or in its Animator) turns the bones from
//! `ChainRoot` down to `EndEffector` toward its `Target`, a Part or an
//! Attachment:
//! - Position puts the end effector at the Target, with the two bones above
//!   it when `ChainRoot` is empty, bending toward `Pole` if it has one.
//! - Rotation turns the end effector as the Target is turned from the
//!   MeshPart.
//! - Transform does both.
//! - LookAt faces the end effector at the Target, sharing the turn among the
//!   bones from `ChainRoot` down, and never turns further than `MaxAngle`
//!   from the way the character faces.
//!
//! The pose moves toward what's solved by `Weight`. `Weight` (or `Enabled`)
//! fades at an even pace over `SmoothTime` seconds, and the Target's place
//! is followed smoothly, so nothing pops.

use std::collections::HashMap;

use axcenity_assets::{BoneTransform, Pose, Rig};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{Mat4, Quat, Vec3};

use crate::bones::{bone_paths, find_bone};
use crate::feet;
use crate::ik;
use crate::ragdoll::{self, rotation_of, to_vec3};

/// `Target` is within this share of `SmoothTime` of where it is now.
const FOLLOW_SHARE: f64 = 1.0 / 3.0;
/// The bones above the end effector that turn when `ChainRoot` is empty.
const DEFAULT_CHAIN: usize = 2;

/// An IKControl's `Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Transform,
    Position,
    Rotation,
    LookAt,
}

impl Kind {
    /// Reads a `Type`, in any case. Anything else is `None`.
    fn parse(text: &str) -> Option<Self> {
        [
            ("transform", Kind::Transform),
            ("position", Kind::Position),
            ("rotation", Kind::Rotation),
            ("lookat", Kind::LookAt),
        ]
        .into_iter()
        .find(|(name, _)| text.eq_ignore_ascii_case(name))
        .map(|(_, kind)| kind)
    }
}

/// Somewhere in the world, and how it's turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub position: Vec3,
    rotation: Quat,
}

/// Where an IKControl has got to.
#[derive(Default)]
pub(crate) struct Run {
    /// `false` until it first runs: a control starts where it's asked to be.
    started: bool,
    /// How far it counts now, 0 to 1, moving toward `Weight`.
    weight: f32,
    /// Where it's following its Target from, kept after the Target goes so
    /// it can fade out from there.
    goal: Option<Frame>,
}

impl Run {
    /// Moves on by `dt` toward `wanted` (the weight) and `target`.
    fn advance(&mut self, wanted: f32, target: Option<Frame>, smooth: f64, dt: f64) {
        if !self.started {
            self.started = true;
            self.weight = wanted;
            self.goal = target;
            return;
        }
        let step = if smooth > 0.0 { dt / smooth } else { 1.0 };
        let gap = wanted - self.weight;
        self.weight += gap.clamp(-step as f32, step as f32);
        let Some(target) = target else {
            return;
        };
        self.goal = Some(match self.goal {
            Some(goal) if smooth > 0.0 => {
                let follow = (1.0 - (-dt / (smooth * FOLLOW_SHARE)).exp()) as f32;
                Frame {
                    position: goal.position.lerp(target.position, follow),
                    rotation: goal.rotation.slerp(target.rotation, follow),
                }
            }
            _ => target,
        });
    }
}

/// Plays `controls` (in the skinned MeshPart `owner`) over `pose`, in order,
/// moved on by `dt` seconds. `runs` is where each has got to. Returns what's
/// wrong with them, one sentence each.
pub(crate) fn apply(
    world: &World,
    owner: InstanceId,
    rig: &Rig,
    pose: &mut Pose,
    controls: &[InstanceId],
    runs: &mut HashMap<InstanceId, Run>,
    dt: f64,
) -> Vec<String> {
    let mut problems = Vec::new();
    runs.retain(|id, _| controls.contains(id));
    let Some(place) = ragdoll::placement(world, owner, rig) else {
        return problems;
    };
    let Some(turned) = world
        .get(owner)
        .and_then(|part| part.property("Orientation"))
        .and_then(Value::as_vector3)
        .map(rotation_of)
    else {
        return problems;
    };
    let unplace = place.inverse();
    let paths = bone_paths(rig);
    for &control in controls {
        let Some(instance) = world.get(control) else {
            continue;
        };
        let name = world.full_name(control).unwrap_or_default();
        let number = |property: &str, default: f64| {
            instance
                .property(property)
                .and_then(Value::as_number)
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let enabled = instance.property("Enabled").and_then(Value::as_bool) != Some(false);
        let target = target_of(world, control, &name, &mut problems);
        let wanted = if enabled && target.is_some() {
            number("Weight", 1.0).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let smooth = number("SmoothTime", 0.0).max(0.0);
        let run = runs.entry(control).or_default();
        run.advance(wanted, target, smooth, dt);
        let Some(goal) = run.goal else {
            continue;
        };
        if run.weight <= 0.0 {
            continue;
        }
        let text = |property: &str| {
            instance
                .property(property)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let Some(kind) = Kind::parse(text("Type")) else {
            problems.push(format!(
                "The IKControl `{name}` has the Type `{}`, which isn't Transform, Position, Rotation or LookAt.",
                text("Type")
            ));
            continue;
        };
        let Some(end) = bone_named(rig, &paths, text("EndEffector")) else {
            problems.push(format!(
                "The IKControl `{name}` has the EndEffector `{}`, which the skeleton doesn't have.",
                text("EndEffector")
            ));
            continue;
        };
        let chain = match chain_of(rig, &paths, end, text("ChainRoot"), kind) {
            Ok(chain) => chain,
            Err(problem) => {
                problems.push(format!("The IKControl `{name}`: {problem}"));
                continue;
            }
        };
        let aim = Frame {
            position: unplace.transform_point3(goal.position),
            rotation: (turned.inverse() * goal.rotation).normalize(),
        };
        let mut solved = pose.clone();
        match kind {
            Kind::Position | Kind::Transform => {
                let pole = pole_of(world, control).map(|at| unplace.transform_point3(at));
                ik::reach_toward(rig, &mut solved, &chain, end, aim.position, pole);
                if kind == Kind::Transform {
                    turn_like(rig, &mut solved, end, aim.rotation);
                }
            }
            Kind::Rotation => turn_like(rig, &mut solved, end, aim.rotation),
            Kind::LookAt => {
                let forward = facing(rig, instance.property("Forward"));
                let limit = (number("MaxAngle", 75.0) as f32).max(0.0).to_radians();
                look_at(rig, &mut solved, &chain, end, aim.position, forward, limit);
            }
        }
        if solved.iter().all(|bone| bone.rotation.is_finite()) {
            for (bone, ik) in pose.iter_mut().zip(&solved) {
                *bone = bone.lerp(ik, run.weight);
            }
        }
    }
    problems
}

/// The world frame of `control`'s `Target`, or `None` if it has none (or
/// it's neither a Part nor an Attachment, which is reported).
fn target_of(
    world: &World,
    control: InstanceId,
    name: &str,
    problems: &mut Vec<String>,
) -> Option<Frame> {
    let target = world.get(control)?.property("Target")?.as_instance()?;
    let frame = frame_of(world, target);
    if frame.is_none() {
        problems.push(format!(
            "The IKControl `{name}` has a Target that isn't a Part or an Attachment."
        ));
    }
    frame
}

/// Where `control`'s `Pole` is in the world.
fn pole_of(world: &World, control: InstanceId) -> Option<Vec3> {
    let pole = world.get(control)?.property("Pole")?.as_instance()?;
    Some(frame_of(world, pole)?.position)
}

/// Where a Part or Attachment is in the world.
pub(crate) fn frame_of(world: &World, id: InstanceId) -> Option<Frame> {
    let instance = world.get(id)?;
    if instance.class() == ClassName::Attachment {
        let part = frame_of(world, instance.parent()?)?;
        let at = local_frame(instance)?;
        return Some(Frame {
            position: part.position + part.rotation * at.position,
            rotation: (part.rotation * at.rotation).normalize(),
        });
    }
    if !instance.class().is_base_part() {
        return None;
    }
    local_frame(instance)
}

/// An Instance's `Position` and `Orientation`, as they are.
fn local_frame(instance: &axcenity_core::Instance) -> Option<Frame> {
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    Some(Frame {
        position: to_vec3(vector("Position")?),
        rotation: rotation_of(vector("Orientation")?),
    })
}

/// The bone an IKControl names: by its path, its name, or its name without
/// a namespace, so `Head` finds `mixamorig:Head`.
fn bone_named(rig: &Rig, paths: &[String], name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    find_bone(rig, paths, name).or_else(|| {
        rig.bones.iter().position(|bone| {
            let short = bone.name.rsplit(':').next().unwrap_or(&bone.name);
            short.eq_ignore_ascii_case(name)
        })
    })
}

/// The bones of the chain above `end`, nearest first, up to and including
/// `ChainRoot`. An empty `ChainRoot` is the two bones above `end`, or for
/// LookAt and Rotation no bones above it at all.
fn chain_of(
    rig: &Rig,
    paths: &[String],
    end: usize,
    root: &str,
    kind: Kind,
) -> Result<Vec<usize>, String> {
    if root.is_empty() {
        return Ok(match kind {
            Kind::Position | Kind::Transform => ik::chain(rig, end, DEFAULT_CHAIN, |_| true),
            Kind::Rotation | Kind::LookAt => Vec::new(),
        });
    }
    let Some(root_bone) = bone_named(rig, paths, root) else {
        return Err(format!(
            "its ChainRoot `{root}` isn't a bone of the skeleton."
        ));
    };
    if root_bone == end {
        return Ok(Vec::new());
    }
    if !ik::is_below(rig, end, root_bone) {
        return Err(format!(
            "its ChainRoot `{root}` isn't above its EndEffector."
        ));
    }
    Ok(ik::chain(rig, end, rig.bones.len(), |_| true)
        .into_iter()
        .scan(false, |past_root, bone| {
            if *past_root {
                return None;
            }
            *past_root = bone == root_bone;
            Some(bone)
        })
        .collect())
}

/// The way the character faces, in the rig's file space: an IKControl's
/// `Forward`, or if it's zero the way its toes point, or else -Z.
fn facing(rig: &Rig, forward: Option<&Value>) -> Vec3 {
    forward
        .and_then(Value::as_vector3)
        .map(|Vector3 { x, y, z }| Vec3::new(x, y, z))
        .and_then(Vec3::try_normalize)
        .or_else(|| feet::rig_forward(rig))
        .unwrap_or(Vec3::NEG_Z)
}

/// Turns `end` so it's turned from where it rests as `turn` says.
fn turn_like(rig: &Rig, pose: &mut [BoneTransform], end: usize, turn: Quat) {
    let world = rig.world(pose);
    let rest = turn_at(&rig.world(&rig.rest_pose())[end]);
    let parent = ik::parent_turn(rig, &world, end);
    pose[end].rotation = (parent.inverse() * turn * rest).normalize();
}

/// How a bone's world transform is turned.
fn turn_at(matrix: &Mat4) -> Quat {
    matrix.to_scale_rotation_translation().1
}

/// Faces `end` at `target`, within `limit` radians of `forward`, sharing
/// the turn among `chain` (bones above `end`, nearest first) and `end`
/// itself: each takes its share of what's left to turn.
fn look_at(
    rig: &Rig,
    pose: &mut [BoneTransform],
    chain: &[usize],
    end: usize,
    target: Vec3,
    forward: Vec3,
    limit: f32,
) {
    // Which way the bone's own axes make forward, from its rest pose.
    let axis = turn_at(&rig.world(&rig.rest_pose())[end]).inverse() * forward;
    let bones: Vec<usize> = chain.iter().rev().copied().chain([end]).collect();
    for (done, &bone) in bones.iter().enumerate() {
        let world = rig.world(pose);
        let at = world[end].w_axis.truncate();
        let Some(wanted) = (target - at).try_normalize() else {
            return;
        };
        let wanted = within(forward, wanted, limit);
        let facing = (turn_at(&world[end]) * axis).normalize();
        let share = 1.0 / (bones.len() - done) as f32;
        let turn = Quat::IDENTITY.slerp(Quat::from_rotation_arc(facing, wanted), share);
        ik::turn_bone(rig, pose, &world, bone, turn);
    }
}

/// `wanted`, or the nearest direction to it within `limit` radians of `from`.
fn within(from: Vec3, wanted: Vec3, limit: f32) -> Vec3 {
    let angle = from.angle_between(wanted);
    if angle <= limit {
        return wanted;
    }
    let axis = from
        .cross(wanted)
        .try_normalize()
        .unwrap_or_else(|| from.any_orthonormal_vector());
    Quat::from_axis_angle(axis, limit) * from
}
