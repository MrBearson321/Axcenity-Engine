//! Fog (graphics Phase 25): the frame's copy of the draw list's [`Fog`]
//! (`FogRaw`, `Fog` in fog.wgsl), which every scene shader and the sprites
//! fade into, and the graphics settings' volumetric fog (`VolumetricFog`,
//! volumetric_fog.wgsl): the sun's and lamps' light scattered by height fog
//! in a grid over the view, with their shadows. The grid and its passes
//! exist only while it's drawn.

use std::sync::Arc;

use glam::Vec3;

use super::lighting::linear_color;
use super::*;
use crate::draw_list::Fog;

/// The volumetric fog's cells across the screen, down it and along each
/// ray. Must match `GRID` in volumetric_fog.wgsl.
pub(super) const GRID: [u32; 3] = [160, 90, 64];
/// How far from the camera the volumetric fog reaches, in studs. Past it,
/// the sun's light in the fog is unshadowed, as below Ultra.
pub(super) const VOLUMETRIC_REACH: f32 = 200.0;
/// Further than anything is seen: distance fog that never starts.
const NEVER: f32 = 1.0e9;
const GRID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The fog as the shaders read it. Must match `Fog` in fog.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct FogRaw {
    pub(super) color: [f32; 4],
    pub(super) distance: [f32; 4],
    pub(super) height: [f32; 4],
    pub(super) sun: [f32; 4],
    pub(super) sun_direction: [f32; 4],
}

impl FogRaw {
    /// `fog` with the sky's color around the horizon (`horizon`, linear
    /// light) for fog that takes it, the sun as the frame has it, and the
    /// volumetric fog reaching `reach` studs (0 while it's off).
    pub(super) fn new(
        fog: &Fog,
        horizon: Vec3,
        sun_direction: Vec3,
        sun_radiance: Vec3,
        reach: f32,
    ) -> Self {
        if !fog.is_drawn() {
            return Self::default();
        }
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
        Self {
            color: color.extend(1.0).to_array(),
            distance: [start, end, scattering, 0.0],
            height: [density, fog.height, fog.falloff.max(0.001), reach],
            sun: (sun_radiance * scattering).extend(forward).to_array(),
            sun_direction: sun_direction.extend(0.0).to_array(),
        }
    }
}

/// Whether the volumetric fog is drawn for `fog`: the settings allow it,
/// and the world has height fog that scatters light.
pub(super) fn volumetric_drawn(fog: &Fog, graphics: &EffectiveGraphics) -> bool {
    graphics.volumetric_fog && fog.has_height_fog() && fog.sun_scattering > 0.0
}

/// The volumetric fog's grid, its passes, and the frame's bindings as its
/// compute shaders see them.
pub(super) struct VolumetricFog {
    /// The frame's bindings, visible to compute shaders too.
    pub(super) frame_layout: wgpu::BindGroupLayout,
    /// Remade with the frame's (`ScenePipeline::bind_frame`).
    pub(super) frame_group: Option<wgpu::BindGroup>,
    scatter_pipeline: wgpu::ComputePipeline,
    scatter_group: wgpu::BindGroup,
    gather_pipeline: wgpu::ComputePipeline,
    gather_group: wgpu::BindGroup,
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
        let scatter_view = grid("volumetric fog scatter").create_view(&Default::default());
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
        let scatter_layout = layout("volumetric fog scatter", &[storage(0)]);
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
        let scatter_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volumetric fog scatter"),
            layout: &scatter_layout,
            entries: &[view_entry(0, &scatter_view)],
        });
        let gather_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volumetric fog gather"),
            layout: &gather_layout,
            entries: &[view_entry(1, &scatter_view), view_entry(2, &gathered_view)],
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("volumetric fog"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let [width, height, depth] = GRID;
        Self {
            frame_layout: compute_frame_layout,
            frame_group: None,
            scatter_pipeline,
            scatter_group,
            gather_pipeline,
            gather_group,
            composite_pipeline,
            composite_layout,
            gathered_view,
            sampler,
            bytes: 2 * target_bytes((width, height, depth), GRID_FORMAT, 1),
            _alive: alive.clone(),
        }
    }

    /// Bytes of its grids.
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
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
        pass.set_bind_group(1, &self.scatter_group, &[]);
        let [width, height, depth] = GRID;
        pass.dispatch_workgroups(width.div_ceil(4), height.div_ceil(4), depth.div_ceil(4));
        pass.set_pipeline(&self.gather_pipeline);
        pass.set_bind_group(1, &self.gather_group, &[]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fog_is_drawn_by_default() {
        let raw = FogRaw::new(&Fog::default(), Vec3::ONE, Vec3::Y, Vec3::ONE, 0.0);
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
        let raw = FogRaw::new(&fog, Vec3::ZERO, Vec3::Y, Vec3::splat(3.0), 0.0);
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
        let raw = FogRaw::new(&fog, horizon, Vec3::Y, Vec3::splat(2.0), VOLUMETRIC_REACH);
        assert_eq!(raw.color, [0.2, 0.3, 0.4, 1.0]);
        assert!(raw.distance[0] >= NEVER, "no distance fog");
        assert_eq!(raw.height, [0.01, -5.0, 20.0, VOLUMETRIC_REACH]);
        assert_eq!(raw.sun, [1.0, 1.0, 1.0, 0.6]);
    }
}
