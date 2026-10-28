//! Sound effects as Kira effects (Phase 52). A Sound's or SoundGroup's
//! effects become an [`EffectChain`] on its track: built once for the
//! effects' shapes (which ones, in which order), then kept in line with
//! their settings and `Enabled` through Kira's handles. An effect that's
//! off is passed by (its mix all dry, or its gains flat).
//!
//! Kira has no pitch shifter, so [`PitchShift`] is a small one of our own,
//! and [`Meter`] measures a track's level for the editor's Mixer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use kira::effect::compressor::{CompressorBuilder, CompressorHandle};
use kira::effect::delay::{DelayBuilder, DelayHandle};
use kira::effect::distortion::{DistortionBuilder, DistortionHandle, DistortionKind};
use kira::effect::eq_filter::{EqFilterBuilder, EqFilterHandle, EqFilterKind};
use kira::effect::filter::{FilterBuilder, FilterHandle, FilterMode};
use kira::effect::reverb::{ReverbBuilder, ReverbHandle};
use kira::effect::volume_control::{VolumeControlBuilder, VolumeControlHandle};
use kira::effect::{Effect as KiraEffect, EffectBuilder};
use kira::info::Info;
use kira::track::{MainTrackBuilder, SpatialTrackBuilder, TrackBuilder};
use kira::{Decibels, Frame, Mix, Tween};

use crate::mix::{Effect, EffectSettings, Reverb};

/// A Kira track builder effects can be added to.
pub(crate) trait AddEffects {
    fn add<B: EffectBuilder>(&mut self, builder: B) -> B::Handle;
}

impl AddEffects for TrackBuilder {
    fn add<B: EffectBuilder>(&mut self, builder: B) -> B::Handle {
        self.add_effect(builder)
    }
}

impl AddEffects for SpatialTrackBuilder {
    fn add<B: EffectBuilder>(&mut self, builder: B) -> B::Handle {
        self.add_effect(builder)
    }
}

impl AddEffects for MainTrackBuilder {
    fn add<B: EffectBuilder>(&mut self, builder: B) -> B::Handle {
        self.add_effect(builder)
    }
}

/// Settings move over this long, so a change doesn't click.
fn quick() -> Tween {
    Tween {
        duration: Duration::from_millis(20),
        ..Tween::default()
    }
}

/// The mix and output level that give `dry` and `wet` decibels from
/// Kira's equal-power mix (`wet * √mix + dry * √(1 - mix)`).
fn dry_wet(dry: f64, wet: f64) -> (Mix, Decibels) {
    let dry = amplitude(dry);
    let wet = amplitude(wet);
    let level = (dry * dry + wet * wet).sqrt();
    if level <= 0.0 {
        return (Mix::DRY, Decibels::SILENCE);
    }
    (
        Mix(((wet * wet) / (level * level)) as f32),
        Decibels((20.0 * level.log10()) as f32),
    )
}

/// Decibels as an amplitude; -60 and below (Kira's silence) is 0.
fn amplitude(decibels: f64) -> f64 {
    if decibels <= f64::from(Decibels::SILENCE.0) {
        0.0
    } else {
        10f64.powf(decibels / 20.0)
    }
}

/// Kira's reverb feedback and damping for `reverb`. A comb filter of about
/// 30 ms falls 60 dB in `decay_time` when it keeps `10^(-3 · 0.03 / decay)`
/// of itself each pass.
pub(crate) fn reverb_feedback(reverb: Reverb) -> (f64, f64) {
    let feedback = 10f64.powf(-3.0 * 0.03 / reverb.decay_time).clamp(0.0, 0.98);
    let damping = 0.2 + 0.6 * (1.0 - reverb.density);
    (feedback, damping)
}

/// One built effect and the settings it was last given.
struct Built {
    effect: Effect,
    handle: Handle,
}

enum Handle {
    Reverb(ReverbHandle, VolumeControlHandle),
    Equalizer([EqFilterHandle; 3]),
    Compressor(CompressorHandle),
    Echo(DelayHandle, VolumeControlHandle),
    Distortion(DistortionHandle),
    PitchShift(Arc<PitchParams>),
    Filter(FilterHandle),
}

/// The effects on one track, in order.
#[derive(Default)]
pub(crate) struct EffectChain {
    built: Vec<Built>,
}

impl EffectChain {
    /// Adds `effects` to `track`, in order, as they're set now.
    pub(crate) fn build(track: &mut impl AddEffects, effects: &[Effect]) -> Self {
        let built = effects
            .iter()
            .map(|&effect| Built {
                effect,
                handle: add(track, effect),
            })
            .collect();
        Self { built }
    }

    /// Brings each effect in line with `effects`, which have the shapes the
    /// chain was built for.
    pub(crate) fn update(&mut self, effects: &[Effect]) {
        for (built, &effect) in self.built.iter_mut().zip(effects) {
            if built.effect != effect {
                set(&mut built.handle, effect, quick());
                built.effect = effect;
            }
        }
    }
}

fn add(track: &mut impl AddEffects, effect: Effect) -> Handle {
    let on = effect.enabled;
    match effect.settings {
        EffectSettings::Reverb(reverb) => {
            let (feedback, damping) = reverb_feedback(reverb);
            let (mix, level) = reverb_mix(reverb, on);
            let handle = track.add(
                ReverbBuilder::new()
                    .feedback(feedback)
                    .damping(damping)
                    .mix(mix),
            );
            Handle::Reverb(handle, track.add(VolumeControlBuilder(level.into())))
        }
        EffectSettings::Equalizer {
            low_gain,
            mid_gain,
            high_gain,
            mid_low,
            mid_high,
        } => {
            let gain = |gain: f64| Decibels(if on { gain as f32 } else { 0.0 });
            let (center, q) = mid_band(mid_low, mid_high);
            Handle::Equalizer([
                track.add(EqFilterBuilder::new(
                    EqFilterKind::LowShelf,
                    mid_low,
                    gain(low_gain),
                    0.7,
                )),
                track.add(EqFilterBuilder::new(
                    EqFilterKind::Bell,
                    center,
                    gain(mid_gain),
                    q,
                )),
                track.add(EqFilterBuilder::new(
                    EqFilterKind::HighShelf,
                    mid_high,
                    gain(high_gain),
                    0.7,
                )),
            ])
        }
        EffectSettings::Compressor {
            threshold,
            ratio,
            attack,
            release,
            makeup,
        } => Handle::Compressor(
            track.add(
                CompressorBuilder::new()
                    .threshold(threshold)
                    .ratio(ratio)
                    .attack_duration(Duration::from_secs_f64(attack))
                    .release_duration(Duration::from_secs_f64(release))
                    .makeup_gain(Decibels(makeup as f32))
                    .mix(on_mix(on)),
            ),
        ),
        EffectSettings::Echo {
            delay,
            feedback,
            dry_level,
            wet_level,
        } => {
            let (mix, level) = if on {
                dry_wet(dry_level, wet_level)
            } else {
                (Mix::DRY, Decibels::IDENTITY)
            };
            let handle = track.add(
                DelayBuilder::new()
                    .delay_time(Duration::from_secs_f64(delay))
                    .feedback(Decibels(feedback as f32))
                    .mix(mix),
            );
            Handle::Echo(handle, track.add(VolumeControlBuilder(level.into())))
        }
        EffectSettings::Distortion { level } => Handle::Distortion(
            track.add(
                DistortionBuilder::new()
                    .kind(DistortionKind::SoftClip)
                    .drive(drive(level))
                    .mix(on_mix(on)),
            ),
        ),
        EffectSettings::PitchShift { octave } => {
            let params = Arc::new(PitchParams::new(octave, on));
            track.add(PitchShiftBuilder(Arc::clone(&params)));
            Handle::PitchShift(params)
        }
        EffectSettings::LowPass { cutoff, resonance }
        | EffectSettings::HighPass { cutoff, resonance } => {
            let mode = if matches!(effect.settings, EffectSettings::LowPass { .. }) {
                FilterMode::LowPass
            } else {
                FilterMode::HighPass
            };
            Handle::Filter(
                track.add(
                    FilterBuilder::new()
                        .mode(mode)
                        .cutoff(cutoff)
                        .resonance(resonance)
                        .mix(on_mix(on)),
                ),
            )
        }
    }
}

fn set(handle: &mut Handle, effect: Effect, tween: Tween) {
    let on = effect.enabled;
    match (handle, effect.settings) {
        (Handle::Reverb(reverb, volume), EffectSettings::Reverb(settings)) => {
            let (feedback, damping) = reverb_feedback(settings);
            let (mix, level) = reverb_mix(settings, on);
            reverb.set_feedback(feedback, tween);
            reverb.set_damping(damping, tween);
            reverb.set_mix(mix, tween);
            volume.set_volume(level, tween);
        }
        (
            Handle::Equalizer([low, mid, high]),
            EffectSettings::Equalizer {
                low_gain,
                mid_gain,
                high_gain,
                mid_low,
                mid_high,
            },
        ) => {
            let gain = |gain: f64| Decibels(if on { gain as f32 } else { 0.0 });
            let (center, q) = mid_band(mid_low, mid_high);
            low.set_frequency(mid_low, tween);
            low.set_gain(gain(low_gain), tween);
            mid.set_frequency(center, tween);
            mid.set_q(q, tween);
            mid.set_gain(gain(mid_gain), tween);
            high.set_frequency(mid_high, tween);
            high.set_gain(gain(high_gain), tween);
        }
        (
            Handle::Compressor(compressor),
            EffectSettings::Compressor {
                threshold,
                ratio,
                attack,
                release,
                makeup,
            },
        ) => {
            compressor.set_threshold(threshold, tween);
            compressor.set_ratio(ratio, tween);
            compressor.set_attack_duration(Duration::from_secs_f64(attack), tween);
            compressor.set_release_duration(Duration::from_secs_f64(release), tween);
            compressor.set_makeup_gain(Decibels(makeup as f32), tween);
            compressor.set_mix(on_mix(on), tween);
        }
        (
            Handle::Echo(delay, volume),
            EffectSettings::Echo {
                feedback,
                dry_level,
                wet_level,
                ..
            },
        ) => {
            let (mix, level) = if on {
                dry_wet(dry_level, wet_level)
            } else {
                (Mix::DRY, Decibels::IDENTITY)
            };
            delay.set_feedback(Decibels(feedback as f32), tween);
            delay.set_mix(mix, tween);
            volume.set_volume(level, tween);
        }
        (Handle::Distortion(distortion), EffectSettings::Distortion { level }) => {
            distortion.set_drive(drive(level), tween);
            distortion.set_mix(on_mix(on), tween);
        }
        (Handle::PitchShift(params), EffectSettings::PitchShift { octave }) => {
            params.set(octave, on);
        }
        (
            Handle::Filter(filter),
            EffectSettings::LowPass { cutoff, resonance }
            | EffectSettings::HighPass { cutoff, resonance },
        ) => {
            filter.set_cutoff(cutoff, tween);
            filter.set_resonance(resonance, tween);
            filter.set_mix(on_mix(on), tween);
        }
        // The chain is rebuilt whenever an effect's kind changes, so a
        // handle always meets settings of its own kind.
        _ => {}
    }
}

fn on_mix(on: bool) -> Mix {
    if on { Mix::WET } else { Mix::DRY }
}

fn reverb_mix(reverb: Reverb, on: bool) -> (Mix, Decibels) {
    if on {
        dry_wet(reverb.dry_level, reverb.wet_level)
    } else {
        (Mix::DRY, Decibels::IDENTITY)
    }
}

/// `Level` 0 to 1 as up to 24 dB of drive.
fn drive(level: f64) -> Decibels {
    Decibels((level * 24.0) as f32)
}

/// The middle of the mids (between their edges, as the ear hears it) and
/// a Q as wide as they are.
fn mid_band(low: f64, high: f64) -> (f64, f64) {
    let center = (low * high).sqrt();
    (center, (center / (high - low)).clamp(0.1, 10.0))
}

/// A ReverbZone's reverb on one spatial Sound's track. It's always there,
/// all dry outside the zones.
pub(crate) struct ZoneReverb(ReverbHandle);

impl ZoneReverb {
    /// Adds it to `track`, `blend` (0 to 1) of the way into `reverb`.
    pub(crate) fn build(track: &mut impl AddEffects, reverb: Reverb, blend: f64) -> Self {
        let (feedback, damping) = reverb_feedback(reverb);
        Self(
            track.add(
                ReverbBuilder::new()
                    .feedback(feedback)
                    .damping(damping)
                    .mix(zone_mix(reverb, blend)),
            ),
        )
    }

    /// Moves to `blend` of `reverb` over `tween`.
    pub(crate) fn set(&mut self, reverb: Reverb, blend: f64, tween: Tween) {
        let (feedback, damping) = reverb_feedback(reverb);
        self.0.set_feedback(feedback, tween);
        self.0.set_damping(damping, tween);
        self.0.set_mix(zone_mix(reverb, blend), tween);
    }
}

/// The mix a zone's preset sounds like, `blend` of the way in. The dry
/// level stays as the sound is.
fn zone_mix(reverb: Reverb, blend: f64) -> Mix {
    let (mix, _) = dry_wet(0.0, reverb.wet_level);
    Mix(mix.0 * blend.clamp(0.0, 1.0) as f32)
}

/// A pitch shifter's settings, shared with the audio thread.
pub(crate) struct PitchParams {
    ratio: AtomicU32,
    on: AtomicU32,
}

impl PitchParams {
    fn new(octave: f64, on: bool) -> Self {
        let params = Self {
            ratio: AtomicU32::new(0),
            on: AtomicU32::new(0),
        };
        params.set(octave, on);
        params
    }

    fn set(&self, octave: f64, on: bool) {
        self.ratio
            .store((octave as f32).to_bits(), Ordering::Relaxed);
        self.on.store(u32::from(on), Ordering::Relaxed);
    }
}

struct PitchShiftBuilder(Arc<PitchParams>);

impl EffectBuilder for PitchShiftBuilder {
    type Handle = ();

    fn build(self) -> (Box<dyn KiraEffect>, Self::Handle) {
        (
            Box::new(PitchShift {
                params: self.0,
                buffer: Vec::new(),
                write: 0,
                phase: 0.0,
            }),
            (),
        )
    }
}

/// Shifts pitch without changing speed: two read heads sweep through the
/// last `WINDOW` of sound at `ratio` speed, half a window apart, each faded
/// in and out so their jumps back don't click.
struct PitchShift {
    params: Arc<PitchParams>,
    buffer: Vec<Frame>,
    write: usize,
    /// How far through its window the first head is, 0 to 1.
    phase: f64,
}

/// Seconds of sound the heads read from.
const PITCH_WINDOW: f64 = 0.05;

impl KiraEffect for PitchShift {
    fn init(&mut self, sample_rate: u32, _internal_buffer_size: usize) {
        self.on_change_sample_rate(sample_rate);
    }

    fn on_change_sample_rate(&mut self, sample_rate: u32) {
        let length = ((f64::from(sample_rate) * PITCH_WINDOW) as usize).max(4);
        self.buffer = vec![Frame::ZERO; length + 2];
        self.write = 0;
        self.phase = 0.0;
    }

    fn process(&mut self, input: &mut [Frame], _dt: f64, _info: &Info) {
        if self.buffer.is_empty() {
            return;
        }
        let ratio = f64::from(f32::from_bits(self.params.ratio.load(Ordering::Relaxed)));
        let on = self.params.on.load(Ordering::Relaxed) != 0;
        let length = self.buffer.len();
        let window = (length - 2) as f64;
        for frame in input.iter_mut() {
            self.buffer[self.write] = *frame;
            // Slower than 1 the delay grows, faster it shrinks.
            self.phase = (self.phase + (1.0 - ratio) / window).rem_euclid(1.0);
            if on {
                let second = (self.phase + 0.5).rem_euclid(1.0);
                let read = |phase: f64| {
                    let delay = phase * window;
                    let position = (self.write as f64 - delay).rem_euclid(length as f64);
                    let index = position.floor() as usize % length;
                    let next = (index + 1) % length;
                    let fraction = (position - position.floor()) as f32;
                    self.buffer[index] * (1.0 - fraction) + self.buffer[next] * fraction
                };
                // Triangles that add up to 1, each 0 as its head jumps.
                let fade = |phase: f64| (1.0 - (2.0 * phase - 1.0).abs()) as f32;
                *frame = read(self.phase) * fade(self.phase) + read(second) * fade(second);
            }
            self.write = (self.write + 1) % length;
        }
    }
}

/// The loudest a track has been since the level was last taken, shared
/// with the audio thread. A peak amplitude, 1 being full scale.
#[derive(Default)]
pub(crate) struct Level(AtomicU32);

impl Level {
    /// The peak since the last call, and starts over.
    pub(crate) fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }

    fn note(&self, peak: f32) {
        // Bit patterns of positive floats sort as the floats do.
        self.0.fetch_max(peak.to_bits(), Ordering::Relaxed);
    }
}

/// Measures the level of the track it's last on, changing nothing.
pub(crate) struct Meter(pub(crate) Arc<Level>);

impl EffectBuilder for Meter {
    type Handle = ();

    fn build(self) -> (Box<dyn KiraEffect>, Self::Handle) {
        (Box::new(self), ())
    }
}

impl KiraEffect for Meter {
    fn process(&mut self, input: &mut [Frame], _dt: f64, _info: &Info) {
        let peak = input
            .iter()
            .map(|frame| frame.left.abs().max(frame.right.abs()))
            .filter(|peak| peak.is_finite())
            .fold(0.0f32, f32::max);
        self.0.note(peak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_and_wet_levels_come_out_of_an_equal_power_mix() {
        let (mix, level) = dry_wet(-6.0, 0.0);
        let gain = level.as_amplitude();
        let dry = gain * (1.0 - mix.0).sqrt();
        let wet = gain * mix.0.sqrt();
        assert!((dry - 0.501).abs() < 0.01, "{dry}");
        assert!((wet - 1.0).abs() < 0.01, "{wet}");
        assert_eq!(dry_wet(-80.0, -80.0).0, Mix::DRY);
    }

    #[test]
    fn longer_decay_feeds_back_more() {
        let short = reverb_feedback(Reverb::preset("Room")).0;
        let long = reverb_feedback(Reverb::preset("Arena")).0;
        assert!(short < long && long < 0.99, "{short} {long}");
    }

    #[test]
    fn the_pitch_shifter_raises_a_tone_by_its_octave() {
        let rate = 48_000;
        let params = Arc::new(PitchParams::new(2.0, true));
        let (mut effect, ()) = PitchShiftBuilder(Arc::clone(&params)).build();
        effect.init(rate, 128);
        let tone = 220.0;
        let mut frames: Vec<Frame> = (0..rate as usize)
            .map(|i| {
                let value = (std::f32::consts::TAU * tone * i as f32 / rate as f32).sin();
                Frame::from_mono(value)
            })
            .collect();
        let info = kira::info::MockInfoBuilder::new().build();
        for chunk in frames.chunks_mut(128) {
            effect.process(chunk, 1.0 / f64::from(rate), &info);
        }
        // Count rising zero crossings in the last half second.
        let tail = &frames[rate as usize / 2..];
        let crossings = tail
            .windows(2)
            .filter(|pair| pair[0].left < 0.0 && pair[1].left >= 0.0)
            .count();
        let heard = crossings as f32 * 2.0;
        assert!(
            (heard - tone * 2.0).abs() < tone * 0.25,
            "an octave up from {tone} Hz, but it's about {heard} Hz"
        );
    }
}
