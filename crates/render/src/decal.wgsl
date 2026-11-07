// Projected decals (renderer/decals.rs): each is a box that lays its picture
// onto the solid surfaces inside it. Compiled after `lighting.wgsl`, whose
// frame bindings (group 0) it reads, `prepass_depth` among them.
//
// The box's back faces are drawn, with no depth test, so it covers its
// pixels even with the camera inside it. Each pixel finds the solid surface
// the prepass drew there, keeps it only if it's inside the box, and paints
// the picture over it, lit like any surface. Since the surface's own depth
// is used, the decal follows any shape with no gaps and never fights it.

// The prepass's view-space normals; w is 1, or 0.5 for a character's pixel
// (`fs_prepass` in shader.wgsl), and 0 where nothing solid was drawn.
@group(1) @binding(0) var decal_normals: texture_2d<f32>;
@group(2) @binding(0) var decal_texture: texture_2d<f32>;
@group(2) @binding(1) var decal_sampler: sampler;

// How rough a decal's surface is: paint and grime, matte.
const DECAL_ROUGHNESS: f32 = 0.8;
// The most of the picture one pixel may span when picking its mip level.
// Where the surface behind a pixel jumps (an edge), the picture's
// coordinates jump too, and would otherwise blur that pixel to its average.
const MAX_GRADIENT: f32 = 0.25;

// One decal. Must match `DecalRaw` in renderer/decals.rs.
struct DecalIn {
    @location(0) model_0: vec4<f32>,
    @location(1) model_1: vec4<f32>,
    @location(2) model_2: vec4<f32>,
    @location(3) model_3: vec4<f32>,
    @location(4) inverse_0: vec4<f32>,
    @location(5) inverse_1: vec4<f32>,
    @location(6) inverse_2: vec4<f32>,
    @location(7) inverse_3: vec4<f32>,
    // Linear color, then opacity.
    @location(8) color: vec4<f32>,
    // x, y: the cosines of the turn away from the picture's facing at which
    // it's gone and at which it starts fading; z: 1 to leave characters
    // bare.
    @location(9) params: vec4<f32>,
}

struct DecalOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) inverse_0: vec4<f32>,
    @location(1) @interpolate(flat) inverse_1: vec4<f32>,
    @location(2) @interpolate(flat) inverse_2: vec4<f32>,
    @location(3) @interpolate(flat) inverse_3: vec4<f32>,
    @location(4) @interpolate(flat) color: vec4<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
    // The way the picture faces (the box's -Z side), in world space.
    @location(6) @interpolate(flat) facing: vec3<f32>,
}

// The unit box's corners (bit 0: x, bit 1: y, bit 2: z), and its faces as
// triangles counter-clockwise seen from outside.
var<private> BOX_TRIANGLES: array<u32, 36> = array<u32, 36>(
    0u, 4u, 6u, 0u, 6u, 2u, // -X
    5u, 1u, 3u, 5u, 3u, 7u, // +X
    0u, 1u, 5u, 0u, 5u, 4u, // -Y
    3u, 2u, 6u, 3u, 6u, 7u, // +Y
    1u, 0u, 2u, 1u, 2u, 3u, // -Z
    4u, 5u, 7u, 4u, 7u, 6u, // +Z
);

@vertex
fn vs_decal(@builtin(vertex_index) index: u32, decal: DecalIn) -> DecalOut {
    let corner = BOX_TRIANGLES[index];
    let local = vec3<f32>(
        f32(corner & 1u),
        f32((corner >> 1u) & 1u),
        f32((corner >> 2u) & 1u),
    ) - vec3<f32>(0.5);
    let model = mat4x4<f32>(decal.model_0, decal.model_1, decal.model_2, decal.model_3);
    var out: DecalOut;
    out.clip_position = frame.view_proj * model * vec4<f32>(local, 1.0);
    out.inverse_0 = decal.inverse_0;
    out.inverse_1 = decal.inverse_1;
    out.inverse_2 = decal.inverse_2;
    out.inverse_3 = decal.inverse_3;
    out.color = decal.color;
    out.params = decal.params;
    out.facing = -normalize(decal.model_2.xyz);
    return out;
}

fn limited(gradient: vec2<f32>) -> vec2<f32> {
    let length_squared = dot(gradient, gradient);
    if length_squared > MAX_GRADIENT * MAX_GRADIENT {
        return gradient * (MAX_GRADIENT * inverseSqrt(length_squared));
    }
    return gradient;
}

@fragment
fn fs_decal(in: DecalOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.clip_position.xy);
    let depth = textureLoad(prepass_depth, pixel, 0);
    // Where the prepass's surface is, in the world and in the box (whose
    // inside runs from -0.5 to 0.5 each way).
    let size = vec2<f32>(textureDimensions(prepass_depth));
    let ndc = vec2<f32>(in.clip_position.x / size.x * 2.0 - 1.0,
        1.0 - in.clip_position.y / size.y * 2.0);
    let world4 = frame.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    let world = world4.xyz / world4.w;
    let inverse = mat4x4<f32>(in.inverse_0, in.inverse_1, in.inverse_2, in.inverse_3);
    let local = (inverse * vec4<f32>(world, 1.0)).xyz;
    // The picture's top is +Y, and seen from the front (looking along +Z)
    // its right is -X.
    let uv = vec2<f32>(0.5 - local.x, 0.5 - local.y);
    // Worked out before anything is discarded, while every pixel is here.
    let ddx = limited(dpdx(uv));
    let ddy = limited(dpdy(uv));
    if depth >= 1.0 || any(abs(local) > vec3<f32>(0.5)) {
        discard;
    }
    let stored = textureLoad(decal_normals, pixel, 0);
    if stored.w < 0.25 || (in.params.z > 0.5 && stored.w < 0.75) {
        discard;
    }
    // The prepass's normal is in view space: turn it back to the world's.
    let view = mat3x3<f32>(frame.view[0].xyz, frame.view[1].xyz, frame.view[2].xyz);
    let n = normalize(transpose(view) * stored.xyz);
    let fade = smoothstep(in.params.x, in.params.y, dot(n, in.facing));
    let picture = textureSampleGrad(decal_texture, decal_sampler, uv, ddx, ddy);
    let alpha = picture.a * in.color.a * fade;
    if alpha <= 1.0 / 512.0 {
        discard;
    }
    let base_color = picture.rgb * in.color.rgb;
    let lit = light_surface(world, n, n, in.clip_position.xy, base_color, 0.0, DECAL_ROUGHNESS,
        screen_occlusion(in.clip_position.xy), vec3<f32>(0.04), true);
    let seen = fogged(frame.fog, frame.camera_position.xyz, world, lit);
    // Premultiplied, so the blend paints it over the surface by `alpha`.
    return vec4<f32>(seen * alpha, alpha);
}
