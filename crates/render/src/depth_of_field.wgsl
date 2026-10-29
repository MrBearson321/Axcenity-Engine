// Depth of field (a DepthOfFieldEffect): the scene blurred by how far each
// pixel's surface lies outside the in-focus range, judged from the
// prepass's depth. Drawn after the scene pass, before bloom, into a target
// that bloom and the composite pass read in the scene's place.
//
// Each pixel averages the pixels under points on a disc as wide as its own
// blur (a spiral, evenly spread). A pixel counts only as much as its own
// blur reaches back to this one, so a sharp surface never smears into the
// blurred one beside it. Color and depth are read from the same whole
// pixel, never blended with a neighbor.

// Must match `DepthOfFieldUniform` in renderer/lens.rs.
struct DepthOfField {
    inv_projection: mat4x4<f32>,
    // x: focus distance, y: in-focus radius (studs), z: near intensity,
    // w: far intensity.
    focus: vec4<f32>,
    // x: the widest blur in pixels, y: samples per pixel.
    settings: vec4<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var<uniform> dof: DepthOfField;

// The angle between one point of the spiral and the next (the golden angle).
const GOLDEN_ANGLE: f32 = 2.39996323;

// One triangle that covers the whole target.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

// How far in front of the camera the prepass's surface at `pixel` is, in
// studs, or very far where nothing solid was drawn (the sky).
fn distance_at(pixel: vec2<i32>) -> f32 {
    let depth = textureLoad(depth_texture, pixel, 0);
    if depth >= 1.0 {
        return 1e9;
    }
    let view = dof.inv_projection * vec4<f32>(0.0, 0.0, depth, 1.0);
    return max(-view.z / view.w, 0.0);
}

// How blurred a surface `distance` studs away is: the radius of its blur in
// pixels. Nearer than the in-focus range, it grows to the near intensity at
// the camera; farther, to the far intensity at twice the range's far edge.
fn blur_at(distance: f32) -> f32 {
    let near_edge = dof.focus.x - dof.focus.y;
    let far_edge = dof.focus.x + dof.focus.y;
    var amount = 0.0;
    if distance < near_edge {
        amount = dof.focus.z * saturate((near_edge - distance) / max(near_edge, 0.001));
    } else if distance > far_edge {
        amount = dof.focus.w * saturate((distance - far_edge) / max(far_edge, 1.0));
    }
    return amount * dof.settings.x;
}

@fragment
fn fs_depth_of_field(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let last = vec2<i32>(textureDimensions(scene)) - 1;
    let center = textureLoad(scene, pixel, 0).rgb;
    let reach = blur_at(distance_at(pixel));
    if reach < 0.5 {
        return vec4<f32>(center, 1.0);
    }
    let samples = u32(dof.settings.y);
    var sum = center;
    var weight = 1.0;
    for (var i = 0u; i < samples; i += 1u) {
        let along = sqrt((f32(i) + 0.5) / f32(samples)) * reach;
        let angle = f32(i) * GOLDEN_ANGLE;
        let at = position.xy + vec2<f32>(cos(angle), sin(angle)) * along;
        let theirs = clamp(vec2<i32>(floor(at)), vec2<i32>(0), last);
        let counts = saturate(blur_at(distance_at(theirs)) - along + 1.0);
        sum += textureLoad(scene, theirs, 0).rgb * counts;
        weight += counts;
    }
    return vec4<f32>(sum / weight, 1.0);
}
