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

// The item's base color texture, sRGB so samples are linear light. Items
// without one get a single white pixel, which changes nothing.
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var base_color_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(12) uv: vec2<f32>,
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
    return out;
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
    let texel = textureSample(base_color_texture, base_color_sampler, in.uv);
    let base_color = in.base_color.rgb * texel.rgb;
    let opacity = in.base_color.a * texel.a;
    // Cut-out materials (leaves, fences) leave holes where the texture is clear.
    if opacity < in.surface.z {
        discard;
    }
    let metalness = clamp(in.surface.x, 0.0, 1.0);
    // Perfectly smooth surfaces make the highlight infinitely small, so keep a minimum.
    let roughness = clamp(in.surface.y, 0.04, 1.0);

    let n = normalize(in.normal);
    let v = normalize(frame.camera_position.xyz - in.world_position);
    let l = frame.sun_direction.xyz;
    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0001);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    // Non-metals reflect about 4% of light; metals reflect in their own color.
    let f0 = mix(vec3<f32>(0.04), base_color, metalness);
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let specular = distribution_ggx(n_dot_h, roughness) * geometry_smith(n_dot_v, n_dot_l, roughness)
        * fresnel / (4.0 * n_dot_v * n_dot_l + 0.0001);
    // Light that isn't reflected enters the surface and scatters (metals absorb it).
    let diffuse = (1.0 - fresnel) * (1.0 - metalness) * base_color / PI;
    let sun = (diffuse + specular) * frame.sun_radiance.rgb * n_dot_l;

    let ambient = frame.ambient.rgb
        * (base_color * (1.0 - metalness) + ambient_specular(f0, roughness, n_dot_v));

    // Solid items stay solid whatever the texture's alpha, so the window's
    // own alpha stays 1.
    let alpha = select(opacity, 1.0, in.base_color.a >= 1.0);
    return vec4<f32>(sun + ambient + in.emissive, alpha);
}
