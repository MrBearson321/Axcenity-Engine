// The editor's Path Traced view (LE6, renderer/path_tracer.rs,
// docs/rendering/ray-tracing.md): the scene as a reference picture, each
// pixel's light found by following rays as they bounce through the ray
// traced scene, one more path a pixel each frame, averaged until the
// camera or the scene changes.
//
// Each path starts at the camera, a little off the pixel's middle each
// frame (so edges smooth out), and at each surface it hits:
//
// - adds the surface's glow;
// - adds the sun, with a ray toward a random point of its disc for its
//   shadow (soft shadows), and every lamp in reach, each with a ray for its
//   shadow (next event estimation);
// - bounces on in a random direction, more often near the surface's normal
//   (cosine weighted, a matte surface's own spread), carrying its color;
// - after two bounces it may stop, more likely the darker the path has
//   become (Russian roulette), so dark paths cost less.
//
// A path that leaves the scene sees the sky: as drawn for the camera's own
// rays, and times the environment intensity for bounced light, as the
// scene's sky light is.
//
// Surfaces are matte (Lambert), each item a single color (its color times
// its texture's average, `SurfaceRaw` in renderer/ray_tracing.rs) with its
// own normals between corners. See-through items, water, fog and the flat
// ambient light are left out.
//
// Compiled after `sky.wgsl`, `fog.wgsl`, `clouds.wgsl` and `lighting.wgsl`
// (for the frame's uniform, lamps and sky; only those of the frame's
// bindings are bound, renderer/path_tracer.rs) and `ray_cut_outs.wgsl`, with
// `enable wgpu_ray_query;` put first (renderer/gpu.rs).

// Instance masks (renderer/ray_tracing.rs): what a path can hit (solid and
// cut-out items, casting shadows or not; not see-through ones), and what
// casts shadows.
const PT_MASK_SURFACES: u32 = 27u;
const PT_MASK_CASTERS: u32 = 3u;
// The most of the frame's lamps a hit is lit by.
const PT_MOST_LAMPS: u32 = 64u;
// How far rays go: past every item the ray traced scene holds.
const PT_FAR: f32 = 100000.0;
// The brightest one path may bring back, so a rare bright bounce doesn't
// leave a speck for hundreds of frames.
const PT_BRIGHTEST: f32 = 64.0;
// A slot whose mesh's normals aren't copied yet.
const PT_NO_NORMALS: u32 = 0xffffffffu;

// Must match `PathTraceUniform` in renderer/path_tracer.rs.
struct PathTrace {
    inv_view_proj: mat4x4<f32>,
    // x, y: the traced picture's width and height in pixels, z: which path
    // this is of each pixel's (0 starts the sum again), w: the most bounces.
    counts: vec4<u32>,
    // x: output pixels a side per traced pixel, yzw: unused.
    settings: vec4<f32>,
}

@group(1) @binding(0) var ray_scene: acceleration_structure;
@group(1) @binding(1) var<uniform> pt: PathTrace;
// Two a TLAS slot (`SurfaceRaw`): rgb its color and w 1 for a cut-out,
// then rgb its glow, both linear.
@group(1) @binding(2) var<storage, read> pt_surfaces: array<vec4<f32>>;
// Each TLAS slot's mesh's first triangle in `pt_normals`.
@group(1) @binding(3) var<storage, read> pt_slots: array<u32>;
// Every traced mesh's normals, three corners a triangle, each packed as
// two 16-bit signed numbers a word (x and y, then z).
@group(1) @binding(4) var<storage, read> pt_normals: array<vec2<u32>>;
// Each traced pixel's sum of paths so far (rgb) and how many (w).
@group(1) @binding(5) var<storage, read_write> pt_sum: array<vec4<f32>>;
// Cut-outs' holes (ray_cut_outs.wgsl): paths and shadows pass through them.
@group(1) @binding(6) var<storage, read> cut_outs: array<CutOut>;
@group(1) @binding(7) var<storage, read> cut_out_uvs: array<vec2<f32>>;
@group(1) @binding(8) var cut_out_alpha: texture_2d_array<f32>;
@group(1) @binding(9) var cut_out_sampler: sampler;

var<private> pt_state: u32;

fn pt_hash(value: u32) -> u32 {
    var x = value * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

// A random number from 0 up to 1.
fn pt_random() -> f32 {
    pt_state = pt_hash(pt_state);
    return f32(pt_state >> 8u) / 16777216.0;
}

// Any direction at right angles to `n`.
fn pt_tangent(n: vec3<f32>) -> vec3<f32> {
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    return normalize(cross(up, n));
}

// A random direction around `n`, more often near it (cosine weighted).
fn pt_bounce(n: vec3<f32>) -> vec3<f32> {
    let u = pt_random();
    let r = sqrt(u);
    let a = 2.0 * PI * pt_random();
    let t = pt_tangent(n);
    let b = cross(n, t);
    return normalize(t * (r * cos(a)) + b * (r * sin(a)) + n * sqrt(max(1.0 - u, 0.0)));
}

// A random direction within `radius` (the tangent of the angle) of `d`.
fn pt_within(d: vec3<f32>, radius: f32) -> vec3<f32> {
    let r = radius * sqrt(pt_random());
    let a = 2.0 * PI * pt_random();
    let t = pt_tangent(d);
    let b = cross(d, t);
    return normalize(d + t * (r * cos(a)) + b * (r * sin(a)));
}

fn pt_unpack(packed: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(unpack2x16snorm(packed.x), unpack2x16snorm(packed.y).x);
}

// The surface's normal in the world where `hit` struck it, between its
// triangle's corners' normals, facing back toward `direction`'s start.
// Without its mesh's normals (or with none to speak of), facing straight
// back along the ray.
fn pt_normal(hit: RayIntersection, direction: vec3<f32>) -> vec3<f32> {
    let first = pt_slots[hit.instance_custom_data];
    if first == PT_NO_NORMALS {
        return -direction;
    }
    let corner = (first + hit.primitive_index) * 3u;
    let b = hit.barycentrics;
    let local = pt_unpack(pt_normals[corner]) * (1.0 - b.x - b.y)
        + pt_unpack(pt_normals[corner + 1u]) * b.x + pt_unpack(pt_normals[corner + 2u]) * b.y;
    // Normals turn by the inverse transpose: the world-to-object matrix's
    // columns dotted with the normal.
    let w = hit.world_to_object;
    let world = vec3<f32>(dot(local, w[0]), dot(local, w[1]), dot(local, w[2]));
    let length_squared = dot(world, world);
    if length_squared < 1e-12 {
        return -direction;
    }
    let n = world * inverseSqrt(length_squared);
    return select(n, -n, dot(n, direction) > 0.0);
}

// Whether anything casts a shadow between `origin` and `distance` along
// `direction`.
fn pt_blocked(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    let hit = trace_through_holes(RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT, PT_MASK_CASTERS, 0.0, distance, origin, direction));
    return hit.kind != RAY_QUERY_INTERSECTION_NONE;
}

// The sun's and lamps' light reaching `position` on a surface facing `n`,
// each checked for shadow with a ray, times 1 / pi (a matte surface's
// share toward any one direction).
fn pt_direct(position: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let lift = position + n * 0.01;
    var light = vec3<f32>(0.0);
    // A random point of the sun's disc, as wide as its shadows are soft.
    let sun = pt_within(frame.sun_direction.xyz, clamp(frame.shadow.y * 0.5, 0.0, 0.05));
    let n_dot_l = dot(n, sun);
    if n_dot_l > 0.0 && any(frame.sun_radiance.rgb > vec3<f32>(0.0))
        && !pt_blocked(lift, sun, PT_FAR) {
        light += frame.sun_radiance.rgb * n_dot_l;
    }
    let lamps = min(frame.lights.x, PT_MOST_LAMPS);
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
        // Every lamp casts shadows here, as real ones do.
        if pt_blocked(lift, toward, max(distance - 0.05, 0.0)) {
            continue;
        }
        light += lamp.radiance_cone.rgb * reach.w * facing;
    }
    return light / PI;
}

fn pt_world_at(ndc: vec2<f32>, depth: f32) -> vec3<f32> {
    let point = pt.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    return point.xyz / point.w;
}

// One path's light from the camera through `uv` (0 to 1 across the
// picture, down from the top).
fn pt_path(uv: vec2<f32>) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // From the near plane through a point farther along, so perspective and
    // orthographic cameras both work.
    var origin = pt_world_at(ndc, 0.0);
    var direction = normalize(pt_world_at(ndc, 0.5) - origin);
    var throughput = vec3<f32>(1.0);
    var light = vec3<f32>(0.0);
    for (var bounce = 0u; bounce <= pt.counts.w; bounce++) {
        let hit = trace_through_holes(RayDesc(
            0u, PT_MASK_SURFACES, 0.0, PT_FAR, origin, direction));
        if hit.kind == RAY_QUERY_INTERSECTION_NONE {
            if bounce == 0u {
                light += sky_specular(direction, 0.0);
            } else {
                // Without the sun's disc (it's lit directly), as the scene's
                // sky light is.
                light += throughput * sky_specular(direction, 0.2) * frame.sky_top.w;
            }
            break;
        }
        let surface = 2u * hit.instance_custom_data;
        let color = pt_surfaces[surface].rgb;
        let glow = pt_surfaces[surface + 1u].rgb;
        let n = pt_normal(hit, direction);
        let position = origin + direction * hit.t;
        light += throughput * (glow + color * pt_direct(position, n));
        throughput *= color;
        if bounce >= 2u {
            let keep = clamp(max(throughput.r, max(throughput.g, throughput.b)), 0.05, 0.95);
            if pt_random() >= keep {
                break;
            }
            throughput /= keep;
        }
        origin = position + n * 0.01;
        direction = pt_bounce(n);
    }
    return light;
}

@compute @workgroup_size(8, 8)
fn cs_path_trace(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = pt.counts.xy;
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let index = id.y * size.x + id.x;
    pt_state = pt_hash(index ^ pt_hash(pt.counts.z * 9781u + 7u));
    let jitter = vec2<f32>(pt_random(), pt_random());
    let uv = (vec2<f32>(id.xy) + jitter) / vec2<f32>(size);
    var found = pt_path(uv);
    // A path gone wrong (not a number) is dropped; a very bright one dimmed.
    if any(found != found) {
        found = vec3<f32>(0.0);
    }
    let brightest = max(found.r, max(found.g, found.b));
    if brightest > PT_BRIGHTEST {
        found *= PT_BRIGHTEST / brightest;
    }
    var sum = vec4<f32>(0.0);
    if pt.counts.z > 0u {
        sum = pt_sum[index];
    }
    pt_sum[index] = sum + vec4<f32>(max(found, vec3<f32>(0.0)), 1.0);
}
