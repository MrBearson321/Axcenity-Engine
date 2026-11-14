//! Cleaning up keyed clips (AP4): mirroring a body's keys left to right,
//! smoothing, reducing keys, fixing the loop seam and re-timing a range.
//!
//! Each works on a [`ClipFile`]'s own keys, so a clip linked to a take is
//! converted to keys first (Convert to keys). Most take the ids of the
//! tracks to change, where `None` means every track.

use axcenity_assets::{BoneTransform, Pose, Rig, RigDefinitions};
use axcenity_core::Vector3;
use glam::{Mat4, Quat, Vec3};

use crate::bones::{bone_from_numbers, bone_numbers, bone_paths, find_bone, pose_bones};
use crate::files::{ClipFile, Interp, PropertyKeyframe, PropertyName, PropertyTrack};
use crate::property_clip::{
    lerp_property, orientation_from_quat, quat_from_orientation, sample_track,
};
use crate::retarget::HumanoidMap;

/// How far a dropped key may be from what the keys around it make: `value`
/// in the property's own units (studs, colour channels, a bone's
/// translation and scale), `angle` in degrees for turns (Orientation and a
/// bone's rotation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    pub value: f32,
    pub angle: f32,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            value: 0.001,
            angle: 0.1,
        }
    }
}

/// What [`mirror`] changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mirrored {
    /// Bones whose keys were swapped with the bone on the other side.
    pub swapped: usize,
    /// Bones in the middle (or without a partner) mirrored in place.
    pub in_place: usize,
}

/// Frames a second [`mirror`] bakes at, between and on top of the keys.
const MIRROR_RATE: f64 = 30.0;

fn chosen(track: &PropertyTrack, tracks: Option<&[String]>) -> bool {
    tracks.is_none_or(|ids| ids.contains(&track.id))
}

/// Re-times `from..to` (seconds into the clip) to last `length` seconds:
/// keys and events inside it spread or squeeze, those after it move by the
/// difference, and the clip gets longer or shorter by as much.
pub fn retime(file: &mut ClipFile, from: f64, to: f64, length: f64) -> Result<(), String> {
    if file.link.is_some() {
        return Err("it plays a take: Convert to keys first".to_owned());
    }
    if !(0.0..to).contains(&from) || to > file.duration + 1e-9 {
        return Err(format!(
            "the range must be inside the clip (0 to {:.3} s) and end after it starts",
            file.duration
        ));
    }
    if length <= 1e-6 {
        return Err("the new length must be more than 0".to_owned());
    }
    let span = to - from;
    let moved = |time: f64| {
        if time < from {
            time
        } else if time <= to {
            from + (time - from) / span * length
        } else {
            time + length - span
        }
    };
    for key in file.tracks.iter_mut().flat_map(|track| &mut track.keys) {
        key.time = moved(key.time);
    }
    for event in &mut file.events {
        event.time = moved(event.time);
    }
    file.duration = (file.duration + length - span).max(0.0);
    Ok(())
}

/// Smooths the chosen tracks' keys: each key but the first and last moves
/// halfway toward the average of its neighbours, `passes` times over.
/// Returns how many keys moved.
pub fn smooth(file: &mut ClipFile, tracks: Option<&[String]>, passes: u32) -> usize {
    let mut moved = 0;
    for track in file.tracks.iter_mut().filter(|t| chosen(t, tracks)) {
        if track.keys.len() < 3 {
            continue;
        }
        let before: Vec<Vec<f32>> = track.keys.iter().map(|k| k.value.clone()).collect();
        for _ in 0..passes {
            let values: Vec<Vec<f32>> = track.keys.iter().map(|k| k.value.clone()).collect();
            for i in 1..values.len() - 1 {
                let between = lerp_property(track.property, &values[i - 1], &values[i + 1], 0.5);
                track.keys[i].value = lerp_property(track.property, &values[i], &between, 0.5);
            }
        }
        moved += track
            .keys
            .iter()
            .zip(&before)
            .filter(|(key, value)| !close(track.property, &key.value, value, Tolerance::NONE))
            .count();
    }
    moved
}

impl Tolerance {
    const NONE: Tolerance = Tolerance {
        value: 1e-6,
        angle: 1e-4,
    };
}

/// Drops the chosen tracks' keys that the keys around them already make,
/// to within `tolerance`, checked at every key and halfway between keys.
/// A track that never changes keeps one key. Returns how many keys went.
pub fn reduce_keys(file: &mut ClipFile, tracks: Option<&[String]>, tolerance: Tolerance) -> usize {
    let mut dropped = 0;
    for track in file.tracks.iter_mut().filter(|t| chosen(t, tracks)) {
        let before = track.keys.len();
        reduce_track(track, tolerance);
        dropped += before - track.keys.len();
    }
    dropped
}

fn reduce_track(track: &mut PropertyTrack, tolerance: Tolerance) {
    let keys = &track.keys;
    if keys.len() < 2 {
        return;
    }
    if keys
        .iter()
        .all(|key| close(track.property, &key.value, &keys[0].value, tolerance))
    {
        track.keys.truncate(1);
        return;
    }
    // Where the track must stay: at each key, and halfway to the next.
    let mut checks: Vec<(f64, Vec<f32>)> = Vec::with_capacity(keys.len() * 2);
    for (index, key) in keys.iter().enumerate() {
        checks.push((key.time, key.value.clone()));
        if let Some(next) = keys.get(index + 1) {
            let half = (key.time + next.time) / 2.0;
            if let Some(value) = sample_track(track, half) {
                checks.push((half, value));
            }
        }
    }
    let mut kept = vec![0];
    for next in 2..keys.len() {
        let from = *kept.last().expect("the first is kept");
        let (a, b) = (&keys[from], &keys[next]);
        let pair = PropertyTrack {
            keys: vec![a.clone(), b.clone()],
            ..track.clone()
        };
        let fits = checks
            .iter()
            .filter(|(time, _)| *time > a.time && *time < b.time)
            .all(|(time, value)| {
                sample_track(&pair, *time)
                    .is_some_and(|made| close(track.property, &made, value, tolerance))
            });
        if !fits {
            kept.push(next - 1);
        }
    }
    kept.push(keys.len() - 1);
    track.keys = kept.into_iter().map(|index| keys[index].clone()).collect();
}

/// Whether two keys' values are the same to within `tolerance`.
fn close(property: PropertyName, a: &[f32], b: &[f32], tolerance: Tolerance) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let angle = tolerance.angle.to_radians();
    match property {
        PropertyName::Bone => {
            let (Some(a), Some(b)) = (bone_from_numbers(a), bone_from_numbers(b)) else {
                return a == b;
            };
            a.translation.abs_diff_eq(b.translation, tolerance.value)
                && a.scale.abs_diff_eq(b.scale, tolerance.value)
                && a.rotation.angle_between(b.rotation) <= angle
        }
        PropertyName::Orientation if a.len() == 3 => {
            let turn = |v: &[f32]| quat_from_orientation(Vector3::new(v[0], v[1], v[2]));
            turn(a).angle_between(turn(b)) <= angle
        }
        _ => a
            .iter()
            .zip(b)
            .all(|(a, b)| (a - b).abs() <= tolerance.value),
    }
}

/// Makes a looping clip's chosen tracks end where they start, so the loop
/// has no jump: over the last `blend` seconds each track is bent, more and
/// more, toward its value at the start (a key at the end is added where
/// there's none). Returns how many tracks changed.
pub fn fix_loop_seam(file: &mut ClipFile, tracks: Option<&[String]>, blend: f64) -> usize {
    let end = file.duration;
    if end <= 1e-6 {
        return 0;
    }
    let blend = blend.clamp(1e-3, end);
    let from = end - blend;
    let mut changed = 0;
    for track in file.tracks.iter_mut().filter(|t| chosen(t, tracks)) {
        let (Some(start), Some(last)) = (sample_track(track, 0.0), sample_track(track, end)) else {
            continue;
        };
        if close(track.property, &start, &last, Tolerance::NONE) {
            continue;
        }
        // A key at the end to bend onto the start, and one where the bend
        // begins, so the change is spread over the blend.
        if track.keys.iter().all(|key| key.time < end - 1e-6) {
            let id = free_key_id(track);
            track
                .keys
                .push(PropertyKeyframe::new(id, end, last.clone()));
        }
        if from > 1e-6
            && !track
                .keys
                .iter()
                .any(|key| key.time > from - 1e-6 && key.time < end - 1e-6)
            && let Some(value) = sample_track(track, from)
        {
            let id = free_key_id(track);
            let at = track.keys.partition_point(|key| key.time < from);
            let interp = at
                .checked_sub(1)
                .map_or(Interp::Linear, |before| track.keys[before].interp);
            let mut key = PropertyKeyframe::new(id, from, value);
            key.interp = if interp == Interp::Step {
                Interp::Step
            } else {
                Interp::Linear
            };
            track.keys.insert(at, key);
        }
        for key in track.keys.iter_mut().filter(|key| key.time > from) {
            let t = ((key.time - from) / blend).clamp(0.0, 1.0) as f32;
            let weight = t * t * (3.0 - 2.0 * t);
            key.value = bend(track.property, &key.value, &last, &start, weight);
        }
        changed += 1;
    }
    changed
}

/// `value` moved `weight` of the way that `from` must move to reach `to`.
fn bend(property: PropertyName, value: &[f32], from: &[f32], to: &[f32], weight: f32) -> Vec<f32> {
    match property {
        PropertyName::Bone => {
            let (Some(value), Some(from), Some(to)) = (
                bone_from_numbers(value),
                bone_from_numbers(from),
                bone_from_numbers(to),
            ) else {
                return value.to_vec();
            };
            let turn = Quat::IDENTITY.slerp(to.rotation * from.rotation.inverse(), weight);
            bone_numbers(&BoneTransform {
                translation: value.translation + (to.translation - from.translation) * weight,
                rotation: (turn * value.rotation).normalize(),
                scale: value.scale + (to.scale - from.scale) * weight,
            })
        }
        PropertyName::Orientation if value.len() == 3 => {
            let q = |v: &[f32]| quat_from_orientation(Vector3::new(v[0], v[1], v[2]));
            let turn = Quat::IDENTITY.slerp(q(to) * q(from).inverse(), weight);
            let v = orientation_from_quat(turn * q(value));
            vec![v.x, v.y, v.z]
        }
        _ => value
            .iter()
            .zip(from.iter().zip(to))
            .map(|(v, (a, b))| v + (b - a) * weight)
            .collect(),
    }
}

/// The first of `k1`, `k2`, … no key of `track` has.
fn free_key_id(track: &PropertyTrack) -> String {
    (1..)
        .map(|n| format!("k{n}"))
        .find(|id| track.keys.iter().all(|key| key.id != *id))
        .expect("some id is free")
}

/// Mirrors a body's bone keys left to right on `rig`: each keyed bone's
/// motion moves to the bone on the other side (found through the humanoid
/// map, then by names such as `Left`/`Right`, `.L`/`.R` or `_l`/`_r`),
/// turned as in a mirror, and bones in the middle are mirrored in place.
/// Motion is mirrored as it moves away from the rest pose, so skeletons
/// whose left and right bones have different axes mirror properly. The
/// result is baked a frame apart (and on every key) and then reduced, as
/// Convert to keys does. Other tracks and events are left as they are.
pub fn mirror(file: &mut ClipFile, rig: &Rig, rigs: &RigDefinitions) -> Result<Mirrored, String> {
    if file.link.is_some() {
        return Err("it plays a take: Convert to keys first".to_owned());
    }
    let paths = bone_paths(rig);
    let keyed: Vec<usize> = file
        .tracks
        .iter()
        .filter(|t| t.property == PropertyName::Bone && !t.keys.is_empty())
        .filter_map(|t| find_bone(rig, &paths, &t.target))
        .collect();
    if keyed.is_empty() {
        return Err("it has no bone keys on this model".to_owned());
    }
    let partner = partners(rig, rigs);
    let mut written = vec![false; rig.bones.len()];
    for &bone in &keyed {
        written[bone] = true;
        written[partner[bone]] = true;
    }
    let normal = side_axis(rig, &partner);

    // When to sample: every key, and a frame apart in between.
    let mut times: Vec<f64> = file
        .tracks
        .iter()
        .filter(|t| t.property == PropertyName::Bone)
        .flat_map(|t| t.keys.iter().map(|k| k.time))
        .collect();
    let frames = (file.duration * MIRROR_RATE).ceil().max(1.0) as usize;
    times.extend((0..=frames).map(|frame| frame as f64 / frames as f64 * file.duration));
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

    let rest = rig.rest_pose();
    let rest_turns = world_turns(rig, &rest);
    let mut baked: Vec<Vec<(f64, Vec<f32>)>> = vec![Vec::new(); rig.bones.len()];
    for &time in &times {
        let mut pose = rest.clone();
        pose_bones(file, rig, time, &mut pose);
        let mirrored = mirror_pose(rig, &pose, &rest_turns, &partner, &written, normal);
        for (bone, keys) in baked.iter_mut().enumerate().filter(|(b, _)| written[*b]) {
            let mut transform = mirrored[bone];
            if let Some((_, last)) = keys.last()
                && let Some(last) = bone_from_numbers(last)
                && last.rotation.dot(transform.rotation) < 0.0
            {
                transform.rotation = -transform.rotation;
            }
            keys.push((time, bone_numbers(&transform)));
        }
    }

    let mut swapped = 0;
    let mut in_place = 0;
    for (bone, keys) in baked.into_iter().enumerate() {
        if !written[bone] {
            continue;
        }
        if partner[bone] == bone {
            in_place += 1;
        } else {
            swapped += 1;
        }
        let index = match file.tracks.iter().position(|t| {
            t.property == PropertyName::Bone && find_bone(rig, &paths, &t.target) == Some(bone)
        }) {
            Some(index) => index,
            None => {
                let id = (1..)
                    .map(|n| format!("bone{n}"))
                    .find(|id| file.tracks.iter().all(|t| t.id != *id))
                    .expect("some id is free");
                file.tracks.push(PropertyTrack {
                    id,
                    target: paths[bone].clone(),
                    property: PropertyName::Bone,
                    keys: Vec::new(),
                });
                file.tracks.len() - 1
            }
        };
        let track = &mut file.tracks[index];
        track.keys = keys
            .into_iter()
            .enumerate()
            .map(|(n, (time, value))| PropertyKeyframe::new(format!("k{}", n + 1), time, value))
            .collect();
        reduce_track(
            track,
            Tolerance {
                value: 1e-4,
                angle: 0.06,
            },
        );
    }
    // Other tracks of the same bone (rare) would fight the mirrored keys.
    let mut seen = vec![false; rig.bones.len()];
    file.tracks.retain(|t| {
        if t.property != PropertyName::Bone {
            return true;
        }
        match find_bone(rig, &paths, &t.target) {
            Some(bone) if written[bone] => !std::mem::replace(&mut seen[bone], true),
            _ => true,
        }
    });
    Ok(Mirrored { swapped, in_place })
}

/// Each bone's partner on the other side (itself in the middle): by the
/// humanoid map's roles, then by side words in its name.
pub fn partners(rig: &Rig, rigs: &RigDefinitions) -> Vec<usize> {
    let mut partner: Vec<usize> = (0..rig.bones.len()).collect();
    let (map, _) = HumanoidMap::for_skeleton(&rig.bones, rigs);
    for (role, bone) in map.roles() {
        if let Some(other) = map.get(role.mirrored())
            && other != bone
        {
            partner[bone] = other;
        }
    }
    for (bone, entry) in rig.bones.iter().enumerate() {
        if partner[bone] != bone {
            continue;
        }
        if let Some(other) = other_side(&entry.name).and_then(|name| rig.bone(&name))
            && other != bone
        {
            partner[bone] = other;
        }
    }
    partner
}

/// `name` with its side word swapped, if it has one: `Left`/`Right` (any
/// case) anywhere, or `L`/`R` after or before a `.`, `_`, `-` or space.
fn other_side(name: &str) -> Option<String> {
    for (a, b) in [("Left", "Right"), ("left", "right"), ("LEFT", "RIGHT")] {
        if name.contains(a) {
            return Some(name.replacen(a, b, 1));
        }
        if name.contains(b) {
            return Some(name.replacen(b, a, 1));
        }
    }
    let swap = |c: char| match c {
        'L' => Some('R'),
        'R' => Some('L'),
        'l' => Some('r'),
        'r' => Some('l'),
        _ => None,
    };
    let marks = ['.', '_', '-', ' '];
    let chars: Vec<char> = name.chars().collect();
    // At the end (`hand.L`, `thigh_l`).
    if chars.len() >= 2
        && marks.contains(&chars[chars.len() - 2])
        && let Some(other) = swap(chars[chars.len() - 1])
    {
        let mut out: String = chars[..chars.len() - 1].iter().collect();
        out.push(other);
        return Some(out);
    }
    // At the start (`L_Hand`, `l-foot`).
    if chars.len() >= 2
        && marks.contains(&chars[1])
        && let Some(other) = swap(chars[0])
    {
        let mut out = String::from(other);
        out.extend(&chars[1..]);
        return Some(out);
    }
    // In the middle (`Bip01 L Hand`, `hand_l_01`).
    for i in 1..chars.len().saturating_sub(1) {
        if marks.contains(&chars[i - 1])
            && marks.contains(&chars[i + 1])
            && let Some(other) = swap(chars[i])
        {
            let mut out = chars.clone();
            out[i] = other;
            return Some(out.into_iter().collect());
        }
    }
    None
}

/// The axis left and right lie along in the rig's space: from a right
/// bone to its left partner at rest, the one farthest apart; X if none.
fn side_axis(rig: &Rig, partner: &[usize]) -> Vec3 {
    let world = rig.world(&rig.rest_pose());
    let at = |bone: usize| world[bone].w_axis.truncate();
    let across = partner
        .iter()
        .enumerate()
        .filter(|(bone, other)| *bone != **other)
        .map(|(bone, &other)| at(bone) - at(other))
        .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
    match across {
        Some(across) if across.length_squared() > 1e-10 => {
            // The nearest of the rig's own axes: a skeleton stands square.
            let a = across.abs();
            if a.x >= a.y && a.x >= a.z {
                Vec3::X
            } else if a.y >= a.z {
                Vec3::Y
            } else {
                Vec3::Z
            }
        }
        _ => Vec3::X,
    }
}

/// Each bone's turn in the rig's space for `pose`.
fn world_turns(rig: &Rig, pose: &Pose) -> Vec<Quat> {
    let mut turns: Vec<Quat> = Vec::with_capacity(rig.bones.len());
    for (index, bone) in rig.bones.iter().enumerate() {
        let local = pose.get(index).map_or(bone.rest.rotation, |t| t.rotation);
        let turn = match bone.parent {
            Some(parent) if parent < index => turns[parent] * local,
            _ => local,
        };
        turns.push(turn.normalize());
    }
    turns
}

/// `v` as in a mirror across the plane through the origin with `normal`.
fn reflect(v: Vec3, normal: Vec3) -> Vec3 {
    v - 2.0 * v.dot(normal) * normal
}

/// A turn as in that mirror: its axis is reflected and flipped (axes are
/// pseudo-vectors), its angle kept.
fn reflect_turn(turn: Quat, normal: Vec3) -> Quat {
    let axis = Vec3::new(turn.x, turn.y, turn.z);
    let mirrored = -reflect(axis, normal);
    Quat::from_xyzw(mirrored.x, mirrored.y, mirrored.z, turn.w).normalize()
}

/// `pose` mirrored: each `written` bone takes its partner's motion away
/// from rest (turn in the rig's space, and offset from where its rest puts
/// it), reflected; other bones keep their own local transforms.
fn mirror_pose(
    rig: &Rig,
    pose: &Pose,
    rest_turns: &[Quat],
    partner: &[usize],
    written: &[bool],
    normal: Vec3,
) -> Pose {
    let world = rig.world(pose);
    let turns = world_turns(rig, pose);
    let parent_world = |matrices: &[Mat4], bone: usize| {
        rig.bones[bone]
            .parent
            .filter(|&parent| parent < bone)
            .map_or(Mat4::IDENTITY, |parent| matrices[parent])
    };
    let mut out = pose.clone();
    let mut new_world: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
    let mut new_turns: Vec<Quat> = Vec::with_capacity(rig.bones.len());
    for (bone, entry) in rig.bones.iter().enumerate() {
        let parent_turn = entry
            .parent
            .filter(|&parent| parent < bone)
            .map_or(Quat::IDENTITY, |parent| new_turns[parent]);
        if written[bone] {
            let from = partner[bone];
            let moved = turns[from] * rest_turns[from].inverse();
            let turn = (reflect_turn(moved, normal) * rest_turns[bone]).normalize();
            // How far `from` sits from where its rest offset puts it.
            let expected =
                parent_world(&world, from).transform_point3(rig.bones[from].rest.translation);
            let offset = reflect(world[from].w_axis.truncate() - expected, normal);
            let parent = parent_world(&new_world, bone);
            out[bone] = BoneTransform {
                translation: entry.rest.translation + parent.inverse().transform_vector3(offset),
                rotation: (parent_turn.inverse() * turn).normalize(),
                scale: pose[from].scale,
            };
        }
        new_turns.push((parent_turn * out[bone].rotation).normalize());
        new_world.push(parent_world(&new_world, bone) * out[bone].matrix());
    }
    out
}

#[cfg(test)]
mod tests;
