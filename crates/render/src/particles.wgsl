// GPU particles: one emitter's particles moved, sorted farthest first and
// written as sprites for sprite.wgsl. See renderer/gpu_particles.rs.
//
// `simulate` moves each particle on (or gives birth in its slot), `sort`
// is one step of a bitonic sort of their distances, and `write` turns them,
// in sorted order, into `SpriteRaw`s. They move and look as `scene`'s
// particles on the CPU do (scene/src/particles.rs): keep them matching.

struct Emitter {
    // Stretches the 1×1×1 cube to fill the emitter's part.
    spawn: mat4x4<f32>,
    // The way they head, and the cosine of the most they head away from it.
    direction: vec4<f32>,
    // Studs a second each second, and the drag.
    acceleration: vec4<f32>,
    // The air's speed, and the speed they're born with.
    air: vec4<f32>,
    // The camera, and the seconds this step moves them on by.
    eye: vec4<f32>,
    // The capacity (a power of two), births before this step, births this
    // step, and a seed.
    counts: vec4<u32>,
    // The lifetime, the size when born and when dying, the turn when born
    // (radians).
    life_size: vec4<f32>,
    // sRGB colors and transparencies when born and when dying.
    color_start: vec4<f32>,
    color_end: vec4<f32>,
    // The spin (radians a second), brightness, light emission, and 1
    // while the curves below are used instead of the start and end values.
    look: vec4<f32>,
    // The spawn shape (0 box, 1 sphere, 2 cylinder, 3 disc), 1 for only
    // its surface, and two unused (`SpawnShape`).
    spawn_shape: vec4<u32>,
    // Size, its envelope, transparency and its envelope over life, and
    // sRGB colors with the speed multiplier in w, at 32 even times from
    // birth to death (`ParticleCurves`).
    shape: array<vec4<f32>, 32>,
    colors: array<vec4<f32>, 32>,
};

struct Particle {
    position_age: vec4<f32>,
    velocity_life: vec4<f32>,
};

struct Key {
    // How far from the camera, squared; below 0 for a dead particle.
    distance: f32,
    index: u32,
};

// Must match `SpriteRaw` in renderer/sprites.rs.
struct SpriteOut {
    center_size: vec4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,
};

struct SortStep {
    j: u32,
    k: u32,
    unused: vec2<u32>,
};

@group(0) @binding(0) var<uniform> emitter: Emitter;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(2) var<storage, read_write> keys: array<Key>;
@group(0) @binding(3) var<storage, read_write> sprites_out: array<SpriteOut>;
@group(1) @binding(0) var<uniform> sort_step: SortStep;

const TAU: f32 = 6.28318530718;

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// A random number from 0 up to 1.
fn random(seed: ptr<function, u32>) -> f32 {
    *seed = pcg(*seed);
    return f32(*seed >> 8u) / 16777216.0;
}

// Where a particle is born in the centered 1×1×1 cube, from three random
// numbers, for the emitter's spawn shape: `SpawnShape::point` on the CPU.
fn spawn_point(a: f32, b: f32, c: f32) -> vec3<f32> {
    let kind = emitter.spawn_shape.x;
    let surface = emitter.spawn_shape.y == 1u;
    if kind == 1u {
        let z = 1.0 - 2.0 * a;
        let across = sqrt(max(1.0 - z * z, 0.0));
        let around = b * TAU;
        let radius = select(0.5 * pow(c, 1.0 / 3.0), 0.5, surface);
        return vec3<f32>(across * cos(around), z, across * sin(around)) * radius;
    }
    if kind == 2u || kind == 3u {
        let around = a * TAU;
        let radius = select(0.5 * sqrt(b), 0.5, surface);
        let y = select(c - 0.5, 0.0, kind == 3u);
        return vec3<f32>(cos(around) * radius, y, sin(around) * radius);
    }
    if surface {
        // A face, by its area, then a point on it.
        let scale = vec3<f32>(
            length(emitter.spawn[0].xyz),
            length(emitter.spawn[1].xyz),
            length(emitter.spawn[2].xyz),
        );
        let areas = vec3<f32>(scale.y * scale.z, scale.x * scale.z, scale.x * scale.y);
        let total = areas.x + areas.y + areas.z;
        if total > 0.0 {
            var pick = a * total;
            var axis = 2u;
            var area = areas.z;
            if pick < areas.x {
                axis = 0u;
                area = areas.x;
            } else {
                pick = pick - areas.x;
                if pick < areas.y {
                    axis = 1u;
                    area = areas.y;
                } else {
                    pick = pick - areas.y;
                }
            }
            let side = select(0.5, -0.5, pick < area / 2.0);
            let u = b - 0.5;
            let v = c - 0.5;
            if axis == 0u {
                return vec3<f32>(side, u, v);
            }
            if axis == 1u {
                return vec3<f32>(u, side, v);
            }
            return vec3<f32>(u, v, side);
        }
    }
    return vec3<f32>(a, b, c) - vec3<f32>(0.5);
}

// The speed multiplier at `t` (0 to 1) from the curve table, 1 without it.
fn speed_at(t: f32) -> f32 {
    if emitter.look.w < 0.5 {
        return 1.0;
    }
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let index = min(u32(floor(x)), 31u);
    let next = min(index + 1u, 31u);
    return mix(emitter.colors[index].w, emitter.colors[next].w, x - f32(index));
}

// A particle born now, number `index` of the emitter's births: somewhere
// in its spawn shape, heading out within the spread.
fn born(index: u32) -> Particle {
    var seed = pcg(index + pcg(emitter.counts.w));
    let first = random(&seed);
    let second = random(&seed);
    let third = random(&seed);
    let inside = spawn_point(first, second, third);
    let position = (emitter.spawn * vec4<f32>(inside, 1.0)).xyz;
    let forward = emitter.direction.xyz;
    let z = 1.0 - random(&seed) * (1.0 - emitter.direction.w);
    let around = random(&seed) * TAU;
    let across = sqrt(max(1.0 - z * z, 0.0));
    // Two directions square to `forward` and each other.
    let sign = select(-1.0, 1.0, forward.z >= 0.0);
    let a = -1.0 / (sign + forward.z);
    let b = forward.x * forward.y * a;
    let u = vec3<f32>(1.0 + sign * forward.x * forward.x * a, sign * b, -sign * forward.x);
    let v = vec3<f32>(b, sign + forward.y * forward.y * a, -forward.y);
    let heading = forward * z + (u * cos(around) + v * sin(around)) * across;
    let life = emitter.life_size.x * (0.8 + 0.4 * random(&seed));
    var particle: Particle;
    particle.position_age = vec4<f32>(position, 0.0);
    particle.velocity_life = vec4<f32>(heading * emitter.air.w, life);
    return particle;
}

@compute @workgroup_size(256)
fn simulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let slot = id.x;
    let capacity = emitter.counts.x;
    if slot >= capacity {
        return;
    }
    // How many births after this step's first this slot's would be.
    let since = (slot - emitter.counts.y) & (capacity - 1u);
    var particle = particles[slot];
    if since < emitter.counts.z {
        particle = born(emitter.counts.y + since);
    } else if particle.position_age.w < particle.velocity_life.w {
        // Drag slows them toward the air's speed.
        let seconds = emitter.eye.w;
        let slow = 1.0 / (1.0 + emitter.acceleration.w * seconds);
        let air = emitter.air.xyz;
        let velocity = particle.velocity_life.xyz + emitter.acceleration.xyz * seconds;
        let moved = air + (velocity - air) * slow;
        particle.velocity_life = vec4<f32>(moved, particle.velocity_life.w);
        // SpeedOverLife scales how far they go, at the age they start at.
        let speed = speed_at(particle.position_age.w / max(particle.velocity_life.w, 1e-6));
        particle.position_age = vec4<f32>(
            particle.position_age.xyz + moved * seconds * speed,
            particle.position_age.w + seconds,
        );
    }
    particles[slot] = particle;
    var key: Key;
    key.index = slot;
    key.distance = -1.0;
    if particle.position_age.w < particle.velocity_life.w {
        let to = particle.position_age.xyz - emitter.eye.xyz;
        key.distance = dot(to, to);
    }
    keys[slot] = key;
}

@compute @workgroup_size(256)
fn sort(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let l = i ^ sort_step.j;
    if l <= i || l >= emitter.counts.x {
        return;
    }
    let a = keys[i];
    let b = keys[l];
    // Farthest first: each run of `k` falls, and the next rises, until the
    // last step merges them all into one falling run.
    let falling = (i & sort_step.k) == 0u;
    let swap = select(a.distance > b.distance, a.distance < b.distance, falling);
    if swap {
        keys[i] = b;
        keys[l] = a;
    }
}

fn linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@compute @workgroup_size(256)
fn write(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = id.x;
    if k >= emitter.counts.x {
        return;
    }
    let particle = particles[keys[k].index];
    var out: SpriteOut;
    out.center_size = vec4<f32>(0.0);
    out.color = vec4<f32>(0.0);
    out.params = vec4<f32>(0.0);
    let age = particle.position_age.w;
    let life = particle.velocity_life.w;
    if age < life {
        let t = clamp(age / max(life, 1e-4), 0.0, 1.0);
        var transparency = clamp(mix(emitter.color_start.w, emitter.color_end.w, t), 0.0, 1.0);
        var size = max(mix(emitter.life_size.y, emitter.life_size.z, t), 0.0);
        var srgb = mix(emitter.color_start.rgb, emitter.color_end.rgb, t);
        if emitter.look.w > 0.5 {
            // As `ParticleCurves::sample` and `spreads`: two spreads from
            // -1 to 1, the same over the particle's life (its slot and life
            // change only when a new one is born there).
            let first = pcg(keys[k].index ^ bitcast<u32>(life));
            let second = pcg(first);
            let spread = vec2<f32>(f32(first >> 8u), f32(second >> 8u)) / 16777216.0 * 2.0 - 1.0;
            let x = t * 31.0;
            let index = min(u32(floor(x)), 31u);
            let next = min(index + 1u, 31u);
            let along = x - f32(index);
            let shape = mix(emitter.shape[index], emitter.shape[next], along);
            size = max(shape.x + shape.y * spread.x, 0.0);
            transparency = clamp(shape.z + shape.w * spread.y, 0.0, 1.0);
            srgb = mix(emitter.colors[index].rgb, emitter.colors[next].rgb, along);
        }
        let opacity = 1.0 - transparency;
        // Hidden ones stay squares of no size, which draw nothing.
        if opacity > 0.0 && size > 0.0 {
            let color = linear(srgb) * max(emitter.look.y, 0.0) * opacity;
            out.center_size = vec4<f32>(particle.position_age.xyz, size);
            out.color = vec4<f32>(color, opacity);
            out.params = vec4<f32>(
                emitter.life_size.w + emitter.look.x * age,
                clamp(emitter.look.z, 0.0, 1.0),
                0.0,
                0.0,
            );
        }
    }
    sprites_out[k] = out;
}
