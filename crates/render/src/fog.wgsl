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
    // x: how many of `dust` are drawn.
    dust_count: vec4<u32>,
    // Boxes of dust, mist or haze (FogEffects with a Density, or marked
    // Interior), in the order they apply: lowest priority first.
    dust: array<DustBox, MAX_DUST>,
}

// The most boxes of dust drawn at once. Must match `MAX_DUST_BOXES` in
// draw_list.rs.
const MAX_DUST: u32 = 8u;

// One box of dust. Must match `DustRaw` in renderer/fog.rs.
struct DustBox {
    // x: how much light it takes away per stud inside (0 for none), y: how
    // much of its light goes forward (g), z: the studs over which it thins
    // to none across the box's faces, w: the studs above its bottom over
    // which it thins by e (0: the same all the way up).
    params: vec4<f32>,
    // rgb: how much of the light it takes away it scatters, w: 1 for an
    // interior, which clears the air outside it (the world's fog and the
    // boxes before it) from inside it.
    color: vec4<f32>,
    // rgb: the sun's light it scatters.
    sun: vec4<f32>,
    // rgb: the sky's light it scatters with nothing in the way, where the
    // volumetric fog (which lights it by the light around it) doesn't reach.
    ambient: vec4<f32>,
    // The rows of the world to the box's own space (studs, its middle at
    // 0), and xyz: half its size.
    x: vec4<f32>,
    y: vec4<f32>,
    z: vec4<f32>,
    extent: vec4<f32>,
}

fn dust_local(b: DustBox, p: vec3<f32>) -> vec3<f32> {
    let q = vec4<f32>(p, 1.0);
    return vec3<f32>(dot(b.x, q), dot(b.y, q), dot(b.z, q));
}

// How far inside box `b` `p` is: whole inside, thinning to none across its
// faces (half the edge outside them, half inside, so the box's own size is
// right on average).
fn dust_inside(b: DustBox, p: vec3<f32>) -> f32 {
    let local = dust_local(b, p);
    let half_size = b.extent.xyz;
    let inside = min(min(half_size.x - abs(local.x), half_size.y - abs(local.y)), half_size.z - abs(local.z));
    if b.params.z <= 0.0 {
        return select(0.0, 1.0, inside > 0.0);
    }
    return saturate(inside / b.params.z + 0.5);
}

// How much of box `b`'s density is at `p`: `dust_inside`, thinned with
// height above its bottom by its falloff.
fn dust_share(b: DustBox, p: vec3<f32>) -> f32 {
    let inside = dust_inside(b, p);
    if b.params.w <= 0.0 || inside <= 0.0 {
        return inside;
    }
    let above = dust_local(b, p).y + b.extent.y;
    return inside * exp(-max(above, 0.0) / b.params.w);
}

// The air of the boxes at a point.
struct DustAt {
    // How much light the boxes take away per stud.
    density: f32,
    // How much of the world's own fog is left (interiors clear it).
    keep: f32,
    // The light they scatter per unit of light around (density × color).
    scatter: vec3<f32>,
    // The sun's light they scatter toward a viewer looking along `dir`.
    sun: vec3<f32>,
}

// The boxes' air at `p`, seen looking along `dir`. Each in order: an
// interior clears what came before it (the world's fog and lower boxes)
// by how far inside it `p` is, then adds its own.
fn dust_at(fog: Fog, p: vec3<f32>, dir: vec3<f32>) -> DustAt {
    var at = DustAt(0.0, 1.0, vec3<f32>(0.0), vec3<f32>(0.0));
    let count = min(fog.dust_count.x, MAX_DUST);
    for (var i = 0u; i < count; i++) {
        let b = fog.dust[i];
        if b.color.w > 0.5 {
            let clear = 1.0 - dust_inside(b, p);
            at.keep *= clear;
            at.density *= clear;
            at.scatter *= clear;
            at.sun *= clear;
        }
        if b.params.x <= 0.0 {
            continue;
        }
        let density = b.params.x * dust_share(b, p);
        if density <= 0.0 {
            continue;
        }
        at.density += density;
        at.scatter += density * b.color.rgb;
        at.sun += density * b.sun.rgb * fog_phase(dir, fog.sun_direction.xyz, b.params.y);
    }
    return at;
}

// Where the way from `eye` along `dir` (normalized) enters and leaves box
// `b`, in studs along it, up to `distance` (x > y: it misses).
fn dust_span(b: DustBox, eye: vec3<f32>, dir: vec3<f32>, distance: f32) -> vec2<f32> {
    let start = dust_local(b, eye);
    let way = vec3<f32>(dot(b.x.xyz, dir), dot(b.y.xyz, dir), dot(b.z.xyz, dir));
    // A way along a face's plane would divide by 0: nudge it off.
    let safe = select(way, vec3<f32>(1e-6), abs(way) < vec3<f32>(1e-6));
    let half_size = b.extent.xyz + vec3<f32>(b.params.z * 0.5);
    let near = (-half_size - start) / safe;
    let far = (half_size - start) / safe;
    let enter = max(max(min(near.x, far.x), min(near.y, far.y)), min(near.z, far.z));
    let leave = min(min(max(near.x, far.x), max(near.y, far.y)), max(near.z, far.z));
    return vec2<f32>(max(enter, 0.0), min(leave, distance));
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
    // Interiors clear the world's fog along the way inside them: what was
    // lost there is given back, taking the fog as even along the way.
    let count = min(fog.dust_count.x, MAX_DUST);
    var cleared = 0.0;
    for (var i = 0u; i < count; i++) {
        let b = fog.dust[i];
        if b.color.w > 0.5 {
            let span = dust_span(b, eye, dir, distance);
            if span.y > span.x {
                cleared += (span.y - span.x) / max(distance, 1e-4)
                    * dust_inside(b, eye + dir * (0.5 * (span.x + span.y)));
            }
        }
    }
    var seen = color * through + light;
    if cleared > 0.0 && through < 1.0 {
        let kept = pow(max(through, 1e-4), 1.0 - min(cleared, 1.0));
        seen = color * kept + light * (1.0 - kept) / (1.0 - through);
    }
    // The boxes of dust, taken as nearer than the rest of the fog (a room's
    // air, around the camera, in front of the world outside).
    for (var i = 0u; i < count; i++) {
        let b = fog.dust[i];
        if b.params.x <= 0.0 {
            continue;
        }
        let span = dust_span(b, eye, dir, distance);
        if span.y <= span.x {
            continue;
        }
        // Its density halfway along the way through it stands for the rest.
        let middle = eye + dir * (0.5 * (span.x + span.y));
        let dust_through = exp(-b.params.x * dust_share(b, middle) * (span.y - span.x));
        seen *= dust_through;
        // Past the volumetric fog's reach (all of it, while that's off),
        // its light is flat: the open sky's and the sun's with nothing in
        // the way.
        let beyond = clamp((span.y - max(span.x, fog.height.w)) / (span.y - span.x), 0.0, 1.0);
        if beyond > 0.0 {
            let sun = b.sun.rgb * fog_phase(dir, fog.sun_direction.xyz, b.params.y);
            seen += (b.ambient.rgb + sun) * (1.0 - dust_through) * beyond;
        }
    }
    return seen;
}
