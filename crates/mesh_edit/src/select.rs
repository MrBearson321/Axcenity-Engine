//! What's selected in the Model Studio's Edit mode (Phase 55): vertices,
//! edges or faces, and the ways of picking more at once: a whole edge
//! loop (or face loop), everything linked, and growing or shrinking by one
//! ring.
//!
//! An edge is named by its key half-edge ([`EditMesh::edge_key`]).

use std::collections::BTreeSet;

use crate::half_edge::EditMesh;

/// What clicks pick in Edit mode (1, 2 and 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectMode {
    #[default]
    Vertex,
    Edge,
    Face,
}

/// The picked parts of one mesh. Only the set of the current mode counts;
/// switching modes converts it ([`Selection::convert`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub mode: SelectMode,
    pub vertices: BTreeSet<u32>,
    pub edges: BTreeSet<u32>,
    pub faces: BTreeSet<u32>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        match self.mode {
            SelectMode::Vertex => self.vertices.is_empty(),
            SelectMode::Edge => self.edges.is_empty(),
            SelectMode::Face => self.faces.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.edges.clear();
        self.faces.clear();
    }

    /// Picks `element` (a vertex, edge key or face, by the mode), or with
    /// `extend` (Shift) adds it, or takes it off again if it was picked.
    pub fn click(&mut self, element: Option<u32>, extend: bool) {
        let mode = self.mode;
        let set = self.set_mut(mode);
        match (element, extend) {
            (Some(element), true) => {
                if !set.remove(&element) {
                    set.insert(element);
                }
            }
            (Some(element), false) => {
                set.clear();
                set.insert(element);
            }
            (None, true) => {}
            (None, false) => set.clear(),
        }
    }

    /// The set the current `mode` uses.
    pub fn set_mut(&mut self, mode: SelectMode) -> &mut BTreeSet<u32> {
        match mode {
            SelectMode::Vertex => &mut self.vertices,
            SelectMode::Edge => &mut self.edges,
            SelectMode::Face => &mut self.faces,
        }
    }

    /// Every vertex the selection covers, which a move moves.
    pub fn vertices_of(&self, mesh: &EditMesh) -> Vec<u32> {
        let set: BTreeSet<u32> = match self.mode {
            SelectMode::Vertex => self.vertices.clone(),
            SelectMode::Edge => self
                .edges
                .iter()
                .flat_map(|&edge| {
                    let (a, b) = mesh.ends(edge);
                    [a, b]
                })
                .collect(),
            SelectMode::Face => self
                .faces
                .iter()
                .flat_map(|&face| mesh.face_vertices(face))
                .collect(),
        };
        set.into_iter().collect()
    }

    /// Switches to `mode`, keeping what's covered: vertices become the
    /// edges and faces they wholly cover, and edges or faces their corners.
    pub fn convert(&mut self, mesh: &EditMesh, mode: SelectMode) {
        if mode == self.mode {
            return;
        }
        let vertices: BTreeSet<u32> = self.vertices_of(mesh).into_iter().collect();
        self.clear();
        self.mode = mode;
        self.set_covered(mesh, &vertices);
    }

    /// Picks, in the current mode, whatever `vertices` wholly cover.
    fn set_covered(&mut self, mesh: &EditMesh, vertices: &BTreeSet<u32>) {
        match self.mode {
            SelectMode::Vertex => self.vertices = vertices.clone(),
            SelectMode::Edge => {
                self.edges = mesh
                    .edges()
                    .into_iter()
                    .filter(|&edge| {
                        let (a, b) = mesh.ends(edge);
                        vertices.contains(&a) && vertices.contains(&b)
                    })
                    .collect();
            }
            SelectMode::Face => {
                self.faces = (0..mesh.face_count() as u32)
                    .filter(|&face| {
                        mesh.face_vertices(face)
                            .iter()
                            .all(|v| vertices.contains(v))
                    })
                    .collect();
            }
        }
    }

    /// Picks every element of `mesh` (A).
    pub fn all(&mut self, mesh: &EditMesh) {
        let vertices = (0..mesh.vertex_count() as u32).collect();
        self.set_covered(mesh, &vertices);
    }

    /// Adds everything connected to the selection (L).
    pub fn linked(&mut self, mesh: &EditMesh) {
        let vertex_faces = mesh.vertex_faces();
        let mut reached: BTreeSet<u32> = self.vertices_of(mesh).into_iter().collect();
        let mut stack: Vec<u32> = reached.iter().copied().collect();
        while let Some(vertex) = stack.pop() {
            for &face in &vertex_faces[vertex as usize] {
                for corner in mesh.face_vertices(face) {
                    if reached.insert(corner) {
                        stack.push(corner);
                    }
                }
            }
        }
        self.set_covered(mesh, &reached);
    }

    /// Adds the ring of elements around the selection (Ctrl +): every
    /// corner of a face touching it.
    pub fn grow(&mut self, mesh: &EditMesh) {
        let vertex_faces = mesh.vertex_faces();
        let mut grown: BTreeSet<u32> = self.vertices_of(mesh).into_iter().collect();
        for vertex in grown.clone() {
            for &face in &vertex_faces[vertex as usize] {
                grown.extend(mesh.face_vertices(face));
            }
        }
        let mode = self.mode;
        let before = self.set_mut(mode).clone();
        self.set_covered(mesh, &grown);
        self.set_mut(mode).extend(before);
    }

    /// Takes off the selection's outer ring (Ctrl −): every vertex on a
    /// face that isn't wholly picked.
    pub fn shrink(&mut self, mesh: &EditMesh) {
        let vertex_faces = mesh.vertex_faces();
        let vertices: BTreeSet<u32> = self.vertices_of(mesh).into_iter().collect();
        let kept: BTreeSet<u32> = vertices
            .iter()
            .copied()
            .filter(|&vertex| {
                vertex_faces[vertex as usize].iter().all(|&face| {
                    mesh.face_vertices(face)
                        .iter()
                        .all(|corner| vertices.contains(corner))
                })
            })
            .collect();
        let mode = self.mode;
        let before = self.set_mut(mode).clone();
        self.set_covered(mesh, &kept);
        // Only ever takes off.
        let set = self.set_mut(mode);
        *set = set.intersection(&before).copied().collect();
    }

    /// Alt-click: picks (or with `extend` adds) the loop through `edge`:
    /// in Face mode the ring of faces it crosses ([`face_loop`]), otherwise
    /// the edge loop ([`edge_loop`]) or its vertices.
    pub fn select_loop(&mut self, mesh: &EditMesh, edge: u32, extend: bool) {
        if !extend {
            self.clear();
        }
        match self.mode {
            SelectMode::Face => self.faces.extend(face_loop(mesh, edge)),
            SelectMode::Edge => self.edges.extend(edge_loop(mesh, edge)),
            SelectMode::Vertex => {
                for edge in edge_loop(mesh, edge) {
                    let (a, b) = mesh.ends(edge);
                    self.vertices.insert(a);
                    self.vertices.insert(b);
                }
            }
        }
    }
}

/// The half-edge from `a` to `b`, if a face has one.
fn between(mesh: &EditMesh, outgoing: &[Vec<u32>], a: u32, b: u32) -> Option<u32> {
    outgoing[a as usize]
        .iter()
        .copied()
        .find(|&edge| mesh.ends(edge).1 == b)
}

/// The vertices joined to `vertex` by an edge.
fn neighbours(mesh: &EditMesh, outgoing: &[Vec<u32>], vertex: u32) -> Vec<u32> {
    let mut around: Vec<u32> = outgoing[vertex as usize]
        .iter()
        .flat_map(|&edge| {
            let half = mesh.half_edge(edge);
            [mesh.ends(edge).1, mesh.half_edge(half.prev).vertex]
        })
        .collect();
    around.sort_unstable();
    around.dedup();
    around
}

/// The edge loop through `edge` (a half-edge), as edge keys in the order
/// they're walked, like Blender's Alt-click. Across a vertex with four
/// edges it goes straight on, to the edge opposite; along the border of a
/// face with more than four sides (a cylinder's cap) it follows that
/// border; along an open border it follows the border. It stops where
/// none of those applies, or back where it started.
pub fn edge_loop(mesh: &EditMesh, edge: u32) -> Vec<u32> {
    let outgoing = mesh.outgoing();
    let start = mesh.edge_key(edge);
    let (a, b) = mesh.ends(start);
    let mut found = vec![start];
    let mut seen = BTreeSet::from([start]);
    // Walk on from b, then (if the loop didn't close) back from a.
    for (from, to) in [(a, b), (b, a)] {
        let (mut from, mut to) = (from, to);
        while let Some(next) = step(mesh, &outgoing, from, to) {
            let key = mesh.edge_key(
                between(mesh, &outgoing, to, next)
                    .or_else(|| between(mesh, &outgoing, next, to))
                    .expect("a step follows an edge"),
            );
            if !seen.insert(key) {
                if key == start {
                    return found;
                }
                break;
            }
            found.push(key);
            (from, to) = (to, next);
        }
    }
    found
}

/// Where an edge loop arriving at `to` from `from` goes next, if anywhere.
fn step(mesh: &EditMesh, outgoing: &[Vec<u32>], from: u32, to: u32) -> Option<u32> {
    let around = neighbours(mesh, outgoing, to);
    let forward = between(mesh, outgoing, from, to);
    let backward = between(mesh, outgoing, to, from);
    let faces =
        |edge: Option<u32>| edge.map(|e| mesh.face_half_edges(mesh.half_edge(e).face).len());
    let manifold = forward.is_some_and(|e| mesh.half_edge(e).twin.is_some());
    // Straight on across four edges: from the face on one side of the
    // edge, through the next face round, to the edge after it.
    if around.len() == 4 && manifold {
        let forward = forward?;
        let next = mesh.half_edge(forward).next;
        let across = mesh.half_edge(next).twin?;
        return Some(mesh.ends(mesh.half_edge(across).next).1);
    }
    // Along the border of a big face (a cap).
    if around.len() == 3 {
        if let Some(edge) = forward.filter(|_| faces(forward).is_some_and(|n| n > 4)) {
            return Some(mesh.ends(mesh.half_edge(edge).next).1);
        }
        if let Some(edge) = backward.filter(|_| faces(backward).is_some_and(|n| n > 4)) {
            return Some(mesh.half_edge(mesh.half_edge(edge).prev).vertex);
        }
    }
    // Along an open border: the other border edge at `to`.
    let open = |a: u32, b: u32| {
        between(mesh, outgoing, a, b).is_none_or(|e| mesh.half_edge(e).twin.is_none())
            && between(mesh, outgoing, b, a).is_none_or(|e| mesh.half_edge(e).twin.is_none())
    };
    if open(from, to) {
        let others: Vec<u32> = around
            .into_iter()
            .filter(|&other| other != from && open(to, other))
            .collect();
        if let [only] = others[..] {
            return Some(only);
        }
    }
    None
}

/// The ring of faces `edge` (a half-edge) crosses: from a four-sided face
/// through its opposite edge into the next, both ways, stopping at a face
/// with another number of sides or an open border.
pub fn face_loop(mesh: &EditMesh, edge: u32) -> Vec<u32> {
    let mut found: Vec<u32> = Vec::new();
    let mut seen = BTreeSet::new();
    let start = mesh.half_edge(edge);
    for first in [Some(edge), start.twin].into_iter().flatten() {
        let mut at = first;
        loop {
            let face = mesh.half_edge(at).face;
            if mesh.face_half_edges(face).len() != 4 || !seen.insert(face) {
                break;
            }
            found.push(face);
            let opposite = mesh.half_edge(mesh.half_edge(at).next).next;
            match mesh.half_edge(opposite).twin {
                Some(twin) => at = twin,
                None => break,
            }
        }
    }
    found
}

impl Selection {
    /// The middle of the vertices the selection covers (the median point
    /// moves, turns and scales go around), or `None` if nothing's picked.
    pub fn center(&self, mesh: &EditMesh) -> Option<glam::Vec3> {
        let vertices = self.vertices_of(mesh);
        if vertices.is_empty() {
            return None;
        }
        let sum: glam::Vec3 = vertices.iter().map(|&v| mesh.position(v)).sum();
        Some(sum / vertices.len() as f32)
    }

    /// The way the selection faces, for its Local axes: the average normal
    /// of its faces, or of the corners at its vertices. Zero if that can't
    /// be told.
    pub fn normal(&self, mesh: &EditMesh) -> glam::Vec3 {
        let sum: glam::Vec3 = match self.mode {
            SelectMode::Face => self.faces.iter().map(|&f| mesh.face_normal(f)).sum(),
            _ => {
                let vertices: BTreeSet<u32> = self.vertices_of(mesh).into_iter().collect();
                mesh.half_edges()
                    .iter()
                    .filter(|edge| vertices.contains(&edge.vertex))
                    .map(|edge| edge.normal)
                    .sum()
            }
        };
        sum.normalize_or_zero()
    }
}
