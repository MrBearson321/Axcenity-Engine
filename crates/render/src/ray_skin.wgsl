// Posed (skinned) characters in the ray traced scene (renderer/ray_sway.rs,
// docs/rendering/ray-tracing.md): each corner of a posed item's mesh moved
// by its joints exactly as `pose_corner` in shader.wgsl moves it, in the
// item's own space, for the item's own BLAS.

// Must match `PoseRaw` in renderer/ray_sway.rs.
struct Pose {
    // x: how many corners, y: how many f32s each corner of `corners`
    // takes, z: how many joints the skin has.
    counts: vec4<u32>,
}

// One corner's joints and how much each counts. Must match `SkinCorner` in
// renderer/skinning.rs (and shader.wgsl's).
struct SkinCorner {
    joints: vec4<u32>,
    weights: vec4<f32>,
}

@group(0) @binding(0) var<uniform> pose: Pose;
// The mesh's corners (`Vertex`s; the position comes first).
@group(0) @binding(1) var<storage, read> corners: array<f32>;
// The mesh's corners' joints and weights.
@group(0) @binding(2) var<storage, read> skin_corners: array<SkinCorner>;
// The item's joints this frame, from the mesh's place to the posed place.
@group(0) @binding(3) var<storage, read> joints: array<mat4x4<f32>>;
// The posed positions, three f32s each, for the BLAS.
@group(0) @binding(4) var<storage, read_write> positions: array<f32>;

// Corners a row of workgroups covers (65535 workgroups of 64).
const ROW: u32 = 4194240u;

@compute @workgroup_size(64)
fn cs_pose(@builtin(global_invocation_id) id: vec3<u32>) {
    let corner = id.x + id.y * ROW;
    if corner >= pose.counts.x {
        return;
    }
    let at = corner * pose.counts.y;
    let rest = vec3<f32>(corners[at], corners[at + 1u], corners[at + 2u]);
    let skin = skin_corners[corner];
    var matrix = mat4x4<f32>();
    var total = 0.0;
    for (var i = 0u; i < 4u; i += 1u) {
        let weight = skin.weights[i];
        let joint = skin.joints[i];
        if weight <= 0.0 || joint >= pose.counts.z {
            continue;
        }
        matrix += joints[joint] * weight;
        total += weight;
    }
    var moved = rest;
    if total > 1e-6 {
        moved = (matrix * vec4<f32>(rest, 1.0)).xyz / total;
    }
    positions[corner * 3u] = moved.x;
    positions[corner * 3u + 1u] = moved.y;
    positions[corner * 3u + 2u] = moved.z;
}
