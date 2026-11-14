//! Humanoid retargeting (roadmap G16): playing a take made for one
//! skeleton on another whose bones have other names, rest poses and
//! proportions.
//!
//! Each skeleton's bones are matched to humanoid roles ([`roles`]): by a
//! rig definition of the project's that fits it (`assets/rigs/*.rig.json`,
//! its own bone map), else by name (Mixamo's `LeftForeArm`, Unreal's
//! `lowerarm_l`, Blender's `forearm.L`, 3ds Max's `Bip01 L Forearm` all
//! become [`HumanBone::LeftLowerArm`]; fingers too, `LeftHandIndex1`,
//! `index_01_l`, `f_index.01.L`). A retargeted take moves each role's bone by
//! how much the take turns that role's bone away from its rest, measured in
//! world space and turned into the other skeleton's frame (which way is up
//! and which way it faces, from its rest pose), so bones whose local axes
//! differ still turn the same way. The hips also move, scaled by how much
//! longer or shorter the legs are. Bones without a role keep their place.
//!
//! Both skeletons should rest in a similar pose (both in a T-pose, or both
//! in an A-pose): the difference between rest poses plays as part of the
//! motion. [`check_rig`] says when they don't.

mod check;
mod profile;
mod roles;

pub use check::{RigProblem, RigReport, Severity, check_pair, check_rig, rest_arm_angle};
pub use profile::{
    HipsTravel, RETARGET_EXTENSION, RetargetProfile, RoleSetting, is_retarget_profile,
};
pub use roles::{Digit, HumanBone, HumanoidMap, ROLE_COUNT, Segment, Side};

use axcenity_assets::{Pose, Rig, RigDefinitions, Take};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};

use crate::ik;

/// One role both skeletons have and the take moves.
#[derive(Clone, Copy, Debug)]
struct Pair {
    /// Its bone in the take's skeleton, then in the target rig.
    source: usize,
    target: usize,
    /// Turns the target bone, in the target's world, from how it rests to
    /// how the take's bone rests (identity unless rest poses are matched).
    correction: Quat,
    /// How much of the take's turn it gets, and its extra turn about its
    /// own axes (from the profile).
    weight: f32,
    offset: Quat,
}

/// A limb IK puts at a point: its end bone and the two above it.
#[derive(Clone, Copy, Debug)]
struct Limb {
    /// The end (foot or hand) in the take's skeleton and the target rig.
    source_end: usize,
    target_end: usize,
    /// The target's upper and lower bones, nearest the end first.
    chain: [usize; 2],
}

/// A take bound to another skeleton through humanoid roles.
#[derive(Clone, Debug)]
pub(crate) struct Retarget {
    /// The take's own skeleton, from its file.
    source: Rig,
    /// Each take track's bone in `source`.
    source_binding: Vec<Option<usize>>,
    /// Each role both skeletons have and the take moves (roles the take
    /// leaves alone are left to other layers), parents first.
    pairs: Vec<Pair>,
    /// The hips, in `source` and in the target rig.
    hips: (usize, usize),
    /// Whether the take moves the hips, so they travel with it.
    moves_hips: bool,
    hips_travel: HipsTravel,
    source_rest: Vec<Mat4>,
    target_rest: Vec<Mat4>,
    /// From the source's world space into the target's: which way is up
    /// and which way the body faces, from each rest pose.
    turn: Quat,
    /// How much longer the target's legs are than the source's, for the
    /// hips' travel.
    scale: f32,
    /// Which way is up in the target's world, from its rest pose.
    up: Vec3,
    /// Feet to plant and hands to place with IK, from the profile.
    feet: Vec<Limb>,
    hands: Vec<Limb>,
}

impl Retarget {
    /// Binds `take`, through its skeleton, to `rig`, as `profile` says (the
    /// defaults without one). Fails, saying why, if either skeleton isn't a
    /// humanoid one by its bone names or the project's rig definitions.
    pub(crate) fn new(
        take: &Take,
        rig: &Rig,
        rigs: &RigDefinitions,
        profile: Option<&RetargetProfile>,
    ) -> Result<Retarget, String> {
        if take.skeleton.is_empty() {
            return Err("its file doesn't say what skeleton it was made for".to_owned());
        }
        let defaults = RetargetProfile::default();
        let profile = profile.unwrap_or(&defaults);
        let source = Rig {
            bones: take.skeleton.to_vec(),
            joints: Vec::new(),
            fit: Mat4::IDENTITY,
        };
        let (from, _) = HumanoidMap::for_skeleton(&source.bones, rigs);
        let (to, _) = HumanoidMap::for_skeleton(&rig.bones, rigs);
        let names = |missing: Vec<HumanBone>| {
            missing
                .into_iter()
                .map(HumanBone::name)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (take_missing, model_missing) = (from.missing(), to.missing());
        if !model_missing.is_empty() {
            return Err(format!(
                "the model's skeleton isn't a humanoid one by its bone names (no {})",
                names(model_missing)
            ));
        }
        if !take_missing.is_empty() {
            return Err(format!(
                "the take's skeleton isn't a humanoid one by its bone names (no {})",
                names(take_missing)
            ));
        }
        let source_rest = source.world(&source.rest_pose());
        let target_rest = rig.world(&rig.rest_pose());
        let source_binding = take.bind(&source);
        let moved: Vec<usize> = source_binding.iter().flatten().copied().collect();
        let hips = (
            from.get(HumanBone::Hips).expect("required"),
            to.get(HumanBone::Hips).expect("required"),
        );
        let moves_hips = moved.contains(&hips.0);
        let target_frame = body_frame(&target_rest, &to);
        let turn = match (body_frame(&source_rest, &from), target_frame) {
            (Some(source), Some(target)) => {
                Quat::from_mat3(&(target * source.transpose())).normalize()
            }
            _ => Quat::IDENTITY,
        };
        let up = target_frame.map_or(Vec3::Y, |frame| frame.y_axis);
        let scale = match (
            leg_length(&source_rest, &from),
            leg_length(&target_rest, &to),
        ) {
            (Some(source), Some(target)) if source > 1e-6 => target / source,
            _ => 1.0,
        };

        // Each role's rest correction, parents' roles first so a role with
        // nothing to point at can take its parent's.
        let mut corrections: Vec<Option<Quat>> = vec![None; ROLE_COUNT];
        for role in HumanBone::all() {
            let (Some(source_bone), Some(target_bone)) = (from.get(role), to.get(role)) else {
                continue;
            };
            let measured = profile.match_rest_pose.then(|| {
                role.toward().into_iter().find_map(|next| {
                    let (s, t) = (from.get(next)?, to.get(next)?);
                    let along_source = self_turned(turn, &source_rest, source_bone, s)?;
                    let along_target = direction(&target_rest, target_bone, t)?;
                    Some(Quat::from_rotation_arc(along_target, along_source))
                })
            });
            let inherited = || {
                let mut above = role.above();
                while let Some(parent) = above {
                    if let Some(correction) = corrections[parent.index()] {
                        return correction;
                    }
                    above = parent.above();
                }
                Quat::IDENTITY
            };
            // The hips set the body's frame, which `turn` already lines up.
            let correction = match (role, measured) {
                (HumanBone::Hips, _) | (_, None) => Quat::IDENTITY,
                (_, Some(Some(measured))) => measured,
                (_, Some(None)) => inherited(),
            };
            corrections[role.index()] = Some(correction);
        }

        let mut pairs: Vec<Pair> = HumanBone::all()
            .filter_map(|role| {
                let (source, target) = (from.get(role)?, to.get(role)?);
                if !moved.contains(&source) {
                    return None;
                }
                let setting = profile.role(role);
                let [x, y, z] = setting.offset.map(f32::to_radians);
                Some(Pair {
                    source,
                    target,
                    correction: corrections[role.index()].unwrap_or(Quat::IDENTITY),
                    weight: setting.weight,
                    offset: Quat::from_euler(EulerRot::XYZ, x, y, z),
                })
            })
            .collect();
        pairs.sort_by_key(|pair| pair.target);

        let limb = |end: HumanBone| {
            let lower = end.above()?;
            let upper = lower.above()?;
            Some(Limb {
                source_end: from.get(end)?,
                target_end: to.get(end)?,
                chain: [to.get(lower)?, to.get(upper)?],
            })
        };
        let feet = if profile.plant_feet {
            [HumanBone::LeftFoot, HumanBone::RightFoot]
                .into_iter()
                .filter_map(limb)
                .collect()
        } else {
            Vec::new()
        };
        let hands = if profile.match_hands {
            [HumanBone::LeftHand, HumanBone::RightHand]
                .into_iter()
                .filter_map(limb)
                .collect()
        } else {
            Vec::new()
        };
        Ok(Retarget {
            source_binding,
            source,
            pairs,
            hips,
            moves_hips,
            hips_travel: profile.hips,
            source_rest,
            target_rest,
            turn,
            scale,
            up,
            feet,
            hands,
        })
    }

    /// The target rig's bones a retargeted take moves.
    pub(crate) fn targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.pairs
            .iter()
            .filter(|pair| pair.weight > 0.0)
            .map(|pair| pair.target)
    }

    /// The target rig's hips, which root motion follows.
    pub(crate) fn hips(&self) -> usize {
        self.hips.1
    }

    /// Where the take's hips put the target's, in the target's world.
    fn hips_at(&self, source_world: &[Mat4]) -> Vec3 {
        let (source_hips, target_hips) = self.hips;
        let travel =
            position(&source_world[source_hips]) - position(&self.source_rest[source_hips]);
        let travel = self.turn * travel;
        let travel = match self.hips_travel {
            HipsTravel::Scaled => travel * self.scale,
            HipsTravel::Unscaled => travel,
            // Only the part along the target's up: bobbing, not travel.
            HipsTravel::InPlace => self.up * travel.dot(self.up) * self.scale,
        };
        position(&self.target_rest[target_hips]) + travel
    }

    /// Poses `rig`'s role bones in `pose` as `take` has its own at `time`
    /// seconds. Other bones keep what `pose` has for them.
    pub(crate) fn sample(&self, take: &Take, time: f32, rig: &Rig, pose: &mut Pose) {
        let mut source_pose = self.source.rest_pose();
        take.sample(time, &self.source_binding, &self.source, &mut source_pose);
        let source_world = self.source.world(&source_pose);
        // Where each target bone should turn to, in the target's world.
        let mut wanted: Vec<Option<&Pair>> = vec![None; rig.bones.len()];
        let mut turned: Vec<Quat> = vec![Quat::IDENTITY; rig.bones.len()];
        for pair in &self.pairs {
            let moved = rotation(&source_world[pair.source])
                * rotation(&self.source_rest[pair.source]).inverse();
            let moved = self.turn * moved * self.turn.inverse();
            if pair.target < rig.bones.len() {
                wanted[pair.target] = Some(pair);
                turned[pair.target] =
                    (moved * pair.correction * rotation(&self.target_rest[pair.target]))
                        .normalize();
            }
        }
        let target_hips = self.hips.1;
        let hips_at = self.hips_at(&source_world);
        // Parents first, so each bone's parent is where it now is.
        let mut world: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
        for (index, bone) in rig.bones.iter().enumerate() {
            let parent = bone
                .parent
                .and_then(|parent| world.get(parent).copied())
                .unwrap_or(Mat4::IDENTITY);
            let mut local = pose.get(index).copied().unwrap_or(bone.rest);
            if let Some(pair) = wanted[index].filter(|pair| pair.weight > 0.0) {
                let full = (rotation(&parent).inverse() * turned[index] * pair.offset).normalize();
                local.rotation = if pair.weight >= 1.0 {
                    full
                } else {
                    local.rotation.slerp(full, pair.weight).normalize()
                };
            }
            if index == target_hips && self.moves_hips {
                local.translation = parent.inverse().transform_point3(hips_at);
            }
            if let Some(slot) = pose.get_mut(index) {
                *slot = local;
            }
            world.push(parent * local.matrix());
        }
        if self.feet.is_empty() && self.hands.is_empty() {
            return;
        }
        // Feet go where the take's are, moved as the hips' rest is; hands
        // where the take's are from its hips, scaled like the body.
        let source_hips = position(&source_world[self.hips.0]);
        let mut goals: Vec<(Limb, Vec3)> = Vec::new();
        for limb in &self.feet {
            let travel = position(&source_world[limb.source_end])
                - position(&self.source_rest[limb.source_end]);
            let goal = position(&self.target_rest[limb.target_end])
                + self.turn * travel * self.travel_scale();
            goals.push((*limb, goal));
        }
        for limb in &self.hands {
            let from_hips = position(&source_world[limb.source_end]) - source_hips;
            goals.push((*limb, hips_at + self.turn * from_hips * self.scale));
        }
        for (limb, goal) in goals {
            // The end keeps the turn the take gave it, in the world.
            let end_turn = rotation(&rig.world(pose)[limb.target_end]);
            ik::reach(rig, pose, &limb.chain, limb.target_end, goal);
            let world = rig.world(pose);
            let parent = rig.bones[limb.target_end]
                .parent
                .map_or(Mat4::IDENTITY, |parent| world[parent]);
            if let Some(slot) = pose.get_mut(limb.target_end) {
                slot.rotation = (rotation(&parent).inverse() * end_turn).normalize();
            }
        }
    }

    /// How far the feet travel per unit of the take's, as the hips do.
    fn travel_scale(&self) -> f32 {
        match self.hips_travel {
            HipsTravel::Unscaled => 1.0,
            HipsTravel::Scaled | HipsTravel::InPlace => self.scale,
        }
    }
}

/// From `bone` to `next` in a resting skeleton, as a unit vector.
fn direction(world: &[Mat4], bone: usize, next: usize) -> Option<Vec3> {
    let along = position(&world[next]) - position(&world[bone]);
    (along.length_squared() > 1e-12).then(|| along.normalize())
}

/// [`direction`] in the source's skeleton, turned into the target's world.
fn self_turned(turn: Quat, world: &[Mat4], bone: usize, next: usize) -> Option<Vec3> {
    direction(world, bone, next).map(|along| turn * along)
}

fn rotation(matrix: &Mat4) -> Quat {
    matrix.to_scale_rotation_translation().1.normalize()
}

fn position(matrix: &Mat4) -> Vec3 {
    matrix.w_axis.truncate()
}

/// Which way a resting skeleton's body faces: its left (from the right
/// arm or leg to the left), up (hips to head) and forward, as the columns
/// of a rotation.
fn body_frame(world: &[Mat4], map: &HumanoidMap) -> Option<Mat3> {
    let at = |role| map.get(role).map(|bone| position(&world[bone]));
    let up = (at(HumanBone::Head)? - at(HumanBone::Hips)?).normalize_or_zero();
    let across = at(HumanBone::LeftUpperArm)? - at(HumanBone::RightUpperArm)?;
    let across = if across.length_squared() > 1e-8 {
        across
    } else {
        at(HumanBone::LeftUpperLeg)? - at(HumanBone::RightUpperLeg)?
    };
    let forward = across.cross(up).normalize_or_zero();
    let left = up.cross(forward).normalize_or_zero();
    (up != Vec3::ZERO && forward != Vec3::ZERO).then(|| Mat3::from_cols(left, up, forward))
}

/// From the hips down to the left foot, resting.
fn leg_length(world: &[Mat4], map: &HumanoidMap) -> Option<f32> {
    let at = |role| map.get(role).map(|bone| position(&world[bone]));
    Some(at(HumanBone::Hips)?.distance(at(HumanBone::LeftFoot)?))
}

#[cfg(test)]
#[path = "retarget_tests.rs"]
mod tests;
