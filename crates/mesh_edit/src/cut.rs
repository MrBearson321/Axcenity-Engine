//! Cutting a mesh in two along a plane, and closing the holes that leaves.
//!
//! Each triangle goes to the side its corners are on. One that crosses the
//! plane is split where its edges cross it, into a triangle on one side and
//! a four-cornered piece (two triangles) on the other. That leaves each half
//! open where the cut went through. The opening is the edges only one
//! triangle of the half uses, all lying on the plane; they're chained into
//! loops and filled with Interior triangles by ear clipping. A loop inside
//! another (a ring cut across) is a hole in the cap, joined to the loop
//! around it by a bridge first so one polygon covers both.
//!
//! Vertices may be split (the same position with different normals or
//! uvs), so edges are matched by position. The point where an edge crosses
//! the plane is worked out from its ends in a fixed order, so the two
//! triangles beside the edge get exactly the same point and still match.

use std::cmp::Ordering;
use std::collections::HashMap;

use glam::{DVec2, Vec2, Vec3};

use crate::fill::{chain, fill};
use crate::{Mesh, Plane, Surface, Triangle, position_key};

/// Corners this close to the plane, as a fraction of the mesh's size, count
/// as on it. Cutting right next to a corner would make needle-thin triangles.
const ON_PLANE: f32 = 1e-5;

/// A half whose volume is at most this fraction of the whole is a sliver,
/// and is left out.
const SLIVER: f32 = 1e-6;

impl Mesh {
    /// Cuts along `plane`: (below, above). Each side is closed: the hole the
    /// cut leaves is filled with Interior triangles. `None` for a side with
    /// nothing (or only slivers of zero volume); the other side is then the
    /// whole mesh, unchanged.
    pub fn cut(&self, plane: Plane) -> (Option<Mesh>, Option<Mesh>) {
        let across = match self.split(plane) {
            Cut::Nothing => return (None, None),
            Cut::Below => return (Some(self.clone()), None),
            Cut::Above => return (None, Some(self.clone())),
            Cut::Across(across) => across,
        };
        let below = across.side(Side::Below);
        let above = across.side(Side::Above);
        let whole = self.volume().abs();
        match (is_sliver(&below, whole), is_sliver(&above, whole)) {
            (false, false) => (Some(below), Some(above)),
            (true, false) => (None, Some(self.clone())),
            (false, true) => (Some(self.clone()), None),
            (true, true) => (None, None),
        }
    }

    /// The side of [`Mesh::cut`] below `plane`, without the work of closing
    /// the side above. A closed mesh's halves add up to the whole, so the
    /// side above is a sliver when this side is nearly all of it.
    pub(crate) fn cut_below(&self, plane: Plane) -> Option<Mesh> {
        let across = match self.split(plane) {
            Cut::Nothing | Cut::Above => return None,
            Cut::Below => return Some(self.clone()),
            Cut::Across(across) => across,
        };
        let below = across.side(Side::Below);
        let whole = self.volume();
        if is_sliver(&below, whole.abs()) {
            None
        } else if whole > 0.0 && whole - below.volume() <= SLIVER * whole {
            Some(self.clone())
        } else {
            Some(below)
        }
    }

    /// Sorts the triangles to the sides of `plane`, splitting the ones
    /// across it.
    fn split(&self, plane: Plane) -> Cut {
        let length = plane.normal.length();
        if self.is_empty() {
            return Cut::Nothing;
        }
        if length <= 0.0 || !length.is_finite() {
            return Cut::Below;
        }
        let normal = plane.normal / length;
        let offset = plane.offset / length;
        let (min, max) = self.bounds();
        // Far from the origin, f32 positions themselves are less precise.
        let tolerance = ON_PLANE * (max - min).max_element()
            + 8.0 * f32::EPSILON * min.abs().max(max.abs()).max_element();
        let distances: Vec<f32> = self
            .positions
            .iter()
            .map(|&p| {
                let distance = normal.dot(p) - offset;
                if distance.abs() <= tolerance {
                    0.0
                } else {
                    distance
                }
            })
            .collect();

        let used = || {
            self.triangles
                .iter()
                .flat_map(|triangle| triangle.corners)
                .map(|corner| distances[corner as usize])
        };
        if !used().any(|distance| distance > 0.0) {
            return Cut::Below;
        }
        if !used().any(|distance| distance < 0.0) {
            return Cut::Above;
        }

        let mut split = Split {
            mesh: self.clone(),
            distances,
            crossings: HashMap::new(),
        };
        let (mut below, mut above) = (Vec::new(), Vec::new());
        for triangle in &self.triangles {
            split.triangle(*triangle, normal, &mut below, &mut above);
        }
        split.weld(tolerance);
        Cut::Across(Box::new(Across {
            split,
            normal,
            below,
            above,
        }))
    }
}

/// True if `side` of a mesh enclosing `whole` is nothing, or a sliver of
/// it. A mesh with no volume (a flat sheet) has no slivers, only sides
/// with triangles and sides without.
fn is_sliver(side: &Mesh, whole: f32) -> bool {
    side.is_empty() || (whole > 0.0 && side.volume().abs() <= SLIVER * whole)
}

/// Where a mesh is from a plane.
enum Cut {
    /// It has no triangles.
    Nothing,
    /// All of it is below (or on) the plane.
    Below,
    /// All of it is above.
    Above,
    /// The plane goes through it.
    Across(Box<Across>),
}

enum Side {
    Below,
    Above,
}

/// A mesh split by a plane, each triangle on one side of it.
struct Across {
    split: Split,
    /// The plane's normal, one unit long.
    normal: Vec3,
    below: Vec<Triangle>,
    above: Vec<Triangle>,
}

impl Across {
    /// One side, closed.
    fn side(&self, side: Side) -> Mesh {
        match side {
            Side::Below => self.split.side(&self.below, self.normal),
            Side::Above => self.split.side(&self.above, -self.normal),
        }
    }
}

/// A mesh being cut: its vertices plus the points where edges cross the
/// plane, which both halves share until [`Split::side`] copies each out.
struct Split {
    mesh: Mesh,
    /// How far above the plane each vertex is; 0 on it.
    distances: Vec<f32>,
    /// The crossing point already made on each edge, by its corners'
    /// indices (smaller first).
    crossings: HashMap<(u32, u32), u32>,
}

impl Split {
    /// Moves corners on the plane that are within `tolerance` of each other
    /// to one place. Where planes meet almost at a point, two edges along
    /// one line can cross the plane a hair apart (rounding), which would
    /// leave needle-thin triangles between them. Each keeps its own normal
    /// and uvs; a triangle left with two corners together is dropped later.
    fn weld(&mut self, tolerance: f32) {
        let mut on_plane: Vec<u32> = (0..self.distances.len() as u32)
            .filter(|&vertex| self.distances[vertex as usize] == 0.0)
            .collect();
        let positions = &mut self.mesh.positions;
        on_plane.sort_by(|&a, &b| {
            compare(positions[a as usize], positions[b as usize]).then(a.cmp(&b))
        });
        // Union-find: each corner points toward the first (smallest index)
        // of the corners it's welded to.
        let mut leader: HashMap<u32, u32> = HashMap::new();
        fn find(leader: &HashMap<u32, u32>, mut at: u32) -> u32 {
            while let Some(&next) = leader.get(&at) {
                if next == at {
                    break;
                }
                at = next;
            }
            at
        }
        for (k, &a) in on_plane.iter().enumerate() {
            // Sorted by x, so only the next few can be close enough.
            for &b in &on_plane[k + 1..] {
                let (pa, pb) = (positions[a as usize], positions[b as usize]);
                if pb.x - pa.x > tolerance {
                    break;
                }
                if pa != pb && pa.distance(pb) <= tolerance {
                    let (ra, rb) = (find(&leader, a), find(&leader, b));
                    leader.insert(ra.max(rb), ra.min(rb));
                }
            }
        }
        for &vertex in &on_plane {
            let first = find(&leader, vertex);
            positions[vertex as usize] = positions[first as usize];
        }
    }

    /// Sends `triangle` to the side it's on, or splits it between them.
    fn triangle(
        &mut self,
        triangle: Triangle,
        normal: Vec3,
        below: &mut Vec<Triangle>,
        above: &mut Vec<Triangle>,
    ) {
        let corners = triangle.corners;
        let distances = corners.map(|corner| self.distances[corner as usize]);
        let has_below = distances.iter().any(|&d| d < 0.0);
        let has_above = distances.iter().any(|&d| d > 0.0);
        match (has_below, has_above) {
            (true, false) => below.push(triangle),
            (false, true) => above.push(triangle),
            // Lying in the plane: it closes the side it faces away from.
            (false, false) => {
                let [a, b, c] = corners.map(|corner| self.mesh.positions[corner as usize]);
                if (b - a).cross(c - a).dot(normal) >= 0.0 {
                    below.push(triangle);
                } else {
                    above.push(triangle);
                }
            }
            (true, true) => {
                // Walk round the corners, keeping each on its side (both
                // when it's on the plane) and adding a crossing point
                // wherever an edge goes from one side to the other.
                let mut below_corners = Vec::with_capacity(4);
                let mut above_corners = Vec::with_capacity(4);
                for k in 0..3 {
                    let (a, b) = (corners[k], corners[(k + 1) % 3]);
                    let (da, db) = (distances[k], distances[(k + 1) % 3]);
                    if da <= 0.0 {
                        below_corners.push(a);
                    }
                    if da >= 0.0 {
                        above_corners.push(a);
                    }
                    if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
                        let crossing = self.crossing(a, b);
                        below_corners.push(crossing);
                        above_corners.push(crossing);
                    }
                }
                fan(&below_corners, triangle.surface, below);
                fan(&above_corners, triangle.surface, above);
            }
        }
    }

    /// The vertex where the edge from `a` to `b` crosses the plane, made the
    /// first time it's asked for.
    fn crossing(&mut self, a: u32, b: u32) -> u32 {
        let key = (a.min(b), a.max(b));
        if let Some(&crossing) = self.crossings.get(&key) {
            return crossing;
        }
        // Start from the end with the smaller position, so a split vertex's
        // twin on the other triangle gives exactly the same point.
        let mesh = &self.mesh;
        let (from, to) = if compare(mesh.positions[a as usize], mesh.positions[b as usize])
            == Ordering::Greater
        {
            (b as usize, a as usize)
        } else {
            (a as usize, b as usize)
        };
        let (d_from, d_to) = (self.distances[from], self.distances[to]);
        let t = d_from / (d_from - d_to);
        let position = mesh.positions[from] + (mesh.positions[to] - mesh.positions[from]) * t;
        // A flat face's corners share one normal: keep it exactly, so the
        // face's triangles can still be told apart from their neighbours
        // (see `merge`).
        let normal = if mesh.normals[from] == mesh.normals[to] {
            mesh.normals[from]
        } else {
            mesh.normals[from]
                .lerp(mesh.normals[to], t)
                .normalize_or(mesh.normals[from])
        };
        let uv = mesh.uvs[from].lerp(mesh.uvs[to], t);
        let crossing = self.mesh.push_vertex(position, normal, uv);
        self.distances.push(0.0);
        self.crossings.insert(key, crossing);
        crossing
    }

    /// One closed half: `triangles` with only the vertices they use, and a
    /// cap facing `outward` over the opening.
    fn side(&self, triangles: &[Triangle], outward: Vec3) -> Mesh {
        let mut mesh = Mesh::default();
        let mut on_plane = Vec::new();
        let mut index = vec![u32::MAX; self.mesh.positions.len()];
        for triangle in triangles {
            // A flat triangle (three corners in a line, as a cap can have)
            // split near its middle gives one with two corners in the same
            // place. It covers nothing, and its two edges cancel out, so
            // it's left out.
            let [a, b, c] = triangle
                .corners
                .map(|corner| self.mesh.positions[corner as usize]);
            if a == b || b == c || c == a {
                continue;
            }
            let corners = triangle.corners.map(|corner| {
                let corner = corner as usize;
                if index[corner] == u32::MAX {
                    index[corner] = mesh.push_vertex(
                        self.mesh.positions[corner],
                        self.mesh.normals[corner],
                        self.mesh.uvs[corner],
                    );
                    on_plane.push(self.distances[corner] == 0.0);
                }
                index[corner]
            });
            mesh.triangles.push(Triangle {
                corners,
                surface: triangle.surface,
            });
        }
        cap(&mut mesh, &on_plane, outward);
        crate::merge::merge_flat_faces(&mesh, &|_| false)
    }
}

/// Orders positions by x, then y, then z.
fn compare(a: Vec3, b: Vec3) -> Ordering {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
}

/// Triangles fanning out from the first of `corners` (a convex polygon of
/// three or four).
fn fan(corners: &[u32], surface: Surface, triangles: &mut Vec<Triangle>) {
    for k in 1..corners.len().saturating_sub(1) {
        triangles.push(Triangle {
            corners: [corners[0], corners[k], corners[k + 1]],
            surface,
        });
    }
}

/// Closes the opening in `mesh` that lies in the plane facing `outward`:
/// the edges only one of its triangles uses, between vertices `on_plane`.
fn cap(mesh: &mut Mesh, on_plane: &[bool], outward: Vec3) {
    // Number the distinct positions, in the order they're first used.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
    let mut points: Vec<Vec3> = Vec::new();
    let mut point_on_plane: Vec<bool> = Vec::new();
    let mut point_of = Vec::with_capacity(mesh.positions.len());
    for (vertex, &position) in mesh.positions.iter().enumerate() {
        let id = *ids.entry(position_key(position)).or_insert_with(|| {
            points.push(position);
            point_on_plane.push(false);
            (points.len() - 1) as u32
        });
        point_on_plane[id as usize] |= on_plane[vertex];
        point_of.push(id);
    }
    let edges_of = |triangle: &Triangle| {
        let [a, b, c] = triangle.corners.map(|corner| point_of[corner as usize]);
        [(a, b), (b, c), (c, a)]
    };

    // An edge used one way more often than the other is on the opening.
    let mut surplus: HashMap<(u32, u32), i32> = HashMap::new();
    for (a, b) in mesh.triangles.iter().flat_map(edges_of) {
        if a != b {
            *surplus.entry((a, b)).or_default() += 1;
            *surplus.entry((b, a)).or_default() -= 1;
        }
    }
    let mut open = Vec::new();
    for (a, b) in mesh.triangles.iter().flat_map(edges_of) {
        if a == b || !point_on_plane[a as usize] || !point_on_plane[b as usize] {
            continue;
        }
        if let Some(count) = surplus.get_mut(&(a, b))
            && *count > 0
        {
            *count -= 1;
            open.push((a, b));
        }
    }
    if open.is_empty() {
        return;
    }

    // The cap runs the other way round each loop, so it faces outward.
    let loops: Vec<Vec<u32>> = chain(&open)
        .into_iter()
        .map(|mut lap| {
            lap.reverse();
            lap
        })
        .collect();
    let u = outward.any_orthonormal_vector();
    let v = outward.cross(u);
    let origin = points[open[0].0 as usize].as_dvec3();
    let flat: Vec<DVec2> = points
        .iter()
        .map(|p| {
            let p = p.as_dvec3() - origin;
            DVec2::new(p.dot(u.as_dvec3()), p.dot(v.as_dvec3()))
        })
        .collect();

    // Each cap corner gets its own vertex, flat-shaded with planar uvs.
    let mut cap_vertex: HashMap<u32, u32> = HashMap::new();
    for triangle in fill(&flat, loops) {
        let corners = triangle.map(|point| {
            *cap_vertex.entry(point).or_insert_with(|| {
                let position = points[point as usize];
                let uv = Vec2::new(position.dot(u), position.dot(v));
                mesh.push_vertex(position, outward, uv)
            })
        });
        mesh.triangles.push(Triangle {
            corners,
            surface: Surface::Interior,
        });
    }
}
