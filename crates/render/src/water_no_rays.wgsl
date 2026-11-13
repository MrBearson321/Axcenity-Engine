// Water drawn without ray tracing (renderer/water.rs): nothing is traced,
// so reflections are the screen's and the sky's alone. `water_rays.wgsl`
// is the same function, traced.
fn traced_reflection(origin: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(0.0);
}
