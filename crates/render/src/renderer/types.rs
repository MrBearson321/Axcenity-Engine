use super::*;
use crate::graphics::SpotShadowMapSize;
use glam::{Mat4, Vec3};
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Turns world space to each shadow-casting spot light's view, one per
    /// layer of the spot shadow map (see [`LightRaw::shadow`]).
    pub(super) spot_shadows: [[[f32; 4]; 4]; MAX_SPOT_SHADOWS],
    /// Turns world space to the camera's (view space), for the prepass's
    /// normals.
    pub(super) view: [[f32; 4]; 4],
    /// How strongly screen-space ambient occlusion darkens ambient light (0
    /// while it's off), then unused, then 1 over the scene's width and
    /// height in pixels, to find a pixel's place in the occlusion texture.
    pub(super) ambient_occlusion: [f32; 4],
    /// 1 while the environment maps are on, the reflection cubemap's last
    /// mip level, 1 when the sky is an image, and the image's width.
    pub(super) environment: [f32; 4],
    /// The sky's diffuse light as spherical harmonics (see
    /// `environment::harmonics`), while the environment maps are on.
    pub(super) sky_light: [[f32; 4]; 9],
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
/// The finished, tone-mapped picture while FXAA is on, before FXAA smooths
/// it into the output. sRGB, so 8 bits keep dark shades from banding.
pub(super) const FXAA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// The prepass's view-space normals, which ambient occlusion reads.
pub(super) const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Ambient occlusion: 1 in the open, toward 0 in corners.
pub(super) const OCCLUSION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// How far around a point ambient occlusion looks for what hides it, in studs.
pub(super) const OCCLUSION_RADIUS: f32 = 1.5;

/// How many blur levels bloom uses at render scale 1, each half the size of
/// the one before. Whatever the scale, the last is this many halvings of the
/// output's size, so the glow spreads as far across the screen.
pub(super) const BLOOM_LEVELS: u32 = 5;

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
    /// The layer of the spot shadow map it casts shadows into (-1 for none),
    /// then the width of one of its shadow texels per stud from the light.
    pub(super) shadow: [f32; 4],
}

/// The most spot lights that cast shadows at once (Ultra's). The graphics
/// settings pick how many do (see [`crate::ShadowQuality::spot_map`]).
/// Must match the size of `spot_shadows` in lighting.wgsl.
pub(super) const MAX_SPOT_SHADOWS: usize = 8;
/// The widest a spot light's shadow map looks, in degrees. Past it, a wider
/// beam's outer edge is lit without shadows.
pub(super) const MAX_SPOT_SHADOW_DEGREES: f32 = 170.0;

/// Post-processing settings for the shader. Must match `Post` in post.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PostUniform {
    /// Bloom threshold, bloom intensity and exposure.
    pub(super) values: [f32; 4],
    /// 1 if bloom is on, 1 if tone mapping is on, and which curve
    /// ([`crate::ToneMapping`]).
    pub(super) flags: [f32; 4],
}

/// The largest shadow cascade, in texels per side (Ultra's). The graphics
/// settings pick the size in use (see [`crate::ShadowQuality`]).
pub(super) const SHADOW_MAP_SIZE: u32 = 4096;
/// The most shadow cascades that cover the view, each further out and
/// coarser than the one before. The graphics settings pick how many are in
/// use. Must match the size of `cascades` in shader.wgsl.
pub(super) const SHADOW_CASCADES: usize = 4;
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

/// A depth texture drawn from a light's views, one layer per view: the
/// sun's cascades ([`ShadowMapSize`]) or shadow-casting spot lights
/// ([`SpotShadowMapSize`]).
pub(super) struct ShadowMap<S> {
    /// Its size and layers, from the effective graphics settings.
    pub(super) size: S,
    /// A clone of [`ScenePipeline::shadow_maps_alive`] or
    /// `spot_shadow_maps_alive`, so maps still alive can be counted.
    pub(super) _alive: Arc<()>,
    /// Bytes of its depth texture.
    pub(super) bytes: u64,
    /// All the layers, for the shader to read.
    pub(super) array: wgpu::TextureView,
    /// One view per layer, to draw into.
    pub(super) layers: Vec<wgpu::TextureView>,
}

/// One spot light's shadow view, planned for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SpotShadow {
    /// Turns world space to the light's view (a perspective one).
    pub(super) view_proj: Mat4,
    /// A sphere around everything the beam reaches: casters outside it are
    /// skipped.
    pub(super) bounds: (Vec3, f32),
}

/// What the ambient occlusion passes read each frame. Must match `Occlusion`
/// in ssao.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct OcclusionUniform {
    /// Turns view space to clip space, and back.
    pub(super) projection: [[f32; 4]; 4],
    pub(super) inv_projection: [[f32; 4]; 4],
    /// Samples per pixel, the radius in studs, then unused.
    pub(super) settings: [f32; 4],
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
    /// Light given off, then how much of the item is drawn: 1 all of it, a
    /// fade in below 1 and a fade out above it (see [`super::lod`] and
    /// `dithered_out` in shader.wgsl).
    pub(super) emissive_fade: [f32; 4],
    /// Metalness, roughness, alpha cutoff, and studs per texture repeat
    /// (0 keeps the mesh's own coordinates).
    pub(super) surface: [f32; 4],
    /// Where the item's skin's joints start in the frame's joints, and how
    /// many it has: 0 for an item that isn't skinned (see `skinning.rs`).
    pub(super) skin: [u32; 2],
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
    /// The box it's cut to, in screen pixels (left, top, width, height):
    /// [`NO_CLIP`] when it isn't cut.
    pub(super) clip: [f32; 4],
    /// That box's corner radius, in pixels, and how far it's turned
    /// clockwise around its middle, in radians.
    pub(super) clip_shape: [f32; 2],
}

/// A clip box so big nothing on screen is outside it.
pub(super) const NO_CLIP: [f32; 4] = [-1.0e7, -1.0e7, 2.0e7, 2.0e7];

/// What a UI quad draws (`UiVertex::border[1]`, matched in `ui.wgsl`): a
/// filled, bordered box, a shadow behind one, or a shadow inside one.
pub(super) const UI_BOX: f32 = 0.0;
pub(super) const UI_SHADOW: f32 = 1.0;
pub(super) const UI_INSET_SHADOW: f32 = 2.0;

/// A running total of bytes on the GPU, shared by everything counted in it.
/// Cloning it shares the same total.
#[derive(Clone, Debug, Default)]
pub(super) struct GpuBytes(Arc<AtomicU64>);

impl GpuBytes {
    /// Adds `bytes` to the total until the returned [`Counted`] is dropped.
    pub(super) fn count(&self, bytes: u64) -> Counted {
        self.0.fetch_add(bytes, Ordering::Relaxed);
        Counted {
            bytes,
            total: self.0.clone(),
        }
    }

    pub(super) fn total(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Bytes counted in a [`GpuBytes`] total while this lives. It's kept with
/// the GPU object it measures, so freeing that object takes its bytes off
/// the total (Rust runs `Drop::drop` when a value goes away).
#[derive(Debug)]
pub(super) struct Counted {
    bytes: u64,
    total: Arc<AtomicU64>,
}

impl Counted {
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.total.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

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
    /// The box its corners span, for culling.
    pub(super) bounds: Bounds,
    /// Its corners' joints and weights, for an animated mesh.
    pub(super) skin: Option<skinning::GpuSkin>,
    /// Its buffers' size, in [`ScenePipeline::mesh_bytes`].
    pub(super) _bytes: Counted,
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
    /// Turns texture coordinates, read in the vertex shader: the columns of
    /// a 2×2 matrix (the material's rotation times its scale).
    pub(super) uv_matrix: [f32; 4],
    /// Added to the turned texture coordinates, then unused.
    pub(super) uv_offset: [f32; 4],
}

/// A texture uploaded to the GPU, with a view for each way it's read.
pub(super) struct GpuTexture {
    /// Kept so it lives as long as the bind groups that use it, and read
    /// from when the texture budget shrinks it.
    pub(super) texture: wgpu::Texture,
    /// Converts sRGB to linear light, for colors.
    pub(super) srgb: wgpu::TextureView,
    /// Gives the bytes as they are (128 is 0.5), for values.
    pub(super) linear: wgpu::TextureView,
    /// The largest level's width and height, and the number of levels, at
    /// the texture quality's size, before the budget left any out.
    pub(super) base: (u32, u32, u32),
    /// How many of those largest levels the texture budget left out.
    pub(super) dropped: u32,
    /// Its size with every mip level, in [`ScenePipeline::texture_bytes`].
    pub(super) bytes: Counted,
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
    pub(super) maps: [[u32; 4]; 9],
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
/// and the same kind of drawing (solid or see-through, one- or
/// double-sided), drawn with one draw call.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Batch {
    pub(super) mesh: MeshKey,
    pub(super) material: MaterialKey,
    pub(super) transparent: bool,
    /// Drawn without culling back faces ([`crate::Material::double_sided`]).
    pub(super) double_sided: bool,
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
    /// Into what's drawn into (the window or texture), or into
    /// [`FXAA_FORMAT`] while FXAA is on.
    pub(super) composite: wgpu::RenderPipeline,
    pub(super) composite_for_fxaa: wgpu::RenderPipeline,
    pub(super) fxaa: wgpu::RenderPipeline,
}

/// The depth and normal prepass's and the ambient occlusion passes'
/// pipelines, which don't depend on the scene's size.
pub(super) struct OcclusionPasses {
    /// Solid items' depth and view-space normals, with back faces culled or
    /// (double-sided) drawn. Terrain's is [`terrain::TerrainPass`]'s.
    pub(super) prepass: wgpu::RenderPipeline,
    pub(super) double_sided_prepass: wgpu::RenderPipeline,
    /// Depth, a second texture and the uniform, for both passes below.
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) uniform: wgpu::Buffer,
    /// Tests the hemisphere around each pixel, at half the scene's size.
    pub(super) occlusion: wgpu::RenderPipeline,
    /// Blurs that, without blurring across a change in depth.
    pub(super) blur: wgpu::RenderPipeline,
}

/// What the prepass draws into and ambient occlusion reads and writes,
/// while ambient occlusion is on.
pub(super) struct OcclusionTargets {
    /// Solid items' depth, at the scene's size, kept for reading.
    pub(super) depth_view: wgpu::TextureView,
    /// Their view-space normals, at the scene's size.
    pub(super) normal_view: wgpu::TextureView,
    /// Occlusion as tested, then blurred, at half the scene's size. The
    /// scene pass reads the blurred one.
    pub(super) raw_view: wgpu::TextureView,
    pub(super) blurred_view: wgpu::TextureView,
    /// Depth and normals, for the occlusion pass.
    pub(super) occlusion_group: wgpu::BindGroup,
    /// Depth and the raw occlusion, for the blur.
    pub(super) blur_group: wgpu::BindGroup,
    /// Bytes of its four textures.
    pub(super) bytes: u64,
    /// A clone of [`TargetsAlive::occlusion`].
    pub(super) _alive: Arc<()>,
}

/// One more strong reference than there are of each kind of scene target
/// alive, so tests can tell that none leaked.
#[derive(Default)]
pub(super) struct TargetsAlive {
    /// Sets of [`FrameTargets`].
    pub(super) sets: Arc<()>,
    pub(super) msaa: Arc<()>,
    pub(super) fxaa: Arc<()>,
    /// Sets of [`OcclusionTargets`].
    pub(super) occlusion: Arc<()>,
}

/// The multisampled color the scene is drawn into while MSAA is on. Its
/// samples are averaged ("resolved") into [`FrameTargets::hdr_view`] at the
/// end of the scene pass, and then thrown away.
pub(super) struct MsaaTarget {
    pub(super) view: wgpu::TextureView,
    /// A clone of [`TargetsAlive::msaa`].
    pub(super) _alive: Arc<()>,
}

/// The tone-mapped picture at the output's size while FXAA is on, and the
/// FXAA pass's binding of it.
pub(super) struct FxaaTarget {
    pub(super) view: wgpu::TextureView,
    pub(super) group: wgpu::BindGroup,
    /// A clone of [`TargetsAlive::fxaa`].
    pub(super) _alive: Arc<()>,
}

/// Every target a frame draws into, at the scene's size (the output's times
/// the render scale) unless said otherwise: the scene's HDR color and depth,
/// the multisampled and FXAA targets, the prepass and ambient occlusion
/// targets, and the bloom levels. Each optional one exists only while the
/// effective graphics settings use it, and all are remade together.
pub(super) struct FrameTargets {
    /// The scene's size in pixels, and the output's it was made for.
    pub(super) size: (u32, u32),
    pub(super) output: (u32, u32),
    /// Samples per pixel of the scene's color and depth.
    pub(super) samples: u32,
    /// Bytes of every texture here, those in `occlusion` included.
    pub(super) bytes: u64,
    /// A clone of [`TargetsAlive::sets`], so sets of targets still alive
    /// can be counted.
    pub(super) _alive: Arc<()>,
    /// Where the scene is drawn, or with MSAA where it's resolved.
    pub(super) hdr_view: wgpu::TextureView,
    /// Only while MSAA is on.
    pub(super) msaa: Option<MsaaTarget>,
    /// The scene's depth, with `samples` per pixel, thrown away after the
    /// scene pass.
    pub(super) depth_view: wgpu::TextureView,
    /// Only while FXAA is on.
    pub(super) fxaa: Option<FxaaTarget>,
    /// Only while ambient occlusion is on.
    pub(super) occlusion: Option<OcclusionTargets>,
    /// Bloom levels from half the scene's size down, each half the one before.
    pub(super) levels: Vec<wgpu::TextureView>,
    /// Scene to the first level.
    pub(super) prefilter_group: wgpu::BindGroup,
    /// Level `i` is made from level `i - 1` (the entry for level 0 is unused).
    pub(super) downsample_groups: Vec<wgpu::BindGroup>,
    /// Level `i` is added into level `i - 1`.
    pub(super) upsample_groups: Vec<wgpu::BindGroup>,
    pub(super) composite_group: wgpu::BindGroup,
}

/// The scene pass's pipelines for parts and models: solid or see-through,
/// each with back faces culled or, for double-sided materials, drawn.
pub(super) struct ItemPipelines {
    pub(super) opaque: wgpu::RenderPipeline,
    pub(super) transparent: wgpu::RenderPipeline,
    pub(super) double_sided: wgpu::RenderPipeline,
    pub(super) double_sided_transparent: wgpu::RenderPipeline,
}

impl ItemPipelines {
    /// The pipeline that draws `batch`.
    pub(super) fn of(&self, batch: &Batch) -> &wgpu::RenderPipeline {
        match (batch.transparent, batch.double_sided) {
            (false, false) => &self.opaque,
            (true, false) => &self.transparent,
            (false, true) => &self.double_sided,
            (true, true) => &self.double_sided_transparent,
        }
    }
}

/// The GPU objects for drawing a scene, whatever it's drawn into. Both
/// [`Renderer`] and [`TextureRenderer`] own one.
pub(super) struct ScenePipeline {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) sky_pipeline: wgpu::RenderPipeline,
    pub(super) item_pipelines: ItemPipelines,
    /// Camera-facing pictures, such as particles.
    pub(super) sprites: sprites::SpritePass,
    pub(super) post: PostPasses,
    /// The scene's targets, remade when the scene's size, the
    /// anti-aliasing or the ambient occlusion changes.
    pub(super) frame_targets: FrameTargets,
    /// The prepass and ambient occlusion pipelines.
    pub(super) occlusion: OcclusionPasses,
    /// The sky's environment maps, baked when the sky changes.
    pub(super) environment: environment::Environment,
    /// A 1×1 white texture, bound in place of the ambient occlusion while
    /// it's off, and the sampler the scene pass reads occlusion with.
    pub(super) no_occlusion_view: wgpu::TextureView,
    pub(super) occlusion_sampler: wgpu::Sampler,
    pub(super) targets_alive: TargetsAlive,
    /// The size of what's drawn into (the window or texture), in pixels.
    pub(super) output_size: (u32, u32),
    pub(super) light_buffer: wgpu::Buffer,
    pub(super) tile_buffer: wgpu::Buffer,
    pub(super) ui_pipeline: wgpu::RenderPipeline,
    pub(super) ui_image_pipeline: wgpu::RenderPipeline,
    /// Draws turned text from [`TurnedText`]'s sheet, whose colours are
    /// premultiplied by their alpha.
    pub(super) ui_sheet_pipeline: wgpu::RenderPipeline,
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
    /// Turned text, drawn upright into a sheet first.
    pub(super) turned_text: TurnedText,
    /// The format of what's drawn into, which the sheet shares.
    pub(super) color_format: wgpu::TextureFormat,
    pub(super) frame_buffer: wgpu::Buffer,
    /// The frame bindings with the shadow maps alive and the ambient
    /// occlusion, and the empty 1×1 stand-in for each one that isn't.
    /// Remade when any of them changes.
    pub(super) frame_bind_group: wgpu::BindGroup,
    pub(super) frame_layout: wgpu::BindGroupLayout,
    /// A 1×1 depth array, bound in place of a shadow map that isn't alive.
    pub(super) empty_shadow_view: wgpu::TextureView,
    pub(super) shadow_sampler: wgpu::Sampler,
    pub(super) shadow_pipeline: wgpu::RenderPipeline,
    /// Each shadow view's matrix, [`CASCADE_UNIFORM_STRIDE`] apart: the sun's
    /// cascades, then the shadow-casting spot lights'.
    pub(super) cascade_buffer: wgpu::Buffer,
    /// Gives the shadow pass one view's matrix (it can't also bind the
    /// shadow map it's drawing into).
    pub(super) shadow_bind_group: wgpu::BindGroup,
    /// What `shadow_bind_group` is made with, remade with the joints.
    pub(super) shadow_layout: wgpu::BindGroupLayout,
    /// Animated meshes' bindings, and the frame's joints.
    pub(super) skinning: skinning::Skinning,
    /// Made when shadows are first turned on, and freed when turned off or
    /// when the graphics settings change its size.
    pub(super) shadow_map: Option<ShadowMap<ShadowMapSize>>,
    /// One more strong reference than there are shadow maps alive.
    pub(super) shadow_maps_alive: Arc<()>,
    /// Made while a spot light casts shadows and the graphics settings
    /// allow it, and freed when none does.
    pub(super) spot_shadow_map: Option<ShadowMap<SpotShadowMapSize>>,
    pub(super) spot_shadow_maps_alive: Arc<()>,
    /// What the device can do, read once.
    pub(super) capabilities: Capabilities,
    /// The graphics settings last asked for, and what's used of them.
    pub(super) settings: GraphicsSettings,
    pub(super) graphics: EffectiveGraphics,
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
    /// `textures` are sized by the texture quality.
    pub(super) custom_meshes: HashMap<u64, Cached<GpuMesh>>,
    pub(super) textures: HashMap<u64, Cached<GpuTexture>>,
    /// Textures that keep their full size whatever the texture quality: UI
    /// images, and terrain weights (which say where each layer is painted).
    pub(super) full_textures: HashMap<u64, Cached<GpuTexture>>,
    pub(super) materials: HashMap<MaterialKey, Cached<GpuMaterial>>,
    /// Terrain chunks' pipeline, layer textures and bindings.
    pub(super) terrain: terrain::TerrainPass,
    /// How many frames have been drawn, to tell what wasn't drawn lately.
    pub(super) frame: u64,
    /// Only on devices that can take timestamps (the editor asks for them).
    pub(super) gpu_timer: Option<gpu_timer::GpuTimer>,
    /// What the last frame drew.
    pub(super) counts: FrameCounts,
    /// While set, items are culled as if seen from this camera, not the
    /// draw list's (see [`TextureRenderer::freeze_culling`]).
    pub(super) frozen_view: Option<crate::Camera>,
    /// Each item's bounds and whether it was drawn, kept while
    /// `frozen_view` is set.
    pub(super) culling_bounds: Vec<CullingBounds>,
    /// Each item's level of detail, kept between frames.
    pub(super) lods: LodStates,
    /// Bytes of textures and of meshes on the GPU.
    pub(super) texture_bytes: GpuBytes,
    pub(super) mesh_bytes: GpuBytes,
}

/// What a frame drew, for [`RenderStats`].
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FrameCounts {
    /// Indexed draws, the shadow passes' and prepass's included.
    pub(super) draw_calls: u32,
    /// Triangles those draws sent, counting every instance.
    pub(super) triangles: u64,
    pub(super) items_submitted: u32,
    /// Instances drawn in the scene pass.
    pub(super) items_drawn: u32,
    /// Instanced draws in the scene pass.
    pub(super) batches: u32,
    /// Items outside the camera's view.
    pub(super) items_culled: u32,
    /// Casters drawn into shadow views, and those left out of a view they
    /// couldn't reach, counted once per view.
    pub(super) shadow_casters_drawn: u32,
    pub(super) shadow_casters_culled: u32,
    pub(super) sprites_culled: u32,
}

/// Turned UI text: glyphon only draws text upright, so each turned text is
/// drawn upright into its own place on a sheet, which is then drawn as a
/// turned picture where the text goes.
pub(super) struct TurnedText {
    pub(super) renderer: GlyphTextRenderer,
    pub(super) viewport: TextViewport,
    /// The sheet, its view's bind group, and its size. Made when first
    /// needed and grown, never shrunk.
    pub(super) sheet: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
}
