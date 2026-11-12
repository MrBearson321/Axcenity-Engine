// Ray query passes for the probe. Each entry point is one effect's rays at
// one ray per pixel (or per pixel of its reduced resolution), the way a
// hybrid renderer would trace them from its depth and normal prepass.
enable wgpu_ray_query;

struct Params {
    cam_pos: vec4<f32>,
    cam_forward: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    // xyz: direction toward the sun; w: tangent of the soft shadow's cone.
    sun: vec4<f32>,
    // width, height, frame, unused
    size: vec4<u32>,
}

@group(0) @binding(0) var scene: acceleration_structure;
@group(0) @binding(1) var<uniform> params: Params;
// Where each pixel's primary ray hit: xyz, w = 1 on a hit.
@group(0) @binding(2) var<storage, read_write> hits: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> normals: array<vec4<f32>>;
// One value per pixel per pass, so no ray is optimized away.
@group(0) @binding(4) var<storage, read_write> results: array<u32>;
@group(0) @binding(5) var<storage, read> vertices: array<f32>;
@group(0) @binding(6) var<storage, read> indices: array<u32>;
// Per mesh: first index, base vertex.
@group(0) @binding(7) var<storage, read> meshes: array<vec4<u32>>;

const BIAS: f32 = 0.01;

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn random2(pixel: vec2<u32>) -> vec2<f32> {
    let seed = pcg(pixel.x + pcg(pixel.y + pcg(params.size.z)));
    let a = pcg(seed);
    let b = pcg(a);
    return vec2<f32>(f32(a) / 4294967295.0, f32(b) / 4294967295.0);
}

fn vertex(index: u32) -> vec3<f32> {
    return vec3<f32>(vertices[index * 3u], vertices[index * 3u + 1u], vertices[index * 3u + 2u]);
}

// Builds an orthonormal basis around `n` (Duff et al. 2017).
fn basis(n: vec3<f32>) -> mat3x3<f32> {
    let s = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    let t = vec3<f32>(1.0 + s * n.x * n.x * a, s * b, -s * n.x);
    let bt = vec3<f32>(b, s + n.y * n.y * a, -n.y);
    return mat3x3<f32>(t, bt, n);
}

fn cosine_hemisphere(n: vec3<f32>, u: vec2<f32>) -> vec3<f32> {
    let r = sqrt(u.x);
    let phi = 6.2831853 * u.y;
    let local = vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - u.x)));
    return normalize(basis(n) * local);
}

// True when nothing is between `origin` and `tmax` along `dir`: the
// cheapest query, stopping at the first opaque hit.
fn visible(origin: vec3<f32>, dir: vec3<f32>, tmax: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, scene, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xffu, 0.0, tmax, origin, dir));
    rayQueryProceed(&rq);
    let hit = rayQueryGetCommittedIntersection(&rq);
    return hit.kind == RAY_QUERY_INTERSECTION_NONE;
}

// The nearest hit along a ray, or t < 0 when it misses.
fn closest(origin: vec3<f32>, dir: vec3<f32>, tmax: f32) -> RayIntersection {
    var rq: ray_query;
    rayQueryInitialize(&rq, scene, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, tmax, origin, dir));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq);
}

fn world_normal(hit: RayIntersection, dir: vec3<f32>) -> vec3<f32> {
    let mesh = meshes[hit.instance_custom_data];
    let first = mesh.x + hit.primitive_index * 3u;
    let a = vertex(mesh.y + indices[first]);
    let b = vertex(mesh.y + indices[first + 1u]);
    let c = vertex(mesh.y + indices[first + 2u]);
    let local = cross(b - a, c - a);
    let w = hit.world_to_object;
    var n = normalize(vec3<f32>(dot(w[0], local), dot(w[1], local), dot(w[2], local)));
    if dot(n, dir) > 0.0 {
        n = -n;
    }
    return n;
}

fn index_of(id: vec3<u32>) -> u32 {
    return id.y * params.size.x + id.x;
}

fn in_bounds(id: vec3<u32>) -> bool {
    return id.x < params.size.x && id.y < params.size.y;
}

// Primary visibility: stands in for the engine's depth and normal prepass,
// so the other passes start from the same pixels a raster prepass gives.
@compute @workgroup_size(8, 8)
fn primary(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id) {
        return;
    }
    let size = vec2<f32>(params.size.xy);
    let uv = (vec2<f32>(id.xy) + 0.5) / size * 2.0 - 1.0;
    let aspect = size.x / size.y;
    let dir = normalize(params.cam_forward.xyz
        + params.cam_right.xyz * uv.x * aspect * 0.6
        - params.cam_up.xyz * uv.y * 0.6);
    let hit = closest(params.cam_pos.xyz, dir, 10000.0);
    let i = index_of(id);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        hits[i] = vec4<f32>(0.0);
        return;
    }
    let p = params.cam_pos.xyz + dir * hit.t;
    let n = world_normal(hit, dir);
    hits[i] = vec4<f32>(p, 1.0);
    normals[i] = vec4<f32>(n, 0.0);
}

// Hard sun shadow: one ray per pixel toward the sun.
@compute @workgroup_size(8, 8)
fn shadow_hard(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id) {
        return;
    }
    let i = index_of(id);
    let p = hits[i];
    if p.w == 0.0 {
        return;
    }
    let n = normals[i].xyz;
    results[i] = u32(visible(p.xyz + n * BIAS, params.sun.xyz, 10000.0));
}

// Soft sun shadow: one ray per pixel jittered inside the sun's cone (the
// denoiser turns the noise into a penumbra).
@compute @workgroup_size(8, 8)
fn shadow_soft(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id) {
        return;
    }
    let i = index_of(id);
    let p = hits[i];
    if p.w == 0.0 {
        return;
    }
    let n = normals[i].xyz;
    let u = random2(id.xy);
    let r = params.sun.w * sqrt(u.x);
    let phi = 6.2831853 * u.y;
    let dir = normalize(basis(params.sun.xyz) * vec3<f32>(r * cos(phi), r * sin(phi), 1.0));
    results[i] = u32(visible(p.xyz + n * BIAS, dir, 10000.0));
}

// Ambient occlusion: one cosine-weighted ray per pixel, 2 studs long.
@compute @workgroup_size(8, 8)
fn ambient_occlusion(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id) {
        return;
    }
    let i = index_of(id);
    let p = hits[i];
    if p.w == 0.0 {
        return;
    }
    let n = normals[i].xyz;
    let dir = cosine_hemisphere(n, random2(id.xy));
    results[i] = u32(visible(p.xyz + n * BIAS, dir, 2.0));
}

// Mirror reflections at half resolution (one ray per 2x2 pixels), nearest
// hit, then the hit's normal as a shading stand-in.
@compute @workgroup_size(8, 8)
fn reflection(@builtin(global_invocation_id) id: vec3<u32>) {
    let full = vec3<u32>(id.xy * 2u, 0u);
    if !in_bounds(full) {
        return;
    }
    let i = index_of(full);
    let p = hits[i];
    if p.w == 0.0 {
        return;
    }
    let n = normals[i].xyz;
    let view = normalize(p.xyz - params.cam_pos.xyz);
    let dir = reflect(view, n);
    let hit = closest(p.xyz + n * BIAS, dir, 1000.0);
    if hit.kind != RAY_QUERY_INTERSECTION_NONE {
        results[i] = u32(world_normal(hit, dir).y * 1000.0) + 1u;
    }
}

// Diffuse GI rays at half resolution: one cosine-weighted ray per 2x2
// pixels to the nearest hit, then a shadow ray from that hit toward the sun
// (one bounce of sunlight, the core of a ReSTIR GI or screen-probe gather).
@compute @workgroup_size(8, 8)
fn diffuse_gi(@builtin(global_invocation_id) id: vec3<u32>) {
    let full = vec3<u32>(id.xy * 2u, 0u);
    if !in_bounds(full) {
        return;
    }
    let i = index_of(full);
    let p = hits[i];
    if p.w == 0.0 {
        return;
    }
    let n = normals[i].xyz;
    let dir = cosine_hemisphere(n, random2(id.xy));
    let hit = closest(p.xyz + n * BIAS, dir, 1000.0);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        results[i] = 2u;
        return;
    }
    let q = p.xyz + n * BIAS + dir * hit.t;
    let m = world_normal(hit, dir);
    results[i] = u32(visible(q + m * BIAS, params.sun.xyz, 10000.0));
}
