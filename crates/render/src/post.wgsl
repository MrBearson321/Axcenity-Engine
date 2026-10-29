// Post-processing (rendering stage 5): bloom, then tone mapping from the
// high dynamic range scene to the screen, then color grading. Colors are
// linear light, except while they're graded.

struct Post {
    // x: bloom threshold, y: bloom intensity, z: exposure.
    values: vec4<f32>,
    // x: 1 if bloom is on, y: 1 if tone mapping is on, z: the curve (0
    // Classic, 1 Neutral, 2 Filmic).
    flags: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;
// The blurred bright parts, for the composite pass only.
@group(0) @binding(3) var bloom: texture_2d<f32>;

// Color grading, for the composite pass only. Must match GradingUniform in
// renderer/grading.rs.
struct Grading {
    // x: brightness, y: contrast, z: saturation, w: posterize levels (0 none).
    values: vec4<f32>,
    // rgb: the tint (sRGB), w: 1 if grading is on.
    tint: vec4<f32>,
    // x: the lookup table's size (0 none), y: how many palette colors.
    counts: vec4<f32>,
    // sRGB.
    palette: array<vec4<f32>, 64>,
}

@group(1) @binding(0) var lookup: texture_3d<f32>;
@group(1) @binding(1) var<uniform> grading: Grading;

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

// The tone mapping curves. Each must match its copy in tone_mapping.rs.

// Classic: light up to the knee passes unchanged, so ordinary scenes look
// as they would without tone mapping. Above it each channel rolls off
// toward 1 instead of clipping.
fn tone_map_classic(color: vec3<f32>) -> vec3<f32> {
    let knee = 0.6;
    let over = max(color - knee, vec3<f32>(0.0));
    let rolled = knee + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
    return select(rolled, color, color <= vec3<f32>(knee));
}

// Khronos PBR Neutral: takes off the 4% a non-metal reflects, so lit colors
// come out as picked, then compresses highlights without changing hue.
fn tone_map_neutral(input: vec3<f32>) -> vec3<f32> {
    let start = 0.8 - 0.04;
    let lowest = min(input.r, min(input.g, input.b));
    var offset = 0.04;
    if lowest < 0.08 {
        offset = lowest - 6.25 * lowest * lowest;
    }
    let color = input - offset;
    let peak = max(color.r, max(color.g, color.b));
    if peak < start {
        return color;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    let blend = 1.0 - 1.0 / (0.15 * (peak - new_peak) + 1.0);
    return mix(color * (new_peak / peak), vec3<f32>(new_peak), blend);
}

// Filmic: Stephen Hill's ACES fit, after scaling light up by 1 / 0.6 so a
// mid-grey stays about as bright. WGSL matrices are built from columns, so
// these are the rows of the usual matrices, transposed.
fn tone_map_filmic(color: vec3<f32>) -> vec3<f32> {
    let aces_input = mat3x3<f32>(
        vec3<f32>(0.59719, 0.07600, 0.02840),
        vec3<f32>(0.35458, 0.90834, 0.13383),
        vec3<f32>(0.04823, 0.01566, 0.83777),
    );
    let aces_output = mat3x3<f32>(
        vec3<f32>(1.60475, -0.10208, -0.00327),
        vec3<f32>(-0.53108, 1.10813, -0.07276),
        vec3<f32>(-0.07367, -0.00605, 1.07602),
    );
    let v = aces_input * (color / 0.6);
    let fitted = (v * (v + 0.0245786) - 0.000090537) / (v * (0.983729 * v + 0.432951) + 0.238081);
    return aces_output * fitted;
}

fn tone_map(color: vec3<f32>) -> vec3<f32> {
    if post.flags.z > 1.5 {
        return tone_map_filmic(color);
    }
    if post.flags.z > 0.5 {
        return tone_map_neutral(color);
    }
    return tone_map_classic(color);
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
    color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    if grading.tint.w > 0.5 {
        color = to_linear(graded(to_srgb(color)));
    }
    return vec4<f32>(color, 1.0);
}

fn to_srgb(color: vec3<f32>) -> vec3<f32> {
    let high = 1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, color * 12.92, color <= vec3<f32>(0.0031308));
}

fn to_linear(color: vec3<f32>) -> vec3<f32> {
    let high = pow((color + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, color / 12.92, color <= vec3<f32>(0.04045));
}

// One entry of the lookup table.
fn lookup_entry(at: vec3<i32>) -> vec3<f32> {
    return textureLoad(lookup, at, 0).rgb;
}

// `color` looked up in the table, blended between its eight nearest
// entries (trilinear), so a neutral table gives back exactly what it got.
fn looked_up(color: vec3<f32>) -> vec3<f32> {
    let last = i32(grading.counts.x) - 1;
    let at = color * f32(last);
    let low = min(vec3<i32>(floor(at)), vec3<i32>(last - 1));
    let f = at - vec3<f32>(low);
    let r0 = mix(lookup_entry(low), lookup_entry(low + vec3<i32>(1, 0, 0)), f.x);
    let r1 = mix(lookup_entry(low + vec3<i32>(0, 1, 0)), lookup_entry(low + vec3<i32>(1, 1, 0)), f.x);
    let r2 = mix(lookup_entry(low + vec3<i32>(0, 0, 1)), lookup_entry(low + vec3<i32>(1, 0, 1)), f.x);
    let r3 = mix(lookup_entry(low + vec3<i32>(0, 1, 1)), lookup_entry(low + vec3<i32>(1, 1, 1)), f.x);
    return mix(mix(r0, r1, f.y), mix(r2, r3, f.y), f.z);
}

// The finished, sRGB-encoded `input` graded: brightness, contrast,
// saturation, tint, the lookup table, posterizing, then the palette.
fn graded(input: vec3<f32>) -> vec3<f32> {
    let values = grading.values;
    var color = input + values.x;
    color = (color - 0.5) * (1.0 + values.y) + 0.5;
    let grey = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(grey), color, 1.0 + values.z);
    color = clamp(color * grading.tint.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    if grading.counts.x > 1.5 {
        color = clamp(looked_up(color), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    // Even steps from black to white, as many as asked for.
    if values.w > 1.5 {
        color = min(floor(color * values.w), vec3<f32>(values.w - 1.0)) / (values.w - 1.0);
    }
    let colors = u32(grading.counts.y);
    if colors > 0u {
        var nearest = grading.palette[0].rgb;
        var best = 4.0;
        for (var i = 0u; i < colors; i += 1u) {
            let offset = grading.palette[i].rgb - color;
            let distance = dot(offset, offset);
            if distance < best {
                best = distance;
                nearest = grading.palette[i].rgb;
            }
        }
        color = nearest;
    }
    return color;
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
