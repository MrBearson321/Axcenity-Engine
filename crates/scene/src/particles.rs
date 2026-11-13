//! Particles: what each ParticleEmitter in the Workspace has given off,
//! moved on every frame and turned into camera-facing sprites for the draw
//! list.
//!
//! A host (the player, or the editor's viewport) owns one [`Particles`]
//! and calls [`Particles::update`] each frame with the time passed, then
//! [`Particles::sprites`] when it draws. Particles live in world space: once
//! out, they don't follow their part. They aren't saved, and aren't part of
//! the world; an emitter that leaves the Workspace takes its particles with
//! it.
//!
//! The built-in particle pictures ([`ParticleType`]) are drawn here, white
//! with their shape in the alpha, so a particle's color tints them.
//!
//! The weather's rain and snow are particles too: the host's `Particles`
//! keeps the world's weather (`WeatherState`, `weather.rs`), and
//! [`Particles::add_weather`] puts it into a draw list.
//!
//! Where the host's renderer runs compute shaders ([`Particles::set_gpu`]),
//! an emitter expected to keep [`GPU_PARTICLE_THRESHOLD`] or more particles
//! out moves them on the GPU instead: they're still born here, step by
//! step, but live in the renderer (`render::ParticleEmitter`), which moves
//! them the same way (`particles.wgsl`) and draws them as the same sprites,
//! up to [`MAX_GPU_PARTICLES`] of them.

use std::collections::{HashMap, VecDeque};
use std::f32::consts::TAU;
use std::sync::{Arc, OnceLock};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Color3, Instance, InstanceId, ParticleType, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{
    CURVE_ENTRIES, DrawList, Flipbook, FlipbookFrame, FlipbookMode, MAX_GPU_PARTICLES,
    ParticleCurves, ParticleEmitter, ParticleLook, ShapeInOut, SpawnKind, SpawnShape, Sprite,
    TextureSource, curve_at,
};

use crate::draw_list::image_texture;
use crate::weather::WeatherState;

#[cfg(test)]
mod collision_tests;
mod collisions;
mod presets;

use collisions::{EmitOn, Event, Landing, Sub, events_on, is_sub, subs_of};
pub use collisions::{MAX_SURFACE_RAYS, SurfaceHit, Surfaces};
pub use presets::{EffectPreset, PresetEmitter, create_preset, preset_emitter};

/// The most particles one emitter keeps; past this, no more are born
/// until some die.
pub const MAX_PER_EMITTER: usize = 4000;
/// The most particles one frame gives birth to per emitter, so a huge
/// Rate (or a long pause) can't freeze the game.
const MAX_BIRTHS_PER_FRAME: usize = 1000;
/// The shortest loop of Bursts, in seconds (`Duration`).
const MIN_BURST_DURATION: f32 = 0.05;
/// The built-in pictures' size in pixels.
const PICTURE: u32 = 64;
/// An emitter whose Rate × Lifetime (the particles it keeps out) reaches
/// this moves them on the GPU where it can, and one already there stays
/// until it falls below half. Below it, the CPU is as fast: see the
/// particle benchmark in `docs/rendering/performance.md`.
pub const GPU_PARTICLE_THRESHOLD: f32 = 3000.0;
/// The fewest particles a GPU emitter has room for.
const MIN_GPU_CAPACITY: u32 = 1024;

/// One particle in flight.
#[derive(Clone, Copy, Debug)]
struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
    /// Seconds since it last met a surface, for an emitter that Collides:
    /// a landing counts (Collision sub-emitters, LifetimeLoss) only after
    /// a moment off them, so resting or skidding on one isn't landing.
    off_surface: f32,
}

#[derive(Debug, Default)]
struct Emitter {
    particles: Vec<Particle>,
    /// Births owed from earlier frames' fractions of a particle.
    owed: f32,
    /// Seconds into the current loop of its Bursts (roadmap R17.2b).
    cycle: f32,
    /// Where its part was at the last update, and so how fast it moves,
    /// for VelocityInheritance.
    last_position: Option<Vec3>,
    velocity: Vec3,
    /// Its particles on the GPU, while it has some there or gives them off
    /// there. It can have some on the CPU too, while those die out.
    gpu: Option<GpuEmitter>,
}

/// An emitter's particles on the GPU: what the renderer is told each step.
#[derive(Debug)]
struct GpuEmitter {
    /// The last step, its look filled in when drawn.
    draw: ParticleEmitter,
    /// Births `emit` asked for since the last step.
    pending: u32,
    /// Each step's births that may still be alive, oldest first.
    batches: VecDeque<Batch>,
}

#[derive(Debug)]
struct Batch {
    /// Seconds since they were born.
    age: f32,
    count: u32,
    /// The emitter's Lifetime then: each lives 0.8 to 1.2 times it.
    lifetime: f32,
}

impl GpuEmitter {
    fn new(key: u64) -> Self {
        Self {
            draw: ParticleEmitter {
                key,
                step: 0,
                seconds: 0.0,
                capacity: MIN_GPU_CAPACITY,
                born: 0,
                births: 0,
                spawn: Mat4::IDENTITY,
                direction: Vec3::Y,
                spread: 0.0,
                speed: 0.0,
                lifetime: 0.0,
                acceleration: Vec3::ZERO,
                drag: 0.0,
                air: Vec3::ZERO,
                shape: SpawnShape::default(),
                in_out: ShapeInOut::default(),
                inherit: Vec3::ZERO,
                look: ParticleLook::default(),
            },
            pending: 0,
            batches: VecDeque::new(),
        }
    }

    /// About how many are alive, as nothing is read back from the GPU:
    /// each batch's share whose lives (spread evenly from 0.8 to 1.2 times
    /// its lifetime) haven't ended, up to the capacity.
    fn alive(&self) -> usize {
        let alive: f32 = self
            .batches
            .iter()
            .map(|batch| {
                let left = (1.2 - batch.age / batch.lifetime.max(1e-4)) / 0.4;
                batch.count as f32 * left.clamp(0.0, 1.0)
            })
            .sum();
        (alive.ceil() as usize).min(self.draw.capacity as usize)
    }
}

/// An emitter's properties, read once per frame.
struct Settings {
    enabled: bool,
    rate: f32,
    lifetime: f32,
    speed: f32,
    direction: Vec3,
    spread: f32,
    acceleration: Vec3,
    drag: f32,
    /// True if Drag slows particles toward the wind rather than to a stop.
    wind_affects_drag: bool,
    /// Where in the part they're born (`Shape`, `ShapeStyle`).
    shape: SpawnShape,
    /// `SpeedOverLife` as the curve table both paths read, if it's set.
    speed_curve: Option<[f32; CURVE_ENTRIES]>,
    /// Which way they head from the shape (`ShapeInOut`).
    in_out: ShapeInOut,
    /// How much of the part's velocity they start with, 0 to 1.
    inheritance: f32,
    /// Each burst's time (0 up to 1 of `duration`), count and envelope.
    bursts: Vec<(f32, f32, f32)>,
    /// Seconds a loop of bursts takes.
    duration: f32,
    /// True if its particles land on surfaces (`Collides`), and how.
    collides: bool,
    landing: Landing,
    /// True if its particles must stay on the CPU, where they can collide
    /// and set off sub-emitters (roadmap R17.4); set by the caller.
    cpu_only: bool,
}

impl Settings {
    fn read(instance: &Instance) -> Self {
        let number = |name, default: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map_or(default, |n| n as f32)
        };
        let vector = |name, default: Vec3| {
            instance
                .property(name)
                .and_then(Value::as_vector3)
                .map_or(default, |v| Vec3::new(v.x, v.y, v.z))
        };
        Self {
            enabled: instance.property("Enabled").and_then(Value::as_bool) == Some(true),
            rate: number("Rate", 0.0).max(0.0),
            lifetime: number("Lifetime", 1.0).max(0.0),
            speed: number("Speed", 0.0),
            direction: vector("Direction", Vec3::Y),
            spread: number("SpreadAngle", 0.0).clamp(0.0, 360.0),
            acceleration: vector("Acceleration", Vec3::ZERO),
            drag: number("Drag", 0.0).max(0.0),
            wind_affects_drag: instance
                .property("WindAffectsDrag")
                .and_then(Value::as_bool)
                == Some(true),
            shape: spawn_shape(instance),
            speed_curve: speed_curve(instance),
            in_out: match instance.property("ShapeInOut") {
                Some(Value::EnumItem(item)) if item.item == "Inward" => ShapeInOut::Inward,
                Some(Value::EnumItem(item)) if item.item == "InAndOut" => ShapeInOut::InAndOut,
                _ => ShapeInOut::Outward,
            },
            inheritance: number("VelocityInheritance", 0.0).clamp(0.0, 1.0),
            bursts: instance
                .property("Bursts")
                .and_then(Value::as_number_sequence)
                .map(|sequence| {
                    sequence
                        .keypoints()
                        .iter()
                        // Time 1 is the next loop's start.
                        .filter(|key| key.time < 1.0)
                        .map(|key| (key.time, key.value, key.envelope))
                        .collect()
                })
                .unwrap_or_default(),
            duration: number("Duration", 1.0).max(MIN_BURST_DURATION),
            collides: instance.property("Collides").and_then(Value::as_bool) == Some(true),
            landing: Landing {
                bounce: number("Bounce", 0.5).clamp(0.0, 1.0),
                lifetime_loss: number("LifetimeLoss", 0.0).clamp(0.0, 1.0),
            },
            cpu_only: false,
        }
    }

    /// How many particles the bursts give off over the `seconds` after
    /// `cycle` seconds into a loop, moving `cycle` on (roadmap R17.2b). A
    /// burst fires when its time is reached: at or after the step's start
    /// and before its end.
    fn bursts_between(&self, cycle: &mut f32, seconds: f32, random: &mut Random) -> u32 {
        if self.bursts.is_empty() {
            *cycle = 0.0;
            return 0;
        }
        let mut count = 0.0_f32;
        let mut from = cycle.clamp(0.0, self.duration);
        let mut left = seconds.max(0.0);
        // A step is at most a quarter of a second and a loop at least
        // 0.05, so this ends within a few turns.
        loop {
            let to = from + left;
            let end = to.min(self.duration);
            for &(time, value, envelope) in &self.bursts {
                let at = time * self.duration;
                if at >= from && at < end {
                    let spread = envelope * (random.next() * 2.0 - 1.0);
                    count += (value + spread).round().max(0.0);
                }
            }
            if to < self.duration {
                *cycle = to;
                break;
            }
            left = to - self.duration;
            from = 0.0;
            if left <= 0.0 {
                *cycle = 0.0;
                break;
            }
        }
        count.min(u32::MAX as f32) as u32
    }
}

/// An emitter's `Shape` and `ShapeStyle` (roadmap R17.2): a Box filled by
/// default, as before they existed.
/// An emitter's flipbook (roadmap R17.2c), or `None` while its
/// FlipbookLayout is None.
fn flipbook(instance: &Instance) -> Option<Flipbook> {
    let item = |name| match instance.property(name) {
        Some(Value::EnumItem(item)) => item.item.clone(),
        _ => String::new(),
    };
    let grid = match item("FlipbookLayout").as_str() {
        "Grid2x2" => 2,
        "Grid4x4" => 4,
        "Grid8x8" => 8,
        _ => return None,
    };
    let mode = match item("FlipbookMode").as_str() {
        "OneShot" => FlipbookMode::OneShot,
        "PingPong" => FlipbookMode::PingPong,
        "Random" => FlipbookMode::Random,
        _ => FlipbookMode::Loop,
    };
    let framerate = instance
        .property("FlipbookFramerate")
        .and_then(Value::as_number)
        .map_or(1.0, |n| n as f32);
    let start_random = instance
        .property("FlipbookStartRandom")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(Flipbook {
        grid,
        mode,
        framerate,
        start_random,
    })
}

fn spawn_shape(instance: &Instance) -> SpawnShape {
    let item = |name| match instance.property(name) {
        Some(Value::EnumItem(item)) => item.item.clone(),
        _ => String::new(),
    };
    let kind = match item("Shape").as_str() {
        "Sphere" => SpawnKind::Sphere,
        "Cylinder" => SpawnKind::Cylinder,
        "Disc" => SpawnKind::Disc,
        _ => SpawnKind::Box,
    };
    SpawnShape {
        kind,
        surface: item("ShapeStyle") == "Surface",
    }
}

/// `SpeedOverLife`'s values (its envelope isn't used) at the curve table's
/// times, if it's set.
fn speed_curve(instance: &Instance) -> Option<[f32; CURVE_ENTRIES]> {
    let sequence = instance
        .property("SpeedOverLife")
        .and_then(Value::as_number_sequence)
        .filter(|sequence| !sequence.is_empty())?;
    Some(std::array::from_fn(|entry| {
        let time = entry as f32 / (CURVE_ENTRIES - 1) as f32;
        sequence.sample(time).map_or(1.0, |(value, _)| value)
    }))
}

/// Every emitter's particles. See the module docs.
#[derive(Debug)]
pub struct Particles {
    emitters: HashMap<InstanceId, Emitter>,
    random: Random,
    /// The world's weather as it eases in, and its rain or snow.
    weather: WeatherState,
    /// Whether big emitters move on the GPU.
    gpu: bool,
    /// The next GPU emitter's key, new for each, so the renderer never
    /// shows one's particles for another's.
    next_key: u64,
}

impl Default for Particles {
    fn default() -> Self {
        Self::new()
    }
}

impl Particles {
    pub fn new() -> Self {
        Self {
            emitters: HashMap::new(),
            random: Random::default(),
            weather: WeatherState::default(),
            gpu: false,
            next_key: 1,
        }
    }

    /// Lets emitters with many particles move them on the GPU: true where
    /// the host's renderer runs compute shaders
    /// (`Capabilities::compute_shaders`). Off at first, so every emitter
    /// moves on the CPU, at most [`MAX_PER_EMITTER`] particles each.
    /// Turning it off takes the GPU's particles away.
    pub fn set_gpu(&mut self, on: bool) {
        self.gpu = on;
        if !on {
            for state in self.emitters.values_mut() {
                state.gpu = None;
            }
        }
    }

    /// Takes every particle away, such as when Play starts or stops. The
    /// weather starts again from the world's as it is.
    pub fn clear(&mut self) {
        self.emitters.clear();
        self.weather.reset();
    }

    /// How many particles are out. For those on the GPU, which aren't
    /// read back, it's how many should be alive.
    pub fn count(&self) -> usize {
        self.emitters
            .values()
            .map(|e| e.particles.len() + e.gpu.as_ref().map_or(0, GpuEmitter::alive))
            .sum()
    }

    /// How many emitters move their particles on the GPU.
    pub fn gpu_emitters(&self) -> usize {
        self.emitters.values().filter(|e| e.gpu.is_some()).count()
    }

    /// True while something moves: particles are out, an emitter in the
    /// Workspace is giving them off, or rain or snow falls or the weather
    /// is easing. Hosts keep drawing frames then.
    pub fn is_active(&self, world: &World) -> bool {
        self.count() > 0
            || emitters(world).any(|(_, instance)| Settings::read(instance).enabled)
            || self.weather.is_active(world)
    }

    /// Puts the weather into `list` (built from the same world): how wet
    /// and snowy surfaces are as it eases in, and its rain or snow around
    /// `list`'s camera. Before the first [`update`](Self::update) the
    /// list keeps the world's settled weather.
    pub fn add_weather(&self, list: &mut DrawList) {
        if let Some(look) = self.weather.look() {
            list.weather = look;
        }
        list.sprites
            .extend(self.weather.sprites(list.camera.position));
    }

    /// Gives off `count` particles from `emitter` now, whether or not it's
    /// Enabled (a script's `Emit`).
    pub fn emit(&mut self, world: &World, emitter: InstanceId, count: u32) {
        let Some(instance) = world.get(emitter) else {
            return;
        };
        if instance.class() != ClassName::ParticleEmitter {
            return;
        }
        let subs = subs_of(world, instance);
        let mut settings = Settings::read(instance);
        settings.cpu_only = cpu_only(world, instance, &settings, &subs);
        let Some(transform) = parent_transform(world, instance) else {
            return;
        };
        let mut state = self.emitters.remove(&emitter).unwrap_or_default();
        let mut events = Vec::new();
        // A big burst goes to the GPU, as do births of an emitter already
        // there. They're born at its next step.
        let burst = count as f32 >= GPU_PARTICLE_THRESHOLD && !settings.cpu_only;
        if self.gpu && (burst || self.on_gpu(&state, &settings)) {
            let gpu = self.gpu_state(&mut state);
            gpu.pending = gpu.pending.saturating_add(count).min(MAX_GPU_PARTICLES);
        } else {
            let count = (count as usize).min(MAX_BIRTHS_PER_FRAME);
            let inherit = state.velocity * settings.inheritance;
            for _ in 0..count {
                self.birth_with(
                    &mut state,
                    &settings,
                    transform,
                    inherit,
                    &subs,
                    &mut events,
                );
            }
        }
        self.emitters.insert(emitter, state);
        collisions::fire(self, world, events);
    }

    /// Gives off `event`'s particles from its sub-emitter, at the event's
    /// place, heading along the sub-emitter's own Direction turned by its
    /// Orientation, adding to `next` the events their births set off.
    fn give_off_at(&mut self, world: &World, event: Event, next: &mut Vec<Event>) {
        let Some(instance) = world.get(event.emitter) else {
            return;
        };
        let Some(mount) = crate::mount_of(world, instance) else {
            return;
        };
        let settings = Settings::read(instance);
        let subs = subs_of(world, instance);
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::splat(FREE_EMITTER_SIZE),
            mount.rotation(),
            event.position,
        );
        let inherit = event.velocity * settings.inheritance;
        let mut state = self.emitters.remove(&event.emitter).unwrap_or_default();
        for _ in 0..(event.count as usize).min(MAX_BIRTHS_PER_FRAME) {
            self.birth_with(&mut state, &settings, transform, inherit, &subs, next);
        }
        self.emitters.insert(event.emitter, state);
    }

    /// True if `state`'s births belong on the GPU: it's allowed, and the
    /// emitter keeps enough out, or more than half that once there.
    fn on_gpu(&self, state: &Emitter, settings: &Settings) -> bool {
        let out = settings.rate * settings.lifetime;
        let threshold = if state.gpu.is_some() {
            GPU_PARTICLE_THRESHOLD / 2.0
        } else {
            GPU_PARTICLE_THRESHOLD
        };
        self.gpu && !settings.cpu_only && out >= threshold
    }

    /// `state`'s GPU particles, made if it has none yet.
    fn gpu_state<'a>(&mut self, state: &'a mut Emitter) -> &'a mut GpuEmitter {
        state.gpu.get_or_insert_with(|| {
            let key = self.next_key;
            self.next_key += 1;
            GpuEmitter::new(key)
        })
    }

    /// Moves every particle on by `seconds`, lets old ones die, and gives
    /// off new ones from each Enabled emitter by its Rate. The weather
    /// eases on and its rain or snow falls.
    pub fn update(&mut self, world: &World, seconds: f32) {
        self.update_against(world, seconds, &mut |_, paths| vec![None; paths.len()]);
    }

    /// [`update`](Self::update), with the particles of emitters that
    /// Collide landing on what `surfaces` finds (roadmap R17.4).
    pub fn update_against(&mut self, world: &World, seconds: f32, surfaces: &mut Surfaces) {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.weather.update(world, seconds, &mut self.random);
        let live: Vec<Live> = emitters(world)
            .map(|(id, instance)| {
                let subs = subs_of(world, instance);
                let mut settings = Settings::read(instance);
                settings.cpu_only = cpu_only(world, instance, &settings, &subs);
                let mount = crate::mount_of(world, instance);
                Live {
                    id,
                    settings,
                    transform: parent_transform(world, instance),
                    own_part: mount.and_then(|mount| mount.part()),
                    sub: is_sub(world, instance),
                    subs,
                }
            })
            .collect();
        self.emitters
            .retain(|id, _| live.iter().any(|live| live.id == *id));
        let wind = global_wind(world);
        let mut rays = MAX_SURFACE_RAYS;
        let mut events = Vec::new();
        for Live {
            id,
            settings,
            transform,
            own_part,
            sub,
            subs,
        } in live
        {
            let mut state = self.emitters.remove(&id).unwrap_or_default();
            // Older particles first: move them, and let them die.
            // Drag slows particles toward the air's speed: still air, or
            // the wind for emitters it affects.
            let slow = 1.0 / (1.0 + settings.drag * seconds);
            let air = if settings.wind_affects_drag {
                wind
            } else {
                Vec3::ZERO
            };
            let mut steps = Vec::new();
            for particle in &mut state.particles {
                let velocity = particle.velocity + settings.acceleration * seconds;
                particle.velocity = air + (velocity - air) * slow;
                // SpeedOverLife scales how far they go, at the age they
                // start the step at, as on the GPU.
                let speed = settings.speed_curve.as_ref().map_or(1.0, |table| {
                    curve_at(table, particle.age / particle.life.max(1e-6))
                });
                let step = particle.velocity * seconds * speed;
                if settings.collides {
                    steps.push(step);
                } else {
                    particle.position += step;
                }
            }
            if settings.collides {
                collisions::move_colliding(
                    &mut state.particles,
                    &steps,
                    seconds,
                    settings.landing,
                    own_part,
                    surfaces,
                    &mut rays,
                    &subs,
                    &mut events,
                );
            }
            for particle in &mut state.particles {
                particle.age += seconds;
                if particle.age >= particle.life {
                    events_on(
                        &subs,
                        EmitOn::Death,
                        particle.position,
                        particle.velocity,
                        &mut events,
                    );
                }
            }
            state.particles.retain(|p| p.age < p.life);
            // How fast the part moves, from where it was at the last update
            // (VelocityInheritance, roadmap R17.2b).
            let at = transform.map(|transform| transform.w_axis.truncate());
            state.velocity = match (at, state.last_position) {
                (Some(now), Some(then)) if seconds > 0.0 => (now - then) / seconds,
                (Some(_), Some(_)) => state.velocity,
                _ => Vec3::ZERO,
            };
            state.last_position = at;
            let inherit = state.velocity * settings.inheritance;
            let mut on_gpu = self.on_gpu(&state, &settings);
            let mut gpu_births = 0;
            // A sub-emitter gives off only at its parent's events.
            match (settings.enabled && !sub, transform) {
                (true, Some(transform)) => {
                    state.owed += settings.rate * seconds;
                    let owed = state.owed.floor();
                    state.owed -= owed;
                    let burst =
                        settings.bursts_between(&mut state.cycle, seconds, &mut self.random);
                    // A big burst goes to the GPU, as a big Emit does.
                    if self.gpu && burst as f32 >= GPU_PARTICLE_THRESHOLD {
                        on_gpu = true;
                    }
                    let births = (owed as u32).saturating_add(burst);
                    if on_gpu {
                        gpu_births = births.min(MAX_GPU_PARTICLES);
                    } else {
                        for _ in 0..(births as usize).min(MAX_BIRTHS_PER_FRAME) {
                            self.birth_with(
                                &mut state,
                                &settings,
                                transform,
                                inherit,
                                &subs,
                                &mut events,
                            );
                        }
                    }
                }
                _ => {
                    state.owed = 0.0;
                    state.cycle = 0.0;
                }
            }
            if on_gpu || state.gpu.is_some() {
                let gpu = self.gpu_state(&mut state);
                step_gpu(
                    gpu,
                    &settings,
                    (transform, air, inherit),
                    seconds,
                    gpu_births,
                );
                // Off the GPU once its last particles there have died, unless
                // it's still giving them off there.
                let giving = on_gpu && settings.enabled && transform.is_some();
                if !giving && gpu.alive() == 0 && gpu.pending == 0 {
                    state.gpu = None;
                }
            }
            if !state.particles.is_empty() || settings.enabled || state.gpu.is_some() {
                self.emitters.insert(id, state);
            }
        }
        collisions::fire(self, world, events);
    }

    /// Puts every particle into `list` (built from the same world): those
    /// on the CPU as [`sprites`](Self::sprites), and those on the GPU as
    /// [`DrawList::particle_emitters`].
    pub fn draw(&self, list: &mut DrawList, world: &World, assets: &Assets) {
        list.sprites.extend(self.sprites(world, assets));
        for (&id, state) in &self.emitters {
            let (Some(gpu), Some(instance)) = (&state.gpu, world.get(id)) else {
                continue;
            };
            let look = Look::read(instance, assets);
            list.particle_emitters.push(ParticleEmitter {
                look: ParticleLook {
                    size: [look.size.0, look.size.1],
                    color: [rgb(look.color.0), rgb(look.color.1)],
                    transparency: [look.transparency.0, look.transparency.1],
                    rotation_degrees: look.rotation,
                    spin_degrees: look.spin,
                    brightness: look.brightness,
                    light_emission: look.light_emission,
                    texture: look.texture,
                    curves: look.curves,
                    flipbook: look.flipbook,
                },
                ..gpu.draw.clone()
            });
        }
    }

    /// The sprites to draw for every particle on the CPU now. Textures
    /// named by emitters come from `assets`.
    pub fn sprites(&self, world: &World, assets: &Assets) -> Vec<Sprite> {
        let mut sprites = Vec::with_capacity(self.count());
        for (&id, state) in &self.emitters {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let look = Look::read(instance, assets);
            for particle in &state.particles {
                let t = (particle.age / particle.life.max(1e-4)).clamp(0.0, 1.0);
                let (size, transparency, color) = match &look.curves {
                    // The same table and spreads as on the GPU; the life
                    // stays the same over the particle's life.
                    Some(curves) => {
                        curves.sample(t, ParticleCurves::spreads(particle.life.to_bits()))
                    }
                    None => (
                        lerp(look.size, t).max(0.0),
                        lerp(look.transparency, t).clamp(0.0, 1.0),
                        [
                            lerp((look.color.0.r, look.color.1.r), t),
                            lerp((look.color.0.g, look.color.1.g), t),
                            lerp((look.color.0.b, look.color.1.b), t),
                        ],
                    ),
                };
                sprites.push(Sprite {
                    position: particle.position,
                    size,
                    rotation_degrees: look.rotation + look.spin * particle.age,
                    color,
                    opacity: 1.0 - transparency,
                    brightness: look.brightness,
                    light_emission: look.light_emission,
                    texture: look.texture.clone(),
                    // The same frame as on the GPU, from the same kind of
                    // seed (its life; the GPU mixes in its slot).
                    frame: look.flipbook.map(|book| FlipbookFrame {
                        grid: book.grid,
                        frame: book.frame(particle.age, particle.life, particle.life.to_bits()),
                    }),
                });
            }
        }
        sprites
    }

    /// [`birth`](Self::birth), adding the events it sets off for the
    /// emitter's Birth sub-emitters (`subs`) to `events`.
    fn birth_with(
        &mut self,
        state: &mut Emitter,
        settings: &Settings,
        transform: Mat4,
        inherit: Vec3,
        subs: &[Sub],
        events: &mut Vec<Event>,
    ) {
        if let Some(born) = self.birth(state, settings, transform, inherit) {
            events_on(subs, EmitOn::Birth, born.position, born.velocity, events);
        }
    }

    /// Adds one particle to `state`, somewhere in the part's spawn shape,
    /// heading out along the emitter's direction within its spread, and
    /// returns it; `None` if the emitter is full.
    fn birth(
        &mut self,
        state: &mut Emitter,
        settings: &Settings,
        transform: Mat4,
        inherit: Vec3,
    ) -> Option<Particle> {
        if state.particles.len() >= MAX_PER_EMITTER {
            return None;
        }
        let random = [self.random.next(), self.random.next(), self.random.next()];
        let (scale, rotation, _) = transform.to_scale_rotation_translation();
        let inside = settings.shape.point(random, scale.abs());
        let position = transform.transform_point3(inside);
        // A round shape heads away from its middle (R17.2b), a box along
        // the direction; the same sum as particles.wgsl's `born`.
        let forward = settings
            .shape
            .outward(inside)
            .map(|out| transform.transform_vector3(out))
            .filter(|out| out.length_squared() > 0.0)
            .unwrap_or(rotation * settings.direction)
            .normalize_or(Vec3::Y);
        let heading = self.within_cone(forward, settings.spread.to_radians() / 2.0);
        let life = settings.lifetime * (0.8 + 0.4 * self.random.next());
        let flip = match settings.in_out {
            ShapeInOut::Outward => 1.0,
            ShapeInOut::Inward => -1.0,
            ShapeInOut::InAndOut if self.random.next() < 0.5 => -1.0,
            ShapeInOut::InAndOut => 1.0,
        };
        let particle = Particle {
            position,
            velocity: heading * flip * settings.speed + inherit,
            age: 0.0,
            life,
            off_surface: f32::MAX,
        };
        state.particles.push(particle);
        Some(particle)
    }

    /// A random direction at most `half_angle` radians from `forward`.
    fn within_cone(&mut self, forward: Vec3, half_angle: f32) -> Vec3 {
        let cos_most = half_angle.min(std::f32::consts::PI).cos();
        let z = 1.0 - self.random.next() * (1.0 - cos_most);
        let around = self.random.next() * TAU;
        let across = (1.0 - z * z).max(0.0).sqrt();
        let (u, v) = forward.any_orthonormal_pair();
        forward * z + (u * around.cos() + v * around.sin()) * across
    }
}

/// The emitter's size, transparency, color and speed over life as one
/// table, if any of `SizeOverLife`, `TransparencyOverLife`, `ColorOverLife`
/// and `SpeedOverLife` (`speed`) is set; the ones that aren't blend their
/// start and end values, or keep the speed.
fn curves_of(
    instance: &Instance,
    size: (f32, f32),
    colors: (Color3, Color3),
    transparency: (f32, f32),
    speed: Option<[f32; CURVE_ENTRIES]>,
) -> Option<Arc<ParticleCurves>> {
    let numbers = |name| {
        instance
            .property(name)
            .and_then(Value::as_number_sequence)
            .filter(|sequence| !sequence.is_empty())
    };
    let sizes = numbers("SizeOverLife");
    let transparencies = numbers("TransparencyOverLife");
    let color_curve = instance
        .property("ColorOverLife")
        .and_then(Value::as_color_sequence)
        .filter(|sequence| !sequence.is_empty());
    if sizes.is_none() && transparencies.is_none() && color_curve.is_none() && speed.is_none() {
        return None;
    }
    let mut curves = ParticleCurves::from_fn(|time| {
        let (size, size_envelope) = sizes
            .and_then(|sequence| sequence.sample(time))
            .unwrap_or((lerp(size, time), 0.0));
        let (seen, seen_envelope) = transparencies
            .and_then(|sequence| sequence.sample(time))
            .unwrap_or((lerp(transparency, time), 0.0));
        let color = color_curve
            .and_then(|sequence| sequence.sample(time))
            .map(rgb)
            .unwrap_or([
                lerp((colors.0.r, colors.1.r), time),
                lerp((colors.0.g, colors.1.g), time),
                lerp((colors.0.b, colors.1.b), time),
            ]);
        ([size, size_envelope, seen, seen_envelope], color)
    });
    if let Some(speed) = speed {
        curves.speed = speed;
    }
    Some(Arc::new(curves))
}

/// A small random number generator (xorshift), so particles spread
/// differently every time without a dependency.
#[derive(Debug)]
pub(crate) struct Random(u64);

impl Default for Random {
    fn default() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
}

impl Random {
    /// A random number from 0 up to 1.
    pub(crate) fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// How an emitter's particles look over their lives.
struct Look {
    size: (f32, f32),
    color: (Color3, Color3),
    transparency: (f32, f32),
    rotation: f32,
    spin: f32,
    light_emission: f32,
    brightness: f32,
    texture: Option<Arc<dyn TextureSource>>,
    /// While any of the OverLife sequences is set (roadmap R17.1).
    curves: Option<Arc<ParticleCurves>>,
    /// While FlipbookLayout cuts a Texture into frames (roadmap R17.2c).
    flipbook: Option<Flipbook>,
}

impl Look {
    fn read(instance: &Instance, assets: &Assets) -> Self {
        let number = |name, default: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map_or(default, |n| n as f32)
        };
        let color = |name| {
            instance
                .property(name)
                .and_then(Value::as_color3)
                .unwrap_or(Color3::new(1.0, 1.0, 1.0))
        };
        let path = instance
            .property("Texture")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // A texture that can't be loaded is reported once by `Assets`, and
        // the built-in picture shows instead.
        let loaded = assets.texture(path).map(|image| image_texture(&image));
        // Only a Texture has frames: the built-in pictures are whole.
        let flipbook = loaded.as_ref().and_then(|_| flipbook(instance));
        let texture = loaded.or_else(|| {
            let kind = instance
                .property("ParticleType")
                .and_then(Value::as_particle_type)
                .unwrap_or(ParticleType::Circle);
            Some(built_in(kind))
        });
        let size = (number("Size", 1.0), number("SizeEnd", 1.0));
        let colors = (color("Color"), color("ColorEnd"));
        let transparency = (number("Transparency", 0.0), number("TransparencyEnd", 1.0));
        let curves = curves_of(instance, size, colors, transparency, speed_curve(instance));
        Self {
            size,
            color: colors,
            transparency,
            curves,
            rotation: number("Rotation", 0.0),
            spin: number("RotSpeed", 0.0),
            light_emission: number("LightEmission", 0.0).clamp(0.0, 1.0),
            brightness: number("Brightness", 1.0).max(0.0),
            texture,
            flipbook,
        }
    }
}

/// The Workspace's `GlobalWind`, in studs a second.
pub(crate) fn global_wind(world: &World) -> Vec3 {
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("GlobalWind"))
        .and_then(Value::as_vector3)
        .map(|wind| Vec3::new(wind.x, wind.y, wind.z))
        // A NaN or infinite wind would send every particle away for good.
        .filter(|wind| wind.is_finite())
        .unwrap_or(Vec3::ZERO)
}

/// Moves `gpu` on a step: its old births age, `births` more are born (with
/// those `emit` asked for) inside the part at `transform`, if it's in one,
/// and the renderer is told the emitter's settings. Drag slows its
/// particles toward `air`.
fn step_gpu(
    gpu: &mut GpuEmitter,
    settings: &Settings,
    (transform, air, inherit): (Option<Mat4>, Vec3, Vec3),
    seconds: f32,
    births: u32,
) {
    for batch in &mut gpu.batches {
        batch.age += seconds;
    }
    gpu.batches.retain(|batch| batch.age < batch.lifetime * 1.2);
    let births = match transform {
        Some(_) => births.saturating_add(gpu.pending).min(MAX_GPU_PARTICLES),
        None => 0,
    };
    gpu.pending = 0;
    if births > 0 {
        gpu.batches.push_back(Batch {
            age: 0.0,
            count: births,
            lifetime: settings.lifetime,
        });
    }
    let draw = &mut gpu.draw;
    draw.step += 1;
    draw.seconds = seconds;
    draw.born = draw.born.wrapping_add(draw.births);
    draw.births = births;
    // Room for all its particles to live out their longest lives. More
    // room starts it again, empty, so it only ever grows.
    let out = (settings.rate * settings.lifetime * 1.2)
        .ceil()
        .min(MAX_GPU_PARTICLES as f32);
    let needed = (out as u32)
        .saturating_add(births)
        .clamp(MIN_GPU_CAPACITY, MAX_GPU_PARTICLES)
        .next_power_of_two();
    draw.capacity = draw.capacity.max(needed);
    if let Some(transform) = transform {
        let (_, rotation, _) = transform.to_scale_rotation_translation();
        draw.spawn = transform;
        draw.direction = (rotation * settings.direction).normalize_or(Vec3::Y);
    }
    draw.spread = settings.spread.to_radians() / 2.0;
    draw.speed = settings.speed;
    draw.lifetime = settings.lifetime;
    draw.acceleration = settings.acceleration;
    draw.drag = settings.drag;
    draw.air = air;
    draw.shape = settings.shape;
    draw.in_out = settings.in_out;
    draw.inherit = inherit;
}

fn rgb(color: Color3) -> [f32; 3] {
    [color.r, color.g, color.b]
}

fn lerp((from, to): (f32, f32), t: f32) -> f32 {
    from + (to - from) * t
}

/// Every ParticleEmitter in the Workspace, except those out of the world
/// as the draw list has them: in a Tool put away in an Inventory, or in the
/// parts kept inside a UnionOperation, whose parts aren't drawn.
fn emitters(world: &World) -> impl Iterator<Item = (InstanceId, &Instance)> {
    world
        .all_below(world.workspace(), |class| {
            class == ClassName::ParticleEmitter
        })
        .into_iter()
        .filter(move |&id| !axcenity_core::csg::out_of_world(world, id))
        .filter_map(move |id| Some((id, world.get(id)?)))
}

/// An emitter in the Workspace, as an update reads it.
struct Live {
    id: InstanceId,
    settings: Settings,
    transform: Option<Mat4>,
    /// The part it's in, which its colliding particles pass by.
    own_part: Option<InstanceId>,
    /// True if it's a sub-emitter, giving off only at its parent's events.
    sub: bool,
    /// Its own sub-emitters.
    subs: Vec<Sub>,
}

/// True if `instance`'s particles must stay on the CPU: it Collides, has
/// sub-emitters or is one (roadmap R17.4).
fn cpu_only(world: &World, instance: &Instance, settings: &Settings, subs: &[Sub]) -> bool {
    settings.collides || !subs.is_empty() || is_sub(world, instance)
}

/// How far across the box a free-standing emitter gives off particles
/// from, in studs: nearly a point.
const FREE_EMITTER_SIZE: f32 = 0.1;

/// Where the emitter gives off particles from, as the transform that
/// stretches the 1×1×1 cube to fill it: its parent part, or a small box at
/// its own Position and Orientation when it isn't in a part (see
/// [`crate::Mount`]).
fn parent_transform(world: &World, emitter: &Instance) -> Option<Mat4> {
    Some(match crate::mount_of(world, emitter)? {
        crate::Mount::Part { transform, .. } => transform,
        crate::Mount::Free { position, rotation } => Mat4::from_scale_rotation_translation(
            Vec3::splat(FREE_EMITTER_SIZE),
            rotation,
            position,
        ),
    })
}

/// A built-in particle picture.
struct BuiltIn {
    key: u64,
    rgba: Vec<u8>,
}

impl TextureSource for BuiltIn {
    fn key(&self) -> u64 {
        self.key
    }
    fn size(&self) -> (u32, u32) {
        (PICTURE, PICTURE)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// The picture for `kind`, drawn the first time it's asked for.
pub fn built_in(kind: ParticleType) -> Arc<dyn TextureSource> {
    static PICTURES: OnceLock<Vec<Arc<BuiltIn>>> = OnceLock::new();
    let pictures = PICTURES.get_or_init(|| {
        ParticleType::ALL
            .iter()
            .enumerate()
            .map(|(i, &kind)| {
                Arc::new(BuiltIn {
                    // Far from the keys `assets` hands out, which count up from 1.
                    key: u64::MAX - i as u64,
                    rgba: draw(kind),
                })
            })
            .collect()
    });
    let index = ParticleType::ALL
        .iter()
        .position(|&k| k == kind)
        .expect("every kind is in ALL");
    pictures[index].clone()
}

/// The picture raindrops show: a thin streak down the middle, fading at
/// its ends, so a drop looks like it's falling fast.
pub(crate) fn rain_streak() -> Arc<dyn TextureSource> {
    static PICTURE_OF_RAIN: OnceLock<Arc<BuiltIn>> = OnceLock::new();
    PICTURE_OF_RAIN
        .get_or_init(|| {
            let mut rgba = Vec::with_capacity((PICTURE * PICTURE * 4) as usize);
            for row in 0..PICTURE {
                for column in 0..PICTURE {
                    let x = (column as f32 + 0.5) / PICTURE as f32 - 0.5;
                    let y = (row as f32 + 0.5) / PICTURE as f32 - 0.5;
                    let across = (1.0 - x.abs() / 0.03).clamp(0.0, 1.0);
                    let along = (1.0 - (y / 0.5).powi(2)).clamp(0.0, 1.0);
                    let alpha = across * along;
                    rgba.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
                }
            }
            // Below the particle types' keys, far from the ones `assets`
            // hands out.
            Arc::new(BuiltIn {
                key: u64::MAX - 64,
                rgba,
            })
        })
        .clone()
}

/// Draws `kind`'s picture: white, with its shape in the alpha channel.
fn draw(kind: ParticleType) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((PICTURE * PICTURE * 4) as usize);
    for row in 0..PICTURE {
        for column in 0..PICTURE {
            // From -0.5 to 0.5 across, and down the picture.
            let x = (column as f32 + 0.5) / PICTURE as f32 - 0.5;
            let y = (row as f32 + 0.5) / PICTURE as f32 - 0.5;
            let alpha = shape(kind, x, y).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
        }
    }
    rgba
}

/// How solid `kind`'s picture is at (`x`, `y`), each from -0.5 to 0.5 (`y`
/// goes down).
fn shape(kind: ParticleType, x: f32, y: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    let soft = |edge: f32, width: f32| ((edge - r) / width).clamp(0.0, 1.0);
    match kind {
        ParticleType::Circle => soft(0.45, 0.15),
        ParticleType::Spark => {
            let core = (-(r * r) * 120.0).exp();
            let glow = 0.55 * (-(r * r) * 18.0).exp();
            let streaks = (-(y.abs()) * 70.0).exp() * (-(x.abs()) * 7.0).exp()
                + (-(x.abs()) * 70.0).exp() * (-(y.abs()) * 7.0).exp();
            (core + glow + 0.6 * streaks) * soft(0.5, 0.1)
        }
        ParticleType::Smoke => {
            let blob = (1.0 - r / 0.48).clamp(0.0, 1.0).powf(1.4);
            blob * (0.55 + 0.45 * noise(x * 6.0, y * 6.0))
        }
        ParticleType::Fire => {
            // A flame: wide at the bottom, coming to a point at the top.
            let height = (y + 0.5).clamp(0.0, 1.0);
            let width = 0.06 + 0.3 * height.powf(0.8);
            let across = x / width;
            let up = (y - 0.12) / 0.4;
            let d = (across * across + up * up).sqrt();
            let flame = (1.0 - d).clamp(0.0, 1.0).powf(0.8);
            flame * (0.7 + 0.3 * noise(x * 8.0, y * 5.0))
        }
        ParticleType::Star => {
            let angle = y.atan2(x) + std::f32::consts::FRAC_PI_2;
            let point = (0.5 + 0.5 * (5.0 * angle).cos()).powf(2.5);
            let edge = 0.18 + 0.27 * point;
            ((edge - r) / 0.03).clamp(0.0, 1.0)
        }
        ParticleType::Magic => {
            let arms = (-(x.abs()) * 50.0).exp() * (-(y.abs()) * 6.0).exp()
                + (-(y.abs()) * 50.0).exp() * (-(x.abs()) * 6.0).exp();
            let core = (-(r * r) * 90.0).exp();
            (arms + core + 0.25 * (-(r * r) * 25.0).exp()) * soft(0.5, 0.1)
        }
    }
}

/// Smooth noise from 0 to 1: random values on whole-number points,
/// blended between them.
fn noise(x: f32, y: f32) -> f32 {
    let hash = |x: i32, y: i32| {
        let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 13;
        (h & 0xffff) as f32 / 65535.0
    };
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let (xi, yi) = (x0 as i32, y0 as i32);
    let top = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
    let bottom = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
    top + (bottom - top) * sy
}

#[cfg(test)]
mod tests {
    use axcenity_core::Vector3;

    use super::*;

    fn world_with_emitter(rate: f64) -> (World, InstanceId, InstanceId) {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(0.0, 10.0, 0.0)),
            )
            .unwrap();
        let emitter = world.create(ClassName::ParticleEmitter).unwrap();
        world.set_parent(emitter, Some(part)).unwrap();
        world
            .set_property(emitter, "Rate", Value::Number(rate))
            .unwrap();
        (world, part, emitter)
    }

    #[test]
    fn an_emitter_gives_off_its_rate_and_particles_die_of_old_age() {
        let (world, _, emitter) = world_with_emitter(30.0);
        let mut particles = Particles::new();
        for _ in 0..30 {
            particles.update(&world, 1.0 / 60.0);
        }
        // Half a second at 30 a second.
        assert_eq!(particles.count(), 15);
        assert!(particles.is_active(&world));

        // Switched off, the ones out live on, then all die (at most 2.4 s).
        let mut world = world;
        world
            .set_property(emitter, "Enabled", Value::Bool(false))
            .unwrap();
        particles.update(&world, 0.1);
        assert_eq!(particles.count(), 15);
        for _ in 0..20 {
            particles.update(&world, 0.2);
        }
        assert_eq!(particles.count(), 0);
        assert!(!particles.is_active(&world));
    }

    /// R17.1: set, the OverLife sequences shape particles in place of the
    /// start and end values; an envelope spreads particles apart, each
    /// keeping its own place in it; cleared, the pairs come back.
    /// R17.2c: with FlipbookLayout set and a Texture, each sprite shows a
    /// frame for its age (here OneShot: a quarter of its life each); the
    /// built-in pictures and None show the whole picture.
    #[test]
    fn particles_show_their_flipbook_frame_of_a_texture() {
        const IMAGE: &str = "assets/textures/flip.png";
        let folder = std::env::temp_dir().join(format!("axcenity-flipbook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        image::RgbaImage::new(4, 4)
            .save(folder.join(IMAGE))
            .unwrap();
        let assets = Assets::new(&folder);
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let set = |world: &mut World, name: &str, value: Value| {
            world.set_property(emitter, name, value).unwrap();
        };
        let item = |enum_name: &str, item: &str| {
            Value::EnumItem(axcenity_core::EnumItem {
                enum_name: enum_name.to_owned(),
                item: item.to_owned(),
            })
        };
        set(&mut world, "Lifetime", Value::Number(1.0));
        set(
            &mut world,
            "FlipbookLayout",
            item("ParticleFlipbookLayout", "Grid2x2"),
        );
        set(
            &mut world,
            "FlipbookMode",
            item("ParticleFlipbookMode", "OneShot"),
        );
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 20);
        let frames = |particles: &Particles, world: &World| -> Vec<Option<FlipbookFrame>> {
            particles
                .sprites(world, &assets)
                .iter()
                .map(|sprite| sprite.frame)
                .collect()
        };
        // Without a Texture, the built-in picture is whole.
        assert!(frames(&particles, &world).iter().all(Option::is_none));
        set(&mut world, "Texture", Value::String(IMAGE.into()));
        let born = frames(&particles, &world);
        assert!(
            born.iter()
                .all(|frame| *frame == Some(FlipbookFrame { grid: 2, frame: 0 })),
            "{born:?}"
        );
        // Lives are 0.8 to 1.2 seconds: at 0.5 s, 42% to 63% of the way,
        // the second or third frame.
        particles.update(&world, 0.25);
        particles.update(&world, 0.25);
        let later = frames(&particles, &world);
        assert!(
            later.iter().all(|frame| matches!(
                frame,
                Some(FlipbookFrame {
                    grid: 2,
                    frame: 1 | 2
                })
            )),
            "{later:?}"
        );
        set(
            &mut world,
            "FlipbookLayout",
            item("ParticleFlipbookLayout", "None"),
        );
        assert!(frames(&particles, &world).iter().all(Option::is_none));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn particles_follow_their_curves_over_life_and_the_pairs_when_unset() {
        use axcenity_core::{ColorSequence, NumberKeypoint, NumberSequence};
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let set = |world: &mut World, name: &str, value: Value| {
            world.set_property(emitter, name, value).unwrap();
        };
        set(&mut world, "Lifetime", Value::Number(1.0));
        // Grows from 1 to 5 by half its life, then shrinks to 0, give or
        // take 0.5 in the middle.
        let sizes = NumberSequence::new(vec![
            NumberKeypoint::new(0.0, 1.0, 0.0),
            NumberKeypoint::new(0.5, 5.0, 0.5),
            NumberKeypoint::new(1.0, 0.0, 0.0),
        ])
        .unwrap();
        set(&mut world, "SizeOverLife", Value::NumberSequence(sizes));
        let green = Color3::new(0.0, 1.0, 0.0);
        set(
            &mut world,
            "ColorOverLife",
            Value::ColorSequence(ColorSequence::ramp(green, green)),
        );
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 40);
        let assets = Assets::default();
        let born = particles.sprites(&world, &assets);
        assert!(born.iter().all(|sprite| (sprite.size - 1.0).abs() < 1e-4));
        assert!(born.iter().all(|sprite| sprite.color == [0.0, 1.0, 0.0]));
        // Transparency isn't set: it still blends 0 to 1 (TransparencyEnd).
        assert!(
            born.iter()
                .all(|sprite| (sprite.opacity - 1.0).abs() < 1e-4)
        );
        // Lives are 0.8 to 1.2 seconds: at 0.5 s every particle is between
        // 40% and 63% of its life, so near its biggest (a step is at most
        // a quarter of a second).
        particles.update(&world, 0.25);
        particles.update(&world, 0.25);
        let grown = particles.sprites(&world, &assets);
        assert!(
            grown
                .iter()
                .all(|sprite| sprite.size > 3.0 && sprite.size < 5.6)
        );
        let sizes: Vec<f32> = grown.iter().map(|sprite| sprite.size).collect();
        let spread = sizes.iter().cloned().fold(f32::MIN, f32::max)
            - sizes.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.3, "the envelope spreads them: {sizes:?}");
        // Each keeps its place in the envelope: drawing again changes nothing.
        let again: Vec<f32> = particles
            .sprites(&world, &assets)
            .iter()
            .map(|sprite| sprite.size)
            .collect();
        assert_eq!(sizes, again);
        set(
            &mut world,
            "SizeOverLife",
            Value::NumberSequence(NumberSequence::default()),
        );
        set(
            &mut world,
            "ColorOverLife",
            Value::ColorSequence(ColorSequence::default()),
        );
        assert!(
            Look::read(world.get(emitter).unwrap(), &assets)
                .curves
                .is_none()
        );
        let unset = particles.sprites(&world, &assets);
        assert!(unset.iter().all(|sprite| (sprite.size - 1.0).abs() < 1e-4));
    }

    #[test]
    fn particles_start_inside_the_part_and_move_along_the_direction() {
        let (mut world, _, emitter) = world_with_emitter(0.0);
        world
            .set_property(emitter, "SpreadAngle", Value::Number(0.0))
            .unwrap();
        world
            .set_property(emitter, "Speed", Value::Number(10.0))
            .unwrap();
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 50);
        let start = particles.sprites(&world, &Assets::default());
        assert_eq!(start.len(), 50);
        for sprite in &start {
            // The default part is 4 × 1 × 2, centered 10 up.
            assert!(sprite.position.x.abs() <= 2.0 && (sprite.position.y - 10.0).abs() <= 0.5);
        }
        particles.update(&world, 0.1);
        let later = particles.sprites(&world, &Assets::default());
        let rise: f32 = later.iter().map(|s| s.position.y).sum::<f32>() / 50.0
            - start.iter().map(|s| s.position.y).sum::<f32>() / 50.0;
        assert!((rise - 1.0).abs() < 1e-3, "{rise}");
    }

    /// R17.2: a sphere's surface stretched over the part, and a speed curve
    /// at half: particles start on the part's ellipsoid and move half as far.
    #[test]
    fn particles_are_born_in_their_shape_and_move_at_their_speed_curve() {
        use axcenity_core::{EnumItem, NumberSequence};
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let mut set = |name: &str, value: Value| {
            world.set_property(emitter, name, value).unwrap();
        };
        set("SpreadAngle", Value::Number(0.0));
        set("Speed", Value::Number(10.0));
        let item = |enum_name: &str, item: &str| {
            Value::EnumItem(EnumItem {
                enum_name: enum_name.to_owned(),
                item: item.to_owned(),
            })
        };
        set("Shape", item("ParticleEmitterShape", "Sphere"));
        set("ShapeStyle", item("ParticleEmitterShapeStyle", "Surface"));
        set(
            "SpeedOverLife",
            Value::NumberSequence(NumberSequence::constant(0.5)),
        );
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 50);
        let start = particles.sprites(&world, &Assets::default());
        assert_eq!(start.len(), 50);
        for sprite in &start {
            // The default part is 4 × 1 × 2, centered 10 up.
            let p = sprite.position - Vec3::new(0.0, 10.0, 0.0);
            let on = (p / Vec3::new(2.0, 0.5, 1.0)).length();
            assert!((on - 1.0).abs() < 1e-3, "{p} is {on} of the way out");
        }
        particles.update(&world, 0.1);
        let later = particles.sprites(&world, &Assets::default());
        // Out from the middle (R17.2b) at half of 10 studs a second.
        for (from, to) in start.iter().zip(&later) {
            let moved = from.position.distance(to.position);
            assert!((moved - 0.5).abs() < 1e-3, "{moved}");
        }
    }

    fn enum_value(enum_name: &str, item: &str) -> Value {
        Value::EnumItem(axcenity_core::EnumItem {
            enum_name: enum_name.to_owned(),
            item: item.to_owned(),
        })
    }

    /// R17.2b: bursts fire at their times of each loop of Duration, again
    /// every loop, and start over when the emitter is enabled again.
    #[test]
    fn bursts_fire_at_their_times_once_a_loop() {
        use axcenity_core::{NumberKeypoint, NumberSequence};
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let bursts = NumberSequence::new(vec![
            NumberKeypoint::new(0.0, 10.0, 0.0),
            NumberKeypoint::new(0.5, 5.0, 0.0),
            NumberKeypoint::new(1.0, 99.0, 0.0),
        ])
        .unwrap();
        world
            .set_property(emitter, "Bursts", Value::NumberSequence(bursts))
            .unwrap();
        world
            .set_property(emitter, "Lifetime", Value::Number(100.0))
            .unwrap();
        let mut particles = Particles::new();
        let step = 1.0 / 16.0;
        particles.update(&world, step);
        assert_eq!(particles.count(), 10, "the burst at the start");
        for _ in 1..8 {
            particles.update(&world, step);
        }
        assert_eq!(particles.count(), 10, "nothing until half way");
        particles.update(&world, step);
        assert_eq!(particles.count(), 15);
        // Time 1 is the next loop's start: its 99 never fire.
        for _ in 9..32 {
            particles.update(&world, step);
        }
        assert_eq!(particles.count(), 30, "two whole loops");
        particles.update(&world, step);
        assert_eq!(particles.count(), 40, "the third begins");
        // Off and on again: the loop starts over.
        world
            .set_property(emitter, "Enabled", Value::Bool(false))
            .unwrap();
        particles.update(&world, step);
        world
            .set_property(emitter, "Enabled", Value::Bool(true))
            .unwrap();
        particles.update(&world, step);
        assert_eq!(particles.count(), 50);
    }

    /// R17.2b: a sphere's particles head away from its middle (Outward),
    /// toward it (Inward) or either way (InAndOut).
    #[test]
    fn round_shapes_head_out_from_their_middle_in_or_both() {
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let mut set = |name: &str, value: Value| {
            world.set_property(emitter, name, value).unwrap();
        };
        set("SpreadAngle", Value::Number(0.0));
        set("Speed", Value::Number(10.0));
        set("Shape", enum_value("ParticleEmitterShape", "Sphere"));
        set(
            "ShapeStyle",
            enum_value("ParticleEmitterShapeStyle", "Surface"),
        );
        let middle = Vec3::new(0.0, 10.0, 0.0);
        let headings = |world: &World| -> Vec<f32> {
            let mut particles = Particles::new();
            particles.emit(world, emitter, 40);
            particles.emitters[&emitter]
                .particles
                .iter()
                .map(|p| {
                    assert!((p.velocity.length() - 10.0).abs() < 1e-3);
                    p.velocity
                        .normalize()
                        .dot((p.position - middle).normalize())
                })
                .collect()
        };
        assert!(headings(&world).iter().all(|&cos| cos > 0.999), "outward");
        world
            .set_property(
                emitter,
                "ShapeInOut",
                enum_value("ParticleEmitterShapeInOut", "Inward"),
            )
            .unwrap();
        assert!(headings(&world).iter().all(|&cos| cos < -0.999), "inward");
        world
            .set_property(
                emitter,
                "ShapeInOut",
                enum_value("ParticleEmitterShapeInOut", "InAndOut"),
            )
            .unwrap();
        let both = headings(&world);
        assert!(both.iter().all(|cos| cos.abs() > 0.999));
        assert!(both.iter().any(|&cos| cos > 0.0) && both.iter().any(|&cos| cos < 0.0));
    }

    /// R17.2b: with VelocityInheritance, particles start with that share
    /// of their moving part's velocity.
    #[test]
    fn particles_inherit_their_moving_parts_velocity() {
        let (mut world, part, emitter) = world_with_emitter(100.0);
        for (name, value) in [
            ("Speed", 0.0),
            ("VelocityInheritance", 0.5),
            ("Lifetime", 10.0),
        ] {
            world
                .set_property(emitter, name, Value::Number(value))
                .unwrap();
        }
        let mut particles = Particles::new();
        particles.update(&world, 0.1);
        let still = particles.emitters[&emitter].particles.len();
        assert!(still > 0);
        assert!(
            particles.emitters[&emitter]
                .particles
                .iter()
                .all(|p| p.velocity == Vec3::ZERO),
            "standing still"
        );
        // Moved 2 studs along X in 0.1 s: 20 studs a second, half kept.
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(2.0, 10.0, 0.0)),
            )
            .unwrap();
        particles.update(&world, 0.1);
        let state = &particles.emitters[&emitter];
        let born: Vec<Vec3> = state.particles[still..]
            .iter()
            .map(|p| p.velocity)
            .collect();
        assert!(!born.is_empty());
        for velocity in born {
            assert!(
                (velocity - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3,
                "{velocity}"
            );
        }
    }

    #[test]
    fn particles_fade_and_grow_toward_their_end_values() {
        let (mut world, _, emitter) = world_with_emitter(0.0);
        world
            .set_property(emitter, "SizeEnd", Value::Number(3.0))
            .unwrap();
        world
            .set_property(emitter, "Lifetime", Value::Number(10.0))
            .unwrap();
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 1);
        let born = &particles.sprites(&world, &Assets::default())[0];
        assert!((born.size - 1.0).abs() < 1e-4 && (born.opacity - 1.0).abs() < 1e-4);
        particles.update(&world, 0.25);
        let older = &particles.sprites(&world, &Assets::default())[0];
        assert!(older.size > 1.0 && older.opacity < 1.0);
    }

    #[test]
    fn an_emitter_outside_a_part_gives_off_from_its_own_place_and_outside_the_workspace_nothing() {
        let (mut world, part, emitter) = world_with_emitter(0.0);
        // Standing free in the Workspace, as a light can: from its Position,
        // turned by its Orientation (a quarter turn about Z sends +Y to -X).
        world.set_parent(emitter, Some(world.workspace())).unwrap();
        world
            .set_property(
                emitter,
                "Position",
                Value::Vector3(Vector3::new(30.0, 4.0, -2.0)),
            )
            .unwrap();
        world
            .set_property(
                emitter,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 0.0, 90.0)),
            )
            .unwrap();
        world
            .set_property(emitter, "SpreadAngle", Value::Number(0.0))
            .unwrap();
        let mut particles = Particles::new();
        particles.emit(&world, emitter, 20);
        let start = particles.sprites(&world, &Assets::default());
        assert_eq!(start.len(), 20);
        for sprite in &start {
            assert!(sprite.position.distance(Vec3::new(30.0, 4.0, -2.0)) < 0.1);
        }
        particles.update(&world, 0.1);
        let later = particles.sprites(&world, &Assets::default());
        assert!(
            later.iter().all(|sprite| sprite.position.x < 29.8),
            "{later:?}"
        );

        world.set_parent(emitter, Some(part)).unwrap();
        world.set_parent(part, None).unwrap();
        particles.update(&world, 5.0);
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn an_emitter_in_a_part_kept_inside_a_union_gives_off_nothing() {
        let (mut world, part, _) = world_with_emitter(100.0);
        let union = world.create(ClassName::UnionOperation).unwrap();
        world.set_parent(union, Some(world.workspace())).unwrap();
        world.set_parent(part, Some(union)).unwrap();
        let mut particles = Particles::new();
        particles.update(&world, 0.5);
        assert_eq!(
            particles.count(),
            0,
            "its part isn't drawn, so neither are they"
        );
    }

    #[test]
    fn every_built_in_picture_is_different_and_has_a_shape() {
        let pictures: Vec<Vec<u8>> = ParticleType::ALL.iter().map(|&k| draw(k)).collect();
        for (i, picture) in pictures.iter().enumerate() {
            let solid = picture.chunks(4).filter(|p| p[3] > 128).count();
            assert!(solid > 20, "{:?} is empty", ParticleType::ALL[i]);
            // The corners are clear.
            assert_eq!(picture[3], 0, "{:?}", ParticleType::ALL[i]);
            for other in &pictures[i + 1..] {
                assert_ne!(picture, other);
            }
        }
        let a = built_in(ParticleType::Fire);
        let b = built_in(ParticleType::Fire);
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), built_in(ParticleType::Smoke).key());
    }

    #[test]
    fn wind_carries_particles_only_from_emitters_it_affects() {
        let (mut world, _, emitter) = world_with_emitter(0.0);
        let workspace = world.workspace();
        world
            .set_property(
                workspace,
                "GlobalWind",
                Value::Vector3(Vector3::new(20.0, 0.0, 0.0)),
            )
            .unwrap();
        for (name, value) in [
            ("Speed", Value::Number(0.0)),
            ("Drag", Value::Number(5.0)),
            ("Lifetime", Value::Number(10.0)),
        ] {
            world.set_property(emitter, name, value).unwrap();
        }
        let drift = |world: &World| {
            let mut particles = Particles::new();
            particles.emit(world, emitter, 20);
            let start = particles.sprites(world, &Assets::default());
            for _ in 0..30 {
                particles.update(world, 1.0 / 30.0);
            }
            let end = particles.sprites(world, &Assets::default());
            let sum: f32 = start
                .iter()
                .zip(&end)
                .map(|(a, b)| b.position.x - a.position.x)
                .sum();
            sum / start.len() as f32
        };
        assert!(drift(&world).abs() < 1e-4);
        world
            .set_property(emitter, "WindAffectsDrag", Value::Bool(true))
            .unwrap();
        // Close to the wind's 20 studs/s by the end of the second.
        assert!(drift(&world) > 10.0, "{}", drift(&world));
    }

    /// A draw list of `particles` alone.
    fn drawn(particles: &Particles, world: &World) -> DrawList {
        let camera = axcenity_render::Camera {
            position: Vec3::ZERO,
            look_at: Vec3::NEG_Z,
            fov_y_degrees: 70.0,
        };
        let mut list = crate::build_draw_list(world, &Assets::default(), camera);
        list.sprites.clear();
        particles.draw(&mut list, world, &Assets::default());
        list
    }

    fn set(world: &mut World, emitter: InstanceId, name: &str, value: f64) {
        world
            .set_property(emitter, name, Value::Number(value))
            .unwrap();
    }

    #[test]
    fn a_hundred_thousand_particles_move_on_the_gpu_and_only_where_it_can() {
        let (mut world, _, emitter) = world_with_emitter(50_000.0);
        set(&mut world, emitter, "Lifetime", 2.0);
        let mut particles = Particles::new();
        particles.set_gpu(true);
        for _ in 0..150 {
            particles.update(&world, 1.0 / 60.0);
        }
        // Two and a half seconds of births at 50,000 a second, each living
        // 1.6 to 2.4 seconds: 100,000 out.
        let out = particles.count();
        assert!((95_000..=105_000).contains(&out), "{out}");
        assert_eq!(particles.gpu_emitters(), 1);
        let list = drawn(&particles, &world);
        assert!(list.sprites.is_empty());
        let [gpu] = &list.particle_emitters[..] else {
            panic!("one GPU emitter: {:?}", list.particle_emitters.len());
        };
        assert_eq!(gpu.capacity, 1 << 17);
        assert_eq!(gpu.step, 150);
        // Every birth so far, 50,000 a second.
        let births = gpu.born + gpu.births;
        assert!(births.abs_diff(125_000) <= 1, "{births}");

        // Without compute shaders, the CPU keeps as many as it can.
        particles.set_gpu(false);
        assert_eq!(particles.gpu_emitters(), 0);
        for _ in 0..30 {
            particles.update(&world, 1.0 / 60.0);
        }
        assert_eq!(particles.count(), MAX_PER_EMITTER);
        let list = drawn(&particles, &world);
        assert!(list.particle_emitters.is_empty());
        assert_eq!(list.sprites.len(), MAX_PER_EMITTER);
    }

    #[test]
    fn small_emitters_stay_on_the_cpu_with_the_gpu_allowed() {
        // hello-particles' busiest emitter: 40 a second for a second.
        let (mut world, _, emitter) = world_with_emitter(40.0);
        set(&mut world, emitter, "Lifetime", 1.0);
        let mut particles = Particles::new();
        particles.set_gpu(true);
        for _ in 0..60 {
            particles.update(&world, 1.0 / 60.0);
        }
        let list = drawn(&particles, &world);
        assert!(list.particle_emitters.is_empty());
        assert_eq!(list.sprites.len(), particles.count());
        assert!(particles.count() > 30);
        // So does a burst below the threshold.
        particles.emit(&world, emitter, 500);
        assert_eq!(particles.gpu_emitters(), 0);
    }

    #[test]
    fn a_big_burst_is_born_on_the_gpu_at_the_next_step() {
        let (mut world, _, emitter) = world_with_emitter(0.0);
        set(&mut world, emitter, "Lifetime", 5.0);
        let mut particles = Particles::new();
        particles.set_gpu(true);
        particles.emit(&world, emitter, 50_000);
        particles.update(&world, 1.0 / 60.0);
        let list = drawn(&particles, &world);
        let gpu = &list.particle_emitters[0];
        assert_eq!((gpu.born, gpu.births), (0, 50_000));
        assert_eq!(gpu.capacity, 1 << 16);
        assert_eq!(particles.count(), 50_000);
        // Each step is new; drawing again repeats it.
        particles.update(&world, 1.0 / 60.0);
        let again = drawn(&particles, &world);
        let next = &again.particle_emitters[0];
        assert_eq!(
            (next.step, next.born, next.births),
            (gpu.step + 1, 50_000, 0)
        );
        assert_eq!(
            drawn(&particles, &world).particle_emitters[0].step,
            next.step
        );
    }

    #[test]
    fn gpu_particles_die_out_and_their_emitter_leaves_the_gpu() {
        let (mut world, _, emitter) = world_with_emitter(10_000.0);
        set(&mut world, emitter, "Lifetime", 1.0);
        let mut particles = Particles::new();
        particles.set_gpu(true);
        particles.update(&world, 0.1);
        let key = drawn(&particles, &world).particle_emitters[0].key;
        world
            .set_property(emitter, "Enabled", Value::Bool(false))
            .unwrap();
        // All gone by 1.2 seconds.
        particles.update(&world, 0.25);
        assert!(particles.count() > 0 && particles.is_active(&world));
        for _ in 0..5 {
            particles.update(&world, 0.25);
        }
        assert_eq!(particles.count(), 0);
        assert_eq!(particles.gpu_emitters(), 0);
        assert!(!particles.is_active(&world));
        assert!(drawn(&particles, &world).particle_emitters.is_empty());

        // Back on, it's a new emitter to the renderer.
        world
            .set_property(emitter, "Enabled", Value::Bool(true))
            .unwrap();
        particles.update(&world, 0.1);
        assert_ne!(drawn(&particles, &world).particle_emitters[0].key, key);
    }

    #[test]
    fn gpu_emitters_are_told_the_emitters_settings_and_look() {
        let (mut world, part, emitter) = world_with_emitter(5000.0);
        world
            .set_property(
                part,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 0.0, 90.0)),
            )
            .unwrap();
        for (name, value) in [
            ("Speed", 7.0),
            ("SpreadAngle", 40.0),
            ("Drag", 2.0),
            ("Size", 2.0),
            ("SizeEnd", 0.5),
            ("Transparency", 0.25),
            ("Rotation", 15.0),
            ("RotSpeed", 90.0),
            ("Brightness", 3.0),
            ("LightEmission", 1.0),
        ] {
            set(&mut world, emitter, name, value);
        }
        let workspace = world.workspace();
        world
            .set_property(
                workspace,
                "GlobalWind",
                Value::Vector3(Vector3::new(4.0, 0.0, 0.0)),
            )
            .unwrap();
        world
            .set_property(emitter, "WindAffectsDrag", Value::Bool(true))
            .unwrap();
        world
            .set_property(
                emitter,
                "Shape",
                Value::EnumItem(axcenity_core::EnumItem {
                    enum_name: "ParticleEmitterShape".to_owned(),
                    item: "Cylinder".to_owned(),
                }),
            )
            .unwrap();
        let mut particles = Particles::new();
        particles.set_gpu(true);
        particles.update(&world, 0.1);
        let list = drawn(&particles, &world);
        let gpu = &list.particle_emitters[0];
        // Direction is +Y turned with the part: a quarter turn about Z.
        assert!(
            (gpu.direction - Vec3::NEG_X).length() < 1e-4,
            "{}",
            gpu.direction
        );
        assert!((gpu.spread - 20f32.to_radians()).abs() < 1e-5);
        assert_eq!(
            (gpu.speed, gpu.drag, gpu.air),
            (7.0, 2.0, Vec3::new(4.0, 0.0, 0.0))
        );
        assert!((gpu.spawn.w_axis.truncate() - Vec3::new(0.0, 10.0, 0.0)).length() < 1e-4);
        let look = &gpu.look;
        assert_eq!(look.size, [2.0, 0.5]);
        assert_eq!(look.transparency, [0.25, 1.0]);
        assert_eq!((look.rotation_degrees, look.spin_degrees), (15.0, 90.0));
        assert_eq!((look.brightness, look.light_emission), (3.0, 1.0));
        assert!(look.texture.is_some(), "the built-in circle");
        let cylinder = SpawnShape {
            kind: SpawnKind::Cylinder,
            surface: false,
        };
        assert_eq!(gpu.shape, cylinder);
    }
}
