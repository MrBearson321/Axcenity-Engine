//! Replays (Phase 81). A replay is the replication stream of one device's
//! world: what a player who joined at the start would have been sent,
//! frame by frame, with every moved part's exact pose. Nothing is
//! simulated again on playback, so a frame looks the same each time and on
//! any machine (the development policy rules out cross-machine
//! determinism; replays don't need it).
//!
//! - [`Recorder`] notes one device's world each frame, from
//!   `Network::send`. Its first frame, and one every
//!   [`KEYFRAME_SECONDS`], carries a whole snapshot, so playback can jump.
//!   A buffer recorder ([`Recorder::buffer`], `ReplayService.BufferSeconds`)
//!   keeps only the last few seconds, for a kill cam.
//! - [`Replay`] is what's saved: `AXREPLAY`, the format's version, then
//!   the replay in postcard ([`Replay::to_bytes`]).
//! - [`Playback`] rebuilds the world at any moment of a replay: from the
//!   keyframe before it, then each frame's changes and poses in order.

use std::collections::VecDeque;
use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, TerrainData, Value, Vector3, World};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::replicate::{Replicator, Update, apply};
use crate::{GameInfo, PROTOCOL_VERSION};

/// The `.replay` format's version: raise it when [`Replay`] changes.
pub const REPLAY_VERSION: u32 = 1;
/// How a `.replay` file starts.
const MAGIC: &[u8; 8] = b"AXREPLAY";
/// How often a recording takes a whole snapshot, so playback can jump to
/// any moment by applying at most this many seconds of frames.
pub const KEYFRAME_SECONDS: f64 = 10.0;
/// How many finished replays are kept in memory with nowhere to save them.
const KEPT_REPLAYS: usize = 4;
/// The longest buffer `ReplayService.BufferSeconds` keeps.
pub const MAX_BUFFER_SECONDS: f64 = 60.0;

/// A recorded game: its frames, the first a keyframe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    pub game: String,
    pub game_version: String,
    /// The network protocol of the engine that made it: Instances and
    /// their properties as that version sends them.
    pub protocol: u32,
    pub frames: Vec<ReplayFrame>,
}

/// One frame of a device's world.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayFrame {
    /// Seconds since the recording's device started its clock.
    pub time: f64,
    /// A keyframe's whole world, as of the end of this frame.
    pub snapshot: Option<Update>,
    /// What changed this frame (made, changed, destroyed), as
    /// replication sends it; empty in the first frame.
    pub update: Update,
    /// Where the parts that moved this frame are, exactly.
    pub poses: Vec<WirePose>,
}

/// A part's place and turn, at full precision.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WirePose {
    pub id: u64,
    pub position: [f32; 3],
    pub orientation: [f32; 3],
}

impl Replay {
    /// The `.replay` file's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend(REPLAY_VERSION.to_le_bytes());
        // A Replay is plain data postcard can always write.
        bytes.extend(postcard::to_allocvec(self).unwrap_or_default());
        bytes
    }

    /// Reads a `.replay` file, or says why it can't.
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, String> {
        let rest = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or("this isn't an Axcenity replay")?;
        let (version, rest) = rest
            .split_first_chunk::<4>()
            .ok_or("the replay is cut short")?;
        let version = u32::from_le_bytes(*version);
        if version != REPLAY_VERSION {
            return Err(format!(
                "the replay uses replay format {version} and this engine reads {REPLAY_VERSION}"
            ));
        }
        let replay: Replay =
            postcard::from_bytes(rest).map_err(|err| format!("the replay is damaged: {err}"))?;
        if replay.protocol != PROTOCOL_VERSION {
            return Err(format!(
                "the replay was made with network protocol {} and this engine uses {PROTOCOL_VERSION}: \
                 play it with the Axcenity version that recorded it",
                replay.protocol
            ));
        }
        if replay.frames.first().is_none_or(|f| f.snapshot.is_none()) {
            return Err("the replay has no frames".to_owned());
        }
        Ok(replay)
    }

    /// Its first frame's time, and its last's.
    pub fn span(&self) -> (f64, f64) {
        let start = self.frames.first().map_or(0.0, |f| f.time);
        let end = self.frames.last().map_or(start, |f| f.time);
        (start, end)
    }
}

/// Notes a device's world each frame (see the module docs).
pub struct Recorder {
    replicator: Replicator,
    /// The replicated parts and the motion revision last noted of each,
    /// for the tree as it was at `structure`.
    parts: FxHashMap<InstanceId, u64>,
    structure: Option<u64>,
    frames: VecDeque<ReplayFrame>,
    clock: f64,
    last_keyframe: Option<f64>,
    keyframe_every: f64,
    /// A buffer's length: frames older than this go (keeping the
    /// keyframe they start from).
    keep: Option<f64>,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder::new()
    }
}

impl Recorder {
    /// A recording of everything from now on.
    pub fn new() -> Recorder {
        Recorder {
            replicator: Replicator::default(),
            parts: FxHashMap::default(),
            structure: None,
            frames: VecDeque::new(),
            clock: 0.0,
            last_keyframe: None,
            keyframe_every: KEYFRAME_SECONDS,
            keep: None,
        }
    }

    /// A recording that keeps the last `seconds` (at most
    /// [`MAX_BUFFER_SECONDS`]).
    pub fn buffer(seconds: f64) -> Recorder {
        let seconds = seconds.clamp(0.5, MAX_BUFFER_SECONDS);
        Recorder {
            keyframe_every: seconds.min(KEYFRAME_SECONDS),
            keep: Some(seconds),
            ..Recorder::new()
        }
    }

    /// The buffer's length, if this is one.
    pub fn buffer_seconds(&self) -> Option<f64> {
        self.keep
    }

    /// Seconds recorded (a buffer: what it holds now).
    pub fn duration(&self) -> f64 {
        match (self.frames.front(), self.frames.back()) {
            (Some(first), Some(last)) => last.time - first.time,
            _ => 0.0,
        }
    }

    /// Notes `world` at the end of a frame `seconds` long.
    pub fn frame(&mut self, world: &World, seconds: f64) {
        profiling::scope!("Recorder::frame");
        self.clock += seconds.max(0.0);
        let first = self.last_keyframe.is_none();
        let mut update = self.replicator.frame(world);
        if first {
            // The snapshot below has all of it.
            update = Update::default();
        }
        update.time = self.clock;
        let structure = Some(world.structure_version());
        if self.structure != structure {
            self.structure = structure;
            let mut parts = FxHashMap::default();
            for id in self.replicator.parts(world) {
                // A part that just came is made where it is.
                let motion = match self.parts.get(&id) {
                    Some(&motion) => motion,
                    None => world.get(id).map_or(0, |part| part.motion_revision()),
                };
                parts.insert(id, motion);
            }
            self.parts = parts;
        }
        let mut poses = Vec::new();
        for (&id, motion) in &mut self.parts {
            let Some(part) = world.get(id) else {
                continue;
            };
            if part.motion_revision() == *motion {
                continue;
            }
            *motion = part.motion_revision();
            let vector = |name| {
                part.property(name)
                    .and_then(Value::as_vector3)
                    .unwrap_or_default()
            };
            let (p, o) = (vector("Position"), vector("Orientation"));
            poses.push(WirePose {
                id: id.to_bits(),
                position: [p.x, p.y, p.z],
                orientation: [o.x, o.y, o.z],
            });
        }
        poses.sort_unstable_by_key(|pose| pose.id);
        let keyframe = self
            .last_keyframe
            .is_none_or(|at| self.clock - at >= self.keyframe_every - 1e-9);
        let snapshot = keyframe.then(|| {
            self.last_keyframe = Some(self.clock);
            let mut snapshot = self.replicator.snapshot(world);
            snapshot.time = self.clock;
            snapshot
        });
        self.frames.push_back(ReplayFrame {
            time: self.clock,
            snapshot,
            update,
            poses,
        });
        if let Some(keep) = self.keep {
            // Start from the last keyframe at or before `keep` ago.
            let oldest = self.clock - keep;
            let start = self
                .frames
                .iter()
                .rposition(|f| f.snapshot.is_some() && f.time <= oldest + 1e-9);
            if let Some(start) = start {
                self.frames.drain(..start);
            }
        }
    }

    /// What's recorded, as a replay of `game`.
    pub fn replay(&self, game: &GameInfo) -> Replay {
        Replay {
            game: game.name.clone(),
            game_version: game.version.clone(),
            protocol: PROTOCOL_VERSION,
            frames: self.frames.iter().cloned().collect(),
        }
    }
}

/// A replay being watched: the world at its current moment.
pub struct Playback {
    replay: Replay,
    /// An empty world for the game, with its Terrains' landscapes (which
    /// replication never sends), to rebuild keyframes in.
    template: World,
    terrains: Vec<(InstanceId, Arc<TerrainData>)>,
    world: World,
    /// The next frame to apply.
    next: usize,
    time: f64,
    /// How fast it plays: 1 is as recorded.
    pub speed: f64,
    pub paused: bool,
}

impl Playback {
    /// Plays `replay` of the game whose own copy is `game`: its Terrains'
    /// landscapes are used, as a joining player's device uses its own.
    pub fn new(replay: Replay, game: &World) -> Result<Playback, String> {
        if replay.frames.first().is_none_or(|f| f.snapshot.is_none()) {
            return Err("the replay has no frames".to_owned());
        }
        let terrains = game
            .descendants_iter(game.root())
            .filter(|&id| {
                game.get(id)
                    .is_some_and(|i| i.class() == ClassName::Terrain)
            })
            .filter_map(|id| Some((id, game.terrain(id)?.clone())))
            .collect();
        let mut playback = Playback {
            template: game.empty_replica(),
            terrains,
            world: World::new(),
            next: 0,
            time: 0.0,
            speed: 1.0,
            paused: false,
            replay,
        };
        playback.jump_to(0);
        Ok(playback)
    }

    pub fn replay(&self) -> &Replay {
        &self.replay
    }

    /// The world as it was at [`time`](Self::time).
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Where playback is, in the replay's own seconds.
    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn span(&self) -> (f64, f64) {
        self.replay.span()
    }

    /// True once the last frame shows.
    pub fn at_end(&self) -> bool {
        self.next >= self.replay.frames.len()
    }

    /// Plays on for `seconds` of real time, at [`speed`](Self::speed),
    /// unless paused.
    pub fn step(&mut self, seconds: f64) {
        if self.paused {
            return;
        }
        let to = self.time + seconds.max(0.0) * self.speed.max(0.0);
        self.advance(to);
    }

    /// Shows the moment `time` (within the replay): forward by applying
    /// frames, else from the keyframe before it.
    pub fn seek(&mut self, time: f64) {
        let (start, end) = self.span();
        let time = time.clamp(start, end);
        let frames = &self.replay.frames;
        // The last frame at or before `time`: what should show.
        let target = frames.partition_point(|f| f.time <= time).max(1) - 1;
        let keyframe = frames[..=target]
            .iter()
            .rposition(|f| f.snapshot.is_some())
            .unwrap_or(0);
        // Back, or a keyframe is nearer than the frames in between.
        if target + 1 < self.next || keyframe >= self.next {
            self.jump_to(keyframe);
        }
        self.advance(time);
    }

    /// The world becomes keyframe `index`'s.
    fn jump_to(&mut self, index: usize) {
        let frame = &self.replay.frames[index];
        let Some(snapshot) = &frame.snapshot else {
            return;
        };
        let mut world = self.template.clone();
        apply(&mut world, snapshot);
        for (id, data) in &self.terrains {
            if world.contains(*id) {
                world.set_terrain(*id, data.clone());
            }
        }
        self.world = world;
        self.next = index + 1;
        self.time = frame.time;
    }

    /// Applies every frame up to `time`.
    fn advance(&mut self, time: f64) {
        while let Some(frame) = self.replay.frames.get(self.next) {
            if frame.time > time + 1e-9 {
                break;
            }
            apply_frame(&mut self.world, frame);
            self.next += 1;
        }
        let end = self.span().1;
        self.time = time.min(end);
    }

    /// The Players in the world now, in ID order, for a camera to follow.
    pub fn players(&self) -> Vec<InstanceId> {
        let players = self.world.players();
        self.world
            .get(players)
            .map(|p| p.children().to_vec())
            .unwrap_or_default()
    }
}

/// One recorded frame's changes, then its poses.
fn apply_frame(world: &mut World, frame: &ReplayFrame) {
    let applied = apply(world, &frame.update);
    // A device would fire PlayerRemoving first; a replay just lets them go.
    for player in applied.players_leaving {
        let _ = world.destroy(player);
    }
    for pose in &frame.poses {
        let id = InstanceId::from_bits(pose.id);
        if !world.contains(id) {
            continue;
        }
        let [x, y, z] = pose.position;
        let _ = world.set_property(id, "Position", Value::Vector3(Vector3::new(x, y, z)));
        let [x, y, z] = pose.orientation;
        let _ = world.set_property(id, "Orientation", Value::Vector3(Vector3::new(x, y, z)));
    }
}

/// The buffer `PlayBuffer()` asked to show: the replay, the moment to
/// start from, and the Player whose character the camera follows.
#[derive(Clone, Debug, PartialEq)]
pub struct BufferPlay {
    pub replay: Replay,
    pub start: f64,
    pub follow: Option<InstanceId>,
}

/// A device's replays (the `Network`'s part): the recording
/// `StartRecording` began, the buffer `BufferSeconds` keeps, and what's
/// finished.
#[derive(Default)]
pub struct Replays {
    recording: Option<Recorder>,
    buffer: Option<Recorder>,
    /// Where finished recordings are saved: the player's data folder's
    /// `replays`.
    folder: Option<std::path::PathBuf>,
    /// Finished recordings, when there's no folder to save them in.
    kept: Vec<(String, Replay)>,
    plays: Vec<BufferPlay>,
}

impl Replays {
    pub fn set_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.folder = folder;
    }

    /// Does what scripts asked, then notes this frame's world.
    pub fn frame(
        &mut self,
        world: &World,
        requests: Vec<axcenity_script::ReplayRequest>,
        seconds: f64,
        game: &GameInfo,
        log: &mut Vec<String>,
    ) {
        use axcenity_script::ReplayRequest;
        let mut plays = Vec::new();
        for request in requests {
            match request {
                ReplayRequest::Start => self.recording = Some(Recorder::new()),
                ReplayRequest::Stop { name } => {
                    if let Some(recording) = self.recording.take() {
                        let replay = recording.replay(game);
                        self.save(name, replay, log);
                    }
                }
                ReplayRequest::Buffer { seconds } => {
                    let same = self
                        .buffer
                        .as_ref()
                        .and_then(Recorder::buffer_seconds)
                        .is_some_and(|had| (had - seconds).abs() < 1e-9);
                    if !same {
                        self.buffer = (seconds > 0.0).then(|| Recorder::buffer(seconds));
                    }
                }
                ReplayRequest::PlayBuffer { follow } => plays.push(follow),
            }
        }
        if let Some(recording) = &mut self.recording {
            recording.frame(world, seconds);
        }
        if let Some(buffer) = &mut self.buffer {
            buffer.frame(world, seconds);
            // The buffer up to this frame, from the first frame of its
            // last `BufferSeconds`.
            for follow in plays {
                let replay = buffer.replay(game);
                let end = replay.span().1;
                let keep = buffer.buffer_seconds().unwrap_or_default();
                let start = replay
                    .frames
                    .iter()
                    .map(|f| f.time)
                    .find(|&time| time >= end - keep - 1e-9)
                    .unwrap_or(end);
                // Only the newest is shown.
                self.plays.clear();
                self.plays.push(BufferPlay {
                    replay,
                    start,
                    follow,
                });
            }
        }
    }

    fn save(&mut self, name: String, replay: Replay, log: &mut Vec<String>) {
        let Some(folder) = &self.folder else {
            // The editor's Play and tests: the newest few, in memory.
            if self.kept.len() >= KEPT_REPLAYS {
                self.kept.remove(0);
            }
            log.push(format!("Recorded the replay \"{name}\" (not saved here)."));
            self.kept.push((name, replay));
            return;
        };
        let path = folder.join(format!("{name}.replay"));
        let saved =
            std::fs::create_dir_all(folder).and_then(|()| std::fs::write(&path, replay.to_bytes()));
        match saved {
            Ok(()) => log.push(format!(
                "Saved the replay \"{name}\" to {}.",
                path.display()
            )),
            Err(err) => log.push(format!(
                "warning: couldn't save the replay \"{name}\" to {}: {err}",
                path.display()
            )),
        }
    }

    /// Finished recordings that had no folder to go to (tests).
    pub fn take_kept(&mut self) -> Vec<(String, Replay)> {
        std::mem::take(&mut self.kept)
    }

    /// The newest `PlayBuffer()` since the last call.
    pub fn take_play(&mut self) -> Option<BufferPlay> {
        let play = self.plays.pop();
        self.plays.clear();
        play
    }
}

/// Where `player`'s character looks from and toward, for a camera that
/// shows the game through their eyes: their Head (else the character's
/// root part), facing its front (-Z, turned by its `Orientation`).
pub fn eyes_of(world: &World, player: InstanceId) -> Option<(Vector3, Vector3)> {
    let character = crate::ownership::character_of(world, player)?;
    let head = world
        .find_first_child(character, "Head")
        .or_else(|| {
            world
                .get(character)?
                .property("PrimaryPart")
                .and_then(Value::as_instance)
                .filter(|&id| world.contains(id))
        })
        .or_else(|| world.find_first_child(character, "HumanoidRootPart"))?;
    let part = world.get(head)?;
    let position = part.property("Position")?.as_vector3()?;
    let orientation = part
        .property("Orientation")
        .and_then(Value::as_vector3)
        .unwrap_or_default();
    let (x, y) = (orientation.x.to_radians(), orientation.y.to_radians());
    let front = Vector3::new(-x.cos() * y.sin(), x.sin(), -x.cos() * y.cos());
    Some((
        position,
        Vector3::new(
            position.x + front.x,
            position.y + front.y,
            position.z + front.z,
        ),
    ))
}
