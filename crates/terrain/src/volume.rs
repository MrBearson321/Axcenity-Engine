//! Terrain volumes, a spike (procedural worlds PW8, Option B in
//! `docs/research/procedural-worlds.md`): a voxel grid of signed distances
//! that can hold what a heightfield can't (caves, arches, overhangs), meshed
//! by surface nets.
//!
//! This is the spike's engine half: the data, brushes, the mesher and
//! memory accounting, measured by `bench_volume_spike`. It isn't an
//! Instance yet, isn't saved, drawn or sent; whether to build that is
//! decided from the benchmark's numbers.
//!
//! - Samples sit on the integer grid, `spacing` studs apart. A sample's
//!   value is the distance to the surface in voxels, negative inside solid,
//!   stored as `i8` in sixteenths of a voxel (so ±7.9 voxels; farther is
//!   clamped, which is all a surface needs).
//! - Chunks are 32³ samples. A chunk that is all one value (all air or all
//!   rock) costs nothing: only chunks the surface passes through hold their
//!   32 KB of samples.
//! - Outside the volume's box is air, so the surface closes at its edges.

use std::collections::HashMap;

use glam::{IVec3, Vec3};

use crate::mesh::ChunkMesh;

/// Samples along a chunk's side.
pub const CHUNK: i32 = 32;
const CHUNK_SAMPLES: usize = (CHUNK * CHUNK * CHUNK) as usize;
/// Steps of a stored value per voxel of distance.
const STEPS: f32 = 16.0;
/// The value of plain air and of plain rock.
pub const AIR: i8 = i8::MAX;
pub const ROCK: i8 = i8::MIN;

/// A distance in voxels as a stored value.
/// Never 0: a sample exactly on the surface would put the crossings of all
/// its edges on it, and the vertices of the cells round it in one place.
fn quantize(distance: f32) -> i8 {
    let value = (distance * STEPS)
        .round()
        .clamp(f32::from(ROCK), f32::from(AIR)) as i8;
    match value {
        0 if distance < 0.0 => -1,
        0 => 1,
        value => value,
    }
}

/// A stored value as a distance in voxels.
fn distance(value: i8) -> f32 {
    f32::from(value) / STEPS
}

/// One chunk's samples: one value for all, or each its own.
#[derive(Clone, Debug)]
enum Chunk {
    Uniform(i8),
    Dense(Box<[i8; CHUNK_SAMPLES]>),
}

impl Chunk {
    fn get(&self, local: IVec3) -> i8 {
        match self {
            Chunk::Uniform(value) => *value,
            Chunk::Dense(samples) => samples[index(local)],
        }
    }

    fn set(&mut self, local: IVec3, value: i8) {
        if let Chunk::Uniform(uniform) = *self {
            if uniform == value {
                return;
            }
            *self = Chunk::Dense(Box::new([uniform; CHUNK_SAMPLES]));
        }
        if let Chunk::Dense(samples) = self {
            samples[index(local)] = value;
        }
    }

    /// Back to one value if every sample is plain air or plain rock.
    fn compact(&mut self) {
        if let Chunk::Dense(samples) = self {
            let first = samples[0];
            if (first == AIR || first == ROCK) && samples.iter().all(|&v| v == first) {
                *self = Chunk::Uniform(first);
            }
        }
    }
}

fn index(local: IVec3) -> usize {
    (local.x + CHUNK * (local.y + CHUNK * local.z)) as usize
}

/// A volume of rock and air, `size` samples along each side.
#[derive(Clone, Debug)]
pub struct VolumeData {
    size: IVec3,
    /// Studs between samples.
    pub spacing: f32,
    chunks: Vec<Chunk>,
    /// Each chunk's version: bumped when one of its samples changes.
    versions: Vec<u64>,
}

impl VolumeData {
    /// All air, `size` samples along each side (each at least 2).
    pub fn new(size: IVec3, spacing: f32) -> Self {
        let size = size.max(IVec3::splat(2));
        let chunks = chunk_count(size);
        let total = (chunks.x * chunks.y * chunks.z) as usize;
        VolumeData {
            size,
            spacing: spacing.max(0.01),
            chunks: vec![Chunk::Uniform(AIR); total],
            versions: vec![0; total],
        }
    }

    pub fn size(&self) -> IVec3 {
        self.size
    }

    /// Chunks along each side.
    pub fn chunks(&self) -> IVec3 {
        chunk_count(self.size)
    }

    /// Every chunk's position in chunks.
    pub fn chunk_coords(&self) -> impl Iterator<Item = IVec3> {
        let n = self.chunks();
        (0..n.z).flat_map(move |z| {
            (0..n.y).flat_map(move |y| (0..n.x).map(move |x| IVec3::new(x, y, z)))
        })
    }

    fn chunk_index(&self, chunk: IVec3) -> Option<usize> {
        let n = self.chunks();
        (chunk.cmpge(IVec3::ZERO).all() && chunk.cmplt(n).all())
            .then(|| (chunk.x + n.x * (chunk.y + n.y * chunk.z)) as usize)
    }

    pub fn chunk_version(&self, chunk: IVec3) -> u64 {
        self.chunk_index(chunk).map_or(0, |i| self.versions[i])
    }

    /// Whether the chunk holds samples of its own (the surface may pass
    /// through it).
    pub fn chunk_is_dense(&self, chunk: IVec3) -> bool {
        self.chunk_index(chunk)
            .is_some_and(|i| matches!(self.chunks[i], Chunk::Dense(_)))
    }

    fn inside(&self, at: IVec3) -> bool {
        at.cmpge(IVec3::ZERO).all() && at.cmplt(self.size).all()
    }

    /// The stored value at a sample; air off the grid.
    pub fn get(&self, at: IVec3) -> i8 {
        if !self.inside(at) {
            return AIR;
        }
        let chunk = at.div_euclid(IVec3::splat(CHUNK));
        let i = self.chunk_index(chunk).expect("inside");
        self.chunks[i].get(at - chunk * CHUNK)
    }

    /// The distance in voxels at a sample (negative in rock).
    pub fn distance(&self, at: IVec3) -> f32 {
        distance(self.get(at))
    }

    fn set(&mut self, at: IVec3, value: i8) {
        if !self.inside(at) {
            return;
        }
        let chunk = at.div_euclid(IVec3::splat(CHUNK));
        let i = self.chunk_index(chunk).expect("inside");
        if self.chunks[i].get(at - chunk * CHUNK) != value {
            self.chunks[i].set(at - chunk * CHUNK, value);
            self.versions[i] += 1;
        }
    }

    /// Sets every sample from `distance(position in voxels)`, a distance
    /// in voxels (negative in rock). Used to start a volume from a shape,
    /// such as rock up to a heightfield.
    pub fn fill(&mut self, distance: impl Fn(Vec3) -> f32) {
        for chunk in self.chunk_coords().collect::<Vec<_>>() {
            let base = chunk * CHUNK;
            let i = self.chunk_index(chunk).expect("listed");
            let mut samples = Box::new([AIR; CHUNK_SAMPLES]);
            for z in 0..CHUNK {
                for y in 0..CHUNK {
                    for x in 0..CHUNK {
                        let local = IVec3::new(x, y, z);
                        let at = base + local;
                        if self.inside(at) {
                            samples[index(local)] = quantize(distance(at.as_vec3()));
                        }
                    }
                }
            }
            let mut filled = Chunk::Dense(samples);
            filled.compact();
            self.chunks[i] = filled;
            self.versions[i] += 1;
        }
    }

    /// Adds rock (`carve` false) or air (`carve` true) in a ball, center
    /// and radius in voxels. Returns the chunks it changed.
    pub fn sphere(&mut self, center: Vec3, radius: f32, carve: bool) -> Vec<IVec3> {
        let reach = radius + 8.0;
        let low = (center - reach).floor().as_ivec3().max(IVec3::ZERO);
        let high = (center + reach).ceil().as_ivec3().min(self.size - 1);
        let mut touched = Vec::new();
        for z in low.z..=high.z {
            for y in low.y..=high.y {
                for x in low.x..=high.x {
                    let at = IVec3::new(x, y, z);
                    let ball = at.as_vec3().distance(center) - radius;
                    let old = self.distance(at);
                    let new = if carve { old.max(-ball) } else { old.min(ball) };
                    let value = quantize(new);
                    if value != self.get(at) {
                        self.set(at, value);
                        let chunk = at.div_euclid(IVec3::splat(CHUNK));
                        if !touched.contains(&chunk) {
                            touched.push(chunk);
                        }
                    }
                }
            }
        }
        for &chunk in &touched {
            let i = self.chunk_index(chunk).expect("touched");
            self.chunks[i].compact();
        }
        touched
    }

    /// Bytes the samples take (the chunks that hold their own).
    pub fn memory_bytes(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| match chunk {
                Chunk::Uniform(_) => std::mem::size_of::<Chunk>(),
                Chunk::Dense(_) => std::mem::size_of::<Chunk>() + CHUNK_SAMPLES,
            })
            .sum::<usize>()
            + self.versions.len() * 8
    }

    /// How many chunks hold their own samples.
    pub fn dense_chunks(&self) -> usize {
        self.chunks
            .iter()
            .filter(|chunk| matches!(chunk, Chunk::Dense(_)))
            .count()
    }

    /// Whether every sample a chunk's mesh reads (its own and one more
    /// each way) is the same plain air or plain rock, so it has no surface.
    fn chunk_is_empty_of_surface(&self, chunk: IVec3) -> bool {
        let mut value = None;
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let near = chunk + IVec3::new(dx, dy, dz);
                    let here = match self.chunk_index(near) {
                        Some(i) => match &self.chunks[i] {
                            Chunk::Uniform(v) => *v,
                            Chunk::Dense(_) => return false,
                        },
                        None => AIR,
                    };
                    if *value.get_or_insert(here) != here {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// The surface through one chunk, by surface nets: one vertex in each
    /// cell the surface crosses, at the average of its edge crossings, and
    /// a quad across each crossed edge. A chunk owns the edges that start
    /// on its own samples, and reads its neighbours' samples for the rest,
    /// so neighbouring chunks meet without gaps or doubled faces.
    /// Positions are in studs from the volume's corner.
    pub fn mesh_chunk(&self, chunk: IVec3) -> ChunkMesh {
        let mut mesh = ChunkMesh::default();
        if self.chunk_is_empty_of_surface(chunk) {
            return mesh;
        }
        let base = chunk * CHUNK;
        // Cells from one before the chunk to its end: (CHUNK + 1)³.
        let side = CHUNK + 1;
        let cell_index = |cell: IVec3| {
            let local = cell - (base - IVec3::ONE);
            (local.x + side * (local.y + side * local.z)) as usize
        };
        // Samples, read once: (CHUNK + 2)³ from one before to one after.
        let reach = CHUNK + 2;
        let mut samples = vec![0.0f32; (reach * reach * reach) as usize];
        let sample_index = |at: IVec3| {
            let local = at - (base - IVec3::ONE);
            (local.x + reach * (local.y + reach * local.z)) as usize
        };
        for z in -1..CHUNK + 1 {
            for y in -1..CHUNK + 1 {
                for x in -1..CHUNK + 1 {
                    let at = base + IVec3::new(x, y, z);
                    samples[sample_index(at)] = self.distance(at);
                }
            }
        }
        let value = |at: IVec3| samples[sample_index(at)];

        const CORNERS: [IVec3; 8] = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(1, 1, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(1, 0, 1),
            IVec3::new(0, 1, 1),
            IVec3::new(1, 1, 1),
        ];
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let mut vertex_of = vec![u32::MAX; (side * side * side) as usize];
        for z in -1..CHUNK {
            for y in -1..CHUNK {
                for x in -1..CHUNK {
                    let cell = base + IVec3::new(x, y, z);
                    let d = CORNERS.map(|c| value(cell + c));
                    let inside = d.iter().filter(|&&v| v < 0.0).count();
                    if inside == 0 || inside == 8 {
                        continue;
                    }
                    let mut sum = Vec3::ZERO;
                    let mut crossings = 0.0;
                    for (a, b) in EDGES {
                        if (d[a] < 0.0) != (d[b] < 0.0) {
                            let t = d[a] / (d[a] - d[b]);
                            sum += CORNERS[a].as_vec3().lerp(CORNERS[b].as_vec3(), t);
                            crossings += 1.0;
                        }
                    }
                    let local = sum / crossings;
                    // The gradient across the cell points out of the rock.
                    let gradient = Vec3::new(
                        (d[1] - d[0]) + (d[3] - d[2]) + (d[5] - d[4]) + (d[7] - d[6]),
                        (d[2] - d[0]) + (d[3] - d[1]) + (d[6] - d[4]) + (d[7] - d[5]),
                        (d[4] - d[0]) + (d[5] - d[1]) + (d[6] - d[2]) + (d[7] - d[3]),
                    )
                    .normalize_or(Vec3::Y);
                    vertex_of[cell_index(cell)] = mesh.positions.len() as u32;
                    let position = (cell.as_vec3() + local) * self.spacing;
                    mesh.positions.push(position.to_array());
                    mesh.normals.push(gradient.to_array());
                    mesh.uvs.push([position.x, position.z]);
                }
            }
        }

        // A quad across each crossed edge this chunk owns. The four cells
        // round an edge along `axis` from sample `at` are at `at` minus
        // nothing or one along each of the other two axes.
        // The first chunk along an axis also owns the edges from the air
        // just outside the volume, so its low side is closed.
        for axis in 0..3 {
            let step = IVec3::AXES[axis];
            let from = |a: usize| if a == axis && chunk[a] == 0 { -1 } else { 0 };
            for z in from(2)..CHUNK {
                for y in from(1)..CHUNK {
                    for x in from(0)..CHUNK {
                        let at = base + IVec3::new(x, y, z);
                        let here = value(at);
                        let there = value(at + step);
                        if (here < 0.0) == (there < 0.0) {
                            continue;
                        }
                        let (u, v) = (IVec3::AXES[(axis + 1) % 3], IVec3::AXES[(axis + 2) % 3]);
                        let cells = [at, at - u, at - u - v, at - v];
                        let ids = cells.map(|cell| vertex_of[cell_index(cell)]);
                        if ids.contains(&u32::MAX) {
                            continue;
                        }
                        // Face out of the rock: toward +axis when the rock is
                        // at `at`.
                        let out_positive = here < 0.0;
                        let [a, b, c, d] = ids;
                        if out_positive {
                            mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
                        } else {
                            mesh.indices.extend_from_slice(&[a, c, b, a, d, c]);
                        }
                    }
                }
            }
        }
        mesh
    }
}

fn chunk_count(size: IVec3) -> IVec3 {
    (size + CHUNK - 1) / CHUNK
}

/// Meshes of the chunks in `chunks`, by position: what a host keeps and
/// remakes when a brush changes some.
pub fn mesh_all(volume: &VolumeData) -> HashMap<IVec3, ChunkMesh> {
    volume
        .chunk_coords()
        .map(|chunk| (chunk, volume.mesh_chunk(chunk)))
        .filter(|(_, mesh)| !mesh.indices.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;
