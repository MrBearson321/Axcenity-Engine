//! Projected decals (`DrawList::decals`, `decal.wgsl`): boxes that paint a
//! picture onto the solid surfaces inside them, drawn in the scene pass
//! after the sky and before see-through items.
//!
//! They read the depth and normal prepass, which stays alive while the
//! list has decals and the graphics settings draw them
//! (`EffectiveGraphics::decals`). Those settings pick the nearest few within
//! a distance; the rest are left out, and the farthest fade out over the
//! last tenth of it. The ones kept are drawn in the list's order, so a newer
//! decal paints over an older one, one instanced draw per run of decals
//! sharing a picture.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use super::culling::Frustum;
use super::{DEPTH_FORMAT, HDR_FORMAT, linear_color};
use crate::draw_list::TextureSource;
use crate::graphics::DecalLimits;
use crate::{Decal, DrawList};

/// One decal for the GPU. Must match `DecalIn` in decal.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DecalRaw {
    model: [[f32; 4]; 4],
    inverse: [[f32; 4]; 4],
    /// Linear color, then opacity.
    color: [f32; 4],
    /// The cosines of the turn at which the picture is gone and at which it
    /// starts fading, then 1 to leave characters bare, then unused.
    params: [f32; 4],
}

/// A run of decals with the same picture (`None` is plain white).
#[derive(Debug, PartialEq)]
pub(super) struct DecalDraw {
    texture: Option<u64>,
    instances: Range<u32>,
}

pub(super) struct DecalPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    normals_layout: wgpu::BindGroupLayout,
    /// The prepass's normals, bound afresh each frame decals are drawn.
    normals_group: Option<wgpu::BindGroup>,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// For decals without a picture.
    white_group: wgpu::BindGroup,
    /// One bind group per picture, by its key.
    groups: HashMap<u64, wgpu::BindGroup>,
    buffer: wgpu::Buffer,
    capacity: usize,
}

impl DecalPass {
    /// `frame_layout` is the scene's frame bindings; `samples` per pixel
    /// must match the scene's targets.
    pub(super) fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        white: &wgpu::TextureView,
        samples: u32,
    ) -> Self {
        let normals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal normals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal picture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("decal pictures"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        });
        let white_group = texture_group(device, &texture_layout, white, &sampler);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("decals"),
            bind_group_layouts: &[
                Some(frame_layout),
                Some(&normals_layout),
                Some(&texture_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = decal_pipeline(device, &layout, samples);
        let capacity = 64;
        Self {
            pipeline,
            layout,
            normals_layout,
            normals_group: None,
            texture_layout,
            sampler,
            white_group,
            groups: HashMap::new(),
            buffer: create_buffer(device, capacity),
            capacity,
        }
    }

    /// Remakes the pipeline for drawing with `samples` per pixel.
    pub(super) fn set_samples(&mut self, device: &wgpu::Device, samples: u32) {
        self.pipeline = decal_pipeline(device, &self.layout, samples);
    }

    /// Uploads the decals of `list` that `limits` keep and `view` can see,
    /// and returns the runs to draw and how many were left out. Nothing is
    /// drawn without the prepass's `normals`. `view_of` finds an uploaded
    /// picture by its key; a decal whose picture couldn't be uploaded is
    /// drawn as a plain square.
    pub(super) fn prepare<'a>(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        list: &DrawList,
        (limits, view): (DecalLimits, &Frustum),
        normals: Option<&wgpu::TextureView>,
        view_of: impl Fn(u64) -> Option<&'a wgpu::TextureView>,
    ) -> (Vec<DecalDraw>, u32) {
        let Some(normals) = normals.filter(|_| !list.decals.is_empty()) else {
            self.normals_group = None;
            return (Vec::new(), list.decals.len() as u32);
        };
        let kept = kept_decals(&list.decals, limits, list.camera.position, view);
        let left_out = (list.decals.len() - kept.len()) as u32;
        if kept.is_empty() {
            self.normals_group = None;
            return (Vec::new(), left_out);
        }
        self.normals_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("decal normals"),
            layout: &self.normals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(normals),
            }],
        }));
        let mut raws = Vec::with_capacity(kept.len());
        let mut draws: Vec<DecalDraw> = Vec::new();
        for (index, fade) in kept {
            let decal = &list.decals[index];
            let texture = self.uploaded(device, &decal.texture, &view_of);
            let at = raws.len() as u32;
            raws.push(decal_raw(decal, fade));
            match draws.last_mut() {
                Some(last) if last.texture == texture => last.instances.end = at + 1,
                _ => draws.push(DecalDraw {
                    texture,
                    instances: at..at + 1,
                }),
            }
        }
        if raws.len() > self.capacity {
            self.capacity = raws.len().next_power_of_two();
            self.buffer = create_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&raws));
        (draws, left_out)
    }

    /// The key of `texture` once it's uploaded, with its bind group made.
    fn uploaded<'a>(
        &mut self,
        device: &wgpu::Device,
        texture: &Option<Arc<dyn TextureSource>>,
        view_of: &impl Fn(u64) -> Option<&'a wgpu::TextureView>,
    ) -> Option<u64> {
        let key = texture.as_ref()?.key();
        let view = view_of(key)?;
        let (layout, sampler) = (&self.texture_layout, &self.sampler);
        self.groups
            .entry(key)
            .or_insert_with(|| texture_group(device, layout, view, sampler));
        Some(key)
    }

    /// Draws the runs [`prepare`](Self::prepare) returned into the scene
    /// pass, whose frame bindings are bound at group 0.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass, draws: &[DecalDraw]) {
        let Some(normals) = self.normals_group.as_ref().filter(|_| !draws.is_empty()) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, normals, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for draw in draws {
            let group = draw
                .texture
                .and_then(|key| self.groups.get(&key))
                .unwrap_or(&self.white_group);
            pass.set_bind_group(2, group, &[]);
            pass.draw(0..36, draw.instances.clone());
        }
    }

    /// Drops the bind groups of pictures that are no longer kept.
    pub(super) fn forget(&mut self, kept: impl Fn(u64) -> bool) {
        self.groups.retain(|key, _| kept(*key));
    }
}

/// Which of `decals` are drawn, by index in the list's order, each with how
/// much of it shows by distance: the `limits.count` nearest `eye` (by their
/// boxes' middles) within `limits.distance` that touch `view`, fading out
/// over the last tenth of that distance. Decals that can't be drawn (no
/// opacity, a flat box, a transform that isn't a number) are left out.
fn kept_decals(
    decals: &[Decal],
    limits: DecalLimits,
    eye: Vec3,
    view: &Frustum,
) -> Vec<(usize, f32)> {
    if limits.count == 0 || limits.distance <= 0.0 {
        return Vec::new();
    }
    let mut near: Vec<(usize, f32)> = decals
        .iter()
        .enumerate()
        .filter(|(_, decal)| {
            decal.opacity > 0.0
                && decal.transform.is_finite()
                && decal.transform.determinant().abs() > 1e-12
        })
        .filter_map(|(index, decal)| {
            let (center, radius) = decal.sphere();
            let distance = (center.distance(eye) - radius).max(0.0);
            (distance < limits.distance && view.touches((center, radius)))
                .then_some((index, distance))
        })
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    near.truncate(limits.count as usize);
    near.sort_by_key(|&(index, _)| index);
    let fade_from = limits.distance * 0.9;
    near.into_iter()
        .map(|(index, distance)| {
            let fade = ((limits.distance - distance) / (limits.distance - fade_from)).min(1.0);
            (index, fade)
        })
        .collect()
}

fn decal_raw(decal: &Decal, fade: f32) -> DecalRaw {
    let angle = decal.fade_angle.clamp(1.0, 180.0).to_radians();
    DecalRaw {
        model: decal.transform.to_cols_array_2d(),
        inverse: decal.transform.inverse().to_cols_array_2d(),
        color: linear_color(decal.color)
            .extend(decal.opacity.clamp(0.0, 1.0) * fade)
            .to_array(),
        params: [
            angle.cos(),
            (angle * 0.5).cos(),
            f32::from(u8::from(decal.ignore_characters)),
            0.0,
        ],
    }
}

/// Draws decals into the HDR scene with `samples` per pixel: the box's back
/// faces, with no depth test or write, blended premultiplied.
fn decal_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    let shader = super::shader_module(device, "decals", super::gpu::DECAL_SHADER);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("decals"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_decal"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<DecalRaw>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
                    4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
                    8 => Float32x4, 9 => Float32x4
                ],
            })],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Front),
            ..Default::default()
        },
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
            entry_point: Some("fs_decal"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    // The scene's own alpha stays as it is.
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

fn texture_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("decal picture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("decals"),
        size: (capacity * size_of::<DecalRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::*;

    fn decal_at(x: f32) -> Decal {
        Decal {
            transform: Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
            texture: None,
            color: [1.0; 3],
            opacity: 1.0,
            fade_angle: 60.0,
            ignore_characters: false,
        }
    }

    fn everywhere() -> Frustum {
        let view =
            glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 50.0), Vec3::ZERO, Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(1.2, 1.0, 0.1, 10_000.0);
        Frustum::of(projection * view)
    }

    #[test]
    fn the_nearest_decals_are_kept_in_list_order_and_far_ones_fade() {
        let decals: Vec<Decal> = [30.0, 0.0, 10.0, -20.0, 5.0]
            .into_iter()
            .map(decal_at)
            .collect();
        let limits = DecalLimits {
            count: 3,
            distance: 100.0,
        };
        let eye = Vec3::ZERO;
        let kept = kept_decals(&decals, limits, eye, &everywhere());
        let indexes: Vec<usize> = kept.iter().map(|&(index, _)| index).collect();
        // 0, 5 and 10 studs away, in the list's order.
        assert_eq!(indexes, [1, 2, 4]);
        assert!(kept.iter().all(|&(_, fade)| fade == 1.0));

        // Past 90% of the distance a decal fades; past it, it's gone.
        let limits = DecalLimits {
            count: 10,
            distance: 20.0,
        };
        // A unit box reaches 1.5 studs (half its sides' sum) from its middle.
        let kept = kept_decals(&decals, limits, eye, &everywhere());
        assert!(kept.iter().all(|&(index, _)| index != 0), "30 studs away");
        let (_, fade) = kept.iter().find(|&&(index, _)| index == 3).unwrap();
        assert!((fade - 0.75).abs() < 1e-4, "18.5 of 20 studs: {fade}");
        let fades = kept_decals(&[decal_at(20.5)], limits, eye, &everywhere());
        assert!((fades[0].1 - 0.5).abs() < 1e-4, "{fades:?}");
    }

    #[test]
    fn low_quality_keeps_no_decals() {
        let decals = vec![decal_at(0.0)];
        let low = crate::Quality::Low.decal_limits();
        assert!(kept_decals(&decals, low, Vec3::ZERO, &everywhere()).is_empty());
        let medium = crate::Quality::Medium.decal_limits();
        assert_eq!(
            kept_decals(&decals, medium, Vec3::ZERO, &everywhere()).len(),
            1
        );
    }

    #[test]
    fn a_decal_raw_holds_its_inverse_and_fade_angles() {
        let mut decal = decal_at(3.0);
        decal.transform *= Mat4::from_scale(Vec3::new(2.0, 2.0, 4.0));
        decal.ignore_characters = true;
        let raw = decal_raw(&decal, 0.5);
        let product = Mat4::from_cols_array_2d(&raw.model) * Mat4::from_cols_array_2d(&raw.inverse);
        assert!(product.abs_diff_eq(Mat4::IDENTITY, 1e-5));
        assert_eq!(raw.color[3], 0.5);
        assert!((raw.params[0] - 0.5).abs() < 1e-5, "cos 60°");
        assert!((raw.params[1] - 30f32.to_radians().cos()).abs() < 1e-5);
        assert_eq!(raw.params[2], 1.0);
    }
}
