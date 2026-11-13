// Fog (graphics Phase 25), shared by the scene's shaders (after `sky.wgsl`,
// before `lighting.wgsl`), the sprites' and the volumetric fog's. No
// bindings: each caller passes in its own copy of the fog. See `Fog` in
// draw_list.rs and renderer/fog.rs, whose `FogRaw` matches `Fog` here.
// Colors are linear light.

// How far away the sky is taken to be, in studs. Must match
// `Fog::SKY_DISTANCE` in draw_list.rs.
const FOG_SKY_DISTANCE: f32 = 5000.0;

struct Fog {
    // rgb: the fog's own light, w: 1 while any fog is drawn.
    color: vec4<f32>,
    // x: where distance fog starts, y: where it's whole, in studs (y is
    // huge without it), z: how much of the lamps' light height fog
    // scatters (the share of the sun's in `sun`), w: 1 while the volumetric
    // fog scatters height fog's own light within its reach, by how open the
    // air is there, in place of this.
    distance: vec4<f32>,
    // x: how much light height fog takes away per stud at and below its
    // height (0 without it), y: that height, z: how many studs above it the
    // fog thins by e, w: how far the volumetric fog reaches (0 while it's
    // off), within which it scatters the sun's light in place of this.
    height: vec4<f32>,
    // rgb: the sun's light the height fog scatters, w: how much of it goes
    // forward (Henyey-Greenstein g).
    sun: vec4<f32>,
    // xyz: toward the sun.
    sun_direction: vec4<f32>,
    // Dust or mist filling a box (a FogEffect with a Density). x: how much
    // light it takes away per stud inside (0 without it), y: how much of
    // its light goes forward (g), z: the studs over which it thins to none
    // across the box's faces.
    dust: vec4<f32>,
    // rgb: how much of the light it takes away it scatters.
    dust_color: vec4<f32>,
    // rgb: the sun's light it scatters.
    dust_sun: vec4<f32>,
    // rgb: the sky's light it scatters with nothing in the way, for while
    // the volumetric fog (which lights it by the light around it) is off.
    dust_ambient: vec4<f32>,
    // The rows of the world to the box's own space (studs, its middle at
    // 0), and xyz: half its size.
    dust_x: vec4<f32>,
    dust_y: vec4<f32>,
    dust_z: vec4<f32>,
    dust_extent: vec4<f32>,
}

fn dust_local(fog: Fog, p: vec3<f32>) -> vec3<f32> {
    let q = vec4<f32>(p, 1.0);
    return vec3<f32>(dot(fog.dust_x, q), dot(fog.dust_y, q), dot(fog.dust_z, q));
}

// How much light the dust takes away per stud at `p`: all of its density
// inside the box, thinning to none across its faces (half the edge outside
// them, half inside, so the box's own size is right on average).
fn dust_density(fog: Fog, p: vec3<f32>) -> f32 {
    if fog.dust.x <= 0.0 {
        return 0.0;
    }
    let local = dust_local(fog, p);
    let half_size = fog.dust_extent.xyz;
    let inside = min(min(half_size.x - abs(local.x), half_size.y - abs(local.y)), half_size.z - abs(local.z));
    if fog.dust.z <= 0.0 {
        return select(0.0, fog.dust.x, inside > 0.0);
    }
    return fog.dust.x * saturate(inside / fog.dust.z + 0.5);
}

// How many studs of the way from `eye` along `dir` (normalized), up to
// `distance`, are inside the dust's box.
fn dust_span(fog: Fog, eye: vec3<f32>, dir: vec3<f32>, distance: f32) -> f32 {
    if fog.dust.x <= 0.0 {
        return 0.0;
    }
    let start = dust_local(fog, eye);
    let way = vec3<f32>(dot(fog.dust_x.xyz, dir), dot(fog.dust_y.xyz, dir), dot(fog.dust_z.xyz, dir));
    // A way along a face's plane would divide by 0: nudge it off.
    let safe = select(way, vec3<f32>(1e-6), abs(way) < vec3<f32>(1e-6));
    let a = (-fog.dust_extent.xyz - start) / safe;
    let b = (fog.dust_extent.xyz - start) / safe;
    let enter = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
    let leave = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
    return max(min(leave, distance) - max(enter, 0.0), 0.0);
}

// How much of the light arriving along `toward_light` is scattered toward
// a viewer looking along `view` (Henyey-Greenstein, over 4π): `g` near 1
// sends most of it straight on.
fn fog_phase(view: vec3<f32>, toward_light: vec3<f32>, g: f32) -> f32 {
    let cos_theta = dot(view, toward_light);
    let g2 = g * g;
    let denominator = max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4);
    return (1.0 - g2) / (4.0 * 3.14159265 * denominator * sqrt(denominator));
}

// The height fog's density at height `y`, as a share of its densest.
fn fog_thickness(fog: Fog, y: f32) -> f32 {
    return exp(-max(y - fog.height.y, 0.0) / max(fog.height.z, 0.001));
}

// How much height fog there is below `y` (a column of it, as `y` grows):
// so the fog between two heights is the difference.
fn fog_column(fog: Fog, y: f32) -> f32 {
    let base = fog.height.y;
    let falloff = max(fog.height.z, 0.001);
    return min(y, base) + falloff * (1.0 - exp(-max(y - base, 0.0) / falloff));
}

// How much of the light from `distance` studs along `dir` (normalized)
// from `eye` gets through the fog: 1 in clear air, toward 0 in thick fog.
fn fog_transmittance(fog: Fog, eye: vec3<f32>, dir: vec3<f32>, distance: f32) -> f32 {
    var through = 1.0;
    // Height fog: its density, which changes only with height, taken along
    // the way. Nearly level, the density halfway stands for the rest.
    if fog.height.x > 0.0 {
        let rise = dir.y * distance;
        var mean: f32;
        if abs(rise) < 0.01 * max(fog.height.z, 0.001) {
            mean = fog_thickness(fog, eye.y + 0.5 * rise);
        } else {
            mean = (fog_column(fog, eye.y + rise) - fog_column(fog, eye.y)) / rise;
        }
        through = exp(-fog.height.x * max(mean, 0.0) * distance);
    }
    // Distance fog: straight from its start to its end.
    let span = max(fog.distance.y - fog.distance.x, 0.001);
    return through * (1.0 - clamp((distance - fog.distance.x) / span, 0.0, 1.0));
}

// The light reaching `eye` from a surface at `position` that gives `color`:
// some of it lost to the fog and dust on the way, with the fog's own light,
// the sun's and the dust's scattered in its place. Within the volumetric
// fog's reach, its grid scatters the sun's light there (with its shadows),
// height fog's own light (by how open the air is) and the dust's, so those
// are left out here.
fn fogged(fog: Fog, eye: vec3<f32>, position: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    if fog.color.w == 0.0 {
        return color;
    }
    let to = position - eye;
    let distance = length(to);
    let dir = to / max(distance, 1e-4);
    return fogged_along(fog, eye, dir, distance, color);
}

// `fogged` for what's `distance` studs along `dir` (normalized) from `eye`.
fn fogged_along(fog: Fog, eye: vec3<f32>, dir: vec3<f32>, distance: f32, color: vec3<f32>) -> vec3<f32> {
    if fog.color.w == 0.0 {
        return color;
    }
    let through = fog_transmittance(fog, eye, dir, distance);
    let reach = min(fog.height.w, distance);
    let near = select(1.0, fog_transmittance(fog, eye, dir, reach), reach > 0.0);
    var light = fog.color.rgb * max(select(1.0, near, fog.distance.w > 0.5) - through, 0.0);
    if any(fog.sun.rgb > vec3<f32>(0.0)) {
        light += fog.sun.rgb * fog_phase(dir, fog.sun_direction.xyz, fog.sun.w) * max(near - through, 0.0);
    }
    var seen = color * through + light;
    // The dust, taken as nearer than the rest of the fog (a room's air,
    // around the camera, in front of the world outside).
    if fog.dust.x > 0.0 {
        let dust_through = exp(-fog.dust.x * dust_span(fog, eye, dir, distance));
        seen *= dust_through;
        // Without the volumetric fog, its light is flat: the open sky's and
        // the sun's with nothing in the way.
        if fog.height.w <= 0.0 {
            let sun = fog.dust_sun.rgb * fog_phase(dir, fog.sun_direction.xyz, fog.dust.y);
            seen += (fog.dust_ambient.rgb + sun) * (1.0 - dust_through);
        }
    }
    return seen;
}
