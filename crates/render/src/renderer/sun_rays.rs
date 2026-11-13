//! Sun rays (`LensEffects::sun_rays`, a world's `SunRaysEffect`): shafts of
//! light streaming from the sun through gaps in whatever is in front of it,
//! such as sky between leaves or a window.
//!
//! A pass of its own after the scene (and volumetric fog's light), before
//! TAA and bloom (`sun_rays.wgsl`). Each pixel looks along its way toward
//! the sun's place on screen and counts the sky it passes in the prepass's
//! depth (nothing drawn there, so a see-through window counts as open),
//! near the sun, nearer the pixel counting more; the sun's light times that
//! is added to the scene. A gap in a canopy so becomes a beam, and the
//! leaves around it cast dark streaks through the glow. It needs no target
//! of its own, only the prepass, which it keeps alive while drawn
//! ([`wanted`]). It shows only while the sun is in or near the view:
//! [`uniform`] fades it out as the sun leaves the screen, sets below the
//! horizon or goes behind the camera.

use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::draw_list::DrawList;
use crate::graphics::EffectiveGraphics;

use super::HDR_FORMAT;
use super::gpu::{SUN_RAYS_SHADER, shader_module};
use super::lighting::view_projection;

/// How far past the screen's edge the sun can be, in halves of the
/// screen, before its rays are gone (they fade out on the way).
const OFF_SCREEN_FADE: f32 = 0.6;
/// How strong rays are at a SunRaysEffect's `Intensity` 1: the share of
/// the sun's light a pixel gets with open sky all the way to the sun.
const MOST_STRENGTH: f32 = 1.5;

/// What the pass reads. Must match `SunRays` in sun_rays.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct SunRaysUniform {
    /// The rays' strength, how far beams reach (in screen heights), and
    /// the sun's place on screen (0 to 1 from the top left; it may be off
    /// the screen).
    pub(super) rays: [f32; 4],
    /// The sun's light (its color times its intensity), and the screen's
    /// aspect (width over height).
    pub(super) light: [f32; 4],
    /// x: a number that changes each frame under TAA (0 without), which
    /// turns each pixel's noise so TAA averages it.
    pub(super) noise: [f32; 4],
}

/// Whether `list` has sun rays the graphics settings draw (wherever the
/// sun is): while it does, the prepass is kept for them.
pub(super) fn wanted(list: &DrawList, graphics: &EffectiveGraphics) -> bool {
    graphics.sun_rays
        && list
            .lens
            .sun_rays
            .is_some_and(|rays| rays.intensity.is_finite() && rays.intensity > 0.0)
}

/// The pass's values this frame, or `None` while nothing would show: no
/// effect, the setting off, or the sun behind the camera, below the horizon
/// or too far off the screen.
pub(super) fn uniform(
    list: &DrawList,
    graphics: &EffectiveGraphics,
    aspect: f32,
    turn: f32,
) -> Option<SunRaysUniform> {
    if !wanted(list, graphics) {
        return None;
    }
    let rays = list.lens.sun_rays?;
    let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
    let intensity = finite(rays.intensity).clamp(0.0, 1.0);
    let spread = finite(rays.spread).clamp(0.0, 1.0);
    let toward_sun = list.sun.direction.try_normalize()?;
    let ((u, v), on_screen) = sun_on_screen(list, toward_sun, aspect)?;
    // Gone once the sun is below the horizon, fading in just above it.
    let above_horizon = smoothstep(-0.02, 0.06, toward_sun.y);
    let strength = intensity * MOST_STRENGTH * on_screen * above_horizon;
    let sun = finite(list.sun.intensity).max(0.0);
    let light = list.sun.color.map(|channel| finite(channel).max(0.0) * sun);
    if strength <= 0.0 || light.iter().all(|&channel| channel <= 0.0) {
        return None;
    }
    let reach = 0.15 + 0.85 * spread;
    Some(SunRaysUniform {
        rays: [strength, reach, u, v],
        light: [light[0], light[1], light[2], aspect],
        noise: [turn, 0.0, 0.0, 0.0],
    })
}

/// The sun's place on screen (0 to 1 from the top left, maybe past the
/// edges) and how much of its rays show there (1 on screen, fading to 0
/// off it), or `None` while it's behind the camera or too far off.
fn sun_on_screen(list: &DrawList, toward_sun: Vec3, aspect: f32) -> Option<((f32, f32), f32)> {
    // A direction (w 0) is where something infinitely far that way shows.
    let clip = view_projection(list, aspect) * Vec4::from((toward_sun, 0.0));
    if clip.w <= 1e-4 {
        return None;
    }
    let (x, y) = (clip.x / clip.w, clip.y / clip.w);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let past_edge = (x.abs().max(y.abs()) - 1.0).max(0.0);
    let on_screen = 1.0 - smoothstep(0.0, OFF_SCREEN_FADE, past_edge);
    if on_screen <= 0.0 {
        return None;
    }
    Some(((x * 0.5 + 0.5, 0.5 - y * 0.5), on_screen))
}

fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The pipeline that adds sun rays to the scene.
pub(super) struct SunRaysPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl SunRaysPass {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun rays"),
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
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let shader = shader_module(device, "sun rays", SUN_RAYS_SHADER);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sun rays"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sun rays"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sun_rays"),
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
        Self { pipeline, layout }
    }

    /// Adds the rays to the scene in `hdr`, telling sky from solid by the
    /// prepass's `depth`.
    pub(super) fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &SunRaysUniform,
        hdr: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sun rays"),
            contents: bytemuck::bytes_of(uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun rays"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sun rays"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
