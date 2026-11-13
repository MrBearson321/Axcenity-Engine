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

// How much of the sun reaches cell `id`'s air: its shadow looked up at
// points spread through the whole cell and averaged, so a shaft's edge
// crossing a cell counts by how much of it is lit. One point in the middle
// made shafts step from cell to cell, and the steps crawled as the view
// moved.
fn cell_sun_visibility(id: vec3<u32>) -> f32 {
    if frame.shadow.x == 0.0 {
        return 1.0;
    }
    let near = slice_distance(f32(id.z));
    let far = slice_distance(f32(id.z) + 1.0);
    var lit = 0.0;
    for (var k = 0u; k < SUN_SAMPLES; k++) {
        let point = CELL_POINTS[k];
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
@compute @workgroup_size(4, 4, 4)
fn cs_scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id >= GRID) {
        return;
    }
    let fog = frame.fog;
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(GRID.xy);
    let dir = view_ray(uv);
    let middle = 0.5 * (slice_distance(f32(id.z)) + slice_distance(f32(id.z) + 1.0));
    let position = frame.camera_position.xyz + dir * middle;
    let air = dust_at(fog, position, dir);
    let fog_density = fog.height.x * fog_thickness(fog, position.y) * air.keep;
    let dust = air.density;
    let density = fog_density + dust;
    if density <= 0.0 {
        textureStore(scatter_out, id, vec4<f32>(0.0));
        return;
    }
    let sun = cell_sun_visibility(id) * cloud_shadow(frame.clouds, position, fog.sun_direction.xyz);
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
    textureStore(scatter_out, id, vec4<f32>(light, density));
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
