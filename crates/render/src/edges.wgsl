// Screen-space outlines (Lighting's `OutlineEffect`): lines where the
// depth or the facing of what's drawn changes sharply, found in the
// prepass's depth and view-space normals. Drawn in the scene pass over the
// solid scene and the sky, before see-through items, blended by how strong
// each pixel's edge is.
//
// Each pixel walks out along eight directions, up to the lines' thickness,
// comparing every step with the one before it, so thick lines stay as
// thick along every edge and round its corners, and a surface's gradual
// slope or curve is never taken for an edge.

// Must match `EdgeUniform` in renderer/types.rs.
struct Edges {
    inv_projection: mat4x4<f32>,
    // The lines' linear color.
    color: vec4<f32>,
    // x: how thick the lines are, in pixels.
    settings: vec4<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
@group(0) @binding(1) var normal_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> edges: Edges;

// How far the next step must fall behind the surface's line (as a share of
// its nearness) to be an edge, and how far two steps' facings must differ
// (1 - the cosine between them): about 45 degrees. Each is blended in over
// the same again.
const DEPTH_EDGE: f32 = 0.05;
const NORMAL_EDGE: f32 = 0.3;
// The most steps along each direction, however thick the lines: thicker
// lines take longer steps, skipping slivers narrower than one.
const MAX_STEPS: f32 = 12.0;

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
    let thickness = max(edges.settings.x, 1.0);
    let step = i32(ceil(thickness / MAX_STEPS));
    let normal = facing(pixel);
    let directions = array<vec2<i32>, 8>(
        vec2<i32>(1, 0), vec2<i32>(1, 1), vec2<i32>(0, 1), vec2<i32>(-1, 1),
        vec2<i32>(-1, 0), vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
    );
    var edge = 0.0;
    for (var i = 0; i < 8; i++) {
        let offset = directions[i] * step;
        let stride = length(vec2<f32>(offset));
        // The surface's slope this way, from whichever neighbor continues
        // it more evenly, so an edge right behind this pixel doesn't hide
        // one right ahead of it.
        let behind = center - nearness(pixel - offset);
        let ahead = nearness(pixel + offset) - center;
        var slope = select(ahead, behind, abs(behind) < abs(ahead));
        var here = center;
        var here_normal = normal;
        var here_pixel = pixel;
        for (var k = 0; ; k++) {
            // How much of this pixel a line from an edge after this step
            // covers: 1 well inside the thickness, fading over its last
            // pixel.
            let coverage = min(thickness + 0.5 - (f32(k) + 0.5) * stride, 1.0);
            if coverage <= 0.0 {
                break;
            }
            let next_pixel = here_pixel + offset;
            let next = nearness(next_pixel);
            // How far the next step strays from the surface's line: 0 on a
            // flat surface, below 0 past its edge (or against the sky).
            let bend = next - here - slope;
            if bend < -DEPTH_EDGE * here {
                edge = max(edge, coverage * smoothstep(DEPTH_EDGE, 2.0 * DEPTH_EDGE, -bend / here));
                break;
            }
            // Something nearer: the line is on it, not here.
            if bend > DEPTH_EDGE * here {
                break;
            }
            let next_normal = facing(next_pixel);
            let crease = 1.0 - dot(here_normal, next_normal);
            if crease > NORMAL_EDGE {
                // A crease's line is on one side only: the one facing the
                // camera more or, about even, the one facing up less (the
                // part, not the floor it stands on), so it never spills
                // off a part's edge onto the floor.
                let toward = normal.z - next_normal.z;
                if toward > 0.01 || (toward > -0.01 && normal.y <= next_normal.y) {
                    edge = max(edge, coverage * smoothstep(NORMAL_EDGE, 2.0 * NORMAL_EDGE, crease));
                }
                break;
            }
            slope = next - here;
            here = next;
            here_normal = next_normal;
            here_pixel = next_pixel;
        }
    }
    if edge <= 0.0 {
        discard;
    }
    return vec4<f32>(edges.color.rgb, edge);
}
