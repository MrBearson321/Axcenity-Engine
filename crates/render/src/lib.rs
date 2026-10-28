//! Axcenity renderer.
//!
//! Callers describe a frame as a [`DrawList`] and hand it to
//! [`Renderer::render`]. This crate never sees Instances, scripts or windows:
//! anything that can provide a window handle (such as a `winit` window) can be
//! drawn into with [`Renderer`], and [`TextureRenderer`] draws into a texture
//! on a GPU device someone else set up (the editor's viewport).

mod atmosphere;
mod draw_list;
mod fonts;
mod graphics;
mod mesh;
mod renderer;
mod tone_mapping;

pub use atmosphere::Atmosphere;
pub use draw_list::{
    AmbientLight, Camera, Channel, ChannelMap, Cone, DrawItem, DrawList, Fog, Light,
    LightProbeGrid, LightProbeVolume, Material, MeshSource, NormalMap, PointShadows,
    PostProcessing, ProbeImage, ReflectionProbe, SURFACE_PARAMETERS, Shadows, Shape, Skin, Sky,
    SkyImage, SpotShadows, Sprite, Sun, TerrainLayer, TerrainLayers, TerrainSurface, TerrainView,
    TextAlign, TextShadow, TextSpan, TextStroke, TextureMaps, TextureSource, UiBorderSide, UiClip,
    UiDrawItem, UiImage, UiRectangle, UiShadow, UiSlice, UiText, VerticalAlign, is_turned,
    turn_point, turned_bounds,
};
pub use fonts::{BUILT_IN_FONTS, BuiltInFont, UiFont, font_families};
pub use graphics::{
    AmbientOcclusion, AntiAliasing, Capabilities, EffectiveGraphics, EnvironmentMapSize,
    GraphicsSettings, POINT_SHADOW_FACES, PointShadowMapSize, Quality, RENDER_SCALE_RANGE,
    ReflectionQuality, ShadowMapSize, ShadowQuality, SpotShadowMapSize, TEXTURE_BUDGET_RANGE,
    TextureQuality,
};
pub use renderer::{
    CullingBounds, FramePass, LIGHT_PROBE_BAKE_SIZE, PROBE_BAKE_SIZE, RenderStats, Renderer,
    ShaderError, SurfaceShader, TextureRenderer,
};
pub use tone_mapping::ToneMapping;

/// Re-exported so callers build transforms with the same `glam` version.
pub use glam;
/// Re-exported so callers that pass in GPU devices use the same `wgpu` version.
pub use wgpu;
