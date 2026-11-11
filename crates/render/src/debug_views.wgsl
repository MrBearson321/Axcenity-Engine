// The editor's debug views (renderer/debug_views.rs): every item drawn
// again as one of its inputs, unlit, as a wireframe, as overdraw or by its
// level of detail. Compiled after shader.wgsl and surface.wgsl, so items
// are posed, swayed and textured as the scene pass does it, and read their
// material with `standard_surface` (a material's own surface shader is
// left out).
//
// The corners are read from the mesh's buffers here ("vertex pulling"),
// not through a vertex layout: drawn without its index buffer, each corner
// knows which corner of its triangle it is, which gives the wireframe its
// edges without line drawing, which isn't on every backend.

// Must match `DebugUniform` in renderer/debug_views.rs: one per batch.
struct Debug {
    // The camera's view, without TAA's jitter.
    view_proj: mat4x4<f32>,
    // x: the view (DEBUG_* below), y: 1 if the indices are 16-bit, z: the
    // batch's level of detail (-1: its mesh has none), w: unused.
    settings: vec4<f32>,
    // xy: the scene's size over the output's, to read ambient occlusion
    // (drawn at the scene's size) at an output pixel.
    scale: vec4<f32>,
}

// The mesh's corners as `Vertex` in mesh.rs lays them out (12 floats each),
// and its indices. Bindings 0 and 1 of this group are shader.wgsl's motion
// vectors', so these start at 2.
@group(3) @binding(2) var<storage, read> debug_corners: array<f32>;
@group(3) @binding(3) var<storage, read> debug_indices: array<u32>;
@group(3) @binding(4) var<uniform> debug: Debug;

// Must match `DebugView` in renderer/debug_views.rs.
const DEBUG_UNLIT: f32 = 0.0;
const DEBUG_WIREFRAME: f32 = 1.0;
const DEBUG_NORMALS: f32 = 2.0;
const DEBUG_BASE_COLOR: f32 = 3.0;
const DEBUG_METALNESS: f32 = 4.0;
const DEBUG_ROUGHNESS: f32 = 5.0;
const DEBUG_OCCLUSION: f32 = 6.0;
const DEBUG_OVERDRAW: f32 = 7.0;
const DEBUG_LOD: f32 = 8.0;

// Each layer drawn over a pixel adds this much light in the overdraw view.
const OVERDRAW_STEP: vec3<f32> = vec3<f32>(0.12, 0.05, 0.02); // overdraw.rs reads it back

struct DebugOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) surface: vec3<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) tangent: vec4<f32>,
    @location(7) @interpolate(flat) fade: f32,
    // 1 at this corner of the triangle, 0 at the other two.
    @location(8) corner: vec3<f32>,
}

// The `at`th index of the mesh.
fn debug_index(at: u32) -> u32 {
    if debug.settings.y > 0.5 {
        let pair = debug_indices[at / 2u];
        return select(pair & 0xffffu, pair >> 16u, (at & 1u) == 1u);
    }
    return debug_indices[at];
}

@vertex
fn vs_debug(@builtin(vertex_index) at: u32, instance: InstanceIn) -> DebugOut {
    let index = debug_index(at);
    let c = index * 12u;
    var vertex: VertexIn;
    vertex.position = vec3<f32>(debug_corners[c], debug_corners[c + 1u], debug_corners[c + 2u]);
    vertex.normal = vec3<f32>(debug_corners[c + 3u], debug_corners[c + 4u],
        debug_corners[c + 5u]);
    vertex.uv = vec2<f32>(debug_corners[c + 6u], debug_corners[c + 7u]);
    vertex.tangent = vec4<f32>(debug_corners[c + 8u], debug_corners[c + 9u],
        debug_corners[c + 10u], debug_corners[c + 11u]);
    vertex.index = index;
    let item = item_vertex(vertex, instance);
    var out: DebugOut;
    out.clip_position = debug.view_proj * vec4<f32>(item.world_position, 1.0);
    out.world_position = item.world_position;
    out.normal = item.normal;
    out.base_color = item.base_color;
    out.emissive = item.emissive;
    out.surface = item.surface;
    out.uv = item.uv;
    out.tangent = item.tangent;
    out.fade = item.fade;
    let which = at % 3u;
    out.corner = vec3<f32>(f32(which == 0u), f32(which == 1u), f32(which == 2u));
    return out;
}

// A value from 0 to 1 shown as grey, so the pixel's byte is the value
// times 255 (the target turns linear light to sRGB).
fn shown(value: vec3<f32>) -> vec3<f32> {
    return srgb(clamp(value, vec3<f32>(0.0), vec3<f32>(1.0)));
}

// Each level of detail's color: green for the item's own mesh, through
// yellow and orange to red for the simplest.
fn lod_color(level: f32) -> vec3<f32> {
    if level < 0.0 {
        return vec3<f32>(0.5);
    }
    let colors = array<vec3<f32>, 4>(
        vec3<f32>(0.2, 0.8, 0.25),
        vec3<f32>(0.95, 0.85, 0.2),
        vec3<f32>(0.95, 0.5, 0.15),
        vec3<f32>(0.9, 0.2, 0.2),
    );
    return colors[min(u32(level), 3u)];
}

@fragment
fn fs_debug(in: DebugOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Every texture is sampled, and the wireframe's edges measured, before
    // anything branches, as WGSL requires.
    let facing = select(-1.0, 1.0, front);
    var input: SurfaceInput;
    input.uv = in.uv;
    input.world_position = in.world_position;
    input.normal = normalize(in.normal) * facing;
    input.tangent = in.tangent * facing;
    input.time = frame.clock.x;
    input.color = in.base_color.rgb;
    input.opacity = in.base_color.a;
    input.metalness = in.surface.x;
    input.roughness = in.surface.y;
    input.emissive = in.emissive;
    let shaded = standard_surface(input);
    let occlusion_texel = textureSample(occlusion_texture, material_sampler, in.uv);
    // How far the pixel is from its triangle's nearest edge, in pixels.
    let edge_distance = in.corner / max(fwidth(in.corner), vec3<f32>(1e-6));
    let edge = min(edge_distance.x, min(edge_distance.y, edge_distance.z));

    if shaded.opacity < in.surface.z || dithered_out(in.fade, in.clip_position.xy) {
        discard;
    }
    let mode = debug.settings.x;
    // Lit from the camera, so a shape's sides can be told apart.
    let toward = normalize(frame.camera_position.xyz - in.world_position);
    let headlight = 0.35 + 0.65 * max(dot(shaded.normal, toward), 0.0);
    var color: vec3<f32>;
    if mode == DEBUG_UNLIT {
        color = shaded.base_color + shaded.emissive;
    } else if mode == DEBUG_WIREFRAME {
        let line = 1.0 - clamp(edge - 0.5, 0.0, 1.0);
        color = mix(vec3<f32>(0.03) * headlight, vec3<f32>(0.85, 0.9, 1.0), line);
    } else if mode == DEBUG_NORMALS {
        color = shown(shaded.normal * 0.5 + 0.5);
    } else if mode == DEBUG_BASE_COLOR {
        color = shaded.base_color;
    } else if mode == DEBUG_METALNESS {
        color = shown(vec3<f32>(shaded.metalness));
    } else if mode == DEBUG_ROUGHNESS {
        color = shown(vec3<f32>(shaded.roughness));
    } else if mode == DEBUG_OCCLUSION {
        // The map's, and for solid items the screen-space ambient occlusion's.
        let solid = in.base_color.a >= 1.0;
        let mapped = mix(1.0, map_value(occlusion_texel, maps.occlusion, maps.invert.z),
            maps.settings.z);
        let screen = select(1.0, screen_occlusion(in.clip_position.xy * debug.scale.xy), solid);
        color = shown(vec3<f32>(mapped * screen));
    } else if mode == DEBUG_OVERDRAW {
        color = OVERDRAW_STEP;
    } else {
        color = lod_color(debug.settings.z) * headlight;
    }
    return vec4<f32>(color, 1.0);
}
