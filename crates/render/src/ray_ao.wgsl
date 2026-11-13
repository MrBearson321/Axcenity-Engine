// Ray traced ambient occlusion (renderer/ray_ao.rs, docs/rendering/ray-tracing.md):
// how much of the hemisphere above each solid pixel is hidden, found with
// short rays through the ray traced scene instead of guessed from the
// screen (ssao.wgsl), so what's off screen or behind something counts too.
// Drawn at half the scene's size into the same target as ssao.wgsl, then
// blurred by its `fs_blur`. Blended over it: near the ray tracing distance
// the rays fade out into what's beneath (screen-space ambient occlusion,
// or nothing hidden). Compiled after `ray_cut_outs.wgsl`, with
// `enable wgpu_ray_query;` put first (renderer/gpu.rs).

// Must match `OcclusionUniform` in renderer/types.rs (ssao.wgsl's).
struct Occlusion {
    projection: mat4x4<f32>,
    inv_projection: mat4x4<f32>,
    // x: screen-space samples per pixel, y: the radius in studs.
    settings: vec4<f32>,
}

// Must match `RayAoUniform` in renderer/ray_ao.rs.
struct RayAo {
    // The camera's view to world space.
    view_to_world: mat4x4<f32>,
    // xyz: the camera, w: how far the rays reach from it, in studs.
    camera: vec4<f32>,
    // x: rays per pixel, y: a number that changes each frame under TAA (0
    // without), zw: unused.
    settings: vec4<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
@group(0) @binding(1) var normal_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> occlusion: Occlusion;
@group(1) @binding(0) var ray_scene: acceleration_structure;
@group(1) @binding(1) var<uniform> rays: RayAo;
// Cut-outs' holes (LE7, ray_cut_outs.wgsl).
@group(1) @binding(2) var<storage, read> cut_outs: array<CutOut>;
@group(1) @binding(3) var<storage, read> cut_out_uvs: array<vec2<f32>>;
@group(1) @binding(4) var cut_out_alpha: texture_2d_array<f32>;
@group(1) @binding(5) var cut_out_sampler: sampler;

// Instance masks (renderer/ray_tracing.rs): solid items and cut-outs,
// casting shadows or not, but not see-through ones, as the prepass draws
// them.
const MASK_SOLID: u32 = 9u;
const MASK_CUT_OUTS: u32 = 18u;
// Rays pass through cut-outs' holes (LE7), but a cut-out nearer than this
// still isn't counted: a leaf card lying on the ground, or a grass clump's
// foot, would darken the ground it stands in (ray_shadows.wgsl's
// CUT_OUT_NEAR).
const CUT_OUT_NEAR: f32 = 0.35;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

// The same noise as `pixel_noise` in lighting.wgsl.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

// The full-size pixel a half-size one stands for (ssao.wgsl's).
fn depth_pixel(half_pixel: vec2<i32>) -> vec2<i32> {
    let last = vec2<i32>(textureDimensions(depth_texture)) - 1;
    return clamp(half_pixel * 2, vec2<i32>(0), last);
}

fn view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let view = occlusion.inv_projection * clip;
    return view.xyz / view.w;
}

// How near what the ray from `origin` along `direction` hits is, as a
// share of `radius` (1 touching, 0 at the radius or nothing hit).
fn hidden_along(origin: vec3<f32>, direction: vec3<f32>, radius: f32) -> f32 {
    let nearest = min(
        nearest_hit(origin, direction, MASK_SOLID, 0.0, radius),
        nearest_hit(origin, direction, MASK_CUT_OUTS, min(CUT_OUT_NEAR, radius), radius));
    return 1.0 - nearest / radius;
}

// How far along the ray from `origin` the nearest item of `mask` is,
// between `near` and `radius` (`radius` for nothing).
fn nearest_hit(origin: vec3<f32>, direction: vec3<f32>, mask: u32, near: f32, radius: f32) -> f32 {
    let hit = trace_through_holes(RayDesc(0u, mask, near, radius, origin, direction));
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return radius;
    }
    return hit.t;
}

// rgb: how open the pixel is (1 nothing hidden), a: how much the rays
// count here (they fade out toward the reach).
@fragment
fn fs_ray_occlusion(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = depth_pixel(vec2<i32>(position.xy));
    let size = vec2<f32>(textureDimensions(depth_texture));
    let depth = textureLoad(depth_texture, pixel, 0);
    // The sky: nothing to hide, and nothing beneath to keep.
    if depth >= 1.0 {
        return vec4<f32>(1.0);
    }
    let uv = (vec2<f32>(pixel) + 0.5) / size;
    let view = view_position(uv, depth);
    let world = (rays.view_to_world * vec4<f32>(view, 1.0)).xyz;
    let distance = length(world - rays.camera.xyz);
    let reach = rays.camera.w;
    if distance >= reach {
        return vec4<f32>(1.0, 1.0, 1.0, 0.0);
    }
    let view_normal = normalize(textureLoad(normal_texture, pixel, 0).xyz);
    let n = normalize((rays.view_to_world * vec4<f32>(view_normal, 0.0)).xyz);
    // A frame around the normal.
    let helper = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.99);
    let t = normalize(cross(helper, n));
    let b = cross(n, t);

    // Off the surface, by more the further away it is as depth gets coarser.
    let origin = world + n * (0.02 + distance * 0.002);
    let radius = occlusion.settings.y;
    let count = max(u32(rays.settings.x), 1u);
    // Cosine-weighted directions (a spiral of golden-angle steps), turned
    // per pixel (and per frame under TAA) so neighbours sample differently
    // and the blur evens them out.
    let turn = (pixel_noise(position.xy) + rays.settings.y * 0.618034) * 6.2831853;
    var hidden = 0.0;
    for (var i = 0u; i < count; i++) {
        let k = (f32(i) + 0.5) / f32(count);
        let angle = turn + f32(i) * 2.3999632;
        let side = sqrt(k);
        let direction = t * (cos(angle) * side) + b * (sin(angle) * side) + n * sqrt(1.0 - k);
        hidden += hidden_along(origin, direction, radius);
    }
    let open = 1.0 - hidden / f32(count);
    let weight = 1.0 - smoothstep(reach * 0.85, reach, distance);
    return vec4<f32>(open, open, open, weight);
}
