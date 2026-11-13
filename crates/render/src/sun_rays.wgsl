// Sun rays (a SunRaysEffect, renderer/sun_rays.rs): shafts of sunlight
// streaming through gaps in whatever is in front of the sun. Each pixel
// looks along its way toward the sun's place on screen and counts the sky
// it passes (nothing drawn in the prepass's depth), near the sun, nearer
// the pixel counting more. The sun's light times that is added to the
// scene, so a gap in a canopy becomes a beam and the leaves around it cast
// dark streaks through the glow.

// Must match SunRaysUniform in renderer/sun_rays.rs.
struct SunRays {
    // x: strength, y: how far beams reach (screen heights), zw: the sun's
    // place on screen (0 to 1 from the top left; it may be off the screen).
    rays: vec4<f32>,
    // rgb: the sun's light, w: the screen's aspect (width over height).
    light: vec4<f32>,
    // x: a number that changes each frame under TAA (0 without).
    noise: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sun_rays: SunRays;
@group(0) @binding(1) var depth: texture_depth_2d;

// How many points each pixel looks at on its way toward the sun.
const SAMPLES: u32 = 48u;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    return vec4<f32>(xy, 0.0, 1.0);
}

// A number from 0 to 1 that changes from pixel to pixel in a fine, even
// pattern (interleaved gradient noise), so the steps don't show as bands.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fs_sun_rays(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(depth));
    let last = vec2<i32>(size) - 1;
    let uv = position.xy / size;
    let sun = sun_rays.rays.zw;
    let reach = sun_rays.rays.y;
    let stretch = vec2<f32>(sun_rays.light.w, 1.0);
    let to_sun = sun - uv;
    let way = length(to_sun * stretch);
    // As far toward the sun as a beam reaches: past that, sky adds nearly
    // nothing.
    let step = to_sun * min(1.0, reach / max(way, 0.0001)) / f32(SAMPLES);
    let step_length = length(step * stretch);
    // Sky counts at full strength at the sun and less away from it, none
    // past this far (in screen heights).
    let around_sun = 0.2 + 0.4 * reach;
    // Under TAA the start turns each frame, so the noise averages away
    // instead of leaving a fixed dither over the picture.
    var at = uv + step * fract(pixel_noise(position.xy) + sun_rays.noise.x * 0.618034);
    var sum = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let texel = clamp(vec2<i32>(at * size), vec2<i32>(0), last);
        let sky = select(0.0, 1.0, textureLoad(depth, texel, 0) >= 1.0);
        let from_pixel = length((at - uv) * stretch);
        let from_sun = length((at - sun) * stretch);
        let along = exp(-3.0 * from_pixel / reach);
        let near_sun = 1.0 - smoothstep(0.0, around_sun, from_sun);
        sum += sky * along * near_sun * near_sun;
        at += step;
    }
    // Each point stands for a step's length of the way; a beam with open
    // sky all the way to the sun next to the pixel sums to about the reach
    // over 3 times `along`'s scale.
    let rays = sum * step_length * 3.0 / reach;
    return vec4<f32>(sun_rays.light.rgb * (rays * sun_rays.rays.x), 0.0);
}
