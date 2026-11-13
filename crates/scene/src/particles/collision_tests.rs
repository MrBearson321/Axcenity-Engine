//! Roadmap R17.4: colliding particles and sub-emitters.

use axcenity_core::{EnumItem, Vector3};

use super::*;

/// A part at (0, 10, 0) with an emitter in it that gives off nothing by
/// itself: particles leave straight down at `speed`, live ten seconds and
/// fall under strong gravity.
fn falling_emitter(speed: f64) -> (World, InstanceId, InstanceId) {
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
    for (name, value) in [
        ("Rate", Value::Number(0.0)),
        ("Speed", Value::Number(speed)),
        ("Lifetime", Value::Number(10.0)),
        ("SpreadAngle", Value::Number(0.0)),
        ("Direction", Value::Vector3(Vector3::new(0.0, -1.0, 0.0))),
        (
            "Acceleration",
            Value::Vector3(Vector3::new(0.0, -50.0, 0.0)),
        ),
    ] {
        world.set_property(emitter, name, value).unwrap();
    }
    (world, part, emitter)
}

/// The world's draw list, to add particles to.
fn empty_list(world: &World) -> DrawList {
    let camera = axcenity_render::Camera {
        position: Vec3::ZERO,
        look_at: Vec3::NEG_Z,
        fov_y_degrees: 70.0,
    };
    crate::build_draw_list(world, &Assets::default(), camera)
}

fn emit_on(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "ParticleEmitterEmitOn".to_owned(),
        item: item.to_owned(),
    })
}

/// A floor at y = 0, as the physics' `surface_hits` would find one: the
/// paths that cross it from above, and the parts each call passed by.
#[allow(clippy::type_complexity)]
fn floor(
    skipped: &mut Vec<Option<InstanceId>>,
) -> impl FnMut(Option<InstanceId>, &[(Vector3, Vector3)]) -> Vec<Option<SurfaceHit>> + '_ {
    move |skip, paths| {
        skipped.push(skip);
        paths
            .iter()
            .map(|&(from, to)| {
                (from.y >= 0.0 && to.y < 0.0).then(|| {
                    let t = from.y / (from.y - to.y);
                    SurfaceHit {
                        position: Vector3::new(
                            from.x + (to.x - from.x) * t,
                            0.0,
                            from.z + (to.z - from.z) * t,
                        ),
                        normal: Vector3::new(0.0, 1.0, 0.0),
                    }
                })
            })
            .collect()
    }
}

fn heights(particles: &Particles, emitter: InstanceId) -> Vec<f32> {
    particles.emitters[&emitter]
        .particles
        .iter()
        .map(|p| p.position.y)
        .collect()
}

#[test]
fn colliding_particles_land_on_a_floor_and_others_fall_through_it() {
    let (mut world, part, emitter) = falling_emitter(0.0);
    world
        .set_property(emitter, "Collides", Value::Bool(true))
        .unwrap();
    world
        .set_property(emitter, "Bounce", Value::Number(0.0))
        .unwrap();
    let mut particles = Particles::new();
    let mut skipped = Vec::new();
    particles.emit(&world, emitter, 20);
    for _ in 0..120 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut skipped));
    }
    let landed = heights(&particles, emitter);
    assert_eq!(landed.len(), 20);
    for y in &landed {
        assert!((0.0..0.05).contains(y), "{landed:?}");
    }
    // Resting, not sliding or sinking.
    for p in &particles.emitters[&emitter].particles {
        assert!(p.velocity.length() < 1.0, "{:?}", p.velocity);
        assert!(p.off_surface < 0.1, "{p:?}");
    }
    // The emitter's own part is passed by.
    assert!(skipped.iter().all(|skip| *skip == Some(part)));

    // Without Collides, the same particles fall through.
    world
        .set_property(emitter, "Collides", Value::Bool(false))
        .unwrap();
    let mut particles = Particles::new();
    particles.emit(&world, emitter, 20);
    for _ in 0..120 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut Vec::new()));
    }
    assert!(heights(&particles, emitter).iter().all(|y| *y < -10.0));
}

#[test]
fn a_bouncy_particle_bounces_back_up_and_lifetime_loss_shortens_its_life() {
    let (mut world, _, emitter) = falling_emitter(20.0);
    world
        .set_property(emitter, "Acceleration", Value::Vector3(Vector3::default()))
        .unwrap();
    for (name, value) in [
        ("Collides", Value::Bool(true)),
        ("Bounce", Value::Number(1.0)),
        ("LifetimeLoss", Value::Number(0.25)),
    ] {
        world.set_property(emitter, name, value).unwrap();
    }
    let mut particles = Particles::new();
    particles.emit(&world, emitter, 1);
    let life = particles.emitters[&emitter].particles[0].life;
    // 10 studs down at 20 a second: it lands at about half a second.
    for _ in 0..40 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut Vec::new()));
    }
    let particle = particles.emitters[&emitter].particles[0];
    assert!(particle.velocity.y > 19.0, "{particle:?}");
    assert!(particle.position.y > 0.0, "{particle:?}");
    // A quarter of its life lost on landing, and the rest of the frames.
    let expected = 40.0 / 60.0 + life * 0.25;
    assert!((particle.age - expected).abs() < 1e-3, "{particle:?}");
}

#[test]
fn a_death_sub_emitter_bursts_where_its_parents_particles_die() {
    let (mut world, _, emitter) = falling_emitter(0.0);
    for (name, value) in [
        ("Collides", Value::Bool(true)),
        ("LifetimeLoss", Value::Number(1.0)),
    ] {
        world.set_property(emitter, name, value).unwrap();
    }
    let sub = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_parent(sub, Some(emitter)).unwrap();
    for (name, value) in [
        ("EmitOn", emit_on("Death")),
        ("EmitCount", Value::Number(5.0)),
        ("Rate", Value::Number(1000.0)),
        ("Speed", Value::Number(0.0)),
        ("Lifetime", Value::Number(10.0)),
    ] {
        world.set_property(sub, name, value).unwrap();
    }
    let mut particles = Particles::new();
    particles.emit(&world, emitter, 4);
    // Falling, nothing from the sub-emitter, whatever its Rate.
    for _ in 0..10 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut Vec::new()));
    }
    assert!(
        particles
            .emitters
            .get(&sub)
            .is_none_or(|state| state.particles.is_empty())
    );
    for _ in 0..110 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut Vec::new()));
    }
    // Every particle died where it landed, and each gave off 5 there.
    assert!(particles.emitters[&emitter].particles.is_empty());
    let burst = &particles.emitters[&sub].particles;
    assert_eq!(burst.len(), 20);
    for p in burst {
        assert!(p.position.y.abs() < 0.2, "{p:?}");
        assert!(
            p.position.x.abs() < 2.1 && p.position.z.abs() < 1.1,
            "{p:?}"
        );
    }
}

#[test]
fn birth_and_collision_sub_emitters_give_off_at_their_events_only() {
    let (mut world, _, emitter) = falling_emitter(0.0);
    world
        .set_property(emitter, "Collides", Value::Bool(true))
        .unwrap();
    world
        .set_property(emitter, "Bounce", Value::Number(0.0))
        .unwrap();
    let make = |world: &mut World, on: &str| {
        let sub = world.create(ClassName::ParticleEmitter).unwrap();
        world.set_parent(sub, Some(emitter)).unwrap();
        world.set_property(sub, "EmitOn", emit_on(on)).unwrap();
        world
            .set_property(sub, "EmitCount", Value::Number(2.0))
            .unwrap();
        world
            .set_property(sub, "Lifetime", Value::Number(10.0))
            .unwrap();
        world
            .set_property(sub, "Speed", Value::Number(0.0))
            .unwrap();
        sub
    };
    let born = make(&mut world, "Birth");
    let landed = make(&mut world, "Collision");
    let mut particles = Particles::new();
    particles.emit(&world, emitter, 3);
    let count = |particles: &Particles, id| {
        particles
            .emitters
            .get(&id)
            .map_or(0, |state: &Emitter| state.particles.len())
    };
    assert_eq!(count(&particles, born), 6);
    assert_eq!(count(&particles, landed), 0);
    for _ in 0..120 {
        particles.update_against(&world, 1.0 / 60.0, &mut floor(&mut Vec::new()));
    }
    // Landing once each, then resting, sets them off once each.
    assert_eq!(count(&particles, landed), 6);
    assert_eq!(count(&particles, born), 6);
    // Turned off, a sub-emitter gives off nothing.
    world
        .set_property(born, "Enabled", Value::Bool(false))
        .unwrap();
    particles.emit(&world, emitter, 3);
    assert_eq!(count(&particles, born), 6);
}

#[test]
fn big_colliding_emitters_land_on_the_gpu_and_sub_emitters_stay_on_the_cpu() {
    let (mut world, _, emitter) = falling_emitter(0.0);
    world
        .set_property(emitter, "Rate", Value::Number(50_000.0))
        .unwrap();
    for (name, value) in [
        ("Collides", Value::Bool(true)),
        ("Bounce", Value::Number(0.25)),
        ("LifetimeLoss", Value::Number(0.5)),
    ] {
        world.set_property(emitter, name, value).unwrap();
    }
    let mut particles = Particles::new();
    particles.set_gpu(true);
    particles.update(&world, 1.0 / 60.0);
    // R17.4b: on the GPU, told how its particles land on the prepass.
    assert_eq!(particles.gpu_emitters(), 1);
    let mut list = empty_list(&world);
    particles.draw(&mut list, &world, &Assets::default());
    assert_eq!(
        list.particle_emitters[0].collision,
        Some(ParticleCollision {
            bounce: 0.25,
            lifetime_loss: 0.5,
        })
    );
    // Without Collides, they pass through.
    world
        .set_property(emitter, "Collides", Value::Bool(false))
        .unwrap();
    particles.update(&world, 1.0 / 60.0);
    let mut list = empty_list(&world);
    particles.draw(&mut list, &world, &Assets::default());
    assert_eq!(list.particle_emitters[0].collision, None);

    // A sub-emitter keeps its parent (and itself) on the CPU.
    let sub = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_parent(sub, Some(emitter)).unwrap();
    world.set_property(sub, "EmitOn", emit_on("Death")).unwrap();
    let mut particles = Particles::new();
    particles.set_gpu(true);
    particles.update(&world, 1.0 / 60.0);
    assert_eq!(particles.gpu_emitters(), 0);

    // An Always child is an ordinary emitter, at its own Position.
    world
        .set_property(sub, "EmitOn", emit_on("Always"))
        .unwrap();
    world
        .set_property(sub, "Rate", Value::Number(60.0))
        .unwrap();
    world
        .set_property(emitter, "Rate", Value::Number(0.0))
        .unwrap();
    let mut particles = Particles::new();
    for _ in 0..10 {
        particles.update(&world, 1.0 / 60.0);
    }
    assert!(particles.emitters[&sub].particles.len() >= 8);
}

#[test]
fn surface_rays_are_capped_a_frame_over_every_emitter() {
    let (mut world, part, emitter) = falling_emitter(0.0);
    world
        .set_property(emitter, "Collides", Value::Bool(true))
        .unwrap();
    // Three full colliding emitters: more particles than the cap.
    let mut ids = vec![emitter];
    for _ in 0..2 {
        let copy = world.create(ClassName::ParticleEmitter).unwrap();
        world.set_parent(copy, Some(part)).unwrap();
        for name in ["Rate", "Speed", "Lifetime", "Collides"] {
            let value = world.get(emitter).unwrap().property(name).unwrap().clone();
            world.set_property(copy, name, value).unwrap();
        }
        ids.push(copy);
    }
    let mut particles = Particles::new();
    for &id in &ids {
        for _ in 0..4 {
            particles.emit(&world, id, 1000);
        }
    }
    assert_eq!(particles.count(), 3 * MAX_PER_EMITTER);
    let mut cast = 0;
    particles.update_against(&world, 1.0 / 60.0, &mut |_, paths| {
        cast += paths.len();
        vec![None; paths.len()]
    });
    assert_eq!(cast, MAX_SURFACE_RAYS);
}
