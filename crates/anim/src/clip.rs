//! A clip file bound to the skeleton it poses, with its root motion and
//! events.

use std::sync::Arc;

use axcenity_assets::{BoneTransform, Pose, Rig, Take};
use glam::{Mat4, Quat, Vec3};

use crate::files::ClipFile;

/// A clip ready to pose one rig: its file, its take, and which bone of the
/// rig each of the take's tracks moves (matched by name, so a take from an
/// animation-only file moves a model loaded from another).
#[derive(Clone, Debug)]
pub struct BoundClip {
    pub file: ClipFile,
    takes: Arc<Vec<Take>>,
    take: usize,
    binding: Vec<Option<usize>>,
    /// The bone root motion takes its travel from, if the clip has root
    /// motion on and the take moves one across the ground.
    root: Option<RootBone>,
}

/// The root bone and where it starts, in the file's space.
#[derive(Clone, Copy, Debug)]
struct RootBone {
    bone: usize,
    start: Vec3,
    /// Its heading (turn around Y, in radians) at the clip's start.
    start_yaw: f32,
}

/// How far a clip's root moved the model: across the ground (in the
/// model's file space, before fitting, already turned into the frame the
/// model faced when it started moving) and around Y (radians).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RootMotion {
    pub travel: Vec3,
    pub yaw: f32,
}

impl RootMotion {
    /// `self` counting `weight` (for blends).
    pub fn scaled(self, weight: f32) -> Self {
        Self {
            travel: self.travel * weight,
            yaw: self.yaw * weight,
        }
    }
}

/// Root motions add up: `a + b` moves by both.
impl std::ops::Add for RootMotion {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            travel: self.travel + other.travel,
            yaw: self.yaw + other.yaw,
        }
    }
}

impl BoundClip {
    /// Binds `file`'s take, found by name in `takes` (its source file's), to
    /// `rig`. Fails, saying why, if the take isn't there or moves none of
    /// the rig's bones.
    pub fn bind(file: ClipFile, takes: Arc<Vec<Take>>, rig: &Rig) -> Result<Self, String> {
        let take = takes
            .iter()
            .position(|take| take.name == file.take)
            .ok_or_else(|| {
                let names: Vec<String> = takes.iter().map(|t| format!("`{}`", t.name)).collect();
                if names.is_empty() {
                    format!("`{}` has no animation in it", file.source)
                } else {
                    format!(
                        "`{}` has no take named `{}` (it has {})",
                        file.source,
                        file.take,
                        names.join(", ")
                    )
                }
            })?;
        let binding = takes[take].bind(rig);
        if !binding.iter().any(Option::is_some) && !takes[take].tracks.is_empty() {
            return Err(format!(
                "the take `{}` moves none of the model's bones (it's for another skeleton)",
                file.take
            ));
        }
        let mut clip = Self {
            file,
            takes,
            take,
            binding,
            root: None,
        };
        if clip.file.root_motion {
            clip.root = clip.find_root(rig);
        }
        Ok(clip)
    }

    /// The first bone (parents come first, so the top-most) whose track
    /// really moves it across the ground during the clip.
    fn find_root(&self, rig: &Rig) -> Option<RootBone> {
        let take = self.take();
        let start = self.file.start as f32;
        let end = self.file.end as f32;
        let mut candidates: Vec<usize> = take
            .tracks
            .iter()
            .zip(&self.binding)
            .filter(|(track, _)| track.translation.values.len() > 1)
            .filter_map(|(_, bone)| *bone)
            .collect();
        candidates.sort_unstable();
        for bone in candidates {
            let at = |time: f32| self.root_world(rig, bone, time);
            let (first, _) = at(start);
            let travelled = (0..=8).any(|step| {
                let (position, _) = at(start + (end - start) * step as f32 / 8.0);
                let moved = position - first;
                Vec3::new(moved.x, 0.0, moved.z).length() > 1e-3
            });
            if travelled {
                let (start, start_yaw) = at(start);
                return Some(RootBone {
                    bone,
                    start,
                    start_yaw,
                });
            }
        }
        None
    }

    /// Where `bone` is in the file's space at `time` seconds of the take,
    /// and its heading.
    fn root_world(&self, rig: &Rig, bone: usize, time: f32) -> (Vec3, f32) {
        let mut pose = rig.rest_pose();
        self.take().sample(time, &self.binding, rig, &mut pose);
        let world = rig.world(&pose);
        let (_, rotation, position) = world[bone].to_scale_rotation_translation();
        (position, yaw(rotation))
    }

    pub fn take(&self) -> &Take {
        &self.takes[self.take]
    }

    /// True if root motion is on and found a bone to take it from.
    pub fn has_root_motion(&self) -> bool {
        self.root.is_some()
    }

    /// Seconds one pass of the clip lasts at its speed. Never 0, so phases
    /// always move on.
    pub fn duration(&self) -> f64 {
        let speed = self.file.speed.abs().max(1e-3);
        (self.file.length() / speed).max(1.0 / 60.0)
    }

    /// How far through one pass `phase` is, from 0 to 1: a looping clip
    /// goes round, one that doesn't holds at its end.
    fn through(&self, phase: f64) -> f64 {
        if self.file.looping {
            phase.rem_euclid(1.0)
        } else {
            phase.clamp(0.0, 1.0)
        }
    }

    /// Where in the take (seconds) the clip is `through` (0 to 1) one pass:
    /// a negative speed plays it backwards.
    fn time_through(&self, through: f64) -> f64 {
        let through = if self.file.speed < 0.0 {
            1.0 - through
        } else {
            through
        };
        self.file.start + through * self.file.length()
    }

    /// Where in the take (seconds) the clip is at `phase`: 0 is its start,
    /// 1 its end, 1.5 halfway through the second pass. A looping clip goes
    /// round; one that doesn't holds its end. A negative speed plays it
    /// backwards.
    pub fn time_at(&self, phase: f64) -> f64 {
        self.time_through(self.through(phase))
    }

    /// Moves the bones the clip animates in `pose` to where they are at
    /// `phase` (see [`time_at`](Self::time_at)). With root motion, the root
    /// bone stays over where it started, facing the way it started (its
    /// travel moves the MeshPart instead; see [`root_motion`](Self::root_motion));
    /// it still moves up and down.
    pub fn sample(&self, phase: f64, rig: &Rig, pose: &mut Pose) {
        let time = self.time_at(phase) as f32;
        self.take().sample(time, &self.binding, rig, pose);
        if let Some(root) = self.root {
            pin_root(rig, pose, root);
        }
    }

    /// The root's travel and turn from the clip's start to `through` one
    /// pass, in the frame it started in.
    fn root_offset(&self, rig: &Rig, root: RootBone, through: f64) -> (Vec3, f32) {
        let time = self.time_through(through) as f32;
        let (position, heading) = self.root_world(rig, root.bone, time);
        let moved = position - root.start;
        (Vec3::new(moved.x, 0.0, moved.z), heading - root.start_yaw)
    }

    /// How far root motion moves the model from phase `from` to phase `to`
    /// (`to` later), counting each whole pass of a looping clip. Zero
    /// without root motion.
    pub fn root_motion(&self, rig: &Rig, from: f64, to: f64) -> RootMotion {
        let Some(root) = self.root else {
            return RootMotion::default();
        };
        if to.is_nan() || from.is_nan() || to <= from {
            return RootMotion::default();
        }
        // The pieces of one pass the time covers, each from..to (0 to 1).
        let mut pieces = Vec::new();
        if self.file.looping {
            let (first, last) = (from.floor(), to.floor());
            if first == last {
                pieces.push((from - first, to - first));
            } else {
                pieces.push((from - first, 1.0));
                for _ in 0..((last - first) as i64 - 1).clamp(0, 1000) {
                    pieces.push((0.0, 1.0));
                }
                pieces.push((0.0, to - last));
            }
        } else {
            pieces.push((from.clamp(0.0, 1.0), to.clamp(0.0, 1.0)));
        }
        let mut motion = RootMotion::default();
        for (a, b) in pieces {
            let (start, start_yaw) = self.root_offset(rig, root, a);
            let (end, end_yaw) = self.root_offset(rig, root, b);
            // Turned from the way the root faced into the way the model
            // faces now, which already includes the turns before.
            let facing = Quat::from_rotation_y(motion.yaw - start_yaw);
            motion.travel += facing * (end - start);
            motion.yaw += end_yaw - start_yaw;
        }
        motion
    }

    /// The names of the events the clip passes from phase `from` (counted)
    /// to `to` (not), in order.
    pub fn events_between(&self, from: f64, to: f64) -> Vec<String> {
        if to.is_nan() || from.is_nan() || to <= from || self.file.events.is_empty() {
            return Vec::new();
        }
        let length = self.file.length().max(1e-9);
        let mut events: Vec<(f64, &str)> = Vec::new();
        for event in &self.file.events {
            let mut at = (event.time / length).clamp(0.0, 1.0);
            if self.file.speed < 0.0 {
                at = 1.0 - at;
            }
            if self.file.looping {
                let mut pass = (from - at).ceil();
                while pass + at < to {
                    if pass + at >= from {
                        events.push((pass + at, &event.name));
                    }
                    pass += 1.0;
                    if events.len() > 10_000 {
                        break;
                    }
                }
            } else if (from..to).contains(&at) {
                events.push((at, &event.name));
            }
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        events
            .into_iter()
            .map(|(_, name)| name.to_owned())
            .collect()
    }
}

/// A rotation's turn around Y (its twist about the vertical), in radians.
fn yaw(rotation: Quat) -> f32 {
    2.0 * rotation.y.atan2(rotation.w)
}

/// Keeps `root`'s bone over where it started and facing the way it
/// started, in `pose`, keeping its height and the rest of its turn.
fn pin_root(rig: &Rig, pose: &mut Pose, root: RootBone) {
    let world = rig.world(pose);
    let Some(bone) = world.get(root.bone) else {
        return;
    };
    let (scale, rotation, position) = bone.to_scale_rotation_translation();
    let turn = Quat::from_rotation_y(yaw(rotation) - root.start_yaw);
    let pinned = Mat4::from_scale_rotation_translation(
        scale,
        turn.inverse() * rotation,
        Vec3::new(root.start.x, position.y, root.start.z),
    );
    let parent = rig.bones[root.bone]
        .parent
        .and_then(|parent| world.get(parent).copied())
        .unwrap_or(Mat4::IDENTITY);
    let (scale, rotation, translation) =
        (parent.inverse() * pinned).to_scale_rotation_translation();
    pose[root.bone] = BoneTransform {
        translation,
        rotation,
        scale,
    };
}
