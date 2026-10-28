// Volumetric fog (graphics Phase 25): the sun's light scattered by height
// fog, with its shadows (sun shafts), and the lamps', in a grid of cells
// over the view (froxels: so many across and down the screen, and slices
// along each ray, finer near the camera). Compiled after `sky.wgsl`,
// `fog.wgsl` and `lighting.wgsl`, for the frame's bindings and shadows.
//
// 1. `cs_scatter` works out each cell's fog and the light it scatters
//    toward the camera.
// 2. `cs_gather` adds the cells up along each ray, front to back, each
//    dimmed by the fog in front of it.
// 3. `fs_composite` adds what was gathered up to each pixel's solid
//    surface (the prepass's depth) to the scene.
//
// Everything else of the fog (its own color, light lost on the way, and
// the sun's light past the grid's reach) is drawn by the surfaces' own
// shaders (`fogged`), which leave the sun's out within the reach.

// The grid's cells across, down and along. Must match `GRID` in renderer/fog.rs.
const GRID: vec3<u32> = vec3<u32>(160u, 90u, 64u);
// Slices are about this many studs deep near the camera, growing deeper
// further out.
const NEAR_SLICE: f32 = 2.0;
// How much of a lamp's light goes forward, as `Fog.sun.w` is the sun's.
const LAMP_FORWARD: f32 = 0.3;

@group(1) @binding(0) var scatter_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(1) var scatter_in: texture_3d<f32>;
@group(1) @binding(2) var gathered_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(3) var gathered: texture_3d<f32>;
@group(1) @binding(4) var gathered_sampler: sampler;
// Solid items' depth, from the prepass.
@group(1) @binding(5) var scene_depth: texture_depth_2d;

// How far along its ray slice `s` ends (0 at the camera, the reach at the
// last).
fn slice_distance(s: f32) -> f32 {
    let reach = frame.fog.height.w;
    return NEAR_SLICE * (pow(1.0 + reach / NEAR_SLICE, s / f32(GRID.z)) - 1.0);
}

// The slice, with its fraction, that ends `distance` along its ray.
fn distance_slice(distance: f32) -> f32 {
    let reach = frame.fog.height.w;
    return f32(GRID.z) * log(1.0 + distance / NEAR_SLICE) / log(1.0 + reach / NEAR_SLICE);
}

// The way from the camera through the screen at `uv` (0 to 1, from the
// top left).
fn view_ray(uv: vec2<f32>) -> vec3<f32> {
    let far = frame.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
    return normalize(far.xyz / far.w - frame.camera_position.xyz);
}

// How much sunlight reaches `position` in the air: one filtered comparison
// in the nearest cascade covering it. Air has no surface to soften a
// shadow on, so the sun's own shadows' blur isn't needed.
fn air_sun_visibility(position: vec3<f32>) -> f32 {
    if frame.shadow.x == 0.0 {
        return 1.0;
    }
    let count = u32(frame.shadow.z);
    for (var i = 0u; i < count; i++) {
        let light = frame.cascades[i] * vec4<f32>(position, 1.0);
        let uv = vec2<f32>(light.x * 0.5 + 0.5, 0.5 - light.y * 0.5);
        if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || light.z > 1.0 || light.z < 0.0 {
            continue;
        }
        let bias = 2.0 * frame.cascade_texels[i] / frame.cascade_depths[i];
        return textureSampleCompareLevel(shadow_map, shadow_sampler, uv, i32(i), light.z - bias);
    }
    return 1.0;
}

// The light scattered toward the camera at `position` (looking along
// `dir`, at `uv` on screen) by the fog there, per stud, as a share of the
// fog: the sun's where it isn't shadowed, and each lamp listed for the
// cell's tile of the screen, where its shadows allow.
fn air_light(position: vec3<f32>, dir: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let fog = frame.fog;
    var light = fog.sun.rgb * fog_phase(dir, fog.sun_direction.xyz, fog.sun.w)
        * air_sun_visibility(position);
    if frame.lights.x == 0u {
        return light;
    }
    let pixel = uv / frame.ambient_occlusion.zw;
    let tile = vec2<u32>(pixel) / max(frame.lights.y, 1u);
    let first = (tile.y * frame.lights.z + tile.x) * (LIGHTS_PER_TILE + 1u);
    let count = min(tile_lights[first], LIGHTS_PER_TILE);
    let unturned = mat2x2<f32>(1.0, 0.0, 0.0, 1.0);
    for (var i = 0u; i < count; i++) {
        let lamp = lights[tile_lights[first + 1u + i]];
        let to_light = lamp.position_range.xyz - position;
        let distance_squared = dot(to_light, to_light);
        let range = lamp.position_range.w;
        if distance_squared >= range * range {
            continue;
        }
        let dist = sqrt(distance_squared);
        let l = to_light / max(dist, 0.0001);
        // The same falloff and cone as on surfaces (`light_surface`).
        let edge = clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0);
        var strength = edge * edge / (distance_squared + 1.0);
        if lamp.radiance_cone.w > -1.5 {
            strength *= smoothstep(lamp.radiance_cone.w, lamp.direction_inner.w,
                dot(-l, lamp.direction_inner.xyz));
        }
        if strength > 0.0 {
            light += lamp.radiance_cone.rgb * strength * fog.distance.z
                * fog_phase(dir, l, LAMP_FORWARD)
                * lamp_visibility(lamp, position, l, to_light, unturned);
        }
    }
    return light;
}

// 1. Each cell's fog (a) and the light it scatters toward the camera per
// stud (rgb), at its middle.
@compute @workgroup_size(4, 4, 4)
fn cs_scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id >= GRID) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(GRID.xy);
    let dir = view_ray(uv);
    let middle = 0.5 * (slice_distance(f32(id.z)) + slice_distance(f32(id.z) + 1.0));
    let position = frame.camera_position.xyz + dir * middle;
    let density = frame.fog.height.x * fog_thickness(frame.fog, position.y);
    textureStore(scatter_out, id, vec4<f32>(air_light(position, dir, uv) * density, density));
}

// 2. Along each ray, front to back: the light gathered up to the end of
// each slice (rgb) and how much gets through that far (a).
@compute @workgroup_size(8, 8, 1)
fn cs_gather(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= GRID.xy) {
        return;
    }
    var light = vec3<f32>(0.0);
    var through = 1.0;
    var start = 0.0;
    for (var z = 0u; z < GRID.z; z++) {
        let end = slice_distance(f32(z + 1u));
        let depth = end - start;
        let cell = vec3<u32>(id.xy, z);
        let scatter = textureLoad(scatter_in, cell, 0);
        let lost = exp(-scatter.a * depth);
        // What the slice scatters, less what the slice itself takes back
        // of it on the way out (Hillaire 2015's energy-conserving step).
        let own = select(scatter.rgb * depth, scatter.rgb * (1.0 - lost) / scatter.a, scatter.a > 1e-6);
        light += through * own;
        through *= lost;
        textureStore(gathered_out, cell, vec4<f32>(light, through));
        start = end;
    }
}

struct CompositeOut {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_composite(@builtin(vertex_index) index: u32) -> CompositeOut {
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: CompositeOut;
    out.position = vec4<f32>(xy, 0.0, 1.0);
    return out;
}

// 3. The light gathered up to the pixel's solid surface, or all of it in
// front of the sky, added to the scene.
@fragment
fn fs_composite(in: CompositeOut) -> @location(0) vec4<f32> {
    let uv = in.position.xy * frame.ambient_occlusion.zw;
    let depth = textureLoad(scene_depth, vec2<i32>(in.position.xy), 0);
    var distance = frame.fog.height.w;
    if depth < 1.0 {
        let world = frame.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
        distance = min(length(world.xyz / world.w - frame.camera_position.xyz), distance);
    }
    // Slice `s` ends at `s + 1`, where its texel's middle is; nearer than
    // the first slice's end is a share of it.
    let s = distance_slice(distance);
    let w = clamp((s - 0.5) / f32(GRID.z), 0.5 / f32(GRID.z), 1.0 - 0.5 / f32(GRID.z));
    let light = textureSampleLevel(gathered, gathered_sampler, vec3<f32>(uv, w), 0.0).rgb;
    return vec4<f32>(light * min(s, 1.0), 0.0);
}
