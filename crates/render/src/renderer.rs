//! The wgpu side: GPU setup, the window surface, buffers and drawing.
//!
//! `renderer.rs` owns the GPU types and draws a frame. `lighting.rs` plans
//! uniforms, tiles, shadows and batches. `post.rs` blooms and tone-maps.
//! `ui_pass.rs` builds screen-space quads and the UI and sky pipelines.
//! `gpu.rs` uploads meshes, textures and materials. `sprites.rs` draws particles.
//! `terrain.rs` draws terrain chunks with their blended layers.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::Range;

use crate::draw_list::{DrawList, Shape, TextAlign, TextureSource, UiDrawItem, UiText};
use crate::mesh::{self};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Cache as GlyphCache, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer as GlyphTextRenderer,
    Viewport as TextViewport,
};

mod gpu;
mod lighting;
mod post;
mod sprites;
mod terrain;
mod ui_pass;

#[cfg(test)]
use crate::draw_list::{ChannelMap, MeshSource, UiRectangle};
#[cfg(test)]
use glam::Vec3;
#[cfg(test)]
use gpu::mip_levels;
use gpu::{
    create_depth_view, create_frame_bind_group, create_instance_buffer, create_material,
    create_pipeline, create_shadow_pipeline, create_shadow_views, create_target_texture,
    create_texture, drawable_size, shader_module, update_custom_mesh, upload_custom_mesh,
    upload_mesh, upload_texture,
};
#[cfg(test)]
use lighting::{cascade_ends, light_raw};
use lighting::{
    frame_uniform, linear_color, maps_uniform_of, material_key, material_textures, plan_draws,
    plan_lights, post_uniform, view_projection,
};
#[cfg(test)]
use std::sync::Arc;
use ui_pass::{
    create_sky_pipeline, create_ui_image_pipeline, create_ui_pipeline, glyph_color, image_quad,
    ui_vertices, vertical_offset,
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// A [`TextureRenderer`]'s texture, as shown (see `create_target_texture`).
const TARGET_SHOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// A [`TextureRenderer`]'s texture, as drawn into.
const TARGET_DRAW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Textures are stored as plain bytes. Color textures are read through an
/// sRGB view of them ([`TEXTURE_SRGB_FORMAT`]), so sampling gives linear light.
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const TEXTURE_SRGB_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 5000.0;
/// A caller's mesh or texture not drawn for this many frames is taken off
/// the GPU (about 10 seconds at 60 fps). Drawn again, it's uploaded again.
const KEEP_UNDRAWN_FRAMES: u64 = 600;

/// Per-frame values for the shader. Must match `Frame` in shader.wgsl.
/// Every field is 16 bytes or a multiple, which is what uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    camera_position: [f32; 4],
    sun_direction: [f32; 4],
    sun_radiance: [f32; 4],
    ambient: [f32; 4],
    /// Turns world space to the sun's view of each shadow cascade.
    cascades: [[[f32; 4]; 4]; SHADOW_CASCADES],
    /// The world size of a shadow texel in each cascade.
    cascade_texels: [f32; 4],
    /// How many studs each cascade's depth range covers.
    cascade_depths: [f32; 4],
    /// 1 if shadows are on, how wide the sun looks (for soft edges), how
    /// many cascades there are, and how far from the camera shadows reach.
    shadow: [f32; 4],
    /// Turns clip space back to world space, to find the sky's direction.
    inv_view_proj: [[f32; 4]; 4],
    /// The sky's colors in linear light. The top color's fourth number is
    /// the environment intensity.
    sky_top: [f32; 4],
    sky_horizon: [f32; 4],
    sky_ground: [f32; 4],
    /// How many lights there are, the size of a light tile in pixels, and
    /// how many tiles are across the screen.
    lights: [u32; 4],
}

/// The most point and spot lights drawn at once.
const MAX_LIGHTS: usize = 1024;
/// The most lights one screen tile lists. Must match the shader.
const LIGHTS_PER_TILE: usize = 64;
/// Most screen tiles there can be. Tiles grow bigger on huge screens to stay under this.
const MAX_TILES: usize = 9216;
const MIN_TILE_SIZE: u32 = 32;
/// Turns a light's brightness into the shader's units, so a brightness of 1
/// is a modest lamp next to the sun's 3.
const LIGHT_SCALE: f32 = 20.0;
/// The scene is drawn in high dynamic range, into a texture that can hold
/// light brighter than 1. Post-processing then brings it down to the screen.
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The most blur levels bloom uses, each half the size of the one before.
const MAX_BLOOM_LEVELS: usize = 5;

/// One point or spot light as the GPU reads it. Must match `Light` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct LightRaw {
    /// Position, and range in the fourth number.
    position_range: [f32; 4],
    /// Linear color times brightness, and the cosine of the cone's edge in
    /// the fourth number (-2 for a point light).
    radiance_cone: [f32; 4],
    /// Where a spot light points, and the cosine where its edge starts to fade.
    direction_inner: [f32; 4],
}

/// Post-processing settings for the shader. Must match `Post` in post.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    /// Bloom threshold, bloom intensity and exposure.
    values: [f32; 4],
    /// 1 if bloom is on, 1 if tone mapping is on.
    flags: [f32; 4],
}

/// The size of each shadow cascade, in texels per side.
const SHADOW_MAP_SIZE: u32 = 2048;
/// How many shadow maps cover the view, each further out and coarser than
/// the one before. Must match the size of `cascades` in shader.wgsl.
const SHADOW_CASCADES: usize = 3;
/// How the cascades' far ends are spread between even steps (0) and steps
/// that grow by the same factor each time (1). Mostly the second, so the
/// nearest cascade is small and sharp.
const CASCADE_SPLIT_BLEND: f32 = 0.75;
/// How far behind the visible area (toward the sun) things still cast shadows into it.
const SHADOW_CASTER_RANGE: f32 = 500.0;
/// How wide the sun looks at `ShadowSoftness` 1: the width of a soft edge
/// per stud between the caster and the surface it shades.
const MAX_SUN_SIZE: f32 = 0.1;
/// Uniform buffer offsets must be multiples of this (wgpu's default limit).
const CASCADE_UNIFORM_STRIDE: u64 = 256;

/// The depth texture the sun's view is drawn into, one layer per cascade,
/// with the frame bindings that read it.
struct ShadowMap {
    /// One view per cascade, to draw into.
    layers: Vec<wgpu::TextureView>,
    /// The frame bindings with this map in place of the empty one.
    frame_bind_group: wgpu::BindGroup,
}

/// One draw item as the GPU reads it. Must match `InstanceIn` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    /// Turns surface directions to world space. Unlike `model`, it keeps them
    /// perpendicular to the surface when a shape is stretched.
    normal_matrix: [[f32; 3]; 3],
    base_color_opacity: [f32; 4],
    emissive: [f32; 3],
    /// Metalness, roughness and alpha cutoff.
    surface: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    position: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
    size: [f32; 2],
    border_color: [f32; 4],
    border: [f32; 2],
    color_end: [f32; 4],
}

/// A mesh uploaded to the GPU.
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    /// 16-bit for the built-in shapes, 32-bit for callers' meshes, which
    /// can have more than 65,536 corners.
    index_format: wgpu::IndexFormat,
    /// The caller's [`MeshSource::version`] these vertices are of.
    version: u64,
}

/// How a material's texture maps are read. Must match `Maps` in
/// shader.wgsl. Each field is 16 bytes, as uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MapsUniform {
    /// Which channel holds each map's value: 1 in that one, 0 in the others.
    metalness: [f32; 4],
    roughness: [f32; 4],
    occlusion: [f32; 4],
    opacity: [f32; 4],
    /// 1 to invert metalness, roughness, occlusion and opacity.
    invert: [f32; 4],
    /// Normal map scale (0: none), green's sign, occlusion strength (0:
    /// none), and 1 if opacity has its own map.
    settings: [f32; 4],
    /// 1 if there's a specular map, then unused.
    specular: [f32; 4],
}

/// A texture uploaded to the GPU, with a view for each way it's read.
struct GpuTexture {
    /// Kept so it lives as long as the bind groups that use it.
    _texture: wgpu::Texture,
    /// Converts sRGB to linear light, for colors.
    srgb: wgpu::TextureView,
    /// Gives the bytes as they are (128 is 0.5), for values.
    linear: wgpu::TextureView,
}

/// A material's textures and [`MapsUniform`], bound together for the shader.
struct GpuMaterial {
    bind_group: wgpu::BindGroup,
    /// Kept so it lives as long as the bind group.
    _maps: wgpu::Buffer,
}

/// The number of textures a material can have: base color, normal,
/// metalness, roughness, occlusion, emissive, opacity and specular.
const MATERIAL_TEXTURES: usize = 8;

/// Which material binding an item uses: its textures' keys and its
/// [`MapsUniform`] (as bits, so it can be compared and sorted). Items with
/// the same key share one binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MaterialKey {
    textures: [Option<u64>; MATERIAL_TEXTURES],
    maps: [u32; 28],
    /// A terrain chunk's binding (see `terrain::TerrainKey`), all 0 for
    /// everything else.
    terrain: terrain::TerrainKey,
}

/// Something of a caller's on the GPU, and the frame it was last drawn in.
/// `None` if its data was broken, so that's only reported once.
struct Cached<T> {
    gpu: Option<T>,
    last_drawn: u64,
}

/// Which mesh a batch draws: a built-in shape, or a caller's by its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MeshKey {
    Block,
    Sphere,
    Cylinder,
    Wedge,
    Custom(u64),
}

impl MeshKey {
    fn of(shape: &Shape) -> Self {
        match shape {
            Shape::Block => MeshKey::Block,
            Shape::Sphere => MeshKey::Sphere,
            Shape::Cylinder => MeshKey::Cylinder,
            Shape::Wedge => MeshKey::Wedge,
            Shape::Mesh(mesh) => MeshKey::Custom(mesh.key()),
        }
    }
}

/// One uploaded mesh per [`Shape`].
struct Meshes {
    block: GpuMesh,
    sphere: GpuMesh,
    cylinder: GpuMesh,
    wedge: GpuMesh,
}

impl Meshes {
    /// The built-in mesh for `key`, or `None` for a caller's mesh.
    fn get(&self, key: MeshKey) -> Option<&GpuMesh> {
        match key {
            MeshKey::Block => Some(&self.block),
            MeshKey::Sphere => Some(&self.sphere),
            MeshKey::Cylinder => Some(&self.cylinder),
            MeshKey::Wedge => Some(&self.wedge),
            MeshKey::Custom(_) => None,
        }
    }
}

/// Consecutive instances with the same mesh, the same material textures
/// and the same kind of drawing (solid or see-through), drawn with one
/// draw call.
#[derive(Clone, Debug, PartialEq)]
struct Batch {
    mesh: MeshKey,
    material: MaterialKey,
    transparent: bool,
    casts_shadow: bool,
    instances: Range<u32>,
}

/// Draws a scene into a window. The player uses this.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// True while the window is minimized (zero-sized), when there's nothing to draw.
    minimized: bool,
    scene: ScenePipeline,
}

/// Draws a scene into a texture instead of a window, using a GPU device that
/// someone else set up. The editor uses this for its viewport: egui owns the
/// window and shows the texture as an image.
pub struct TextureRenderer {
    scene: ScenePipeline,
    texture: wgpu::Texture,
    /// The texture seen as sRGB, so drawing converts linear light to sRGB
    /// the same way a window surface does.
    draw_view: wgpu::TextureView,
    /// The same texture seen as plain bytes, for others to show.
    view: wgpu::TextureView,
}

/// The bloom and tone mapping pipelines, which don't depend on the screen's size.
struct PostPasses {
    sampler: wgpu::Sampler,
    /// Texture, sampler and settings, for the bloom passes.
    layout: wgpu::BindGroupLayout,
    /// The same plus the bloom texture, for the final pass.
    composite_layout: wgpu::BindGroupLayout,
    settings: wgpu::Buffer,
    prefilter: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    /// Adds to what's already in the level it draws into.
    upsample: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
}

/// What post-processing draws into and reads, which depends on the screen's size.
struct PostTargets {
    /// Where the scene is drawn.
    hdr_view: wgpu::TextureView,
    /// Bloom levels from half the screen's size down, each half the one before.
    levels: Vec<wgpu::TextureView>,
    /// Scene to the first level.
    prefilter_group: wgpu::BindGroup,
    /// Level `i` is made from level `i - 1` (the entry for level 0 is unused).
    downsample_groups: Vec<wgpu::BindGroup>,
    /// Level `i` is added into level `i - 1`.
    upsample_groups: Vec<wgpu::BindGroup>,
    composite_group: wgpu::BindGroup,
}

/// The GPU objects for drawing a scene, whatever it's drawn into. Both
/// [`Renderer`] and [`TextureRenderer`] own one.
struct ScenePipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_view: wgpu::TextureView,
    sky_pipeline: wgpu::RenderPipeline,
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    /// Camera-facing pictures, such as particles.
    sprites: sprites::SpritePass,
    post: PostPasses,
    /// The scene's HDR texture and the bloom levels, remade at each resize.
    post_targets: PostTargets,
    light_buffer: wgpu::Buffer,
    tile_buffer: wgpu::Buffer,
    ui_pipeline: wgpu::RenderPipeline,
    ui_image_pipeline: wgpu::RenderPipeline,
    ui_image_layout: wgpu::BindGroupLayout,
    ui_sampler: wgpu::Sampler,
    /// One bind group per UI image texture. Dropped with the texture.
    ui_image_groups: HashMap<u64, wgpu::BindGroup>,
    ui_buffer: wgpu::Buffer,
    ui_capacity: usize,
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_viewport: TextViewport,
    text_atlas: TextAtlas,
    text_renderer: GlyphTextRenderer,
    frame_buffer: wgpu::Buffer,
    /// The frame bindings with an empty shadow map, for when shadows are off.
    frame_bind_group: wgpu::BindGroup,
    frame_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    shadow_pipeline: wgpu::RenderPipeline,
    /// Each cascade's view from the sun, [`CASCADE_UNIFORM_STRIDE`] apart.
    cascade_buffer: wgpu::Buffer,
    /// Gives the shadow pass one cascade's matrix (it can't also bind the
    /// shadow map it's drawing into).
    shadow_bind_group: wgpu::BindGroup,
    /// Made when shadows are first turned on, and freed when turned off.
    shadow_map: Option<ShadowMap>,
    meshes: Meshes,
    instance_buffer: wgpu::Buffer,
    /// How many instances fit in `instance_buffer` before it must grow.
    instance_capacity: usize,
    material_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One white pixel, for maps an item doesn't have: it changes nothing.
    white_texture: GpuTexture,
    /// For items with no textures, such as every built-in shape.
    plain_material: GpuMaterial,
    /// Callers' meshes and textures, by key, and the materials using them.
    custom_meshes: HashMap<u64, Cached<GpuMesh>>,
    textures: HashMap<u64, Cached<GpuTexture>>,
    materials: HashMap<MaterialKey, Cached<GpuMaterial>>,
    /// Terrain chunks' pipeline, layer textures and bindings.
    terrain: terrain::TerrainPass,
    /// How many frames have been drawn, to tell what wasn't drawn lately.
    frame: u64,
}

impl Renderer {
    /// Sets up the GPU to draw into `target`, which is usually a window.
    /// `width` and `height` are the target's size in physical pixels.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        // `Instance` picks a graphics API: DirectX 12 or Vulkan on Windows,
        // Metal on macOS, Vulkan on Linux.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(target)
            .context("could not create a drawing surface for the window")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible graphics card found")?;
        log::info!("using graphics adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("axcenity device"),
                ..Default::default()
            })
            .await
            .context("could not open the graphics device")?;

        let (width, height) = drawable_size(width, height).unwrap_or((1, 1));
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .context("the graphics card cannot draw to this window")?;
        // An sRGB surface converts the shader's linear colors for the screen.
        let formats = surface.get_capabilities(&adapter).formats;
        match formats.iter().find(|format| format.is_srgb()) {
            Some(&format) => config.format = format,
            None => log::warn!(
                "no sRGB surface format available ({formats:?}); colors will look too dark"
            ),
        }
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            minimized: false,
            scene: ScenePipeline::new(device, queue, config.format, width, height),
            config,
        })
    }

    /// Call when the window changes size. Sizes are in physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        let Some((width, height)) = drawable_size(width, height) else {
            self.minimized = true;
            return;
        };
        self.minimized = false;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.scene.device, &self.config);
        self.scene.resize(width, height);
    }

    /// Draws one frame. `before_present` runs just before the frame is shown;
    /// windowing libraries use that moment to pace frames (for example
    /// `winit`'s `Window::pre_present_notify`).
    pub fn render(&mut self, list: &DrawList, before_present: impl FnOnce()) -> Result<()> {
        if self.minimized {
            return Ok(());
        }

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            // The surface no longer matches the window (for example, it moved
            // to another monitor). Reconfigure and draw on the next frame.
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.scene.device, &self.config);
                return Ok(());
            }
            // The GPU took too long to hand over a frame. Skip this one.
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(err) => return Err(err).context("could not get the next frame to draw into"),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let aspect = self.config.width as f32 / self.config.height as f32;
        self.scene
            .draw(list, &view, aspect, (self.config.width, self.config.height))?;
        before_present();
        frame.present();
        Ok(())
    }
}

impl TextureRenderer {
    /// Sets up drawing into a `width` × `height` texture on an existing
    /// device. Sizes of zero are treated as 1, since a texture can't be empty.
    ///
    /// `wgpu::Device` and `wgpu::Queue` are cheap handles to shared GPU
    /// objects, so cloning them here doesn't copy anything big.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let scene = ScenePipeline::new(
            device.clone(),
            queue.clone(),
            TARGET_DRAW_FORMAT,
            width,
            height,
        );
        let (texture, draw_view, view) = create_target_texture(device, width, height);
        Self {
            scene,
            texture,
            draw_view,
            view,
        }
    }

    /// The texture's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }

    /// Replaces the texture with one of a new size, unless it's already that
    /// size. The old texture view stops being updated, so anything showing it
    /// (such as an egui image) must switch to [`TextureRenderer::view`] again.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if self.size() == (width, height) {
            return;
        }
        (self.texture, self.draw_view, self.view) =
            create_target_texture(&self.scene.device, width, height);
        self.scene.resize(width, height);
    }

    /// The finished picture, for showing it (for example as an egui image).
    ///
    /// Its format is `Rgba8Unorm` holding sRGB-encoded colors, the same as
    /// egui's own textures: sampling it gives the colors as they'd appear on
    /// screen, not linear light.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Draws one frame into the texture. The GPU runs it in order with
    /// anything submitted afterward, such as egui drawing the texture.
    pub fn render(&mut self, list: &DrawList) -> Result<()> {
        let (width, height) = self.size();
        self.scene.draw(
            list,
            &self.draw_view,
            width as f32 / height as f32,
            (width, height),
        )
    }

    /// Reads the texture back to the CPU as rows of RGBA bytes. Slow: for
    /// tests and screenshots.
    pub fn read_pixels(&self) -> Vec<u8> {
        let (width, height) = self.size();
        let device = &self.scene.device;
        // Each copied row must be a multiple of 256 bytes long.
        let row_bytes = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read back"),
            size: u64::from(row_bytes * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: None,
                },
            },
            self.texture.size(),
        );
        self.scene.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
            result.expect("could not read the texture back");
        });
        device.poll(wgpu::PollType::Wait).unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        mapped
            .chunks(row_bytes as usize)
            .flat_map(|row| &row[..(width * 4) as usize])
            .copied()
            .collect()
    }
}

impl ScenePipeline {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let depth_view = create_depth_view(&device, width, height);
        let storage_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                storage_entry(3),
                storage_entry(4),
            ],
        });
        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lights"),
            size: (MAX_LIGHTS * size_of::<LightRaw>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tile_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light tiles"),
            size: (MAX_TILES * (LIGHTS_PER_TILE + 1) * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow map"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let (empty_shadow_view, _) = create_shadow_views(&device, 1);
        // The bind group keeps the stand-in 1×1 map alive while shadows are off.
        let frame_bind_group = create_frame_bind_group(
            &device,
            &frame_layout,
            [&frame_buffer, &light_buffer, &tile_buffer],
            &empty_shadow_view,
            &shadow_sampler,
        );
        let cascade_size = size_of::<[[f32; 4]; 4]>() as u64;
        let cascade_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow cascades"),
            size: CASCADE_UNIFORM_STRIDE * SHADOW_CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Binding 5 matches `shadow_cascade` in shader.wgsl. Each pass picks
        // its cascade with an offset into the buffer (a "dynamic offset").
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow pass"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(cascade_size),
                },
                count: None,
            }],
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow pass"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &cascade_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(cascade_size),
                }),
            }],
        });
        let shadow_pipeline = create_shadow_pipeline(&device, &shadow_layout);

        // Bindings 0 and 3–9 are the textures (see `material_bindings`).
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut entries = vec![
            texture_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        entries.extend((3..=9).map(texture_entry));
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &entries,
        });
        // glTF's default: smooth, blending between mipmap levels, and
        // repeating past the edges.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material textures"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_texture = create_texture(&device, &queue, 1, 1, &[255; 4]);
        let plain_material = create_material(
            &device,
            &material_layout,
            &sampler,
            [&white_texture; MATERIAL_TEXTURES],
            maps_uniform_of(&MaterialKey::default()),
        );

        let scene_shader = shader_module(&device, "scene", gpu::SCENE_SHADER);
        let entry_points = ("vs_main", "fs_main");
        let layouts = [&frame_layout, &material_layout];
        let opaque_pipeline = create_pipeline(
            &device,
            &scene_shader,
            entry_points,
            &layouts,
            HDR_FORMAT,
            false,
        );
        let transparent_pipeline = create_pipeline(
            &device,
            &scene_shader,
            entry_points,
            &layouts,
            HDR_FORMAT,
            true,
        );
        let terrain = terrain::TerrainPass::new(&device, &frame_layout);
        let sky_pipeline = create_sky_pipeline(&device, &frame_layout);
        let sprites = sprites::SpritePass::new(&device, &white_texture.srgb);
        let post = PostPasses::new(&device, color_format);
        let post_targets = PostTargets::new(&device, &post, width, height);
        let ui_pipeline = create_ui_pipeline(&device, color_format);
        let (ui_image_layout, ui_sampler, ui_image_pipeline) =
            create_ui_image_pipeline(&device, color_format);
        let meshes = Meshes {
            block: upload_mesh(&device, "block", &mesh::block()),
            sphere: upload_mesh(&device, "sphere", &mesh::sphere()),
            cylinder: upload_mesh(&device, "cylinder", &mesh::cylinder()),
            wedge: upload_mesh(&device, "wedge", &mesh::wedge()),
        };
        let instance_capacity = 64;
        let instance_buffer = create_instance_buffer(&device, instance_capacity);
        let ui_capacity = 128;
        let ui_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI vertices"),
            size: (ui_capacity * size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(
            include_bytes!("../../../assets/fonts/Montserrat-Regular.ttf").to_vec(),
        );
        font_system.db_mut().load_font_data(
            include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf").to_vec(),
        );
        let swash_cache = SwashCache::new();
        let glyph_cache = GlyphCache::new(&device);
        let text_viewport = TextViewport::new(&device, &glyph_cache);
        let mut text_atlas = TextAtlas::new(&device, &queue, &glyph_cache, color_format);
        let text_renderer = GlyphTextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        Self {
            device,
            queue,
            depth_view,
            sky_pipeline,
            opaque_pipeline,
            transparent_pipeline,
            sprites,
            post,
            post_targets,
            light_buffer,
            tile_buffer,
            ui_pipeline,
            ui_image_pipeline,
            ui_image_layout,
            ui_sampler,
            ui_image_groups: HashMap::new(),
            ui_buffer,
            ui_capacity,
            font_system,
            swash_cache,
            text_viewport,
            text_atlas,
            text_renderer,
            frame_buffer,
            frame_bind_group,
            frame_layout,
            shadow_sampler,
            shadow_pipeline,
            cascade_buffer,
            shadow_bind_group,
            shadow_map: None,
            meshes,
            instance_buffer,
            instance_capacity,
            material_layout,
            sampler,
            white_texture,
            plain_material,
            custom_meshes: HashMap::new(),
            textures: HashMap::new(),
            materials: HashMap::new(),
            terrain,
            frame: 0,
        }
    }

    /// Call when the target changes size. Sizes are in pixels and never zero.
    fn resize(&mut self, width: u32, height: u32) {
        self.depth_view = create_depth_view(&self.device, width, height);
        self.post_targets = PostTargets::new(&self.device, &self.post, width, height);
    }

    /// Draws `list` into `view`, which must be the size last given to
    /// `new` or `resize`, and sends the work to the GPU.
    fn draw(
        &mut self,
        list: &DrawList,
        view: &wgpu::TextureView,
        aspect: f32,
        viewport_size: (u32, u32),
    ) -> Result<()> {
        let planned_lights = plan_lights(list, aspect, viewport_size);
        let mut frame_uniform = frame_uniform(list, aspect);
        frame_uniform.lights = [
            planned_lights.lights.len() as u32,
            planned_lights.tile_size,
            planned_lights.tiles_across,
            0,
        ];
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));
        for (index, cascade) in frame_uniform.cascades.iter().enumerate() {
            self.queue.write_buffer(
                &self.cascade_buffer,
                index as u64 * CASCADE_UNIFORM_STRIDE,
                bytemuck::bytes_of(cascade),
            );
        }
        if !planned_lights.lights.is_empty() {
            self.queue.write_buffer(
                &self.light_buffer,
                0,
                bytemuck::cast_slice(&planned_lights.lights),
            );
            self.queue.write_buffer(
                &self.tile_buffer,
                0,
                bytemuck::cast_slice(&planned_lights.tiles),
            );
        }
        self.queue.write_buffer(
            &self.post.settings,
            0,
            bytemuck::bytes_of(&post_uniform(&list.post)),
        );

        self.frame += 1;
        self.upload_new(list);
        let (instances, batches) = plan_draws(list);
        self.ensure_instance_capacity(instances.len());
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        let mut ui_vertices = ui_vertices(&list.ui, viewport_size);
        let rect_count = ui_vertices.len() as u32;
        let image_draws = self.ui_image_draws(&list.ui, viewport_size, &mut ui_vertices);
        self.ensure_ui_capacity(ui_vertices.len());
        if !ui_vertices.is_empty() {
            self.queue
                .write_buffer(&self.ui_buffer, 0, bytemuck::cast_slice(&ui_vertices));
        }

        let text_buffers = self.prepare_ui_text(&list.ui, viewport_size)?;
        let sprite_draws = {
            let textures = &self.textures;
            self.sprites.prepare(
                &self.device,
                &self.queue,
                list,
                view_projection(list, aspect),
                |key| {
                    textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .map(|gpu| &gpu.srgb)
                },
            )
        };

        // Shadow maps cost memory, so one exists only while shadows are on.
        let shadows_on = list.shadows.enabled;
        if shadows_on && self.shadow_map.is_none() {
            let (view, layers) = create_shadow_views(&self.device, SHADOW_MAP_SIZE);
            let frame_bind_group = create_frame_bind_group(
                &self.device,
                &self.frame_layout,
                [&self.frame_buffer, &self.light_buffer, &self.tile_buffer],
                &view,
                &self.shadow_sampler,
            );
            self.shadow_map = Some(ShadowMap {
                layers,
                frame_bind_group,
            });
        } else if !shadows_on {
            self.shadow_map = None;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // One pass per cascade, each drawing into its own layer of the map.
        let layers = self.shadow_map.iter().flat_map(|map| &map.layers);
        for (index, layer) in layers.enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            let offset = (index as u64 * CASCADE_UNIFORM_STRIDE) as u32;
            pass.set_bind_group(0, &self.shadow_bind_group, &[offset]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            // See-through items, and parts with `CastShadow` off, cast no shadow.
            for batch in batches
                .iter()
                .filter(|batch| !batch.transparent && batch.casts_shadow)
            {
                let Some(mesh) = self.gpu_mesh(batch.mesh) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
            }
        }
        let frame_bind_group = self
            .shadow_map
            .as_ref()
            .map_or(&self.frame_bind_group, |map| &map.frame_bind_group);

        // The scene is drawn into the HDR texture; post-processing then brings
        // it to `view`. The pass is a block so it ends (is dropped) before
        // `encoder.finish()`, which needs the encoder back.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post_targets.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            for batch in batches {
                // A caller's mesh with broken data was never uploaded.
                let Some(mesh) = self.gpu_mesh(batch.mesh) else {
                    continue;
                };
                if batch.material.terrain != terrain::TerrainKey::default() {
                    let Some(group) = self.terrain.group(&batch.material.terrain) else {
                        continue;
                    };
                    pass.set_pipeline(&self.terrain.pipeline);
                    pass.set_bind_group(1, group, &[]);
                } else {
                    let pipeline = if batch.transparent {
                        &self.transparent_pipeline
                    } else {
                        &self.opaque_pipeline
                    };
                    let material = self
                        .materials
                        .get(&batch.material)
                        .and_then(|cached| cached.gpu.as_ref())
                        .unwrap_or(&self.plain_material);
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(1, &material.bind_group, &[]);
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
            // Last, over everything, but hidden behind solid items.
            self.sprites.draw(&mut pass, &sprite_draws);
        }

        self.post.draw(
            &mut encoder,
            &self.post_targets,
            view,
            list.post.enabled && list.post.bloom_enabled && list.post.bloom_intensity > 0.0,
        );

        if !ui_vertices.is_empty() || !text_buffers.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            if rect_count > 0 {
                pass.set_pipeline(&self.ui_pipeline);
                pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
                pass.draw(0..rect_count, 0..1);
            }
            if !image_draws.is_empty() {
                pass.set_pipeline(&self.ui_image_pipeline);
                pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
                for (key, start, count) in &image_draws {
                    let Some(group) = self.ui_image_groups.get(key) else {
                        continue;
                    };
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(*start..*start + *count, 0..1);
                }
            }
            if !text_buffers.is_empty() {
                self.text_renderer
                    .render(&self.text_atlas, &self.text_viewport, &mut pass)
                    .context("could not render UI text")?;
            }
        }

        self.queue.submit([encoder.finish()]);
        self.forget_undrawn();
        Ok(())
    }

    /// The GPU copy of a mesh, or `None` for a caller's mesh that was never uploaded.
    fn gpu_mesh(&self, key: MeshKey) -> Option<&GpuMesh> {
        match key {
            MeshKey::Custom(key) => self
                .custom_meshes
                .get(&key)
                .and_then(|cached| cached.gpu.as_ref()),
            builtin => self.meshes.get(builtin),
        }
    }

    fn prepare_ui_text(
        &mut self,
        ui: &[UiDrawItem],
        (width, height): (u32, u32),
    ) -> Result<Vec<Buffer>> {
        self.text_viewport
            .update(&self.queue, Resolution { width, height });

        let text_items: Vec<&UiText> = ui
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text),
                UiDrawItem::Rectangle(_) | UiDrawItem::Image(_) => None,
            })
            .collect();
        let mut buffers = Vec::with_capacity(text_items.len());
        for text in &text_items {
            let font_size = text.font_size.max(1.0);
            let mut buffer = Buffer::new(
                &mut self.font_system,
                Metrics::new(font_size, font_size * 1.2),
            );
            buffer.set_size(
                &mut self.font_system,
                Some(text.size[0].max(0.0)),
                Some(text.size[1].max(0.0)),
            );
            buffer.set_text(
                &mut self.font_system,
                &text.text,
                &Attrs::new().family(Family::Name(&text.font_family)),
                Shaping::Advanced,
            );
            let align = match text.align {
                TextAlign::Left => None,
                TextAlign::Center => Some(glyphon::cosmic_text::Align::Center),
                TextAlign::Right => Some(glyphon::cosmic_text::Align::Right),
            };
            for line in &mut buffer.lines {
                line.set_align(align);
            }
            buffer.shape_until_scroll(&mut self.font_system, false);
            buffers.push(buffer);
        }

        if !buffers.is_empty() {
            let areas: Vec<_> = text_items
                .iter()
                .zip(&buffers)
                .map(|(text, buffer)| TextArea {
                    buffer,
                    left: text.position[0],
                    top: text.position[1] + vertical_offset(text, buffer),
                    scale: 1.0,
                    // Where the text may show, in screen pixels (not
                    // relative to the text's position).
                    bounds: TextBounds {
                        left: text.position[0].floor() as i32,
                        top: text.position[1].floor() as i32,
                        right: (text.position[0] + text.size[0].max(0.0)).ceil() as i32,
                        bottom: (text.position[1] + text.size[1].max(0.0)).ceil() as i32,
                    },
                    default_color: glyph_color(text.color),
                    custom_glyphs: &[],
                })
                .collect();
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.text_viewport,
                    areas,
                    &mut self.swash_cache,
                )
                .context("could not prepare UI text")?;
        }
        Ok(buffers)
    }

    /// Uploads the callers' meshes and textures in `list` that aren't on
    /// the GPU yet, and notes that the rest were drawn this frame.
    fn upload_new(&mut self, list: &DrawList) {
        let frame = self.frame;
        for item in &list.items {
            if let Shape::Mesh(source) = &item.shape {
                match self.custom_meshes.entry(source.key()) {
                    Entry::Occupied(mut cached) => {
                        let cached = cached.get_mut();
                        cached.last_drawn = frame;
                        // A posed (animated) mesh has new corners: rewrite them.
                        if let Some(gpu) = &mut cached.gpu
                            && gpu.version != source.version()
                        {
                            update_custom_mesh(&self.device, &self.queue, gpu, source.as_ref());
                        }
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(Cached {
                            gpu: upload_custom_mesh(&self.device, source.as_ref()),
                            last_drawn: frame,
                        });
                    }
                }
            }
            if let Some(surface) = &item.material.terrain {
                for weights in &surface.weights {
                    self.note_texture(weights.as_ref(), frame);
                }
                let weights = surface.weights.each_ref().map(|source| {
                    self.textures
                        .get(&source.key())
                        .and_then(|cached| cached.gpu.as_ref())
                });
                self.terrain.prepare(
                    &self.device,
                    &self.queue,
                    surface,
                    weights,
                    &self.white_texture,
                    frame,
                );
                continue;
            }
            let sources = material_textures(item);
            for source in sources.iter().flatten() {
                self.note_texture(source.as_ref(), frame);
            }
            let key = material_key(item);
            if key == MaterialKey::default() {
                continue;
            }
            match self.materials.entry(key) {
                Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                Entry::Vacant(slot) => {
                    // A texture with broken data is drawn as white instead.
                    let textures = sources.map(|source| {
                        source
                            .and_then(|source| self.textures.get(&source.key())?.gpu.as_ref())
                            .unwrap_or(&self.white_texture)
                    });
                    slot.insert(Cached {
                        gpu: Some(create_material(
                            &self.device,
                            &self.material_layout,
                            &self.sampler,
                            textures,
                            maps_uniform_of(&key),
                        )),
                        last_drawn: frame,
                    });
                }
            }
        }
        for item in &list.ui {
            if let UiDrawItem::Image(image) = item
                && let Some(source) = &image.texture
            {
                self.note_texture(source.as_ref(), frame);
            }
        }
        for sprite in &list.sprites {
            if let Some(source) = &sprite.texture {
                self.note_texture(source.as_ref(), frame);
            }
        }
    }

    /// Appends one quad per UI image and remembers which texture it uses.
    fn ui_image_draws(
        &mut self,
        ui: &[UiDrawItem],
        viewport: (u32, u32),
        vertices: &mut Vec<UiVertex>,
    ) -> Vec<(u64, u32, u32)> {
        let mut draws = Vec::new();
        for item in ui {
            let UiDrawItem::Image(image) = item else {
                continue;
            };
            let Some(source) = &image.texture else {
                continue;
            };
            let key = source.key();
            if self
                .textures
                .get(&key)
                .and_then(|cached| cached.gpu.as_ref())
                .is_none()
            {
                continue;
            }
            if !self.ui_image_groups.contains_key(&key) {
                let group = {
                    let gpu = self
                        .textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .expect("checked just above");
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("UI image"),
                        layout: &self.ui_image_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&gpu.srgb),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.ui_sampler),
                            },
                        ],
                    })
                };
                self.ui_image_groups.insert(key, group);
            }
            let quad = image_quad(image, viewport);
            if quad.is_empty() {
                continue;
            }
            let start = vertices.len() as u32;
            let count = quad.len() as u32;
            vertices.extend(quad);
            draws.push((key, start, count));
        }
        draws
    }

    /// Uploads `source` the first time, and marks it drawn this frame.
    fn note_texture(&mut self, source: &dyn TextureSource, frame: u64) {
        match self.textures.entry(source.key()) {
            Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
            Entry::Vacant(slot) => {
                slot.insert(Cached {
                    gpu: upload_texture(&self.device, &self.queue, source),
                    last_drawn: frame,
                });
            }
        }
    }

    /// Frees the callers' meshes, textures and materials that haven't been
    /// drawn lately.
    fn forget_undrawn(&mut self) {
        let frame = self.frame;
        let recent = |last_drawn: u64| frame - last_drawn < KEEP_UNDRAWN_FRAMES;
        self.custom_meshes
            .retain(|_, cached| recent(cached.last_drawn));
        self.textures.retain(|_, cached| recent(cached.last_drawn));
        self.ui_image_groups
            .retain(|key, _| self.textures.contains_key(key));
        self.sprites.forget(|key| self.textures.contains_key(&key));
        self.materials.retain(|_, cached| recent(cached.last_drawn));
        self.terrain.forget(frame);
    }

    /// Replaces the instance buffer with a bigger one if `count` won't fit.
    fn ensure_instance_capacity(&mut self, count: usize) {
        if count <= self.instance_capacity {
            return;
        }
        self.instance_capacity = count.next_power_of_two();
        self.instance_buffer = create_instance_buffer(&self.device, self.instance_capacity);
    }

    fn ensure_ui_capacity(&mut self, count: usize) {
        if count <= self.ui_capacity {
            return;
        }
        self.ui_capacity = count.next_power_of_two();
        self.ui_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI vertices"),
            size: (self.ui_capacity * size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::{
        AmbientLight, Camera, Channel, Cone, DrawItem, Light, Material, NormalMap, PostProcessing,
        Shadows, Sky, Sun,
    };
    use glam::Mat4;

    #[test]
    fn zero_sized_window_has_nothing_to_draw() {
        assert_eq!(drawable_size(0, 0), None);
        assert_eq!(drawable_size(0, 720), None);
        assert_eq!(drawable_size(1280, 0), None);
    }

    #[test]
    fn normal_window_keeps_its_size() {
        assert_eq!(drawable_size(1280, 720), Some((1280, 720)));
        assert_eq!(drawable_size(1, 1), Some((1, 1)));
    }

    #[test]
    fn srgb_to_linear_keeps_black_and_white_and_darkens_midtones() {
        assert_eq!(linear_color([0.0, 1.0, 0.5]).x, 0.0);
        assert!((linear_color([0.0, 1.0, 0.5]).y - 1.0).abs() < 1e-6);
        assert!((linear_color([0.0, 1.0, 0.5]).z - 0.214).abs() < 0.001);
    }

    #[test]
    fn ui_rectangles_keep_rounded_border_data_and_allow_border_only_shapes() {
        let item = UiDrawItem::Rectangle(UiRectangle {
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            color: [0.0, 0.0, 0.0, 0.0],
            border_color: [1.0, 0.0, 0.0, 1.0],
            border_width: 3.0,
            border_radius: 80.0,
            color_end: [0.0, 0.0, 0.0, 0.0],
        });

        let vertices = ui_vertices(&[item], (200, 100));
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].size, [100.0, 50.0]);
        assert_eq!(vertices[0].border, [25.0, 3.0]);
        assert_eq!(vertices[0].border_color[3], 1.0);
    }

    /// A list seen from the origin.
    fn list_of(items: Vec<DrawItem>) -> DrawList {
        DrawList {
            sky: Sky {
                top_color: [0.0; 3],
                horizon_color: [0.0; 3],
                ground_color: [0.0; 3],
                environment_intensity: 0.0,
            },
            camera: Camera {
                position: Vec3::ZERO,
                look_at: Vec3::NEG_Z,
                fov_y_degrees: 70.0,
            },
            sun: Sun {
                direction: Vec3::Y,
                color: [1.0; 3],
                intensity: 1.0,
            },
            ambient: AmbientLight {
                color: [1.0; 3],
                intensity: 0.0,
            },
            items,
            sprites: Vec::new(),
            shadows: Shadows::default(),
            lights: Vec::new(),
            post: PostProcessing {
                enabled: false,
                ..PostProcessing::default()
            },
            ui: Vec::new(),
        }
    }

    /// A unit-sized item at `position`.
    fn item(shape: Shape, position: Vec3, opacity: f32) -> DrawItem {
        DrawItem {
            shape,
            transform: Mat4::from_translation(position),
            material: Material {
                opacity,
                ..Material::default()
            },
            casts_shadow: true,
        }
    }

    fn batch(mesh: MeshKey, transparent: bool, instances: Range<u32>) -> Batch {
        Batch {
            mesh,
            material: MaterialKey::default(),
            transparent,
            casts_shadow: true,
            instances,
        }
    }

    /// A caller's mesh with no triangles, for planning tests (it's never uploaded).
    struct TestMesh(u64);

    impl MeshSource for TestMesh {
        fn key(&self) -> u64 {
            self.0
        }
        fn positions(&self) -> &[[f32; 3]] {
            &[]
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[]
        }
        fn indices(&self) -> &[u32] {
            &[]
        }
    }

    /// One white pixel.
    struct TestTexture(u64);

    impl TextureSource for TestTexture {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn rgba(&self) -> &[u8] {
            &[255; 4]
        }
    }

    fn textured(shape: Shape, texture: Option<u64>) -> DrawItem {
        let mut item = item(shape, Vec3::ZERO, 1.0);
        item.material.base_color_texture =
            texture.map(|key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>);
        item
    }

    #[test]
    fn callers_meshes_are_grouped_by_mesh_then_texture() {
        let mesh = |key| Shape::Mesh(Arc::new(TestMesh(key)));
        let list = list_of(vec![
            textured(mesh(7), Some(2)),
            textured(mesh(5), None),
            textured(mesh(7), Some(1)),
            textured(mesh(7), Some(2)),
            // Built-in shapes have no texture coordinates, so no texture.
            textured(Shape::Block, Some(1)),
        ]);
        let (_, batches) = plan_draws(&list);
        let textures: Vec<(MeshKey, Option<u64>, Range<u32>)> = batches
            .into_iter()
            .map(|batch| (batch.mesh, batch.material.textures[0], batch.instances))
            .collect();
        assert_eq!(
            textures,
            vec![
                (MeshKey::Block, None, 0..1),
                (MeshKey::Custom(5), None, 1..2),
                (MeshKey::Custom(7), Some(1), 2..3),
                (MeshKey::Custom(7), Some(2), 3..5),
            ]
        );
    }

    #[test]
    fn maps_and_their_settings_pick_the_material_binding() {
        let mesh = Shape::Mesh(Arc::new(TestMesh(1)));
        let texture = |key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>;
        let mut packed = textured(mesh.clone(), None);
        // One ORM texture: occlusion in red, roughness in green, metalness in blue.
        let channel = |channel| {
            Some(ChannelMap {
                texture: texture(9),
                channel,
                invert: false,
            })
        };
        packed.material.maps.occlusion = channel(Channel::Red);
        packed.material.maps.roughness = channel(Channel::Green);
        packed.material.maps.metalness = channel(Channel::Blue);
        let key = material_key(&packed);
        assert_eq!(
            key.textures,
            [None, None, Some(9), Some(9), Some(9), None, None, None]
        );
        let maps = maps_uniform_of(&key);
        assert_eq!(maps.metalness, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(maps.roughness, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(maps.settings, [0.0, 1.0, 1.0, 0.0]);

        // The same textures read another way need another binding.
        let mut glossy = packed.clone();
        glossy.material.maps.roughness.as_mut().unwrap().invert = true;
        assert_ne!(material_key(&glossy), key);
        assert_eq!(maps_uniform_of(&material_key(&glossy)).invert[1], 1.0);
        // Built-in shapes have no texture coordinates, so no maps.
        let mut block = packed.clone();
        block.shape = Shape::Block;
        assert_eq!(material_key(&block), MaterialKey::default());
    }

    #[test]
    fn mipmaps_halve_down_to_one_pixel_averaging_each_square() {
        // 3×2: an odd width, so the last column is averaged with itself.
        let rgba: Vec<u8> = [0, 100, 200, 40, 80, 120]
            .iter()
            .flat_map(|&v| [v, v, v, 255])
            .collect();
        let levels = mip_levels(3, 2, &rgba);
        let sizes: Vec<(u32, u32)> = levels.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(sizes, [(3, 2), (1, 1)]);
        // The top-left 2×2: (0 + 100 + 40 + 80) / 4 = 55.
        assert_eq!(levels[1].2[..4], [55, 55, 55, 255]);
    }

    #[test]
    fn flat_and_invisible_shapes_are_skipped() {
        let mut flat = item(Shape::Block, Vec3::ZERO, 1.0);
        flat.transform = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
        let list = list_of(vec![
            item(Shape::Block, Vec3::ZERO, 1.0),
            flat,
            item(Shape::Block, Vec3::ZERO, 0.0),
        ]);
        let (instances, batches) = plan_draws(&list);
        assert_eq!(instances.len(), 1);
        assert_eq!(batches, vec![batch(MeshKey::Block, false, 0..1)]);
    }

    #[test]
    fn solid_items_are_grouped_by_shape() {
        let list = list_of(vec![
            item(Shape::Wedge, Vec3::ZERO, 1.0),
            item(Shape::Block, Vec3::ZERO, 1.0),
            item(Shape::Wedge, Vec3::ZERO, 1.0),
            item(Shape::Block, Vec3::ZERO, 1.0),
        ]);
        let (_, batches) = plan_draws(&list);
        assert_eq!(
            batches,
            vec![
                batch(MeshKey::Block, false, 0..2),
                batch(MeshKey::Wedge, false, 2..4)
            ]
        );
    }

    #[test]
    fn see_through_items_come_last_and_farthest_first() {
        let near_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -2.0), 0.5);
        let far_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -9.0), 0.5);
        let middle_glass = item(Shape::Block, Vec3::new(0.0, 0.0, -5.0), 0.5);
        let solid = item(Shape::Sphere, Vec3::new(0.0, 0.0, -1.0), 1.0);
        let list = list_of(vec![near_glass, far_glass, solid, middle_glass]);

        let (instances, batches) = plan_draws(&list);
        assert_eq!(
            batches,
            vec![
                batch(MeshKey::Sphere, false, 0..1),
                batch(MeshKey::Sphere, true, 1..2),
                batch(MeshKey::Block, true, 2..3),
                batch(MeshKey::Sphere, true, 3..4),
            ]
        );
        let z = |i: usize| instances[i].model[3][2];
        assert_eq!([z(1), z(2), z(3)], [-9.0, -5.0, -2.0]);
    }

    /// A GPU device for tests, or `None` on machines without one (some CI
    /// runners have no graphics adapter at all).
    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    fn point_light(position: Vec3, range: f32) -> Light {
        Light {
            position,
            color: [1.0; 3],
            brightness: 1.0,
            range,
            cone: None,
        }
    }

    #[test]
    fn lights_are_listed_only_in_the_tiles_they_can_reach() {
        let mut list = list_of(vec![]);
        // Straight ahead and small: the middle tile only.
        list.lights
            .push(point_light(Vec3::new(0.0, 0.0, -50.0), 1.0));
        // Behind the camera: not drawn at all.
        list.lights
            .push(point_light(Vec3::new(0.0, 0.0, 50.0), 1.0));
        // Not a real light.
        list.lights.push(point_light(Vec3::ZERO, 0.0));
        let planned = plan_lights(&list, 1.0, (320, 320));
        assert_eq!(planned.lights.len(), 1);
        assert_eq!((planned.tile_size, planned.tiles_across), (32, 10));
        let count = |x: usize, y: usize| planned.tiles[(y * 10 + x) * (LIGHTS_PER_TILE + 1)];
        assert_eq!(count(5, 5), 1);
        assert_eq!(count(0, 0), 0);
        assert_eq!(count(9, 9), 0);
    }

    #[test]
    fn a_light_around_the_camera_reaches_every_tile_and_the_nearest_are_kept() {
        let mut list = list_of(vec![]);
        list.lights
            .push(point_light(Vec3::new(0.0, 0.0, 0.1), 100.0));
        let planned = plan_lights(&list, 1.0, (64, 64));
        assert!(
            planned
                .tiles
                .chunks(LIGHTS_PER_TILE + 1)
                .all(|tile| tile[0] == 1)
        );

        list.lights = (0..MAX_LIGHTS + 10)
            .map(|i| point_light(Vec3::new(0.0, 0.0, -1.0 - i as f32), 5.0))
            .collect();
        let planned = plan_lights(&list, 1.0, (64, 64));
        assert_eq!(planned.lights.len(), MAX_LIGHTS);
        let farthest = planned
            .lights
            .iter()
            .map(|l| l.position_range[2])
            .fold(0.0, f32::min);
        assert_eq!(farthest, -(MAX_LIGHTS as f32));
    }

    #[test]
    fn spot_lights_describe_their_cone() {
        let raw = light_raw(&Light {
            cone: Some(Cone {
                direction: Vec3::new(0.0, -2.0, 0.0),
                angle_degrees: 90.0,
            }),
            ..point_light(Vec3::ZERO, 10.0)
        });
        assert_eq!(raw.direction_inner[..3], [0.0, -1.0, 0.0]);
        assert!((raw.radiance_cone[3] - 45f32.to_radians().cos()).abs() < 1e-6);
        assert!(raw.direction_inner[3] > raw.radiance_cone[3]);
        assert_eq!(
            light_raw(&point_light(Vec3::ZERO, 1.0)).radiance_cone[3],
            -2.0
        );
    }

    /// The middle pixel of a `size` × `size` texture drawn from `list`.
    fn middle_pixel(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> [u8; 4] {
        let mut renderer = TextureRenderer::new(device, queue, 64, 64);
        renderer.render(list).unwrap();
        let pixels = renderer.read_pixels();
        pixels[(32 * 64 + 32) * 4..][..4].try_into().unwrap()
    }

    fn big_block() -> DrawItem {
        DrawItem {
            shape: Shape::Block,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -5.0),
            ),
            material: Material::default(),
            casts_shadow: true,
        }
    }

    #[test]
    fn parts_shadow_others_unless_they_dont_cast_shadows() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let block = |size: Vec3, position: Vec3| DrawItem {
            transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
            ..big_block()
        };
        // A wall facing the camera, lit from the front right. The small
        // block's shadow falls on the middle of the wall, beside the block.
        let wall = block(Vec3::new(30.0, 30.0, 1.0), Vec3::new(0.0, 0.0, -10.0));
        let blocker = block(Vec3::splat(2.0), Vec3::new(4.0, 0.0, -6.0));
        let mut list = list_of(vec![wall, blocker]);
        list.sun.direction = Vec3::new(1.0, 0.0, 1.0);
        list.shadows.softness = 0.0;

        let shaded = middle_pixel(&device, &queue, &list);
        list.items[1].casts_shadow = false;
        let lit = middle_pixel(&device, &queue, &list);
        assert!(
            lit[0] > shaded[0] + 40,
            "the block's shadow is gone: {shaded:?} vs {lit:?}"
        );

        list.items[1].casts_shadow = true;
        list.shadows.enabled = false;
        assert_eq!(middle_pixel(&device, &queue, &list), lit, "shadows off");
    }

    #[test]
    fn cascades_end_further_out_each_time_and_the_last_at_the_distance() {
        let ends = cascade_ends(150.0);
        assert!(ends[0] > 1.0 && ends[0] < ends[1] && ends[1] < ends[2]);
        assert!((ends[SHADOW_CASCADES - 1] - 150.0).abs() < 1e-3);
        // The nearest cascade is small, so it's the sharpest.
        assert!(ends[0] < 25.0, "{ends:?}");
    }

    #[test]
    fn a_point_light_lights_the_part_in_front_of_it() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut list = list_of(vec![big_block()]);
        list.sun.intensity = 0.0;
        let dark = middle_pixel(&device, &queue, &list);
        assert_eq!(dark[..3], [0, 0, 0], "no light, no color");

        list.lights
            .push(point_light(Vec3::new(0.0, 0.0, -1.0), 10.0));
        let lit = middle_pixel(&device, &queue, &list);
        assert!(lit[0] > 40, "the front face is lit: {lit:?}");

        // Out of reach, the light changes nothing.
        list.lights[0].range = 1.0;
        assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
    }

    #[test]
    fn a_spot_light_lights_only_inside_its_cone() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut list = list_of(vec![big_block()]);
        list.sun.intensity = 0.0;
        let mut light = point_light(Vec3::new(0.0, 0.0, -1.0), 10.0);
        light.cone = Some(Cone {
            direction: Vec3::NEG_Z,
            angle_degrees: 40.0,
        });
        list.lights.push(light);
        assert!(
            middle_pixel(&device, &queue, &list)[0] > 40,
            "inside the beam"
        );

        list.lights[0].cone = Some(Cone {
            direction: Vec3::X,
            angle_degrees: 40.0,
        });
        assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
    }

    #[test]
    fn a_metal_part_reflects_the_sky() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut block = big_block();
        block.material = Material {
            base_color: [1.0; 3],
            metalness: 1.0,
            roughness: 0.1,
            ..Material::default()
        };
        let mut list = list_of(vec![block]);
        list.sun.intensity = 0.0;
        assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
        list.sky = Sky::default();
        let reflecting = middle_pixel(&device, &queue, &list);
        assert!(
            reflecting[2] > 60,
            "the block shows the sky: {reflecting:?}"
        );
    }

    fn red_sprite(z: f32, light_emission: f32) -> crate::Sprite {
        crate::Sprite {
            position: Vec3::new(0.0, 0.0, z),
            size: 2.0,
            rotation_degrees: 0.0,
            color: [1.0, 0.0, 0.0],
            opacity: 1.0,
            brightness: 1.0,
            light_emission,
            texture: None,
        }
    }

    #[test]
    fn sprites_show_in_front_and_hide_behind_solid_items() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut list = list_of(Vec::new());
        list.sprites = vec![red_sprite(-3.0, 0.0)];
        let shown = middle_pixel(&device, &queue, &list);
        assert!(shown[0] > 200 && shown[1] < 40, "{shown:?}");

        // Additive: black behind plus red light is still red.
        list.sprites = vec![red_sprite(-3.0, 1.0)];
        let added = middle_pixel(&device, &queue, &list);
        assert!(added[0] > 200, "{added:?}");

        // Behind a solid block, it's hidden.
        list.items = vec![big_block()];
        list.sprites = vec![red_sprite(-9.0, 0.0)];
        let block_only = {
            let mut plain = list_of(vec![big_block()]);
            plain.sprites.clear();
            middle_pixel(&device, &queue, &plain)
        };
        assert_eq!(middle_pixel(&device, &queue, &list), block_only);
    }

    #[test]
    fn bloom_makes_bright_parts_glow_past_their_edges() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let glowing = DrawItem {
            material: Material {
                base_color: [0.0; 3],
                emissive: [4.0, 0.5, 0.5],
                ..Material::default()
            },
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(2.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -5.0),
            ),
            ..big_block()
        };
        let mut list = list_of(vec![glowing]);
        // The block covers pixels 21 to 43 across; pixel 14 is in the sky
        // just outside it.
        let outside = |list: &DrawList| {
            let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
            renderer.render(list).unwrap();
            renderer.read_pixels()[(32 * 64 + 14) * 4..][..4].to_vec()
        };
        list.post.enabled = true;
        list.post.bloom_enabled = false;
        let without = outside(&list);
        list.post.bloom_enabled = true;
        let with = outside(&list);
        assert!(
            with[0] > without[0],
            "bloom adds glow: {without:?} vs {with:?}"
        );

        // Tone mapping keeps the block's bright color from clipping to flat white.
        let middle = middle_pixel(&device, &queue, &list);
        assert!(
            middle[0] > middle[1],
            "still red, not clipped white: {middle:?}"
        );
    }

    #[test]
    fn texture_renderer_draws_the_sky_and_a_part_into_a_texture() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        // Pure red is 255 whether or not the GPU sRGB-encodes the background
        // clear (some drivers don't when drawing through an sRGB view).
        let red = [255, 0, 0, 255];
        let mut list = list_of(vec![]);
        list.sky.top_color = [1.0, 0.0, 0.0];
        list.sky.horizon_color = [1.0, 0.0, 0.0];
        list.sky.ground_color = [1.0, 0.0, 0.0];
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 32);
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        assert_eq!(pixels.len(), 64 * 32 * 4);
        assert_eq!(pixels[..4], red);

        // A big glowing block straight ahead covers the middle of the view,
        // after resizing to a different shape. With no sun or ambient light,
        // its pixels are exactly its glow color.
        list.sun.intensity = 0.0;
        list.items.push(DrawItem {
            shape: Shape::Block,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -5.0),
            ),
            material: Material {
                base_color: [0.0; 3],
                emissive: [0.63, 0.8, 0.95],
                ..Material::default()
            },
            casts_shadow: true,
        });
        renderer.resize(64, 64);
        assert_eq!(renderer.size(), (64, 64));
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
        assert_eq!(pixel(0, 0), red, "the corner shows the sky");
        // The texture holds the sRGB color as picked (0.63 × 255 ≈ 161), the
        // way egui expects, not linear light (which would be about 90).
        assert_eq!(
            pixel(32, 32),
            [161, 204, 242, 255],
            "the middle shows the block"
        );
    }

    /// A square facing +Z, filling the unit cube's front, with texture
    /// coordinates and tangents.
    struct Square;

    impl MeshSource for Square {
        fn key(&self) -> u64 {
            1
        }
        fn positions(&self) -> &[[f32; 3]] {
            &[
                [-0.5, -0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0],
                [-0.5, 0.5, 0.0],
            ]
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[[0.0, 0.0, 1.0]; 4]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        }
        fn tangents(&self) -> &[[f32; 4]] {
            &[[1.0, 0.0, 0.0, 1.0]; 4]
        }
        fn indices(&self) -> &[u32] {
            &[0, 1, 2, 0, 2, 3]
        }
    }

    /// One pixel of `rgba`.
    struct Pixel(u64, [u8; 4]);

    impl TextureSource for Pixel {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn rgba(&self) -> &[u8] {
            &self.1
        }
    }

    #[test]
    fn texture_renderer_draws_a_mesh_with_texture_maps() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut list = list_of(vec![]);
        list.sun.intensity = 0.0;
        let pixel = |key, rgba| Arc::new(Pixel(key, rgba)) as Arc<dyn TextureSource>;
        let mut material = Material {
            base_color: [0.0; 3],
            emissive: [1.0; 3],
            ..Material::default()
        };
        // Glows only in the emissive map's green; the other maps are bound
        // but change nothing without light.
        material.maps.emissive = Some(pixel(2, [0, 255, 0, 255]));
        material.maps.normal = Some(NormalMap {
            texture: pixel(3, [128, 128, 255, 255]),
            scale: 1.0,
            flip_green: true,
        });
        material.maps.roughness = Some(ChannelMap {
            texture: pixel(4, [0, 0, 0, 255]),
            channel: Channel::Alpha,
            invert: true,
        });
        list.items.push(DrawItem {
            shape: Shape::Mesh(Arc::new(Square)),
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -3.0),
            ),
            material,
            casts_shadow: true,
        });
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        assert_eq!(pixels[(16 * 32 + 16) * 4..][..4], [0, 255, 0, 255]);
    }

    #[test]
    fn texture_renderer_never_makes_an_empty_texture() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 0, 0);
        assert_eq!(renderer.size(), (1, 1));
        renderer.resize(300, 0);
        assert_eq!(renderer.size(), (300, 1));
    }

    /// Flat ground 20 studs across, as a terrain chunk would be.
    struct Ground;

    impl MeshSource for Ground {
        fn key(&self) -> u64 {
            9001
        }
        fn positions(&self) -> &[[f32; 3]] {
            &[
                [-10.0, 0.0, -10.0],
                [-10.0, 0.0, 10.0],
                [10.0, 0.0, -10.0],
                [10.0, 0.0, 10.0],
            ]
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[[0.0, 1.0, 0.0]; 4]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]
        }
        fn indices(&self) -> &[u32] {
            &[0, 1, 2, 1, 3, 2]
        }
    }

    /// The same weights everywhere.
    struct Weights(u64, [u8; 4]);

    impl TextureSource for Weights {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn rgba(&self) -> &[u8] {
            &self.1
        }
    }

    #[test]
    fn terrain_chunks_show_the_layer_their_weights_pick() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let layer = |color| crate::TerrainLayer {
            color,
            ..terrain::tests::layer()
        };
        let layers = Arc::new(crate::TerrainLayers {
            id: 1,
            key: 1,
            layers: vec![layer([1.0, 0.0, 0.0]), layer([0.0, 0.0, 1.0])],
            view: crate::TerrainView::Shaded,
            height_range: [0.0, 1.0],
            grid_origin: [-10.0, -10.0],
            grid_cell: [1.0, 1.0],
        });
        let ground = |weights: [u8; 4], key| {
            let mut list = list_of(vec![DrawItem {
                shape: Shape::Mesh(Arc::new(Ground)),
                transform: Mat4::from_translation(Vec3::new(0.0, -2.0, 0.0)),
                material: Material {
                    terrain: Some(crate::TerrainSurface {
                        layers: layers.clone(),
                        weights: [
                            Arc::new(Weights(key, weights)),
                            Arc::new(Weights(key + 1, [0; 4])),
                        ],
                    }),
                    ..Material::default()
                },
                casts_shadow: true,
            }]);
            // Looking straight down at it, in sunlight.
            list.camera.look_at = Vec3::new(0.0, -1.0, -0.001);
            list.ambient.intensity = 0.2;
            list
        };
        let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([255, 0, 0, 0], 10));
        // Some of the blue is the sun's white highlight.
        assert!(
            red > 100 && red > blue * 3,
            "the first layer: {red}, {blue}"
        );
        let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([0, 255, 0, 0], 20));
        assert!(
            blue > 100 && blue > red * 3,
            "the second layer: {red}, {blue}"
        );
        let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([128, 127, 0, 0], 30));
        assert!(red > 40 && blue > 40, "half and half: {red}, {blue}");
    }
}
