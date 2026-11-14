use std::sync::Arc;

use axcenity_assets::{Bone, BoneTransform, Keys, RigDefinition, RigDefinitions, Take, Track};
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
    // End bones, metacarpals and unsided limbs play no role.
    for name in [
        "mixamorig:LeftHandIndex4",
        "mixamorig:HeadTop_End",
        "index_metacarpal_l",
        "Bip01 L Finger0Nub",
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
    let retarget = Retarget::new(&take, &rig, &RigDefinitions::default()).unwrap();
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

fn one_bone(name: &str) -> [Bone; 1] {
    [Bone {
        name: name.into(),
        parent: None,
        rest: BoneTransform::IDENTITY,
    }]
}

#[test]
fn finger_bone_names_from_common_tools_are_matched_to_finger_roles() {
    use HumanBone::Finger;
    let (l, r) = (Side::Left, Side::Right);
    for (name, role) in [
        (
            "mixamorig:LeftHandIndex1",
            Finger(l, Digit::Index, Segment::Proximal),
        ),
        (
            "mixamorig:RightHandPinky3",
            Finger(r, Digit::Little, Segment::Distal),
        ),
        (
            "mixamorig:LeftHandThumb2",
            Finger(l, Digit::Thumb, Segment::Intermediate),
        ),
        ("index_01_l", Finger(l, Digit::Index, Segment::Proximal)),
        ("middle_03_r", Finger(r, Digit::Middle, Segment::Distal)),
        ("ring_02_l", Finger(l, Digit::Ring, Segment::Intermediate)),
        (
            "DEF-f_pinky.01.R",
            Finger(r, Digit::Little, Segment::Proximal),
        ),
        ("thumb.03.L", Finger(l, Digit::Thumb, Segment::Distal)),
        (
            "LeftLittleIntermediate",
            Finger(l, Digit::Little, Segment::Intermediate),
        ),
        (
            "J_Bip_R_Middle1",
            Finger(r, Digit::Middle, Segment::Proximal),
        ),
        (
            "Bip01 L Finger0",
            Finger(l, Digit::Thumb, Segment::Proximal),
        ),
        ("Bip01 R Finger12", Finger(r, Digit::Index, Segment::Distal)),
        (
            "Bip01 L Finger41",
            Finger(l, Digit::Little, Segment::Intermediate),
        ),
        ("lMid1", Finger(l, Digit::Middle, Segment::Proximal)),
        ("rThumb3", Finger(r, Digit::Thumb, Segment::Distal)),
    ] {
        assert_eq!(
            HumanoidMap::detect(&one_bone(name)).get(role),
            Some(0),
            "{name}"
        );
    }
    // DAZ's glued sides work for limbs too, and don't eat real names.
    for (name, role) in [
        ("lForearmBend", HumanBone::LeftLowerArm),
        ("rShin", HumanBone::RightLowerLeg),
        ("lShldrBend", HumanBone::LeftUpperArm),
        ("leg_l", HumanBone::LeftLowerLeg),
    ] {
        assert_eq!(
            HumanoidMap::detect(&one_bone(name)).get(role),
            Some(0),
            "{name}"
        );
    }
}

#[test]
fn every_role_has_a_name_that_reads_back_and_a_place() {
    let all: Vec<HumanBone> = HumanBone::all().collect();
    assert_eq!(all.len(), ROLE_COUNT);
    for (index, role) in all.iter().enumerate() {
        assert_eq!(role.index(), index);
        assert_eq!(HumanBone::from_name(role.name()), Some(*role));
        assert_eq!(role.mirrored().mirrored(), *role);
    }
    assert_eq!(
        HumanBone::from_name("lefthand"),
        Some(HumanBone::LeftHand),
        "any case"
    );
    assert_eq!(HumanBone::fingers(Side::Right).count(), 15);
}

/// [`BODY`] with odd names a rig definition maps, and an extra spine bone
/// names alone would call the Chest.
fn odd_skeleton() -> (Vec<Bone>, RigDefinition) {
    let names: Vec<String> = (0..BODY.len()).map(|i| format!("b{i:02}")).collect();
    let world: Vec<(Vec3, Quat)> = BODY
        .iter()
        .map(|&(_, _, at)| (Vec3::from_array(at), Quat::IDENTITY))
        .collect();
    let mut bones = skeleton(&names, &world);
    // `b02` looks like nothing; call it a spine so names alone pick it.
    bones[2].name = "spine_extra".into();
    let mut definition = RigDefinition::new();
    for (index, &(name, ..)) in BODY.iter().enumerate() {
        let role = HumanoidMap::detect(&one_bone(name))
            .roles()
            .next()
            .map(|(role, _)| role);
        if let Some(role) = role {
            definition
                .bones
                .insert(role.name().into(), bones[index].name.clone());
        }
    }
    // Spine1 isn't a role on its own; Chest is set to none.
    definition.bones.insert("Chest".into(), String::new());
    (bones, definition)
}

#[test]
fn a_rig_definition_maps_bones_names_cant_and_wins_over_names() {
    let (bones, definition) = odd_skeleton();
    assert!(!HumanoidMap::detect(&bones).missing().is_empty());
    let map = HumanoidMap::with_definition(&bones, &definition);
    assert!(map.missing().is_empty(), "{:?}", map.missing());
    assert_eq!(map.get(HumanBone::LeftLowerArm), Some(8));
    // Set to none: the name match `spine_extra` doesn't take Chest back.
    assert_eq!(map.get(HumanBone::Chest), None);

    // The project's definition is picked because it fits the skeleton.
    let rigs = RigDefinitions {
        definitions: vec![("assets/rigs/odd.rig.json".into(), definition.clone())],
        problems: Vec::new(),
    };
    let (found, path) = HumanoidMap::for_skeleton(&bones, &rigs);
    assert_eq!(found, map);
    assert_eq!(path.as_deref(), Some("assets/rigs/odd.rig.json"));
    // A skeleton it doesn't fit is matched by names.
    let (by_name, path) = HumanoidMap::for_skeleton(&mixamo(), &rigs);
    assert_eq!(by_name, HumanoidMap::detect(&mixamo()));
    assert!(path.is_none());

    // A definition round-trips a map.
    assert_eq!(
        HumanoidMap::with_definition(&bones, &map.to_definition(&bones)),
        map
    );
}

#[test]
fn a_take_retargets_onto_a_skeleton_only_its_rig_definition_maps() {
    let (bones, definition) = odd_skeleton();
    let rig = Rig {
        bones,
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    };
    let mut file = ClipFile::linked("assets/anims/walk.fbx", "Walk", 0.0, 1.0);
    file.looping = false;
    let takes = Arc::new(vec![walk()]);
    let error = Clip::bind(file.clone(), Some(takes.clone()), Some(&rig)).unwrap_err();
    assert!(error.contains("can't be retargeted"), "{error}");
    let rigs = RigDefinitions {
        definitions: vec![("assets/rigs/odd.rig.json".into(), definition)],
        problems: Vec::new(),
    };
    let clip = Clip::bind_with(file, Some(takes), Some(&rig), &rigs).unwrap();
    let mut pose = rig.rest_pose();
    clip.sample(1.0, &rig, &mut pose);
    assert_ne!(pose[8], rig.bones[8].rest, "the forearm turned");
}

/// [`BODY`] plus a hand's index finger, three bones along +X from the left
/// hand, named Mixamo's way.
fn with_fingers(mut bones: Vec<Bone>, names: [&str; 3]) -> Vec<Bone> {
    let hand = 9;
    for (i, name) in names.into_iter().enumerate() {
        bones.push(Bone {
            name: name.into(),
            parent: Some(if i == 0 { hand } else { bones.len() - 1 }),
            rest: BoneTransform {
                translation: Vec3::X * 0.04,
                ..BoneTransform::IDENTITY
            },
        });
    }
    bones
}

#[test]
fn fingers_retarget_like_any_other_bone() {
    let source = with_fingers(
        mixamo(),
        [
            "mixamorig:LeftHandIndex1",
            "mixamorig:LeftHandIndex2",
            "mixamorig:LeftHandIndex3",
        ],
    );
    let names: Vec<String> = UNREAL.iter().map(|name| (*name).to_owned()).collect();
    let world: Vec<(Vec3, Quat)> = BODY
        .iter()
        .map(|&(_, _, at)| (Vec3::from_array(at), Quat::IDENTITY))
        .collect();
    let target = Rig {
        bones: with_fingers(
            skeleton(&names, &world),
            ["index_01_l", "index_02_l", "index_03_l"],
        ),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    };
    // The take curls the first index bone 1 radian about Z.
    let curl = Quat::from_rotation_z(-1.0);
    let take = Take {
        name: "Grip".into(),
        duration: 1.0,
        frame_rate: 30.0,
        tracks: vec![Track {
            bone: "mixamorig:LeftHandIndex1".into(),
            rotation: Keys {
                times: vec![0.0],
                values: vec![curl],
            },
            ..Track::default()
        }],
        skeleton: Arc::new(source),
    };
    let retarget = Retarget::new(&take, &target, &RigDefinitions::default()).unwrap();
    let mut pose = target.rest_pose();
    retarget.sample(&take, 0.0, &target, &mut pose);
    assert!(pose[22].rotation.abs_diff_eq(curl, 1e-5), "{:?}", pose[22]);
    // The tip follows, bent down.
    let tip = world_position(&target, &pose, 24) - world_position(&target, &pose, 22);
    assert!(tip.y < -0.05, "{tip}");
}

#[test]
fn checking_a_skeleton_finds_missing_swapped_and_misplaced_roles() {
    let rigs = RigDefinitions::default();
    let report = check_rig(&mixamo(), None, &rigs);
    assert!(report.usable(), "{:?}", report.problems);
    let notes: Vec<&str> = report.problems.iter().map(|p| p.message.as_str()).collect();
    assert!(notes.iter().any(|n| n.contains("T-pose")), "{notes:?}");
    assert!(
        notes.iter().any(|n| n.contains("left hand has no finger")),
        "{notes:?}"
    );
    assert!(
        report.problems.iter().all(|p| p.severity == Severity::Note),
        "{:?}",
        report.problems
    );

    // Missing roles are an error naming them.
    let mut no_arm = mixamo();
    no_arm[8].name = "elbow_thing_x".into();
    let report = check_rig(&no_arm, None, &rigs);
    assert!(!report.usable());
    assert!(report.problems[0].message.contains("LeftLowerArm"));

    // Left and right swapped by a definition.
    let mut swap = HumanoidMap::detect(&mixamo()).to_definition(&mixamo());
    for (left, right) in [
        ("LeftUpperLeg", "RightUpperLeg"),
        ("LeftLowerLeg", "RightLowerLeg"),
        ("LeftFoot", "RightFoot"),
        ("LeftToes", "RightToes"),
    ] {
        let (a, b) = (swap.bones[left].clone(), swap.bones[right].clone());
        swap.bones.insert(left.into(), b);
        swap.bones.insert(right.into(), a);
    }
    let report = check_rig(&mixamo(), Some(&swap), &rigs);
    assert!(
        report
            .problems
            .iter()
            .any(|p| p.message.contains("look swapped")),
        "{:?}",
        report.problems
    );

    // A hand mapped to a bone outside its arm, and one bone in two roles.
    let mut wrong = HumanoidMap::detect(&mixamo()).to_definition(&mixamo());
    wrong
        .bones
        .insert("LeftHand".into(), "mixamorig:RightHand".into());
    wrong.bones.insert("Nonsense".into(), "x".into());
    let report = check_rig(&mixamo(), Some(&wrong), &rigs);
    let messages: Vec<&str> = report.problems.iter().map(|p| p.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("isn't below")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("plays both")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("`Nonsense`")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("doesn't have")),
        "{messages:?}"
    );
}

#[test]
fn comparing_skeletons_finds_different_rest_poses_and_proportions() {
    let rigs = RigDefinitions::default();
    // An A-pose: the arms turned 45° down from the shoulders.
    let names: Vec<String> = BODY.iter().map(|(name, ..)| (*name).to_owned()).collect();
    let down = |side: f32| Quat::from_rotation_z(-side * std::f32::consts::FRAC_PI_4);
    let world: Vec<(Vec3, Quat)> = BODY
        .iter()
        .enumerate()
        .map(|(index, &(_, _, at))| {
            let at = Vec3::from_array(at);
            let (shoulder, side) = match index {
                7..=9 => (Vec3::from_array(BODY[7].2), 1.0),
                11..=13 => (Vec3::from_array(BODY[11].2), -1.0),
                _ => return (at * 1.5, Quat::IDENTITY),
            };
            (
                (shoulder + down(side) * (at - shoulder)) * 1.5,
                Quat::IDENTITY,
            )
        })
        .collect();
    let a_pose = skeleton(&names, &world);
    let angle = rest_arm_angle(&a_pose, &HumanoidMap::detect(&a_pose)).unwrap();
    assert!((angle - 45.0).abs() < 0.5, "{angle}");
    let problems = check_pair(&mixamo(), &a_pose, &rigs);
    let messages: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
    assert!(messages[0].contains("off by 45°"), "{messages:?}");
    assert!(
        messages.iter().any(|m| m.contains("1.50 times")),
        "{messages:?}"
    );
    assert!(check_pair(&mixamo(), &mixamo(), &rigs).is_empty());
}
