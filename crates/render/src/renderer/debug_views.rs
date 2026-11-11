//! The editor's debug views: the scene drawn unlit, as a wireframe, as its
//! normals, base color, metalness, roughness or ambient occlusion, as
//! overdraw, or colored by level of detail
//! ([`TextureRenderer::set_debug_view`](super::TextureRenderer::set_debug_view)).
//! Only the `debug-views` feature builds this, which the editor turns on
//! (and the player, for its console's `wireframe`), and nothing here
//! exists while the lit view is shown.
//!
//! A debug view is drawn after the frame's own passes, over the finished
//! picture and under the UI, at the output's size, with a depth target of
//! its own. Every batch the scene drew is drawn again by `vs_debug` and
//! `fs_debug` (debug_views.wgsl), which read the mesh's corners from its
//! buffers rather than through a vertex layout, so the wireframe knows
//! each triangle's corners. Terrain chunks show their shape (wireframe,
//! normals, overdraw), with the plain white material in their layers'
//! place, and materials' own surface shaders are left out: their maps and
//! values are shown as the standard material reads them.

use std::collections::BTreeMap;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use super::gpu::{INSTANCE_ATTRIBUTES, shader_module};
use super::{Batch, DEPTH_FORMAT, Draw, InstanceRaw, MeshKey, ScenePipeline, terrain};

/// What the editor's viewport can show in place of the lit scene. The
/// order matches `DEBUG_*` in debug_views.wgsl.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugView {
    /// Each surface's color and glow, with no light or shadow.
    Unlit,
    /// Each triangle's edges, over its surface dimmed.
    Wireframe,
    /// The way each surface faces in the world, normal maps included: x,
    /// y and z as red, green and blue, from -1 (0) to 1 (255).
    Normals,
    /// Each surface's base color, its color map included, unlit.
    BaseColor,
    /// Metalness as grey, from 0 (black) to 1 (white).
    Metalness,
    /// Roughness as grey, from 0 (black) to 1 (white).
    Roughness,
    /// How much surrounding light reaches each surface, from its occlusion
    /// map and screen-space ambient occlusion: white is wholly open.
    AmbientOcclusion,
    /// How many surfaces are drawn over each pixel, hidden ones included:
    /// brighter the more.
    Overdraw,
    /// Each item's level of detail: green for its own mesh, then yellow,
    /// orange and red for its simpler copies. Grey has none.
    LodLevels,
}

impl DebugView {
    /// Every view, in menu order.
    pub const ALL: [DebugView; 9] = [
        DebugView::Unlit,
        DebugView::Wireframe,
        DebugView::Normals,
        DebugView::BaseColor,
        DebugView::Metalness,
        DebugView::Roughness,
        DebugView::AmbientOcclusion,
        DebugView::Overdraw,
        DebugView::LodLevels,
    ];
}

/// What `fs_debug` is told about one batch. Must match `Debug` in
/// debug_views.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DebugUniform {
    view_proj: [[f32; 4]; 4],
    /// The view, 1 for 16-bit indices, the batch's level of detail (-1 for
    /// none), then unused.
    settings: [f32; 4],
    /// The scene's size over the output's, then unused.
    scale: [f32; 4],
}

/// The scene's shader with the debug views' entry points after it.
const DEBUG_SHADER: &str = concat!(
    include_str!("../sky.wgsl"),
    include_str!("../fog.wgsl"),
    include_str!("../clouds.wgsl"),
    include_str!("../lighting.wgsl"),
    include_str!("../shader.wgsl"),
    include_str!("../surface.wgsl"),
    include_str!("../debug_views.wgsl")
);

/// The debug view shown, and what drawing it needs. Exists only while one is.
pub(super) struct DebugViews {
    pub(super) view: DebugView,
    layout: wgpu::BindGroupLayout,
    /// Depth tested, with back faces culled and (for double-sided
    /// materials) drawn.
    solid: [wgpu::RenderPipeline; 2],
    /// Added up, untested, the same two ways.
    overdraw: [wgpu::RenderPipeline; 2],
    /// One [`DebugUniform`] per batch, `stride` bytes apart.
    uniforms: wgpu::Buffer,
    stride: u64,
    capacity: usize,
    /// At the output's size.
    depth: Option<((u32, u32), wgpu::TextureView)>,
}

impl ScenePipeline {
    /// Shows `view` in place of the lit scene from the next frame, or the
    /// lit scene again for `None`, which frees everything the debug views use.
    pub(super) fn set_debug_view(&mut self, view: Option<DebugView>) {
        match (view, &mut self.debug_views) {
            (None, _) => self.debug_views = None,
            (Some(view), Some(debug)) => debug.view = view,
            (Some(view), None) => {
                self.debug_views = Some(Box::new(DebugViews::new(
                    &self.device,
                    [
                        &self.frame_layout,
                        &self.material_layout,
                        &self.skinning.layout,
                    ],
                    self.color_format,
                    view,
                )));
            }
        }
    }
}

impl DebugViews {
    fn new(
        device: &wgpu::Device,
        [frame, material, skin]: [&wgpu::BindGroupLayout; 3],
        color_format: wgpu::TextureFormat,
        view: DebugView,
    ) -> Self {
        let read_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("debug view"),
            entries: &[
                read_entry(2),
                read_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size_of::<DebugUniform>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("debug view"),
            bind_group_layouts: &[Some(frame), Some(material), Some(skin), Some(&layout)],
            immediate_size: 0,
        });
        let shader = shader_module(device, "debug view", DEBUG_SHADER);
        let pipeline = |overdraw: bool, double_sided: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if overdraw {
                    "debug view overdraw"
                } else {
                    "debug view"
                }),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_debug"),
                    compilation_options: Default::default(),
                    // Only the instances: the corners are read by hand.
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<InstanceRaw>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &INSTANCE_ATTRIBUTES,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: (!double_sided).then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!overdraw),
                    depth_compare: Some(if overdraw {
                        wgpu::CompareFunction::Always
                    } else {
                        wgpu::CompareFunction::Less
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_debug"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: Some(if overdraw {
                            wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent::REPLACE,
                            }
                        } else {
                            wgpu::BlendState::REPLACE
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let stride = (size_of::<DebugUniform>() as u64).next_multiple_of(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        ));
        let capacity = 64;
        Self {
            view,
            solid: [pipeline(false, false), pipeline(false, true)],
            overdraw: [pipeline(true, false), pipeline(true, true)],
            uniforms: uniform_buffer(device, stride, capacity),
            layout,
            stride,
            capacity,
            depth: None,
        }
    }

    /// Draws the view over `target`, which is `scene`'s output: each of
    /// `batches` (the frame's, at `draws`' levels of detail) as the camera
    /// sees it through `view_proj`.
    pub(super) fn draw(
        &mut self,
        scene: &ScenePipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        (batches, draws): (&[Batch], &[Draw]),
        view_proj: Mat4,
    ) {
        let device = &scene.device;
        let size = scene.output_size;
        if self.depth.as_ref().map(|(at, _)| *at) != Some(size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("debug view depth"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((size, texture.create_view(&Default::default())));
        }
        if batches.len() > self.capacity {
            self.capacity = batches.len().next_power_of_two();
            self.uniforms = uniform_buffer(device, self.stride, self.capacity);
        }
        // Each level of detail is its own mesh.
        let levels: BTreeMap<MeshKey, usize> = draws
            .iter()
            .filter_map(|draw| Some((draw.mesh(), draw.level?)))
            .collect();
        let scene_size = scene.frame_targets.size;
        let scale = [
            scene_size.0 as f32 / size.0.max(1) as f32,
            scene_size.1 as f32 / size.1.max(1) as f32,
            0.0,
            0.0,
        ];
        let mut bytes = vec![0; batches.len() * self.stride as usize];
        for (batch, slot) in batches
            .iter()
            .zip(bytes.chunks_exact_mut(self.stride as usize))
        {
            let sixteen = scene
                .gpu_mesh(batch.mesh)
                .is_some_and(|mesh| mesh.index_format == wgpu::IndexFormat::Uint16);
            let level = levels.get(&batch.mesh).map_or(-1.0, |&level| level as f32);
            let uniform = DebugUniform {
                view_proj: view_proj.to_cols_array_2d(),
                settings: [
                    self.view as u32 as f32,
                    f32::from(u8::from(sixteen)),
                    level,
                    0.0,
                ],
                scale,
            };
            slot[..size_of::<DebugUniform>()].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        if !bytes.is_empty() {
            scene.queue.write_buffer(&self.uniforms, 0, &bytes);
        }
        // One binding per mesh, made each frame: a mesh's buffers can be
        // remade between frames, and only the editor draws this.
        let mut groups: BTreeMap<MeshKey, wgpu::BindGroup> = BTreeMap::new();
        for batch in batches {
            let Some(mesh) = scene.gpu_mesh(batch.mesh) else {
                continue;
            };
            groups.entry(batch.mesh).or_insert_with(|| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("debug view mesh"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: mesh.vertices.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: mesh.indices.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.uniforms,
                                offset: 0,
                                size: wgpu::BufferSize::new(size_of::<DebugUniform>() as u64),
                            }),
                        },
                    ],
                })
            });
        }

        let overdraw = self.view == DebugView::Overdraw;
        let background = if overdraw { 0.0 } else { 0.02 };
        let Some((_, depth)) = &self.depth else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("debug view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: background,
                        g: background,
                        b: background,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &scene.frame_bind_group, &[]);
        pass.set_vertex_buffer(0, scene.instance_buffer.slice(..));
        let pipelines = if overdraw {
            &self.overdraw
        } else {
            &self.solid
        };
        for (index, batch) in batches.iter().enumerate() {
            let (Some(mesh), Some(group)) = (scene.gpu_mesh(batch.mesh), groups.get(&batch.mesh))
            else {
                continue;
            };
            let material = if batch.material.terrain == terrain::TerrainKey::default() {
                scene
                    .materials
                    .get(&batch.material)
                    .and_then(|cached| cached.gpu.as_ref())
                    .unwrap_or(&scene.plain_material)
            } else {
                &scene.plain_material
            };
            pass.set_pipeline(&pipelines[usize::from(batch.double_sided)]);
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_bind_group(2, scene.skin_group(mesh), &[]);
            pass.set_bind_group(3, group, &[(index as u64 * self.stride) as u32]);
            pass.draw(0..mesh.index_count, batch.instances.clone());
        }
    }
}

fn uniform_buffer(device: &wgpu::Device, stride: u64, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("debug view batches"),
        size: stride * capacity as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
