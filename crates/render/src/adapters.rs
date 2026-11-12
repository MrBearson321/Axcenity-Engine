//! Which graphics card and API to draw with.
//!
//! On Windows the renderer prefers DirectX 12 to Vulkan. With wgpu 30,
//! Vulkan on an AMD RX 9070 XT drew the owner's projects 25–75% slower on
//! the GPU (and up to 177% slower a frame), while DirectX 12 matched what
//! wgpu 25 managed (`docs/rendering/pipeline.md`, "Graphics API").
//! `WGPU_BACKEND=vulkan` (or `dx12`, `metal`, `gl`) picks one by hand.

/// A wgpu instance for every API this machine has, or only the ones
/// `WGPU_BACKEND` names.
pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
}

/// The graphics card to draw with: the best of `instance`'s that can draw
/// to `surface` (when there is one), by [`preference`]. Falls back to
/// wgpu's own choice if none is listed.
pub async fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, wgpu::RequestAdapterError> {
    let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
    if let Some(best) = pick(&adapters, surface) {
        return Ok(best);
    }
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            force_fallback_adapter: false,
            // Bucketing hides a GPU's exact limits from untrusted web
            // content; a desktop engine wants the real ones.
            apply_limit_buckets: false,
        })
        .await
}

/// The best of `adapters` that can draw to `surface`, by [`preference`];
/// for eframe's adapter selector too.
pub fn pick(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Option<wgpu::Adapter> {
    adapters
        .iter()
        .filter(|adapter| surface.is_none_or(|surface| adapter.is_surface_supported(surface)))
        .max_by_key(|adapter| preference(&adapter.get_info(), cfg!(target_os = "windows")))
        .cloned()
}

/// How much an adapter is wanted: a real graphics card before a built-in
/// or software one, then (`windows`) DirectX 12 before Vulkan before the
/// rest, elsewhere Vulkan or Metal before the rest. Higher is better.
fn preference(info: &wgpu::AdapterInfo, windows: bool) -> (u8, u8) {
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 3,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 1,
        wgpu::DeviceType::Cpu => 0,
    };
    let api = match info.backend {
        wgpu::Backend::Dx12 if windows => 3,
        wgpu::Backend::Vulkan | wgpu::Backend::Metal => 2,
        wgpu::Backend::Dx12 => 1,
        _ => 0,
    };
    (kind, api)
}

/// Adds hardware ray tracing to `descriptor` when `adapter` has it: wgpu's
/// ray queries (an experimental wgpu feature) and the adapter's
/// acceleration structure limits. Ray traced effects are off until a
/// setting turns one on (`docs/rendering/ray-tracing.md`), but the device
/// must be opened with the feature for them to be turned on later.
/// `AXCENITY_RAY_TRACING=0` opens devices without it.
// The one unsafe call is wgpu's opt-in to its experimental features.
#[allow(unsafe_code)]
pub fn with_ray_queries(adapter: &wgpu::Adapter, descriptor: &mut wgpu::DeviceDescriptor<'_>) {
    let wanted = std::env::var("AXCENITY_RAY_TRACING").map_or(true, |value| value != "0");
    if !wanted
        || !adapter
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        return;
    }
    descriptor.required_features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
    descriptor.required_limits = descriptor
        .required_limits
        .clone()
        .using_acceleration_structure_values(adapter.limits());
    // SAFETY: wgpu marks ray queries experimental: its API may change and
    // misuse may not be caught. The renderer only builds acceleration
    // structures and traces rays while a ray traced effect is on, through
    // `renderer/ray_tracing.rs`, whose tests run on every backend we have.
    descriptor.experimental_features = unsafe { wgpu::ExperimentalFeatures::enabled() };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(device_type: wgpu::DeviceType, backend: wgpu::Backend) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo::new(device_type, backend)
    }

    #[test]
    fn windows_picks_directx_12_and_a_real_card_comes_first() {
        let card = wgpu::DeviceType::DiscreteGpu;
        let dx12 = preference(&info(card, wgpu::Backend::Dx12), true);
        let vulkan = preference(&info(card, wgpu::Backend::Vulkan), true);
        assert!(dx12 > vulkan);
        assert!(
            preference(&info(card, wgpu::Backend::Vulkan), false)
                > preference(&info(card, wgpu::Backend::Gl), false)
        );
        let built_in = preference(
            &info(wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Dx12),
            true,
        );
        assert!(
            vulkan > built_in,
            "a discrete card on Vulkan beats a built-in one on DX12"
        );
        let software = preference(&info(wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan), false);
        assert!(software < preference(&info(card, wgpu::Backend::Gl), false));
    }
}
