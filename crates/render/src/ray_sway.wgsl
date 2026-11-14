// Swaying plants in the ray traced scene (renderer/ray_sway.rs,
// docs/rendering/ray-tracing.md): each corner of a swaying item's mesh
// bent by the wind exactly as `vs_main` and `vs_shadow` bend it
// (sway.wgsl's `swayed_at`), back in the item's own space, for the item's
// own BLAS. Compiled after clouds.wgsl and sway.wgsl.

// Must match `BentRaw` in renderer/ray_sway.rs.
struct Bent {
    // The item's own space to the world, and back.
    model: mat4x4<f32>,
    to_object: mat4x4<f32>,
    // As `InstanceRaw::sway`.
    sway: vec4<f32>,
    // As `Frame::clock`: the time, then the wind.
    clock: vec4<f32>,
    // x: how many corners, y: how many f32s each corner of `corners` takes.
    counts: vec4<u32>,
}

@group(0) @binding(0) var<uniform> bent: Bent;
// The mesh's corners (`Vertex`s; the position comes first).
@group(0) @binding(1) var<storage, read> corners: array<f32>;
// The bent positions, three f32s each, for the BLAS.
@group(0) @binding(2) var<storage, read_write> positions: array<f32>;

// Corners a row of workgroups covers (65535 workgroups of 64).
const ROW: u32 = 4194240u;

@compute @workgroup_size(64)
fn cs_bend(@builtin(global_invocation_id) id: vec3<u32>) {
    let corner = id.x + id.y * ROW;
    if corner >= bent.counts.x {
        return;
    }
    let at = corner * bent.counts.y;
    let rest = vec3<f32>(corners[at], corners[at + 1u], corners[at + 2u]);
    let world = (bent.model * vec4<f32>(rest, 1.0)).xyz;
    let moved = (bent.to_object * vec4<f32>(swayed_at(world, bent.sway, bent.clock), 1.0)).xyz;
    positions[corner * 3u] = moved.x;
    positions[corner * 3u + 1u] = moved.y;
    positions[corner * 3u + 2u] = moved.z;
}
