// Ray traced GI's rays (`gi_trace`, ray_gi.wgsl) traced in software (RL3,
// ray_soft.wgsl, renderer/ray_soft.rs) on a GPU without ray queries.
// Compiled after `ray_soft.wgsl` and before `ray_gi.wgsl`
// (renderer/gpu.rs).

@group(1) @binding(0) var<storage, read> soft_scene: array<vec4<f32>>;

fn gi_trace(origin: vec3<f32>, direction: vec3<f32>, t_max: f32, mask: u32, any_hit: bool) -> GiHit {
    let hit = soft_trace(origin, direction, t_max, mask, any_hit);
    return GiHit(hit.found, hit.t, hit.slot, hit.front_face);
}
