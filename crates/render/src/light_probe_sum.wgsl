// Real-time GI (graphics Phase 20): sums the six faces drawn around a light
// probe into its four spherical harmonics coefficients, and writes them
// into the light probes' buffer where the scene reads them. A copy of
// `cube_light` in renderer/probe_bake.rs: keep them matching.

// How many probes one dispatch sums, one workgroup each. Must match
// `PROBES_PER_FRAME` in renderer/realtime_gi.rs.
const SLOTS: u32 = 2u;
const THREADS: u32 = 64u;
const PI: f32 = 3.14159265;

// Six faces per probe, in cubemap order (+X, -X, +Y, -Y, +Z, -Z), each as
// a camera drew it (+X to the right: a cubemap face mirrored).
@group(0) @binding(0) var faces: texture_2d_array<f32>;
// x: which probe in `light_probes` each workgroup's faces are written to.
@group(0) @binding(1) var<uniform> targets: array<vec4<u32>, SLOTS>;
// Four vec4s a probe, rgb each: lighting.wgsl's `light_probes`.
@group(0) @binding(2) var<storage, read_write> light_probes: array<vec4<f32>>;

// Each thread's sums of the four coefficients; the first's w adds up the
// texels' solid angles.
var<workgroup> sums: array<array<vec4<f32>, 4>, THREADS>;

// The direction a cubemap texel at `u`, `v` of `face` looks along: a copy
// of `face_direction` in environment.wgsl.
fn face_direction(face: u32, u: f32, v: f32) -> vec3<f32> {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;
    var d: vec3<f32>;
    switch face {
        case 0u: { d = vec3<f32>(1.0, -t, -s); }
        case 1u: { d = vec3<f32>(-1.0, -t, s); }
        case 2u: { d = vec3<f32>(s, 1.0, t); }
        case 3u: { d = vec3<f32>(s, -1.0, -t); }
        case 4u: { d = vec3<f32>(s, -t, 1.0); }
        default: { d = vec3<f32>(-s, -t, -1.0); }
    }
    return normalize(d);
}

@compute @workgroup_size(64)
fn sum_probe(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    let size = textureDimensions(faces).x;
    let per_face = size * size;
    var c0 = vec4<f32>(0.0);
    var c1 = vec3<f32>(0.0);
    var c2 = vec3<f32>(0.0);
    var c3 = vec3<f32>(0.0);
    for (var i = thread; i < per_face * 6u; i += THREADS) {
        let face = i / per_face;
        let x = (i % per_face) % size;
        let y = (i % per_face) / size;
        let u = (f32(x) + 0.5) / f32(size);
        let v = (f32(y) + 0.5) / f32(size);
        // The share of all directions the texel covers.
        let s = u * 2.0 - 1.0;
        let t = v * 2.0 - 1.0;
        let solid_angle = pow(1.0 + s * s + t * t, -1.5);
        // The camera's picture is the face mirrored left to right.
        let texel = vec2<i32>(i32(size - 1u - x), i32(y));
        let drawn = textureLoad(faces, texel, i32(group.x * 6u + face), 0).rgb;
        let light = max(drawn, vec3<f32>(0.0)) * solid_angle;
        let d = face_direction(face, u, v);
        c0 += vec4<f32>(light * 0.282095, solid_angle);
        c1 += light * (0.488603 * d.y);
        c2 += light * (0.488603 * d.z);
        c3 += light * (0.488603 * d.x);
    }
    sums[thread] = array<vec4<f32>, 4>(c0, vec4<f32>(c1, 0.0), vec4<f32>(c2, 0.0), vec4<f32>(c3, 0.0));
    workgroupBarrier();
    for (var stride = THREADS / 2u; stride > 0u; stride = stride / 2u) {
        if thread < stride {
            for (var k = 0u; k < 4u; k++) {
                sums[thread][k] += sums[thread + stride][k];
            }
        }
        workgroupBarrier();
    }
    if thread == 0u {
        // Scaled so the texels cover all directions (4 pi) exactly, then
        // the cosine lobe's share of each band, over pi: 1, then 2/3.
        let scale = 4.0 * PI / max(sums[0][0].w, 1e-30);
        let first = 4u * targets[group.x].x;
        // Its w (1 for a probe baked inside a solid, R18.2) stays.
        light_probes[first] = vec4<f32>(sums[0][0].rgb * scale, light_probes[first].w);
        for (var k = 1u; k < 4u; k++) {
            // The second's w marks it traced now, so its sky light isn't
            // added again (lighting.wgsl, roadmap R18.3).
            light_probes[first + k] = vec4<f32>(sums[0][k].rgb * scale * (2.0 / 3.0), select(0.0, 1.0, k == 1u));
        }
    }
}
