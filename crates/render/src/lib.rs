//! Axcenity renderer.
//!
//! Callers describe a frame as a [`DrawList`] and hand it to
//! [`Renderer::render`]. This crate never sees Instances, scripts or windows:
//! anything that can provide a window handle (such as a `winit` window) can be
//! drawn into with [`Renderer`], and [`TextureRenderer`] draws into a texture
//! on a GPU device someone else set up (the editor's viewport).

pub mod adapters;
mod atmosphere;
mod decal;
mod draw_list;
mod fonts;
mod graphics;
mod mesh;
mod overdraw;
mod renderer;
mod tone_mapping;
mod water;

pub use atmosphere::Atmosphere;
pub use axcenity_render_types::Camera;
pub use decal::{Decal, DecalRole, decal_quad};
pub use draw_list::{
    AmbientLight, CURVE_ENTRIES, Channel, ChannelMap, Clouds, ColorGrading, ColorLookup, Cone,
    DepthOfField, DrawItem, DrawList, Edges, FilmGrain, Fog, GridPlane, GridTint, ItemChunk,
    LensEffects, Light, LightProbeGrid, LightProbeVolume, MAX_PALETTE_COLORS, Material, MeshSource,
    NormalMap, Outline, ParticleCurves, ParticleEmitter, ParticleLook, PointShadows,
    PostProcessing, ProbeImage, RectArea, ReflectionProbe, SURFACE_PARAMETERS, Shading, Shadows,
    Shape, Skin, Sky, SkyImage, SpawnKind, SpawnShape, SpotShadows, Sprite, Sun, Sway,
    TerrainLayer, TerrainLayers, TerrainSurface, TerrainView, TextAlign, TextShadow, TextSpan,
    TextStroke, TextureMaps, TextureSource, Toon, UiBorderSide, UiClip, UiDrawItem, UiImage,
    UiRectangle, UiShadow, UiSlice, UiText, VerticalAlign, Vignette, Weather, curve_at, is_turned,
    turn_point, turned_bounds,
};
pub use fonts::{BUILT_IN_FONTS, BuiltInFont, UiFont, font_families};
pub use graphics::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DecalLimits, DepthOfFieldQuality,
    EffectiveGraphics, EnvironmentMapSize, GraphicsSettings, MotionBlurQuality, POINT_SHADOW_FACES,
    PointShadowMapSize, Quality, RAY_TRACING_DISTANCE_RANGE, RENDER_SCALE_RANGE,
    RayTracedAmbientOcclusion, RayTracedGlobalIllumination, RayTracedShadows, ReflectionQuality,
    ShadowMapSize, ShadowQuality, SpotShadowMapSize, TEXTURE_BUDGET_RANGE, TextureQuality,
    read_capabilities,
};
pub use overdraw::{MAX_LAYERS, OVERDRAW_BANDS, OVERDRAW_STEP, OverdrawStats, layers_of};
#[cfg(feature = "debug-views")]
pub use renderer::DebugView;
pub use renderer::{
    CullingBounds, FramePass, LIGHT_PROBE_BAKE_SIZE, LOD_LEVELS, LOD_SCREEN_SIZES, LightProbeBaker,
    MAX_GPU_PARTICLES, PROBE_BAKE_SIZE, RayTracingStats, RenderStats, Renderer,
    SHADOW_CASCADE_COUNT, ShaderError, SurfaceShader, TextureRenderer,
};
pub use tone_mapping::ToneMapping;
pub use water::{WATER_GRID, Water, water_surface};

/// Re-exported so callers build transforms with the same `glam` version.
pub use glam;
/// Re-exported so callers that pass in GPU devices use the same `wgpu` version.
pub use wgpu;
