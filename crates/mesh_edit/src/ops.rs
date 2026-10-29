//! Modeling operations (Phase 56): the Model Studio's Blender tools on an
//! [`EditMesh`] and what's picked of it. Extrude, inset, bevel, loop cut,
//! edge slide, knife, bridge, merge, dissolve, delete, fill, grid fill,
//! subdivide, spin, flipping and recalculating normals, and shading.
//!
//! Each operation takes the mesh apart into a [`Soup`] of polygons, edits
//! that, and builds a new mesh from it, so twins and shading are worked
//! out afresh rather than patched. A [`MeshOp`] holds every setting it
//! runs with, so the Studio can run it again on the mesh it started from
//! when a setting changes (Blender's "Adjust last operation").
//!
//! Every operation keeps a manifold mesh manifold ([`check_manifold`]):
//! each edge has one or two faces (two running opposite ways), and the
//! faces round each vertex make one fan. Only Delete opens a closed one.

mod bevel;
mod cleanup;
mod cuts;
mod extrude;
mod shading;

use std::collections::{BTreeSet, HashMap, HashSet};

use glam::{Vec2, Vec3};

use crate::half_edge::{EditMesh, Polygon, newell};
use crate::select::{SelectMode, Selection};

/// How Extrude pushes faces out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExtrudeMode {
    /// Each connected region together, along its average normal.
    #[default]
    Region,
    /// Each face on its own, along its own normal.
    Individual,
    /// Each vertex along the normal of the picked faces round it, keeping
    /// the region's thickness even.
    AlongNormals,
}

/// Where Merge puts the vertices it joins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MergeAt {
    /// All of them at their middle.
    #[default]
    Center,
    /// All of them at the 3D cursor.
    Cursor,
    /// Only those closer together than the distance, each group at its
    /// middle.
    Distance,
}

/// What Delete takes away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteKind {
    /// The picked vertices and every face using one.
    Vertices,
    /// Every face along a picked edge.
    Edges,
    /// The picked faces, and the vertices only they used.
    Faces,
    /// The picked faces, keeping every vertex.
    OnlyFaces,
}

/// What Dissolve takes away, joining the faces round it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DissolveKind {
    Vertices,
    Edges,
    Faces,
}

/// A point a knife cut passes through: on the edge from vertex `a` to `b`,
/// `t` of the way along (`a == b` for the vertex itself).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnifePoint {
    pub a: u32,
    pub b: u32,
    pub t: f32,
}

/// A modeling operation and everything it runs with.
#[derive(Clone, Debug, PartialEq)]
pub enum MeshOp {
    /// E: the picked faces pushed out with walls round them, or edges on
    /// an open border pulled out into new faces.
    Extrude {
        mode: ExtrudeMode,
        distance: f32,
    },
    /// I: a smaller copy of the picked faces inside them, joined by a ring
    /// of faces; `depth` pushes it out (or in).
    Inset {
        thickness: f32,
        depth: f32,
        individual: bool,
    },
    /// Ctrl+B (edges) and Ctrl+Shift+B (`vertices`): the picked edges or
    /// corners cut off and rounded. `profile` is 0.5 for a round bevel, 1
    /// for a square one and 0.25 for a flat one.
    Bevel {
        width: f32,
        segments: u32,
        profile: f32,
        vertices: bool,
    },
    /// Ctrl+R: `cuts` loops across the ring of quads the edge from
    /// `edge[0]` to `edge[1]` crosses, slid by `slide` (−1 to 1).
    LoopCut {
        edge: [u32; 2],
        cuts: u32,
        slide: f32,
    },
    /// G G: the picked edges slid along the edges beside them, `factor`
    /// of the way (−1 to 1; the sign picks the side).
    EdgeSlide {
        factor: f32,
    },
    /// K: faces cut along a line through these points.
    Knife {
        points: Vec<KnifePoint>,
    },
    /// Faces joining two picked edge loops on open borders.
    Bridge,
    /// M: picked vertices joined into one.
    Merge {
        at: MergeAt,
        cursor: Vec3,
        distance: f32,
    },
    Dissolve(DissolveKind),
    Delete(DeleteKind),
    /// F: a face filling each picked loop of open-border edges.
    Fill,
    /// A grid of quads filling a picked open-border loop: `span` quads one
    /// way (0 picks a square-ish grid), starting `offset` edges round.
    GridFill {
        span: u32,
        offset: u32,
    },
    /// The picked faces cut into smaller ones, each edge in `cuts + 1`.
    Subdivide {
        cuts: u32,
    },
    /// The picked faces (or open-border edges) swept round `axis` through
    /// `center`, in `steps` steps.
    Spin {
        degrees: f32,
        steps: u32,
        axis: Vec3,
        center: Vec3,
    },
    FlipNormals,
    /// Shift+N (outside) and Ctrl+Shift+N (`inside`).
    RecalculateNormals {
        inside: bool,
    },
    ShadeSmooth,
    ShadeFlat,
    /// Smooth, except across edges sharper than `degrees`.
    AutoSmooth {
        degrees: f32,
    },
}

impl MeshOp {
    /// Its name, for the undo step and the adjust panel.
    pub fn label(&self) -> &'static str {
        match self {
            MeshOp::Extrude { .. } => "Extrude",
            MeshOp::Inset { .. } => "Inset",
            MeshOp::Bevel { .. } => "Bevel",
            MeshOp::LoopCut { .. } => "Loop Cut",
            MeshOp::EdgeSlide { .. } => "Edge Slide",
            MeshOp::Knife { .. } => "Knife",
            MeshOp::Bridge => "Bridge Edge Loops",
            MeshOp::Merge { .. } => "Merge",
            MeshOp::Dissolve(_) => "Dissolve",
            MeshOp::Delete(_) => "Delete",
            MeshOp::Fill => "Fill",
            MeshOp::GridFill { .. } => "Grid Fill",
            MeshOp::Subdivide { .. } => "Subdivide",
            MeshOp::Spin { .. } => "Spin",
            MeshOp::FlipNormals => "Flip Normals",
            MeshOp::RecalculateNormals { .. } => "Recalculate Normals",
            MeshOp::ShadeSmooth => "Shade Smooth",
            MeshOp::ShadeFlat => "Shade Flat",
            MeshOp::AutoSmooth { .. } => "Shade Auto Smooth",
        }
    }
}

/// Runs `operation` on `mesh` with `selection` picked: the new mesh, and
/// what's picked of it after (the new faces, loops or vertices, in the
/// same select mode). An error says why it couldn't run.
pub fn apply(
    mesh: &EditMesh,
    selection: &Selection,
    operation: &MeshOp,
) -> Result<(EditMesh, Selection), String> {
    let input = Input::of(mesh, selection);
    if let MeshOp::EdgeSlide { factor } = operation {
        let mut mesh = mesh.clone();
        cuts::edge_slide(&mut mesh, &input, *factor)?;
        return Ok((mesh, selection.clone()));
    }
    let mut soup = Soup::of(mesh);
    let mut keep_loose = false;
    let picked = match operation {
        MeshOp::Extrude { mode, distance } => {
            extrude::extrude(&mut soup, &input, *mode, *distance)?
        }
        MeshOp::Inset {
            thickness,
            depth,
            individual,
        } => extrude::inset(&mut soup, &input, *thickness, *depth, *individual)?,
        MeshOp::Bevel {
            width,
            segments,
            profile,
            vertices,
        } => bevel::bevel(&mut soup, &input, *width, *segments, *profile, *vertices)?,
        MeshOp::LoopCut { edge, cuts, slide } => cuts::loop_cut(&mut soup, *edge, *cuts, *slide)?,
        MeshOp::EdgeSlide { .. } => unreachable!("handled above"),
        MeshOp::Knife { points } => cuts::knife(&mut soup, points)?,
        MeshOp::Bridge => cleanup::bridge(&mut soup, &input)?,
        MeshOp::Merge {
            at,
            cursor,
            distance,
        } => cleanup::merge(&mut soup, &input, *at, *cursor, *distance)?,
        MeshOp::Dissolve(kind) => cleanup::dissolve(&mut soup, &input, *kind)?,
        MeshOp::Delete(kind) => {
            keep_loose = *kind == DeleteKind::OnlyFaces;
            cleanup::delete(&mut soup, &input, *kind)?
        }
        MeshOp::Fill => cleanup::fill(&mut soup, &input)?,
        MeshOp::GridFill { span, offset } => cleanup::grid_fill(&mut soup, &input, *span, *offset)?,
        MeshOp::Subdivide { cuts } => cuts::subdivide(&mut soup, &input, *cuts)?,
        MeshOp::Spin {
            degrees,
            steps,
            axis,
            center,
        } => extrude::spin(&mut soup, &input, *degrees, *steps, *axis, *center)?,
        MeshOp::FlipNormals => shading::flip(&mut soup, &input)?,
        MeshOp::RecalculateNormals { inside } => shading::recalculate(&mut soup, &input, *inside)?,
        MeshOp::ShadeSmooth => shading::shade(&mut soup, &input, true, None)?,
        MeshOp::ShadeFlat => shading::shade(&mut soup, &input, false, None)?,
        MeshOp::AutoSmooth { degrees } => {
            shading::shade(&mut soup, &input, true, Some(degrees.to_radians()))?
        }
    };
    let built = soup.build(keep_loose);
    let selection = built.selection(selection.mode, &picked);
    Ok((built.mesh, selection))
}

/// Why `mesh` isn't a manifold surface, if it isn't: an edge with two
/// faces running the same way along it (or more than two), or a vertex
/// where the faces round it make more than one fan. With `closed`, an edge
/// with only one face fails too.
pub fn check_manifold(mesh: &EditMesh, closed: bool) -> Result<(), String> {
    let mut directed: HashSet<(u32, u32)> = HashSet::new();
    for index in 0..mesh.half_edges().len() as u32 {
        let ends = mesh.ends(index);
        if !directed.insert(ends) {
            return Err(format!("edge {ends:?} has two faces running one way"));
        }
    }
    if closed && let Some(open) = directed.iter().find(|(a, b)| !directed.contains(&(*b, *a))) {
        return Err(format!("edge {open:?} is on an open border"));
    }
    // The faces round each vertex, joined across the edges they share.
    let mut parent: Vec<u32> = (0..mesh.face_count() as u32).collect();
    fn root(parent: &mut [u32], face: u32) -> u32 {
        let mut at = face;
        while parent[at as usize] != at {
            parent[at as usize] = parent[parent[at as usize] as usize];
            at = parent[at as usize];
        }
        at
    }
    for (vertex, outgoing) in mesh.outgoing().iter().enumerate() {
        let faces: Vec<u32> = outgoing.iter().map(|&e| mesh.half_edge(e).face).collect();
        for &face in &faces {
            parent[face as usize] = face;
        }
        for &edge in outgoing {
            // The twin of the edge into this corner leaves the vertex too.
            let into = mesh.half_edge(edge).prev;
            if let Some(twin) = mesh.half_edge(into).twin {
                let (a, b) = (
                    root(&mut parent, mesh.half_edge(into).face),
                    root(&mut parent, mesh.half_edge(twin).face),
                );
                parent[a as usize] = b;
            }
        }
        let fans: BTreeSet<u32> = faces.iter().map(|&f| root(&mut parent, f)).collect();
        if fans.len() > 1 {
            return Err(format!(
                "vertex {vertex} joins {} fans of faces",
                fans.len()
            ));
        }
    }
    Ok(())
}

/// What an operation works on, from the selection: its vertices, edges
/// (each as its two ends, smaller first) and faces.
pub(crate) struct Input {
    pub vertices: BTreeSet<u32>,
    pub edges: BTreeSet<[u32; 2]>,
    pub faces: BTreeSet<usize>,
}

impl Input {
    fn of(mesh: &EditMesh, selection: &Selection) -> Input {
        let vertices: BTreeSet<u32> = selection.vertices_of(mesh).into_iter().collect();
        let faces: BTreeSet<usize> = match selection.mode {
            SelectMode::Face => selection.faces.iter().map(|&f| f as usize).collect(),
            _ => (0..mesh.face_count())
                .filter(|&f| {
                    mesh.face_vertices(f as u32)
                        .iter()
                        .all(|v| vertices.contains(v))
                })
                .collect(),
        };
        let edges: BTreeSet<[u32; 2]> = match selection.mode {
            SelectMode::Edge => selection
                .edges
                .iter()
                .map(|&e| {
                    let (a, b) = mesh.ends(e);
                    key(a, b)
                })
                .collect(),
            SelectMode::Face => faces
                .iter()
                .flat_map(|&f| mesh.face_half_edges(f as u32))
                .map(|e| {
                    let (a, b) = mesh.ends(e);
                    key(a, b)
                })
                .collect(),
            SelectMode::Vertex => mesh
                .edges()
                .into_iter()
                .map(|e| mesh.ends(e))
                .filter(|(a, b)| vertices.contains(a) && vertices.contains(b))
                .map(|(a, b)| key(a, b))
                .collect(),
        };
        Input {
            vertices,
            edges,
            faces,
        }
    }
}

/// An edge by its two ends, smaller first.
pub(crate) fn key(a: u32, b: u32) -> [u32; 2] {
    [a.min(b), a.max(b)]
}

/// What an operation leaves picked, in the [`Soup`]'s numbers.
#[derive(Default)]
pub(crate) struct Picked {
    pub vertices: BTreeSet<u32>,
    pub edges: Vec<[u32; 2]>,
    pub faces: BTreeSet<usize>,
}

/// One face of a [`Soup`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoupFace {
    /// Vertex numbers, counter-clockwise seen from outside.
    pub corners: Vec<u32>,
    /// One per corner.
    pub uvs: Vec<Vec2>,
    /// One per corner, as the mesh had them; empty when the face changed
    /// and is shaded afresh.
    pub normals: Vec<Vec3>,
    pub material: u32,
    pub smooth: bool,
}

impl SoupFace {
    /// A new face through `corners` (with their `uvs`), drawn like `like`.
    pub fn like(like: &SoupFace, corners: Vec<u32>, uvs: Vec<Vec2>) -> SoupFace {
        SoupFace {
            corners,
            uvs,
            normals: Vec::new(),
            material: like.material,
            smooth: like.smooth,
        }
    }

    /// The edge from corner `k` to the next.
    pub fn edge(&self, k: usize) -> (u32, u32) {
        (self.corners[k], self.corners[(k + 1) % self.corners.len()])
    }

    /// The uv of the corner at `vertex` (zero if it has none).
    pub fn uv_at(&self, vertex: u32) -> Vec2 {
        self.corners
            .iter()
            .position(|&c| c == vertex)
            .map_or(Vec2::ZERO, |k| self.uvs[k])
    }

    /// Turns it to face the other way.
    pub fn flip(&mut self) {
        self.corners.reverse();
        self.uvs.reverse();
        self.normals.reverse();
        for normal in &mut self.normals {
            *normal = -*normal;
        }
    }

    /// Corners repeated in a row dropped, and a corner that comes back
    /// later split off into a face of its own; pieces of fewer than three
    /// corners are left out.
    fn cleaned(mut self) -> Vec<SoupFace> {
        let count = self.corners.len();
        let keep: Vec<usize> = (0..count)
            .filter(|&k| self.corners[k] != self.corners[(k + 1) % count])
            .collect();
        if keep.len() != count {
            self.corners = keep.iter().map(|&k| self.corners[k]).collect();
            self.uvs = keep.iter().map(|&k| self.uvs[k]).collect();
            self.normals.clear();
        }
        let repeat = (1..self.corners.len()).find_map(|j| {
            let i = self.corners[..j]
                .iter()
                .position(|&c| c == self.corners[j])?;
            Some((i, j))
        });
        let Some((i, j)) = repeat else {
            return if self.corners.len() >= 3 {
                vec![self]
            } else {
                Vec::new()
            };
        };
        let inner = SoupFace::like(&self, self.corners[i..j].to_vec(), self.uvs[i..j].to_vec());
        let outer = SoupFace::like(
            &self,
            [&self.corners[..i], &self.corners[j..]].concat(),
            [&self.uvs[..i], &self.uvs[j..]].concat(),
        );
        let mut pieces = outer.cleaned();
        pieces.extend(inner.cleaned());
        pieces
    }
}

/// A mesh taken apart into its vertices and polygons, which an operation
/// edits before it's built back into half-edges.
#[derive(Clone, Debug)]
pub(crate) struct Soup {
    pub positions: Vec<Vec3>,
    pub faces: Vec<SoupFace>,
    pub auto_smooth: Option<f32>,
}

impl Soup {
    pub fn of(mesh: &EditMesh) -> Soup {
        let faces = (0..mesh.face_count() as u32)
            .map(|face| {
                let edges = mesh.face_half_edges(face);
                let corner = |e: &u32| *mesh.half_edge(*e);
                SoupFace {
                    corners: edges.iter().map(|e| corner(e).vertex).collect(),
                    uvs: edges.iter().map(|e| corner(e).uv).collect(),
                    normals: edges.iter().map(|e| corner(e).normal).collect(),
                    material: mesh.face(face).material,
                    smooth: mesh.face(face).smooth,
                }
            })
            .collect();
        Soup {
            positions: mesh.positions().to_vec(),
            faces,
            auto_smooth: mesh.auto_smooth(),
        }
    }

    /// Adds a vertex at `position` and returns its number.
    pub fn add(&mut self, position: Vec3) -> u32 {
        self.positions.push(position);
        (self.positions.len() - 1) as u32
    }

    pub fn position(&self, vertex: u32) -> Vec3 {
        self.positions[vertex as usize]
    }

    /// The way `face` faces, one unit long (zero with no area).
    pub fn normal(&self, face: usize) -> Vec3 {
        let corners: Vec<Vec3> = self.faces[face]
            .corners
            .iter()
            .map(|&v| self.position(v))
            .collect();
        newell(&corners)
    }

    /// The middle of `face`'s corners.
    pub fn center(&self, face: usize) -> Vec3 {
        let corners = &self.faces[face].corners;
        corners.iter().map(|&v| self.position(v)).sum::<Vec3>() / corners.len() as f32
    }

    /// For each edge as a face runs along it (from, to): that face, and
    /// the corner it starts at.
    pub fn directed(&self) -> HashMap<(u32, u32), (usize, usize)> {
        let mut directed = HashMap::new();
        for (index, face) in self.faces.iter().enumerate() {
            for k in 0..face.corners.len() {
                directed.entry(face.edge(k)).or_insert((index, k));
            }
        }
        directed
    }

    /// The faces along each edge.
    pub fn edge_faces(&self) -> HashMap<[u32; 2], Vec<usize>> {
        let mut faces: HashMap<[u32; 2], Vec<usize>> = HashMap::new();
        for (index, face) in self.faces.iter().enumerate() {
            for k in 0..face.corners.len() {
                let (a, b) = face.edge(k);
                faces.entry(key(a, b)).or_default().push(index);
            }
        }
        faces
    }

    /// `faces` split into groups joined by shared edges, each in order.
    pub fn components(&self, faces: &BTreeSet<usize>) -> Vec<Vec<usize>> {
        let edge_faces = self.edge_faces();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut groups = Vec::new();
        for &start in faces {
            if !seen.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut stack = vec![start];
            while let Some(face) = stack.pop() {
                let face_ref = &self.faces[face];
                for k in 0..face_ref.corners.len() {
                    let (a, b) = face_ref.edge(k);
                    for &next in &edge_faces[&key(a, b)] {
                        if faces.contains(&next) && seen.insert(next) {
                            group.push(next);
                            stack.push(next);
                        }
                    }
                }
            }
            group.sort_unstable();
            groups.push(group);
        }
        groups
    }

    /// The loops of picked `edges` on an open border, each as its vertices
    /// in the order the border's faces run along them. A loop that
    /// doesn't close, or branches, is left out.
    pub fn border_loops(&self, edges: &BTreeSet<[u32; 2]>) -> Vec<Vec<u32>> {
        let directed = self.directed();
        let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(a, b) in directed.keys() {
            if edges.contains(&key(a, b)) && !directed.contains_key(&(b, a)) {
                next.entry(a).or_default().push(b);
            }
        }
        let mut starts: Vec<u32> = next.keys().copied().collect();
        starts.sort_unstable();
        let mut used: HashSet<u32> = HashSet::new();
        let mut loops = Vec::new();
        'start: for start in starts {
            if used.contains(&start) {
                continue;
            }
            let mut path = vec![start];
            let mut at = start;
            loop {
                let Some([to]) = next.get(&at).map(Vec::as_slice) else {
                    continue 'start;
                };
                if *to == start {
                    break;
                }
                if path.contains(to) {
                    continue 'start;
                }
                path.push(*to);
                at = *to;
            }
            used.extend(&path);
            if path.len() >= 3 {
                loops.push(path);
            }
        }
        loops
    }

    /// Puts the points along each edge (`points[[a, b]]`, from the smaller
    /// end `a` to `b`) into every face along it but those in `skip`, with
    /// uvs carried along the edge.
    pub fn insert_edge_points(
        &mut self,
        points: &HashMap<[u32; 2], Vec<u32>>,
        skip: &HashSet<usize>,
    ) {
        if points.is_empty() {
            return;
        }
        for index in 0..self.faces.len() {
            if skip.contains(&index) {
                continue;
            }
            let face = &self.faces[index];
            let mut corners = Vec::new();
            let mut uvs = Vec::new();
            for k in 0..face.corners.len() {
                let (a, b) = face.edge(k);
                corners.push(a);
                uvs.push(face.uvs[k]);
                let Some(along) = points.get(&key(a, b)) else {
                    continue;
                };
                let (pa, pb) = (self.position(a), self.position(b));
                let (ua, ub) = (face.uvs[k], face.uvs[(k + 1) % face.corners.len()]);
                let mut ordered = along.clone();
                if a > b {
                    ordered.reverse();
                }
                for point in ordered {
                    corners.push(point);
                    uvs.push(ua.lerp(ub, along_edge(pa, pb, self.position(point))));
                }
            }
            if corners.len() != self.faces[index].corners.len() {
                let face = &mut self.faces[index];
                face.corners = corners;
                face.uvs = uvs;
                face.normals.clear();
            }
        }
    }

    /// A uv for each vertex, from some face using it.
    pub fn vertex_uvs(&self) -> HashMap<u32, Vec2> {
        let mut uvs = HashMap::new();
        for face in &self.faces {
            for (&corner, &uv) in face.corners.iter().zip(&face.uvs) {
                uvs.entry(corner).or_insert(uv);
            }
        }
        uvs
    }

    /// Builds the mesh: faces cleaned of repeated corners, unused vertices
    /// dropped (unless `keep_loose`), and changed faces and the smooth
    /// faces next to them shaded again.
    fn build(self, keep_loose: bool) -> Built {
        let Soup {
            positions,
            faces,
            auto_smooth,
        } = self;
        let mut cleaned: Vec<SoupFace> = Vec::new();
        let mut face_map = vec![None; faces.len()];
        for (index, face) in faces.into_iter().enumerate() {
            let pieces = face.cleaned();
            if !pieces.is_empty() {
                face_map[index] = Some(cleaned.len() as u32);
            }
            cleaned.extend(pieces);
        }
        let mut used = vec![keep_loose; positions.len()];
        for face in &cleaned {
            for &corner in &face.corners {
                used[corner as usize] = true;
            }
        }
        let mut vertex_map = vec![None; positions.len()];
        let mut kept = Vec::new();
        for (vertex, &used) in used.iter().enumerate() {
            if used {
                vertex_map[vertex] = Some(kept.len() as u32);
                kept.push(positions[vertex]);
            }
        }
        let polygons: Vec<Polygon> = cleaned
            .into_iter()
            .map(|face| Polygon {
                corners: face
                    .corners
                    .iter()
                    .map(|&c| vertex_map[c as usize].expect("a used vertex"))
                    .collect(),
                uvs: face.uvs,
                normals: face.normals,
                material: face.material,
                smooth: face.smooth,
            })
            .collect();
        let mut mesh = EditMesh::new(kept, &polygons);
        mesh.set_auto_smooth(auto_smooth);
        let mut changed: Vec<u32> = polygons
            .iter()
            .filter(|polygon| polygon.normals.is_empty())
            .flat_map(|polygon| polygon.corners.iter().copied())
            .collect();
        changed.sort_unstable();
        changed.dedup();
        mesh.reshade_around(&changed);
        Built {
            mesh,
            vertex_map,
            face_map,
        }
    }
}

/// How far along the edge from `a` to `b` the point `p` is (0 to 1).
pub(crate) fn along_edge(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    let ab = b - a;
    ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0)
}

/// A built mesh, and where the [`Soup`]'s vertices and faces went in it.
struct Built {
    mesh: EditMesh,
    vertex_map: Vec<Option<u32>>,
    face_map: Vec<Option<u32>>,
}

impl Built {
    /// `picked` in the new mesh's numbers, as a selection in `mode`.
    fn selection(&self, mode: SelectMode, picked: &Picked) -> Selection {
        let mesh = &self.mesh;
        let vertex = |v: u32| self.vertex_map.get(v as usize).copied().flatten();
        let faces: BTreeSet<u32> = picked
            .faces
            .iter()
            .filter_map(|&f| self.face_map.get(f).copied().flatten())
            .collect();
        let outgoing = mesh.outgoing();
        let edges: BTreeSet<u32> = picked
            .edges
            .iter()
            .filter_map(|&[a, b]| {
                let (a, b) = (vertex(a)?, vertex(b)?);
                outgoing[a as usize]
                    .iter()
                    .chain(&outgoing[b as usize])
                    .copied()
                    .find(|&e| {
                        let ends = mesh.ends(e);
                        ends == (a, b) || ends == (b, a)
                    })
                    .map(|e| mesh.edge_key(e))
            })
            .collect();
        let mut vertices: BTreeSet<u32> =
            picked.vertices.iter().filter_map(|&v| vertex(v)).collect();
        for &face in &faces {
            vertices.extend(mesh.face_vertices(face));
        }
        for &edge in &edges {
            let (a, b) = mesh.ends(edge);
            vertices.extend([a, b]);
        }
        let mut selection = Selection {
            mode,
            ..Selection::default()
        };
        match mode {
            SelectMode::Face if !faces.is_empty() => selection.faces = faces,
            SelectMode::Edge if !edges.is_empty() && faces.is_empty() => selection.edges = edges,
            SelectMode::Vertex => selection.vertices = vertices,
            _ => selection.set_covered(mesh, &vertices),
        }
        selection
    }
}
