//! The wgpu side: GPU setup, the window surface, buffers and drawing.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::Range;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Mat4, Vec2, Vec3};
use glyphon::{
    Attrs, Buffer, Cache as GlyphCache, Color as GlyphColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer as GlyphTextRenderer, Viewport as TextViewport,
};
use wgpu::util::DeviceExt;

use crate::draw_list::{
    ChannelMap, DrawItem, DrawList, Light, MeshSource, PostProcessing, Shape, TextAlign,
    TextureSource, UiDrawItem, UiImage, UiRectangle, UiText,
};
use crate::mesh::{self, MeshData, Vertex};

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

        let opaque_pipeline =
            create_pipeline(&device, &frame_layout, &material_layout, HDR_FORMAT, false);
        let transparent_pipeline =
            create_pipeline(&device, &frame_layout, &material_layout, HDR_FORMAT, true);
        let sky_pipeline = create_sky_pipeline(&device, &frame_layout);
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
                let pipeline = if batch.transparent {
                    &self.transparent_pipeline
                } else {
                    &self.opaque_pipeline
                };
                // A caller's mesh with broken data was never uploaded.
                let Some(mesh) = self.gpu_mesh(batch.mesh) else {
                    continue;
                };
                let material = self
                    .materials
                    .get(&batch.material)
                    .and_then(|cached| cached.gpu.as_ref())
                    .unwrap_or(&self.plain_material);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
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
                    Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                    Entry::Vacant(slot) => {
                        slot.insert(Cached {
                            gpu: upload_custom_mesh(&self.device, source.as_ref()),
                            last_drawn: frame,
                        });
                    }
                }
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
        self.materials.retain(|_, cached| recent(cached.last_drawn));
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

fn view_projection(list: &DrawList, aspect: f32) -> Mat4 {
    let camera = &list.camera;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    // wgpu uses DirectX-style clip space: depth from 0 to 1, Y up.
    let projection = rh::proj::directx::perspective(
        camera.fov_y_degrees.to_radians(),
        aspect,
        NEAR_PLANE,
        FAR_PLANE,
    );
    projection * view
}

fn frame_uniform(list: &DrawList, aspect: f32) -> FrameUniform {
    let camera = &list.camera;
    let view_proj = view_projection(list, aspect);
    let sun_direction = list.sun.direction.normalize_or(Vec3::Y);
    let sun_radiance = linear_color(list.sun.color) * list.sun.intensity;
    let ambient = linear_color(list.ambient.color) * list.ambient.intensity;
    let shadows = shadow_setup(list, sun_direction, aspect);
    let sky = &list.sky;
    let environment = if sky.environment_intensity.is_finite() {
        sky.environment_intensity.max(0.0)
    } else {
        0.0
    };

    FrameUniform {
        view_proj: view_proj.to_cols_array_2d(),
        camera_position: camera.position.extend(1.0).to_array(),
        sun_direction: sun_direction.extend(0.0).to_array(),
        sun_radiance: sun_radiance.extend(0.0).to_array(),
        ambient: ambient.extend(0.0).to_array(),
        cascades: shadows.cascades.map(|cascade| cascade.to_cols_array_2d()),
        cascade_texels: shadows.texels,
        cascade_depths: shadows.depths,
        shadow: shadows.settings,
        inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        sky_top: linear_color(sky.top_color).extend(environment).to_array(),
        sky_horizon: linear_color(sky.horizon_color).extend(0.0).to_array(),
        sky_ground: linear_color(sky.ground_color).extend(0.0).to_array(),
        lights: [0; 4],
    }
}

/// The lights to draw, and which of them reach each tile of the screen.
struct PlannedLights {
    lights: Vec<LightRaw>,
    /// For each tile, row by row: how many lights reach it, then up to
    /// [`LIGHTS_PER_TILE`] indexes into `lights`.
    tiles: Vec<u32>,
    tile_size: u32,
    tiles_across: u32,
}

/// Picks the lights to draw (the nearest [`MAX_LIGHTS`]) and sorts them into
/// screen tiles, so each pixel only loops over the few lights that can
/// reach it instead of all of them. A light goes in every tile that its
/// sphere of reach could touch; tiles with more than [`LIGHTS_PER_TILE`]
/// keep the nearest ones to the camera.
fn plan_lights(list: &DrawList, aspect: f32, (width, height): (u32, u32)) -> PlannedLights {
    let camera = list.camera.position;
    let mut lights: Vec<&Light> = list
        .lights
        .iter()
        .filter(|light| {
            light.position.is_finite()
                && light.range.is_finite()
                && light.range > 0.0
                && light.brightness.is_finite()
                && light.brightness > 0.0
        })
        .collect();
    lights.sort_by(|a, b| {
        let (a, b) = (
            a.position.distance_squared(camera),
            b.position.distance_squared(camera),
        );
        a.total_cmp(&b)
    });
    lights.truncate(MAX_LIGHTS);

    let mut tile_size = MIN_TILE_SIZE;
    while (width.div_ceil(tile_size) * height.div_ceil(tile_size)) as usize > MAX_TILES {
        tile_size *= 2;
    }
    let (across, down) = (width.div_ceil(tile_size), height.div_ceil(tile_size));
    let mut planned = PlannedLights {
        lights: Vec::new(),
        tiles: Vec::new(),
        tile_size,
        tiles_across: across,
    };
    if lights.is_empty() || width == 0 || height == 0 {
        return planned;
    }

    let view_proj = view_projection(list, aspect);
    let stride = LIGHTS_PER_TILE + 1;
    planned.tiles = vec![0; (across * down) as usize * stride];
    for light in lights {
        let Some((first, last)) = light_tiles(view_proj, light, (width, height), tile_size) else {
            continue;
        };
        let index = planned.lights.len() as u32;
        planned.lights.push(light_raw(light));
        for y in first.1..=last.1 {
            for x in first.0..=last.0 {
                let tile = (y * across + x) as usize * stride;
                let count = planned.tiles[tile] as usize;
                if count < LIGHTS_PER_TILE {
                    planned.tiles[tile + 1 + count] = index;
                    planned.tiles[tile] += 1;
                }
            }
        }
    }
    planned
}

/// The first and last tile (column, row) a light's sphere of reach could
/// touch, or `None` if it's entirely out of view. Works from the sphere's
/// bounding box, which is slightly generous but never misses.
fn light_tiles(
    view_proj: Mat4,
    light: &Light,
    (width, height): (u32, u32),
    tile_size: u32,
) -> Option<((u32, u32), (u32, u32))> {
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    let mut behind = 0;
    for corner in 0..8 {
        let offset = Vec3::new(
            if corner & 1 == 0 { -1.0 } else { 1.0 },
            if corner & 2 == 0 { -1.0 } else { 1.0 },
            if corner & 4 == 0 { -1.0 } else { 1.0 },
        );
        let clip = view_proj * (light.position + offset * light.range).extend(1.0);
        if clip.w <= NEAR_PLANE * 0.5 {
            behind += 1;
            continue;
        }
        let ndc = Vec2::new(clip.x, clip.y) / clip.w;
        min = min.min(ndc);
        max = max.max(ndc);
    }
    if behind == 8 {
        return None;
    }
    // A box that reaches behind the camera could cover anywhere on screen.
    if behind > 0 {
        (min, max) = (Vec2::splat(-1.0), Vec2::splat(1.0));
    }
    if max.x < -1.0 || min.x > 1.0 || max.y < -1.0 || min.y > 1.0 {
        return None;
    }
    let to_pixels = |ndc: Vec2| {
        Vec2::new(
            (ndc.x * 0.5 + 0.5) * width as f32,
            (0.5 - ndc.y * 0.5) * height as f32,
        )
    };
    let (a, b) = (to_pixels(min), to_pixels(max));
    let tile = |pixel: f32, limit: u32| -> u32 {
        ((pixel / tile_size as f32).floor().max(0.0) as u32).min(limit.div_ceil(tile_size) - 1)
    };
    // The screen's Y grows downward, so the NDC maximum is the top.
    Some((
        (tile(a.x, width), tile(b.y, height)),
        (tile(b.x, width), tile(a.y, height)),
    ))
}

fn light_raw(light: &Light) -> LightRaw {
    let radiance = linear_color(light.color) * light.brightness * LIGHT_SCALE;
    let (cone_edge, direction, cone_inner) = match light.cone {
        None => (-2.0, Vec3::NEG_Y, 0.0),
        Some(cone) => {
            let half = (cone.angle_degrees.clamp(1.0, 179.0) * 0.5).to_radians();
            (
                half.cos(),
                cone.direction.normalize_or(Vec3::NEG_Y),
                (half * 0.8).cos(),
            )
        }
    };
    LightRaw {
        position_range: light.position.extend(light.range).to_array(),
        radiance_cone: radiance.extend(cone_edge).to_array(),
        direction_inner: direction.extend(cone_inner).to_array(),
    }
}

fn post_uniform(post: &PostProcessing) -> PostUniform {
    let flag = |on: bool| if on { 1.0 } else { 0.0 };
    let exposure = if post.enabled && post.exposure.is_finite() {
        post.exposure.max(0.0)
    } else {
        1.0
    };
    PostUniform {
        values: [
            post.bloom_threshold.max(0.0),
            post.bloom_intensity.max(0.0),
            exposure,
            0.0,
        ],
        flags: [
            flag(post.enabled && post.bloom_enabled),
            flag(post.enabled),
            0.0,
            0.0,
        ],
    }
}

/// The shadow values for the shader (see [`FrameUniform`]).
struct ShadowSetup {
    cascades: [Mat4; SHADOW_CASCADES],
    texels: [f32; 4],
    depths: [f32; 4],
    settings: [f32; 4],
}

/// Where each shadow cascade ends, in studs from the camera along the view.
/// The ends are spread so each cascade reaches a few times further than
/// the one before (see [`CASCADE_SPLIT_BLEND`]), and the last ends at `distance`.
fn cascade_ends(distance: f32) -> [f32; SHADOW_CASCADES] {
    // From 1 stud, not the near plane, or the first cascade would be tiny.
    let near = 1.0_f32.min(distance * 0.5);
    std::array::from_fn(|index| {
        let fraction = (index + 1) as f32 / SHADOW_CASCADES as f32;
        let even = near + (distance - near) * fraction;
        let growing = near * (distance / near).powf(fraction);
        even + (growing - even) * CASCADE_SPLIT_BLEND
    })
}

/// The sun's views of the shadow cascades and the shadow values for the
/// shader. Each cascade covers one slice of the view, so the nearest
/// shadows get the most texels per stud and are the sharpest. The area of
/// each is a sphere around its slice, so turning the camera doesn't change
/// the map's scale, and its position is snapped to whole texels so shadow
/// edges don't shimmer as the camera moves.
fn shadow_setup(list: &DrawList, sun_direction: Vec3, aspect: f32) -> ShadowSetup {
    let settings = list.shadows;
    let mut setup = ShadowSetup {
        cascades: [Mat4::IDENTITY; SHADOW_CASCADES],
        texels: [1.0; 4],
        depths: [1.0; 4],
        settings: [0.0; 4],
    };
    if !settings.enabled || !settings.distance.is_finite() || settings.distance <= 0.0 {
        return setup;
    }
    let camera = &list.camera;
    let forward = (camera.look_at - camera.position).normalize_or(Vec3::NEG_Z);
    let distance = settings.distance.clamp(NEAR_PLANE * 4.0, FAR_PLANE);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    let k2 = tan_half * tan_half * (1.0 + aspect * aspect);

    // Looking from the sun toward the scene.
    let toward_sun = sun_direction;
    let up_hint = if toward_sun.y.abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let right = (-toward_sun).cross(up_hint).normalize();
    let up = right.cross(-toward_sun);

    let mut slice_near = NEAR_PLANE;
    for (index, far) in cascade_ends(distance).into_iter().enumerate() {
        // The smallest sphere around the slice of the view between
        // `slice_near` and `far`, centered on the view direction `center` away.
        let center = (0.5 * (far + slice_near) * (1.0 + k2)).min(far);
        let radius = ((far - center).powi(2) + far * far * k2).sqrt().max(1.0);
        let sphere_center = camera.position + forward * center;
        slice_near = far;

        let texel = 2.0 * radius / SHADOW_MAP_SIZE as f32;
        let snap = |distance: f32| (distance / texel).round() * texel;
        let snapped_center = right * snap(sphere_center.dot(right))
            + up * snap(sphere_center.dot(up))
            + toward_sun * sphere_center.dot(toward_sun);

        let back = radius + SHADOW_CASTER_RANGE;
        let eye = snapped_center + toward_sun * back;
        let view = rh::view::look_at_mat4(eye, snapped_center, up);
        let (near, far_depth) = (0.1, back + radius);
        let projection =
            rh::proj::directx::orthographic(-radius, radius, -radius, radius, near, far_depth);
        setup.cascades[index] = projection * view;
        setup.texels[index] = texel;
        setup.depths[index] = far_depth - near;
    }
    let softness = if settings.softness.is_finite() {
        settings.softness.clamp(0.0, 1.0)
    } else {
        0.0
    };
    setup.settings = [
        1.0,
        softness * MAX_SUN_SIZE,
        SHADOW_CASCADES as f32,
        settings.distance,
    ];
    setup
}

/// Puts the visible items in drawing order and groups them into batches.
///
/// Solid items come first, grouped by mesh and texture. See-through items come last,
/// farthest from the camera first, so each one blends over what's behind it.
/// They're sorted by their centers, so two that overlap can still blend in the
/// wrong order; that's the usual trade-off for this simple approach.
fn plan_draws(list: &DrawList) -> (Vec<InstanceRaw>, Vec<Batch>) {
    let (mut transparent, mut opaque): (Vec<&DrawItem>, Vec<&DrawItem>) = list
        .items
        .iter()
        .filter(|item| item.material.opacity > 0.0)
        .partition(|item| item.material.opacity < 1.0);

    opaque.sort_by_key(|item| {
        (
            MeshKey::of(&item.shape),
            material_key(item),
            !item.casts_shadow,
        )
    });
    let distance = |item: &DrawItem| {
        item.transform
            .w_axis
            .truncate()
            .distance_squared(list.camera.position)
    };
    // `total_cmp` orders floats even when one is NaN, so sorting can't panic.
    transparent.sort_by(|a, b| distance(b).total_cmp(&distance(a)));

    let mut instances = Vec::new();
    let mut batches: Vec<Batch> = Vec::new();
    let ordered = opaque
        .into_iter()
        .map(|item| (item, false))
        .chain(transparent.into_iter().map(|item| (item, true)));
    for (item, transparent) in ordered {
        let Some(raw) = instance_raw(item) else {
            continue;
        };
        let index = instances.len() as u32;
        instances.push(raw);
        let (mesh, material) = (MeshKey::of(&item.shape), material_key(item));
        let casts_shadow = item.casts_shadow;
        match batches.last_mut() {
            Some(last)
                if last.mesh == mesh
                    && last.material == material
                    && last.transparent == transparent
                    && last.casts_shadow == casts_shadow =>
            {
                last.instances.end = index + 1;
            }
            _ => batches.push(Batch {
                mesh,
                material,
                transparent,
                casts_shadow,
                instances: index..index + 1,
            }),
        }
    }
    (instances, batches)
}

/// The textures an item is drawn with, in binding order (see
/// [`MATERIAL_TEXTURES`]). Only callers' meshes have texture coordinates,
/// so the built-in shapes never use any.
fn material_textures(item: &DrawItem) -> [Option<&Arc<dyn TextureSource>>; MATERIAL_TEXTURES] {
    if !matches!(item.shape, Shape::Mesh(_)) {
        return [None; MATERIAL_TEXTURES];
    }
    let material = &item.material;
    let maps = &material.maps;
    [
        material.base_color_texture.as_ref(),
        maps.normal.as_ref().map(|map| &map.texture),
        maps.metalness.as_ref().map(|map| &map.texture),
        maps.roughness.as_ref().map(|map| &map.texture),
        maps.occlusion.as_ref().map(|map| &map.texture),
        maps.emissive.as_ref(),
        maps.opacity.as_ref().map(|map| &map.texture),
        maps.specular.as_ref(),
    ]
}

/// The material binding an item is drawn with (see [`MaterialKey`]).
fn material_key(item: &DrawItem) -> MaterialKey {
    let textures = material_textures(item).map(|source| source.map(|source| source.key()));
    if textures.iter().all(Option::is_none) {
        return MaterialKey::default();
    }
    let maps = &item.material.maps;
    let mask = |map: &Option<ChannelMap>| {
        let mut mask = [0.0; 4];
        mask[map.as_ref().map_or(0, |map| map.channel as usize)] = 1.0;
        mask
    };
    let inverted = |map: &Option<ChannelMap>| f32::from(map.as_ref().is_some_and(|map| map.invert));
    let used = |used: bool| f32::from(used);
    let uniform = MapsUniform {
        metalness: mask(&maps.metalness),
        roughness: mask(&maps.roughness),
        occlusion: mask(&maps.occlusion),
        opacity: mask(&maps.opacity),
        invert: [
            inverted(&maps.metalness),
            inverted(&maps.roughness),
            inverted(&maps.occlusion),
            inverted(&maps.opacity),
        ],
        settings: [
            maps.normal.as_ref().map_or(0.0, |map| map.scale),
            if maps.normal.as_ref().is_some_and(|map| map.flip_green) {
                -1.0
            } else {
                1.0
            },
            if maps.occlusion.is_some() {
                maps.occlusion_strength.clamp(0.0, 1.0)
            } else {
                0.0
            },
            used(maps.opacity.is_some()),
        ],
        specular: [used(maps.specular.is_some()), 0.0, 0.0, 0.0],
    };
    MaterialKey {
        textures,
        maps: bytemuck::cast(uniform),
    }
}

/// The [`MapsUniform`] a key was made from. The default key's is the plain
/// material's: no maps.
fn maps_uniform_of(key: &MaterialKey) -> MapsUniform {
    if *key == MaterialKey::default() {
        let red = [1.0, 0.0, 0.0, 0.0];
        return MapsUniform {
            metalness: red,
            roughness: red,
            occlusion: red,
            opacity: red,
            invert: [0.0; 4],
            settings: [0.0, 1.0, 0.0, 0.0],
            specular: [0.0; 4],
        };
    }
    bytemuck::cast(key.maps)
}

/// The GPU data for one item, or `None` if it can't be seen.
fn instance_raw(item: &DrawItem) -> Option<InstanceRaw> {
    let model3 = Mat3::from_mat4(item.transform);
    // A shape squashed flat (size 0 on some axis) has no inverse, and can't be seen anyway.
    if model3.determinant().abs() < 1e-12 {
        return None;
    }
    let material = &item.material;
    let base_color = linear_color(material.base_color);
    Some(InstanceRaw {
        model: item.transform.to_cols_array_2d(),
        normal_matrix: model3.inverse().transpose().to_cols_array_2d(),
        base_color_opacity: base_color.extend(material.opacity).to_array(),
        emissive: linear_color(material.emissive).to_array(),
        surface: [
            material.metalness,
            material.roughness,
            material.alpha_cutoff,
        ],
    })
}

/// Converts an sRGB color (as people pick it) to linear light (what shading
/// math needs).
fn linear_color([r, g, b]: [f32; 3]) -> Vec3 {
    fn channel(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    Vec3::new(channel(r), channel(g), channel(b))
}

fn ui_vertices(items: &[UiDrawItem], (width, height): (u32, u32)) -> Vec<UiVertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let (width, height) = (width as f32, height as f32);
    let mut vertices = Vec::new();
    for item in items {
        let UiDrawItem::Rectangle(UiRectangle {
            position: [x, y],
            size: [w, h],
            color: [r, g, b, alpha],
            color_end: [er, eg, eb, ea],
            border_color: [br, bg, bb, ba],
            border_width,
            border_radius,
        }) = item
        else {
            continue;
        };
        if ![
            x,
            y,
            w,
            h,
            r,
            g,
            b,
            alpha,
            br,
            bg,
            bb,
            ba,
            border_width,
            border_radius,
            er,
            eg,
            eb,
            ea,
        ]
        .into_iter()
        .all(|component| component.is_finite())
            || *w <= 0.0
            || *h <= 0.0
            || (*alpha <= 0.0 && *ea <= 0.0 && (*ba <= 0.0 || *border_width <= 0.0))
        {
            continue;
        }
        let point = |px: f32, py: f32| [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0];
        let top_left = point(*x, *y);
        let top_right = point(*x + *w, *y);
        let bottom_left = point(*x, *y + *h);
        let bottom_right = point(*x + *w, *y + *h);
        let color = linear_color([*r, *g, *b]);
        let color = [color.x, color.y, color.z, alpha.clamp(0.0, 1.0)];
        let border_color = linear_color([*br, *bg, *bb]);
        let border_color = [
            border_color.x,
            border_color.y,
            border_color.z,
            ba.clamp(0.0, 1.0),
        ];
        let end = linear_color([*er, *eg, *eb]);
        let color_end = [end.x, end.y, end.z, ea.clamp(0.0, 1.0)];
        for (position, uv) in [
            (top_left, [0.0, 0.0]),
            (bottom_left, [0.0, 1.0]),
            (top_right, [1.0, 0.0]),
            (top_right, [1.0, 0.0]),
            (bottom_left, [0.0, 1.0]),
            (bottom_right, [1.0, 1.0]),
        ] {
            vertices.push(UiVertex {
                position,
                color,
                uv,
                size: [*w, *h],
                border_color,
                border: [
                    border_radius.clamp(0.0, w.min(*h) * 0.5),
                    border_width.clamp(0.0, w.min(*h) * 0.5),
                ],
                color_end,
            });
        }
    }
    vertices
}

fn image_quad(image: &UiImage, (width, height): (u32, u32)) -> Vec<UiVertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let [x, y] = image.position;
    let [w, h] = image.size;
    let [r, g, b, alpha] = image.tint;
    if ![x, y, w, h, r, g, b, alpha, image.border_radius]
        .into_iter()
        .all(f32::is_finite)
        || w <= 0.0
        || h <= 0.0
        || alpha <= 0.0
    {
        return Vec::new();
    }
    let (width, height) = (width as f32, height as f32);
    let point = |px: f32, py: f32| [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0];
    let color = linear_color([r, g, b]);
    let color = [color.x, color.y, color.z, alpha.clamp(0.0, 1.0)];
    let radius = image.border_radius.clamp(0.0, w.min(h) * 0.5);
    [
        (point(x, y), [0.0, 0.0]),
        (point(x, y + h), [0.0, 1.0]),
        (point(x + w, y), [1.0, 0.0]),
        (point(x + w, y), [1.0, 0.0]),
        (point(x, y + h), [0.0, 1.0]),
        (point(x + w, y + h), [1.0, 1.0]),
    ]
    .into_iter()
    .map(|(position, uv)| UiVertex {
        position,
        color,
        uv,
        size: [w, h],
        border_color: [0.0; 4],
        border: [radius, 0.0],
        color_end: color,
    })
    .collect()
}

fn glyph_color([r, g, b, a]: [f32; 4]) -> GlyphColor {
    let byte = |channel: f32| {
        (if channel.is_finite() { channel } else { 1.0 })
            .clamp(0.0, 1.0)
            .mul_add(255.0, 0.5) as u8
    };
    GlyphColor::rgba(byte(r), byte(g), byte(b), byte(a))
}

fn create_ui_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI overlay"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<UiVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
            3 => Float32x2, 4 => Float32x4, 5 => Float32x2, 6 => Float32x4
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UI overlay"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// A second UI pipeline that samples one texture. Rounded corners match
/// [`create_ui_pipeline`]; the tint is the vertex color.
fn create_ui_image_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
) -> (
    wgpu::BindGroupLayout,
    wgpu::Sampler,
    wgpu::RenderPipeline,
) {
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UI image"),
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
        label: Some("UI image sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI image"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<UiVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
            3 => Float32x2, 4 => Float32x4, 5 => Float32x2, 6 => Float32x4
        ],
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UI image"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_image"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    });
    (bind_layout, sampler, pipeline)
}

/// Draws the sky as a full-screen triangle at the far plane, before any item.
fn create_sky_pipeline(
    device: &wgpu::Device,
    frame_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky"),
        bind_group_layouts: &[frame_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_sky"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_sky"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

impl PostPasses {
    fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &entries,
        });
        entries.push(texture_entry(3));
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &entries,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-processing"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post-processing settings"),
            size: size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));
        let pipeline = |label, layout: &wgpu::BindGroupLayout, entry, format, blend| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
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
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        Self {
            prefilter: pipeline("bloom prefilter", &layout, "fs_prefilter", HDR_FORMAT, None),
            downsample: pipeline(
                "bloom downsample",
                &layout,
                "fs_downsample",
                HDR_FORMAT,
                None,
            ),
            upsample: pipeline(
                "bloom upsample",
                &layout,
                "fs_upsample",
                HDR_FORMAT,
                Some(wgpu::BlendState {
                    color: add,
                    alpha: add,
                }),
            ),
            composite: pipeline(
                "composite",
                &composite_layout,
                "fs_composite",
                color_format,
                None,
            ),
            sampler,
            layout,
            composite_layout,
            settings,
        }
    }

    /// Adds bloom (if `bloom`) to the scene and draws the result, tone
    /// mapped, into `target`.
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        targets: &PostTargets,
        target: &wgpu::TextureView,
        bloom: bool,
    ) {
        let levels = &targets.levels;
        if bloom {
            fullscreen_pass(
                encoder,
                "bloom prefilter",
                &levels[0],
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                &self.prefilter,
                &targets.prefilter_group,
            );
            for (level, target) in levels.iter().enumerate().skip(1) {
                fullscreen_pass(
                    encoder,
                    "bloom downsample",
                    target,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    &self.downsample,
                    &targets.downsample_groups[level],
                );
            }
            for level in (1..levels.len()).rev() {
                fullscreen_pass(
                    encoder,
                    "bloom upsample",
                    &levels[level - 1],
                    wgpu::LoadOp::Load,
                    &self.upsample,
                    &targets.upsample_groups[level],
                );
            }
        }
        fullscreen_pass(
            encoder,
            "composite",
            target,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            &self.composite,
            &targets.composite_group,
        );
    }
}

/// Draws one full-screen triangle with `pipeline` into `target`.
fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

impl PostTargets {
    /// Makes the HDR texture for a `width` × `height` screen and the bloom
    /// levels below it, with the bind groups that connect them.
    fn new(device: &wgpu::Device, post: &PostPasses, width: u32, height: u32) -> Self {
        let make = |label, width: u32, height: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: width.max(1),
                        height: height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let hdr_view = make("scene (HDR)", width, height);
        let mut levels = Vec::new();
        let (mut level_width, mut level_height) = ((width / 2).max(1), (height / 2).max(1));
        loop {
            levels.push(make("bloom level", level_width, level_height));
            if levels.len() == MAX_BLOOM_LEVELS || level_width == 1 || level_height == 1 {
                break;
            }
            level_width = (level_width / 2).max(1);
            level_height = (level_height / 2).max(1);
        }

        let group = |layout: &wgpu::BindGroupLayout, source: &wgpu::TextureView, bloom| {
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&post.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post.settings.as_entire_binding(),
                },
            ];
            if let Some(bloom) = bloom {
                entries.push(wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bloom),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post-processing"),
                layout,
                entries: &entries,
            })
        };
        let prefilter_group = group(&post.layout, &hdr_view, None);
        // Index 0 is never used, but keeps the lists indexed by level.
        let downsample_groups = (0..levels.len())
            .map(|level| group(&post.layout, &levels[level.saturating_sub(1)], None))
            .collect();
        let upsample_groups = (0..levels.len())
            .map(|level| group(&post.layout, &levels[level], None))
            .collect();
        let composite_group = group(&post.composite_layout, &hdr_view, Some(&levels[0]));
        Self {
            hdr_view,
            levels,
            prefilter_group,
            downsample_groups,
            upsample_groups,
            composite_group,
        }
    }
}

/// The pipeline for solid items, or for see-through ones when `transparent`
/// is true. Both use the same shader.
fn create_pipeline(
    device: &wgpu::Device,
    frame_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    color_format: wgpu::TextureFormat,
    transparent: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene"),
        bind_group_layouts: &[frame_layout, material_layout],
        push_constant_ranges: &[],
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 12 => Float32x2, 13 => Float32x4
        ],
    };
    // Advances once per drawn shape instead of once per corner.
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x3, 7 => Float32x3, 8 => Float32x3,
            9 => Float32x4,
            10 => Float32x3,
            11 => Float32x3,
        ],
    };

    let blend = if transparent {
        wgpu::BlendState {
            // Mix the surface's color over what's already drawn, by its opacity.
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            // Keep the window's own alpha at 1. Some desktops would otherwise
            // show what's behind the window through see-through parts.
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }
    } else {
        wgpu::BlendState::REPLACE
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if transparent { "transparent" } else { "opaque" }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout, instance_layout],
        },
        primitive: wgpu::PrimitiveState {
            // Counter-clockwise triangles face the camera; the backs are skipped.
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            // See-through items are hidden behind solid ones, but don't hide
            // other see-through items behind them.
            depth_write_enabled: !transparent,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// A square depth texture with one layer per shadow cascade, which the
/// sun's views are drawn into and the scene pass then reads: a view of the
/// whole array for reading, and one view per layer for drawing.
fn create_shadow_views(
    device: &wgpu::Device,
    size: u32,
) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow map"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: SHADOW_CASCADES as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow map"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layers = (0..SHADOW_CASCADES as u32)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow cascade"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (array, layers)
}

fn create_frame_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    [frame_buffer, light_buffer, tile_buffer]: [&wgpu::Buffer; 3],
    shadow_view: &wgpu::TextureView,
    shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: tile_buffer.as_entire_binding(),
            },
        ],
    })
}

/// Draws solid items' depth as the sun sees them. No color, no materials.
fn create_shadow_pipeline(
    device: &wgpu::Device,
    shadow_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow"),
        bind_group_layouts: &[shadow_layout],
        push_constant_ranges: &[],
    });
    // Only the position and the model matrix matter here.
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_shadow"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout, instance_layout],
        },
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            // Both sides are drawn, so thin and open shapes still cast shadows.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            // Pushes steep surfaces' depth back a little, so they don't
            // shadow themselves in stripes ("shadow acne").
            bias: wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 1.5,
                clamp: 0.0,
            },
        }),
        multisample: Default::default(),
        fragment: None,
        multiview: None,
        cache: None,
    })
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// The texture a [`TextureRenderer`] draws into, with its two views: one to
/// draw through and one to show. Others can sample it (`TEXTURE_BINDING`),
/// and tests can copy it back (`COPY_SRC`).
///
/// The texture itself is plain `Rgba8Unorm`, which egui expects. Drawing goes
/// through an `Rgba8UnormSrgb` view of the same pixels, so the GPU encodes the
/// shader's linear colors as sRGB on the way in. (Sampling an sRGB view would
/// decode them back to linear, which egui would then show far too dark.)
fn create_target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_SHOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        // Views may reinterpret the pixels in these formats too.
        view_formats: &[TARGET_DRAW_FORMAT],
    });
    let draw_view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TARGET_DRAW_FORMAT),
        ..Default::default()
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, draw_view, view)
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn upload_mesh(device: &wgpu::Device, name: &str, mesh: &MeshData) -> GpuMesh {
    upload_buffers(
        device,
        name,
        &mesh.vertices,
        bytemuck::cast_slice(&mesh.indices),
        mesh.indices.len(),
        wgpu::IndexFormat::Uint16,
    )
}

/// Uploads a caller's mesh, or `None` (with a warning) if its data doesn't
/// fit together, such as an index past the last corner.
fn upload_custom_mesh(device: &wgpu::Device, source: &dyn MeshSource) -> Option<GpuMesh> {
    let positions = source.positions();
    let normals = source.normals();
    let uvs = source.uvs();
    let tangents = source.tangents();
    let indices = source.indices();
    let count = positions.len();
    let fits = count > 0
        && normals.len() == count
        && (uvs.is_empty() || uvs.len() == count)
        && (tangents.is_empty() || tangents.len() == count)
        && !indices.is_empty()
        && indices.len().is_multiple_of(3)
        && indices.iter().all(|&index| (index as usize) < count);
    if !fits {
        log::warn!("mesh #{} has broken data, so it isn't drawn", source.key());
        return None;
    }
    let vertices: Vec<Vertex> = (0..count)
        .map(|i| Vertex {
            position: positions[i],
            normal: normals[i],
            uv: uvs.get(i).copied().unwrap_or_default(),
            tangent: tangents.get(i).copied().unwrap_or(mesh::NO_TANGENT),
        })
        .collect();
    Some(upload_buffers(
        device,
        "mesh",
        &vertices,
        bytemuck::cast_slice(indices),
        indices.len(),
        wgpu::IndexFormat::Uint32,
    ))
}

fn upload_buffers(
    device: &wgpu::Device,
    name: &str,
    vertices: &[Vertex],
    indices: &[u8],
    index_count: usize,
    index_format: wgpu::IndexFormat,
) -> GpuMesh {
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: indices,
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertices,
        indices,
        index_count: index_count as u32,
        index_format,
    }
}

/// Uploads a caller's texture, or `None` (with a warning) if it has no
/// pixels or the wrong number of them.
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &dyn TextureSource,
) -> Option<GpuTexture> {
    let (width, height) = source.size();
    let rgba = source.rgba();
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        log::warn!(
            "texture #{} has broken data, so it isn't drawn",
            source.key()
        );
        return None;
    }
    Some(create_texture(device, queue, width, height, rgba))
}

/// Binds a material's `textures` (in [`MATERIAL_TEXTURES`] order) and
/// `maps` for the shader. Colors are read through sRGB views, the rest as
/// plain values.
fn create_material(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    textures: [&GpuTexture; MATERIAL_TEXTURES],
    maps: MapsUniform,
) -> GpuMaterial {
    let maps = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("material maps"),
        contents: bytemuck::bytes_of(&maps),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let [
        base,
        normal,
        metalness,
        roughness,
        occlusion,
        emissive,
        opacity,
        specular,
    ] = textures;
    let view = |binding, view| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    };
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material"),
        layout,
        entries: &[
            view(0, &base.srgb),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: maps.as_entire_binding(),
            },
            view(3, &normal.linear),
            view(4, &metalness.linear),
            view(5, &roughness.linear),
            view(6, &occlusion.linear),
            view(7, &emissive.srgb),
            view(8, &opacity.linear),
            view(9, &specular.linear),
        ],
    });
    GpuMaterial {
        bind_group,
        _maps: maps,
    }
}

/// Uploads RGBA pixels with a full set of mipmaps (smaller copies, so far
/// away surfaces don't shimmer), with an sRGB and a plain view.
fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> GpuTexture {
    let mut levels = mip_levels(width, height, rgba);
    // Leave out levels too big for this GPU, so a huge texture still shows,
    // just less sharp. The last level is 1×1, so one is always left.
    let max = device.limits().max_texture_dimension_2d;
    levels.retain(|(width, height, _)| *width <= max && *height <= max);
    let (width, height) = (levels[0].0, levels[0].1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[TEXTURE_SRGB_FORMAT],
    });
    for (level, (width, height, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
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
    let linear = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TEXTURE_SRGB_FORMAT),
        ..Default::default()
    });
    GpuTexture {
        _texture: texture,
        srgb,
        linear,
    }
}

/// The image and each half-size copy of it, down to 1×1, as (width,
/// height, pixels). `Cow` lets the first be the caller's pixels, uncopied.
fn mip_levels(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Cow<'_, [u8]>)> {
    let mut levels = vec![(width, height, Cow::Borrowed(rgba))];
    loop {
        let (width, height, pixels) = levels.last().expect("starts with one");
        if *width == 1 && *height == 1 {
            return levels;
        }
        let half = half_size(*width, *height, pixels);
        levels.push(half);
    }
}

/// An image at half the size (at least 1 pixel), each pixel the average of
/// the 2×2 it covers. Averaging the sRGB values rather than linear light is
/// slightly off, but not visibly for textures.
fn half_size(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Cow<'static, [u8]>) {
    let (half_width, half_height) = ((width / 2).max(1), (height / 2).max(1));
    let mut half = Vec::with_capacity(half_width as usize * half_height as usize * 4);
    for y in 0..half_height {
        for x in 0..half_width {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                // An odd last row or column is averaged with itself.
                let from_x = (x * 2 + dx).min(width - 1);
                let from_y = (y * 2 + dy).min(height - 1);
                let at = (from_y as usize * width as usize + from_x as usize) * 4;
                for (channel, total) in sum.iter_mut().enumerate() {
                    *total += u32::from(rgba[at + channel]);
                }
            }
            half.extend(sum.map(|total| ((total + 2) / 4) as u8));
        }
    }
    (half_width, half_height, Cow::Owned(half))
}

/// Returns the size to draw at, or `None` when the window has no area
/// (minimized windows report 0×0, and a GPU surface can't be zero-sized).
fn drawable_size(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

/// How far down to move text so it's centered up and down in its box, if
/// it asks to be: half the room its lines leave.
fn vertical_offset(text: &UiText, buffer: &Buffer) -> f32 {
    if !text.center_vertically {
        return 0.0;
    }
    let lines: f32 = buffer.layout_runs().map(|run| run.line_height).sum();
    ((text.size[1] - lines) * 0.5).max(0.0)
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
}
