//! The sky's environment maps (`environment.wgsl`).
//!
//! While the reflection quality is above Low, the sky (its gradient, or a
//! sky image) is baked whenever it changes, never every frame: drawn into a
//! source cubemap, halved into that cubemap's mip levels, then blurred into
//! the reflection cubemap, one roughness per level (GGX). Its diffuse light
//! is worked out on the CPU as spherical harmonics. At Low nothing here is
//! allocated, and the scene shaders work the gradient out per pixel.

use std::borrow::Cow;
use std::f32::consts::PI;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::draw_list::{Sky, SkyImage};
use crate::graphics::{EnvironmentMapSize, ReflectionQuality};

use super::gpu::shader_module;
use super::lighting::linear_color;

/// The environment maps' and sky image's format: light brighter than 1,
/// which every GPU can draw into and filter.
pub(super) const ENVIRONMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The size of the reflection cubemap's smallest (roughest) level.
pub(super) const ROUGHEST_LEVEL_SIZE: u32 = 4;
/// The widest grid of a sky image's pixels its harmonics are worked out on.
const HARMONICS_WIDTH: u32 = 128;

/// The bake shader's settings. Must match `Bake` in environment.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakeUniform {
    top: [f32; 4],
    horizon: [f32; 4],
    ground: [f32; 4],
    /// 1 to draw the sky image, the image's level to read at the horizon,
    /// directions per texel of a rough level, and the reflection cubemap's
    /// last level.
    settings: [f32; 4],
}

/// The sky's environment maps and the passes that bake them.
pub(super) struct Environment {
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sky_face: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    prefilter: wgpu::RenderPipeline,
    /// Smooth between texels and levels. Wraps around a sky image's sides.
    pub(super) sampler: wgpu::Sampler,
    /// Bound in place of the reflection cubemap and the sky image while
    /// there are none.
    blank_cube: wgpu::TextureView,
    blank_image: wgpu::TextureView,
    maps: Option<Maps>,
    image: Option<ImageTexture>,
    /// What the maps were last baked from.
    baked: Option<BakeKey>,
    /// The sky's diffuse light (see [`harmonics`]).
    harmonics: [Vec3; 9],
    /// How many times the maps have been baked, for tests.
    pub(super) bakes: u32,
}

/// The cubemaps for one reflection quality.
struct Maps {
    size: EnvironmentMapSize,
    /// The sky, with every mip level down to 1×1.
    source: wgpu::Texture,
    /// The sky as surfaces of rising roughness reflect it, one per level.
    reflections: wgpu::Texture,
    reflections_view: wgpu::TextureView,
    bytes: u64,
}

/// A sky image on the GPU, with every mip level.
struct ImageTexture {
    key: u64,
    /// The largest texture size it was fitted into.
    max_size: u32,
    /// Its width on the GPU, after any fitting.
    width: u32,
    view: wgpu::TextureView,
    /// Its diffuse light (see [`harmonics`]).
    harmonics: [Vec3; 9],
    bytes: u64,
}

/// What a bake depends on: the gradient's colors (linear light, as bits)
/// or the sky image, and the cubemaps' size. The environment intensity and
/// the sun aren't baked, so changing them doesn't bake again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BakeKey {
    colors: Option<[u32; 9]>,
    image: Option<u64>,
    map: EnvironmentMapSize,
}

impl Environment {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("environment bake"),
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
                texture(1, wgpu::TextureViewDimension::Cube),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3, wgpu::TextureViewDimension::D2),
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment bake"),
            size: size_of::<BakeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = shader_module(device, "environment bake", super::gpu::ENVIRONMENT_SHADER);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("environment bake"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_face"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(ENVIRONMENT_FORMAT.into())],
                }),
                multiview: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment"),
            address_mode_u: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blank = |label, layers, dimension| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ENVIRONMENT_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            // New textures start out zeroed, but say so.
            queue.write_texture(
                texture.as_image_copy(),
                &vec![0; 8 * layers as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8),
                    rows_per_image: Some(1),
                },
                texture.size(),
            );
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(dimension),
                ..Default::default()
            })
        };
        Self {
            sky_face: pipeline("fs_sky_face"),
            downsample: pipeline("fs_downsample"),
            prefilter: pipeline("fs_prefilter"),
            layout,
            uniform,
            sampler,
            blank_cube: blank("no environment map", 6, wgpu::TextureViewDimension::Cube),
            blank_image: blank("no sky image", 1, wgpu::TextureViewDimension::D2),
            maps: None,
            image: None,
            baked: None,
            harmonics: [Vec3::ZERO; 9],
            bakes: 0,
        }
    }

    /// Makes, frees or bakes the maps so they show `sky` at `quality`, with
    /// textures no bigger than `max_texture_size`. True when the textures
    /// to bind changed, so the frame's bindings must be made again.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sky: &Sky,
        quality: ReflectionQuality,
        max_texture_size: u32,
    ) -> bool {
        let map = quality.map();
        let mut rebind = false;
        // The image is shown only through the maps, so at Low it's freed.
        let image = sky.image.as_ref().filter(|_| map.is_some());
        let wanted = image.map(|image| (image.key, max_texture_size));
        if self.image.as_ref().map(|image| (image.key, image.max_size)) != wanted {
            self.image = image.map(|image| upload_image(device, queue, image, max_texture_size));
            rebind = true;
        }
        if self.maps.as_ref().map(|maps| maps.size) != map {
            self.maps = map.map(|size| Maps::new(device, size));
            self.baked = None;
            rebind = true;
        }
        let Some(map) = map else {
            return rebind;
        };
        let colors = [sky.top_color, sky.horizon_color, sky.ground_color].map(linear_color);
        let key = BakeKey {
            colors: self.image.is_none().then(|| {
                let [top, horizon, ground] = colors.map(|color| color.to_array().map(f32::to_bits));
                [top, horizon, ground]
                    .concat()
                    .try_into()
                    .expect("nine numbers")
            }),
            image: self.image.as_ref().map(|image| image.key),
            map,
        };
        if self.baked != Some(key) {
            self.harmonics = match &self.image {
                Some(image) => image.harmonics,
                None => harmonics(64, 32, |_, _, dir| gradient_sky(dir, colors)),
            };
            self.bake(device, queue, colors);
            self.baked = Some(key);
            self.bakes += 1;
        }
        rebind
    }

    /// The frame uniform's `environment` and `sky_light` (see lighting.wgsl).
    pub(super) fn uniform(&self) -> ([f32; 4], [[f32; 4]; 9]) {
        let Some(maps) = &self.maps else {
            return ([0.0; 4], [[0.0; 4]; 9]);
        };
        let last_level = (maps.reflections.mip_level_count() - 1) as f32;
        let (image, width) = self
            .image
            .as_ref()
            .map_or((0.0, 0.0), |image| (1.0, image.width as f32));
        (
            [1.0, last_level, image, width],
            self.harmonics.map(|light| light.extend(0.0).to_array()),
        )
    }

    /// The reflection cubemap and the sky image to bind, or blank ones.
    pub(super) fn views(&self) -> (&wgpu::TextureView, &wgpu::TextureView) {
        (
            self.maps
                .as_ref()
                .map_or(&self.blank_cube, |maps| &maps.reflections_view),
            self.image
                .as_ref()
                .map_or(&self.blank_image, |image| &image.view),
        )
    }

    /// Bytes of the cubemaps and the sky image on the GPU.
    pub(super) fn bytes(&self) -> u64 {
        self.maps.as_ref().map_or(0, |maps| maps.bytes)
            + self.image.as_ref().map_or(0, |image| image.bytes)
    }

    /// Draws the sky (the gradient in `colors`, linear light, or the sky
    /// image) into the maps and blurs it, all in one submission.
    fn bake(&self, device: &wgpu::Device, queue: &wgpu::Queue, [top, horizon, ground]: [Vec3; 3]) {
        let Some(maps) = &self.maps else {
            return;
        };
        let size = maps.size.size;
        let image_level = self.image.as_ref().map_or(0.0, |image| {
            (image.width as f32 / (4.0 * size as f32)).log2()
        });
        let last_level = (maps.reflections.mip_level_count() - 1) as f32;
        let uniform = BakeUniform {
            top: top.extend(0.0).to_array(),
            horizon: horizon.extend(0.0).to_array(),
            ground: ground.extend(0.0).to_array(),
            settings: [
                if self.image.is_some() { 1.0 } else { 0.0 },
                image_level,
                maps.size.samples as f32,
                last_level,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("environment bake"),
        });
        // 1. The sky into the source's largest level. Nothing is read from
        // the source yet, so the blank cube stands in for it.
        let blank = self.group(device, &self.blank_cube);
        draw_faces(&mut encoder, (&maps.source, 0, 0), &self.sky_face, &blank);
        // 2. and 3.
        self.blur(device, &mut encoder, &maps.source, (&maps.reflections, 0));
        queue.submit([encoder.finish()]);
    }

    /// Blurs `source`, a cubemap whose largest level is drawn, into each
    /// level of the cube at array layer `first_layer` of `target`, rougher
    /// at each level, with `samples` directions per texel of a rough level:
    /// for a reflection probe (`renderer/probes.rs`), in one submission.
    pub(super) fn filter(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &wgpu::Texture,
        (target, first_layer): (&wgpu::Texture, u32),
        samples: u32,
    ) {
        let last_level = (target.mip_level_count() - 1) as f32;
        let uniform = BakeUniform {
            settings: [0.0, 0.0, samples as f32, last_level],
            ..Zeroable::zeroed()
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("reflection probe filter"),
        });
        self.blur(device, &mut encoder, source, (target, first_layer));
        queue.submit([encoder.finish()]);
    }

    /// Steps 2 and 3 of a bake: each level of `source` below its largest
    /// from the one above it, then each level of the cube at `first_layer`
    /// of `target` from the whole source, one roughness per level (see
    /// environment.wgsl).
    fn blur(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        (target, first_layer): (&wgpu::Texture, u32),
    ) {
        let source_levels = source.mip_level_count();
        for level in 1..source_levels {
            let above = self.group(device, &cube_view(source, level - 1..level));
            draw_faces(encoder, (source, level, 0), &self.downsample, &above);
        }
        let whole = self.group(device, &cube_view(source, 0..source_levels));
        for level in 0..target.mip_level_count() {
            draw_faces(
                encoder,
                (target, level, first_layer),
                &self.prefilter,
                &whole,
            );
        }
    }

    /// The bake passes' bindings, reading `source` (and the sky image).
    fn group(&self, device: &wgpu::Device, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("environment bake"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(self.views().1),
                },
            ],
        })
    }
}

/// Draws `pipeline` into every face of `level` of the cube at array layer
/// `first_layer` of `texture`.
fn draw_faces(
    encoder: &mut wgpu::CommandEncoder,
    (texture, level, first_layer): (&wgpu::Texture, u32, u32),
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    for face in 0..6 {
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: level,
            mip_level_count: Some(1),
            base_array_layer: first_layer + face,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("environment face"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        // The instance says which face and level (environment.wgsl).
        let instance = face + 6 * level;
        pass.draw(0..3, instance..instance + 1);
    }
}

impl Maps {
    fn new(device: &wgpu::Device, size: EnvironmentMapSize) -> Self {
        let cube = |label, levels| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.size,
                    height: size.size,
                    depth_or_array_layers: 6,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ENVIRONMENT_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let source = cube("environment source", size.size.ilog2() + 1);
        let reflection_levels = (size.size / ROUGHEST_LEVEL_SIZE).max(1).ilog2() + 1;
        let reflections = cube("environment reflections", reflection_levels);
        let reflections_view = cube_view(&reflections, 0..reflection_levels);
        let bytes = [&source, &reflections]
            .iter()
            .map(|texture| {
                (0..texture.mip_level_count())
                    .map(|level| u64::from((size.size >> level).max(1)).pow(2) * 6 * 8)
                    .sum::<u64>()
            })
            .sum();
        Self {
            size,
            source,
            reflections,
            reflections_view,
            bytes,
        }
    }
}

/// `texture`'s six faces as a cube, with only `levels`.
fn cube_view(texture: &wgpu::Texture, levels: Range<u32>) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("environment cube"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        base_mip_level: levels.start,
        mip_level_count: Some(levels.end - levels.start),
        base_array_layer: 0,
        array_layer_count: Some(6),
        ..Default::default()
    })
}

/// One level of a sky image on the CPU: its width, height and linear rgb.
/// The largest level borrows the draw list's pixels instead of copying them
/// (`Cow`: borrowed or owned).
struct Level<'a> {
    width: u32,
    height: u32,
    rgb: Cow<'a, [f32]>,
}

impl Level<'_> {
    /// Half the width and height (at least 1), each pixel the average of
    /// the square of four it covers.
    fn halved(&self) -> Level<'static> {
        let (width, height) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let pixel = |x: u32, y: u32| {
            let at = ((y.min(self.height - 1) * self.width + x.min(self.width - 1)) * 3) as usize;
            Vec3::from_slice(&self.rgb[at..at + 3])
        };
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                let (x2, y2) = (x * 2, y * 2);
                let sum =
                    pixel(x2, y2) + pixel(x2 + 1, y2) + pixel(x2, y2 + 1) + pixel(x2 + 1, y2 + 1);
                rgb.extend_from_slice(&(sum * 0.25).to_array());
            }
        }
        Level {
            width,
            height,
            rgb: Cow::Owned(rgb),
        }
    }
}

/// Uploads `image` with every mip level, halved first until it fits in
/// `max_size`, and works out its harmonics. An image with broken data is
/// uploaded black (and logged).
fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &SkyImage,
    max_size: u32,
) -> ImageTexture {
    let pixels = (image.width as usize) * (image.height as usize);
    let mut level = if pixels > 0 && image.rgb.len() == pixels * 3 {
        Level {
            width: image.width,
            height: image.height,
            rgb: Cow::Borrowed(image.rgb.as_slice()),
        }
    } else {
        log::warn!("{image:?} has broken data, so the sky is drawn black");
        Level {
            width: 1,
            height: 1,
            rgb: Cow::Owned(vec![0.0; 3]),
        }
    };
    while level.width > max_size.max(1) || level.height > max_size.max(1) {
        level = level.halved();
    }
    let mut levels = vec![level];
    while let Some(last) = levels
        .last()
        .filter(|last| last.width > 1 || last.height > 1)
    {
        let next = last.halved();
        levels.push(next);
    }
    let (width, height) = (levels[0].width, levels[0].height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ENVIRONMENT_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = 0;
    for (index, level) in levels.iter().enumerate() {
        let texels: Vec<u16> = level
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[red, green, blue]| [half(red), half(green), half(blue), half(1.0)])
            .collect();
        let data: &[u8] = bytemuck::cast_slice(&texels);
        bytes += data.len() as u64;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: index as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(level.width * 8),
                rows_per_image: Some(level.height),
            },
            wgpu::Extent3d {
                width: level.width,
                height: level.height,
                depth_or_array_layers: 1,
            },
        );
    }
    // The first level small enough to add up quickly.
    let small = levels
        .iter()
        .find(|level| level.width <= HARMONICS_WIDTH)
        .unwrap_or(&levels[levels.len() - 1]);
    let harmonics = harmonics(small.width, small.height, |x, y, _| {
        let at = ((y * small.width + x) * 3) as usize;
        Vec3::from_slice(&small.rgb[at..at + 3])
    });
    ImageTexture {
        key: image.key,
        max_size,
        width,
        view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        harmonics,
        bytes,
    }
}

/// `value` as a 16-bit float's bits, for a texture. Light is never below
/// 0: anything that isn't a positive number becomes 0, anything too big
/// for 16 bits the largest it holds, and anything too small 0.
pub(super) fn half(value: f32) -> u16 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    let bits = value.min(65504.0).to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    if exponent <= 0 {
        return 0;
    }
    ((exponent as u16) << 10) | ((bits >> 13) & 0x3ff) as u16
}

/// The procedural sky looking along `dir`, from its colors (linear light):
/// a copy of `gradient_sky` in sky.wgsl.
fn gradient_sky(dir: Vec3, [top, horizon, ground]: [Vec3; 3]) -> Vec3 {
    let up = dir.y.clamp(0.0, 1.0);
    let sky = horizon.lerp(top, up.powf(0.45));
    let t = (-dir.y / 0.2).clamp(0.0, 1.0);
    sky.lerp(ground, t * t * (3.0 - 2.0 * t))
}

/// The direction the middle of pixel (`x`, `y`) of a `width` × `height`
/// sky image looks along: the inverse of `equirect_uv` in sky.wgsl.
fn pixel_direction(x: u32, y: u32, width: u32, height: u32) -> Vec3 {
    let theta = (y as f32 + 0.5) / height as f32 * PI;
    let phi = ((x as f32 + 0.5) / width as f32 - 0.5) * 2.0 * PI;
    Vec3::new(
        theta.sin() * phi.sin(),
        theta.cos(),
        -theta.sin() * phi.cos(),
    )
}

/// The nine second-order spherical harmonics at direction `d`, in the
/// order lighting.wgsl's `sky_irradiance` reads them.
pub(super) fn basis(d: Vec3) -> [f32; 9] {
    [
        0.282095,
        0.488603 * d.y,
        0.488603 * d.z,
        0.488603 * d.x,
        1.092548 * d.x * d.y,
        1.092548 * d.y * d.z,
        0.315392 * (3.0 * d.z * d.z - 1.0),
        1.092548 * d.x * d.z,
        0.546274 * (d.x * d.x - d.y * d.y),
    ]
}

/// The sky's diffuse light as spherical harmonics, from `radiance` (given
/// a pixel and its direction) at the middle of each pixel of a `width` ×
/// `height` grid wrapped around the world like a sky image. Each pixel
/// counts by the part of the sphere it covers. The result is blurred over
/// the hemisphere around each direction (a cosine lobe) and divided by
/// pi, so it gives what a white matte surface facing that way returns.
fn harmonics(width: u32, height: u32, radiance: impl Fn(u32, u32, Vec3) -> Vec3) -> [Vec3; 9] {
    let mut sums = [Vec3::ZERO; 9];
    let cell = (2.0 * PI / width as f32) * (PI / height as f32);
    for y in 0..height {
        let solid_angle = cell * ((y as f32 + 0.5) / height as f32 * PI).sin();
        for x in 0..width {
            let dir = pixel_direction(x, y, width, height);
            let light = radiance(x, y, dir) * solid_angle;
            for (sum, basis) in sums.iter_mut().zip(basis(dir)) {
                *sum += light * basis;
            }
        }
    }
    // The cosine lobe's share of each band, over pi: 1, 2/3 and 1/4.
    let bands = [
        1.0,
        2.0 / 3.0,
        2.0 / 3.0,
        2.0 / 3.0,
        0.25,
        0.25,
        0.25,
        0.25,
        0.25,
    ];
    std::array::from_fn(|i| sums[i] * bands[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a white matte surface facing `n` returns, as lighting.wgsl's
    /// `sky_irradiance` works it out.
    fn irradiance(harmonics: &[Vec3; 9], n: Vec3) -> Vec3 {
        let mut light = Vec3::ZERO;
        for (coefficient, basis) in harmonics.iter().zip(basis(n)) {
            light += *coefficient * basis;
        }
        light.max(Vec3::ZERO)
    }

    #[test]
    fn an_even_sky_lights_every_surface_with_its_own_color() {
        let color = Vec3::new(0.2, 0.5, 1.5);
        let even = harmonics(64, 32, |_, _, _| color);
        for n in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::NEG_Z,
            Vec3::ONE.normalize(),
        ] {
            let light = irradiance(&even, n);
            assert!(
                (light - color).abs().max_element() < 0.01,
                "{n} got {light}"
            );
        }
    }

    #[test]
    fn a_sky_lit_from_above_lights_upward_faces_most() {
        // White above the horizon, black below: a surface facing up sees
        // all of it (1), one facing sideways half, one facing down none.
        let above = harmonics(128, 64, |_, _, dir| Vec3::splat(f32::from(dir.y > 0.0)));
        let up = irradiance(&above, Vec3::Y).x;
        let side = irradiance(&above, Vec3::X).x;
        let down = irradiance(&above, Vec3::NEG_Y).x;
        assert!((up - 1.0).abs() < 0.1, "up {up}");
        assert!((side - 0.5).abs() < 0.05, "side {side}");
        assert!(down < 0.1, "down {down}");
    }

    #[test]
    fn pixel_directions_wrap_the_way_the_shader_reads_them() {
        let (width, height) = (8, 4);
        // The top row looks up, the bottom row down, the middle column
        // toward -Z and a quarter of the way across toward -X.
        assert!(pixel_direction(0, 0, width, height).y > 0.9);
        assert!(pixel_direction(0, height - 1, width, height).y < -0.9);
        let middle = pixel_direction(512, 256, 1024, 512);
        assert!(middle.z < -0.99, "{middle}");
        let quarter = pixel_direction(256, 256, 1024, 512);
        assert!(quarter.x < -0.9, "{quarter}");
        // equirect_uv, on the CPU, gives each pixel's middle back.
        for (x, y) in [(0, 0), (3, 1), (7, 3), (5, 2)] {
            let dir = pixel_direction(x, y, width, height);
            let u = dir.x.atan2(-dir.z) / (2.0 * PI) + 0.5;
            let v = dir.y.clamp(-1.0, 1.0).acos() / PI;
            assert!((u * width as f32 - (x as f32 + 0.5)).abs() < 1e-3);
            assert!((v * height as f32 - (y as f32 + 0.5)).abs() < 1e-3);
        }
    }

    #[test]
    fn halves_are_sixteen_bit_floats() {
        assert_eq!(half(1.0), 0x3c00);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(2.0), 0x4000);
        assert_eq!(half(65504.0), 0x7bff);
        assert_eq!(half(1.0e9), 0x7bff, "too big holds the largest");
        assert_eq!(half(-1.0), 0);
        assert_eq!(half(f32::NAN), 0);
        assert_eq!(half(1.0e-9), 0);
    }

    #[test]
    fn a_halved_level_averages_each_square_of_four() {
        let level = Level {
            width: 2,
            height: 2,
            rgb: vec![0.0, 0.0, 0.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0].into(),
        };
        let half = level.halved();
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(*half.rgb, [2.0; 3]);
        // An odd edge repeats its last pixel.
        let row = Level {
            width: 3,
            height: 1,
            rgb: vec![1.0, 1.0, 1.0, 3.0, 3.0, 3.0, 8.0, 8.0, 8.0].into(),
        };
        assert_eq!(*row.halved().rgb, [2.0; 3]);
    }

    #[test]
    fn the_gradient_matches_the_shaders_colors() {
        let colors = [
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        ];
        assert_eq!(gradient_sky(Vec3::Y, colors), colors[0]);
        assert_eq!(gradient_sky(Vec3::X, colors), colors[1]);
        assert_eq!(gradient_sky(Vec3::NEG_Y, colors), colors[2]);
    }
}
