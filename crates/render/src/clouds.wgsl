// Clouds (graphics Phase 26), shared by the scene's shaders (after
// `fog.wgsl`, before `lighting.wgsl`) and the volumetric fog's. No
// bindings: each caller passes in its own copy of the clouds. See `Clouds`
// in draw_list.rs and renderer/clouds.rs, whose `CloudsRaw` matches
// `Clouds` here. Colors are linear light.
//
// The clouds are a layer between two heights. Above each point of the
// ground, a smooth noise says how thick the cloud is (`cloud_cover`, 0 to
// 1): a cloud fills its layer from the bottom up to that share of it, so
// thick clouds stand tall and thin ones are wisps near the bottom. The sky
// pass looks through the layer (`clouded_sky`), and surfaces take the
// sun's light through it (`cloud_shadow`), from the same noise, so the
// shadows lie under the clouds and move with them.

struct Clouds {
    // x: how much of the sky is cloud, y: how much light the thickest
    // cloud takes away per stud, z: the layer's bottom, w: its top, in
    // studs.
    shape: vec4<f32>,
    // rgb: the clouds' own color, w: 1 while they're drawn.
    color: vec4<f32>,
    // xy: how far the wind has carried them along x and z, in studs.
    offset: vec4<f32>,
    // x: steps through the layer (0 for a flat sheet), y: steps from each
    // toward the sun (0 to judge it by the cloud above), z: 1 for fine
    // detail.
    quality: vec4<f32>,
}

// How many studs across the clouds' largest shapes are.
const CLOUD_SCALE: f32 = 1600.0;
// How many studs the clouds' fine detail repeats over.
const CLOUD_DETAIL_SCALE: f32 = 150.0;
// How far away clouds are seen, in studs: they fade into the sky toward it.
const CLOUD_FAR: f32 = 20000.0;
// The longest stretch of the layer a ray is marched through, in studs.
// Near the horizon a ray crosses many clouds, which hide what's beyond, and
// longer steps would miss their detail and show as streaks.
const CLOUD_MARCH: f32 = 4000.0;

// A number from 0 to 1 for each whole-numbered point (a hash).
fn cloud_hash2(cell: vec2<f32>) -> f32 {
    var p = fract(vec3<f32>(cell.xyx) * 0.1031);
    p += dot(p, p.yzx + 33.33);
    return fract((p.x + p.y) * p.z);
}

fn cloud_hash3(cell: vec3<f32>) -> f32 {
    var p = fract(cell * 0.1031);
    p += dot(p, p.zyx + 31.32);
    return fract((p.x + p.y) * p.z);
}

// Smooth noise from 0 to 1 (value noise).
fn cloud_noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = cloud_hash2(i);
    let b = cloud_hash2(i + vec2<f32>(1.0, 0.0));
    let c = cloud_hash2(i + vec2<f32>(0.0, 1.0));
    let d = cloud_hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn cloud_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let x0 = mix(
        mix(cloud_hash3(i), cloud_hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
        mix(cloud_hash3(i + vec3<f32>(0.0, 1.0, 0.0)), cloud_hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x),
        u.y,
    );
    let x1 = mix(
        mix(cloud_hash3(i + vec3<f32>(0.0, 0.0, 1.0)), cloud_hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
        mix(cloud_hash3(i + vec3<f32>(0.0, 1.0, 1.0)), cloud_hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x),
        u.y,
    );
    return mix(x0, x1, u.z);
}

// How thick the cloud is above the level point `xz`, from 0 (clear sky)
// to 1 (filling the layer): four octaves of noise, each turned so their
// grids don't line up, cut at a level set by the cover.
fn cloud_cover(clouds: Clouds, xz: vec2<f32>) -> f32 {
    var p = (xz - clouds.offset.xy) / CLOUD_SCALE;
    let turn = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    var sum = 0.0;
    var amplitude = 0.5;
    for (var octave = 0; octave < 4; octave++) {
        sum += amplitude * cloud_noise2(p);
        p = turn * p * 2.03 + vec2<f32>(17.1, 3.7);
        amplitude *= 0.5;
    }
    // The noise mostly lies between 0.2 and 0.8: no cover cuts above all
    // of it, full cover below.
    let cut = 0.85 - 0.75 * clouds.shape.x;
    return clamp((sum / 0.9375 - cut) / 0.25, 0.0, 1.0);
}

// How much light the cloud takes away per stud at `p`.
fn cloud_density(clouds: Clouds, p: vec3<f32>) -> f32 {
    let h = (p.y - clouds.shape.z) / (clouds.shape.w - clouds.shape.z);
    if h <= 0.0 || h >= 1.0 {
        return 0.0;
    }
    let cover = cloud_cover(clouds, p.xz);
    if cover <= 0.0 {
        return 0.0;
    }
    // Up to `cover` of the layer, rounded over the top and softened at the
    // bottom.
    var density = clamp((cover - h) * 4.0, 0.0, 1.0) * clamp(h * 12.0, 0.0, 1.0);
    if clouds.quality.z > 0.5 && density > 0.0 {
        let detail = cloud_noise3((p - vec3<f32>(clouds.offset.x, 0.0, clouds.offset.y)) / CLOUD_DETAIL_SCALE);
        density = clamp(density * 1.4 - 0.5 * detail, 0.0, 1.0);
    }
    return density * clouds.shape.y;
}

// How much of the sun's light gets through `depth` of cloud (optical
// depth). Light scattered many times inside it gets further than a single
// path suggests, so a share fades much more slowly.
fn cloud_light_through(depth: f32) -> f32 {
    return 0.7 * exp(-depth) + 0.3 * exp(-0.25 * depth);
}

// How much of the sun's light the clouds scatter toward a viewer looking
// along `dir`: mostly forward (the silver lining toward the sun), a little
// back. Scaled so a thick cloud lit from above is about as bright as a
// white surface.
fn cloud_phase(dir: vec3<f32>, toward_sun: vec3<f32>) -> f32 {
    let cos_theta = dot(dir, toward_sun);
    let forward = (1.0 - 0.25) / pow(1.25 - cos_theta, 1.5);
    let back = (1.0 - 0.09) / pow(1.09 + 0.6 * cos_theta, 1.5);
    return (0.8 * forward + 0.2 * back) / 3.14159265;
}

// How much of the sun's light reaches `position` through the clouds: 1
// in the open, less in a cloud's shadow. The sun's path is taken through
// the cloud above the point where it crosses the layer, low down where
// the clouds are.
fn cloud_shadow(clouds: Clouds, position: vec3<f32>, toward_sun: vec3<f32>) -> f32 {
    if clouds.color.w == 0.0 || toward_sun.y <= 0.0 || position.y >= clouds.shape.w {
        return 1.0;
    }
    let thickness = clouds.shape.w - clouds.shape.z;
    let crossing = max(clouds.shape.z + 0.25 * thickness, position.y);
    let at = position.xz + toward_sun.xz * (crossing - position.y) / max(toward_sun.y, 0.05);
    let cover = cloud_cover(clouds, at);
    let depth = clouds.shape.y * cover * cover * thickness / max(toward_sun.y, 0.25);
    return cloud_light_through(depth);
}

// The sun's light reaching `p`, inside the clouds: through `steps` steps
// toward the sun, or, with none, through as much cloud as stands above it.
fn cloud_sunlight(clouds: Clouds, p: vec3<f32>, toward_sun: vec3<f32>, steps: u32) -> f32 {
    let thickness = clouds.shape.w - clouds.shape.z;
    let up = max(toward_sun.y, 0.25);
    if steps == 0u {
        let h = (p.y - clouds.shape.z) / thickness;
        let above = max(cloud_cover(clouds, p.xz) - h, 0.0) * thickness;
        return cloud_light_through(clouds.shape.y * above / up);
    }
    // To the top of the layer along the sun's way, but no further than
    // twice its thickness.
    let length = min((clouds.shape.w - p.y) / up, 2.0 * thickness);
    let step = length / f32(steps);
    var depth = 0.0;
    for (var i = 0u; i < steps; i++) {
        depth += cloud_density(clouds, p + toward_sun * (f32(i) + 0.5) * step) * step;
    }
    return cloud_light_through(depth);
}

// What's seen looking along `dir` (normalized) from `eye` through the
// clouds, in front of `sky`: the sky dimmed by them, plus the sun's light
// (`sun`, toward `toward_sun`) and the sky's (`sky_above` above, `sky_around`
// around the horizon) that they scatter. `jitter` (0 to 1, changing from
// pixel to pixel) staggers the steps so they don't show as bands.
fn clouded_sky(
    clouds: Clouds,
    eye: vec3<f32>,
    dir: vec3<f32>,
    sky: vec3<f32>,
    toward_sun: vec3<f32>,
    sun: vec3<f32>,
    sky_above: vec3<f32>,
    sky_around: vec3<f32>,
    jitter: f32,
) -> vec3<f32> {
    if clouds.color.w == 0.0 {
        return sky;
    }
    let bottom = clouds.shape.z;
    let top = clouds.shape.w;
    let thickness = top - bottom;
    let phase = cloud_phase(dir, toward_sun);
    let color = clouds.color.rgb;
    let steps = u32(clouds.quality.x);

    // A flat sheet a quarter of the way up, where the clouds are.
    if steps == 0u {
        let height = bottom + 0.25 * thickness;
        let t = (height - eye.y) / dir.y;
        if abs(dir.y) < 1e-4 || t <= 0.0 || t >= CLOUD_FAR {
            return sky;
        }
        let p = eye + dir * t;
        let cover = cloud_cover(clouds, p.xz);
        let fade = 1.0 - smoothstep(0.25 * CLOUD_FAR, CLOUD_FAR, t);
        let alpha = (1.0 - exp(-clouds.shape.y * cover * thickness * 0.5)) * cover * fade;
        let through_sun = cloud_light_through(clouds.shape.y * cover * cover * thickness / max(toward_sun.y, 0.25));
        let light = color * (sun * phase * through_sun + mix(sky_around, sky_above, 0.5) * 0.75);
        return mix(sky, light, alpha);
    }

    // Where the ray is inside the layer.
    var near = 0.0;
    var far = 0.0;
    if abs(dir.y) < 1e-4 {
        if eye.y <= bottom || eye.y >= top {
            return sky;
        }
        far = CLOUD_FAR;
    } else {
        let to_bottom = (bottom - eye.y) / dir.y;
        let to_top = (top - eye.y) / dir.y;
        near = max(min(to_bottom, to_top), 0.0);
        far = min(max(to_bottom, to_top), CLOUD_FAR);
    }
    far = min(far, near + CLOUD_MARCH);
    if far <= near {
        return sky;
    }
    let step = (far - near) / f32(steps);
    let light_steps = u32(clouds.quality.y);
    var through = 1.0;
    var light = vec3<f32>(0.0);
    for (var i = 0u; i < steps; i++) {
        let p = eye + dir * (near + (f32(i) + jitter) * step);
        let density = cloud_density(clouds, p);
        if density <= 0.0 {
            continue;
        }
        let h = (p.y - bottom) / thickness;
        let lit = sun * phase * cloud_sunlight(clouds, p, toward_sun, light_steps)
            + mix(sky_around, sky_above, h) * (0.5 + 0.5 * h);
        // Each step's own light, dimmed by the cloud in front of it
        // (integrated exactly over the step, so it conserves energy).
        let step_through = exp(-density * step);
        light += through * color * lit * (1.0 - step_through);
        through *= step_through;
        if through < 0.01 {
            break;
        }
    }
    // Far clouds fade into the sky behind them.
    let fade = 1.0 - smoothstep(0.25 * CLOUD_FAR, CLOUD_FAR, near);
    return sky * mix(1.0, through, fade) + light * fade;
}
