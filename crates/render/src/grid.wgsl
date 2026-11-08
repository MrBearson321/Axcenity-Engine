// The editor's measuring grid (`DrawList::grid`): thin lines on a flat
// rectangle, hidden by whatever stands in front of it. Lines keep their
// width on screen, and a set of lines fades out before it gets too dense
// to read.

// Must match `GridUniform` in renderer/grid.rs.
struct Grid {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    // The rectangle: smallest X and Z, then largest X and Z.
    bounds: vec4<f32>,
    // x: its height, y: studs between lines, z: lines per heavy line,
    // w: how far from the camera it has faded out (0 never).
    params: vec4<f32>,
    // Linear color of the lines.
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> grid: Grid;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) xz: vec2<f32>,
}

// The rectangle as a strip of two triangles.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let xz = mix(grid.bounds.xy, grid.bounds.zw, corner);
    var out: VertexOut;
    out.position = grid.view_proj * vec4<f32>(xz.x, grid.params.x, xz.y, 1.0);
    out.xz = xz;
    return out;
}

// How much of this pixel a line at each whole number of `at` covers, the
// line `width` pixels wide.
fn lines(at: vec2<f32>, change: vec2<f32>, width: f32) -> f32 {
    let distance = abs(fract(at - 0.5) - 0.5) / max(change, vec2<f32>(0.00001));
    return 1.0 - clamp(min(distance.x, distance.y) - width * 0.5 + 0.5, 0.0, 1.0);
}

// How much of this pixel the line through 0 covers, along one coordinate.
fn axis(at: f32, change: f32, width: f32) -> f32 {
    return 1.0 - clamp(abs(at) / max(change, 0.00001) - width * 0.5 + 0.5, 0.0, 1.0);
}

// Fully there while lines are at least about 8 pixels apart, gone by 3.
fn readable(cells_per_pixel: f32) -> f32 {
    return 1.0 - smoothstep(0.12, 0.33, cells_per_pixel);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let spacing = grid.params.y;
    let major = spacing * grid.params.z;
    let change = fwidth(in.xz);
    let dense = max(change.x, change.y);

    let minor_line = lines(in.xz / spacing, change / spacing, 1.0)
        * readable(dense / spacing) * 0.32;
    let major_line = lines(in.xz / major, change / major, 1.6)
        * (1.0 - smoothstep(0.2, 0.5, dense / major)) * 0.7;
    var color = grid.color.rgb;
    var alpha = max(minor_line, major_line);

    // The world's X axis in red and its Z axis in blue, as the handles are.
    let along_x = axis(in.xz.y, change.y, 2.2);
    let along_z = axis(in.xz.x, change.x, 2.2);
    if along_x > 0.0 {
        color = mix(color, vec3<f32>(0.8, 0.07, 0.07), along_x);
        alpha = max(alpha, along_x * 0.9);
    }
    if along_z > 0.0 {
        color = mix(color, vec3<f32>(0.04, 0.2, 0.9), along_z);
        alpha = max(alpha, along_z * 0.9);
    }

    if grid.params.w > 0.0 {
        let distance = length(in.xz - grid.camera.xz);
        alpha *= 1.0 - smoothstep(grid.params.w * 0.6, grid.params.w, distance);
    }
    if alpha <= 0.002 {
        discard;
    }
    return vec4<f32>(color * alpha, alpha);
}
