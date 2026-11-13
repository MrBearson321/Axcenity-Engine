//! What the engine actually plays (Phase 52): each test runs it on a
//! backend that renders Kira's output into memory, so the buses, effects
//! and zones are heard, not just set up.

use axcenity_core::EnumItem;
use kira::Frame;
use kira::backend::Renderer;
use kira::sound::static_sound::StaticSoundSettings;

use super::*;

const RATE: u32 = 48_000;
/// Frames Kira renders at a time.
const BLOCK: usize = 128;

/// A backend with no device: it renders when the test asks.
struct Capture {
    renderer: Option<Renderer>,
}

impl Backend for Capture {
    type Settings = ();
    type Error = ();

    fn setup(_settings: (), _internal_buffer_size: usize) -> Result<(Self, u32), ()> {
        Ok((Self { renderer: None }, RATE))
    }

    fn start(&mut self, renderer: Renderer) -> Result<(), ()> {
        self.renderer = Some(renderer);
        Ok(())
    }
}

/// One game's audio with nothing to play it on, and the world it plays.
struct Rig {
    engine: AudioEngine<Capture>,
    world: World,
    assets: Assets,
}

impl Rig {
    fn new() -> Self {
        let mut engine = AudioEngine::<Capture>::with_backend(()).unwrap();
        engine.decoded.insert(NOISE.to_owned(), noise());
        engine.decoded.insert(STEPS.to_owned(), steps());
        Self {
            engine,
            world: World::new(),
            assets: Assets::default(),
        }
    }

    /// One frame of the host: the engine catches up with the world.
    fn sync(&mut self) {
        let mut update = AudioUpdate::default();
        self.engine.sync(&self.world, &self.assets, &mut update);
        assert!(update.errors.is_empty(), "{:?}", update.errors);
    }

    /// Syncs, then plays `seconds` (in whole blocks, so none is lost) and
    /// returns the left channel.
    fn play(&mut self, seconds: f64) -> Vec<f32> {
        self.sync();
        let frames = (seconds * f64::from(RATE)) as usize;
        let renderer = self.engine.manager.backend_mut().renderer.as_mut().unwrap();
        let mut out = Vec::with_capacity(frames);
        let mut buffer = vec![0.0; BLOCK * 2];
        while out.len() < frames {
            renderer.on_start_processing();
            renderer.process(&mut buffer, 2);
            out.extend(buffer.chunks(2).map(|frame| frame[0]));
        }
        out
    }

    fn add(&mut self, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
        let id = self.world.create(class).unwrap();
        self.world.set_name(id, name).unwrap();
        self.world.set_parent(id, Some(parent)).unwrap();
        id
    }

    fn set(&mut self, id: InstanceId, name: &str, value: Value) {
        self.world.set_property(id, name, value).unwrap();
    }

    /// A looping Sound of `sound_id`, playing through `group`.
    fn sound(&mut self, sound_id: &str, group: Option<InstanceId>) -> InstanceId {
        let workspace = self.world.workspace();
        let sound = self.add(ClassName::Sound, sound_id, workspace);
        self.set(sound, "SoundId", Value::String(sound_id.to_owned()));
        self.set(sound, "Looped", Value::Bool(true));
        self.set(sound, "Playing", Value::Bool(true));
        self.set(sound, "SoundGroup", Value::Instance(group));
        sound
    }

    /// Master, with Music and SFX inside it, in SoundService.
    fn master_music_sfx(&mut self) -> [InstanceId; 3] {
        let service = self.world.sound_service();
        let master = self.add(ClassName::SoundGroup, "Master", service);
        let music = self.add(ClassName::SoundGroup, "Music", master);
        let sfx = self.add(ClassName::SoundGroup, "SFX", master);
        [master, music, sfx]
    }
}

const NOISE: &str = "test/noise";
const STEPS: &str = "test/steps";

/// Repeatable white noise, from a small linear congruential generator.
fn noise_samples(frames: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..frames)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1 << 24) as f32 * 1.0 - 0.5
        })
        .collect()
}

fn data(samples: Vec<f32>) -> StaticSoundData {
    StaticSoundData {
        sample_rate: RATE,
        frames: samples.into_iter().map(Frame::from_mono).collect(),
        settings: StaticSoundSettings::default(),
        slice: None,
    }
}

/// A second of noise: bright, with plenty above any low-pass cutoff.
fn noise() -> StaticSoundData {
    data(noise_samples(RATE as usize, 7))
}

/// Footsteps: a 30 ms scuff of noise every half second, silence between.
const STEP_PERIOD: usize = RATE as usize / 2;
const STEP_LENGTH: usize = RATE as usize * 30 / 1000;

fn steps() -> StaticSoundData {
    let scuff = noise_samples(STEP_LENGTH, 11);
    data(
        (0..STEP_PERIOD)
            .map(|i| scuff.get(i).copied().unwrap_or(0.0))
            .collect(),
    )
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// How bright: the loudness of the change from each sample to the next,
/// which is mostly the highs.
fn brightness(samples: &[f32]) -> f32 {
    let steps: Vec<f32> = samples.windows(2).map(|pair| pair[1] - pair[0]).collect();
    rms(&steps)
}

#[test]
fn group_volumes_multiply_down_the_tree_and_by_the_master_volume() {
    let mut rig = Rig::new();
    let [master, _, sfx] = rig.master_music_sfx();
    rig.sound(NOISE, Some(sfx));
    let full = rms(&rig.play(0.3)[BLOCK * 4..]);
    assert!(full > 0.1, "noise plays through SFX and Master: {full}");

    rig.set(master, "Volume", Value::Number(0.5));
    let half = rms(&rig.play(0.3)[BLOCK * 4..]);
    rig.set(sfx, "Volume", Value::Number(0.5));
    let quarter = rms(&rig.play(0.3)[BLOCK * 4..]);
    assert!((half / full - 0.5).abs() < 0.05, "{half} vs {full}");
    assert!((quarter / full - 0.25).abs() < 0.05, "{quarter} vs {full}");

    rig.world.set_master_volume(0.0);
    let silent = rms(&rig.play(0.3)[BLOCK * 4..]);
    assert!(silent < 1e-4, "master volume 0 is silent: {silent}");
}

#[test]
fn a_low_pass_on_the_music_group_dulls_the_music_while_it_is_enabled() {
    let mut rig = Rig::new();
    let [_, music, _] = rig.master_music_sfx();
    rig.sound(NOISE, Some(music));
    let clear = brightness(&rig.play(0.3)[BLOCK * 4..]);

    // The pause menu's low-pass, off until the menu opens.
    let low = rig.add(ClassName::LowPassSoundEffect, "PauseMuffle", music);
    rig.set(low, "Cutoff", Value::Number(600.0));
    rig.set(low, "Enabled", Value::Bool(false));
    let off = brightness(&rig.play(0.3)[BLOCK * 8..]);
    assert!(
        (off / clear - 1.0).abs() < 0.2,
        "an effect that's off changes nothing: {off} vs {clear}"
    );

    rig.set(low, "Enabled", Value::Bool(true));
    let samples = rig.play(0.3);
    let dull = brightness(&samples[BLOCK * 8..]);
    assert!(dull < clear / 4.0, "the highs are cut: {dull} vs {clear}");
    assert!(rms(&samples[BLOCK * 8..]) > 0.02, "the music still plays");

    rig.set(low, "Enabled", Value::Bool(false));
    let again = brightness(&rig.play(0.3)[BLOCK * 8..]);
    assert!(
        again > clear * 0.8,
        "closing the menu brings it back: {again}"
    );
}

#[test]
fn walking_into_a_cave_fades_reverb_in_on_footsteps_and_out_on_leaving() {
    let mut rig = Rig::new();
    let workspace = rig.world.workspace();
    let cave = rig.add(ClassName::ReverbZone, "Cave", workspace);
    rig.set(
        cave,
        "Position",
        Value::Vector3(Vector3::new(100.0, 0.0, 0.0)),
    );
    rig.set(
        cave,
        "ReverbType",
        Value::EnumItem(EnumItem {
            enum_name: "ReverbType".to_owned(),
            item: "Cave".to_owned(),
        }),
    );
    rig.set(cave, "FadeTime", Value::Number(0.5));
    let steps = rig.sound(STEPS, None);
    rig.set(steps, "Spatial", Value::Bool(true));

    // The camera and the footsteps move together.
    let mut played = 0;
    let mut walk_to = |rig: &mut Rig, x: f32, seconds: f64| -> f32 {
        let at = Vector3::new(x, 0.0, 0.0);
        rig.engine
            .set_listener(Listener::from_view(at, Vector3::default()));
        rig.set(
            steps,
            "Position",
            Value::Vector3(Vector3::new(x, 0.0, -2.0)),
        );
        let samples = rig.play(seconds);
        // The tail: what's heard between steps, from 100 ms after each.
        let tail: Vec<f32> = samples
            .iter()
            .enumerate()
            .filter(|(i, _)| (played + i) % STEP_PERIOD > RATE as usize / 10)
            .map(|(_, sample)| *sample)
            .collect();
        played += samples.len();
        rms(&tail)
    };

    let outside = walk_to(&mut rig, 0.0, 1.0);
    assert!(outside < 1e-4, "no reverb outside: {outside}");

    let fading_in = walk_to(&mut rig, 100.0, 0.25);
    let inside = walk_to(&mut rig, 100.0, 1.5);
    assert!(inside > 0.002, "the cave rings on between steps: {inside}");
    assert!(
        fading_in < inside,
        "it blends in over FadeTime: {fading_in} then {inside}"
    );

    walk_to(&mut rig, 0.0, 0.5);
    let left = walk_to(&mut rig, 0.0, 5.0);
    assert!(
        left < inside / 20.0,
        "it fades out on leaving: {left} vs {inside}"
    );
}

#[test]
fn the_mixer_meters_each_group_and_solo_silences_the_rest() {
    let mut rig = Rig::new();
    let [master, music, sfx] = rig.master_music_sfx();
    rig.sound(NOISE, Some(music));
    rig.play(0.2);
    let levels = rig.engine.take_levels();
    assert!(levels.groups[&music] > 0.1, "{levels:?}");
    assert!(levels.groups[&master] > 0.1);
    assert_eq!(levels.groups[&sfx], 0.0);
    assert!(levels.master > 0.1);

    rig.engine.monitor.soloed.insert(sfx);
    let soloed = rms(&rig.play(0.3)[BLOCK * 4..]);
    assert!(soloed < 1e-4, "soloing SFX leaves out Music: {soloed}");
    rig.engine.monitor.soloed.clear();
    rig.engine.monitor.muted.insert(music);
    let muted = rms(&rig.play(0.3)[BLOCK * 4..]);
    assert!(muted < 1e-4, "muted: {muted}");
    rig.engine.monitor.muted.clear();
    assert!(rms(&rig.play(0.3)[BLOCK * 4..]) > 0.1);
}

#[test]
fn adding_an_effect_rebuilds_the_bus_and_the_music_carries_on() {
    let mut rig = Rig::new();
    let [_, music, _] = rig.master_music_sfx();
    let sound = rig.sound(NOISE, Some(music));
    rig.play(0.4);
    let before = rig.engine.sounds[&sound].handle.position();
    rig.add(ClassName::EqualizerSoundEffect, "Eq", music);
    let samples = rig.play(0.1);
    assert!(rms(&samples[BLOCK * 2..]) > 0.05, "still playing");
    let after = rig.engine.sounds[&sound].handle.position();
    assert!(after >= before, "from where it was: {before} then {after}");
}

/// A spatial noise Sound `distance` studs in front of the listener, rolling
/// off from 10 studs to 110 by `mode`.
fn noise_ahead(rig: &mut Rig, mode: &str) -> InstanceId {
    let sound = rig.sound(NOISE, None);
    rig.set(sound, "Spatial", Value::Bool(true));
    rig.set(sound, "RollOffMinDistance", Value::Number(10.0));
    rig.set(sound, "RollOffMaxDistance", Value::Number(110.0));
    rig.set(
        sound,
        "RollOffMode",
        Value::EnumItem(EnumItem {
            enum_name: "RollOffMode".to_owned(),
            item: mode.to_owned(),
        }),
    );
    sound
}

/// Moves `sound` `distance` studs ahead, lets the volume glide there, then
/// returns what's heard.
fn heard_at(rig: &mut Rig, sound: InstanceId, distance: f32) -> Vec<f32> {
    rig.set(
        sound,
        "Position",
        Value::Vector3(Vector3::new(0.0, 0.0, -distance)),
    );
    rig.play(0.2);
    rig.play(0.3)
}

#[test]
fn each_roll_off_mode_is_heard_at_its_curve_s_volume() {
    for mode in mix::ROLL_OFF_MODES {
        let mut rig = Rig::new();
        let sound = noise_ahead(&mut rig, mode);
        let near = rms(&heard_at(&mut rig, sound, 5.0));
        assert!(near > 0.05, "{mode} plays close up: {near}");
        for distance in [20.0, 60.0, 100.0] {
            let far = rms(&heard_at(&mut rig, sound, distance));
            let expected = mix::RollOff::named(mode).gain(distance, 10.0, 110.0);
            assert!(
                (far / near - expected).abs() < 0.02 + expected * 0.05,
                "{mode} at {distance}: {} vs {expected}",
                far / near
            );
        }
        let past = rms(&heard_at(&mut rig, sound, 200.0));
        assert!(past < 1e-4, "{mode} is silent past the max: {past}");
    }
}

#[test]
fn a_wall_between_makes_a_sound_quieter_and_duller() {
    let mut rig = Rig::new();
    let sound = noise_ahead(&mut rig, "Inverse");
    let open = heard_at(&mut rig, sound, 5.0);
    let (open_rms, open_bright) = (rms(&open), brightness(&open));

    rig.engine.walls.insert(sound, 1);
    let walled = heard_at(&mut rig, sound, 5.0);
    let (walled_rms, walled_bright) = (rms(&walled), brightness(&walled));
    assert!(
        walled_rms < open_rms * 0.5,
        "half as loud, less the highs: {walled_rms} vs {open_rms}"
    );
    assert!(
        walled_bright / walled_rms < open_bright / open_rms * 0.6,
        "duller: {} vs {}",
        walled_bright / walled_rms,
        open_bright / open_rms
    );

    rig.engine.walls.insert(sound, 3);
    let thick = rms(&heard_at(&mut rig, sound, 5.0));
    assert!(thick < walled_rms * 0.5, "more walls, quieter: {thick}");

    rig.engine.walls.clear();
    let clear = rms(&heard_at(&mut rig, sound, 5.0));
    assert!(
        (clear / open_rms - 1.0).abs() < 0.1,
        "it opens up again: {clear} vs {open_rms}"
    );
}

#[test]
fn the_decoded_sounds_memory_is_their_samples() {
    let rig = Rig::new();
    let used = rig.engine.memory();
    assert_eq!(used.count, 2);
    // A second of noise and half a second of steps, two f32s a frame.
    let frames = RATE as usize + STEP_PERIOD;
    assert_eq!(used.bytes, frames * 8);
}

/// A 16-bit mono WAV file of `samples` at [`RATE`].
fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// G1: a Toolbox preview plays a file's bytes on their own, is heard, and
/// stops when asked or at its end; a file that isn't sound says so.
#[test]
fn a_file_plays_on_its_own_until_it_ends_or_is_stopped() {
    let mut rig = Rig::new();
    let noise = noise_samples(RATE as usize / 4, 3);
    rig.engine.play_file("birds.wav", wav(&noise)).unwrap();
    assert!(rig.engine.file_playing());
    let heard = rig.play(0.1);
    assert!(rms(&heard) > 0.05, "heard: {}", rms(&heard));
    rig.engine.stop_file();
    rig.play(0.05);
    assert!(!rig.engine.file_playing());
    // Played to its end, it stops by itself.
    rig.engine.play_file("birds.wav", wav(&noise)).unwrap();
    rig.play(0.5);
    assert!(!rig.engine.file_playing());
    let err = rig.engine.play_file("page.html", b"<html>".to_vec());
    assert!(matches!(err, Err(AudioError::Decode { .. })), "{err:?}");
    assert!(!rig.engine.file_playing());
}
