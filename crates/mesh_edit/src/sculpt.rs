//! Sculpting (Phase 60): brushes that push an object's surface about, as
//! clay is pushed, on a copy of its mesh made fast for that.
//!
//! A [`Sculpt`] is the object's vertices (the same numbers as its
//! [`EditMesh`]'s), its faces and those cut into triangles, with each
//! vertex's neighbours (along the faces' edges), faces and triangles, a
//! smooth normal per vertex (from its faces, so a quad's diagonal doesn't
//! tip it, and mirrored faces give mirrored normals), a mask, and a
//! bounding volume hierarchy (`bvh.rs`) so a brush only looks at the
//! triangles near it. The triangles are split into chunks of the hierarchy
//! (at most [`CHUNK_TRIANGLES`] each), which the Studio draws as meshes of
//! their own: a dab marks only the chunks it touched to be sent to the GPU
//! again ([`Sculpt::take_dirty_chunks`]).
//!
//! - Brushes (`brush.rs`): [`SculptTool`]s, each [`Sculpt::dab`] moving
//!   the vertices within its radius by the falloff, held back by the mask.
//! - Masks: painted by the Mask brush; [`Sculpt::invert_mask`],
//!   [`Sculpt::clear_mask`], and [`Sculpt::hide_masked`] (which brushes
//!   and the view then pass over until [`Sculpt::reveal`]).
//! - Detail (`remesh.rs`): [`voxel_remesh`] rebuilds the surface as even
//!   quads a voxel apart, and [`subdivide`] splits every face into four.
//!
//! [`Sculpt::write_back`] puts the moved vertices into the mesh, which a
//! stroke's undo step keeps.

mod brush;
pub(crate) mod bvh;
pub(crate) mod remesh;

pub use brush::{SculptBrush, SculptDab, SculptTool};
pub use remesh::{MAX_VOXELS, subdivide, voxel_remesh};

use glam::Vec3;

use crate::half_edge::EditMesh;
pub(crate) use bvh::Bvh;
#[cfg(test)]
pub(crate) use bvh::hit;

/// Triangles in a chunk drawn as one mesh, at most.
pub const CHUNK_TRIANGLES: usize = 4096;

/// A mask at least this strong counts as masked, for drawing and hiding.
const MASKED: f32 = 0.5;

/// Lists per vertex, packed into one: `items[offsets[v]..offsets[v + 1]]`.
#[derive(Clone, Debug, Default)]
struct Adjacency {
    offsets: Vec<u32>,
    items: Vec<u32>,
}

impl Adjacency {
    fn of(&self, vertex: u32) -> &[u32] {
        let v = vertex as usize;
        &self.items[self.offsets[v] as usize..self.offsets[v + 1] as usize]
    }
}

/// Part of the triangles, drawn as one mesh.
#[derive(Clone, Debug, Default)]
pub struct Chunk {
    /// The vertices its triangles use: its corners, in this order.
    pub vertices: Vec<u32>,
    triangles: Vec<u32>,
    /// Its shown triangles, three corners each (indices into `vertices`),
    /// those not masked and those masked.
    pub clay: Vec<u32>,
    pub masked: Vec<u32>,
}

/// An object's mesh being sculpted.
#[derive(Clone, Debug, Default)]
pub struct Sculpt {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    /// Each face's corners.
    faces: Adjacency,
    vertex_faces: Adjacency,
    vertex_triangles: Adjacency,
    neighbors: Adjacency,
    /// 0 is free, 1 doesn't move at all.
    mask: Vec<f32>,
    /// Per triangle: hidden ones aren't drawn or touched by brushes.
    hidden: Vec<bool>,
    bvh: Bvh,
    chunks: Vec<Chunk>,
    chunk_of: Vec<u32>,
    dirty: Vec<bool>,
    /// Vertices moved since the last `write_back`.
    moved: Vec<bool>,
    moved_list: Vec<u32>,
    /// Scratch: a vertex is in the current set while its `seen` is
    /// `generation` (so the sets need no clearing).
    seen: Vec<u32>,
    generation: u32,
    /// Scratch: each vertex's number in the chunk being indexed.
    local: Vec<u32>,
    /// The Grab brush's hold, from the stroke's first dab, for each
    /// mirrored copy.
    grabbed: Option<Vec<Held>>,
}

/// What one copy of a Grab dab holds: vertices and their weights, and the
/// axes the copy is mirrored on.
type Held = (Vec<(u32, f32)>, [bool; 3]);

impl Sculpt {
    /// `mesh`'s vertices and faces (cut into triangles), ready to sculpt.
    pub fn new(mesh: &EditMesh) -> Sculpt {
        let positions = mesh.positions().to_vec();
        let triangles: Vec<[u32; 3]> = (0..mesh.face_count() as u32)
            .flat_map(|face| mesh.triangles(face))
            .map(|corners| corners.map(|e| mesh.half_edge(e).vertex))
            .collect();
        let count = positions.len();
        let mut faces = Adjacency {
            offsets: vec![0],
            items: Vec::with_capacity(mesh.half_edges().len()),
        };
        for face in 0..mesh.face_count() as u32 {
            faces.items.extend(mesh.face_vertices(face));
            faces.offsets.push(faces.items.len() as u32);
        }
        let face_count = mesh.face_count() as u32;
        let vertex_faces = packed(
            count,
            (0..face_count).flat_map(|f| faces.of(f).iter().map(move |&v| (v, f))),
        );
        let vertex_triangles = packed(
            count,
            triangles
                .iter()
                .enumerate()
                .flat_map(|(t, corners)| corners.map(|v| (v, t as u32))),
        );
        let mut neighbors = Adjacency {
            offsets: vec![0],
            items: Vec::with_capacity(triangles.len() * 3),
        };
        let mut around = Vec::new();
        for vertex in 0..count as u32 {
            around.clear();
            for &f in vertex_faces.of(vertex) {
                let corners = faces.of(f);
                let k = corners.iter().position(|&v| v == vertex).unwrap_or(0);
                let n = corners.len();
                around.push(corners[(k + 1) % n]);
                around.push(corners[(k + n - 1) % n]);
            }
            around.sort_unstable();
            around.dedup();
            neighbors.items.extend_from_slice(&around);
            neighbors.offsets.push(neighbors.items.len() as u32);
        }
        let bvh = Bvh::build(&positions, &triangles);
        let mut sculpt = Sculpt {
            normals: vec![Vec3::ZERO; count],
            mask: vec![0.0; count],
            hidden: vec![false; triangles.len()],
            chunk_of: vec![0; triangles.len()],
            moved: vec![false; count],
            seen: vec![0; count],
            local: vec![0; count],
            positions,
            triangles,
            faces,
            vertex_faces,
            vertex_triangles,
            neighbors,
            bvh,
            ..Sculpt::default()
        };
        for vertex in 0..count as u32 {
            sculpt.normals[vertex as usize] = sculpt.vertex_normal(vertex);
        }
        for run in sculpt.bvh.runs(CHUNK_TRIANGLES) {
            let index = sculpt.chunks.len() as u32;
            let triangles = sculpt.bvh.order[run].to_vec();
            for &t in &triangles {
                sculpt.chunk_of[t as usize] = index;
            }
            let generation = sculpt.next_generation();
            let mut vertices = Vec::new();
            for &t in &triangles {
                for v in sculpt.triangles[t as usize] {
                    if sculpt.seen[v as usize] != generation {
                        sculpt.seen[v as usize] = generation;
                        vertices.push(v);
                    }
                }
            }
            sculpt.chunks.push(Chunk {
                vertices,
                triangles,
                ..Chunk::default()
            });
        }
        sculpt.dirty = vec![true; sculpt.chunks.len()];
        for chunk in 0..sculpt.chunks.len() {
            sculpt.index_chunk(chunk);
        }
        sculpt
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }

    pub fn normals(&self) -> &[Vec3] {
        &self.normals
    }

    pub fn triangles(&self) -> &[[u32; 3]] {
        &self.triangles
    }

    pub fn mask(&self) -> &[f32] {
        &self.mask
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// True if triangle `t` is hidden.
    pub fn is_hidden(&self, t: u32) -> bool {
        self.hidden[t as usize]
    }

    /// How many triangles are hidden.
    pub fn hidden_count(&self) -> usize {
        self.hidden.iter().filter(|&&h| h).count()
    }

    /// The chunks changed since the last call: their corners (or, after a
    /// mask or hide, their triangles) need sending to the GPU again.
    pub fn take_dirty_chunks(&mut self) -> Vec<usize> {
        let dirty: Vec<usize> = (0..self.dirty.len()).filter(|&c| self.dirty[c]).collect();
        self.dirty.fill(false);
        dirty
    }

    /// Chunk `chunk`'s corners: positions and normals, in its vertices'
    /// order.
    pub fn chunk_corners(&self, chunk: usize) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        let vertices = &self.chunks[chunk].vertices;
        let positions = vertices
            .iter()
            .map(|&v| self.positions[v as usize].to_array())
            .collect();
        let normals = vertices
            .iter()
            .map(|&v| self.normals[v as usize].to_array())
            .collect();
        (positions, normals)
    }

    /// The nearest shown triangle `origin + t·direction` hits, and t.
    pub fn ray(&self, origin: Vec3, direction: Vec3) -> Option<(u32, f32)> {
        self.bvh
            .ray(&self.positions, &self.triangles, origin, direction, |t| {
                self.hidden[t as usize]
            })
    }

    /// Triangle `t`'s normal (one unit long, or zero if it has no area).
    pub fn triangle_normal(&self, t: u32) -> Vec3 {
        let [a, b, c] = self.triangles[t as usize].map(|v| self.positions[v as usize]);
        (b - a).cross(c - a).normalize_or_zero()
    }

    /// Puts the vertices moved since the last call into `mesh` (the mesh
    /// this was made from), which shades the faces round them again.
    /// False if none moved.
    pub fn write_back(&mut self, mesh: &mut EditMesh) -> bool {
        if self.moved_list.is_empty() || mesh.vertex_count() != self.positions.len() {
            return false;
        }
        let vertices = std::mem::take(&mut self.moved_list);
        let positions: Vec<Vec3> = vertices
            .iter()
            .map(|&v| self.positions[v as usize])
            .collect();
        for &v in &vertices {
            self.moved[v as usize] = false;
        }
        mesh.set_positions(&vertices, &positions);
        true
    }

    /// Puts `mask` in place of the mask (one per vertex; ignored if the
    /// count is wrong), as kept across a rebuild of the same vertices.
    pub fn set_mask(&mut self, mask: &[f32]) {
        if mask.len() == self.mask.len() {
            self.mask.copy_from_slice(mask);
            self.reindex_all();
        }
    }

    /// Masked becomes free and free masked.
    pub fn invert_mask(&mut self) {
        for value in &mut self.mask {
            *value = 1.0 - *value;
        }
        self.reindex_all();
    }

    pub fn clear_mask(&mut self) {
        self.mask.fill(0.0);
        self.reindex_all();
    }

    /// Hides each triangle with a masked corner. False if none was.
    pub fn hide_masked(&mut self) -> bool {
        let mut any = false;
        for (t, corners) in self.triangles.iter().enumerate() {
            if corners.iter().any(|&v| self.mask[v as usize] >= MASKED) && !self.hidden[t] {
                self.hidden[t] = true;
                any = true;
            }
        }
        if any {
            self.reindex_all();
        }
        any
    }

    /// Shows every hidden triangle again.
    pub fn reveal(&mut self) {
        self.hidden.fill(false);
        self.reindex_all();
    }

    fn reindex_all(&mut self) {
        for chunk in 0..self.chunks.len() {
            self.index_chunk(chunk);
            self.dirty[chunk] = true;
        }
    }

    /// Works out chunk `chunk`'s shown triangles, masked and not.
    fn index_chunk(&mut self, chunk: usize) {
        let Chunk {
            vertices,
            triangles,
            ..
        } = &self.chunks[chunk];
        // Every corner read below is one of `vertices`, so `local` needs
        // no clearing.
        for (local, &v) in vertices.iter().enumerate() {
            self.local[v as usize] = local as u32;
        }
        let (mut clay, mut masked) = (Vec::new(), Vec::new());
        for &t in triangles {
            if self.hidden[t as usize] {
                continue;
            }
            let corners = self.triangles[t as usize];
            let mask: f32 = corners.iter().map(|&v| self.mask[v as usize]).sum::<f32>() / 3.0;
            let list = if mask >= MASKED {
                &mut masked
            } else {
                &mut clay
            };
            list.extend(corners.map(|v| self.local[v as usize]));
        }
        let chunk = &mut self.chunks[chunk];
        chunk.clay = clay;
        chunk.masked = masked;
    }

    /// A new generation for the `seen` set.
    fn next_generation(&mut self) -> u32 {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        self.generation
    }

    /// The area-weighted normal of the faces round `vertex`.
    fn vertex_normal(&self, vertex: u32) -> Vec3 {
        self.vertex_faces
            .of(vertex)
            .iter()
            .map(|&f| {
                // Newell's method: the face's area times its normal, the
                // same however the face would be cut into triangles.
                let corners = self.faces.of(f);
                let mut sum = Vec3::ZERO;
                for (k, &v) in corners.iter().enumerate() {
                    let a = self.positions[v as usize];
                    let b = self.positions[corners[(k + 1) % corners.len()] as usize];
                    sum += a.cross(b);
                }
                sum
            })
            .sum::<Vec3>()
            .normalize_or_zero()
    }

    /// After `moved` vertices moved: their and their neighbours' normals,
    /// the hierarchy's boxes, and the chunks to draw again.
    fn after_move(&mut self, moved: &[u32]) {
        let generation = self.next_generation();
        let mut reshade = Vec::with_capacity(moved.len() * 2);
        let mut changed = Vec::with_capacity(moved.len() * 2);
        for &v in moved {
            if !self.moved[v as usize] {
                self.moved[v as usize] = true;
                self.moved_list.push(v);
            }
            changed.extend_from_slice(self.vertex_triangles.of(v));
            for &f in self.vertex_faces.of(v) {
                for &corner in self.faces.of(f) {
                    if self.seen[corner as usize] != generation {
                        self.seen[corner as usize] = generation;
                        reshade.push(corner);
                    }
                }
            }
        }
        for &v in &reshade {
            self.normals[v as usize] = self.vertex_normal(v);
            for &t in self.vertex_triangles.of(v) {
                self.dirty[self.chunk_of[t as usize] as usize] = true;
            }
        }
        changed.sort_unstable();
        changed.dedup();
        self.bvh.refit(changed, &self.positions, &self.triangles);
    }

    /// The shown vertices within `radius` of `center`, and how far out
    /// each is (0 the middle, 1 the edge), with `metric` measuring.
    fn gather(
        &mut self,
        center: Vec3,
        radius: f32,
        metric: impl Fn(Vec3) -> f32,
    ) -> Vec<(u32, f32)> {
        let mut triangles = Vec::new();
        self.bvh.sphere(center, radius, &mut triangles);
        let generation = self.next_generation();
        let mut found = Vec::new();
        for t in triangles {
            if self.hidden[t as usize] {
                continue;
            }
            for v in self.triangles[t as usize] {
                if self.seen[v as usize] == generation {
                    continue;
                }
                self.seen[v as usize] = generation;
                let out = metric(self.positions[v as usize]);
                if out < 1.0 {
                    found.push((v, out));
                }
            }
        }
        // In vertex order, so a dab's sums come out the same however the
        // hierarchy was walked.
        found.sort_unstable_by_key(|&(v, _)| v);
        found
    }
}

/// `pairs` of (vertex, item) as lists per vertex, items in order.
fn packed(count: usize, pairs: impl Iterator<Item = (u32, u32)> + Clone) -> Adjacency {
    let mut offsets = vec![0u32; count + 1];
    for (v, _) in pairs.clone() {
        offsets[v as usize + 1] += 1;
    }
    for v in 0..count {
        offsets[v + 1] += offsets[v];
    }
    let mut next = offsets.clone();
    let mut items = vec![0; offsets[count] as usize];
    for (v, item) in pairs {
        items[next[v as usize] as usize] = item;
        next[v as usize] += 1;
    }
    Adjacency { offsets, items }
}
