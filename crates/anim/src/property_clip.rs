//! Authored property clips: keyframed properties of Parts, Models and UI
//! elements that play without an FBX take.
//!
//! Sampling is pure: evaluate a time into property values, then the host
//! applies them ([`apply_evaluated`]). Orientation keys are degrees; linear
//! interpolation uses quaternion slerp for them and goes straight between
//! numbers for everything else.
//!
//! Applying moves more than each track's own target:
//! - A **Model** has no position of its own, so its `Position` and
//!   `Orientation` tracks drive its pivot: the middle of the box around its
//!   parts as they were when it was first played, turned by `Orientation`
//!   degrees from how it was built. Every part inside it moves with it.
//! - A **Part** whose `Position` or `Orientation` is animated carries the
//!   parts inside it (a door Panel inside its Hinge turns with the Hinge).
//!
//! Parents are applied before their children, so a child's own tracks
//! win over being carried. Values are where things are in the world, as
//! the Properties panel shows them.

use std::collections::{BTreeSet, HashMap};

use axcenity_core::{ClassName, Color3, InstanceId, UDim2, Value, Vector3, World};
use glam::{EulerRot, Quat, Vec3};

use crate::files::{
    ClipEvent, FileError, Interp, PropertyClipFile, PropertyKeyframe, PropertyName, PropertyTrack,
};

/// A validated property clip ready to sample.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyClip {
    pub file: PropertyClipFile,
}

impl PropertyClip {
    /// Validates `file` (duration, key times and values, sorted keys,
    /// coalesced duplicate times) and returns a clip ready to sample.
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
            let width = track.keys.first().map(|key| key.value.len());
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
                if !key.value.iter().all(|n| n.is_finite()) {
                    return Err(FileError(format!(
                        "track `{}` has a key with a non-finite value",
                        track.id
                    )));
                }
                let widths = track.property.widths();
                if !widths.contains(&key.value.len()) {
                    return Err(FileError(format!(
                        "track `{}` ({}) has a key with {} numbers; it takes {}",
                        track.id,
                        track.property.as_str(),
                        key.value.len(),
                        widths
                            .iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(" or ")
                    )));
                }
                if Some(key.value.len()) != width {
                    return Err(FileError(format!(
                        "track `{}` mixes keys with different numbers of values",
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
        events_between(
            &self.file.events,
            self.file.duration,
            self.file.looping,
            from,
            to,
        )
    }
}

/// One evaluated track binding.
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedProperty {
    pub target: String,
    pub property: PropertyName,
    /// The value as numbers (see [`PropertyKeyframe::value`]).
    pub value: Vec<f32>,
}

/// A property value as the numbers a key stores, or `None` for a value
/// that can't be keyed (text, enums).
pub fn numbers_of(value: &Value) -> Option<Vec<f32>> {
    Some(match value {
        Value::Number(n) => vec![*n as f32],
        Value::Vector3(v) => vec![v.x, v.y, v.z],
        Value::Color3(c) => vec![c.r, c.g, c.b],
        Value::UDim2(u) => vec![u.x.scale, u.x.offset, u.y.scale, u.y.offset],
        _ => return None,
    })
}

/// `numbers` as a value of the same kind as `like`, or `None` if they
/// don't fit it.
pub fn value_from_numbers(like: &Value, numbers: &[f32]) -> Option<Value> {
    Some(match (like, numbers) {
        (Value::Number(_), [n]) => Value::Number(f64::from(*n)),
        (Value::Vector3(_), [x, y, z]) => Value::Vector3(Vector3::new(*x, *y, *z)),
        (Value::Color3(_), [r, g, b]) => Value::Color3(Color3::new(*r, *g, *b)),
        (Value::UDim2(_), [xs, xo, ys, yo]) => Value::UDim2(UDim2::new(*xs, *xo, *ys, *yo)),
        _ => return None,
    })
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
        if t == 0.0 && time > 0.0 { duration } else { t }
    } else {
        time.clamp(0.0, duration)
    }
}

fn sample_track(track: &PropertyTrack, time: f64) -> Option<Vec<f32>> {
    let keys = &track.keys;
    if keys.is_empty() {
        return None;
    }
    if time <= keys[0].time {
        return Some(keys[0].value.clone());
    }
    let last = keys.last().expect("non-empty");
    if time >= last.time {
        return Some(last.value.clone());
    }
    let mut i = 0;
    while i + 1 < keys.len() && keys[i + 1].time <= time {
        i += 1;
    }
    let a = &keys[i];
    let b = &keys[i + 1];
    match a.interp {
        Interp::Step => Some(a.value.clone()),
        Interp::Linear => {
            let span = (b.time - a.time).max(1e-12);
            let t = ((time - a.time) / span) as f32;
            Some(lerp_property(track.property, &a.value, &b.value, t))
        }
    }
}

fn lerp_property(property: PropertyName, a: &[f32], b: &[f32], t: f32) -> Vec<f32> {
    if let (PropertyName::Orientation, [ax, ay, az], [bx, by, bz]) = (property, a, b) {
        let qa = quat_from_orientation(Vector3::new(*ax, *ay, *az));
        let qb = quat_from_orientation(Vector3::new(*bx, *by, *bz));
        let v = orientation_from_quat(qa.slerp(qb, t));
        return vec![v.x, v.y, v.z];
    }
    a.iter().zip(b).map(|(a, b)| a + (b - a) * t).collect()
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
/// child names joined by `/` (`Handle` or `Hinge/Panel`).
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

/// The target path of `id` from `root` (`.` for `root` itself), or `None`
/// if it isn't `root` or inside it.
pub fn target_path(world: &World, root: InstanceId, id: InstanceId) -> Option<String> {
    let mut names = Vec::new();
    let mut current = id;
    while current != root {
        let instance = world.get(current)?;
        names.push(instance.name().to_owned());
        current = instance.parent()?;
    }
    if names.is_empty() {
        return Some(".".to_owned());
    }
    names.reverse();
    Some(names.join("/"))
}

/// Where something is and how it's turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Placement {
    /// A Part or MeshPart's placement, or `None` for anything without a
    /// Vector3 `Position` and `Orientation`.
    pub fn of_part(world: &World, id: InstanceId) -> Option<Placement> {
        let instance = world.get(id)?;
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        let (position, orientation) = (vector("Position")?, vector("Orientation")?);
        Some(Placement {
            position: Vec3::new(position.x, position.y, position.z),
            rotation: quat_from_orientation(orientation),
        })
    }

    /// Puts a Part here (its `Position` and `Orientation`).
    fn put(self, world: &mut World, id: InstanceId) {
        let p = self.position;
        let _ = world.set_property(id, "Position", Value::Vector3(Vector3::new(p.x, p.y, p.z)));
        let orientation = orientation_from_quat(self.rotation);
        let _ = world.set_property(id, "Orientation", Value::Vector3(orientation));
    }

    /// `other` (in the world) as seen from here.
    fn relative(self, other: Placement) -> Placement {
        let inverse = self.rotation.inverse();
        Placement {
            position: inverse * (other.position - self.position),
            rotation: inverse * other.rotation,
        }
    }

    /// `local` (relative to here) in the world.
    fn then(self, local: Placement) -> Placement {
        Placement {
            position: self.position + self.rotation * local.position,
            rotation: self.rotation * local.rotation,
        }
    }

    /// The pivot keys store for this placement: `[x, y, z]` position and
    /// `[x, y, z]` Orientation degrees.
    pub fn numbers(self) -> ([f32; 3], [f32; 3]) {
        let o = orientation_from_quat(self.rotation);
        (self.position.to_array(), [o.x, o.y, o.z])
    }
}

/// A Model's pivot when nothing has moved it: the middle of the box around
/// its parts, turned the way it was built.
pub fn model_pivot(world: &World, model: InstanceId) -> Option<Placement> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for id in world.descendants(model) {
        let Some(placement) = Placement::of_part(world, id) else {
            continue;
        };
        let size = world
            .get(id)
            .and_then(|i| i.property("Size"))
            .and_then(Value::as_vector3)
            .map_or(Vec3::ZERO, |s| Vec3::new(s.x, s.y, s.z));
        let reach = [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .zip(size.to_array())
            .fold(Vec3::ZERO, |sum, (axis, length)| {
                sum + (placement.rotation * axis).abs() * length / 2.0
            });
        min = min.min(placement.position - reach);
        max = max.max(placement.position + reach);
    }
    min.x.is_finite().then(|| Placement {
        position: (min + max) / 2.0,
        rotation: Quat::IDENTITY,
    })
}

/// What applying a clip remembers from one frame to the next.
#[derive(Debug, Default)]
pub struct Applied {
    /// Targets (`path`) and properties (`path:Property`) that couldn't be
    /// set, so a host reports each once.
    pub missing: BTreeSet<String>,
    /// For each Model or Part that carries parts: where it was when first
    /// played, and where each part inside it sat relative to that.
    rest: HashMap<InstanceId, Rest>,
    /// Where each animated Model's pivot was put last.
    pivots: HashMap<InstanceId, Placement>,
}

#[derive(Debug)]
struct Rest {
    at: Placement,
    parts: Vec<(InstanceId, Placement)>,
}

impl Applied {
    /// Where `model`'s pivot is now: where its tracks last put it, or where
    /// it is unmoved.
    pub fn pivot(&self, world: &World, model: InstanceId) -> Option<Placement> {
        self.pivots
            .get(&model)
            .copied()
            .or_else(|| model_pivot(world, model))
    }

    /// Remembers where `id` (at `at`) and the parts inside it are, the first
    /// time it carries them.
    fn rest(&mut self, world: &World, id: InstanceId, at: Placement) -> &Rest {
        self.rest.entry(id).or_insert_with(|| Rest {
            at,
            parts: world
                .descendants(id)
                .into_iter()
                .filter_map(|part| Some((part, at.relative(Placement::of_part(world, part)?))))
                .collect(),
        })
    }
}

/// Applies evaluated properties under `root`, parents before children.
/// Targets or properties that can't be set are added to `applied.missing`
/// and skipped.
pub fn apply_evaluated(
    world: &mut World,
    root: InstanceId,
    evaluated: &[EvaluatedProperty],
    applied: &mut Applied,
) {
    // Group by target, keeping the clip's order, then sort parents first.
    let mut targets: Vec<(&str, Vec<&EvaluatedProperty>)> = Vec::new();
    for item in evaluated {
        match targets.iter_mut().find(|(t, _)| *t == item.target) {
            Some((_, items)) => items.push(item),
            None => targets.push((&item.target, vec![item])),
        }
    }
    let depth = |target: &str| {
        target
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .count()
    };
    targets.sort_by_key(|(target, _)| depth(target));

    for (target, items) in targets {
        let Some(id) = resolve_target(world, root, target) else {
            applied.missing.insert(target.to_owned());
            continue;
        };
        let moves = |item: &&EvaluatedProperty| {
            matches!(
                item.property,
                PropertyName::Position | PropertyName::Orientation
            ) && item.value.len() == 3
        };
        let is_model = world.get(id).is_some_and(|i| i.class() == ClassName::Model);
        if is_model {
            let Some(start) = model_pivot(world, id) else {
                continue;
            };
            let rest = applied.rest(world, id, start).at;
            let mut pivot = rest;
            for item in &items {
                match (item.property, item.value.as_slice()) {
                    (PropertyName::Position, [x, y, z]) => pivot.position = Vec3::new(*x, *y, *z),
                    (PropertyName::Orientation, [x, y, z]) => {
                        pivot.rotation = quat_from_orientation(Vector3::new(*x, *y, *z))
                    }
                    _ => {
                        applied
                            .missing
                            .insert(format!("{target}:{}", item.property.as_str()));
                    }
                }
            }
            carry(world, applied, id, pivot);
            applied.pivots.insert(id, pivot);
            continue;
        }
        // A part that moves carries the parts inside it from where they
        // were the first time it did.
        let carries = items.iter().any(moves)
            && world
                .descendants(id)
                .iter()
                .any(|&d| Placement::of_part(world, d).is_some());
        if carries && let Some(at) = Placement::of_part(world, id) {
            applied.rest(world, id, at);
        }
        for item in &items {
            let name = item.property.as_str();
            let value = world
                .get(id)
                .and_then(|i| i.property(name))
                .and_then(|like| value_from_numbers(like, &item.value));
            let set = value.is_some_and(|value| world.set_property(id, name, value).is_ok());
            if !set {
                applied.missing.insert(format!("{target}:{name}"));
            }
        }
        if carries && let Some(now) = Placement::of_part(world, id) {
            carry(world, applied, id, now);
        }
    }
}

/// Puts every part `id` carries where it sits relative to `id` at `now`.
fn carry(world: &mut World, applied: &Applied, id: InstanceId, now: Placement) {
    let Some(rest) = applied.rest.get(&id) else {
        return;
    };
    for &(part, local) in &rest.parts {
        now.then(local).put(world, part);
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
                        value: vec![0.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.5,
                        value: vec![0.0, 8.0, 0.0],
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
        assert_eq!(at(-1.0), vec![0.0, 0.0, 0.0]);
        assert_eq!(at(0.0), vec![0.0, 0.0, 0.0]);
        assert_eq!(at(0.75), vec![0.0, 4.0, 0.0]);
        assert_eq!(at(1.5), vec![0.0, 8.0, 0.0]);
        assert_eq!(at(10.0), vec![0.0, 8.0, 0.0]);
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
                    value: vec![1.0, 2.0, 3.0],
                    interp: Interp::Linear,
                }],
            }],
            events: vec![],
        })
        .unwrap();
        assert_eq!(clip.evaluate(0.0)[0].value, vec![1.0, 2.0, 3.0]);
        assert_eq!(clip.evaluate(5.0)[0].value, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn looping_and_large_time_steps() {
        let mut file = door_clip().file;
        file.looping = true;
        file.duration = 1.0;
        file.tracks[0].keys[1].time = 1.0;
        file.tracks[0].keys[1].value = vec![0.0, 10.0, 0.0];
        file.events.clear();
        let clip = PropertyClip::from_file(file).unwrap();
        assert_eq!(clip.evaluate(0.5)[0].value[1], 5.0);
        assert_eq!(clip.evaluate(1.5)[0].value[1], 5.0);
        assert_eq!(clip.evaluate(2.0)[0].value[1], 10.0);
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
                        value: vec![0.0, 0.0, 0.0],
                        interp: Interp::Step,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.0,
                        value: vec![0.0, 10.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![],
        })
        .unwrap();
        assert_eq!(clip.evaluate(0.9)[0].value[1], 0.0);
        assert_eq!(clip.evaluate(1.0)[0].value[1], 10.0);
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
                        value: vec![0.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "k1".into(),
                        time: 1.0,
                        value: vec![0.0, 90.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![],
        })
        .unwrap();
        let mid = clip.evaluate(0.5)[0].value.clone();
        assert!((mid[1] - 45.0).abs() < 0.1, "{mid:?}");
        assert!(mid[0].abs() < 0.1 && mid[2].abs() < 0.1, "{mid:?}");
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
                    value: vec![0.0, 0.0, 0.0],
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
                        value: vec![1.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "b".into(),
                        time: 0.5,
                        value: vec![2.0, 0.0, 0.0],
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
        let mut applied = Applied::default();
        apply_evaluated(&mut world, part, &evaluated, &mut applied);
        assert!(applied.missing.is_empty());
        assert_eq!(
            world.get(part).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(0.0, 4.0, 0.0)))
        );

        let bad = [EvaluatedProperty {
            target: "NoSuch".into(),
            property: PropertyName::Position,
            value: vec![0.0, 0.0, 0.0],
        }];
        apply_evaluated(&mut world, part, &bad, &mut applied);
        assert!(applied.missing.contains("NoSuch"));
    }

    fn part_in(world: &mut World, parent: InstanceId, name: &str, position: Vec3) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, name).unwrap();
        let p = Vector3::new(position.x, position.y, position.z);
        world
            .set_property(part, "Position", Value::Vector3(p))
            .unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 1.0, 1.0)))
            .unwrap();
        world.set_parent(part, Some(parent)).unwrap();
        part
    }

    fn position(world: &World, id: InstanceId) -> Vec3 {
        let p = world
            .get(id)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap();
        Vec3::new(p.x, p.y, p.z)
    }

    fn one(target: &str, property: PropertyName, value: &[f32]) -> EvaluatedProperty {
        EvaluatedProperty {
            target: target.into(),
            property,
            value: value.to_vec(),
        }
    }

    #[test]
    fn a_turning_hinge_carries_the_panel_inside_it() {
        let mut world = World::new();
        let door = world.create(ClassName::Model).unwrap();
        world.set_parent(door, Some(world.workspace())).unwrap();
        let hinge = part_in(&mut world, door, "Hinge", Vec3::ZERO);
        let panel = part_in(&mut world, hinge, "Panel", Vec3::new(2.0, 0.0, 0.0));
        let mut applied = Applied::default();
        let turn = [one("Hinge", PropertyName::Orientation, &[0.0, 90.0, 0.0])];
        apply_evaluated(&mut world, door, &turn, &mut applied);
        // Turning 90° around Y swings +X to -Z.
        assert!(position(&world, panel).abs_diff_eq(Vec3::new(0.0, 0.0, -2.0), 1e-4));
        let o = world.get(panel).unwrap().property("Orientation").cloned();
        assert_eq!(o, Some(Value::Vector3(Vector3::new(0.0, 90.0, 0.0))));
        // Back again, from where the panel first was, so nothing drifts.
        let back = [one("Hinge", PropertyName::Orientation, &[0.0, 0.0, 0.0])];
        apply_evaluated(&mut world, door, &back, &mut applied);
        assert!(position(&world, panel).abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-4));
    }

    #[test]
    fn a_model_pivot_moves_every_part_inside_it() {
        let mut world = World::new();
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(world.workspace())).unwrap();
        let a = part_in(&mut world, model, "A", Vec3::new(-1.0, 0.0, 0.0));
        let b = part_in(&mut world, model, "B", Vec3::new(1.0, 0.0, 0.0));
        let pivot = model_pivot(&world, model).unwrap();
        assert!(pivot.position.abs_diff_eq(Vec3::ZERO, 1e-5));

        let mut applied = Applied::default();
        let moved = [
            one(".", PropertyName::Position, &[0.0, 5.0, 0.0]),
            one(".", PropertyName::Orientation, &[0.0, 180.0, 0.0]),
        ];
        apply_evaluated(&mut world, model, &moved, &mut applied);
        assert!(applied.missing.is_empty(), "{:?}", applied.missing);
        assert!(position(&world, a).abs_diff_eq(Vec3::new(1.0, 5.0, 0.0), 1e-4));
        assert!(position(&world, b).abs_diff_eq(Vec3::new(-1.0, 5.0, 0.0), 1e-4));
        let now = applied.pivot(&world, model).unwrap();
        assert!(now.position.abs_diff_eq(Vec3::new(0.0, 5.0, 0.0), 1e-5));

        // A Model has no Color of its own.
        apply_evaluated(
            &mut world,
            model,
            &[one(".", PropertyName::Color, &[1.0, 0.0, 0.0])],
            &mut applied,
        );
        assert!(applied.missing.contains(".:Color"));
    }

    #[test]
    fn colors_numbers_and_ui_udims_apply_by_their_kind() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = part_in(&mut world, workspace, "P", Vec3::ZERO);
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_name(frame, "Menu").unwrap();
        world.set_parent(frame, Some(gui)).unwrap();

        let mut applied = Applied::default();
        let part_values = [
            one(".", PropertyName::Color, &[1.0, 0.5, 0.0]),
            one(".", PropertyName::Transparency, &[0.25]),
        ];
        apply_evaluated(&mut world, part, &part_values, &mut applied);
        let ui_values = [
            one("Menu", PropertyName::Position, &[0.5, -100.0, 0.0, 20.0]),
            one("Menu", PropertyName::BackgroundTransparency, &[0.5]),
        ];
        apply_evaluated(&mut world, gui, &ui_values, &mut applied);
        assert!(applied.missing.is_empty(), "{:?}", applied.missing);
        let part = world.get(part).unwrap();
        assert_eq!(
            part.property("Color"),
            Some(&Value::Color3(Color3::new(1.0, 0.5, 0.0)))
        );
        assert_eq!(part.property("Transparency"), Some(&Value::Number(0.25)));
        let frame = world.get(frame).unwrap();
        assert_eq!(
            frame.property("Position"),
            Some(&Value::UDim2(UDim2::new(0.5, -100.0, 0.0, 20.0)))
        );

        // Three numbers don't fit a UDim2.
        let wrong = [one("Menu", PropertyName::Size, &[1.0, 2.0, 3.0])];
        apply_evaluated(&mut world, gui, &wrong, &mut applied);
        assert!(applied.missing.contains("Menu:Size"));
    }

    #[test]
    fn keys_must_fit_their_property() {
        let mut file = door_clip().file;
        file.tracks[0].property = PropertyName::Transparency;
        let err = PropertyClip::from_file(file).unwrap_err();
        assert!(err.0.contains("takes 1"), "{err}");

        let mut file = door_clip().file;
        file.tracks[0].keys[1].value = vec![0.0, 0.5, 0.0, 10.0];
        let err = PropertyClip::from_file(file).unwrap_err();
        assert!(err.0.contains("mixes"), "{err}");
    }

    #[test]
    fn older_position_only_clips_still_read_the_same() {
        let text = r#"{
  "format_version": 1,
  "kind": "property",
  "duration": 1.5,
  "loop": false,
  "tracks": [
    { "id": "position", "target": ".", "property": "Position",
      "keys": [ { "id": "k1", "time": 0.0, "value": [0, 2, 0], "interp": "Linear" },
                { "id": "k2", "time": 1.5, "value": [0, 8, 0], "interp": "Linear" } ] }
  ],
  "events": [ { "time": 1.4, "name": "Opened" } ]
}"#;
        let clip = PropertyClip::from_json(text).unwrap();
        assert_eq!(clip.evaluate(0.75)[0].value, vec![0.0, 5.0, 0.0]);
    }

    #[test]
    fn target_paths_go_from_the_root() {
        let mut world = World::new();
        let door = world.create(ClassName::Model).unwrap();
        world.set_parent(door, Some(world.workspace())).unwrap();
        let hinge = part_in(&mut world, door, "Hinge", Vec3::ZERO);
        let panel = part_in(&mut world, hinge, "Panel", Vec3::ZERO);
        assert_eq!(target_path(&world, door, door).as_deref(), Some("."));
        let path = target_path(&world, door, panel).unwrap();
        assert_eq!(path, "Hinge/Panel");
        assert_eq!(resolve_target(&world, door, &path), Some(panel));
        assert_eq!(target_path(&world, hinge, door), None);
    }
}
