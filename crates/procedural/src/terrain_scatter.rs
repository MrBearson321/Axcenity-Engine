//! What a Terrain's ScatterRules place (procedural worlds PW1): plants,
//! props and models spread over the ground by height, slope, material
//! layer, clumping noise and water, each rule keeping clear of what the
//! rules before it placed.
//!
//! Placement is a **hash grid**. A rule cuts the terrain (from its corner,
//! in studs) into cells `Spacing` studs across. Each cell has one candidate
//! spot, and whether it's used, where exactly, which model, its turn and
//! its scale come from the rule's seed and the cell's coordinates only
//! ([`Rng`], drawn in that order). So the result in one chunk never
//! depends on another chunk's result, the same rule gives the same layout
//! on every device, and "is anything of an earlier rule within
//! `AvoidRadius`?" means looking at that rule's few cells nearby.
//!
//! To answer that near a chunk's edge, the earlier rules are worked out a
//! little past it: rule *k* over the chunk grown by the `AvoidRadius` of
//! every rule after it ([`margin`]).

use std::collections::HashMap;

use axcenity_core::{
    ClassName, FoliageInstance, FoliagePlant, FoliageProp, FoliageSource, Instance, InstanceId,
    MAX_TERRAIN_LAYERS, TerrainData, Value, World,
};
use axcenity_terrain::Placement;
use axcenity_terrain::materials::{LayerRule, MaterialRules, sample_weights};
use axcenity_terrain::noise::{Noise, smoothstep};
use glam::Vec2;

use crate::rng::Rng;

/// The rules one terrain reads, at most (later ones are left out).
pub const MAX_RULES: usize = 16;
/// The closest a rule's cells can be, in studs.
pub const MIN_SPACING: f32 = 1.0;
/// The farthest a rule keeps from earlier rules' spots, in studs.
pub const MAX_AVOID_RADIUS: f32 = 32.0;
/// What one chunk can hold, at most, over all its rules.
pub const MAX_PER_CHUNK: usize = 4096;
/// How far a candidate moves from its cell's middle, as a share of the
/// cell: below 1, so two spots are always at least a fifth of `Spacing`
/// apart and the layout looks even rather than clustered.
const JITTER: f32 = 0.8;

/// One ScatterRule's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub sources: Vec<FoliageSource>,
    pub seed: u64,
    pub spacing: f32,
    pub density: f32,
    /// The height and slope windows, as a TerrainLayer's rule (strength 1,
    /// no noise).
    pub window: LayerRule,
    /// The TerrainLayer (by its place among the terrain's layers) it grows
    /// on, and the least of it the ground must have.
    pub layer: Option<usize>,
    pub layer_threshold: f32,
    pub clumping: f32,
    pub clump_size: f32,
    pub avoid_radius: f32,
    pub underwater: bool,
    pub rotation: Vec2,
    pub scale: Vec2,
    pub collide: bool,
}

impl Rule {
    /// A ScatterRule's settings, or `None` when it's switched off, places
    /// nothing (no usable `Models`, or a `Density` of 0), or names a
    /// `Layer` the terrain doesn't have. `layers` are the names of the
    /// terrain's TerrainLayers, in order.
    pub fn of(instance: &Instance, layers: &[String]) -> Option<Rule> {
        if instance.class() != ClassName::ScatterRule
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
        let pair = |name, default: Vec2| {
            instance
                .property(name)
                .and_then(Value::as_vector2)
                .map(|v| Vec2::new(v.x, v.y))
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        let text = |name| {
            instance
                .property(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let models = text("Models");
        let sources: Vec<FoliageSource> = models.split(';').filter_map(source).collect();
        let density = number("Density", 1.0).max(0.0);
        if sources.is_empty() || density <= 0.0 {
            return None;
        }
        let layer_name = text("Layer");
        let layer = if layer_name.is_empty() {
            None
        } else {
            Some(layers.iter().position(|name| *name == layer_name)?)
        };
        let seed = number("Seed", 1.0) as i64 as u64;
        Some(Rule {
            // Two rules with the same seed but different models don't
            // share their spots.
            seed: seed ^ fnv(models.as_bytes()),
            sources,
            spacing: number("Spacing", 8.0).max(MIN_SPACING),
            density,
            window: LayerRule {
                min_height: number("MinHeight", 0.0),
                max_height: number("MaxHeight", 10000.0),
                height_fade: number("HeightFade", 4.0).max(0.0),
                min_slope: number("MinSlope", 0.0),
                max_slope: number("MaxSlope", 35.0),
                slope_fade: number("SlopeFade", 5.0).max(0.0),
                strength: 1.0,
                noise: 0.0,
                noise_scale: 1.0,
            },
            layer,
            layer_threshold: number("LayerThreshold", 0.5).clamp(0.0, 1.0),
            clumping: number("Clumping", 0.0).clamp(0.0, 1.0),
            clump_size: number("ClumpSize", 48.0).max(1.0),
            avoid_radius: number("AvoidRadius", 0.0).clamp(0.0, MAX_AVOID_RADIUS),
            underwater: instance.property("Underwater") == Some(&Value::Bool(true)),
            rotation: pair("RotationRange", Vec2::new(0.0, 360.0)),
            scale: pair("ScaleRange", Vec2::new(0.85, 1.15)),
            collide: instance.property("CanCollide") == Some(&Value::Bool(true)),
        })
    }
}

/// One entry of a rule's `Models`: `plant:<name>` and `prop:<name>` for the
/// built-in ones, anything else a model file. `None` for an empty entry or
/// an unknown built-in.
pub fn source(entry: &str) -> Option<FoliageSource> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    if let Some(name) = entry.strip_prefix("plant:") {
        return FoliagePlant::from_name(name.trim()).map(FoliageSource::Plant);
    }
    if let Some(name) = entry.strip_prefix("prop:") {
        return FoliageProp::from_name(name.trim()).map(FoliageSource::Prop);
    }
    Some(FoliageSource::Model(entry.to_owned()))
}

/// How a source is written in a rule's `Models` (the reverse of [`source`]).
pub fn entry(source: &FoliageSource) -> String {
    match source {
        FoliageSource::Plant(plant) => format!("plant:{}", plant.label()),
        FoliageSource::Prop(prop) => format!("prop:{}", prop.label()),
        FoliageSource::Model(path) => path.clone(),
    }
}

/// The ScatterRules a terrain uses: its first [`MAX_RULES`] ScatterRule
/// children that are on and place something, in order.
pub fn rules_of(world: &World, terrain: InstanceId) -> Vec<Rule> {
    let layers: Vec<String> = axcenity_terrain::materials::layers(world, terrain)
        .into_iter()
        .filter_map(|layer| world.get(layer).map(|i| i.name().to_owned()))
        .collect();
    world
        .get(terrain)
        .map(Instance::children)
        .unwrap_or_default()
        .iter()
        .filter_map(|&child| world.get(child))
        .filter(|child| child.class() == ClassName::ScatterRule)
        .filter_map(|child| Rule::of(child, &layers))
        .take(MAX_RULES)
        .collect()
}

/// What the rules stand on: the terrain, where it is, its material rules
/// (for `Layer`), and its water level (world Y), if it has water.
pub struct Ground<'a> {
    pub data: &'a TerrainData,
    pub placement: Placement,
    pub materials: &'a MaterialRules,
    pub water: Option<f32>,
}

/// Studs past a chunk's edge that rules have to be worked out to, so the
/// rules after them can keep clear near the edge: every rule's
/// `AvoidRadius` but the first's added up.
pub fn margin(rules: &[Rule]) -> f32 {
    rules.iter().skip(1).map(|rule| rule.avoid_radius).sum()
}

/// Everything `rules` place in chunk (`cx`, `cz`) of `ground`, rule by
/// rule, cell by cell (so always in the same order).
pub fn chunk(rules: &[Rule], ground: &Ground, cx: usize, cz: usize) -> Vec<FoliageInstance> {
    let data = ground.data;
    let (size_x, size_z) = data.size();
    let rect = data.chunk_rect(cx, cz);
    let (chunks_x, chunks_z) = data.chunk_counts();
    let cell = ground.placement.cell_size(data);
    // The chunk's own samples: up to the next chunk's first, or the edge.
    let own_min = Vec2::new(rect.x0 as f32 * cell.x, rect.z0 as f32 * cell.y);
    let end = |chunk: usize, chunks: usize, first: usize, size: usize| {
        if chunk + 1 == chunks {
            size as f32
        } else {
            first as f32
        }
    };
    let own_max = Vec2::new(
        end(cx, chunks_x, rect.x1 - 1, size_x) * cell.x,
        end(cz, chunks_z, rect.z1 - 1, size_z) * cell.y,
    );
    let mut placed: Vec<Spots> = Vec::with_capacity(rules.len());
    let mut out = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        let grow = Vec2::splat(margin(&rules[index..]));
        let spots = Spots::of(rule, ground, own_min - grow, own_max + grow, &placed);
        for spot in &spots.spots {
            let at = spot.at;
            if at.x >= own_min.x && at.y >= own_min.y && at.x < own_max.x && at.y < own_max.y {
                if out.len() >= MAX_PER_CHUNK {
                    return out;
                }
                out.push(FoliageInstance {
                    source: rule.sources[spot.source].clone(),
                    x: at.x / cell.x,
                    z: at.y / cell.y,
                    yaw: spot.yaw,
                    scale: spot.scale,
                    collide: rule.collide,
                });
            }
        }
        placed.push(spots);
    }
    out
}

/// One used candidate: where (studs from the terrain's corner), which of
/// the rule's sources, its turn (radians) and its scale.
#[derive(Clone, Copy, Debug)]
struct Spot {
    at: Vec2,
    source: usize,
    yaw: f32,
    scale: f32,
}

/// A rule's used spots over a box, findable by cell.
struct Spots {
    spacing: f32,
    /// The first cell (X, Z) and how many cells across.
    first: (i64, i64),
    width: i64,
    depth: i64,
    /// Per cell, row by row: its spot's index in `spots`.
    cells: Vec<Option<u32>>,
    spots: Vec<Spot>,
}

impl Spots {
    /// The spots `rule` uses with candidates between `min` and `max`
    /// (studs from the terrain's corner), keeping `AvoidRadius` from the
    /// spots in `earlier`.
    fn of(rule: &Rule, ground: &Ground, min: Vec2, max: Vec2, earlier: &[Spots]) -> Spots {
        let s = rule.spacing;
        let first = ((min.x / s).floor() as i64, (min.y / s).floor() as i64);
        let last = ((max.x / s).floor() as i64, (max.y / s).floor() as i64);
        let width = last.0 - first.0 + 1;
        let depth = last.1 - first.1 + 1;
        let mut spots = Spots {
            spacing: s,
            first,
            width,
            depth,
            cells: vec![None; (width * depth).max(0) as usize],
            spots: Vec::new(),
        };
        let clumps = Noise::new(rule.seed).offset(0xC1u64);
        let base = (rule.density * s * s / 100.0).min(1.0);
        for j in first.1..=last.1 {
            for i in first.0..=last.0 {
                let mut rng = Rng::new(rule.seed, cell_index(i, j));
                let at = Vec2::new(
                    (i as f32 + 0.5 + (rng.unit() - 0.5) * JITTER) * s,
                    (j as f32 + 0.5 + (rng.unit() - 0.5) * JITTER) * s,
                );
                let roll = rng.unit();
                if at.x < min.x || at.y < min.y || at.x >= max.x || at.y >= max.y {
                    continue;
                }
                let chance = base * suitability(rule, ground, at, &clumps);
                if roll >= chance {
                    continue;
                }
                if rule.avoid_radius > 0.0
                    && earlier
                        .iter()
                        .any(|other| other.any_within(at, rule.avoid_radius))
                {
                    continue;
                }
                let source = rng.below(rule.sources.len());
                let yaw = rng.range(rule.rotation.x, rule.rotation.y).to_radians();
                let scale = rng.range(rule.scale.x, rule.scale.y).max(0.01);
                let cell = ((j - first.1) * width + (i - first.0)) as usize;
                spots.cells[cell] = Some(spots.spots.len() as u32);
                spots.spots.push(Spot {
                    at,
                    source,
                    yaw,
                    scale,
                });
            }
        }
        spots
    }

    /// Whether any spot is within `radius` studs of `at`.
    fn any_within(&self, at: Vec2, radius: f32) -> bool {
        let s = self.spacing;
        let low = (
            ((at.x - radius) / s).floor() as i64,
            ((at.y - radius) / s).floor() as i64,
        );
        let high = (
            ((at.x + radius) / s).floor() as i64,
            ((at.y + radius) / s).floor() as i64,
        );
        for j in low.1.max(self.first.1)..=high.1.min(self.first.1 + self.depth - 1) {
            for i in low.0.max(self.first.0)..=high.0.min(self.first.0 + self.width - 1) {
                let cell = ((j - self.first.1) * self.width + (i - self.first.0)) as usize;
                if let Some(index) = self.cells[cell]
                    && self.spots[index as usize].at.distance_squared(at) < radius * radius
                {
                    return true;
                }
            }
        }
        false
    }
}

/// 0 to 1: how well `rule` likes the ground at `at` (studs from the
/// terrain's corner): 0 off the terrain, under water it may not stand in,
/// outside its height and slope windows, or off its layer; less away from
/// its clumps.
fn suitability(rule: &Rule, ground: &Ground, at: Vec2, clumps: &Noise) -> f32 {
    let data = ground.data;
    let placement = &ground.placement;
    let cell = placement.cell_size(data);
    let (size_x, size_z) = data.size();
    let grid = at / cell;
    if grid.x < 0.0 || grid.y < 0.0 || grid.x > (size_x - 1) as f32 || grid.y > (size_z - 1) as f32
    {
        return 0.0;
    }
    let height = data.height_at(grid.x, grid.y);
    if let Some(level) = ground.water
        && !rule.underwater
        && placement.world_y(height) < level
    {
        return 0.0;
    }
    let (nx, nz) = (
        (grid.x.round() as usize).min(size_x - 1),
        (grid.y.round() as usize).min(size_z - 1),
    );
    let slope = placement.slope_degrees(data, nx, nz);
    let mut fit = window(&rule.window, height * placement.size.y, slope);
    if fit <= 0.0 {
        return 0.0;
    }
    if let Some(layer) = rule.layer {
        if layer >= MAX_TERRAIN_LAYERS {
            return 0.0;
        }
        let weights = sample_weights(data, placement, ground.materials, nx, nz);
        if weights[layer] < rule.layer_threshold {
            return 0.0;
        }
    }
    if rule.clumping > 0.0 {
        let n = clumps.fbm(at.x / rule.clump_size, at.y / rule.clump_size, 3, 0.5, 2.0);
        let inside = smoothstep(-0.15, 0.25, n);
        fit *= 1.0 - rule.clumping * (1.0 - inside);
    }
    fit
}

/// 0 to 1: how far inside its height and slope windows a spot is. Unlike
/// a TerrainLayer's rule, an edge at the very bottom (0 studs, flat) or the
/// very top (90 degrees, or no top) is open, so a rule from 0 degrees
/// covers flat ground fully.
fn window(window: &LayerRule, height: f32, slope: f32) -> f32 {
    let edge = |value: f32, low: f32, high: f32, fade: f32, bottom: f32, top: f32| {
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
    };
    edge(
        height,
        window.min_height,
        window.max_height,
        window.height_fade,
        0.0,
        f32::MAX,
    ) * edge(
        slope,
        window.min_slope,
        window.max_slope,
        window.slope_fade,
        0.0,
        90.0,
    )
}

/// A cell's number for its generator: both coordinates, mixed by `Rng`.
fn cell_index(i: i64, j: i64) -> u64 {
    (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).rotate_left(32)
}

/// The 64-bit FNV-1a hash, the same in every build.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Keeps every terrain's generated foliage up to date. One per world, kept
/// by [`Generators`](crate::Generators).
#[derive(Default)]
pub struct TerrainScatter {
    /// Per terrain: a fingerprint of everything its foliage was last
    /// checked against, so an unchanged terrain costs one pass over its
    /// chunk versions.
    seen: HashMap<InstanceId, u64>,
}

impl TerrainScatter {
    /// Makes again the generated foliage of every terrain chunk whose
    /// inputs changed (its own and nearby heights and paint, the terrain's
    /// rules, layers, place and water), and clears it from terrains
    /// without rules. Returns how many chunks were made. Nothing it does
    /// is an edit: the result isn't saved, and every device makes the same.
    pub fn update(&mut self, world: &mut World) -> usize {
        let terrains = world.all_below(world.root(), |class| class == ClassName::Terrain);
        self.seen.retain(|id, _| terrains.contains(id));
        let mut made = 0;
        for terrain in terrains {
            made += self.update_terrain(world, terrain);
        }
        made
    }

    fn update_terrain(&mut self, world: &mut World, terrain: InstanceId) -> usize {
        let Some(placement) = world.get(terrain).and_then(Placement::of) else {
            return 0;
        };
        let Some(data) = world.terrain(terrain) else {
            return 0;
        };
        let rules = rules_of(world, terrain);
        let (chunks_x, chunks_z) = data.chunk_counts();
        if rules.is_empty() {
            self.seen.remove(&terrain);
            let any =
                (0..chunks_z).any(|cz| (0..chunks_x).any(|cx| data.generated_key(cx, cz) != 0));
            if any && let Some(data) = world.terrain_mut(terrain) {
                data.clear_generated();
            }
            return 0;
        }
        let materials = MaterialRules::of(world, terrain);
        let water = world
            .get(terrain)
            .filter(|i| i.property("Water") == Some(&Value::Bool(true)))
            .and_then(|i| i.property("WaterLevel"))
            .and_then(Value::as_number)
            .map(|level| level as f32);
        let signature = {
            let mut text = format!("{rules:?}|{placement:?}|{water:?}|{:?}", data.size());
            if rules.iter().any(|rule| rule.layer.is_some()) {
                text.push_str(&format!("|{}", materials.key()));
            }
            fnv(text.as_bytes())
        };
        let mut fingerprint = mix_in(signature, data.generated_version());
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                fingerprint = mix_in(fingerprint, data.chunk_height_version(cx, cz));
                fingerprint = mix_in(fingerprint, data.chunk_paint_version(cx, cz));
            }
        }
        if self.seen.get(&terrain) == Some(&fingerprint) {
            return 0;
        }
        // The chunks around each one whose ground its rules look at.
        let cell = placement.cell_size(data);
        let chunk_studs = CHUNK_STUDS_CELLS * cell.x.min(cell.y);
        let ring = (margin(&rules) / chunk_studs.max(f32::EPSILON)).ceil() as usize;
        let mut dirty = Vec::new();
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                let mut key = signature;
                for z in cz.saturating_sub(ring)..=(cz + ring).min(chunks_z - 1) {
                    for x in cx.saturating_sub(ring)..=(cx + ring).min(chunks_x - 1) {
                        key = mix_in(key, data.chunk_height_version(x, z));
                        key = mix_in(key, data.chunk_paint_version(x, z));
                    }
                }
                let key = key.max(1);
                if data.generated_key(cx, cz) != key {
                    dirty.push((cx, cz, key));
                }
            }
        }
        let count = dirty.len();
        if !dirty.is_empty() {
            let ground = Ground {
                data,
                placement,
                materials: &materials,
                water,
            };
            let results = make_chunks(&rules, &ground, &dirty);
            if let Some(data) = world.terrain_mut(terrain) {
                for ((cx, cz, key), instances) in dirty.into_iter().zip(results) {
                    data.set_generated(cx, cz, key, instances);
                }
            }
        }
        // Setting chunks moved the generated version on: remember the new one.
        if let Some(data) = world.terrain(terrain) {
            let mut fingerprint = mix_in(signature, data.generated_version());
            for cz in 0..chunks_z {
                for cx in 0..chunks_x {
                    fingerprint = mix_in(fingerprint, data.chunk_height_version(cx, cz));
                    fingerprint = mix_in(fingerprint, data.chunk_paint_version(cx, cz));
                }
            }
            self.seen.insert(terrain, fingerprint);
        }
        count
    }
}

/// `hash` with `value` mixed in (FNV-1a over its bytes).
fn mix_in(hash: u64, value: u64) -> u64 {
    value.to_le_bytes().iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Cells across a chunk (`core::CHUNK_CELLS`), as studs per cell scale it.
const CHUNK_STUDS_CELLS: f32 = axcenity_core::CHUNK_CELLS as f32;

/// Each chunk's foliage, in the order asked, on several threads when
/// there are many (each chunk's result is its own, so the order of work
/// doesn't matter).
fn make_chunks(
    rules: &[Rule],
    ground: &Ground,
    chunks: &[(usize, usize, u64)],
) -> Vec<Vec<FoliageInstance>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(8);
    if chunks.len() < 8 || threads < 2 {
        return chunks
            .iter()
            .map(|&(cx, cz, _)| chunk(rules, ground, cx, cz))
            .collect();
    }
    let per = chunks.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = chunks
            .chunks(per)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|&(cx, cz, _)| chunk(rules, ground, cx, cz))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a scatter worker doesn't panic"))
            .collect()
    })
}
