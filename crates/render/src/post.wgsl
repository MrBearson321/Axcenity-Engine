// Post-processing (rendering stage 5): bloom, then tone mapping from the
// high dynamic range scene to the screen. Colors are linear light.

struct Post {
    // x: bloom threshold, y: bloom intensity, z: exposure.
    values: vec4<f32>,
    // x: 1 if bloom is on, y: 1 if tone mapping is on.
    flags: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;
// The blurred bright parts, for the composite pass only.
@group(0) @binding(3) var bloom: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// One triangle that covers the whole screen.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.clip_position = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(xy.x, 1.0 - xy.y);
    return out;
}

// The average of a 4x4 block of pixels around `uv` (four bilinear samples).
fn blur_4x4(uv: vec2<f32>, offset: f32) -> array<vec3<f32>, 4> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var taps: array<vec3<f32>, 4>;
    taps[0] = textureSampleLevel(source, linear_sampler, uv + texel * vec2<f32>(-offset, -offset), 0.0).rgb;
    taps[1] = textureSampleLevel(source, linear_sampler, uv + texel * vec2<f32>(offset, -offset), 0.0).rgb;
    taps[2] = textureSampleLevel(source, linear_sampler, uv + texel * vec2<f32>(-offset, offset), 0.0).rgb;
    taps[3] = textureSampleLevel(source, linear_sampler, uv + texel * vec2<f32>(offset, offset), 0.0).rgb;
    return taps;
}

// Keeps only the light brighter than the threshold, in its own color.
fn bright_part(color: vec3<f32>) -> vec3<f32> {
    let peak = max(color.r, max(color.g, color.b));
    return color * (max(peak - post.values.x, 0.0) / max(peak, 0.0001));
}

// Scene to the first, half-size bloom level, keeping the bright parts.
@fragment
fn fs_prefilter(in: VertexOut) -> @location(0) vec4<f32> {
    let taps = blur_4x4(in.uv, 1.0);
    let sum = bright_part(taps[0]) + bright_part(taps[1]) + bright_part(taps[2]) + bright_part(taps[3]);
    return vec4<f32>(sum * 0.25, 1.0);
}

// One bloom level to the next, smaller and blurrier.
@fragment
fn fs_downsample(in: VertexOut) -> @location(0) vec4<f32> {
    let taps = blur_4x4(in.uv, 1.0);
    return vec4<f32>((taps[0] + taps[1] + taps[2] + taps[3]) * 0.25, 1.0);
}

// A bloom level, blown up and added to the bigger one (the pipeline adds).
@fragment
fn fs_upsample(in: VertexOut) -> @location(0) vec4<f32> {
    let taps = blur_4x4(in.uv, 0.5);
    return vec4<f32>((taps[0] + taps[1] + taps[2] + taps[3]) * 0.25, 1.0);
}

// Light up to the knee passes unchanged, so ordinary scenes look as they
// would without tone mapping. Above it brightness rolls off toward 1
// instead of clipping.
fn tone_map(color: vec3<f32>) -> vec3<f32> {
    let knee = 0.6;
    let over = max(color - knee, vec3<f32>(0.0));
    let rolled = knee + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
    return select(rolled, color, color <= vec3<f32>(knee));
}

@fragment
fn fs_composite(in: VertexOut) -> @location(0) vec4<f32> {
    var color = textureSampleLevel(source, linear_sampler, in.uv, 0.0).rgb;
    if post.flags.x > 0.5 {
        color += textureSampleLevel(bloom, linear_sampler, in.uv, 0.0).rgb * post.values.y;
    }
    color *= post.values.z;
    if post.flags.y > 0.5 {
        color = tone_map(color);
    }
    // Colors past 1 can't be shown. The screen's sRGB format does the rest.
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// FXAA: smooths stair-stepped edges in the finished picture (`source`, the
// composite's output). Reading it gives linear light, so contrast is judged
// on the square root of its brightness, closer to how the eye sees it.
fn fxaa_luma(uv: vec2<f32>) -> f32 {
    let color = textureSampleLevel(source, linear_sampler, uv, 0.0).rgb;
    return sqrt(dot(color, vec3<f32>(0.299, 0.587, 0.114)));
}

fn fxaa_color(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(source, linear_sampler, uv, 0.0).rgb;
}

@fragment
fn fs_fxaa(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    let center = fxaa_color(in.uv);
    let middle = sqrt(dot(center, vec3<f32>(0.299, 0.587, 0.114)));
    let nw = fxaa_luma(in.uv + texel * vec2<f32>(-1.0, -1.0));
    let ne = fxaa_luma(in.uv + texel * vec2<f32>(1.0, -1.0));
    let sw = fxaa_luma(in.uv + texel * vec2<f32>(-1.0, 1.0));
    let se = fxaa_luma(in.uv + texel * vec2<f32>(1.0, 1.0));
    let lowest = min(middle, min(min(nw, ne), min(sw, se)));
    let highest = max(middle, max(max(nw, ne), max(sw, se)));
    // Flat areas and faint edges are left exactly as they are.
    if highest - lowest < max(0.0312, highest * 0.125) {
        return vec4<f32>(center, 1.0);
    }
    // The edge runs across the direction brightness changes fastest:
    // blend along it, up to 8 pixels each way.
    var along = vec2<f32>(-((nw + ne) - (sw + se)), (nw + sw) - (ne + se));
    let reduce = max((nw + ne + sw + se) * (0.25 / 8.0), 1.0 / 128.0);
    let scale = 1.0 / (min(abs(along.x), abs(along.y)) + reduce);
    along = clamp(along * scale, vec2<f32>(-8.0), vec2<f32>(8.0)) * texel;
    let near = 0.5 * (fxaa_color(in.uv + along * (1.0 / 3.0 - 0.5))
        + fxaa_color(in.uv + along * (2.0 / 3.0 - 0.5)));
    let far = near * 0.5 + 0.25 * (fxaa_color(in.uv - along * 0.5)
        + fxaa_color(in.uv + along * 0.5));
    // The wider blend reached past the edge into other shades: keep the near one.
    let far_luma = sqrt(dot(far, vec3<f32>(0.299, 0.587, 0.114)));
    if far_luma < lowest || far_luma > highest {
        return vec4<f32>(near, 1.0);
    }
    return vec4<f32>(far, 1.0);
}
