//! Triangle meshes for the built-in shapes, built on the CPU at startup.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// One corner of a triangle, as the GPU reads it.
///
/// `#[repr(C)]` fixes the field order and layout, and `Pod`/`Zeroable` let
/// `bytemuck` turn a `&[Vertex]` into raw bytes for upload.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    /// Three indices per triangle, counter-clockwise when seen from outside.
    pub indices: Vec<u16>,
}

/// A 1×1×1 cube centered on the origin. Each face has its own four corners so
/// the edges stay sharp instead of being smoothed.
pub fn block() -> MeshData {
    // For each face: its outward normal, and two edge directions `u` and `v`
    // with u × v = normal, which makes the corners below go counter-clockwise.
    let faces = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];

    let mut mesh = MeshData {
        vertices: Vec::with_capacity(24),
        indices: Vec::with_capacity(36),
    };
    for (normal, u, v) in faces {
        let first = mesh.vertices.len() as u16;
        let center = normal * 0.5;
        for (su, sv) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            mesh.vertices.push(Vertex {
                position: (center + u * su + v * sv).to_array(),
                normal: normal.to_array(),
            });
        }
        mesh.indices
            .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_has_six_faces() {
        let mesh = block();
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn block_fits_the_unit_cube() {
        for vertex in block().vertices {
            for coordinate in vertex.position {
                assert_eq!(coordinate.abs(), 0.5);
            }
        }
    }

    /// The GPU skips back faces, so a triangle wound the wrong way would
    /// make that face disappear.
    #[test]
    fn block_triangles_face_outward() {
        let mesh = block();
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize]);
            let pa = Vec3::from(a.position);
            let pb = Vec3::from(b.position);
            let pc = Vec3::from(c.position);
            let facing = (pb - pa).cross(pc - pa);
            assert!(facing.dot(Vec3::from(a.normal)) > 0.0);
            // And the normal points away from the center.
            assert!(pa.dot(Vec3::from(a.normal)) > 0.0);
        }
    }
}
