//! Ray traced ambient occlusion (RT3, `docs/rendering/ray-tracing.md`): a
//! fragment pass (`ray_ao.wgsl`) traces a few short rays over the
//! hemisphere above each solid pixel of the prepass, at half the scene's
//! size, through the ray traced scene (`ray_tracing.rs`). It draws into
//! ambient occlusion's own target, over screen-space ambient occlusion when
//! that's on (or over nothing hidden), fading into it toward the ray
//! tracing distance; ambient occlusion's blur then smooths the result as it
//! smooths screen-space's.
//!
//! Exists only while the setting is on and the GPU can ray trace.

use glam::Mat4;

use crate::draw_list::DrawList;
use crate::graphics::{EffectiveGraphics, RayTracedAmbientOcclusion};

use super::OCCLUSION_FORMAT;
use super::types::{OcclusionPasses, OcclusionTargets};

/// Must match `RayAo` in ray_ao.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct RayAoUniform {
    view_to_world: [[f32; 4]; 4],
    /// xyz: the camera, w: how far the rays reach from it.
    camera: [f32; 4],
    /// Rays per pixel, a number changing each frame under TAA, unused,
    /// unused.
    settings: [f32; 4],
}

/// The pass's pipeline and uniform.
pub(super) struct RayAmbientOcclusion {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
}

impl RayAmbientOcclusion {
    /// Its pipeline reads ambient occlusion's bindings (`passes.layout`:
    /// the prepass's depth and normals and the occlusion uniform) and its
    /// own: the ray traced scene and [`RayAoUniform`].
    pub(super) fn new(device: &wgpu::Device, passes: &OcclusionPasses) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray traced ambient occlusion"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ray traced ambient occlusion"),
            bind_group_layouts: &[Some(&passes.layout), Some(&layout)],
            immediate_size: 0,
        });
        let shader = super::gpu::shader_module(
            device,
            "ray traced ambient occlusion",
            include_str!("../ray_ao.wgsl"),
        );
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ray traced ambient occlusion"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ray_occlusion"),
                compilation_options: Default::default(),
                // Over what's beneath by the rays' weight (alpha).
                targets: &[Some(wgpu::ColorTargetState {
                    format: OCCLUSION_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray traced ambient occlusion"),
            size: size_of::<RayAoUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            layout,
            pipeline,
            uniform,
        }
    }

    /// Records the pass into `targets`' raw occlusion: over what
    /// screen-space ambient occlusion drew there if `over_screen_space`,
    /// otherwise over nothing hidden.
    pub(super) fn draw(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (tlas, targets): (&wgpu::Tlas, &OcclusionTargets),
        uniform: &RayAoUniform,
        over_screen_space: bool,
    ) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray traced ambient occlusion"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: tlas.as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let load = if over_screen_space {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::WHITE)
        };
        super::post::fullscreen_pass(
            encoder,
            "ray traced ambient occlusion",
            &targets.raw_view,
            load,
            &self.pipeline,
            &[&targets.occlusion_group, &group],
        );
    }
}

/// Whether ray traced ambient occlusion is drawn with `graphics`.
pub(super) fn traced(graphics: &EffectiveGraphics) -> bool {
    graphics.ray_queries && graphics.ray_traced_ambient_occlusion != RayTracedAmbientOcclusion::Off
}

/// What the pass reads this frame: `graphics`' rays for `list`'s camera
/// seen through `view`. `turn` changes each frame under TAA (0 without).
pub(super) fn ray_ao_uniform(
    list: &DrawList,
    graphics: &EffectiveGraphics,
    view: Mat4,
    turn: f32,
) -> RayAoUniform {
    RayAoUniform {
        view_to_world: view.inverse().to_cols_array_2d(),
        camera: list
            .camera
            .position
            .extend(graphics.ray_tracing_distance)
            .to_array(),
        settings: [
            graphics.ray_traced_ambient_occlusion.rays() as f32,
            turn,
            0.0,
            0.0,
        ],
    }
}
