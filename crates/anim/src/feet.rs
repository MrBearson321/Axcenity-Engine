//! A character controller's `FootPlacement` (Phase 79): each foot is
//! planted on what's under it, on slopes and stairs, and the hips lower to
//! match. It runs on each device, after the Animator's pose, from the
//! MeshPart's bones and rays the host's [`GroundProbe`] casts (`anim` has no
//! physics of its own).
//!
//! A foot keeps the height the animation gives it above the character's
//! feet, moved by how much higher or lower the ground under it is than the
//! bottom of the MeshPart: a walk's lifted foot stays lifted, and on flat
//! ground nothing changes. So a character in the air, with no ground near
//! its feet, plays its plain animation.

use axcenity_assets::{Assets, Pose, Rig};
use axcenity_core::{InstanceId, Value, Vector3, World};
use glam::{Quat, Vec3};

use crate::ik;
use crate::ragdoll::{self, Side, bone_kind, common_ancestor};

/// A foot or the hips are this many seconds from where they're going.
const SMOOTHING: f32 = 0.06;
/// Ground steeper than this (the normal's height) isn't stood on: it's a
/// wall beside the foot.
const SHALLOWEST_NORMAL: f32 = 0.5;
/// How far up or down a foot looks, as a share of the leg's length.
const REACH_SHARE: f32 = 0.5;
/// A foot this far above its resting height, as a share of the leg's
/// length, has left the ground and isn't laid flat on it.
const LIFTED_SHARE: f32 = 0.25;

/// What a ray found (see [`GroundProbe`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    pub position: Vector3,
    pub normal: Vector3,
    /// How far along the ray; 0 if the ray started inside something.
    pub distance: f32,
}

/// Casts a ray at solid ground from a point along a direction (its length
/// is how far it reaches), passing by the given parts. The hosts give
/// `character::foot_ground`.
pub type GroundProbe = fn(&World, &Assets, Vector3, Vector3, &[InstanceId]) -> Option<GroundHit>;

/// A leg's bones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Leg {
    foot: usize,
    calf: usize,
    thigh: usize,
    /// The bone under the foot, if it has one: the way the toes point.
    toe: Option<usize>,
}

/// The bones foot placement moves, and where the ankles rest.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Legs {
    hips: usize,
    legs: [Leg; 2],
    /// Each ankle's resting place in the file's space.
    rest: [Vec3; 2],
}

impl Legs {
    /// The bones of a two-legged skeleton, found by name (`LeftFoot`,
    /// `mixamorig:RightFoot`, `foot.L`), or `None` without them.
    pub(crate) fn find(rig: &Rig) -> Option<Self> {
        let leg = |side: Side| {
            let foot = rig.bones.iter().position(|bone| {
                let (kind, on) = bone_kind(&bone.name);
                on == Some(side) && (kind == "foot" || kind == "ankle")
            })?;
            let above = ik::chain(rig, foot, 2, |_| true);
            let (&calf, &thigh) = (above.first()?, above.get(1)?);
            let toe = rig.bones.iter().position(|bone| bone.parent == Some(foot));
            Some(Leg {
                foot,
                calf,
                thigh,
                toe,
            })
        };
        let legs = [leg(Side::Left)?, leg(Side::Right)?];
        let hips = common_ancestor(rig, legs[0].thigh, legs[1].thigh)?;
        let joints = ik::joint_positions(rig, &rig.rest_pose());
        Some(Self {
            hips,
            legs,
            rest: [joints[legs[0].foot], joints[legs[1].foot]],
        })
    }
}

/// The way a two-legged skeleton's toes point at rest, in the file's space.
pub(crate) fn rig_forward(rig: &Rig) -> Option<Vec3> {
    let legs = Legs::find(rig)?;
    let joints = ik::joint_positions(rig, &rig.rest_pose());
    let along: Vec3 = legs
        .legs
        .iter()
        .filter_map(|leg| Some(joints[leg.toe?] - joints[leg.foot]))
        .sum();
    Vec3::new(along.x, 0.0, along.z).try_normalize()
}

/// How far a foot has been moved, and the ground it's on.
#[derive(Clone, Copy, Debug)]
struct Planted {
    lift: f32,
    normal: Vec3,
}

impl Default for Planted {
    fn default() -> Self {
        Self {
            lift: 0.0,
            normal: Vec3::Y,
        }
    }
}

/// Where a character's foot placement has got to.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Run {
    /// `false` until it first runs: the feet start planted.
    started: bool,
    feet: [Planted; 2],
}

impl Run {
    /// Whether the feet are still off where the animation has them.
    pub(crate) fn busy(&self) -> bool {
        self.feet
            .iter()
            .any(|foot| foot.lift.abs() > 1e-4 || foot.normal.y < 1.0 - 1e-5)
    }
}

/// The controller's `FootPlacement` for the skinned MeshPart `owner`, and
/// the Instance the controller's parts are in. `None` with no controller.
pub(crate) fn wanted(world: &World, owner: InstanceId) -> Option<(bool, InstanceId)> {
    let mut at = Some(owner);
    while let Some(id) = at.filter(|&id| id != world.workspace()) {
        let instance = world.get(id)?;
        for &child in instance.children() {
            let Some(child) = world.get(child) else {
                continue;
            };
            if child.class().is_character_controller() {
                let on = child.property("FootPlacement").and_then(Value::as_bool);
                return Some((on == Some(true), id));
            }
        }
        at = instance.parent();
    }
    None
}

/// Plants the feet of the skinned MeshPart `owner` on the ground under them
/// in `pose`, with `on` saying its `FootPlacement` is on (off, they ease back
/// to the animation). `holder` is what the character's parts are in.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plant(
    world: &World,
    assets: &Assets,
    probe: GroundProbe,
    owner: InstanceId,
    holder: InstanceId,
    rig: &Rig,
    legs: &Legs,
    pose: &mut Pose,
    run: &mut Run,
    on: bool,
    dt: f64,
) {
    if !on && !run.busy() {
        return;
    }
    let Some(place) = ragdoll::placement(world, owner, rig) else {
        return;
    };
    let Some(bottom) = bottom_of(world, owner) else {
        return;
    };
    let joints = ik::joint_positions(rig, pose);
    let place_inv = place.inverse();
    let in_world = |point: Vec3| place.transform_point3(point);
    let ankles = legs.legs.map(|leg| in_world(joints[leg.foot]));
    let rest_ankles = legs.rest.map(in_world);
    let reach = legs.legs.map(|leg| {
        let thigh = in_world(joints[leg.thigh]);
        let knee = in_world(joints[leg.calf]);
        thigh.distance(knee) + knee.distance(in_world(joints[leg.foot]))
    });
    let reach = reach[0].max(reach[1]);
    if !reach.is_finite() || reach <= 0.0 {
        return;
    }

    let goals = if on {
        goals(world, assets, probe, owner, holder, bottom, &ankles, reach)
    } else {
        [Planted::default(); 2]
    };
    let follow = if !run.started {
        1.0
    } else {
        1.0 - (-(dt as f32) / SMOOTHING).exp()
    };
    run.started = true;
    for (foot, goal) in run.feet.iter_mut().zip(goals) {
        foot.lift += (goal.lift - foot.lift) * follow;
        foot.normal = foot
            .normal
            .lerp(goal.normal, follow)
            .try_normalize()
            .unwrap_or(Vec3::Y);
    }

    // The hips go down as far as the lowest foot needs, so it can reach.
    let drop = run.feet[0].lift.min(run.feet[1].lift).min(0.0);
    let before = rig.world(pose);
    if drop != 0.0 {
        let in_file = place_inv.transform_vector3(Vec3::Y * drop);
        let parent = rig.bones[legs.hips]
            .parent
            .map_or(glam::Mat4::IDENTITY, |parent| before[parent]);
        pose[legs.hips].translation += parent.inverse().transform_vector3(in_file);
    }

    // Leans the file's space as `normal` leans from straight up.
    let (_, turned, _) = place.to_scale_rotation_translation();
    for (side, leg) in legs.legs.iter().enumerate() {
        let planted = run.feet[side];
        if drop == 0.0 && planted.lift.abs() <= 1e-4 && planted.normal.y >= 1.0 - 1e-5 {
            continue;
        }
        let target = ankles[side] + Vec3::Y * planted.lift;
        let pole = leg.toe.and_then(|toe| {
            let toes = (joints[toe] - joints[leg.foot]).try_normalize()?;
            Some(joints[leg.calf] + toes * joints[leg.thigh].distance(joints[leg.calf]))
        });
        ik::reach_toward(
            rig,
            pose,
            &[leg.calf, leg.thigh],
            leg.foot,
            place_inv.transform_point3(target),
            pole,
        );
        // A foot on the ground lies flat on it, keeping the animation's
        // turn; one the animation lifts away is left as it is.
        let lifted = (ankles[side].y - rest_ankles[side].y) / (reach * LIFTED_SHARE);
        let flat = 1.0 - lifted.clamp(0.0, 1.0);
        let lean = Quat::IDENTITY.slerp(Quat::from_rotation_arc(Vec3::Y, planted.normal), flat);
        let lean = turned.inverse() * lean * turned;
        let now = rig.world(pose);
        let animated = before[leg.foot].to_scale_rotation_translation().1;
        let parent = ik::parent_turn(rig, &now, leg.foot);
        pose[leg.foot].rotation = (parent.inverse() * lean * animated).normalize();
    }
}

/// What's under each foot: how far to move it, and the ground's normal. An
/// ankle with no ground near it, or a character in the air, stays put.
#[allow(clippy::too_many_arguments)]
fn goals(
    world: &World,
    assets: &Assets,
    probe: GroundProbe,
    owner: InstanceId,
    holder: InstanceId,
    bottom: f32,
    ankles: &[Vec3; 2],
    reach: f32,
) -> [Planted; 2] {
    let skip = parts_of(world, holder);
    let range = reach * REACH_SHARE;
    let look = |x: f32, z: f32| {
        let origin = Vector3::new(x, bottom + range, z);
        let hit = probe(
            world,
            assets,
            origin,
            Vector3::new(0.0, -2.0 * range, 0.0),
            &skip,
        )?;
        if hit.distance <= 0.0 || hit.normal.y < SHALLOWEST_NORMAL {
            return None;
        }
        Some(hit)
    };
    let hits = ankles.map(|ankle| look(ankle.x, ankle.z));
    let middle = world
        .get(owner)
        .and_then(|part| part.property("Position"))
        .and_then(Value::as_vector3)
        .and_then(|at| look(at.x, at.z));
    // Something within this of the character's bottom is what it stands on.
    let tolerance = (reach * 0.05).max(0.05);
    let standing = hits
        .iter()
        .chain([&middle])
        .flatten()
        .any(|hit| hit.position.y - bottom >= -tolerance);
    if !standing {
        return [Planted::default(); 2];
    }
    hits.map(|hit| {
        hit.map_or_else(Planted::default, |hit| Planted {
            lift: hit.position.y - bottom,
            normal: Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z),
        })
    })
}

/// Every part in the character.
fn parts_of(world: &World, holder: InstanceId) -> Vec<InstanceId> {
    std::iter::once(holder)
        .chain(world.descendants(holder))
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class().is_base_part())
        })
        .collect()
}

/// The height of the bottom of a part's box.
fn bottom_of(world: &World, part: InstanceId) -> Option<f32> {
    let instance = world.get(part)?;
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    Some(vector("Position")?.y - vector("Size")?.y.abs() * 0.5)
}
