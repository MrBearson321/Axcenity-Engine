// Camera-facing sprites, such as particles. See renderer/sprites.rs.

struct Sprites {
    view_proj: mat4x4<f32>,
    // The camera's right and up directions, one stud long.
    right: vec4<f32>,
    up: vec4<f32>,
};

@group(0) @binding(0) var<uniform> sprites: Sprites;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct SpriteIn {
    // The center, and the size in studs.
    @location(0) center_size: vec4<f32>,
    // Linear color, already times brightness and opacity, and the opacity.
    @location(1) color: vec4<f32>,
    // The turn in radians, the light emission.
    @location(2) params: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) emission: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32, sprite: SpriteIn) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
        vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, 0.5),
    );
    let corner = corners[index];
    let c = cos(sprite.params.x);
    let s = sin(sprite.params.x);
    let turned = vec2<f32>(corner.x * c - corner.y * s, corner.x * s + corner.y * c)
        * sprite.center_size.w;
    let world = sprite.center_size.xyz
        + sprites.right.xyz * turned.x
        + sprites.up.xyz * turned.y;
    var out: VertexOut;
    out.clip = sprites.view_proj * vec4<f32>(world, 1.0);
    // (0, 0) is the texture's top left.
    out.uv = vec2<f32>(corner.x + 0.5, 0.5 - corner.y);
    out.color = sprite.color;
    out.emission = sprite.params.y;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(sprite_texture, sprite_sampler, in.uv);
    // Premultiplied: the color already carries the sprite's opacity.
    let rgb = texel.rgb * in.color.rgb * texel.a;
    let alpha = texel.a * in.color.a;
    return vec4<f32>(rgb, alpha * (1.0 - in.emission));
}
