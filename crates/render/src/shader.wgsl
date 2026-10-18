// Physically based shading with a sun, sun shadows, point and spot lights,
// and a procedural sky that lights and reflects (rendering stages 1 to 4).
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
    // Turns world space to the sun's view of the scene (the shadow map).
    light_view_proj: mat4x4<f32>,
    // x: 1 if shadows are on, y: the world size of one shadow map texel,
    // z: depth bias, w: how far from the camera shadows reach.
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
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var<storage, read> lights: array<Light>;
// For each screen tile, `LIGHTS_PER_TILE + 1` numbers: how many lights
// reach it, then their indexes in `lights`.
@group(0) @binding(4) var<storage, read> tile_lights: array<u32>;

// How the item's texture maps are read. Must match `MapsUniform` in
// renderer.rs.
struct Maps {
    // Which channel of each map holds its value: 1 in that channel, 0 in
    // the others.
    metalness: vec4<f32>,
    roughness: vec4<f32>,
    occlusion: vec4<f32>,
    opacity: vec4<f32>,
    // 1 to use 1 minus the value, for metalness, roughness, occlusion and opacity.
    invert: vec4<f32>,
    // x: normal map scale (0: no normal map), y: 1, or -1 to flip the
    // normal map's green, z: occlusion strength, w: 1 if opacity comes from
    // its own map rather than the base color's alpha.
    settings: vec4<f32>,
    // x: 1 if there's a specular map.
    specular: vec4<f32>,
}

// The item's material textures. Color ones (base color, emission) are read
// through sRGB views, so samples are linear light; the rest hold plain
// values. A map an item doesn't have is a single white pixel, which
// changes nothing.
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@group(1) @binding(2) var<uniform> maps: Maps;
@group(1) @binding(3) var normal_texture: texture_2d<f32>;
@group(1) @binding(4) var metalness_texture: texture_2d<f32>;
@group(1) @binding(5) var roughness_texture: texture_2d<f32>;
@group(1) @binding(6) var occlusion_texture: texture_2d<f32>;
@group(1) @binding(7) var emissive_texture: texture_2d<f32>;
@group(1) @binding(8) var opacity_texture: texture_2d<f32>;
@group(1) @binding(9) var specular_texture: texture_2d<f32>;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(12) uv: vec2<f32>,
    @location(13) tangent: vec4<f32>,
}

// Per draw item. Must match `InstanceRaw` in renderer.rs.
struct InstanceIn {
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) normal_0: vec3<f32>,
    @location(7) normal_1: vec3<f32>,
    @location(8) normal_2: vec3<f32>,
    @location(9) base_color: vec4<f32>, // rgb, opacity
    @location(10) emissive: vec3<f32>,
    @location(11) surface: vec3<f32>, // metalness, roughness, alpha cutoff
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) surface: vec3<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) tangent: vec4<f32>,
}

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let normal_matrix = mat3x3<f32>(instance.normal_0, instance.normal_1, instance.normal_2);
    let world_position = model * vec4<f32>(vertex.position, 1.0);

    var out: VertexOut;
    out.clip_position = frame.view_proj * world_position;
    out.world_position = world_position.xyz;
    out.normal = normal_matrix * vertex.normal;
    out.base_color = instance.base_color;
    out.emissive = instance.emissive;
    out.surface = instance.surface;
    out.uv = vertex.uv;
    // A direction along the surface, so it stretches with the shape.
    out.tangent = vec4<f32>((model * vec4<f32>(vertex.tangent.xyz, 0.0)).xyz, vertex.tangent.w);
    return out;
}

// Draws solid items into the shadow map, from the sun's point of view.
@vertex
fn vs_shadow(
    @location(0) position: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(model_0, model_1, model_2, model_3);
    return frame.light_view_proj * model * vec4<f32>(position, 1.0);
}

// How much sunlight reaches `world_position`: 1 in the open, 0 in shadow.
// `normal` is the surface's own (unmapped) normal, used to push the point
// off the surface so it doesn't shadow itself.
fn sun_visibility(world_position: vec3<f32>, normal: vec3<f32>) -> f32 {
    if frame.shadow.x == 0.0 {
        return 1.0;
    }
    let texel = frame.shadow.y;
    let shifted = world_position + normal * (texel * 1.5);
    let light = frame.light_view_proj * vec4<f32>(shifted, 1.0);
    let uv = vec2<f32>(light.x * 0.5 + 0.5, 0.5 - light.y * 0.5);
    let depth = light.z - frame.shadow.z;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || depth > 1.0 {
        return 1.0;
    }
    // Averages a 3x3 block of comparisons, for soft edges. The "level"
    // variant can be used after the shader may have left early.
    let size = vec2<f32>(textureDimensions(shadow_map));
    var lit = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) / size;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, depth);
        }
    }
    lit /= 9.0;
    // Shadows fade out toward their distance limit instead of ending sharply.
    let reach = frame.shadow.w;
    let fade = 1.0 - smoothstep(reach * 0.8, reach, distance(frame.camera_position.xyz, world_position));
    return mix(1.0, lit, fade);
}

// One map's value: the channel picked by `mask`, inverted if `invert` is 1.
fn map_value(texel: vec4<f32>, mask: vec4<f32>, invert: f32) -> f32 {
    let value = dot(texel, mask);
    return mix(value, 1.0 - value, invert);
}

// The surface normal, tipped by the normal map if there is one.
fn surface_normal(in: VertexOut) -> vec3<f32> {
    let n = normalize(in.normal);
    let texel = textureSample(normal_texture, material_sampler, in.uv).xyz;
    // The tangent, made perpendicular to the normal.
    let along = in.tangent.xyz - n * dot(n, in.tangent.xyz);
    if maps.settings.x == 0.0 || dot(along, along) < 1e-12 {
        return n;
    }
    let t = normalize(along);
    let b = cross(n, t) * in.tangent.w;
    var tipped = texel * 2.0 - 1.0;
    tipped = vec3<f32>(tipped.x, tipped.y * maps.settings.y, tipped.z);
    tipped = vec3<f32>(tipped.xy * maps.settings.x, max(tipped.z, 0.0));
    let mapped = mat3x3<f32>(t, b, n) * tipped;
    if dot(mapped, mapped) < 1e-12 {
        return n;
    }
    return normalize(mapped);
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

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Every texture is sampled before anything branches, as WGSL requires.
    let texel = textureSample(base_color_texture, material_sampler, in.uv);
    let metalness_texel = textureSample(metalness_texture, material_sampler, in.uv);
    let roughness_texel = textureSample(roughness_texture, material_sampler, in.uv);
    let occlusion_texel = textureSample(occlusion_texture, material_sampler, in.uv);
    let emissive_texel = textureSample(emissive_texture, material_sampler, in.uv);
    let opacity_texel = textureSample(opacity_texture, material_sampler, in.uv);
    let specular_texel = textureSample(specular_texture, material_sampler, in.uv);
    let n = surface_normal(in);

    let base_color = in.base_color.rgb * texel.rgb;
    let opacity_map = map_value(opacity_texel, maps.opacity, maps.invert.w);
    let opacity = in.base_color.a * mix(texel.a, opacity_map, maps.settings.w);
    // Cut-out materials (leaves, fences) leave holes where the texture is clear.
    if opacity < in.surface.z {
        discard;
    }
    let metalness = clamp(
        in.surface.x * map_value(metalness_texel, maps.metalness, maps.invert.x), 0.0, 1.0);
    // Perfectly smooth surfaces make the highlight infinitely small, so keep a minimum.
    let roughness = clamp(
        in.surface.y * map_value(roughness_texel, maps.roughness, maps.invert.y), 0.04, 1.0);
    let occlusion = mix(
        1.0, map_value(occlusion_texel, maps.occlusion, maps.invert.z), maps.settings.z);
    let emissive = in.emissive * emissive_texel.rgb;
    // Non-metals reflect about 4% of light, or as their specular map says.
    let dielectric = mix(vec3<f32>(0.04), 0.08 * specular_texel.rgb, maps.specular.x);

    let v = normalize(frame.camera_position.xyz - in.world_position);
    let n_dot_v = max(dot(n, v), 0.0001);
    // Metals reflect in their own color.
    let f0 = mix(dielectric, base_color, metalness);

    let sun = shade(n, v, frame.sun_direction.xyz, base_color, metalness, roughness, f0)
        * frame.sun_radiance.rgb
        * sun_visibility(in.world_position, normalize(in.normal));

    // Point and spot lights: only the ones listed for this pixel's tile.
    var lit = vec3<f32>(0.0);
    let tile_size = max(frame.lights.y, 1u);
    let tile = vec2<u32>(in.clip_position.xy) / tile_size;
    let first = (tile.y * frame.lights.z + tile.x) * (LIGHTS_PER_TILE + 1u);
    var count = 0u;
    if frame.lights.x > 0u {
        count = min(tile_lights[first], LIGHTS_PER_TILE);
    }
    for (var i = 0u; i < count; i++) {
        let light = lights[tile_lights[first + 1u + i]];
        let to_light = light.position_range.xyz - in.world_position;
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

    // Solid items stay solid whatever the texture's alpha, so the window's
    // own alpha stays 1.
    let alpha = select(opacity, 1.0, in.base_color.a >= 1.0);
    return vec4<f32>(sun + lit + ambient + emissive, alpha);
}

// A full-screen triangle at the far plane, for drawing the sky.
struct SkyOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) clip_xy: vec2<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOut {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip_position = vec4<f32>(xy, 1.0, 1.0);
    out.clip_xy = xy;
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = frame.inv_view_proj * vec4<f32>(in.clip_xy, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - frame.camera_position.xyz);
    return vec4<f32>(sky_radiance(dir), 1.0);
}
