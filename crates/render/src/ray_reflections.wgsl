// Ray traced reflections (RL4, renderer/ray_reflections.rs,
// docs/rendering/ray-tracing.md): from each glossy solid pixel of the
// prepass (smoother than the setting's cutoff, its roughness kept in the
// prepass normal's w as 1 plus it, `fs_prepass` in shader.wgsl), a ray
// along the mirrored view through the ray traced scene. What it hits is lit
// as ray traced GI lights a hit (ray_gi.wgsl's `gi_hit_light`, water's
// `water_hit_light`): its color (with its texture's average) and glow, the
// sun with a ray toward it for its shadow, the lamps in reach, the flat
// ambient light and the light probes, fogged on the way back. Written as
// half floats, two u32s a traced pixel, which the scene's `shade_surface`
// (lighting.wgsl's `traced_reflected`) puts in the reflection probes'
// place. A miss, or a pixel not traced, writes a weight of 0: the probes
// and the sky stay. Compiled after `sky.wgsl`, `fog.wgsl`, `clouds.wgsl`,
// `lighting.wgsl` and `ray_cut_outs.wgsl`, with `enable wgpu_ray_query;`
// put first (renderer/gpu.rs).

// Must match `RayReflectionUniform` in renderer/ray_reflections.rs.
struct RayReflections {
    // The scene's clip space (as drawn this frame) to world space.
    clip_to_world: mat4x4<f32>,
    // The camera's view space to world space (for the prepass normals).
    view_to_world: mat4x4<f32>,
    // x, y: the traced size in pixels, z: how many scene pixels one stands
    // for each way (1 or 2), w: unused.
    size: vec4<u32>,
    // x: the roughness from which surfaces aren't traced, y: how far the
    // rays reach in studs, z: how far a hit's shadow rays start off it,
    // w: unused.
    settings: vec4<f32>,
}

@group(1) @binding(0) var ray_scene: acceleration_structure;
@group(1) @binding(1) var<uniform> reflections: RayReflections;
// Two a TLAS slot (renderer/ray_tracing.rs `SurfaceRaw`): rgb its color and
// w 1 for a cut-out, then rgb its glow, both linear.
@group(1) @binding(2) var<storage, read> reflection_surfaces: array<vec4<f32>>;
// Two u32s a traced pixel: rg, then b and the weight, as half floats.
@group(1) @binding(3) var<storage, read_write> traced_reflections: array<vec2<u32>>;
@group(1) @binding(4) var reflection_depth: texture_depth_2d;
@group(1) @binding(5) var reflection_normals: texture_2d<f32>;
// Cut-outs' holes (LE7, ray_cut_outs.wgsl).
@group(1) @binding(6) var<storage, read> cut_outs: array<CutOut>;
@group(1) @binding(7) var<storage, read> cut_out_uvs: array<vec2<f32>>;
@group(1) @binding(8) var cut_out_alpha: texture_2d_array<f32>;
@group(1) @binding(9) var cut_out_sampler: sampler;

// Instance masks (renderer/ray_tracing.rs): solid items and cut-outs,
// casting shadows (1, 2) or not (8, 16). See-through items and water
// aren't seen (they're drawn after the scene this is read in).
const REFLECT_MASK_SURFACES: u32 = 27u;
const REFLECT_MASK_CASTERS: u32 = 3u;
// The most lamps one hit is lit by: the first so many of the frame's.
const REFLECT_MOST_LAMPS: u32 = 8u;

fn reflect_blocked(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    let hit = trace_through_holes(RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT, REFLECT_MASK_CASTERS, 0.0, distance, origin, direction));
    return hit.kind != RAY_QUERY_INTERSECTION_NONE;
}

// The light leaving a surface at `position` facing `n` of `color` (linear).
fn reflect_hit_light(position: vec3<f32>, n: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let lift = n * reflections.settings.z;
    var light = vec3<f32>(0.0);
    let l = frame.sun_direction.xyz;
    let n_dot_l = dot(n, l);
    if n_dot_l > 0.0 && (frame.shadow.x < 0.5 || !reflect_blocked(position + lift, l, 1e6)) {
        light += frame.sun_radiance.rgb * n_dot_l / PI
            * cloud_shadow(frame.clouds, position, l);
    }
    let lamps = min(frame.lights.x, REFLECT_MOST_LAMPS);
    for (var i = 0u; i < lamps; i++) {
        let lamp = lights[i];
        let reach = lamp_reach(lamp, position);
        if reach.w <= 0.0 {
            continue;
        }
        let distance = length(reach.xyz);
        let toward = reach.xyz / max(distance, 0.0001);
        let facing = dot(n, toward);
        if facing <= 0.0 {
            continue;
        }
        if lamp.shadow.x >= 0.0 && reflect_blocked(position + lift, toward, max(distance - 0.05, 0.0)) {
            continue;
        }
        light += lamp.radiance_cone.rgb * reach.w * facing / PI;
    }
    light += frame.ambient.rgb + surrounding_diffuse(position, n, frame.sky_top.w);
    return color * light;
}

@compute @workgroup_size(8, 8)
fn cs_trace_reflections(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = reflections.size;
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let out = id.y * size.x + id.x;
    traced_reflections[out] = vec2<u32>(0u);
    let full = vec2<i32>(textureDimensions(reflection_depth));
    let pixel = min(vec2<i32>(id.xy * size.z), full - 1);
    let depth = textureLoad(reflection_depth, pixel, 0);
    // The sky.
    if depth >= 1.0 {
        return;
    }
    // Only plain solid pixels keep their roughness (1 plus it); a
    // character's (0.5) and a cut-out's (0.875) aren't traced.
    let texel = textureLoad(reflection_normals, pixel, 0);
    if texel.w < 0.99 {
        return;
    }
    let roughness = texel.w - 1.0;
    if roughness >= reflections.settings.x {
        return;
    }
    let uv = (vec2<f32>(pixel) + 0.5) / vec2<f32>(full);
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world4 = reflections.clip_to_world * clip;
    let world = world4.xyz / world4.w;
    let camera = frame.camera_position.xyz;
    let distance = length(world - camera);
    let reach = reflections.settings.y;
    if distance >= reach {
        return;
    }
    let n = normalize((reflections.view_to_world * vec4<f32>(normalize(texel.xyz), 0.0)).xyz);
    let v = normalize(camera - world);
    let direction = reflect(-v, n);
    // Off the surface, by more the further away it is as depth gets coarser.
    let origin = world + n * (0.02 + distance * 0.002);
    let hit = trace_through_holes(RayDesc(
        0u, REFLECT_MASK_SURFACES, 0.0, reach, origin, direction));
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return;
    }
    let surface = 2u * hit.instance_custom_data;
    let color = reflection_surfaces[surface];
    // Inside something solid: leave it to the probes.
    if !hit.front_face && color.w < 0.5 {
        return;
    }
    let glow = reflection_surfaces[surface + 1u].rgb;
    let position = origin + direction * hit.t;
    // Rays don't give the triangle's own normal: a hit faces back along
    // the ray.
    let light = fogged(frame.fog, origin, position,
        glow + reflect_hit_light(position, -direction, color.rgb));
    // Fading out toward the ray tracing distance, where the probes take
    // over.
    let weight = 1.0 - smoothstep(reach * 0.85, reach, distance);
    let kept = min(max(light, vec3<f32>(0.0)), vec3<f32>(60000.0));
    traced_reflections[out] = vec2<u32>(pack2x16float(kept.rg),
        pack2x16float(vec2<f32>(kept.b, weight)));
}
