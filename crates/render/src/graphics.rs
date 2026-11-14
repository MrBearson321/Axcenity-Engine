//! The GPU side of the graphics settings. The settings themselves are plain
//! data in `render-types` (roadmap M8), re-exported here so the renderer
//! keeps naming them `crate::graphics::…`.

pub use axcenity_render_types::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DecalLimits, DepthOfFieldQuality,
    EffectiveGraphics, EnvironmentMapSize, GraphicsSettings, MotionBlurQuality, POINT_SHADOW_FACES,
    PointShadowMapSize, Quality, RAY_TRACING_DISTANCE_RANGE, RENDER_SCALE_RANGE,
    RayTracedAmbientOcclusion, RayTracedGlobalIllumination, RayTracedReflections, RayTracedShadows,
    RealtimeGiTier, ReflectionQuality, ShadowMapSize, ShadowQuality, SpotShadowMapSize,
    TEXTURE_BUDGET_RANGE, TextureQuality,
};

/// The formats of the scene's HDR color and its depth, which multisampling
/// must work for. Must match `HDR_FORMAT` and `DEPTH_FORMAT` in the renderer.
pub(crate) const SCENE_FORMATS: [wgpu::TextureFormat; 2] = [
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Depth32Float,
];

/// Reads what `device` can do: its limits and features. `adapter` (when
/// known) says whether it draws in software.
pub fn read_capabilities(
    device: &wgpu::Device,
    adapter: Option<&wgpu::AdapterInfo>,
) -> Capabilities {
    let limits = device.limits();
    Capabilities {
        max_texture_size: limits.max_texture_dimension_2d,
        max_texture_layers: limits.max_texture_array_layers,
        timestamp_queries: device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        software: adapter.is_some_and(|info| info.device_type == wgpu::DeviceType::Cpu),
        msaa_samples: msaa_samples(device.features()),
        // Devices limited to WebGL 2 report no compute workgroups.
        compute_shaders: limits.max_compute_workgroups_per_dimension > 0
            && limits.max_compute_invocations_per_workgroup >= 64
            && limits.max_storage_buffers_per_shader_stage > 0,
        ray_queries: device
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY),
        // The device's own adapter was the last one `with_ray_queries` saw.
        ray_tracing_offered: device
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            || crate::adapters::adapter_can_ray_trace(),
    }
}

/// The sample counts both [`SCENE_FORMATS`] support, as bits. Devices are
/// opened without wgpu's adapter-specific format features, so wgpu checks
/// textures against the formats' guaranteed features: these.
fn msaa_samples(features: wgpu::Features) -> u32 {
    [1, 2, 4, 8, 16]
        .into_iter()
        .filter(|&count| {
            SCENE_FORMATS.iter().all(|format| {
                format
                    .guaranteed_format_features(features)
                    .flags
                    .sample_count_supported(count)
            })
        })
        .fold(0, |bits, count| bits | count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Capabilities::default` writes wgpu's numbers out by hand (so
    /// `render-types` needs no wgpu); they must stay wgpu's.
    #[test]
    fn default_capabilities_are_wgpus_defaults() {
        let limits = wgpu::Limits::default();
        let defaults = Capabilities::default();
        assert_eq!(defaults.max_texture_size, limits.max_texture_dimension_2d);
        assert_eq!(defaults.max_texture_layers, limits.max_texture_array_layers);
        assert_eq!(defaults.msaa_samples, msaa_samples(wgpu::Features::empty()));
    }
}
