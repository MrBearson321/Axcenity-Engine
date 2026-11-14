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
mod roles;

pub use check::{RigProblem, RigReport, Severity, check_pair, check_rig, rest_arm_angle};
pub use roles::{Digit, HumanBone, HumanoidMap, ROLE_COUNT, Segment, Side};

use axcenity_assets::{Pose, Rig, RigDefinitions, Take};
use glam::{Mat3, Mat4, Quat, Vec3};

/// A take bound to another skeleton through humanoid roles.
#[derive(Clone, Debug)]
pub(crate) struct Retarget {
    /// The take's own skeleton, from its file.
    source: Rig,
    /// Each take track's bone in `source`.
    source_binding: Vec<Option<usize>>,
    /// Each role both skeletons have and the take moves: its bone in
    /// `source`, then in the target rig. Roles the take leaves alone are
    /// left to other layers.
    pairs: Vec<(usize, usize)>,
    /// The hips, in `source` and in the target rig.
    hips: (usize, usize),
    /// Whether the take moves the hips, so they travel with it.
    moves_hips: bool,
    source_rest: Vec<Mat4>,
    target_rest: Vec<Mat4>,
    /// From the source's world space into the target's: which way is up
    /// and which way the body faces, from each rest pose.
    turn: Quat,
    /// How much longer the target's legs are than the source's, for the
    /// hips' travel.
    scale: f32,
}

impl Retarget {
    /// Binds `take`, through its skeleton, to `rig`. Fails, saying why, if
    /// either skeleton isn't a humanoid one by its bone names.
    pub(crate) fn new(take: &Take, rig: &Rig, rigs: &RigDefinitions) -> Result<Retarget, String> {
        if take.skeleton.is_empty() {
            return Err("its file doesn't say what skeleton it was made for".to_owned());
        }
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
        let pairs: Vec<(usize, usize)> = HumanBone::all()
            .filter_map(|role| Some((from.get(role)?, to.get(role)?)))
            .filter(|(source, _)| moved.contains(source))
            .collect();
        let hips = (
            from.get(HumanBone::Hips).expect("required"),
            to.get(HumanBone::Hips).expect("required"),
        );
        let moves_hips = moved.contains(&hips.0);
        let turn = match (
            body_frame(&source_rest, &from),
            body_frame(&target_rest, &to),
        ) {
            (Some(source), Some(target)) => {
                Quat::from_mat3(&(target * source.transpose())).normalize()
            }
            _ => Quat::IDENTITY,
        };
        let scale = match (
            leg_length(&source_rest, &from),
            leg_length(&target_rest, &to),
        ) {
            (Some(source), Some(target)) if source > 1e-6 => target / source,
            _ => 1.0,
        };
        Ok(Retarget {
            source_binding,
            source,
            pairs,
            hips,
            moves_hips,
            source_rest,
            target_rest,
            turn,
            scale,
        })
    }

    /// The target rig's bones a retargeted take moves.
    pub(crate) fn targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.pairs.iter().map(|&(_, target)| target)
    }

    /// The target rig's hips, which root motion follows.
    pub(crate) fn hips(&self) -> usize {
        self.hips.1
    }

    /// Poses `rig`'s role bones in `pose` as `take` has its own at `time`
    /// seconds. Other bones keep what `pose` has for them.
    pub(crate) fn sample(&self, take: &Take, time: f32, rig: &Rig, pose: &mut Pose) {
        let mut source_pose = self.source.rest_pose();
        take.sample(time, &self.source_binding, &self.source, &mut source_pose);
        let source_world = self.source.world(&source_pose);
        // Where each target bone should turn to, in the target's world.
        let mut wanted: Vec<Option<Quat>> = vec![None; rig.bones.len()];
        for &(source, target) in &self.pairs {
            let moved =
                rotation(&source_world[source]) * rotation(&self.source_rest[source]).inverse();
            let moved = self.turn * moved * self.turn.inverse();
            if let Some(slot) = wanted.get_mut(target) {
                *slot = Some((moved * rotation(&self.target_rest[target])).normalize());
            }
        }
        let (source_hips, target_hips) = self.hips;
        let travel =
            position(&source_world[source_hips]) - position(&self.source_rest[source_hips]);
        let hips_at = position(&self.target_rest[target_hips]) + self.turn * travel * self.scale;
        // Parents first, so each bone's parent is where it now is.
        let mut world: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
        for (index, bone) in rig.bones.iter().enumerate() {
            let parent = bone
                .parent
                .and_then(|parent| world.get(parent).copied())
                .unwrap_or(Mat4::IDENTITY);
            let mut local = pose.get(index).copied().unwrap_or(bone.rest);
            if let Some(turned) = wanted[index] {
                local.rotation = (rotation(&parent).inverse() * turned).normalize();
            }
            if index == target_hips && self.moves_hips {
                local.translation = parent.inverse().transform_point3(hips_at);
            }
            if let Some(slot) = pose.get_mut(index) {
                *slot = local;
            }
            world.push(parent * local.matrix());
        }
    }
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
