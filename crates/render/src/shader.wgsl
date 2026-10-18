// Physically based shading with one sun and constant ambient light
// (rendering stage 1). All colors here are linear light.

struct Frame {
    view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
    // Normalized direction toward the sun.
    sun_direction: vec4<f32>,
    // Sun color times intensity.
    sun_radiance: vec4<f32>,
    // Ambient color times intensity.
    ambient: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

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

// Average specular reflection of light arriving from every direction
// (Karis' analytic approximation). Keeps metals from going black in ambient
// light until environment lighting arrives in rendering stage 4.
fn ambient_specular(f0: vec3<f32>, roughness: f32, n_dot_v: f32) -> vec3<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + ab.y;
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
    let l = frame.sun_direction.xyz;
    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0001);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    // Metals reflect in their own color.
    let f0 = mix(dielectric, base_color, metalness);
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let specular = distribution_ggx(n_dot_h, roughness) * geometry_smith(n_dot_v, n_dot_l, roughness)
        * fresnel / (4.0 * n_dot_v * n_dot_l + 0.0001);
    // Light that isn't reflected enters the surface and scatters (metals absorb it).
    let diffuse = (1.0 - fresnel) * (1.0 - metalness) * base_color / PI;
    let sun = (diffuse + specular) * frame.sun_radiance.rgb * n_dot_l;

    let ambient = frame.ambient.rgb * occlusion
        * (base_color * (1.0 - metalness) + ambient_specular(f0, roughness, n_dot_v));

    // Solid items stay solid whatever the texture's alpha, so the window's
    // own alpha stays 1.
    let alpha = select(opacity, 1.0, in.base_color.a >= 1.0);
    return vec4<f32>(sun + ambient + emissive, alpha);
}
