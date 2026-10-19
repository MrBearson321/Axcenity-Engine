//! Project audio playback through Kira.
//!
//! A game session owns one [`GameAudio`]. Each frame the host hands it the
//! [`SoundAction`]s scripts queued (`Sound:Play()` and `Sound:Stop()`), and
//! it brings what's playing in line with every Sound Instance's properties:
//! `Playing` pauses and resumes (and, true when the game starts, plays the
//! sound right away), and changes to `Volume`, `PlaybackSpeed` and `Looped`
//! apply to a sound that's already playing.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, InstanceId, World};
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};
use thiserror::Error;

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
    /// `world` (see the module documentation).
    pub fn update(
        &mut self,
        actions: Vec<SoundAction>,
        world: &World,
        assets: &Assets,
    ) -> AudioUpdate {
        let mut update = AudioUpdate::default();
        let wants_sound = actions
            .iter()
            .any(|action| matches!(action, SoundAction::Play { .. }))
            || sounds(world).any(|(_, sound)| SoundProperties::of(sound).playing);
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
        for action in actions {
            if let Err(err) = engine.apply(action, assets) {
                update.errors.push(err);
            }
        }
        engine.sync(world, assets, &mut update);
        update
    }
}

/// A Sound's settings, as its properties have them now.
#[derive(Clone, Debug, PartialEq)]
struct SoundProperties {
    sound_id: String,
    volume: f64,
    speed: f64,
    looped: bool,
    playing: bool,
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
        }
    }
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
    settings: SoundProperties,
}

/// Audio state for one running game. Dropping it stops every sound and
/// releases the platform audio device.
pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
    sounds: HashMap<InstanceId, Playback>,
    decoded: HashMap<String, StaticSoundData>,
    /// Sounds that ended, or couldn't be played, while `Playing` was still
    /// true. They aren't started again until `Playing` goes false first,
    /// so a broken file is reported once, not every frame.
    settled: HashSet<InstanceId>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|err| AudioError::Output(err.to_string()))?;
        Ok(Self {
            manager,
            sounds: HashMap::new(),
            decoded: HashMap::new(),
            settled: HashSet::new(),
        })
    }

    /// Applies one command. Playing the same Instance again starts it over.
    pub fn apply(&mut self, action: SoundAction, assets: &Assets) -> Result<(), AudioError> {
        match action {
            SoundAction::Stop(instance) => {
                self.stop(instance);
                Ok(())
            }
            SoundAction::Play {
                instance,
                sound_id,
                volume,
                looped,
            } => {
                let settings = SoundProperties {
                    sound_id,
                    volume,
                    speed: 1.0,
                    looped,
                    playing: true,
                };
                self.play(instance, settings, assets)
            }
        }
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
            let wanted = SoundProperties::of(sound);
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
                continue;
            };

            if playback.handle.state() == PlaybackState::Stopped {
                // It played to its end.
                self.sounds.remove(&id);
                if wanted.playing {
                    self.settled.insert(id);
                    update.ended.push(id);
                }
                continue;
            }
            if wanted.sound_id != playback.settings.sound_id {
                self.stop(id);
                if wanted.playing
                    && let Err(err) = self.play(id, wanted, assets)
                {
                    self.settled.insert(id);
                    update.errors.push(err);
                }
                continue;
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
            playback.settings = wanted;
        }
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
        let sound_id = settings.sound_id.as_str();
        if sound_id.is_empty() {
            return Err(AudioError::Asset {
                sound_id: sound_id.to_owned(),
                message: "SoundId is empty".to_owned(),
            });
        }

        let data = if let Some(data) = self.decoded.get(sound_id) {
            data.clone()
        } else {
            let bytes = assets
                .read_file(sound_id)
                .map_err(|message| AudioError::Asset {
                    sound_id: sound_id.to_owned(),
                    message,
                })?;
            let data = decode_sound(sound_id, bytes)?;
            self.decoded.insert(sound_id.to_owned(), data.clone());
            data
        };

        let data = data
            .volume(decibels(settings.volume))
            .playback_rate(settings.speed)
            .loop_region(loop_region(settings.looped));

        self.stop(instance);
        self.settled.remove(&instance);
        let handle = self.manager.play(data).map_err(|err| AudioError::Play {
            sound_id: sound_id.to_owned(),
            message: err.to_string(),
        })?;
        self.sounds.insert(instance, Playback { handle, settings });
        Ok(())
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

#[cfg(test)]
mod tests {
    use axcenity_core::Value;

    use super::*;

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
    }

    #[test]
    fn a_game_without_playing_sounds_never_opens_the_audio_output() {
        let mut world = World::new();
        world.create(ClassName::Sound).unwrap();
        let mut audio = GameAudio::default();
        let update = audio.update(Vec::new(), &world, &Assets::default());
        assert!(update.errors.is_empty() && update.ended.is_empty());
        assert!(audio.engine.is_none());
    }

    #[test]
    fn volume_converts_to_decibels() {
        assert_eq!(decibels(1.0), Decibels(0.0));
        assert_eq!(decibels(0.0), Decibels::SILENCE);
        assert!((decibels(0.5).0 + 6.0206).abs() < 1e-3);
    }
}
