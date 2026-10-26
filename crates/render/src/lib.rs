//! Axcenity renderer.
//!
//! Callers describe a frame as a [`DrawList`] and hand it to
//! [`Renderer::render`]. This crate never sees Instances, scripts or windows:
//! anything that can provide a window handle (such as a `winit` window) can be
//! drawn into with [`Renderer`], and [`TextureRenderer`] draws into a texture
//! on a GPU device someone else set up (the editor's viewport).

mod draw_list;
mod fonts;
mod graphics;
mod mesh;
mod renderer;

pub use draw_list::{
    AmbientLight, Camera, Channel, ChannelMap, Cone, DrawItem, DrawList, Light, Material,
    MeshSource, NormalMap, PostProcessing, Shadows, Shape, Sky, Sprite, Sun, TerrainLayer,
    TerrainLayers, TerrainSurface, TerrainView, TextAlign, TextShadow, TextSpan, TextStroke,
    TextureMaps, TextureSource, UiBorderSide, UiClip, UiDrawItem, UiImage, UiRectangle, UiShadow,
    UiSlice, UiText, VerticalAlign, is_turned, turn_point, turned_bounds,
};
pub use fonts::{BUILT_IN_FONTS, BuiltInFont, UiFont, font_families};
pub use graphics::{
    Capabilities, EffectiveGraphics, GraphicsSettings, Quality, ShadowMapSize, ShadowQuality,
};
pub use renderer::{RenderStats, Renderer, TextureRenderer};

/// Re-exported so callers build transforms with the same `glam` version.
pub use glam;
/// Re-exported so callers that pass in GPU devices use the same `wgpu` version.
pub use wgpu;
