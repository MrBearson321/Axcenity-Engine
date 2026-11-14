//! The renderer's plain data: what scripts, the network code and the
//! dedicated server share with `render` without linking a GPU library
//! (roadmap M8). `render` re-exports all of it, so the types have one name
//! everywhere: `axcenity_render::GraphicsSettings` is
//! `axcenity_render_types::GraphicsSettings`.

mod camera;
mod graphics;

pub use camera::Camera;
pub use graphics::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DecalLimits, DepthOfFieldQuality,
    EffectiveGraphics, EnvironmentMapSize, GraphicsSettings, MotionBlurQuality, POINT_SHADOW_FACES,
    PointShadowMapSize, Quality, RAY_TRACING_DISTANCE_RANGE, RENDER_SCALE_RANGE,
    RayTracedAmbientOcclusion, RayTracedGlobalIllumination, RayTracedReflections, RayTracedShadows,
    RealtimeGiTier, ReflectionQuality, ShadowMapSize, ShadowQuality, SpotShadowMapSize,
    TEXTURE_BUDGET_RANGE, TextureQuality,
};
