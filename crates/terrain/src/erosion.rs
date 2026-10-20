//! Erosion: wearing a landscape down the way nature does.
//!
//! - **Hydraulic** ([`hydraulic`]): raindrops fall at random places, run
//!   downhill picking up soil where they speed up and dropping it where they
//!   slow down or the ground rises, then evaporate. Cuts gullies into
//!   slopes and fills valley floors with sediment.
//! - **Thermal** ([`thermal`]): loose material on slopes steeper than the
//!   talus angle slides down until they aren't, leaving scree slopes at the
//!   foot of cliffs.
//!
//! Both work on a copy of the heights and give back new ones (or `None` if
//! cancelled), and give the same result for the same settings and seed.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::job::{JobControl, parallel_rows};
use crate::noise::Rng;

/// The grid's spacing and height range, so erosion works in real proportions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// Studs between samples along X and Z.
    pub cell: Vec2,
    /// Studs from height 0 to height 1.
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HydraulicSettings {
    /// How many raindrops fall.
    pub droplets: u32,
    pub seed: u64,
    /// 0 to 1: how quickly running water digs.
    pub erosion: f32,
    /// 0 to 1: how quickly it drops what it carries.
    pub deposition: f32,
    /// 0 to 0.5: how much of the water evaporates each step.
    pub evaporation: f32,
    /// How much soil water can carry, for its speed and amount.
    pub capacity: f32,
    /// 0 to 1: how much a drop keeps going its way rather than straight downhill.
    pub inertia: f32,
    /// Samples around a drop it digs from, so gullies are smooth.
    pub radius: u32,
    /// Most steps one drop runs.
    pub lifetime: u32,
    pub gravity: f32,
}

impl Default for HydraulicSettings {
    fn default() -> Self {
        Self {
            droplets: 50_000,
            seed: 1,
            erosion: 0.3,
            deposition: 0.3,
            evaporation: 0.02,
            capacity: 4.0,
            inertia: 0.05,
            radius: 3,
            lifetime: 30,
            gravity: 4.0,
        }
    }
}

impl HydraulicSettings {
    /// Raindrops for a grid of `samples`: about `per_sample` each.
    pub fn droplets_for(samples: usize, per_sample: f32) -> u32 {
        (samples as f32 * per_sample).clamp(1000.0, 5_000_000.0) as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThermalSettings {
    pub iterations: u32,
    /// Degrees: slopes steeper than this shed material.
    pub talus_angle: f32,
    /// 0 to 1: how much of the extra slides each iteration.
    pub strength: f32,
}

impl Default for ThermalSettings {
    fn default() -> Self {
        Self {
            iterations: 40,
            talus_angle: 35.0,
            strength: 0.5,
        }
    }
}

/// The smallest a drop's capacity gets, so it still digs a little on flat ground.
const MIN_CAPACITY: f32 = 0.01;

/// Hydraulic erosion of `heights` (0 to 1, `size_x` × `size_z`, row by row).
pub fn hydraulic(
    heights: &[f32],
    (size_x, size_z): (usize, usize),
    scale: Scale,
    settings: &HydraulicSettings,
    control: &JobControl,
) -> Option<Vec<f32>> {
    if size_x < 3 || size_z < 3 || heights.len() != size_x * size_z {
        return Some(heights.to_vec());
    }
    // Work in cells: a height of 1 is one cell's width, so slopes are real slopes.
    let cell = ((scale.cell.x + scale.cell.y) / 2.0).max(1e-4);
    let to_cells = scale.height / cell;
    let mut map: Vec<f32> = heights.iter().map(|h| h * to_cells).collect();
    let brush = erosion_brush(settings.radius.clamp(1, 8) as i32);
    let mut rng = Rng::new(settings.seed);
    let inertia = settings.inertia.clamp(0.0, 0.99);
    let erosion = settings.erosion.clamp(0.0, 1.0);
    let deposition = settings.deposition.clamp(0.0, 1.0);
    let evaporation = settings.evaporation.clamp(0.0, 0.5);
    let capacity_factor = settings.capacity.max(0.0);
    let gravity = settings.gravity.max(0.0);
    let (width, depth) = (size_x as f32, size_z as f32);

    for droplet in 0..settings.droplets {
        if droplet % 2048 == 0 {
            if control.cancelled() {
                return None;
            }
            control.set_progress(droplet as f32 / settings.droplets.max(1) as f32);
        }
        let mut position = Vec2::new(
            rng.next_f32() * (width - 1.0),
            rng.next_f32() * (depth - 1.0),
        );
        let mut direction = Vec2::ZERO;
        let (mut speed, mut water, mut sediment) = (1.0f32, 1.0f32, 0.0f32);
        for _ in 0..settings.lifetime.max(1) {
            let node = position.floor();
            let (nx, nz) = (node.x as usize, node.y as usize);
            let offset = position - node;
            let (height, gradient) = height_and_gradient(&map, size_x, position);
            direction = direction * inertia - gradient * (1.0 - inertia);
            let length = direction.length();
            if length < 1e-6 {
                break;
            }
            direction /= length;
            position += direction;
            let outside = position.x < 0.0
                || position.y < 0.0
                || position.x >= width - 1.0
                || position.y >= depth - 1.0;
            if outside {
                break;
            }
            let (new_height, _) = height_and_gradient(&map, size_x, position);
            let fall = new_height - height;
            let capacity = (-fall * speed * water * capacity_factor).max(MIN_CAPACITY);
            if sediment > capacity || fall > 0.0 {
                // Uphill: fill the dip behind it. Too full: drop some.
                let amount = if fall > 0.0 {
                    fall.min(sediment)
                } else {
                    (sediment - capacity) * deposition
                };
                sediment -= amount;
                let corners = [
                    (nx, nz, (1.0 - offset.x) * (1.0 - offset.y)),
                    (nx + 1, nz, offset.x * (1.0 - offset.y)),
                    (nx, nz + 1, (1.0 - offset.x) * offset.y),
                    (nx + 1, nz + 1, offset.x * offset.y),
                ];
                for (x, z, share) in corners {
                    map[z * size_x + x] += amount * share;
                }
            } else {
                // Dig from around where it was, never deeper than the fall.
                let amount = ((capacity - sediment) * erosion).min(-fall);
                for &(dx, dz, share) in &brush {
                    let x = nx as i32 + dx;
                    let z = nz as i32 + dz;
                    if x < 0 || z < 0 || x >= size_x as i32 || z >= size_z as i32 {
                        continue;
                    }
                    let index = z as usize * size_x + x as usize;
                    let dug = (amount * share).min(map[index].max(0.0));
                    map[index] -= dug;
                    sediment += dug;
                }
            }
            // Faster downhill (`fall` below 0), slower uphill.
            speed = (speed * speed - fall * gravity).max(0.0).sqrt().min(64.0);
            water *= 1.0 - evaporation;
        }
    }
    control.set_progress(1.0);
    Some(map.iter().map(|h| (h / to_cells).clamp(0.0, 1.0)).collect())
}

/// The height at `position` (in cells) blended from its four samples, and
/// which way is uphill.
fn height_and_gradient(map: &[f32], size_x: usize, position: Vec2) -> (f32, Vec2) {
    let (x, z) = (position.x as usize, position.y as usize);
    let (u, v) = (position.x - x as f32, position.y - z as f32);
    let at = |x: usize, z: usize| map[z * size_x + x];
    let (nw, ne, sw, se) = (at(x, z), at(x + 1, z), at(x, z + 1), at(x + 1, z + 1));
    let gradient = Vec2::new(
        (ne - nw) * (1.0 - v) + (se - sw) * v,
        (sw - nw) * (1.0 - u) + (se - ne) * u,
    );
    let height = nw * (1.0 - u) * (1.0 - v) + ne * u * (1.0 - v) + sw * (1.0 - u) * v + se * u * v;
    (height, gradient)
}

/// The cells within `radius` of a point, each with its share (closer is
/// more), adding up to 1.
fn erosion_brush(radius: i32) -> Vec<(i32, i32, f32)> {
    let mut cells = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let distance = ((dx * dx + dz * dz) as f32).sqrt();
            let weight = radius as f32 - distance;
            if weight > 0.0 {
                cells.push((dx, dz, weight));
            }
        }
    }
    let total: f32 = cells.iter().map(|c| c.2).sum();
    cells
        .into_iter()
        .map(|(x, z, w)| (x, z, w / total))
        .collect()
}

/// The eight neighbours, and how far each is in cells.
const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Thermal erosion of `heights` (0 to 1, `size_x` × `size_z`, row by row).
/// Each iteration works out every sample's slide first, then moves the
/// material, so the order samples are visited in doesn't matter (and rows
/// run in parallel).
pub fn thermal(
    heights: &[f32],
    (size_x, size_z): (usize, usize),
    scale: Scale,
    settings: &ThermalSettings,
    control: &JobControl,
) -> Option<Vec<f32>> {
    if heights.len() != size_x * size_z {
        return Some(heights.to_vec());
    }
    let mut map: Vec<f32> = heights.iter().map(|h| h * scale.height).collect();
    let talus = settings.talus_angle.clamp(0.0, 89.0).to_radians().tan();
    let strength = settings.strength.clamp(0.0, 1.0) * 0.5;
    let distances = NEIGHBOURS
        .map(|(dx, dz)| Vec2::new(dx as f32 * scale.cell.x, dz as f32 * scale.cell.y).length());
    let iterations = settings.iterations.max(1);
    for iteration in 0..iterations {
        let span = (
            iteration as f32 / iterations as f32,
            (iteration as f32 + 0.5) / iterations as f32,
        );
        // How much each sample sends to each neighbour.
        let outflow: Vec<[f32; 8]> = parallel_rows(size_z, control, span, |z| {
            (0..size_x)
                .map(|x| {
                    let here = map[z * size_x + x];
                    let mut excess = [0.0f32; 8];
                    let mut total = 0.0;
                    let mut most = 0.0f32;
                    for (k, &(dx, dz)) in NEIGHBOURS.iter().enumerate() {
                        let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                        if nx < 0 || nz < 0 || nx >= size_x as i32 || nz >= size_z as i32 {
                            continue;
                        }
                        let drop = here - map[nz as usize * size_x + nx as usize];
                        let over = drop - talus * distances[k];
                        if over > 0.0 {
                            excess[k] = over;
                            total += over;
                            most = most.max(over);
                        }
                    }
                    if total > 0.0 {
                        let moved = most * strength;
                        excess.map(|over| moved * over / total)
                    } else {
                        [0.0; 8]
                    }
                })
                .collect::<Vec<_>>()
        })?
        .concat();
        let rows = parallel_rows(size_z, control, (span.1, span.1), |z| {
            (0..size_x)
                .map(|x| {
                    let index = z * size_x + x;
                    let mut height = map[index] - outflow[index].iter().sum::<f32>();
                    for (k, &(dx, dz)) in NEIGHBOURS.iter().enumerate() {
                        let (nx, nz) = (x as i32 - dx, z as i32 - dz);
                        if nx < 0 || nz < 0 || nx >= size_x as i32 || nz >= size_z as i32 {
                            continue;
                        }
                        // The neighbour this way sends to us along direction k.
                        height += outflow[nz as usize * size_x + nx as usize][k];
                    }
                    height
                })
                .collect::<Vec<f32>>()
        })?;
        map = rows.concat();
    }
    control.set_progress(1.0);
    Some(
        map.iter()
            .map(|h| (h / scale.height).clamp(0.0, 1.0))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone 40 studs high in a 64 × 64-stud square, with 1-stud cells.
    fn cone() -> (Vec<f32>, (usize, usize), Scale) {
        let size = 65;
        let heights = (0..size * size)
            .map(|i| {
                let (x, z) = ((i % size) as f32 - 32.0, (i / size) as f32 - 32.0);
                (1.0 - (x * x + z * z).sqrt() / 32.0).max(0.0) * 0.4
            })
            .collect();
        let scale = Scale {
            cell: Vec2::ONE,
            height: 100.0,
        };
        (heights, (size, size), scale)
    }

    fn total(heights: &[f32]) -> f32 {
        heights.iter().sum()
    }

    #[test]
    fn hydraulic_erosion_changes_the_ground_the_same_way_each_time() {
        let (heights, size, scale) = cone();
        let settings = HydraulicSettings {
            droplets: 3000,
            ..HydraulicSettings::default()
        };
        let control = JobControl::new();
        let a = hydraulic(&heights, size, scale, &settings, &control).unwrap();
        let b = hydraulic(&heights, size, scale, &settings, &control).unwrap();
        assert_eq!(a, b);
        let changed = heights.iter().zip(&a).filter(|(x, y)| x != y).count();
        assert!(changed > 100, "{changed} samples changed");
        // The peak wears down.
        assert!(a[32 * 65 + 32] <= heights[32 * 65 + 32]);
        let other = HydraulicSettings {
            seed: 2,
            ..settings
        };
        assert_ne!(
            a,
            hydraulic(&heights, size, scale, &other, &control).unwrap()
        );
    }

    #[test]
    fn thermal_erosion_moves_material_off_steep_slopes_and_keeps_it() {
        let (heights, size, scale) = cone();
        // The cone's sides are about 51°, steeper than 35°.
        let settings = ThermalSettings {
            iterations: 30,
            ..ThermalSettings::default()
        };
        let control = JobControl::new();
        let eroded = thermal(&heights, size, scale, &settings, &control).unwrap();
        assert!(eroded[32 * 65 + 32] < heights[32 * 65 + 32] - 0.01);
        // Nothing is lost, only moved (none reaches the edge).
        assert!((total(&eroded) - total(&heights)).abs() < 0.01);
        // A slope gentler than the talus angle stays.
        let gentle: Vec<f32> = heights.iter().map(|h| h * 0.2).collect();
        let kept = thermal(&gentle, size, scale, &settings, &control).unwrap();
        let moved: f32 = gentle.iter().zip(&kept).map(|(a, b)| (a - b).abs()).sum();
        assert!(moved < 1e-4, "{moved}");
    }

    #[test]
    fn cancelling_stops_erosion() {
        let (heights, size, scale) = cone();
        let control = JobControl::new();
        control.cancel();
        assert!(
            hydraulic(
                &heights,
                size,
                scale,
                &HydraulicSettings::default(),
                &control
            )
            .is_none()
        );
        assert!(thermal(&heights, size, scale, &ThermalSettings::default(), &control).is_none());
    }
}
