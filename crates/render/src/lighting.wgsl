// Lighting shared by every scene shader: the frame's bindings (group 0),
// the sun and its shadows, point and spot lights, and the procedural sky
// that lights and reflects. `shader.wgsl` (parts and models) and
// `terrain.wgsl` are each compiled after this file, and both light their
// surfaces with `light_surface`.
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
    cascades: array<mat4x4<f32>, 3>,
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

const PI: f32 = 3.14159265;

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
    let up = clamp(dir.y, 0.0, 1.0);
    let sky = mix(frame.sky_horizon.rgb, frame.sky_top.rgb, pow(up, 0.45));
    return mix(sky, frame.sky_ground.rgb, smoothstep(0.0, 0.2, -dir.y));
}

// What's drawn as the sky: the gradient plus the sun. The sun's disc is
// far brighter than 1, so it blooms.
fn sky_radiance(dir: vec3<f32>) -> vec3<f32> {
    let toward_sun = dot(dir, frame.sun_direction.xyz);
    let disc = smoothstep(0.99965, 0.99985, toward_sun);
    let glow = pow(max(toward_sun, 0.0), 200.0) * 0.4;
    return sky_gradient(dir) + frame.sun_radiance.rgb * (glow + disc * 8.0);
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
// `geometric` the surface's own, `pixel` where it is on screen, and
// `dielectric` how much a non-metal reflects head-on (usually 4%).
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
) -> vec3<f32> {
    let v = normalize(frame.camera_position.xyz - world_position);
    let n_dot_v = max(dot(n, v), 0.0001);
    // Metals reflect in their own color.
    let f0 = mix(dielectric, base_color, metalness);

    let sun = shade(n, v, frame.sun_direction.xyz, base_color, metalness, roughness, f0)
        * frame.sun_radiance.rgb
        * sun_visibility(world_position, geometric, pixel);

    // Point and spot lights: only the ones listed for this pixel's tile.
    var lit = vec3<f32>(0.0);
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
            lit += shade(n, v, l, base_color, metalness, roughness, f0)
                * light.radiance_cone.rgb * strength;
        }
    }

    // Light from the sky, and a little flat ambient light on top.
    let reflection = reflect(-v, n);
    let environment = frame.sky_top.w;
    let reflectance = ambient_specular(f0, roughness, n_dot_v);
    let diffuse_color = base_color * (1.0 - metalness);
    let ambient = occlusion * (
        frame.ambient.rgb * (diffuse_color + reflectance)
        + environment * (environment_diffuse(n) * diffuse_color
            + environment_specular(reflection, roughness) * reflectance));
    return sun + lit + ambient;
}
