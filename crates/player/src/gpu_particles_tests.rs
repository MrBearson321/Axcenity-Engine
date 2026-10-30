//! GPU particles in a world, drawn as a host does: hello-particles looks
//! the same with them allowed, and an emitter of a hundred thousand
//! particles runs on the GPU, or on the CPU where compute shaders can't.

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::{Capabilities, TextureRenderer};
use axcenity_scene::{FlyCamera, Particles};

use super::tests::{start_example_in, test_gpu};
use super::*;

/// Moves `particles` on by `seconds` a sixtieth at a time, as the player
/// does (allowing the GPU where `renderer` runs compute shaders), then
/// draws them over `world` from `camera` and returns the pixels.
fn host_frame(
    renderer: &mut TextureRenderer,
    particles: &mut Particles,
    (world, assets): (&World, &Assets),
    seconds: f32,
    camera: FlyCamera,
) -> Vec<u8> {
    particles.set_gpu(renderer.capabilities().compute_shaders);
    for _ in 0..(seconds * 60.0).round() as usize {
        particles.update(world, 1.0 / 60.0);
    }
    let mut list = axcenity_scene::build_draw_list(world, assets, camera.to_render_camera());
    particles.draw(&mut list, world, assets);
    renderer.render(&list).unwrap();
    renderer.read_pixels()
}

#[test]
fn hello_particles_looks_the_same_with_gpu_particles_allowed() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (world, _scripts, folder) = start_example_in("hello-particles");
    let assets = Assets::new(&folder);
    let camera = FlyCamera::looking_at(Vec3::new(0.0, 8.0, 24.0), Vec3::new(0.0, 4.0, 0.0));
    let mut frames = Vec::new();
    for compute in [true, false] {
        let mut renderer = TextureRenderer::new(&device, &queue, 160, 90);
        if !compute {
            renderer.limit_capabilities(&Capabilities {
                compute_shaders: false,
                ..*renderer.capabilities()
            });
        }
        let mut particles = Particles::new();
        let pixels = host_frame(
            &mut renderer,
            &mut particles,
            (&world, &assets),
            2.0,
            camera,
        );
        assert!(particles.count() > 0);
        assert_eq!(particles.gpu_emitters(), 0, "its emitters are small");
        assert_eq!(renderer.stats().gpu_particles, 0);
        frames.push(pixels);
    }
    assert!(frames[0] == frames[1], "the same frame either way");
    assert!(assets.take_errors().is_empty());
}

#[test]
fn a_hundred_thousand_particles_run_on_the_gpu_and_on_the_cpu_without_compute() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Position", Value::Vector3(Vector3::new(0.0, 4.0, 0.0))),
        ("Size", Value::Vector3(Vector3::new(6.0, 6.0, 6.0))),
        ("Anchored", Value::Bool(true)),
        ("Transparency", Value::Number(1.0)),
    ] {
        world.set_property(part, name, value).unwrap();
    }
    let emitter = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_parent(emitter, Some(part)).unwrap();
    for (name, value) in [
        ("Rate", 50_000.0),
        ("Lifetime", 2.0),
        ("Speed", 0.5),
        ("Size", 0.5),
        ("TransparencyEnd", 0.0),
    ] {
        world
            .set_property(emitter, name, Value::Number(value))
            .unwrap();
    }
    let assets = Assets::default();
    let camera = FlyCamera::looking_at(Vec3::new(0.0, 4.0, 14.0), Vec3::new(0.0, 4.0, 0.0));
    let mut renderer = TextureRenderer::new(&device, &queue, 96, 96);
    if !renderer.capabilities().compute_shaders {
        eprintln!("skipped: this adapter has no compute shaders");
        return;
    }
    let empty = host_frame(
        &mut renderer,
        &mut Particles::new(),
        (&world, &assets),
        0.0,
        camera,
    );
    let mut particles = Particles::new();
    let gpu = host_frame(
        &mut renderer,
        &mut particles,
        (&world, &assets),
        2.5,
        camera,
    );
    let out = particles.count();
    assert!((95_000..=105_000).contains(&out), "{out}");
    assert_eq!(particles.gpu_emitters(), 1);
    assert_eq!(renderer.stats().gpu_particles, 1 << 17);
    // A white cloud in the middle of the frame: pixels brighter than
    // without it.
    let lit = |pixels: &[u8]| {
        let sum = |pixel: &[u8]| pixel[..3].iter().map(|&c| u32::from(c)).sum::<u32>();
        pixels
            .chunks(4)
            .zip(empty.chunks(4))
            .filter(|(pixel, empty)| sum(pixel) > sum(empty) + 60)
            .count()
    };
    let shown = lit(&gpu);
    assert!(shown > 96 * 96 / 10, "{shown} pixels of particles");

    // Where compute shaders can't run, the CPU keeps as many as it can.
    renderer.limit_capabilities(&Capabilities {
        compute_shaders: false,
        ..*renderer.capabilities()
    });
    let cpu = host_frame(
        &mut renderer,
        &mut particles,
        (&world, &assets),
        0.5,
        camera,
    );
    assert_eq!(particles.gpu_emitters(), 0);
    assert_eq!(particles.count(), axcenity_scene::MAX_PER_EMITTER);
    assert_eq!(renderer.stats().gpu_particles, 0);
    assert!(lit(&cpu) > 0, "the CPU's particles are drawn");
}
