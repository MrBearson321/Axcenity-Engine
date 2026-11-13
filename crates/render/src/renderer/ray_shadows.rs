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
use super::ray_tracing::RayScene;
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
    /// size per stud away, and how far the fog's grid reaches while its
    /// air is traced (0 while not).
    settings: [f32; 4],
    /// The scene's width and height in pixels.
    size: [u32; 4],
    /// 1 to trace lamps' shadows, the light tiles' size in pixels and how
    /// many there are across.
    lamps: [u32; 4],
    /// How wide a lamp's bulb is in studs (0: hard shadows), rays per lamp
    /// for soft shadows.
    lamp: [f32; 4],
}

/// How wide a lamp's bulb is taken to be, in studs, at the world's
/// `ShadowSoftness` of 1: its shadows' soft edge per stud between caster and
/// receiver, over the distance to the light. A rect light's soft shadows
/// spread over its own rectangle instead.
pub(super) const MAX_LAMP_SIZE: f32 = 1.0;

/// The volumetric fog's grid, whose cells' sunlight is traced (LE3).
const AIR_GRID: [u32; 3] = super::fog::GRID;
const AIR_CELLS: u64 = AIR_GRID[0] as u64 * AIR_GRID[1] as u64 * AIR_GRID[2] as u64;

/// What a `RayShadows`' buffers are made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    /// The soft passes' buffers.
    pub(super) soft: bool,
    /// The lamps' part of the mask (LE1).
    pub(super) lamps: bool,
    /// The volumetric fog's air's part of the mask (LE3).
    pub(super) air: bool,
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
    /// Whether lamps' shadows are traced too: some lamp drawn casts
    /// shadows (LE1).
    pub(super) lamps: bool,
    /// Whether the volumetric fog's air is traced too (LE3): its grid is
    /// drawn. The scene sets it.
    pub(super) air: bool,
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
            lamps: list.lights.iter().any(casts_shadows),
            air: false,
        })
    }

    pub(super) fn soft(&self) -> bool {
        self.sun_size > 0.0
    }

    /// What the buffers are sized for: the soft passes, and lamps.
    pub(super) fn shape(&self) -> Shape {
        Shape {
            soft: self.soft(),
            lamps: self.lamps,
            air: self.air,
        }
    }
}

/// Whether `light` casts shadows (a light's `Shadows`): a spot light's
/// beam, or a point or rect light's.
pub(super) fn casts_shadows(light: &crate::draw_list::Light) -> bool {
    light
        .cone
        .map_or(light.point_shadows.is_some(), |cone| cone.shadows.is_some())
}

/// For `LightRaw::shadow`'s third number: 0 for a light that casts no
/// shadows, otherwise 1 plus how far from it ray traced shadows start (the
/// farthest its own part reaches, so a lamp doesn't shade its own light).
pub(super) fn traced_start(light: &crate::draw_list::Light) -> f32 {
    let near = match light.cone {
        Some(cone) => cone.shadows.map(|shadows| shadows.near),
        None => light
            .point_shadows
            .map(|shadows| shadows.near.into_iter().fold(0.0, f32::max)),
    };
    near.map_or(0.0, |near| {
        1.0 + if near.is_finite() { near.max(0.0) } else { 0.0 }
    })
}

/// The pass's pipelines and buffers, at one scene size.
pub(super) struct RayShadows {
    layout: wgpu::BindGroupLayout,
    trace: wgpu::ComputePipeline,
    blur_across: wgpu::ComputePipeline,
    blur_down: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    /// The scene's size the buffers are for, whether the soft passes'
    /// two buffers exist, and whether the mask has the lamps' half.
    size: (u32, u32),
    soft: bool,
    lamps: bool,
    /// What the scene reads: a u32 per pixel for the sun (see
    /// ray_shadows.wgsl), then, from `lamp_offset` bytes, one per pixel
    /// for its lamps.
    pub(super) mask: wgpu::Buffer,
    lamp_offset: u64,
    /// Where the air's part starts (LE3), in bytes, while traced.
    air_offset: Option<u64>,
    /// The soft passes' rays, then rays smoothed across.
    soft_buffers: Option<[wgpu::Buffer; 2]>,
    /// Bound where a pass reads nothing.
    nothing: wgpu::Buffer,
    /// Bound for the lamps' half while lamps aren't traced (written by
    /// nothing; apart from `nothing`, which is bound read-only).
    no_lamps: wgpu::Buffer,
    /// Bound for the air's part while the air isn't traced.
    no_air: wgpu::Buffer,
    /// Traces the air's sunlight (`cs_air`).
    air: wgpu::ComputePipeline,
}

impl RayShadows {
    pub(super) fn new(
        device: &wgpu::Device,
        (size, Shape { soft, lamps, air }): ((u32, u32), Shape),
    ) -> Self {
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
                // Cut-outs' holes (ray_cut_outs.rs).
                buffer(6, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(7, wgpu::BufferBindingType::Storage { read_only: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Lamps (LE1): the frame's lights, their tiles, and the
                // lamps' half of the mask.
                buffer(10, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(11, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(12, wgpu::BufferBindingType::Storage { read_only: false }),
                // The volumetric fog's air (LE3): its part of the mask.
                buffer(13, wgpu::BufferBindingType::Storage { read_only: false }),
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
        let pixels = u64::from(size.0.max(1)) * u64::from(size.1.max(1)) * 4;
        // Where the lamps' half starts: storage bindings start on 256 bytes.
        let lamp_offset = pixels.next_multiple_of(256);
        let lamps_end = if lamps { lamp_offset + pixels } else { pixels };
        // Then the air's, a u32 per cell of the fog's grid.
        let air_offset = air.then(|| lamps_end.next_multiple_of(256));
        let mask = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray traced shadow mask"),
            size: air_offset.map_or(lamps_end, |offset| offset + AIR_CELLS * 4),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Self {
            trace: pipeline("cs_trace"),
            blur_across: pipeline("cs_blur_across"),
            blur_down: pipeline("cs_blur_down"),
            air: pipeline("cs_air"),
            layout,
            uniform,
            size,
            soft,
            lamps,
            mask,
            lamp_offset,
            air_offset,
            soft_buffers: soft.then(|| {
                [
                    pixel_buffer(device, "ray traced shadow rays", size),
                    pixel_buffer(device, "ray traced shadow rays (smoothed across)", size),
                ]
            }),
            nothing: stand_in(device),
            no_lamps: stand_in(device),
            no_air: stand_in(device),
        }
    }

    /// Whether these buffers are for a scene of `size` drawn with this
    /// `shape` (soft or not, lamps or not).
    pub(super) fn fits(&self, size: (u32, u32), shape: Shape) -> bool {
        self.size == size
            && self.soft == shape.soft
            && self.lamps == shape.lamps
            && self.air_offset.is_some() == shape.air
    }

    /// The sun's half of the mask.
    fn sun_mask(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &self.mask,
            offset: 0,
            size: wgpu::BufferSize::new(
                u64::from(self.size.0.max(1)) * u64::from(self.size.1.max(1)) * 4,
            ),
        })
    }

    /// Where the lamps' half of the mask starts, in u32s (for the frame's
    /// `ray_shadows.w`), or 0 without one.
    pub(super) fn lamp_words(&self) -> u32 {
        if self.lamps {
            (self.lamp_offset / 4) as u32
        } else {
            0
        }
    }

    /// Where the air's part of the mask starts, in u32s (for the fog's
    /// `dust_count.y`), or 0 while the air isn't traced.
    pub(super) fn air_words(&self) -> u32 {
        self.air_offset.map_or(0, |offset| (offset / 4) as u32)
    }

    /// Bytes of its buffers.
    pub(super) fn bytes(&self) -> u64 {
        self.mask.size()
            + self
                .soft_buffers
                .as_ref()
                .map_or(0, |[a, b]| a.size() + b.size())
    }

    /// Records the pass: rays from what `prepass` drew through `scene`'s
    /// TLAS (and its cut-outs' holes), as `uniform` says, into the mask.
    pub(super) fn draw(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (scene, prepass): (&RayScene, &OcclusionTargets),
        (lights, tiles): (&wgpu::Buffer, &wgpu::Buffer),
        uniform: &RayShadowUniform,
    ) {
        let tlas = scene.tlas();
        let cut_outs = &scene.cut_outs;
        let lamp_mask = if self.lamps {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &self.mask,
                offset: self.lamp_offset,
                size: None,
            })
        } else {
            self.no_lamps.as_entire_binding()
        };
        let air_mask = match self.air_offset {
            Some(offset) => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &self.mask,
                offset,
                size: wgpu::BufferSize::new(AIR_CELLS * 4),
            }),
            None => self.no_air.as_entire_binding(),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
        let group = |source: &wgpu::Buffer, destination: wgpu::BindingResource| {
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
                        resource: destination,
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: cut_outs.info.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: cut_outs.uvs.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::TextureView(&cut_outs.alpha_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::Sampler(&cut_outs.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: lights.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: tiles.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: lamp_mask.clone(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: air_mask.clone(),
                    },
                ],
            })
        };
        let steps: Vec<(&wgpu::ComputePipeline, wgpu::BindGroup)> = match &self.soft_buffers {
            None => vec![(&self.trace, group(&self.nothing, self.sun_mask()))],
            Some([rays, across]) => vec![
                (&self.trace, group(&self.nothing, rays.as_entire_binding())),
                (&self.blur_across, group(rays, across.as_entire_binding())),
                (&self.blur_down, group(across, self.sun_mask())),
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
        // The air's sunlight, a cell of the fog's grid per thread.
        if self.air_offset.is_some() {
            let group = group(&self.nothing, self.sun_mask());
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ray traced shadows (air)"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.air);
            pass.set_bind_group(0, &group, &[]);
            let [x, y, z] = AIR_GRID;
            pass.dispatch_workgroups(x.div_ceil(4), y.div_ceil(4), z.div_ceil(4));
        }
    }
}

/// What the pass reads this frame: `plan` for `list`, drawn through
/// `inv_view_proj` (the prepass's, with TAA's jitter) from `view`, at
/// `size`, its lamps tiled in `lamps` (tile size, tiles across). `turn`
/// changes each frame under TAA (0 without).
pub(super) fn ray_shadow_uniform(
    list: &DrawList,
    plan: &RayShadowPlan,
    (inv_view_proj, view): (Mat4, Mat4),
    (size, lamps): ((u32, u32), (u32, u32)),
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
            if plan.air {
                super::fog::VOLUMETRIC_REACH
            } else {
                0.0
            },
        ],
        size: [size.0, size.1, 0, 0],
        lamps: [
            u32::from(plan.lamps && lamps.1 > 0),
            lamps.0.max(1),
            lamps.1,
            0,
        ],
        lamp: [
            plan.sun_size / MAX_SUN_SIZE * MAX_LAMP_SIZE,
            plan.rays as f32,
            0.0,
            0.0,
        ],
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
