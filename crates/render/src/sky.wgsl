// The sky's colors by direction, shared by the scene's shaders (before
// `lighting.wgsl`) and the environment bake (before `environment.wgsl`).
// No bindings: each caller passes in its own colors and textures.
// Colors are linear light. Keep `gradient_sky` and `equirect_uv` matching
// `renderer/environment.rs`, which works out the sky's diffuse light on the CPU.

const PI: f32 = 3.14159265;

// The procedural sky looking along `dir`: `ground` below the horizon, then
// a gradient from `horizon` up to `top`.
fn gradient_sky(dir: vec3<f32>, top: vec3<f32>, horizon: vec3<f32>, ground: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    let sky = mix(horizon, top, pow(up, 0.45));
    return mix(sky, ground, smoothstep(0.0, 0.2, -dir.y));
}

// Where `dir` is in a sky image wrapped around the world: u once around
// (0.5 toward -Z, 0 and 1 toward +Z), v from straight up (0) to straight
// down (1).
fn equirect_uv(dir: vec3<f32>) -> vec2<f32> {
    let u = atan2(dir.x, -dir.z) / (2.0 * PI) + 0.5;
    let v = acos(clamp(dir.y, -1.0, 1.0)) / PI;
    return vec2<f32>(u, v);
}
