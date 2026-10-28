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
//! The host's camera is the listener ([`Listener`]). Kira pans it in stereo
//! and turns it down with distance, from full at `RollOffMinDistance` to
//! silent at `RollOffMaxDistance`.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use kira::listener::ListenerHandle;
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{SpatialTrackBuilder, SpatialTrackDistances, SpatialTrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};
use thiserror::Error;

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

/// A request made by a `Sound` Instance. Hosts apply requests after a script
/// step and own the resulting engine for exactly one game session.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundAction {
    /// Plays from the start, even if it's already playing.
    Play {
        instance: InstanceId,
        sound_id: String,
        volume: f64,
        looped: bool,
    },
    /// Stops, so the next play starts from the beginning.
    Stop(InstanceId),
    /// Plays a sound file once, at its own volume and not from anywhere in
    /// the world, belonging to no Sound: a button's `HoverSound` or
    /// `ClickSound`. Nothing stops it; it plays to its end.
    PlayOnce { sound_id: String },
}

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
        engine.set_listener(listener);
        for action in actions {
            if let Err(err) = engine.apply(action, world, assets) {
                update.errors.push(err);
            }
        }
        engine.sync(world, assets, &mut update);
        update
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
    /// `Sound.Position`, before an ancestor part replaces it.
    position: [f32; 3],
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
            position: vector(sound),
        }
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
    let fallback = world.get(sound).map(vector).unwrap_or([0.0, 0.0, 0.0]);
    let [x, y, z] = emitter_position(world, sound, fallback);
    Vector3::new(x, y, z)
}

/// Where a spatial Sound is heard from: the nearest ancestor part, or its
/// own `Position` when nothing above it has one.
fn emitter_position(world: &World, sound: InstanceId, fallback: [f32; 3]) -> [f32; 3] {
    let mut id = world.get(sound).and_then(|instance| instance.parent());
    while let Some(current) = id {
        let Some(instance) = world.get(current) else {
            break;
        };
        if matches!(
            instance.class(),
            ClassName::Part
                | ClassName::MeshPart
                | ClassName::Seat
                | ClassName::VehicleSeat
                | ClassName::Terrain
        ) && instance.property("Position").is_some()
        {
            return vector(instance);
        }
        id = instance.parent();
    }
    fallback
}

/// Every Sound in the game, wherever it is.
fn sounds(world: &World) -> impl Iterator<Item = (InstanceId, &Instance)> {
    world
        .descendants(world.root())
        .into_iter()
        .filter_map(|id| Some((id, world.get(id)?)))
        .filter(|(_, instance)| instance.class() == ClassName::Sound)
}

/// A sound that's playing or paused, with the settings it was last given.
struct Playback {
    handle: StaticSoundHandle,
    /// Set while `Spatial` is on. Dropping it removes the emitter track.
    emitter: Option<SpatialTrackHandle>,
    settings: SoundProperties,
}

/// Audio state for one running game. Dropping it stops every sound and
/// releases the platform audio device.
pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
    listener: ListenerHandle,
    sounds: HashMap<InstanceId, Playback>,
    decoded: HashMap<String, StaticSoundData>,
    /// Sounds that ended, or couldn't be played, while `Playing` was still
    /// true. They aren't started again until `Playing` goes false first,
    /// so a broken file is reported once, not every frame.
    settled: HashSet<InstanceId>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|err| AudioError::Output(err.to_string()))?;
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
            sounds: HashMap::new(),
            decoded: HashMap::new(),
            settled: HashSet::new(),
        })
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
                    .map(SoundProperties::of)
                    .unwrap_or_else(|| SoundProperties {
                        sound_id: String::new(),
                        volume: 1.0,
                        speed: 1.0,
                        looped: false,
                        playing: true,
                        spatial: false,
                        min_distance: DEFAULT_MIN_DISTANCE as f32,
                        max_distance: DEFAULT_MAX_DISTANCE as f32,
                        position: [0.0, 0.0, 0.0],
                    });
                settings.sound_id = sound_id;
                settings.volume = volume;
                settings.looped = looped;
                settings.playing = true;
                settings.position = emitter_position(world, instance, settings.position);
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
            let mut wanted = SoundProperties::of(instance);
            wanted.playing = true;
            wanted.position = emitter_position(world, id, wanted.position);
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

        for (id, sound) in sounds(world) {
            let mut wanted = SoundProperties::of(sound);
            wanted.position = emitter_position(world, id, wanted.position);
            self.sync_one(id, wanted, assets, update);
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
            || wanted.min_distance != playback.settings.min_distance
            || wanted.max_distance != playback.settings.max_distance;
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
        if wanted.volume != old.volume {
            playback.handle.set_volume(decibels(wanted.volume), tween);
        }
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
            && let Some(emitter) = &mut playback.emitter
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

        let data = self
            .sound_data(&sound_id, assets)?
            .volume(decibels(settings.volume))
            .playback_rate(settings.speed)
            .loop_region(loop_region(settings.looped));

        self.stop(instance);
        self.settled.remove(&instance);
        let failed = |message: String| AudioError::Play {
            sound_id: sound_id.clone(),
            message,
        };
        if settings.spatial {
            let mut track = self.emitter_track(&settings)?;
            let handle = track.play(data).map_err(|err| failed(err.to_string()))?;
            self.sounds.insert(
                instance,
                Playback {
                    handle,
                    emitter: Some(track),
                    settings,
                },
            );
        } else {
            let handle = self
                .manager
                .play(data)
                .map_err(|err| failed(err.to_string()))?;
            self.sounds.insert(
                instance,
                Playback {
                    handle,
                    emitter: None,
                    settings,
                },
            );
        }
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

    /// A track that pans and attenuates one Sound. One voice is enough;
    /// playing the Sound again replaces it.
    fn emitter_track(
        &mut self,
        settings: &SoundProperties,
    ) -> Result<SpatialTrackHandle, AudioError> {
        self.manager
            .add_spatial_sub_track(
                &self.listener,
                mint_vec(settings.position),
                SpatialTrackBuilder::new()
                    .sound_capacity(4)
                    .distances(SpatialTrackDistances {
                        min_distance: settings.min_distance,
                        max_distance: settings.max_distance,
                    })
                    .spatialization_strength(SPATIALIZATION_STRENGTH),
            )
            .map_err(|err| AudioError::Play {
                sound_id: settings.sound_id.clone(),
                message: err.to_string(),
            })
    }

    fn stop(&mut self, instance: InstanceId) {
        if let Some(mut sound) = self.sounds.remove(&instance) {
            sound.handle.stop(Tween::default());
        }
    }
}

/// `Volume` (an amplitude, 1 the file's own level) as Kira's decibels.
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

    /// Kira's linear distance curve: full at `min_distance`, silent at
    /// `max_distance`, and a blend from 0 dB to -60 dB in between.
    fn distance_gain(distance: f32, min_distance: f32, max_distance: f32) -> f32 {
        let distance = distance.clamp(min_distance, max_distance);
        let span = max_distance - min_distance;
        let relative = if span <= f32::EPSILON {
            1.0
        } else {
            (distance - min_distance) / span
        };
        let db = -60.0 * relative;
        if db <= -60.0 {
            0.0
        } else {
            10f32.powf(db / 20.0)
        }
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
    fn distance_gain_is_full_nearby_and_silent_past_the_max() {
        assert_eq!(distance_gain(0.0, 10.0, 150.0), 1.0);
        assert_eq!(distance_gain(10.0, 10.0, 150.0), 1.0);
        assert_eq!(distance_gain(150.0, 10.0, 150.0), 0.0);
        assert_eq!(distance_gain(1_000.0, 10.0, 150.0), 0.0);
        let mid = distance_gain(80.0, 10.0, 150.0);
        assert!((mid - 10f32.powf(-1.5)).abs() < 1e-4, "{mid}");
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
        let own = SoundProperties::of(world.get(sound).unwrap()).position;
        assert_eq!(
            emitter_position(&world, sound, own),
            [3.0, 4.0, 5.0],
            "the part wins over Sound.Position"
        );

        world.set_parent(sound, Some(world.workspace())).unwrap();
        assert_eq!(emitter_position(&world, sound, own), [9.0, 9.0, 9.0]);
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
