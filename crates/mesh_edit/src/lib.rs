//! Axcenity mesh editing: triangle meshes on the CPU that can be cut and
//! broken into pieces.
//!
//! - [`Mesh`] is a closed triangle mesh, each triangle marked with the
//!   [`Surface`] it came from.
//! - [`Mesh::cut`] splits one along a [`Plane`] and closes both halves.
//! - [`fracture`] breaks one into convex Voronoi pieces, and [`write_glb`]
//!   saves them as one `.glb` file with a node per piece.
//! - [`generate`] is what the editor's Fracture Generate button calls.
//! - [`boolean`] is the union, difference or intersection of two closed
//!   meshes, and [`combine_parts`] what the editor's Union and Intersect
//!   (and scripts' `UnionAsync`, `SubtractAsync`, `IntersectAsync`) call.
//!
//! Everything here is deterministic: the same input gives the same output,
//! bit for bit, so a project regenerates the same pieces anywhere.

mod csg;
#[cfg(test)]
mod csg_tests;
mod cut;
mod fill;
mod fracture;
mod generate;
mod glb;
mod merge;
mod shapes;
mod solid;
#[cfg(test)]
mod tests;

pub use csg::{Operation, boolean};
pub use fracture::{Pattern, fracture, seed_points, voronoi_pieces};
pub use generate::{Settings, generate, generate_for};
pub use glb::{GlbMaterial, write_glb};
pub use solid::{Combined, Solid, combine, combine_parts, save_union, solid_of};

use glam::{Vec2, Vec3};

/// A closed triangle mesh. Vertices may be split (same position, different
/// normal/uv), so whether it's closed is judged by positions, not indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    /// One per position, one unit long.
    pub normals: Vec<Vec3>,
    /// One per position.
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<Triangle>,
}

/// Three corners (indices into the mesh's vertices), counter-clockwise seen
/// from outside, and the surface they belong to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub corners: [u32; 3],
    pub surface: Surface,
}

/// Where a triangle came from, which decides how it's drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// The original surface, with the index of its material.
    Outer(u32),
    /// A face made by closing a cut.
    Interior,
}

/// A flat plane. Points p with normal·p <= offset are below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub offset: f32,
}

impl Mesh {
    /// The smallest and largest corner of the box around the positions its
    /// triangles use. Both are zero for a mesh with no triangles.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut corners = self
            .triangles
            .iter()
            .flat_map(|triangle| triangle.corners)
            .map(|corner| self.positions[corner as usize]);
        let Some(first) = corners.next() else {
            return (Vec3::ZERO, Vec3::ZERO);
        };
        corners.fold((first, first), |(min, max), p| (min.min(p), max.max(p)))
    }

    /// The space it encloses. Each triangle and the origin make a
    /// tetrahedron; adding up their signed volumes gives the volume of a
    /// closed mesh wherever the origin is (the divergence theorem). Positive
    /// when the triangles face outward.
    pub fn volume(&self) -> f32 {
        // Summed in f64: thousands of small terms lose precision in f32.
        let sum: f64 = self
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle
                    .corners
                    .map(|corner| self.positions[corner as usize].as_dvec3());
                a.dot(b.cross(c))
            })
            .sum();
        (sum / 6.0) as f32
    }

    /// True if it has no triangles.
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Adds a vertex and returns its index.
    fn push_vertex(&mut self, position: Vec3, normal: Vec3, uv: Vec2) -> u32 {
        self.positions.push(position);
        self.normals.push(normal);
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }
}

/// `point` as exact bits, for matching positions. `+ 0.0` turns -0 into 0,
/// which compare equal as numbers but not as bits.
fn position_key(point: Vec3) -> [u32; 3] {
    (point + Vec3::ZERO).to_array().map(f32::to_bits)
}
