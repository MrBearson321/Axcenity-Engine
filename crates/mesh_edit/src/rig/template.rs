//! The Humanoid template rig: Mixamo's skeleton by its bone names
//! (`mixamorig:Hips` and so on), so Mixamo clips play on it and Phase 41's
//! ragdoll preset finds its bones.
//!
//! The rest offsets and turns are a Mixamo character's own (centimetres,
//! as its FBX files key them), without the fingers and eyes. A take from
//! Mixamo keys every bone's offset as well as its turn, so they have to
//! be Mixamo's for a clip to play without retargeting; the template is
//! fitted to a model by the root bone's place and scale alone.

use glam::{Quat, Vec3};

use super::{ArmBone, Armature};
use crate::half_edge::EditMesh;

/// One template bone: its name (after `mixamorig:`), its parent's, where
/// it rests in its parent (centimetres) and how it's turned there (x, y,
/// z, w), its length, and whether it deforms.
type TemplateBone = (&'static str, &'static str, [f32; 3], [f32; 4], f32, bool);

const I: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The template's bones, parents first.
const BONES: &[TemplateBone] = &[
    ("Hips", "", [-0.0419, 209.1508, -1.4164], I, 19.8048, true),
    (
        "Spine",
        "Hips",
        [-0.1948, 19.8048, -3.8898],
        [-0.050025415, -0.00028784093, 0.005746594, 0.9987314],
        23.62007,
        true,
    ),
    (
        "Spine1",
        "Spine",
        [0.0, 23.62007, 0.0],
        [0.028562967, 0.0002709803, -0.0037820297, 0.9995848],
        27.8335,
        true,
    ),
    (
        "Spine2",
        "Spine1",
        [0.0, 27.8335, 0.0],
        [0.08163175, -0.00043349987, -0.011176394, 0.9965998],
        31.294106,
        true,
    ),
    (
        "Neck",
        "Spine2",
        [0.0, 31.294106, 0.0],
        [0.08201045, -0.0028557447, -0.013901642, 0.9965304],
        23.18896,
        true,
    ),
    ("Head", "Neck", [0.0, 23.18896, 0.0], I, 39.77745, true),
    (
        "HeadTop_End",
        "Head",
        [-1.6398, 39.77745, -0.3611],
        I,
        10.0,
        false,
    ),
    (
        "LeftShoulder",
        "Spine2",
        [12.603374, 19.313717, 0.3591669],
        [0.4180132, 0.5150067, -0.5576158, 0.49909687],
        26.58329,
        true,
    ),
    (
        "LeftArm",
        "LeftShoulder",
        [0.0, 26.58329, 0.0],
        [0.014690257, 0.0011267901, 0.07647028, 0.996963],
        55.127666,
        true,
    ),
    (
        "LeftForeArm",
        "LeftArm",
        [0.0, 55.127666, 0.0],
        I,
        52.773346,
        true,
    ),
    (
        "LeftHand",
        "LeftForeArm",
        [0.0, 52.773346, 0.0],
        I,
        27.74456,
        true,
    ),
    (
        "RightShoulder",
        "Spine2",
        [-12.989572, 19.103437, -0.11624028],
        [0.41931355, -0.51388144, 0.58030665, 0.4725952],
        26.891928,
        true,
    ),
    (
        "RightArm",
        "RightShoulder",
        [0.0, 26.891928, 0.0],
        [0.0087051345, -0.00088684453, -0.10134755, 0.9948126],
        55.842678,
        true,
    ),
    (
        "RightForeArm",
        "RightArm",
        [0.0, 55.842678, 0.0],
        I,
        50.313686,
        true,
    ),
    (
        "RightHand",
        "RightForeArm",
        [0.0, 50.313686, 0.0],
        I,
        26.065542,
        true,
    ),
    (
        "LeftUpLeg",
        "Hips",
        [18.524397, -13.838534, 2.6620154],
        [2.112637e-5, -0.008861608, 0.9999579, 0.0023839332],
        85.24563,
        true,
    ),
    (
        "LeftLeg",
        "LeftUpLeg",
        [0.0, 85.24563, 0.0],
        [-0.021215294, 5.3014755e-6, 0.005016797, 0.99976236],
        85.571144,
        true,
    ),
    (
        "LeftFoot",
        "LeftLeg",
        [0.0, 85.571144, 0.0],
        [0.43852195, -0.01936389, 0.03490798, 0.89783347],
        32.481228,
        true,
    ),
    (
        "LeftToeBase",
        "LeftFoot",
        [0.0, 32.481228, 0.0],
        [0.35920027, -0.011804608, -0.046767928, 0.9320132],
        19.064793,
        true,
    ),
    (
        "LeftToe_End",
        "LeftToeBase",
        [0.0, 19.064793, 0.0],
        I,
        8.0,
        false,
    ),
    (
        "RightUpLeg",
        "Hips",
        [-18.440273, -14.411688, 0.16034028],
        [-1.9138073e-5, -0.0059108487, -0.9999773, 0.003237714],
        84.81971,
        true,
    ),
    (
        "RightLeg",
        "RightUpLeg",
        [0.0, 84.81971, 0.0],
        [-0.03611204, -8.633645e-6, -0.006710202, 0.9993252],
        85.4194,
        true,
    ),
    (
        "RightFoot",
        "RightLeg",
        [0.0, 85.4194, 0.0],
        [0.44226038, 0.022734664, -0.03980189, 0.89571464],
        32.98118,
        true,
    ),
    (
        "RightToeBase",
        "RightFoot",
        [0.0, 32.98118, 0.0],
        [0.35226452, 0.04155386, -0.020242894, 0.93475837],
        19.702324,
        true,
    ),
    (
        "RightToe_End",
        "RightToeBase",
        [0.0, 19.702324, 0.0],
        I,
        8.0,
        false,
    ),
];

/// The template's bone names, parents first.
pub const HUMANOID_BONES: [&str; 25] = {
    let mut names = [""; 25];
    let mut k = 0;
    while k < BONES.len() {
        names[k] = BONES[k].0;
        k += 1;
    }
    names
};

/// Mixamo's namespace before each bone name.
const PREFIX: &str = "mixamorig:";

impl Armature {
    /// The Humanoid template fitted to a model standing in the box from
    /// `min` to `max` (Y up, facing +Z, its left at +X): the ground at the
    /// bottom, the head's tip at the top, in the middle across.
    pub fn humanoid(min: Vec3, max: Vec3) -> Armature {
        // Each bone's place and frame in the template's centimetres.
        let mut world: Vec<(Vec3, Quat)> = Vec::with_capacity(BONES.len());
        for &(_, parent, translation, rotation, _, _) in BONES {
            let local = (
                Vec3::from(translation),
                Quat::from_array(rotation).normalize(),
            );
            let placed = match BONES.iter().position(|bone| bone.0 == parent) {
                Some(p) => {
                    let (head, turn) = world[p];
                    (head + turn * local.0, (turn * local.1).normalize())
                }
                None => local,
            };
            world.push(placed);
        }
        let top = BONES
            .iter()
            .position(|bone| bone.0 == "HeadTop_End")
            .map_or(1.0, |k| world[k].0.y);
        let height = (max.y - min.y).max(1e-3);
        let unit = height / top;
        let origin = Vec3::new((min.x + max.x) / 2.0, min.y, (min.z + max.z) / 2.0);
        let bones = BONES
            .iter()
            .zip(&world)
            .map(
                |(&(name, parent, _, _, length, deform), &(head, rotation))| ArmBone {
                    name: format!("{PREFIX}{name}"),
                    parent: BONES.iter().position(|bone| bone.0 == parent),
                    head: origin + head * unit,
                    rotation,
                    length: length * unit,
                    deform,
                },
            )
            .collect();
        Armature {
            bones,
            origin,
            unit,
        }
    }
}

/// How thick the stand-in body is round each template bone, as a share of
/// the height (a sphere for the head). The legs are thin enough to stay
/// apart, as Mixamo's are close together.
const BODY: &[(&str, f32)] = &[
    ("Hips", 0.085),
    ("Spine", 0.08),
    ("Spine1", 0.085),
    ("Spine2", 0.09),
    ("Neck", 0.035),
    ("LeftShoulder", 0.035),
    ("LeftArm", 0.033),
    ("LeftForeArm", 0.028),
    ("LeftHand", 0.022),
    ("RightShoulder", 0.035),
    ("RightArm", 0.033),
    ("RightForeArm", 0.028),
    ("RightHand", 0.022),
    ("LeftUpLeg", 0.036),
    ("LeftLeg", 0.03),
    ("LeftFoot", 0.024),
    ("LeftToeBase", 0.02),
    ("RightUpLeg", 0.036),
    ("RightLeg", 0.03),
    ("RightFoot", 0.024),
    ("RightToeBase", 0.02),
];

impl Armature {
    /// A plain closed body round a [`humanoid`](Armature::humanoid)
    /// template (capsules along its bones blended together, a ball for the
    /// head, cut off at the ground), sampled `size` apart: a model to rig
    /// in tests. `None` if this isn't the template.
    #[doc(hidden)]
    pub fn stand_in_body(&self, size: f32) -> Option<EditMesh> {
        let find = |name: &str| self.bone(&format!("{PREFIX}{name}"));
        let ground = self.origin.y;
        let top = self.bones[find("HeadTop_End")?].head.y;
        let height = top - ground;
        let mut capsules = Vec::new();
        for &(name, radius) in BODY {
            let bone = &self.bones[find(name)?];
            capsules.push((bone.head, bone.tail(), radius * height));
        }
        let head = &self.bones[find("Head")?];
        let ball = (head.head + head.tail()) / 2.0;
        let ball_radius = top - ball.y;
        let blend = 0.02 * height;
        let distance = |p: Vec3| {
            let mut d = p.distance(ball) - ball_radius;
            for &(a, b, r) in &capsules {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                let e = p.distance(a + ab * t) - r;
                // A smooth minimum, so limbs join the body without a crease.
                let h = (0.5 + 0.5 * (e - d) / blend).clamp(0.0, 1.0);
                d = e + (d - e) * h - blend * h * (1.0 - h);
            }
            d.max(ground - p.y)
        };
        let (min, max) = capsules.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &(a, b, r)| (lo.min(a.min(b) - r), hi.max(a.max(b) + r)),
        );
        let min = min
            .min(ball - ball_radius)
            .max(Vec3::new(f32::MIN, ground, f32::MIN));
        let max = max.max(ball + ball_radius);
        crate::remesh::field_mesh(min, max, size, distance).ok()
    }
}
