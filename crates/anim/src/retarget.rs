//! Humanoid retargeting (roadmap G16): playing a take made for one
//! skeleton on another whose bones have other names, rest poses and
//! proportions.
//!
//! Each skeleton's bones are matched to humanoid roles by name
//! ([`HumanoidMap::detect`]: Mixamo's `LeftForeArm`, Unreal's `lowerarm_l`,
//! Blender's `forearm.L`, 3ds Max's `Bip01 L Forearm` all become
//! [`HumanBone::LeftLowerArm`]). A retargeted take moves each role's bone by
//! how much the take turns that role's bone away from its rest, measured in
//! world space and turned into the other skeleton's frame (which way is up
//! and which way it faces, from its rest pose), so bones whose local axes
//! differ still turn the same way. The hips also move, scaled by how much
//! longer or shorter the legs are. Bones without a role keep their place.
//!
//! Both skeletons should rest in a similar pose (both in a T-pose, or both
//! in an A-pose): the difference between rest poses plays as part of the
//! motion.

use axcenity_assets::{Bone, Pose, Rig, Take};
use glam::{Mat3, Mat4, Quat, Vec3};

/// A role a bone of a humanoid skeleton plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanBone {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    LeftUpperLeg,
    LeftLowerLeg,
    LeftFoot,
    LeftToes,
    RightUpperLeg,
    RightLowerLeg,
    RightFoot,
    RightToes,
}

impl HumanBone {
    pub const ALL: [HumanBone; 22] = [
        HumanBone::Hips,
        HumanBone::Spine,
        HumanBone::Chest,
        HumanBone::UpperChest,
        HumanBone::Neck,
        HumanBone::Head,
        HumanBone::LeftShoulder,
        HumanBone::LeftUpperArm,
        HumanBone::LeftLowerArm,
        HumanBone::LeftHand,
        HumanBone::RightShoulder,
        HumanBone::RightUpperArm,
        HumanBone::RightLowerArm,
        HumanBone::RightHand,
        HumanBone::LeftUpperLeg,
        HumanBone::LeftLowerLeg,
        HumanBone::LeftFoot,
        HumanBone::LeftToes,
        HumanBone::RightUpperLeg,
        HumanBone::RightLowerLeg,
        HumanBone::RightFoot,
        HumanBone::RightToes,
    ];

    /// The roles a skeleton needs to be retargeted to or from.
    pub const REQUIRED: [HumanBone; 15] = [
        HumanBone::Hips,
        HumanBone::Spine,
        HumanBone::Head,
        HumanBone::LeftUpperArm,
        HumanBone::LeftLowerArm,
        HumanBone::LeftHand,
        HumanBone::RightUpperArm,
        HumanBone::RightLowerArm,
        HumanBone::RightHand,
        HumanBone::LeftUpperLeg,
        HumanBone::LeftLowerLeg,
        HumanBone::LeftFoot,
        HumanBone::RightUpperLeg,
        HumanBone::RightLowerLeg,
        HumanBone::RightFoot,
    ];

    /// Its name, as messages show it.
    pub fn name(self) -> &'static str {
        match self {
            HumanBone::Hips => "Hips",
            HumanBone::Spine => "Spine",
            HumanBone::Chest => "Chest",
            HumanBone::UpperChest => "UpperChest",
            HumanBone::Neck => "Neck",
            HumanBone::Head => "Head",
            HumanBone::LeftShoulder => "LeftShoulder",
            HumanBone::LeftUpperArm => "LeftUpperArm",
            HumanBone::LeftLowerArm => "LeftLowerArm",
            HumanBone::LeftHand => "LeftHand",
            HumanBone::RightShoulder => "RightShoulder",
            HumanBone::RightUpperArm => "RightUpperArm",
            HumanBone::RightLowerArm => "RightLowerArm",
            HumanBone::RightHand => "RightHand",
            HumanBone::LeftUpperLeg => "LeftUpperLeg",
            HumanBone::LeftLowerLeg => "LeftLowerLeg",
            HumanBone::LeftFoot => "LeftFoot",
            HumanBone::LeftToes => "LeftToes",
            HumanBone::RightUpperLeg => "RightUpperLeg",
            HumanBone::RightLowerLeg => "RightLowerLeg",
            HumanBone::RightFoot => "RightFoot",
            HumanBone::RightToes => "RightToes",
        }
    }

    fn index(self) -> usize {
        HumanBone::ALL
            .iter()
            .position(|&role| role == self)
            .expect("every role is listed")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Middle,
}

/// What kind of bone a name is, before sides and the spine are sorted out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Hips,
    Spine,
    Neck,
    Head,
    Shoulder,
    UpperArm,
    LowerArm,
    Hand,
    UpperLeg,
    LowerLeg,
    Foot,
    Toes,
}

/// Which bone of a skeleton plays each [`HumanBone`] role.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HumanoidMap {
    bones: [Option<usize>; 22],
}

impl HumanoidMap {
    /// Matches `bones` (parents first) to roles by their names. The first
    /// bone of a kind and side wins; spine bones between the hips and the
    /// neck become Spine, then Chest, and the last of three or more
    /// UpperChest.
    pub fn detect(bones: &[Bone]) -> HumanoidMap {
        let mut map = HumanoidMap::default();
        let mut spine = Vec::new();
        for (index, bone) in bones.iter().enumerate() {
            let Some((kind, side)) = classify(&bone.name) else {
                continue;
            };
            let role = match (kind, side) {
                (Kind::Spine, _) => {
                    spine.push(index);
                    continue;
                }
                (Kind::Hips, _) => HumanBone::Hips,
                (Kind::Neck, _) => HumanBone::Neck,
                (Kind::Head, _) => HumanBone::Head,
                (_, Side::Middle) => continue,
                (Kind::Shoulder, Side::Left) => HumanBone::LeftShoulder,
                (Kind::Shoulder, _) => HumanBone::RightShoulder,
                (Kind::UpperArm, Side::Left) => HumanBone::LeftUpperArm,
                (Kind::UpperArm, _) => HumanBone::RightUpperArm,
                (Kind::LowerArm, Side::Left) => HumanBone::LeftLowerArm,
                (Kind::LowerArm, _) => HumanBone::RightLowerArm,
                (Kind::Hand, Side::Left) => HumanBone::LeftHand,
                (Kind::Hand, _) => HumanBone::RightHand,
                (Kind::UpperLeg, Side::Left) => HumanBone::LeftUpperLeg,
                (Kind::UpperLeg, _) => HumanBone::RightUpperLeg,
                (Kind::LowerLeg, Side::Left) => HumanBone::LeftLowerLeg,
                (Kind::LowerLeg, _) => HumanBone::RightLowerLeg,
                (Kind::Foot, Side::Left) => HumanBone::LeftFoot,
                (Kind::Foot, _) => HumanBone::RightFoot,
                (Kind::Toes, Side::Left) => HumanBone::LeftToes,
                (Kind::Toes, _) => HumanBone::RightToes,
            };
            map.bones[role.index()].get_or_insert(index);
        }
        let roles: &[HumanBone] = match spine.len() {
            0 => &[],
            1 => &[HumanBone::Spine],
            2 => &[HumanBone::Spine, HumanBone::Chest],
            _ => &[HumanBone::Spine, HumanBone::Chest, HumanBone::UpperChest],
        };
        // Spine, the next as Chest, the last as UpperChest.
        let picks = match spine.len() {
            0..=2 => spine.clone(),
            _ => vec![spine[0], spine[1], spine[spine.len() - 1]],
        };
        for (&role, bone) in roles.iter().zip(picks) {
            map.bones[role.index()] = Some(bone);
        }
        map
    }

    /// The bone playing `role`, if any.
    pub fn get(&self, role: HumanBone) -> Option<usize> {
        self.bones[role.index()]
    }

    /// Every bone that plays a role.
    pub fn bones(&self) -> impl Iterator<Item = usize> + '_ {
        self.bones.iter().flatten().copied()
    }

    /// The [`HumanBone::REQUIRED`] roles no bone plays.
    pub fn missing(&self) -> Vec<HumanBone> {
        HumanBone::REQUIRED
            .into_iter()
            .filter(|&role| self.get(role).is_none())
            .collect()
    }
}

/// A bone name's kind and side, if it's a humanoid bone's name. Namespaces
/// (`mixamorig:`), common prefixes (`DEF-`, `Bip01`) and side markers
/// (`Left`, `_l`, `.L`, ` L `) are taken off first; finger bones, end bones
/// and twist bones aren't humanoid bones.
fn classify(name: &str) -> Option<(Kind, Side)> {
    let lower = name.to_ascii_lowercase();
    let lower = lower.rsplit([':', '|']).next().unwrap_or(&lower);
    let mut side = Side::Middle;
    let mut words: Vec<String> = Vec::new();
    for word in lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        match word {
            "l" | "left" => side = Side::Left,
            "r" | "right" => side = Side::Right,
            "def" | "org" | "mch" | "bip01" | "bip001" | "mixamorig" | "armature" | "rig"
            | "cc" | "base" | "jnt" | "bn" | "b" => {}
            _ => words.push(word.to_owned()),
        }
    }
    let mut key = words.concat();
    for (prefix, found) in [("left", Side::Left), ("right", Side::Right)] {
        if let Some(rest) = key.strip_prefix(prefix) {
            side = found;
            key = rest.to_owned();
        }
    }
    // Spine1, spine_01, Spine.003, Toe0: the number only orders them.
    let base = key.trim_end_matches(|c: char| c.is_ascii_digit());
    let kind = match base {
        "hips" | "hip" | "pelvis" => Kind::Hips,
        "spine" | "chest" | "upperchest" | "torso" | "abdomen" | "abdomenupper" | "waist" => {
            Kind::Spine
        }
        "neck" => Kind::Neck,
        "head" => Kind::Head,
        "shoulder" | "clavicle" | "collar" | "collarbone" => Kind::Shoulder,
        "upperarm" | "arm" | "uparm" => Kind::UpperArm,
        "forearm" | "lowerarm" | "elbow" => Kind::LowerArm,
        "hand" | "wrist" => Kind::Hand,
        "upleg" | "thigh" | "upperleg" => Kind::UpperLeg,
        "leg" | "calf" | "shin" | "lowerleg" | "knee" => Kind::LowerLeg,
        "foot" | "ankle" => Kind::Foot,
        "toe" | "toes" | "toebase" | "ball" => Kind::Toes,
        _ => return None,
    };
    Some((kind, side))
}

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
    pub(crate) fn new(take: &Take, rig: &Rig) -> Result<Retarget, String> {
        if take.skeleton.is_empty() {
            return Err("its file doesn't say what skeleton it was made for".to_owned());
        }
        let source = Rig {
            bones: take.skeleton.to_vec(),
            joints: Vec::new(),
            fit: Mat4::IDENTITY,
        };
        let from = HumanoidMap::detect(&source.bones);
        let to = HumanoidMap::detect(&rig.bones);
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
        let pairs: Vec<(usize, usize)> = HumanBone::ALL
            .into_iter()
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
