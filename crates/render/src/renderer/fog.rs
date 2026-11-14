//! Fog (graphics Phase 25): the frame's copy of the draw list's [`Fog`]
//! (`FogRaw`, `Fog` in fog.wgsl), which every scene shader and the sprites
//! fade into, and the graphics settings' volumetric fog (`VolumetricFog`,
//! volumetric_fog.wgsl): the sun's and lamps' light scattered by height fog
//! in a grid over the view, with their shadows. The grid and its passes
//! exist only while it's drawn.

use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::lighting::linear_color;
use super::*;
use crate::draw_list::{Dust, Fog, MAX_DUST_BOXES};

/// The volumetric fog's cells across the screen, down it and along each
/// ray. Must match `GRID` in volumetric_fog.wgsl.
pub(super) const GRID: [u32; 3] = [160, 90, 64];
/// How far from the camera the volumetric fog reaches, in studs. Past it,
/// the sun's light in the fog is unshadowed, as below Ultra.
pub(super) const VOLUMETRIC_REACH: f32 = 200.0;
/// Further than anything is seen: distance fog that never starts.
const NEVER: f32 = 1.0e9;
const GRID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// How much of last frame's light each cell keeps: about the last 10
/// frames are averaged, each looked at through different points of the
/// cell (`cell_shift` in volumetric_fog.wgsl).
pub(super) const HISTORY_KEEP: f32 = 0.9;
/// A camera moving further than this in a frame (in studs), or turning
/// more than [`CUT_TURN`], has cut to another view: the grid starts again
/// rather than blend in light from where it was.
const CUT_DISTANCE: f32 = 25.0;
/// The cosine of the widest turn in a frame that isn't a cut (about 40°).
const CUT_TURN: f32 = 0.75;
/// Turns run 1 to this, then start again (turn 0 shifts nothing).
const TURNS: u32 = 1024;

/// Last frame's camera, for the grid's history. Must match `FogHistory` in
/// volumetric_fog.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct FogHistoryRaw {
    view_proj: [[f32; 4]; 4],
    /// xyz: last frame's camera, w: how much of last frame's light each
    /// cell keeps (0: none).
    camera: [f32; 4],
}

/// What the grid's history needs of a frame's camera: world to clip
/// space (`Frame::view_proj`) and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FogCamera {
    pub(super) view_proj: Mat4,
    pub(super) position: Vec3,
}

impl FogCamera {
    /// Which way it looks.
    fn forward(&self) -> Vec3 {
        // The clip-space w row is the view's depth axis.
        let row = self.view_proj.row(3);
        row.truncate().normalize_or(Vec3::NEG_Z)
    }

    /// Whether going from `self` to `next` is a cut to another view.
    fn cuts_to(&self, next: &FogCamera) -> bool {
        self.position.distance(next.position) > CUT_DISTANCE
            || self.forward().dot(next.forward()) < CUT_TURN
    }
}

/// The fog as the shaders read it. Must match `Fog` in fog.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct FogRaw {
    pub(super) color: [f32; 4],
    /// x, y: distance fog's start and end; z: the lamps' share scattered;
    /// w: 1 while the volumetric fog scatters height fog's own light
    /// within its reach (the surfaces leave it out there).
    pub(super) distance: [f32; 4],
    pub(super) height: [f32; 4],
    pub(super) sun: [f32; 4],
    pub(super) sun_direction: [f32; 4],
    /// x: how many of `dust` are drawn, y: where the ray traced air (LE3)
    /// starts in the ray traced shadow mask, in u32s (0: not traced; the
    /// scene sets it, `RayShadows::air_words`).
    pub(super) dust_count: [u32; 4],
    /// The boxes of dust (`Fog::dust`), in the order they apply.
    pub(super) dust: [DustRaw; MAX_DUST_BOXES],
}

/// One box of dust. Must match `DustBox` in fog.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct DustRaw {
    /// x the light it takes away per stud (0 for none), y how much of its
    /// light goes forward, z the studs over which it thins at the box's
    /// faces, w the studs above its bottom over which it thins by e (0:
    /// even).
    pub(super) params: [f32; 4],
    /// How much of the light it takes away it scatters (linear), and w: 1
    /// for an interior, which clears the air outside it.
    pub(super) color: [f32; 4],
    /// The sun's light it scatters (the sun's times its color).
    pub(super) sun: [f32; 4],
    /// The sky's light it scatters with nothing in the way (the sky's
    /// average times its color), past the volumetric fog's reach.
    pub(super) ambient: [f32; 4],
    /// The rows of the world to the box's space, and half its size.
    pub(super) rows: [[f32; 4]; 3],
    pub(super) extent: [f32; 4],
}

impl DustRaw {
    fn new(dust: &Dust, sky: Vec3, sun_radiance: Vec3) -> Self {
        let color = linear_color(dust.color).max(Vec3::ZERO);
        let finite_or = |value: f32, otherwise: f32| {
            if value.is_finite() { value } else { otherwise }
        };
        let forward = finite_or(dust.forward_scattering, 0.0).clamp(-0.95, 0.95);
        let edge = finite_or(dust.edge, 0.0).max(0.0);
        let falloff = finite_or(dust.falloff, 0.0).max(0.0);
        let rows = dust.to_box.transpose();
        Self {
            params: [dust.density.max(0.0), forward, edge, falloff],
            color: color.extend(f32::from(u8::from(dust.interior))).to_array(),
            sun: (sun_radiance * color).extend(0.0).to_array(),
            ambient: (sky.max(Vec3::ZERO) * color).extend(0.0).to_array(),
            rows: [
                rows.x_axis.to_array(),
                rows.y_axis.to_array(),
                rows.z_axis.to_array(),
            ],
            extent: dust.half_size.extend(0.0).to_array(),
        }
    }
}

impl FogRaw {
    /// `fog` with the sky's color around the horizon (`horizon`, linear
    /// light) for fog that takes it, the sky's average light (`sky`) for
    /// dust, the sun as the frame has it, and the volumetric fog reaching
    /// `reach` studs (0 while it's off).
    pub(super) fn new(
        fog: &Fog,
        horizon: Vec3,
        sky: Vec3,
        sun_direction: Vec3,
        sun_radiance: Vec3,
        reach: f32,
    ) -> Self {
        if !fog.is_drawn() {
            return Self::default();
        }
        let mut raw = Self::without_dust(fog, horizon, sun_direction, sun_radiance, reach);
        let mut count = 0;
        for dust in fog.dust.iter().filter(|dust| dust.is_drawn()) {
            raw.dust[count] = DustRaw::new(dust, sky, sun_radiance);
            count += 1;
        }
        raw.dust_count = [count as u32, 0, 0, 0];
        raw
    }

    fn without_dust(
        fog: &Fog,
        horizon: Vec3,
        sun_direction: Vec3,
        sun_radiance: Vec3,
        reach: f32,
    ) -> Self {
        let color = fog.color.map_or(horizon, linear_color);
        let (start, end) = if fog.has_distance_fog() {
            (fog.start.max(0.0), fog.end.max(fog.start.max(0.0)))
        } else {
            (NEVER, NEVER)
        };
        let scattering = if fog.has_height_fog() && fog.sun_scattering.is_finite() {
            fog.sun_scattering.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let density = if fog.has_height_fog() {
            fog.density
        } else {
            0.0
        };
        let forward = if fog.forward_scattering.is_finite() {
            fog.forward_scattering.clamp(-0.95, 0.95)
        } else {
            0.0
        };
        // Within the volumetric fog's reach, the grid scatters height fog's
        // own light too, by how open the air is to the sky there (so the
        // air of a closed room doesn't glow like the open air outside);
        // distance fog keeps its flat color.
        let own_in_grid = reach > 0.0 && density > 0.0 && start >= NEVER;
        Self {
            color: color.extend(1.0).to_array(),
            distance: [start, end, scattering, f32::from(u8::from(own_in_grid))],
            height: [density, fog.height, fog.falloff.max(0.001), reach],
            sun: (sun_radiance * scattering).extend(forward).to_array(),
            sun_direction: sun_direction.extend(0.0).to_array(),
            ..Self::default()
        }
    }
}

/// Whether the volumetric fog is drawn for `fog`: the settings allow it,
/// and the world has height fog that scatters light, or dust.
pub(super) fn volumetric_drawn(fog: &Fog, graphics: &EffectiveGraphics) -> bool {
    graphics.volumetric_fog
        && ((fog.has_height_fog() && fog.sun_scattering > 0.0) || fog.has_dust())
}

/// The volumetric fog's grid, its passes, and the frame's bindings as its
/// compute shaders see them.
pub(super) struct VolumetricFog {
    /// The frame's bindings, visible to compute shaders too.
    pub(super) frame_layout: wgpu::BindGroupLayout,
    /// Remade with the frame's (`ScenePipeline::bind_frame`).
    pub(super) frame_group: Option<wgpu::BindGroup>,
    scatter_pipeline: wgpu::ComputePipeline,
    /// The scatter grids take turns: each frame one is written while the
    /// other, last frame's, is its history. By which is written.
    scatter_groups: [wgpu::BindGroup; 2],
    gather_pipeline: wgpu::ComputePipeline,
    gather_groups: [wgpu::BindGroup; 2],
    /// Which scatter grid this frame writes.
    current: usize,
    history_buffer: wgpu::Buffer,
    /// Last frame's camera, if the grid has a history.
    last: Option<FogCamera>,
    /// Frames drawn, for the turn.
    frames: u32,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    gathered_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bytes: u64,
    _alive: Arc<()>,
}

impl VolumetricFog {
    /// `frame_entries` are the frame's bindings and `frame_layout` theirs
    /// as the scene pass sees them. `alive` counts sets of these.
    pub(super) fn new(
        device: &wgpu::Device,
        frame_entries: &[wgpu::BindGroupLayoutEntry],
        frame_layout: &wgpu::BindGroupLayout,
        alive: &Arc<()>,
    ) -> Self {
        let compute_entries: Vec<_> = frame_entries
            .iter()
            .map(|entry| wgpu::BindGroupLayoutEntry {
                visibility: entry.visibility | wgpu::ShaderStages::COMPUTE,
                ..*entry
            })
            .collect();
        let compute_frame_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("frame (volumetric fog)"),
                entries: &compute_entries,
            });
        let grid = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: GRID[0],
                    height: GRID[1],
                    depth_or_array_layers: GRID[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: GRID_FORMAT,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let scatter_views = [
            grid("volumetric fog scatter").create_view(&Default::default()),
            grid("volumetric fog scatter").create_view(&Default::default()),
        ];
        let gathered_view = grid("volumetric fog gathered").create_view(&Default::default());
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let storage = |binding| {
            entry(
                binding,
                wgpu::ShaderStages::COMPUTE,
                wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: GRID_FORMAT,
                    view_dimension: wgpu::TextureViewDimension::D3,
                },
            )
        };
        let sampled = |binding, visibility, filterable| {
            entry(
                binding,
                visibility,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
            )
        };
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let scatter_layout = layout(
            "volumetric fog scatter",
            &[
                storage(0),
                sampled(6, wgpu::ShaderStages::COMPUTE, true),
                entry(
                    7,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    8,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        );
        let gather_layout = layout(
            "volumetric fog gather",
            &[sampled(1, wgpu::ShaderStages::COMPUTE, false), storage(2)],
        );
        let composite_layout = layout(
            "volumetric fog composite",
            &[
                sampled(3, wgpu::ShaderStages::FRAGMENT, true),
                entry(
                    4,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    5,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        );
        let view_entry = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("volumetric fog"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let history_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volumetric fog history"),
            size: std::mem::size_of::<FogHistoryRaw>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scatter_groups = [0, 1].map(|written| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("volumetric fog scatter"),
                layout: &scatter_layout,
                entries: &[
                    view_entry(0, &scatter_views[written]),
                    view_entry(6, &scatter_views[1 - written]),
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: history_buffer.as_entire_binding(),
                    },
                ],
            })
        });
        let gather_groups = [0, 1].map(|written| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("volumetric fog gather"),
                layout: &gather_layout,
                entries: &[
                    view_entry(1, &scatter_views[written]),
                    view_entry(2, &gathered_view),
                ],
            })
        });
        let shader = shader_module(device, "volumetric fog", gpu::VOLUMETRIC_FOG_SHADER);
        let compute = |label, group: &wgpu::BindGroupLayout, entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &[Some(&compute_frame_layout), Some(group)],
                        immediate_size: 0,
                    }),
                ),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let scatter_pipeline = compute("volumetric fog scatter", &scatter_layout, "cs_scatter");
        let gather_pipeline = compute("volumetric fog gather", &gather_layout, "cs_gather");
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("volumetric fog composite"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("volumetric fog composite"),
                    bind_group_layouts: &[Some(frame_layout), Some(&composite_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_composite"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    // Added to the scene's light.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let [width, height, depth] = GRID;
        Self {
            frame_layout: compute_frame_layout,
            frame_group: None,
            scatter_pipeline,
            scatter_groups,
            gather_pipeline,
            gather_groups,
            current: 0,
            history_buffer,
            last: None,
            frames: 0,
            composite_pipeline,
            composite_layout,
            gathered_view,
            sampler,
            bytes: 3 * target_bytes((width, height, depth), GRID_FORMAT, 1),
            _alive: alive.clone(),
        }
    }

    /// Bytes of its grids.
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Starts a frame seen by `camera`: picks the scatter grid to write,
    /// and tells the history where last frame's camera was. Returns the
    /// frame's turn for `Fog.dust_count.z` (0, shifting nothing, on the
    /// grid's first frame or after a cut, when there's no history to
    /// average the shifted points with).
    pub(super) fn begin_frame(&mut self, queue: &wgpu::Queue, camera: FogCamera) -> u32 {
        let (raw, turn) = history_raw(self.last.as_ref(), &camera, self.frames);
        queue.write_buffer(&self.history_buffer, 0, bytemuck::bytes_of(&raw));
        self.current = 1 - self.current;
        self.last = Some(camera);
        self.frames = self.frames.wrapping_add(1);
        turn
    }

    /// Starts the grid again from the next frame, keeping nothing of
    /// what it had (new graphics settings light it differently).
    pub(super) fn forget_history(&mut self) {
        self.last = None;
    }

    /// Works out the grid's light for this frame: each cell's, then each
    /// ray's gathered front to back. After the shadow maps are drawn.
    pub(super) fn scatter(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(frame_group) = &self.frame_group else {
            return;
        };
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("volumetric fog"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, frame_group, &[]);
        pass.set_pipeline(&self.scatter_pipeline);
        pass.set_bind_group(1, &self.scatter_groups[self.current], &[]);
        let [width, height, depth] = GRID;
        pass.dispatch_workgroups(width.div_ceil(4), height.div_ceil(4), depth.div_ceil(4));
        pass.set_pipeline(&self.gather_pipeline);
        pass.set_bind_group(1, &self.gather_groups[self.current], &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }

    /// Adds the gathered light in front of each pixel's solid surface
    /// (`depth`, the prepass's) to the scene in `hdr`, after the scene pass.
    pub(super) fn composite(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame_group: &wgpu::BindGroup,
        hdr: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volumetric fog composite"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.gathered_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("volumetric fog composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: hdr,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, frame_group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The history's uniform and the turn for a frame seen by `camera`, after
/// `last` (`frames` drawn before it).
fn history_raw(last: Option<&FogCamera>, camera: &FogCamera, frames: u32) -> (FogHistoryRaw, u32) {
    match last.filter(|last| !last.cuts_to(camera)) {
        Some(last) => (
            FogHistoryRaw {
                view_proj: last.view_proj.to_cols_array_2d(),
                camera: last.position.extend(HISTORY_KEEP).to_array(),
            },
            frames % TURNS + 1,
        ),
        None => (FogHistoryRaw::default(), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(position: Vec3, toward: Vec3) -> FogCamera {
        use glam::camera::rh;
        let view = rh::view::look_at_mat4(position, toward, Vec3::Y);
        let proj = rh::proj::directx::perspective(70f32.to_radians(), 16.0 / 9.0, 0.1, 1000.0);
        FogCamera {
            view_proj: proj * view,
            position,
        }
    }

    #[test]
    fn the_first_frame_has_no_history_and_shifts_nothing() {
        let first = camera(Vec3::ZERO, Vec3::NEG_Z);
        let (raw, turn) = history_raw(None, &first, 0);
        assert_eq!((raw, turn), (FogHistoryRaw::default(), 0));
    }

    #[test]
    fn later_frames_keep_last_frames_camera_and_turn() {
        let last = camera(Vec3::ZERO, Vec3::NEG_Z);
        let now = camera(Vec3::new(0.5, 0.0, -0.5), Vec3::new(0.6, 0.0, -10.0));
        let (raw, turn) = history_raw(Some(&last), &now, 7);
        assert_eq!(raw.camera, [0.0, 0.0, 0.0, HISTORY_KEEP]);
        assert_eq!(raw.view_proj, last.view_proj.to_cols_array_2d());
        assert_eq!(turn, 8);
        // Turns never come back to 0, which shifts nothing.
        assert_eq!(history_raw(Some(&last), &now, TURNS - 1).1, TURNS);
        assert_eq!(history_raw(Some(&last), &now, TURNS).1, 1);
    }

    #[test]
    fn a_cut_to_another_view_starts_the_grid_again() {
        let last = camera(Vec3::ZERO, Vec3::NEG_Z);
        let far = camera(Vec3::new(100.0, 0.0, 0.0), Vec3::new(100.0, 0.0, -10.0));
        assert_eq!(history_raw(Some(&last), &far, 3).1, 0);
        let turned = camera(Vec3::ZERO, Vec3::X);
        assert_eq!(history_raw(Some(&last), &turned, 3).1, 0);
        assert!(!last.cuts_to(&camera(Vec3::ZERO, Vec3::new(1.0, 0.0, -10.0))));
    }

    #[test]
    fn no_fog_is_drawn_by_default() {
        let raw = FogRaw::new(
            &Fog::default(),
            Vec3::ONE,
            Vec3::ONE,
            Vec3::Y,
            Vec3::ONE,
            0.0,
        );
        assert_eq!(raw, FogRaw::default());
        assert_eq!(raw.color[3], 0.0, "the shaders leave the color as it is");
    }

    #[test]
    fn distance_fog_keeps_its_color_and_range_and_scatters_no_sun() {
        let fog = Fog {
            color: Some([1.0, 0.0, 0.0]),
            start: 10.0,
            end: 50.0,
            ..Fog::default()
        };
        let raw = FogRaw::new(&fog, Vec3::ZERO, Vec3::ONE, Vec3::Y, Vec3::splat(3.0), 0.0);
        assert_eq!(raw.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(raw.distance[..2], [10.0, 50.0]);
        assert_eq!(raw.height[0], 0.0, "no height fog");
        assert_eq!(raw.sun[..3], [0.0; 3]);
    }

    #[test]
    fn height_fog_takes_the_horizon_color_and_scatters_its_share_of_the_sun() {
        let fog = Fog {
            color: None,
            density: 0.01,
            height: -5.0,
            falloff: 20.0,
            sun_scattering: 0.5,
            forward_scattering: 0.6,
            ..Fog::default()
        };
        let horizon = Vec3::new(0.2, 0.3, 0.4);
        let raw = FogRaw::new(
            &fog,
            horizon,
            Vec3::ONE,
            Vec3::Y,
            Vec3::splat(2.0),
            VOLUMETRIC_REACH,
        );
        assert_eq!(raw.color, [0.2, 0.3, 0.4, 1.0]);
        assert!(raw.distance[0] >= NEVER, "no distance fog");
        assert_eq!(raw.height, [0.01, -5.0, 20.0, VOLUMETRIC_REACH]);
        assert_eq!(raw.sun, [1.0, 1.0, 1.0, 0.6]);
    }
    #[test]
    fn height_fogs_own_light_moves_into_the_volumetric_grid_but_distance_fogs_does_not() {
        let height = Fog {
            color: None,
            density: 0.01,
            sun_scattering: 0.8,
            ..Fog::default()
        };
        let sky = Vec3::splat(0.3);
        let on = FogRaw::new(&height, sky, sky, Vec3::Y, Vec3::ONE, VOLUMETRIC_REACH);
        assert_eq!(
            on.distance[3], 1.0,
            "the grid scatters it by how open the air is"
        );
        let off = FogRaw::new(&height, sky, sky, Vec3::Y, Vec3::ONE, 0.0);
        assert_eq!(
            off.distance[3], 0.0,
            "without the grid, the surfaces keep it"
        );
        let distance = Fog {
            start: 10.0,
            end: 50.0,
            ..height
        };
        let both = FogRaw::new(&distance, sky, sky, Vec3::Y, Vec3::ONE, VOLUMETRIC_REACH);
        assert_eq!(
            both.distance[3], 0.0,
            "distance fog's flat color stays as it was"
        );
    }

    #[test]
    fn dust_packs_its_box_color_and_the_suns_light_it_scatters() {
        let middle = Vec3::new(5.0, 2.0, -10.0);
        let fog = Fog {
            dust: crate::DustBoxes::one(Dust {
                density: 0.03,
                color: [1.0, 1.0, 1.0],
                forward_scattering: 0.7,
                to_box: glam::Mat4::from_translation(-middle),
                half_size: Vec3::new(4.0, 3.0, 20.0),
                edge: 0.5,
                falloff: 6.0,
                interior: false,
            }),
            ..Fog::default()
        };
        assert!(fog.is_drawn() && fog.has_dust());
        let raw = FogRaw::new(
            &fog,
            Vec3::ZERO,
            Vec3::splat(0.2),
            Vec3::Y,
            Vec3::splat(3.0),
            0.0,
        );
        assert_eq!(raw.color[3], 1.0, "dust alone is fog to draw");
        assert_eq!(raw.dust_count[0], 1);
        let dust = raw.dust[0];
        assert_eq!(dust.params, [0.03, 0.7, 0.5, 6.0]);
        assert_eq!(dust.color[3], 0.0, "not an interior");
        assert_eq!(dust.sun, [3.0, 3.0, 3.0, 0.0]);
        assert_eq!(dust.ambient, [0.2, 0.2, 0.2, 0.0]);
        // The box's rows take its middle to 0.
        let row = |r: [f32; 4]| r[0] * middle.x + r[1] * middle.y + r[2] * middle.z + r[3];
        assert_eq!(dust.rows.map(row), [0.0; 3]);
        assert_eq!(dust.extent, [4.0, 3.0, 20.0, 0.0]);
        // Without a box worth drawing, nothing.
        let empty = Fog {
            dust: crate::DustBoxes::one(Dust {
                density: 0.0,
                ..*fog.dust.first().unwrap()
            }),
            ..Fog::default()
        };
        assert!(!empty.is_drawn());
        // An interior is drawn with no dust of its own: it clears the air.
        let interior = Fog {
            dust: crate::DustBoxes::one(Dust {
                density: 0.0,
                interior: true,
                ..*fog.dust.first().unwrap()
            }),
            ..Fog::default()
        };
        assert!(interior.is_drawn());
        let raw = FogRaw::new(&interior, Vec3::ZERO, Vec3::ZERO, Vec3::Y, Vec3::ONE, 0.0);
        assert_eq!((raw.dust_count[0], raw.dust[0].color[3]), (1, 1.0));
    }

    #[test]
    fn many_boxes_pack_in_order_up_to_the_most() {
        let boxes: crate::DustBoxes = (0..12)
            .map(|i| Dust {
                density: 0.01 * (i + 1) as f32,
                ..Dust::default()
            })
            .collect();
        assert_eq!(boxes.len(), MAX_DUST_BOXES);
        let fog = Fog {
            dust: boxes,
            ..Fog::default()
        };
        let raw = FogRaw::new(&fog, Vec3::ZERO, Vec3::ZERO, Vec3::Y, Vec3::ONE, 0.0);
        assert_eq!(raw.dust_count[0], MAX_DUST_BOXES as u32);
        assert_eq!(raw.dust[2].params[0], 0.03);
    }
}
