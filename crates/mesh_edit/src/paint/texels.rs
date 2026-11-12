//! Where on the surface each texel of a painted map is (Phase 59): the
//! slot's triangles drawn into uv space, a triangle number per texel (the
//! cached triangle-ID buffer), with each covered texel's place and normal
//! on the mesh.
//!
//! Faces whose uvs overlap share their texels: a texel lists each.
//!
//! A gutter of texels round each uv island gets the place its triangle
//! would have there, carried on past its edge. A brush in the 3D view
//! paints every texel near it on the surface, so the texels either side of
//! a seam, and the gutter the GPU's filter reads at an island's edge, get
//! the same paint, and the seam doesn't show.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};

use crate::half_edge::EditMesh;

const NONE: u32 = u32::MAX;

/// One triangle of the slot's faces.
#[derive(Clone, Copy, Debug)]
struct Triangle {
    face: u32,
    positions: [Vec3; 3],
    normals: [Vec3; 3],
    uvs: [Vec2; 3],
    /// The middle of its texels on the surface, and how far the farthest
    /// is from it.
    center: Vec3,
    reach: f32,
}

/// A map's texels on the surface of one material slot of a mesh.
#[derive(Clone, Debug)]
pub struct TexelMap {
    size: u32,
    triangles: Vec<Triangle>,
    /// Triangle `t`'s texels are `starts[t]..starts[t + 1]` of `texels`,
    /// `positions` and `normals`.
    starts: Vec<u32>,
    texels: Vec<u32>,
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    /// The (first) triangle each texel is on, or in the gutter of, or
    /// none.
    owner: Vec<u32>,
}

impl TexelMap {
    /// The texels of a map `size` across on `mesh`'s faces drawn with
    /// material `slot`, by their first uvs (repeating past the texture's
    /// edges).
    pub fn build(mesh: &EditMesh, slot: u32, size: u32) -> TexelMap {
        let size = size.max(1);
        let mut triangles = Vec::new();
        for face in 0..mesh.face_count() as u32 {
            if mesh.face(face).material != slot {
                continue;
            }
            for triangle in mesh.triangles(face) {
                let corners = triangle.map(|e| *mesh.half_edge(e));
                triangles.push(Triangle {
                    face,
                    positions: corners.map(|corner| mesh.position(corner.vertex)),
                    normals: corners.map(|corner| corner.normal),
                    uvs: corners.map(|corner| corner.uv),
                    center: Vec3::ZERO,
                    reach: 0.0,
                });
            }
        }
        let count = (size as usize) * (size as usize);
        let mut owner = vec![NONE; count];
        let mut claimed = vec![NONE; count];
        // (triangle, texel, place, normal) for each covered texel.
        let mut found: Vec<(u32, u32, Vec3, Vec3)> = Vec::new();
        let scale = size as f32;
        let wrap = |x: i64, y: i64| {
            let s = i64::from(size);
            (y.rem_euclid(s) * s + x.rem_euclid(s)) as u32
        };
        for (t, triangle) in triangles.iter().enumerate() {
            let points = triangle.uvs.map(|uv| uv * scale);
            if !points.iter().all(|p| p.is_finite()) {
                continue;
            }
            let min = points[0].min(points[1]).min(points[2]);
            let max = points[0].max(points[1]).max(points[2]);
            // A triangle spread over many copies of the texture is drawn
            // once at most.
            if (max - min).max_element() > scale * 2.0 {
                continue;
            }
            for y in (min.y - 0.5).ceil() as i64..=(max.y - 0.5).floor() as i64 {
                for x in (min.x - 0.5).ceil() as i64..=(max.x - 0.5).floor() as i64 {
                    let at = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let Some(weights) = barycentric(points, at) else {
                        continue;
                    };
                    if weights.iter().any(|&w| w < -1e-4) {
                        continue;
                    }
                    let texel = wrap(x, y);
                    // Once for each face: a texel on the edge between two
                    // of a face's triangles is that face's once, but faces
                    // whose uvs overlap (a cube's six squares) each have it.
                    if claimed[texel as usize] == triangle.face {
                        continue;
                    }
                    claimed[texel as usize] = triangle.face;
                    if owner[texel as usize] == NONE {
                        owner[texel as usize] = t as u32;
                    }
                    let (place, normal) = triangle.at(weights);
                    found.push((t as u32, texel, place, normal));
                }
            }
        }
        // The gutter, a few texels deep, spreading out from the islands.
        let depth = (size / 128).clamp(2, 16);
        let mut frontier: Vec<u32> = found.iter().map(|&(_, texel, _, _)| texel).collect();
        for _ in 0..depth {
            let mut next = Vec::new();
            for &texel in &frontier {
                let t = owner[texel as usize];
                let (x, y) = (i64::from(texel % size), i64::from(texel / size));
                for (dx, dy) in [
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                    (-1, -1),
                ] {
                    let neighbour = wrap(x + dx, y + dy);
                    if owner[neighbour as usize] != NONE {
                        continue;
                    }
                    owner[neighbour as usize] = t;
                    let triangle = &triangles[t as usize];
                    let points = triangle.uvs.map(|uv| uv * scale);
                    // The texel's middle as near the triangle as wrapping
                    // allows.
                    let middle = (points[0] + points[1] + points[2]) / 3.0;
                    let mut at = Vec2::new((x + dx) as f32 + 0.5, (y + dy) as f32 + 0.5);
                    at.x += ((middle.x - at.x) / scale).round() * scale;
                    at.y += ((middle.y - at.y) / scale).round() * scale;
                    let weights = barycentric(points, at).unwrap_or([1.0 / 3.0; 3]);
                    let (place, normal) = triangle.at(weights);
                    found.push((t, neighbour, place, normal));
                    next.push(neighbour);
                }
            }
            frontier = next;
        }
        // By triangle, keeping the order found within each.
        let mut starts = vec![0u32; triangles.len() + 1];
        for &(t, ..) in &found {
            starts[t as usize + 1] += 1;
        }
        for t in 0..triangles.len() {
            starts[t + 1] += starts[t];
        }
        let mut fill = starts.clone();
        let mut texels = vec![0; found.len()];
        let mut positions = vec![Vec3::ZERO; found.len()];
        let mut normals = vec![Vec3::ZERO; found.len()];
        for (t, texel, place, normal) in found {
            let at = fill[t as usize] as usize;
            fill[t as usize] += 1;
            texels[at] = texel;
            positions[at] = place;
            normals[at] = normal;
        }
        for (t, triangle) in triangles.iter_mut().enumerate() {
            let range = starts[t] as usize..starts[t + 1] as usize;
            let places = &positions[range];
            let center = triangle.positions.iter().sum::<Vec3>() / 3.0;
            let reach = places
                .iter()
                .chain(&triangle.positions)
                .map(|p| p.distance(center))
                .fold(0.0, f32::max);
            triangle.center = center;
            triangle.reach = reach;
        }
        TexelMap {
            size,
            triangles,
            starts,
            texels,
            positions,
            normals,
            owner,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// True if texel `texel` is on the surface (or in an island's gutter).
    pub fn covers(&self, texel: u32) -> bool {
        self.owner
            .get(texel as usize)
            .is_some_and(|&owner| owner != NONE)
    }

    /// The face texel `texel` is on, if any.
    pub fn face_of(&self, texel: u32) -> Option<u32> {
        let owner = *self.owner.get(texel as usize)?;
        (owner != NONE).then(|| self.triangles[owner as usize].face)
    }

    /// Every covered texel within `radius` of `center`: its number, place
    /// and normal on the surface, and its triangle.
    pub fn near(&self, center: Vec3, radius: f32) -> Vec<(u32, Vec3, Vec3, u32)> {
        let mut near = Vec::new();
        for (t, triangle) in self.triangles.iter().enumerate() {
            if triangle.center.distance(center) > triangle.reach + radius {
                continue;
            }
            for k in self.starts[t] as usize..self.starts[t + 1] as usize {
                if self.positions[k].distance_squared(center) <= radius * radius {
                    near.push((self.texels[k], self.positions[k], self.normals[k], t as u32));
                }
            }
        }
        near
    }

    /// The texels of `faces` (and their gutters).
    pub fn face_texels(&self, faces: &BTreeSet<u32>) -> Vec<u32> {
        let mut texels = Vec::new();
        for (t, triangle) in self.triangles.iter().enumerate() {
            if faces.contains(&triangle.face) {
                texels.extend(&self.texels[self.starts[t] as usize..self.starts[t + 1] as usize]);
            }
        }
        texels
    }

    /// The uv at `point` on face `face` (on its triangle nearest it), if
    /// the face is in this map's slot.
    pub fn uv_at(&self, face: u32, point: Vec3) -> Option<Vec2> {
        self.triangles
            .iter()
            .filter(|triangle| triangle.face == face)
            .filter_map(|triangle| {
                let weights = surface_weights(triangle.positions, point)?;
                let outside = weights.iter().map(|w| (-w).max(0.0)).sum::<f32>();
                Some((outside, triangle.uv_at(weights)))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, uv)| uv)
    }

    /// How far a move of `motion` on the surface goes in uvs, on triangle
    /// `triangle` (a Smear's drag).
    pub(crate) fn uv_motion(&self, triangle: u32, motion: Vec3) -> Vec2 {
        let Some(triangle) = self.triangles.get(triangle as usize) else {
            return Vec2::ZERO;
        };
        let [p0, p1, p2] = triangle.positions;
        let [t0, t1, t2] = triangle.uvs;
        let (e1, e2) = (p1 - p0, p2 - p0);
        // motion ≈ a·e1 + b·e2 by least squares.
        let (a11, a12, a22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
        let (b1, b2) = (e1.dot(motion), e2.dot(motion));
        let det = a11 * a22 - a12 * a12;
        if det.abs() < 1e-20 {
            return Vec2::ZERO;
        }
        let a = (b1 * a22 - b2 * a12) / det;
        let b = (a11 * b2 - a12 * b1) / det;
        (t1 - t0) * a + (t2 - t0) * b
    }
}

impl Triangle {
    /// The place and normal at barycentric `weights` (which may be
    /// outside it: the place carries on flat; the normal stays inside).
    fn at(&self, weights: [f32; 3]) -> (Vec3, Vec3) {
        let place = self.positions[0] * weights[0]
            + self.positions[1] * weights[1]
            + self.positions[2] * weights[2];
        let inside = weights.map(|w| w.max(0.0));
        let total = inside.iter().sum::<f32>().max(1e-12);
        let normal = (self.normals[0] * inside[0]
            + self.normals[1] * inside[1]
            + self.normals[2] * inside[2])
            / total;
        let flat = (self.positions[1] - self.positions[0])
            .cross(self.positions[2] - self.positions[0])
            .normalize_or_zero();
        (place, normal.try_normalize().unwrap_or(flat))
    }

    fn uv_at(&self, weights: [f32; 3]) -> Vec2 {
        self.uvs[0] * weights[0] + self.uvs[1] * weights[1] + self.uvs[2] * weights[2]
    }
}

/// `at`'s barycentric weights in the flat triangle `points`, or `None` if
/// it has no area.
fn barycentric([a, b, c]: [Vec2; 3], at: Vec2) -> Option<[f32; 3]> {
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-12 {
        return None;
    }
    let wa = (b - at).perp_dot(c - at) / area;
    let wb = (c - at).perp_dot(a - at) / area;
    Some([wa, wb, 1.0 - wa - wb])
}

/// `point`'s barycentric weights in the triangle `corners` in space,
/// after dropping it onto the triangle's plane.
pub(crate) fn surface_weights([a, b, c]: [Vec3; 3], point: Vec3) -> Option<[f32; 3]> {
    let (ab, ac, ap) = (b - a, c - a, point - a);
    let (d00, d01, d11) = (ab.dot(ab), ab.dot(ac), ac.dot(ac));
    let (d20, d21) = (ap.dot(ab), ap.dot(ac));
    let det = d00 * d11 - d01 * d01;
    if det.abs() < 1e-20 {
        return None;
    }
    let wb = (d11 * d20 - d01 * d21) / det;
    let wc = (d00 * d21 - d01 * d20) / det;
    Some([1.0 - wb - wc, wb, wc])
}
