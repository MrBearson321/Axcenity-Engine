// Ray traced water reflections (LE5, renderer/water.rs,
// docs/rendering/water.md): where the screen's reflection finds nothing,
// a ray from the water along the reflected way through the ray traced
// scene. What it hits is lit as ray traced GI lights a hit (ray_gi.wgsl's
// `gi_hit_light`): its color (with its texture's average) and glow, the
// sun with a ray toward it for its shadow, the lamps in reach, the flat
// ambient light and the light probes. Rays don't give the triangle's own
// normal, so a hit faces back along the ray. Compiled after water.wgsl,
// with `enable wgpu_ray_query;` put first (renderer/gpu.rs).

// Must match `TracedWaterUniform` in renderer/water.rs.
struct TracedWater {
    // x: how far the rays reach in studs, y: how far a hit's shadow rays
    // start off it, zw: unused.
    settings: vec4<f32>,
}

@group(3) @binding(0) var water_scene: acceleration_structure;
// Two a TLAS slot (renderer/ray_tracing.rs `SurfaceRaw`): rgb its color and
// w 1 for a cut-out, then rgb its glow, both linear.
@group(3) @binding(1) var<storage, read> water_surfaces: array<vec4<f32>>;
@group(3) @binding(2) var<uniform> traced_water: TracedWater;

// Instance masks (renderer/ray_tracing.rs): solid items and cut-outs,
// casting shadows (1, 2) or not (8, 16). See-through items (and the water
// itself) aren't seen.
const WATER_MASK_SURFACES: u32 = 27u;
const WATER_MASK_CASTERS: u32 = 3u;
// The most lamps one hit is lit by: the first so many of the frame's.
const WATER_MOST_LAMPS: u32 = 8u;

fn water_ray_blocked(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    var query: ray_query;
    rayQueryInitialize(&query, water_scene, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, WATER_MASK_CASTERS, 0.0,
        distance, origin, direction));
    rayQueryProceed(&query);
    return rayQueryGetCommittedIntersection(&query).kind != RAY_QUERY_INTERSECTION_NONE;
}

// The light leaving a surface at `position` facing `n` of `color` (linear).
fn water_hit_light(position: vec3<f32>, n: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let lift = n * traced_water.settings.y;
    var light = vec3<f32>(0.0);
    let l = frame.sun_direction.xyz;
    let n_dot_l = dot(n, l);
    if n_dot_l > 0.0 && (frame.shadow.x < 0.5 || !water_ray_blocked(position + lift, l, 1e6)) {
        light += frame.sun_radiance.rgb * n_dot_l / PI
            * cloud_shadow(frame.clouds, position, l);
    }
    let lamps = min(frame.lights.x, WATER_MOST_LAMPS);
    for (var i = 0u; i < lamps; i++) {
        let lamp = lights[i];
        let reach = lamp_reach(lamp, position);
        if reach.w <= 0.0 {
            continue;
        }
        let distance = length(reach.xyz);
        let toward = reach.xyz / max(distance, 0.0001);
        let facing = dot(n, toward);
        if facing <= 0.0 {
            continue;
        }
        if lamp.shadow.x >= 0.0 && water_ray_blocked(position + lift, toward, max(distance - 0.05, 0.0)) {
            continue;
        }
        light += lamp.radiance_cone.rgb * reach.w * facing / PI;
    }
    light += frame.ambient.rgb + surrounding_diffuse(position, n, frame.sky_top.w);
    return color * light;
}

// What's seen reflected from `origin` along `dir` by a ray: rgb its light
// (fogged on the way), a 1 where the ray hit something, 0 where it found
// nothing within the reach (the sky, as `reflected` gives it).
fn traced_reflection(origin: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    var query: ray_query;
    rayQueryInitialize(&query, water_scene, RayDesc(
        RAY_FLAG_FORCE_OPAQUE, WATER_MASK_SURFACES, 0.05, traced_water.settings.x, origin, dir));
    while rayQueryProceed(&query) {}
    let hit = rayQueryGetCommittedIntersection(&query);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return vec4<f32>(0.0);
    }
    let surface = 2u * hit.instance_custom_data;
    let color = water_surfaces[surface];
    let glow = water_surfaces[surface + 1u].rgb;
    let position = origin + dir * hit.t;
    let light = glow + water_hit_light(position, -dir, color.rgb);
    return vec4<f32>(fogged(frame.fog, origin, position, light), 1.0);
}
