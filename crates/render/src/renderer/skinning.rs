//! Skinning animated meshes on the GPU.
//!
//! An animated mesh is uploaded once, as it is in its file, with each
//! corner's joints and weights ([`MeshSource::skin_joints`]) in a storage
//! buffer of its own (group 2 in shader.wgsl). Each frame, the joints of
//! every [`Skin`] drawn go into one shared buffer (binding 9 of group 0), and
//! each item's instance says where its skin's joints start and how many
//! there are. The vertex shader blends a corner's joints as `assets`'
//! `Posed` does on the CPU. So items posed differently share one copy of
//! the mesh, and batch together.
//!
//! Blend shapes (morph targets, facial animation F2) go the same way: a
//! mesh's shapes are uploaded once, as each corner's range of moves (only
//! the corners a shape moves; bindings 1 and 2 of group 2), and an item's
//! shape weights follow its joints in the shared buffer, sixteen to an
//! entry. The instance's `skin.y` holds the joint count in its low 16 bits
//! and the shape count in its high 16. The vertex shader morphs a corner
//! before skinning it, as `Posed` does.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::draw_list::{MeshSource, Skin};

use super::{Bounds, Counted, GpuBytes};

/// One joint as the GPU reads it. Must match `Joint` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct JointRaw {
    pub(super) position: [[f32; 4]; 4],
    /// A `mat3x3` in a storage buffer: each column is 16 bytes.
    pub(super) normal: [[f32; 4]; 3],
}

/// Shape weights per entry of the joints buffer: those in its `position`.
const WEIGHTS_PER_ENTRY: usize = 16;

impl JointRaw {
    /// Each joint of `skin`, then its shape weights, sixteen to an entry
    /// (column by column of `position`, as `morph_weight` in shader.wgsl
    /// reads them).
    pub(super) fn of(skin: &Skin) -> impl Iterator<Item = Self> + '_ {
        let joints = skin
            .joints()
            .iter()
            .zip(skin.normals())
            .map(|(joint, normal)| Self {
                position: joint.to_cols_array_2d(),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|column| {
                    let [x, y, z] = column.to_array();
                    [x, y, z, 0.0]
                }),
            });
        let weights = skin
            .morph_weights()
            .chunks(WEIGHTS_PER_ENTRY)
            .take(usize::from(u16::MAX))
            .map(|chunk| {
                let mut position = [[0.0; 4]; 4];
                for (at, weight) in chunk.iter().enumerate() {
                    position[at / 4][at % 4] = if weight.is_finite() { *weight } else { 0.0 };
                }
                Self {
                    position,
                    normal: [[0.0; 4]; 3],
                }
            });
        joints.chain(weights)
    }
}

/// `skin`'s counts as an instance's `skin.y`: its joints in the low 16
/// bits and its shape weights in the high 16. 0 for a skin that neither
/// skins nor morphs.
pub(super) fn packed_counts(skin: &Skin) -> u32 {
    let joints = skin.joints().len().min(0xFFFF) as u32;
    let weights = skin.morph_weights().len().min(0xFFFF) as u32;
    joints | (weights << 16)
}

/// How many entries of the joints buffer an instance's `skin.y` (see
/// [`packed_counts`]) takes.
pub(super) fn entry_count(packed: u32) -> usize {
    let joints = (packed & 0xFFFF) as usize;
    let weights = (packed >> 16) as usize;
    joints + weights.div_ceil(WEIGHTS_PER_ENTRY)
}

/// Where one corner's shape moves are in a mesh's `MorphDelta`s. Must
/// match the range read by `morph_corner` in shader.wgsl (a `vec2<u32>`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct MorphRange {
    first: u32,
    count: u32,
}

/// One corner's move by one shape at weight 1. Must match `MorphDelta` in
/// shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct MorphDelta {
    position: [f32; 3],
    target: u32,
    normal: [f32; 3],
    _pad: u32,
}

/// One corner's joints and weights. Must match `SkinCorner` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct SkinCorner {
    joints: [u32; 4],
    weights: [f32; 4],
}

/// An animated mesh's joints and weights, and its blend shapes, on the GPU.
pub(super) struct GpuSkin {
    pub(super) bind_group: wgpu::BindGroup,
    /// The corners' joints and weights (`SkinCorner`s), which ray tracing
    /// also poses its copy of the mesh by (`ray_skin.wgsl`). One unweighted
    /// corner for a mesh with blend shapes and no joints.
    pub(super) buffer: wgpu::Buffer,
    /// Whether the mesh has joints (and not only blend shapes).
    pub(super) jointed: bool,
    /// The box of the corners weighted to each joint, by joint, `None` for
    /// a joint no corner is weighted to; and the box of the corners
    /// weighted to nothing, which never move.
    joints: Vec<Option<Bounds>>,
    still: Option<Bounds>,
    _bytes: Counted,
}

impl GpuSkin {
    /// The box `skin` moves the corners into. A corner ends up between the
    /// places its joints move it to (its weights add up to 1), so it's
    /// inside the box around each joint's corners as that joint moves them.
    pub(super) fn bounds(&self, skin: &Skin) -> Bounds {
        let moved = self
            .joints
            .iter()
            .enumerate()
            .filter_map(|(joint, bounds)| {
                let bounds = (*bounds)?;
                // A joint past the skin's is skipped, as in the shader.
                Some((bounds, *skin.joints().get(joint)?))
            });
        let corners = moved
            .flat_map(|(bounds, joint)| {
                corners(bounds).map(|corner| joint.transform_point3(corner))
            })
            .chain(self.still.into_iter().flat_map(corners));
        Bounds::around(corners)
    }
}

fn corners(Bounds { min, max }: Bounds) -> [Vec3; 8] {
    [0, 1, 2, 3, 4, 5, 6, 7].map(|bits| {
        Vec3::new(
            if bits & 1 == 0 { min.x } else { max.x },
            if bits & 2 == 0 { min.y } else { max.y },
            if bits & 4 == 0 { min.z } else { max.z },
        )
    })
}

/// The bindings skinning adds to the scene's pipelines.
#[derive(Clone)]
pub(super) struct Skinning {
    /// Group 2: a mesh's corners' joints and weights.
    pub(super) layout: wgpu::BindGroupLayout,
    /// What a mesh that isn't animated binds: one corner weighted to
    /// nothing, which every corner reads.
    pub(super) none: wgpu::BindGroup,
    /// Group 1 of the shadow passes, which bind no material.
    pub(super) empty_layout: wgpu::BindGroupLayout,
    pub(super) empty: wgpu::BindGroup,
    /// Every drawn skin's joints this frame.
    pub(super) joints: wgpu::Buffer,
    /// What a mesh without blend shapes binds as its shapes (1 and 2).
    no_ranges: wgpu::Buffer,
    no_moves: wgpu::Buffer,
}

/// Binding 9 of group 0 (the frame's and the shadow passes'): the frame's
/// joints, read by the vertex shader.
pub(super) const JOINTS_BINDING: u32 = 9;

pub(super) fn joints_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: JOINTS_BINDING,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: storage(),
        count: None,
    }
}

fn storage() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

impl Skinning {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        // 0: corners' joints and weights; 1: corners' ranges of shape
        // moves; 2: the moves.
        let entries = [0, 1, 2].map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: storage(),
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin"),
            entries: &entries,
        });
        let nothing = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("no skin"),
            contents: bytemuck::bytes_of(&SkinCorner::default()),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let no_ranges = storage_buffer(device, "no morph ranges", &[MorphRange::default()]);
        let no_moves = storage_buffer(device, "no morphs", &[MorphDelta::default()]);
        let none = skin_group(device, &layout, [&nothing, &no_ranges, &no_moves]);
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nothing"),
            entries: &[],
        });
        let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nothing"),
            layout: &empty_layout,
            entries: &[],
        });
        Self {
            layout,
            none,
            empty_layout,
            empty,
            joints: joint_buffer(device, 64),
            no_ranges,
            no_moves,
        }
    }

    /// Writes this frame's `joints`, first making the buffer bigger if
    /// they don't fit. True when it was remade, so the bindings holding it
    /// must be too.
    pub(super) fn write_joints(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        joints: &[JointRaw],
    ) -> bool {
        let needed = size_of_val(joints) as u64;
        let grew = needed > self.joints.size();
        if grew {
            let capacity = joints.len().next_power_of_two();
            self.joints = joint_buffer(device, capacity);
        }
        if !joints.is_empty() {
            queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(joints));
        }
        grew
    }

    /// Uploads `source`'s joints and weights and its blend shapes, or
    /// `None` if it has neither, or (with a warning) if its joints and
    /// weights don't fit its corners: it's then drawn as it is, though
    /// still morphed.
    pub(super) fn upload(
        &self,
        device: &wgpu::Device,
        bytes: &GpuBytes,
        source: &dyn MeshSource,
    ) -> Option<GpuSkin> {
        let positions = source.positions();
        let (joints, weights) = (source.skin_joints(), source.skin_weights());
        let mut jointed = !(joints.is_empty() && weights.is_empty());
        if jointed && (joints.len() != positions.len() || weights.len() != positions.len()) {
            log::warn!(
                "mesh #{} has joints or weights that don't fit its corners, so it isn't animated",
                source.key()
            );
            jointed = false;
        }
        let morphs = morph_moves(source);
        if !jointed && morphs.is_none() {
            return None;
        }
        let corners: Vec<SkinCorner> = match jointed {
            true => joints
                .iter()
                .zip(weights)
                .map(|(joints, weights)| SkinCorner {
                    joints: joints.map(u32::from),
                    weights: *weights,
                })
                .collect(),
            false => vec![SkinCorner::default()],
        };
        // How far each corner's shapes can move it, all at weight 1.
        let reach: Vec<f32> = match &morphs {
            Some((ranges, moves)) => ranges
                .iter()
                .map(|range| {
                    let first = range.first as usize;
                    moves[first..first + range.count as usize]
                        .iter()
                        .map(|delta| Vec3::from(delta.position).length())
                        .sum()
                })
                .collect(),
            None => Vec::new(),
        };
        // Each joint's corners' box, and the box of those that never move,
        // each grown by how far their shapes can move them.
        let mut joint_bounds: Vec<Option<Bounds>> = Vec::new();
        let mut still: Option<Bounds> = None;
        let grow = |bounds: &mut Option<Bounds>, point: Vec3, reach: f32| {
            let (low, high) = (point - Vec3::splat(reach), point + Vec3::splat(reach));
            *bounds = Some(match *bounds {
                Some(Bounds { min, max }) => Bounds {
                    min: min.min(low),
                    max: max.max(high),
                },
                None => Bounds {
                    min: low,
                    max: high,
                },
            });
        };
        for (index, position) in positions.iter().enumerate() {
            let point = Vec3::from(*position);
            let reach = reach.get(index).copied().unwrap_or(0.0);
            let mut weighted = false;
            if jointed {
                let corner = corners[index];
                for (&joint, &weight) in corner.joints.iter().zip(&corner.weights) {
                    if weight <= 0.0 {
                        continue;
                    }
                    let joint = joint as usize;
                    if joint >= joint_bounds.len() {
                        joint_bounds.resize(joint + 1, None);
                    }
                    grow(&mut joint_bounds[joint], point, reach);
                    weighted = true;
                }
            }
            if !weighted {
                grow(&mut still, point, reach);
            }
        }
        let buffer = storage_buffer(device, "skin", &corners);
        let (ranges, moves) = match &morphs {
            Some((ranges, moves)) => (
                storage_buffer(device, "morph ranges", ranges),
                storage_buffer(device, "morphs", moves),
            ),
            None => (self.no_ranges.clone(), self.no_moves.clone()),
        };
        let size = buffer.size() + morphs.as_ref().map_or(0, |_| ranges.size() + moves.size());
        Some(GpuSkin {
            bind_group: skin_group(device, &self.layout, [&buffer, &ranges, &moves]),
            buffer,
            jointed,
            joints: joint_bounds,
            still,
            _bytes: bytes.count(size),
        })
    }
}

/// `source`'s blend shapes as each corner's range of moves and the moves,
/// or `None` if it has none (or they don't fit its corners).
fn morph_moves(source: &dyn MeshSource) -> Option<(Vec<MorphRange>, Vec<MorphDelta>)> {
    let corners = source.positions().len();
    let mut by_corner: Vec<Vec<MorphDelta>> = vec![Vec::new(); corners];
    let mut any = false;
    for index in 0..source.morph_count() {
        let Some(morph) = source.morph(index) else {
            continue;
        };
        let moves = morph.corners.iter().zip(morph.positions).zip(
            morph
                .normals
                .iter()
                .copied()
                .chain(std::iter::repeat([0.0; 3])),
        );
        for ((&corner, position), normal) in moves {
            let Some(list) = by_corner.get_mut(corner as usize) else {
                log::warn!(
                    "mesh #{} has a blend shape moving a corner it hasn't got, so it isn't morphed",
                    source.key()
                );
                return None;
            };
            list.push(MorphDelta {
                position: *position,
                target: u32::from(morph.target),
                normal,
                _pad: 0,
            });
            any = true;
        }
    }
    if !any {
        return None;
    }
    let mut ranges = Vec::with_capacity(corners);
    let mut moves = Vec::new();
    for list in by_corner {
        ranges.push(MorphRange {
            first: moves.len() as u32,
            count: list.len() as u32,
        });
        moves.extend(list);
    }
    Some((ranges, moves))
}

fn storage_buffer<T: Pod>(device: &wgpu::Device, label: &str, contents: &[T]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(contents),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn skin_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: [&wgpu::Buffer; 3],
) -> wgpu::BindGroup {
    let entries = [0, 1, 2].map(|binding| wgpu::BindGroupEntry {
        binding,
        resource: buffers[binding as usize].as_entire_binding(),
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("skin"),
        layout,
        entries: &entries,
    })
}

fn joint_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("joints"),
        size: (capacity.max(1) * size_of::<JointRaw>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
