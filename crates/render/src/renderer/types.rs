use super::*;

pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// A [`TextureRenderer`]'s texture, as shown (see `create_target_texture`).
pub(super) const TARGET_SHOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// A [`TextureRenderer`]'s texture, as drawn into.
pub(super) const TARGET_DRAW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Textures are stored as plain bytes. Color textures are read through an
/// sRGB view of them ([`TEXTURE_SRGB_FORMAT`]), so sampling gives linear light.
pub(super) const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(super) const TEXTURE_SRGB_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub(super) const NEAR_PLANE: f32 = 0.1;
pub(super) const FAR_PLANE: f32 = 5000.0;
/// A caller's mesh or texture not drawn for this many frames is taken off
/// the GPU (about 10 seconds at 60 fps). Drawn again, it's uploaded again.
pub(super) const KEEP_UNDRAWN_FRAMES: u64 = 600;

/// Per-frame values for the shader. Must match `Frame` in shader.wgsl.
/// Every field is 16 bytes or a multiple, which is what uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct FrameUniform {
    pub(super) view_proj: [[f32; 4]; 4],
    pub(super) camera_position: [f32; 4],
    pub(super) sun_direction: [f32; 4],
    pub(super) sun_radiance: [f32; 4],
    pub(super) ambient: [f32; 4],
    /// Turns world space to the sun's view of each shadow cascade.
    pub(super) cascades: [[[f32; 4]; 4]; SHADOW_CASCADES],
    /// The world size of a shadow texel in each cascade.
    pub(super) cascade_texels: [f32; 4],
    /// How many studs each cascade's depth range covers.
    pub(super) cascade_depths: [f32; 4],
    /// 1 if shadows are on, how wide the sun looks (for soft edges), how
    /// many cascades there are, and how far from the camera shadows reach.
    pub(super) shadow: [f32; 4],
    /// Turns clip space back to world space, to find the sky's direction.
    pub(super) inv_view_proj: [[f32; 4]; 4],
    /// The sky's colors in linear light. The top color's fourth number is
    /// the environment intensity.
    pub(super) sky_top: [f32; 4],
    pub(super) sky_horizon: [f32; 4],
    pub(super) sky_ground: [f32; 4],
    /// How many lights there are, the size of a light tile in pixels, and
    /// how many tiles are across the screen.
    pub(super) lights: [u32; 4],
}

/// The most point and spot lights drawn at once.
pub(super) const MAX_LIGHTS: usize = 1024;
/// The most lights one screen tile lists. Must match the shader.
pub(super) const LIGHTS_PER_TILE: usize = 64;
/// Most screen tiles there can be. Tiles grow bigger on huge screens to stay under this.
pub(super) const MAX_TILES: usize = 9216;
pub(super) const MIN_TILE_SIZE: u32 = 32;
/// Turns a light's brightness into the shader's units, so a brightness of 1
/// is a modest lamp next to the sun's 3.
pub(super) const LIGHT_SCALE: f32 = 20.0;
/// The scene is drawn in high dynamic range, into a texture that can hold
/// light brighter than 1. Post-processing then brings it down to the screen.
pub(super) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The most blur levels bloom uses, each half the size of the one before.
pub(super) const MAX_BLOOM_LEVELS: usize = 5;

/// One point or spot light as the GPU reads it. Must match `Light` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct LightRaw {
    /// Position, and range in the fourth number.
    pub(super) position_range: [f32; 4],
    /// Linear color times brightness, and the cosine of the cone's edge in
    /// the fourth number (-2 for a point light).
    pub(super) radiance_cone: [f32; 4],
    /// Where a spot light points, and the cosine where its edge starts to fade.
    pub(super) direction_inner: [f32; 4],
}

/// Post-processing settings for the shader. Must match `Post` in post.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PostUniform {
    /// Bloom threshold, bloom intensity and exposure.
    pub(super) values: [f32; 4],
    /// 1 if bloom is on, 1 if tone mapping is on.
    pub(super) flags: [f32; 4],
}

/// The size of each shadow cascade, in texels per side.
pub(super) const SHADOW_MAP_SIZE: u32 = 2048;
/// How many shadow maps cover the view, each further out and coarser than
/// the one before. Must match the size of `cascades` in shader.wgsl.
pub(super) const SHADOW_CASCADES: usize = 3;
/// How the cascades' far ends are spread between even steps (0) and steps
/// that grow by the same factor each time (1). Mostly the second, so the
/// nearest cascade is small and sharp.
pub(super) const CASCADE_SPLIT_BLEND: f32 = 0.75;
/// How far behind the visible area (toward the sun) things still cast shadows into it.
pub(super) const SHADOW_CASTER_RANGE: f32 = 500.0;
/// How wide the sun looks at `ShadowSoftness` 1: the width of a soft edge
/// per stud between the caster and the surface it shades.
pub(super) const MAX_SUN_SIZE: f32 = 0.1;
/// Uniform buffer offsets must be multiples of this (wgpu's default limit).
pub(super) const CASCADE_UNIFORM_STRIDE: u64 = 256;

/// The depth texture the sun's view is drawn into, one layer per cascade,
/// with the frame bindings that read it.
pub(super) struct ShadowMap {
    /// One view per cascade, to draw into.
    pub(super) layers: Vec<wgpu::TextureView>,
    /// The frame bindings with this map in place of the empty one.
    pub(super) frame_bind_group: wgpu::BindGroup,
}

/// One draw item as the GPU reads it. Must match `InstanceIn` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct InstanceRaw {
    pub(super) model: [[f32; 4]; 4],
    /// Turns surface directions to world space. Unlike `model`, it keeps them
    /// perpendicular to the surface when a shape is stretched.
    pub(super) normal_matrix: [[f32; 3]; 3],
    pub(super) base_color_opacity: [f32; 4],
    pub(super) emissive: [f32; 3],
    /// Metalness, roughness, alpha cutoff, and studs per texture repeat
    /// (0 keeps the mesh's own coordinates).
    pub(super) surface: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct UiVertex {
    pub(super) position: [f32; 2],
    pub(super) color: [f32; 4],
    pub(super) uv: [f32; 2],
    pub(super) size: [f32; 2],
    /// The top border's colour.
    pub(super) border_color: [f32; 4],
    /// The corner radius, and what's drawn: [`UI_BOX`], [`UI_SHADOW`] or
    /// [`UI_INSET_SHADOW`].
    pub(super) border: [f32; 2],
    pub(super) color_end: [f32; 4],
    /// Border widths: top, right, bottom, left.
    pub(super) border_widths: [f32; 4],
    pub(super) border_right: [f32; 4],
    pub(super) border_bottom: [f32; 4],
    pub(super) border_left: [f32; 4],
    /// A shadow's offset, blur and spread.
    pub(super) shadow: [f32; 4],
}

/// What a UI quad draws (`UiVertex::border[1]`, matched in `ui.wgsl`): a
/// filled, bordered box, a shadow behind one, or a shadow inside one.
pub(super) const UI_BOX: f32 = 0.0;
pub(super) const UI_SHADOW: f32 = 1.0;
pub(super) const UI_INSET_SHADOW: f32 = 2.0;

/// A mesh uploaded to the GPU.
pub(super) struct GpuMesh {
    pub(super) vertices: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    pub(super) index_count: u32,
    /// 16-bit for the built-in shapes, 32-bit for callers' meshes, which
    /// can have more than 65,536 corners.
    pub(super) index_format: wgpu::IndexFormat,
    /// The caller's [`MeshSource::version`] these vertices are of.
    pub(super) version: u64,
}

/// How a material's texture maps are read. Must match `Maps` in
/// shader.wgsl. Each field is 16 bytes, as uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct MapsUniform {
    /// Which channel holds each map's value: 1 in that one, 0 in the others.
    pub(super) metalness: [f32; 4],
    pub(super) roughness: [f32; 4],
    pub(super) occlusion: [f32; 4],
    pub(super) opacity: [f32; 4],
    /// 1 to invert metalness, roughness, occlusion and opacity.
    pub(super) invert: [f32; 4],
    /// Normal map scale (0: none), green's sign, occlusion strength (0:
    /// none), and 1 if opacity has its own map.
    pub(super) settings: [f32; 4],
    /// 1 if there's a specular map, then unused.
    pub(super) specular: [f32; 4],
}

/// A texture uploaded to the GPU, with a view for each way it's read.
pub(super) struct GpuTexture {
    /// Kept so it lives as long as the bind groups that use it.
    pub(super) _texture: wgpu::Texture,
    /// Converts sRGB to linear light, for colors.
    pub(super) srgb: wgpu::TextureView,
    /// Gives the bytes as they are (128 is 0.5), for values.
    pub(super) linear: wgpu::TextureView,
}

/// A material's textures and [`MapsUniform`], bound together for the shader.
pub(super) struct GpuMaterial {
    pub(super) bind_group: wgpu::BindGroup,
    /// Kept so it lives as long as the bind group.
    pub(super) _maps: wgpu::Buffer,
}

/// The number of textures a material can have: base color, normal,
/// metalness, roughness, occlusion, emissive, opacity and specular.
pub(super) const MATERIAL_TEXTURES: usize = 8;

/// Which material binding an item uses: its textures' keys and its
/// [`MapsUniform`] (as bits, so it can be compared and sorted). Items with
/// the same key share one binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct MaterialKey {
    pub(super) textures: [Option<u64>; MATERIAL_TEXTURES],
    pub(super) maps: [u32; 28],
    /// A terrain chunk's binding (see `terrain::TerrainKey`), all 0 for
    /// everything else.
    pub(super) terrain: terrain::TerrainKey,
}

/// Something of a caller's on the GPU, and the frame it was last drawn in.
/// `None` if its data was broken, so that's only reported once.
pub(super) struct Cached<T> {
    pub(super) gpu: Option<T>,
    pub(super) last_drawn: u64,
}

/// Which mesh a batch draws: a built-in shape, or a caller's by its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MeshKey {
    Block,
    Sphere,
    Cylinder,
    Wedge,
    Custom(u64),
}

impl MeshKey {
    pub(super) fn of(shape: &Shape) -> Self {
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
pub(super) struct Meshes {
    pub(super) block: GpuMesh,
    pub(super) sphere: GpuMesh,
    pub(super) cylinder: GpuMesh,
    pub(super) wedge: GpuMesh,
}

impl Meshes {
    /// The built-in mesh for `key`, or `None` for a caller's mesh.
    pub(super) fn get(&self, key: MeshKey) -> Option<&GpuMesh> {
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
pub(super) struct Batch {
    pub(super) mesh: MeshKey,
    pub(super) material: MaterialKey,
    pub(super) transparent: bool,
    pub(super) casts_shadow: bool,
    pub(super) instances: Range<u32>,
}

/// The bloom and tone mapping pipelines, which don't depend on the screen's size.
pub(super) struct PostPasses {
    pub(super) sampler: wgpu::Sampler,
    /// Texture, sampler and settings, for the bloom passes.
    pub(super) layout: wgpu::BindGroupLayout,
    /// The same plus the bloom texture, for the final pass.
    pub(super) composite_layout: wgpu::BindGroupLayout,
    pub(super) settings: wgpu::Buffer,
    pub(super) prefilter: wgpu::RenderPipeline,
    pub(super) downsample: wgpu::RenderPipeline,
    /// Adds to what's already in the level it draws into.
    pub(super) upsample: wgpu::RenderPipeline,
    pub(super) composite: wgpu::RenderPipeline,
}

/// What post-processing draws into and reads, which depends on the screen's size.
pub(super) struct PostTargets {
    /// Where the scene is drawn.
    pub(super) hdr_view: wgpu::TextureView,
    /// Bloom levels from half the screen's size down, each half the one before.
    pub(super) levels: Vec<wgpu::TextureView>,
    /// Scene to the first level.
    pub(super) prefilter_group: wgpu::BindGroup,
    /// Level `i` is made from level `i - 1` (the entry for level 0 is unused).
    pub(super) downsample_groups: Vec<wgpu::BindGroup>,
    /// Level `i` is added into level `i - 1`.
    pub(super) upsample_groups: Vec<wgpu::BindGroup>,
    pub(super) composite_group: wgpu::BindGroup,
}

/// The GPU objects for drawing a scene, whatever it's drawn into. Both
/// [`Renderer`] and [`TextureRenderer`] own one.
pub(super) struct ScenePipeline {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) sky_pipeline: wgpu::RenderPipeline,
    pub(super) opaque_pipeline: wgpu::RenderPipeline,
    pub(super) transparent_pipeline: wgpu::RenderPipeline,
    /// Camera-facing pictures, such as particles.
    pub(super) sprites: sprites::SpritePass,
    pub(super) post: PostPasses,
    /// The scene's HDR texture and the bloom levels, remade at each resize.
    pub(super) post_targets: PostTargets,
    pub(super) light_buffer: wgpu::Buffer,
    pub(super) tile_buffer: wgpu::Buffer,
    pub(super) ui_pipeline: wgpu::RenderPipeline,
    pub(super) ui_image_pipeline: wgpu::RenderPipeline,
    pub(super) ui_image_layout: wgpu::BindGroupLayout,
    pub(super) ui_sampler: wgpu::Sampler,
    /// One bind group per UI image texture. Dropped with the texture.
    pub(super) ui_image_groups: HashMap<u64, wgpu::BindGroup>,
    pub(super) ui_buffer: wgpu::Buffer,
    pub(super) ui_capacity: usize,
    pub(super) font_system: FontSystem,
    /// The paths of the project fonts loaded into `font_system`.
    pub(super) loaded_fonts: HashSet<String>,
    /// The size each `TextScaled` text was fitted at, by everything that
    /// changes it, so it isn't worked out again every frame.
    pub(super) fitted_sizes: HashMap<String, f32>,
    pub(super) swash_cache: SwashCache,
    pub(super) text_viewport: TextViewport,
    pub(super) text_atlas: TextAtlas,
    /// One per UI layer with text (see `ui_pass::ui_layers`), made as
    /// they're first needed and kept.
    pub(super) text_renderers: Vec<GlyphTextRenderer>,
    pub(super) frame_buffer: wgpu::Buffer,
    /// The frame bindings with an empty shadow map, for when shadows are off.
    pub(super) frame_bind_group: wgpu::BindGroup,
    pub(super) frame_layout: wgpu::BindGroupLayout,
    pub(super) shadow_sampler: wgpu::Sampler,
    pub(super) shadow_pipeline: wgpu::RenderPipeline,
    /// Each cascade's view from the sun, [`CASCADE_UNIFORM_STRIDE`] apart.
    pub(super) cascade_buffer: wgpu::Buffer,
    /// Gives the shadow pass one cascade's matrix (it can't also bind the
    /// shadow map it's drawing into).
    pub(super) shadow_bind_group: wgpu::BindGroup,
    /// Made when shadows are first turned on, and freed when turned off.
    pub(super) shadow_map: Option<ShadowMap>,
    pub(super) meshes: Meshes,
    pub(super) instance_buffer: wgpu::Buffer,
    /// How many instances fit in `instance_buffer` before it must grow.
    pub(super) instance_capacity: usize,
    pub(super) material_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    /// One white pixel, for maps an item doesn't have: it changes nothing.
    pub(super) white_texture: GpuTexture,
    /// For items with no textures, such as every built-in shape.
    pub(super) plain_material: GpuMaterial,
    /// Callers' meshes and textures, by key, and the materials using them.
    pub(super) custom_meshes: HashMap<u64, Cached<GpuMesh>>,
    pub(super) textures: HashMap<u64, Cached<GpuTexture>>,
    pub(super) materials: HashMap<MaterialKey, Cached<GpuMaterial>>,
    /// Terrain chunks' pipeline, layer textures and bindings.
    pub(super) terrain: terrain::TerrainPass,
    /// How many frames have been drawn, to tell what wasn't drawn lately.
    pub(super) frame: u64,
    /// Only on devices that can take timestamps (the editor asks for them).
    pub(super) gpu_timer: Option<gpu_timer::GpuTimer>,
    /// Draw calls and triangles of the last frame.
    pub(super) draws: (u32, u64),
}
