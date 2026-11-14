//! Ray traced reflections (RL4, `docs/rendering/ray-tracing.md`): a
//! compute pass (`ray_reflections.wgsl`) traces one ray along the mirrored
//! view from each glossy solid pixel of the prepass (its roughness, below
//! the setting's cutoff, kept in the prepass normal's w) through the ray
//! traced scene (`ray_tracing.rs`, which keeps a color and glow for each of
//! its items while this is on), at the scene's size or half of it. What
//! the rays bring back is bound in the frame's place 18, where the scene's
//! `shade_surface` (lighting.wgsl's `traced_reflected`) puts it in the
//! reflection probes' place, fading into them toward the cutoff. Misses,
//! rough surfaces, see-through items and water keep the probes.
//!
//! Exists only while the setting is on and the GPU can ray trace.

use std::num::NonZeroU64;

use glam::Mat4;

use crate::graphics::{EffectiveGraphics, RayTracedReflections};

use super::types::OcclusionTargets;

/// Must match `RayReflections` in ray_reflections.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct RayReflectionUniform {
    clip_to_world: [[f32; 4]; 4],
    view_to_world: [[f32; 4]; 4],
    /// The traced size, how many scene pixels a traced one stands for each
    /// way, unused.
    size: [u32; 4],
    /// The roughness cutoff, how far the rays reach, how far a hit's shadow
    /// rays start off it, unused.
    settings: [f32; 4],
}

/// Bytes a traced pixel takes: rgb and its weight as half floats.
const PIXEL_BYTES: u64 = 8;

/// The pass's pipeline and buffers.
pub(super) struct RayReflections {
    /// The frame's bindings the pass reads, as a compute pass sees them,
    /// and their bind group (made again with the frame's).
    pub(super) frame_layout: wgpu::BindGroupLayout,
    pub(super) frame_group: Option<wgpu::BindGroup>,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    /// What the rays found, bound in the frame's place 18.
    pub(super) buffer: wgpu::Buffer,
    /// The traced size it holds, and how many scene pixels a traced one
    /// stands for each way.
    size: (u32, u32),
    downscale: u32,
}

impl RayReflections {
    /// `frame_entries` are the frame's bindings; those the pass reads (as
    /// ray traced GI's, `ray_gi::FRAME_BINDINGS`) are made visible to it.
    /// Holds what's traced for a scene of `scene_size` at `downscale`.
    pub(super) fn new(
        device: &wgpu::Device,
        frame_entries: &[wgpu::BindGroupLayoutEntry],
        (scene_size, downscale): ((u32, u32), u32),
    ) -> Self {
        let compute_entries: Vec<_> = frame_entries
            .iter()
            .filter(|entry| super::ray_gi::FRAME_BINDINGS.contains(&entry.binding))
            .map(|entry| wgpu::BindGroupLayoutEntry {
                visibility: entry.visibility | wgpu::ShaderStages::COMPUTE,
                ..*entry
            })
            .collect();
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame (ray traced reflections)"),
            entries: &compute_entries,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let texture = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let mut entries = vec![
            entry(
                0,
                wgpu::BindingType::AccelerationStructure {
                    vertex_return: false,
                },
            ),
            entry(
                1,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<RayReflectionUniform>() as u64),
                },
            ),
            entry(2, storage(true)),
            entry(3, storage(false)),
            entry(4, texture(wgpu::TextureSampleType::Depth)),
            entry(
                5,
                texture(wgpu::TextureSampleType::Float { filterable: false }),
            ),
        ];
        // Cut-outs' holes (LE7).
        entries.extend(super::ray_cut_outs::layout_entries(
            6,
            wgpu::ShaderStages::COMPUTE,
        ));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray traced reflections"),
            entries: &entries,
        });
        let shader = super::gpu::shader_module(
            device,
            "ray traced reflections",
            super::gpu::RAY_REFLECTIONS_SHADER,
        );
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ray traced reflections"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("ray traced reflections"),
                    bind_group_layouts: &[Some(&frame_layout), Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("cs_trace_reflections"),
            compilation_options: Default::default(),
            cache: None,
        });
        let size = traced_size(scene_size, downscale);
        Self {
            frame_layout,
            frame_group: None,
            layout,
            pipeline,
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ray traced reflections"),
                size: size_of::<RayReflectionUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ray traced reflections"),
                size: PIXEL_BYTES * u64::from(size.0) * u64::from(size.1),
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }),
            size,
            downscale,
        }
    }

    /// Whether it holds what's traced for a scene of `scene_size` at
    /// `downscale`.
    pub(super) fn fits(&self, scene_size: (u32, u32), downscale: u32) -> bool {
        self.downscale == downscale && self.size == traced_size(scene_size, downscale)
    }

    /// `Frame::ray_reflections` for `graphics`: the cutoff, the downscale
    /// and the traced width.
    pub(super) fn frame_values(&self, graphics: &EffectiveGraphics) -> [f32; 4] {
        [
            graphics.ray_traced_reflections.roughness_cutoff(),
            self.downscale as f32,
            self.size.0 as f32,
            0.0,
        ]
    }

    /// What the pass reads this frame: `graphics`' rays, with `clip_to_world`
    /// (the scene as drawn this frame, inverted) and `view` (the camera's).
    pub(super) fn uniform(
        &self,
        graphics: &EffectiveGraphics,
        clip_to_world: Mat4,
        view: Mat4,
    ) -> RayReflectionUniform {
        RayReflectionUniform {
            clip_to_world: clip_to_world.to_cols_array_2d(),
            view_to_world: view.inverse().to_cols_array_2d(),
            size: [self.size.0, self.size.1, self.downscale, 0],
            settings: [
                graphics.ray_traced_reflections.roughness_cutoff(),
                graphics.ray_tracing_distance,
                0.05,
                0.0,
            ],
        }
    }

    /// Records tracing the reflections of the prepass in `targets` through
    /// `scene` (whose items' colors are in `surfaces`) into `encoder`.
    /// Does nothing until the frame's bindings are made (`frame_group`).
    pub(super) fn trace(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (scene, surfaces, targets): (
            &super::ray_tracing::RayScene,
            &wgpu::Buffer,
            &OcclusionTargets,
        ),
        uniform: &RayReflectionUniform,
    ) -> bool {
        let Some(frame_group) = &self.frame_group else {
            return false;
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: scene.tlas().as_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: surfaces.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: self.buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&targets.depth_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&targets.normal_view),
            },
        ];
        entries.extend(scene.cut_outs.entries(6));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray traced reflections"),
            layout: &self.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("ray traced reflections"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, frame_group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(self.size.0.div_ceil(8), self.size.1.div_ceil(8), 1);
        true
    }

    /// Bytes of its buffers.
    pub(super) fn bytes(&self) -> u64 {
        self.buffer.size() + self.uniform.size()
    }
}

/// The size traced for a scene of `scene_size` at `downscale`: at least 1
/// each way.
fn traced_size((width, height): (u32, u32), downscale: u32) -> (u32, u32) {
    let downscale = downscale.max(1);
    (
        width.div_ceil(downscale).max(1),
        height.div_ceil(downscale).max(1),
    )
}

/// Whether ray traced reflections are drawn with `graphics`.
pub(super) fn traced(graphics: &EffectiveGraphics) -> bool {
    graphics.ray_queries && graphics.ray_traced_reflections != RayTracedReflections::Off
}

/// The one blank entry bound in the frame's place 18 while they're off.
pub(super) fn stand_in(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("no ray traced reflections"),
        size: PIXEL_BYTES,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_size_rounds_up_so_every_scene_pixel_has_a_traced_one() {
        assert_eq!(traced_size((1920, 1080), 1), (1920, 1080));
        assert_eq!(traced_size((1921, 1081), 2), (961, 541));
        assert_eq!(traced_size((0, 0), 2), (1, 1));
    }
}
