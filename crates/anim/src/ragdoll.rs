//! Ragdolls (Phase 41): a Ragdoll under a skinned MeshPart is a capsule
//! Part per main bone and a joint between each body and the one above it:
//! a BallSocketConstraint, or a HingeConstraint for elbows and knees.
//!
//! - [`build`] makes them from the humanoid preset: bones are found by name
//!   (Mixamo's `mixamorig:LeftArm`, Blender's `upper_arm.L`, and the like),
//!   each body is a capsule from its bone to the next, and each joint's
//!   limits come from the preset ([`JOINTS`]). Elbows and knees are hinges,
//!   since a ball socket's cone can't hold a limb that bends one way only
//!   as firmly as a hinge's locked axes do.
//! - Each body is named after its bone and has an Attachment named
//!   [`BONE_ATTACHMENT`] where its bone is, so the body can be moved or
//!   resized and still pose the bone.
//! - [`crate::Animators`] drives them (see `animators.rs`): while the
//!   ragdoll is off the bodies follow the animated bones and physics leaves
//!   them out; while it's on, physics moves them and they pose the bones
//!   ([`pose_from_bodies`]), then the pose blends back to the Animator's.

use std::collections::HashMap;

use axcenity_assets::{Assets, BoneTransform, Pose, Rig};
use axcenity_core::{ClassName, InstanceId, PartType, PhysicalProperties, Value, Vector3, World};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};

/// What each body is made of: Plastic's weight, more grip, and no bounce
/// (its elasticity counts far more than what it lands on).
const BODY_PHYSICS: PhysicalProperties = PhysicalProperties::new(1.0, 0.8, 0.0, 1.0, 100.0);

/// The Attachment on each body where its bone is.
pub const BONE_ATTACHMENT: &str = "Bone";

/// What kind of joint a preset joint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    /// A BallSocketConstraint: turns any way within a cone (`UpperAngle`),
    /// and twists around the bone within `TwistLowerAngle`..`TwistUpperAngle`.
    Ball,
    /// A HingeConstraint: bends one way only, like an elbow or a knee,
    /// within `LowerAngle`..`UpperAngle` (positive bends it).
    Hinge,
}

/// A joint of the humanoid preset, between a body and its parent's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointPreset {
    /// The constraint's name ("Left" or "Right" in front for a limb).
    pub name: &'static str,
    pub kind: JointKind,
    /// A ball socket's cone, in degrees (none for a hinge).
    pub cone: f32,
    /// Degrees the middle of a ball socket's cone leans from the bone at
    /// rest: arms down from a T-pose, legs ahead, so a cone narrow enough
    /// to keep a limb believable still reaches where it hangs.
    pub lean: f32,
    /// Degrees: a ball socket's twist, or a hinge's bend.
    pub range: (f32, f32),
}

const fn ball(name: &'static str, cone: f32, lean: f32, twist: f32) -> JointPreset {
    JointPreset {
        name,
        kind: JointKind::Ball,
        cone,
        lean,
        range: (-twist, twist),
    }
}

const fn hinge(name: &'static str, bend: f32) -> JointPreset {
    JointPreset {
        name,
        kind: JointKind::Hinge,
        cone: 0.0,
        lean: 0.0,
        range: (-3.0, bend),
    }
}

/// The humanoid preset's joints: the waist (chest on hips), neck, shoulders,
/// elbows, hips and knees.
pub const JOINTS: [JointPreset; 6] = [
    ball("Waist", 25.0, 0.0, 30.0),
    ball("Neck", 35.0, 0.0, 45.0),
    ball("Shoulder", 70.0, 40.0, 45.0),
    hinge("Elbow", 140.0),
    ball("Hip", 50.0, 25.0, 30.0),
    hinge("Knee", 140.0),
];

/// The bodies of the preset, parents first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Limb {
    Pelvis,
    Chest,
    Head,
    UpperArm(Side),
    ForeArm(Side),
    Thigh(Side),
    Calf(Side),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    Left,
    Right,
}

/// The preset joint above each limb (none above the pelvis).
fn joint_of(limb: Limb) -> Option<JointPreset> {
    let index = match limb {
        Limb::Pelvis => return None,
        Limb::Chest => 0,
        Limb::Head => 1,
        Limb::UpperArm(_) => 2,
        Limb::ForeArm(_) => 3,
        Limb::Thigh(_) => 4,
        Limb::Calf(_) => 5,
    };
    Some(JOINTS[index])
}

/// A bone name without its namespace (`mixamorig:`), lowercase, letters and
/// digits only, and which side it's on, if it says: `LeftArm`, `upper_arm.L`
/// and `Arm_R` all work.
pub(crate) fn bone_kind(name: &str) -> (String, Option<Side>) {
    let name = name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase();
    let mut side = None;
    let mut rest = name.as_str();
    for (prefix, s) in [
        ("left", Side::Left),
        ("right", Side::Right),
        ("l_", Side::Left),
        ("r_", Side::Right),
    ] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r;
            side = Some(s);
            break;
        }
    }
    if side.is_none() {
        for (suffix, s) in [
            (".l", Side::Left),
            ("_l", Side::Left),
            (" l", Side::Left),
            (".r", Side::Right),
            ("_r", Side::Right),
            (" r", Side::Right),
        ] {
            if let Some(r) = rest.strip_suffix(suffix) {
                rest = r;
                side = Some(s);
                break;
            }
        }
    }
    let base = rest.chars().filter(char::is_ascii_alphanumeric).collect();
    (base, side)
}

/// The right hand's bone in `rig` (Phase 45, where Tools are held):
/// `RightHand`, `mixamorig:RightHand`, `hand.R`, `Hand_R` and the like.
pub fn right_hand(rig: &Rig) -> Option<usize> {
    rig.bones
        .iter()
        .position(|bone| bone_kind(&bone.name) == ("hand".to_owned(), Some(Side::Right)))
}

/// Where bone `bone` of the skinned MeshPart `part` is in the world, as
/// `posed` (its pose from [`Animators::poses`](crate::Animators::poses))
/// puts it, or resting with no pose. It carries the MeshPart's scale.
pub fn bone_in_world(
    world: &World,
    part: InstanceId,
    rig: &Rig,
    posed: Option<&axcenity_assets::Posed>,
    bone: usize,
) -> Option<Mat4> {
    let place = placement(world, part, rig)?;
    let joint = rig.joints.iter().position(|joint| joint.bone == bone);
    let in_file = match (posed, joint) {
        // A joint's matrix is fit * bone * inverse_bind * unfit, in the
        // model's fitted space: undo the rest to get the bone.
        (Some(posed), Some(index)) if posed.joints().len() == rig.joints.len() => {
            let fit = rig.fit;
            fit.inverse() * posed.joints()[index] * fit * rig.joints[index].inverse_bind.inverse()
        }
        _ => *rig.world(&rig.rest_pose()).get(bone)?,
    };
    let world_bone = place * in_file;
    world_bone.is_finite().then_some(world_bone)
}

/// The names (as [`bone_kind`] gives them) a limb's bone may have.
fn names_of(limb: Limb) -> &'static [&'static str] {
    match limb {
        Limb::Head => &["head"],
        Limb::UpperArm(_) => &["arm", "upperarm"],
        Limb::ForeArm(_) => &["forearm", "lowerarm"],
        Limb::Thigh(_) => &["upleg", "thigh", "upperleg"],
        Limb::Calf(_) => &["leg", "shin", "calf", "lowerleg"],
        // Found from the limbs: see `find_limbs`.
        Limb::Pelvis | Limb::Chest => &[],
    }
}

fn side_of(limb: Limb) -> Option<Side> {
    match limb {
        Limb::UpperArm(side) | Limb::ForeArm(side) | Limb::Thigh(side) | Limb::Calf(side) => {
            Some(side)
        }
        _ => None,
    }
}

/// The body above `limb`'s.
fn parent_of(limb: Limb) -> Option<Limb> {
    match limb {
        Limb::Pelvis => None,
        Limb::Chest | Limb::Thigh(_) => Some(Limb::Pelvis),
        Limb::Head | Limb::UpperArm(_) => Some(Limb::Chest),
        Limb::ForeArm(side) => Some(Limb::UpperArm(side)),
        Limb::Calf(side) => Some(Limb::Thigh(side)),
    }
}

/// Every limb, parents first.
const LIMBS: [Limb; 11] = [
    Limb::Pelvis,
    Limb::Chest,
    Limb::Head,
    Limb::UpperArm(Side::Left),
    Limb::ForeArm(Side::Left),
    Limb::UpperArm(Side::Right),
    Limb::ForeArm(Side::Right),
    Limb::Thigh(Side::Left),
    Limb::Calf(Side::Left),
    Limb::Thigh(Side::Right),
    Limb::Calf(Side::Right),
];

fn limb_label(limb: Limb) -> String {
    let side = match side_of(limb) {
        Some(Side::Left) => "left ",
        Some(Side::Right) => "right ",
        None => "",
    };
    let what = match limb {
        Limb::Pelvis => "hips",
        Limb::Chest => "chest",
        Limb::Head => "head",
        Limb::UpperArm(_) => "upper arm",
        Limb::ForeArm(_) => "forearm",
        Limb::Thigh(_) => "thigh",
        Limb::Calf(_) => "calf",
    };
    format!("{side}{what}")
}

/// True if `ancestor` is `bone` or above it.
fn is_above(rig: &Rig, ancestor: usize, mut bone: usize) -> bool {
    loop {
        if bone == ancestor {
            return true;
        }
        match rig.bones[bone].parent {
            Some(parent) if parent < bone => bone = parent,
            _ => return false,
        }
    }
}

/// The lowest bone above both `a` and `b` (or one of them).
pub(crate) fn common_ancestor(rig: &Rig, a: usize, b: usize) -> Option<usize> {
    let mut at = a;
    loop {
        if is_above(rig, at, b) {
            return Some(at);
        }
        at = rig.bones[at].parent.filter(|&parent| parent < at)?;
    }
}

/// Each limb's bone in `rig`: the named ones by name (the first in the
/// rig's order), the pelvis where the thighs meet and the chest where the
/// arms meet. An error names what's missing or out of place.
fn find_limbs(rig: &Rig) -> Result<HashMap<Limb, usize>, String> {
    let kinds: Vec<(String, Option<Side>)> =
        rig.bones.iter().map(|bone| bone_kind(&bone.name)).collect();
    let mut found = HashMap::new();
    let mut missing = Vec::new();
    for limb in LIMBS {
        let names = names_of(limb);
        if names.is_empty() {
            continue;
        }
        let side = side_of(limb);
        match kinds
            .iter()
            .position(|(base, s)| *s == side && names.contains(&base.as_str()))
        {
            Some(bone) => {
                found.insert(limb, bone);
            }
            None => missing.push(limb_label(limb)),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "its skeleton has no bone for the {} (bones are found by names such as Mixamo's `LeftArm` or `upper_arm.L`)",
            missing.join(", ")
        ));
    }
    let meet = |a: Limb, b: Limb| common_ancestor(rig, found[&a], found[&b]);
    let pelvis = meet(Limb::Thigh(Side::Left), Limb::Thigh(Side::Right))
        .ok_or("its thighs aren't in one skeleton")?;
    let chest = meet(Limb::UpperArm(Side::Left), Limb::UpperArm(Side::Right))
        .ok_or("its arms aren't in one skeleton")?;
    found.insert(Limb::Pelvis, pelvis);
    found.insert(Limb::Chest, chest);
    for limb in LIMBS {
        if let Some(parent) = parent_of(limb)
            && (found[&parent] == found[&limb] || !is_above(rig, found[&parent], found[&limb]))
        {
            return Err(format!(
                "its {} bone isn't below its {} bone",
                limb_label(limb),
                limb_label(parent)
            ));
        }
    }
    Ok(found)
}

/// Where a MeshPart's model's file space is in the world: its `Position`,
/// `Orientation` and `Size`, then the rig's fit into the 1×1×1 box.
pub(crate) fn placement(world: &World, part: InstanceId, rig: &Rig) -> Option<Mat4> {
    let instance = world.get(part)?;
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let size = vector("Size")?;
    let place = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, size.z),
        rotation_of(vector("Orientation")?),
        to_vec3(vector("Position")?),
    ) * rig.fit;
    place.is_finite().then_some(place)
}

pub(crate) fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn to_vector3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// The rotation an `Orientation` in degrees stands for (Z, then X, then Y).
pub(crate) fn rotation_of(orientation: Vector3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        orientation.y.to_radians(),
        orientation.x.to_radians(),
        orientation.z.to_radians(),
    )
}

/// The `Orientation` in degrees for a rotation.
fn orientation_of(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    Vector3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// A Part's or Attachment's `Position` and `Orientation` as one transform.
fn pose_of(world: &World, id: InstanceId) -> Option<Mat4> {
    let instance = world.get(id)?;
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let pose = Mat4::from_rotation_translation(
        rotation_of(vector("Orientation")?),
        to_vec3(vector("Position")?),
    );
    pose.is_finite().then_some(pose)
}

/// `m` without its scale: where it puts the origin and how it turns.
fn rigid(m: Mat4) -> Mat4 {
    let (_, rotation, translation) = m.to_scale_rotation_translation();
    Mat4::from_rotation_translation(rotation.normalize(), translation)
}

/// A turn whose X axis is `x` and whose Y axis is as near `toward` as it
/// can be (any other way when they're parallel).
fn frame(x: Vec3, toward: Vec3) -> Quat {
    let x = x.normalize_or(Vec3::X);
    let mut y = toward - x * x.dot(toward);
    if y.length_squared() < 1e-8 {
        y = x.any_orthonormal_vector();
    }
    let y = y.normalize();
    Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y))).normalize()
}

/// One body of a built ragdoll: its part, the bone it poses, and where that
/// bone is on it (from its [`BONE_ATTACHMENT`]).
#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub part: InstanceId,
    pub bone: usize,
    pub offset: Mat4,
}

/// The bodies of `ragdoll` for `rig`: every part in it named after one of
/// the rig's bones with a [`BONE_ATTACHMENT`] in it, in the rig's order.
pub(crate) fn bodies(world: &World, ragdoll: InstanceId, rig: &Rig) -> Vec<Body> {
    let mut found: Vec<Body> = world
        .descendants(ragdoll)
        .into_iter()
        .filter_map(|part| {
            let instance = world.get(part)?;
            if !instance.class().is_base_part() {
                return None;
            }
            let bone = rig.bone(instance.name())?;
            let attachment = world.find_first_child(part, BONE_ATTACHMENT)?;
            let offset = pose_of(world, attachment)?;
            Some(Body { part, bone, offset })
        })
        .collect();
    found.sort_by_key(|body| body.bone);
    found.dedup_by_key(|body| body.bone);
    found
}

/// The first Ragdoll in `part`, if there is one.
pub fn ragdoll_of(world: &World, part: InstanceId) -> Option<InstanceId> {
    world.get(part)?.children().iter().copied().find(|&id| {
        world
            .get(id)
            .is_some_and(|c| c.class() == ClassName::Ragdoll)
    })
}

/// The body of `ragdoll` for the bone `bone`: named exactly that, or that
/// without its namespace (`Head` finds `mixamorig:Head`).
pub fn body_of(world: &World, ragdoll: InstanceId, bone: &str) -> Option<InstanceId> {
    let parts: Vec<InstanceId> = world
        .descendants(ragdoll)
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .collect();
    let named = |exact: bool| {
        parts.iter().copied().find(|&id| {
            let name = world.get(id).map_or("", |i| i.name());
            if exact {
                name == bone
            } else {
                name.rsplit(':').next() == Some(bone)
            }
        })
    };
    named(true).or_else(|| named(false))
}

/// The pose physics gives the bones: each body's bone where the body has it,
/// and every other bone where `base` has it relative to its parent. Each
/// bone keeps the scale it has in `base`. `place` is the MeshPart's
/// [`placement`].
pub(crate) fn pose_from_bodies(
    world: &World,
    rig: &Rig,
    place: Mat4,
    base: &[BoneTransform],
    bodies: &[Body],
) -> Pose {
    let base_world = rig.world(base);
    let unplace = place.inverse();
    let mut driven: Vec<Option<Mat4>> = vec![None; rig.bones.len()];
    for body in bodies {
        let Some(at) = pose_of(world, body.part) else {
            continue;
        };
        let (scale, _, _) = base_world[body.bone].to_scale_rotation_translation();
        // The bone's turn and place in the world, back into the file's space.
        let bone = unplace * at * body.offset;
        let (_, rotation, translation) = bone.to_scale_rotation_translation();
        driven[body.bone] = Some(Mat4::from_scale_rotation_translation(
            scale,
            rotation.normalize(),
            translation,
        ));
    }
    let mut pose: Pose = rig
        .bones
        .iter()
        .enumerate()
        .map(|(i, bone)| base.get(i).copied().unwrap_or(bone.rest))
        .collect();
    let mut world_of: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
    for (i, bone) in rig.bones.iter().enumerate() {
        let parent = bone.parent.filter(|&p| p < i).map(|p| world_of[p]);
        let matrix = match (driven[i], parent) {
            (Some(matrix), parent) => {
                let local = parent.map_or(matrix, |p| p.inverse() * matrix);
                let (scale, rotation, translation) = local.to_scale_rotation_translation();
                if local.is_finite() {
                    pose[i] = BoneTransform {
                        translation,
                        rotation: rotation.normalize(),
                        scale,
                    };
                }
                matrix
            }
            (None, Some(parent)) => parent * pose[i].matrix(),
            (None, None) => base_world[i],
        };
        world_of.push(matrix);
    }
    pose
}

/// Where each body goes for its bone in `pose`: `(part, position,
/// orientation)`.
pub(crate) fn body_places(
    rig: &Rig,
    place: Mat4,
    pose: &[BoneTransform],
    bodies: &[Body],
) -> Vec<(InstanceId, Vec3, Quat)> {
    let world = rig.world(pose);
    bodies
        .iter()
        .filter_map(|body| {
            let bone = rigid(place * *world.get(body.bone)?);
            let at = bone * body.offset.inverse();
            let (_, rotation, translation) = at.to_scale_rotation_translation();
            at.is_finite()
                .then_some((body.part, translation, rotation.normalize()))
        })
        .collect()
}

/// Moves a body to `position` and `orientation`, giving it the speed it
/// moved at over `dt` seconds (none after a jump of more than `MAX_FOLLOW`
/// studs a second, such as a teleport), so a ragdoll turned on carries on
/// as its animation was moving.
pub(crate) fn move_body(
    world: &mut World,
    part: InstanceId,
    position: Vec3,
    rotation: Quat,
    dt: f64,
) {
    const MAX_FOLLOW: f32 = 200.0;
    let before = world
        .get(part)
        .and_then(|i| i.property("Position"))
        .and_then(Value::as_vector3)
        .map(to_vec3);
    let velocity = match before {
        Some(before) if dt > 0.0 => {
            let v = (position - before) / dt as f32;
            if v.is_finite() && v.length() <= MAX_FOLLOW {
                v
            } else {
                Vec3::ZERO
            }
        }
        _ => Vec3::ZERO,
    };
    let _ = world.set_property(part, "Position", Value::Vector3(to_vector3(position)));
    let _ = world.set_property(
        part,
        "Orientation",
        Value::Vector3(orientation_of(rotation)),
    );
    let _ = world.set_property(
        part,
        "AssemblyLinearVelocity",
        Value::Vector3(to_vector3(velocity)),
    );
    let _ = world.set_property(
        part,
        "AssemblyAngularVelocity",
        Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
    );
}

/// Makes `ragdoll`'s bodies and joints from the skeleton of the MeshPart
/// it's in, in its rest pose, replacing whatever was in it. Returns how many
/// bodies it made, or why it can't.
pub fn build(world: &mut World, assets: &Assets, ragdoll: InstanceId) -> Result<usize, String> {
    let part = world
        .get(ragdoll)
        .filter(|r| r.class() == ClassName::Ragdoll)
        .and_then(|r| r.parent())
        .filter(|&p| {
            world
                .get(p)
                .is_some_and(|p| p.class() == ClassName::MeshPart)
        })
        .ok_or("A Ragdoll builds from the skinned MeshPart it's in; put it in one first.")?;
    let mesh_id = world
        .get(part)
        .and_then(|p| p.property("MeshId"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let rig = assets
        .model(&mesh_id)
        .and_then(|model| model.rig.clone())
        .ok_or_else(|| format!("The MeshPart's model `{mesh_id}` has no skeleton."))?;
    let limbs = find_limbs(&rig).map_err(|problem| format!("Can't build a ragdoll: {problem}."))?;
    let place = placement(world, part, &rig).ok_or("The MeshPart's placement isn't a number.")?;
    let bone_world: Vec<Mat4> = rig
        .world(&rig.rest_pose())
        .iter()
        .map(|m| place * *m)
        .collect();
    let at = |limb: Limb| bone_world[limbs[&limb]].w_axis.truncate();
    let bone_at = |bone: usize| bone_world[bone].w_axis.truncate();

    // The character's axes: up the spine, to its left, and ahead.
    let up = (at(Limb::Chest) - at(Limb::Pelvis)).normalize_or(Vec3::Y);
    let left = at(Limb::Thigh(Side::Left)) - at(Limb::Thigh(Side::Right));
    let width = left.length().max(1e-3);
    let left = (left - up * up.dot(left)).normalize_or(Vec3::X);
    let ahead = left.cross(up);

    // Where each body ends: at the next body's bone, at its bone's first
    // child (the end of a hand, foot or head), or as far again as its parent.
    let first_child = |bone: usize| rig.bones.iter().position(|b| b.parent == Some(bone));
    let tip = |limb: Limb| -> Vec3 {
        let next = match limb {
            Limb::Pelvis => Some(at(Limb::Chest)),
            Limb::Chest => {
                // The neck: the bone above the head, if it's below the chest.
                let head = limbs[&Limb::Head];
                rig.bones[head]
                    .parent
                    .filter(|&neck| neck != limbs[&Limb::Chest])
                    .filter(|&neck| is_above(&rig, limbs[&Limb::Chest], neck))
                    .map(bone_at)
                    .or(Some(at(Limb::Head)))
            }
            Limb::UpperArm(side) => Some(at(Limb::ForeArm(side))),
            Limb::Thigh(side) => Some(at(Limb::Calf(side))),
            Limb::Head | Limb::ForeArm(_) | Limb::Calf(_) => first_child(limbs[&limb]).map(bone_at),
        };
        let start = at(limb);
        match next.filter(|end| end.distance(start) > width * 0.05) {
            Some(end) => end,
            None => {
                let parent = parent_of(limb).map_or(start - up * width, at);
                start + (start - parent)
            }
        }
    };
    // Radii, by the width of the hips.
    let radius = |limb: Limb, length: f32| match limb {
        Limb::Pelvis => 0.6 * width,
        Limb::Chest => 0.7 * width,
        Limb::Head => (0.5 * length).max(0.45 * width),
        Limb::UpperArm(_) => 0.3 * width,
        Limb::ForeArm(_) => 0.25 * width,
        Limb::Thigh(_) => 0.4 * width,
        Limb::Calf(_) => 0.3 * width,
    };

    for child in world
        .get(ragdoll)
        .map(|r| r.children().to_vec())
        .unwrap_or_default()
    {
        let _ = world.destroy(child);
    }
    let make = |world: &mut World, class, name: &str, parent, properties: Vec<(&str, Value)>| {
        let id = world.create(class).expect("not a service");
        world.set_name(id, name).expect("just made");
        for (property, value) in properties {
            world
                .set_property(id, property, value)
                .expect("the class has it");
        }
        world.set_parent(id, Some(parent)).expect("a new Instance");
        id
    };
    let local = |body: Mat4, rotation: Quat, position: Vec3| {
        let relative = body.inverse() * Mat4::from_rotation_translation(rotation, position);
        let (_, rotation, translation) = relative.to_scale_rotation_translation();
        vec![
            ("Position", Value::Vector3(to_vector3(translation))),
            (
                "Orientation",
                Value::Vector3(orientation_of(rotation.normalize())),
            ),
        ]
    };
    let mut made: HashMap<Limb, (InstanceId, Mat4)> = HashMap::new();
    for limb in LIMBS {
        let bone = limbs[&limb];
        let start = at(limb);
        let end = tip(limb);
        let along = end - start;
        let length = along.length();
        let r = radius(limb, length);
        let rotation = frame(along, ahead);
        let middle = (start + end) / 2.0;
        let body_pose = Mat4::from_rotation_translation(rotation, middle);
        let name = rig.bones[bone].name.clone();
        let body = make(
            world,
            ClassName::Part,
            &name,
            ragdoll,
            vec![
                ("Shape", Value::PartType(PartType::Capsule)),
                (
                    "Size",
                    Value::Vector3(Vector3::new(length + r, 2.0 * r, 2.0 * r)),
                ),
                ("Position", Value::Vector3(to_vector3(middle))),
                ("Orientation", Value::Vector3(orientation_of(rotation))),
                ("Anchored", Value::Bool(false)),
                // Limbs don't bounce, and grip what they land on.
                (
                    "CustomPhysicalProperties",
                    Value::PhysicalProperties(Some(BODY_PHYSICS)),
                ),
                ("Transparency", Value::Number(1.0)),
                ("CastShadow", Value::Bool(false)),
            ],
        );
        let (_, bone_rotation, _) = bone_world[bone].to_scale_rotation_translation();
        make(
            world,
            ClassName::Attachment,
            BONE_ATTACHMENT,
            body,
            local(body_pose, bone_rotation.normalize(), start),
        );
        if let (Some(parent), Some(preset)) = (parent_of(limb), joint_of(limb)) {
            let (parent_body, parent_pose) = made[&parent];
            let name = match side_of(limb) {
                Some(Side::Left) => format!("Left{}", preset.name),
                Some(Side::Right) => format!("Right{}", preset.name),
                None => preset.name.to_owned(),
            };
            let down = along.normalize_or(-up);
            // A hinge's axis: elbows bend the forearm ahead, knees the calf
            // back, so a positive angle bends it.
            let joint_rotation = match preset.kind {
                JointKind::Ball => frame(down, ahead),
                JointKind::Hinge => {
                    let bend = if matches!(limb, Limb::Calf(_)) {
                        -ahead
                    } else {
                        ahead
                    };
                    frame(down.cross(bend), down)
                }
            };
            // The parent's frame leans the cone's middle: arms down, legs
            // ahead.
            let lean_toward = if matches!(limb, Limb::UpperArm(_)) {
                -up
            } else {
                ahead
            };
            let lean_axis = down.cross(lean_toward).normalize_or_zero();
            let leaned = Quat::from_axis_angle(lean_axis, preset.lean.to_radians()) * down;
            let parent_rotation = match preset.kind {
                JointKind::Ball if preset.lean != 0.0 => frame(leaned, ahead),
                _ => joint_rotation,
            };
            let attachment = format!("{name}Attachment");
            let a0 = make(
                world,
                ClassName::Attachment,
                &attachment,
                parent_body,
                local(parent_pose, parent_rotation, start),
            );
            let a1 = make(
                world,
                ClassName::Attachment,
                &attachment,
                body,
                local(body_pose, joint_rotation, start),
            );
            let (lower, upper) = (
                Value::Number(f64::from(preset.range.0)),
                Value::Number(f64::from(preset.range.1)),
            );
            let mut properties = vec![
                ("Attachment0", Value::Instance(Some(a0))),
                ("Attachment1", Value::Instance(Some(a1))),
                ("LimitsEnabled", Value::Bool(true)),
            ];
            let class = match preset.kind {
                JointKind::Ball => {
                    properties.extend([
                        ("UpperAngle", Value::Number(f64::from(preset.cone))),
                        ("TwistLimitsEnabled", Value::Bool(true)),
                        ("TwistLowerAngle", lower),
                        ("TwistUpperAngle", upper),
                    ]);
                    ClassName::BallSocketConstraint
                }
                JointKind::Hinge => {
                    properties.extend([("LowerAngle", lower), ("UpperAngle", upper)]);
                    ClassName::HingeConstraint
                }
            };
            make(world, class, &name, body, properties);
        }
        made.insert(limb, (body, body_pose));
    }
    Ok(made.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bones_are_found_by_mixamo_and_blender_names() {
        assert_eq!(
            bone_kind("mixamorig:LeftForeArm"),
            ("forearm".into(), Some(Side::Left))
        );
        assert_eq!(
            bone_kind("upper_arm.R"),
            ("upperarm".into(), Some(Side::Right))
        );
        assert_eq!(bone_kind("thigh_L"), ("thigh".into(), Some(Side::Left)));
        assert_eq!(bone_kind("mixamorig:Head"), ("head".into(), None));
        assert_eq!(bone_kind("Leg"), ("leg".into(), None));
        assert_eq!(
            bone_kind("mixamorig:RightHand"),
            ("hand".into(), Some(Side::Right))
        );
        assert_eq!(bone_kind("hand.R"), ("hand".into(), Some(Side::Right)));
    }
}
