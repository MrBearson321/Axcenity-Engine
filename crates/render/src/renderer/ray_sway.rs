//! Swaying plants in the ray traced scene (`docs/rendering/ray-tracing.md`):
//! a swaying item's mesh is bent on the GPU each frame by the same wind
//! `vs_main` and `vs_shadow` bend it by (`ray_sway.wgsl`, sway.wgsl's
//! `swayed_at`) and built into a BLAS of its own, so ray traced shadows,
//! ambient occlusion, GI and the fog's shafts move with the leaves.
//!
//! - Only while the wind blows: with none, every plant is at rest and uses
//!   its mesh's shared BLAS.
//! - The nearest swaying items first, up to [`BENT_TRIANGLES`] triangles a
//!   frame; the rest keep the rest pose.
//! - wgpu has no refit, so each bent BLAS is built afresh every frame. Its
//!   buffers and BLAS are kept by mesh and by how many items of that mesh
//!   were bent before it this frame, and reused while that holds.
//!
//! Posed (skinned) characters are copied the same way (`ray_skin.wgsl`):
//! each corner moved by its item's joints, as `pose_corner` in shader.wgsl
//! moves it, into a BLAS of the item's own, nearest first within
//! [`POSED_TRIANGLES`]. A character past that budget is left out of the ray
//! traced scene (its rest pose would cast a T-shaped shadow).

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use rustc_hash::FxHashMap;

use super::gpu::{self, shader_module};
use super::{GpuMesh, MeshKey};
use crate::draw_list::DrawItem;

/// The most triangles bent and built into BLASes in one frame, nearest
/// items first: about 1 ms on an RX 9070 XT (2.6 ms per million triangles,
/// `docs/research/hardware-ray-tracing.md`).
pub(super) const BENT_TRIANGLES: u64 = 400_000;

/// The most triangles of posed (skinned) items moved and built into
/// BLASes in one frame, nearest first: about 0.8 ms on an RX 9070 XT. A
/// budget of their own, so a windy forest never takes the players' shadows.
pub(super) const POSED_TRIANGLES: u64 = 300_000;

/// A bent copy unused for this many frames is freed.
const FORGET_AFTER: u64 = 120;

/// Which bent copy: the mesh, whether it's for a cut-out, and how many
/// items of that mesh were bent before it this frame.
pub(super) type BentKey = (MeshKey, bool, u32);

/// One item's bending. Must match `Bent` in ray_sway.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct BentRaw {
    model: [[f32; 4]; 4],
    to_object: [[f32; 4]; 4],
    sway: [f32; 4],
    clock: [f32; 4],
    counts: [u32; 4],
}

/// The `InstanceRaw::sway` of `item` (without its decal role) if it moves
/// in the wind of `clock` (`Frame::clock`), or `None` if it's still.
pub(super) fn bending(item: &DrawItem, clock: [f32; 4]) -> Option<[f32; 4]> {
    let sway = item.sway.filter(|sway| !sway.is_still())?;
    let level_wind = glam::Vec2::new(clock[1], clock[3]);
    if !level_wind.is_finite() || level_wind.length() < 1e-3 {
        return None;
    }
    Some([
        sway.base,
        1.0 / sway.height,
        sway.strength,
        sway.flutter.clamp(0.0, 0.95),
    ])
}

/// `item`'s bending for the GPU.
pub(super) fn bent_raw(transform: Mat4, sway: [f32; 4], clock: [f32; 4], gpu: &GpuMesh) -> BentRaw {
    BentRaw {
        model: transform.to_cols_array_2d(),
        to_object: transform.inverse().to_cols_array_2d(),
        sway,
        clock,
        counts: [
            gpu.vertex_count,
            (size_of::<crate::mesh::Vertex>() / 4) as u32,
            0,
            0,
        ],
    }
}

/// One posed item's posing. Must match `Pose` in ray_skin.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct PoseRaw {
    /// x: how many corners, y: how many f32s each corner of the mesh's
    /// vertices takes, z: how many joints the item's skin has.
    counts: [u32; 4],
}

struct Bent {
    /// Its bent positions, three f32s a corner.
    positions: wgpu::Buffer,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// A posed item's joints (each a `mat4x4`), `None` for a swaying one.
    joints: Option<wgpu::Buffer>,
    blas: wgpu::Blas,
    size: wgpu::BlasTriangleGeometrySizeDescriptor,
    /// The mesh's version it was made for.
    version: u64,
    last_used: u64,
    /// Bent this frame.
    used: bool,
}

/// The bent copies of swaying items and posed characters, and the passes
/// that bend and pose them.
pub(super) struct BentMeshes {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    /// Made the first time a character is posed.
    posing: Option<(wgpu::ComputePipeline, wgpu::BindGroupLayout)>,
    bent: FxHashMap<BentKey, Bent>,
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn compute_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    code: &'static str,
    entry: &str,
) -> wgpu::ComputePipeline {
    let shader = shader_module(device, label, code);
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            }),
        ),
        module: &shader,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}

impl BentMeshes {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bent meshes"),
            entries: &[
                uniform_entry(0),
                storage_entry(1, true),
                storage_entry(2, false),
            ],
        });
        let pipeline = compute_pipeline(
            device,
            "bent meshes",
            &layout,
            gpu::RAY_SWAY_SHADER,
            "cs_bend",
        );
        Self {
            pipeline,
            layout,
            posing: None,
            bent: FxHashMap::default(),
        }
    }

    /// Starts a frame: nothing is bent yet.
    pub(super) fn begin(&mut self) {
        for bent in self.bent.values_mut() {
            bent.used = false;
        }
    }

    /// Bends `gpu` (the mesh of `key`) as `raw` says this frame, making its
    /// copy if it has none (or one for an older version of the mesh).
    pub(super) fn bend(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        key: BentKey,
        gpu: &GpuMesh,
        size: wgpu::BlasTriangleGeometrySizeDescriptor,
        raw: &BentRaw,
        frame: u64,
    ) {
        let fresh = |bent: Option<&Bent>| bent.is_none_or(|bent| bent.version != gpu.version);
        if fresh(self.bent.get(&key)) {
            let positions = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bent positions"),
                size: (u64::from(gpu.vertex_count) * 12).max(16),
                usage: wgpu::BufferUsages::STORAGE | super::ray_tracing::blas_input(device),
                mapped_at_creation: false,
            });
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bent item"),
                size: size_of::<BentRaw>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bent meshes"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: gpu.vertices.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: positions.as_entire_binding(),
                    },
                ],
            });
            let size = wgpu::BlasTriangleGeometrySizeDescriptor {
                vertex_format: wgpu::VertexFormat::Float32x3,
                ..size
            };
            let blas = device.create_blas(
                &wgpu::CreateBlasDescriptor {
                    label: Some("bent BLAS"),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_BUILD,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                wgpu::BlasGeometrySizeDescriptors::Triangles {
                    descriptors: vec![size.clone()],
                },
            );
            self.bent.insert(
                key,
                Bent {
                    positions,
                    uniform,
                    group,
                    joints: None,
                    blas,
                    size,
                    version: gpu.version,
                    last_used: frame,
                    used: false,
                },
            );
        }
        let bent = self.bent.get_mut(&key).expect("made above");
        queue.write_buffer(&bent.uniform, 0, bytemuck::bytes_of(raw));
        bent.last_used = frame;
        bent.used = true;
    }

    /// Poses `gpu` (the mesh of `key`, animated by `skin`) with `joints`
    /// this frame, making its copy if it has none (or one for an older
    /// version of the mesh, or with too few joints).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pose(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        key: BentKey,
        gpu: &GpuMesh,
        skin: &wgpu::Buffer,
        size: wgpu::BlasTriangleGeometrySizeDescriptor,
        joints: &[[[f32; 4]; 4]],
        frame: u64,
    ) {
        let (_, layout) = self.posing.get_or_insert_with(|| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("posed meshes"),
                entries: &[
                    uniform_entry(0),
                    storage_entry(1, true),
                    storage_entry(2, true),
                    storage_entry(3, true),
                    storage_entry(4, false),
                ],
            });
            let pipeline = compute_pipeline(
                device,
                "posed meshes",
                &layout,
                gpu::RAY_SKIN_SHADER,
                "cs_pose",
            );
            (pipeline, layout)
        });
        let joint_bytes = (size_of_val(joints) as u64).max(64);
        let fresh = |bent: Option<&Bent>| {
            bent.is_none_or(|bent| {
                bent.version != gpu.version
                    || bent
                        .joints
                        .as_ref()
                        .is_none_or(|buffer| buffer.size() < joint_bytes)
            })
        };
        if fresh(self.bent.get(&key)) {
            let positions = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed positions"),
                size: (u64::from(gpu.vertex_count) * 12).max(16),
                usage: wgpu::BufferUsages::STORAGE | super::ray_tracing::blas_input(device),
                mapped_at_creation: false,
            });
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed item"),
                size: size_of::<PoseRaw>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let joint_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed joints"),
                size: joint_bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("posed meshes"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: gpu.vertices.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: skin.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: joint_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: positions.as_entire_binding(),
                    },
                ],
            });
            let size = wgpu::BlasTriangleGeometrySizeDescriptor {
                vertex_format: wgpu::VertexFormat::Float32x3,
                ..size
            };
            let blas = device.create_blas(
                &wgpu::CreateBlasDescriptor {
                    label: Some("posed BLAS"),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_BUILD,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                wgpu::BlasGeometrySizeDescriptors::Triangles {
                    descriptors: vec![size.clone()],
                },
            );
            self.bent.insert(
                key,
                Bent {
                    positions,
                    uniform,
                    group,
                    joints: Some(joint_buffer),
                    blas,
                    size,
                    version: gpu.version,
                    last_used: frame,
                    used: false,
                },
            );
        }
        let bent = self.bent.get_mut(&key).expect("made above");
        let raw = PoseRaw {
            counts: [
                gpu.vertex_count,
                (size_of::<crate::mesh::Vertex>() / 4) as u32,
                joints.len() as u32,
                0,
            ],
        };
        queue.write_buffer(&bent.uniform, 0, bytemuck::bytes_of(&raw));
        if let Some(buffer) = &bent.joints
            && !joints.is_empty()
        {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(joints));
        }
        bent.last_used = frame;
        bent.used = true;
    }

    /// Records this frame's bending and posing, before the BLASes are built.
    pub(super) fn record(&self, encoder: &mut wgpu::CommandEncoder) {
        let used: Vec<&Bent> = self.bent.values().filter(|bent| bent.used).collect();
        if used.is_empty() {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("bent meshes"),
            timestamp_writes: None,
        });
        let bent_pipeline = Some(&self.pipeline);
        let posed_pipeline = self.posing.as_ref().map(|(pipeline, _)| pipeline);
        for (pipeline, posed) in [(bent_pipeline, false), (posed_pipeline, true)] {
            let Some(pipeline) = pipeline else {
                continue;
            };
            let mut set = false;
            for bent in used.iter().filter(|bent| bent.joints.is_some() == posed) {
                if !set {
                    pass.set_pipeline(pipeline);
                    set = true;
                }
                let corners = bent.positions.size() / 12;
                let groups = corners.div_ceil(64) as u32;
                pass.set_bind_group(0, &bent.group, &[]);
                pass.dispatch_workgroups(groups.min(65_535), groups.div_ceil(65_535), 1);
            }
        }
    }

    /// The BLAS of `key`, bent this frame.
    pub(super) fn blas(&self, key: &BentKey) -> &wgpu::Blas {
        &self.bent[key].blas
    }

    /// What builds `key`'s BLAS from its bent positions and `gpu`'s
    /// triangles.
    pub(super) fn build_entry<'a>(
        &'a self,
        key: &BentKey,
        gpu: &'a GpuMesh,
    ) -> wgpu::BlasBuildEntry<'a> {
        let bent = &self.bent[key];
        wgpu::BlasBuildEntry {
            blas: &bent.blas,
            geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
                size: &bent.size,
                vertex_buffer: &bent.positions,
                first_vertex: 0,
                vertex_stride: 12,
                index_buffer: Some(&gpu.indices),
                first_index: Some(0),
                transform_buffer: None,
                transform_buffer_offset: None,
            }]),
        }
    }

    /// Frees copies nothing has bent for a while.
    pub(super) fn forget_unused(&mut self, frame: u64) {
        self.bent
            .retain(|_, bent| frame.saturating_sub(bent.last_used) < FORGET_AFTER);
    }

    /// How many swaying items were bent this frame.
    pub(super) fn bent_count(&self) -> u32 {
        self.count_used(false)
    }

    /// How many posed characters were moved into BLASes this frame.
    pub(super) fn posed_count(&self) -> u32 {
        self.count_used(true)
    }

    fn count_used(&self, posed: bool) -> u32 {
        self.bent
            .values()
            .filter(|bent| bent.used && bent.joints.is_some() == posed)
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::Sway;

    fn plant() -> DrawItem {
        DrawItem {
            shape: crate::Shape::Block,
            transform: Mat4::IDENTITY,
            material: Default::default(),
            uv_tile: 0.0,
            casts_shadow: true,
            id: 0,
            cull_distance: 0.0,
            skin: None,
            sway: Some(Sway {
                base: 2.0,
                height: 4.0,
                strength: 0.5,
                flutter: 1.5,
            }),
            decal_role: Default::default(),
        }
    }

    #[test]
    fn only_a_swaying_item_in_a_level_wind_is_bent() {
        let windy = [3.0, 10.0, 0.0, 0.0];
        assert_eq!(
            bending(&plant(), windy),
            Some([2.0, 0.25, 0.5, 0.95]),
            "the flutter is kept under 1, as in `InstanceRaw::sway`"
        );
        assert_eq!(bending(&plant(), [3.0, 0.0, 0.0, 0.0]), None, "no wind");
        assert_eq!(
            bending(&plant(), [3.0, 0.0, 50.0, 0.0]),
            None,
            "a wind straight up bends nothing"
        );
        assert_eq!(bending(&plant(), [3.0, f32::NAN, 0.0, 0.0]), None);
        let still = DrawItem {
            sway: None,
            ..plant()
        };
        assert_eq!(bending(&still, windy), None);
    }
}
