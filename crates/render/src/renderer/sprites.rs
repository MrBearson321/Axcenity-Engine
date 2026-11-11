//! The sprite pass: camera-facing pictures in the 3D scene, such as
//! particles, drawn into the HDR scene after every item.
//!
//! Each sprite is one instance of a 6-corner quad, which `sprite.wgsl` turns
//! to face the camera. Sprites are depth-tested against solid items but
//! don't write depth, and are drawn farthest first. Their color is
//! premultiplied by their opacity, and the alpha written is
//! `opacity × (1 - light emission)`, so one blend state covers painting over
//! what's behind (light emission 0) and adding light (1).
//!
//! GPU particle emitters (`gpu_particles.rs`) write their particles as
//! `SpriteRaw`s into buffers of their own, which this pass draws before
//! the list's sprites.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use super::culling::Frustum;
use super::{DEPTH_FORMAT, HDR_FORMAT, linear_color};
use crate::draw_list::{DrawList, TextureSource};

/// One sprite for the GPU. Must match `SpriteIn` in sprite.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SpriteRaw {
    /// The center, and the size in studs.
    center_size: [f32; 4],
    /// Linear color times brightness and opacity, and the opacity.
    color: [f32; 4],
    /// The turn in radians, the light emission, then unused.
    params: [f32; 4],
}

/// The camera, for turning the quads. Must match `Sprites` in sprite.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SpriteUniform {
    view_proj: [[f32; 4]; 4],
    right: [f32; 4],
    up: [f32; 4],
    eye: [f32; 4],
    fog: super::fog::FogRaw,
}

/// A run of sprites with the same texture (`None` is plain white).
pub(super) struct SpriteDraw {
    texture: Option<u64>,
    instances: Range<u32>,
}

pub(super) struct SpritePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// For sprites without a texture.
    white_group: wgpu::BindGroup,
    /// One bind group per texture, by its key.
    groups: HashMap<u64, wgpu::BindGroup>,
    buffer: wgpu::Buffer,
    capacity: usize,
}

impl SpritePass {
    /// `samples` per pixel must match the scene's targets.
    pub(super) fn new(device: &wgpu::Device, white: &wgpu::TextureView, samples: u32) -> Self {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite texture"),
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
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite camera"),
            size: size_of::<SpriteUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite camera"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite textures"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white_group = texture_group(device, &texture_layout, white, &sampler);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprites"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = sprite_pipeline(device, &layout, samples);
        let capacity = 256;
        Self {
            pipeline,
            layout,
            uniform_buffer,
            uniform_group,
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
        self.pipeline = sprite_pipeline(device, &self.layout, samples);
    }

    /// Uploads `list`'s sprites that `view` can see, farthest first, and
    /// returns the runs to draw, how many were culled, and the texture keys
    /// of `list`'s GPU particle emitters, in its order. `view_of` finds an
    /// uploaded texture by its key; a sprite whose texture couldn't be
    /// uploaded is drawn as a plain square.
    pub(super) fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        (view_proj, fog): (Mat4, super::fog::FogRaw),
        view: &Frustum,
        view_of: impl Fn(u64) -> Option<&'a wgpu::TextureView>,
    ) -> (Vec<SpriteDraw>, u32, Vec<Option<u64>>) {
        if list.sprites.is_empty() && list.particle_emitters.is_empty() {
            return (Vec::new(), 0, Vec::new());
        }
        let eye = list.camera.position;
        let forward = (list.camera.look_at - eye).normalize_or(Vec3::NEG_Z);
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let uniform = SpriteUniform {
            view_proj: view_proj.to_cols_array_2d(),
            right: right.extend(0.0).to_array(),
            up: up.extend(0.0).to_array(),
            eye: eye.extend(1.0).to_array(),
            fog,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut order: Vec<usize> = (0..list.sprites.len())
            .filter(|&i| {
                let sprite = &list.sprites[i];
                sprite.opacity > 0.0 && sprite.size > 0.0 && sprite.position.is_finite()
            })
            .collect();
        // A turned square reaches half its diagonal from its center.
        let reach = |size: f32| size * std::f32::consts::FRAC_1_SQRT_2;
        let shown = order.len();
        order.retain(|&i| {
            let sprite = &list.sprites[i];
            view.touches((sprite.position, reach(sprite.size)))
        });
        let culled = (shown - order.len()) as u32;
        let distance = |i: usize| list.sprites[i].position.distance_squared(eye);
        order.sort_by(|&a, &b| distance(b).total_cmp(&distance(a)));

        let mut raws = Vec::with_capacity(order.len());
        let mut draws: Vec<SpriteDraw> = Vec::new();
        // The key of an uploaded texture, with its bind group made.
        let mut uploaded = |texture: &Option<Arc<dyn TextureSource>>| {
            let key = texture.as_ref()?.key();
            let view = view_of(key)?;
            let (layout, sampler) = (&self.texture_layout, &self.sampler);
            self.groups
                .entry(key)
                .or_insert_with(|| texture_group(device, layout, view, sampler));
            Some(key)
        };
        let emitter_textures = list
            .particle_emitters
            .iter()
            .map(|emitter| uploaded(&emitter.look.texture))
            .collect();
        for i in order {
            let sprite = &list.sprites[i];
            let texture = uploaded(&sprite.texture);
            let opacity = sprite.opacity.clamp(0.0, 1.0);
            let color = linear_color(sprite.color) * sprite.brightness.max(0.0) * opacity;
            let index = raws.len() as u32;
            raws.push(SpriteRaw {
                center_size: sprite.position.extend(sprite.size).to_array(),
                color: color.extend(opacity).to_array(),
                params: [
                    sprite.rotation_degrees.to_radians(),
                    sprite.light_emission.clamp(0.0, 1.0),
                    0.0,
                    0.0,
                ],
            });
            match draws.last_mut() {
                Some(last) if last.texture == texture => last.instances.end = index + 1,
                _ => draws.push(SpriteDraw {
                    texture,
                    instances: index..index + 1,
                }),
            }
        }
        if raws.len() > self.capacity {
            self.capacity = raws.len().next_power_of_two();
            self.buffer = create_buffer(device, self.capacity);
        }
        if !raws.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&raws));
        }
        (draws, culled, emitter_textures)
    }

    /// Draws GPU particle emitters' sprites (`emitters`: each one's buffer
    /// of `SpriteRaw`s, how many, and its texture key), then the runs
    /// [`prepare`](Self::prepare) returned, into the scene pass.
    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[SpriteDraw],
        emitters: &[(&wgpu::Buffer, u32, Option<u64>)],
    ) {
        if draws.is_empty() && emitters.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_group, &[]);
        let group = |texture: Option<u64>| {
            texture
                .and_then(|key| self.groups.get(&key))
                .unwrap_or(&self.white_group)
        };
        for &(buffer, count, texture) in emitters {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_bind_group(1, group(texture), &[]);
            pass.draw(0..6, 0..count);
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for draw in draws {
            pass.set_bind_group(1, group(draw.texture), &[]);
            pass.draw(0..6, draw.instances.clone());
        }
    }

    /// Drops the bind groups of textures that are no longer kept.
    pub(super) fn forget(&mut self, kept: impl Fn(u64) -> bool) {
        self.groups.retain(|key, _| kept(*key));
    }
}

/// Draws sprites into the HDR scene with `samples` per pixel.
fn sprite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    let shader = super::shader_module(device, "sprites", super::gpu::SPRITE_SHADER);
    let blend = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprites"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<SpriteRaw>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x4, 1 => Float32x4, 2 => Float32x4
                ],
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: blend,
                    alpha: blend,
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
        label: Some("sprite texture"),
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
        label: Some("sprites"),
        size: (capacity * size_of::<SpriteRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
