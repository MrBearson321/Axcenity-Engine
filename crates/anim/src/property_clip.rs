//! Authored property clips: keyframed Instance properties (Position,
//! Orientation, Size) that play without an FBX take.
//!
//! Sampling is pure: evaluate a time into property values, then the host
//! applies them. Orientation keys are degrees; linear interpolation uses
//! quaternion slerp.

use std::collections::HashMap;

use axcenity_core::{InstanceId, Value, Vector3, World};
use glam::{EulerRot, Quat};

use crate::files::{
    ClipEvent, FileError, Interp, PropertyClipFile, PropertyKeyframe, PropertyName, PropertyTrack,
};

/// A validated property clip ready to sample.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyClip {
    pub file: PropertyClipFile,
}

impl PropertyClip {
    /// Validates `file` (duration, key times, sorted keys, coalesced
    /// duplicate times) and returns a clip ready to sample.
    pub fn from_file(mut file: PropertyClipFile) -> Result<Self, FileError> {
        if !file.duration.is_finite() || file.duration < 0.0 {
            return Err(FileError(
                "its duration must be a finite number 0 or more".into(),
            ));
        }
        for track in &mut file.tracks {
            if track.id.is_empty() {
                return Err(FileError("a track needs an id".into()));
            }
            if track.target.is_empty() {
                return Err(FileError(format!(
                    "track `{}` needs a target (`.` for the animated root)",
                    track.id
                )));
            }
            for key in &track.keys {
                if !key.time.is_finite() {
                    return Err(FileError(format!(
                        "track `{}` has a key with a non-finite time",
                        track.id
                    )));
                }
                if key.time < 0.0 || key.time > file.duration {
                    return Err(FileError(format!(
                        "track `{}` has a key at {}s outside the clip's duration 0..{}",
                        track.id, key.time, file.duration
                    )));
                }
                if !(key.value[0].is_finite()
                    && key.value[1].is_finite()
                    && key.value[2].is_finite())
                {
                    return Err(FileError(format!(
                        "track `{}` has a key with a non-finite value",
                        track.id
                    )));
                }
            }
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            // Duplicate timestamps: keep the last.
            let mut coalesced = Vec::with_capacity(track.keys.len());
            for key in track.keys.drain(..) {
                if coalesced
                    .last()
                    .is_some_and(|last: &PropertyKeyframe| last.time == key.time)
                {
                    *coalesced.last_mut().expect("just checked") = key;
                } else {
                    coalesced.push(key);
                }
            }
            track.keys = coalesced;
        }
        for event in &file.events {
            if !event.time.is_finite() || event.time < 0.0 || event.time > file.duration {
                return Err(FileError(format!(
                    "event `{}` is at {}s, outside the clip's duration",
                    event.name, event.time
                )));
            }
            if event.name.is_empty() {
                return Err(FileError("an event needs a name".into()));
            }
        }
        Ok(Self { file })
    }

    pub fn from_json(text: &str) -> Result<Self, FileError> {
        Self::from_file(PropertyClipFile::from_json(text)?)
    }

    pub fn duration(&self) -> f64 {
        self.file.duration
    }

    pub fn looping(&self) -> bool {
        self.file.looping
    }

    /// Sample time in clip seconds, after looping / clamping.
    pub fn sample_time(&self, time: f64) -> f64 {
        sample_time(time, self.file.duration, self.file.looping)
    }

    /// Every track's value at `time` (clip seconds; looping applied).
    pub fn evaluate(&self, time: f64) -> Vec<EvaluatedProperty> {
        let t = self.sample_time(time);
        self.file
            .tracks
            .iter()
            .filter_map(|track| {
                let value = sample_track(track, t)?;
                Some(EvaluatedProperty {
                    target: track.target.clone(),
                    property: track.property,
                    value,
                })
            })
            .collect()
    }

    /// Event names whose times fall in `[from, to)` in clip seconds, after
    /// mapping through looping the same way playback advances. `from` is
    /// counted, `to` is not. Seeking backward should pass an empty range.
    pub fn events_between(&self, from: f64, to: f64) -> Vec<String> {
        events_between(&self.file.events, self.file.duration, self.file.looping, from, to)
    }
}

/// One evaluated track binding.
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedProperty {
    pub target: String,
    pub property: PropertyName,
    pub value: Vector3,
}

impl EvaluatedProperty {
    pub fn as_value(&self) -> Value {
        Value::Vector3(self.value)
    }
}

/// Maps a playback clock into [0, duration], looping or holding at the end.
pub fn sample_time(time: f64, duration: f64, looping: bool) -> f64 {
    if !time.is_finite() {
        return 0.0;
    }
    if duration <= 0.0 {
        return 0.0;
    }
    if looping {
        let mut t = time % duration;
        if t < 0.0 {
            t += duration;
        }
        // Exactly on a loop boundary (time == n * duration) is the end of a
        // pass, which holds the last frame for sampling.
        if t == 0.0 && time > 0.0 {
            duration
        } else {
            t
        }
    } else {
        time.clamp(0.0, duration)
    }
}

fn sample_track(track: &PropertyTrack, time: f64) -> Option<Vector3> {
    let keys = &track.keys;
    if keys.is_empty() {
        return None;
    }
    if time <= keys[0].time {
        return Some(vector3(keys[0].value));
    }
    let last = keys.last().expect("non-empty");
    if time >= last.time {
        return Some(vector3(last.value));
    }
    let mut i = 0;
    while i + 1 < keys.len() && keys[i + 1].time <= time {
        i += 1;
    }
    let a = &keys[i];
    let b = &keys[i + 1];
    match a.interp {
        Interp::Step => Some(vector3(a.value)),
        Interp::Linear => {
            let span = (b.time - a.time).max(1e-12);
            let t = ((time - a.time) / span) as f32;
            Some(lerp_property(track.property, vector3(a.value), vector3(b.value), t))
        }
    }
}

fn lerp_property(property: PropertyName, a: Vector3, b: Vector3, t: f32) -> Vector3 {
    match property {
        PropertyName::Orientation => {
            let qa = quat_from_orientation(a);
            let qb = quat_from_orientation(b);
            orientation_from_quat(qa.slerp(qb, t))
        }
        PropertyName::Position | PropertyName::Size => Vector3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        ),
    }
}

fn vector3(v: [f32; 3]) -> Vector3 {
    Vector3::new(v[0], v[1], v[2])
}

fn quat_from_orientation(orientation: Vector3) -> Quat {
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

fn orientation_from_quat(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    let tidy = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
    Vector3::new(tidy(x), tidy(y), tidy(z))
}

fn events_between(
    events: &[ClipEvent],
    duration: f64,
    looping: bool,
    from: f64,
    to: f64,
) -> Vec<String> {
    if to.is_nan() || from.is_nan() || to <= from || events.is_empty() {
        return Vec::new();
    }
    let length = duration.max(1e-9);
    let mut hits: Vec<(f64, &str)> = Vec::new();
    for event in events {
        let at = event.time.clamp(0.0, length);
        if looping {
            let mut pass = (from - at).ceil();
            while pass + at < to {
                if pass + at >= from {
                    hits.push((pass + at, &event.name));
                }
                pass += 1.0;
                if hits.len() > 10_000 {
                    break;
                }
            }
        } else if (from..to).contains(&at) {
            hits.push((at, &event.name));
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    hits.into_iter().map(|(_, name)| name.to_owned()).collect()
}

/// Resolves a track `target` relative to `root`: `.` is `root`, otherwise
/// dotted child names (`Handle` or `Model/Handle`).
pub fn resolve_target(world: &World, root: InstanceId, target: &str) -> Option<InstanceId> {
    let target = target.trim();
    if target.is_empty() || target == "." {
        return Some(root);
    }
    let mut current = root;
    for segment in target.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        current = world.find_first_child(current, segment)?;
    }
    Some(current)
}

/// Applies evaluated properties under `root`. Missing targets are collected
/// once into `missing` (keyed by target path) and skipped.
pub fn apply_evaluated(
    world: &mut World,
    root: InstanceId,
    evaluated: &[EvaluatedProperty],
    missing: &mut HashMap<String, ()>,
) {
    for item in evaluated {
        let Some(id) = resolve_target(world, root, &item.target) else {
            missing.entry(item.target.clone()).or_insert(());
            continue;
        };
        let name = item.property.as_str();
        if world.set_property(id, name, item.as_value()).is_err() {
            missing.entry(format!("{}:{}", item.target, name)).or_insert(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Interp, PropertyKeyframe, PropertyName, PropertyTrack};

    fn door_clip() -> PropertyClip {
        PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 1.5,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![
                    PropertyKeyframe {
                        id: "k0".into(),
                        time: 0.0,
                        value: [0.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.5,
                        value: [0.0, 8.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![ClipEvent {
                time: 1.0,
                name: "Opened".into(),
            }],
        })
        .unwrap()
    }

    #[test]
    fn samples_before_on_between_and_at_end() {
        let clip = door_clip();
        let at = |t| {
            clip.evaluate(t)
                .into_iter()
                .find(|e| e.property == PropertyName::Position)
                .unwrap()
                .value
        };
        assert_eq!(at(-1.0), Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(at(0.0), Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(at(0.75), Vector3::new(0.0, 4.0, 0.0));
        assert_eq!(at(1.5), Vector3::new(0.0, 8.0, 0.0));
        assert_eq!(at(10.0), Vector3::new(0.0, 8.0, 0.0));
    }

    #[test]
    fn zero_duration_and_single_key() {
        let clip = PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 0.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![PropertyKeyframe {
                    id: "k0".into(),
                    time: 0.0,
                    value: [1.0, 2.0, 3.0],
                    interp: Interp::Linear,
                }],
            }],
            events: vec![],
        })
        .unwrap();
        assert_eq!(
            clip.evaluate(0.0)[0].value,
            Vector3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            clip.evaluate(5.0)[0].value,
            Vector3::new(1.0, 2.0, 3.0)
        );
    }

    #[test]
    fn looping_and_large_time_steps() {
        let mut file = door_clip().file;
        file.looping = true;
        file.duration = 1.0;
        file.tracks[0].keys[1].time = 1.0;
        file.tracks[0].keys[1].value = [0.0, 10.0, 0.0];
        file.events.clear();
        let clip = PropertyClip::from_file(file).unwrap();
        assert_eq!(clip.evaluate(0.5)[0].value.y, 5.0);
        assert_eq!(clip.evaluate(1.5)[0].value.y, 5.0);
        assert_eq!(clip.evaluate(2.0)[0].value.y, 10.0);
    }

    #[test]
    fn step_holds_until_next_key() {
        let clip = PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 2.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![
                    PropertyKeyframe {
                        id: "k0".into(),
                        time: 0.0,
                        value: [0.0, 0.0, 0.0],
                        interp: Interp::Step,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.0,
                        value: [0.0, 10.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![],
        })
        .unwrap();
        assert_eq!(clip.evaluate(0.9)[0].value.y, 0.0);
        assert_eq!(clip.evaluate(1.0)[0].value.y, 10.0);
    }

    #[test]
    fn orientation_uses_slerp() {
        let clip = PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 1.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "rot".into(),
                target: ".".into(),
                property: PropertyName::Orientation,
                keys: vec![
                    PropertyKeyframe {
                        id: "k0".into(),
                        time: 0.0,
                        value: [0.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.0,
                        value: [0.0, 90.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![],
        })
        .unwrap();
        let mid = clip.evaluate(0.5)[0].value;
        assert!((mid.y - 45.0).abs() < 0.1, "{mid:?}");
        assert!(mid.x.abs() < 0.1 && mid.z.abs() < 0.1, "{mid:?}");
    }

    #[test]
    fn events_fire_once_per_pass() {
        let clip = door_clip();
        // Half-open [from, to): the marker at 1.0 fires when the clock crosses it.
        assert_eq!(clip.events_between(0.0, 1.01), ["Opened"]);
        assert!(clip.events_between(1.01, 3.0).is_empty());
        assert!(clip.events_between(0.0, 1.0).is_empty());
    }

    #[test]
    fn json_round_trip() {
        let clip = door_clip();
        let text = clip.file.to_json();
        let again = PropertyClip::from_json(&text).unwrap();
        assert_eq!(again.file, clip.file);
        assert!(text.contains("\"kind\": \"property\""));
    }

    #[test]
    fn rejects_key_past_duration() {
        let err = PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 1.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![PropertyKeyframe {
                    id: "k0".into(),
                    time: 2.0,
                    value: [0.0, 0.0, 0.0],
                    interp: Interp::Linear,
                }],
            }],
            events: vec![],
        })
        .unwrap_err();
        assert!(err.0.contains("outside"), "{err}");
    }

    #[test]
    fn coalesce_duplicate_times() {
        let clip = PropertyClip::from_file(PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 1.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![
                    PropertyKeyframe {
                        id: "a".into(),
                        time: 0.5,
                        value: [1.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "b".into(),
                        time: 0.5,
                        value: [2.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![],
        })
        .unwrap();
        assert_eq!(clip.file.tracks[0].keys.len(), 1);
        assert_eq!(clip.file.tracks[0].keys[0].value[0], 2.0);
    }

    #[test]
    fn apply_and_missing_target() {
        let mut world = World::new();
        let part = world.create(axcenity_core::ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let clip = door_clip();
        let evaluated = clip.evaluate(0.75);
        let mut missing = HashMap::new();
        apply_evaluated(&mut world, part, &evaluated, &mut missing);
        assert!(missing.is_empty());
        assert_eq!(
            world.get(part).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(0.0, 4.0, 0.0)))
        );

        let bad = [EvaluatedProperty {
            target: "NoSuch".into(),
            property: PropertyName::Position,
            value: Vector3::new(0.0, 0.0, 0.0),
        }];
        apply_evaluated(&mut world, part, &bad, &mut missing);
        assert!(missing.contains_key("NoSuch"));
    }
}
