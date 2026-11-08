//! The editor's measuring grid (`DrawList::grid`, `grid.wgsl`), drawn in the
//! scene pass after the sky and before see-through items. Its own uniform
//! holds all it needs, so it costs one draw and no frame bindings.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::{DEPTH_FORMAT, HDR_FORMAT};
use crate::draw_list::GridPlane;

/// Must match `Grid` in grid.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GridUniform {
    view_proj: [[f32; 4]; 4],
    camera: [f32; 4],
    bounds: [f32; 4],
    params: [f32; 4],
    color: [f32; 4],
    tint_at: [f32; 4],
    tint_color: [f32; 4],
}

pub(super) struct GridPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("grid shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../grid.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            // Pulled toward the camera, so it wins over a surface it lies on.
            bias: wgpu::DepthBiasState {
                constant: -8,
                slope_scale: -2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

impl GridPass {
    pub(super) fn new(device: &wgpu::Device, samples: u32) -> Self {
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid"),
            contents: bytemuck::bytes_of(&GridUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid"),
            layout: &group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[&group_layout],
            push_constant_ranges: &[],
        });
        Self {
            pipeline: pipeline(device, &layout, samples),
            layout,
            group,
            buffer,
        }
    }

    pub(super) fn set_samples(&mut self, device: &wgpu::Device, samples: u32) {
        self.pipeline = pipeline(device, &self.layout, samples);
    }

    /// Uploads `grid` for this frame's camera.
    pub(super) fn prepare(
        &self,
        queue: &wgpu::Queue,
        view_proj: [[f32; 4]; 4],
        camera: [f32; 4],
        grid: &GridPlane,
    ) {
        let (tint_at, tint_color) = match grid.tint {
            Some(tint) => (
                [tint.at[0], tint.at[1], tint.radius.max(0.001), tint.amount],
                [tint.color[0], tint.color[1], tint.color[2], 1.0],
            ),
            None => ([0.0, 0.0, 1.0, 0.0], [0.0; 4]),
        };
        let uniform = GridUniform {
            view_proj,
            camera,
            bounds: [grid.min[0], grid.min[1], grid.max[0], grid.max[1]],
            params: [
                grid.height,
                grid.spacing.max(0.001),
                grid.major_every.max(1.0),
                grid.fade_distance,
            ],
            color: [grid.color[0], grid.color[1], grid.color[2], grid.strength],
            tint_at,
            tint_color,
        };
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Draws the grid. Binds its own group 0: set the frame's again after.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..4, 0..1);
    }
}
