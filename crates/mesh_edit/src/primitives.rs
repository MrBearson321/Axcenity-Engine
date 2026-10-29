//! The Model Studio's New shapes (Phase 55): a cube, plane, UV sphere, ico
//! sphere, cylinder, cone and torus, each with its segments and sizes, as
//! [`EditMesh`]es. Y is up, and each sits centred on the origin.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use crate::half_edge::{EditMesh, Polygon};

/// The shapes New makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimitiveShape {
    #[default]
    Cube,
    Plane,
    UvSphere,
    IcoSphere,
    Cylinder,
    Cone,
    Torus,
}

impl PrimitiveShape {
    pub const ALL: [PrimitiveShape; 7] = [
        PrimitiveShape::Cube,
        PrimitiveShape::Plane,
        PrimitiveShape::UvSphere,
        PrimitiveShape::IcoSphere,
        PrimitiveShape::Cylinder,
        PrimitiveShape::Cone,
        PrimitiveShape::Torus,
    ];

    /// Its name, which a new model is named after.
    pub fn label(self) -> &'static str {
        match self {
            PrimitiveShape::Cube => "Cube",
            PrimitiveShape::Plane => "Plane",
            PrimitiveShape::UvSphere => "UV Sphere",
            PrimitiveShape::IcoSphere => "Ico Sphere",
            PrimitiveShape::Cylinder => "Cylinder",
            PrimitiveShape::Cone => "Cone",
            PrimitiveShape::Torus => "Torus",
        }
    }

    /// Which settings it uses, besides `size`: (height, thickness,
    /// segments, rings, subdivisions).
    pub fn uses(self) -> (bool, bool, bool, bool, bool) {
        match self {
            PrimitiveShape::Cube | PrimitiveShape::Plane => (false, false, true, false, false),
            PrimitiveShape::UvSphere => (false, false, true, true, false),
            PrimitiveShape::IcoSphere => (false, false, false, false, true),
            PrimitiveShape::Cylinder | PrimitiveShape::Cone => (true, false, true, true, false),
            PrimitiveShape::Torus => (false, true, true, true, false),
        }
    }
}

/// A shape and its sizes, in studs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrimitiveSettings {
    pub shape: PrimitiveShape,
    /// A cube's or plane's side, or a sphere's, cylinder's, cone's or
    /// torus's width.
    pub size: f32,
    /// A cylinder's or cone's height.
    pub height: f32,
    /// How thick a torus's ring is.
    pub thickness: f32,
    /// Cuts along each side of a cube or plane, or around a sphere,
    /// cylinder, cone or torus.
    pub segments: u32,
    /// A UV sphere's rings from pole to pole, a cylinder's or cone's cuts
    /// up its side, or the steps around a torus's ring.
    pub rings: u32,
    /// How many times an ico sphere's 20 triangles are split in four.
    pub subdivisions: u32,
}

impl Default for PrimitiveSettings {
    fn default() -> Self {
        Self::of(PrimitiveShape::Cube)
    }
}

impl PrimitiveSettings {
    /// `shape`'s usual settings, like Blender's.
    pub fn of(shape: PrimitiveShape) -> Self {
        let (segments, rings) = match shape {
            PrimitiveShape::Cube | PrimitiveShape::Plane => (1, 1),
            PrimitiveShape::UvSphere => (32, 16),
            PrimitiveShape::IcoSphere => (1, 1),
            PrimitiveShape::Cylinder | PrimitiveShape::Cone => (32, 1),
            PrimitiveShape::Torus => (48, 12),
        };
        Self {
            shape,
            size: 2.0,
            height: 2.0,
            thickness: 0.5,
            segments,
            rings,
            subdivisions: 2,
        }
    }

    /// The shape as a mesh. Settings out of range are brought into it.
    pub fn build(&self) -> EditMesh {
        let size = self.size.max(0.01);
        let height = self.height.max(0.01);
        let segments = self.segments.clamp(1, 256);
        let around = self.segments.clamp(3, 256);
        let rings = self.rings.clamp(1, 256);
        let mut builder = Builder::default();
        match self.shape {
            PrimitiveShape::Cube => builder.cube(size, segments),
            PrimitiveShape::Plane => builder.plane(size, segments),
            PrimitiveShape::UvSphere => builder.uv_sphere(size / 2.0, around, rings.max(2)),
            PrimitiveShape::IcoSphere => builder.ico_sphere(size / 2.0, self.subdivisions.min(5)),
            PrimitiveShape::Cylinder => {
                builder.cylinder(size / 2.0, size / 2.0, height, around, rings)
            }
            PrimitiveShape::Cone => builder.cylinder(size / 2.0, 0.0, height, around, rings),
            PrimitiveShape::Torus => builder.torus(
                size / 2.0,
                (self.thickness.max(0.01) / 2.0).min(size / 2.0),
                around,
                rings.max(3),
            ),
        }
        builder.finish()
    }
}

/// Collects faces, sharing a vertex between every corner at one place.
#[derive(Default)]
struct Builder {
    positions: Vec<Vec3>,
    polygons: Vec<Polygon>,
    at: HashMap<[i64; 3], u32>,
}

impl Builder {
    /// The vertex at `point`, made the first time. Points closer than a
    /// hundred-thousandth of a stud are the same place, so the corners of
    /// a seam or a pole meet whatever `sin` and `cos` round to.
    fn vertex(&mut self, point: Vec3) -> u32 {
        let key = (point * 1e5).round().as_i64vec3().to_array();
        *self.at.entry(key).or_insert_with(|| {
            self.positions.push(point);
            (self.positions.len() - 1) as u32
        })
    }

    /// Adds a face through `points` (counter-clockwise seen from outside).
    /// Corners at one place are merged, so a quad at a pole is a triangle.
    fn face(&mut self, points: &[Vec3], uvs: &[Vec2], smooth: bool) {
        let mut corners = Vec::new();
        let mut corner_uvs = Vec::new();
        for (&point, &uv) in points.iter().zip(uvs) {
            let vertex = self.vertex(point);
            if corners.last() != Some(&vertex) && corners.first() != Some(&vertex) {
                corners.push(vertex);
                corner_uvs.push(uv);
            }
        }
        self.polygons.push(Polygon {
            corners,
            uvs: corner_uvs,
            normals: Vec::new(),
            material: 0,
            smooth,
        });
    }

    fn finish(self) -> EditMesh {
        EditMesh::new(self.positions, &self.polygons)
    }

    /// A grid of `cuts` × `cuts` quads on a square: corner `origin`, sides
    /// along `u` and `v` (so it faces `u × v`).
    fn grid(&mut self, origin: Vec3, u: Vec3, v: Vec3, cuts: u32) {
        let step = 1.0 / cuts as f32;
        for i in 0..cuts {
            for j in 0..cuts {
                let (a, b) = (i as f32 * step, (i + 1) as f32 * step);
                let (c, d) = (j as f32 * step, (j + 1) as f32 * step);
                let corner = |s: f32, t: f32| origin + u * s + v * t;
                self.face(
                    &[corner(a, c), corner(b, c), corner(b, d), corner(a, d)],
                    &[
                        Vec2::new(a, 1.0 - c),
                        Vec2::new(b, 1.0 - c),
                        Vec2::new(b, 1.0 - d),
                        Vec2::new(a, 1.0 - d),
                    ],
                    false,
                );
            }
        }
    }

    fn cube(&mut self, size: f32, cuts: u32) {
        let h = size / 2.0;
        // Each face: its corner at the start of u and v, with u × v outward.
        let faces = [
            (
                Vec3::new(h, -h, h),
                Vec3::new(0.0, 0.0, -size),
                Vec3::new(0.0, size, 0.0),
            ),
            (
                Vec3::new(-h, -h, -h),
                Vec3::new(0.0, 0.0, size),
                Vec3::new(0.0, size, 0.0),
            ),
            (
                Vec3::new(-h, h, h),
                Vec3::new(size, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -size),
            ),
            (
                Vec3::new(-h, -h, -h),
                Vec3::new(size, 0.0, 0.0),
                Vec3::new(0.0, 0.0, size),
            ),
            (
                Vec3::new(-h, -h, h),
                Vec3::new(size, 0.0, 0.0),
                Vec3::new(0.0, size, 0.0),
            ),
            (
                Vec3::new(h, -h, -h),
                Vec3::new(-size, 0.0, 0.0),
                Vec3::new(0.0, size, 0.0),
            ),
        ];
        for (origin, u, v) in faces {
            self.grid(origin, u, v, cuts);
        }
    }

    fn plane(&mut self, size: f32, cuts: u32) {
        let h = size / 2.0;
        self.grid(
            Vec3::new(-h, 0.0, h),
            Vec3::new(size, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -size),
            cuts,
        );
    }

    fn uv_sphere(&mut self, radius: f32, around: u32, rings: u32) {
        let point = |i: u32, j: u32| {
            let polar = PI * i as f32 / rings as f32;
            let azimuth = TAU * j as f32 / around as f32;
            Vec3::new(
                polar.sin() * azimuth.cos(),
                polar.cos(),
                -polar.sin() * azimuth.sin(),
            ) * radius
        };
        let uv = |i: u32, j: u32| Vec2::new(j as f32 / around as f32, i as f32 / rings as f32);
        for i in 0..rings {
            for j in 0..around {
                self.face(
                    &[
                        point(i, j),
                        point(i + 1, j),
                        point(i + 1, j + 1),
                        point(i, j + 1),
                    ],
                    &[uv(i, j), uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1)],
                    true,
                );
            }
        }
    }

    fn ico_sphere(&mut self, radius: f32, subdivisions: u32) {
        // The 12 corners of an icosahedron: three golden rectangles.
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let corners = [
            Vec3::new(-1.0, t, 0.0),
            Vec3::new(1.0, t, 0.0),
            Vec3::new(-1.0, -t, 0.0),
            Vec3::new(1.0, -t, 0.0),
            Vec3::new(0.0, -1.0, t),
            Vec3::new(0.0, 1.0, t),
            Vec3::new(0.0, -1.0, -t),
            Vec3::new(0.0, 1.0, -t),
            Vec3::new(t, 0.0, -1.0),
            Vec3::new(t, 0.0, 1.0),
            Vec3::new(-t, 0.0, -1.0),
            Vec3::new(-t, 0.0, 1.0),
        ]
        .map(Vec3::normalize);
        let mut triangles: Vec<[Vec3; 3]> = [
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ]
        .iter()
        .map(|t: &[usize; 3]| t.map(|k| corners[k]))
        .collect();
        for _ in 0..subdivisions {
            triangles = triangles
                .into_iter()
                .flat_map(|[a, b, c]| {
                    let ab = (a + b).normalize();
                    let bc = (b + c).normalize();
                    let ca = (c + a).normalize();
                    [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]
                })
                .collect();
        }
        for triangle in triangles {
            // Wrapped around like a globe; a triangle across the back seam
            // takes its u past 1 rather than back to 0.
            let mut uvs =
                triangle.map(|p| Vec2::new(0.5 + (-p.z).atan2(p.x) / TAU, 0.5 - p.y.asin() / PI));
            let max = uvs.iter().map(|uv| uv.x).fold(0.0, f32::max);
            for uv in &mut uvs {
                if max - uv.x > 0.5 {
                    uv.x += 1.0;
                }
            }
            self.face(&triangle.map(|p| p * radius), &uvs, true);
        }
    }

    /// A cylinder, or a cone when `top` is 0: `rings` bands of quads up
    /// the side (smooth) and a flat many-sided face on each end.
    fn cylinder(&mut self, bottom: f32, top: f32, height: f32, around: u32, rings: u32) {
        let h = height / 2.0;
        let circle = |j: u32, radius: f32, y: f32| {
            let azimuth = TAU * j as f32 / around as f32;
            Vec3::new(azimuth.cos() * radius, y, -azimuth.sin() * radius)
        };
        let ring = |i: u32| {
            let along = i as f32 / rings as f32;
            (bottom + (top - bottom) * along, -h + height * along)
        };
        for i in 0..rings {
            let ((r0, y0), (r1, y1)) = (ring(i), ring(i + 1));
            for j in 0..around {
                let (u0, u1) = (j as f32 / around as f32, (j + 1) as f32 / around as f32);
                let (v0, v1) = (
                    1.0 - i as f32 / rings as f32,
                    1.0 - (i + 1) as f32 / rings as f32,
                );
                self.face(
                    &[
                        circle(j, r0, y0),
                        circle(j + 1, r0, y0),
                        circle(j + 1, r1, y1),
                        circle(j, r1, y1),
                    ],
                    &[
                        Vec2::new(u0, v0),
                        Vec2::new(u1, v0),
                        Vec2::new(u1, v1),
                        Vec2::new(u0, v1),
                    ],
                    true,
                );
            }
        }
        let cap_uv = |p: Vec3, radius: f32| {
            Vec2::new(0.5 + p.x / (2.0 * radius), 0.5 + p.z / (2.0 * radius))
        };
        // The bottom, seen from below, goes the other way round.
        let points: Vec<Vec3> = (0..around).rev().map(|j| circle(j, bottom, -h)).collect();
        let uvs: Vec<Vec2> = points.iter().map(|&p| cap_uv(p, bottom)).collect();
        self.face(&points, &uvs, false);
        if top > 0.0 {
            let points: Vec<Vec3> = (0..around).map(|j| circle(j, top, h)).collect();
            let uvs: Vec<Vec2> = points.iter().map(|&p| cap_uv(p, top)).collect();
            self.face(&points, &uvs, false);
        }
    }

    fn torus(&mut self, major: f32, minor: f32, around: u32, steps: u32) {
        let point = |j: u32, k: u32| {
            let azimuth = TAU * j as f32 / around as f32;
            let turn = TAU * k as f32 / steps as f32;
            let out = Vec3::new(azimuth.cos(), 0.0, -azimuth.sin());
            out * (major + minor * turn.cos()) + Vec3::Y * minor * turn.sin()
        };
        let uv = |j: u32, k: u32| Vec2::new(j as f32 / around as f32, k as f32 / steps as f32);
        for j in 0..around {
            for k in 0..steps {
                self.face(
                    &[
                        point(j, k),
                        point(j + 1, k),
                        point(j + 1, k + 1),
                        point(j, k + 1),
                    ],
                    &[uv(j, k), uv(j + 1, k), uv(j + 1, k + 1), uv(j, k + 1)],
                    true,
                );
            }
        }
    }
}
