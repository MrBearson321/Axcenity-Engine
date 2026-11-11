//! The depth and normal prepass and screen-space ambient occlusion
//! (`ssao.wgsl`).
//!
//! While ambient occlusion is on, solid items' depth and view-space normals
//! are drawn first (the prepass, at the scene's size). The occlusion pass
//! reads them at half that size and the blur smooths its result, which the
//! scene pass multiplies into ambient and sky light. Screen-space outlines
//! (`edges.wgsl`) read the same prepass.

use std::sync::Arc;

use crate::draw_list::{DrawList, Edges};
use crate::graphics::AmbientOcclusion;

use super::gpu::{create_pipeline, shader_module, target_bytes};
use super::lighting::{camera_matrices, linear_color};
use super::post::fullscreen_pass;
use super::{
    DEPTH_FORMAT, EdgeUniform, HDR_FORMAT, NORMAL_FORMAT, OCCLUSION_FORMAT, OCCLUSION_RADIUS,
    OcclusionPasses, OcclusionTargets, OcclusionUniform,
};

impl OcclusionPasses {
    /// The prepass pipelines bind the frame, a material and a skin like the
    /// scene pass's (`item_layouts`), so they draw items the same way.
    pub(super) fn new(device: &wgpu::Device, item_layouts: [&wgpu::BindGroupLayout; 3]) -> Self {
        let scene_shader = shader_module(device, "prepass", super::gpu::SCENE_SHADER);
        let prepass = |double_sided| {
            create_pipeline(
                device,
                &scene_shader,
                ("vs_main", "fs_prepass"),
                &item_layouts,
                (NORMAL_FORMAT, 1),
                (false, double_sided),
            )
        };
        // Both passes read the prepass's depth and one other texture by
        // pixel (`textureLoad`), so nothing here is filtered.
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient occlusion"),
            entries: &[
                texture(0, wgpu::TextureSampleType::Depth),
                texture(1, wgpu::TextureSampleType::Float { filterable: false }),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ambient occlusion"),
            size: size_of::<OcclusionUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let edge_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-space outlines"),
            size: size_of::<EdgeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("../ssao.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ambient occlusion"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let fullscreen = |label, entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(OCCLUSION_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            prepass: prepass(false),
            double_sided_prepass: prepass(true),
            occlusion: fullscreen("ambient occlusion", "fs_occlusion"),
            blur: fullscreen("ambient occlusion blur", "fs_blur"),
            layout,
            uniform,
            edge_uniform,
        }
    }

    /// Tests and blurs the occlusion of what the prepass drew into `targets`.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder, targets: &OcclusionTargets) {
        let clear = wgpu::LoadOp::Clear(wgpu::Color::WHITE);
        fullscreen_pass(
            encoder,
            "ambient occlusion",
            &targets.raw_view,
            clear,
            &self.occlusion,
            &[&targets.occlusion_group],
        );
        fullscreen_pass(
            encoder,
            "ambient occlusion blur",
            &targets.blurred_view,
            clear,
            &self.blur,
            &[&targets.blur_group],
        );
    }
}

impl OcclusionTargets {
    /// The prepass's targets for a scene of `size` pixels and the occlusion
    /// targets at half of it, holding a clone of `alive`.
    pub(super) fn new(
        device: &wgpu::Device,
        passes: &OcclusionPasses,
        (width, height): (u32, u32),
        alive: &Arc<()>,
    ) -> Self {
        let bytes = std::cell::Cell::new(0);
        let view = |label, (width, height): (u32, u32), format| {
            let (width, height) = (width.max(1), height.max(1));
            bytes.set(bytes.get() + target_bytes((width, height, 1), format, 1));
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let half = (width.div_ceil(2), height.div_ceil(2));
        let depth_view = view("prepass depth", (width, height), DEPTH_FORMAT);
        let normal_view = view("prepass normals", (width, height), NORMAL_FORMAT);
        let raw_view = view("ambient occlusion", half, OCCLUSION_FORMAT);
        let blurred_view = view("ambient occlusion (blurred)", half, OCCLUSION_FORMAT);
        let group = |source: &wgpu::TextureView, uniform: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ambient occlusion"),
                layout: &passes.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        Self {
            occlusion_group: group(&normal_view, &passes.uniform),
            blur_group: group(&raw_view, &passes.uniform),
            edge_group: group(&normal_view, &passes.edge_uniform),
            depth_view,
            normal_view,
            raw_view,
            blurred_view,
            bytes: bytes.get(),
            _alive: alive.clone(),
        }
    }
}

/// What the occlusion passes read for drawing `list` at `aspect` with
/// `quality`'s samples.
pub(super) fn occlusion_uniform(
    list: &DrawList,
    aspect: f32,
    quality: AmbientOcclusion,
) -> OcclusionUniform {
    let (_, projection) = camera_matrices(&list.camera, aspect);
    OcclusionUniform {
        projection: projection.to_cols_array_2d(),
        inv_projection: projection.inverse().to_cols_array_2d(),
        settings: [quality.samples() as f32, OCCLUSION_RADIUS, 0.0, 0.0],
    }
}

/// What the screen-space outline pass reads for drawing `edges` over a
/// scene of `size` pixels seen as `list`'s camera at `aspect` sees it.
pub(super) fn edge_uniform(
    list: &DrawList,
    edges: &Edges,
    aspect: f32,
    (_, height): (u32, u32),
) -> EdgeUniform {
    let (_, projection) = camera_matrices(&list.camera, aspect);
    let thickness = if edges.thickness.is_finite() {
        edges.thickness.max(0.0)
    } else {
        1.0
    };
    EdgeUniform {
        inv_projection: projection.inverse().to_cols_array_2d(),
        color: linear_color(edges.color).extend(0.0).to_array(),
        settings: [thickness * height as f32 / 1080.0, 0.0, 0.0, 0.0],
    }
}

/// The pipeline that draws screen-space outlines into the scene pass, with
/// `samples` per pixel, reading the prepass through `layout`.
pub(super) fn create_edge_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("../edges.wgsl"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("screen-space outlines"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("screen-space outlines"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        // Over everything drawn so far, leaving the depth alone.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_edges"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
