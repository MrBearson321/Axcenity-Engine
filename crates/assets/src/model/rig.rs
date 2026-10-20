//! Skeletons, skin weights and animation takes, and posing a model with them.
//!
//! - A [`Rig`] is the skeleton a model's triangles hang from: its [`Bone`]s
//!   (parents first) and the [`Joint`]s its vertices are weighted to. Only
//!   animated models have one ([`Model::rig`]); FBX files give it.
//! - A [`Take`] is one timeline of an FBX file (an animation stack), baked
//!   into keys per bone. Takes name their bones, so a take from one file can
//!   move the skeleton of a model loaded from another (an animation-only
//!   FBX for the same character), matched by bone name ([`Take::bind`]).
//! - A [`Pose`] is where every bone is, relative to its parent. A take
//!   [samples](Take::sample) into one, and [`Model::pose`] skins the
//!   model's triangles with it, on the CPU, into a [`Posed`] copy.

use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

use super::Model;

/// Where a bone is relative to its parent: moved, then turned, then
/// stretched (applied to its children in the opposite order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl BoneTransform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// `t` of the way from `self` (0) to `other` (1). Rotations take the
    /// short way round.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }
}

/// Where every bone of a [`Rig`] is, in the rig's bone order.
pub type Pose = Vec<BoneTransform>;

/// One bone of a skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Its name in the file, which takes use to find it.
    pub name: String,
    /// Its parent's index in [`Rig::bones`], always smaller than its own.
    /// `None` for the top of the skeleton.
    pub parent: Option<usize>,
    /// Where it is in the file when nothing animates it.
    pub rest: BoneTransform,
}

/// Something vertices are weighted to: a bone, and how to undo where that
/// bone was when the skin was bound to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub bone: usize,
    /// From the file's space (where the model's vertices were, before
    /// fitting) into the bone's own space as it was when bound.
    pub inverse_bind: Mat4,
}

/// A model's skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    /// Parents before their children.
    pub bones: Vec<Bone>,
    pub joints: Vec<Joint>,
    /// From the file's space into the model's 1×1×1 fitted space.
    pub fit: Mat4,
}

impl Rig {
    /// Every bone where it rests.
    pub fn rest_pose(&self) -> Pose {
        self.bones.iter().map(|bone| bone.rest).collect()
    }

    /// The first bone named `name`.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// Each bone's transform from its own space into the file's space, for
    /// `pose`. Bones missing from a short pose are taken as resting.
    pub fn world(&self, pose: &[BoneTransform]) -> Vec<Mat4> {
        let mut world: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let local = pose.get(index).unwrap_or(&bone.rest).matrix();
            let matrix = match bone.parent {
                Some(parent) if parent < index => world[parent] * local,
                _ => local,
            };
            world.push(matrix);
        }
        world
    }

    /// Each joint's transform from a bound vertex to where `pose` puts it,
    /// in the fitted space the model's triangles are in.
    pub fn joint_matrices(&self, pose: &[BoneTransform]) -> Vec<Mat4> {
        let world = self.world(pose);
        let unfit = self.fit.inverse();
        self.joints
            .iter()
            .map(|joint| {
                let bone = world.get(joint.bone).copied().unwrap_or(Mat4::IDENTITY);
                self.fit * bone * joint.inverse_bind * unfit
            })
            .collect()
    }
}

/// Values at times, for one channel of a [`Track`]. Between two keys the
/// value is blended; before the first and after the last, it holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys<T> {
    /// In seconds from the start of the take, increasing.
    pub times: Vec<f32>,
    pub values: Vec<T>,
}

/// A value [`Keys`] can blend between.
pub trait Blend: Copy {
    fn blend(a: Self, b: Self, t: f32) -> Self;
}

impl Blend for Vec3 {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        a.lerp(b, t)
    }
}

impl Blend for Quat {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        a.slerp(b, t)
    }
}

impl<T: Blend> Keys<T> {
    /// The value at `time`, or `None` if there are no keys.
    pub fn sample(&self, time: f32) -> Option<T> {
        let count = self.times.len().min(self.values.len());
        if count == 0 {
            return None;
        }
        // The first key after `time`. (`partition_point` is a binary search.)
        let after = self.times[..count].partition_point(|&key| key <= time);
        if after == 0 {
            return Some(self.values[0]);
        }
        if after == count {
            return Some(self.values[count - 1]);
        }
        let (t0, t1) = (self.times[after - 1], self.times[after]);
        let span = t1 - t0;
        let t = if span > 0.0 { (time - t0) / span } else { 0.0 };
        Some(T::blend(self.values[after - 1], self.values[after], t))
    }
}

/// How one bone moves through a take.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// The bone's name, matched against a rig's (see [`Take::bind`]).
    pub bone: String,
    pub translation: Keys<Vec3>,
    pub rotation: Keys<Quat>,
    pub scale: Keys<Vec3>,
}

impl Track {
    /// Where the bone is at `time`, starting from `rest` for any channel
    /// the track doesn't key.
    pub fn sample(&self, time: f32, rest: BoneTransform) -> BoneTransform {
        BoneTransform {
            translation: self.translation.sample(time).unwrap_or(rest.translation),
            rotation: self.rotation.sample(time).unwrap_or(rest.rotation),
            scale: self.scale.sample(time).unwrap_or(rest.scale),
        }
    }
}

/// One timeline of animation from a file (an FBX animation stack).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Take {
    /// Its name in the file, such as `Take 001` or `mixamo.com`.
    pub name: String,
    /// Seconds from its first frame to its last.
    pub duration: f32,
    /// The file's frames per second, for showing times as frames.
    pub frame_rate: f32,
    /// One per bone it moves.
    pub tracks: Vec<Track>,
}

impl Take {
    /// How many frames long it is, at its frame rate.
    pub fn frames(&self) -> u32 {
        (self.duration * self.frame_rate).round().max(0.0) as u32
    }

    /// Which bone of `rig` each track moves, by name: `None` for a track
    /// whose bone the rig doesn't have. Work it out once per rig.
    pub fn bind(&self, rig: &Rig) -> Vec<Option<usize>> {
        self.tracks
            .iter()
            .map(|track| rig.bone(&track.bone))
            .collect()
    }

    /// Moves the bones this take animates to where they are at `time`
    /// seconds (held before 0 and after the end). `binding` is from
    /// [`Take::bind`] for the rig `pose` is of; other bones keep their place.
    pub fn sample(
        &self,
        time: f32,
        binding: &[Option<usize>],
        rig: &Rig,
        pose: &mut [BoneTransform],
    ) {
        for (track, bone) in self.tracks.iter().zip(binding) {
            let Some(bone) = *bone else {
                continue;
            };
            let (Some(slot), Some(rest)) = (pose.get_mut(bone), rig.bones.get(bone)) else {
                continue;
            };
            *slot = track.sample(time, rest.rest);
        }
    }
}

/// Every [`Posed`] gets a new version, so the renderer can tell new
/// vertices from ones it already has.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// A model's triangles moved by a pose (see [`Model::pose`]). Texture
/// coordinates and indices don't move, so they stay in the model.
#[derive(Clone, Debug, PartialEq)]
pub struct Posed {
    /// The model this is a pose of ([`Model::key`]).
    pub model_key: u64,
    /// Different for every pose made.
    pub version: u64,
    /// One per primitive of the model, in the same order.
    pub primitives: Vec<PosedPrimitive>,
}

/// One primitive's moved corners.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PosedPrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Empty when the primitive has none.
    pub tangents: Vec<[f32; 4]>,
}

impl Model {
    /// The model's triangles moved by `pose` (one bone transform per bone
    /// of its rig), or `None` if it has no rig. Vertices weighted to
    /// nothing stay where they are.
    pub fn pose(&self, pose: &[BoneTransform]) -> Option<Posed> {
        let rig = self.rig.as_ref()?;
        let joints = rig.joint_matrices(pose);
        // Normals are turned by the inverse transpose, so they stay
        // perpendicular to stretched surfaces.
        let normal_matrices: Vec<Mat3> = joints
            .iter()
            .map(|joint| Mat3::from_mat4(*joint).inverse().transpose())
            .collect();
        let primitives = self
            .primitives
            .iter()
            .map(|primitive| {
                let mut posed = PosedPrimitive {
                    positions: primitive.positions.clone(),
                    normals: primitive.normals.clone(),
                    tangents: primitive.tangents.clone(),
                };
                let count = primitive.joints.len().min(primitive.weights.len());
                for vertex in 0..count.min(posed.positions.len()) {
                    let (indices, weights) = (primitive.joints[vertex], primitive.weights[vertex]);
                    let mut matrix = Mat4::ZERO;
                    let mut normal_matrix = Mat3::ZERO;
                    let mut total = 0.0;
                    for (&joint, &weight) in indices.iter().zip(&weights) {
                        let joint = usize::from(joint);
                        if weight <= 0.0 || joint >= joints.len() {
                            continue;
                        }
                        matrix += joints[joint] * weight;
                        normal_matrix += normal_matrices[joint] * weight;
                        total += weight;
                    }
                    if total <= 1e-6 {
                        continue;
                    }
                    let position = matrix.transform_point3(Vec3::from(posed.positions[vertex]));
                    posed.positions[vertex] = (position / total).to_array();
                    if let Some(normal) = posed.normals.get_mut(vertex) {
                        *normal = (normal_matrix * Vec3::from(*normal))
                            .normalize_or_zero()
                            .to_array();
                    }
                    if let Some(tangent) = posed.tangents.get_mut(vertex) {
                        let turned = matrix.transform_vector3(Vec4::from(*tangent).truncate());
                        let turned = turned.normalize_or_zero();
                        *tangent = [turned.x, turned.y, turned.z, tangent[3]];
                    }
                }
                posed
            })
            .collect();
        Some(Posed {
            model_key: self.key(),
            version: NEXT_VERSION.fetch_add(1, Ordering::Relaxed),
            primitives,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys<T>(pairs: &[(f32, T)]) -> Keys<T>
    where
        T: Copy,
    {
        Keys {
            times: pairs.iter().map(|(t, _)| *t).collect(),
            values: pairs.iter().map(|(_, v)| *v).collect(),
        }
    }

    #[test]
    fn keys_blend_between_and_hold_outside() {
        let track = keys(&[(0.0, Vec3::ZERO), (1.0, Vec3::new(2.0, 0.0, 0.0))]);
        assert_eq!(track.sample(-1.0), Some(Vec3::ZERO));
        assert_eq!(track.sample(0.5), Some(Vec3::new(1.0, 0.0, 0.0)));
        assert_eq!(track.sample(5.0), Some(Vec3::new(2.0, 0.0, 0.0)));
        assert_eq!(Keys::<Vec3>::default().sample(0.0), None);
    }

    #[test]
    fn a_bone_chain_multiplies_parents_first() {
        let rig = Rig {
            bones: vec![
                Bone {
                    name: "Root".into(),
                    parent: None,
                    rest: BoneTransform {
                        translation: Vec3::new(0.0, 1.0, 0.0),
                        ..BoneTransform::IDENTITY
                    },
                },
                Bone {
                    name: "Tip".into(),
                    parent: Some(0),
                    rest: BoneTransform {
                        translation: Vec3::new(0.0, 1.0, 0.0),
                        ..BoneTransform::IDENTITY
                    },
                },
            ],
            joints: Vec::new(),
            fit: Mat4::IDENTITY,
        };
        let mut pose = rig.rest_pose();
        pose[0].rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let world = rig.world(&pose);
        // Turning the root a quarter turn swings the tip from above it to
        // its left.
        let tip = world[1].transform_point3(Vec3::ZERO);
        assert!((tip - Vec3::new(-1.0, 1.0, 0.0)).length() < 1e-5, "{tip}");
    }
}
