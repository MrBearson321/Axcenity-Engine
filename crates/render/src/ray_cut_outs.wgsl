// Cut-outs' holes for rays (renderer/ray_cut_outs.rs,
// docs/rendering/ray-tracing.md), shared by ray traced ambient occlusion
// and GI (LE7): the shader including it binds `cut_outs`, `cut_out_uvs`,
// `cut_out_alpha` and `cut_out_sampler` (`ray_cut_outs::layout_entries`),
// and its ray traced scene as `ray_scene`.
// ray_shadows.wgsl keeps its own copy of this: keep them matching.

// Must match `CutOutRaw`.
struct CutOut {
    // x: its mesh's first triangle in `cut_out_uvs`, y: its alpha's layer
    // plus 1 (0: no holes), z: the alpha cutoff's bits.
    mesh: vec4<u32>,
    // The columns of the material's texture coordinate matrix.
    uv_matrix: vec4<f32>,
    // xy: the material's texture coordinate offset.
    uv_offset: vec4<f32>,
}

// Whether a ray's candidate hit on a cut-out's triangle lands where its
// texture isn't cut out: its texture coordinates there (the triangle's
// three, by where it was hit, through the material's own scale, turn and
// offset), and the alpha at them against the cutoff.
fn cut_out_covers(hit: RayIntersection) -> bool {
    let cut = cut_outs[hit.instance_custom_data];
    if cut.mesh.y == 0u {
        return true;
    }
    let corner = (cut.mesh.x + hit.primitive_index) * 3u;
    let b = hit.barycentrics;
    let uv = cut_out_uvs[corner] * (1.0 - b.x - b.y) + cut_out_uvs[corner + 1u] * b.x
        + cut_out_uvs[corner + 2u] * b.y;
    let placed = mat2x2<f32>(cut.uv_matrix.xy, cut.uv_matrix.zw) * uv + cut.uv_offset.xy;
    let alpha = textureSampleLevel(cut_out_alpha, cut_out_sampler, placed, i32(cut.mesh.y - 1u), 0.0).r;
    return alpha >= bitcast<f32>(cut.mesh.z);
}

// The hit a ray `desc` through `ray_scene` commits to, confirming
// cut-outs' candidate hits only where they aren't cut out (solid items'
// hits are committed as they come). A ray query can't be passed to a
// function, so the whole query runs here.
fn trace_through_holes(desc: RayDesc) -> RayIntersection {
    var query: ray_query;
    rayQueryInitialize(&query, ray_scene, desc);
    while rayQueryProceed(&query) {
        let candidate = rayQueryGetCandidateIntersection(&query);
        if candidate.kind == RAY_QUERY_INTERSECTION_TRIANGLE && cut_out_covers(candidate) {
            rayQueryConfirmIntersection(&query);
        }
    }
    return rayQueryGetCommittedIntersection(&query);
}
