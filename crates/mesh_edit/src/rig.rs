//! Basic rigging (Phase 62): a model's armature, its objects' skin
//! weights, and posing them to test the deformation.
//!
//! - An [`Armature`] is the model's bones as Armature mode edits them:
//!   each [`ArmBone`] is a head, a frame (its Y axis runs from the head to
//!   the tail; turning it round Y is its roll) and a length, all in the
//!   file's space. Saving turns it into an `assets` [`Rig`] under one root
//!   bone, `Armature`, that holds the armature's origin and unit, so a
//!   template copied from Mixamo keeps Mixamo's own bone offsets (in
//!   centimetres) and its clips play on it unchanged.
//! - A [`Skin`] is an object's weights: per vertex, any number of bones
//!   and how much each moves it. What's drawn and saved is each vertex's 4
//!   strongest, adding up to 1 ([`Skin::influences`]), so the Studio's
//!   Pose test deforms exactly as the game does.
//! - Automatic weights (bone heat), weights by distance and weight paint
//!   are `rig/weights.rs`; the Humanoid template is `rig/template.rs`.

use std::collections::HashMap;

use axcenity_assets::{Bone, BoneTransform, Joint, Pose, Rig};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::half_edge::EditMesh;

mod template;
pub mod weights;

pub use template::HUMANOID_BONES;
pub use weights::{
    WeightBrush, WeightPainter, WeightTool, automatic_weights, mirror_weights, weights_by_distance,
};

/// The name of the root bone a saved armature hangs from.
pub const ROOT_NAME: &str = "Armature";

/// The most bones a vertex moves with once saved (glTF's `JOINTS_0`).
pub const MAX_INFLUENCES: usize = 4;

/// One bone of an [`Armature`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArmBone {
    pub name: String,
    /// Its parent's index in [`Armature::bones`], if any.
    pub parent: Option<usize>,
    /// Where it starts, in the file's space.
    #[serde(with = "arrays::vec3")]
    pub head: Vec3,
    /// Its frame in the file's space: Y along the bone, toward the tail.
    #[serde(with = "arrays::quat")]
    pub rotation: Quat,
    pub length: f32,
    /// Whether automatic weights and weights by distance give it any.
    /// Tips (`HeadTop_End`) and helpers don't deform.
    pub deform: bool,
}

impl ArmBone {
    pub fn new(name: &str, head: Vec3, tail: Vec3, parent: Option<usize>) -> ArmBone {
        let length = head.distance(tail);
        let direction = (tail - head).try_normalize().unwrap_or(Vec3::Y);
        ArmBone {
            name: name.to_owned(),
            parent,
            head,
            rotation: zero_roll(direction),
            length,
            deform: true,
        }
    }

    pub fn tail(&self) -> Vec3 {
        self.head + self.direction() * self.length
    }

    /// Its Y axis: from the head toward the tail.
    pub fn direction(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    /// From the bone's own space into the file's.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation, self.head)
    }

    /// Moves its tail to `tail`, turning it the shortest way, so its roll
    /// stays as near as it can.
    pub fn set_tail(&mut self, tail: Vec3) {
        let Some(direction) = (tail - self.head).try_normalize() else {
            return;
        };
        self.rotation =
            (Quat::from_rotation_arc(self.direction(), direction) * self.rotation).normalize();
        self.length = self.head.distance(tail);
    }

    /// Moves its head to `head`, keeping its tail.
    pub fn set_head(&mut self, head: Vec3) {
        let tail = self.tail();
        if head.distance(tail) < 1e-6 {
            return;
        }
        self.head = head;
        self.set_tail(tail);
    }

    /// How far it's turned round its own Y from no roll, in radians.
    pub fn roll(&self) -> f32 {
        let unrolled = zero_roll(self.direction());
        let twist = unrolled.inverse() * self.rotation;
        // What's left is a turn round Y.
        2.0 * twist.y.atan2(twist.w)
    }

    pub fn set_roll(&mut self, roll: f32) {
        self.rotation = (zero_roll(self.direction()) * Quat::from_rotation_y(roll)).normalize();
    }
}

/// The frame of a bone along `direction` with no roll: Y turned onto it
/// the shortest way.
fn zero_roll(direction: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, direction.normalize_or(Vec3::Y))
}

/// A model's skeleton as Armature mode edits it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Armature {
    pub bones: Vec<ArmBone>,
    /// Where the root bone is, in the file's space.
    #[serde(with = "arrays::vec3")]
    pub origin: Vec3,
    /// The root's scale: the saved bones' offsets are their distances in
    /// the file over this (0.01 × the fit for Mixamo's centimetres).
    pub unit: f32,
}

impl Default for Armature {
    fn default() -> Self {
        Armature {
            bones: Vec::new(),
            origin: Vec3::ZERO,
            unit: 1.0,
        }
    }
}

impl Armature {
    /// The first bone named `name`.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// `name`, or with `.001`, `.002`… after it until no other bone has it.
    pub fn unique_name(&self, name: &str, except: Option<usize>) -> String {
        let taken = |candidate: &str| {
            self.bones
                .iter()
                .enumerate()
                .any(|(k, bone)| Some(k) != except && bone.name == candidate)
        };
        let name = if name.trim().is_empty() {
            "Bone"
        } else {
            name.trim()
        };
        if !taken(name) {
            return name.to_owned();
        }
        (1..)
            .map(|n| format!("{name}.{n:03}"))
            .find(|candidate| !taken(candidate))
            .expect("some number is free")
    }

    /// Adds a bone from `head` to `tail` and returns its index.
    pub fn add_bone(&mut self, name: &str, head: Vec3, tail: Vec3, parent: Option<usize>) -> usize {
        let name = self.unique_name(name, None);
        self.bones.push(ArmBone::new(&name, head, tail, parent));
        self.bones.len() - 1
    }

    /// E: a child of `bone` from its tail, the same way and as long, named
    /// after it. Returns the new bone.
    pub fn extrude(&mut self, bone: usize) -> Option<usize> {
        let parent = self.bones.get(bone)?.clone();
        let name = self.unique_name(&parent.name, None);
        self.bones.push(ArmBone {
            name,
            parent: Some(bone),
            head: parent.tail(),
            ..parent
        });
        Some(self.bones.len() - 1)
    }

    /// True if `bone` is `ancestor` or inside it.
    pub fn is_inside(&self, bone: usize, ancestor: usize) -> bool {
        let mut at = Some(bone);
        let mut steps = 0;
        while let Some(current) = at {
            if current == ancestor {
                return true;
            }
            steps += 1;
            if steps > self.bones.len() {
                return false;
            }
            at = self.bones.get(current).and_then(|b| b.parent);
        }
        false
    }

    /// Makes `parent` the parent of `bone` (it stays where it is). False,
    /// changing nothing, if that would put a bone inside itself.
    pub fn set_parent(&mut self, bone: usize, parent: Option<usize>) -> bool {
        if bone >= self.bones.len()
            || parent.is_some_and(|p| p >= self.bones.len() || self.is_inside(p, bone))
        {
            return false;
        }
        self.bones[bone].parent = parent;
        true
    }

    /// Renames `bone` (made unique) and returns the name it got.
    pub fn rename(&mut self, bone: usize, name: &str) -> String {
        let name = self.unique_name(name, Some(bone));
        if let Some(bone) = self.bones.get_mut(bone) {
            bone.name = name.clone();
        }
        name
    }

    /// Takes `bone` out; its children go to its parent. Returns where each
    /// old bone went (`None` for the deleted one).
    pub fn delete(&mut self, bone: usize) -> Vec<Option<usize>> {
        let mut map: Vec<Option<usize>> = (0..self.bones.len()).map(Some).collect();
        if bone >= self.bones.len() {
            return map;
        }
        let grandparent = self.bones[bone].parent;
        for other in &mut self.bones {
            if other.parent == Some(bone) {
                other.parent = grandparent;
            }
        }
        self.bones.remove(bone);
        map[bone] = None;
        for slot in map.iter_mut().skip(bone + 1) {
            *slot = slot.map(|k| k - 1);
        }
        for other in &mut self.bones {
            other.parent = other.parent.and_then(|p| map[p]);
        }
        map
    }

    /// The bone with `bone`'s name mirrored (`.L` ↔ `.R`, `Left` ↔
    /// `Right`), if there's one.
    pub fn mirror_of(&self, bone: usize) -> Option<usize> {
        let name = mirror_name(&self.bones.get(bone)?.name)?;
        self.bone(&name)
    }

    /// Symmetrize: a copy of each of `bones` with a sided name mirrored
    /// across X = 0 (the file's own plane), named for the other side;
    /// one already there is moved to match. A copy's parent is the mirror
    /// of the bone's parent, if there's one. Returns the copies.
    pub fn symmetrize(&mut self, bones: &[usize]) -> Vec<usize> {
        // Parents first, so a copy's parent is copied before it.
        let mut bones: Vec<usize> = bones
            .iter()
            .copied()
            .filter(|&b| b < self.bones.len())
            .collect();
        bones.sort_by_key(|&b| self.depth(b));
        let mut made = Vec::new();
        for bone in bones {
            let source = self.bones[bone].clone();
            let Some(name) = mirror_name(&source.name) else {
                continue;
            };
            let parent = source.parent.map(|p| self.mirror_of(p).unwrap_or(p));
            let mirrored = ArmBone {
                name: name.clone(),
                parent,
                head: mirror(source.head),
                rotation: mirror_rotation(source.rotation),
                ..source
            };
            let index = match self.bone(&name) {
                Some(existing) => {
                    self.bones[existing] = mirrored;
                    existing
                }
                None => {
                    self.bones.push(mirrored);
                    self.bones.len() - 1
                }
            };
            made.push(index);
        }
        made
    }

    /// How many parents `bone` has above it.
    pub fn depth(&self, bone: usize) -> usize {
        let mut depth = 0;
        let mut at = self.bones.get(bone).and_then(|b| b.parent);
        while let Some(parent) = at {
            depth += 1;
            if depth > self.bones.len() {
                break;
            }
            at = self.bones.get(parent).and_then(|b| b.parent);
        }
        depth
    }

    /// The bones parents first (then in their order), as they're saved.
    pub fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.bones.len()).collect();
        order.sort_by_key(|&b| self.depth(b));
        // Children right after their parents, as a tree is walked.
        let mut walked = Vec::with_capacity(order.len());
        let mut placed = vec![false; self.bones.len()];
        fn walk(arm: &Armature, bone: usize, placed: &mut [bool], out: &mut Vec<usize>) {
            if placed[bone] {
                return;
            }
            placed[bone] = true;
            out.push(bone);
            for child in (0..arm.bones.len()).filter(|&c| arm.bones[c].parent == Some(bone)) {
                walk(arm, child, placed, out);
            }
        }
        for bone in order {
            let top = self.bones[bone]
                .parent
                .is_none_or(|p| p >= self.bones.len());
            if top {
                walk(self, bone, &mut placed, &mut walked);
            }
        }
        for bone in 0..self.bones.len() {
            walk(self, bone, &mut placed, &mut walked);
        }
        walked
    }

    /// The root bone's transform from its own space into the file's.
    fn root_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.unit), Quat::IDENTITY, self.origin)
    }

    /// Bone `bone`'s rest transform from its own space (scaled by the
    /// root, as its children see it) into the file's.
    fn world(&self, bone: usize) -> Mat4 {
        self.bones[bone].matrix() * Mat4::from_scale(Vec3::splat(self.unit))
    }

    /// Where `bone` rests relative to its parent (or the root bone).
    fn local_rest(&self, bone: usize) -> BoneTransform {
        let parent = match self.bones[bone].parent {
            Some(parent) if parent < self.bones.len() => self.world(parent),
            _ => self.root_matrix(),
        };
        let (scale, rotation, translation) =
            (parent.inverse() * self.world(bone)).to_scale_rotation_translation();
        BoneTransform {
            translation,
            rotation: rotation.normalize(),
            scale: if scale.abs_diff_eq(Vec3::ONE, 1e-4) {
                Vec3::ONE
            } else {
                scale
            },
        }
    }

    /// The skeleton saved: the root bone first, then the bones parents
    /// first; one joint per bone (not the root), in that order. Also each
    /// armature bone's place in the rig's bones (and joints, one less).
    /// `fit` is identity: the rig is in the file's space.
    pub fn to_rig(&self) -> (Rig, Vec<usize>) {
        let order = self.order();
        let mut place = vec![0; self.bones.len()];
        for (k, &bone) in order.iter().enumerate() {
            place[bone] = k + 1;
        }
        let mut bones = vec![Bone {
            name: ROOT_NAME.to_owned(),
            parent: None,
            rest: BoneTransform {
                translation: self.origin,
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(self.unit),
            },
        }];
        let mut joints = Vec::new();
        for &bone in &order {
            let parent = match self.bones[bone].parent {
                Some(parent) if parent < self.bones.len() => place[parent],
                _ => 0,
            };
            bones.push(Bone {
                name: self.bones[bone].name.clone(),
                parent: Some(parent),
                rest: self.local_rest(bone),
            });
            joints.push(Joint {
                bone: place[bone],
                inverse_bind: self.world(bone).inverse(),
            });
        }
        let rig = Rig {
            bones,
            joints,
            fit: Mat4::IDENTITY,
        };
        (rig, place)
    }

    /// The armature of a loaded skeleton (whose bones are in the file's
    /// space; `fit` is undone), and each rig bone's armature bone (`None`
    /// for the root). A top bone that no vertex is weighted to and that
    /// only holds the others (Mixamo's, or a saved `Armature`) becomes
    /// the root: its place and scale are the origin and unit.
    pub fn from_rig(rig: &Rig) -> (Armature, Vec<Option<usize>>) {
        let jointed: Vec<bool> = (0..rig.bones.len())
            .map(|b| rig.joints.iter().any(|joint| joint.bone == b))
            .collect();
        let world = rig.world(&rig.rest_pose());
        let tops: Vec<usize> = (0..rig.bones.len())
            .filter(|&b| rig.bones[b].parent.is_none())
            .collect();
        let root = match tops.as_slice() {
            [top] if !jointed[*top] => Some(*top),
            _ => None,
        };
        let (unit, origin) = match root {
            Some(root) => {
                let (scale, _, translation) = world[root].to_scale_rotation_translation();
                (scale.x.abs().max(1e-9), translation)
            }
            None => (1.0, Vec3::ZERO),
        };
        let mut arm = Armature {
            bones: Vec::new(),
            origin,
            unit,
        };
        let mut map = vec![None; rig.bones.len()];
        for (index, bone) in rig.bones.iter().enumerate() {
            if Some(index) == root {
                continue;
            }
            let (_, rotation, head) = world[index].to_scale_rotation_translation();
            map[index] = Some(arm.bones.len());
            arm.bones.push(ArmBone {
                name: bone.name.clone(),
                parent: bone.parent.and_then(|p| map.get(p).copied().flatten()),
                head,
                rotation: rotation.normalize(),
                length: 0.0,
                deform: jointed[index],
            });
        }
        // A bone is as long as the way to its child along its Y, or half
        // its parent if it has none.
        for bone in 0..arm.bones.len() {
            let (head, direction) = (arm.bones[bone].head, arm.bones[bone].direction());
            let along = (0..arm.bones.len())
                .filter(|&c| arm.bones[c].parent == Some(bone))
                .map(|c| (arm.bones[c].head - head).dot(direction))
                .filter(|&d| d > 1e-6)
                .fold(f32::INFINITY, f32::min);
            if along.is_finite() {
                arm.bones[bone].length = along;
            }
        }
        for bone in arm.order() {
            if arm.bones[bone].length <= 0.0 {
                let parent = arm.bones[bone].parent.map(|p| arm.bones[p].length);
                arm.bones[bone].length = parent.map_or(0.1 * unit.max(0.01), |l| l * 0.5).max(1e-4);
            }
        }
        (arm, map)
    }

    /// Every bone resting, in the rig's order ([`to_rig`](Self::to_rig)).
    pub fn rest_pose(&self) -> Pose {
        self.to_rig().0.rest_pose()
    }

    /// A pose turning each armature bone by `turns` (one per bone, in
    /// its own space, from where it rests), in the rig's order.
    pub fn pose(&self, turns: &[Quat]) -> Pose {
        let (rig, place) = self.to_rig();
        let mut pose = rig.rest_pose();
        for (bone, &turn) in turns.iter().enumerate() {
            if let Some(&at) = place.get(bone) {
                pose[at].rotation = (pose[at].rotation * turn).normalize();
            }
        }
        pose
    }

    /// Each armature bone's skinning matrix for `pose` (in the rig's
    /// order): from where a vertex rests to where the pose puts it, in the
    /// file's space. The same as `Rig::joint_matrices` with no fit.
    pub fn skin_matrices(&self, pose: &[BoneTransform]) -> Vec<Mat4> {
        let (rig, place) = self.to_rig();
        let matrices = rig.joint_matrices(pose);
        place
            .iter()
            .map(|&at| matrices.get(at - 1).copied().unwrap_or(Mat4::IDENTITY))
            .collect()
    }
}

/// `name` with its side swapped: `.L`/`.R`, `_L`/`_R` (either case) at
/// the end, or `Left`/`Right` (either case) in it. `None` without a side.
pub fn mirror_name(name: &str) -> Option<String> {
    for (a, b) in [
        (".L", ".R"),
        (".l", ".r"),
        ("_L", "_R"),
        ("_l", "_r"),
        (".R", ".L"),
        (".r", ".l"),
        ("_R", "_L"),
        ("_r", "_l"),
    ] {
        // A number Blender adds (`hand.L.001`) stays after the side.
        let (stem, number) = match name.rsplit_once('.') {
            Some((stem, number))
                if number.len() == 3 && number.chars().all(|c| c.is_ascii_digit()) =>
            {
                (stem, &name[stem.len()..])
            }
            _ => (name, ""),
        };
        if let Some(base) = stem.strip_suffix(a) {
            return Some(format!("{base}{b}{number}"));
        }
    }
    for (a, b) in [
        ("Left", "Right"),
        ("Right", "Left"),
        ("left", "right"),
        ("right", "left"),
        ("LEFT", "RIGHT"),
        ("RIGHT", "LEFT"),
    ] {
        if let Some(at) = name.find(a) {
            return Some(format!("{}{b}{}", &name[..at], &name[at + a.len()..]));
        }
    }
    None
}

fn mirror(p: Vec3) -> Vec3 {
    Vec3::new(-p.x, p.y, p.z)
}

/// A frame reflected across X = 0, kept right-handed: its X axis flips
/// back (as Blender's symmetrize does), so a mirrored roll mirrors.
fn mirror_rotation(rotation: Quat) -> Quat {
    let m = Mat3::from_quat(rotation);
    let flip = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
    let (x, y, z) = (-flip(m.x_axis), flip(m.y_axis), flip(m.z_axis));
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

/// One object's skin weights.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Skin {
    /// Per vertex: armature bones and how much each moves it.
    pub weights: Vec<Vec<(u16, f32)>>,
    /// Where the vertices were when the weights were last set, to carry
    /// them over by place when an operation renumbers the mesh.
    #[serde(with = "arrays::vec3s")]
    pub(crate) positions: Vec<Vec3>,
}

impl Skin {
    /// `weights` for `mesh`'s vertices.
    pub fn new(mesh: &EditMesh, weights: Vec<Vec<(u16, f32)>>) -> Skin {
        let mut skin = Skin {
            weights,
            positions: mesh.positions().to_vec(),
        };
        skin.weights.resize(mesh.vertex_count(), Vec::new());
        skin
    }

    /// How much `bone` moves `vertex`.
    pub fn weight(&self, vertex: usize, bone: usize) -> f32 {
        self.weights.get(vertex).map_or(0.0, |list| {
            list.iter()
                .filter(|(b, _)| *b as usize == bone)
                .map(|(_, w)| *w)
                .sum()
        })
    }

    /// Sets how much `bone` moves `vertex` (0 takes it off).
    pub fn set_weight(&mut self, vertex: usize, bone: usize, weight: f32) {
        let Some(list) = self.weights.get_mut(vertex) else {
            return;
        };
        list.retain(|(b, _)| *b as usize != bone);
        if weight > 1e-4 {
            list.push((bone as u16, weight.min(1.0)));
        }
    }

    /// The bones moving `vertex` and how much, as it's drawn and saved:
    /// its [`MAX_INFLUENCES`] strongest, adding up to 1 (all 0 if none).
    pub fn influences(&self, vertex: usize) -> ([u16; 4], [f32; 4]) {
        let mut found: Vec<(u16, f32)> = self
            .weights
            .get(vertex)
            .map(|list| list.iter().copied().filter(|(_, w)| *w > 0.0).collect())
            .unwrap_or_default();
        strongest(&mut found, MAX_INFLUENCES);
        let total: f32 = found.iter().map(|(_, w)| w).sum();
        let (mut bones, mut weights) = ([0; 4], [0.0; 4]);
        if total > 1e-6 {
            for (slot, (bone, weight)) in found.into_iter().enumerate() {
                bones[slot] = bone;
                weights[slot] = weight / total;
            }
        }
        (bones, weights)
    }

    /// Normalize All: each vertex's weights scaled to add up to 1.
    pub fn normalize(&mut self) {
        for list in &mut self.weights {
            let total: f32 = list.iter().map(|(_, w)| w).sum();
            if total > 1e-6 {
                for (_, w) in list.iter_mut() {
                    *w /= total;
                }
            }
        }
    }

    /// Limit Total: each vertex keeps its `limit` strongest bones (less
    /// the next one's weight, so neighbours whose strongest differ don't
    /// jump apart).
    pub fn limit(&mut self, limit: usize) {
        for list in &mut self.weights {
            strongest(list, limit);
        }
    }

    /// Puts the bones through `map` (from [`Armature::delete`]): a deleted
    /// bone's weights go.
    pub fn remap(&mut self, map: &[Option<usize>]) {
        for list in &mut self.weights {
            list.retain_mut(
                |(bone, _)| match map.get(*bone as usize).copied().flatten() {
                    Some(to) => {
                        *bone = to as u16;
                        true
                    }
                    None => false,
                },
            );
        }
    }

    /// Notes `mesh`'s vertices as where the weights are now.
    pub fn touch(&mut self, mesh: &EditMesh) {
        self.positions = mesh.positions().to_vec();
        self.weights.resize(mesh.vertex_count(), Vec::new());
    }

    /// The weights for `mesh`: these, if it has as many vertices as they
    /// were set on; else each vertex takes the weights of the nearest one
    /// they were set on (an operation renumbered it, or modifiers made
    /// more).
    pub fn fitted(&self, mesh: &EditMesh) -> std::borrow::Cow<'_, Skin> {
        if mesh.vertex_count() == self.weights.len() {
            return std::borrow::Cow::Borrowed(self);
        }
        let nearest = nearest_points(&self.positions, mesh.positions());
        let weights = nearest
            .into_iter()
            .map(|k| {
                k.and_then(|k| self.weights.get(k).cloned())
                    .unwrap_or_default()
            })
            .collect();
        std::borrow::Cow::Owned(Skin::new(mesh, weights))
    }
}

/// Keeps `list`'s `limit` strongest weights, strongest first (equal ones
/// by bone, so the order is fixed), each less the strongest one dropped.
/// Cutting off at the next weight keeps the kept ones continuous: where two
/// vertices side by side drop different bones, those bones' weights were
/// alike, and both come out near nothing.
fn strongest(list: &mut Vec<(u16, f32)>, limit: usize) {
    list.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if list.len() > limit {
        let next = list[limit].1;
        list.truncate(limit);
        for (_, w) in list.iter_mut() {
            *w -= next;
        }
        list.retain(|&(_, w)| w > 0.0);
    }
}

/// For each of `to`, the index of the nearest of `from` (a grid of cells
/// searched outward from its own).
pub(crate) fn nearest_points(from: &[Vec3], to: &[Vec3]) -> Vec<Option<usize>> {
    if from.is_empty() {
        return vec![None; to.len()];
    }
    let (min, max) = from
        .iter()
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), &p| {
            (lo.min(p), hi.max(p))
        });
    let span = (max - min).max_element().max(1e-6);
    let cell = span / (from.len() as f32).cbrt().max(1.0);
    let key = |p: Vec3| ((p - min) / cell).floor().as_ivec3();
    let mut grid: HashMap<glam::IVec3, Vec<usize>> = HashMap::new();
    for (k, &p) in from.iter().enumerate() {
        grid.entry(key(p)).or_default().push(k);
    }
    let reach = ((span / cell).ceil() as i32 + 2).max(1);
    to.iter()
        .map(|&p| {
            let at = key(p);
            let mut best: Option<(f32, usize)> = None;
            for ring in 0..=reach {
                // A point in ring r is at least (r - 1) cells away.
                if let Some((d, _)) = best
                    && d.sqrt() < (ring as f32 - 1.0) * cell
                {
                    break;
                }
                for x in -ring..=ring {
                    for y in -ring..=ring {
                        for z in -ring..=ring {
                            if x.abs().max(y.abs()).max(z.abs()) != ring {
                                continue;
                            }
                            let Some(list) = grid.get(&(at + glam::IVec3::new(x, y, z))) else {
                                continue;
                            };
                            for &k in list {
                                let d = from[k].distance_squared(p);
                                if best.is_none_or(|(bd, bk)| d < bd || (d == bd && k < bk)) {
                                    best = Some((d, k));
                                }
                            }
                        }
                    }
                }
            }
            best.map(|(_, k)| k)
        })
        .collect()
}

/// How the game moves each of `count` vertices by `skin` with these
/// skinning `matrices` (one per armature bone): by its
/// [`Skin::influences`], as `assets`' `Posed` does. `None` for a vertex no
/// bone moves.
pub fn vertex_skinning(skin: &Skin, count: usize, matrices: &[Mat4]) -> Vec<Option<VertexSkin>> {
    let normal_matrices: Vec<Mat3> = matrices
        .iter()
        .map(|m| Mat3::from_mat4(*m).inverse().transpose())
        .collect();
    (0..count)
        .map(|vertex| {
            let (bones, weights) = skin.influences(vertex);
            let mut matrix = Mat4::ZERO;
            let mut normal = Mat3::ZERO;
            let mut total = 0.0;
            for (&bone, &weight) in bones.iter().zip(&weights) {
                let bone = bone as usize;
                if weight <= 0.0 || bone >= matrices.len() {
                    continue;
                }
                matrix += matrices[bone] * weight;
                normal += normal_matrices[bone] * weight;
                total += weight;
            }
            (total > 1e-6).then_some(VertexSkin {
                matrix,
                normal,
                total,
            })
        })
        .collect()
}

/// One vertex's blended skinning (see [`vertex_skinning`]).
#[derive(Clone, Copy, Debug)]
pub struct VertexSkin {
    matrix: Mat4,
    normal: Mat3,
    total: f32,
}

impl VertexSkin {
    pub fn position(&self, rest: Vec3) -> Vec3 {
        self.matrix.transform_point3(rest) / self.total
    }

    pub fn normal(&self, rest: Vec3) -> Vec3 {
        (self.normal * rest).normalize_or_zero()
    }
}

/// glam's vectors saved as plain arrays (`[x, y, z]`; `[x, y, z, w]`).
mod arrays {
    pub mod vec3 {
        use glam::Vec3;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(v: &Vec3, s: S) -> Result<S::Ok, S::Error> {
            v.to_array().serialize(s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec3, D::Error> {
            <[f32; 3]>::deserialize(d).map(Vec3::from)
        }
    }

    pub mod quat {
        use glam::Quat;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(q: &Quat, s: S) -> Result<S::Ok, S::Error> {
            q.to_array().serialize(s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Quat, D::Error> {
            <[f32; 4]>::deserialize(d).map(Quat::from_array)
        }
    }

    pub mod vec3s {
        use glam::Vec3;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(v: &[Vec3], s: S) -> Result<S::Ok, S::Error> {
            let arrays: Vec<[f32; 3]> = v.iter().map(|p| p.to_array()).collect();
            arrays.serialize(s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec3>, D::Error> {
            Vec::<[f32; 3]>::deserialize(d).map(|v| v.into_iter().map(Vec3::from).collect())
        }
    }
}
