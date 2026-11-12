//! Lag compensation (Phase 81). A player sees the server's world a little
//! late: the update takes half their ping to arrive, and their device
//! shows parts `net::motion::DELAY` behind the newest motion it has. By
//! the time their shot reaches the server, a moving target has moved on.
//!
//! The server keeps where each part that moved was over the last
//! [`HISTORY_SECONDS`] ([`LagHistory::record`], from the host's network
//! send). A server raycast with `RaycastParams.CompensateFor = player`, or
//! one made in the handler of a RemoteEvent inside a Tool with
//! `LagCompensated` on (before it first waits), tests the moving parts
//! where they were that long ago, as that player saw them: their round
//! trip plus the smoothing delay, at most [`MAX_REWIND`]. Their own
//! character is tested where it is now. Parts that didn't move in that
//! time are where they are now, so a big still world costs nothing.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use axcenity_core::{Instance, InstanceId, Value, Vector3, World};
use axcenity_physics::PastPose;
use rustc_hash::{FxHashMap, FxHashSet};

/// How far back the history goes, in seconds.
pub const HISTORY_SECONDS: f64 = 1.0;
/// The most a shot is turned back: a player whose ping is worse is
/// compensated this far only, so a very late shot can't hit what's long
/// gone.
pub const MAX_REWIND: f64 = 1.0;

/// The server's record of where moving parts were (see the module docs).
#[derive(Default)]
pub struct LagHistory {
    /// The server's clock at the last record.
    clock: f64,
    /// The Workspace's structure version `parts` is up to date with.
    workspace: Option<u64>,
    /// Every part, its motion revision and pose when last read.
    parts: FxHashMap<InstanceId, Seen>,
    /// The poses of the parts that moved in the last second, oldest first:
    /// each holds from its time until the next.
    tracks: FxHashMap<InstanceId, VecDeque<(f64, PastPose)>>,
    /// How far back each Player's shots are turned, in seconds.
    rewind: FxHashMap<InstanceId, f64>,
}

struct Seen {
    motion: u64,
    pose: PastPose,
}

impl LagHistory {
    /// Notes where the parts that moved since the last call are now, at
    /// the server's `clock`, and forgets what's older than the history.
    pub fn record(&mut self, world: &World, clock: f64) {
        let before = self.clock;
        self.clock = clock;
        self.catch_up(world);
        for (&id, seen) in &mut self.parts {
            let Some(part) = world.get(id) else {
                continue;
            };
            if part.motion_revision() == seen.motion {
                continue;
            }
            seen.motion = part.motion_revision();
            let now = pose_of(part);
            let track = self.tracks.entry(id).or_default();
            if track.is_empty() {
                // Where it stood until now.
                track.push_back((before, seen.pose));
            }
            track.push_back((clock, now));
            seen.pose = now;
        }
        let oldest = clock - HISTORY_SECONDS;
        self.tracks.retain(|_, track| {
            // Keep the pose that held at `oldest`.
            while track.len() >= 2 && track[1].0 <= oldest {
                track.pop_front();
            }
            // Still for the whole second: where it is now is right.
            track.back().is_some_and(|&(time, _)| time > oldest)
        });
    }

    /// Reads parts that came into the Workspace (or every part, the first
    /// time or when what moved isn't known), and forgets those that left.
    fn catch_up(&mut self, world: &World) {
        let workspace = world.workspace();
        let version = world.structure_version_of(workspace);
        if self.workspace == Some(version) {
            return;
        }
        let moves = self
            .workspace
            .and_then(|before| world.workspace_moves_since(before));
        self.workspace = Some(version);
        let tops = match moves {
            Some(moves) => {
                self.parts
                    .retain(|&id, _| world.is_descendant_of(id, workspace));
                self.tracks
                    .retain(|&id, _| world.is_descendant_of(id, workspace));
                moves
            }
            None => {
                self.parts.clear();
                self.tracks.clear();
                world
                    .get(workspace)
                    .map(|workspace| workspace.children().to_vec())
                    .unwrap_or_default()
            }
        };
        for top in tops {
            if !world.is_descendant_of(top, workspace) {
                continue;
            }
            for id in std::iter::once(top).chain(world.descendants_iter(top)) {
                if let Some(part) = world.get(id)
                    && part.class().is_base_part()
                {
                    self.parts.entry(id).or_insert_with(|| Seen {
                        motion: part.motion_revision(),
                        pose: pose_of(part),
                    });
                }
            }
        }
    }

    /// How far back `player`'s shots are turned: their round trip plus the
    /// smoothing delay, as the host measures it. 0 for the host's own.
    pub fn set_rewind(&mut self, player: InstanceId, seconds: f64) {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, MAX_REWIND)
        } else {
            0.0
        };
        self.rewind.insert(player, seconds);
    }

    /// Forgets players who left.
    pub fn keep_players(&mut self, world: &World) {
        self.rewind.retain(|&player, _| world.contains(player));
    }

    /// When `player` saw the world they shot at, if they're compensated.
    pub fn seen_by(&self, player: InstanceId) -> Option<f64> {
        let rewind = *self.rewind.get(&player)?;
        (rewind > 0.0).then_some(self.clock - rewind)
    }

    /// Where part `id` was at `time`, if it moved since: `None` means
    /// where it is now.
    pub fn pose_at(&self, id: InstanceId, time: f64) -> Option<PastPose> {
        let track = self.tracks.get(&id)?;
        let &(last, _) = track.back()?;
        if time >= last {
            return None;
        }
        let after = track.iter().position(|&(at, _)| at > time)?;
        if after == 0 {
            return Some(track[0].1);
        }
        let (from, (p0, o0)) = track[after - 1];
        let (to, (p1, o1)) = track[after];
        let t = if to > from {
            ((time - from) / (to - from)) as f32
        } else {
            1.0
        };
        Some((lerp(p0, p1, t), turn(o0, o1, t)))
    }
}

/// The server's lag history, shared with its Lua (`workspace:Raycast`).
#[derive(Clone, Default)]
pub struct SharedLag(pub Rc<RefCell<LagHistory>>);

/// While a compensated Tool's RemoteEvent handler runs, the Player who
/// fired it.
#[derive(Clone, Copy, Default)]
pub(crate) struct Compensating(pub Option<InstanceId>);

/// The poses `player`'s shot is tested against, or `None` when it isn't
/// compensated (no history for them, or not on a server). Their own
/// character is left where it is.
pub(crate) fn rewound_for(
    lag: &LagHistory,
    world: &World,
    player: InstanceId,
) -> Option<(f64, FxHashSet<InstanceId>)> {
    let time = lag.seen_by(player)?;
    let own: FxHashSet<InstanceId> = world
        .get(player)
        .and_then(|player| player.property("Character"))
        .and_then(Value::as_instance)
        .map(|character| world.descendants_iter(character).collect())
        .unwrap_or_default();
    Some((time, own))
}

fn pose_of(part: &Instance) -> PastPose {
    let vector = |name| {
        part.property(name)
            .and_then(Value::as_vector3)
            .unwrap_or_default()
    };
    (vector("Position"), vector("Orientation"))
}

fn lerp(a: Vector3, b: Vector3, t: f32) -> Vector3 {
    Vector3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Orientation angles in degrees, each turned the short way round.
fn turn(a: Vector3, b: Vector3, t: f32) -> Vector3 {
    let one = |a: f32, b: f32| {
        let d = (b - a + 540.0).rem_euclid(360.0) - 180.0;
        a + d * t
    };
    Vector3::new(one(a.x, b.x), one(a.y, b.y), one(a.z, b.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    fn part(world: &mut World, x: f32) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(part, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
            .unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        part
    }

    fn at(world: &mut World, part: InstanceId, x: f32) {
        world
            .set_property(part, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
            .unwrap();
    }

    #[test]
    fn a_moving_part_is_found_where_it_was_and_a_still_one_where_it_is() {
        let mut world = World::new();
        let moving = part(&mut world, 0.0);
        let still = part(&mut world, 50.0);
        let mut lag = LagHistory::default();
        lag.record(&world, 0.0);
        // 10 studs a frame, ten frames of 0.1 s.
        for frame in 1..=10 {
            at(&mut world, moving, frame as f32 * 10.0);
            lag.record(&world, frame as f64 * 0.1);
        }
        let x = |time| lag.pose_at(moving, time).map(|(p, _)| p.x);
        let near = |time: f64, want: f32| {
            let got = x(time).unwrap();
            assert!((got - want).abs() < 1e-3, "at {time}: {got}, not {want}");
        };
        assert_eq!(x(1.0), None, "now: where it is");
        near(0.5, 50.0);
        near(0.55, 55.0);
        near(0.05, 5.0);
        near(-0.5, 0.0);
        assert_eq!(lag.pose_at(still, 0.5), None);
    }

    #[test]
    fn history_older_than_a_second_is_forgotten_and_a_part_that_stopped_is_where_it_is() {
        let mut world = World::new();
        let moving = part(&mut world, 0.0);
        let mut lag = LagHistory::default();
        lag.record(&world, 0.0);
        at(&mut world, moving, 10.0);
        lag.record(&world, 0.1);
        for frame in 2..=30 {
            lag.record(&world, frame as f64 * 0.1);
        }
        assert!(lag.tracks.is_empty());
        assert_eq!(lag.pose_at(moving, 2.5), None);
    }

    #[test]
    fn rewind_is_capped_and_zero_means_uncompensated() {
        let mut world = World::new();
        let player = world.create(ClassName::Player).unwrap();
        world.set_parent(player, Some(world.players())).unwrap();
        let mut lag = LagHistory::default();
        lag.record(&world, 10.0);
        lag.set_rewind(player, 5.0);
        assert_eq!(lag.seen_by(player), Some(10.0 - MAX_REWIND));
        lag.set_rewind(player, 0.0);
        assert_eq!(lag.seen_by(player), None);
    }
}
