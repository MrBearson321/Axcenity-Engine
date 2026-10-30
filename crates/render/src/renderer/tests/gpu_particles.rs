//! GPU particles: a hundred thousand of them, drawn as the same sprites
//! would be, moved once per step, sorted farthest first, and kept only
//! while the list has their emitter and the device runs compute shaders.

use super::*;
use crate::{ParticleEmitter, ParticleLook};

const SIZE: u32 = 64;

/// An emitter of still particles born in a cube `spread_out` studs across
/// at `at`, red, 1 stud across, living 10 seconds.
fn emitter(at: Vec3, spread_out: f32, births: u32, capacity: u32) -> ParticleEmitter {
    ParticleEmitter {
        key: 7,
        step: 1,
        seconds: 1.0 / 60.0,
        capacity,
        born: 0,
        births,
        spawn: Mat4::from_scale_rotation_translation(
            Vec3::splat(spread_out),
            glam::Quat::IDENTITY,
            at,
        ),
        direction: Vec3::Y,
        spread: 0.0,
        speed: 0.0,
        lifetime: 10.0,
        acceleration: Vec3::ZERO,
        drag: 0.0,
        air: Vec3::ZERO,
        look: ParticleLook {
            color: [[1.0, 0.0, 0.0]; 2],
            transparency: [0.0; 2],
            ..ParticleLook::default()
        },
    }
}

fn renderer() -> Option<TextureRenderer> {
    let (device, queue) = test_device()?;
    Some(TextureRenderer::new(&device, &queue, SIZE, SIZE))
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    pixels[((y * SIZE + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}

#[test]
fn an_emitter_of_a_hundred_thousand_particles_draws_and_is_freed_without_it() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(Vec::new());
    let mut cloud = emitter(Vec3::new(0.0, 0.0, -10.0), 6.0, 100_000, 1 << 17);
    cloud.look.size = [0.1; 2];
    list.particle_emitters = vec![cloud];
    renderer.render(&list).unwrap();
    let stats = renderer.stats();
    assert_eq!(stats.gpu_particles, 1 << 17);
    assert_eq!(renderer.scene.gpu_particle_emitters(), 1);
    let pixels = renderer.read_pixels();
    // The cloud fills the middle of the view, and only that.
    let red = (SIZE / 4..SIZE * 3 / 4)
        .flat_map(|y| (SIZE / 4..SIZE * 3 / 4).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 100)
        .count();
    let middle = (SIZE / 2 * SIZE / 2) as usize;
    assert!(red > middle * 3 / 4, "{red} of {middle} red");
    assert_eq!(pixel(&pixels, 0, 0)[0], 0);

    // A list without it frees its buffers, and draws nothing.
    list.particle_emitters.clear();
    renderer.render(&list).unwrap();
    assert_eq!(renderer.stats().gpu_particles, 0);
    assert_eq!(renderer.scene.gpu_particle_emitters(), 0);
    assert!(renderer.scene.gpu_particles.is_none());
    assert_eq!(pixel(&renderer.read_pixels(), SIZE / 2, SIZE / 2)[0], 0);
}

#[test]
fn gpu_particles_look_like_the_same_sprites_drawn_from_the_cpu() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Every particle the same, still, at one point: what they look like
    // can't depend on which was born where.
    let at = Vec3::new(0.5, -0.3, -6.0);
    let mut gpu = emitter(at, 0.0, 3, 1024);
    gpu.look = ParticleLook {
        size: [2.5; 2],
        color: [[0.9, 0.5, 0.2]; 2],
        transparency: [0.6; 2],
        rotation_degrees: 30.0,
        spin_degrees: 0.0,
        brightness: 1.5,
        light_emission: 0.4,
        texture: None,
    };
    let mut on_gpu = list_of(vec![big_block()]);
    on_gpu.items[0].transform = Mat4::from_scale_rotation_translation(
        Vec3::new(20.0, 20.0, 1.0),
        glam::Quat::IDENTITY,
        Vec3::new(0.0, 0.0, -12.0),
    );
    let mut on_cpu = on_gpu.clone();
    on_gpu.particle_emitters = vec![gpu];
    on_cpu.sprites = vec![
        crate::Sprite {
            position: at,
            size: 2.5,
            rotation_degrees: 30.0,
            color: [0.9, 0.5, 0.2],
            opacity: 0.4,
            brightness: 1.5,
            light_emission: 0.4,
            texture: None,
        };
        3
    ];
    renderer.render(&on_cpu).unwrap();
    let cpu = renderer.read_pixels();
    renderer.render(&on_gpu).unwrap();
    let gpu = renderer.read_pixels();
    let most = cpu
        .iter()
        .zip(&gpu)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(most <= 2, "differ by up to {most}");
    // And the sprites were drawn: the middle isn't the block's own color.
    let block = {
        let mut plain = on_cpu.clone();
        plain.sprites.clear();
        renderer.render(&plain).unwrap();
        renderer.read_pixels()
    };
    let (x, y) = (SIZE / 2 + 3, SIZE / 2 + 2);
    assert_ne!(pixel(&block, x, y), pixel(&gpu, x, y));
}

#[test]
fn particles_move_once_per_step_and_drawing_a_step_again_moves_nothing() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(Vec::new());
    let mut moving = emitter(Vec3::new(-3.0, 0.0, -8.0), 0.0, 20, 1024);
    moving.direction = Vec3::X;
    moving.speed = 10.0;
    moving.look.size = [1.5; 2];
    list.particle_emitters = vec![moving.clone()];
    let red_at = |renderer: &mut TextureRenderer, list: &DrawList, x: f32| {
        let clip = view_projection(list, 1.0) * Vec3::new(x, 0.0, -8.0).extend(1.0);
        let px = ((clip.x / clip.w * 0.5 + 0.5) * SIZE as f32) as u32;
        renderer.render(list).unwrap();
        pixel(&renderer.read_pixels(), px, SIZE / 2)[0] > 100
    };
    // Born at x = -3.
    assert!(red_at(&mut renderer, &list, -3.0));
    assert!(!red_at(&mut renderer, &list, 0.0));
    // A third of a second later, at 10 studs a second: x = 0.
    moving.step = 2;
    moving.born = 20;
    moving.births = 0;
    moving.seconds = 0.3;
    list.particle_emitters = vec![moving];
    assert!(red_at(&mut renderer, &list, 0.0));
    // The same step drawn again three times: still there.
    for _ in 0..3 {
        assert!(red_at(&mut renderer, &list, 0.0));
    }
    assert!(!red_at(&mut renderer, &list, -3.0));
    assert!(!red_at(&mut renderer, &list, 3.0));
}

#[test]
fn gpu_particles_are_drawn_farthest_first() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(Vec::new());
    // One particle, red when born and blue as it dies, near the camera.
    let mut fading = emitter(Vec3::new(0.0, 0.0, -5.0), 0.0, 1, 1024);
    fading.lifetime = 2.0;
    fading.look.size = [3.0; 2];
    fading.look.color = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    // A second later, one more, newborn and red, behind it in the next
    // slot. Drawn in slot order, it would cover the first.
    fading.step = 2;
    list.particle_emitters = vec![fading.clone()];
    renderer.render(&list).unwrap();
    fading.step = 3;
    fading.seconds = 1.0;
    fading.born = 1;
    fading.spawn = Mat4::from_translation(Vec3::new(0.0, 0.0, -8.0)) * Mat4::from_scale(Vec3::ZERO);
    list.particle_emitters = vec![fading];
    renderer.render(&list).unwrap();
    let [red, _, blue, _] = pixel(&renderer.read_pixels(), SIZE / 2, SIZE / 2);
    assert!(
        blue > 60 && red < 250,
        "the nearer, older one is on top: {red} {blue}"
    );
}

#[test]
fn without_compute_shaders_gpu_emitters_draw_nothing_and_keep_nothing() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    renderer.limit_capabilities(&Capabilities {
        compute_shaders: false,
        ..*renderer.capabilities()
    });
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![emitter(Vec3::new(0.0, 0.0, -10.0), 4.0, 5000, 8192)];
    renderer.render(&list).unwrap();
    assert!(renderer.scene.gpu_particles.is_none());
    assert_eq!(renderer.stats().gpu_particles, 0);
    assert_eq!(pixel(&renderer.read_pixels(), SIZE / 2, SIZE / 2)[0], 0);
}
