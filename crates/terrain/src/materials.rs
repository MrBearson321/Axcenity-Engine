//! Which material layers cover the ground where, from the layers' automatic
//! rules and the hand-painted weights.
//!
//! Three things are kept apart, so changing the rules never loses paint:
//!
//! - **Automatic weights** come from each TerrainLayer's rule ([`LayerRule`]):
//!   the first layer covers everything, and each later layer whose rule holds
//!   covers the layers before it (so later layers win), with soft edges and
//!   noise to break them up. They're worked out again whenever the heights
//!   or the rules change, and never saved.
//! - **Paint** is saved with the terrain (`TerrainData::paint`): up to 255
//!   in all per sample. Whatever paint doesn't claim is left to the rules.
//! - **The final weights** ([`blend`]) mix the two and add up to 1. They're
//!   what the renderer draws.
//!
//! A terrain's Biomes ([`crate::biomes`]) lay their ground layers down
//! first, under every layer's own rule: the first layer covers whatever no
//! biome does.

use std::hash::{DefaultHasher, Hash, Hasher};

use axcenity_core::{
    ClassName, InstanceId, MAX_TERRAIN_LAYERS, PaintWeights, TerrainData, Value, World,
};
use glam::Vec2;

use crate::Placement;
use crate::biomes::{self, BiomeRule, MAX_BIOMES, auto_biome_weights, blend_biomes};
use crate::noise::{Noise, smoothstep};

/// One layer's automatic rule: "where the height is between these and the
/// slope between those, cover the layers before with this one".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerRule {
    /// Studs above the terrain's bottom.
    pub min_height: f32,
    pub max_height: f32,
    /// Studs over which each height edge fades.
    pub height_fade: f32,
    /// Degrees from flat.
    pub min_slope: f32,
    pub max_slope: f32,
    pub slope_fade: f32,
    /// 0 to 1: how fully it covers.
    pub strength: f32,
    /// 0 to 1: how much noise moves its edges.
    pub noise: f32,
    /// Studs across one bump of that noise.
    pub noise_scale: f32,
}

impl Default for LayerRule {
    fn default() -> Self {
        Self {
            min_height: 0.0,
            max_height: 10000.0,
            height_fade: 4.0,
            min_slope: 0.0,
            max_slope: 90.0,
            slope_fade: 5.0,
            strength: 1.0,
            noise: 0.3,
            noise_scale: 24.0,
        }
    }
}

impl LayerRule {
    /// A TerrainLayer's rule, or `None` if its `AutoEnabled` is off.
    pub fn of(layer: &axcenity_core::Instance) -> Option<LayerRule> {
        if layer.property("AutoEnabled").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let number = |name, default: f32| {
            layer
                .property(name)
                .and_then(Value::as_number)
                .map_or(default, |n| n as f32)
        };
        let defaults = LayerRule::default();
        Some(LayerRule {
            min_height: number("MinHeight", defaults.min_height),
            max_height: number("MaxHeight", defaults.max_height),
            height_fade: number("HeightFade", defaults.height_fade).max(0.0),
            min_slope: number("MinSlope", defaults.min_slope),
            max_slope: number("MaxSlope", defaults.max_slope),
            slope_fade: number("SlopeFade", defaults.slope_fade).max(0.0),
            strength: number("RuleStrength", defaults.strength).clamp(0.0, 1.0),
            noise: number("RuleNoise", defaults.noise).clamp(0.0, 1.0),
            noise_scale: number("NoiseScale", defaults.noise_scale).max(0.1),
        })
    }

    /// How much it covers at `height` studs and `slope` degrees, before
    /// noise: 0 to `strength`.
    pub fn coverage(&self, height: f32, slope: f32) -> f32 {
        let window = |value: f32, low: f32, high: f32, fade: f32| {
            let half = fade / 2.0;
            smoothstep(low - half, low + half, value)
                * (1.0 - smoothstep(high - half, high + half, value))
        };
        let by_height = window(height, self.min_height, self.max_height, self.height_fade);
        let by_slope = window(slope, self.min_slope, self.max_slope, self.slope_fade);
        by_height * by_slope * self.strength
    }
}

/// The TerrainLayers a terrain blends: its first [`MAX_TERRAIN_LAYERS`]
/// TerrainLayer children, in order.
pub fn layers(world: &World, terrain: InstanceId) -> Vec<InstanceId> {
    world
        .get(terrain)
        .map(|instance| instance.children())
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|&child| {
            world
                .get(child)
                .is_some_and(|instance| instance.class() == ClassName::TerrainLayer)
        })
        .take(MAX_TERRAIN_LAYERS)
        .collect()
}

/// Every layer's rule (`None` where it has none), and every biome's
/// (`None` where it's switched off).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialRules {
    pub rules: Vec<Option<LayerRule>>,
    pub biomes: Vec<Option<BiomeRule>>,
}

impl MaterialRules {
    /// The rules of `terrain`'s layers and biomes.
    pub fn of(world: &World, terrain: InstanceId) -> Self {
        let layers = layers(world, terrain);
        let names: Vec<String> = layers
            .iter()
            .map(|&layer| {
                world
                    .get(layer)
                    .map_or_else(String::new, |i| i.name().to_owned())
            })
            .collect();
        let rules = layers
            .into_iter()
            .map(|layer| world.get(layer).and_then(LayerRule::of))
            .collect();
        let biomes = biomes::biomes(world, terrain)
            .into_iter()
            .map(|biome| world.get(biome).and_then(|i| BiomeRule::of(i, &names)))
            .collect();
        Self { rules, biomes }
    }

    pub fn layer_count(&self) -> usize {
        self.rules.len()
    }

    /// Changes whenever a rule (or the number of layers) does.
    pub fn key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.rules.len().hash(&mut hasher);
        for rule in &self.rules {
            match rule {
                None => 0u8.hash(&mut hasher),
                Some(rule) => {
                    1u8.hash(&mut hasher);
                    for value in [
                        rule.min_height,
                        rule.max_height,
                        rule.height_fade,
                        rule.min_slope,
                        rule.max_slope,
                        rule.slope_fade,
                        rule.strength,
                        rule.noise,
                        rule.noise_scale,
                    ] {
                        value.to_bits().hash(&mut hasher);
                    }
                }
            }
        }
        self.biomes.len().hash(&mut hasher);
        for biome in &self.biomes {
            match biome {
                None => 0u8.hash(&mut hasher),
                Some(biome) => {
                    1u8.hash(&mut hasher);
                    biome.hash_into(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    /// Each biome's automatic weight at a point (see [`Self::auto_weights`]).
    pub fn auto_biome_weights(&self, height: f32, slope: f32, at: Vec2) -> [f32; MAX_BIOMES] {
        auto_biome_weights(&self.biomes, height, slope, at)
    }

    /// The automatic weights at a point `height` studs above the terrain's
    /// bottom, `slope` degrees steep, `at` studs (X, Z) from the terrain's
    /// corner (so moving the terrain doesn't move the noise). The first
    /// layer is underneath everything.
    pub fn auto_weights(&self, height: f32, slope: f32, at: Vec2) -> [f32; MAX_TERRAIN_LAYERS] {
        let mut weights = [0.0; MAX_TERRAIN_LAYERS];
        if self.rules.is_empty() {
            return weights;
        }
        weights[0] = 1.0;
        // The biomes' ground first, where no layer's paint is.
        if self
            .biomes
            .iter()
            .any(|b| b.is_some_and(|b| b.ground.is_some()))
        {
            let covers = self.auto_biome_weights(height, slope, at);
            for (biome, cover) in self.biomes.iter().zip(covers) {
                if let Some(ground) = biome.and_then(|b| b.ground)
                    && ground < self.rules.len()
                    && cover > 0.0
                {
                    weights[0] -= cover;
                    weights[ground] += cover;
                }
            }
            weights[0] = weights[0].max(0.0);
        }
        for (index, rule) in self.rules.iter().enumerate().skip(1) {
            let Some(rule) = rule else {
                continue;
            };
            let (mut height, mut slope) = (height, slope);
            if rule.noise > 0.0 {
                let noise = Noise::new(0x5eed ^ index as u64);
                let n = noise.fbm(
                    at.x / rule.noise_scale,
                    at.y / rule.noise_scale,
                    3,
                    0.5,
                    2.0,
                );
                // Moves the edges by up to twice their fade (at least a little).
                height += n * rule.noise * 2.0 * rule.height_fade.max(2.0);
                slope += n * rule.noise * 2.0 * rule.slope_fade.max(3.0);
            }
            let cover = rule.coverage(height, slope);
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
}

/// The final weights: paint where it's painted, the automatic weights in
/// what it leaves, adding up to 1 over the first `layer_count` layers.
/// Paint for a layer that's gone is ignored. With no layers, all 0.
pub fn blend(
    auto: [f32; MAX_TERRAIN_LAYERS],
    paint: PaintWeights,
    layer_count: usize,
) -> [f32; MAX_TERRAIN_LAYERS] {
    let mut weights = [0.0; MAX_TERRAIN_LAYERS];
    if layer_count == 0 {
        return weights;
    }
    let painted: f32 = paint
        .iter()
        .map(|&w| w as f32 / 255.0)
        .sum::<f32>()
        .min(1.0);
    for layer in 0..layer_count.min(MAX_TERRAIN_LAYERS) {
        weights[layer] = auto[layer] * (1.0 - painted) + paint[layer] as f32 / 255.0;
    }
    let total: f32 = weights.iter().sum();
    if total <= 1e-6 {
        weights[0] = 1.0;
        return weights;
    }
    weights.map(|w| w / total)
}

/// The final weights at sample (`x`, `z`).
pub fn sample_weights(
    data: &TerrainData,
    placement: &Placement,
    rules: &MaterialRules,
    x: usize,
    z: usize,
) -> [f32; MAX_TERRAIN_LAYERS] {
    let height = data.height(x as isize, z as isize) * placement.size.y;
    let slope = placement.slope_degrees(data, x, z);
    let at = Vec2::new(x as f32, z as f32) * placement.cell_size(data);
    let auto = rules.auto_weights(height, slope, at);
    blend(auto, data.paint()[data.index(x, z)], rules.layer_count())
}

/// Each biome's final weight at sample (`x`, `z`), 0 to 1 (see
/// [`crate::biomes`]). All 0 with no biomes.
pub fn biome_weights(
    data: &TerrainData,
    placement: &Placement,
    rules: &MaterialRules,
    x: usize,
    z: usize,
) -> [f32; MAX_BIOMES] {
    if rules.biomes.is_empty() {
        return [0.0; MAX_BIOMES];
    }
    let height = data.height(x as isize, z as isize) * placement.size.y;
    let slope = placement.slope_degrees(data, x, z);
    let at = Vec2::new(x as f32, z as f32) * placement.cell_size(data);
    let auto = rules.auto_biome_weights(height, slope, at);
    blend_biomes(&rules.biomes, auto, data.paint()[data.index(x, z)])
}

/// The layer that covers the most at the sample nearest world (`x`, `z`),
/// for picking a layer from the ground. `None` off the terrain or with no layers.
pub fn dominant_layer(
    data: &TerrainData,
    placement: &Placement,
    rules: &MaterialRules,
    x: f32,
    z: f32,
) -> Option<usize> {
    if rules.layer_count() == 0 {
        return None;
    }
    let grid = placement.to_grid(data, x, z).round();
    let (size_x, size_z) = data.size();
    if grid.x < 0.0 || grid.y < 0.0 || grid.x >= size_x as f32 || grid.y >= size_z as f32 {
        return None;
    }
    let weights = sample_weights(data, placement, rules, grid.x as usize, grid.y as usize);
    weights
        .iter()
        .enumerate()
        .take(rules.layer_count())
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::Vector3;

    fn quiet(rule: LayerRule) -> Option<LayerRule> {
        Some(LayerRule { noise: 0.0, ..rule })
    }

    /// Grass underneath, rock on slopes over 35°, snow over 80 studs.
    fn alpine() -> MaterialRules {
        MaterialRules {
            biomes: Vec::new(),
            rules: vec![
                None,
                quiet(LayerRule {
                    min_slope: 35.0,
                    ..LayerRule::default()
                }),
                quiet(LayerRule {
                    min_height: 80.0,
                    ..LayerRule::default()
                }),
            ],
        }
    }

    #[test]
    fn later_layers_cover_earlier_ones_where_their_rules_hold() {
        let rules = alpine();
        let at = Vec2::ZERO;
        let grass = rules.auto_weights(10.0, 5.0, at);
        assert_eq!(&grass[..3], &[1.0, 0.0, 0.0]);
        let rock = rules.auto_weights(10.0, 60.0, at);
        assert_eq!(&rock[..3], &[0.0, 1.0, 0.0]);
        let snow = rules.auto_weights(120.0, 60.0, at);
        assert_eq!(&snow[..3], &[0.0, 0.0, 1.0]);
        // At the edge, half and half.
        let edge = rules.auto_weights(80.0, 5.0, at);
        assert!((edge[0] - 0.5).abs() < 1e-5 && (edge[2] - 0.5).abs() < 1e-5);
        for weights in [grass, rock, snow, edge] {
            assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn noise_moves_the_edges_the_same_way_every_time() {
        let mut rules = alpine();
        rules.rules[2] = Some(LayerRule {
            min_height: 80.0,
            noise: 1.0,
            ..LayerRule::default()
        });
        let a = rules.auto_weights(80.0, 5.0, Vec2::new(13.0, 7.0));
        let b = rules.auto_weights(80.0, 5.0, Vec2::new(13.0, 7.0));
        let c = rules.auto_weights(80.0, 5.0, Vec2::new(40.0, -21.0));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn paint_wins_where_painted_and_the_rules_fill_the_rest() {
        let auto = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        // Nothing painted: all automatic.
        assert_eq!(blend(auto, [0; 8], 3)[1], 1.0);
        // Fully painted grass: all grass, whatever the rules say.
        let painted = blend(auto, [255, 0, 0, 0, 0, 0, 0, 0], 3);
        assert_eq!(&painted[..2], &[1.0, 0.0]);
        // Half painted.
        let half = blend(auto, [0, 0, 51, 0, 0, 0, 0, 0], 3);
        assert!((half[1] - 0.8).abs() < 1e-5 && (half[2] - 0.2).abs() < 1e-5);
        // Paint for a layer that was removed is ignored, and the weights
        // still add up to 1.
        let gone = blend(auto, [0, 0, 0, 0, 0, 255, 0, 0], 3);
        assert!((gone.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(gone[5], 0.0);
        assert_eq!(blend(auto, [0; 8], 0), [0.0; 8]);
    }

    #[test]
    fn rules_come_from_the_terrains_layers_and_their_key_follows_them() {
        let mut world = World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        let grass = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(grass, Some(terrain)).unwrap();
        let rock = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(rock, Some(terrain)).unwrap();
        world
            .set_property(rock, "AutoEnabled", Value::Bool(true))
            .unwrap();
        world
            .set_property(rock, "MinSlope", Value::Number(40.0))
            .unwrap();
        let rules = MaterialRules::of(&world, terrain);
        assert_eq!(rules.layer_count(), 2);
        assert!(rules.rules[0].is_none());
        assert_eq!(rules.rules[1].unwrap().min_slope, 40.0);
        let key = rules.key();
        world
            .set_property(rock, "MinSlope", Value::Number(30.0))
            .unwrap();
        assert_ne!(MaterialRules::of(&world, terrain).key(), key);

        // Steep ground picks rock.
        let data = world.terrain(terrain).unwrap().clone();
        let mut data = (*data).clone();
        let all = data.bounds();
        let (size_x, _) = data.size();
        for (index, height) in data.heights_mut_in(all).iter_mut().enumerate() {
            *height = (index % size_x) as f32 / size_x as f32;
        }
        world
            .set_property(
                terrain,
                "Size",
                Value::Vector3(Vector3::new(256.0, 512.0, 256.0)),
            )
            .unwrap();
        let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
        let rules = MaterialRules::of(&world, terrain);
        assert_eq!(dominant_layer(&data, &placement, &rules, 0.0, 0.0), Some(1));
        assert_eq!(dominant_layer(&data, &placement, &rules, 9999.0, 0.0), None);
    }
}
