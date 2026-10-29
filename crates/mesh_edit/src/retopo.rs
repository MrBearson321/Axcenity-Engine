//! Retopology (Phase 61): a new, lighter mesh made over a high-poly one
//! (a sculpt), which then has the detail baked onto it (`bake.rs`).
//!
//! - A [`Reference`] is the high-poly object's surface, kept in a bounding
//!   volume hierarchy so the nearest point on it, or a ray's hit, is quick
//!   to find. Snap to surface puts new and moved vertices on it
//!   ([`Reference::snap`]).
//! - [`poly_build`] adds a quad: through four points, or out from an open
//!   edge of the mesh to a point ([`PolyStep`]), welding to vertices
//!   already near.
//! - [`decimate_to`] cuts a mesh to a number of triangles, a quick
//!   low-poly version.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};

use crate::half_edge::{EditMesh, newell};
use crate::modifiers::reduce;
use crate::ops::{Soup, SoupFace};
use crate::sculpt::Bvh;
#[cfg(test)]
use crate::sculpt::bvh::hit;

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

/// One quad for [`poly_build`] to add.
#[derive(Clone, Debug, PartialEq)]
pub enum PolyStep {
    /// A quad through four points, in order round it, turned so it faces
    /// the way `facing` points.
    Quad { points: [Vec3; 4], facing: Vec3 },
    /// A quad on an open edge (a half-edge with no twin) out to `to`: the
    /// new edge is as long as the open one, with `to` in its middle.
    FromEdge { edge: u32, to: Vec3 },
}

/// `mesh` with the quad `step` describes, its new corners put where
/// `snap` says (onto the reference) and joined to any vertex within
/// `weld` of them. Returns the new mesh and the new face.
pub fn poly_build(
    mesh: &EditMesh,
    step: &PolyStep,
    weld: f32,
    snap: &dyn Fn(Vec3) -> Vec3,
) -> Result<(EditMesh, u32), String> {
    let mut soup = Soup::of(mesh);
    // Drawn like the face beside it: its material slot and shading.
    let like = |face: Option<u32>| {
        (
            face.map_or(0, |f| mesh.face(f).material),
            face.is_none_or(|f| mesh.face(f).smooth),
        )
    };
    let vertex = |soup: &mut Soup, point: Vec3, keep: &[u32]| -> u32 {
        let point = snap(point);
        let near = (0..mesh.vertex_count() as u32)
            .filter(|v| !keep.contains(v))
            .map(|v| (v, mesh.position(v).distance(point)))
            .filter(|&(_, distance)| distance <= weld)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match near {
            Some((v, _)) => v,
            None => soup.add(point),
        }
    };
    let (corners, template) = match step {
        PolyStep::Quad { points, facing } => {
            let mut corners = Vec::new();
            for &point in points {
                let v = vertex(&mut soup, point, &corners);
                corners.push(v);
            }
            let places: Vec<Vec3> = corners.iter().map(|&v| soup.position(v)).collect();
            if newell(&places).dot(*facing) < 0.0 {
                corners.reverse();
            }
            (corners, like(None))
        }
        PolyStep::FromEdge { edge, to } => {
            let half = mesh
                .half_edges()
                .get(*edge as usize)
                .ok_or("That edge isn't there any more")?;
            if half.twin.is_some() {
                return Err("Poly Build extends open edges (with a face on one side)".to_owned());
            }
            let (a, b) = mesh.ends(*edge);
            let along = (mesh.position(b) - mesh.position(a)) / 2.0;
            // The face has a → b, so the new one runs b → a and on.
            let near_a = vertex(&mut soup, *to - along, &[a, b]);
            let near_b = vertex(&mut soup, *to + along, &[a, b, near_a]);
            (vec![b, a, near_a, near_b], like(Some(half.face)))
        }
    };
    let mut unique = corners.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() < 3 {
        return Err("Those points make no face: they're on top of each other".to_owned());
    }
    let (material, smooth) = template;
    soup.faces.push(SoupFace {
        uvs: vec![Vec2::ZERO; corners.len()],
        corners,
        uvs2: Vec::new(),
        normals: Vec::new(),
        material,
        smooth,
    });
    let built = soup.build(true);
    let face = built
        .face_map
        .last()
        .copied()
        .flatten()
        .ok_or("Those points make no face")?;
    Ok((built.mesh, face))
}

/// Evens out `mesh` on `reference`: `times` over, each vertex moves
/// halfway to the middle of its neighbours (along the faces' edges) and
/// back onto the surface. A decimated mesh's thin triangles, and those
/// folded across a ridge, open out. Open borders stay where they are.
pub fn relax(mesh: &mut EditMesh, reference: &Reference, times: usize) {
    let count = mesh.vertex_count();
    let mut neighbours: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); count];
    let mut border = vec![false; count];
    for (index, half) in mesh.half_edges().iter().enumerate() {
        let (a, b) = mesh.ends(index as u32);
        neighbours[a as usize].insert(b);
        neighbours[b as usize].insert(a);
        if half.twin.is_none() {
            border[a as usize] = true;
            border[b as usize] = true;
        }
    }
    let vertices: Vec<u32> = (0..count as u32)
        .filter(|&v| !border[v as usize] && !neighbours[v as usize].is_empty())
        .collect();
    for _ in 0..times {
        let places: Vec<Vec3> = vertices
            .iter()
            .map(|&v| {
                let around = &neighbours[v as usize];
                let middle =
                    around.iter().map(|&n| mesh.position(n)).sum::<Vec3>() / around.len() as f32;
                reference.snap(mesh.position(v).lerp(middle, 0.5))
            })
            .collect();
        mesh.set_positions(&vertices, &places);
    }
}

/// `mesh` cut to about `triangles` triangles (or fewer) by meshoptimizer,
/// as the Decimate modifier does; uv seams and material borders stay.
pub fn decimate_to(mesh: &EditMesh, triangles: usize) -> EditMesh {
    let count: usize = (0..mesh.face_count() as u32)
        .map(|face| mesh.triangles(face).len())
        .sum();
    if count == 0 || triangles >= count {
        return mesh.clone();
    }
    let mut ratio = triangles as f32 / count as f32;
    let mut result = reduce::decimate(mesh, ratio);
    // meshoptimizer stops short of the target where it can't collapse
    // more without breaking the surface: aim lower until under it.
    for _ in 0..4 {
        let made = result.face_count();
        if made <= triangles {
            break;
        }
        ratio *= triangles as f32 / made as f32 * 0.98;
        result = reduce::decimate(mesh, ratio);
    }
    result
}
