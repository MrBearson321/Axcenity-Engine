//! Project audio playback through Kira.
//!
//! A game session owns one [`GameAudio`]. Each frame the host hands it the
//! [`SoundAction`]s scripts queued (`Sound:Play()` and `Sound:Stop()`), and
//! it brings what's playing in line with every Sound Instance's properties:
//! `Playing` pauses and resumes (and, true when the game starts, plays the
//! sound right away), and changes to `Volume`, `PlaybackSpeed` and `Looped`
//! apply to a sound that's already playing.
//!
//! A Sound with `Spatial` on is an emitter. Its place is the nearest ancestor
//! `Part`, `MeshPart` or `Terrain`, or its own `Position` when it has none.
//! The host's camera is the listener ([`Listener`]). Kira pans it in stereo;
//! this crate turns it down with distance each frame, from full at
//! `RollOffMinDistance` to silent at `RollOffMaxDistance`, along the
//! Sound's `RollOffMode` curve ([`mix::RollOff`], roadmap G8), and muffles
//! it (quieter, through a low-pass filter) by the walls the host found
//! between it and the listener ([`GameAudio::set_walls`], [`mix::muffle`]).
//!
//! Buses (Phase 52): each SoundGroup is a Kira sub-track, inside its parent
//! group's, at its `Volume`, through its effects ([`mix`], `effects`). Every
//! Sound plays on a track of its own, inside its `SoundGroup`'s (or the main
//! track), with its own effects; a spatial one's track also has the
//! ReverbZone reverb, which blends in while the listener is inside a zone.
//! Adding, removing or reordering groups or effects builds the group tracks
//! again, and the Sounds on them carry on from where they were. The main
//! track is at the world's master volume. The editor's Mixer reads each
//! group's level ([`GameAudio::take_levels`]) and can mute and solo them
//! ([`GameAudio::set_monitor`]).

/// The mix as the world describes it: `core`'s, so scripts and servers
/// read it without linking audio output (roadmap M8).
pub use axcenity_core::sound as mix;
pub use axcenity_core::sound::SoundAction;

mod effects;

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use kira::backend::Backend;
use kira::effect::filter::{FilterBuilder, FilterHandle, FilterMode};
use kira::listener::ListenerHandle;
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{
    MainTrackBuilder, SpatialTrackBuilder, SpatialTrackDistances, SpatialTrackHandle, TrackBuilder,
    TrackHandle,
};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};
use thiserror::Error;

use effects::{AddEffects, EffectChain, Level, Meter, ZoneReverb};
use mix::{Effect, EffectShape, Group, Reverb, RollOff};
pub use mix::{Monitor, REVERB_TYPES};

/// Full volume within this many studs, unless a Sound says otherwise.
const DEFAULT_MIN_DISTANCE: f64 = 10.0;
/// Silent from this many studs out, unless a Sound says otherwise.
const DEFAULT_MAX_DISTANCE: f64 = 150.0;
/// Distances past this are clamped. Far enough that a level never needs more.
const MAX_ROLLOFF: f64 = 100_000.0;
/// Kira divides by the span between the two distances, so they can't be equal.
const MIN_ROLLOFF_SPAN: f32 = 0.001;
/// How strongly a spatial sound moves between the ears. Kira's own default.
const SPATIALIZATION_STRENGTH: f32 = 0.75;
/// How long a spatial Sound's distance and wall changes take to apply, so
/// they never click.
const ATTENUATION_GLIDE: Duration = Duration::from_millis(60);

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("could not start audio output: {0}")]
    Output(String),
    #[error("could not load sound `{sound_id}`: {message}")]
    Asset { sound_id: String, message: String },
    #[error("could not decode sound `{sound_id}`: {message}")]
    Decode { sound_id: String, message: String },
    #[error("could not play sound `{sound_id}`: {message}")]
    Play { sound_id: String, message: String },
    #[error("Sound.Volume must be a finite number greater than or equal to 0")]
    InvalidVolume,
}

/// What one frame of [`GameAudio::update`] found.
#[derive(Debug, Default)]
pub struct AudioUpdate {
    /// Problems to show the creator, such as a sound file that can't be read.
    pub errors: Vec<AudioError>,
    /// Sounds that played to their end this frame (not looped, not
    /// stopped). Hosts set their `Playing` to false and fire `Ended`.
    pub ended: Vec<InstanceId>,
}

/// Where the listener is and which way it faces, in studs.
///
/// `from_view` takes the same position and orientation as
/// `workspace.CurrentCamera`: orientation is degrees, X looks up and down,
/// Y turns, and zero looks toward -Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    position: [f32; 3],
    forward: [f32; 3],
    up: [f32; 3],
}

impl Default for Listener {
    fn default() -> Self {
        Self::from_view(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0))
    }
}

impl Listener {
    /// The listener at `position`, facing `orientation` (degrees, as a camera).
    pub fn from_view(position: Vector3, orientation: Vector3) -> Self {
        let pitch = orientation.x.to_radians();
        let yaw = orientation.y.to_radians();
        let (sin_yaw, cos_yaw) = yaw.sin_cos();
        let (sin_pitch, cos_pitch) = pitch.sin_cos();
        let forward = [-sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch];
        let right = [cos_yaw, 0.0, -sin_yaw];
        Self {
            position: [finite(position.x), finite(position.y), finite(position.z)],
            forward,
            up: cross(right, forward),
        }
    }
}

/// One game session's audio. The audio output is only opened when the
/// first sound plays, so a game without sounds never touches the device,
/// and a machine without one only reports it once.
#[derive(Default)]
pub struct GameAudio {
    engine: Option<AudioEngine>,
    failed: bool,
    /// The editor Mixer's mute and solo, kept for when the output opens.
    monitor: Monitor,
    /// The walls between each spatial Sound and the listener, as the host
    /// last found them ([`set_walls`](Self::set_walls)).
    walls: HashMap<InstanceId, u32>,
}

/// How loud the mix has been since the levels were last taken: peak
/// amplitudes, 1 being full scale.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levels {
    /// The main track, after the master volume.
    pub master: f32,
    /// Each SoundGroup's own output.
    pub groups: HashMap<InstanceId, f32>,
}

impl GameAudio {
    /// Applies `actions`, then brings playback in line with the Sounds in
    /// `world` (see the module documentation). `listener` is the camera.
    pub fn update(
        &mut self,
        actions: Vec<SoundAction>,
        world: &World,
        assets: &Assets,
        listener: Listener,
    ) -> AudioUpdate {
        let mut update = AudioUpdate::default();
        let wants_sound = actions.iter().any(|action| {
            matches!(
                action,
                SoundAction::Play { .. } | SoundAction::PlayOnce { .. }
            )
        }) || sounds(world).any(|(_, sound)| SoundProperties::of(sound).playing);
        if self.engine.is_none() && wants_sound && !self.failed {
            match AudioEngine::new() {
                Ok(engine) => self.engine = Some(engine),
                Err(err) => {
                    self.failed = true;
                    update.errors.push(err);
                }
            }
        }
        let Some(engine) = &mut self.engine else {
            return update;
        };
        engine.monitor.clone_from(&self.monitor);
        engine.walls.clone_from(&self.walls);
        engine.set_listener(listener);
        for action in actions {
            if let Err(err) = engine.apply(action, world, assets) {
                update.errors.push(err);
            }
        }
        engine.sync(world, assets, &mut update);
        update
    }

    /// The solid parts between each spatial Sound and the listener (up to
    /// [`mix::MAX_WALLS`]), found by the host's rays (`sim::Occlusion`).
    /// A Sound left out has none. Applied from the next update.
    pub fn set_walls(&mut self, walls: &HashMap<InstanceId, u32>) {
        self.walls.clone_from(walls);
    }

    /// Moves the listener without starting or stopping anything. Used after
    /// a script moves the camera later in the same frame.
    pub fn set_listener(&mut self, listener: Listener) {
        if let Some(engine) = &mut self.engine {
            engine.set_listener(listener);
        }
    }

    /// How many Sounds are playing now (not paused or finished), for the
    /// editor's profiler.
    pub fn playing_count(&self) -> usize {
        self.engine.as_ref().map_or(0, AudioEngine::playing_count)
    }

    /// The decoded sounds kept to play again (G7): how many and their
    /// samples' bytes. None before the output opens.
    pub fn memory(&self) -> axcenity_core::memory::MemoryUse {
        self.engine
            .as_ref()
            .map_or_else(Default::default, AudioEngine::memory)
    }

    /// Mutes and solos SoundGroups from the next update on, as the editor's
    /// Mixer asks. The world doesn't change.
    pub fn set_monitor(&mut self, monitor: Monitor) {
        self.monitor = monitor;
    }

    /// The peak levels since the last call (all 0 before the output opens).
    pub fn take_levels(&mut self) -> Levels {
        self.engine
            .as_mut()
            .map_or_else(Levels::default, AudioEngine::take_levels)
    }

    /// Decodes the sound file at `path` again after it changed on disk, so
    /// a Sound uses the new version the next time it plays (what's playing
    /// now carries on). True if it had been played; a file that can't be
    /// decoded any more keeps its last good version and gives the error.
    pub fn reload(&mut self, path: &str, assets: &Assets) -> Result<bool, AudioError> {
        match &mut self.engine {
            Some(engine) => reload_decoded(&mut engine.decoded, path, assets),
            None => Ok(false),
        }
    }
}

/// Decodes `path` again into `decoded` if it's there, keeping the old
/// version if the new one can't be read or decoded.
fn reload_decoded(
    decoded: &mut HashMap<String, StaticSoundData>,
    path: &str,
    assets: &Assets,
) -> Result<bool, AudioError> {
    if !decoded.contains_key(path) {
        return Ok(false);
    }
    let bytes = assets
        .read_file(path)
        .map_err(|message| AudioError::Asset {
            sound_id: path.to_owned(),
            message,
        })?;
    decoded.insert(path.to_owned(), decode_sound(path, bytes)?);
    Ok(true)
}

/// A Sound's settings, as its properties have them now.
#[derive(Clone, Debug, PartialEq)]
struct SoundProperties {
    sound_id: String,
    volume: f64,
    speed: f64,
    looped: bool,
    playing: bool,
    spatial: bool,
    min_distance: f32,
    max_distance: f32,
    roll_off: RollOff,
    /// `Sound.Position`, before an ancestor part replaces it.
    position: [f32; 3],
    /// The SoundGroup it plays through ([`mix::sound_group`]).
    group: Option<InstanceId>,
    /// Its own effects, in order.
    effects: Vec<Effect>,
}

impl SoundProperties {
    fn of(sound: &Instance) -> Self {
        let number = |name, default| {
            sound
                .property(name)
                .and_then(|value| value.as_number())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let flag = |name| sound.property(name).and_then(|value| value.as_bool()) == Some(true);
        let (min_distance, max_distance) = rolloff(
            number("RollOffMinDistance", DEFAULT_MIN_DISTANCE),
            number("RollOffMaxDistance", DEFAULT_MAX_DISTANCE),
        );
        Self {
            sound_id: sound
                .property("SoundId")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            volume: number("Volume", 1.0).max(0.0),
            // Kira can't play backwards or infinitely slowly.
            speed: number("PlaybackSpeed", 1.0).clamp(0.01, 20.0),
            looped: flag("Looped"),
            playing: flag("Playing"),
            spatial: flag("Spatial"),
            min_distance,
            max_distance,
            roll_off: RollOff::of(sound),
            position: vector(sound),
            group: None,
            effects: Vec::new(),
        }
    }

    /// The Sound `id` as it plays in `world`: with its place, its group
    /// and its effects.
    fn read(world: &World, id: InstanceId, sound: &Instance) -> Self {
        let mut properties = Self::of(sound);
        properties.position = vector3(mix::emitter_position(world, id));
        properties.group = mix::sound_group(world, sound);
        properties.effects = mix::effects(world, id);
        properties
    }

    fn effect_shapes(&self) -> Vec<EffectShape> {
        self.effects.iter().map(Effect::shape).collect()
    }
}

/// A non-finite component becomes 0, so a broken property can't send the
/// emitter off to infinity.
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn vector(sound: &Instance) -> [f32; 3] {
    let position = sound
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap_or(Vector3::new(0.0, 0.0, 0.0));
    [finite(position.x), finite(position.y), finite(position.z)]
}

/// Both distances are finite and `0..=MAX_ROLLOFF`, and max is at least a
/// hair past min so the falloff has a span.
fn rolloff(min: f64, max: f64) -> (f32, f32) {
    let min = if min.is_finite() {
        min.clamp(0.0, MAX_ROLLOFF)
    } else {
        DEFAULT_MIN_DISTANCE
    };
    let max = if max.is_finite() {
        max.clamp(0.0, MAX_ROLLOFF)
    } else {
        DEFAULT_MAX_DISTANCE
    };
    let mut min = min as f32;
    let mut max = max as f32;
    if max < min {
        max = min;
    }
    if max - min < MIN_ROLLOFF_SPAN {
        if f64::from(min) + f64::from(MIN_ROLLOFF_SPAN) <= MAX_ROLLOFF {
            max = min + MIN_ROLLOFF_SPAN;
        } else {
            min = max - MIN_ROLLOFF_SPAN;
        }
    }
    (min, max)
}

/// Where a spatial Sound is heard from, in studs: the nearest ancestor
/// Part, MeshPart or Terrain, or the Sound's own `Position`.
pub fn source_position(world: &World, sound: InstanceId) -> Vector3 {
    mix::emitter_position(world, sound)
}

fn vector3(position: Vector3) -> [f32; 3] {
    [position.x, position.y, position.z]
}

/// Every Sound in the game, wherever it is.
fn sounds(world: &World) -> impl Iterator<Item = (InstanceId, &Instance)> {
    // By ID: nothing about a Sound depends on the others'.
    world
        .all_of(ClassName::Sound)
        .filter_map(|id| Some((id, world.get(id)?)))
}

/// A sound that's playing or paused, with the settings it was last given.
struct Playback {
    handle: StaticSoundHandle,
    /// The Sound's own track. Dropping it removes the track.
    track: SoundTrack,
    /// The Sound's own effects, on its track.
    chain: EffectChain,
    /// The ReverbZone reverb, on a spatial Sound's track.
    zone: Option<ZoneReverb>,
    settings: SoundProperties,
    /// The Mixer's gain on it (0 while solo leaves its group out).
    gain: f64,
    /// A spatial Sound's low-pass for the walls in front of it.
    muffler: Option<FilterHandle>,
    /// What distance and walls last set: the track's amplitude and the
    /// low-pass cutoff in hertz.
    heard: (f32, f64),
    /// The group tracks' generation it was played on, for a Sound in a group.
    generation: u64,
}

/// A Sound's own track, kept for as long as it plays: dropping the handle
/// removes the track.
enum SoundTrack {
    // Only held, never read.
    Plain(#[allow(dead_code)] TrackHandle),
    /// Moves with the emitter.
    Spatial(SpatialTrackHandle),
}

/// One SoundGroup's track.
struct Bus {
    track: TrackHandle,
    chain: EffectChain,
    level: Arc<Level>,
    /// The volume it was last set to, Mixer gain included.
    volume: f64,
}

/// What the group tracks were built for: each group, the group it's in,
/// and its effects' shapes.
type BusShape = Vec<(InstanceId, Option<InstanceId>, Vec<EffectShape>)>;

/// The ReverbZone reverb the spatial Sounds have now.
struct ZoneMix {
    /// The zone the listener is in.
    zone: Option<InstanceId>,
    reverb: Reverb,
    /// How far the reverb is blended in, 0 to 1.
    blend: f64,
    /// The seconds the zone it's in (or last left) fades over.
    fade_time: f64,
}

/// Audio state for one running game. Dropping it stops every sound and
/// releases the platform audio device. Tests run it on a backend of their
/// own (`B`) to hear what it plays.
pub struct AudioEngine<B: Backend = DefaultBackend> {
    manager: AudioManager<B>,
    listener: ListenerHandle,
    /// Where the listener is, to find the ReverbZone it's in.
    listener_position: Vector3,
    sounds: HashMap<InstanceId, Playback>,
    decoded: HashMap<String, StaticSoundData>,
    /// Sounds that ended, or couldn't be played, while `Playing` was still
    /// true. They aren't started again until `Playing` goes false first,
    /// so a broken file is reported once, not every frame.
    settled: HashSet<InstanceId>,
    buses: HashMap<InstanceId, Bus>,
    bus_shape: BusShape,
    /// Goes up each time the group tracks are built again.
    generation: u64,
    /// The SoundGroups as last read.
    groups: Vec<Group>,
    master_level: Arc<Level>,
    master_volume: f64,
    zone: ZoneMix,
    monitor: Monitor,
    /// The walls in front of each spatial Sound ([`GameAudio::set_walls`]).
    walls: HashMap<InstanceId, u32>,
    /// A file heard on its own, from no Sound ([`AudioEngine::play_file`]).
    file: Option<StaticSoundHandle>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        Self::with_backend(Default::default())
    }
}

impl<B: Backend> AudioEngine<B>
where
    B::Settings: Default,
    B::Error: std::fmt::Debug,
{
    /// Opens `B`'s output, with `settings`.
    pub fn with_backend(settings: B::Settings) -> Result<Self, AudioError> {
        let master_level = Arc::new(Level::default());
        let mut main_track = MainTrackBuilder::new();
        main_track.add(Meter(Arc::clone(&master_level)));
        let mut manager = AudioManager::<B>::new(AudioManagerSettings {
            main_track_builder: main_track,
            backend_settings: settings,
            ..AudioManagerSettings::default()
        })
        .map_err(|err| AudioError::Output(format!("{err:?}")))?;
        let listener = manager
            .add_listener(
                mint::Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                mint::Quaternion {
                    s: 1.0,
                    v: mint::Vector3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                },
            )
            .map_err(|err| AudioError::Output(err.to_string()))?;
        Ok(Self {
            manager,
            listener,
            listener_position: Vector3::default(),
            sounds: HashMap::new(),
            decoded: HashMap::new(),
            settled: HashSet::new(),
            buses: HashMap::new(),
            bus_shape: Vec::new(),
            generation: 0,
            groups: Vec::new(),
            master_level,
            master_volume: 1.0,
            zone: ZoneMix {
                zone: None,
                reverb: Reverb::preset("Room"),
                blend: 0.0,
                fade_time: 0.0,
            },
            monitor: Monitor::default(),
            walls: HashMap::new(),
            file: None,
        })
    }

    /// Plays a sound file's `bytes` once, on its own, from no Sound (the
    /// Toolbox's listen button, roadmap G1), stopping the one it played
    /// before. `name` is for errors. Nothing is kept once it stops.
    pub fn play_file(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), AudioError> {
        self.stop_file();
        let data = decode_sound(name, bytes)?;
        let handle = self.manager.play(data).map_err(|err| AudioError::Play {
            sound_id: name.to_owned(),
            message: err.to_string(),
        })?;
        self.file = Some(handle);
        Ok(())
    }

    /// Stops the file [`AudioEngine::play_file`] is playing.
    pub fn stop_file(&mut self) {
        if let Some(mut handle) = self.file.take() {
            handle.stop(Tween::default());
        }
    }

    /// Whether the file [`AudioEngine::play_file`] started is still playing.
    pub fn file_playing(&self) -> bool {
        self.file
            .as_ref()
            .is_some_and(|handle| handle.state() != PlaybackState::Stopped)
    }

    /// The peak levels since the last call.
    pub fn take_levels(&mut self) -> Levels {
        Levels {
            master: self.master_level.take(),
            groups: self
                .buses
                .iter()
                .map(|(&id, bus)| (id, bus.level.take()))
                .collect(),
        }
    }

    /// The decoded sounds kept to play again: how many and their samples'
    /// bytes (each frame is two `f32`s).
    pub fn memory(&self) -> axcenity_core::memory::MemoryUse {
        let mut used = axcenity_core::memory::MemoryUse::default();
        for data in self.decoded.values() {
            used.add(std::mem::size_of_val(&*data.frames));
        }
        used
    }

    /// How many sounds are playing now: `Playing` and not yet at their end.
    pub fn playing_count(&self) -> usize {
        self.sounds
            .values()
            .filter(|playback| {
                playback.settings.playing && playback.handle.state() != PlaybackState::Stopped
            })
            .count()
    }

    /// Points the listener at `listener`. An unmoved camera is a no-op for
    /// what you hear, apart from the command itself.
    pub fn set_listener(&mut self, listener: Listener) {
        let [x, y, z] = listener.position;
        self.listener_position = Vector3::new(x, y, z);
        let tween = Tween::default();
        self.listener
            .set_position(mint_vec(listener.position), tween);
        self.listener
            .set_orientation(listener_orientation(listener), tween);
    }

    /// Applies one command. Playing the same Instance again starts it over,
    /// in the place and rolloff its properties have now.
    pub fn apply(
        &mut self,
        action: SoundAction,
        world: &World,
        assets: &Assets,
    ) -> Result<(), AudioError> {
        match action {
            SoundAction::Stop(instance) => {
                self.stop(instance);
                Ok(())
            }
            SoundAction::PlayOnce { sound_id } => {
                let data = self.sound_data(&sound_id, assets)?;
                // Dropping the handle leaves the sound playing to its end.
                self.manager
                    .play(data)
                    .map(drop)
                    .map_err(|err| AudioError::Play {
                        sound_id,
                        message: err.to_string(),
                    })
            }
            SoundAction::Play {
                instance,
                sound_id,
                volume,
                looped,
            } => {
                let mut settings = world
                    .get(instance)
                    .filter(|sound| sound.class() == ClassName::Sound)
                    .map(|sound| SoundProperties::read(world, instance, sound))
                    .unwrap_or_else(|| SoundProperties {
                        sound_id: String::new(),
                        volume: 1.0,
                        speed: 1.0,
                        looped: false,
                        playing: true,
                        spatial: false,
                        min_distance: DEFAULT_MIN_DISTANCE as f32,
                        max_distance: DEFAULT_MAX_DISTANCE as f32,
                        roll_off: RollOff::default(),
                        position: [0.0, 0.0, 0.0],
                        group: None,
                        effects: Vec::new(),
                    });
                settings.sound_id = sound_id;
                settings.volume = volume;
                settings.looped = looped;
                settings.playing = true;
                self.sync_mix(world);
                self.play(instance, settings, assets)
            }
        }
    }

    /// Keeps a preview playing from the editor: the listener follows the
    /// viewport camera, and an emitter follows its part. Playback stays
    /// going even when the Sound's `Playing` property is false.
    pub fn follow(
        &mut self,
        world: &World,
        assets: &Assets,
        listener: Listener,
    ) -> Vec<AudioError> {
        self.set_listener(listener);
        let mut update = AudioUpdate::default();
        let ids: Vec<InstanceId> = self.sounds.keys().copied().collect();
        for id in ids {
            let Some(instance) = world.get(id).filter(|i| i.class() == ClassName::Sound) else {
                self.stop(id);
                continue;
            };
            let mut wanted = SoundProperties::read(world, id, instance);
            wanted.playing = true;
            self.sync_one(id, wanted, assets, &mut update);
        }
        update.errors
    }

    /// Brings playback in line with the Sounds in `world`: stops sounds
    /// whose Instance is gone, starts, pauses and resumes them as `Playing`
    /// says, and applies changed settings to what's playing.
    fn sync(&mut self, world: &World, assets: &Assets, update: &mut AudioUpdate) {
        let gone: Vec<InstanceId> = self
            .sounds
            .keys()
            .copied()
            .filter(|&id| world.get(id).is_none_or(|i| i.class() != ClassName::Sound))
            .collect();
        for id in gone {
            self.stop(id);
        }
        self.settled.retain(|&id| world.contains(id));

        self.sync_mix(world);
        for (id, sound) in sounds(world) {
            let wanted = SoundProperties::read(world, id, sound);
            self.sync_one(id, wanted, assets, update);
        }
        self.attenuate();
    }

    /// How loud a spatial Sound is heard now, and its low-pass cutoff:
    /// its `RollOffMode` at its distance from the listener, and the walls
    /// in front of it.
    fn heard(&self, id: InstanceId, settings: &SoundProperties) -> (f32, f64) {
        let [x, y, z] = settings.position;
        let listener = self.listener_position;
        let distance =
            ((x - listener.x).powi(2) + (y - listener.y).powi(2) + (z - listener.z).powi(2)).sqrt();
        let gain = settings
            .roll_off
            .gain(distance, settings.min_distance, settings.max_distance);
        let (kept, cutoff) = mix::muffle(self.walls.get(&id).copied().unwrap_or(0));
        (gain * kept, cutoff)
    }

    /// Turns each spatial Sound's track to how loud it's [`heard`]
    /// (Self::heard) from where the listener is now, gliding there.
    fn attenuate(&mut self) {
        let tween = Tween {
            duration: ATTENUATION_GLIDE,
            ..Tween::default()
        };
        let ids: Vec<InstanceId> = self.sounds.keys().copied().collect();
        for id in ids {
            let Some(playback) = self.sounds.get(&id) else {
                continue;
            };
            if !playback.settings.spatial {
                continue;
            }
            let heard = self.heard(id, &playback.settings);
            let Some(playback) = self.sounds.get_mut(&id) else {
                continue;
            };
            let (gain, cutoff) = heard;
            if (amplitude_db(gain) - amplitude_db(playback.heard.0)).abs() > 0.05 {
                if let SoundTrack::Spatial(track) = &mut playback.track {
                    track.set_volume(Decibels(amplitude_db(gain)), tween);
                }
                playback.heard.0 = gain;
            }
            if cutoff != playback.heard.1 {
                if let Some(muffler) = &mut playback.muffler {
                    muffler.set_cutoff(cutoff, tween);
                }
                playback.heard.1 = cutoff;
            }
        }
    }

    /// Brings the buses, the master volume and the zone reverb in line
    /// with `world`: builds the group tracks again when the groups or
    /// their effects changed shape, else sets their volumes and effects.
    fn sync_mix(&mut self, world: &World) {
        let tween = Tween::default();
        let master = world.master_volume();
        if master != self.master_volume {
            self.manager
                .main_track()
                .set_volume(decibels(master), tween);
            self.master_volume = master;
        }

        self.groups = mix::groups(world);
        let shape: BusShape = self
            .groups
            .iter()
            .map(|group| {
                let effects = group.effects.iter().map(Effect::shape).collect();
                (group.id, group.parent, effects)
            })
            .collect();
        if shape != self.bus_shape {
            self.build_buses();
            self.bus_shape = shape;
        } else {
            for group in &self.groups {
                let volume = group.volume * self.monitor.group_gain(&self.groups, group.id);
                if let Some(bus) = self.buses.get_mut(&group.id) {
                    if volume != bus.volume {
                        bus.track.set_volume(decibels(volume), tween);
                        bus.volume = volume;
                    }
                    bus.chain.update(&group.effects);
                }
            }
        }

        self.sync_zone(world);
    }

    /// Builds every group track again, parents first. The old tracks go,
    /// and the Sounds on them move to the new ones when they next sync.
    fn build_buses(&mut self) {
        self.buses.clear();
        self.generation += 1;
        for group in &self.groups {
            let volume = group.volume * self.monitor.group_gain(&self.groups, group.id);
            let mut builder = TrackBuilder::new()
                .sub_track_capacity(256)
                .volume(decibels(volume));
            let chain = EffectChain::build(&mut builder, &group.effects);
            let level = Arc::new(Level::default());
            builder.add(Meter(Arc::clone(&level)));
            let track = match group.parent.and_then(|parent| self.buses.get_mut(&parent)) {
                Some(parent) => parent.track.add_sub_track(builder),
                None => self.manager.add_sub_track(builder),
            };
            // Out of tracks: its Sounds play straight out.
            if let Ok(track) = track {
                self.buses.insert(
                    group.id,
                    Bus {
                        track,
                        chain,
                        level,
                        volume,
                    },
                );
            }
        }
    }

    /// Blends the reverb of the ReverbZone the listener is in onto every
    /// spatial Sound, over the zone's `FadeTime`, and out again over it
    /// when the listener leaves.
    fn sync_zone(&mut self, world: &World) {
        let zone = mix::zone_at(world, self.listener_position);
        let same = match zone {
            Some(zone) => self.zone.zone == Some(zone.id) && self.zone.reverb == zone.reverb,
            None => self.zone.zone.is_none(),
        };
        if same {
            return;
        }
        match zone {
            Some(zone) => {
                self.zone.zone = Some(zone.id);
                self.zone.reverb = zone.reverb;
                self.zone.blend = 1.0;
                self.zone.fade_time = zone.fade_time;
            }
            None => {
                self.zone.zone = None;
                self.zone.blend = 0.0;
            }
        }
        let tween = Tween {
            duration: Duration::from_secs_f64(self.zone.fade_time),
            ..Tween::default()
        };
        for playback in self.sounds.values_mut() {
            if let Some(reverb) = &mut playback.zone {
                reverb.set(self.zone.reverb, self.zone.blend, tween);
            }
        }
    }

    /// The generation a Sound in `group` plays on: the group tracks' now,
    /// or 0 for a Sound that plays straight out.
    fn generation_for(&self, group: Option<InstanceId>) -> u64 {
        if group.is_some_and(|group| self.buses.contains_key(&group)) {
            self.generation
        } else {
            0
        }
    }

    fn sync_one(
        &mut self,
        id: InstanceId,
        wanted: SoundProperties,
        assets: &Assets,
        update: &mut AudioUpdate,
    ) {
        if !wanted.playing {
            self.settled.remove(&id);
        }
        let generation = self.generation_for(wanted.group);
        let gain = self.monitor.sound_gain(&self.groups, wanted.group);
        let Some(playback) = self.sounds.get_mut(&id) else {
            if wanted.playing
                && !self.settled.contains(&id)
                && let Err(err) = self.play(id, wanted, assets)
            {
                self.settled.insert(id);
                update.errors.push(err);
            }
            return;
        };

        if playback.handle.state() == PlaybackState::Stopped {
            self.sounds.remove(&id);
            if wanted.playing {
                self.settled.insert(id);
                update.ended.push(id);
            }
            return;
        }
        if wanted.sound_id != playback.settings.sound_id {
            self.stop(id);
            if wanted.playing
                && let Err(err) = self.play(id, wanted, assets)
            {
                self.settled.insert(id);
                update.errors.push(err);
            }
            return;
        }
        let route_changed = wanted.spatial != playback.settings.spatial
            || wanted.group != playback.settings.group
            || wanted.effect_shapes() != playback.settings.effect_shapes()
            || generation != playback.generation;
        if route_changed {
            let at = playback.handle.position();
            if let Err(err) = self.play(id, wanted, assets) {
                self.settled.insert(id);
                update.errors.push(err);
            } else if let Some(playback) = self.sounds.get_mut(&id) {
                playback.handle.seek_to(at);
                if !playback.settings.playing {
                    playback.handle.pause(Tween::default());
                }
            }
            return;
        }

        let tween = Tween::default();
        let old = &playback.settings;
        if wanted.volume != old.volume || gain != playback.gain {
            playback
                .handle
                .set_volume(decibels(wanted.volume * gain), tween);
            playback.gain = gain;
        }
        playback.chain.update(&wanted.effects);
        if wanted.speed != old.speed {
            playback.handle.set_playback_rate(wanted.speed, tween);
        }
        if wanted.looped != old.looped {
            playback.handle.set_loop_region(loop_region(wanted.looped));
        }
        if wanted.playing != old.playing {
            if wanted.playing {
                playback.handle.resume(tween);
            } else {
                playback.handle.pause(tween);
            }
        }
        if wanted.position != old.position
            && let SoundTrack::Spatial(emitter) = &mut playback.track
        {
            emitter.set_position(mint_vec(wanted.position), tween);
        }
        playback.settings = wanted;
    }

    fn play(
        &mut self,
        instance: InstanceId,
        settings: SoundProperties,
        assets: &Assets,
    ) -> Result<(), AudioError> {
        if !settings.volume.is_finite() || settings.volume < 0.0 {
            return Err(AudioError::InvalidVolume);
        }
        let sound_id = settings.sound_id.clone();
        if sound_id.is_empty() {
            return Err(AudioError::Asset {
                sound_id,
                message: "SoundId is empty".to_owned(),
            });
        }

        let gain = self.monitor.sound_gain(&self.groups, settings.group);
        let data = self
            .sound_data(&sound_id, assets)?
            .volume(decibels(settings.volume * gain))
            .playback_rate(settings.speed)
            .loop_region(loop_region(settings.looped));

        self.stop(instance);
        self.settled.remove(&instance);
        let failed = |message: String| AudioError::Play {
            sound_id: sound_id.clone(),
            message,
        };
        let generation = self.generation_for(settings.group);
        let heard = self.heard(instance, &settings);
        let parent = settings.group.and_then(|group| self.buses.get_mut(&group));
        let (handle, track, chain, zone, muffler, heard) = if settings.spatial {
            // Distance is applied here each frame (`attenuate`), along the
            // Sound's RollOffMode, so Kira only pans it.
            let mut builder = SpatialTrackBuilder::new()
                .sound_capacity(4)
                .distances(SpatialTrackDistances {
                    min_distance: settings.min_distance,
                    max_distance: settings.max_distance,
                })
                .attenuation_function(None)
                .volume(Decibels(amplitude_db(heard.0)))
                .spatialization_strength(SPATIALIZATION_STRENGTH);
            let chain = EffectChain::build(&mut builder, &settings.effects);
            let muffler = builder.add_effect(
                FilterBuilder::new()
                    .mode(FilterMode::LowPass)
                    .cutoff(heard.1),
            );
            let zone = ZoneReverb::build(&mut builder, self.zone.reverb, self.zone.blend);
            let position = mint_vec(settings.position);
            let mut track = match parent {
                Some(bus) => bus
                    .track
                    .add_spatial_sub_track(&self.listener, position, builder),
                None => self
                    .manager
                    .add_spatial_sub_track(&self.listener, position, builder),
            }
            .map_err(|err| failed(err.to_string()))?;
            let handle = track.play(data).map_err(|err| failed(err.to_string()))?;
            (
                handle,
                SoundTrack::Spatial(track),
                chain,
                Some(zone),
                Some(muffler),
                heard,
            )
        } else {
            let mut builder = TrackBuilder::new().sound_capacity(4);
            let chain = EffectChain::build(&mut builder, &settings.effects);
            let mut track = match parent {
                Some(bus) => bus.track.add_sub_track(builder),
                None => self.manager.add_sub_track(builder),
            }
            .map_err(|err| failed(err.to_string()))?;
            let handle = track.play(data).map_err(|err| failed(err.to_string()))?;
            (
                handle,
                SoundTrack::Plain(track),
                chain,
                None,
                None,
                (1.0, mix::OPEN_CUTOFF),
            )
        };
        self.sounds.insert(
            instance,
            Playback {
                handle,
                track,
                chain,
                zone,
                settings,
                gain,
                muffler,
                heard,
                generation,
            },
        );
        Ok(())
    }

    /// The decoded file `sound_id`, read and decoded the first time it's played.
    fn sound_data(
        &mut self,
        sound_id: &str,
        assets: &Assets,
    ) -> Result<StaticSoundData, AudioError> {
        if let Some(data) = self.decoded.get(sound_id) {
            return Ok(data.clone());
        }
        if sound_id.is_empty() {
            return Err(AudioError::Asset {
                sound_id: String::new(),
                message: "SoundId is empty".to_owned(),
            });
        }
        let bytes = assets
            .read_file(sound_id)
            .map_err(|message| AudioError::Asset {
                sound_id: sound_id.to_owned(),
                message,
            })?;
        let data = decode_sound(sound_id, bytes)?;
        self.decoded.insert(sound_id.to_owned(), data.clone());
        Ok(data)
    }

    fn stop(&mut self, instance: InstanceId) {
        if let Some(mut sound) = self.sounds.remove(&instance) {
            sound.handle.stop(Tween::default());
        }
    }
}

/// `Volume` (an amplitude, 1 the file's own level) as Kira's decibels.
/// An amplitude (0 to 1) in decibels, silence below -80 dB.
fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude <= 1e-4 {
        Decibels::SILENCE.0
    } else {
        20.0 * amplitude.log10()
    }
}

fn decibels(volume: f64) -> Decibels {
    if volume <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels((20.0 * volume.log10()) as f32)
    }
}

/// The whole sound, when looped.
fn loop_region(looped: bool) -> Option<kira::sound::Region> {
    looped.then_some(kira::sound::Region::from(0.0..))
}

/// Decodes a sound file's bytes (WAV, OGG, MP3 or FLAC) into one channel
/// (the channels averaged) and its samples per second, for tools that
/// look at a recording, such as making a lip sync from it.
pub fn decode_mono(sound_id: &str, bytes: Vec<u8>) -> Result<(Vec<f32>, u32), AudioError> {
    let data = decode_sound(sound_id, bytes)?;
    let samples = data
        .frames
        .iter()
        .map(|frame| (frame.left + frame.right) * 0.5)
        .collect();
    Ok((samples, data.sample_rate))
}

fn decode_sound(sound_id: &str, bytes: Vec<u8>) -> Result<StaticSoundData, AudioError> {
    StaticSoundData::from_cursor(Cursor::new(bytes)).map_err(|err| AudioError::Decode {
        sound_id: sound_id.to_owned(),
        message: err.to_string(),
    })
}

fn mint_vec(value: [f32; 3]) -> mint::Vector3<f32> {
    mint::Vector3 {
        x: value[0],
        y: value[1],
        z: value[2],
    }
}

/// Kira's listener faces -Z when its orientation is the identity, with +X
/// to the right and +Y up. This builds that orientation from the camera's
/// forward and up.
fn listener_orientation(listener: Listener) -> mint::Quaternion<f32> {
    let forward = normalize(listener.forward).unwrap_or([0.0, 0.0, -1.0]);
    let up = normalize(listener.up).unwrap_or([0.0, 1.0, 0.0]);
    let right = normalize(cross(forward, up)).unwrap_or([1.0, 0.0, 0.0]);
    let up = cross(right, forward);
    let back = [-forward[0], -forward[1], -forward[2]];
    quat_from_basis(right, up, back)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if length > 1e-6 {
        Some([value[0] / length, value[1] / length, value[2] / length])
    } else {
        None
    }
}

/// Columns are where +X, +Y and +Z go.
fn quat_from_basis(x: [f32; 3], y: [f32; 3], z: [f32; 3]) -> mint::Quaternion<f32> {
    let (m00, m10, m20) = (x[0], x[1], x[2]);
    let (m01, m11, m21) = (y[0], y[1], y[2]);
    let (m02, m12, m22) = (z[0], z[1], z[2]);
    let trace = m00 + m11 + m22;
    let (qx, qy, qz, qw) = if trace > 0.0 {
        let scale = (trace + 1.0).sqrt() * 2.0;
        (
            (m21 - m12) / scale,
            (m02 - m20) / scale,
            (m10 - m01) / scale,
            0.25 * scale,
        )
    } else if m00 > m11 && m00 > m22 {
        let scale = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        (
            0.25 * scale,
            (m01 + m10) / scale,
            (m02 + m20) / scale,
            (m21 - m12) / scale,
        )
    } else if m11 > m22 {
        let scale = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        (
            (m01 + m10) / scale,
            0.25 * scale,
            (m12 + m21) / scale,
            (m02 - m20) / scale,
        )
    } else {
        let scale = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        (
            (m02 + m20) / scale,
            (m12 + m21) / scale,
            0.25 * scale,
            (m10 - m01) / scale,
        )
    };
    mint::Quaternion {
        s: qw,
        v: mint::Vector3 {
            x: qx,
            y: qy,
            z: qz,
        },
    }
}

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod tests {
    use axcenity_core::Value;

    use super::*;

    fn assert_near(a: [f32; 3], b: [f32; 3]) {
        for (a, b) in a.iter().zip(b) {
            assert!((a - b).abs() < 1e-4, "{a} != {b}");
        }
    }

    fn quat_mul(q: mint::Quaternion<f32>, v: [f32; 3]) -> [f32; 3] {
        let (x, y, z, w) = (q.v.x, q.v.y, q.v.z, q.s);
        let (vx, vy, vz) = (v[0], v[1], v[2]);
        let tx = 2.0 * (y * vz - z * vy);
        let ty = 2.0 * (z * vx - x * vz);
        let tz = 2.0 * (x * vy - y * vx);
        [
            vx + w * tx + (y * tz - z * ty),
            vy + w * ty + (z * tx - x * tz),
            vz + w * tz + (x * ty - y * tx),
        ]
    }

    /// Left and right ear multipliers, matching Kira's stereo pan.
    fn ear_gains(listener: Listener, emitter: [f32; 3]) -> (f32, f32) {
        const EAR_DISTANCE: f32 = 0.1;
        const EAR_ANGLE: f32 = std::f32::consts::FRAC_PI_8;
        let orientation = listener_orientation(listener);
        let left_offset = quat_mul(orientation, [-EAR_DISTANCE, 0.0, 0.0]);
        let right_offset = quat_mul(orientation, [EAR_DISTANCE, 0.0, 0.0]);
        let left_ear = [
            listener.position[0] + left_offset[0],
            listener.position[1] + left_offset[1],
            listener.position[2] + left_offset[2],
        ];
        let right_ear = [
            listener.position[0] + right_offset[0],
            listener.position[1] + right_offset[1],
            listener.position[2] + right_offset[2],
        ];
        let (sin_a, cos_a) = EAR_ANGLE.sin_cos();
        let left_dir = quat_mul(orientation, [-cos_a, 0.0, -sin_a]);
        let right_dir = quat_mul(orientation, [cos_a, 0.0, -sin_a]);
        let toward = |ear: [f32; 3]| {
            let delta = [
                emitter[0] - ear[0],
                emitter[1] - ear[1],
                emitter[2] - ear[2],
            ];
            normalize(delta).unwrap_or([0.0, 0.0, 0.0])
        };
        let left_volume = (dot(left_dir, toward(left_ear)) + 1.0) / 2.0;
        let right_volume = (dot(right_dir, toward(right_ear)) + 1.0) / 2.0;
        let min_ear = 1.0 - SPATIALIZATION_STRENGTH;
        (
            min_ear + (1.0 - min_ear) * left_volume,
            min_ear + (1.0 - min_ear) * right_volume,
        )
    }

    #[test]
    fn example_sounds_decode_without_audio_output() {
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-audio");
        let assets = Assets::new(folder);
        for sound_id in ["assets/audio/chime.wav", "assets/audio/ambient-loop.wav"] {
            let bytes = assets.read_file(sound_id).unwrap();
            assert!(decode_sound(sound_id, bytes).is_ok(), "{sound_id}");
        }
    }

    #[test]
    fn sound_properties_are_read_with_safe_fallbacks() {
        let mut world = World::new();
        let sound = world.create(ClassName::Sound).unwrap();
        world
            .set_property(sound, "PlaybackSpeed", Value::Number(-3.0))
            .unwrap();
        world
            .set_property(sound, "Playing", Value::Bool(true))
            .unwrap();
        let read = SoundProperties::of(world.get(sound).unwrap());
        assert_eq!(read.speed, 0.01, "never backwards");
        assert!(read.playing && !read.looped);
        assert_eq!(read.volume, 1.0);
        assert!(!read.spatial, "existing sounds stay non-spatial");
        assert_eq!(read.min_distance, 10.0);
        assert_eq!(read.max_distance, 150.0);
    }

    #[test]
    fn rolloff_clamps_to_a_safe_span() {
        let (min, max) = rolloff(-4.0, f64::NAN);
        assert_eq!(min, 0.0);
        assert!((max - DEFAULT_MAX_DISTANCE as f32).abs() < 1e-3);
        let (min, max) = rolloff(40.0, 10.0);
        assert_eq!(min, 40.0);
        assert!(max > min);
        let (min, max) = rolloff(5.0, 5.0);
        assert!((max - min - MIN_ROLLOFF_SPAN).abs() < 1e-6);
        assert!(min >= 0.0 && max.is_finite());
    }

    #[test]
    fn a_sound_on_the_right_is_louder_in_the_right_ear() {
        let listener = Listener::default();
        assert_near(
            quat_mul(listener_orientation(listener), [0.0, 0.0, -1.0]),
            [0.0, 0.0, -1.0],
        );
        let (left, right) = ear_gains(listener, [10.0, 0.0, 0.0]);
        assert!(right > left * 2.0, "left {left}, right {right}");
        let (left, right) = ear_gains(listener, [-10.0, 0.0, 0.0]);
        assert!(left > right * 2.0, "left {left}, right {right}");
        let (left, right) = ear_gains(listener, [0.0, 0.0, -10.0]);
        assert!(
            (left - right).abs() < 0.05,
            "ahead should be centered: {left} {right}"
        );
    }

    #[test]
    fn the_listener_faces_the_camera() {
        let ahead = Listener::from_view(Vector3::new(1.0, 2.0, 3.0), Vector3::new(0.0, 0.0, 0.0));
        assert_near(ahead.forward, [0.0, 0.0, -1.0]);
        assert_near(ahead.position, [1.0, 2.0, 3.0]);
        let turned = Listener::from_view(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 90.0, 0.0));
        assert_near(turned.forward, [-1.0, 0.0, 0.0]);
        assert_near(
            quat_mul(listener_orientation(turned), [0.0, 0.0, -1.0]),
            turned.forward,
        );
    }

    #[test]
    fn an_emitter_follows_its_ancestor_part() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(3.0, 4.0, 5.0)),
            )
            .unwrap();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(part)).unwrap();
        let sound = world.create(ClassName::Sound).unwrap();
        world
            .set_property(
                sound,
                "Position",
                Value::Vector3(Vector3::new(9.0, 9.0, 9.0)),
            )
            .unwrap();
        world.set_parent(sound, Some(folder)).unwrap();
        assert_eq!(
            SoundProperties::read(&world, sound, world.get(sound).unwrap()).position,
            [3.0, 4.0, 5.0],
            "the part wins over Sound.Position"
        );

        world.set_parent(sound, Some(world.workspace())).unwrap();
        assert_eq!(
            SoundProperties::read(&world, sound, world.get(sound).unwrap()).position,
            [9.0, 9.0, 9.0]
        );
        assert_eq!(source_position(&world, sound), Vector3::new(9.0, 9.0, 9.0));
    }

    #[test]
    fn a_game_without_playing_sounds_never_opens_the_audio_output() {
        let mut world = World::new();
        world.create(ClassName::Sound).unwrap();
        let mut audio = GameAudio::default();
        let update = audio.update(Vec::new(), &world, &Assets::default(), Listener::default());
        assert!(update.errors.is_empty() && update.ended.is_empty());
        assert!(audio.engine.is_none());
    }

    #[test]
    fn a_changed_sound_file_is_decoded_again_and_a_broken_one_is_kept() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-audio-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/audio")).unwrap();
        let example = |name: &str| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/hello-audio/assets/audio")
                .join(name)
        };
        let path = "assets/audio/tone.wav";
        std::fs::copy(example("chime.wav"), folder.join(path)).unwrap();
        let assets = Assets::new(&folder);
        let mut decoded = HashMap::new();
        assert!(
            !reload_decoded(&mut decoded, path, &assets).unwrap(),
            "not played yet"
        );

        let chime = decode_sound(path, assets.read_file(path).unwrap()).unwrap();
        decoded.insert(path.to_owned(), chime.clone());
        std::fs::copy(example("ambient-loop.wav"), folder.join(path)).unwrap();
        assert!(reload_decoded(&mut decoded, path, &assets).unwrap());
        let ambient = decoded[path].duration();
        assert_ne!(ambient, chime.duration());

        std::fs::write(folder.join(path), b"not a sound").unwrap();
        assert!(reload_decoded(&mut decoded, path, &assets).is_err());
        assert_eq!(decoded[path].duration(), ambient, "the last good one stays");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn volume_converts_to_decibels() {
        assert_eq!(decibels(1.0), Decibels(0.0));
        assert_eq!(decibels(0.0), Decibels::SILENCE);
        assert!((decibels(0.5).0 + 6.0206).abs() < 1e-3);
    }
}
