// Volumetric fog (graphics Phase 25): the sun's light scattered by height
// fog, with its shadows (sun shafts), and the lamps', in a grid of cells
// over the view (froxels: so many across and down the screen, and slices
// along each ray, finer near the camera). Compiled after `sky.wgsl`,
// `fog.wgsl` and `lighting.wgsl`, for the frame's bindings and shadows.
//
// 1. `cs_scatter` works out each cell's fog and the light it scatters
//    toward the camera, at points that shift through the cell each frame,
//    and blends it with what last frame's grid had there (its history).
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
// Last frame's scattered light (`cs_scatter`'s output), and where it was.
@group(1) @binding(6) var history: texture_3d<f32>;
@group(1) @binding(7) var history_sampler: sampler;
@group(1) @binding(8) var<uniform> last: FogHistory;

// Must match `FogHistoryRaw` in renderer/fog.rs.
struct FogHistory {
    // Last frame's camera, world to clip space (as its `frame.view_proj`).
    view_proj: mat4x4<f32>,
    // xyz: last frame's camera, w: how much of last frame's light each
    // cell keeps (0: none, as on the first frame).
    camera: vec4<f32>,
}

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

// The lamps' light at `position` scattered toward the camera looking along
// `dir` (at `uv` on screen), before the fog's own share of it: each lamp
// listed for the cell's tile of the screen, where its shadows allow.
fn lamp_air_light(position: vec3<f32>, dir: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    var light = vec3<f32>(0.0);
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
        // The same falloff, cone and rectangle as on surfaces.
        let reach = lamp_reach(lamp, position);
        let strength = reach.w;
        let l = reach.xyz / max(length(reach.xyz), 0.0001);
        let to_light = lamp.position_range.xyz - position;
        if strength > 0.0 {
            light += lamp.radiance_cone.rgb * strength * fog_phase(dir, l, LAMP_FORWARD)
                * lamp_visibility(lamp, position, l, to_light, unturned);
        }
    }
    return light;
}

// How many points of each cell the sun's shadow is looked up at.
const SUN_SAMPLES: u32 = 8u;

// Points spread evenly through a cell (0 to 1 each way): the first of the
// Halton (2, 3, 5) sequence. Every cell uses the same ones: shifting them
// at random per cell left a speckle of cell-sized blotches, which TAA's
// clamp can't average away (it would flicker if they changed per frame).
const CELL_POINTS = array<vec3<f32>, 8>(
    vec3<f32>(0.5, 0.333333, 0.2),
    vec3<f32>(0.25, 0.666667, 0.4),
    vec3<f32>(0.75, 0.111111, 0.6),
    vec3<f32>(0.125, 0.444444, 0.8),
    vec3<f32>(0.625, 0.777778, 0.04),
    vec3<f32>(0.375, 0.222222, 0.24),
    vec3<f32>(0.875, 0.555556, 0.44),
    vec3<f32>(0.0625, 0.888889, 0.64),
);

// The radical inverse of `n` in `base`: the Halton sequence's `n`th.
fn radical_inverse(n: u32, base: u32) -> f32 {
    var i = n;
    var scale = 1.0 / f32(base);
    var value = 0.0;
    while i > 0u {
        value += f32(i % base) * scale;
        i /= base;
        scale /= f32(base);
    }
    return value;
}

// How far (0 to 1 each way, wrapping) cell `id`'s points are shifted on
// turn `turn` (`frame.fog.dust_count.z`): the Halton (2, 3, 5) sequence's
// `turn`th point, plus a shift of the cell's own, so neighbouring cells
// don't sample alike. Over the frames the history keeps, each cell is
// looked at all through, not at the same few points: fine shadows (leaves
// over a window) averaged over 4 or 8 fixed points made cell-sized blocks.
// Turn 0 shifts nothing. A copy is in ray_shadows.wgsl (`air_shift`).
fn cell_shift(id: vec3<u32>, turn: u32) -> vec3<f32> {
    if turn == 0u {
        return vec3<f32>(0.0);
    }
    var h = id * vec3<u32>(1664525u, 1013904223u, 2654435761u) + vec3<u32>(id.z, id.x, id.y);
    h ^= h >> vec3<u32>(16u);
    h *= vec3<u32>(0x7feb352du);
    h ^= h >> vec3<u32>(15u);
    let own = vec3<f32>(h >> vec3<u32>(8u)) / 16777216.0;
    let step = vec3<f32>(radical_inverse(turn, 2u), radical_inverse(turn, 3u), radical_inverse(turn, 5u));
    return fract(own + step);
}

// How much of the sun reaches cell `id`'s air: its shadow looked up at
// points spread through the whole cell (shifted by `shift`) and averaged,
// so a shaft's edge crossing a cell counts by how much of it is lit. One
// point in the middle made shafts step from cell to cell, and the steps
// crawled as the view moved.
fn cell_sun_visibility(id: vec3<u32>, shift: vec3<f32>) -> f32 {
    // Traced by rays (LE3), fading into the cascades toward the reach.
    let traced = traced_air(id);
    let near = slice_distance(f32(id.z));
    if traced >= 0.0 {
        let reach = frame.ray_shadows.z;
        let weight = 1.0 - smoothstep(reach * 0.85, reach, near);
        if weight >= 1.0 {
            return traced;
        }
        return mix(cell_cascade_sun(id, shift), traced, weight);
    }
    return cell_cascade_sun(id, shift);
}

// How much sunlight reaches cell `id`'s air by ray_shadows.wgsl's
// `cs_air`, or -1 where it isn't traced (ray traced shadows off, or past
// their reach).
fn traced_air(id: vec3<u32>) -> f32 {
    let start = frame.fog.dust_count.y;
    if start == 0u {
        return -1.0;
    }
    let word = ray_shadow_mask[start + id.x + GRID.x * (id.y + GRID.y * id.z)];
    if word == 0xffffffffu {
        return -1.0;
    }
    return f32(word) / 65535.0;
}

// The same from the sun's shadow maps.
fn cell_cascade_sun(id: vec3<u32>, shift: vec3<f32>) -> f32 {
    if frame.shadow.x == 0.0 {
        return 1.0;
    }
    let near = slice_distance(f32(id.z));
    let far = slice_distance(f32(id.z) + 1.0);
    var lit = 0.0;
    for (var k = 0u; k < SUN_SAMPLES; k++) {
        let point = fract(CELL_POINTS[k] + shift);
        let uv = (vec2<f32>(id.xy) + point.xy) / vec2<f32>(GRID.xy);
        let position = frame.camera_position.xyz + view_ray(uv) * mix(near, far, point.z);
        lit += air_sun_visibility(position);
    }
    return lit / f32(SUN_SAMPLES);
}

// The light arriving at `position` from all around, averaged over every
// way (for light scattered alike every way): what light probe volumes, or
// ray traced GI's probes, found there, or the sky's where there are none.
// The probes hold first-order harmonics, whose average over every way is
// exactly the mean of two opposite ones.
fn surrounding_light(position: vec3<f32>) -> vec3<f32> {
    let environment = frame.sky_top.w;
    let up = vec3<f32>(0.0, 1.0, 0.0);
    return 0.5 * (surrounding_diffuse(position, up, environment)
        + surrounding_diffuse(position, -up, environment));
}

// How open the air is to the sky where the light around it is `around`:
// that light over the open sky's (1 in the open, or with no light probes
// to say; toward 0 in a closed room). Height fog's own color is the open
// air's.
fn openness(around: vec3<f32>) -> f32 {
    let environment = frame.sky_top.w;
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let open = environment * 0.5 * (sky_diffuse(up) + sky_diffuse(-up));
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let sky = dot(open, luma);
    if sky <= 1e-5 {
        return 1.0;
    }
    return clamp(dot(around, luma) / sky, 0.0, 2.0);
}

// 1. Each cell's fog and dust (a: the light they take away per stud) and
// the light they scatter toward the camera per stud (rgb): the sun's where
// it isn't shadowed, the lamps', and the light around (height fog's own
// color by how open the air is, and for dust the light probes' light).
// Worked out at this frame's point in the cell, then blended with what
// last frame's grid had where the cell's middle was (`with_history`).
@compute @workgroup_size(4, 4, 4)
fn cs_scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id >= GRID) {
        return;
    }
    textureStore(scatter_out, id, with_history(id, cell_scatter(id)));
}

// What last frame's grid had where cell `id`'s middle is, blended into
// this frame's `now` by `last.camera.w`; `now` alone where last frame's
// grid didn't reach (newly in view) or on the first frame.
fn with_history(id: vec3<u32>, now: vec4<f32>) -> vec4<f32> {
    let keep = last.camera.w;
    if keep <= 0.0 {
        return now;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(GRID.xy);
    let position = frame.camera_position.xyz + view_ray(uv) * slice_distance(f32(id.z) + 0.5);
    let clip = last.view_proj * vec4<f32>(position, 1.0);
    if clip.w <= 0.0 {
        return now;
    }
    let last_uv = vec2<f32>(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5);
    let w = distance_slice(length(position - last.camera.xyz)) / f32(GRID.z);
    if any(last_uv < vec2<f32>(0.0)) || any(last_uv > vec2<f32>(1.0)) || w > 1.0 {
        return now;
    }
    let before = textureSampleLevel(history, history_sampler, vec3<f32>(last_uv, w), 0.0);
    return mix(now, before, keep);
}

// Cell `id`'s fog and light this frame, as `cs_scatter` says.
fn cell_scatter(id: vec3<u32>) -> vec4<f32> {
    let fog = frame.fog;
    // This frame's point in the cell: its middle, shifted.
    let shift = cell_shift(id, fog.dust_count.z);
    let point = fract(vec3<f32>(0.5) + shift);
    let uv = (vec2<f32>(id.xy) + point.xy) / vec2<f32>(GRID.xy);
    let dir = view_ray(uv);
    let along = mix(slice_distance(f32(id.z)), slice_distance(f32(id.z) + 1.0), point.z);
    let position = frame.camera_position.xyz + dir * along;
    let air = dust_at(fog, position, dir);
    let fog_density = fog.height.x * fog_thickness(fog, position.y) * air.keep;
    let dust = air.density;
    let density = fog_density + dust;
    if density <= 0.0 {
        return vec4<f32>(0.0);
    }
    let sun = cell_sun_visibility(id, shift) * cloud_shadow(frame.clouds, position, fog.sun_direction.xyz);
    let lamps = lamp_air_light(position, dir, uv);
    var light = fog_density * (fog.sun.rgb * fog_phase(dir, fog.sun_direction.xyz, fog.sun.w) * sun
        + fog.distance.z * lamps);
    let own = fog.distance.w > 0.5;
    if own || dust > 0.0 {
        let around = surrounding_light(position);
        if own {
            light += fog_density * fog.color.rgb * openness(around);
        }
        light += air.scatter * (around + lamps);
    }
    light += air.sun * sun;
    return vec4<f32>(light, density);
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
    return vec4<f32>(smooth_gathered(uv, w) * min(s, 1.0), 0.0);
}

// The gathered light at `uv` across the screen and `w` along the rays:
// across, a cubic B-spline over the 4 × 4 cells around (4 filtered
// looks), along, straight between slices. Straight across too, each cell
// showed as a soft diamond, and a shaft's edge as a row of them.
fn smooth_gathered(uv: vec2<f32>, w: f32) -> vec3<f32> {
    let size = vec2<f32>(GRID.xy);
    let p = uv * size - 0.5;
    let f = fract(p);
    let corner = floor(p);
    let f2 = f * f;
    let f3 = f2 * f;
    let w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    let w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    let w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    let w3 = f3 / 6.0;
    // Each pair of cells' weights, and where one filtered look between
    // them gives their weighted sum.
    let low = w0 + w1;
    let high = w2 + w3;
    let at_low = (corner - 0.5 + w1 / low) / size;
    let at_high = (corner + 1.5 + w3 / high) / size;
    return low.y * (low.x * gathered_at(vec2<f32>(at_low.x, at_low.y), w)
            + high.x * gathered_at(vec2<f32>(at_high.x, at_low.y), w))
        + high.y * (low.x * gathered_at(vec2<f32>(at_low.x, at_high.y), w)
            + high.x * gathered_at(vec2<f32>(at_high.x, at_high.y), w));
}

fn gathered_at(uv: vec2<f32>, w: f32) -> vec3<f32> {
    return textureSampleLevel(gathered, gathered_sampler, vec3<f32>(uv, w), 0.0).rgb;
}
