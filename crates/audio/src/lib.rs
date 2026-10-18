//! Project audio playback through Kira.

use std::collections::HashMap;
use std::io::Cursor;

use axcenity_assets::Assets;
use axcenity_core::InstanceId;
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};
use thiserror::Error;

/// A request made by a `Sound` Instance. Hosts apply requests after a script
/// step and own the resulting engine for exactly one game session.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundAction {
    Play {
        instance: InstanceId,
        sound_id: String,
        volume: f64,
        looped: bool,
    },
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

/// Audio state for one running game. Dropping it stops every sound and
/// releases the platform audio device.
pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
    sounds: HashMap<InstanceId, StaticSoundHandle>,
    decoded: HashMap<String, StaticSoundData>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|err| AudioError::Output(err.to_string()))?;
        Ok(Self {
            manager,
            sounds: HashMap::new(),
            decoded: HashMap::new(),
        })
    }

    /// Applies one command. A sound's settings are captured when it starts;
    /// playing the same Instance again replaces its previous playback.
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
            } => self.play(instance, &sound_id, volume, looped, assets),
        }
    }

    fn play(
        &mut self,
        instance: InstanceId,
        sound_id: &str,
        volume: f64,
        looped: bool,
        assets: &Assets,
    ) -> Result<(), AudioError> {
        if !volume.is_finite() || volume < 0.0 {
            return Err(AudioError::InvalidVolume);
        }
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

        let decibels = if volume == 0.0 {
            Decibels::SILENCE
        } else {
            Decibels((20.0 * volume.log10()) as f32)
        };
        let data = data
            .volume(decibels)
            .loop_region(looped.then_some(kira::sound::Region::from(0.0..)));

        self.stop(instance);
        let handle = self.manager.play(data).map_err(|err| AudioError::Play {
            sound_id: sound_id.to_owned(),
            message: err.to_string(),
        })?;
        self.sounds.insert(instance, handle);
        Ok(())
    }

    fn stop(&mut self, instance: InstanceId) {
        if let Some(mut sound) = self.sounds.remove(&instance) {
            sound.stop(Tween::default());
        }
    }

    /// Removes handles for sounds that have finished naturally.
    pub fn collect_finished(&mut self) {
        self.sounds
            .retain(|_, sound| sound.state() != PlaybackState::Stopped);
    }
}

fn decode_sound(sound_id: &str, bytes: Vec<u8>) -> Result<StaticSoundData, AudioError> {
    StaticSoundData::from_cursor(Cursor::new(bytes)).map_err(|err| AudioError::Decode {
        sound_id: sound_id.to_owned(),
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
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
}
