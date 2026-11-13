// Motion vectors, TAA and motion blur (renderer/temporal.rs).
//
// The motion target holds how far each pixel moved across the screen since
// last frame, in texture coordinates (0 to 1 across the scene). The camera's
// motion is worked out here from the prepass's depth, then items that moved
// draw their own over it (`fs_motion` in shader.wgsl).
//
// TAA draws the scene a fraction of a pixel to one side each frame (the
// jitter) and blends each frame into the last ones, read back from where
// each pixel was. History that doesn't fit what's around the pixel now is
// pulled into the colors around it, and history from a pixel that moved
// differently (something uncovered) is dropped, so moving parts leave no
// ghosts.
//
// Below 100% render scale TAA upscales too (`fs_taau`): drawn at the
// output's size, each output pixel gathers the scene's jittered samples
// near it, weighted by how near, and last frame's picture is read back by a
// Catmull-Rom spline so it stays sharp.
//
// Motion blur streaks each pixel along the fastest motion near it (in tiles
// of the screen), counting what's in front only as far as its own streak
// reaches, so it spreads over what's behind it and a still surface stays
// sharp.

// Must match `TemporalUniform` in renderer/temporal.rs. Its first two
// matrices are `MotionView` in shader.wgsl.
struct Temporal {
    // This frame's and last frame's views, without the jitter.
    view_proj: mat4x4<f32>,
    previous_view_proj: mat4x4<f32>,
    // Last frame's time and wind, for swayed plants (shader.wgsl).
    previous_clock: vec4<f32>,
    // Turns this frame's clip space (as drawn, with the jitter) to the world.
    inv_view_proj: mat4x4<f32>,
    // x: 1 when last frame's history can be used, y: this frame's share of
    // TAA's blend, z: the near plane, w: the far plane.
    settings: vec4<f32>,
    // The scene's width and height, and 1 over each.
    size: vec4<f32>,
    // Motion blur: x the share of a frame's motion that streaks, y the
    // longest half-streak in pixels, z samples, w the tile size in pixels.
    blur: vec4<f32>,
    // x: the frame, for the noise.
    noise: vec4<f32>,
    // Upscaling TAA: the output's width and height, and 1 over each.
    output: vec4<f32>,
    // x, y: this frame's jitter in the scene's pixels; z: 1 while TAA
    // upscales.
    jitter: vec4<f32>,
}

@group(0) @binding(0) var<uniform> temporal: Temporal;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var scene: texture_2d<f32>;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var motion: texture_2d<f32>;
@group(0) @binding(5) var previous_motion: texture_2d<f32>;
@group(0) @binding(6) var history_sampler: sampler;
@group(0) @binding(7) var tiles: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

fn clip_to_uv(clip: vec4<f32>) -> vec2<f32> {
    return clip.xy / clip.w * vec2<f32>(0.5, -0.5) + 0.5;
}

// The camera's motion at each pixel: where the prepass's surface there (or
// the far plane, for the sky) was on screen last frame.
@fragment
fn fs_camera_motion(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = textureLoad(depth_texture, vec2<i32>(position.xy), 0);
    let uv = position.xy * temporal.size.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world = temporal.inv_view_proj * ndc;
    let point = vec4<f32>(world.xyz / world.w, 1.0);
    let now = clip_to_uv(temporal.view_proj * point);
    let before = clip_to_uv(temporal.previous_view_proj * point);
    return vec4<f32>(now - before, 0.0, 0.0);
}

fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}

fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

// How much a color counts in the blend: bright pixels count less, so one
// bright frame doesn't flicker through the history.
fn blend_weight(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + max(c.r, max(c.g, c.b)));
}

@fragment
fn fs_taa(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let last = vec2<i32>(temporal.size.xy) - 1;
    let current = max(textureLoad(scene, pixel, 0).rgb, vec3<f32>(0.0));
    // The colors around the pixel, and the nearest surface among them,
    // whose motion is taken so a moving part's edge moves with it.
    var low = vec3<f32>(1e9);
    var high = vec3<f32>(-1e9);
    var nearest = pixel;
    var nearest_depth = 2.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let at = clamp(pixel + vec2<i32>(x, y), vec2<i32>(0), last);
            let color = to_ycocg(max(textureLoad(scene, at, 0).rgb, vec3<f32>(0.0)));
            low = min(low, color);
            high = max(high, color);
            let depth = textureLoad(depth_texture, at, 0);
            if depth < nearest_depth {
                nearest_depth = depth;
                nearest = at;
            }
        }
    }
    if temporal.settings.x == 0.0 {
        return vec4<f32>(current, 1.0);
    }
    let moved = textureLoad(motion, nearest, 0).xy;
    let uv = (position.xy) * temporal.size.zw;
    let was = uv - moved;
    if any(was < vec2<f32>(0.0)) || any(was > vec2<f32>(1.0)) {
        return vec4<f32>(current, 1.0);
    }
    // History from a pixel that moved differently last frame belongs to
    // something else (what's now uncovered was behind it): drop it.
    let previous_pixel = clamp(vec2<i32>(was * temporal.size.xy), vec2<i32>(0), last);
    let moved_before = textureLoad(previous_motion, previous_pixel, 0).xy;
    let difference = length((moved - moved_before) * temporal.size.xy);
    let kept = 1.0 - saturate((difference - 0.5) / 1.5);
    var past = max(textureSampleLevel(history, history_sampler, was, 0.0).rgb, vec3<f32>(0.0));
    past = from_ycocg(clamp(to_ycocg(past), low, high));
    let share = mix(1.0, temporal.settings.y, kept);
    let current_weight = share * blend_weight(current);
    let past_weight = (1.0 - share) * blend_weight(past);
    let blended = (current * current_weight + past * past_weight) / max(current_weight + past_weight, 1e-6);
    // A NaN or infinity anywhere would stay in the history for good.
    if any(blended != blended) || any(abs(blended) > vec3<f32>(65000.0)) {
        return vec4<f32>(current, 1.0);
    }
    return vec4<f32>(blended, 1.0);
}

// How much one of the scene's samples counts for an output pixel `d` of
// the scene's pixels from it: a Gaussian about as wide as a Lanczos 2
// kernel's middle lobe (1 at the pixel, 0.61 half a pixel off, 0.14 a whole
// pixel off).
fn sample_weight(d: vec2<f32>) -> f32 {
    return exp(-2.0 * dot(d, d));
}

fn history_texel(x: f32, y: f32) -> vec3<f32> {
    return textureSampleLevel(history, history_sampler, vec2<f32>(x, y), 0.0).rgb;
}

// Last frame's picture at `uv`, through a Catmull-Rom spline over its
// pixels in five bilinear reads (the corners' share is left out), so the
// history doesn't soften a little more each frame as a bilinear read would.
fn history_at(uv: vec2<f32>) -> vec3<f32> {
    let place = uv * temporal.output.xy;
    let center = floor(place - 0.5) + 0.5;
    let f = place - center;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let texel = temporal.output.zw;
    let at0 = (center - 1.0) * texel;
    let at3 = (center + 2.0) * texel;
    let at12 = (center + w2 / w12) * texel;
    let weights = array<f32, 5>(
        w12.x * w0.y,
        w0.x * w12.y,
        w12.x * w12.y,
        w3.x * w12.y,
        w12.x * w3.y,
    );
    let color = history_texel(at12.x, at0.y) * weights[0]
        + history_texel(at0.x, at12.y) * weights[1]
        + history_texel(at12.x, at12.y) * weights[2]
        + history_texel(at3.x, at12.y) * weights[3]
        + history_texel(at12.x, at3.y) * weights[4];
    let total = weights[0] + weights[1] + weights[2] + weights[3] + weights[4];
    return max(color / max(total, 1e-6), vec3<f32>(0.0));
}

// TAA that upscales: drawn at the output's size, reading the scene (at its
// own, smaller size) and last frame's output-sized picture.
@fragment
fn fs_taau(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy * temporal.output.zw;
    let last = vec2<i32>(temporal.size.xy) - 1;
    // Where this output pixel is on the scene, in its pixels. The jitter
    // moved the picture by `jitter` pixels, so the scene's pixel `i` shows
    // what's at `i + 0.5 - jitter`; `base` is the pixel showing the place.
    let jitter = temporal.jitter.xy;
    let place = uv * temporal.size.xy;
    let base = vec2<i32>(floor(place + jitter));
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    var nearest_weight = 0.0;
    var low = vec3<f32>(1e9);
    var high = vec3<f32>(-1e9);
    var nearest = clamp(base, vec2<i32>(0), last);
    var nearest_depth = 2.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let offset = vec2<i32>(x, y);
            let at = clamp(base + offset, vec2<i32>(0), last);
            let color = max(textureLoad(scene, at, 0).rgb, vec3<f32>(0.0));
            let weight = sample_weight(vec2<f32>(base + offset) + 0.5 - jitter - place);
            sum += color * weight;
            total += weight;
            nearest_weight = max(nearest_weight, weight);
            let ycocg = to_ycocg(color);
            low = min(low, ycocg);
            high = max(high, ycocg);
            let depth = textureLoad(depth_texture, at, 0);
            if depth < nearest_depth {
                nearest_depth = depth;
                nearest = at;
            }
        }
    }
    let current = sum / max(total, 1e-6);
    if temporal.settings.x == 0.0 {
        return vec4<f32>(current, 1.0);
    }
    let moved = textureLoad(motion, nearest, 0).xy;
    let was = uv - moved;
    if any(was < vec2<f32>(0.0)) || any(was > vec2<f32>(1.0)) {
        return vec4<f32>(current, 1.0);
    }
    let previous_pixel = clamp(vec2<i32>(was * temporal.size.xy), vec2<i32>(0), last);
    let moved_before = textureLoad(previous_motion, previous_pixel, 0).xy;
    let difference = length((moved - moved_before) * temporal.size.xy);
    let kept = 1.0 - saturate((difference - 0.5) / 1.5);
    let past = from_ycocg(clamp(to_ycocg(history_at(was)), low, high));
    // Each output pixel has a sample of its own only now and then: this
    // frame counts for more when one lands near it.
    let fresh = temporal.settings.y * (0.25 + 0.75 * nearest_weight);
    let share = mix(1.0, fresh, kept);
    let current_weight = share * blend_weight(current);
    let past_weight = (1.0 - share) * blend_weight(past);
    let blended = (current * current_weight + past * past_weight) / max(current_weight + past_weight, 1e-6);
    if any(blended != blended) || any(abs(blended) > vec3<f32>(65000.0)) {
        return vec4<f32>(current, 1.0);
    }
    return vec4<f32>(blended, 1.0);
}

// A pixel's streak: half of it each way, in pixels, held to the longest.
fn streak_at(pixel: vec2<i32>) -> vec2<f32> {
    let streak = textureLoad(motion, pixel, 0).xy * temporal.size.xy * temporal.blur.x * 0.5;
    let length_now = length(streak);
    if length_now > temporal.blur.y {
        return streak * (temporal.blur.y / length_now);
    }
    return streak;
}

// The longest streak in each tile of the screen.
@fragment
fn fs_tile_max(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let tile = i32(temporal.blur.w);
    let first = vec2<i32>(position.xy) * tile;
    let last = vec2<i32>(temporal.size.xy) - 1;
    var longest = vec2<f32>(0.0);
    for (var y = 0; y < tile; y += 1) {
        for (var x = 0; x < tile; x += 1) {
            let at = first + vec2<i32>(x, y);
            if any(at > last) {
                continue;
            }
            let streak = streak_at(at);
            if dot(streak, streak) > dot(longest, longest) {
                longest = streak;
            }
        }
    }
    return vec4<f32>(longest, 0.0, 0.0);
}

// How far in front of the camera the prepass's surface at `pixel` is, in
// studs (the far plane for the sky).
fn distance_at(pixel: vec2<i32>) -> f32 {
    let depth = textureLoad(depth_texture, pixel, 0);
    let near = temporal.settings.z;
    let far = temporal.settings.w;
    return near * far / max(far - depth * (far - near), 1e-6);
}

// 1 when a surface at `a` studs is in front of one at `b`, fading to 0 as it
// falls behind.
fn in_front(a: f32, b: f32) -> f32 {
    return saturate(1.0 - (a - b) / max(0.05 * b, 0.1));
}

fn cone(along: f32, reach: f32) -> f32 {
    return saturate(1.0 - along / max(reach, 1e-3)) * f32(reach > 0.0);
}

fn cylinder(along: f32, reach: f32) -> f32 {
    return 1.0 - smoothstep(0.95 * reach, 1.05 * reach, along);
}

@fragment
fn fs_motion_blur(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let last = vec2<i32>(temporal.size.xy) - 1;
    let center = textureLoad(scene, pixel, 0).rgb;
    // The longest streak in the tiles around this one.
    let tile = vec2<i32>(position.xy) / i32(temporal.blur.w);
    let last_tile = vec2<i32>(textureDimensions(tiles)) - 1;
    var longest = vec2<f32>(0.0);
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let at = clamp(tile + vec2<i32>(x, y), vec2<i32>(0), last_tile);
            let streak = textureLoad(tiles, at, 0).xy;
            if dot(streak, streak) > dot(longest, longest) {
                longest = streak;
            }
        }
    }
    let reach = length(longest);
    if reach < 0.5 {
        return vec4<f32>(center, 1.0);
    }
    let own = length(streak_at(pixel));
    let distance = distance_at(pixel);
    var weight = 1.0 / max(own, 0.5);
    var sum = center * weight;
    let samples = u32(temporal.blur.z);
    let noise = fract(52.9829189 * fract(dot(position.xy + temporal.noise.x * 5.588238,
        vec2<f32>(0.06711056, 0.00583715))));
    for (var i = 0u; i < samples; i += 1u) {
        // Evenly along the streak, both ways, turned a little per pixel.
        let t = mix(-1.0, 1.0, (f32(i) + noise) / f32(samples));
        let at = clamp(vec2<i32>(floor(position.xy + longest * t)), vec2<i32>(0), last);
        let along = reach * abs(t);
        let theirs = length(streak_at(at));
        let their_distance = distance_at(at);
        let front = in_front(their_distance, distance);
        let behind = in_front(distance, their_distance);
        // What's in front counts as far as its own streak reaches, what's
        // behind as far as this pixel's does, and two moving alike blur
        // together.
        let counts = front * cone(along, theirs) + behind * cone(along, own)
            + cylinder(along, theirs) * cylinder(along, own) * 2.0 * f32(own > 0.0 && theirs > 0.0);
        sum += textureLoad(scene, at, 0).rgb * counts;
        weight += counts;
    }
    return vec4<f32>(sum / weight, 1.0);
}
