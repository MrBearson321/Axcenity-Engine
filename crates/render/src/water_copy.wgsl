// Copies the solid scene for water (renderer/water.rs): each pixel's light
// in rgb, and in a how far what it shows is from the camera, in studs, so
// water can see through itself (water.wgsl's `scene_copy`). Drawn between
// the solid items and the see-through ones. With MSAA, `scene_depth` is
// multisampled and its first sample is read.

// Must match `CopyView` in renderer/water.rs.
struct CopyView {
    // Clip space back to world space.
    inv_view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
}

// How far the sky is taken to be, in studs: about the most a half float
// holds, and far past the camera's far plane.
const SCENE_COPY_SKY: f32 = 65000.0;

@group(0) @binding(0) var<uniform> copy_view: CopyView;
@group(0) @binding(1) var scene_color: texture_2d<f32>;
@group(0) @binding(2) var scene_depth: texture_depth_2d;

@vertex
fn vs_copy(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    return vec4<f32>(xy, 0.0, 1.0);
}

@fragment
fn fs_copy(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let color = textureLoad(scene_color, pixel, 0);
    let depth = textureLoad(scene_depth, pixel, 0);
    var far = SCENE_COPY_SKY;
    if depth < 1.0 {
        let size = vec2<f32>(textureDimensions(scene_color));
        let ndc = vec2<f32>(position.x / size.x * 2.0 - 1.0, 1.0 - position.y / size.y * 2.0);
        let world = copy_view.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
        far = min(length(world.xyz / world.w - copy_view.camera_position.xyz), SCENE_COPY_SKY);
    }
    return vec4<f32>(color.rgb, far);
}
