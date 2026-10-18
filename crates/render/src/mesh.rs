//! Triangle meshes for the built-in shapes, built on the CPU at startup.
//!
//! Every shape fits the 1×1×1 cube centered on the origin. Triangles are
//! counter-clockwise when seen from outside, because back faces are culled.

use std::f32::consts::{PI, TAU};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// How many slices round shapes are cut into. More looks smoother but costs more triangles.
const ROUND_SEGMENTS: u16 = 32;

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

#[derive(Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    /// Three indices per triangle, counter-clockwise when seen from outside.
    pub indices: Vec<u16>,
}

impl MeshData {
    fn push_vertex(&mut self, position: Vec3, normal: Vec3) -> u16 {
        let index = self.vertices.len() as u16;
        self.vertices.push(Vertex {
            position: position.to_array(),
            normal: normal.to_array(),
        });
        index
    }

    /// A flat four-cornered face. Its corners are `center ± u ± v`, and
    /// `u × v` must point the same way as `normal` so the corners go
    /// counter-clockwise.
    fn push_quad(&mut self, normal: Vec3, center: Vec3, u: Vec3, v: Vec3) {
        let [a, b, c, d] =
            [-u - v, u - v, u + v, v - u].map(|offset| self.push_vertex(center + offset, normal));
        self.indices.extend([a, b, c, a, c, d]);
    }

    /// A flat triangle. The corners must be counter-clockwise seen from outside.
    fn push_triangle(&mut self, normal: Vec3, corners: [Vec3; 3]) {
        let indices = corners.map(|corner| self.push_vertex(corner, normal));
        self.indices.extend(indices);
    }
}

/// A cube. Each face has its own four corners so the edges stay sharp
/// instead of being smoothed.
pub fn block() -> MeshData {
    // For each face: its outward normal, and two edge directions `u` and `v`
    // with u × v = normal.
    let faces = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];

    let mut mesh = MeshData::default();
    for (normal, u, v) in faces {
        mesh.push_quad(normal, normal * 0.5, u * 0.5, v * 0.5);
    }
    mesh
}

/// A sphere with smooth normals, made of rings from the top (+Y) to the bottom.
pub fn sphere() -> MeshData {
    let rings = ROUND_SEGMENTS / 2;
    let mut mesh = MeshData::default();

    // A grid of corners. The first and last column are the same points, so
    // the seam closes without wrapping indices around.
    for ring in 0..=rings {
        let down = PI * f32::from(ring) / f32::from(rings);
        for segment in 0..=ROUND_SEGMENTS {
            let around = TAU * f32::from(segment) / f32::from(ROUND_SEGMENTS);
            let normal = Vec3::new(
                down.sin() * around.cos(),
                down.cos(),
                down.sin() * around.sin(),
            );
            mesh.push_vertex(normal * 0.5, normal);
        }
    }

    let row = ROUND_SEGMENTS + 1;
    for ring in 0..rings {
        for segment in 0..ROUND_SEGMENTS {
            let top = ring * row + segment;
            let bottom = top + row;
            // At the poles, two corners of each square are the same point, so
            // one of its two triangles would have no area. Skip that one.
            if ring > 0 {
                mesh.indices.extend([top, top + 1, bottom + 1]);
            }
            if ring < rings - 1 {
                mesh.indices.extend([top, bottom + 1, bottom]);
            }
        }
    }
    mesh
}

/// A cylinder lying along the X axis, with flat ends at x = ±0.5.
pub fn cylinder() -> MeshData {
    let mut mesh = MeshData::default();
    // The direction from the axis to the rim, `segment` slices around.
    let rim = |segment: u16| {
        let angle = TAU * f32::from(segment) / f32::from(ROUND_SEGMENTS);
        Vec3::new(0.0, angle.cos(), angle.sin())
    };

    // The curved side: pairs of corners, one at each end, with smooth normals.
    for segment in 0..=ROUND_SEGMENTS {
        let normal = rim(segment);
        mesh.push_vertex(normal * 0.5 - Vec3::X * 0.5, normal);
        mesh.push_vertex(normal * 0.5 + Vec3::X * 0.5, normal);
    }
    for segment in 0..ROUND_SEGMENTS {
        let left = segment * 2;
        let right = left + 1;
        let (next_left, next_right) = (left + 2, right + 2);
        mesh.indices
            .extend([left, next_left, next_right, left, next_right, right]);
    }

    // The two flat ends, each a fan of triangles around its center.
    for end in [Vec3::X, Vec3::NEG_X] {
        let center = end * 0.5;
        for segment in 0..ROUND_SEGMENTS {
            let a = center + rim(segment) * 0.5;
            let b = center + rim(segment + 1) * 0.5;
            // Seen from each end, the rim runs the opposite way around.
            let corners = if end == Vec3::X {
                [center, a, b]
            } else {
                [center, b, a]
            };
            mesh.push_triangle(end, corners);
        }
    }
    mesh
}

/// A ramp: full bottom and back (+Z) faces, and a slope rising from the
/// bottom front edge to the top back edge.
pub fn wedge() -> MeshData {
    let mut mesh = MeshData::default();
    mesh.push_quad(Vec3::NEG_Y, Vec3::NEG_Y * 0.5, Vec3::X * 0.5, Vec3::Z * 0.5);
    mesh.push_quad(Vec3::Z, Vec3::Z * 0.5, Vec3::X * 0.5, Vec3::Y * 0.5);
    mesh.push_quad(
        Vec3::new(0.0, 1.0, -1.0).normalize(),
        Vec3::ZERO,
        Vec3::NEG_X * 0.5,
        Vec3::new(0.0, 0.5, 0.5),
    );

    // The two triangular sides.
    let front_bottom = Vec3::new(0.0, -0.5, -0.5);
    let back_bottom = Vec3::new(0.0, -0.5, 0.5);
    let back_top = Vec3::new(0.0, 0.5, 0.5);
    let [a, b, c] = [front_bottom, back_top, back_bottom];
    let right = Vec3::X * 0.5;
    mesh.push_triangle(Vec3::X, [a + right, b + right, c + right]);
    // Seen from the other side, the same corners run the opposite way around.
    mesh.push_triangle(Vec3::NEG_X, [a - right, c - right, b - right]);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each mesh with a point that lies inside it.
    fn all_meshes() -> [(&'static str, MeshData, Vec3); 4] {
        [
            ("block", block(), Vec3::ZERO),
            ("sphere", sphere(), Vec3::ZERO),
            ("cylinder", cylinder(), Vec3::ZERO),
            // The wedge's center lies on its slope, so use a point below the slope.
            ("wedge", wedge(), Vec3::new(0.0, -0.25, 0.25)),
        ]
    }

    #[test]
    fn block_has_six_faces() {
        let mesh = block();
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn block_corners_are_on_the_unit_cube() {
        for vertex in block().vertices {
            for coordinate in vertex.position {
                assert_eq!(coordinate.abs(), 0.5);
            }
        }
    }

    #[test]
    fn every_shape_fills_the_unit_cube() {
        for (name, mesh, _) in all_meshes() {
            let positions = mesh.vertices.iter().map(|v| Vec3::from(v.position));
            let min = positions.clone().fold(Vec3::INFINITY, Vec3::min);
            let max = positions.fold(Vec3::NEG_INFINITY, Vec3::max);
            assert!(min.abs_diff_eq(Vec3::splat(-0.5), 1e-6), "{name}: {min}");
            assert!(max.abs_diff_eq(Vec3::splat(0.5), 1e-6), "{name}: {max}");
        }
    }

    #[test]
    fn every_index_points_at_a_vertex() {
        for (name, mesh, _) in all_meshes() {
            assert_eq!(mesh.indices.len() % 3, 0, "{name}");
            for &index in &mesh.indices {
                assert!((index as usize) < mesh.vertices.len(), "{name}");
            }
        }
    }

    /// The GPU skips back faces, so a triangle wound the wrong way would
    /// make that face disappear, and a normal pointing inward would light it wrongly.
    #[test]
    fn triangles_and_normals_face_outward() {
        for (name, mesh, inside) in all_meshes() {
            for triangle in mesh.indices.chunks(3) {
                let corners = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize]);
                let [pa, pb, pc] = corners.map(|v| Vec3::from(v.position));
                let facing = (pb - pa).cross(pc - pa);
                let outward = (pa + pb + pc) / 3.0 - inside;
                assert!(
                    facing.dot(outward) > 0.0,
                    "{name}: {triangle:?} is wound inward"
                );
                for corner in corners {
                    let normal = Vec3::from(corner.normal);
                    assert!(
                        facing.dot(normal) > 0.0,
                        "{name}: {triangle:?} normal points inward"
                    );
                }
            }
        }
    }

    #[test]
    fn normals_are_unit_length() {
        for (name, mesh, _) in all_meshes() {
            for vertex in &mesh.vertices {
                let length = Vec3::from(vertex.normal).length();
                assert!((length - 1.0).abs() < 1e-5, "{name}: {length}");
            }
        }
    }
}
