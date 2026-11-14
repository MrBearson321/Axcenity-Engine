//! Blend into Terrain (procedural worlds PW5): the ground under a model's
//! footprint brought to its bottom and painted, blending back into the
//! ground around it, so a house stands on a level patch instead of on
//! stilts or half buried.

use axcenity_core::{SampleRect, TerrainData};
use axcenity_terrain::Placement;
use axcenity_terrain::brush::{Operation, PaintMode, paint_sample};
use axcenity_terrain::noise::smoothstep;
use glam::Vec2;

use crate::obstacles::Obstacle;

/// Sets every sample of `data` under `parts` (seen from above) to world
/// height `height` and paints it with layer `layer` (its place among the
/// terrain's layers; `None` paints nothing), blending both back into the
/// ground over `falloff` studs around. Returns how many samples it changed.
pub fn blend_footprint(
    data: &mut TerrainData,
    placement: &Placement,
    parts: &[Obstacle],
    height: f32,
    falloff: f32,
    layer: Option<usize>,
) -> usize {
    if parts.is_empty() {
        return 0;
    }
    let falloff = falloff.max(0.0);
    let (min, max) = parts
        .iter()
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), part| {
            (min.min(part.min), max.max(part.max))
        });
    let (size_x, size_z) = data.size();
    let low = placement
        .to_grid(data, min.x - falloff, min.y - falloff)
        .floor();
    let high = placement
        .to_grid(data, max.x + falloff, max.y + falloff)
        .ceil();
    let clamp_x = |v: f32| (v.max(0.0) as usize).min(size_x);
    let clamp_z = |v: f32| (v.max(0.0) as usize).min(size_z);
    let rect = SampleRect::new(
        clamp_x(low.x),
        clamp_z(low.y),
        clamp_x(high.x + 1.0),
        clamp_z(high.y + 1.0),
    );
    if rect.is_empty() {
        return 0;
    }
    let target = placement.height_of(height);
    let paint = layer.map(|layer| Operation::Paint {
        layer,
        mode: PaintMode::Blend,
        locked: [false; axcenity_core::MAX_TERRAIN_LAYERS],
    });
    let mut set = Vec::new();
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let at = placement.to_world_xz(data, Vec2::new(x as f32, z as f32));
            let away = parts
                .iter()
                .map(|part| part.distance_from_above(at))
                .fold(f32::INFINITY, f32::min);
            if away > falloff || (away > 0.0 && falloff <= 0.0) {
                continue;
            }
            let amount = if away <= 0.0 {
                1.0
            } else {
                1.0 - smoothstep(0.0, falloff, away)
            };
            let index = z * size_x + x;
            let old = data.heights()[index];
            let weights = data.paint()[index];
            set.push((
                index,
                old + (target - old) * amount,
                paint.as_ref().map_or(weights, |operation| {
                    paint_sample(weights, operation, amount)
                }),
            ));
        }
    }
    if set.is_empty() {
        return 0;
    }
    let (heights, paints) = data.both_mut_in(rect);
    for &(index, height, weights) in &set {
        heights[index] = height;
        paints[index] = weights;
    }
    data.clamp_heights_in(rect);
    set.len()
}
