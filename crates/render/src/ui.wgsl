struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border: vec2<f32>,
    @location(5) color_end: vec4<f32>,
    @location(6) border_widths: vec4<f32>,
    @location(7) border_right: vec4<f32>,
    @location(8) border_bottom: vec4<f32>,
    @location(9) border_left: vec4<f32>,
    @location(10) shadow: vec4<f32>,
}

@group(0) @binding(0) var ui_texture: texture_2d<f32>;
@group(0) @binding(1) var ui_sampler: sampler;

// What a quad draws (`border.y`); the same numbers as `UI_BOX`,
// `UI_SHADOW` and `UI_INSET_SHADOW` in `renderer/types.rs`.
const SHADOW: f32 = 1.0;
const INSET_SHADOW: f32 = 2.0;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) border: vec2<f32>,
    @location(6) color_end: vec4<f32>,
    @location(7) border_widths: vec4<f32>,
    @location(8) border_right: vec4<f32>,
    @location(9) border_bottom: vec4<f32>,
    @location(10) border_left: vec4<f32>,
    @location(11) shadow: vec4<f32>,
) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.color = color;
    output.uv = uv;
    output.size = size;
    output.border_color = border_color;
    output.border = border;
    output.color_end = color_end;
    output.border_widths = border_widths;
    output.border_right = border_right;
    output.border_bottom = border_bottom;
    output.border_left = border_left;
    output.shadow = shadow;
    return output;
}

// How far `point` is outside a rounded box around `centre` (negative inside).
fn box_distance(point: vec2<f32>, centre: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let inner = max(half_size - vec2<f32>(r), vec2<f32>(0.0));
    let delta = abs(point - centre) - inner;
    return length(max(delta, vec2<f32>(0.0))) + min(max(delta.x, delta.y), 0.0) - r;
}

fn rounded_distance(input: VertexOut) -> f32 {
    let half_size = input.size * 0.5;
    return box_distance(input.uv * input.size, half_size, half_size, input.border.x);
}

// The colour of a border pixel at `point`: the side it's nearest, measured
// in each side's own width, so corners split on the diagonal.
fn border_side(input: VertexOut, point: vec2<f32>) -> vec4<f32> {
    let widths = input.border_widths;
    let far = 1e9;
    let top = select(far, point.y / widths.x, widths.x > 0.0);
    let right = select(far, (input.size.x - point.x) / widths.y, widths.y > 0.0);
    let bottom = select(far, (input.size.y - point.y) / widths.z, widths.z > 0.0);
    let left = select(far, point.x / widths.w, widths.w > 0.0);
    let nearest = min(min(top, right), min(bottom, left));
    if nearest == top {
        return input.border_color;
    }
    if nearest == right {
        return input.border_right;
    }
    if nearest == bottom {
        return input.border_bottom;
    }
    return input.border_left;
}

// How much of a shadow fading over `blur` covers a point `distance` outside its edge.
fn shadow_cover(distance: f32, blur: f32) -> f32 {
    if blur <= 0.0 {
        return select(0.0, 1.0, distance <= 0.0);
    }
    return 1.0 - smoothstep(-blur * 0.5, blur * 0.5, distance);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let point = input.uv * input.size;
    let half_size = input.size * 0.5;
    let radius = input.border.x;
    if input.border.y == SHADOW {
        // The quad is the shadow's box grown by its blur on every side.
        let blur = input.shadow.z;
        let distance = box_distance(point, half_size, half_size - vec2<f32>(blur), radius);
        let cover = shadow_cover(distance, blur);
        if cover <= 0.0 {
            discard;
        }
        return vec4<f32>(input.color.rgb, input.color.a * cover);
    }
    let distance = box_distance(point, half_size, half_size, radius);
    if distance > 0.0 {
        discard;
    }
    if input.border.y == INSET_SHADOW {
        // Shadow everywhere in the box but a hole: the box moved and shrunk.
        let spread = input.shadow.w;
        let hole = box_distance(
            point,
            half_size + input.shadow.xy,
            max(half_size - vec2<f32>(spread), vec2<f32>(0.0)),
            max(radius - spread, 0.0),
        );
        let cover = 1.0 - shadow_cover(hole, input.shadow.z);
        if cover <= 0.0 {
            discard;
        }
        return vec4<f32>(input.color.rgb, input.color.a * cover);
    }
    let widths = input.border_widths;
    if max(max(widths.x, widths.y), max(widths.z, widths.w)) > 0.0 {
        // Inside the border is the box shrunk by each side's width, with
        // its corners rounded less by the widest side.
        let low = vec2<f32>(widths.w, widths.x);
        let high = input.size - vec2<f32>(widths.y, widths.z);
        let inner = box_distance(
            point,
            (low + high) * 0.5,
            max((high - low) * 0.5, vec2<f32>(0.0)),
            max(radius - max(max(widths.x, widths.y), max(widths.z, widths.w)), 0.0),
        );
        if inner > 0.0 || high.x <= low.x || high.y <= low.y {
            return border_side(input, point);
        }
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
