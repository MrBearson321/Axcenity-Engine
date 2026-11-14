use std::f32::consts::FRAC_PI_2;

use axcenity_assets::{BoneTransform, Rig, RigDefinitions};
use glam::{Quat, Vec3};

use super::*;
use crate::files::{ClipEvent, ClipFile, Interp, PropertyKeyframe, PropertyName, PropertyTrack};
use crate::property_clip::sample_track;

fn track(id: &str, property: PropertyName, keys: &[(f64, Vec<f32>)]) -> PropertyTrack {
    PropertyTrack {
        id: id.into(),
        target: "Part".into(),
        property,
        keys: keys
            .iter()
            .enumerate()
            .map(|(n, (time, value))| {
                PropertyKeyframe::new(format!("k{}", n + 1), *time, value.clone())
            })
            .collect(),
    }
}

fn numbers(keys: &[(f64, f32)]) -> Vec<(f64, Vec<f32>)> {
    keys.iter().map(|&(t, v)| (t, vec![v])).collect()
}

fn at(track: &PropertyTrack, time: f64) -> f32 {
    sample_track(track, time).unwrap()[0]
}

#[test]
fn retiming_a_range_spreads_its_keys_and_moves_the_rest() {
    let mut file = ClipFile::new(3.0);
    file.tracks.push(track(
        "t",
        PropertyName::Transparency,
        &numbers(&[(0.5, 0.0), (1.0, 0.2), (1.5, 0.4), (2.0, 0.6), (3.0, 1.0)]),
    ));
    file.events.push(ClipEvent {
        time: 1.5,
        name: "Step".into(),
    });
    file.events.push(ClipEvent {
        time: 2.5,
        name: "Land".into(),
    });
    retime(&mut file, 1.0, 2.0, 2.0).unwrap();
    let times: Vec<f64> = file.tracks[0].keys.iter().map(|k| k.time).collect();
    assert_eq!(times, [0.5, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(file.events[0].time, 2.0);
    assert_eq!(file.events[1].time, 3.5);
    assert_eq!(file.duration, 4.0);

    // Squeezed back.
    retime(&mut file, 1.0, 3.0, 1.0).unwrap();
    let times: Vec<f64> = file.tracks[0].keys.iter().map(|k| k.time).collect();
    assert_eq!(times, [0.5, 1.0, 1.5, 2.0, 3.0]);
    assert_eq!(file.duration, 3.0);

    assert!(retime(&mut file, 2.0, 1.0, 1.0).is_err(), "backwards");
    assert!(retime(&mut file, 0.0, 5.0, 1.0).is_err(), "past the end");
    assert!(retime(&mut file, 0.0, 1.0, 0.0).is_err(), "no length");
    let mut linked = ClipFile::linked("a.fbx", "Walk", 0.0, 1.0);
    assert!(retime(&mut linked, 0.0, 0.5, 1.0).is_err(), "a take");
}

#[test]
fn smoothing_softens_a_spike_and_keeps_the_ends() {
    let mut file = ClipFile::new(4.0);
    file.tracks.push(track(
        "t",
        PropertyName::Transparency,
        &numbers(&[(0.0, 0.0), (1.0, 0.0), (2.0, 1.0), (3.0, 0.0), (4.0, 0.0)]),
    ));
    file.tracks.push(track(
        "other",
        PropertyName::TextSize,
        &numbers(&[(0.0, 0.0), (1.0, 10.0), (2.0, 0.0)]),
    ));
    let moved = smooth(&mut file, Some(&["t".to_owned()]), 1);
    assert_eq!(moved, 3);
    let values: Vec<f32> = file.tracks[0].keys.iter().map(|k| k.value[0]).collect();
    assert_eq!(values, [0.0, 0.25, 0.5, 0.25, 0.0]);
    assert_eq!(file.tracks[1].keys[1].value, [10.0], "not chosen");
    smooth(&mut file, None, 3);
    assert!(file.tracks[0].keys[2].value[0] < 0.5);
    assert!(file.tracks[1].keys[1].value[0] < 10.0);
}

#[test]
fn reducing_keys_drops_what_the_others_make_and_keeps_the_shape() {
    let mut file = ClipFile::new(2.0);
    // A straight ramp: only its ends matter.
    let ramp: Vec<(f64, f32)> = (0..=20)
        .map(|i| (f64::from(i) / 10.0, i as f32 * 0.05))
        .collect();
    file.tracks
        .push(track("ramp", PropertyName::Transparency, &numbers(&ramp)));
    // Still: one key.
    let still: Vec<(f64, f32)> = (0..=20).map(|i| (f64::from(i) / 10.0, 0.5)).collect();
    file.tracks
        .push(track("still", PropertyName::Transparency, &numbers(&still)));
    // A curve: some keys stay, and the curve stays close.
    let curve: Vec<(f64, f32)> = (0..=40)
        .map(|i| {
            let t = f64::from(i) / 20.0;
            (t, (t * 3.0).sin() as f32)
        })
        .collect();
    file.tracks
        .push(track("curve", PropertyName::TextSize, &numbers(&curve)));
    // An eased key and a step stay as they are: dropping them would change
    // how the track moves.
    let mut eased = track(
        "eased",
        PropertyName::Transparency,
        &numbers(&[(0.0, 0.0), (1.0, 0.5), (2.0, 1.0)]),
    );
    eased.keys[0].interp = Interp::Step;
    file.tracks.push(eased);
    let original = file.clone();

    let tolerance = Tolerance {
        value: 0.01,
        angle: 0.1,
    };
    let dropped = reduce_keys(&mut file, None, tolerance);
    assert_eq!(file.tracks[0].keys.len(), 2);
    assert_eq!(file.tracks[1].keys.len(), 1);
    let curve_keys = file.tracks[2].keys.len();
    assert!(curve_keys > 3 && curve_keys < 41, "{curve_keys}");
    assert_eq!(file.tracks[3].keys.len(), 3);
    assert_eq!(dropped, 19 + 20 + (41 - curve_keys));
    for (before, after) in original.tracks.iter().zip(&file.tracks) {
        for step in 0..=200 {
            let time = f64::from(step) / 100.0;
            let (a, b) = (at(before, time), at(after, time));
            assert!(
                (a - b).abs() <= 0.011,
                "{} at {time}: {a} vs {b}",
                before.id
            );
        }
    }
}

#[test]
fn reducing_bone_keys_compares_turns_by_angle() {
    let bone = |angle: f32| {
        bone_numbers(&BoneTransform {
            translation: Vec3::ZERO,
            rotation: Quat::from_rotation_y(angle.to_radians()),
            scale: Vec3::ONE,
        })
    };
    let mut file = ClipFile::new(1.0);
    // Turning steadily (slerp makes the in-betweens): ends only.
    let keys: Vec<(f64, Vec<f32>)> = (0..=10)
        .map(|i| (f64::from(i) / 10.0, bone(i as f32 * 9.0)))
        .collect();
    file.tracks.push(track("bone1", PropertyName::Bone, &keys));
    reduce_keys(&mut file, None, Tolerance::default());
    assert_eq!(file.tracks[0].keys.len(), 2);
}

#[test]
fn fixing_the_loop_seam_makes_the_end_meet_the_start() {
    let mut file = ClipFile::new(2.0);
    file.looping = true;
    // Drifts from 0 to 1 and never comes back.
    file.tracks.push(track(
        "drift",
        PropertyName::Transparency,
        &numbers(&[(0.0, 0.0), (2.0, 1.0)]),
    ));
    // Already meets: left alone.
    file.tracks.push(track(
        "round",
        PropertyName::Transparency,
        &numbers(&[(0.0, 0.2), (1.0, 0.8), (2.0, 0.2)]),
    ));
    // A bone that ends turned 0.4 radians from where it starts, with its
    // last key early.
    let turned = |angle: f32| {
        bone_numbers(&BoneTransform {
            translation: Vec3::new(0.0, angle, 0.0),
            rotation: Quat::from_rotation_x(angle),
            scale: Vec3::ONE,
        })
    };
    file.tracks.push(track(
        "bone1",
        PropertyName::Bone,
        &[(0.0, turned(0.0)), (1.0, turned(0.4))],
    ));
    let original = file.clone();
    let changed = fix_loop_seam(&mut file, None, 0.5);
    assert_eq!(changed, 2);
    assert_eq!(file.tracks[1], original.tracks[1]);

    let drift = &file.tracks[0];
    assert!(at(drift, 2.0).abs() < 1e-5, "ends where it starts");
    for time in [0.0, 0.5, 1.0, 1.5] {
        assert!(
            (at(drift, time) - at(&original.tracks[0], time)).abs() < 1e-5,
            "unchanged before the blend at {time}"
        );
    }
    assert!(
        at(drift, 1.75) < at(&original.tracks[0], 1.75),
        "bent toward the start"
    );

    let bone = &file.tracks[2];
    let start = bone_from_numbers(&sample_track(bone, 0.0).unwrap()).unwrap();
    let end = bone_from_numbers(&sample_track(bone, 2.0).unwrap()).unwrap();
    assert!(start.rotation.angle_between(end.rotation) < 1e-4);
    assert!(start.translation.abs_diff_eq(end.translation, 1e-5));
    let held = bone_from_numbers(&sample_track(bone, 1.4).unwrap()).unwrap();
    assert!(
        (held.rotation.angle_between(Quat::IDENTITY) - 0.4).abs() < 1e-4,
        "held until the blend"
    );
}

#[test]
fn side_words_in_bone_names_swap() {
    for (name, other) in [
        ("LeftArm", Some("RightArm")),
        ("mixamorig:RightHand", Some("mixamorig:LeftHand")),
        ("hand.L", Some("hand.R")),
        ("thigh_l", Some("thigh_r")),
        ("Bip01 L Hand", Some("Bip01 R Hand")),
        ("L_Foot", Some("R_Foot")),
        ("upper_arm.R.001", Some("upper_arm.L.001")),
        ("Spine", None),
        ("Hips", None),
        ("Tail_01", None),
    ] {
        assert_eq!(other_side(name).as_deref(), other, "{name}");
    }
}

/// [`crate::retarget::tests::unreal`]'s skeleton (bones with every axis
/// turned some other way), standing up as `world_turn` says.
fn body(world_turn: Quat) -> Rig {
    crate::retarget::tests::unreal(world_turn)
}

/// `bone`'s place in the rig's space for `file`'s keys at `time`.
fn place(rig: &Rig, file: &ClipFile, time: f64, bone: usize) -> Vec3 {
    let mut pose = rig.rest_pose();
    pose_bones(file, rig, time, &mut pose);
    rig.world(&pose)[bone].w_axis.truncate()
}

#[test]
fn mirroring_moves_motion_to_the_other_side_as_in_a_mirror() {
    for world_turn in [Quat::IDENTITY, Quat::from_rotation_x(-FRAC_PI_2)] {
        let rig = body(world_turn);
        let paths = bone_paths(&rig);
        let bone = |name: &str| rig.bone(name).unwrap();
        let (left_arm, left_hand, right_hand, pelvis) = (
            bone("lowerarm_l"),
            bone("hand_l"),
            bone("hand_r"),
            bone("pelvis"),
        );
        // The left forearm swings forward (about the up axis) and the
        // pelvis slides to the left (+X), over a second.
        let world = rig.world(&rig.rest_pose());
        let parent_turn = {
            let (_, turn, _) = world[bone("upperarm_l")].to_scale_rotation_translation();
            turn
        };
        let (_, rest_turn, _) = world[left_arm].to_scale_rotation_translation();
        let up = world_turn * Vec3::Y;
        let swung = |angle: f32| {
            let local = parent_turn.inverse() * Quat::from_axis_angle(up, angle) * rest_turn;
            bone_numbers(&BoneTransform {
                rotation: local,
                ..rig.bones[left_arm].rest
            })
        };
        let slid = |x: f32| {
            let mut rest = rig.bones[pelvis].rest;
            rest.translation += world_turn * Vec3::new(x, 0.0, 0.0);
            bone_numbers(&rest)
        };
        let mut file = ClipFile::new(1.0);
        let mut arm = track(
            "bone1",
            PropertyName::Bone,
            &[(0.0, swung(0.0)), (1.0, swung(-1.0))],
        );
        arm.target = paths[left_arm].clone();
        let mut hips = track(
            "bone2",
            PropertyName::Bone,
            &[(0.0, slid(0.0)), (1.0, slid(0.3))],
        );
        hips.target = paths[pelvis].clone();
        file.tracks.extend([arm, hips]);
        let original = file.clone();

        let mirrored = mirror(&mut file, &rig, &RigDefinitions::default()).unwrap();
        assert_eq!(
            mirrored,
            Mirrored {
                swapped: 2,
                in_place: 1
            }
        );
        assert_eq!(file.tracks.len(), 3, "the right forearm gets a track");

        let side = world_turn * Vec3::X;
        let flip = |v: Vec3| v - 2.0 * v.dot(side) * side;
        for time in [0.0, 0.25, 0.5, 1.0] {
            let was = place(&rig, &original, time, left_hand);
            let now = place(&rig, &file, time, right_hand);
            assert!(
                now.abs_diff_eq(flip(was), 1e-3),
                "{world_turn:?} at {time}: right hand {now} vs {}",
                flip(was)
            );
            // The left hand now does what the right did: rest, moved with
            // the hips.
            let rest_hand = place(&rig, &ClipFile::new(1.0), time, left_hand);
            let hips_moved =
                place(&rig, &file, time, pelvis) - place(&rig, &ClipFile::new(1.0), time, pelvis);
            assert!(place(&rig, &file, time, left_hand).abs_diff_eq(rest_hand + hips_moved, 1e-3));
            let hips = place(&rig, &original, time, pelvis);
            assert!(
                place(&rig, &file, time, pelvis).abs_diff_eq(flip(hips), 1e-3),
                "hips at {time}"
            );
        }

        // Mirroring twice gives back the clip's motion.
        mirror(&mut file, &rig, &RigDefinitions::default()).unwrap();
        for time in [0.0, 0.5, 1.0] {
            assert!(
                place(&rig, &file, time, left_hand)
                    .abs_diff_eq(place(&rig, &original, time, left_hand), 1e-3)
            );
        }
    }
}

#[test]
fn mirroring_needs_bone_keys() {
    let rig = body(Quat::IDENTITY);
    let mut file = ClipFile::new(1.0);
    assert!(mirror(&mut file, &rig, &RigDefinitions::default()).is_err());
    let mut linked = ClipFile::linked("a.fbx", "Walk", 0.0, 1.0);
    assert!(mirror(&mut linked, &rig, &RigDefinitions::default()).is_err());
}
