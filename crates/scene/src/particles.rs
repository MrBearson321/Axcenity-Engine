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
    DrawList, MAX_GPU_PARTICLES, ParticleEmitter, ParticleLook, Sprite, TextureSource,
};

use crate::draw_list::image_texture;
use crate::weather::WeatherState;

/// The most particles one emitter keeps; past this, no more are born
/// until some die.
pub const MAX_PER_EMITTER: usize = 4000;
/// The most particles one frame gives birth to per emitter, so a huge
/// Rate (or a long pause) can't freeze the game.
const MAX_BIRTHS_PER_FRAME: usize = 1000;
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
}

#[derive(Debug, Default)]
struct Emitter {
    particles: Vec<Particle>,
    /// Births owed from earlier frames' fractions of a particle.
    owed: f32,
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
        }
    }
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
        let settings = Settings::read(instance);
        let Some(transform) = parent_transform(world, instance) else {
            return;
        };
        let mut state = self.emitters.remove(&emitter).unwrap_or_default();
        // A big burst goes to the GPU, as do births of an emitter already
        // there. They're born at its next step.
        let burst = count as f32 >= GPU_PARTICLE_THRESHOLD;
        if self.gpu && (burst || self.on_gpu(&state, &settings)) {
            let gpu = self.gpu_state(&mut state);
            gpu.pending = gpu.pending.saturating_add(count).min(MAX_GPU_PARTICLES);
        } else {
            let count = (count as usize).min(MAX_BIRTHS_PER_FRAME);
            for _ in 0..count {
                self.birth(&mut state, &settings, transform);
            }
        }
        self.emitters.insert(emitter, state);
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
        self.gpu && out >= threshold
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
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.weather.update(world, seconds, &mut self.random);
        let live: Vec<(InstanceId, Settings, Option<Mat4>)> = emitters(world)
            .map(|(id, instance)| {
                (
                    id,
                    Settings::read(instance),
                    parent_transform(world, instance),
                )
            })
            .collect();
        self.emitters
            .retain(|id, _| live.iter().any(|(live, _, _)| live == id));
        let wind = global_wind(world);
        for (id, settings, transform) in live {
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
            for particle in &mut state.particles {
                let velocity = particle.velocity + settings.acceleration * seconds;
                particle.velocity = air + (velocity - air) * slow;
                particle.position += particle.velocity * seconds;
                particle.age += seconds;
            }
            state.particles.retain(|p| p.age < p.life);
            let on_gpu = self.on_gpu(&state, &settings);
            let mut gpu_births = 0;
            match (settings.enabled, transform) {
                (true, Some(transform)) => {
                    state.owed += settings.rate * seconds;
                    let owed = state.owed.floor();
                    state.owed -= owed;
                    if on_gpu {
                        gpu_births = (owed as u32).min(MAX_GPU_PARTICLES);
                    } else {
                        for _ in 0..(owed as usize).min(MAX_BIRTHS_PER_FRAME) {
                            self.birth(&mut state, &settings, transform);
                        }
                    }
                }
                _ => state.owed = 0.0,
            }
            if on_gpu || state.gpu.is_some() {
                let gpu = self.gpu_state(&mut state);
                step_gpu(gpu, &settings, (transform, air), seconds, gpu_births);
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
                let transparency = lerp(look.transparency, t).clamp(0.0, 1.0);
                sprites.push(Sprite {
                    position: particle.position,
                    size: lerp(look.size, t).max(0.0),
                    rotation_degrees: look.rotation + look.spin * particle.age,
                    color: [
                        lerp((look.color.0.r, look.color.1.r), t),
                        lerp((look.color.0.g, look.color.1.g), t),
                        lerp((look.color.0.b, look.color.1.b), t),
                    ],
                    opacity: 1.0 - transparency,
                    brightness: look.brightness,
                    light_emission: look.light_emission,
                    texture: look.texture.clone(),
                });
            }
        }
        sprites
    }

    /// Adds one particle to `state`, somewhere inside the part, heading out
    /// along the emitter's direction within its spread.
    fn birth(&mut self, state: &mut Emitter, settings: &Settings, transform: Mat4) {
        if state.particles.len() >= MAX_PER_EMITTER {
            return;
        }
        let inside = Vec3::new(
            self.random.next() - 0.5,
            self.random.next() - 0.5,
            self.random.next() - 0.5,
        );
        let position = transform.transform_point3(inside);
        let (_, rotation, _) = transform.to_scale_rotation_translation();
        let forward = (rotation * settings.direction).normalize_or(Vec3::Y);
        let heading = self.within_cone(forward, settings.spread.to_radians() / 2.0);
        let life = settings.lifetime * (0.8 + 0.4 * self.random.next());
        state.particles.push(Particle {
            position,
            velocity: heading * settings.speed,
            age: 0.0,
            life,
        });
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
        let texture = assets
            .texture(path)
            .map(|image| image_texture(&image))
            .or_else(|| {
                let kind = instance
                    .property("ParticleType")
                    .and_then(Value::as_particle_type)
                    .unwrap_or(ParticleType::Circle);
                Some(built_in(kind))
            });
        Self {
            size: (number("Size", 1.0), number("SizeEnd", 1.0)),
            color: (color("Color"), color("ColorEnd")),
            transparency: (number("Transparency", 0.0), number("TransparencyEnd", 1.0)),
            rotation: number("Rotation", 0.0),
            spin: number("RotSpeed", 0.0),
            light_emission: number("LightEmission", 0.0).clamp(0.0, 1.0),
            brightness: number("Brightness", 1.0).max(0.0),
            texture,
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
    (transform, air): (Option<Mat4>, Vec3),
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
        .descendants_iter(world.workspace())
        .filter(move |&id| !axcenity_core::csg::out_of_world(world, id))
        .filter_map(move |id| Some((id, world.get(id)?)))
        .filter(|(_, instance)| instance.class() == ClassName::ParticleEmitter)
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
    }
}
