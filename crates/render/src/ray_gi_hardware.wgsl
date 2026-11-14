// Ray traced GI's rays (`gi_trace`, ray_gi.wgsl) through the hardware's
// ray traced scene (renderer/ray_tracing.rs), letting light through
// cut-outs' holes (LE7, ray_cut_outs.wgsl). Compiled after
// `ray_cut_outs.wgsl` and before `ray_gi.wgsl`, with
// `enable wgpu_ray_query;` put first (renderer/gpu.rs).

@group(1) @binding(0) var ray_scene: acceleration_structure;
@group(1) @binding(5) var<storage, read> cut_outs: array<CutOut>;
@group(1) @binding(6) var<storage, read> cut_out_uvs: array<vec2<f32>>;
@group(1) @binding(7) var cut_out_alpha: texture_2d_array<f32>;
@group(1) @binding(8) var cut_out_sampler: sampler;

fn gi_trace(origin: vec3<f32>, direction: vec3<f32>, t_max: f32, mask: u32, any_hit: bool) -> GiHit {
    let flags = select(0u, RAY_FLAG_TERMINATE_ON_FIRST_HIT, any_hit);
    let hit = trace_through_holes(RayDesc(flags, mask, 0.0, t_max, origin, direction));
    return GiHit(hit.kind != RAY_QUERY_INTERSECTION_NONE, hit.t, hit.instance_custom_data,
        hit.front_face);
}
