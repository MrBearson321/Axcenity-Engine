// Screen-space outlines (Lighting's `OutlineEffect`): lines where the
// depth or the facing of what's drawn changes sharply, found in the
// prepass's depth and view-space normals. Drawn in the scene pass over the
// solid scene and the sky, before see-through items, blended by how strong
// each pixel's edge is.

// Must match `EdgeUniform` in renderer/types.rs.
struct Edges {
    inv_projection: mat4x4<f32>,
    // The lines' linear color.
    color: vec4<f32>,
    // x: how far each pixel looks for an edge, in pixels.
    settings: vec4<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
@group(0) @binding(1) var normal_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> edges: Edges;

// How far the nearer side of a step in depth must stand out from the
// surface around it (as a share of its nearness) to be an edge, and how
// far two neighbors' facings must differ (1 - the cosine between them):
// about 45 degrees. Each is blended in over the same again.
const DEPTH_EDGE: f32 = 0.05;
const NORMAL_EDGE: f32 = 0.3;

// One triangle that covers the whole target.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

// 1 over how far in front of the camera the prepass's surface at `pixel`
// is, or 0 where nothing solid was drawn (the sky). It changes evenly
// across any flat surface on screen, so a step in it is an edge.
fn nearness(pixel: vec2<i32>) -> f32 {
    let last = vec2<i32>(textureDimensions(depth_texture)) - 1;
    let depth = textureLoad(depth_texture, clamp(pixel, vec2<i32>(0), last), 0);
    if depth >= 1.0 {
        return 0.0;
    }
    let view = edges.inv_projection * vec4<f32>(0.0, 0.0, depth, 1.0);
    return max(-view.w / view.z, 0.0);
}

fn facing(pixel: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(normal_texture)) - 1;
    return textureLoad(normal_texture, clamp(pixel, vec2<i32>(0), last), 0).xyz;
}

@fragment
fn fs_edges(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let center = nearness(pixel);
    // Lines are drawn on the nearer side of an edge only, never on the sky.
    if center <= 0.0 {
        discard;
    }
    let reach = i32(max(round(edges.settings.x), 1.0));
    let n = facing(pixel);
    var depth_edge = 0.0;
    var normal_edge = 0.0;
    let axes = array<vec2<i32>, 2>(vec2<i32>(reach, 0), vec2<i32>(0, reach));
    for (var i = 0; i < 2; i++) {
        let before = nearness(pixel - axes[i]);
        let after = nearness(pixel + axes[i]);
        // How much nearer this pixel is than the line through its
        // neighbors: 0 on a flat surface, large on the near side of a step
        // (or against the sky), below 0 on the far side.
        depth_edge = max(depth_edge, (2.0 * center - before - after) / center);
        // A crease: the neighbor after it, at about the same depth, faces
        // another way. One side only, so the line is as thick as asked.
        if abs(after - center) < 0.1 * center {
            normal_edge = max(normal_edge, 1.0 - dot(n, facing(pixel + axes[i])));
        }
    }
    let edge = max(smoothstep(DEPTH_EDGE, 2.0 * DEPTH_EDGE, depth_edge),
        smoothstep(NORMAL_EDGE, 2.0 * NORMAL_EDGE, normal_edge));
    if edge <= 0.0 {
        discard;
    }
    return vec4<f32>(edges.color.rgb, edge);
}
