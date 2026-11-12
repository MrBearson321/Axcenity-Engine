//! A clip made ready to play: its tracks checked, and, on a skeleton, its
//! linked take and bone tracks bound to the bones they move.
//!
//! Playback counts in **phases**: 0 is the clip's start, 1 its end, 1.5
//! halfway through a second pass. A looping clip goes round; one that
//! doesn't holds its last frame.

use std::sync::Arc;

use axcenity_assets::{BoneTransform, Pose, Rig, Take};
use glam::{Mat4, Quat, Vec3};

use crate::bones::{bone_from_numbers, bone_paths, find_bone};
use crate::files::{ClipFile, PropertyName, TakeLink};
use crate::property_clip::{EvaluatedProperty, PropertyClip, sample_track};
use crate::retarget::{HumanoidMap, Retarget};

/// A clip ready to play on one thing: its property tracks checked and,
/// with a rig, its take and bone tracks bound to the rig's bones (matched
/// by name, so a take from an animation-only file moves a model loaded from
/// another).
#[derive(Clone, Debug)]
pub struct Clip {
    clip: PropertyClip,
    /// The linked take, bound to the rig. `None` without a link or a rig.
    take: Option<BoundTake>,
    /// Each bone track's rig bone and its index in the file's tracks.
    bones: Vec<(usize, usize)>,
    /// One per rig bone: whether the clip moves it.
    animated: Vec<bool>,
}

#[derive(Clone, Debug)]
struct BoundTake {
    link: TakeLink,
    takes: Arc<Vec<Take>>,
    take: usize,
    binding: Vec<Option<usize>>,
    /// Set when the take is for another skeleton and plays through humanoid
    /// roles (G16); `binding` is then unused.
    retarget: Option<Retarget>,
    /// The bone root motion takes its travel from, if the link has root
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

impl Clip {
    /// Makes `file` ready to play. With a `rig`, its link's take (found by
    /// name in `takes`, the takes of the link's source file) and its bone
    /// tracks are bound to the rig's bones; bone tracks for bones the take
    /// moves are left out, since the take moves those. Fails, saying why, if
    /// a track is broken, or the take isn't there or moves none of the rig's
    /// bones. Without a rig, only its property tracks play.
    pub fn bind(
        file: ClipFile,
        takes: Option<Arc<Vec<Take>>>,
        rig: Option<&Rig>,
    ) -> Result<Self, String> {
        let clip = PropertyClip::from_file(file).map_err(|err| err.0)?;
        let mut animated = rig.map_or_else(Vec::new, |rig| vec![false; rig.bones.len()]);
        let take = match (&clip.file.link, rig) {
            (Some(link), Some(rig)) => {
                let takes =
                    takes.ok_or_else(|| format!("its model `{}` can't be read", link.source))?;
                let take = BoundTake::bind(link.clone(), takes, rig)?;
                for bone in take.targets() {
                    if let Some(slot) = animated.get_mut(bone) {
                        *slot = true;
                    }
                }
                Some(take)
            }
            _ => None,
        };
        let mut bones = Vec::new();
        if let Some(rig) = rig {
            let paths = bone_paths(rig);
            for (index, track) in clip.file.tracks.iter().enumerate() {
                if track.property != PropertyName::Bone {
                    continue;
                }
                let Some(bone) = find_bone(rig, &paths, &track.target) else {
                    continue;
                };
                if !animated[bone] {
                    animated[bone] = true;
                    bones.push((bone, index));
                }
            }
        }
        Ok(Self {
            clip,
            take,
            bones,
            animated,
        })
    }

    pub fn file(&self) -> &ClipFile {
        &self.clip.file
    }

    pub fn looping(&self) -> bool {
        self.clip.file.looping
    }

    /// Seconds one pass lasts. Never 0, so phases always move on.
    pub fn duration(&self) -> f64 {
        self.clip.file.duration.max(1.0 / 60.0)
    }

    /// The linked take, if it's bound to a rig.
    pub fn take(&self) -> Option<&Take> {
        self.take.as_ref().map(BoundTake::take)
    }

    /// True if root motion is on and found a bone to take it from.
    pub fn has_root_motion(&self) -> bool {
        self.take.as_ref().is_some_and(|take| take.root.is_some())
    }

    /// One per rig bone: whether the clip moves it (by its take or a bone
    /// track). Empty when it wasn't bound to a rig.
    pub fn animated_bones(&self) -> &[bool] {
        &self.animated
    }

    /// How far through one pass `phase` is, from 0 to 1: a looping clip
    /// goes round, one that doesn't holds at its end.
    fn through(&self, phase: f64) -> f64 {
        if !phase.is_finite() {
            return 0.0;
        }
        if self.looping() {
            phase.rem_euclid(1.0)
        } else {
            phase.clamp(0.0, 1.0)
        }
    }

    /// Seconds into the clip at `phase` (see [`through`](Self::through)).
    pub fn seconds_at(&self, phase: f64) -> f64 {
        self.through(phase) * self.clip.file.duration
    }

    /// Moves the bones the clip animates in `pose` to where they are at
    /// `phase`: those its take moves, then its bone tracks. With root
    /// motion, the root bone stays over where it started, facing the way it
    /// started (its travel moves the MeshPart instead; see
    /// [`root_motion`](Self::root_motion)); it still moves up and down.
    pub fn sample(&self, phase: f64, rig: &Rig, pose: &mut Pose) {
        let through = self.through(phase);
        if let Some(take) = &self.take {
            take.sample(through, rig, pose);
        }
        let seconds = through * self.clip.file.duration;
        for &(bone, index) in &self.bones {
            let track = &self.clip.file.tracks[index];
            if let (Some(values), Some(slot)) = (sample_track(track, seconds), pose.get_mut(bone))
                && let Some(transform) = bone_from_numbers(&values)
            {
                *slot = transform;
            }
        }
    }

    /// Every property track's value at `phase`.
    pub fn properties(&self, phase: f64) -> Vec<EvaluatedProperty> {
        self.clip.evaluate(self.seconds_at(phase))
    }

    /// How far root motion moves the model from phase `from` to phase `to`
    /// (`to` later), counting each whole pass of a looping clip. Zero
    /// without root motion.
    pub fn root_motion(&self, rig: &Rig, from: f64, to: f64) -> RootMotion {
        let Some(take) = &self.take else {
            return RootMotion::default();
        };
        let Some(root) = take.root else {
            return RootMotion::default();
        };
        if to.is_nan() || from.is_nan() || to <= from {
            return RootMotion::default();
        }
        // The pieces of one pass the time covers, each from..to (0 to 1).
        let mut pieces = Vec::new();
        if self.looping() {
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
            let (start, start_yaw) = take.root_offset(rig, root, a);
            let (end, end_yaw) = take.root_offset(rig, root, b);
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
        let events = &self.clip.file.events;
        if to.is_nan() || from.is_nan() || to <= from || events.is_empty() {
            return Vec::new();
        }
        let length = self.clip.file.duration.max(1e-9);
        let mut hits: Vec<(f64, &str)> = Vec::new();
        for event in events {
            let at = (event.time / length).clamp(0.0, 1.0);
            if self.looping() {
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
}

/// Where in the take (seconds) a link is `through` (0 to 1) one pass.
fn link_time(link: &TakeLink, through: f64) -> f64 {
    link.start + through * (link.end - link.start)
}

impl BoundTake {
    /// Binds `link`'s take, found by name in `takes`, to `rig`.
    fn bind(link: TakeLink, takes: Arc<Vec<Take>>, rig: &Rig) -> Result<Self, String> {
        let take = takes
            .iter()
            .position(|take| take.name == link.take)
            .ok_or_else(|| {
                let names: Vec<String> = takes.iter().map(|t| format!("`{}`", t.name)).collect();
                if names.is_empty() {
                    format!("`{}` has no animation in it", link.source)
                } else {
                    format!(
                        "`{}` has no take named `{}` (it has {})",
                        link.source,
                        link.take,
                        names.join(", ")
                    )
                }
            })?;
        let binding = takes[take].bind(rig);
        // A take for another skeleton (other bone names) plays through
        // humanoid roles, unless it moves some of the model's humanoid
        // bones by name (the same skeleton, or one named like it).
        let named = HumanoidMap::detect(&rig.bones)
            .bones()
            .any(|bone| binding.contains(&Some(bone)));
        let mut retarget = None;
        if !named && !takes[take].tracks.is_empty() {
            match Retarget::new(&takes[take], rig) {
                Ok(bound) => retarget = Some(bound),
                Err(why) if !binding.iter().any(Option::is_some) => {
                    return Err(format!(
                        "the take `{}` moves none of the model's bones: it's for another \
                         skeleton, and can't be retargeted because {why}",
                        link.take
                    ));
                }
                // Some bones match by name: play those, as before.
                Err(_) => {}
            }
        }
        let mut bound = Self {
            link,
            takes,
            take,
            binding,
            retarget,
            root: None,
        };
        if bound.link.root_motion {
            bound.root = bound.find_root(rig);
        }
        Ok(bound)
    }

    fn take(&self) -> &Take {
        &self.takes[self.take]
    }

    /// The rig's bones the take moves.
    fn targets(&self) -> Vec<usize> {
        match &self.retarget {
            Some(retarget) => retarget.targets().collect(),
            None => self.binding.iter().flatten().copied().collect(),
        }
    }

    /// Poses the bones the take moves at `time` seconds into it.
    fn pose_at(&self, time: f32, rig: &Rig, pose: &mut Pose) {
        match &self.retarget {
            Some(retarget) => retarget.sample(self.take(), time, rig, pose),
            None => self.take().sample(time, &self.binding, rig, pose),
        }
    }

    /// The first bone (parents come first, so the top-most) whose track
    /// really moves it across the ground during the clip.
    fn find_root(&self, rig: &Rig) -> Option<RootBone> {
        let take = self.take();
        let mut candidates: Vec<usize> = match &self.retarget {
            Some(retarget) => vec![retarget.hips()],
            None => take
                .tracks
                .iter()
                .zip(&self.binding)
                .filter(|(track, _)| track.translation.values.len() > 1)
                .filter_map(|(_, bone)| *bone)
                .collect(),
        };
        candidates.sort_unstable();
        for bone in candidates {
            let at = |through: f64| self.root_world(rig, bone, link_time(&self.link, through));
            let (first, _) = at(0.0);
            let travelled = (0..=8).any(|step| {
                let (position, _) = at(f64::from(step) / 8.0);
                let moved = position - first;
                Vec3::new(moved.x, 0.0, moved.z).length() > 1e-3
            });
            if travelled {
                let (start, start_yaw) = at(0.0);
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
    fn root_world(&self, rig: &Rig, bone: usize, time: f64) -> (Vec3, f32) {
        let mut pose = rig.rest_pose();
        self.pose_at(time as f32, rig, &mut pose);
        let world = rig.world(&pose);
        let (_, rotation, position) = world[bone].to_scale_rotation_translation();
        (position, yaw(rotation))
    }

    fn sample(&self, through: f64, rig: &Rig, pose: &mut Pose) {
        let time = link_time(&self.link, through) as f32;
        self.pose_at(time, rig, pose);
        if let Some(root) = self.root {
            pin_root(rig, pose, root);
        }
    }

    /// The root's travel and turn from the clip's start to `through` one
    /// pass, in the frame it started in.
    fn root_offset(&self, rig: &Rig, root: RootBone, through: f64) -> (Vec3, f32) {
        let (position, heading) = self.root_world(rig, root.bone, link_time(&self.link, through));
        let moved = position - root.start;
        (Vec3::new(moved.x, 0.0, moved.z), heading - root.start_yaw)
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
