// Bakes the sky into environment maps (renderer/environment.rs), compiled
// after `sky.wgsl`. Each pass draws a full-screen triangle into one face
// and mip level of a cubemap. The instance index says which: face
// `instance % 6`, level `instance / 6`.
//
// 1. `fs_sky_face` draws the sky (the gradient or the sky image) into the
//    source cubemap's largest level.
// 2. `fs_downsample` halves each source level into the next, so rough
//    reflections can read a blurred sky without missing small bright spots.
// 3. `fs_prefilter` blurs the sky for each level of the reflection
//    cubemap, rougher at each level, the way a GGX surface of that
//    roughness reflects it.

// Must match `BakeUniform` in renderer/environment.rs.
struct Bake {
    // The gradient's colors (linear light).
    top: vec4<f32>,
    horizon: vec4<f32>,
    ground: vec4<f32>,
    // x: 1 to draw the sky image instead of the gradient, y: the image's mip
    // level whose texels match the source faces' at the horizon (may be
    // below 0), z: directions averaged per texel of a rough level, w: the
    // reflection cubemap's last level.
    settings: vec4<f32>,
}

@group(0) @binding(0) var<uniform> bake: Bake;
// The source cubemap: every level for `fs_prefilter`, only the level above
// the one drawn for `fs_downsample`.
@group(0) @binding(1) var source: texture_cube<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var sky_image: texture_2d<f32>;

struct FaceOut {
    @builtin(position) clip_position: vec4<f32>,
    // Where the pixel is on its face: 0 to 1, left to right and top to bottom.
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) face: u32,
    @location(2) @interpolate(flat) level: u32,
}

@vertex
fn vs_face(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> FaceOut {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: FaceOut;
    out.clip_position = vec4<f32>(xy, 0.0, 1.0);
    out.uv = vec2<f32>(xy.x * 0.5 + 0.5, 0.5 - xy.y * 0.5);
    out.face = instance % 6u;
    out.level = instance / 6u;
    return out;
}

// The direction a cubemap texel at `uv` on `face` looks along, in the
// order and orientation GPUs store cubemap faces: +X, -X, +Y, -Y, +Z, -Z.
fn face_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, -t, -s)); }
        case 1u: { return normalize(vec3<f32>(-1.0, -t, s)); }
        case 2u: { return normalize(vec3<f32>(s, 1.0, t)); }
        case 3u: { return normalize(vec3<f32>(s, -1.0, -t)); }
        case 4u: { return normalize(vec3<f32>(s, -t, 1.0)); }
        default: { return normalize(vec3<f32>(-s, -t, -1.0)); }
    }
}

@fragment
fn fs_sky_face(in: FaceOut) -> @location(0) vec4<f32> {
    let dir = face_direction(in.face, in.uv);
    if bake.settings.x > 0.5 {
        // Toward straight up and down the image's columns crowd together,
        // so a blurrier level is read there (at most 4 levels), as the sky
        // pass does.
        let across = max(sqrt(max(1.0 - dir.y * dir.y, 0.0)), 0.0625);
        let level = max(bake.settings.y - log2(across), 0.0);
        let texel = textureSampleLevel(sky_image, linear_sampler, equirect_uv(dir), level);
        return vec4<f32>(texel.rgb, 1.0);
    }
    return vec4<f32>(gradient_sky(dir, bake.top.rgb, bake.horizon.rgb, bake.ground.rgb), 1.0);
}

@fragment
fn fs_downsample(in: FaceOut) -> @location(0) vec4<f32> {
    // Each texel's middle sits between four of the level above, so one
    // filtered sample averages them.
    let dir = face_direction(in.face, in.uv);
    return vec4<f32>(textureSampleLevel(source, linear_sampler, dir, 0.0).rgb, 1.0);
}

// A number from 0 to 1 for sample `i` of `count`, spread evenly in two
// dimensions with the others (a Hammersley point).
fn hammersley(i: u32, count: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(count), f32(reverseBits(i)) * 2.3283064365386963e-10);
}

// How many microfacets point along a direction `n_dot_h` from the normal
// (GGX), as in `lighting.wgsl`.
fn ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// A microfacet normal around `n` for the point `xi`, picked as often as a
// GGX surface of `roughness` has them (importance sampling).
fn ggx_half_vector(xi: vec2<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return normalize(tangent * (sin_theta * cos(phi)) + bitangent * (sin_theta * sin(phi)) + n * cos_theta);
}

@fragment
fn fs_prefilter(in: FaceOut) -> @location(0) vec4<f32> {
    let n = face_direction(in.face, in.uv);
    if in.level == 0u {
        return vec4<f32>(textureSampleLevel(source, linear_sampler, n, 0.0).rgb, 1.0);
    }
    let roughness = f32(in.level) / bake.settings.w;
    // Looking straight at the surface, so the view, the normal and the
    // reflection share a direction (the split-sum approximation).
    let size = f32(textureDimensions(source).x);
    let texel_solid_angle = 4.0 * PI / (6.0 * size * size);
    let count = u32(bake.settings.z);
    var color = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = 0u; i < count; i++) {
        let h = ggx_half_vector(hammersley(i, count), n, roughness);
        let l = normalize(2.0 * dot(n, h) * h - n);
        let n_dot_l = dot(n, l);
        if n_dot_l > 0.0 {
            // A sample standing for a wide cone reads a blurrier level of
            // the source, so a small bright spot isn't missed or sparkly
            // (filtered importance sampling).
            let pdf = ggx(max(dot(n, h), 0.0), roughness) * 0.25 + 0.0001;
            let sample_solid_angle = 1.0 / (f32(count) * pdf);
            let level = max(0.5 * log2(sample_solid_angle / texel_solid_angle) + 1.0, 0.0);
            color += textureSampleLevel(source, linear_sampler, l, level).rgb * n_dot_l;
            weight += n_dot_l;
        }
    }
    return vec4<f32>(color / max(weight, 0.0001), 1.0);
}
