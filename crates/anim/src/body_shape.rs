//! Body sliders (default humanoid HD4): a skinned MeshPart's attributes
//! reshape its humanoid skeleton's pose before it's drawn. Each is a factor
//! (1 is the body as made, kept within 0.5–1.5):
//!
//! | Attribute | What it does |
//! |---|---|
//! | `BodyHeight` | Lengthens the spine and the legs |
//! | `LegLength`, `ArmLength` | Lengthens the thighs and shins, the upper arms and forearms |
//! | `ShoulderWidth` | Lengthens the shoulders (collarbones) and widens the upper chest |
//! | `WaistWidth`, `HipWidth` | Widens the spine and the hips, side to side |
//! | `HeadSize`, `HandSize`, `FootSize` | Scales the head, hands and feet and everything on them |
//!
//! Lengths scale a bone's own vertices along it (the bone's Y, which glTF
//! and FBX humanoids point along the bone) and move its children out to
//! match; widths scale along the bone's X (side to side on a spine); sizes
//! scale a bone and all below it. A bone's scale isn't passed on to its
//! children ([`axcenity_assets::Model::pose_shaped`]), so nothing shears.
//! Longer legs raise the hips by as much, so the feet stay on the ground.
//! Bones are found by humanoid role ([`HumanoidMap`]); a skeleton without
//! a role ignores that slider.

use axcenity_assets::{Pose, Rig};
use axcenity_core::{InstanceId, Value, World};
use glam::Vec3;

use crate::retarget::{HumanBone, HumanoidMap};

/// The sliders, by attribute name.
pub const BODY_SLIDERS: [&str; 9] = [
    "BodyHeight",
    "LegLength",
    "ArmLength",
    "ShoulderWidth",
    "WaistWidth",
    "HipWidth",
    "HeadSize",
    "HandSize",
    "FootSize",
];

/// The smallest and largest a slider goes.
const RANGE: (f32, f32) = (0.5, 1.5);

/// A body's slider values, in [`BODY_SLIDERS`] order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyShape([f32; 9]);

impl BodyShape {
    /// The sliders on the MeshPart `part`, or `None` when none is set away
    /// from 1 (the common case, which costs nothing).
    pub fn read(world: &World, part: InstanceId) -> Option<Self> {
        let instance = world.get(part)?;
        let mut values = [1.0; 9];
        let mut any = false;
        for (value, name) in values.iter_mut().zip(BODY_SLIDERS) {
            let Some(number) = instance.attribute(name).and_then(Value::as_number) else {
                continue;
            };
            let number = number as f32;
            if number.is_finite() && (number - 1.0).abs() > 1e-4 {
                *value = number.clamp(RANGE.0, RANGE.1);
                any = true;
            }
        }
        any.then_some(Self(values))
    }

    fn get(&self, name: &str) -> f32 {
        BODY_SLIDERS
            .iter()
            .position(|slider| *slider == name)
            .map_or(1.0, |index| self.0[index])
    }

    /// Reshapes `pose` (of `rig`) and returns each bone's own scale for
    /// [`axcenity_assets::Model::pose_shaped`].
    pub fn apply(&self, rig: &Rig, pose: &mut Pose) -> Vec<Vec3> {
        let map = HumanoidMap::detect(&rig.bones);
        let mut shape = vec![Vec3::ONE; rig.bones.len()];
        if pose.len() < rig.bones.len() {
            pose.extend(rig.bones[pose.len()..].iter().map(|bone| bone.rest));
        }
        let children = |bone: usize| -> Vec<usize> {
            rig.bones
                .iter()
                .enumerate()
                .filter(|(_, child)| child.parent == Some(bone))
                .map(|(index, _)| index)
                .collect()
        };
        let mut stretch = |pose: &mut Pose, role: HumanBone, factor: f32| {
            let Some(bone) = map.get(role) else { return };
            shape[bone].y *= factor;
            for child in children(bone) {
                pose[child].translation.y *= factor;
            }
        };
        let height = self.get("BodyHeight");
        let legs = self.get("LegLength") * height;
        let arms = self.get("ArmLength");
        let shoulders = self.get("ShoulderWidth");
        for role in [HumanBone::Spine, HumanBone::Chest, HumanBone::UpperChest] {
            stretch(pose, role, height);
        }
        for role in [
            HumanBone::LeftUpperLeg,
            HumanBone::LeftLowerLeg,
            HumanBone::RightUpperLeg,
            HumanBone::RightLowerLeg,
        ] {
            stretch(pose, role, legs);
        }
        for role in [
            HumanBone::LeftUpperArm,
            HumanBone::LeftLowerArm,
            HumanBone::RightUpperArm,
            HumanBone::RightLowerArm,
        ] {
            stretch(pose, role, arms);
        }
        for role in [HumanBone::LeftShoulder, HumanBone::RightShoulder] {
            stretch(pose, role, shoulders);
        }
        let mut widen = |pose: &mut Pose, role: HumanBone, factor: f32| {
            let Some(bone) = map.get(role) else { return };
            shape[bone].x *= factor;
            for child in children(bone) {
                pose[child].translation.x *= factor;
            }
        };
        widen(pose, HumanBone::UpperChest, shoulders.sqrt());
        widen(pose, HumanBone::Spine, self.get("WaistWidth"));
        widen(pose, HumanBone::Hips, self.get("HipWidth"));
        let mut grow = |pose: &mut Pose, role: HumanBone, factor: f32| {
            let Some(top) = map.get(role) else { return };
            if factor == 1.0 {
                return;
            }
            shape[top] *= factor;
            // Bones are listed parents first, so one pass finds every bone
            // below `top`.
            let mut below = vec![false; rig.bones.len()];
            below[top] = true;
            for (index, bone) in rig.bones.iter().enumerate().skip(top + 1) {
                if bone.parent.is_some_and(|parent| below[parent]) {
                    below[index] = true;
                    shape[index] *= factor;
                    pose[index].translation *= factor;
                }
            }
        };
        grow(pose, HumanBone::Head, self.get("HeadSize"));
        for role in [HumanBone::LeftHand, HumanBone::RightHand] {
            grow(pose, role, self.get("HandSize"));
        }
        for role in [HumanBone::LeftFoot, HumanBone::RightFoot] {
            grow(pose, role, self.get("FootSize"));
        }
        if legs != 1.0 {
            raise_hips(rig, &map, pose, legs);
        }
        shape
    }
}

/// Moves the hips up by how much longer `legs` makes the leg (thigh and
/// shin, measured at rest), straight up in the model, so the feet stay on
/// the ground.
fn raise_hips(rig: &Rig, map: &HumanoidMap, pose: &mut Pose, legs: f32) {
    let (Some(hips), Some(thigh), Some(shin), Some(foot)) = (
        map.get(HumanBone::Hips),
        map.get(HumanBone::LeftUpperLeg),
        map.get(HumanBone::LeftLowerLeg),
        map.get(HumanBone::LeftFoot),
    ) else {
        return;
    };
    let rest = rig.world(&rig.rest_pose());
    let at = |bone: usize| rest[bone].transform_point3(Vec3::ZERO);
    let length = at(thigh).distance(at(shin)) + at(shin).distance(at(foot));
    let rise = length * (legs - 1.0);
    if !rise.is_finite() {
        return;
    }
    // Up in the model, in the frame the hips' translation is in.
    let parent = rig.bones[hips]
        .parent
        .map_or(glam::Mat4::IDENTITY, |parent| rig.world(pose)[parent]);
    let up = parent.inverse().transform_vector3(Vec3::Y * rise);
    if up.is_finite() {
        pose[hips].translation += up;
    }
}
