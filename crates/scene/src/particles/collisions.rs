//! Particles that collide and sub-emitters (roadmap R17.4).
//!
//! A ParticleEmitter with `Collides` on moves its particles against the
//! world's solid surfaces: the host passes a [`Surfaces`] (physics'
//! `surface_hits`), and each frame every particle's path is cast, at most
//! [`MAX_SURFACE_RAYS`] a frame over all emitters. A particle that lands
//! keeps `Bounce` of its speed into the surface, back out, and loses
//! `LifetimeLoss` of its life.
//!
//! A ParticleEmitter inside another whose `EmitOn` is Birth, Death or
//! Collision is a sub-emitter: it gives off `EmitCount` particles where
//! each of its parent's particles is born, dies or lands, and nothing by
//! its Rate. Particles nothing is read back for can't have either, so an
//! emitter that collides, has sub-emitters or is one moves its particles
//! on the CPU.

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use axcenity_render::glam::Vec3;

use super::Particle;

/// The most particle paths cast against surfaces in a frame, over every
/// colliding emitter. Particles past it move without colliding that frame.
pub const MAX_SURFACE_RAYS: usize = 8000;
/// How far above a surface a landed particle rests, in studs, so its next
/// path starts outside it.
const SKIN: f32 = 0.02;
/// Bounced back slower than this (studs a second), a particle comes to
/// rest on the surface instead of hopping on it forever.
const REST_SPEED: f32 = 1.0;
/// How long a particle must be off surfaces, in seconds, before meeting
/// one counts as landing again.
const LANDING_GAP: f32 = 0.1;
/// How many times a frame sub-emitters' own births can set off theirs.
const MAX_EVENT_ROUNDS: usize = 4;

/// Where a particle's path first meets a solid surface, and the surface's
/// normal there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub position: Vector3,
    pub normal: Vector3,
}

/// What colliding particles land on: given the part to pass by (the
/// emitter's own) and paths from `.0` to `.1`, the first solid surface on
/// each, or `None`. Hosts pass `physics::surface_hits`; without one
/// ([`Particles::update`](super::Particles::update)) nothing is hit.
pub type Surfaces<'a> =
    dyn FnMut(Option<InstanceId>, &[(Vector3, Vector3)]) -> Vec<Option<SurfaceHit>> + 'a;

/// When a sub-emitter gives off particles (`EmitOn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmitOn {
    Always,
    Birth,
    Death,
    Collision,
}

impl EmitOn {
    pub(super) fn read(instance: &Instance) -> Self {
        match instance.property("EmitOn") {
            Some(Value::EnumItem(item)) => match item.item.as_str() {
                "Birth" => EmitOn::Birth,
                "Death" => EmitOn::Death,
                "Collision" => EmitOn::Collision,
                _ => EmitOn::Always,
            },
            _ => EmitOn::Always,
        }
    }
}

/// An emitter's sub-emitter.
#[derive(Clone, Copy, Debug)]
pub(super) struct Sub {
    pub id: InstanceId,
    pub on: EmitOn,
    pub count: u32,
}

/// `count` particles for sub-emitter `emitter` to give off at `position`,
/// where its parent's particle was, moving at `velocity`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Event {
    pub emitter: InstanceId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub count: u32,
}

/// True if `instance` is a sub-emitter: inside a ParticleEmitter, with an
/// `EmitOn` other than Always.
pub(super) fn is_sub(world: &World, instance: &Instance) -> bool {
    EmitOn::read(instance) != EmitOn::Always
        && instance
            .parent()
            .and_then(|parent| world.get(parent))
            .is_some_and(|parent| parent.class() == ClassName::ParticleEmitter)
}

/// The Enabled sub-emitters directly inside `instance`.
pub(super) fn subs_of(world: &World, instance: &Instance) -> Vec<Sub> {
    instance
        .children()
        .iter()
        .filter_map(|&id| {
            let child = world.get(id)?;
            if child.class() != ClassName::ParticleEmitter
                || child.property("Enabled").and_then(Value::as_bool) != Some(true)
            {
                return None;
            }
            let on = EmitOn::read(child);
            let count = child
                .property("EmitCount")
                .and_then(Value::as_number)
                .unwrap_or(0.0)
                .clamp(0.0, super::MAX_PER_EMITTER as f64) as u32;
            (on != EmitOn::Always && count > 0).then_some(Sub { id, on, count })
        })
        .collect()
}

/// `subs`' events for one of their parent's particles, at `position`
/// moving at `velocity`, when `on` happens to it.
pub(super) fn events_on(
    subs: &[Sub],
    on: EmitOn,
    position: Vec3,
    velocity: Vec3,
    events: &mut Vec<Event>,
) {
    for sub in subs.iter().filter(|sub| sub.on == on) {
        events.push(Event {
            emitter: sub.id,
            position,
            velocity,
            count: sub.count,
        });
    }
}

/// How a colliding emitter's particles land (`Bounce`, `LifetimeLoss`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Landing {
    pub bounce: f32,
    pub lifetime_loss: f32,
}

/// Moves `particles` each by its `steps` entry, stopping those whose
/// paths meet a surface there and bouncing them off it. `rays` is how
/// many paths may still be cast this frame; the particles past it move on
/// without colliding. Each particle that lands (rather than resting or
/// skidding on a surface: see `LANDING_GAP`) gives `subs`' Collision
/// events.
#[allow(clippy::too_many_arguments)]
pub(super) fn move_colliding(
    particles: &mut [Particle],
    steps: &[Vec3],
    seconds: f32,
    landing: Landing,
    skip: Option<InstanceId>,
    surfaces: &mut Surfaces,
    rays: &mut usize,
    subs: &[Sub],
    events: &mut Vec<Event>,
) {
    let cast = particles.len().min(*rays);
    *rays -= cast;
    let paths: Vec<(Vector3, Vector3)> = particles[..cast]
        .iter()
        .zip(steps)
        .map(|(particle, step)| {
            (
                vector3(particle.position),
                vector3(particle.position + *step),
            )
        })
        .collect();
    let hits = if paths.is_empty() {
        Vec::new()
    } else {
        surfaces(skip, &paths)
    };
    for (index, (particle, step)) in particles.iter_mut().zip(steps).enumerate() {
        let Some(hit) = hits.get(index).copied().flatten() else {
            particle.position += *step;
            particle.off_surface += seconds;
            continue;
        };
        let normal = vec3(hit.normal).normalize_or(Vec3::Y);
        particle.position = vec3(hit.position) + normal * SKIN;
        let into = particle.velocity.dot(normal);
        if into < 0.0 {
            let along = particle.velocity - normal * into;
            let back = -into * landing.bounce;
            particle.velocity = if back < REST_SPEED {
                along
            } else {
                along + normal * back
            };
        }
        if particle.off_surface > LANDING_GAP {
            particle.age += particle.life * landing.lifetime_loss;
            events_on(
                subs,
                EmitOn::Collision,
                particle.position,
                particle.velocity,
                events,
            );
        }
        particle.off_surface = 0.0;
    }
}

/// Gives off the particles `events` ask for, from their sub-emitters,
/// and the events those births set off in turn, a few rounds deep.
pub(super) fn fire(particles: &mut super::Particles, world: &World, mut events: Vec<Event>) {
    for _ in 0..MAX_EVENT_ROUNDS {
        if events.is_empty() {
            return;
        }
        let mut next = Vec::new();
        for event in events {
            particles.give_off_at(world, event, &mut next);
        }
        events = next;
    }
}

fn vector3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

fn vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
