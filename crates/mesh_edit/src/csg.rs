//! Solid modeling: the union, difference and intersection of closed meshes
//! (Phase 50), with a BSP tree of each.
//!
//! Each mesh becomes a tree of planes: a polygon's plane splits space, the
//! polygons in front of it go down one branch and the ones behind it down
//! the other. Clipping one mesh's polygons by the other's tree keeps only
//! the pieces outside (or, inverted, inside) it, and the operations are
//! those clips put together (Evan Wallace's csg.js does the same).
//!
//! Splitting leaves corners in the middle of a neighbour's edge (a
//! T-junction), and the two sides of a split edge can work out its point a
//! hair apart. So afterwards corners that close are made one, each edge
//! gets the corners lying on it, and each polygon is ear-clipped into
//! triangles: the result is closed, so it can be the input of the next
//! operation. Flat faces are then filled again without their needless
//! corners, as after a cut.
//!
//! The work is in f64 with the meshes moved and scaled into a unit box, so
//! one tolerance fits a pebble and a castle alike.

use std::collections::HashMap;

use glam::{DVec2, DVec3};

use crate::fill::fill;
use crate::merge::merge_flat_faces;
use crate::{Mesh, Surface, Triangle};

/// Points this close to a plane, in the unit box, count as on it.
const ON_PLANE: f64 = 1e-6;

/// Corners this close together, in the unit box, are one corner.
const SAME_POINT: f64 = 1e-5;

/// Which solid to make of two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Everything inside either.
    Union,
    /// Everything inside the first and outside the second.
    Subtract,
    /// Everything inside both.
    Intersect,
}

impl Mesh {
    /// The number of edges (matched by position) without exactly one
    /// partner running the other way. 0 for a closed mesh whose triangles
    /// all face out (or all in).
    pub fn open_edges(&self) -> usize {
        let key = |corner: u32| crate::position_key(self.positions[corner as usize]);
        let mut edges: HashMap<([u32; 3], [u32; 3]), i32> = HashMap::new();
        for triangle in &self.triangles {
            let [a, b, c] = triangle.corners.map(key);
            for (from, to) in [(a, b), (b, c), (c, a)] {
                if from != to {
                    *edges.entry((from, to)).or_default() += 1;
                }
            }
        }
        edges
            .iter()
            .filter(|&(&(from, to), &count)| count != 1 || edges.get(&(to, from)) != Some(&1))
            .count()
    }
}

/// `a` and `b` combined by `operation`. Each must be closed (see
/// [`Mesh::open_edges`]); the result is too, with every triangle keeping
/// the surface of the one it came from. An empty mesh when nothing is left
/// (two that don't touch, intersected).
pub fn boolean(a: &Mesh, b: &Mesh, operation: Operation) -> Mesh {
    let (min_a, max_a) = a.bounds();
    let (min_b, max_b) = b.bounds();
    let (min, max) = match (a.is_empty(), b.is_empty()) {
        (true, true) => return Mesh::default(),
        (false, true) => (min_a, max_a),
        (true, false) => (min_b, max_b),
        (false, false) => (min_a.min(min_b), max_a.max(max_b)),
    };
    let unit = Unit {
        center: ((min + max) / 2.0).as_dvec3(),
        scale: (max - min).max_element().max(1e-9) as f64,
    };
    let mut first = Node::new(polygons(a, &unit));
    let mut second = Node::new(polygons(b, &unit));
    match operation {
        Operation::Union => {
            first.clip_to(&second);
            second.clip_to(&first);
            second.invert();
            second.clip_to(&first);
            second.invert();
            first.build(second.all_polygons());
        }
        Operation::Subtract => {
            first.invert();
            first.clip_to(&second);
            second.clip_to(&first);
            second.invert();
            second.clip_to(&first);
            second.invert();
            first.build(second.all_polygons());
            first.invert();
        }
        Operation::Intersect => {
            first.invert();
            second.clip_to(&first);
            second.invert();
            first.clip_to(&second);
            second.clip_to(&first);
            first.build(second.all_polygons());
            first.invert();
        }
    }
    let result = to_mesh(first.all_polygons(), &unit);
    if result.is_empty() {
        return result;
    }
    let merged = merge_flat_faces(&result);
    // Merging only ever leaves out corners both sides agree on; should it
    // open the mesh anyway, the unmerged one is kept.
    if merged.open_edges() == 0 {
        merged
    } else {
        result
    }
}

/// How the meshes were moved into the unit box: `(p - center) / scale`.
struct Unit {
    center: DVec3,
    scale: f64,
}

#[derive(Clone, Copy, Debug)]
struct Vertex {
    position: DVec3,
    normal: DVec3,
    uv: DVec2,
}

impl Vertex {
    fn lerp(&self, other: &Vertex, t: f64) -> Vertex {
        Vertex {
            position: self.position.lerp(other.position, t),
            normal: self.normal.lerp(other.normal, t),
            uv: self.uv.lerp(other.uv, t),
        }
    }
}

/// Points p with `normal · p == offset` are on it; greater is in front.
#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: DVec3,
    offset: f64,
}

impl Plane {
    fn flipped(self) -> Plane {
        Plane {
            normal: -self.normal,
            offset: -self.offset,
        }
    }

    fn distance(&self, point: DVec3) -> f64 {
        self.normal.dot(point) - self.offset
    }
}

/// A flat convex polygon, counter-clockwise seen from in front.
#[derive(Clone, Debug)]
struct Polygon {
    vertices: Vec<Vertex>,
    plane: Plane,
    surface: Surface,
}

impl Polygon {
    fn flip(&mut self) {
        self.vertices.reverse();
        for vertex in &mut self.vertices {
            vertex.normal = -vertex.normal;
        }
        self.plane = self.plane.flipped();
    }
}

/// `mesh`'s triangles as polygons in the unit box. Triangles with no area
/// are left out: they have no plane.
fn polygons(mesh: &Mesh, unit: &Unit) -> Vec<Polygon> {
    mesh.triangles
        .iter()
        .filter_map(|triangle| {
            let vertices: Vec<Vertex> = triangle
                .corners
                .iter()
                .map(|&corner| Vertex {
                    position: (mesh.positions[corner as usize].as_dvec3() - unit.center)
                        / unit.scale,
                    normal: mesh.normals[corner as usize].as_dvec3(),
                    uv: mesh.uvs[corner as usize].as_dvec2(),
                })
                .collect();
            let [a, b, c] = [0, 1, 2].map(|k| vertices[k].position);
            let normal = (b - a).cross(c - a).try_normalize()?;
            Some(Polygon {
                plane: Plane {
                    normal,
                    offset: normal.dot(a),
                },
                vertices,
                surface: triangle.surface,
            })
        })
        .collect()
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

/// Sorts `polygon` by `plane`: into `coplanar_front` or `coplanar_back` if
/// it's in the plane (by which way it faces), `front` or `back` if it's on
/// one side, or split in two across it.
fn split_polygon(
    plane: &Plane,
    polygon: Polygon,
    coplanar_front: &mut Vec<Polygon>,
    coplanar_back: &mut Vec<Polygon>,
    front: &mut Vec<Polygon>,
    back: &mut Vec<Polygon>,
) {
    let sides: Vec<u8> = polygon
        .vertices
        .iter()
        .map(|vertex| {
            let distance = plane.distance(vertex.position);
            if distance < -ON_PLANE {
                BACK
            } else if distance > ON_PLANE {
                FRONT
            } else {
                COPLANAR
            }
        })
        .collect();
    match sides.iter().fold(COPLANAR, |all, side| all | side) {
        COPLANAR => {
            if plane.normal.dot(polygon.plane.normal) > 0.0 {
                coplanar_front.push(polygon);
            } else {
                coplanar_back.push(polygon);
            }
        }
        FRONT => front.push(polygon),
        BACK => back.push(polygon),
        _ => {
            let count = polygon.vertices.len();
            let (mut ahead, mut behind) = (Vec::new(), Vec::new());
            for i in 0..count {
                let j = (i + 1) % count;
                let (side_i, side_j) = (sides[i], sides[j]);
                let (vi, vj) = (&polygon.vertices[i], &polygon.vertices[j]);
                if side_i != BACK {
                    ahead.push(*vi);
                }
                if side_i != FRONT {
                    behind.push(*vi);
                }
                if (side_i | side_j) == SPANNING {
                    // Worked out from the lower point of the two, so both
                    // polygons beside this edge get the very same point.
                    let (from, to) = if key(vi.position) <= key(vj.position) {
                        (vi, vj)
                    } else {
                        (vj, vi)
                    };
                    let t = plane.distance(from.position)
                        / plane.normal.dot(from.position - to.position);
                    let crossing = from.lerp(to, t);
                    ahead.push(crossing);
                    behind.push(crossing);
                }
            }
            let piece = |vertices: Vec<Vertex>| Polygon {
                vertices,
                plane: polygon.plane,
                surface: polygon.surface,
            };
            if ahead.len() >= 3 {
                front.push(piece(ahead));
            }
            if behind.len() >= 3 {
                back.push(piece(behind));
            }
        }
    }
}

/// A point's bits, for ordering the ends of an edge the same way from
/// either side.
fn key(point: DVec3) -> [u64; 3] {
    (point + DVec3::ZERO).to_array().map(f64::to_bits)
}

/// A BSP tree: the polygons in its plane, and the trees in front of and
/// behind it.
#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Polygon>,
}

impl Node {
    fn new(polygons: Vec<Polygon>) -> Node {
        let mut node = Node::default();
        node.build(polygons);
        node
    }

    /// Turns the solid inside out: inside becomes outside.
    fn invert(&mut self) {
        for polygon in &mut self.polygons {
            polygon.flip();
        }
        self.plane = self.plane.map(Plane::flipped);
        if let Some(front) = &mut self.front {
            front.invert();
        }
        if let Some(back) = &mut self.back {
            back.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// `polygons` without the pieces inside this tree's solid.
    fn clip_polygons(&self, polygons: Vec<Polygon>) -> Vec<Polygon> {
        let Some(plane) = &self.plane else {
            return polygons;
        };
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for polygon in polygons {
            // Coplanar polygons go with the side they face.
            let (mut coplanar_front, mut coplanar_back) = (Vec::new(), Vec::new());
            split_polygon(
                plane,
                polygon,
                &mut coplanar_front,
                &mut coplanar_back,
                &mut front,
                &mut back,
            );
            front.append(&mut coplanar_front);
            back.append(&mut coplanar_back);
        }
        let mut front = match &self.front {
            Some(node) => node.clip_polygons(front),
            None => front,
        };
        let back = match &self.back {
            Some(node) => node.clip_polygons(back),
            // Nothing behind the last plane: that's inside, so it goes.
            None => Vec::new(),
        };
        front.extend(back);
        front
    }

    /// Removes the pieces of this tree's polygons inside `other`'s solid.
    fn clip_to(&mut self, other: &Node) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(front) = &mut self.front {
            front.clip_to(other);
        }
        if let Some(back) = &mut self.back {
            back.clip_to(other);
        }
    }

    fn all_polygons(&self) -> Vec<Polygon> {
        let mut all = self.polygons.clone();
        if let Some(front) = &self.front {
            all.extend(front.all_polygons());
        }
        if let Some(back) = &self.back {
            all.extend(back.all_polygons());
        }
        all
    }

    /// Adds `polygons` to the tree, splitting them by its planes.
    fn build(&mut self, polygons: Vec<Polygon>) {
        let Some(first) = polygons.first() else {
            return;
        };
        let plane = *self.plane.get_or_insert(first.plane);
        let (mut front, mut back) = (Vec::new(), Vec::new());
        let mut coplanar = Vec::new();
        for polygon in polygons {
            let mut coplanar_back = Vec::new();
            split_polygon(
                &plane,
                polygon,
                &mut coplanar,
                &mut coplanar_back,
                &mut front,
                &mut back,
            );
            coplanar.append(&mut coplanar_back);
        }
        self.polygons.extend(coplanar);
        if !front.is_empty() {
            self.front.get_or_insert_with(Box::default).build(front);
        }
        if !back.is_empty() {
            self.back.get_or_insert_with(Box::default).build(back);
        }
    }
}

/// The polygons as a closed triangle mesh back in the meshes' own space:
/// near corners made one, every corner lying on an edge added to it, then
/// each polygon ear-clipped.
fn to_mesh(polygons: Vec<Polygon>, unit: &Unit) -> Mesh {
    // One id per distinct corner, by a grid of SAME_POINT cells: a point
    // takes the id of any close one in its cell or the cells around it.
    let mut points: Vec<DVec3> = Vec::new();
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let cell = |p: DVec3| (p / (SAME_POINT * 4.0)).floor().as_i64vec3().to_array();
    let mut id_of = |p: DVec3| -> u32 {
        let [x, y, z] = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = grid.get(&[x + dx, y + dy, z + dz]) {
                        for &id in ids {
                            if points[id as usize].distance(p) <= SAME_POINT {
                                return id;
                            }
                        }
                    }
                }
            }
        }
        let id = points.len() as u32;
        points.push(p);
        grid.entry([x, y, z]).or_default().push(id);
        id
    };
    let rings: Vec<Vec<(u32, Vertex)>> = polygons
        .iter()
        .map(|polygon| {
            let mut ring: Vec<(u32, Vertex)> = polygon
                .vertices
                .iter()
                .map(|vertex| (id_of(vertex.position), *vertex))
                .collect();
            ring.dedup_by_key(|(id, _)| *id);
            while ring.len() > 1 && ring.first().map(|c| c.0) == ring.last().map(|c| c.0) {
                ring.pop();
            }
            ring
        })
        .collect();

    // Corners on each polygon's edges, found through a coarse grid of the
    // points so each edge only looks at the points near it.
    let reach = 1.0 / 16.0;
    let coarse = |p: DVec3| (p / reach).floor().as_i64vec3();
    let mut near: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for (id, &point) in points.iter().enumerate() {
        near.entry(coarse(point).to_array())
            .or_default()
            .push(id as u32);
    }

    let mut mesh = Mesh::default();
    for (polygon, ring) in polygons.iter().zip(rings) {
        if ring.len() < 3 {
            continue;
        }
        let mut corners: Vec<(u32, Vertex)> = Vec::new();
        for k in 0..ring.len() {
            let (a, va) = ring[k];
            let (b, vb) = ring[(k + 1) % ring.len()];
            corners.push((a, va));
            let (pa, pb) = (points[a as usize], points[b as usize]);
            let length2 = pa.distance_squared(pb);
            if length2 <= 0.0 {
                continue;
            }
            let pad = DVec3::splat(SAME_POINT);
            let (low, high) = (coarse(pa.min(pb) - pad), coarse(pa.max(pb) + pad));
            let mut on_edge: Vec<(f64, u32)> = Vec::new();
            for x in low.x..=high.x {
                for y in low.y..=high.y {
                    for z in low.z..=high.z {
                        let Some(ids) = near.get(&[x, y, z]) else {
                            continue;
                        };
                        for &id in ids {
                            if id == a || id == b {
                                continue;
                            }
                            let p = points[id as usize];
                            let t = (p - pa).dot(pb - pa) / length2;
                            if t <= 0.0 || t >= 1.0 {
                                continue;
                            }
                            if (pa + (pb - pa) * t).distance(p) <= SAME_POINT {
                                on_edge.push((t, id));
                            }
                        }
                    }
                }
            }
            on_edge.sort_by(|x, y| x.0.total_cmp(&y.0));
            for (t, id) in on_edge {
                corners.push((id, va.lerp(&vb, t)));
            }
        }

        // Flattened onto the polygon's plane and ear-clipped, which copes
        // with the corners now lying in a straight line along its edges.
        let normal = polygon.plane.normal;
        let u = normal.any_orthonormal_vector();
        let v = normal.cross(u);
        let flat: Vec<DVec2> = corners
            .iter()
            .map(|(id, _)| {
                let p = points[*id as usize];
                DVec2::new(p.dot(u), p.dot(v))
            })
            .collect();
        let lap: Vec<u32> = (0..corners.len() as u32).collect();
        let first = mesh.positions.len() as u32;
        for (id, vertex) in &corners {
            let position = points[*id as usize] * unit.scale + unit.center;
            mesh.push_vertex(
                position.as_vec3(),
                vertex.normal.normalize_or_zero().as_vec3(),
                vertex.uv.as_vec2(),
            );
        }
        for triangle in fill(&flat, vec![lap]) {
            mesh.triangles.push(Triangle {
                corners: triangle.map(|k| first + k),
                surface: polygon.surface,
            });
        }
    }
    // Every corner of one id is the same f32 position, so edges match.
    mesh
}
