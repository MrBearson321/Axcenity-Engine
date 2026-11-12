//! A mesh's surface kept in a bounding volume hierarchy, so a ray's hit is
//! quick to find. Rigging's bone heat (`rig/weights.rs`) uses it to test
//! whether a bone can see a vertex. Retopology, which made it (Phase 61),
//! was removed in Phase 90.

use glam::Vec3;

use crate::bvh::Bvh;
use crate::half_edge::EditMesh;

/// An object's surface, to cast occlusion rays at.
#[derive(Clone, Debug)]
pub struct Reference {
    positions: Vec<Vec3>,
    /// Its faces cut into triangles, by vertex.
    triangles: Vec<[u32; 3]>,
    bvh: Bvh,
    /// The length of its box's diagonal.
    size: f32,
}

impl Reference {
    pub fn new(mesh: &EditMesh) -> Reference {
        let mut triangles = Vec::new();
        for face in 0..mesh.face_count() as u32 {
            for triangle in mesh.triangles(face) {
                triangles.push(triangle.map(|e| mesh.half_edge(e).vertex));
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
            size,
        }
    }

    /// The length of the diagonal of the box round it.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// True if the ray meets the surface closer than `limit`.
    pub fn occluded(&self, origin: Vec3, direction: Vec3, limit: f32) -> bool {
        self.bvh
            .occluded(&self.positions, &self.triangles, origin, direction, limit)
    }
}
