//! Procedural landscapes.
//!
//! [`GeneratorSettings`] say what to make: which algorithm, its version, a
//! seed and the shape controls. A [`HeightGenerator`] turns them into
//! heights. The same settings give the same landscape at any resolution
//! (sizes are in studs), and saved terrains keep the settings that made
//! them, so a result can be made again or tweaked later.
//!
//! The one algorithm so far, `fractal`, mixes several techniques: fractal
//! noise for rolling ground, ridged noise for mountain crests, domain
//! warping so shapes bend naturally, channels for valleys, wave fields for
//! dunes, a radial falloff for islands and craters, and a height curve with
//! terraces and plains. New algorithms implement [`HeightGenerator`] and
//! join [`GENERATORS`]; the Terrain Studio lists them without changes.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::job::{JobControl, parallel_rows};
use crate::noise::{Noise, lerp, smoothstep};

/// The fractal generator's version. Saved with the settings: bump it
/// whenever the same settings would make a different landscape.
pub const GENERATOR_VERSION: u32 = 1;

/// What the generator is asked to fill: `size_x` × `size_z` samples over
/// `world_size` studs (X, Z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub size_x: usize,
    pub size_z: usize,
    pub world_size: Vec2,
}

/// A starting point for the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    #[default]
    RollingHills,
    MountainRange,
    AlpineValley,
    DesertDunes,
    RockyHighlands,
    Island,
    Flatlands,
    Crater,
    SnowyMountains,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 10] = [
        Preset::RollingHills,
        Preset::MountainRange,
        Preset::AlpineValley,
        Preset::DesertDunes,
        Preset::RockyHighlands,
        Preset::Island,
        Preset::Flatlands,
        Preset::Crater,
        Preset::SnowyMountains,
        Preset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::RollingHills => "Rolling Hills",
            Preset::MountainRange => "Mountain Range",
            Preset::AlpineValley => "Alpine Valley",
            Preset::DesertDunes => "Desert Dunes",
            Preset::RockyHighlands => "Rocky Highlands",
            Preset::Island => "Island",
            Preset::Flatlands => "Flatlands",
            Preset::Crater => "Crater",
            Preset::SnowyMountains => "Snow-Covered Mountains",
            Preset::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Preset::RollingHills => "Soft, grassy hills with gentle valleys.",
            Preset::MountainRange => "Tall ridged peaks with sharp crests.",
            Preset::AlpineValley => "Mountains around a wide, flat valley floor.",
            Preset::DesertDunes => "Long, wind-shaped sand dunes.",
            Preset::RockyHighlands => "High, broken ground with rocky steps.",
            Preset::Island => "Land rising from the sea, sinking at the edges.",
            Preset::Flatlands => "Nearly flat ground with a little variation.",
            Preset::Crater => "A large impact crater with a raised rim.",
            Preset::SnowyMountains => "Very tall peaks, ready for snow on top.",
            Preset::Custom => "Your own settings.",
        }
    }

    /// The settings this preset starts with.
    pub fn settings(self) -> GeneratorSettings {
        let base = GeneratorSettings {
            preset: self,
            ..GeneratorSettings::default()
        };
        match self {
            Preset::RollingHills | Preset::Custom => base,
            Preset::MountainRange => GeneratorSettings {
                base_height: 0.12,
                amplitude: 0.75,
                feature_size: 220.0,
                octaves: 7,
                persistence: 0.5,
                mountains: 0.85,
                valleys: 0.2,
                roughness: 0.25,
                warp: 0.4,
                plains: 0.0,
                curve: 1.6,
                ..base
            },
            Preset::AlpineValley => GeneratorSettings {
                base_height: 0.15,
                amplitude: 0.7,
                feature_size: 180.0,
                octaves: 6,
                mountains: 0.75,
                valleys: 0.6,
                roughness: 0.2,
                warp: 0.3,
                plains: 0.6,
                curve: 1.5,
                ..base
            },
            Preset::DesertDunes => GeneratorSettings {
                base_height: 0.2,
                amplitude: 0.12,
                feature_size: 120.0,
                octaves: 3,
                roughness: 0.05,
                warp: 0.2,
                plains: 0.0,
                dunes: 1.0,
                curve: 1.0,
                ..base
            },
            Preset::RockyHighlands => GeneratorSettings {
                base_height: 0.3,
                amplitude: 0.35,
                feature_size: 90.0,
                octaves: 6,
                persistence: 0.55,
                mountains: 0.5,
                roughness: 0.5,
                terraces: 6,
                terrace_strength: 0.5,
                plains: 0.0,
                curve: 1.1,
                ..base
            },
            Preset::Island => GeneratorSettings {
                base_height: 0.2,
                amplitude: 0.5,
                feature_size: 150.0,
                mountains: 0.4,
                roughness: 0.15,
                warp: 0.5,
                island: 1.0,
                plains: 0.1,
                curve: 1.3,
                ..base
            },
            Preset::Flatlands => GeneratorSettings {
                base_height: 0.25,
                amplitude: 0.04,
                feature_size: 300.0,
                octaves: 3,
                roughness: 0.05,
                plains: 0.8,
                ..base
            },
            Preset::Crater => GeneratorSettings {
                base_height: 0.3,
                amplitude: 0.15,
                feature_size: 200.0,
                roughness: 0.1,
                crater: 1.0,
                plains: 0.0,
                ..base
            },
            Preset::SnowyMountains => GeneratorSettings {
                base_height: 0.08,
                amplitude: 0.85,
                feature_size: 240.0,
                octaves: 7,
                mountains: 1.0,
                valleys: 0.3,
                roughness: 0.25,
                warp: 0.35,
                plains: 0.0,
                curve: 1.8,
                ..base
            },
        }
    }
}

/// Everything a generator needs. Heights are fractions of the terrain's
/// height range (`Size.Y`); `feature_size` is in studs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorSettings {
    /// Which [`HeightGenerator`] makes it, by name.
    pub algorithm: String,
    /// The algorithm's version when the settings were saved.
    pub version: u32,
    pub preset: Preset,
    pub seed: u64,
    /// Where the lowlands sit, 0 to 1.
    pub base_height: f32,
    /// How much the land rises and falls, 0 to 1.
    pub amplitude: f32,
    /// Studs across the biggest hills.
    pub feature_size: f32,
    /// Layers of detail, each `lacunarity` times smaller and `persistence`
    /// times weaker.
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    /// 0 to 1: rounded hills to ridged mountains.
    pub mountains: f32,
    /// 0 to 1: how deep river valleys cut.
    pub valleys: f32,
    /// 0 to 1: small bumps on top.
    pub roughness: f32,
    /// 0 to 1: how much shapes bend and swirl.
    pub warp: f32,
    /// 0 to 1: sinks the edges, leaving an island.
    pub island: f32,
    /// 0 to 1: flattens the lowlands into plains.
    pub plains: f32,
    /// Steps in the height (0 for none), and how sharp they are (0 to 1).
    pub terraces: u32,
    pub terrace_strength: f32,
    /// 0 to 1: long wind-blown dunes.
    pub dunes: f32,
    /// Degrees the dunes run across.
    pub dune_direction: f32,
    /// 0 to 1: a big crater in the middle.
    pub crater: f32,
    /// The height curve: above 1 makes broad valleys and steep peaks.
    pub curve: f32,
    /// Heights stay between these, 0 to 1.
    pub min_height: f32,
    pub max_height: f32,
}

impl Default for GeneratorSettings {
    /// Rolling hills.
    fn default() -> Self {
        Self {
            algorithm: FRACTAL.to_owned(),
            version: GENERATOR_VERSION,
            preset: Preset::RollingHills,
            seed: 1,
            base_height: 0.22,
            amplitude: 0.2,
            feature_size: 160.0,
            octaves: 5,
            persistence: 0.45,
            lacunarity: 2.0,
            mountains: 0.0,
            valleys: 0.1,
            roughness: 0.1,
            warp: 0.3,
            island: 0.0,
            plains: 0.2,
            terraces: 0,
            terrace_strength: 0.0,
            dunes: 0.0,
            dune_direction: 30.0,
            crater: 0.0,
            curve: 1.2,
            min_height: 0.0,
            max_height: 1.0,
        }
    }
}

impl GeneratorSettings {
    /// The settings as saved with a terrain.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("settings are plain numbers and names")
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum GenerateError {
    #[error("there's no terrain generator called `{0}`")]
    UnknownAlgorithm(String),
    #[error(
        "these settings are from version {found} of `{name}`, but this engine has version {have}; update Axcenity to make the same landscape"
    )]
    TooNew { name: String, found: u32, have: u32 },
    #[error("{0}")]
    Invalid(String),
}

/// Something that makes heights from settings.
pub trait HeightGenerator: Sync {
    /// The name settings use to pick it.
    fn name(&self) -> &'static str;
    fn version(&self) -> u32;
    /// Heights from 0 to 1, row by row, or `None` if cancelled.
    fn generate(
        &self,
        settings: &GeneratorSettings,
        target: Target,
        control: &JobControl,
    ) -> Option<Vec<f32>>;
}

const FRACTAL: &str = "fractal";

/// Every generator, by name.
pub static GENERATORS: &[&dyn HeightGenerator] = &[&Fractal];

/// Makes heights with the generator `settings` name, checking the
/// settings first. `Ok(None)` if cancelled.
pub fn generate(
    settings: &GeneratorSettings,
    target: Target,
    control: &JobControl,
) -> Result<Option<Vec<f32>>, GenerateError> {
    let generator = check(settings, target)?;
    Ok(generator.generate(settings, target, control))
}

/// The generator `settings` name, if the settings and size can be used:
/// a known algorithm, not from a newer engine, a terrain size that's
/// allowed and numbers that are numbers.
pub fn check(
    settings: &GeneratorSettings,
    target: Target,
) -> Result<&'static dyn HeightGenerator, GenerateError> {
    let generator = GENERATORS
        .iter()
        .find(|generator| generator.name() == settings.algorithm)
        .ok_or_else(|| GenerateError::UnknownAlgorithm(settings.algorithm.clone()))?;
    if settings.version > generator.version() {
        return Err(GenerateError::TooNew {
            name: settings.algorithm.clone(),
            found: settings.version,
            have: generator.version(),
        });
    }
    if !axcenity_core::valid_terrain_size(target.size_x, target.size_z) {
        return Err(GenerateError::Invalid(format!(
            "a terrain can't have {} × {} samples",
            target.size_x, target.size_z
        )));
    }
    let numbers = [
        settings.base_height,
        settings.amplitude,
        settings.feature_size,
        settings.persistence,
        settings.lacunarity,
        settings.curve,
        target.world_size.x,
        target.world_size.y,
    ];
    if numbers.iter().any(|n| !n.is_finite()) || target.world_size.min_element() <= 0.0 {
        return Err(GenerateError::Invalid(
            "every setting must be a number, and the terrain must have a size".to_owned(),
        ));
    }
    Ok(*generator)
}

/// The fractal landscape generator (see the module docs).
struct Fractal;

impl HeightGenerator for Fractal {
    fn name(&self) -> &'static str {
        FRACTAL
    }

    fn version(&self) -> u32 {
        GENERATOR_VERSION
    }

    fn generate(
        &self,
        settings: &GeneratorSettings,
        target: Target,
        control: &JobControl,
    ) -> Option<Vec<f32>> {
        let cell = Vec2::new(
            target.world_size.x / (target.size_x - 1) as f32,
            target.world_size.y / (target.size_z - 1) as f32,
        );
        let rows = parallel_rows(target.size_z, control, (0.0, 1.0), |z| {
            (0..target.size_x)
                .map(|x| {
                    let at = Vec2::new(x as f32 * cell.x, z as f32 * cell.y);
                    fractal_height(settings, at, target.world_size)
                })
                .collect::<Vec<f32>>()
        })?;
        Some(rows.concat())
    }
}

/// The fractal landscape's height at `at` studs from the terrain's corner.
fn fractal_height(s: &GeneratorSettings, at: Vec2, world: Vec2) -> f32 {
    let noise = Noise::new(s.seed);
    let octaves = s.octaves.clamp(1, 12);
    let persistence = s.persistence.clamp(0.05, 0.95);
    let lacunarity = s.lacunarity.clamp(1.2, 4.0);
    let feature = s.feature_size.max(1.0);
    let mut q = at / feature;

    // Bends the space the shapes are drawn in, so they swirl and branch.
    if s.warp > 0.0 {
        let p = q * 0.7;
        let wx = noise.offset(1).fbm(p.x, p.y, 4, 0.5, 2.0);
        let wz = noise.offset(2).fbm(p.x + 5.2, p.y + 1.3, 4, 0.5, 2.0);
        q += Vec2::new(wx, wz) * s.warp * 1.5;
    }

    let hills = noise.fbm(q.x, q.y, octaves, persistence, lacunarity) * 0.5 + 0.5;
    let mut shape = hills;
    if s.mountains > 0.0 {
        let ridges = noise
            .offset(3)
            .ridged(q.x * 0.6, q.y * 0.6, octaves, persistence, lacunarity);
        shape = lerp(hills, ridges * 1.1, s.mountains.clamp(0.0, 1.0));
    }
    // Rivers: channels where a slow noise crosses zero.
    if s.valleys > 0.0 {
        let n = noise
            .offset(5)
            .fbm(q.x * 0.35, q.y * 0.35, 3, 0.5, 2.0)
            .abs();
        let carve = 1.0 - smoothstep(0.0, 0.16, n);
        shape -= s.valleys.clamp(0.0, 1.0) * carve * 0.45 * shape.max(0.2);
    }
    if s.dunes > 0.0 {
        let (sin, cos) = s.dune_direction.to_radians().sin_cos();
        let across = (at.x * cos + at.y * sin) / (feature * 0.18);
        let bend = noise.offset(6).fbm(q.x * 0.6, q.y * 0.6, 2, 0.5, 2.0) * 3.0;
        // Gentle on the windward side, steep on the other.
        let wave = (across + bend).sin() * 0.5 + 0.5;
        let dune = wave.powf(2.2) * (0.6 + 0.4 * hills);
        shape = lerp(shape, dune, s.dunes.clamp(0.0, 1.0));
    }
    if s.roughness > 0.0 {
        let detail = noise.offset(4).fbm(q.x * 6.0, q.y * 6.0, 4, 0.55, 2.0);
        shape += detail * s.roughness.clamp(0.0, 1.0) * 0.18;
    }

    // The height curve: broad lowlands and steep peaks above 1.
    let mut shape = shape.clamp(0.0, 1.5);
    if s.curve > 0.0 && s.curve != 1.0 {
        shape = shape.powf(s.curve.clamp(0.2, 5.0));
    }
    if s.terraces > 0 && s.terrace_strength > 0.0 {
        let steps = s.terraces as f32;
        let t = shape * steps;
        let stepped = (t.floor() + smoothstep(0.3, 0.7, t.fract())) / steps;
        shape = lerp(shape, stepped, s.terrace_strength.clamp(0.0, 1.0));
    }
    // Plains: lowlands pulled up flat toward a third of the way up.
    if s.plains > 0.0 {
        let floor = 0.3;
        if shape < floor {
            shape = floor - (floor - shape) * (1.0 - s.plains.clamp(0.0, 1.0));
        }
    }
    let mut height = s.base_height + s.amplitude * (shape - 0.3);

    // How far from the middle, 0 there and 1 at the middle of each edge.
    let from_middle = ((at - world / 2.0) / (world / 2.0)).length();
    if s.island > 0.0 {
        let sink = smoothstep(0.45, 1.0, from_middle) * s.island.clamp(0.0, 1.0);
        height = lerp(height, -0.05, sink);
    }
    if s.crater > 0.0 {
        let r = from_middle / 0.6;
        let rim = (-((r - 0.85) / 0.14).powi(2)).exp() * 0.22;
        let bowl = -(1.0 - smoothstep(0.0, 0.85, r)) * 0.3;
        height += (rim + bowl) * s.crater.clamp(0.0, 1.0);
    }
    let low = s.min_height.clamp(0.0, 1.0);
    let high = s.max_height.clamp(low, 1.0);
    height.clamp(low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(size: usize) -> Target {
        Target {
            size_x: size,
            size_z: size,
            world_size: Vec2::splat(512.0),
        }
    }

    fn make(settings: &GeneratorSettings, size: usize) -> Vec<f32> {
        generate(settings, target(size), &JobControl::new())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn the_same_seed_makes_the_same_landscape_and_another_seed_another() {
        let settings = Preset::MountainRange.settings();
        let a = make(&settings, 65);
        assert_eq!(a, make(&settings, 65));
        let other = GeneratorSettings {
            seed: 2,
            ..settings
        };
        assert_ne!(a, make(&other, 65));
        assert!(a.iter().all(|h| (0.0..=1.0).contains(h)));
    }

    #[test]
    fn a_finer_grid_makes_the_same_landscape() {
        let settings = Preset::RollingHills.settings();
        let coarse = make(&settings, 33);
        let fine = make(&settings, 65);
        // Every coarse sample is every other fine one.
        for z in 0..33 {
            for x in 0..33 {
                assert_eq!(coarse[z * 33 + x], fine[z * 2 * 65 + x * 2]);
            }
        }
    }

    #[test]
    fn every_preset_makes_land_of_its_own_kind() {
        let range = |heights: &[f32]| {
            let low = heights.iter().copied().fold(f32::MAX, f32::min);
            let high = heights.iter().copied().fold(f32::MIN, f32::max);
            high - low
        };
        let flat = make(&Preset::Flatlands.settings(), 65);
        let mountains = make(&Preset::MountainRange.settings(), 65);
        assert!(range(&flat) < 0.1, "{}", range(&flat));
        assert!(range(&mountains) > 0.3, "{}", range(&mountains));
        for preset in Preset::ALL {
            let heights = make(&preset.settings(), 33);
            assert!(heights.iter().all(|h| h.is_finite()), "{preset:?}");
        }
        // An island's edges sink below its middle.
        let island = make(&Preset::Island.settings(), 65);
        let edge = island[0].max(island[64]).max(island[64 * 65]);
        let middle = island[32 * 65 + 32];
        assert!(edge < middle, "{edge} {middle}");
    }

    #[test]
    fn settings_round_trip_through_json_and_bad_ones_are_refused() {
        let settings = Preset::DesertDunes.settings();
        let text = settings.to_json();
        assert_eq!(GeneratorSettings::from_json(&text).unwrap(), settings);
        // Older saves without newer fields still load.
        let old = GeneratorSettings::from_json(r#"{"seed": 9}"#).unwrap();
        assert_eq!(old.seed, 9);
        assert_eq!(old.algorithm, "fractal");

        let control = JobControl::new();
        let unknown = GeneratorSettings {
            algorithm: "voxels".to_owned(),
            ..GeneratorSettings::default()
        };
        assert!(matches!(
            generate(&unknown, target(33), &control),
            Err(GenerateError::UnknownAlgorithm(_))
        ));
        let newer = GeneratorSettings {
            version: GENERATOR_VERSION + 1,
            ..GeneratorSettings::default()
        };
        assert!(matches!(
            generate(&newer, target(33), &control),
            Err(GenerateError::TooNew { .. })
        ));
        assert!(generate(&GeneratorSettings::default(), target(3), &control).is_err());
        let nan = GeneratorSettings {
            amplitude: f32::NAN,
            ..GeneratorSettings::default()
        };
        assert!(generate(&nan, target(33), &control).is_err());
    }

    #[test]
    fn cancelling_stops_generation() {
        let control = JobControl::new();
        control.cancel();
        assert_eq!(
            generate(&GeneratorSettings::default(), target(65), &control),
            Ok(None)
        );
    }
}
