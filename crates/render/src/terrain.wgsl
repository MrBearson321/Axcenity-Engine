// Terrain: up to eight material layers blended across each chunk by two
// weight ("splat") textures, then lit like everything else. Compiled after
// `lighting.wgsl`.
//
// Each layer's textures are one slice of three texture arrays: base color,
// normal map, and surface (R roughness, G metalness, B ambient occlusion,
// A height). Layers are placed in the world, not on the mesh, so they tile
// evenly however the ground is shaped: from above, or (triplanar) from
// above and both sides, so cliffs aren't stretched.

// One layer's settings. Must match `TerrainLayerRaw` in renderer/terrain.rs.
struct TerrainLayer {
    // Linear tint, and studs one copy of the textures covers.
    tint_tile: vec4<f32>,
    // Metalness, roughness, normal map strength (0: none), 1 or -1 to flip green.
    surface: vec4<f32>,
    // Turn in radians, 1 for triplanar, anti-tiling (0 to 1), height blend (0 to 1).
    options: vec4<f32>,
    // The layer's color in the Weights view.
    debug: vec4<f32>,
}

// Must match `TerrainUniform` in renderer/terrain.rs.
struct Terrain {
    layers: array<TerrainLayer, 8>,
    // x: how many layers, y: the view (0 shaded, 1 height, 2 slope,
    // 3 weights, 4 grid), z and w: the world Y of heights 0 and 1.
    info: vec4<f32>,
    // xy: the world X and Z of the grid's corner, zw: studs between samples.
    grid: vec4<f32>,
}

@group(1) @binding(0) var splat_low: texture_2d<f32>;
@group(1) @binding(1) var splat_high: texture_2d<f32>;
@group(1) @binding(2) var splat_sampler: sampler;
@group(1) @binding(3) var layer_sampler: sampler;
@group(1) @binding(4) var albedo_layers: texture_2d_array<f32>;
@group(1) @binding(5) var normal_layers: texture_2d_array<f32>;
@group(1) @binding(6) var surface_layers: texture_2d_array<f32>;
@group(1) @binding(7) var<uniform> terrain: Terrain;

struct TerrainIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(12) uv: vec2<f32>,
}

// Per chunk. The same layout as the items' `InstanceIn`; only the model
// matrix is used.
struct ChunkIn {
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
}

struct TerrainOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vs_terrain(vertex: TerrainIn, chunk: ChunkIn) -> TerrainOut {
    let model = mat4x4<f32>(chunk.model_0, chunk.model_1, chunk.model_2, chunk.model_3);
    let world_position = model * vec4<f32>(vertex.position, 1.0);
    var out: TerrainOut;
    out.clip_position = frame.view_proj * world_position;
    out.world_position = world_position.xyz;
    // Chunks are only ever moved, never turned or stretched.
    out.normal = vertex.normal;
    out.uv = vertex.uv;
    return out;
}

// `p` turned by `angle` radians.
fn turn(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(p.x * c + p.y * s, -p.x * s + p.y * c);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Smooth value noise from 0 to 1, one bump per unit.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y);
}

// One layer's textures at one projection.
struct Taps {
    albedo: vec3<f32>,
    // The normal map's direction in the texture's own frame (z up), scaled
    // by the layer's strength.
    normal: vec3<f32>,
    surface: vec4<f32>,
}

// Samples layer `index` at `uv`. Sampled with explicit derivatives (the
// rate `uv` changes across the screen), so it can be inside branches.
fn taps(index: u32, uv: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>, anti_tiling: f32) -> Taps {
    let layer = terrain.layers[index];
    let slice = i32(index);
    var out: Taps;
    var albedo = textureSampleGrad(albedo_layers, layer_sampler, uv, slice, ddx, ddy).rgb;
    if anti_tiling > 0.0 {
        // A second, larger and turned copy mixed in patches, so the
        // repeats don't line up.
        let far_uv = turn(uv, 1.7) * 0.31 + vec2<f32>(0.37, 0.71);
        let far = textureSampleGrad(albedo_layers, layer_sampler, far_uv, slice,
            turn(ddx, 1.7) * 0.31, turn(ddy, 1.7) * 0.31).rgb;
        albedo = mix(albedo, far, anti_tiling);
    }
    out.albedo = albedo;
    let packed = textureSampleGrad(normal_layers, layer_sampler, uv, slice, ddx, ddy).xyz * 2.0 - 1.0;
    let strength = layer.surface.z;
    out.normal = normalize(vec3<f32>(
        packed.x * strength, packed.y * layer.surface.w * strength, max(packed.z, 0.001)));
    out.surface = textureSampleGrad(surface_layers, layer_sampler, uv, slice, ddx, ddy);
    return out;
}

struct LayerSample {
    albedo: vec3<f32>,
    // The shading normal in world space.
    normal: vec3<f32>,
    surface: vec4<f32>,
}

// Layer `index` at `world`, on ground facing `n`. `dpx` and `dpy` are how
// `world` changes across the screen.
fn sample_layer(
    index: u32, world: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>, n: vec3<f32>, mixing: f32,
) -> LayerSample {
    let layer = terrain.layers[index];
    let tile = max(layer.tint_tile.w, 0.01);
    let angle = layer.options.x;
    let anti = clamp(layer.options.z * mixing, 0.0, 1.0);
    var out: LayerSample;
    // From above: U along the turned X, the texture's up along the turned -Z.
    let along = vec3<f32>(cos(angle), 0.0, sin(angle));
    let up = vec3<f32>(sin(angle), 0.0, -cos(angle));
    let top = taps(index, turn(world.xz, angle) / tile,
        turn(dpx.xz, angle) / tile, turn(dpy.xz, angle) / tile, anti);
    if layer.options.y < 0.5 {
        // Planar: the normal map tips the surface's own frame.
        let t = normalize(along - n * dot(n, along));
        let b = cross(n, t);
        out.albedo = top.albedo;
        out.normal = normalize(t * top.normal.x + b * top.normal.y + n * top.normal.z);
        out.surface = top.surface;
        return out;
    }
    // Triplanar: from above and from each side, by how much the ground faces each.
    var blend = pow(abs(n), vec3<f32>(4.0));
    blend = blend / max(blend.x + blend.y + blend.z, 0.0001);
    let side_x = taps(index, vec2<f32>(world.z, -world.y) / tile,
        vec2<f32>(dpx.z, -dpx.y) / tile, vec2<f32>(dpy.z, -dpy.y) / tile, 0.0);
    let side_z = taps(index, vec2<f32>(world.x, -world.y) / tile,
        vec2<f32>(dpx.x, -dpx.y) / tile, vec2<f32>(dpy.x, -dpy.y) / tile, 0.0);
    out.albedo = side_x.albedo * blend.x + top.albedo * blend.y + side_z.albedo * blend.z;
    out.surface = side_x.surface * blend.x + top.surface * blend.y + side_z.surface * blend.z;
    // Each projection tips the normal within its own plane (a "UDN" blend).
    let tipped_x = n + vec3<f32>(0.0, 0.0, 1.0) * side_x.normal.x + vec3<f32>(0.0, 1.0, 0.0) * side_x.normal.y;
    let tipped_y = n + along * top.normal.x + up * top.normal.y;
    let tipped_z = n + vec3<f32>(1.0, 0.0, 0.0) * side_z.normal.x + vec3<f32>(0.0, 1.0, 0.0) * side_z.normal.y;
    out.normal = normalize(tipped_x * blend.x + tipped_y * blend.y + tipped_z * blend.z);
    return out;
}

// Blue low, through green and yellow, to white high.
fn height_ramp(t: f32) -> vec3<f32> {
    let low = mix(vec3<f32>(0.05, 0.15, 0.6), vec3<f32>(0.15, 0.6, 0.25), smoothstep(0.0, 0.35, t));
    let mid = mix(low, vec3<f32>(0.85, 0.75, 0.25), smoothstep(0.35, 0.7, t));
    return mix(mid, vec3<f32>(0.97, 0.97, 0.97), smoothstep(0.7, 1.0, t));
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4<f32> {
    // Everything that needs the neighbouring pixels comes first, before
    // anything branches, as WGSL requires.
    let low = textureSample(splat_low, splat_sampler, in.uv);
    let high = textureSample(splat_high, splat_sampler, in.uv);
    let dpx = dpdx(in.world_position);
    let dpy = dpdy(in.world_position);
    let grid = (in.world_position.xz - terrain.grid.xy) / max(terrain.grid.zw, vec2<f32>(0.001));
    let grid_width = fwidth(grid);

    let n = normalize(in.normal);
    let count = u32(terrain.info.x);
    let view = u32(terrain.info.y);
    var weights = array<f32, 8>(low.r, low.g, low.b, low.a, high.r, high.g, high.b, high.a);
    let distance_to_camera = distance(frame.camera_position.xyz, in.world_position);
    // Far away, more of the large copy shows, so repeats fade out with distance.
    let mixing = value_noise(in.world_position.xz / 50.0) * 0.8
        + smoothstep(60.0, 400.0, distance_to_camera) * 0.5;

    var base_color = vec3<f32>(0.5);
    var shading_normal = n;
    var roughness = 0.9;
    var metalness = 0.0;
    var occlusion = 1.0;
    if count > 0u {
        var samples: array<LayerSample, 8>;
        var shares = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        var sharpness = 0.0;
        for (var i = 0u; i < count; i++) {
            if weights[i] > 0.003 {
                samples[i] = sample_layer(i, in.world_position, dpx, dpy, n, mixing);
                // Where layers meet, the one standing higher (by its height
                // map) shows through: stones poking out of sand.
                let height_blend = terrain.layers[i].options.w;
                shares[i] = weights[i] * exp2(height_blend * 8.0 * (samples[i].surface.a - 0.5));
                sharpness = max(sharpness, height_blend);
            }
        }
        var total = 0.0;
        for (var i = 0u; i < count; i++) {
            shares[i] = pow(shares[i], 1.0 + 3.0 * sharpness);
            total += shares[i];
        }
        if total <= 0.0 {
            samples[0] = sample_layer(0u, in.world_position, dpx, dpy, n, mixing);
            shares[0] = 1.0;
            total = 1.0;
        }
        base_color = vec3<f32>(0.0);
        var blended_normal = vec3<f32>(0.0);
        roughness = 0.0;
        metalness = 0.0;
        occlusion = 0.0;
        for (var i = 0u; i < count; i++) {
            let share = shares[i] / total;
            if share <= 0.0 {
                continue;
            }
            let layer = terrain.layers[i];
            let s = samples[i];
            base_color += share * s.albedo * layer.tint_tile.rgb;
            blended_normal += share * s.normal;
            metalness += share * s.surface.g * layer.surface.x;
            roughness += share * s.surface.r * layer.surface.y;
            occlusion += share * s.surface.b;
        }
        if dot(blended_normal, blended_normal) > 1e-8 {
            shading_normal = normalize(blended_normal);
        }
        if view == 3u {
            var debug = vec3<f32>(0.0);
            for (var i = 0u; i < count; i++) {
                debug += weights[i] * terrain.layers[i].debug.rgb;
            }
            base_color = debug;
            shading_normal = n;
        }
    }
    if view == 1u {
        let range = max(terrain.info.w - terrain.info.z, 0.001);
        base_color = height_ramp(clamp((in.world_position.y - terrain.info.z) / range, 0.0, 1.0));
        shading_normal = n;
        roughness = 0.9;
        metalness = 0.0;
    } else if view == 2u {
        let slope = acos(clamp(n.y, -1.0, 1.0)) / 1.5707963;
        base_color = mix(vec3<f32>(0.2, 0.65, 0.25),
            mix(vec3<f32>(0.9, 0.8, 0.2), vec3<f32>(0.85, 0.2, 0.15), smoothstep(0.4, 0.7, slope)),
            smoothstep(0.15, 0.4, slope));
        shading_normal = n;
        roughness = 0.9;
        metalness = 0.0;
    }

    var color = light_surface(in.world_position, n, shading_normal, in.clip_position.xy,
        base_color, clamp(metalness, 0.0, 1.0), clamp(roughness, 0.04, 1.0),
        clamp(occlusion, 0.0, 1.0) * screen_occlusion(in.clip_position.xy), vec3<f32>(0.04), true);
    if view == 4u {
        // Lines along the grid of samples, the same width on screen.
        let to_line = abs(fract(grid - 0.5) - 0.5) / max(grid_width, vec2<f32>(0.0001));
        let line = 1.0 - clamp(min(to_line.x, to_line.y), 0.0, 1.0);
        color = mix(color, vec3<f32>(0.02, 0.02, 0.03), line * 0.6);
    }
    return vec4<f32>(color, 1.0);
}

// The depth and normal prepass, for ambient occlusion.
@fragment
fn fs_terrain_prepass(in: TerrainOut) -> @location(0) vec4<f32> {
    return prepass_normal(normalize(in.normal));
}
