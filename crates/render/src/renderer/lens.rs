//! Lens effects (`DrawList::lens`): depth of field, vignette, film grain,
//! chromatic aberration and sharpening.
//!
//! Depth of field is its own pass (`depth_of_field.wgsl`). It blurs the
//! scene by the prepass's depth into a target that bloom and the composite
//! pass then read in the scene's place, and that target exists only while
//! it's drawn. The other four are worked out in the composite pass
//! (`post.wgsl`) from [`PostUniform`]'s lens values, so they need no target,
//! and while they're off the shader skips them.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::draw_list::{DrawList, LensEffects};
use crate::graphics::{DepthOfFieldQuality, EffectiveGraphics};

use super::lighting::camera_matrices;
use super::post::fullscreen_pass;
use super::{HDR_FORMAT, PostUniform};

/// Depth of field's widest blur, as a share of the scene's height (15
/// pixels across at 1080).
pub(super) const MAX_BLUR: f32 = 1.0 / 72.0;

/// What the depth of field pass reads. Must match `DepthOfField` in
/// depth_of_field.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct DepthOfFieldUniform {
    /// Turns clip space back to view space.
    inv_projection: [[f32; 4]; 4],
    /// Focus distance and in-focus radius in studs, then near and far
    /// intensity.
    focus: [f32; 4],
    /// The widest blur in the scene's pixels, samples per pixel, then unused.
    settings: [f32; 4],
}

/// The depth of field pass's pipeline and uniform, which don't depend on
/// the scene's size.
pub(super) struct DepthOfFieldPass {
    pub(super) layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
}

/// The blurred scene, at the scene's size, while depth of field is drawn,
/// and the pass's binding of what it reads.
pub(super) struct DepthOfFieldTarget {
    pub(super) view: wgpu::TextureView,
    group: wgpu::BindGroup,
    /// A clone of [`super::TargetsAlive::depth_of_field`].
    pub(super) _alive: Arc<()>,
}

impl DepthOfFieldPass {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth of field"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: fragment,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fragment,
                    ty: wgpu::BindingType::Texture {
                        // Read pixel by pixel, never filtered.
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: fragment,
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
            label: Some("depth of field"),
            size: size_of::<DepthOfFieldUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("../depth_of_field.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth of field"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth of field"),
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
                entry_point: Some("fs_depth_of_field"),
                compilation_options: Default::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            layout,
            pipeline,
            uniform,
        }
    }

    /// Writes what this frame's pass reads: `list`'s depth of field, seen as
    /// its camera at `aspect` sees it, over a scene `height` pixels tall.
    pub(super) fn prepare(
        &self,
        queue: &wgpu::Queue,
        list: &DrawList,
        aspect: f32,
        height: u32,
        quality: DepthOfFieldQuality,
    ) {
        let Some(blur) = &list.lens.depth_of_field else {
            return;
        };
        let (_, projection) = camera_matrices(&list.camera, aspect);
        let number = |value: f32| if value.is_finite() { value } else { 0.0 };
        let uniform = DepthOfFieldUniform {
            inv_projection: projection.inverse().to_cols_array_2d(),
            focus: [
                number(blur.focus_distance).max(0.0),
                number(blur.in_focus_radius).max(0.0),
                number(blur.near_intensity).clamp(0.0, 1.0),
                number(blur.far_intensity).clamp(0.0, 1.0),
            ],
            settings: [height as f32 * MAX_BLUR, quality.samples() as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    /// Blurs the scene into `target`.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &DepthOfFieldTarget) {
        fullscreen_pass(
            encoder,
            "depth of field",
            &target.view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            &self.pipeline,
            &[&target.group],
        );
    }
}

impl DepthOfFieldTarget {
    /// Blurs `scene` by `depth` (the prepass's) into `view`, holding a clone
    /// of `alive`.
    pub(super) fn new(
        device: &wgpu::Device,
        pass: &DepthOfFieldPass,
        (scene, depth): (&wgpu::TextureView, &wgpu::TextureView),
        view: wgpu::TextureView,
        alive: &Arc<()>,
    ) -> Self {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth of field"),
            layout: &pass.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pass.uniform.as_entire_binding(),
                },
            ],
        });
        Self {
            view,
            group,
            _alive: alive.clone(),
        }
    }
}

/// Whether `lens` has depth of field that blurs anything.
pub(super) fn wants_depth_of_field(lens: &LensEffects) -> bool {
    lens.depth_of_field.is_some_and(|blur| {
        let blurs = |intensity: f32| intensity.is_finite() && intensity > 0.0;
        blurs(blur.near_intensity) || blurs(blur.far_intensity)
    })
}

/// Fills in `uniform`'s lens values: `lens`'s vignette, film grain,
/// chromatic aberration and sharpening, each only if `graphics` allows it,
/// for an output `height` pixels tall at `aspect`, on frame `frame` (the
/// grain moves each frame).
pub(super) fn add_lens(
    uniform: &mut PostUniform,
    lens: &LensEffects,
    graphics: &EffectiveGraphics,
    (aspect, height): (f32, u32),
    frame: u64,
) {
    let amount = |value: f32| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let aberration = lens
        .chromatic_aberration
        .filter(|_| graphics.chromatic_aberration)
        .map_or(0.0, |intensity| {
            // 1 parts red and blue by 1% of the width at the side edges,
            // where the screen's coordinates are half a width from the middle.
            if intensity.is_finite() {
                intensity.max(0.0) * 0.02
            } else {
                0.0
            }
        });
    let sharpen = lens
        .sharpen
        .filter(|_| graphics.sharpen)
        .map_or(0.0, amount);
    let grain = lens.film_grain.filter(|_| graphics.film_grain);
    let (grain_intensity, grain_size) = grain.map_or((0.0, 1.0), |grain| {
        let size = if grain.size.is_finite() {
            grain.size.max(0.0)
        } else {
            1.0
        };
        (
            amount(grain.intensity),
            (size * height as f32 / 1080.0).max(1.0),
        )
    });
    uniform.lens = [aberration, sharpen, grain_intensity, grain_size];
    let vignette = lens.vignette.filter(|_| graphics.vignette);
    let (intensity, size, color) = vignette.map_or((0.0, 0.0, [0.0; 3]), |vignette| {
        (
            amount(vignette.intensity),
            amount(vignette.size).min(0.99),
            vignette.color.map(amount),
        )
    });
    // The grain's pattern changes each frame, without growing too big for
    // an f32 to hold exactly.
    let seed = (frame % 4096) as f32;
    uniform.vignette = [intensity, size, aspect, seed];
    uniform.vignette_color = [color[0], color[1], color[2], 0.0];
}
