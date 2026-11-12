// The editor's Ray Hits view (renderer/ray_tracing.rs): the camera's rays
// traced against the ray traced scene. Each item gets a color of its own
// (from its TLAS slot), its triangles alternate lighter and darker, and it
// dims toward the ray tracing distance. Rays that hit nothing show a dark
// blue-grey.
enable wgpu_ray_query;

struct RayView {
    inv_view_proj: mat4x4<f32>,
    // The output's width and height, the ray tracing distance, unused.
    size: vec4<f32>,
}

@group(0) @binding(0) var scene: acceleration_structure;
@group(0) @binding(1) var<uniform> ray_view: RayView;

@vertex
fn vs_ray_hits(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the screen.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn hash(value: u32) -> u32 {
    var x = value * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn world_at(ndc: vec2<f32>, depth: f32) -> vec3<f32> {
    let point = ray_view.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    return point.xyz / point.w;
}

@fragment
fn fs_ray_hits(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / ray_view.size.xy;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // From the near plane through a point farther along, so perspective and
    // orthographic cameras both work.
    let origin = world_at(ndc, 0.0);
    let direction = normalize(world_at(ndc, 0.5) - origin);
    let reach = ray_view.size.z;
    var query: ray_query;
    rayQueryInitialize(&query, scene, RayDesc(
        RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, reach * 2.0, origin, direction));
    rayQueryProceed(&query);
    let hit = rayQueryGetCommittedIntersection(&query);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return vec4<f32>(0.03, 0.035, 0.05, 1.0);
    }
    let bits = hash(hit.instance_custom_data + 1u);
    let color = vec3<f32>(
        f32(bits & 255u),
        f32((bits >> 8u) & 255u),
        f32((bits >> 16u) & 255u),
    ) / 255.0 * 0.75 + 0.25;
    let triangle = select(0.8, 1.0, (hit.primitive_index & 1u) == 0u);
    let distance = 1.0 - 0.6 * clamp(hit.t / reach, 0.0, 1.0);
    return vec4<f32>(color * triangle * distance, 1.0);
}
