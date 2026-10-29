//! Water (graphics Phase 29; `water.wgsl`, `docs/rendering/water.md`).
//!
//! Water is drawn first among the see-through items. From Medium
//! reflection quality, the scene pass stops after the solid items and the
//! sky, a full-screen pass copies the scene's light and each pixel's
//! distance into the [`SceneCopy`], and a second scene pass draws water
//! (seeing through itself by the copy) and then the other see-through
//! items and sprites. The copy is bound in the frame's ambient occlusion
//! slot (binding 7), which see-through surfaces never read, so the item
//! shader's sampled textures stay within wgpu's default limit. At High and
//! Ultra the water also reflects what's on the screen. At Low there's no
//! copy: water is blended over the scene, and the scene pass isn't split.

use std::borrow::Cow;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use super::gpu::{self, create_pipeline, target_bytes};
use super::lighting::linear_color;
use super::types::HDR_FORMAT;
use crate::{DrawList, EffectiveGraphics, ReflectionQuality, Water};

/// The copy's format: light in rgb, distance in a (filterable, so the
/// water shader reads it through the occlusion slot's sampler).
const COPY_FORMAT: wgpu::TextureFormat = HDR_FORMAT;

/// How water is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum WaterMode {
    /// No water in the list.
    None,
    /// Blended over the scene, with no copy of it (Low).
    Blended,
    /// Seeing through itself by the copy: bent, tinted by depth, foam.
    Refracted,
    /// And reflecting what's on the screen (High and Ultra).
    Reflected,
}

impl WaterMode {
    /// How `list`'s water is drawn with `graphics`, by its reflection quality.
    pub(super) fn of(list: &DrawList, graphics: &EffectiveGraphics) -> Self {
        if !list.items.iter().any(|item| item.material.water.is_some()) {
            return WaterMode::None;
        }
        match graphics.reflection_quality {
            ReflectionQuality::Low => WaterMode::Blended,
            ReflectionQuality::Medium => WaterMode::Refracted,
            ReflectionQuality::High | ReflectionQuality::Ultra => WaterMode::Reflected,
        }
    }

    /// The frame uniform's `reflection_probes.y` (reflect the screen) and
    /// `.z` (see through by the copy).
    pub(super) fn flags(self) -> [u32; 2] {
        [
            u32::from(self == WaterMode::Reflected),
            u32::from(self >= WaterMode::Refracted),
        ]
    }

    pub(super) fn copies(self) -> bool {
        self >= WaterMode::Refracted
    }
}

/// A water material's values as `water.wgsl`'s `water_look` reads them,
/// in the material's `params`: the deep color (linear) and clarity; the
/// waves' height, length, speed and direction (radians); the foam and
/// refraction, then 1, so a water material's key never matches a plain one.
pub(super) fn water_params(water: &Water) -> [[f32; 4]; 3] {
    let deep = linear_color(water.deep_color);
    let finite = |value: f32, otherwise: f32| if value.is_finite() { value } else { otherwise };
    [
        [deep.x, deep.y, deep.z, finite(water.clarity, 4.0)],
        [
            finite(water.wave_height, 0.0),
            finite(water.wave_length, 12.0),
            finite(water.wave_speed, 0.0),
            finite(water.wave_direction, 0.0).to_radians(),
        ],
        [
            finite(water.foam, 0.0),
            finite(water.refraction, 0.0),
            1.0,
            0.0,
        ],
    ]
}

/// What the copy pass needs to turn depth into distance. Must match
/// `CopyView` in water_copy.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct CopyView {
    pub(super) inv_view_proj: [[f32; 4]; 4],
    pub(super) camera_position: [f32; 4],
}

/// The copy of the solid scene, at the scene's size.
pub(super) struct SceneCopy {
    pub(super) size: (u32, u32),
    pub(super) view: wgpu::TextureView,
    pub(super) bytes: u64,
    /// A clone of [`super::TargetsAlive::water`].
    _alive: Arc<()>,
}

/// Water's pipelines, made for the scene's samples per pixel.
pub(super) struct WaterPipelines {
    /// Writes the water's whole color (and its depth), from the copy.
    pub(super) refracted: wgpu::RenderPipeline,
    /// Blends it over the scene, without the copy.
    pub(super) blended: wgpu::RenderPipeline,
}

pub(super) struct WaterPass {
    pub(super) pipelines: WaterPipelines,
    copy_layout: wgpu::BindGroupLayout,
    copy_pipeline: wgpu::RenderPipeline,
    copy_view: wgpu::Buffer,
    /// Only while water is drawn from Medium reflection quality.
    pub(super) copy: Option<SceneCopy>,
}

impl WaterPass {
    /// `layouts` are the scene's (frame, material, skin).
    pub(super) fn new(
        device: &wgpu::Device,
        layouts: [&wgpu::BindGroupLayout; 3],
        samples: u32,
    ) -> Self {
        let copy_layout = copy_layout(device, samples);
        Self {
            pipelines: water_pipelines(device, layouts, samples),
            copy_pipeline: copy_pipeline(device, &copy_layout, samples),
            copy_layout,
            copy_view: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("water copy view"),
                size: size_of::<CopyView>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            copy: None,
        }
    }

    /// Remakes the pipelines for a new number of samples per pixel.
    pub(super) fn set_samples(
        &mut self,
        device: &wgpu::Device,
        layouts: [&wgpu::BindGroupLayout; 3],
        samples: u32,
    ) {
        self.pipelines = water_pipelines(device, layouts, samples);
        self.copy_layout = copy_layout(device, samples);
        self.copy_pipeline = copy_pipeline(device, &self.copy_layout, samples);
    }

    /// Makes the copy at `size`, or frees it for `None`. True when it
    /// changed, so the frame bindings that hold it are remade.
    pub(super) fn fit(
        &mut self,
        device: &wgpu::Device,
        size: Option<(u32, u32)>,
        alive: &Arc<()>,
    ) -> bool {
        if self.copy.as_ref().map(|copy| copy.size) == size {
            return false;
        }
        self.copy = size.map(|(width, height)| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("water's copy of the scene"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COPY_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            SceneCopy {
                size: (width, height),
                view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                bytes: target_bytes((width, height, 1), COPY_FORMAT, 1),
                _alive: alive.clone(),
            }
        });
        true
    }

    /// Copies the solid scene (`hdr`, resolved, and `depth`, with the
    /// scene's samples) into the copy, if there is one.
    pub(super) fn copy_scene(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (hdr, depth): (&wgpu::TextureView, &wgpu::TextureView),
        view: CopyView,
    ) {
        let Some(copy) = &self.copy else {
            return;
        };
        queue.write_buffer(&self.copy_view, 0, bytemuck::bytes_of(&view));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water copy"),
            layout: &self.copy_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.copy_view.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water copy"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &copy.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.copy_pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn water_pipelines(
    device: &wgpu::Device,
    layouts: [&wgpu::BindGroupLayout; 3],
    samples: u32,
) -> WaterPipelines {
    let shader = gpu::shader_module(device, "water", gpu::WATER_SHADER);
    let pipeline = |transparent| {
        create_pipeline(
            device,
            &shader,
            ("vs_water", "fs_water"),
            &layouts,
            (HDR_FORMAT, samples),
            (transparent, false),
        )
    };
    WaterPipelines {
        refracted: pipeline(false),
        blended: pipeline(true),
    }
}

/// The copy pass's bindings: its view, the scene's light and its depth,
/// multisampled with MSAA.
fn copy_layout(device: &wgpu::Device, samples: u32) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("water copy"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: samples > 1,
                },
                count: None,
            },
        ],
    })
}

fn copy_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    // The same shader, reading a multisampled depth with MSAA (its
    // `textureLoad` then takes a sample where it took a mip level).
    let source = include_str!("../water_copy.wgsl");
    let source = if samples > 1 {
        Cow::Owned(source.replace(
            "var scene_depth: texture_depth_2d;",
            "var scene_depth: texture_depth_multisampled_2d;",
        ))
    } else {
        Cow::Borrowed(source)
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water copy"),
        source: wgpu::ShaderSource::Wgsl(source),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("water copy"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water copy"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_copy"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_copy"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: COPY_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_params_keep_their_values_and_never_match_a_plain_material() {
        let water = Water {
            deep_color: [1.0, 0.0, 0.0],
            wave_direction: 90.0,
            ..Water::default()
        };
        let params = water_params(&water);
        assert_eq!(params[0], [1.0, 0.0, 0.0, 4.0]);
        assert!((params[1][3] - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(params[2][2], 1.0);
        let broken = Water {
            clarity: f32::NAN,
            wave_length: f32::INFINITY,
            ..Water::default()
        };
        assert!(
            water_params(&broken)
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        );
    }
}
