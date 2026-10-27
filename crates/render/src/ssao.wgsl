// Screen-space ambient occlusion: how much of the hemisphere above each
// solid surface is hidden by what's near it, from the prepass's depth and
// view-space normals. Drawn at half the scene's size, then blurred without
// blurring across edges. The scene pass multiplies the result into ambient
// and sky light only.
// View space is right-handed: the camera looks down -Z, so nearer points
// have larger (less negative) z.

// Must match `OcclusionUniform` in renderer/types.rs.
struct Occlusion {
    projection: mat4x4<f32>,
    inv_projection: mat4x4<f32>,
    // x: samples per pixel, y: the radius in studs.
    settings: vec4<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
// The occlusion pass reads the normals here; the blur reads the raw occlusion.
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var<uniform> occlusion: Occlusion;

// One triangle that covers the whole target.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

// The same noise as `pixel_noise` in lighting.wgsl.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

// The full-size pixel a half-size one stands for, kept on the texture.
fn depth_pixel(half_pixel: vec2<i32>) -> vec2<i32> {
    let last = vec2<i32>(textureDimensions(depth_texture)) - 1;
    return clamp(half_pixel * 2, vec2<i32>(0), last);
}

// The view-space point at texture coordinate `uv` (0 to 1, down from the
// top) with prepass depth `depth`.
fn view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let view = occlusion.inv_projection * clip;
    return view.xyz / view.w;
}

@fragment
fn fs_occlusion(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = depth_pixel(vec2<i32>(position.xy));
    let size = vec2<f32>(textureDimensions(depth_texture));
    let depth = textureLoad(depth_texture, pixel, 0);
    // Nothing solid here (the sky): nothing to hide.
    if depth >= 1.0 {
        return vec4<f32>(1.0);
    }
    let uv = (vec2<f32>(pixel) + 0.5) / size;
    let p = view_position(uv, depth);
    let n = normalize(textureLoad(source, pixel, 0).xyz);

    // A frame around the normal, turned per pixel so the few samples hit
    // different places from one pixel to the next (the blur evens them out).
    let angle = pixel_noise(position.xy) * 6.2831853;
    let turn = vec3<f32>(cos(angle), sin(angle), 0.0);
    var t = turn - n * dot(turn, n);
    if dot(t, t) < 1e-4 {
        t = vec3<f32>(0.0, 0.0, 1.0) - n * n.z;
    }
    t = normalize(t);
    let b = cross(n, t);

    let count = u32(occlusion.settings.x);
    let radius = occlusion.settings.y;
    // Small, so flat ground isn't hidden by itself, and growing with
    // distance as depth gets coarser.
    let bias = 0.02 + 0.002 * -p.z;
    var hidden = 0.0;
    for (var i = 0u; i < count; i++) {
        // Points over the hemisphere (a spiral of golden-angle steps, kept
        // off the surface), closer to the middle the earlier they are, so
        // near creases count most.
        let k = (f32(i) + 0.5) / f32(count);
        let up = mix(0.15, 1.0, k);
        let around = f32(i) * 2.3999632;
        let side = sqrt(1.0 - up * up);
        let direction = vec3<f32>(cos(around) * side, sin(around) * side, up);
        let reach = radius * mix(0.1, 1.0, k * k);
        let probe = p + (t * direction.x + b * direction.y + n * direction.z) * reach;

        let clip = occlusion.projection * vec4<f32>(probe, 1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let ndc = clip.xy / clip.w;
        let sample_uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if any(sample_uv < vec2<f32>(0.0)) || any(sample_uv >= vec2<f32>(1.0)) {
            continue;
        }
        let sample_pixel = vec2<i32>(sample_uv * size);
        let scene_depth = textureLoad(depth_texture, sample_pixel, 0);
        let scene = view_position((vec2<f32>(sample_pixel) + 0.5) / size, scene_depth);
        // Something solid is in front of the sample point. A surface far in
        // front (a part between here and the camera) counts less and less.
        let in_front = select(0.0, 1.0, scene.z >= probe.z + bias);
        let near = smoothstep(0.0, 1.0, radius / max(abs(p.z - scene.z), 1e-4));
        hidden += in_front * near;
    }
    let open = 1.0 - hidden / f32(max(count, 1u));
    return vec4<f32>(open, open, open, 1.0);
}

// How far in front of the camera the prepass's surface at `pixel` is.
fn view_distance(pixel: vec2<i32>) -> f32 {
    let size = vec2<f32>(textureDimensions(depth_texture));
    let uv = (vec2<f32>(pixel) + 0.5) / size;
    return -view_position(uv, textureLoad(depth_texture, pixel, 0)).z;
}

// A 3x3 average of the raw occlusion, leaving out pixels whose surface is
// at a different distance, so a part's occlusion doesn't smear onto what's
// behind it.
@fragment
fn fs_blur(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let here = vec2<i32>(position.xy);
    let last = vec2<i32>(textureDimensions(source)) - 1;
    let center = view_distance(depth_pixel(here));
    let tolerance = 0.05 * center + 0.05;
    var total = 0.0;
    var weights = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let pixel = clamp(here + vec2<i32>(x, y), vec2<i32>(0), last);
            let other = view_distance(depth_pixel(pixel));
            let weight = max(1.0 - abs(other - center) / tolerance, 0.0);
            total += textureLoad(source, pixel, 0).r * weight;
            weights += weight;
        }
    }
    let open = select(1.0, total / weights, weights > 0.0);
    return vec4<f32>(open, open, open, 1.0);
}
