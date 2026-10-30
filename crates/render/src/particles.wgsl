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
    // The spin (radians a second), brightness, light emission.
    look: vec4<f32>,
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

// A particle born now, number `index` of the emitter's births: somewhere
// inside the part, heading out within the spread.
fn born(index: u32) -> Particle {
    var seed = pcg(index + pcg(emitter.counts.w));
    let inside = vec3<f32>(random(&seed), random(&seed), random(&seed)) - vec3<f32>(0.5);
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
        particle.position_age = vec4<f32>(
            particle.position_age.xyz + moved * seconds,
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
        let transparency = clamp(mix(emitter.color_start.w, emitter.color_end.w, t), 0.0, 1.0);
        let opacity = 1.0 - transparency;
        let size = max(mix(emitter.life_size.y, emitter.life_size.z, t), 0.0);
        // Hidden ones stay squares of no size, which draw nothing.
        if opacity > 0.0 && size > 0.0 {
            let srgb = mix(emitter.color_start.rgb, emitter.color_end.rgb, t);
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
