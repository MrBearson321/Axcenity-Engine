//! Meshes to cut: the built-in part shapes at a part's size, and loaded
//! models.
//!
//! The shapes match the drawn ones (`render`'s `mesh.rs`) and the colliders
//! (`physics`' `shapes.rs`): a Ball is as wide as the part's smallest side,
//! and a Cylinder lies along X, as long as `Size.X` and as wide as the
//! smaller of `Size.Y` and `Size.Z`. Round shapes use fewer slices than the
//! drawn ones, since every piece of a fracture repeats them. Each shape is
//! closed: a seam or pole is the very same position on both sides, so the
//! cut can match its edges.

use std::f32::consts::{PI, TAU};

use axcenity_assets::Model;
use glam::{Vec2, Vec3};

use crate::{Mesh, Surface, Triangle};

/// Slices around a Ball, and rings from its top to its bottom.
const BALL_SEGMENTS: u32 = 16;
const BALL_RINGS: u32 = 12;

/// Sides of a Cylinder.
const CYLINDER_SIDES: u32 = 24;

/// The surface every built-in shape has: its part's one look.
const OUTER: Surface = Surface::Outer(0);

impl Mesh {
    /// A box of `size` centered on the origin. Each face has its own four
    /// corners (sharp edges), with uvs from 0 to 1 across it.
    pub fn block(size: Vec3) -> Mesh {
        let half = size / 2.0;
        // Each face's outward normal and two edge directions with u × v = normal.
        let faces = [
            (Vec3::X, Vec3::Y, Vec3::Z),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::Y, Vec3::X),
        ];
        let mut mesh = Mesh::default();
        for (normal, u, v) in faces {
            let corners = [-u - v, u - v, u + v, -u + v].map(|corner| (normal + corner) * half);
            mesh.push_face(&corners, &SQUARE_UVS);
        }
        mesh
    }

    /// Axcenity's Wedge at `size`: the bottom and back (+Z) faces are full
    /// rectangles, and the slope rises from the bottom front (-Z) edge to the
    /// top back edge.
    pub fn wedge(size: Vec3) -> Mesh {
        let half = size / 2.0;
        let corner = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * half;
        let mut mesh = Mesh::default();
        // Bottom, back and slope.
        let quads = [
            [(1, -1, -1), (1, -1, 1), (-1, -1, 1), (-1, -1, -1)],
            [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)],
            [(1, -1, -1), (-1, -1, -1), (-1, 1, 1), (1, 1, 1)],
        ];
        for quad in quads {
            let corners = quad.map(|(x, y, z)| corner(x as f32, y as f32, z as f32));
            mesh.push_face(&corners, &SQUARE_UVS);
        }
        // The two triangular sides, with uvs along Z and Y.
        let uv = |p: Vec3| Vec2::new(p.z / size.z + 0.5, p.y / size.y + 0.5);
        for (x, order) in [(1.0, [0, 1, 2]), (-1.0, [0, 2, 1])] {
            let side = [
                corner(x, -1.0, -1.0),
                corner(x, 1.0, 1.0),
                corner(x, -1.0, 1.0),
            ];
            let corners = order.map(|k| side[k]);
            mesh.push_face(&corners, &corners.map(uv));
        }
        mesh
    }

    /// A sphere as wide as the smallest side of `size`, centered on the
    /// origin, with smooth normals.
    pub fn ball(size: Vec3) -> Mesh {
        let radius = size.min_element() / 2.0;
        let mut mesh = Mesh::default();
        // A grid of corners, rings from the top (+Y) down. The last column
        // repeats the first's positions (with other uvs), and every corner
        // of the top and bottom rings is exactly the pole.
        for ring in 0..=BALL_RINGS {
            let down = PI * ring as f32 / BALL_RINGS as f32;
            for segment in 0..=BALL_SEGMENTS {
                let around = TAU * (segment % BALL_SEGMENTS) as f32 / BALL_SEGMENTS as f32;
                let normal = match ring {
                    0 => Vec3::Y,
                    BALL_RINGS => Vec3::NEG_Y,
                    _ => Vec3::new(
                        down.sin() * around.cos(),
                        down.cos(),
                        down.sin() * around.sin(),
                    ),
                };
                let uv = Vec2::new(
                    segment as f32 / BALL_SEGMENTS as f32,
                    ring as f32 / BALL_RINGS as f32,
                );
                mesh.push_vertex(normal * radius, normal, uv);
            }
        }
        let row = BALL_SEGMENTS + 1;
        for ring in 0..BALL_RINGS {
            for segment in 0..BALL_SEGMENTS {
                let top = ring * row + segment;
                let bottom = top + row;
                // At the poles one triangle of each square has no area.
                if ring > 0 {
                    mesh.push_triangle([top, top + 1, bottom + 1]);
                }
                if ring < BALL_RINGS - 1 {
                    mesh.push_triangle([top, bottom + 1, bottom]);
                }
            }
        }
        mesh
    }

    /// A cylinder along X, as long as `size.x` and as wide as the smaller of
    /// `size.y` and `size.z`, centered on the origin. Smooth sides, flat ends.
    pub fn cylinder(size: Vec3) -> Mesh {
        let length = size.x / 2.0;
        let radius = size.y.min(size.z) / 2.0;
        let rim = |segment: u32| {
            let angle = TAU * (segment % CYLINDER_SIDES) as f32 / CYLINDER_SIDES as f32;
            Vec3::new(0.0, angle.cos(), angle.sin())
        };
        let mut mesh = Mesh::default();
        // The side: a pair of corners per slice, one at each end.
        for segment in 0..=CYLINDER_SIDES {
            let normal = rim(segment);
            let u = segment as f32 / CYLINDER_SIDES as f32;
            for (x, v) in [(-length, 0.0), (length, 1.0)] {
                let position = normal * radius + Vec3::X * x;
                mesh.push_vertex(position, normal, Vec2::new(u, v));
            }
        }
        for segment in 0..CYLINDER_SIDES {
            let left = segment * 2;
            let (right, next_left, next_right) = (left + 1, left + 2, left + 3);
            mesh.push_triangle([left, next_left, next_right]);
            mesh.push_triangle([left, next_right, right]);
        }
        // The ends, each a fan around its middle.
        for end in [1.0, -1.0] {
            let normal = Vec3::X * end;
            let uv = |p: Vec3| Vec2::new(p.y / (2.0 * radius) + 0.5, p.z / (2.0 * radius) + 0.5);
            let middle = normal * length;
            let center = mesh.push_vertex(middle, normal, uv(middle));
            let first = center + 1;
            for segment in 0..CYLINDER_SIDES {
                let position = rim(segment) * radius + middle;
                mesh.push_vertex(position, normal, uv(position));
            }
            for segment in 0..CYLINDER_SIDES {
                let a = first + segment;
                let b = first + (segment + 1) % CYLINDER_SIDES;
                // Seen from each end, the rim runs the other way round.
                if end > 0.0 {
                    mesh.push_triangle([center, a, b]);
                } else {
                    mesh.push_triangle([center, b, a]);
                }
            }
        }
        mesh
    }

    /// A loaded model's triangles in its file's units. Loading fits a model
    /// into the 1×1×1 cube, so this undoes that: scaled back by
    /// [`Model::size`] (except on a flat axis, which wasn't scaled) and moved
    /// back to [`Model::center`]. Primitive `i`'s triangles are `Outer(i)`.
    /// Skinning and simpler copies (levels of detail) are left out.
    pub fn from_model(model: &Model) -> Mesh {
        let size = Vec3::from(model.size);
        let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
        let center = Vec3::from(model.center);
        let mut mesh = Mesh::default();
        for (index, primitive) in model.primitives.iter().enumerate() {
            let first = mesh.positions.len() as u32;
            for (k, &position) in primitive.positions.iter().enumerate() {
                let position = Vec3::from(position) * scale + center;
                // Stretching tips normals the other way, by the inverse.
                let normal = primitive.normals.get(k).map_or(Vec3::ZERO, |&normal| {
                    (Vec3::from(normal) / scale).normalize_or_zero()
                });
                let uv = primitive
                    .uvs
                    .get(k)
                    .map_or(Vec2::ZERO, |&uv| Vec2::from(uv));
                mesh.push_vertex(position, normal, uv);
            }
            let count = primitive.positions.len() as u32;
            for &triangle in primitive.indices.as_chunks::<3>().0 {
                if triangle.iter().any(|&corner| corner >= count) {
                    continue;
                }
                mesh.triangles.push(Triangle {
                    corners: triangle.map(|corner| first + corner),
                    surface: Surface::Outer(index as u32),
                });
            }
        }
        mesh
    }

    /// Adds a flat convex face through `corners` (counter-clockwise seen from
    /// outside), with its own vertices.
    fn push_face(&mut self, corners: &[Vec3], uvs: &[Vec2]) {
        // Newell's method: the normal of any flat polygon, however its
        // corners are spaced. (a - b) × (a + b) is 2 a × b.
        let mut normal = Vec3::ZERO;
        for (k, &a) in corners.iter().enumerate() {
            let b = corners[(k + 1) % corners.len()];
            normal += (a - b).cross(a + b);
        }
        let normal = normal.normalize_or_zero();
        let first = self.positions.len() as u32;
        for (&corner, &uv) in corners.iter().zip(uvs) {
            self.push_vertex(corner, normal, uv);
        }
        for k in 1..corners.len() as u32 - 1 {
            self.push_triangle([first, first + k, first + k + 1]);
        }
    }

    fn push_triangle(&mut self, corners: [u32; 3]) {
        self.triangles.push(Triangle {
            corners,
            surface: OUTER,
        });
    }
}

/// A four-cornered face's uvs, from 0 to 1 across it.
const SQUARE_UVS: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(0.0, 1.0),
];
