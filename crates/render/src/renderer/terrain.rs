//! Terrain chunks on the GPU (`terrain.wgsl`).
//!
//! A terrain's layers become three texture arrays, one slice per layer:
//! base colors, normal maps, and "surface" (roughness, metalness, ambient
//! occlusion and height packed into R, G, B and A). Arrays need every slice
//! the same size, so each layer's maps are resized on the CPU to the
//! largest one's size (a power of two, at most [`MAX_LAYER_SIZE`]). That
//! happens once per set of textures, not when a color or tiling changes:
//! those go in a small uniform buffer, rewritten in place.
//!
//! Each chunk's weight textures are ordinary textures, uploaded like any
//! other. A chunk is drawn with one bind group: its weights, the arrays and
//! the uniform.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};

use bytemuck::{Pod, Zeroable};

use crate::draw_list::{ChannelMap, TerrainLayers, TerrainSurface, TerrainView, TextureSource};

use super::gpu::mip_levels;
use super::lighting::linear_color;
use super::{
    Cached, Counted, GpuBytes, GpuTexture, KEEP_UNDRAWN_FRAMES, TEXTURE_FORMAT, TEXTURE_SRGB_FORMAT,
};

/// The most layers a terrain blends. Must match `layers` in terrain.wgsl.
pub(super) const TERRAIN_LAYERS: usize = 8;
/// The largest a layer's textures are kept, in texels a side.
const MAX_LAYER_SIZE: u32 = 1024;
/// A terrain's bind groups and uniforms not drawn for this many frames are
/// dropped. Shorter than other things: painting makes new weight textures
/// (and so new bind groups) every frame.
const KEEP_TERRAIN_GROUPS: u64 = 60;

/// One layer as the GPU reads it. Must match `TerrainLayer` in terrain.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct TerrainLayerRaw {
    /// Linear tint, and studs per copy of the textures.
    tint_tile: [f32; 4],
    /// Metalness, roughness, normal strength (0: none), green's sign.
    surface: [f32; 4],
    /// Turn (radians), triplanar (1), anti-tiling, height blend.
    options: [f32; 4],
    /// Linear color in the Weights view.
    debug: [f32; 4],
}

/// Must match `Terrain` in terrain.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TerrainUniform {
    layers: [TerrainLayerRaw; TERRAIN_LAYERS],
    /// Layer count, view, and the world Y of heights 0 and 1.
    info: [f32; 4],
    /// The grid's corner (world X, Z) and the studs between samples.
    grid: [f32; 4],
}

/// The layers' textures, one slice per layer.
struct TerrainArrays {
    /// Kept so they live as long as the bind groups that use them.
    _textures: Vec<wgpu::Texture>,
    /// Their size with every mip level, in the scene's texture bytes.
    _bytes: Counted,
    albedo: wgpu::TextureView,
    normal: wgpu::TextureView,
    surface: wgpu::TextureView,
}

/// The bind group a chunk is drawn with: its terrain's [`TerrainLayers::id`],
/// its textures' key ([`arrays_key`]) and its two weight textures' keys.
pub(super) type TerrainKey = [u64; 4];

/// The key of `surface`'s bind group.
pub(super) fn terrain_key(surface: &TerrainSurface) -> TerrainKey {
    [
        surface.layers.id,
        arrays_key(&surface.layers),
        surface.weights[0].key(),
        surface.weights[1].key(),
    ]
}

/// Changes when a layer's textures (or which channels are read) do, but
/// not with colors, tiling and the rest.
fn arrays_key(layers: &TerrainLayers) -> u64 {
    let mut hasher = DefaultHasher::new();
    let channel = |map: &Option<ChannelMap>, hasher: &mut DefaultHasher| match map {
        Some(map) => (map.texture.key(), map.channel, map.invert).hash(hasher),
        None => 0u64.hash(hasher),
    };
    for layer in layers.layers.iter().take(TERRAIN_LAYERS) {
        layer
            .base_color_texture
            .as_ref()
            .map(|texture| texture.key())
            .hash(&mut hasher);
        layer
            .normal
            .as_ref()
            .map(|map| map.texture.key())
            .hash(&mut hasher);
        channel(&layer.roughness_map, &mut hasher);
        channel(&layer.metalness_map, &mut hasher);
        channel(&layer.occlusion, &mut hasher);
        channel(&layer.height, &mut hasher);
    }
    layers.layers.len().min(TERRAIN_LAYERS).hash(&mut hasher);
    hasher.finish()
}

/// The terrain pipeline and everything uploaded for terrains.
pub(super) struct TerrainPass {
    layout: wgpu::BindGroupLayout,
    pub(super) pipeline: wgpu::RenderPipeline,
    layer_sampler: wgpu::Sampler,
    weight_sampler: wgpu::Sampler,
    arrays: HashMap<u64, Cached<TerrainArrays>>,
    /// Each terrain's uniform, by [`TerrainLayers::id`], with the
    /// [`TerrainLayers::key`] it holds.
    uniforms: HashMap<u64, Cached<(u64, wgpu::Buffer)>>,
    groups: HashMap<TerrainKey, Cached<wgpu::BindGroup>>,
    /// The scene's texture bytes, which the arrays are counted in.
    texture_bytes: GpuBytes,
    /// How many times the arrays are halved, by the texture quality.
    skipped_levels: u32,
}

impl TerrainPass {
    pub(super) fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        texture_bytes: GpuBytes,
    ) -> Self {
        let texture = |binding, dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[
                texture(0, wgpu::TextureViewDimension::D2),
                texture(1, wgpu::TextureViewDimension::D2),
                sampler(2),
                sampler(3),
                texture(4, wgpu::TextureViewDimension::D2Array),
                texture(5, wgpu::TextureViewDimension::D2Array),
                texture(6, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain layers"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            // Ground is mostly seen at a glancing angle, where this keeps
            // the textures sharp.
            anisotropy_clamp: 8,
            ..Default::default()
        });
        // Weights stop at the chunk's edge, which its neighbour shares.
        let weight_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain weights"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = super::gpu::shader_module(device, "terrain", super::gpu::TERRAIN_SHADER);
        let pipeline = super::gpu::create_pipeline(
            device,
            &shader,
            ("vs_terrain", "fs_terrain"),
            &[frame_layout, &layout],
            super::HDR_FORMAT,
            false,
        );
        Self {
            layout,
            pipeline,
            layer_sampler,
            weight_sampler,
            arrays: HashMap::new(),
            uniforms: HashMap::new(),
            groups: HashMap::new(),
            texture_bytes,
            skipped_levels: 0,
        }
    }

    /// Halves the layer arrays `skipped_levels` times (the texture quality),
    /// freeing those already made so they're made again at the new size.
    pub(super) fn set_skipped_levels(&mut self, skipped_levels: u32) {
        if skipped_levels != self.skipped_levels {
            self.skipped_levels = skipped_levels;
            self.arrays.clear();
            self.groups.clear();
        }
    }

    /// Gets `surface` ready to draw this frame: its arrays, its uniform
    /// and its bind group. `weights` are its two weight textures, already
    /// uploaded (`None` if their data was broken).
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface: &TerrainSurface,
        weights: [Option<&GpuTexture>; 2],
        white: &GpuTexture,
        frame: u64,
    ) {
        let layers = &surface.layers;
        let key = terrain_key(surface);
        let arrays_key = key[1];
        if let Entry::Vacant(slot) = self.arrays.entry(arrays_key) {
            slot.insert(Cached {
                gpu: Some(create_arrays(
                    device,
                    queue,
                    layers,
                    &self.texture_bytes,
                    self.skipped_levels,
                )),
                last_drawn: frame,
            });
        } else if let Some(cached) = self.arrays.get_mut(&arrays_key) {
            cached.last_drawn = frame;
        }
        let contents = bytemuck::bytes_of(&uniform(layers)).to_vec();
        match self.uniforms.entry(layers.id) {
            Entry::Occupied(mut cached) => {
                let cached = cached.get_mut();
                cached.last_drawn = frame;
                if let Some((held, buffer)) = &mut cached.gpu
                    && *held != layers.key
                {
                    queue.write_buffer(buffer, 0, &contents);
                    *held = layers.key;
                }
            }
            Entry::Vacant(slot) => {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("terrain layers"),
                    size: contents.len() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&buffer, 0, &contents);
                slot.insert(Cached {
                    gpu: Some((layers.key, buffer)),
                    last_drawn: frame,
                });
            }
        }
        if let Some(cached) = self.groups.get_mut(&key) {
            cached.last_drawn = frame;
            return;
        }
        let (Some(arrays), Some((_, uniform))) = (
            self.arrays.get(&arrays_key).and_then(|c| c.gpu.as_ref()),
            self.uniforms.get(&layers.id).and_then(|c| c.gpu.as_ref()),
        ) else {
            return;
        };
        let [low, high] = weights.map(|texture| texture.unwrap_or(white));
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain chunk"),
            layout: &self.layout,
            entries: &[
                view(0, &low.linear),
                view(1, &high.linear),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.weight_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.layer_sampler),
                },
                view(4, &arrays.albedo),
                view(5, &arrays.normal),
                view(6, &arrays.surface),
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        self.groups.insert(
            key,
            Cached {
                gpu: Some(group),
                last_drawn: frame,
            },
        );
    }

    pub(super) fn group(&self, key: &TerrainKey) -> Option<&wgpu::BindGroup> {
        self.groups.get(key).and_then(|cached| cached.gpu.as_ref())
    }

    /// Frees what hasn't been drawn lately.
    pub(super) fn forget(&mut self, frame: u64) {
        let within = |last: u64, keep: u64| frame - last < keep;
        self.groups
            .retain(|_, cached| within(cached.last_drawn, KEEP_TERRAIN_GROUPS));
        self.uniforms
            .retain(|_, cached| within(cached.last_drawn, KEEP_TERRAIN_GROUPS));
        self.arrays
            .retain(|_, cached| within(cached.last_drawn, KEEP_UNDRAWN_FRAMES));
    }
}

/// The uniform for `layers`.
fn uniform(layers: &TerrainLayers) -> TerrainUniform {
    let mut raw = [TerrainLayerRaw::default(); TERRAIN_LAYERS];
    for (slot, layer) in raw.iter_mut().zip(&layers.layers) {
        let tint = linear_color(layer.color);
        let debug = linear_color(layer.debug_color);
        // The shader multiplies these by the layer's maps (white without
        // one), so with a map they scale it: 1 is the map as it is.
        let (metalness, roughness) = (layer.metalness, layer.roughness);
        let (strength, green) = match &layer.normal {
            Some(map) => (map.scale, if map.flip_green { -1.0 } else { 1.0 }),
            None => (0.0, 1.0),
        };
        *slot = TerrainLayerRaw {
            tint_tile: [tint.x, tint.y, tint.z, layer.tile_size.max(0.01)],
            surface: [
                metalness.clamp(0.0, 1.0),
                roughness.clamp(0.0, 1.0),
                strength,
                green,
            ],
            options: [
                layer.rotation_degrees.to_radians(),
                f32::from(layer.triplanar),
                layer.anti_tiling.clamp(0.0, 1.0),
                layer.height_blend.clamp(0.0, 1.0),
            ],
            debug: [debug.x, debug.y, debug.z, 1.0],
        };
    }
    let view = match layers.view {
        TerrainView::Shaded => 0.0,
        TerrainView::Height => 1.0,
        TerrainView::Slope => 2.0,
        TerrainView::Weights => 3.0,
        TerrainView::Grid => 4.0,
    };
    TerrainUniform {
        layers: raw,
        info: [
            layers.layers.len().min(TERRAIN_LAYERS) as f32,
            view,
            layers.height_range[0],
            layers.height_range[1],
        ],
        grid: [
            layers.grid_origin[0],
            layers.grid_origin[1],
            layers.grid_cell[0],
            layers.grid_cell[1],
        ],
    }
}

/// The three arrays for `layers`, each slice filled from a layer's maps
/// (or a neutral texel where it has none), halved `skipped_levels` times.
fn create_arrays(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &TerrainLayers,
    bytes: &GpuBytes,
    skipped_levels: u32,
) -> TerrainArrays {
    let layers: Vec<_> = layers.layers.iter().take(TERRAIN_LAYERS).collect();
    let largest = layers
        .iter()
        .flat_map(|layer| {
            [
                layer.base_color_texture.as_ref(),
                layer.normal.as_ref().map(|map| &map.texture),
                layer.roughness_map.as_ref().map(|map| &map.texture),
                layer.metalness_map.as_ref().map(|map| &map.texture),
                layer.occlusion.as_ref().map(|map| &map.texture),
                layer.height.as_ref().map(|map| &map.texture),
            ]
        })
        .flatten()
        .map(|texture| {
            let (width, height) = texture.size();
            width.max(height)
        })
        .max()
        .unwrap_or(4);
    let max = device.limits().max_texture_dimension_2d.min(MAX_LAYER_SIZE);
    let size = (largest.clamp(4, max).next_power_of_two().min(max) >> skipped_levels).max(4);
    let slices = layers.len().max(1);
    let mut albedo = Vec::with_capacity(slices);
    let mut normal = Vec::with_capacity(slices);
    let mut surface = Vec::with_capacity(slices);
    for layer in &layers {
        albedo.push(match &layer.base_color_texture {
            Some(texture) => resized(texture.as_ref(), size),
            None => vec![255; (size * size * 4) as usize],
        });
        normal.push(match &layer.normal {
            Some(map) => resized(map.texture.as_ref(), size),
            None => [128, 128, 255, 255].repeat((size * size) as usize),
        });
        let channel = |map: &Option<ChannelMap>, default: u8| match map {
            Some(map) => channel_of(map, size),
            None => vec![default; (size * size) as usize],
        };
        let (rough, metal) = (
            channel(&layer.roughness_map, 255),
            channel(&layer.metalness_map, 255),
        );
        let (occlusion, height) = (channel(&layer.occlusion, 255), channel(&layer.height, 128));
        surface.push(
            (0..(size * size) as usize)
                .flat_map(|i| [rough[i], metal[i], occlusion[i], height[i]])
                .collect(),
        );
    }
    if layers.is_empty() {
        albedo.push(vec![255; (size * size * 4) as usize]);
        normal.push([128, 128, 255, 255].repeat((size * size) as usize));
        surface.push([255, 255, 255, 128].repeat((size * size) as usize));
    }
    let (albedo_texture, albedo, albedo_bytes) = create_array(device, queue, size, &albedo, true);
    let (normal_texture, normal, normal_bytes) = create_array(device, queue, size, &normal, false);
    let (surface_texture, surface, surface_bytes) =
        create_array(device, queue, size, &surface, false);
    TerrainArrays {
        _textures: vec![albedo_texture, normal_texture, surface_texture],
        _bytes: bytes.count(albedo_bytes + normal_bytes + surface_bytes),
        albedo,
        normal,
        surface,
    }
}

/// An array texture of `slices` (each `size` × `size` RGBA), with mipmaps,
/// viewed as sRGB colors or plain values, and its size in bytes.
fn create_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    slices: &[Vec<u8>],
    srgb: bool,
) -> (wgpu::Texture, wgpu::TextureView, u64) {
    let levels: Vec<_> = slices
        .iter()
        .map(|pixels| mip_levels(size, size, pixels))
        .collect();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain layers"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: slices.len() as u32,
        },
        mip_level_count: levels[0].len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[TEXTURE_SRGB_FORMAT],
    });
    for (slice, mips) in levels.iter().enumerate() {
        for (level, (width, height, pixels)) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: slice as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(*height),
                },
                wgpu::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: srgb.then_some(TEXTURE_SRGB_FORMAT),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let bytes = levels
        .iter()
        .flatten()
        .map(|(_, _, pixels)| pixels.len() as u64)
        .sum();
    (texture, view, bytes)
}

/// `texture` scaled to `size` × `size` by blending the nearest four texels.
/// A broken texture (the wrong number of bytes) gives white.
pub(super) fn resized(texture: &dyn TextureSource, size: u32) -> Vec<u8> {
    let (width, height) = texture.size();
    let rgba = texture.rgba();
    let count = (size * size * 4) as usize;
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
        return vec![255; count];
    }
    if (width, height) == (size, size) {
        return rgba.to_vec();
    }
    let mut out = Vec::with_capacity(count);
    let at = |x: u32, y: u32, c: usize| rgba[((y * width + x) * 4) as usize + c] as f32;
    for y in 0..size {
        let fy =
            ((y as f32 + 0.5) * height as f32 / size as f32 - 0.5).clamp(0.0, (height - 1) as f32);
        let (y0, ty) = (fy.floor() as u32, fy.fract());
        let y1 = (y0 + 1).min(height - 1);
        for x in 0..size {
            let fx = ((x as f32 + 0.5) * width as f32 / size as f32 - 0.5)
                .clamp(0.0, (width - 1) as f32);
            let (x0, tx) = (fx.floor() as u32, fx.fract());
            let x1 = (x0 + 1).min(width - 1);
            for c in 0..4 {
                let top = at(x0, y0, c) + (at(x1, y0, c) - at(x0, y0, c)) * tx;
                let bottom = at(x0, y1, c) + (at(x1, y1, c) - at(x0, y1, c)) * tx;
                out.push((top + (bottom - top) * ty).round() as u8);
            }
        }
    }
    out
}

/// One channel of a map, resized, inverted if it says so.
fn channel_of(map: &ChannelMap, size: u32) -> Vec<u8> {
    let channel = map.channel as usize;
    resized(map.texture.as_ref(), size)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|texel| {
            let value = texel[channel];
            if map.invert { 255 - value } else { value }
        })
        .collect()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::draw_list::{Channel, TerrainLayer};

    struct Checker(u64);

    impl TextureSource for Checker {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (2, 2)
        }
        fn rgba(&self) -> &[u8] {
            &[
                0, 10, 20, 255, 200, 10, 20, 255, 200, 10, 20, 255, 0, 10, 20, 255,
            ]
        }
    }

    pub(crate) fn layer() -> TerrainLayer {
        TerrainLayer {
            color: [1.0; 3],
            base_color_texture: None,
            normal: None,
            metalness_map: None,
            roughness_map: None,
            occlusion: None,
            height: None,
            metalness: 0.0,
            roughness: 0.8,
            tile_size: 8.0,
            rotation_degrees: 0.0,
            triplanar: false,
            anti_tiling: 0.0,
            height_blend: 0.0,
            debug_color: [1.0, 0.0, 0.0],
        }
    }

    #[test]
    fn resizing_keeps_corners_and_blends_between() {
        let big = resized(&Checker(1), 4);
        assert_eq!(big.len(), 64);
        assert_eq!(&big[0..4], &[0, 10, 20, 255]);
        assert_eq!(&big[12..16], &[200, 10, 20, 255]);
        // Between a dark and a light texel: in between.
        assert!(big[4] > 0 && big[4] < 200);
        assert_eq!(resized(&Checker(1), 2), Checker(1).rgba());
    }

    #[test]
    fn channel_maps_pick_and_invert_their_channel() {
        let map = ChannelMap {
            texture: Arc::new(Checker(1)),
            channel: Channel::Red,
            invert: true,
        };
        assert_eq!(channel_of(&map, 2), vec![255, 55, 55, 255]);
    }

    #[test]
    fn colors_and_tiling_dont_change_the_arrays_key_but_textures_do() {
        let mut layers = TerrainLayers {
            id: 1,
            key: 1,
            layers: vec![layer()],
            view: TerrainView::Shaded,
            height_range: [0.0, 1.0],
            grid_origin: [0.0; 2],
            grid_cell: [1.0; 2],
        };
        let before = arrays_key(&layers);
        layers.layers[0].color = [0.2, 0.5, 0.1];
        layers.layers[0].tile_size = 3.0;
        assert_eq!(arrays_key(&layers), before);
        layers.layers[0].base_color_texture = Some(Arc::new(Checker(7)));
        assert_ne!(arrays_key(&layers), before);
        // With a map, the layer's value scales it (the shader multiplies).
        layers.layers[0].roughness_map = Some(ChannelMap {
            texture: Arc::new(Checker(8)),
            channel: Channel::Green,
            invert: false,
        });
        layers.layers[0].roughness = 0.6;
        let raw = uniform(&layers);
        assert_eq!(raw.layers[0].surface[1], 0.6);
        assert_eq!(raw.info[0], 1.0);
        assert_eq!(raw.layers[0].tint_tile[3], 3.0);
    }
}
