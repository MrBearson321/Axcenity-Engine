//! A high-poly object's surface (a sculpt), kept in a bounding volume
//! hierarchy so the nearest point on it, or a ray's hit, is quick to find.
//! Baking (`bake.rs`) reads its normals, uvs and curvature through it,
//! Make Low Poly evens a decimated copy out on it ([`crate::relax`]), and
//! rigging's bone heat (`rig/weights.rs`) casts occlusion rays at it.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};

use crate::bvh::Bvh;
#[cfg(test)]
use crate::bvh::hit;
use crate::half_edge::EditMesh;

/// A high-poly object's surface, to snap to and bake from.
#[derive(Clone, Debug)]
pub struct Reference {
    positions: Vec<Vec3>,
    /// Its faces cut into triangles, by vertex.
    triangles: Vec<[u32; 3]>,
    /// Each triangle's corners' normals and uvs, and its material slot.
    normals: Vec<[Vec3; 3]>,
    uvs: Vec<[Vec2; 3]>,
    slots: Vec<u32>,
    bvh: Bvh,
    /// The length of its box's diagonal.
    size: f32,
}

/// A point on a [`Reference`]: the triangle it's on and its barycentric
/// weights there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePoint {
    pub triangle: u32,
    pub position: Vec3,
    pub weights: [f32; 3],
}

impl Reference {
    pub fn new(mesh: &EditMesh) -> Reference {
        let mut triangles = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut slots = Vec::new();
        for face in 0..mesh.face_count() as u32 {
            for triangle in mesh.triangles(face) {
                let corners = triangle.map(|e| *mesh.half_edge(e));
                triangles.push(corners.map(|corner| corner.vertex));
                normals.push(corners.map(|corner| corner.normal));
                uvs.push(corners.map(|corner| corner.uv));
                slots.push(mesh.face(face).material);
            }
        }
        let positions = mesh.positions().to_vec();
        let (min, max) = positions
            .iter()
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &p| {
                (min.min(p), max.max(p))
            });
        let size = if positions.is_empty() {
            0.0
        } else {
            min.distance(max)
        };
        Reference {
            bvh: Bvh::build(&positions, &triangles),
            positions,
            triangles,
            normals,
            uvs,
            slots,
            size,
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// The length of the diagonal of the box round it.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// The point of the surface nearest `point`.
    pub fn nearest(&self, point: Vec3) -> Option<SurfacePoint> {
        let (triangle, position, weights) =
            self.bvh.nearest(&self.positions, &self.triangles, point)?;
        Some(SurfacePoint {
            triangle,
            position,
            weights,
        })
    }

    /// `point` moved onto the surface (where it is, with no surface).
    pub fn snap(&self, point: Vec3) -> Vec3 {
        self.nearest(point).map_or(point, |on| on.position)
    }

    /// Where `origin + t·direction` first meets the surface (from either
    /// side, t > 0), and t.
    pub fn ray(&self, origin: Vec3, direction: Vec3) -> Option<(SurfacePoint, f32)> {
        let (triangle, t) =
            self.bvh
                .ray(&self.positions, &self.triangles, origin, direction, |_| {
                    false
                })?;
        let position = origin + direction * t;
        let corners = self.corners(triangle);
        let weights = crate::paint::texels::surface_weights(corners, position)
            .unwrap_or([1.0 / 3.0; 3])
            .map(|w| w.max(0.0));
        let total: f32 = weights.iter().sum::<f32>().max(1e-12);
        Some((
            SurfacePoint {
                triangle,
                position,
                weights: weights.map(|w| w / total),
            },
            t,
        ))
    }

    /// True if the ray meets the surface closer than `limit`.
    pub fn occluded(&self, origin: Vec3, direction: Vec3, limit: f32) -> bool {
        self.bvh
            .occluded(&self.positions, &self.triangles, origin, direction, limit)
    }

    /// Triangle `t`'s corners.
    pub fn corners(&self, t: u32) -> [Vec3; 3] {
        self.triangles[t as usize].map(|v| self.positions[v as usize])
    }

    /// The surface's (shaded) normal at `at`.
    pub fn normal(&self, at: &SurfacePoint) -> Vec3 {
        let normals = self.normals[at.triangle as usize];
        let blended =
            normals[0] * at.weights[0] + normals[1] * at.weights[1] + normals[2] * at.weights[2];
        blended.try_normalize().unwrap_or_else(|| {
            let [a, b, c] = self.corners(at.triangle);
            (b - a).cross(c - a).normalize_or_zero()
        })
    }

    /// The uv at `at`, and the material slot of its face.
    pub fn uv(&self, at: &SurfacePoint) -> (Vec2, u32) {
        let uvs = self.uvs[at.triangle as usize];
        let uv = uvs[0] * at.weights[0] + uvs[1] * at.weights[1] + uvs[2] * at.weights[2];
        (uv, self.slots[at.triangle as usize])
    }

    /// Each vertex's mean curvature (1 / radius: positive where the
    /// surface bulges out, negative in a hollow), from how far its
    /// neighbours sit below or above the plane along its normal, smoothed
    /// once over the neighbours.
    pub fn curvature(&self) -> Vec<f32> {
        let count = self.positions.len();
        let mut normals = vec![Vec3::ZERO; count];
        let mut neighbours: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); count];
        for (t, triangle) in self.triangles.iter().enumerate() {
            for k in 0..3 {
                let v = triangle[k] as usize;
                normals[v] += self.normals[t][k];
                neighbours[v].insert(triangle[(k + 1) % 3]);
                neighbours[v].insert(triangle[(k + 2) % 3]);
            }
        }
        let raw: Vec<f32> = (0..count)
            .map(|v| {
                let n = normals[v].normalize_or_zero();
                let p = self.positions[v];
                let (sum, used) = neighbours[v]
                    .iter()
                    .map(|&other| self.positions[other as usize] - p)
                    .filter(|edge| edge.length_squared() > 1e-20)
                    // A sphere of radius r: a neighbour d away sits d²/2r
                    // below the plane, so this is 1/r.
                    .map(|edge| -2.0 * edge.dot(n) / edge.length_squared())
                    .fold((0.0, 0), |(sum, used), k| (sum + k, used + 1));
                if used == 0 { 0.0 } else { sum / used as f32 }
            })
            .collect();
        (0..count)
            .map(|v| {
                let around = neighbours[v].iter().map(|&other| raw[other as usize]);
                let (sum, used) = around.fold((raw[v], 1), |(sum, used), k| (sum + k, used + 1));
                sum / used as f32
            })
            .collect()
    }

    /// `values` (one per vertex) at `at`.
    pub fn blend(&self, values: &[f32], at: &SurfacePoint) -> f32 {
        let corners = self.triangles[at.triangle as usize];
        (0..3)
            .map(|k| values[corners[k] as usize] * at.weights[k])
            .sum()
    }

    /// The first hit's distance along a ray, for tests checking `ray`.
    #[cfg(test)]
    pub(crate) fn ray_by_brute_force(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        (0..self.triangles.len() as u32)
            .filter_map(|t| hit(origin, direction, self.corners(t)))
            .min_by(f32::total_cmp)
    }
}
