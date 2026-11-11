// Spatial upscaling while the render scale is below 100% (roadmap G9): an
// edge-adaptive upscale and a contrast-adaptive sharpen, after AMD
// FidelityFX Super Resolution 1.0's EASU and RCAS (MIT licence, Copyright
// (c) 2021 Advanced Micro Devices, Inc.; the notice is in NOTICE),
// written again for WGSL with plain loads instead of gathers.
//
// The composite pass (and FXAA) draw the finished picture at the scene's
// size into an 8-bit texture through an sRGB view, so `source` read through
// its plain view holds sRGB-encoded values: the perceptual space both
// passes are made for. `fs_easu` writes encoded values into the output-sized
// texture `fs_rcas` reads, and `fs_rcas` turns them linear for the sRGB
// target, as the composite pass's output is.

@group(0) @binding(0) var source: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// One triangle that covers the screen.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

// The texel at `at`, kept inside the texture.
fn texel(at: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(source));
    return textureLoad(source, clamp(at, vec2<i32>(0), size - 1), 0).rgb;
}

// FSR's cheap luma: red and blue at half weight, green at full.
fn luma(color: vec3<f32>) -> f32 {
    return color.b * 0.5 + (color.r * 0.5 + color.g);
}

// 1/x, kept finite at 0 (where what it scales is 0 too).
fn reciprocal(x: f32) -> f32 {
    return 1.0 / max(x, 1.0 / 32768.0);
}

// How strongly one of the four middle texels (weight `w`) sees an edge, and
// which way, from its neighbours: a above, b left, c itself, d right, e
// below.
fn edge(direction: ptr<function, vec2<f32>>, length: ptr<function, f32>, w: f32, a: f32, b: f32, c: f32, d: f32, e: f32) {
    let across_x = d - b;
    var length_x = saturate(abs(across_x) * reciprocal(max(abs(d - c), abs(c - b))));
    length_x *= length_x;
    let across_y = e - a;
    var length_y = saturate(abs(across_y) * reciprocal(max(abs(e - c), abs(c - a))));
    length_y *= length_y;
    *direction += vec2<f32>(across_x, across_y) * w;
    *length += (length_x + length_y) * w;
}

// Adds one tap of the stretched, rotated Lanczos-like kernel.
fn tap(sum: ptr<function, vec3<f32>>, weights: ptr<function, f32>, offset: vec2<f32>, direction: vec2<f32>, stretch: vec2<f32>, lobe: f32, clip: f32, color: vec3<f32>) {
    var v = vec2<f32>(
        offset.x * direction.x + offset.y * direction.y,
        offset.x * -direction.y + offset.y * direction.x,
    );
    v *= stretch;
    let d2 = min(v.x * v.x + v.y * v.y, clip);
    var base = (2.0 / 5.0) * d2 - 1.0;
    var window = lobe * d2 - 1.0;
    base *= base;
    window *= window;
    base = (25.0 / 16.0) * base - (25.0 / 16.0 - 1.0);
    let w = base * window;
    *sum += color * w;
    *weights += w;
}

@fragment
fn fs_easu(in: VertexOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source));
    // Where this output pixel's middle falls among the source's texels.
    var position = in.uv * size - 0.5;
    let corner = floor(position);
    position -= corner;
    let at = vec2<i32>(corner);
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    let b = texel(at + vec2<i32>(0, -1));
    let c = texel(at + vec2<i32>(1, -1));
    let e = texel(at + vec2<i32>(-1, 0));
    let f = texel(at);
    let g = texel(at + vec2<i32>(1, 0));
    let h = texel(at + vec2<i32>(2, 0));
    let i = texel(at + vec2<i32>(-1, 1));
    let j = texel(at + vec2<i32>(0, 1));
    let k = texel(at + vec2<i32>(1, 1));
    let l = texel(at + vec2<i32>(2, 1));
    let n = texel(at + vec2<i32>(0, 2));
    let o = texel(at + vec2<i32>(1, 2));

    // The edge's direction and how sharp it is, bilinearly from the four
    // middle texels.
    var direction = vec2<f32>(0.0);
    var length = 0.0;
    let p = position;
    edge(&direction, &length, (1.0 - p.x) * (1.0 - p.y), luma(b), luma(e), luma(f), luma(g), luma(j));
    edge(&direction, &length, p.x * (1.0 - p.y), luma(c), luma(f), luma(g), luma(h), luma(k));
    edge(&direction, &length, (1.0 - p.x) * p.y, luma(f), luma(i), luma(j), luma(k), luma(n));
    edge(&direction, &length, p.x * p.y, luma(g), luma(j), luma(k), luma(l), luma(o));
    let squared = dot(direction, direction);
    if squared < 1.0 / 32768.0 {
        direction = vec2<f32>(1.0, 0.0);
    } else {
        direction *= inverseSqrt(squared);
    }
    length = length * 0.5;
    length *= length;
    // Stretched along the edge, narrowed across it, as much as it's an edge.
    let along = dot(direction, direction) * reciprocal(max(abs(direction.x), abs(direction.y)));
    let stretch = vec2<f32>(1.0 + (along - 1.0) * length, 1.0 - 0.5 * length);
    let lobe = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * length;
    let clip = 1.0 / lobe;

    var sum = vec3<f32>(0.0);
    var weights = 0.0;
    tap(&sum, &weights, vec2<f32>(0.0, -1.0) - p, direction, stretch, lobe, clip, b);
    tap(&sum, &weights, vec2<f32>(1.0, -1.0) - p, direction, stretch, lobe, clip, c);
    tap(&sum, &weights, vec2<f32>(-1.0, 1.0) - p, direction, stretch, lobe, clip, i);
    tap(&sum, &weights, vec2<f32>(0.0, 1.0) - p, direction, stretch, lobe, clip, j);
    tap(&sum, &weights, vec2<f32>(0.0, 0.0) - p, direction, stretch, lobe, clip, f);
    tap(&sum, &weights, vec2<f32>(-1.0, 0.0) - p, direction, stretch, lobe, clip, e);
    tap(&sum, &weights, vec2<f32>(1.0, 1.0) - p, direction, stretch, lobe, clip, k);
    tap(&sum, &weights, vec2<f32>(2.0, 1.0) - p, direction, stretch, lobe, clip, l);
    tap(&sum, &weights, vec2<f32>(2.0, 0.0) - p, direction, stretch, lobe, clip, h);
    tap(&sum, &weights, vec2<f32>(1.0, 0.0) - p, direction, stretch, lobe, clip, g);
    tap(&sum, &weights, vec2<f32>(1.0, 2.0) - p, direction, stretch, lobe, clip, o);
    tap(&sum, &weights, vec2<f32>(0.0, 2.0) - p, direction, stretch, lobe, clip, n);
    // Kept within the four middle texels' range, so it never rings.
    let low = min(min(f, g), min(j, k));
    let high = max(max(f, g), max(j, k));
    let color = clamp(sum / weights, low, high);
    return vec4<f32>(color, 1.0);
}

// How much RCAS sharpens, in stops below its strongest (FSR's default).
const SHARPNESS_STOPS: f32 = 0.2;
// RCAS's limit, which keeps it from going past where it would ring.
const RCAS_LIMIT: f32 = 0.25 - 1.0 / 16.0;

fn to_linear(color: vec3<f32>) -> vec3<f32> {
    let high = pow((color + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, color / 12.92, color <= vec3<f32>(0.04045));
}

// How far a channel can be pushed from its neighbours' average before it
// leaves their range, as a negative lobe.
fn lobe_of(middle: f32, low: f32, high: f32) -> f32 {
    let hit_low = min(low, middle) / max(4.0 * high, 1.0 / 32768.0);
    let hit_high = (1.0 - max(high, middle)) / min(4.0 * low - 4.0, -1.0 / 32768.0);
    return max(-hit_low, hit_high);
}

@fragment
fn fs_rcas(in: VertexOut) -> @location(0) vec4<f32> {
    let at = vec2<i32>(in.clip_position.xy);
    //    b
    //  d e f
    //    h
    let b = texel(at + vec2<i32>(0, -1));
    let d = texel(at + vec2<i32>(-1, 0));
    let e = texel(at);
    let f = texel(at + vec2<i32>(1, 0));
    let h = texel(at + vec2<i32>(0, 1));
    // Less sharpening where the middle differs from all around it alone
    // (noise rather than an edge).
    let lb = luma(b);
    let ld = luma(d);
    let le = luma(e);
    let lf = luma(f);
    let lh = luma(h);
    let range = max(max(max(lb, ld), max(le, lf)), lh) - min(min(min(lb, ld), min(le, lf)), lh);
    var noise = abs(0.25 * (lb + ld + lf + lh) - le) * reciprocal(range);
    noise = -0.5 * saturate(noise) + 1.0;
    let low = min(min(b, d), min(f, h));
    let high = max(max(b, d), max(f, h));
    let lobe_r = lobe_of(e.r, low.r, high.r);
    let lobe_g = lobe_of(e.g, low.g, high.g);
    let lobe_b = lobe_of(e.b, low.b, high.b);
    var lobe = max(-RCAS_LIMIT, min(max(max(lobe_r, lobe_g), lobe_b), 0.0)) * exp2(-SHARPNESS_STOPS);
    lobe *= noise;
    let color = (lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0);
    return vec4<f32>(to_linear(saturate(color)), 1.0);
}
