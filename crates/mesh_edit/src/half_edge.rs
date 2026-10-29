//! An editable mesh of polygons (Phase 55), kept as half-edges, as
//! Blender's is, so the Model Studio can walk from a face to its edges,
//! across an edge to the next face, and around a vertex.
//!
//! Each face is a loop of half-edges, one per corner, going
//! counter-clockwise seen from outside. A half-edge starts at its corner's
//! vertex and runs to the next corner; it carries that corner's texture
//! coordinates and normal, so a seam (two uvs at one vertex) or a hard
//! edge (two normals) needs no split vertex. The half-edge running the
//! other way along the same edge, in the face next door, is its twin. An
//! edge with only one face (an open border), or with more than two (which
//! a half-edge mesh can't hold), has half-edges without twins.
//!
//! Vertices and faces are only added in this phase, never removed, so
//! their numbers stay put and selections can hold them.

use std::collections::HashMap;

use glam::{DVec2, Mat4, Vec2, Vec3};

use crate::fill::fill;

/// One side of an edge: from `vertex` to the next corner of `face`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfEdge {
    /// The vertex it starts at.
    pub vertex: u32,
    pub face: u32,
    pub next: u32,
    pub prev: u32,
    /// The half-edge running the other way, in the neighbouring face.
    pub twin: Option<u32>,
    /// The texture coordinates of its starting corner.
    pub uv: Vec2,
    /// The normal at its starting corner, one unit long.
    pub normal: Vec3,
}

/// A polygon: its first half-edge, which material slot draws it, and
/// whether it's shaded smooth (corners take the normal of every smooth
/// face around their vertex) or flat (each corner takes the face's own).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub first: u32,
    pub material: u32,
    pub smooth: bool,
}

/// A face to add: its corners (vertex numbers, counter-clockwise seen from
/// outside) and each corner's uv. `normals` is one per corner, or empty to
/// work them out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polygon {
    pub corners: Vec<u32>,
    pub uvs: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub material: u32,
    pub smooth: bool,
}

/// An editable polygon mesh, in the file's units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditMesh {
    positions: Vec<Vec3>,
    half_edges: Vec<HalfEdge>,
    faces: Vec<Face>,
}

impl EditMesh {
    /// A mesh with these vertices and faces. A face with fewer than three
    /// corners, a corner that isn't a vertex, or a vertex used twice is
    /// left out.
    pub fn new(positions: Vec<Vec3>, polygons: &[Polygon]) -> EditMesh {
        let mut mesh = EditMesh {
            positions,
            ..EditMesh::default()
        };
        for polygon in polygons {
            mesh.push_face(polygon);
        }
        mesh.link_twins();
        let all: Vec<u32> = (0..mesh.faces.len() as u32).collect();
        // Faces given no normals get them now.
        let unshaded: Vec<u32> = all
            .into_iter()
            .filter(|&face| polygons_normals_missing(&mesh, face))
            .collect();
        mesh.shade(&unshaded);
        mesh
    }

    fn push_face(&mut self, polygon: &Polygon) {
        let count = polygon.corners.len();
        let valid = count >= 3
            && polygon
                .corners
                .iter()
                .all(|&v| (v as usize) < self.positions.len())
            && (1..count).all(|i| !polygon.corners[..i].contains(&polygon.corners[i]));
        if !valid {
            return;
        }
        let face = self.faces.len() as u32;
        let first = self.half_edges.len() as u32;
        for (k, &vertex) in polygon.corners.iter().enumerate() {
            let k = k as u32;
            let count = count as u32;
            self.half_edges.push(HalfEdge {
                vertex,
                face,
                next: first + (k + 1) % count,
                prev: first + (k + count - 1) % count,
                twin: None,
                uv: polygon.uvs.get(k as usize).copied().unwrap_or(Vec2::ZERO),
                // NaN marks a corner whose normal is worked out below.
                normal: polygon
                    .normals
                    .get(k as usize)
                    .copied()
                    .unwrap_or(Vec3::NAN),
            });
        }
        self.faces.push(Face {
            first,
            material: polygon.material,
            smooth: polygon.smooth,
        });
    }

    /// Pairs each half-edge with the one running the other way along the
    /// same edge. A third face on an edge gets no twin.
    fn link_twins(&mut self) {
        let mut by_ends: HashMap<(u32, u32), u32> = HashMap::new();
        for index in 0..self.half_edges.len() as u32 {
            let (a, b) = self.ends(index);
            by_ends.entry((a, b)).or_insert(index);
        }
        for index in 0..self.half_edges.len() as u32 {
            let (a, b) = self.ends(index);
            if by_ends.get(&(a, b)) != Some(&index) {
                continue;
            }
            if let Some(&other) = by_ends.get(&(b, a))
                && self.half_edges[other as usize].twin.is_none()
                && self.half_edges[index as usize].twin.is_none()
            {
                self.half_edges[index as usize].twin = Some(other);
                self.half_edges[other as usize].twin = Some(index);
            }
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }

    pub fn position(&self, vertex: u32) -> Vec3 {
        self.positions[vertex as usize]
    }

    pub fn half_edges(&self) -> &[HalfEdge] {
        &self.half_edges
    }

    pub fn half_edge(&self, index: u32) -> &HalfEdge {
        &self.half_edges[index as usize]
    }

    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    pub fn face(&self, face: u32) -> &Face {
        &self.faces[face as usize]
    }

    /// The vertex `half_edge` starts at and the one it runs to.
    pub fn ends(&self, half_edge: u32) -> (u32, u32) {
        let edge = &self.half_edges[half_edge as usize];
        (edge.vertex, self.half_edges[edge.next as usize].vertex)
    }

    /// The half-edges around `face`, from its first.
    pub fn face_half_edges(&self, face: u32) -> Vec<u32> {
        let first = self.faces[face as usize].first;
        let mut edges = vec![first];
        let mut at = self.half_edges[first as usize].next;
        while at != first && edges.len() <= self.half_edges.len() {
            edges.push(at);
            at = self.half_edges[at as usize].next;
        }
        edges
    }

    /// The corners of `face`, counter-clockwise seen from outside.
    pub fn face_vertices(&self, face: u32) -> Vec<u32> {
        self.face_half_edges(face)
            .into_iter()
            .map(|edge| self.half_edges[edge as usize].vertex)
            .collect()
    }

    /// The middle of `face`'s corners.
    pub fn face_center(&self, face: u32) -> Vec3 {
        let corners = self.face_vertices(face);
        corners.iter().map(|&v| self.position(v)).sum::<Vec3>() / corners.len() as f32
    }

    /// The way `face` faces, one unit long (zero for a face with no area).
    pub fn face_normal(&self, face: u32) -> Vec3 {
        let corners: Vec<Vec3> = self
            .face_vertices(face)
            .into_iter()
            .map(|v| self.position(v))
            .collect();
        newell(&corners)
    }

    /// Every edge once, each named by one of its half-edges: the one
    /// without a twin, or the lower-numbered of the two.
    pub fn edges(&self) -> Vec<u32> {
        (0..self.half_edges.len() as u32)
            .filter(|&index| self.is_edge_key(index))
            .collect()
    }

    /// True if `half_edge` is the one that names its edge (see [`edges`]).
    ///
    /// [`edges`]: Self::edges
    pub fn is_edge_key(&self, half_edge: u32) -> bool {
        self.half_edges[half_edge as usize]
            .twin
            .is_none_or(|twin| half_edge < twin)
    }

    /// The half-edge that names `half_edge`'s edge.
    pub fn edge_key(&self, half_edge: u32) -> u32 {
        match self.half_edges[half_edge as usize].twin {
            Some(twin) => half_edge.min(twin),
            None => half_edge,
        }
    }

    /// For each vertex, the half-edges leaving it. Worked out afresh: the
    /// mesh only keeps what faces need.
    pub fn outgoing(&self) -> Vec<Vec<u32>> {
        let mut outgoing = vec![Vec::new(); self.positions.len()];
        for (index, edge) in self.half_edges.iter().enumerate() {
            outgoing[edge.vertex as usize].push(index as u32);
        }
        outgoing
    }

    /// For each vertex, the faces that have it as a corner.
    pub fn vertex_faces(&self) -> Vec<Vec<u32>> {
        let mut faces = vec![Vec::new(); self.positions.len()];
        for edge in &self.half_edges {
            faces[edge.vertex as usize].push(edge.face);
        }
        faces
    }

    /// Moves `vertices` by `matrix`, then shades the faces around them again.
    pub fn transform(&mut self, vertices: &[u32], matrix: Mat4) {
        for &vertex in vertices {
            let position = &mut self.positions[vertex as usize];
            *position = matrix.transform_point3(*position);
        }
        self.reshade_around(vertices);
    }

    /// Puts `vertices` at `positions` (one each), as they were before an
    /// edit, and shades the faces around them again.
    pub fn set_positions(&mut self, vertices: &[u32], positions: &[Vec3]) {
        for (&vertex, &position) in vertices.iter().zip(positions) {
            self.positions[vertex as usize] = position;
        }
        self.reshade_around(vertices);
    }

    /// Works out the normals of every face touching `vertices`, and of the
    /// smooth faces next to those (whose shared corners average theirs).
    fn reshade_around(&mut self, vertices: &[u32]) {
        let vertex_faces = self.vertex_faces();
        let mut touched = vec![false; self.positions.len()];
        for &vertex in vertices {
            touched[vertex as usize] = true;
        }
        let mut faces: Vec<u32> = Vec::new();
        for &vertex in vertices {
            for &face in &vertex_faces[vertex as usize] {
                for corner in self.face_vertices(face) {
                    touched[corner as usize] = true;
                }
            }
        }
        for (vertex, faces_here) in vertex_faces.iter().enumerate() {
            if touched[vertex] {
                faces.extend(faces_here);
            }
        }
        faces.sort_unstable();
        faces.dedup();
        self.shade(&faces);
    }

    /// Works out the corner normals of `faces`: a flat face's corners take
    /// its normal, and a smooth face's the average of the smooth faces
    /// around each corner's vertex, weighted by the corner's angle so a
    /// face cut in two doesn't count twice.
    fn shade(&mut self, faces: &[u32]) {
        if faces.is_empty() {
            return;
        }
        let normals: Vec<Vec3> = (0..self.faces.len() as u32)
            .map(|face| self.face_normal(face))
            .collect();
        let mut smooth = vec![Vec3::ZERO; self.positions.len()];
        for edge in &self.half_edges {
            let face = edge.face as usize;
            if !self.faces[face].smooth {
                continue;
            }
            let here = self.positions[edge.vertex as usize];
            let next = self.positions[self.half_edges[edge.next as usize].vertex as usize];
            let prev = self.positions[self.half_edges[edge.prev as usize].vertex as usize];
            let angle = (next - here).angle_between(prev - here);
            let angle = if angle.is_finite() { angle } else { 0.0 };
            smooth[edge.vertex as usize] += normals[face] * angle;
        }
        for &face in faces {
            let flat = normals[face as usize];
            let is_smooth = self.faces[face as usize].smooth;
            for edge in self.face_half_edges(face) {
                let vertex = self.half_edges[edge as usize].vertex as usize;
                let normal = if is_smooth {
                    smooth[vertex].normalize_or(flat)
                } else {
                    flat
                };
                self.half_edges[edge as usize].normal = normal.normalize_or(Vec3::Y);
            }
        }
    }

    /// The faces cut into triangles: each `[half-edge; 3]`, so a triangle's
    /// corners keep their uvs and normals. Triangles are counter-clockwise.
    pub fn triangles(&self, face: u32) -> Vec<[u32; 3]> {
        let edges = self.face_half_edges(face);
        if edges.len() == 3 {
            return vec![[edges[0], edges[1], edges[2]]];
        }
        let corners: Vec<Vec3> = edges
            .iter()
            .map(|&e| self.position(self.half_edges[e as usize].vertex))
            .collect();
        let fan = || {
            (1..edges.len() - 1)
                .map(|k| [edges[0], edges[k], edges[k + 1]])
                .collect()
        };
        let normal = newell(&corners);
        if normal == Vec3::ZERO {
            return fan();
        }
        // Flattened onto the face's plane, seen from outside, the corners
        // go counter-clockwise, which ear clipping takes as an outline.
        let u = normal.any_orthonormal_vector();
        let v = normal.cross(u);
        let flat: Vec<DVec2> = corners
            .iter()
            .map(|&p| DVec2::new(p.dot(u) as f64, p.dot(v) as f64))
            .collect();
        let triangles = fill(&flat, vec![(0..edges.len() as u32).collect()]);
        if triangles.len() != edges.len() - 2 {
            return fan();
        }
        triangles
            .into_iter()
            .map(|t| t.map(|k| edges[k as usize]))
            .collect()
    }
}

/// True if a corner of `face` was given no normal.
fn polygons_normals_missing(mesh: &EditMesh, face: u32) -> bool {
    mesh.face_half_edges(face)
        .into_iter()
        .any(|edge| !mesh.half_edges[edge as usize].normal.is_finite())
}

/// The normal of a polygon through `corners` (Newell's method), however
/// its corners are spaced; zero if it has no area.
pub(crate) fn newell(corners: &[Vec3]) -> Vec3 {
    let mut normal = Vec3::ZERO;
    for (k, &a) in corners.iter().enumerate() {
        let b = corners[(k + 1) % corners.len()];
        normal += (a - b).cross(a + b);
    }
    normal.normalize_or_zero()
}
