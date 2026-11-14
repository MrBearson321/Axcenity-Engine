//! Where a terrain's Biomes cover the ground (procedural worlds PW2).
//!
//! A Biome is an area with its own ScatterRules and a ground layer. Its
//! weight at a sample comes from two places, kept apart as material layers
//! keep theirs ([`crate::materials`]), so changing a biome's conditions
//! never loses paint:
//!
//! - **Automatic**: where its height, slope and moisture conditions hold
//!   (with soft, noisy edges), each later biome covering the ones before.
//!   Never saved.
//! - **Paint**: its ground layer's hand-painted weight. Painting a biome is
//!   painting its ground layer, so nothing new is saved, undone or sent.
//!
//! Whatever no biome covers belongs to none: rules outside every Biome
//! apply there (and everywhere else too).

use std::hash::{Hash, Hasher};

use axcenity_core::{
    ClassName, Instance, InstanceId, MAX_TERRAIN_LAYERS, PaintWeights, Value, World,
};
use glam::Vec2;

use crate::noise::{Noise, smoothstep};

/// The Biomes a terrain reads, at most (later ones are left out).
pub const MAX_BIOMES: usize = 8;

/// One Biome's automatic conditions and ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeRule {
    /// The TerrainLayer (by its place among the terrain's layers) covering
    /// its ground, whose paint is its paint.
    pub ground: Option<usize>,
    /// Whether the conditions below place it by themselves.
    pub auto: bool,
    /// Studs above the terrain's bottom.
    pub min_height: f32,
    pub max_height: f32,
    pub height_fade: f32,
    /// Degrees from flat.
    pub min_slope: f32,
    pub max_slope: f32,
    pub slope_fade: f32,
    /// 0 (dry) to 1 (wet).
    pub min_moisture: f32,
    pub max_moisture: f32,
    pub moisture_fade: f32,
    /// Studs across one patch of moisture.
    pub moisture_scale: f32,
    /// 0 to 1: how much noise moves its edges.
    pub edge_noise: f32,
    pub noise_scale: f32,
}

impl BiomeRule {
    /// A Biome's rule, or `None` when it's switched off. `layers` are the
    /// names of the terrain's TerrainLayers, in order.
    pub fn of(instance: &Instance, layers: &[String]) -> Option<BiomeRule> {
        if instance.class() != ClassName::Biome
            || instance.property("Enabled") != Some(&Value::Bool(true))
        {
            return None;
        }
        let number = |name, default: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
                .unwrap_or(default)
        };
        let ground_name = instance
            .property("GroundLayer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let ground = (!ground_name.is_empty())
            .then(|| layers.iter().position(|name| name == ground_name))
            .flatten()
            .filter(|&index| index < MAX_TERRAIN_LAYERS);
        Some(BiomeRule {
            ground,
            auto: instance.property("AutoEnabled") != Some(&Value::Bool(false)),
            min_height: number("MinHeight", 0.0),
            max_height: number("MaxHeight", 10000.0),
            height_fade: number("HeightFade", 8.0).max(0.0),
            min_slope: number("MinSlope", 0.0),
            max_slope: number("MaxSlope", 90.0),
            slope_fade: number("SlopeFade", 5.0).max(0.0),
            min_moisture: number("MinMoisture", 0.0),
            max_moisture: number("MaxMoisture", 1.0),
            moisture_fade: number("MoistureFade", 0.1).max(0.0),
            moisture_scale: number("MoistureScale", 400.0).max(1.0),
            edge_noise: number("EdgeNoise", 0.3).clamp(0.0, 1.0),
            noise_scale: number("NoiseScale", 48.0).max(0.1),
        })
    }

    /// Mixes every setting into `hasher`.
    pub(crate) fn hash_into(&self, hasher: &mut impl Hasher) {
        self.ground.hash(hasher);
        self.auto.hash(hasher);
        for value in [
            self.min_height,
            self.max_height,
            self.height_fade,
            self.min_slope,
            self.max_slope,
            self.slope_fade,
            self.min_moisture,
            self.max_moisture,
            self.moisture_fade,
            self.moisture_scale,
            self.edge_noise,
            self.noise_scale,
        ] {
            value.to_bits().hash(hasher);
        }
    }

    /// 0 to 1: how fully its conditions hold at `height` studs, `slope`
    /// degrees, `at` studs (X, Z) from the terrain's corner. An edge at the
    /// very bottom or top of a range (0 studs or no top, flat or 90°, dry
    /// or wet) is open, so a biome from 0 studs covers the lowest ground.
    pub fn coverage(&self, index: usize, height: f32, slope: f32, at: Vec2) -> f32 {
        if !self.auto {
            return 0.0;
        }
        let (mut height, mut slope, mut wet) = (height, slope, moisture(at, self.moisture_scale));
        if self.edge_noise > 0.0 {
            let noise = Noise::new(0xB10E ^ index as u64);
            let n = noise.fbm(
                at.x / self.noise_scale,
                at.y / self.noise_scale,
                3,
                0.5,
                2.0,
            );
            // Moves the edges by up to twice their fade (at least a little).
            height += n * self.edge_noise * 2.0 * self.height_fade.max(2.0);
            slope += n * self.edge_noise * 2.0 * self.slope_fade.max(3.0);
            wet += n * self.edge_noise * 2.0 * self.moisture_fade.max(0.03);
        }
        window(
            height,
            self.min_height,
            self.max_height,
            self.height_fade,
            0.0,
            f32::MAX,
        ) * window(
            slope,
            self.min_slope,
            self.max_slope,
            self.slope_fade,
            0.0,
            90.0,
        ) * window(
            wet,
            self.min_moisture,
            self.max_moisture,
            self.moisture_fade,
            0.0,
            1.0,
        )
    }
}

/// 0 (dry) to 1 (wet) at `at` studs from the terrain's corner: noise with
/// patches about `scale` studs across, the same for every biome (and every
/// terrain) at the same scale, so biomes with matching scales split the
/// same moisture between them.
pub fn moisture(at: Vec2, scale: f32) -> f32 {
    let noise = Noise::new(0x0003_0157_u64);
    let n = noise.fbm(at.x / scale, at.y / scale, 4, 0.5, 2.0);
    // fbm mostly stays within ±0.5: spread that over 0 to 1.
    smoothstep(-0.5, 0.5, n)
}

/// 1 inside `low`..`high`, 0 outside, faded over `fade` at each edge; an
/// edge at `bottom` or `top` doesn't fade.
fn window(value: f32, low: f32, high: f32, fade: f32, bottom: f32, top: f32) -> f32 {
    let half = fade / 2.0;
    let above = if low <= bottom {
        1.0
    } else {
        smoothstep(low - half, low + half, value)
    };
    let below = if high >= top {
        1.0
    } else {
        1.0 - smoothstep(high - half, high + half, value)
    };
    above * below
}

/// The Biomes a terrain reads: its first [`MAX_BIOMES`] Biome children, in
/// order (switched off ones too, so a biome's number doesn't change when
/// another is switched off).
pub fn biomes(world: &World, terrain: InstanceId) -> Vec<InstanceId> {
    world
        .get(terrain)
        .map(Instance::children)
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|&child| {
            world
                .get(child)
                .is_some_and(|i| i.class() == ClassName::Biome)
        })
        .take(MAX_BIOMES)
        .collect()
}

/// Each biome's automatic weight: later biomes cover earlier ones where
/// their conditions hold. They add up to at most 1; the rest is no biome.
pub fn auto_biome_weights(
    biomes: &[Option<BiomeRule>],
    height: f32,
    slope: f32,
    at: Vec2,
) -> [f32; MAX_BIOMES] {
    let mut weights = [0.0; MAX_BIOMES];
    for (index, biome) in biomes.iter().enumerate().take(MAX_BIOMES) {
        let Some(biome) = biome else {
            continue;
        };
        let cover = biome.coverage(index, height, slope, at);
        if cover <= 0.0 {
            continue;
        }
        for weight in &mut weights {
            *weight *= 1.0 - cover;
        }
        weights[index] += cover;
    }
    weights
}

/// The final biome weights: each biome's ground paint, and the automatic
/// weights in what the biomes' paint leaves. Each is 0 to 1; together at
/// most 1, unless two biomes share a ground layer (then both have its paint).
pub fn blend_biomes(
    biomes: &[Option<BiomeRule>],
    auto: [f32; MAX_BIOMES],
    paint: PaintWeights,
) -> [f32; MAX_BIOMES] {
    let painted = |biome: &Option<BiomeRule>| {
        biome
            .and_then(|b| b.ground)
            .map_or(0.0, |layer| f32::from(paint[layer]) / 255.0)
    };
    // Each painted layer counted once, however many biomes it grounds.
    let mut counted = [false; MAX_TERRAIN_LAYERS];
    let mut total = 0.0;
    for biome in biomes.iter().take(MAX_BIOMES).flatten() {
        if let Some(layer) = biome.ground
            && !counted[layer]
        {
            counted[layer] = true;
            total += f32::from(paint[layer]) / 255.0;
        }
    }
    let left = 1.0 - total.min(1.0);
    let mut weights = [0.0; MAX_BIOMES];
    for (index, biome) in biomes.iter().enumerate().take(MAX_BIOMES) {
        if biome.is_some() {
            weights[index] = (auto[index] * left + painted(biome)).min(1.0);
        }
    }
    weights
}

#[cfg(test)]
#[path = "biomes_tests.rs"]
mod tests;
