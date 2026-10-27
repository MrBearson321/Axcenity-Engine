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

impl JointRaw {
    /// Each joint of `skin`.
    pub(super) fn of(skin: &Skin) -> impl Iterator<Item = Self> + '_ {
        skin.joints()
            .iter()
            .zip(skin.normals())
            .map(|(joint, normal)| Self {
                position: joint.to_cols_array_2d(),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|column| {
                    let [x, y, z] = column.to_array();
                    [x, y, z, 0.0]
                }),
            })
    }
}

/// One corner's joints and weights. Must match `SkinCorner` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct SkinCorner {
    joints: [u32; 4],
    weights: [f32; 4],
}

/// An animated mesh's joints and weights on the GPU.
pub(super) struct GpuSkin {
    pub(super) bind_group: wgpu::BindGroup,
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: storage(),
                count: None,
            }],
        });
        let nothing = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("no skin"),
            contents: bytemuck::bytes_of(&SkinCorner::default()),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let none = skin_group(device, &layout, &nothing);
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

    /// Uploads `source`'s joints and weights, or `None` if it isn't
    /// animated, or (with a warning) if they don't fit its corners: it's
    /// then drawn as it is.
    pub(super) fn upload(
        &self,
        device: &wgpu::Device,
        bytes: &GpuBytes,
        source: &dyn MeshSource,
    ) -> Option<GpuSkin> {
        let (joints, weights) = (source.skin_joints(), source.skin_weights());
        if joints.is_empty() && weights.is_empty() {
            return None;
        }
        let positions = source.positions();
        if joints.len() != positions.len() || weights.len() != positions.len() {
            log::warn!(
                "mesh #{} has joints or weights that don't fit its corners, so it isn't animated",
                source.key()
            );
            return None;
        }
        let corners: Vec<SkinCorner> = joints
            .iter()
            .zip(weights)
            .map(|(joints, weights)| SkinCorner {
                joints: joints.map(u32::from),
                weights: *weights,
            })
            .collect();
        // Each joint's corners' box, and the box of those that never move.
        let mut joint_bounds: Vec<Option<Bounds>> = Vec::new();
        let mut still: Option<Bounds> = None;
        let grow = |bounds: &mut Option<Bounds>, point: Vec3| {
            *bounds = Some(match *bounds {
                Some(Bounds { min, max }) => Bounds {
                    min: min.min(point),
                    max: max.max(point),
                },
                None => Bounds {
                    min: point,
                    max: point,
                },
            });
        };
        for (corner, position) in corners.iter().zip(positions) {
            let point = Vec3::from(*position);
            let mut weighted = false;
            for (&joint, &weight) in corner.joints.iter().zip(&corner.weights) {
                if weight <= 0.0 {
                    continue;
                }
                let joint = joint as usize;
                if joint >= joint_bounds.len() {
                    joint_bounds.resize(joint + 1, None);
                }
                grow(&mut joint_bounds[joint], point);
                weighted = true;
            }
            if !weighted {
                grow(&mut still, point);
            }
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skin"),
            contents: bytemuck::cast_slice(&corners),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Some(GpuSkin {
            bind_group: skin_group(device, &self.layout, &buffer),
            joints: joint_bounds,
            still,
            _bytes: bytes.count(buffer.size()),
        })
    }
}

fn skin_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("skin"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
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
