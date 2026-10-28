//! Breaking a mesh into pieces: the Voronoi cells of scattered seed points.
//!
//! Each piece is the part of the mesh nearer its seed than any other. It's
//! made by cutting the mesh along the plane halfway between its seed and
//! every other seed, keeping the near side each time. Cutting a convex mesh
//! with planes keeps it convex, so a convex mesh breaks into convex pieces,
//! which physics can use as colliders as they are. Neighbouring pieces are
//! cut by the very same plane from either side, so they meet exactly: no
//! gaps, no overlaps.

use std::f32::consts::TAU;

use glam::{DVec3, Vec3};

use crate::{Mesh, Plane};

/// The most pieces one fracture makes.
const MAX_PIECES: usize = 256;

/// Tries for each seed to land inside the mesh. After that a seed in its
/// box but outside it is kept: it may still have a piece of the mesh
/// nearest to it, and otherwise its piece is dropped.
const SEED_TRIES: usize = 32;

/// How seeds are spread over the mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pattern {
    /// Evenly, so pieces are about the same size.
    #[default]
    Uniform,
    /// More, smaller pieces toward the middle, as where something struck.
    Impact,
}

/// `count` seed points inside the mesh's box, deterministic from `seed`.
/// Seeds outside the mesh (for shapes that don't fill their box) are tried
/// again a few times. Uniform spreads them evenly over the box; Impact
/// crowds them toward its middle.
pub fn seed_points(mesh: &Mesh, count: usize, seed: u64, pattern: Pattern) -> Vec<Vec3> {
    if mesh.is_empty() {
        return Vec::new();
    }
    let (min, max) = mesh.bounds();
    let (middle, half) = ((min + max) / 2.0, (max - min) / 2.0);
    // A mesh without volume (a flat sheet) has no inside to test.
    let solid = mesh.volume() > 0.0;
    // Two seeds closer than this would make a piece too thin to keep.
    let apart = 1e-4 * half.max_element();
    let thin = thin_axis(half);
    let mut random = Random(seed);
    let mut seeds: Vec<Vec3> = Vec::with_capacity(count);
    while seeds.len() < count {
        let mut chosen = None;
        for _ in 0..SEED_TRIES {
            let mut point = match pattern {
                Pattern::Uniform => min + (max - min) * random.vec3(),
                // Squaring the distance from the middle bunches seeds there.
                Pattern::Impact => middle + random.direction() * half * random.unit().powi(2),
            };
            // In a thin part (a pane, a plank) every seed is on the middle
            // plane, so the cuts go straight through it, as cracks in glass
            // do, and no piece rests on a slanted one.
            if let Some(axis) = thin {
                point[axis] = middle[axis];
            }
            if seeds.iter().any(|seed| seed.distance(point) <= apart) {
                continue;
            }
            chosen = Some(point);
            if !solid || winding_number(mesh, point) > 0.5 {
                break;
            }
        }
        // Only a mesh almost too small to cut runs out of room; its
        // repeated seeds get no piece.
        seeds.push(chosen.unwrap_or(middle));
    }
    seeds
}

/// A part counts as thin when its smallest side is under this share of the
/// next smallest.
const THIN: f32 = 0.25;

/// The axis a mesh of these half sizes is thin along, if it is.
fn thin_axis(half: Vec3) -> Option<usize> {
    let mut axes = [0, 1, 2];
    axes.sort_by(|&a, &b| half[a].total_cmp(&half[b]));
    (half[axes[0]] < THIN * half[axes[1]]).then_some(axes[0])
}

/// The Voronoi cell of each seed, clipped to the mesh: the mesh cut by the
/// plane halfway to every other seed, keeping the side nearer this seed.
/// Seeds with nothing (outside the mesh, or at the same point as an earlier
/// seed) are left out, so there may be fewer pieces than seeds. A convex
/// mesh gives convex pieces that tile it exactly.
pub fn voronoi_pieces(mesh: &Mesh, seeds: &[Vec3]) -> Vec<Mesh> {
    (0..seeds.len())
        .filter_map(|index| cell(mesh, seeds, index))
        .collect()
}

/// [`seed_points`] then [`voronoi_pieces`]. `count` is clamped to 1..=256;
/// 1 gives the mesh itself.
pub fn fracture(mesh: &Mesh, count: usize, seed: u64, pattern: Pattern) -> Vec<Mesh> {
    let count = count.clamp(1, MAX_PIECES);
    if mesh.is_empty() {
        return Vec::new();
    }
    if count == 1 {
        return vec![mesh.clone()];
    }
    voronoi_pieces(mesh, &seed_points(mesh, count, seed, pattern))
}

/// The cell of seed `index`, or `None` if it's empty.
fn cell(mesh: &Mesh, seeds: &[Vec3], index: usize) -> Option<Mesh> {
    let seed = seeds[index];
    // Nearest seeds first: their planes cut the most away, which makes
    // later cuts cheaper and lets the loop stop early.
    let mut others: Vec<(f32, usize)> = seeds
        .iter()
        .enumerate()
        .filter(|&(other, _)| other != index)
        .map(|(other, &point)| (point.distance(seed), other))
        .collect();
    others.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut cell = mesh.clone();
    // The cell is inside the sphere this big around the seed. A plane
    // farther from the seed than that can't touch it, and since the planes
    // come nearest first, neither can any after it.
    let mut radius = reach(&cell, seed);
    for (distance, other) in others {
        if distance == 0.0 {
            // Two seeds at one point: the first takes the cell.
            if other < index {
                return None;
            }
            continue;
        }
        if distance / 2.0 > radius {
            break;
        }
        let point = seeds[other];
        let normal = (point - seed).normalize();
        let plane = Plane {
            normal,
            offset: normal.dot((seed + point) / 2.0),
        };
        cell = cell.cut_below(plane)?;
        radius = reach(&cell, seed);
    }
    Some(cell)
}

/// How far the farthest corner of `mesh` is from `point`.
fn reach(mesh: &Mesh, point: Vec3) -> f32 {
    mesh.triangles
        .iter()
        .flat_map(|triangle| triangle.corners)
        .map(|corner| mesh.positions[corner as usize].distance(point))
        .fold(0.0, f32::max)
}

/// How many times `mesh` wraps around `point`: about 1 inside a closed mesh
/// and 0 outside, even when the mesh has small cracks. It adds up the solid
/// angle each triangle covers seen from the point (Van Oosterom and
/// Strackee's formula), over the whole sphere's 4π.
fn winding_number(mesh: &Mesh, point: Vec3) -> f64 {
    let point = point.as_dvec3();
    let angle: f64 = mesh
        .triangles
        .iter()
        .map(|triangle| {
            let [a, b, c]: [DVec3; 3] = triangle
                .corners
                .map(|corner| mesh.positions[corner as usize].as_dvec3() - point);
            let (la, lb, lc) = (a.length(), b.length(), c.length());
            let above = a.dot(b.cross(c));
            let across = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
            2.0 * above.atan2(across)
        })
        .sum();
    angle / (4.0 * std::f64::consts::PI)
}

/// A small random number generator (SplitMix64), the same on every OS, so
/// a seed always gives the same pieces.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from 0 up to (not including) 1. The top 24 bits fit an f32
    /// exactly.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// Each axis from 0 to 1.
    fn vec3(&mut self) -> Vec3 {
        let x = self.unit();
        let y = self.unit();
        let z = self.unit();
        Vec3::new(x, y, z)
    }

    /// A direction one unit long, any way equally likely.
    fn direction(&mut self) -> Vec3 {
        let z = self.unit() * 2.0 - 1.0;
        let angle = self.unit() * TAU;
        let across = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(across * angle.cos(), across * angle.sin(), z)
    }
}
