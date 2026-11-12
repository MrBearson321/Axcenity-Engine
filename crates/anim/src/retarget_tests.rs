use std::sync::Arc;

use axcenity_assets::{Bone, BoneTransform, Keys, Take, Track};
use glam::{Mat4, Quat, Vec3};

use super::*;
use crate::Clip;
use crate::files::ClipFile;

/// A humanoid's bones: name, parent and where it rests in the world (a
/// T-pose facing +Z, Y up, left along +X, hips 1 high).
const BODY: [(&str, Option<usize>, [f32; 3]); 22] = [
    ("Hips", None, [0.0, 1.0, 0.0]),
    ("Spine", Some(0), [0.0, 1.1, 0.0]),
    ("Spine1", Some(1), [0.0, 1.25, 0.0]),
    ("Spine2", Some(2), [0.0, 1.4, 0.0]),
    ("Neck", Some(3), [0.0, 1.55, 0.0]),
    ("Head", Some(4), [0.0, 1.65, 0.0]),
    ("LeftShoulder", Some(3), [0.05, 1.5, 0.0]),
    ("LeftArm", Some(6), [0.2, 1.5, 0.0]),
    ("LeftForeArm", Some(7), [0.45, 1.5, 0.0]),
    ("LeftHand", Some(8), [0.7, 1.5, 0.0]),
    ("RightShoulder", Some(3), [-0.05, 1.5, 0.0]),
    ("RightArm", Some(10), [-0.2, 1.5, 0.0]),
    ("RightForeArm", Some(11), [-0.45, 1.5, 0.0]),
    ("RightHand", Some(12), [-0.7, 1.5, 0.0]),
    ("LeftUpLeg", Some(0), [0.1, 0.95, 0.0]),
    ("LeftLeg", Some(14), [0.1, 0.5, 0.0]),
    ("LeftFoot", Some(15), [0.1, 0.08, 0.0]),
    ("LeftToeBase", Some(16), [0.1, 0.0, 0.12]),
    ("RightUpLeg", Some(0), [-0.1, 0.95, 0.0]),
    ("RightLeg", Some(18), [-0.1, 0.5, 0.0]),
    ("RightFoot", Some(19), [-0.1, 0.08, 0.0]),
    ("RightToeBase", Some(20), [-0.1, 0.0, 0.12]),
];

/// Unreal's names for [`BODY`]'s bones, in the same order.
const UNREAL: [&str; 22] = [
    "pelvis",
    "spine_01",
    "spine_02",
    "spine_03",
    "neck_01",
    "head",
    "clavicle_l",
    "upperarm_l",
    "lowerarm_l",
    "hand_l",
    "clavicle_r",
    "upperarm_r",
    "lowerarm_r",
    "hand_r",
    "thigh_l",
    "calf_l",
    "foot_l",
    "ball_l",
    "thigh_r",
    "calf_r",
    "foot_r",
    "ball_r",
];

/// Bones named `names`, resting where `world` puts each (positions and
/// turns in the world), their local rests worked out from their parents'.
fn skeleton(names: &[String], world: &[(Vec3, Quat)]) -> Vec<Bone> {
    let matrices: Vec<Mat4> = world
        .iter()
        .map(|&(at, turn)| Mat4::from_rotation_translation(turn, at))
        .collect();
    BODY.iter()
        .enumerate()
        .map(|(index, &(_, parent, _))| {
            let local = match parent {
                Some(parent) => matrices[parent].inverse() * matrices[index],
                None => matrices[index],
            };
            let (scale, rotation, translation) = local.to_scale_rotation_translation();
            Bone {
                name: names[index].clone(),
                parent,
                rest: BoneTransform {
                    translation,
                    rotation,
                    scale,
                },
            }
        })
        .collect()
}

fn mixamo() -> Vec<Bone> {
    let names: Vec<String> = BODY
        .iter()
        .map(|(name, ..)| format!("mixamorig:{name}"))
        .collect();
    let world: Vec<(Vec3, Quat)> = BODY
        .iter()
        .map(|&(_, _, at)| (Vec3::from_array(at), Quat::IDENTITY))
        .collect();
    skeleton(&names, &world)
}

/// [`BODY`] 1.2 times as big, Z up instead of Y (turned by `world_turn`),
/// and every bone's own axes turned some other way.
fn unreal(world_turn: Quat) -> Rig {
    let names: Vec<String> = UNREAL.iter().map(|name| (*name).to_owned()).collect();
    let world: Vec<(Vec3, Quat)> = BODY
        .iter()
        .enumerate()
        .map(|(index, &(_, _, at))| {
            let own = Quat::from_euler(glam::EulerRot::XYZ, 0.3 * index as f32, 0.7, -0.2);
            (world_turn * (Vec3::from_array(at) * 1.2), world_turn * own)
        })
        .collect();
    Rig {
        bones: skeleton(&names, &world),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    }
}

/// A take on [`mixamo`]'s skeleton: the left forearm turns 1.2 radians
/// about Y (its own axes are the world's) and the hips go 0.5 forward.
fn walk() -> Take {
    let skeleton = Arc::new(mixamo());
    let forearm = skeleton[8].rest;
    let hips = skeleton[0].rest;
    Take {
        name: "Walk".into(),
        duration: 1.0,
        frame_rate: 30.0,
        tracks: vec![
            Track {
                bone: "mixamorig:LeftForeArm".into(),
                rotation: Keys {
                    times: vec![0.0, 1.0],
                    values: vec![forearm.rotation, Quat::from_rotation_y(1.2)],
                },
                ..Track::default()
            },
            Track {
                bone: "mixamorig:Hips".into(),
                translation: Keys {
                    times: vec![0.0, 1.0],
                    values: vec![hips.translation, hips.translation + Vec3::Z * 0.5],
                },
                ..Track::default()
            },
        ],
        skeleton,
    }
}

fn world_position(rig: &Rig, pose: &Pose, bone: usize) -> Vec3 {
    rig.world(pose)[bone].w_axis.truncate()
}

#[test]
fn bone_names_from_common_tools_are_matched_to_roles() {
    let map = HumanoidMap::detect(&mixamo());
    assert!(map.missing().is_empty(), "{:?}", map.missing());
    assert_eq!(map.get(HumanBone::LeftLowerArm), Some(8));
    assert_eq!(map.get(HumanBone::RightToes), Some(21));
    assert_eq!(
        [HumanBone::Spine, HumanBone::Chest, HumanBone::UpperChest].map(|role| map.get(role)),
        [Some(1), Some(2), Some(3)]
    );

    let map = HumanoidMap::detect(&unreal(Quat::IDENTITY).bones);
    assert!(map.missing().is_empty(), "{:?}", map.missing());
    assert_eq!(map.get(HumanBone::LeftUpperLeg), Some(14));
    assert_eq!(map.get(HumanBone::LeftToes), Some(17));

    for (name, role) in [
        ("DEF-forearm.L", HumanBone::LeftLowerArm),
        ("DEF-shin.R", HumanBone::RightLowerLeg),
        ("Bip01 L UpperArm", HumanBone::LeftUpperArm),
        ("Bip01 R Calf", HumanBone::RightLowerLeg),
        ("upper_arm.L", HumanBone::LeftUpperArm),
        ("Thigh_R", HumanBone::RightUpperLeg),
        ("Armature|LeftHand", HumanBone::LeftHand),
    ] {
        let bones = [Bone {
            name: name.into(),
            parent: None,
            rest: BoneTransform::IDENTITY,
        }];
        assert_eq!(HumanoidMap::detect(&bones).get(role), Some(0), "{name}");
    }
    // Fingers, end bones and unsided limbs play no role.
    for name in [
        "mixamorig:LeftHandIndex1",
        "mixamorig:HeadTop_End",
        "UpperArm",
    ] {
        let bones = [Bone {
            name: name.into(),
            parent: None,
            rest: BoneTransform::IDENTITY,
        }];
        assert_eq!(
            HumanoidMap::detect(&bones),
            HumanoidMap::default(),
            "{name}"
        );
    }
}

#[test]
fn a_take_plays_on_a_skeleton_with_other_names_axes_and_proportions() {
    let take = walk();
    let source = Rig {
        bones: take.skeleton.to_vec(),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    };
    // Z up: the target's world is the source's turned -90° about X.
    let world_turn = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let rig = unreal(world_turn);
    let retarget = Retarget::new(&take, &rig).unwrap();
    assert_eq!(retarget.targets().collect::<Vec<_>>(), [0, 8]);

    let mut source_pose = source.rest_pose();
    take.sample(1.0, &take.bind(&source), &source, &mut source_pose);
    let mut pose = rig.rest_pose();
    retarget.sample(&take, 1.0, &rig, &mut pose);

    // The forearm points the way the source's does, in the target's world.
    let direction = |rig: &Rig, pose: &Pose| {
        (world_position(rig, pose, 9) - world_position(rig, pose, 8)).normalize()
    };
    let wanted = world_turn * direction(&source, &source_pose);
    let got = direction(&rig, &pose);
    assert!(got.abs_diff_eq(wanted, 1e-4), "{got} != {wanted}");
    // It really turned (not the rest).
    assert!(!got.abs_diff_eq(direction(&rig, &rig.rest_pose()), 0.1));

    // The hips travel 0.5 forward, scaled with the bigger body.
    let hips = world_position(&rig, &pose, 0);
    let expected = world_turn * (Vec3::new(0.0, 1.0, 0.5) * 1.2);
    assert!(hips.abs_diff_eq(expected, 1e-4), "{hips} != {expected}");

    // Bones the take doesn't move keep what the pose had.
    let rest = rig.rest_pose();
    for bone in [5, 12, 19] {
        assert_eq!(pose[bone], rest[bone]);
    }
    // The upper arm keeps its rest turn, so the elbow only travels with the
    // hips.
    let elbow = |pose: &Pose| world_position(&rig, pose, 8) - world_position(&rig, pose, 0);
    assert!(elbow(&pose).abs_diff_eq(elbow(&rest), 1e-4));
}

#[test]
fn a_clip_linked_to_another_skeletons_take_binds_through_roles_or_says_why_not() {
    let takes = Arc::new(vec![walk()]);
    let mut file = ClipFile::linked("assets/anims/walk.fbx", "Walk", 0.0, 1.0);
    // Held at its end, so sampling at 1 is the take's last frame.
    file.looping = false;
    let rig = unreal(Quat::IDENTITY);
    let clip = Clip::bind(file.clone(), Some(takes.clone()), Some(&rig)).unwrap();
    let animated: Vec<usize> = (0..rig.bones.len())
        .filter(|&bone| clip.animated_bones()[bone])
        .collect();
    assert_eq!(animated, [0, 8]);
    let mut pose = rig.rest_pose();
    clip.sample(1.0, &rig, &mut pose);
    assert_ne!(pose[8], rig.bones[8].rest, "the forearm turned");

    // A skeleton that isn't a humanoid's by its names: refused, saying why.
    let mut other = unreal(Quat::IDENTITY);
    for bone in &mut other.bones[6..10] {
        bone.name = format!("wing_{}", bone.name);
    }
    let error = Clip::bind(file, Some(takes), Some(&other)).unwrap_err();
    assert!(error.contains("can't be retargeted"), "{error}");
    assert!(
        error.contains("LeftUpperArm, LeftLowerArm, LeftHand"),
        "{error}"
    );
}

#[test]
fn a_take_for_the_same_skeleton_still_binds_by_name() {
    let take = walk();
    let rig = Rig {
        bones: take.skeleton.to_vec(),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    };
    let mut file = ClipFile::linked("assets/anims/walk.fbx", "Walk", 0.0, 1.0);
    // Held at its end, so sampling at 1 is the take's last frame.
    file.looping = false;
    let clip = Clip::bind(file, Some(Arc::new(vec![take.clone()])), Some(&rig)).unwrap();
    let mut pose = rig.rest_pose();
    clip.sample(1.0, &rig, &mut pose);
    // By name, the track's own values land as they are.
    assert!(
        pose[8]
            .rotation
            .abs_diff_eq(Quat::from_rotation_y(1.2), 1e-5)
    );
}
