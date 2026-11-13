// Ray traced sun shadows (renderer/ray_shadows.rs, docs/rendering/ray-tracing.md):
// from each solid pixel of the prepass, rays toward the sun through the ray
// traced scene. Hard shadows trace one ray and write the mask straight
// away; soft ones trace a few rays spread over the sun's disc, then smooth
// the result across and down, by how wide each pixel's soft edge is.
//
// The mask the scene reads (lighting.wgsl's `ray_shadow_mask`) holds a u32
// per pixel: the surface's distance from the camera in the top 24 bits (an
// f32 with its lowest mantissa bits cleared) and how much sunlight reaches
// it in the low 8 (0 to 255). A distance of 0 means "no ray traced shadow
// here": the scene uses the shadow maps.
enable wgpu_ray_query;

// Must match `RayShadowUniform` in renderer/ray_shadows.rs.
struct RayShadows {
    // The prepass's camera (with TAA's jitter), clip to world space.
    inv_view_proj: mat4x4<f32>,
    // The camera's view to world space, for the prepass's normals.
    view_to_world: mat4x4<f32>,
    // xyz: the camera, w: how far ray traced shadows reach in studs.
    camera: vec4<f32>,
    // xyz: toward the sun, w: how wide the sun looks (a shadow's soft edge
    // per stud between caster and receiver; 0 for hard shadows).
    sun: vec4<f32>,
    // x: rays per pixel, y: a number that changes each frame (0 without
    // TAA, so a still picture keeps still), z: the size of a pixel per
    // stud away from the camera, w: unused.
    settings: vec4<f32>,
    // xy: the scene's width and height in pixels.
    size: vec4<u32>,
    // Lamps' ray traced shadows: x: 1 to trace them, y: the size of a light
    // tile in pixels, z: how many tiles across.
    lamps: vec4<u32>,
    // x: how wide a lamp's bulb is taken to be, in studs (0 for hard
    // shadows), y: rays per lamp for soft shadows.
    lamp: vec4<f32>,
}

@group(0) @binding(0) var scene: acceleration_structure;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var normal_texture: texture_2d<f32>;
@group(0) @binding(3) var<uniform> rays: RayShadows;
// What the pass reads (the blurs) and writes, a u32 per pixel each.
@group(0) @binding(4) var<storage, read> source: array<u32>;
@group(0) @binding(5) var<storage, read_write> destination: array<u32>;

// Cut-outs' holes (renderer/ray_cut_outs.rs). Must match `CutOutRaw`.
struct CutOut {
    // x: its mesh's first triangle in `cut_out_uvs`, y: its alpha's layer
    // plus 1 (0: no holes), z: the alpha cutoff's bits.
    mesh: vec4<u32>,
    // The columns of the material's texture coordinate matrix.
    uv_matrix: vec4<f32>,
    // xy: the material's texture coordinate offset.
    uv_offset: vec4<f32>,
}
// One a TLAS slot, by the instance's custom data.
@group(0) @binding(6) var<storage, read> cut_outs: array<CutOut>;
// Three a triangle, from each cut-out mesh's first.
@group(0) @binding(7) var<storage, read> cut_out_uvs: array<vec2<f32>>;
@group(0) @binding(8) var cut_out_alpha: texture_2d_array<f32>;
@group(0) @binding(9) var cut_out_sampler: sampler;

// A point, spot or rect light. Must match `Light` in lighting.wgsl and
// `LightRaw` in renderer/types.rs.
struct Light {
    position_range: vec4<f32>,
    radiance_cone: vec4<f32>,
    direction_inner: vec4<f32>,
    // z: 0 for a lamp that casts no shadows, otherwise 1 plus how far from
    // the light its shadows start (so the lamp's own part doesn't shade).
    shadow: vec4<f32>,
    rect: vec4<f32>,
}
// The most lights one tile lists. Must match `LIGHTS_PER_TILE`.
const LIGHTS_PER_TILE: u32 = 64u;
// The most lamps a pixel traces shadows for: 4 bits each in a u32. Must
// match `TRACED_LAMPS` in lighting.wgsl.
const TRACED_LAMPS: u32 = 8u;
@group(0) @binding(10) var<storage, read> lights: array<Light>;
@group(0) @binding(11) var<storage, read> tile_lights: array<u32>;
// Each pixel's lamps' shadows: 4 bits a lamp (0 in shadow, 15 lit), in the
// order its tile lists the lamps that cast shadows.
@group(0) @binding(12) var<storage, read_write> lamp_mask: array<u32>;

// Instance masks of what casts shadows (renderer/ray_tracing.rs): solid
// items, and cut-outs.
const MASK_SOLID: u32 = 1u;
const MASK_CUT_OUT: u32 = 2u;
// A cut-out nearer than this to the surface isn't counted: a leaf card
// lying on the ground, or a grass clump's foot, would shade the ground it
// stands in, and its own card can graze the point the ray starts from.
const CUT_OUT_NEAR: f32 = 0.35;
// The widest a soft edge is smoothed, in pixels each way.
const MAX_BLUR: i32 = 12;
// Marks a pixel the soft passes have nothing for (sky, or past the reach).
const NOTHING: f32 = -1.0;

// The same noise as `pixel_noise` in lighting.wgsl.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn index_of(pixel: vec2<i32>) -> u32 {
    return u32(pixel.y) * rays.size.x + u32(pixel.x);
}

// The world point the prepass drew at `pixel`, or w = 0 for the sky.
fn world_at(pixel: vec2<i32>) -> vec4<f32> {
    let depth = textureLoad(depth_texture, pixel, 0);
    if depth >= 1.0 {
        return vec4<f32>(0.0);
    }
    let size = vec2<f32>(rays.size.xy);
    let uv = (vec2<f32>(pixel) + 0.5) / size;
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world = rays.inv_view_proj * clip;
    return vec4<f32>(world.xyz / world.w, 1.0);
}

// The mask's u32 for a surface `distance` from the camera, `visible` lit.
fn pack_mask(distance: f32, visible: f32) -> u32 {
    let shade = u32(round(clamp(visible, 0.0, 1.0) * 255.0));
    return (bitcast<u32>(distance) & 0xFFFFFF00u) | shade;
}

// The soft passes' u32: how much is lit and how far away what blocks it is
// (0 for nothing), as two halves.
fn pack_soft(visible: f32, blocker: f32) -> u32 {
    return pack2x16float(vec2<f32>(visible, blocker));
}

// Whether a ray's candidate hit on a cut-out's triangle lands where its
// texture isn't cut out: its texture coordinates there (the triangle's
// three, by where it was hit, through the material's own scale, turn and
// offset), and the alpha at them against the cutoff.
fn cut_out_covers(hit: RayIntersection) -> bool {
    let cut = cut_outs[hit.instance_custom_data];
    if cut.mesh.y == 0u {
        return true;
    }
    let corner = (cut.mesh.x + hit.primitive_index) * 3u;
    let b = hit.barycentrics;
    let uv = cut_out_uvs[corner] * (1.0 - b.x - b.y) + cut_out_uvs[corner + 1u] * b.x
        + cut_out_uvs[corner + 2u] * b.y;
    let placed = mat2x2<f32>(cut.uv_matrix.xy, cut.uv_matrix.zw) * uv + cut.uv_offset.xy;
    let alpha = textureSampleLevel(cut_out_alpha, cut_out_sampler, placed, i32(cut.mesh.y - 1u), 0.0).r;
    return alpha >= bitcast<f32>(cut.mesh.z);
}

// How far along the ray from `origin` an item of `mask` blocks it, from
// `near` on, or 0 for nothing. Cut-outs' triangles come as candidates
// (their BLAS isn't opaque) and block only where they aren't cut out.
fn first_hit(origin: vec3<f32>, direction: vec3<f32>, mask: u32, near: f32, far: f32) -> f32 {
    if far <= near {
        return 0.0;
    }
    var query: ray_query;
    rayQueryInitialize(&query, scene, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT, mask, near, far, origin, direction));
    while rayQueryProceed(&query) {
        let candidate = rayQueryGetCandidateIntersection(&query);
        if candidate.kind == RAY_QUERY_INTERSECTION_TRIANGLE && cut_out_covers(candidate) {
            rayQueryConfirmIntersection(&query);
        }
    }
    let hit = rayQueryGetCommittedIntersection(&query);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return 0.0;
    }
    return max(hit.t, 1e-3);
}

// One ray from `origin` along `direction` (normalized), up to `far`: how
// far to what blocks it, or 0.
fn blocked_within(origin: vec3<f32>, direction: vec3<f32>, far: f32) -> f32 {
    let solid = first_hit(origin, direction, MASK_SOLID, 0.0, far);
    if solid > 0.0 {
        return solid;
    }
    return first_hit(origin, direction, MASK_CUT_OUT, CUT_OUT_NEAR, far);
}

// One ray toward the sun: how far to what blocks it, or 0.
fn blocked_at(origin: vec3<f32>, direction: vec3<f32>) -> f32 {
    return blocked_within(origin, direction, rays.camera.w * 2.0);
}

// How much of `light` reaches the surface at `point` (facing `normal`): 1
// lit, 0 in shadow. Rays go to points spread over the bulb (a disc as wide
// as `rays.lamp.x`, facing the surface) or, for a rect light with soft
// shadows, over its rectangle, and stop short of the light by its shadows'
// start, so the part it sits in doesn't shade it.
fn lamp_lit(light: Light, point: vec3<f32>, normal: vec3<f32>, distance: f32, turn: f32) -> f32 {
    let center = light.position_range.xyz;
    let to_center = center - point;
    let range = light.position_range.w;
    if dot(to_center, to_center) >= range * range {
        return 1.0;
    }
    let rect = light.radiance_cone.w < -2.5;
    let half_width = length(light.rect.xyz);
    let right = light.rect.xyz / max(half_width, 0.0001);
    let up = cross(light.direction_inner.xyz, right);
    if rect && dot(-to_center, light.direction_inner.xyz) <= 0.0 {
        return 0.0;
    }
    let origin = point + normal * (0.02 + distance * 0.002);
    let start = max(light.shadow.z - 1.0, 0.0);
    let soft = rays.lamp.x > 0.0;
    let count = select(1u, max(u32(rays.lamp.y), 1u), soft);
    // Square to the way to the light, for spreading over a round bulb.
    let way = normalize(to_center);
    let side = normalize(cross(way, select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0),
        abs(way.y) > 0.99)));
    let other = cross(way, side);
    var lit = 0.0;
    for (var i = 0u; i < count; i++) {
        var aim = center;
        if rect {
            if soft {
                // A stratified point of the rectangle, turned per pixel.
                let k = fract((f32(i) + 0.5) / f32(count) + turn);
                let j = fract(f32(i) * 0.618034 + turn * 1.618034);
                aim += right * (k * 2.0 - 1.0) * half_width + up * (j * 2.0 - 1.0) * light.rect.w;
            } else {
                // Hard: from the nearest point, as the rect light shines.
                let from_center = -to_center;
                aim += right * clamp(dot(from_center, right), -half_width, half_width)
                    + up * clamp(dot(from_center, up), -light.rect.w, light.rect.w);
            }
        } else if soft {
            let k = (f32(i) + 0.5) / f32(count);
            let angle = turn * 6.2831853 + f32(i) * 2.3999632;
            let offset = sqrt(k) * rays.lamp.x * 0.5 * vec2<f32>(cos(angle), sin(angle));
            aim += side * offset.x + other * offset.y;
        }
        let toward = aim - origin;
        let length_to = length(toward);
        if length_to < 1e-4 || dot(toward, normal) <= 0.0 {
            continue;
        }
        let direction = toward / length_to;
        if blocked_within(origin, direction, length_to - start) == 0.0 {
            lit += 1.0;
        }
    }
    return lit / f32(count);
}

// The lamps' shadows at `pixel` (on `point`, facing `normal`), packed 4
// bits a lamp in the order its tile lists the lamps that cast shadows.
fn traced_lamps(pixel: vec2<i32>, point: vec3<f32>, normal: vec3<f32>, distance: f32) -> u32 {
    let tile = vec2<u32>(pixel) / max(rays.lamps.y, 1u);
    let first = (tile.y * rays.lamps.z + tile.x) * (LIGHTS_PER_TILE + 1u);
    let count = min(tile_lights[first], LIGHTS_PER_TILE);
    let turn = fract(pixel_noise(vec2<f32>(pixel)) + rays.settings.y * 0.618034);
    var packed = 0xFFFFFFFFu;
    var slot = 0u;
    for (var i = 0u; i < count && slot < TRACED_LAMPS; i++) {
        let light = lights[tile_lights[first + 1u + i]];
        if light.shadow.z <= 0.0 {
            continue;
        }
        let lit = lamp_lit(light, point, normal, distance, turn);
        let shade = u32(round(clamp(lit, 0.0, 1.0) * 15.0));
        packed = (packed & ~(15u << (slot * 4u))) | (shade << (slot * 4u));
        slot += 1u;
    }
    return packed;
}

@compute @workgroup_size(8, 8)
fn cs_trace(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= rays.size.xy) {
        return;
    }
    let pixel = vec2<i32>(id.xy);
    let index = index_of(pixel);
    let soft = rays.sun.w > 0.0;
    let point = world_at(pixel);
    let distance = length(point.xyz - rays.camera.xyz);
    if point.w == 0.0 || distance > rays.camera.w {
        destination[index] = select(0u, pack_soft(NOTHING, 0.0), soft);
        return;
    }
    let view_normal = textureLoad(normal_texture, pixel, 0).xyz;
    let normal = normalize((rays.view_to_world * vec4<f32>(view_normal, 0.0)).xyz);
    if rays.lamps.x != 0u && index < arrayLength(&lamp_mask) {
        lamp_mask[index] = traced_lamps(pixel, point.xyz, normal, distance);
    }
    let sun = rays.sun.xyz;
    // Facing away from the sun: it shades itself.
    if dot(normal, sun) <= 0.0 {
        destination[index] = select(pack_mask(distance, 0.0), pack_soft(0.0, 0.0), soft);
        return;
    }
    // Off the surface, by more the further away it is as depth gets coarser.
    let origin = point.xyz + normal * (0.02 + distance * 0.002);
    if !soft {
        let lit = select(0.0, 1.0, blocked_at(origin, sun) == 0.0);
        destination[index] = pack_mask(distance, lit);
        return;
    }

    // Rays spread over the sun's disc: a spiral of golden-angle steps,
    // turned per pixel (and per frame under TAA) so neighbours sample it
    // differently and the smoothing evens them out.
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(sun.y) > 0.99);
    let right = normalize(cross(sun, up));
    let forward = cross(right, sun);
    let radius = rays.sun.w * 0.5;
    let count = max(u32(rays.settings.x), 1u);
    let turn = (pixel_noise(vec2<f32>(pixel)) + rays.settings.y * 0.618034) * 6.2831853;
    var lit = 0.0;
    var blocker = 0.0;
    var blocked = 0.0;
    for (var i = 0u; i < count; i++) {
        let k = (f32(i) + 0.5) / f32(count);
        let angle = turn + f32(i) * 2.3999632;
        let offset = sqrt(k) * radius * vec2<f32>(cos(angle), sin(angle));
        let direction = normalize(sun + right * offset.x + forward * offset.y);
        let t = blocked_at(origin, direction);
        if t == 0.0 {
            lit += 1.0;
        } else {
            blocker += t;
            blocked += 1.0;
        }
    }
    destination[index] = pack_soft(lit / f32(count), select(0.0, blocker / max(blocked, 1.0), blocked > 0.0));
}

// How many pixels wide the soft edge of a shadow is, each way, for a
// surface `distance` away whose blocker is `blocker` studs off.
fn blur_radius(distance: f32, blocker: f32) -> f32 {
    let edge = blocker * rays.sun.w;
    let pixel = max(distance * rays.settings.z, 1e-5);
    return clamp(0.5 * edge / pixel, 1.0, f32(MAX_BLUR));
}

// Smooths the soft passes' result along `step` (one pixel across or down),
// leaving out neighbours on other surfaces (a different distance, or
// facing another way), over each pixel's soft edge.
fn smoothed_along(pixel: vec2<i32>, step: vec2<i32>) -> vec2<f32> {
    let here = unpack2x16float(source[index_of(pixel)]);
    if here.x < 0.0 {
        return here;
    }
    let point = world_at(pixel);
    let distance = length(point.xyz - rays.camera.xyz);
    let normal = textureLoad(normal_texture, pixel, 0).xyz;
    let world_normal = normalize((rays.view_to_world * vec4<f32>(normal, 0.0)).xyz);
    let last = vec2<i32>(rays.size.xy) - 1;
    // How far a neighbour may be off this pixel's surface (its plane, so a
    // slope seen side-on, whose pixels lie far apart in depth, still
    // smooths: comparing distances from the camera left it dithered).
    let tolerance = 0.01 * distance + 0.05;
    // The widest soft edge near here: a lit pixel next to a shadow has no
    // blocker of its own but lies in that shadow's edge.
    var blocker = here.y;
    for (var k = -MAX_BLUR; k <= MAX_BLUR; k += 3) {
        let other = unpack2x16float(source[index_of(clamp(pixel + step * k, vec2<i32>(0), last))]);
        blocker = max(blocker, other.y);
    }
    let radius = blur_radius(distance, blocker);
    let reach = i32(ceil(radius));
    var total = 0.0;
    var weights = 0.0;
    for (var k = -reach; k <= reach; k++) {
        let at = clamp(pixel + step * k, vec2<i32>(0), last);
        let other = unpack2x16float(source[index_of(at)]);
        if other.x < 0.0 {
            continue;
        }
        let there = world_at(at);
        let apart = abs(dot(there.xyz - point.xyz, world_normal));
        let facing = dot(textureLoad(normal_texture, at, 0).xyz, normal);
        let x = f32(k) / radius;
        let weight = exp(-2.0 * x * x)
            * max(1.0 - apart / tolerance, 0.0)
            * smoothstep(0.7, 0.95, facing);
        total += other.x * weight;
        weights += weight;
    }
    return vec2<f32>(select(here.x, total / weights, weights > 0.0), blocker);
}

@compute @workgroup_size(8, 8)
fn cs_blur_across(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= rays.size.xy) {
        return;
    }
    let pixel = vec2<i32>(id.xy);
    let smoothed = smoothed_along(pixel, vec2<i32>(1, 0));
    destination[index_of(pixel)] = pack_soft(smoothed.x, smoothed.y);
}

@compute @workgroup_size(8, 8)
fn cs_blur_down(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= rays.size.xy) {
        return;
    }
    let pixel = vec2<i32>(id.xy);
    let smoothed = smoothed_along(pixel, vec2<i32>(0, 1));
    if smoothed.x < 0.0 {
        destination[index_of(pixel)] = 0u;
        return;
    }
    let distance = length(world_at(pixel).xyz - rays.camera.xyz);
    destination[index_of(pixel)] = pack_mask(distance, smoothed.x);
}
