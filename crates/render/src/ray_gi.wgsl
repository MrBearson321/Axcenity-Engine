// Ray traced global illumination (RT2, renderer/ray_gi.rs,
// docs/rendering/ray-tracing.md): a batch of light probe volumes' probes,
// each traced with rays in every direction through the ray traced scene,
// summed into the four spherical harmonics coefficients the scene reads
// (`surrounding_diffuse`) and blended over what the probe had, so the light
// settles over a few updates instead of flickering.
//
// What a ray brings back: the sky (without the sun's disc) where it hits
// nothing; where it hits something, that item's glow plus its color times
// the light reaching it there: the sun (with a ray toward it for its
// shadow), the lamps in reach (each with a ray for its shadow), the flat
// ambient light, and the light probes themselves as last updated, so light
// bounces again each update (many bounces over a second or so).
//
// Compiled after `sky.wgsl`, `fog.wgsl`, `clouds.wgsl` and `lighting.wgsl`
// (for the frame's bindings, its sky and its probes), with
// `enable wgpu_ray_query;` put first (renderer/gpu.rs).

// Threads a probe's rays are shared between, one workgroup a probe.
const GI_THREADS: u32 = 64u;
// Instance masks (renderer/ray_tracing.rs): solid items, cut-outs and items
// that don't cast shadows, solid (8) or cut-out (16). See-through ones let
// light through.
const GI_MASK_SURFACES: u32 = 27u;
// Solid items and cut-outs: what casts shadows.
const GI_MASK_CASTERS: u32 = 3u;
// The most lamps one hit is lit by: the first so many of the frame's.
const GI_MOST_LAMPS: u32 = 16u;

// Must match `RayGiUniform` in renderer/ray_gi.rs.
struct RayGi {
    // x: probes in this batch, y: rays from each, z: a number that changes
    // each update (turns the rays), w: unused.
    counts: vec4<u32>,
    // x: how much of what a probe had is kept (0 to 1), y: how far rays
    // reach in studs, z: how far a hit's shadow rays start off it, w: unused.
    settings: vec4<f32>,
}

@group(1) @binding(0) var gi_scene: acceleration_structure;
@group(1) @binding(1) var<uniform> gi: RayGi;
// xyz: where each probe of the batch stands, w: its place in
// `light_probes` (the bits of a u32).
@group(1) @binding(2) var<storage, read> gi_batch: array<vec4<f32>>;
// Two a TLAS slot (renderer/ray_tracing.rs `SurfaceRaw`): rgb its color
// (with its texture's average), then rgb its glow, both linear.
@group(1) @binding(3) var<storage, read> gi_surfaces: array<vec4<f32>>;
// Four a probe of the batch: what's copied over its place in
// `light_probes` once this pass is done.
@group(1) @binding(4) var<storage, read_write> gi_traced: array<vec4<f32>>;

var<workgroup> gi_sums: array<array<vec3<f32>, 4>, GI_THREADS>;

// Direction `i` of `count` spread evenly over the sphere (a spherical
// Fibonacci spiral), turned by `turn`.
fn gi_direction(i: u32, count: u32, turn: f32) -> vec3<f32> {
    let golden = 2.39996323;
    let y = 1.0 - (f32(i) + 0.5) * 2.0 / f32(count);
    let r = sqrt(max(1.0 - y * y, 0.0));
    let a = f32(i) * golden + turn;
    let d = vec3<f32>(cos(a) * r, y, sin(a) * r);
    // Tipped a different way each update too, so the poles move.
    let tip = turn * 0.6180339;
    let c = cos(tip);
    let s = sin(tip);
    return vec3<f32>(d.x, c * d.y - s * d.z, s * d.y + c * d.z);
}

// Whether anything casts a shadow between `origin` and `distance` along
// `direction`.
fn gi_blocked(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    var query: ray_query;
    rayQueryInitialize(&query, gi_scene, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, GI_MASK_CASTERS, 0.0,
        distance, origin, direction));
    rayQueryProceed(&query);
    return rayQueryGetCommittedIntersection(&query).kind != RAY_QUERY_INTERSECTION_NONE;
}

// The light leaving a surface at `position` facing `n` of `color` (linear)
// toward where the ray came from.
fn gi_hit_light(position: vec3<f32>, n: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let lift = n * gi.settings.z;
    // The sun, if it reaches.
    var light = vec3<f32>(0.0);
    let l = frame.sun_direction.xyz;
    let n_dot_l = dot(n, l);
    if n_dot_l > 0.0 && frame.shadow.x > 0.5 {
        if !gi_blocked(position + lift, l, 1e6) {
            light += frame.sun_radiance.rgb * n_dot_l / PI;
        }
    } else if n_dot_l > 0.0 {
        light += frame.sun_radiance.rgb * n_dot_l / PI;
    }
    // The lamps in reach.
    let lamps = min(frame.lights.x, GI_MOST_LAMPS);
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
        if lamp.shadow.x >= 0.0 && gi_blocked(position + lift, toward, max(distance - 0.05, 0.0)) {
            continue;
        }
        light += lamp.radiance_cone.rgb * reach.w * facing / PI;
    }
    // The flat ambient light, and what the probes last found here.
    light += frame.ambient.rgb + surrounding_diffuse(position, n, frame.sky_top.w);
    return color * light;
}

// What a probe at `origin` sees looking along `direction`.
fn gi_ray_light(origin: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {
    var query: ray_query;
    rayQueryInitialize(&query, gi_scene, RayDesc(
        RAY_FLAG_FORCE_OPAQUE, GI_MASK_SURFACES, 0.0, gi.settings.y, origin, direction));
    while rayQueryProceed(&query) {}
    let hit = rayQueryGetCommittedIntersection(&query);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        // The sky as drawn, without the sun's disc (the sun lights what
        // rays hit). Not times the environment intensity: a probe's light
        // stands in for the sky's on surfaces, as a bake's does.
        return sky_specular(direction, 0.2);
    }
    if !hit.front_face {
        // Inside something: no light.
        return vec3<f32>(0.0);
    }
    let surface = 2u * hit.instance_custom_data;
    let color = gi_surfaces[surface].rgb;
    let glow = gi_surfaces[surface + 1u].rgb;
    // Facing back along the ray: the ray tracing doesn't give the
    // triangle's own normal.
    let position = origin + direction * hit.t;
    return glow + gi_hit_light(position, -direction, color);
}

@compute @workgroup_size(64)
fn cs_trace_probes(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    if group.x >= gi.counts.x {
        return;
    }
    let probe = gi_batch[group.x];
    let origin = probe.xyz;
    let rays = gi.counts.y;
    let turn = f32(gi.counts.z % 1024u) * 0.7548777;
    var c0 = vec3<f32>(0.0);
    var c1 = vec3<f32>(0.0);
    var c2 = vec3<f32>(0.0);
    var c3 = vec3<f32>(0.0);
    for (var i = thread; i < rays; i += GI_THREADS) {
        let d = gi_direction(i, rays, turn);
        let light = max(gi_ray_light(origin, d), vec3<f32>(0.0));
        c0 += light * 0.282095;
        c1 += light * (0.488603 * d.y);
        c2 += light * (0.488603 * d.z);
        c3 += light * (0.488603 * d.x);
    }
    gi_sums[thread] = array<vec3<f32>, 4>(c0, c1, c2, c3);
    workgroupBarrier();
    for (var stride = GI_THREADS / 2u; stride > 0u; stride = stride / 2u) {
        if thread < stride {
            for (var k = 0u; k < 4u; k++) {
                gi_sums[thread][k] += gi_sums[thread + stride][k];
            }
        }
        workgroupBarrier();
    }
    if thread == 0u {
        // Each ray stands for 4 pi / rays of all directions, then the
        // cosine lobe's share of each band over pi: 1, then 2/3, as
        // light_probe_sum.wgsl stores them.
        let scale = 4.0 * PI / f32(max(rays, 1u));
        let place = 4u * bitcast<u32>(probe.w);
        let keep = gi.settings.x;
        for (var k = 0u; k < 4u; k++) {
            let band = select(2.0 / 3.0, 1.0, k == 0u);
            let fresh = gi_sums[0][k] * scale * band;
            let old = light_probes[place + k];
            // w keeps the first coefficient's invalid flag (R18.2).
            gi_traced[4u * group.x + k] = vec4<f32>(mix(fresh, old.rgb, keep), old.w);
        }
    }
}
