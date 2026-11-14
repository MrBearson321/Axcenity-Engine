//! Rays traced in software (RL3, `docs/rendering/ray-tracing.md`): the
//! frame's items within the ray tracing distance as bounding volume
//! hierarchies (BVHs) built on the CPU, for ray traced GI on a GPU without
//! hardware ray queries (`ray_soft.wgsl` walks them in a compute shader).
//!
//! - Each mesh gets a BVH of its own the first frame it's wanted, from its
//!   simplest level of detail (GI's light is soft, so the simplest copy
//!   does), up to [`TRIANGLES_PER_FRAME`] triangles a frame; until then its
//!   items are left out. A caller's mesh whose version changes is built
//!   again.
//! - The items' own BVH (the top one) is made again each frame over their
//!   boxes, and written to the GPU only when it changed.
//! - Everything is one storage buffer of `vec4`s, laid out as
//!   `ray_soft.wgsl` says: the meshes first, rewritten only when one comes
//!   or goes, then the items and the top BVH.
//! - Left out, unlike the hardware's scene: posed (skinned) characters, as
//!   their rest pose would put them in the wrong place; swaying plants keep
//!   their still shape; a cut-out's holes are guessed (half its triangles'
//!   hits are passed).

use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;

use super::ray_cut_outs::has_holes;
use super::ray_tracing::{SurfaceRaw, Surfaces, mask_of, surface_of};
use super::{Draw, GpuMesh, MeshKey};
use crate::draw_list::Shape;

/// The most triangles made into new BVHs in one frame. One mesh bigger
/// than this is still built whole, on a frame of its own.
pub(super) const TRIANGLES_PER_FRAME: u64 = 100_000;

/// Triangles in a mesh BVH's leaf, and items in the top one's.
const LEAF_TRIANGLES: usize = 4;
const LEAF_ITEMS: usize = 2;

/// The deepest a BVH is built: a walk of one needs a stack this deep plus
/// one, within `SOFT_STACK` (32) in ray_soft.wgsl.
const MOST_DEPTH: usize = 28;

/// A mesh's BVH unused for this many frames is freed.
const FORGET_AFTER: u64 = 600;

/// One `vec4` of the scene's buffer.
type Word = [f32; 4];

/// How many items and meshes the software scene holds, for tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SoftStats {
    /// Items in the top BVH.
    pub(super) items: u32,
    /// Meshes with a BVH.
    pub(super) meshes: u32,
    /// Their triangles.
    pub(super) triangles: u64,
    /// Meshes wanted but not built yet (waiting for the per-frame budget,
    /// or past the buffer's size).
    pub(super) meshes_waiting: u32,
}

/// A node of a BVH as it's built: its box, and either its first child's
/// index (`count` 0; the second is next to it) or its first thing's.
#[derive(Clone, Copy, Debug)]
struct Node {
    low: Vec3,
    high: Vec3,
    first: u32,
    count: u32,
}

/// A BVH over the boxes it was built from: its nodes, the first the root,
/// and the boxes' order in its leaves.
struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

/// A mesh's BVH, laid out from 0: its nodes, then its triangles in the
/// order its leaves use them (three words each).
struct MeshBvh {
    words: Vec<Word>,
    nodes: usize,
    triangles: u32,
    /// The mesh's box, in its own space.
    low: Vec3,
    high: Vec3,
    version: u64,
    last_used: u64,
}

/// An item of the frame: its mesh, world-to-mesh matrix, mask, slot and
/// whether it's a cut-out.
#[derive(PartialEq)]
struct Item {
    mesh: MeshKey,
    to_mesh: Mat4,
    low: Vec3,
    high: Vec3,
    mask: u8,
    cut_out: bool,
}

/// The software scene: the meshes' BVHs and the buffer the GPU walks.
pub(super) struct SoftScene {
    meshes: FxHashMap<MeshKey, MeshBvh>,
    /// Where each mesh starts in the buffer (in words), while laid out.
    placed: FxHashMap<MeshKey, u32>,
    /// The meshes' part of the buffer, from word 1, as last written; `None`
    /// once a mesh came or went.
    mesh_words: Option<usize>,
    /// The items' part (the items and the top BVH) as last written.
    tail: Vec<Word>,
    /// The items it was made from.
    items: Vec<Item>,
    buffer: wgpu::Buffer,
    /// Its size in words.
    capacity: usize,
    /// What rays hitting each slot find.
    pub(super) surfaces: Surfaces,
    pub(super) stats: SoftStats,
}

impl SoftScene {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let capacity = 1024;
        Self {
            meshes: FxHashMap::default(),
            placed: FxHashMap::default(),
            mesh_words: None,
            tail: Vec::new(),
            items: Vec::new(),
            buffer: scene_buffer(device, capacity),
            capacity,
            surfaces: Surfaces::new(device),
            stats: SoftStats::default(),
        }
    }

    /// The buffer `ray_soft.wgsl` reads as `soft_scene`.
    pub(super) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Brings the scene up to date with this frame's `draws` within `reach`
    /// of `camera`: new meshes' BVHs within the budget, the items and their
    /// surfaces. `frame` counts frames; `mesh` finds an uploaded mesh, for
    /// its bounds.
    pub(super) fn prepare<'a>(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        (frame, camera, reach): (u64, Vec3, f32),
        draws: &[Draw],
        mesh: impl Fn(MeshKey) -> Option<&'a GpuMesh>,
    ) {
        // The most words the buffer may hold.
        let limits = device.limits();
        let most_words = (limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size)
            / 16) as usize;
        let mut budget = TRIANGLES_PER_FRAME;
        let mut waiting = 0;
        let mut mesh_total: usize = self.meshes.values().map(|bvh| bvh.words.len()).sum();
        let mut items = Vec::new();
        let mut surfaces: Vec<SurfaceRaw> = Vec::new();
        for draw in draws {
            let item = draw.item;
            if draw.fade > 1.0 {
                continue;
            }
            // A posed character's rest pose isn't where it stands.
            if item.skin.is_some() {
                continue;
            }
            let Some(gpu) = mesh(draw.mesh()) else {
                continue;
            };
            let (center, radius) = gpu.bounds.sphere(item.transform);
            if center.distance(camera) - radius > reach {
                continue;
            }
            let to_mesh = item.transform.inverse();
            if !to_mesh.is_finite() {
                continue;
            }
            let Some((key, version, built)) = self.mesh_for(&item.shape, frame) else {
                continue;
            };
            if !built {
                let (triangles, size) = triangles_of(&item.shape);
                // Half the buffer at most, leaving the rest for the items.
                let fits = mesh_total + size < most_words / 2;
                if (triangles > budget && budget < TRIANGLES_PER_FRAME) || !fits {
                    waiting += 1;
                    continue;
                }
                budget = budget.saturating_sub(triangles);
                let built = build_mesh(&item.shape, version, frame);
                mesh_total += built.words.len();
                self.meshes.insert(key, built);
                self.mesh_words = None;
            }
            let bvh = &self.meshes[&key];
            let (low, high) = world_box(item.transform, bvh.low, bvh.high);
            items.push(Item {
                mesh: key,
                to_mesh,
                low,
                high,
                mask: mask_of(&item.material, draw),
                cut_out: has_holes(&item.material),
            });
            surfaces.push(surface_of(
                &item.material,
                &mut self.surfaces.texture_colors,
            ));
        }

        // Meshes nothing has wanted for a while are freed.
        let before = self.meshes.len();
        self.meshes
            .retain(|_, bvh| frame.saturating_sub(bvh.last_used) < FORGET_AFTER);
        if self.meshes.len() != before {
            self.mesh_words = None;
        }

        // The meshes' part, laid out again when one came or went.
        let mut mesh_part = None;
        if self.mesh_words.is_none() {
            let mut words = Vec::new();
            self.placed.clear();
            for (key, bvh) in &self.meshes {
                let base = 1 + words.len() as u32;
                self.placed.insert(*key, base);
                words.extend(bvh.words.iter().enumerate().map(|(index, word)| {
                    // A node's first child or thing moves along with it.
                    if index < bvh.nodes * 2 && index % 2 == 0 {
                        let first = word[3].to_bits() + base;
                        [word[0], word[1], word[2], f32::from_bits(first)]
                    } else {
                        *word
                    }
                }));
            }
            self.mesh_words = Some(words.len());
            mesh_part = Some(words);
        }
        let tail_start = 1 + self.mesh_words.unwrap_or(0);
        // The same items over the same meshes keep their part as it is.
        let unchanged = mesh_part.is_none() && items == self.items;
        let tail = if unchanged {
            std::mem::take(&mut self.tail)
        } else {
            self.tail_words(&items, tail_start as u32)
        };
        let header: Word = if items.is_empty() {
            [0.0; 4]
        } else {
            // The top BVH's root is after the items.
            let root = tail_start + 4 * items.len();
            [
                f32::from_bits(root as u32),
                f32::from_bits(items.len() as u32),
                0.0,
                0.0,
            ]
        };

        let total = tail_start + tail.len();
        if total > self.capacity {
            self.capacity = total.next_power_of_two();
            self.buffer = scene_buffer(device, self.capacity);
            // Everything is written again into the new buffer.
            if mesh_part.is_none() {
                mesh_part = Some(self.laid_out_meshes());
            }
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&header));
        if let Some(words) = &mesh_part
            && !words.is_empty()
        {
            queue.write_buffer(&self.buffer, 16, bytemuck::cast_slice(words));
        }
        if (mesh_part.is_some() || !unchanged) && !tail.is_empty() {
            queue.write_buffer(
                &self.buffer,
                16 * tail_start as u64,
                bytemuck::cast_slice(&tail),
            );
        }
        self.tail = tail;
        let item_count = items.len() as u32;
        self.items = items;
        self.surfaces.write(device, queue, surfaces);
        self.stats = SoftStats {
            items: item_count,
            meshes: self.meshes.len() as u32,
            triangles: self
                .meshes
                .values()
                .map(|bvh| u64::from(bvh.triangles))
                .sum(),
            meshes_waiting: waiting,
        };
    }

    /// The key, version and whether it's built of the mesh `shape` is traced
    /// with (its simplest copy), marking it used this `frame`. `None` for a
    /// mesh with no triangles.
    fn mesh_for(&mut self, shape: &Shape, frame: u64) -> Option<(MeshKey, u64, bool)> {
        let (key, version) = match shape {
            Shape::Mesh(_) => {
                let source = simplest(shape)?;
                if source.indices().len() < 3 {
                    return None;
                }
                (MeshKey::Custom(source.key()), source.version())
            }
            builtin => (MeshKey::of(builtin), 0),
        };
        let built = match self.meshes.get_mut(&key) {
            Some(bvh) if bvh.version == version => {
                bvh.last_used = frame;
                true
            }
            _ => false,
        };
        Some((key, version, built))
    }

    /// The meshes' part as laid out in `placed`.
    fn laid_out_meshes(&self) -> Vec<Word> {
        let mut placed: Vec<_> = self.placed.iter().collect();
        placed.sort_by_key(|(_, base)| **base);
        let mut words = Vec::new();
        for (key, base) in placed {
            let bvh = &self.meshes[key];
            words.extend(bvh.words.iter().enumerate().map(|(index, word)| {
                if index < bvh.nodes * 2 && index % 2 == 0 {
                    [
                        word[0],
                        word[1],
                        word[2],
                        f32::from_bits(word[3].to_bits() + base),
                    ]
                } else {
                    *word
                }
            }));
        }
        words
    }

    /// The items (in the top BVH's order) and the top BVH, from word
    /// `start`.
    fn tail_words(&self, items: &[Item], start: u32) -> Vec<Word> {
        if items.is_empty() {
            return Vec::new();
        }
        let boxes: Vec<(Vec3, Vec3)> = items.iter().map(|item| (item.low, item.high)).collect();
        let bvh = build_bvh(&boxes, LEAF_ITEMS);
        let mut words = Vec::with_capacity(items.len() * 4 + bvh.nodes.len() * 2);
        for &slot in &bvh.order {
            let item = &items[slot as usize];
            let rows = [0, 1, 2].map(|row| item.to_mesh.row(row).to_array());
            words.extend(rows);
            let root = self.placed.get(&item.mesh).copied().unwrap_or(0);
            words.push([
                f32::from_bits(root),
                f32::from_bits(u32::from(item.mask)),
                f32::from_bits(slot),
                f32::from_bits(u32::from(item.cut_out)),
            ]);
        }
        let nodes_start = start + 4 * items.len() as u32;
        for node in &bvh.nodes {
            let first = if node.count == 0 {
                nodes_start + 2 * node.first
            } else {
                start + 4 * node.first
            };
            words.push(node.low.extend(f32::from_bits(first)).to_array());
            words.push(node.high.extend(f32::from_bits(node.count)).to_array());
        }
        words
    }

    /// Bytes of its buffers.
    pub(super) fn bytes(&self) -> u64 {
        self.buffer.size() + self.surfaces.buffer.size()
    }
}

fn scene_buffer(device: &wgpu::Device, words: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("software ray traced scene"),
        size: 16 * words as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The simplest copy of a caller's mesh `shape`: its last level of detail,
/// or itself.
fn simplest(shape: &Shape) -> Option<&dyn crate::MeshSource> {
    let Shape::Mesh(source) = shape else {
        return None;
    };
    let levels = source.lod_count();
    let simplest = (levels > 0)
        .then(|| source.lod(levels))
        .flatten()
        .unwrap_or(source);
    Some(simplest.as_ref())
}

/// A built-in shape's corners and triangles.
fn builtin_mesh(shape: &Shape) -> (Vec<[f32; 3]>, Vec<u32>) {
    let data = match shape {
        Shape::Sphere => crate::mesh::sphere(),
        Shape::Cylinder => crate::mesh::cylinder(),
        Shape::Wedge => crate::mesh::wedge(),
        Shape::CornerWedge => crate::mesh::corner_wedge(),
        _ => crate::mesh::block(),
    };
    (
        data.vertices.iter().map(|vertex| vertex.position).collect(),
        data.indices.iter().map(|&index| u32::from(index)).collect(),
    )
}

/// How many triangles `shape`'s traced mesh has, and about how many words
/// its BVH takes.
fn triangles_of(shape: &Shape) -> (u64, usize) {
    let triangles = match simplest(shape) {
        Some(source) => source.indices().len() / 3,
        // The built-in shapes are a few hundred at most.
        None => 256,
    };
    (triangles as u64, triangles * 4)
}

/// The BVH of `shape`'s traced mesh.
fn build_mesh(shape: &Shape, version: u64, frame: u64) -> MeshBvh {
    let (positions, indices) = match simplest(shape) {
        Some(source) => (source.positions().to_vec(), source.indices().to_vec()),
        None => builtin_mesh(shape),
    };
    let corner = |index: u32| {
        positions
            .get(index as usize)
            .map_or(Vec3::ZERO, |&position| Vec3::from(position))
    };
    let triangles: Vec<[Vec3; 3]> = indices
        .chunks_exact(3)
        .map(|t| [corner(t[0]), corner(t[1]), corner(t[2])])
        .collect();
    let boxes: Vec<(Vec3, Vec3)> = triangles
        .iter()
        .map(|[a, b, c]| (a.min(*b).min(*c), a.max(*b).max(*c)))
        .collect();
    let bvh = build_bvh(&boxes, LEAF_TRIANGLES);
    let triangles_start = 2 * bvh.nodes.len() as u32;
    let mut words = Vec::with_capacity(bvh.nodes.len() * 2 + triangles.len() * 3);
    for node in &bvh.nodes {
        let first = if node.count == 0 {
            2 * node.first
        } else {
            triangles_start + 3 * node.first
        };
        words.push(node.low.extend(f32::from_bits(first)).to_array());
        words.push(node.high.extend(f32::from_bits(node.count)).to_array());
    }
    for &index in &bvh.order {
        let [a, b, c] = triangles[index as usize];
        words.push(a.extend(0.0).to_array());
        words.push((b - a).extend(0.0).to_array());
        words.push((c - a).extend(0.0).to_array());
    }
    let root = bvh.nodes[0];
    MeshBvh {
        words,
        nodes: bvh.nodes.len(),
        triangles: triangles.len() as u32,
        low: root.low,
        high: root.high,
        version,
        last_used: frame,
    }
}

/// The world box of a mesh box `low`..`high` placed by `transform`.
fn world_box(transform: Mat4, low: Vec3, high: Vec3) -> (Vec3, Vec3) {
    let mut world_low = Vec3::splat(f32::INFINITY);
    let mut world_high = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let point = Vec3::new(
            if corner & 1 == 0 { low.x } else { high.x },
            if corner & 2 == 0 { low.y } else { high.y },
            if corner & 4 == 0 { low.z } else { high.z },
        );
        let placed = transform.transform_point3(point);
        world_low = world_low.min(placed);
        world_high = world_high.max(placed);
    }
    (world_low, world_high)
}

/// A BVH over `boxes`, at most `leaf` of them a leaf (more only past
/// [`MOST_DEPTH`]): each node split at the median of its boxes' centers
/// along the axis they spread furthest. Its root is a leaf of nothing for
/// no boxes.
fn build_bvh(boxes: &[(Vec3, Vec3)], leaf: usize) -> Bvh {
    let mut order: Vec<u32> = (0..boxes.len() as u32).collect();
    let mut nodes = vec![Node {
        low: Vec3::ZERO,
        high: Vec3::ZERO,
        first: 0,
        count: 0,
    }];
    // Nodes still to fill: their index, their boxes in `order`, depth.
    let mut to_fill = vec![(0usize, 0usize, boxes.len(), 0usize)];
    while let Some((index, start, end, depth)) = to_fill.pop() {
        let these = &mut order[start..end];
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        let mut center_low = low;
        let mut center_high = high;
        for &each in these.iter() {
            let (a, b) = boxes[each as usize];
            low = low.min(a);
            high = high.max(b);
            let center = (a + b) * 0.5;
            center_low = center_low.min(center);
            center_high = center_high.max(center);
        }
        if these.is_empty() {
            low = Vec3::ZERO;
            high = Vec3::ZERO;
        }
        if these.len() <= leaf || depth >= MOST_DEPTH {
            nodes[index] = Node {
                low,
                high,
                first: start as u32,
                count: these.len() as u32,
            };
            continue;
        }
        let spread = center_high - center_low;
        let axis = if spread.x >= spread.y && spread.x >= spread.z {
            0
        } else if spread.y >= spread.z {
            1
        } else {
            2
        };
        let middle = these.len() / 2;
        these.select_nth_unstable_by(middle, |a, b| {
            let center = |each: &u32| {
                let (low, high) = boxes[*each as usize];
                low[axis] + high[axis]
            };
            center(a).total_cmp(&center(b))
        });
        let children = nodes.len();
        nodes[index] = Node {
            low,
            high,
            first: children as u32,
            count: 0,
        };
        nodes.extend([nodes[index]; 2]);
        to_fill.push((children, start, start + middle, depth + 1));
        to_fill.push((children + 1, start + middle, end, depth + 1));
    }
    Bvh { nodes, order }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boxes a leaf of `bvh` holds, by walking it.
    fn leaves(bvh: &Bvh, node: usize, found: &mut Vec<u32>, depth: usize) -> usize {
        let at = bvh.nodes[node];
        if at.count > 0 {
            let first = at.first as usize;
            found.extend(&bvh.order[first..first + at.count as usize]);
            return depth;
        }
        let first = at.first as usize;
        leaves(bvh, first, found, depth + 1).max(leaves(bvh, first + 1, found, depth + 1))
    }

    #[test]
    fn a_bvh_holds_every_box_once_within_its_parents_and_stays_shallow() {
        let boxes: Vec<(Vec3, Vec3)> = (0..1000)
            .map(|i| {
                let at = Vec3::new((i % 10) as f32, (i / 10 % 10) as f32, (i / 100) as f32);
                (at, at + Vec3::splat(0.5))
            })
            .collect();
        let bvh = build_bvh(&boxes, LEAF_TRIANGLES);
        let mut found = Vec::new();
        let depth = leaves(&bvh, 0, &mut found, 0);
        found.sort_unstable();
        assert_eq!(found, (0..1000).collect::<Vec<u32>>());
        assert!(
            depth <= 9,
            "1,000 boxes four a leaf: about 8 deep, not {depth}"
        );
        for node in &bvh.nodes {
            if node.count == 0 {
                for child in [node.first, node.first + 1] {
                    let child = bvh.nodes[child as usize];
                    assert!(child.low.cmpge(node.low).all() && child.high.cmple(node.high).all());
                }
            }
        }
        let empty = build_bvh(&[], LEAF_ITEMS);
        assert_eq!((empty.nodes.len(), empty.nodes[0].count), (1, 0));
    }

    #[test]
    fn a_mesh_bvh_points_its_leaves_at_their_triangles() {
        let bvh = build_mesh(&Shape::Block, 0, 0);
        assert_eq!(bvh.triangles, 12);
        assert_eq!(bvh.words.len(), bvh.nodes * 2 + 12 * 3);
        assert_eq!((bvh.low, bvh.high), (Vec3::splat(-0.5), Vec3::splat(0.5)));
        let mut triangles = 0;
        for node in 0..bvh.nodes {
            let count = bvh.words[node * 2 + 1][3].to_bits();
            let first = bvh.words[node * 2][3].to_bits() as usize;
            if count > 0 {
                assert!(first >= bvh.nodes * 2 && (first - bvh.nodes * 2) % 3 == 0);
                triangles += count;
            } else {
                assert!(first % 2 == 0 && first < bvh.nodes * 2);
            }
        }
        assert_eq!(triangles, 12);
    }
}
