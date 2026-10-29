// Water (graphics Phase 29): a material's `water` drawn on the water grid
// (water.rs). Compiled after shader.wgsl, whose vertex, instance and
// material bindings it shares; see docs/rendering/water.md.
//
// Waves are Gerstner waves: each moves the surface's corners round in
// circles, so crests sharpen and troughs flatten. The pixel shader works
// out the surface's normal from the same waves plus smaller ripples, then
// mixes what the surface reflects (the sun, lamps, sky, reflection probes
// and, at High, the screen) with what's under it, seen through the water
// by a copy of the solid scene taken before see-through items
// (`scene_copy`), tinted and darkened by how deep it is, with foam where
// it's shallow.

// Gravity, in studs a second squared, for how fast waves of a length roll
// (about real gravity, at 0.28 m a stud).
const WATER_GRAVITY: f32 = 35.0;
// How rough the surface itself is: waves give it its shape.
const WATER_ROUGHNESS: f32 = 0.04;
// How much light water reflects head-on.
const WATER_F0: f32 = 0.02;
// How many cells across the water grid is. Must match `WATER_GRID` in water.rs.
const WATER_GRID_CELLS: f32 = 64.0;
// The waves the corners move by, and those the pixels' normals add to them.
const CORNER_WAVES: u32 = 4u;
const ALL_WAVES: u32 = 7u;
// How many steps a screen-space reflection takes, each longer than the last.
const REFLECTION_STEPS: u32 = 24u;

// A water material's values: `WaterRaw` in renderer/water.rs, in the
// material's `params`.
struct WaterLook {
    // Linear light.
    deep: vec3<f32>,
    clarity: f32,
    height: f32,
    length: f32,
    speed: f32,
    // Radians, from +X toward +Z.
    direction: f32,
    foam: f32,
    refraction: f32,
}

fn water_look() -> WaterLook {
    let a = maps.params[0];
    let b = maps.params[1];
    let c = maps.params[2];
    return WaterLook(a.xyz, max(a.w, 0.01), max(b.x, 0.0), max(b.y, 0.1), b.z, b.w, max(c.x, 0.0),
        c.y);
}

// Where the waves move a point of the surface resting at `rest` (over the
// ground), and the surface's normal there.
struct Waves {
    offset: vec3<f32>,
    normal: vec3<f32>,
}

// The first `count` waves at `rest`, `time` seconds in. Each smaller wave
// is shorter, lower and turned from the one before, so they don't line up.
// Waves shorter than four `cell`s (the grid's spacing, or the span of a
// pixel; 0 for none) fade out, since the corners are too far apart, or
// the pixels too big, to show them.
fn water_waves(rest: vec2<f32>, time: f32, look: WaterLook, count: u32, cell: f32) -> Waves {
    var turns = array<f32, 7>(0.0, 0.55, -0.8, 1.3, -1.9, 2.4, 0.9);
    var lengths = array<f32, 7>(1.0, 0.62, 0.41, 0.27, 0.13, 0.083, 0.052);
    var heights = array<f32, 7>(1.0, 0.55, 0.32, 0.2, 0.08, 0.05, 0.03);
    var offset = vec3<f32>(0.0);
    var normal = vec3<f32>(0.0, 1.0, 0.0);
    for (var i = 0u; i < count; i++) {
        let angle = look.direction + turns[i];
        let direction = vec2<f32>(cos(angle), sin(angle));
        let wave_length = look.length * lengths[i];
        let height = look.height * heights[i]
            * clamp((wave_length / max(cell, 1e-6) - 2.0) * 0.5, 0.0, 1.0);
        let k = 6.2831853 / wave_length;
        let speed = sqrt(WATER_GRAVITY * k) * look.speed;
        let phase = k * dot(direction, rest) - speed * time + f32(i) * 1.7;
        let kh = k * height;
        // How far the corners circle along the wave, as a share of its
        // height: sharp crests, but never so sharp, even with every wave
        // at its crest, that the surface loops over itself.
        let pinch = min(0.6 / max(kh * f32(ALL_WAVES), 1e-6), 1.0);
        let c = cos(phase);
        let s = sin(phase);
        offset += vec3<f32>(direction.x * pinch * height * c, height * s, direction.y * pinch * height * c);
        normal -= vec3<f32>(direction.x * kh * c, pinch * kh * s, direction.y * kh * c);
    }
    return Waves(offset, normalize(normal));
}

struct WaterOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    // Where the point rests when the water is still, over the ground.
    @location(1) rest: vec2<f32>,
    // The surface's own normal, still.
    @location(2) up: vec3<f32>,
    // The item's color (what's under is tinted by it) and opacity.
    @location(3) tint: vec4<f32>,
    @location(4) @interpolate(flat) fade: f32,
}

@vertex
fn vs_water(vertex: VertexIn, instance: InstanceIn) -> WaterOut {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let normal_matrix = mat3x3<f32>(instance.normal_0, instance.normal_1, instance.normal_2);
    let rest = (model * vec4<f32>(vertex.position, 1.0)).xyz;
    // The grid's spacing in studs, the coarser way.
    let cell = max(length(model[0].xyz), length(model[2].xyz)) / WATER_GRID_CELLS;
    let waves = water_waves(rest.xz, frame.clock.x, water_look(), CORNER_WAVES, cell);
    let world = rest + waves.offset;
    var out: WaterOut;
    out.clip_position = frame.view_proj * vec4<f32>(world, 1.0);
    out.world_position = world;
    out.rest = rest.xz;
    out.up = normal_matrix * vertex.normal;
    out.tint = instance.base_color;
    out.fade = instance.emissive_fade.w;
    return out;
}

// The copy of the solid scene at `uv` (0 to 1 across the scene): rgb its
// light, a how far it is from the camera in studs (`SCENE_COPY_SKY` for
// the sky). It's bound in ambient occlusion's place, which see-through
// surfaces never read (renderer/water.rs).
fn scene_copy(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(screen_occlusion_map, screen_occlusion_sampler, uv, 0.0);
}

// Where `world` is on the screen (0 to 1 across, down), and how far in
// front of the camera (z, at most 0 when behind it).
fn screen_point(world: vec3<f32>) -> vec3<f32> {
    let clip = frame.view_proj * vec4<f32>(world, 1.0);
    if clip.w <= 1e-4 {
        return vec3<f32>(-1.0, -1.0, 0.0);
    }
    let ndc = clip.xy / clip.w;
    return vec3<f32>(ndc * vec2<f32>(0.5, -0.5) + 0.5, clip.w);
}

// What's seen reflected from `origin` along `dir` among what's on the
// screen: rgb its light, a how sure the hit is (0 for none). The ray
// steps out, each step longer than the last, until it passes behind what
// the screen shows there; then it's narrowed down to the surface it
// crossed. A ray that comes out far behind it (it went under something),
// or leaves the screen, finds nothing, and the hit fades near the
// screen's edges and the ray's end.
fn screen_reflection(origin: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    let eye = frame.camera_position.xyz;
    var stride = 0.4;
    var near = 0.0;
    var far = stride;
    for (var i = 0u; i < REFLECTION_STEPS; i++) {
        let at = screen_point(origin + dir * far);
        if at.z <= 0.0 || any(at.xy < vec2<f32>(0.0)) || any(at.xy > vec2<f32>(1.0)) {
            break;
        }
        if distance(eye, origin + dir * far) > scene_copy(at.xy).a {
            for (var j = 0u; j < 5u; j++) {
                let middle = 0.5 * (near + far);
                let point = origin + dir * middle;
                if distance(eye, point) > scene_copy(screen_point(point).xy).a {
                    far = middle;
                } else {
                    near = middle;
                }
            }
            let point = origin + dir * far;
            let hit = screen_point(point);
            let scene = scene_copy(hit.xy);
            if distance(eye, point) - scene.a > max(stride * 1.5, 0.5) {
                return vec4<f32>(0.0);
            }
            let edge = 1.0 - smoothstep(0.85, 1.0, max(abs(hit.x * 2.0 - 1.0), abs(hit.y * 2.0 - 1.0)));
            let end = 1.0 - smoothstep(0.75, 1.0, f32(i) / f32(REFLECTION_STEPS));
            return vec4<f32>(scene.rgb, edge * end);
        }
        near = far;
        stride *= 1.3;
        far += stride;
    }
    return vec4<f32>(0.0);
}

@fragment
fn fs_water(in: WaterOut) -> @location(0) vec4<f32> {
    let pixel = in.clip_position.xy;
    // How much of the surface one pixel spans, before anything branches:
    // ripples shorter than a few pixels would only sparkle, so they fade.
    let footprint = length(fwidth(in.rest));
    let straight = scene_copy(pixel * frame.ambient_occlusion.zw);
    if dithered_out(in.fade, pixel) {
        discard;
    }
    let look = water_look();
    let time = frame.clock.x;
    let up = normalize(in.up);
    let n = water_waves(in.rest, time, look, ALL_WAVES, footprint).normal;
    let eye = frame.camera_position.xyz;
    let dist = length(eye - in.world_position);
    let v = (eye - in.world_position) / max(dist, 1e-4);
    let f0 = vec3<f32>(WATER_F0);
    let reflectance = ambient_specular(f0, WATER_ROUGHNESS, max(dot(n, v), 1e-4));
    let environment = frame.sky_top.w;

    // What the surface reflects: the sun's and lamps' glints, the sky and
    // the reflection probes, and at High what's on the screen.
    var color = shade_surface(in.world_position, up, n, pixel, vec3<f32>(0.0), 0.0,
        WATER_ROUGHNESS, 1.0, f0, false);
    let r = reflect(-v, n);
    // A reflection that dips under the surface would find what's under it.
    if frame.reflection_probes.y != 0u && dot(r, up) > 0.0 {
        let hit = screen_reflection(in.world_position, r);
        if hit.w > 0.0 {
            let sky = reflected(in.world_position, r, WATER_ROUGHNESS, environment);
            color += reflectance * (hit.rgb - sky) * hit.w;
        }
    }

    // Light the water scatters back from inside, lit as a matte surface
    // facing up would be, and the foam likewise.
    let lit = frame.sun_radiance.rgb * max(dot(up, frame.sun_direction.xyz), 0.0)
        * sun_visibility(in.world_position, up, pixel)
        * cloud_shadow(frame.clouds, in.world_position, frame.sun_direction.xyz) / PI
        + frame.ambient.rgb + surrounding_diffuse(in.world_position, up, environment);
    let scattered = look.deep * lit;
    let through = 1.0 - reflectance.x;

    var alpha = 1.0;
    if frame.reflection_probes.z != 0u {
        // What's under, bent by the waves (less where it's shallow), unless
        // the bent point shows something in front of the water.
        let shallow = clamp(straight.a - dist, 0.0, 1.0);
        let bend = (n - up) * look.refraction * shallow;
        var under = scene_copy(screen_point(in.world_position + vec3<f32>(bend.x, 0.0, bend.z)).xy);
        if under.a < dist {
            under = straight;
        }
        let depth = max(under.a - dist, 0.0);
        let clear = exp(-depth / look.clarity);
        var water = color + through * (under.rgb * in.tint.rgb * clear + scattered * (1.0 - clear));
        if look.foam > 0.0 {
            // Depth straight down, from depth along the view.
            let edge = 1.0 - smoothstep(0.0, look.foam, depth * max(dot(v, up), 0.05));
            let drift = vec2<f32>(0.15, 0.11) * time * look.speed;
            let pattern = smoothstep(0.3, 0.7, cloud_noise2(in.rest * 0.9 + drift) + 0.6 * edge - 0.3);
            water = mix(water, vec3<f32>(0.9) * lit, edge * pattern);
        }
        // A see-through part's water fades out over what's behind it.
        color = mix(straight.rgb, water, clamp(in.tint.a, 0.0, 1.0));
    } else {
        // Low: no copy of the scene, so the water is blended over it.
        color += through * scattered;
        alpha = clamp(in.tint.a * mix(0.75, 1.0, reflectance.x), 0.0, 1.0);
    }
    return vec4<f32>(fogged(frame.fog, eye, in.world_position, color), alpha);
}
