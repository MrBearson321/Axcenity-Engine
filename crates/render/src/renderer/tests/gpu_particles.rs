//! GPU particles: a hundred thousand of them, drawn as the same sprites
//! would be, moved once per step, sorted farthest first, and kept only
//! while the list has their emitter and the device runs compute shaders.

use super::*;
use crate::{ParticleEmitter, ParticleLook, SpawnShape};

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
        shape: SpawnShape::default(),
        in_out: crate::ShapeInOut::Outward,
        inherit: Vec3::ZERO,
        collision: None,
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
        curves: None,
        flipbook: None,
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
            frame: None,
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

/// R17.1: with curves, GPU particles draw the curves' size, transparency
/// and color in place of the start and end pairs, as the CPU's sprites
/// sampled from the same table would.
#[test]
fn gpu_particles_draw_their_curves_in_place_of_the_start_and_end_values() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let at = Vec3::new(0.5, -0.3, -6.0);
    // The pairs say small and blue; the curves say big and orange, the
    // same all life long, so each particle's age doesn't matter.
    let curves = crate::ParticleCurves::from_fn(|_| ([2.5, 0.0, 0.6, 0.0], [0.9, 0.5, 0.2]));
    let (size, transparency, color) = curves.sample(0.3, [0.7, -0.2]);
    assert_eq!((size, transparency), (2.5, 0.6));
    let mut gpu = emitter(at, 0.0, 3, 1024);
    gpu.look = ParticleLook {
        size: [0.2; 2],
        color: [[0.0, 0.0, 1.0]; 2],
        transparency: [0.0; 2],
        curves: Some(std::sync::Arc::new(curves)),
        ..ParticleLook::default()
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
            size,
            rotation_degrees: 0.0,
            color,
            opacity: 1.0 - transparency,
            brightness: 1.0,
            light_emission: 0.0,
            texture: None,
            frame: None,
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

/// R17.2: every spawn shape's points stay in the cube, inside the shape,
/// or on its surface when only the surface is asked for; a box's volume is
/// born as before, and its surface spreads over the faces by their area.
#[test]
fn spawn_shapes_stay_inside_their_shape_or_on_its_surface() {
    use crate::SpawnKind;
    let steps = [0.0, 0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.95, 0.999_999];
    let randoms: Vec<[f32; 3]> = steps
        .iter()
        .flat_map(|&a| steps.iter().flat_map(move |&b| steps.map(|c| [a, b, c])))
        .collect();
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    for kind in [
        SpawnKind::Box,
        SpawnKind::Sphere,
        SpawnKind::Cylinder,
        SpawnKind::Disc,
    ] {
        for surface in [false, true] {
            let shape = SpawnShape { kind, surface };
            for &random in &randoms {
                let point = shape.point(random, Vec3::new(4.0, 1.0, 2.0));
                assert!(
                    point.abs().max_element() <= 0.5 + 1e-5,
                    "{shape:?} {random:?} → {point}"
                );
                let round = point.x.hypot(point.z);
                match (kind, surface) {
                    (SpawnKind::Box, false) => {
                        assert_eq!(point, Vec3::from(random) - 0.5)
                    }
                    (SpawnKind::Box, true) => assert!(close(point.abs().max_element(), 0.5)),
                    (SpawnKind::Sphere, false) => assert!(point.length() <= 0.5 + 1e-5),
                    (SpawnKind::Sphere, true) => assert!(close(point.length(), 0.5)),
                    (SpawnKind::Cylinder, false) => assert!(round <= 0.5 + 1e-5),
                    (SpawnKind::Cylinder, true) => assert!(close(round, 0.5)),
                    (SpawnKind::Disc, false) => {
                        assert!(point.y == 0.0 && round <= 0.5 + 1e-5)
                    }
                    (SpawnKind::Disc, true) => assert!(point.y == 0.0 && close(round, 0.5)),
                }
            }
        }
    }
    // A 4 × 1 × 1 box: its two square ends are a ninth of its surface.
    let surface = SpawnShape {
        kind: SpawnKind::Box,
        surface: true,
    };
    let ends = (0..900)
        .map(|n| {
            surface.point(
                [(n as f32 + 0.5) / 900.0, 0.5, 0.5],
                Vec3::new(4.0, 1.0, 1.0),
            )
        })
        .filter(|point| close(point.x.abs(), 0.5))
        .count();
    assert_eq!(ends, 100);
}

/// R17.2: born on a disc's rim facing the camera, GPU particles leave its
/// middle empty; born anywhere on the disc, they fill it.
#[test]
fn gpu_particles_are_born_in_their_spawn_shape() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let at = Vec3::new(0.0, 0.0, -10.0);
    let mut rim = emitter(at, 0.0, 3000, 4096);
    // A disc 5 studs across, turned to face the camera.
    rim.spawn = Mat4::from_scale_rotation_translation(
        Vec3::splat(5.0),
        glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        at,
    );
    rim.look.size = [0.3; 2];
    rim.shape = SpawnShape {
        kind: crate::SpawnKind::Disc,
        surface: true,
    };
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![rim.clone()];
    let red = |pixel: [u8; 4]| pixel[0] > 100 && pixel[1] < 60;
    assert!(!red(pixel_at(&mut renderer, &list, at)));
    for side in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y] {
        assert!(
            red(pixel_at(&mut renderer, &list, at + side * 2.5)),
            "{side}"
        );
    }
    let mut filled = rim;
    filled.key = 8;
    filled.shape.surface = false;
    list.particle_emitters = vec![filled];
    assert!(red(pixel_at(&mut renderer, &list, at)));
}

/// R17.2: a speed curve scales how far GPU particles move each step, read
/// at their age, as the CPU's `curve_at` does.
#[test]
fn gpu_particles_move_at_their_speed_curve() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut curves = crate::ParticleCurves::from_fn(|_| ([1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    curves.speed = [0.5; crate::CURVE_ENTRIES];
    assert_eq!(crate::curve_at(&curves.speed, 0.0), 0.5);
    let mut moving = emitter(Vec3::new(-3.0, 0.0, -8.0), 0.0, 20, 1024);
    moving.direction = Vec3::X;
    moving.speed = 10.0;
    moving.look.curves = Some(std::sync::Arc::new(curves));
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![moving.clone()];
    let red = |pixel: [u8; 4]| pixel[0] > 100;
    assert!(red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(-3.0, 0.0, -8.0)
    )));
    // A third of a second at half of 10 studs a second: x = -1.5, not 0.
    moving.step = 2;
    moving.born = 20;
    moving.births = 0;
    moving.seconds = 0.3;
    list.particle_emitters = vec![moving];
    assert!(red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(-1.5, 0.0, -8.0)
    )));
    assert!(!red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(0.0, 0.0, -8.0)
    )));
    assert!(!red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(-3.0, 0.0, -8.0)
    )));
}

/// R17.2b: born on a disc's rim heading Inward, GPU particles meet in its
/// middle; a Box emitter's particles take the inherited velocity.
#[test]
fn gpu_particles_head_inward_and_inherit_velocity() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let at = Vec3::new(0.0, 0.0, -10.0);
    let mut inward = emitter(at, 0.0, 2000, 4096);
    // A disc 5 studs across, turned to face the camera.
    inward.spawn = Mat4::from_scale_rotation_translation(
        Vec3::splat(5.0),
        glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        at,
    );
    inward.shape = SpawnShape {
        kind: crate::SpawnKind::Disc,
        surface: true,
    };
    inward.in_out = crate::ShapeInOut::Inward;
    inward.speed = 2.5 / 0.3;
    inward.look.size = [0.4; 2];
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![inward.clone()];
    let red = |pixel: [u8; 4]| pixel[0] > 100;
    assert!(!red(pixel_at(&mut renderer, &list, at)), "a ring");
    assert!(red(pixel_at(&mut renderer, &list, at + Vec3::X * 2.5)));
    // A third of a second later, at 2.5 studs in that time: in the middle.
    inward.step = 2;
    inward.born = 2000;
    inward.births = 0;
    inward.seconds = 0.3;
    list.particle_emitters = vec![inward];
    assert!(red(pixel_at(&mut renderer, &list, at)), "met in the middle");
    assert!(!red(pixel_at(&mut renderer, &list, at + Vec3::X * 2.5)));

    // Still, but given 10 studs a second along X by the emitter.
    let mut carried = emitter(Vec3::new(-3.0, 0.0, -8.0), 0.0, 20, 1024);
    carried.key = 9;
    carried.inherit = Vec3::new(10.0, 0.0, 0.0);
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![carried.clone()];
    assert!(red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(-3.0, 0.0, -8.0)
    )));
    carried.step = 2;
    carried.born = 20;
    carried.births = 0;
    carried.seconds = 0.3;
    list.particle_emitters = vec![carried];
    assert!(red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(0.0, 0.0, -8.0)
    )));
    assert!(!red(pixel_at(
        &mut renderer,
        &list,
        Vec3::new(-3.0, 0.0, -8.0)
    )));
}

/// A 2 × 2 flipbook: red, green, then blue, yellow, each frame 16 pixels.
fn four_frames() -> Arc<dyn TextureSource> {
    Arc::new(Painted::new(32, |x, y| match (x / 16, y / 16) {
        (0, 0) => [255, 0, 0, 255],
        (1, 0) => [0, 255, 0, 255],
        (0, _) => [0, 0, 255, 255],
        _ => [255, 255, 0, 255],
    }))
}

/// Which of the four frames' colors the middle of the view shows.
fn frame_shown(pixels: &[u8]) -> Option<u32> {
    let [r, g, b, _] = pixel(pixels, SIZE / 2, SIZE / 2);
    let (r, g, b) = (r > 150, g > 150, b > 150);
    match (r, g, b) {
        (true, false, false) => Some(0),
        (false, true, false) => Some(1),
        (false, false, true) => Some(2),
        (true, true, false) => Some(3),
        _ => None,
    }
}

/// R17.2c: a sprite's flipbook frame draws that square of its texture,
/// left to right, then top to bottom.
#[test]
fn a_sprite_draws_its_flipbook_frame_of_the_texture() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(Vec::new());
    for frame in 0..4 {
        list.sprites = vec![crate::Sprite {
            position: Vec3::new(0.0, 0.0, -6.0),
            size: 4.0,
            rotation_degrees: 0.0,
            color: [1.0; 3],
            opacity: 1.0,
            brightness: 1.0,
            light_emission: 0.0,
            texture: Some(four_frames()),
            frame: Some(crate::FlipbookFrame { grid: 2, frame }),
        }];
        renderer.render(&list).unwrap();
        assert_eq!(frame_shown(&renderer.read_pixels()), Some(frame));
    }
}

/// R17.2c: GPU particles pick their flipbook frame as `Flipbook::frame`
/// does: looping at 4 frames a second, about 0.8 seconds old shows the
/// fourth frame.
#[test]
fn gpu_particles_show_their_flipbook_frame_for_their_age() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut gpu = emitter(Vec3::new(0.0, 0.0, -6.0), 0.0, 1, 1024);
    gpu.seconds = 0.0;
    gpu.look = ParticleLook {
        size: [4.0; 2],
        color: [[1.0; 3]; 2],
        transparency: [0.0; 2],
        texture: Some(four_frames()),
        flipbook: Some(crate::Flipbook {
            grid: 2,
            mode: crate::FlipbookMode::Loop,
            framerate: 4.0,
            start_random: false,
        }),
        ..ParticleLook::default()
    };
    let mut list = list_of(Vec::new());
    list.particle_emitters = vec![gpu.clone()];
    renderer.render(&list).unwrap();
    assert_eq!(frame_shown(&renderer.read_pixels()), Some(0), "just born");
    gpu.step = 2;
    gpu.born = 1;
    gpu.births = 0;
    gpu.seconds = 0.8;
    list.particle_emitters = vec![gpu];
    renderer.render(&list).unwrap();
    assert_eq!(frame_shown(&renderer.read_pixels()), Some(3));
}

/// R17.2c: each flipbook mode's frame for a particle's age.
#[test]
fn flipbook_modes_pick_their_frames_by_age() {
    let book = |mode, start_random| crate::Flipbook {
        grid: 2,
        mode,
        framerate: 2.0,
        start_random,
    };
    use crate::FlipbookMode::*;
    // Loop: two frames a second, round again after the fourth.
    let frames: Vec<u32> = (0..10)
        .map(|half| book(Loop, false).frame(half as f32 * 0.5 + 0.1, 100.0, 7))
        .collect();
    assert_eq!(frames, [0, 1, 2, 3, 0, 1, 2, 3, 0, 1]);
    // PingPong: up and back down, never showing an end twice in a row.
    let frames: Vec<u32> = (0..8)
        .map(|half| book(PingPong, false).frame(half as f32 * 0.5 + 0.1, 100.0, 7))
        .collect();
    assert_eq!(frames, [0, 1, 2, 3, 2, 1, 0, 1]);
    // OneShot: each frame a quarter of the life, whatever the rate.
    let frames: Vec<u32> = [0.0, 0.3, 0.6, 0.9, 1.0, 2.0]
        .iter()
        .map(|&age| book(OneShot, false).frame(age, 1.0, 7))
        .collect();
    assert_eq!(frames, [0, 1, 2, 3, 3, 3]);
    // Random: any frame, the same for the same particle and moment.
    let mut seen = [false; 4];
    for seed in 0..64 {
        let frame = book(Random, false).frame(0.7, 1.0, seed);
        assert!(frame < 4);
        assert_eq!(frame, book(Random, false).frame(0.7, 1.0, seed));
        seen[frame as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "{seen:?}");
    // A random start spreads particles over the frames, then loops on.
    let starts: std::collections::HashSet<u32> = (0..64)
        .map(|seed| book(Loop, true).frame(0.0, 1.0, seed))
        .collect();
    assert_eq!(starts.len(), 4);
    let first = book(Loop, true).frame(0.0, 1.0, 5);
    assert_eq!(book(Loop, true).frame(0.6, 1.0, 5), (first + 1) % 4);
    // A broken rate stays on the first frame.
    let still = crate::Flipbook {
        framerate: f32::NAN,
        ..book(Loop, false)
    };
    assert_eq!(still.frame(5.0, 10.0, 1), 0);
}

/// R17.4b: a colliding emitter's particles land on the wall the prepass
/// shows in front of them and stay there, where others pass behind it,
/// and a lifetime loss of 1 kills them there.
#[test]
fn colliding_gpu_particles_land_on_the_wall_in_front_of_them() {
    let Some(mut renderer) = renderer() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // A block whose face toward the camera is at z = -10.
    let wall = big_block_at(Vec3::new(0.0, 0.0, -12.0));
    let start = Vec3::new(0.0, 0.0, -6.0);
    let thrown = |collision| {
        let mut thrown = emitter(start, 0.0, 50, 1024);
        thrown.direction = Vec3::NEG_Z;
        thrown.speed = 10.0;
        thrown.look.size = [1.0; 2];
        thrown.collision = collision;
        thrown
    };
    let red = |pixel: [u8; 4]| pixel[0] > 100 && pixel[1] < 60;
    for (collision, lands) in [
        (
            Some(crate::ParticleCollision {
                bounce: 0.0,
                lifetime_loss: 0.0,
            }),
            true,
        ),
        (None, false),
        // Dying where they land.
        (
            Some(crate::ParticleCollision {
                bounce: 0.0,
                lifetime_loss: 1.0,
            }),
            false,
        ),
    ] {
        let mut list = list_of(vec![wall.clone()]);
        let mut particles = thrown(collision);
        list.particle_emitters = vec![particles.clone()];
        assert!(red(pixel_at(&mut renderer, &list, start)), "born");
        // Two steps of 0.3 seconds: 3 studs short of the wall, then 2
        // past its face.
        for step in 2..=4 {
            particles.step = step;
            particles.born = 50;
            particles.births = 0;
            particles.seconds = 0.3;
            list.particle_emitters = vec![particles.clone()];
            renderer.render(&list).unwrap();
        }
        let face = Vec3::new(0.0, 0.0, -10.0);
        assert_eq!(
            red(pixel_at(&mut renderer, &list, face)),
            lands,
            "collision {collision:?}"
        );
        // A new key for the next emitter, so it starts afresh.
        renderer.render(&list_of(Vec::new())).unwrap();
    }
}
