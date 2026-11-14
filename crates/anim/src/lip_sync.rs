//! Lip sync (facial animation F5): a line's visemes over time
//! ([`LipSync`]) blended into a face's Dialogue layer through a viseme
//! map, and lip syncs made offline from phoneme timings or from a
//! recording.
//!
//! Each viseme key lasts until the next one. Neighbours cross-fade over
//! the face's `LipSyncSmoothing` seconds, centred `LipSyncAnticipation`
//! seconds before each key's time (mouths move before the sound comes),
//! and the weights always add up to 1 while the line plays, fading to 0
//! (the face's other layers) before the first key and after the end. A
//! key's weight says how much of its viseme there is over Silence.

use std::sync::OnceLock;

use axcenity_assets::{LipSync, VisemeKey, VisemeMap, VisemeMaps};
use axcenity_core::face::{VISEMES, face_control, viseme};

use crate::face_play::LayerValues;

/// The built-in viseme map (`faces/visemes/default.visemes.json`).
pub fn default_viseme_map() -> &'static VisemeMap {
    static MAP: OnceLock<VisemeMap> = OnceLock::new();
    MAP.get_or_init(|| {
        VisemeMap::from_json(include_str!("../faces/visemes/default.visemes.json"))
            .expect("the built-in viseme map")
    })
}

/// The viseme map `name` names: the project's (by name or path), else the
/// built-in one for `""` or `"default"`.
pub fn find_viseme_map<'a>(project: &'a VisemeMaps, name: &str) -> Option<&'a VisemeMap> {
    let name = name.trim();
    let wanted = if name.is_empty() { "default" } else { name };
    match project.find(wanted) {
        Some((_, map)) => Some(map),
        None if wanted.eq_ignore_ascii_case("default") => Some(default_viseme_map()),
        None => None,
    }
}

/// A viseme map ready to blend: the controls it moves, and each viseme's
/// value for each of them (the jaw's share taken out of `Mouth_Open`).
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledVisemes {
    /// Indices in `FACE_CONTROLS`.
    pub controls: Vec<usize>,
    /// By viseme (in `VISEMES` order), then by `controls`.
    pub values: Vec<Vec<f32>>,
}

impl CompiledVisemes {
    /// Compiles `map`. What doesn't fit (an unknown viseme or control) is
    /// left out and said in the problems.
    pub fn new(map: &VisemeMap) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut controls = Vec::new();
        let mut set: Vec<Vec<(usize, f32)>> = vec![Vec::new(); VISEMES.len()];
        for (name, values) in &map.visemes {
            let Some(index) = viseme(name) else {
                problems.push(format!(
                    "`{name}` isn't a viseme (Silence, AI, E, O, U, FV, L, MBP, WQ, TH, TDN or KG)"
                ));
                continue;
            };
            for (control, &value) in values {
                let Some(control) = face_control(control) else {
                    problems.push(format!("`{control}` (in `{name}`) isn't a face control"));
                    continue;
                };
                set[index].push((control, value.clamp(0.0, 1.0)));
                if !controls.contains(&control) {
                    controls.push(control);
                }
            }
        }
        let share = map.jaw_share.clamp(0.0, 1.0);
        let (mouth, jaw) = (
            face_control("Mouth_Open").expect("standard"),
            face_control("Jaw_Open").expect("standard"),
        );
        if share > 0.0 && controls.contains(&mouth) && !controls.contains(&jaw) {
            controls.push(jaw);
        }
        controls.sort_unstable();
        let values = set
            .iter()
            .map(|pairs| {
                let mut row: Vec<f32> = controls
                    .iter()
                    .map(|control| {
                        pairs
                            .iter()
                            .find(|(c, _)| c == control)
                            .map_or(0.0, |(_, value)| *value)
                    })
                    .collect();
                if share > 0.0
                    && let (Some(m), Some(j)) = (
                        controls.iter().position(|&c| c == mouth),
                        controls.iter().position(|&c| c == jaw),
                    )
                {
                    let open = row[m];
                    row[m] = open * (1.0 - share);
                    row[j] = (row[j] + open * share).min(1.0);
                }
                row
            })
            .collect();
        (Self { controls, values }, problems)
    }

    /// The Dialogue layer for `weights` (one per viseme), all of it
    /// counting `amount` (0 to 1).
    pub(crate) fn layer(&self, weights: &[f32; 12], amount: f32) -> LayerValues {
        let mut layer = LayerValues::empty();
        let total: f32 = weights.iter().sum::<f32>().min(1.0) * amount.clamp(0.0, 1.0);
        if total <= 0.0 {
            return layer;
        }
        let scale = amount.clamp(0.0, 1.0);
        for (slot, &control) in self.controls.iter().enumerate() {
            let value: f32 = weights
                .iter()
                .zip(&self.values)
                .map(|(weight, row)| weight * row[slot])
                .sum();
            layer.value[control] = value * scale;
            layer.cover[control] = total;
        }
        layer
    }
}

/// How lip sync keys blend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LipTiming {
    /// Seconds a cross-fade between neighbours takes.
    pub smoothing: f64,
    /// Seconds before its time a key's cross-fade is centred.
    pub anticipation: f64,
}

impl Default for LipTiming {
    fn default() -> Self {
        Self {
            smoothing: 0.06,
            anticipation: 0.04,
        }
    }
}

/// 0 well before `x`, 1 well after, eased over `width` seconds centred
/// on 0.
fn ramp(x: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return if x >= 0.0 { 1.0 } else { 0.0 };
    }
    let t = (x / width + 0.5).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Each viseme's weight at `time` seconds into `lip_sync`: they add up to
/// 1 while it's said, and to 0 before and after. Unknown visemes count as
/// Silence.
pub fn viseme_weights(lip_sync: &LipSync, time: f64, timing: LipTiming) -> [f32; 12] {
    let mut weights = [0.0f32; 12];
    let keys = &lip_sync.visemes;
    let width = timing.smoothing.max(0.0);
    let early = timing.anticipation;
    for (index, key) in keys.iter().enumerate() {
        let start = key.time - early;
        let end = keys
            .get(index + 1)
            .map_or(lip_sync.duration.max(key.time), |next| next.time)
            - early;
        if time < start - width || time > end + width {
            continue;
        }
        let present = (ramp(time - start, width) - ramp(time - end, width)) as f32;
        if present <= 0.0 {
            continue;
        }
        let said = viseme(&key.viseme).unwrap_or(0);
        let weight = key.weight.clamp(0.0, 1.0);
        weights[said] += present * weight;
        weights[0] += present * (1.0 - weight);
    }
    weights
}

/// The viseme a phoneme looks like: ARPAbet (as CMUdict and most aligners
/// write them, stress digits allowed: `AA1`, `IY0`) or a viseme's own
/// name; pauses (`sil`, `sp`, `pau`, empty) are Silence.
pub fn phoneme_viseme(phoneme: &str) -> Option<usize> {
    let plain: String = phoneme
        .trim()
        .chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>()
        .to_ascii_uppercase();
    let name = match plain.as_str() {
        "" | "SIL" | "SP" | "PAU" | "SPN" => "Silence",
        "AA" | "AH" | "AY" | "AW" | "HH" => "AI",
        "AE" | "EH" | "EY" | "IH" | "IY" | "Y" | "ER" => "E",
        "AO" | "OW" | "OY" => "O",
        "UH" | "UW" => "U",
        "F" | "V" => "FV",
        "L" => "L",
        "M" | "B" | "P" => "MBP",
        "W" | "R" => "WQ",
        "TH" | "DH" => "TH",
        "T" | "D" | "N" | "S" | "Z" | "SH" | "ZH" | "CH" | "JH" => "TDN",
        "K" | "G" | "NG" => "KG",
        _ => return viseme(phoneme),
    };
    viseme(name)
}

/// A lip sync from phoneme timings (`(start, end, phoneme)` in seconds, as
/// a forced aligner or a dialogue tool gives them): one key per phoneme,
/// Silence in the gaps, neighbours that look the same merged. Unknown
/// phonemes are returned beside it.
pub fn lip_sync_from_phonemes(
    phonemes: &[(f64, f64, String)],
    duration: f64,
) -> (LipSync, Vec<String>) {
    let mut unknown = Vec::new();
    let mut keys: Vec<VisemeKey> = Vec::new();
    let push = |keys: &mut Vec<VisemeKey>, time: f64, index: usize| {
        if keys
            .last()
            .is_some_and(|last| viseme(&last.viseme) == Some(index))
        {
            return;
        }
        keys.push(VisemeKey {
            time,
            viseme: VISEMES[index].name.to_owned(),
            weight: 1.0,
        });
    };
    let mut sorted: Vec<&(f64, f64, String)> = phonemes
        .iter()
        .filter(|(start, end, _)| start.is_finite() && end.is_finite())
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut until = 0.0f64;
    for (start, end, phoneme) in sorted {
        if *start > until + 0.01 || keys.is_empty() && *start > 0.0 {
            push(&mut keys, until.min(*start), 0);
        }
        let index = phoneme_viseme(phoneme).unwrap_or_else(|| {
            if !unknown.contains(phoneme) {
                unknown.push(phoneme.clone());
            }
            0
        });
        push(&mut keys, *start, index);
        until = until.max(*end);
    }
    let duration = duration.max(until);
    if until < duration && !keys.is_empty() {
        push(&mut keys, until, 0);
    }
    (
        LipSync {
            duration,
            visemes: keys,
            ..LipSync::default()
        },
        unknown,
    )
}

/// A lip sync worked out from a recording (mono samples at `rate` per
/// second), on the machine: each 20 ms is Silence when quiet, else a
/// viseme guessed from how noisy and how bright it sounds (hiss: TDN; a
/// breathy, bright noise: FV; voiced sounds from brightest to dullest: E,
/// AI, O, U), worded as loud as it is. Runs shorter than 60 ms join the
/// one before. Rough, but it opens and closes the mouth with the words;
/// for exact shapes, give phoneme timings ([`lip_sync_from_phonemes`]) or
/// fix keys by hand.
pub fn lip_sync_from_audio(samples: &[f32], rate: u32) -> LipSync {
    let rate = rate.max(1) as usize;
    let hop = (rate / 50).max(1);
    let window = hop;
    let duration = samples.len() as f64 / rate as f64;
    struct Frame {
        loudness: f32,
        crossings: f32,
        brightness: f32,
    }
    let frames: Vec<Frame> = (0..samples.len().div_ceil(hop))
        .map(|frame| {
            let start = frame * hop;
            let part = &samples[start..(start + window).min(samples.len())];
            let energy = part.iter().map(|s| s * s).sum::<f32>() / part.len().max(1) as f32;
            let change = part
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).powi(2))
                .sum::<f32>()
                / part.len().max(1) as f32;
            let crossings = part
                .windows(2)
                .filter(|pair| (pair[0] >= 0.0) != (pair[1] >= 0.0))
                .count() as f32
                / part.len().max(1) as f32;
            Frame {
                loudness: energy.sqrt(),
                crossings,
                brightness: if energy > 0.0 {
                    (change / energy).sqrt()
                } else {
                    0.0
                },
            }
        })
        .collect();
    let loudest = frames.iter().map(|f| f.loudness).fold(0.0f32, f32::max);
    let quiet = (loudest * 0.08).max(0.004);
    let pick = |frame: &Frame| -> (usize, f32) {
        if frame.loudness < quiet {
            return (0, 1.0);
        }
        let weight = (frame.loudness / loudest).sqrt().clamp(0.35, 1.0);
        let name = if frame.crossings > 0.3 {
            "TDN"
        } else if frame.crossings > 0.18 {
            "FV"
        } else if frame.brightness > 0.5 {
            "E"
        } else if frame.brightness > 0.2 {
            "AI"
        } else if frame.brightness > 0.08 {
            "O"
        } else {
            "U"
        };
        (viseme(name).expect("a viseme"), weight)
    };
    // Runs of the same viseme, then short runs joined to the one before.
    let mut runs: Vec<(usize, usize, f32)> = Vec::new(); // (first frame, viseme, weight)
    for (index, frame) in frames.iter().enumerate() {
        let (said, weight) = pick(frame);
        match runs.last_mut() {
            Some(last) if last.1 == said => last.2 = last.2.max(weight),
            _ => runs.push((index, said, weight)),
        }
    }
    let shortest = (0.06 * 50.0) as usize; // frames
    let mut kept: Vec<(usize, usize, f32)> = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        let next = runs.get(index + 1).map_or(frames.len(), |next| next.0);
        let short = next - run.0 < shortest;
        match kept.last_mut() {
            Some(last) if short || last.1 == run.1 => last.2 = last.2.max(run.2),
            _ => kept.push(*run),
        }
    }
    LipSync {
        duration,
        visemes: kept
            .into_iter()
            .map(|(frame, said, weight)| VisemeKey {
                time: (frame * hop) as f64 / rate as f64,
                viseme: VISEMES[said].name.to_owned(),
                weight: (weight * 100.0).round() / 100.0,
            })
            .collect(),
        ..LipSync::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(keys: &[(f64, &str)], duration: f64) -> LipSync {
        LipSync {
            duration,
            visemes: keys
                .iter()
                .map(|(time, name)| VisemeKey {
                    time: *time,
                    viseme: (*name).into(),
                    weight: 1.0,
                })
                .collect(),
            ..LipSync::default()
        }
    }

    #[test]
    fn the_built_in_map_names_only_visemes_and_controls() {
        let (compiled, problems) = CompiledVisemes::new(default_viseme_map());
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(default_viseme_map().visemes.len(), 12);
        let jaw = face_control("Jaw_Open").unwrap();
        let mouth = face_control("Mouth_Open").unwrap();
        let ai = viseme("AI").unwrap();
        let at = |control| {
            compiled
                .controls
                .iter()
                .position(|&c| c == control)
                .unwrap()
        };
        // 0.75 open, 40% of it carried by the jaw.
        assert!((compiled.values[ai][at(mouth)] - 0.45).abs() < 1e-6);
        assert!((compiled.values[ai][at(jaw)] - 0.3).abs() < 1e-6);
        assert!(
            compiled.values[0].iter().all(|&v| v == 0.0),
            "Silence rests"
        );
        assert!(find_viseme_map(&VisemeMaps::default(), "").is_some());
        assert!(find_viseme_map(&VisemeMaps::default(), "Klingon").is_none());
    }

    #[test]
    fn neighbours_cross_fade_early_and_the_line_fades_in_and_out() {
        let lip = line(&[(0.0, "MBP"), (0.5, "AI"), (1.0, "Silence")], 1.2);
        let timing = LipTiming {
            smoothing: 0.1,
            anticipation: 0.05,
        };
        let at = |time| viseme_weights(&lip, time, timing);
        let (mbp, ai) = (viseme("MBP").unwrap(), viseme("AI").unwrap());
        assert_eq!(at(-1.0), [0.0; 12]);
        assert_eq!(at(0.25)[mbp], 1.0);
        // The change to AI is centred 50 ms early.
        let middle = at(0.45);
        assert!((middle[mbp] - 0.5).abs() < 1e-6 && (middle[ai] - 0.5).abs() < 1e-6);
        assert_eq!(at(0.6)[ai], 1.0);
        for time in [0.0, 0.3, 0.42, 0.47, 0.9, 1.0] {
            let total: f32 = at(time).iter().sum();
            assert!((total - 1.0).abs() < 1e-5 || time < 0.01, "{time}: {total}");
        }
        assert_eq!(
            at(2.0),
            [0.0; 12],
            "after the end the face is its own again"
        );
        // A soft key is partly Silence.
        let mut soft = lip.clone();
        soft.visemes[1].weight = 0.25;
        let weights = viseme_weights(&soft, 0.7, timing);
        assert_eq!((weights[ai], weights[0]), (0.25, 0.75));
    }

    #[test]
    fn the_layer_covers_the_maps_controls_by_how_much_is_said() {
        let (compiled, _) = CompiledVisemes::new(default_viseme_map());
        let mut weights = [0.0; 12];
        weights[viseme("O").unwrap()] = 0.5;
        let layer = compiled.layer(&weights, 1.0);
        let pucker = face_control("Mouth_Pucker").unwrap();
        assert_eq!(layer.cover[pucker], 0.5);
        assert!((layer.value[pucker] - 0.15).abs() < 1e-6, "premultiplied");
        let blink = face_control("Blink_Left").unwrap();
        assert_eq!(layer.cover[blink], 0.0, "the eyes are left alone");
    }

    #[test]
    fn phonemes_become_visemes_with_silence_in_the_gaps() {
        let phonemes = [
            (0.1, 0.2, "HH".to_owned()),
            (0.2, 0.35, "EH1".to_owned()),
            (0.35, 0.45, "L".to_owned()),
            (0.45, 0.6, "OW0".to_owned()),
            (0.9, 1.0, "M".to_owned()),
            (1.0, 1.05, "XX".to_owned()),
        ];
        let (lip, unknown) = lip_sync_from_phonemes(&phonemes, 1.2);
        let said: Vec<(f64, &str)> = lip
            .visemes
            .iter()
            .map(|key| (key.time, key.viseme.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                (0.0, "Silence"),
                (0.1, "AI"),
                (0.2, "E"),
                (0.35, "L"),
                (0.45, "O"),
                (0.6, "Silence"),
                (0.9, "MBP"),
                (1.0, "Silence"),
            ]
        );
        assert_eq!(unknown, ["XX"]);
        assert_eq!(lip.duration, 1.2);
    }

    #[test]
    fn a_recording_opens_the_mouth_for_sounds_and_closes_it_between() {
        let rate = 16_000u32;
        let mut samples = vec![0.0f32; rate as usize / 4];
        // A low hum: a dull voiced vowel.
        samples
            .extend((0..rate / 2).map(|i| {
                0.5 * (i as f32 * 2.0 * std::f32::consts::PI * 140.0 / rate as f32).sin()
            }));
        samples.extend(vec![0.0; rate as usize / 4]);
        // Hiss.
        let mut seed = 9u32;
        samples.extend((0..rate / 4).map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32 - 0.5) * 0.6
        }));
        samples.extend(vec![0.0; rate as usize / 4]);
        let lip = lip_sync_from_audio(&samples, rate);
        let said: Vec<&str> = lip.visemes.iter().map(|key| key.viseme.as_str()).collect();
        assert_eq!(
            said,
            ["Silence", "U", "Silence", "TDN", "Silence"],
            "{lip:?}"
        );
        assert!((lip.visemes[1].time - 0.25).abs() < 0.03);
        assert!((lip.duration - 1.5).abs() < 1e-9);
    }
}
