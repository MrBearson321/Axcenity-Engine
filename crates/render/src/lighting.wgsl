// Lighting shared by every scene shader: the frame's bindings (group 0),
// the sun and its shadows, point and spot lights, and the sky that lights
// and reflects. Compiled after `sky.wgsl`, `fog.wgsl` and `clouds.wgsl`. `shader.wgsl` (parts and models)
// and `terrain.wgsl` are each compiled after this file, and both light
// their surfaces with `light_surface`.
// Colors here are linear light, and can be brighter than 1 (stage 5 maps them down).

struct Frame {
    view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
    // Normalized direction toward the sun.
    sun_direction: vec4<f32>,
    // Sun color times intensity.
    sun_radiance: vec4<f32>,
    // Ambient color times intensity.
    ambient: vec4<f32>,
    // Turns world space to the sun's view of each shadow cascade (one layer
    // of the shadow map each, nearest the camera first).
    cascades: array<mat4x4<f32>, 4>,
    // The world size of one shadow map texel in each cascade.
    cascade_texels: vec4<f32>,
    // How many studs a cascade's depth range (0 to 1) covers.
    cascade_depths: vec4<f32>,
    // x: 1 if shadows are on, y: how wide the sun looks (penumbra width per
    // stud between caster and receiver), z: how many cascades, w: how far
    // from the camera shadows reach.
    shadow: vec4<f32>,
    // Turns clip space back to world space, to find the sky's direction.
    inv_view_proj: mat4x4<f32>,
    // The sky's colors. The top color's w is the environment intensity.
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
    sky_ground: vec4<f32>,
    // x: how many lights there are, y: the size of a light tile in pixels,
    // z: how many tiles across.
    lights: vec4<u32>,
    // Turns world space to each shadow-casting spot light's view, by its
    // layer of `spot_shadow_map`.
    spot_shadows: array<mat4x4<f32>, 8>,
    // Turns world space to the camera's view space.
    view: mat4x4<f32>,
    // x: how strongly screen-space ambient occlusion darkens ambient light
    // (0 while it's off), zw: 1 over the scene's width and height in pixels.
    ambient_occlusion: vec4<f32>,
    // x: 1 when the environment maps light and are reflected (otherwise
    // the gradient is worked out per pixel), y: the reflection cubemap's
    // last mip level (the roughest), z: 1 when the sky is `sky_image`,
    // w: the sky image's width in pixels.
    environment: vec4<f32>,
    // The sky's diffuse light while the environment maps are on: spherical
    // harmonics (rgb each), already blurred over the hemisphere and divided
    // by pi, so `sky_irradiance` gives what a white matte surface returns.
    sky_light: array<vec4<f32>, 9>,
    // x: how many of `probes` are drawn.
    reflection_probes: vec4<u32>,
    // The reflection probes drawn, smallest box first.
    probes: array<Probe, MAX_PROBES>,
    // Turns world space to each face of each shadow-casting point light's
    // view, six per light (+X, -X, +Y, -Y, +Z, -Z), by its layer of
    // `point_shadow_map`.
    point_shadows: array<mat4x4<f32>, 24>,
    // x: 1 while contact shadows are drawn, y: how far toward a light they
    // look in studs, z: how thick what's on screen is taken to be.
    contact: vec4<f32>,
    // x: how many of `volumes` are drawn, y: 1 to leave the sun's disc out
    // of the sky (while baking light probes: the sun lights surfaces
    // directly, so a disc seen by a probe would count it twice).
    light_probes: vec4<u32>,
    // The light probe volumes drawn, smallest box first.
    volumes: array<Volume, MAX_VOLUMES>,
    // x: seconds (`DrawList::time`), for surface shaders that move. yzw:
    // the wind in studs a second (`DrawList::wind`), for swaying plants.
    clock: vec4<f32>,
    // x: how bright the stars are (0 by day), y: 1 when the sky image is an
    // atmosphere's, so the sun or moon and the stars are drawn over it.
    night: vec4<f32>,
    // What fades into the distance (fog.wgsl).
    fog: Fog,
    // The layer of cloud in the sky (clouds.wgsl).
    clouds: Clouds,
    // x: how wet surfaces are, y: how much snow covers them, each from 0
    // to 1 (`weathered`).
    weather: vec4<f32>,
}

// The most light probe volumes drawn at once. Must match `MAX_VOLUMES` in
// renderer/light_probes.rs.
const MAX_VOLUMES: u32 = 8u;

// A box of the world lit by its grid of baked light probes in place of the
// sky's diffuse light. Must match `VolumeRaw` in renderer/light_probes.rs.
struct Volume {
    // Turns world space to the box's own, one row each (as `Probe.to_box`).
    to_box: array<vec4<f32>, 3>,
    // xyz: half the box's size, w: how far inside its faces it fades in.
    extent: vec4<f32>,
    // xyz: how many probes stand along the box's own axes, each in the
    // middle of its cell, w: its first probe in `light_probes`.
    grid: vec4<u32>,
}

// The most reflection probes drawn at once. Must match `MAX_PROBES` in
// renderer/probes.rs.
const MAX_PROBES: u32 = 8u;

// A box of the world that reflects its bake in place of the sky. Must
// match `ProbeRaw` in renderer/probes.rs.
struct Probe {
    // Turns world space to the box's own (its middle at 0, its sides along
    // the axes), one row each. Each row's xyz is also one of the box's axes
    // in world space.
    to_box: array<vec4<f32>, 3>,
    // xyz: half the box's size, w: how far inside its faces it fades in.
    extent: vec4<f32>,
    // x: its layer of `probe_maps`.
    layer: vec4<u32>,
}

// A point or spot light. Must match `LightRaw` in renderer.rs.
struct Light {
    // xyz: position, w: range.
    position_range: vec4<f32>,
    // rgb: color times brightness, w: cosine of the cone's edge, or -2 for
    // a point light.
    radiance_cone: vec4<f32>,
    // xyz: the direction a spot light points, w: cosine where its edge
    // starts to fade.
    direction_inner: vec4<f32>,
    // x: a spot light's layer of `spot_shadow_map`, or a point light's place
    // in `point_shadow_map` (its six layers start at six times it), or -1
    // if it casts no shadows, y: the width of one shadow texel per stud
    // from the light.
    shadow: vec4<f32>,
}

// The most lights one tile of the screen lists. Must match `LIGHTS_PER_TILE`
// in renderer.rs.
const LIGHTS_PER_TILE: u32 = 64u;

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d_array;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var<storage, read> lights: array<Light>;
// For each screen tile, `LIGHTS_PER_TILE + 1` numbers: how many lights
// reach it, then their indexes in `lights`.
@group(0) @binding(4) var<storage, read> tile_lights: array<u32>;
// One layer per shadow-casting spot light: its depth as the light sees it.
// (Binding 5 is the shadow pass's own, in shader.wgsl.)
@group(0) @binding(6) var spot_shadow_map: texture_depth_2d_array;
// Screen-space ambient occlusion at half the scene's size (ssao.wgsl): 1
// in the open, toward 0 in corners. White while it's off.
@group(0) @binding(7) var screen_occlusion_map: texture_2d<f32>;
@group(0) @binding(8) var screen_occlusion_sampler: sampler;
// (Binding 9 is the frame's joints, in shader.wgsl.)
// The sky as surfaces reflect it, blurrier at each mip level: level
// `roughness * frame.environment.y` for a surface of that roughness.
// A blank 1×1 cube while the environment maps are off.
@group(0) @binding(10) var environment_map: texture_cube<f32>;
@group(0) @binding(11) var environment_sampler: sampler;
// The sky image (see `equirect_uv`), or a blank 1×1 texture without one.
@group(0) @binding(12) var sky_image: texture_2d<f32>;
// Each reflection probe's bake, blurred like `environment_map`, one cube
// per probe (its `layer`). A blank 1×1 cube while none are drawn.
@group(0) @binding(13) var probe_maps: texture_cube_array<f32>;
// Six layers per shadow-casting point light, one for each way along each
// axis: its depth as the light sees it that way.
@group(0) @binding(14) var point_shadow_map: texture_depth_2d_array;
// The prepass's depth (solid items only), for contact shadows. A 1×1
// stand-in while they're off.
@group(0) @binding(15) var prepass_depth: texture_depth_2d;
// Every light probe of the volumes drawn, four entries each: its light as
// spherical harmonics (constant, then along y, z and x; rgb), blurred over
// the hemisphere and divided by pi, like `Frame.sky_light`. Probes run x
// first, then y, then z. One blank probe while there are none.
@group(0) @binding(16) var<storage, read> light_probes: array<vec4<f32>>;

// Points spread evenly over a disc of radius 1 (a Poisson disc), for
// sampling the shadow map around a point. `private` so it can be indexed
// in a loop.
var<private> POISSON: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
    vec2<f32>(-0.94201624, -0.39906216), vec2<f32>(0.94558609, -0.76890725),
    vec2<f32>(-0.09418410, -0.92938870), vec2<f32>(0.34495938, 0.29387760),
    vec2<f32>(-0.91588581, 0.45771432), vec2<f32>(-0.81544232, -0.87912464),
    vec2<f32>(-0.38277543, 0.27676845), vec2<f32>(0.97484398, 0.75648379),
    vec2<f32>(0.44323325, -0.97511554), vec2<f32>(0.53742981, -0.47373420),
    vec2<f32>(-0.26496911, -0.41893023), vec2<f32>(0.79197514, 0.19090188),
    vec2<f32>(-0.24188840, 0.99706507), vec2<f32>(-0.81409955, 0.91437590),
    vec2<f32>(0.19984126, 0.78641367), vec2<f32>(0.14383161, -0.14100790),
);
// The widest a shadow's soft edge is blurred, in shadow map texels.
const MAX_FILTER_TEXELS: f32 = 32.0;
// How far from a receiver blockers are looked for, in studs.
const BLOCKER_SEARCH_STUDS: f32 = 30.0;

// A number from 0 to 1 that changes from pixel to pixel without a visible
// pattern (interleaved gradient noise). It turns the sample pattern per
// pixel, so blurred edges show fine grain instead of banding.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

// Sunlight reaching `world_position` as seen by cascade `index`: x is 1 in
// the open and 0 in shadow, y how far toward the cascade's edge the point
// is (0 inside, rising to 1 at the edge), or -1 if the cascade doesn't
// cover the point.
//
// Soft shadows the way real ones are (percentage-closer soft shadows):
// first the map is searched for what blocks the sun, and the further the
// blocker is from the surface, the wider the blur. So a shadow is crisp
// where a part touches the ground and softens further away.
fn cascade_visibility(
    index: u32,
    world_position: vec3<f32>,
    normal: vec3<f32>,
    rotation: mat2x2<f32>,
    slope: f32,
) -> vec2<f32> {
    let texel = frame.cascade_texels[index];
    // Pushed off the surface so it doesn't shadow itself.
    let shifted = world_position + normal * (texel * 1.5);
    let light = frame.cascades[index] * vec4<f32>(shifted, 1.0);
    let uv = vec2<f32>(light.x * 0.5 + 0.5, 0.5 - light.y * 0.5);
    let receiver = light.z;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || receiver > 1.0 || receiver < 0.0 {
        return vec2<f32>(1.0, -1.0);
    }
    let edge = smoothstep(0.8, 1.0, max(abs(light.x), abs(light.y)));
    let size = f32(textureDimensions(shadow_map).x);
    let depth_range = frame.cascade_depths[index];
    let sun_size = frame.shadow.y;
    let layer = i32(index);
    // One texel of depth, as the map stores it.
    let texel_depth = texel / depth_range;

    // 1. Blockers: how far, on average, the things shading this point are.
    let search = clamp(sun_size * BLOCKER_SEARCH_STUDS / texel, 1.0, MAX_FILTER_TEXELS);
    let search_bias = texel_depth * (1.0 + slope);
    let last = i32(size) - 1;
    var blockers = 0.0;
    var blocker_depth = 0.0;
    for (var k = 0; k < 16; k++) {
        let offset = rotation * POISSON[k] * search;
        let coord = clamp(vec2<i32>(uv * size + offset), vec2<i32>(0), vec2<i32>(last));
        let depth = textureLoad(shadow_map, coord, layer, 0);
        if depth < receiver - search_bias {
            blockers += 1.0;
            blocker_depth += depth;
        }
    }
    if blockers == 0.0 {
        return vec2<f32>(1.0, edge);
    }
    blocker_depth /= blockers;

    // 2. The soft edge's width, from how far light travels past the blocker.
    let penumbra = (receiver - blocker_depth) * depth_range * sun_size;
    let radius = clamp(penumbra / texel, 1.0, MAX_FILTER_TEXELS);

    // 3. Averages comparisons over that width. Each comparison is itself
    // filtered between four texels, so even radius 1 has smooth edges.
    let bias = texel_depth * (1.0 + 0.5 * radius * slope);
    var lit = 0.0;
    for (var k = 0; k < 16; k++) {
        let offset = rotation * POISSON[k] * (radius / size);
        lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, layer, receiver - bias);
    }
    return vec2<f32>(lit / 16.0, edge);
}

// How much sunlight reaches `world_position`: 1 in the open, 0 in shadow.
// `normal` is the surface's own (unmapped) normal, used to push the point
// off the surface so it doesn't shadow itself, and `pixel` where it is on
// screen. The nearest cascade covering the point is used, blended into the
// next one near its edge so the change in sharpness doesn't show.
fn sun_visibility(world_position: vec3<f32>, normal: vec3<f32>, pixel: vec2<f32>) -> f32 {
    if frame.shadow.x == 0.0 {
        return 1.0;
    }
    let angle = pixel_noise(pixel) * 6.2831853;
    let rotation = mat2x2<f32>(cos(angle), sin(angle), -sin(angle), cos(angle));
    // How steeply the surface faces away from the sun (the tangent of the
    // angle), for depth bias: tilted surfaces need more.
    let n_dot_l = clamp(dot(normal, frame.sun_direction.xyz), 0.05, 1.0);
    let slope = min(sqrt(1.0 - n_dot_l * n_dot_l) / n_dot_l, 3.0);

    let count = u32(frame.shadow.z);
    var lit = 1.0;
    for (var i = 0u; i < count; i++) {
        let here = cascade_visibility(i, world_position, normal, rotation, slope);
        if here.y < 0.0 {
            continue;
        }
        lit = here.x;
        if here.y > 0.0 && i + 1u < count {
            let next = cascade_visibility(i + 1u, world_position, normal, rotation, slope);
            if next.y >= 0.0 {
                lit = mix(here.x, next.x, here.y);
            }
        }
        break;
    }
    // Shadows fade out toward their distance limit instead of ending sharply.
    let reach = frame.shadow.w;
    let fade = 1.0 - smoothstep(reach * 0.8, reach, distance(frame.camera_position.xyz, world_position));
    return mix(1.0, lit, fade);
}

// How much of a lamp's light reaches `point`, by layer `layer` of `map`,
// drawn through `view`: 1 in the open, 0 in its shadow, and 1 outside the
// view. Four comparisons around the point, each filtered between four
// texels like the sun's, are averaged.
fn lamp_shadow(
    map: texture_depth_2d_array,
    view: mat4x4<f32>,
    layer: i32,
    point: vec3<f32>,
    rotation: mat2x2<f32>,
) -> f32 {
    let clip = view * vec4<f32>(point, 1.0);
    if clip.w <= 0.0 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0 || ndc.z < 0.0 {
        return 1.0;
    }
    let size = f32(textureDimensions(map).x);
    var lit = 0.0;
    for (var k = 0; k < 4; k++) {
        let corner = vec2<f32>(f32(k & 1), f32(k >> 1u)) * 2.0 - 1.0;
        let offset = rotation * corner * (0.75 / size);
        lit += textureSampleCompareLevel(map, shadow_sampler, uv + offset, layer, ndc.z);
    }
    return lit / 4.0;
}

// How much of a spot or point light reaches `world_position`: 1 in the
// open, 0 in its shadow. `to_light` is the way to the light, `normal` the
// surface's own (unmapped) normal. The point is pushed off the surface and
// toward the light by amounts that grow with its distance from the light,
// as the map's texels do, so it doesn't shadow itself. A point light's is
// looked up in the face of its six that looks the way the point is: the
// one along its longest axis from the light.
fn lamp_visibility(
    light: Light,
    world_position: vec3<f32>,
    normal: vec3<f32>,
    to_light: vec3<f32>,
    rotation: mat2x2<f32>,
) -> f32 {
    let index = i32(light.shadow.x);
    if index < 0 {
        return 1.0;
    }
    let texel = length(to_light) * light.shadow.y;
    let shifted = world_position + normal * (texel * 1.5) + normalize(to_light) * (texel * 2.0);
    if light.radiance_cone.w > -1.5 {
        return lamp_shadow(spot_shadow_map, frame.spot_shadows[index], index, shifted, rotation);
    }
    let away = shifted - light.position_range.xyz;
    let size = abs(away);
    var face = select(5, 4, away.z > 0.0);
    if size.x >= size.y && size.x >= size.z {
        face = select(1, 0, away.x > 0.0);
    } else if size.y >= size.z {
        face = select(3, 2, away.y > 0.0);
    }
    let layer = index * 6 + face;
    return lamp_shadow(point_shadow_map, frame.point_shadows[layer], layer, shifted, rotation);
}

// How many steps contact shadows take toward a light.
const CONTACT_STEPS: u32 = 12u;

// How much light from the way `toward` (normalized) reaches the solid
// surface at `world_position`, by contact shadows: a short march toward
// the light through the prepass's depth, for the small gaps the shadow
// maps are too coarse to see. A step that's behind what's on screen there
// (by less than its thickness) is blocked. Darkest where the blocker is
// nearest, fading to 1 at `reach` studs. 1 while they're off.
fn contact_visibility(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    toward: vec3<f32>,
    reach: f32,
    pixel: vec2<f32>,
) -> f32 {
    if frame.contact.x == 0.0 || reach <= 0.0 || dot(normal, toward) <= 0.0 {
        return 1.0;
    }
    let size = vec2<i32>(textureDimensions(prepass_depth));
    let camera = frame.camera_position.xyz;
    // Off the surface, by more the further away it is as depth gets coarser.
    let bias = 0.01 + distance(camera, world_position) * 0.002;
    let start = world_position + normal * bias;
    let step = toward * (reach / f32(CONTACT_STEPS));
    let jitter = pixel_noise(pixel);
    for (var i = 0u; i < CONTACT_STEPS; i++) {
        let along = f32(i) + jitter;
        let p = start + step * along;
        let clip = frame.view_proj * vec4<f32>(p, 1.0);
        if clip.w <= 0.0 {
            break;
        }
        let ndc = clip.xyz / clip.w;
        if any(abs(ndc.xy) >= vec2<f32>(1.0)) {
            break;
        }
        let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        let texel = min(vec2<i32>(uv * vec2<f32>(size)), size - vec2<i32>(1));
        let depth = textureLoad(prepass_depth, texel, 0);
        if depth >= 1.0 {
            continue;
        }
        // What's on screen there, back in world space.
        let seen = frame.inv_view_proj * vec4<f32>(ndc.xy, depth, 1.0);
        let behind = distance(camera, p) - distance(camera, seen.xyz / seen.w);
        if behind > bias && behind < frame.contact.z {
            return along / f32(CONTACT_STEPS);
        }
    }
    return 1.0;
}

// How much ambient and sky light reaches the solid surface drawn at
// `pixel`, by screen-space ambient occlusion: 1 in the open, less in
// corners, raised to the world's strength. 1 while it's off. See-through
// surfaces aren't in the prepass, so they mustn't use this.
fn screen_occlusion(pixel: vec2<f32>) -> f32 {
    let uv = pixel * frame.ambient_occlusion.zw;
    let open = textureSampleLevel(screen_occlusion_map, screen_occlusion_sampler, uv, 0.0).r;
    return exp2(frame.ambient_occlusion.x * log2(max(open, 0.001)));
}

// What the depth and normal prepass writes for a surface facing `normal`
// (world space): that direction in view space.
fn prepass_normal(normal: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(normalize((frame.view * vec4<f32>(normal, 0.0)).xyz), 1.0);
}

// How many microfacets point along `h` (GGX / Trowbridge-Reitz).
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// How much microfacets shadow each other (Smith with Schlick-GGX).
fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = r * r / 8.0;
    let g_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return g_v * g_l;
}

// How much light reflects instead of entering the surface, by angle.
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

// The sky's color looking along `dir`: ground below the horizon, then a
// gradient up to the top of the sky.
fn sky_gradient(dir: vec3<f32>) -> vec3<f32> {
    return gradient_sky(dir, frame.sky_top.rgb, frame.sky_horizon.rgb, frame.sky_ground.rgb);
}

// What's drawn as the sky looking along `dir`, where one pixel covers
// `spread` radians of it: the sky image, or the gradient, plus the sun (or
// the moon at night: whichever lights the scene) and the stars. The disc is
// far brighter than 1, so it blooms. A creator's sky image has its own sun
// (or none), so nothing is drawn over it; an atmosphere's has none.
fn sky_radiance(dir: vec3<f32>, spread: f32) -> vec3<f32> {
    var sky: vec3<f32>;
    if frame.environment.z > 0.5 {
        // The image's level whose pixels are about the size of the screen's,
        // so its detail doesn't shimmer. Toward straight up and down its
        // columns crowd together, so a blurrier level is read there (at
        // most 4 levels blurrier, so the very top isn't the whole image's
        // average).
        let across = max(sqrt(max(1.0 - dir.y * dir.y, 0.0)), 0.0625);
        let level = log2(max(spread * frame.environment.w / (2.0 * PI) / across, 1.0));
        sky = textureSampleLevel(sky_image, environment_sampler, equirect_uv(dir), level).rgb;
        if frame.night.y < 0.5 {
            return sky;
        }
    } else {
        sky = sky_gradient(dir);
    }
    if frame.light_probes.y != 0u {
        return sky;
    }
    let toward_sun = dot(dir, frame.sun_direction.xyz);
    // Hidden by the ground once it sets.
    let above = smoothstep(-0.004, 0.004, dir.y);
    let disc = smoothstep(0.99965, 0.99985, toward_sun) * above;
    let glow = pow(max(toward_sun, 0.0), 200.0) * 0.4;
    return sky + frame.sun_radiance.rgb * (glow + disc * 8.0) + stars(dir, spread) * frame.night.x;
}

// A number from 0 to 1 for each whole-numbered point (a hash).
fn cell_hash(cell: vec3<f32>) -> vec4<f32> {
    var p = fract(cell * vec3<f32>(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    let q = fract((p.xxy + p.yzz) * p.zyx);
    return vec4<f32>(q, fract(dot(q, vec3<f32>(17.13, 31.71, 11.37))));
}

// The stars' light looking along `dir`: a few of the cells of a grid around
// the viewer hold a star, at a point of their own, about a pixel wide
// (`spread`) and of its own brightness, fading out toward the horizon.
fn stars(dir: vec3<f32>, spread: f32) -> vec3<f32> {
    if dir.y <= 0.0 {
        return vec3<f32>(0.0);
    }
    let scaled = dir * 150.0;
    let cell = floor(scaled);
    let hash = cell_hash(cell);
    if hash.w < 0.993 {
        return vec3<f32>(0.0);
    }
    let star = normalize(cell + 0.15 + hash.xyz * 0.7);
    let width = max(spread, 0.0004) * 1.2;
    let near = 1.0 - smoothstep(0.0, width, acos(clamp(dot(dir, star), -1.0, 1.0)));
    // Most are faint; a few are bright.
    let brightness = pow((hash.w - 0.993) / 0.007, 3.0);
    let tint = mix(vec3<f32>(0.75, 0.85, 1.0), vec3<f32>(1.0, 0.9, 0.75), hash.x);
    return tint * near * (0.15 + 1.85 * brightness) * smoothstep(0.0, 0.15, dir.y);
}

// Light from the whole sky that reaches a surface facing `n`: a smooth
// blend from the ground's color to the sky's average.
fn environment_diffuse(n: vec3<f32>) -> vec3<f32> {
    let sky = mix(frame.sky_horizon.rgb, frame.sky_top.rgb, 0.5);
    return mix(frame.sky_ground.rgb, sky, smoothstep(-0.7, 0.7, n.y));
}

// What a surface reflects looking along `dir`: the sharp sky when smooth,
// blurring toward its smooth average as roughness grows.
fn environment_specular(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    return mix(sky_gradient(dir), environment_diffuse(dir), smoothstep(0.1, 0.9, roughness));
}

// The sky's light reaching a surface facing `n`, from its spherical
// harmonics. Never below 0, which a few coefficients can ring under.
fn sky_irradiance(n: vec3<f32>) -> vec3<f32> {
    let light = frame.sky_light[0].rgb * 0.282095
        + frame.sky_light[1].rgb * (0.488603 * n.y)
        + frame.sky_light[2].rgb * (0.488603 * n.z)
        + frame.sky_light[3].rgb * (0.488603 * n.x)
        + frame.sky_light[4].rgb * (1.092548 * n.x * n.y)
        + frame.sky_light[5].rgb * (1.092548 * n.y * n.z)
        + frame.sky_light[6].rgb * (0.315392 * (3.0 * n.z * n.z - 1.0))
        + frame.sky_light[7].rgb * (1.092548 * n.x * n.z)
        + frame.sky_light[8].rgb * (0.546274 * (n.x * n.x - n.y * n.y));
    return max(light, vec3<f32>(0.0));
}

// The sky's diffuse light on a surface facing `n`: from the environment
// maps' harmonics while they're on, otherwise from the gradient.
fn sky_diffuse(n: vec3<f32>) -> vec3<f32> {
    if frame.environment.x > 0.5 {
        return sky_irradiance(n);
    }
    return environment_diffuse(n);
}

// What a surface of `roughness` reflects looking along `dir`: the
// reflection cubemap's level for that roughness while the maps are on,
// otherwise the gradient.
fn sky_specular(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    if frame.environment.x > 0.5 {
        let level = roughness * frame.environment.y;
        return textureSampleLevel(environment_map, environment_sampler, dir, level).rgb;
    }
    return environment_specular(dir, roughness);
}

// The diffuse light reaching a surface at `position` facing `n` from all
// around: each light probe volume whose box it's inside, from the eight
// probes around it (blended by how near each is), fading in over the
// volume's blend distance from its faces. Smaller boxes come first, and
// the bigger ones and then the sky (times the environment intensity) fill
// what they leave.
fn surrounding_diffuse(position: vec3<f32>, n: vec3<f32>, environment: f32) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    var left = 1.0;
    let count = min(frame.light_probes.x, MAX_VOLUMES);
    for (var i = 0u; i < count; i++) {
        let volume = frame.volumes[i];
        let p = vec4<f32>(position, 1.0);
        let local = vec3<f32>(dot(volume.to_box[0], p), dot(volume.to_box[1], p), dot(volume.to_box[2], p));
        let half_size = volume.extent.xyz;
        let inside = min(min(half_size.x - abs(local.x), half_size.y - abs(local.y)), half_size.z - abs(local.z));
        if inside <= 0.0 {
            continue;
        }
        let weight = left * smoothstep(0.0, 1.0, inside / max(volume.extent.w, 0.001));
        // Where it is in the grid, in probes: between the two probes nearest
        // it along each axis, held at the outermost ones near the faces.
        let counts = vec3<f32>(volume.grid.xyz);
        let cell = clamp((local / (2.0 * half_size) + 0.5) * counts - 0.5, vec3<f32>(0.0), counts - 1.0);
        let low = vec3<u32>(floor(cell));
        let high = min(low + 1u, volume.grid.xyz - 1u);
        let t = cell - floor(cell);
        var c0 = vec3<f32>(0.0);
        var c1 = vec3<f32>(0.0);
        var c2 = vec3<f32>(0.0);
        var c3 = vec3<f32>(0.0);
        for (var corner = 0u; corner < 8u; corner++) {
            let far = vec3<bool>((corner & 1u) != 0u, (corner & 2u) != 0u, (corner & 4u) != 0u);
            let at = select(low, high, far);
            let shares = select(1.0 - t, t, far);
            let share = shares.x * shares.y * shares.z;
            let probe = 4u * (volume.grid.w + at.x + volume.grid.x * (at.y + volume.grid.y * at.z));
            c0 += share * light_probes[probe].rgb;
            c1 += share * light_probes[probe + 1u].rgb;
            c2 += share * light_probes[probe + 2u].rgb;
            c3 += share * light_probes[probe + 3u].rgb;
        }
        let light = c0 * 0.282095 + (c1 * n.y + c2 * n.z + c3 * n.x) * 0.488603;
        color += weight * max(light, vec3<f32>(0.0));
        left -= weight;
        if left <= 0.001 {
            break;
        }
    }
    return color + left * environment * sky_diffuse(n);
}

// What a surface at `position` of `roughness` reflects looking along
// `dir`: each reflection probe whose box it's inside, read where the
// reflection leaves the box (so near walls line up with the real ones:
// parallax-corrected), fading in over the probe's blend distance from its
// faces. Smaller boxes come first and the bigger ones and then the sky
// (times the environment intensity) fill what they leave.
fn reflected(position: vec3<f32>, dir: vec3<f32>, roughness: f32, environment: f32) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    var left = 1.0;
    let count = min(frame.reflection_probes.x, MAX_PROBES);
    for (var i = 0u; i < count; i++) {
        let probe = frame.probes[i];
        let p = vec4<f32>(position, 1.0);
        let local = vec3<f32>(dot(probe.to_box[0], p), dot(probe.to_box[1], p), dot(probe.to_box[2], p));
        let half_size = probe.extent.xyz;
        let inside = min(min(half_size.x - abs(local.x), half_size.y - abs(local.y)), half_size.z - abs(local.z));
        if inside <= 0.0 {
            continue;
        }
        let weight = left * smoothstep(0.0, 1.0, inside / max(probe.extent.w, 0.001));
        // The reflection in the box's own space, and how far along it each
        // pair of faces is: the nearer face it's heading for leaves first.
        let d = vec3<f32>(
            dot(probe.to_box[0].xyz, dir),
            dot(probe.to_box[1].xyz, dir),
            dot(probe.to_box[2].xyz, dir),
        );
        let safe = select(d, vec3<f32>(1e-6), abs(d) < vec3<f32>(1e-6));
        let exits = (sign(safe) * half_size - local) / safe;
        let hit = local + d * min(min(exits.x, exits.y), exits.z);
        // From the box's middle, where it was baked, back in world space.
        let toward = probe.to_box[0].xyz * hit.x + probe.to_box[1].xyz * hit.y + probe.to_box[2].xyz * hit.z;
        let level = roughness * frame.environment.y;
        color += weight * textureSampleLevel(probe_maps, environment_sampler, toward, i32(probe.layer.x), level).rgb;
        left -= weight;
        if left <= 0.001 {
            break;
        }
    }
    return color + left * environment * sky_specular(dir, roughness);
}

// Average specular reflection of light arriving from every direction
// (Karis' analytic approximation). Multiplies the environment's color to
// give what a surface reflects of it.
fn ambient_specular(f0: vec3<f32>, roughness: f32, n_dot_v: f32) -> vec3<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + ab.y;
}

// How a surface reflects light arriving from direction `l`: the diffuse and
// specular parts together, times the cosine of the angle of arrival.
// Multiply by the light's color to get what the surface gives back.
fn shade(
    n: vec3<f32>, v: vec3<f32>, l: vec3<f32>,
    base_color: vec3<f32>, metalness: f32, roughness: f32, f0: vec3<f32>,
) -> vec3<f32> {
    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0001);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let specular = distribution_ggx(n_dot_h, roughness) * geometry_smith(n_dot_v, n_dot_l, roughness)
        * fresnel / (4.0 * n_dot_v * n_dot_l + 0.0001);
    // Light that isn't reflected enters the surface and scatters (metals absorb it).
    let diffuse = (1.0 - fresnel) * (1.0 - metalness) * base_color / PI;
    return (diffuse + specular) * n_dot_l;
}

// The light a surface point gives back toward the camera: the sun (with
// its shadow), the point and spot lights listed for the point's screen
// tile, and the sky. `n` is the shading normal (tipped by any normal map),
// `geometric` the surface's own, `pixel` where it is on screen,
// `occlusion` how much ambient and sky light reaches it (texture maps and
// screen-space ambient occlusion together), `dielectric` how much a
// non-metal reflects head-on (usually 4%), and `solid` whether it's in the
// prepass (see-through surfaces aren't), so contact shadows can fall on it.
// The weather changes the surface first (`weathered`).
fn light_surface(
    world_position: vec3<f32>,
    geometric: vec3<f32>,
    n: vec3<f32>,
    pixel: vec2<f32>,
    base_color: vec3<f32>,
    metalness: f32,
    roughness: f32,
    occlusion: f32,
    dielectric: vec3<f32>,
    solid: bool,
) -> vec3<f32> {
    let weather = weathered(geometric, n, base_color, metalness, roughness);
    return shade_surface(world_position, geometric, weather.normal, pixel, weather.base_color,
        weather.metalness, weather.roughness, occlusion, dielectric, solid);
}

// How much darker a soaked non-metal is: water fills its pores, so less
// light scatters back out of it.
const WET_DARKENING: f32 = 0.5;
// How rough a soaked surface is at most: a film of water is smooth.
const WET_ROUGHNESS: f32 = 0.08;
// Snow's color (linear) and roughness.
const SNOW_COLOR: vec3<f32> = vec3<f32>(0.85, 0.87, 0.9);
const SNOW_ROUGHNESS: f32 = 0.8;

// A surface as the weather leaves it.
struct Weathered {
    base_color: vec3<f32>,
    metalness: f32,
    roughness: f32,
    normal: vec3<f32>,
}

// The weather on a surface (`frame.weather`): wet surfaces are darker and
// glossier, undersides less so; snow covers surfaces facing up, the
// deeper it is the further down their slopes, and never walls. Snow is
// judged by the surface's own normal (`geometric`), so a normal map's
// bumps don't shed it, and it smooths the shading normal over. Dry and
// bare, the surface is left as it is.
fn weathered(
    geometric: vec3<f32>,
    n: vec3<f32>,
    base_color: vec3<f32>,
    metalness: f32,
    roughness: f32,
) -> Weathered {
    var out = Weathered(base_color, metalness, roughness, n);
    let wetness = frame.weather.x;
    let cover = frame.weather.y;
    if wetness <= 0.0 && cover <= 0.0 {
        return out;
    }
    let up = geometric.y;
    let wet = wetness * smoothstep(-1.0, -0.2, up);
    out.base_color *= mix(1.0, WET_DARKENING, wet * (1.0 - metalness));
    out.roughness = mix(roughness, min(roughness, WET_ROUGHNESS), wet);
    // Full on a flat top from two thirds cover, and at full cover down to
    // about 37 degrees of slope; nothing steeper than about 66 degrees.
    let snow = clamp(1.5 * cover - 2.5 * (1.0 - up), 0.0, 1.0);
    out.base_color = mix(out.base_color, SNOW_COLOR, snow);
    out.metalness = mix(metalness, 0.0, snow);
    out.roughness = mix(out.roughness, SNOW_ROUGHNESS, snow);
    out.normal = normalize(mix(n, geometric, snow));
    return out;
}

// `light_surface` for a surface the weather has already changed.
fn shade_surface(
    world_position: vec3<f32>,
    geometric: vec3<f32>,
    n: vec3<f32>,
    pixel: vec2<f32>,
    base_color: vec3<f32>,
    metalness: f32,
    roughness: f32,
    occlusion: f32,
    dielectric: vec3<f32>,
    solid: bool,
) -> vec3<f32> {
    let v = normalize(frame.camera_position.xyz - world_position);
    let n_dot_v = max(dot(n, v), 0.0001);
    // Metals reflect in their own color.
    let f0 = mix(dielectric, base_color, metalness);

    var sun = shade(n, v, frame.sun_direction.xyz, base_color, metalness, roughness, f0)
        * frame.sun_radiance.rgb
        * sun_visibility(world_position, geometric, pixel)
        * cloud_shadow(frame.clouds, world_position, frame.sun_direction.xyz);
    if solid {
        sun *= contact_visibility(world_position, geometric, frame.sun_direction.xyz,
            frame.contact.y, pixel);
    }

    // Point and spot lights: only the ones listed for this pixel's tile.
    var lit = vec3<f32>(0.0);
    let angle = pixel_noise(pixel) * 6.2831853;
    let rotation = mat2x2<f32>(cos(angle), sin(angle), -sin(angle), cos(angle));
    let tile_size = max(frame.lights.y, 1u);
    let tile = vec2<u32>(pixel) / tile_size;
    let first = (tile.y * frame.lights.z + tile.x) * (LIGHTS_PER_TILE + 1u);
    var count = 0u;
    if frame.lights.x > 0u {
        count = min(tile_lights[first], LIGHTS_PER_TILE);
    }
    for (var i = 0u; i < count; i++) {
        let light = lights[tile_lights[first + 1u + i]];
        let to_light = light.position_range.xyz - world_position;
        let distance_squared = dot(to_light, to_light);
        let range = light.position_range.w;
        if distance_squared >= range * range {
            continue;
        }
        let dist = sqrt(distance_squared);
        let l = to_light / max(dist, 0.0001);
        // Inverse-square falloff that reaches exactly 0 at the range.
        let edge = clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0);
        var strength = edge * edge / (distance_squared + 1.0);
        if light.radiance_cone.w > -1.5 {
            strength *= smoothstep(light.radiance_cone.w, light.direction_inner.w,
                dot(-l, light.direction_inner.xyz));
        }
        if strength > 0.0 {
            var visible = lamp_visibility(light, world_position, geometric, to_light, rotation);
            // Contact shadows for lamps that cast shadows, up to the lamp.
            if solid && light.shadow.x >= 0.0 {
                visible *= contact_visibility(world_position, geometric, l,
                    min(frame.contact.y, dist), pixel);
            }
            lit += shade(n, v, l, base_color, metalness, roughness, f0)
                * light.radiance_cone.rgb * strength * visible;
        }
    }

    // Light from the sky, and a little flat ambient light on top.
    let reflection = reflect(-v, n);
    let environment = frame.sky_top.w;
    let reflectance = ambient_specular(f0, roughness, n_dot_v);
    let diffuse_color = base_color * (1.0 - metalness);
    let ambient = occlusion * (
        frame.ambient.rgb * (diffuse_color + reflectance)
        + surrounding_diffuse(world_position, n, environment) * diffuse_color
        + reflected(world_position, reflection, roughness, environment) * reflectance);
    return sun + lit + ambient;
}
