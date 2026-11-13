//! The editor's Path Traced view (LE6, `docs/rendering/ray-tracing.md`): a
//! reference picture of the scene, made by following rays as they bounce
//! through hardware ray tracing's scene (`ray_tracing.rs`), one more path a
//! pixel each frame, averaged while nothing changes (`path_tracer.wgsl`).
//!
//! It's a debug view ([`super::DebugView::PathTraced`]), so it exists only
//! while shown, on a GPU with ray queries. Each frame a compute pass adds a
//! path to each pixel's sum, and a fullscreen pass shows the sums' average
//! over the frame's own picture. The sums start again when the camera
//! moves, the output's size changes, an item moves (the TLAS is rebuilt) or
//! the light changes (the sun, the sky or a lamp); after [`MOST_PATHS`] the
//! picture is left as it is, tracing nothing.
//!
//! Paths find each item's normals between its triangle's corners: like
//! cut-outs' texture coordinates, each traced mesh's are copied once on the
//! GPU, three a triangle, into a buffer of the view's own.

use std::hash::{Hash, Hasher};

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use rustc_hash::{FxHashMap, FxHasher};
use wgpu::util::DeviceExt;

use super::ray_tracing::RayScene;
use super::{GpuMesh, MeshKey};

/// After this many paths a pixel the Path Traced view's picture is left as
/// it is.
pub const MOST_PATHS: u32 = 2048;
/// The most bounces a path takes after the camera's ray.
const MOST_BOUNCES: u32 = 4;
/// A slot whose mesh has no normals copied (`PT_NO_NORMALS`).
const NO_NORMALS: u32 = u32::MAX;
/// Bytes a triangle's normals take: three corners, two words each.
const TRIANGLE_BYTES: u64 = 24;

/// Must match `PathTrace` in path_tracer.wgsl and path_traced_view.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PathTraceUniform {
    inv_view_proj: [[f32; 4]; 4],
    /// The traced picture's width and height, which path this is of each
    /// pixel's (0 starts the sums again), the most bounces.
    counts: [u32; 4],
    /// Output pixels a side per traced pixel, then unused.
    settings: [f32; 4],
}

/// Must match `Extract` in path_tracer_normals.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ExtractUniform {
    mesh: [u32; 4],
}

/// What the sums so far were traced with: a change starts them again.
#[derive(Clone, Copy, PartialEq)]
struct Traced {
    view_proj: [f32; 16],
    size: (u32, u32),
    lighting: u64,
}

/// The frame's bindings the view reads: its uniform, its lamps, and its
/// sky (`sky_specular`). Only these, so the view stays within eight
/// storage buffers a stage.
pub(super) const FRAME_BINDINGS: [u32; 5] = [0, 3, 10, 11, 12];

/// The Path Traced view's pipelines and buffers.
pub(super) struct PathTracer {
    /// The frame's bindings the compute pass sees ([`FRAME_BINDINGS`]), and
    /// their bind group (made again with the frame's).
    pub(super) frame_layout: wgpu::BindGroupLayout,
    pub(super) frame_group: Option<wgpu::BindGroup>,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    show_layout: wgpu::BindGroupLayout,
    show: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    /// Each traced pixel's sum, at the traced size.
    sums: Option<((u32, u32), wgpu::Buffer)>,
    normals: Normals,
    /// Each TLAS slot's mesh's first triangle in the normals.
    slots: wgpu::Buffer,
    slot_capacity: usize,
    /// Paths a pixel so far.
    pub(super) paths: u32,
    traced: Option<Traced>,
}

/// Every traced mesh's normals, three a triangle.
struct Normals {
    buffer: wgpu::Buffer,
    capacity: u32,
    next_triangle: u32,
    /// Each mesh's (and version's) first triangle.
    meshes: FxHashMap<(MeshKey, u64), u32>,
    extract: wgpu::ComputePipeline,
    extract_layout: wgpu::BindGroupLayout,
}

impl PathTracer {
    /// `frame_entries` are the frame's bindings; those the view reads are
    /// made visible to its compute pass.
    pub(super) fn new(
        device: &wgpu::Device,
        frame_entries: &[wgpu::BindGroupLayoutEntry],
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let frame_entries: Vec<_> = frame_entries
            .iter()
            .filter(|entry| FRAME_BINDINGS.contains(&entry.binding))
            .map(|entry| wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::COMPUTE,
                ..*entry
            })
            .collect();
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame (path traced)"),
            entries: &frame_entries,
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let compute = wgpu::ShaderStages::COMPUTE;
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size_of::<PathTraceUniform>() as u64),
        };
        let cut_outs = super::ray_cut_outs::layout_entries(6, compute);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path traced"),
            entries: &[
                entry(
                    0,
                    compute,
                    wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                ),
                entry(1, compute, uniform),
                entry(2, compute, storage(true)),
                entry(3, compute, storage(true)),
                entry(4, compute, storage(true)),
                entry(5, compute, storage(false)),
                cut_outs[0],
                cut_outs[1],
                cut_outs[2],
                cut_outs[3],
            ],
        });
        let shader =
            super::gpu::shader_module(device, "path traced", super::gpu::PATH_TRACER_SHADER);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("path traced"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("path traced"),
                    bind_group_layouts: &[Some(&frame_layout), Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("cs_path_trace"),
            compilation_options: Default::default(),
            cache: None,
        });

        let fragment = wgpu::ShaderStages::FRAGMENT;
        let show_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path traced view"),
            entries: &[
                entry(0, fragment, uniform),
                entry(1, fragment, storage(true)),
            ],
        });
        let show_shader = super::gpu::shader_module(
            device,
            "path traced view",
            include_str!("../path_traced_view.wgsl"),
        );
        let show = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("path traced view"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("path traced view"),
                    bind_group_layouts: &[Some(&show_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &show_shader,
                entry_point: Some("vs_path_traced"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &show_shader,
                entry_point: Some("fs_path_traced"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let slot_capacity = 256;
        Self {
            frame_layout,
            frame_group: None,
            layout,
            pipeline,
            show_layout,
            show,
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("path traced"),
                size: size_of::<PathTraceUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sums: None,
            normals: Normals::new(device),
            slots: slot_buffer(device, slot_capacity),
            slot_capacity,
            paths: 0,
            traced: None,
        }
    }

    /// Adds a path to each pixel and shows their average over `target`
    /// (`size` pixels), seen through `view_proj`, in `rays`' scene.
    /// `lighting` stands for the frame's light (a change starts the sums
    /// again) and `mesh` finds an uploaded mesh. Does nothing until the
    /// frame's bindings are made (`frame_group`).
    pub(super) fn draw<'a>(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        (rays, view_proj, size, lighting): (&RayScene, Mat4, (u32, u32), u64),
        storage_limit: u64,
        mesh: impl Fn(MeshKey) -> Option<&'a GpuMesh>,
    ) {
        let Some(frame_group) = &self.frame_group else {
            return;
        };
        let Some(surfaces) = &rays.surfaces else {
            return;
        };
        // A picture too big for one storage buffer is traced at half the
        // size (or less) and shown stretched.
        let mut scale = 1;
        while u64::from(size.0.div_ceil(scale)) * u64::from(size.1.div_ceil(scale)) * 16
            > storage_limit
        {
            scale *= 2;
        }
        let traced_size = (size.0.div_ceil(scale).max(1), size.1.div_ceil(scale).max(1));
        if self.sums.as_ref().map(|(at, _)| *at) != Some(traced_size) {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("path traced sums"),
                size: u64::from(traced_size.0) * u64::from(traced_size.1) * 16,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            self.sums = Some((traced_size, buffer));
            self.traced = None;
        }

        // Each slot's normals, copied for meshes new to the view.
        let mut first_triangles = Vec::with_capacity(rays.slot_count());
        for key in rays.slot_meshes() {
            let first = mesh(key).map_or(NO_NORMALS, |gpu| {
                self.normals.add_mesh(device, encoder, key, gpu)
            });
            first_triangles.push(first);
        }
        if first_triangles.len() > self.slot_capacity {
            self.slot_capacity = first_triangles.len().next_power_of_two();
            self.slots = slot_buffer(device, self.slot_capacity);
        }
        if !first_triangles.is_empty() {
            queue.write_buffer(&self.slots, 0, bytemuck::cast_slice(&first_triangles));
        }

        let now = Traced {
            view_proj: view_proj.to_cols_array(),
            size,
            lighting,
        };
        if self.traced != Some(now) || rays.stats.rebuilt {
            self.paths = 0;
            self.traced = Some(now);
        }
        let Some((_, sums)) = &self.sums else {
            return;
        };
        let uniform = PathTraceUniform {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            counts: [traced_size.0, traced_size.1, self.paths, MOST_BOUNCES],
            settings: [scale as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        if self.paths < MOST_PATHS {
            let [a, b, c, d] = rays.cut_outs.entries(6);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path traced"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: rays.tlas().as_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: surfaces.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.slots.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.normals.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: sums.as_entire_binding(),
                    },
                    a,
                    b,
                    c,
                    d,
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("path traced"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, frame_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.dispatch_workgroups(traced_size.0.div_ceil(8), traced_size.1.div_ceil(8), 1);
            drop(pass);
            self.paths += 1;
        }

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path traced view"),
            layout: &self.show_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: sums.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("path traced view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.show);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Normals {
    fn new(device: &wgpu::Device) -> Self {
        let capacity = 4096;
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let extract_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path traced normals"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = super::gpu::shader_module(
            device,
            "path traced normals",
            include_str!("../path_tracer_normals.wgsl"),
        );
        let extract = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("path traced normals"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("path traced normals"),
                    bind_group_layouts: &[Some(&extract_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("cs_extract_normals"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            buffer: normal_buffer(device, capacity),
            capacity,
            next_triangle: 0,
            meshes: FxHashMap::default(),
            extract,
            extract_layout,
        }
    }

    /// The first triangle of `gpu`'s (the mesh `key`'s) normals, recording
    /// their copy into `encoder` if they aren't there yet (growing the
    /// buffer to fit).
    fn add_mesh(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        key: MeshKey,
        gpu: &GpuMesh,
    ) -> u32 {
        if let Some(&first) = self.meshes.get(&(key, gpu.version)) {
            return first;
        }
        let triangles = gpu.index_count / 3;
        let first = self.next_triangle;
        let end = first + triangles;
        if end > self.capacity {
            let capacity = end.next_power_of_two().max(self.capacity * 2);
            let bigger = normal_buffer(device, capacity);
            encoder.copy_buffer_to_buffer(&self.buffer, 0, &bigger, 0, self.buffer.size());
            self.buffer = bigger;
            self.capacity = capacity;
        }
        self.next_triangle = end;
        self.meshes.insert((key, gpu.version), first);
        if triangles == 0 {
            return first;
        }
        let wide = gpu.index_format == wgpu::IndexFormat::Uint32;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("path traced normals"),
            contents: bytemuck::bytes_of(&ExtractUniform {
                mesh: [first, triangles, u32::from(wide), 0],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path traced normals"),
            layout: &self.extract_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gpu.vertices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: gpu.indices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("path traced normals"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.extract);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((triangles * 3).div_ceil(64), 1, 1);
        first
    }
}

fn normal_buffer(device: &wgpu::Device, triangles: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("path traced normals"),
        size: u64::from(triangles) * TRIANGLE_BYTES,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn slot_buffer(device: &wgpu::Device, slots: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("path traced slots"),
        size: (slots * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A number that changes when the frame's light does: the sun, the sky and
/// every lamp (`frame` is the frame's uniform's bytes from its sun
/// direction to its sky colors' end, `lamps` the lamps' bytes).
pub(super) fn lighting_key(parts: &[&[u8]]) -> u64 {
    let mut hasher = FxHasher::default();
    for part in parts {
        part.hash(&mut hasher);
    }
    hasher.finish()
}
