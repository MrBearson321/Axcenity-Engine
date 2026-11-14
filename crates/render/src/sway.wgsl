// Plants swaying in the wind (docs/rendering/wind.md), shared by the scene
// shaders (shader.wgsl's `swayed`) and the ray traced scene's bent copies
// of swaying items (ray_sway.wgsl). Needs clouds.wgsl's `cloud_noise2`.

// Plants sway in the wind (`DrawItem::sway`). The most a plant of strength
// 1 leans, as a share of its height, and the wind speed in studs a second
// at which it leans about two thirds of that.
const SWAY_LEAN: f32 = 0.25;
const SWAY_WIND: f32 = 20.0;

// Where the world-space corner `world` of a plant (its instance's `sway`:
// the height of its ground, 1 over its height, its strength) is bent by
// the frame's wind. It leans downwind, more the stronger the wind and the
// higher up the plant, and stays still in no wind. Gusts roll over the
// ground with the wind, and a flutter shakes it. Both vary smoothly over
// the world, so a plant drawn as several items bends as one. The farthest
// a corner goes is `Sway::reach` in draw_list.rs: keep them matching.
// The fraction of `sway.w` is its flutter: how many studs each corner
// shivers, in waves that differ from one leaf to the next. `clock` is
// as `Frame::clock` (the time, then the wind), so the motion pass can bend
// a plant as it was last frame too.
fn swayed_at(world: vec3<f32>, sway: vec4<f32>, clock: vec4<f32>) -> vec3<f32> {
    let wind = clock.yw;
    let speed = length(wind);
    if sway.z <= 0.0 || speed < 1e-3 {
        return world;
    }
    let up = clamp((world.y - sway.x) * sway.y, 0.0, 1.0);
    // Stiff at the foot, giving more toward the top.
    let bend = up * up;
    let time = clock.x;
    let lean = SWAY_LEAN * sway.z / sway.y * (1.0 - exp(-speed / SWAY_WIND));
    let gust = cloud_noise2((world.xz - wind * time) * 0.03);
    let flutter = sin(time * (1.7 + speed * 0.05) + dot(world.xz, vec2<f32>(0.21, 0.17)));
    let along = lean * bend * (0.7 + 0.5 * gust + 0.15 * flutter);
    let offset = wind / speed * along;
    // Bending lowers the top a little, so the plant keeps its length.
    let drop = 0.5 * along * along * sway.y;
    var moved = vec3<f32>(offset.x, -drop, offset.y);
    let shiver = fract(sway.w);
    if shiver > 0.0 {
        // Quick waves, out of step between leaves: position decides the
        // phase, so a card keeps its shape and its neighbour doesn't match.
        // The gusts drive them, and the foot of the plant stays put.
        let phase = dot(world, vec3<f32>(2.3, 1.7, 3.1));
        let quick = time * (5.0 + speed * 0.25);
        let side = sin(quick + phase);
        let lift = sin(quick * 1.37 + phase * 1.7);
        let drive = (1.0 - exp(-speed / SWAY_WIND)) * (0.35 + 0.9 * gust) * min(up * 4.0, 1.0);
        let across = vec2<f32>(-wind.y, wind.x) / speed;
        let level = wind / speed * side + across * lift;
        moved += vec3<f32>(level.x, 0.6 * lift, level.y) * shiver * drive;
    }
    return world + moved;
}
