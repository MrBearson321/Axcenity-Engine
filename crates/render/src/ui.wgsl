struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) border: vec2<f32>,
) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.color = color;
    output.uv = uv;
    output.size = size;
    output.border_color = border_color;
    output.border = border;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let half_size = input.size * 0.5;
    let radius = input.border.x;
    let point = abs(input.uv * input.size - half_size);
    let inner = half_size - vec2<f32>(radius);
    let delta = point - inner;
    let distance = length(max(delta, vec2<f32>(0.0))) + min(max(delta.x, delta.y), 0.0) - radius;
    if distance > 0.0 {
        discard;
    }
    if input.border.y > 0.0 && distance >= -input.border.y {
        return input.border_color;
    }
    return input.color;
}
