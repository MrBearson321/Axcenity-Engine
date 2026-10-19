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

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::{Arc, OnceLock};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Color3, Instance, InstanceId, ParticleType, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Sprite, TextureSource};

use crate::draw_list::{image_texture, part_placement};

/// The most particles one emitter keeps; past this, no more are born
/// until some die.
pub const MAX_PER_EMITTER: usize = 4000;
/// The most particles one frame gives birth to per emitter, so a huge
/// Rate (or a long pause) can't freeze the game.
const MAX_BIRTHS_PER_FRAME: usize = 1000;
/// The built-in pictures' size in pixels.
const PICTURE: u32 = 64;

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
        }
    }
}

/// Every emitter's particles. See the module docs.
#[derive(Debug)]
pub struct Particles {
    emitters: HashMap<InstanceId, Emitter>,
    /// A small random number generator's state (xorshift), so particles
    /// spread differently every time without a dependency.
    random: u64,
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
            random: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Takes every particle away, such as when Play starts or stops.
    pub fn clear(&mut self) {
        self.emitters.clear();
    }

    /// How many particles are out.
    pub fn count(&self) -> usize {
        self.emitters.values().map(|e| e.particles.len()).sum()
    }

    /// True while something moves: particles are out, or an emitter in
    /// the Workspace is giving them off. Hosts keep drawing frames then.
    pub fn is_active(&self, world: &World) -> bool {
        self.count() > 0 || emitters(world).any(|(_, instance)| Settings::read(instance).enabled)
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
        let count = (count as usize).min(MAX_BIRTHS_PER_FRAME);
        let mut state = self.emitters.remove(&emitter).unwrap_or_default();
        for _ in 0..count {
            self.birth(&mut state, &settings, transform);
        }
        self.emitters.insert(emitter, state);
    }

    /// Moves every particle on by `seconds`, lets old ones die, and gives
    /// off new ones from each Enabled emitter by its Rate.
    pub fn update(&mut self, world: &World, seconds: f32) {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        let live: Vec<(InstanceId, Settings, Option<axcenity_render::glam::Mat4>)> =
            emitters(world)
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
        for (id, settings, transform) in live {
            let mut state = self.emitters.remove(&id).unwrap_or_default();
            // Older particles first: move them, and let them die.
            let slow = 1.0 / (1.0 + settings.drag * seconds);
            for particle in &mut state.particles {
                particle.velocity = (particle.velocity + settings.acceleration * seconds) * slow;
                particle.position += particle.velocity * seconds;
                particle.age += seconds;
            }
            state.particles.retain(|p| p.age < p.life);
            match (settings.enabled, transform) {
                (true, Some(transform)) => {
                    state.owed += settings.rate * seconds;
                    let births = (state.owed.floor() as usize).min(MAX_BIRTHS_PER_FRAME);
                    state.owed -= state.owed.floor();
                    for _ in 0..births {
                        self.birth(&mut state, &settings, transform);
                    }
                }
                _ => state.owed = 0.0,
            }
            if !state.particles.is_empty() || settings.enabled {
                self.emitters.insert(id, state);
            }
        }
    }

    /// The sprites to draw for every particle now. Textures named by
    /// emitters come from `assets`.
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
    fn birth(
        &mut self,
        state: &mut Emitter,
        settings: &Settings,
        transform: axcenity_render::glam::Mat4,
    ) {
        if state.particles.len() >= MAX_PER_EMITTER {
            return;
        }
        let inside = Vec3::new(self.next() - 0.5, self.next() - 0.5, self.next() - 0.5);
        let position = transform.transform_point3(inside);
        let (_, rotation, _) = transform.to_scale_rotation_translation();
        let forward = (rotation * settings.direction).normalize_or(Vec3::Y);
        let heading = self.within_cone(forward, settings.spread.to_radians() / 2.0);
        let life = settings.lifetime * (0.8 + 0.4 * self.next());
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
        let z = 1.0 - self.next() * (1.0 - cos_most);
        let around = self.next() * TAU;
        let across = (1.0 - z * z).max(0.0).sqrt();
        let (u, v) = forward.any_orthonormal_pair();
        forward * z + (u * around.cos() + v * around.sin()) * across
    }

    /// A random number from 0 up to 1.
    fn next(&mut self) -> f32 {
        let mut x = self.random;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.random = x;
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

fn lerp((from, to): (f32, f32), t: f32) -> f32 {
    from + (to - from) * t
}

/// Every ParticleEmitter in the Workspace.
fn emitters(world: &World) -> impl Iterator<Item = (InstanceId, &Instance)> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(move |id| Some((id, world.get(id)?)))
        .filter(|(_, instance)| instance.class() == ClassName::ParticleEmitter)
}

/// Where the emitter's parent part is, as the transform that stretches the
/// 1×1×1 cube to fill it. `None` if it isn't inside a part.
fn parent_transform(world: &World, emitter: &Instance) -> Option<axcenity_render::glam::Mat4> {
    let parent = world.get(emitter.parent()?)?;
    let (_, transform) = part_placement(parent)?;
    Some(transform)
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
    fn an_emitter_outside_a_part_or_the_workspace_gives_off_nothing() {
        let (mut world, part, emitter) = world_with_emitter(100.0);
        world.set_parent(emitter, Some(world.workspace())).unwrap();
        let mut particles = Particles::new();
        particles.update(&world, 0.5);
        assert_eq!(particles.count(), 0);
        world.set_parent(emitter, Some(part)).unwrap();
        particles.update(&world, 0.1);
        assert!(particles.count() > 0);
        world.set_parent(part, None).unwrap();
        particles.update(&world, 0.1);
        assert_eq!(particles.count(), 0);
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
}
