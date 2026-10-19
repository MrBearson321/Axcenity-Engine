struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border: vec2<f32>,
    @location(5) color_end: vec4<f32>,
}

@group(0) @binding(0) var ui_texture: texture_2d<f32>;
@group(0) @binding(1) var ui_sampler: sampler;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) border: vec2<f32>,
    @location(6) color_end: vec4<f32>,
) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.color = color;
    output.uv = uv;
    output.size = size;
    output.border_color = border_color;
    output.border = border;
    output.color_end = color_end;
    return output;
}

fn rounded_distance(input: VertexOut) -> f32 {
    let half_size = input.size * 0.5;
    let radius = input.border.x;
    let point = abs(input.uv * input.size - half_size);
    let inner = half_size - vec2<f32>(radius);
    let delta = point - inner;
    return length(max(delta, vec2<f32>(0.0))) + min(max(delta.x, delta.y), 0.0) - radius;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let distance = rounded_distance(input);
    if distance > 0.0 {
        discard;
    }
    if input.border.y > 0.0 && distance >= -input.border.y {
        return input.border_color;
    }
    return mix(input.color, input.color_end, input.uv.y);
}

@fragment
fn fs_image(input: VertexOut) -> @location(0) vec4<f32> {
    let distance = rounded_distance(input);
    if distance > 0.0 {
        discard;
    }
    return textureSample(ui_texture, ui_sampler, input.uv) * input.color;
}
