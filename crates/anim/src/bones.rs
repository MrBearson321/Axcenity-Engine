//! Bones in clips: how a bone track names its bone and stores its
//! transform, which bones a take moves, and baking a linked clip's take
//! into bone keys (Convert to keys).

use std::sync::Arc;

use axcenity_assets::{BoneTransform, Pose, Rig, Take};
use glam::{Quat, Vec3};

use crate::clip::Clip;
use crate::files::{ClipFile, Interp, PropertyKeyframe, PropertyName, PropertyTrack};
use crate::property_clip::{lerp_property, sample_track};

/// How far a dropped key may be from what the keys around it make: in the
/// bone's units for translation and scale, and radians for rotation.
const KEEP_IF_OFF_BY: f32 = 1e-4;
const KEEP_IF_TURNED_BY: f32 = 1e-3;

/// Each bone's path from the top of the skeleton, by names (`Hips/Spine`),
/// as a bone track's `target` names it. Unnamed bones are left out of the
/// paths below them.
pub fn bone_paths(rig: &Rig) -> Vec<String> {
    let mut paths: Vec<String> = Vec::with_capacity(rig.bones.len());
    for bone in &rig.bones {
        let path = match bone.parent.and_then(|parent| paths.get(parent)) {
            Some(parent) if !parent.is_empty() && !bone.name.is_empty() => {
                format!("{parent}/{}", bone.name)
            }
            Some(parent) if bone.name.is_empty() => parent.clone(),
            _ => bone.name.clone(),
        };
        paths.push(path);
    }
    paths
}

/// The bone a track's `target` names: by its whole path, or else by its
/// last name.
pub fn find_bone(rig: &Rig, paths: &[String], target: &str) -> Option<usize> {
    let target = target.trim_matches('/');
    paths.iter().position(|path| path == target).or_else(|| {
        let name = target.rsplit('/').next()?;
        rig.bone(name)
    })
}

/// A bone's transform as a `Bone` key's 10 numbers: translation, rotation
/// (`x, y, z, w`) and scale.
pub fn bone_numbers(transform: &BoneTransform) -> Vec<f32> {
    let (t, q, s) = (transform.translation, transform.rotation, transform.scale);
    vec![t.x, t.y, t.z, q.x, q.y, q.z, q.w, s.x, s.y, s.z]
}

/// A `Bone` key's 10 numbers as a transform, or `None` if there aren't 10.
pub fn bone_from_numbers(numbers: &[f32]) -> Option<BoneTransform> {
    let [tx, ty, tz, qx, qy, qz, qw, sx, sy, sz] = numbers[..] else {
        return None;
    };
    Some(BoneTransform {
        translation: Vec3::new(tx, ty, tz),
        rotation: Quat::from_xyzw(qx, qy, qz, qw).normalize(),
        scale: Vec3::new(sx, sy, sz),
    })
}

/// Poses `rig`'s bones in `pose` as `file`'s bone tracks have them at
/// `seconds` into the clip (bones without a track are left as they are).
/// Its linked take, if any, isn't played.
pub fn pose_bones(file: &ClipFile, rig: &Rig, seconds: f64, pose: &mut Pose) {
    let paths = bone_paths(rig);
    for track in file
        .tracks
        .iter()
        .filter(|t| t.property == PropertyName::Bone)
    {
        if let Some(bone) = find_bone(rig, &paths, &track.target)
            && let Some(values) = sample_track(track, seconds)
            && let (Some(transform), Some(slot)) = (bone_from_numbers(&values), pose.get_mut(bone))
        {
            *slot = transform;
        }
    }
}

/// One per rig bone: whether `take` moves it. In a clip linked to the take,
/// those bones are the take's and can't be keyed.
pub fn take_bones(take: &Take, rig: &Rig) -> Vec<bool> {
    let mut moved = vec![false; rig.bones.len()];
    for bone in take.bind(rig).into_iter().flatten() {
        moved[bone] = true;
    }
    moved
}

/// Convert to keys: `file`'s linked take baked into bone keys on `rig`,
/// one key a frame at the take's frame rate over the clip's range, then
/// the keys that don't change the motion dropped. The link goes; the
/// clip's own tracks and events stay. With root motion on, the root is
/// baked where the clip shows it (in place), since a keyed clip has no
/// root motion. `takes` are the link's source file's.
pub fn convert_to_keys(
    file: &ClipFile,
    takes: Arc<Vec<Take>>,
    rig: &Rig,
) -> Result<ClipFile, String> {
    let Some(link) = &file.link else {
        return Err("it isn't linked to a take".to_owned());
    };
    // Not looping, so the last phase is the last frame, not the first.
    let mut once = file.clone();
    once.looping = false;
    let clip = Clip::bind(once, Some(takes), Some(rig))?;
    let take = clip
        .take()
        .ok_or_else(|| format!("its take `{}` can't be read", link.take))?;
    let moved = take_bones(take, rig);
    let frame_rate = f64::from(take.frame_rate.max(1.0));
    let frames = ((link.end - link.start).abs() * frame_rate)
        .round()
        .max(1.0) as usize;
    let poses: Vec<Pose> = (0..=frames)
        .map(|frame| {
            let mut pose = rig.rest_pose();
            clip.sample(frame as f64 / frames as f64, rig, &mut pose);
            pose
        })
        .collect();

    let paths = bone_paths(rig);
    let mut out = file.clone();
    out.link = None;
    // Tracks for the take's bones never played; the baked ones replace them.
    out.tracks.retain(|track| {
        track.property != PropertyName::Bone
            || find_bone(rig, &paths, &track.target).is_none_or(|bone| !moved[bone])
    });
    for (bone, path) in paths.iter().enumerate().filter(|(bone, _)| moved[*bone]) {
        let mut keys: Vec<(f64, Vec<f32>)> = Vec::with_capacity(poses.len());
        let mut last_rotation: Option<Quat> = None;
        for (frame, pose) in poses.iter().enumerate() {
            let mut transform = pose[bone];
            // Each rotation on the same side as the one before, so keys
            // blend the short way and read smoothly.
            if let Some(last) = last_rotation
                && last.dot(transform.rotation) < 0.0
            {
                transform.rotation = -transform.rotation;
            }
            last_rotation = Some(transform.rotation);
            let time = frame as f64 / frames as f64 * file.duration;
            keys.push((time, bone_numbers(&transform)));
        }
        let id = free_track_id(&out.tracks);
        out.tracks.push(PropertyTrack {
            id,
            target: path.clone(),
            property: PropertyName::Bone,
            keys: simplify(keys)
                .into_iter()
                .enumerate()
                .map(|(index, (time, value))| PropertyKeyframe {
                    id: format!("k{}", index + 1),
                    time,
                    value,
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                })
                .collect(),
        });
    }
    Ok(out)
}

/// The first of `bone1`, `bone2`, … no track has.
fn free_track_id(tracks: &[PropertyTrack]) -> String {
    (1..)
        .map(|n| format!("bone{n}"))
        .find(|id| tracks.iter().all(|track| track.id != *id))
        .expect("some id is free")
}

/// `keys` without those the keys around them already make: a key is
/// dropped when blending from the last key kept to the next one passes
/// within [`KEEP_IF_OFF_BY`] of it and of every key dropped since. A bone
/// that never moves keeps one key.
fn simplify(keys: Vec<(f64, Vec<f32>)>) -> Vec<(f64, Vec<f32>)> {
    if keys.len() <= 2 {
        return if keys.len() == 2 && same(&keys[0].1, &keys[1].1) {
            keys.into_iter().take(1).collect()
        } else {
            keys
        };
    }
    let mut kept = vec![0];
    for next in 2..keys.len() {
        let from = *kept.last().expect("the first is kept");
        let (a, b) = (&keys[from], &keys[next]);
        let span = (b.0 - a.0).max(1e-12);
        let fits = (from + 1..next).all(|between| {
            let (time, value) = &keys[between];
            let t = ((time - a.0) / span) as f32;
            same(&lerp_property(PropertyName::Bone, &a.1, &b.1, t), value)
        });
        if !fits {
            kept.push(next - 1);
        }
    }
    kept.push(keys.len() - 1);
    if kept.len() == 2 && same(&keys[0].1, &keys[keys.len() - 1].1) {
        // It may still move in between; only a bone at rest throughout goes
        // down to one key.
        let still = keys.iter().all(|(_, value)| same(value, &keys[0].1));
        if still {
            kept.pop();
        }
    }
    kept.into_iter().map(|index| keys[index].clone()).collect()
}

/// Whether two bone transforms (as numbers) are the same, near enough.
fn same(a: &[f32], b: &[f32]) -> bool {
    let (Some(a), Some(b)) = (bone_from_numbers(a), bone_from_numbers(b)) else {
        return a == b;
    };
    a.translation.abs_diff_eq(b.translation, KEEP_IF_OFF_BY)
        && a.scale.abs_diff_eq(b.scale, KEEP_IF_OFF_BY)
        && a.rotation.angle_between(b.rotation) <= KEEP_IF_TURNED_BY
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axcenity_assets::Assets;

    use super::*;
    use crate::files::TakeLink;

    /// The example's noodle rig and its takes.
    fn noodle() -> (Arc<Rig>, Arc<Vec<Take>>) {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation");
        let assets = Assets::new(example);
        let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
        let takes = assets.takes("assets/models/noodle.fbx").unwrap();
        (model.rig.clone().unwrap(), takes)
    }

    fn linked(take: &str, start: f64, end: f64, duration: f64) -> ClipFile {
        let mut file = ClipFile::new(duration);
        file.looping = true;
        file.link = Some(TakeLink {
            source: "assets/models/noodle.fbx".into(),
            take: take.into(),
            start,
            end,
            root_motion: false,
        });
        file
    }

    fn pose_at(file: &ClipFile, takes: &Arc<Vec<Take>>, rig: &Rig, phase: f64) -> Pose {
        let clip = Clip::bind(file.clone(), Some(takes.clone()), Some(rig)).unwrap();
        let mut pose = rig.rest_pose();
        clip.sample(phase, rig, &mut pose);
        pose
    }

    fn assert_same_pose(a: &Pose, b: &Pose, at: f64) {
        for (bone, (a, b)) in a.iter().zip(b).enumerate() {
            assert!(
                a.translation.abs_diff_eq(b.translation, 1e-3)
                    && a.rotation.angle_between(b.rotation) < 2e-3
                    && a.scale.abs_diff_eq(b.scale, 1e-3),
                "bone {bone} at phase {at}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn idle_moves_only_mid_so_only_mid_is_locked() {
        let (rig, takes) = noodle();
        let idle = takes.iter().find(|take| take.name == "Idle").unwrap();
        let moved = take_bones(idle, &rig);
        let names: Vec<&str> = rig
            .bones
            .iter()
            .zip(&moved)
            .filter(|(_, moved)| **moved)
            .map(|(bone, _)| bone.name.as_str())
            .collect();
        assert_eq!(names, ["Mid"]);
    }

    #[test]
    fn converting_to_keys_looks_the_same_and_drops_keys_that_change_nothing() {
        let (rig, takes) = noodle();
        // The wave of Moves (Top turns; Root's track is there but still)
        // at double speed, with a key of its own on Mid, which Moves
        // doesn't move.
        let mut file = linked("Moves", 0.0, 1.0, 0.5);
        file.tracks.push(PropertyTrack {
            id: "bone1".into(),
            target: "Root/Mid".into(),
            property: PropertyName::Bone,
            keys: vec![PropertyKeyframe {
                id: "k1".into(),
                time: 0.0,
                value: bone_numbers(&BoneTransform {
                    translation: Vec3::new(0.0, 0.0, 0.0),
                    rotation: Quat::from_rotation_y(0.3),
                    scale: Vec3::ONE,
                }),
                interp: Interp::Linear,
                easing: None,
                bezier: None,
            }],
        });
        file.events.push(crate::ClipEvent {
            time: 0.25,
            name: "Wave".into(),
        });
        let keyed = convert_to_keys(&file, takes.clone(), &rig).unwrap();
        assert!(keyed.link.is_none());
        assert_eq!(keyed.duration, file.duration);
        assert_eq!(keyed.events, file.events);
        let own = keyed.tracks.iter().find(|t| t.id == "bone1").unwrap();
        assert_eq!(own.target, "Root/Mid", "its own key stays");
        let baked: Vec<&str> = keyed.tracks[1..]
            .iter()
            .map(|t| t.target.as_str())
            .collect();
        assert_eq!(baked, ["Root", "Root/Mid/Top"]);
        for step in 0..=20 {
            let phase = f64::from(step) / 20.0 * 0.999;
            assert_same_pose(
                &pose_at(&file, &takes, &rig, phase),
                &pose_at(&keyed, &takes, &rig, phase),
                phase,
            );
        }
        let take = takes.iter().find(|take| take.name == "Moves").unwrap();
        let frames = (take.frame_rate.round() as usize) + 1;
        for track in keyed.tracks.iter().filter(|t| t.id != "bone1") {
            assert!(
                track.keys.len() < frames,
                "{} kept all {} frames",
                track.target,
                track.keys.len()
            );
        }
        // Round trip as a file.
        assert_eq!(ClipFile::from_json(&keyed.to_json()).unwrap(), keyed);
    }

    #[test]
    fn a_bone_that_never_moves_keeps_one_key() {
        let rest = bone_numbers(&BoneTransform {
            translation: Vec3::ONE,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        });
        let keys: Vec<(f64, Vec<f32>)> = (0..5).map(|i| (f64::from(i), rest.clone())).collect();
        assert_eq!(simplify(keys).len(), 1);
    }

    #[test]
    fn a_clip_without_a_link_can_t_be_converted() {
        let (rig, takes) = noodle();
        assert!(convert_to_keys(&ClipFile::new(1.0), takes, &rig).is_err());
    }
}
