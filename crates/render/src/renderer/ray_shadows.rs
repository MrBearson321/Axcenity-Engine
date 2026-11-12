//! Ray traced sun shadows (RT1, `docs/rendering/ray-tracing.md`): a compute
//! pass traces rays toward the sun from each solid pixel of the prepass
//! through the ray traced scene (`ray_tracing.rs`), and for soft shadows
//! smooths the result across and down (`ray_shadows.wgsl`). The scene pass
//! reads the mask in `sun_visibility` (lighting.wgsl, frame binding 17), and
//! keeps the shadow maps where the mask has nothing: see-through surfaces,
//! past the reach, and a pixel whose surface isn't the one the prepass drew.
//!
//! Exists only while the setting is on, the GPU can ray trace, the world
//! wants shadows and the prepass is drawn; otherwise the frame binds a
//! 4-byte stand-in.

use glam::{Mat4, Vec3};

use crate::draw_list::DrawList;
use crate::graphics::{EffectiveGraphics, RayTracedShadows};

use super::MAX_SUN_SIZE;
use super::types::OcclusionTargets;

/// Must match `RayShadows` in ray_shadows.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct RayShadowUniform {
    inv_view_proj: [[f32; 4]; 4],
    view_to_world: [[f32; 4]; 4],
    /// xyz: the camera, w: how far ray traced shadows reach.
    camera: [f32; 4],
    /// xyz: toward the sun, w: how wide the sun looks (0: hard shadows).
    sun: [f32; 4],
    /// Rays per pixel, a number changing each frame under TAA, a pixel's
    /// size per stud away, unused.
    settings: [f32; 4],
    /// The scene's width and height in pixels.
    size: [u32; 4],
}

/// How the world's shadows are ray traced this frame, if they are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RayShadowPlan {
    /// How far from the camera, in studs: the ray tracing distance, or the
    /// world's `ShadowDistance` if that's nearer.
    pub(super) reach: f32,
    /// How wide the sun looks (a soft edge's width per stud between caster
    /// and receiver), from the world's `ShadowSoftness`: 0 for hard.
    pub(super) sun_size: f32,
    /// Rays per pixel: one for hard shadows.
    pub(super) rays: u32,
}

impl RayShadowPlan {
    /// The plan for `list` with `graphics`, or `None` while ray traced
    /// shadows are off, the GPU can't trace rays or the world's shadows
    /// are off.
    pub(super) fn of(list: &DrawList, graphics: &EffectiveGraphics) -> Option<Self> {
        let world = list.shadows;
        // A distance that isn't a number counts as no shadows.
        let has_distance = world.distance > 0.0;
        if graphics.ray_traced_shadows == RayTracedShadows::Off
            || !graphics.ray_queries
            || !world.enabled
            || !has_distance
        {
            return None;
        }
        let softness = if world.softness.is_finite() {
            world.softness.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let sun_size = softness * MAX_SUN_SIZE;
        Some(Self {
            reach: graphics.ray_tracing_distance.min(world.distance),
            sun_size,
            rays: if sun_size > 0.0 {
                graphics.ray_traced_shadows.rays()
            } else {
                1
            },
        })
    }

    pub(super) fn soft(&self) -> bool {
        self.sun_size > 0.0
    }
}

/// The pass's pipelines and buffers, at one scene size.
pub(super) struct RayShadows {
    layout: wgpu::BindGroupLayout,
    trace: wgpu::ComputePipeline,
    blur_across: wgpu::ComputePipeline,
    blur_down: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    /// The scene's size the buffers are for, and whether the soft passes'
    /// two buffers exist.
    size: (u32, u32),
    soft: bool,
    /// What the scene reads: a u32 per pixel (see ray_shadows.wgsl).
    pub(super) mask: wgpu::Buffer,
    /// The soft passes' rays, then rays smoothed across.
    soft_buffers: Option<[wgpu::Buffer; 2]>,
    /// Bound where a pass reads nothing.
    nothing: wgpu::Buffer,
}

impl RayShadows {
    pub(super) fn new(device: &wgpu::Device, (size, soft): ((u32, u32), bool)) -> Self {
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let buffer = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray traced shadows"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                },
                texture(1, wgpu::TextureSampleType::Depth),
                texture(2, wgpu::TextureSampleType::Float { filterable: false }),
                buffer(3, wgpu::BufferBindingType::Uniform),
                buffer(4, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(5, wgpu::BufferBindingType::Storage { read_only: false }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ray traced shadows"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = super::gpu::shader_module(
            device,
            "ray traced shadows",
            include_str!("../ray_shadows.wgsl"),
        );
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray traced shadows"),
            size: size_of::<RayShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            trace: pipeline("cs_trace"),
            blur_across: pipeline("cs_blur_across"),
            blur_down: pipeline("cs_blur_down"),
            layout,
            uniform,
            size,
            soft,
            mask: pixel_buffer(device, "ray traced shadow mask", size),
            soft_buffers: soft.then(|| {
                [
                    pixel_buffer(device, "ray traced shadow rays", size),
                    pixel_buffer(device, "ray traced shadow rays (smoothed across)", size),
                ]
            }),
            nothing: stand_in(device),
        }
    }

    /// Whether these buffers are for a scene of `size` drawn `soft` or not.
    pub(super) fn fits(&self, size: (u32, u32), soft: bool) -> bool {
        self.size == size && self.soft == soft
    }

    /// Bytes of its buffers.
    pub(super) fn bytes(&self) -> u64 {
        self.mask.size()
            + self
                .soft_buffers
                .as_ref()
                .map_or(0, |[a, b]| a.size() + b.size())
    }

    /// Records the pass: rays from what `prepass` drew through `tlas`, as
    /// `uniform` says, into the mask.
    pub(super) fn draw(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (tlas, prepass): (&wgpu::Tlas, &OcclusionTargets),
        uniform: &RayShadowUniform,
    ) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
        let group = |source: &wgpu::Buffer, destination: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ray traced shadows"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: tlas.as_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&prepass.normal_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: source.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: destination.as_entire_binding(),
                    },
                ],
            })
        };
        let steps: Vec<(&wgpu::ComputePipeline, wgpu::BindGroup)> = match &self.soft_buffers {
            None => vec![(&self.trace, group(&self.nothing, &self.mask))],
            Some([rays, across]) => vec![
                (&self.trace, group(&self.nothing, rays)),
                (&self.blur_across, group(rays, across)),
                (&self.blur_down, group(across, &self.mask)),
            ],
        };
        let groups = (self.size.0.div_ceil(8), self.size.1.div_ceil(8));
        for (pipeline, bind_group) in &steps {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ray traced shadows"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }
    }
}

/// What the pass reads this frame: `plan` for `list`, drawn through
/// `inv_view_proj` (the prepass's, with TAA's jitter) from `view`, at
/// `size`. `turn` changes each frame under TAA (0 without).
pub(super) fn ray_shadow_uniform(
    list: &DrawList,
    plan: &RayShadowPlan,
    (inv_view_proj, view): (Mat4, Mat4),
    size: (u32, u32),
    turn: f32,
) -> RayShadowUniform {
    let sun = list.sun.direction.normalize_or(Vec3::Y);
    let tan_half = (list.camera.fov_y_degrees.to_radians() * 0.5).tan();
    RayShadowUniform {
        inv_view_proj: inv_view_proj.to_cols_array_2d(),
        view_to_world: view.inverse().to_cols_array_2d(),
        camera: list.camera.position.extend(plan.reach).to_array(),
        sun: sun.extend(plan.sun_size).to_array(),
        settings: [
            plan.rays as f32,
            turn,
            2.0 * tan_half / size.1.max(1) as f32,
            0.0,
        ],
        size: [size.0, size.1, 0, 0],
    }
}

/// A u32 per pixel of a scene of `size`.
fn pixel_buffer(device: &wgpu::Device, label: &str, (width, height): (u32, u32)) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: u64::from(width.max(1)) * u64::from(height.max(1)) * 4,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

/// The 4-byte mask bound while there's none (the scene never reads it).
pub(super) fn stand_in(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("no ray traced shadows"),
        size: 4,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}
