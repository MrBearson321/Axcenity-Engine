// Shows the Path Traced view's sums (path_tracer.wgsl, renderer/path_tracer.rs):
// each output pixel the average of its traced pixel's paths so far, through
// the classic tone mapping curve (post.wgsl's `tone_map_classic`; the view
// is drawn after the frame's own tone mapping).

// Must match `PathTraceUniform` in renderer/path_tracer.rs.
struct PathTrace {
    inv_view_proj: mat4x4<f32>,
    counts: vec4<u32>,
    settings: vec4<f32>,
}

@group(0) @binding(0) var<uniform> pt: PathTrace;
@group(0) @binding(1) var<storage, read> pt_sum: array<vec4<f32>>;

@vertex
fn vs_path_traced(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the screen.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn tone_map_classic(color: vec3<f32>) -> vec3<f32> {
    let knee = 0.6;
    let over = max(color - knee, vec3<f32>(0.0));
    let rolled = knee + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
    return select(rolled, color, color <= vec3<f32>(knee));
}

@fragment
fn fs_path_traced(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = pt.counts.xy;
    let traced = min(vec2<u32>(position.xy / pt.settings.x), size - vec2<u32>(1u));
    let sum = pt_sum[traced.y * size.x + traced.x];
    let color = sum.rgb / max(sum.w, 1.0);
    return vec4<f32>(tone_map_classic(color), 1.0);
}
