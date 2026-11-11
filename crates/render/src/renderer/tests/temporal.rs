//! Motion vectors, TAA and motion blur: the motion target holds how far each
//! pixel moved (moved parts, posed meshes and the camera alike), TAA smooths
//! edges and thin shapes over frames without leaving ghosts behind a moving
//! part, motion blur streaks only what moves relative to the camera, and
//! their targets exist only while they're used.

use super::skinning::{skin, square};
use super::*;
use crate::graphics::{AmbientOcclusion, AntiAliasing, MotionBlurQuality, Quality};
use crate::{GraphicsSettings, LensEffects, Shading};
use glam::Vec2;

/// The frame size, square.
const SIZE: u32 = 128;

/// High with `anti_aliasing` and `motion_blur`, and nothing else that keeps
/// the prepass alive.
fn settings(anti_aliasing: AntiAliasing, motion_blur: MotionBlurQuality) -> GraphicsSettings {
    GraphicsSettings {
        anti_aliasing: Some(anti_aliasing),
        motion_blur: Some(motion_blur),
        ambient_occlusion: Some(AmbientOcclusion::Off),
        contact_shadows: Some(false),
        volumetric_fog: Some(false),
        ..GraphicsSettings::preset(Quality::High)
    }
}

fn renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    settings: &GraphicsSettings,
) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(settings);
    renderer
}

fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    pixels[((y * SIZE + x) * 4) as usize..][..3]
        .try_into()
        .unwrap()
}

/// An unlit box of `color` (sRGB), `size` studs, at `center`, known as `id`.
fn card(color: [f32; 3], center: Vec3, size: Vec3, id: u64) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, center),
        material: Material {
            base_color: color,
            shading: Shading::Unlit,
            ..Material::default()
        },
        id,
        ..big_block()
    }
}

/// A grey wall across the back of the view.
fn wall(z: f32) -> DrawItem {
    card(
        [0.5; 3],
        Vec3::new(0.0, 0.0, z),
        Vec3::new(60.0, 60.0, 0.1),
        1,
    )
}

/// A white cube `size` studs wide at `center`.
fn cube(center: Vec3, size: f32, id: u64) -> DrawItem {
    card([1.0; 3], center, Vec3::splat(size), id)
}

/// Where `point` is on screen for `list`'s camera, in texture coordinates.
fn uv_of(list: &DrawList, point: Vec3) -> Vec2 {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    Vec2::new(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5)
}

fn pixel_of(uv: Vec2) -> (u32, u32) {
    let at = (uv * SIZE as f32).floor();
    (at.x as u32, at.y as u32)
}

/// The pixels around everything `items` cover on screen, `margin` pixels
/// wider each way: left, top, right and bottom.
fn screen_box(list: &DrawList, items: &[&DrawItem], margin: f32) -> [u32; 4] {
    let corners = items.iter().flat_map(|item| {
        [0, 1, 2, 3, 4, 5, 6, 7].map(|bits| {
            let corner = Vec3::new(
                if bits & 1 == 0 { -0.5 } else { 0.5 },
                if bits & 2 == 0 { -0.5 } else { 0.5 },
                if bits & 4 == 0 { -0.5 } else { 0.5 },
            );
            uv_of(list, item.transform.transform_point3(corner)) * SIZE as f32
        })
    });
    let (low, high) = corners.fold((Vec2::splat(1e9), Vec2::splat(-1e9)), |(low, high), at| {
        (low.min(at), high.max(at))
    });
    let low = (low - margin).max(Vec2::ZERO);
    let high = (high + margin).min(Vec2::splat(SIZE as f32 - 1.0));
    [low.x as u32, low.y as u32, high.x as u32, high.y as u32]
}

fn inside([left, top, right, bottom]: [u32; 4], x: u32, y: u32) -> bool {
    (left..=right).contains(&x) && (top..=bottom).contains(&y)
}

/// The motion target the last frame drew: how far each pixel moved since
/// the frame before, in texture coordinates.
fn read_motion(renderer: &TextureRenderer) -> Vec<Vec2> {
    let scene = &renderer.scene;
    let texture = scene
        .frame_targets
        .temporal
        .as_ref()
        .expect("motion targets")
        .last_motion(&scene.history);
    let (width, height) = (texture.width(), texture.height());
    let row_bytes = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = scene.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("motion read back"),
        size: u64::from(row_bytes * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = scene.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    scene.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("could not read the motion back");
    });
    scene
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .expect("the buffer is mapped");
    mapped
        .chunks(row_bytes as usize)
        .flat_map(|row| row[..(width * 4) as usize].chunks(4))
        .map(|texel| {
            let half = |at: usize| half_to_f32(u16::from_le_bytes([texel[at], texel[at + 1]]));
            Vec2::new(half(0), half(2))
        })
        .collect()
}

/// A 16-bit float's value.
fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff) / 1024.0;
    sign * match exponent {
        0 => fraction * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + fraction) * 2f32.powi(exponent - 15),
    }
}

fn motion_at(motion: &[Vec2], (x, y): (u32, u32)) -> Vec2 {
    motion[(y * SIZE + x) as usize]
}

#[test]
fn motion_vectors_hold_how_far_moved_parts_posed_meshes_and_the_camera_went() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Taa, MotionBlurQuality::Off),
    );
    // A part moved half a stud to the right; the wall stays.
    let at = |x: f32| list_of(vec![wall(-10.0), cube(Vec3::new(x, 0.0, -5.0), 1.0, 2)]);
    let (before, now) = (at(-1.0), at(-0.5));
    renderer.render(&before).unwrap();
    renderer.render(&now).unwrap();
    let motion = read_motion(&renderer);
    // The middle of its face toward the camera, half a stud in front.
    let center = Vec3::new(-0.5, 0.0, -4.5);
    let expected = uv_of(&now, center) - uv_of(&before, Vec3::new(-1.0, 0.0, -4.5));
    let moved = motion_at(&motion, pixel_of(uv_of(&now, center)));
    assert!(expected.x > 0.02, "{expected}");
    assert!(moved.distance(expected) < 0.002, "{moved} vs {expected}");
    let still = motion_at(&motion, pixel_of(uv_of(&now, Vec3::new(3.0, 3.0, -10.0))));
    assert!(still.length() < 1e-4, "the wall didn't move: {still}");

    // The camera and the part both move another half stud right: the part
    // is still on screen, and the wall moves left.
    let mut follow = at(0.0);
    follow.camera.position.x += 0.5;
    follow.camera.look_at.x += 0.5;
    renderer.render(&follow).unwrap();
    let motion = read_motion(&renderer);
    let moved = motion_at(&motion, pixel_of(uv_of(&follow, Vec3::new(0.0, 0.0, -4.5))));
    assert!(
        moved.length() < 0.001,
        "the part kept its place on screen: {moved}"
    );
    let point = Vec3::new(3.0, 3.0, -10.0);
    let expected = uv_of(&follow, point) - uv_of(&now, point);
    let wall_moved = motion_at(&motion, pixel_of(uv_of(&follow, point)));
    assert!(expected.x < -0.01, "{expected}");
    assert!(
        wall_moved.distance(expected) < 0.001,
        "{wall_moved} vs {expected}"
    );

    // A posed mesh whose joint moves (its transform stays) moves with it.
    let posed = |x: f32| {
        let mut item = square(0, Some(skin(1, Vec3::new(x - 30.0, 0.0, 0.0))));
        item.id = 3;
        item.material.shading = Shading::Unlit;
        list_of(vec![wall(-10.0), item])
    };
    let (before, now) = (posed(0.0), posed(0.25));
    renderer.render(&before).unwrap();
    renderer.render(&now).unwrap();
    let motion = read_motion(&renderer);
    let center = Vec3::new(0.25, 0.0, -3.0);
    let expected = uv_of(&now, center) - uv_of(&before, Vec3::new(0.0, 0.0, -3.0));
    let moved = motion_at(&motion, pixel_of(uv_of(&now, center)));
    assert!(expected.x > 0.02, "{expected}");
    assert!(
        moved.distance(expected) < 0.002,
        "posed: {moved} vs {expected}"
    );
}

#[test]
fn taa_smooths_edges_and_thin_geometry_over_frames() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let after = |list: &DrawList, mode, frames| {
        let mut renderer = renderer(&device, &queue, &settings(mode, MotionBlurQuality::Off));
        assert_eq!(renderer.graphics().anti_aliasing, mode);
        for _ in 1..frames {
            renderer.render(list).unwrap();
        }
        draw(&mut renderer, list)
    };
    let in_between = |pixels: &[u8]| {
        pixels
            .chunks(4)
            .filter(|pixel| (10..=245).contains(&pixel[0]))
            .count()
    };
    // A tilted block's edge over the sky.
    let block = tilted_block();
    let off = after(&block, AntiAliasing::Off, 1);
    assert_eq!(in_between(&off), 0, "hard steps");
    let first = after(&block, AntiAliasing::Taa, 1);
    assert_eq!(in_between(&first), 0, "one frame has no history yet");
    let smoothed = after(&block, AntiAliasing::Taa, 16);
    let inside = smoothed.chunks(4).filter(|pixel| pixel[0] > 245).count();
    assert!(inside > 1000, "the block is still drawn whole: {inside}");
    let smoothed = in_between(&smoothed);
    assert!(smoothed >= 60, "TAA blends edge pixels: {smoothed}");

    // A glowing bar about a pixel thick, a little tilted: without
    // anti-aliasing it's a staircase of whole pixels. With TAA each column
    // shares its light between the pixels it crosses, and keeps about as
    // much of it (none is clamped away).
    let bar = {
        let item = DrawItem {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(5.0, 0.066, 0.05),
                glam::Quat::from_rotation_z(4f32.to_radians()),
                Vec3::new(0.0, 0.0, -5.0),
            ),
            material: Material {
                base_color: [0.0; 3],
                emissive: [1.0; 3],
                ..Material::default()
            },
            ..big_block()
        };
        let mut list = list_of(vec![item]);
        list.shadows.enabled = false;
        list
    };
    let linear = |value: u8| linear_color([f32::from(value) / 255.0; 3]).x;
    let columns = |pixels: &[u8]| {
        (25..SIZE - 25)
            .map(|x| {
                (0..SIZE)
                    .map(|y| linear(pixel(pixels, x, y)[0]))
                    .sum::<f32>()
            })
            .collect::<Vec<_>>()
    };
    let stepped = after(&bar, AntiAliasing::Off, 1);
    assert_eq!(in_between(&stepped), 0, "whole pixels only");
    let thin = after(&bar, AntiAliasing::Taa, 24);
    let shared = in_between(&thin);
    assert!(shared >= 60, "the bar's pixels share its light: {shared}");
    for (x, (theirs, ours)) in columns(&stepped).iter().zip(columns(&thin)).enumerate() {
        assert!(*theirs >= 0.99, "column {x}: the bar crosses every column");
        assert!(
            (0.5..=2.2).contains(&ours),
            "column {x}: TAA keeps the bar's light: {ours} (without, {theirs})"
        );
    }
}

#[test]
fn taa_leaves_no_ghost_behind_a_moving_part() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // A white part crossing a grey wall, a third of a stud (about 6 pixels)
    // a frame.
    let at = |frame: u32| {
        let x = -3.0 + frame as f32 / 3.0;
        list_of(vec![wall(-6.0), cube(Vec3::new(x, 0.0, -5.0), 1.5, 2)])
    };
    let mut taa = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Taa, MotionBlurQuality::Off),
    );
    let frames = 12;
    for frame in 0..frames {
        taa.render(&at(frame)).unwrap();
    }
    let last = at(frames - 1);
    let ours = draw(&mut taa, &last);
    let mut off = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Off, MotionBlurQuality::Off),
    );
    let theirs = draw(&mut off, &last);
    // Away from the part's edges (where the anti-aliasing differs), and
    // above all where it was in the frames before, TAA shows the wall.
    let part = screen_box(&last, &[&last.items[1]], 2.0);
    let trail = screen_box(&at(frames - 6), &[&at(frames - 6).items[1]], 0.0);
    assert!(trail[0] < part[0], "the part was to the left");
    let mut trail_pixels = 0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            if inside(part, x, y) {
                continue;
            }
            let (a, b) = (pixel(&ours, x, y), pixel(&theirs, x, y));
            let difference = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
            assert!(
                difference <= 6,
                "({x}, {y}): {a:?} vs {b:?}, a ghost where the part was"
            );
            trail_pixels += usize::from(inside(trail, x, y));
        }
    }
    assert!(trail_pixels > 200, "the trail was checked: {trail_pixels}");
    // The part itself kept up with its motion: its middle is white.
    let middle = pixel_of(uv_of(&last, last.items[1].transform.w_axis.truncate()));
    assert_eq!(pixel(&ours, middle.0, middle.1), [255; 3]);
}

#[test]
fn motion_blur_streaks_only_what_moves_relative_to_the_camera() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // A part that stays on the left, and one moving right on the right.
    let at = |moving: f32, camera: f32| {
        let mut list = list_of(vec![
            wall(-8.0),
            cube(Vec3::new(-2.5, 0.0, -5.0), 1.2, 2),
            cube(Vec3::new(moving, 0.0, -5.0), 1.2, 3),
        ]);
        list.camera.position.x += camera;
        list.camera.look_at.x += camera;
        list.lens = LensEffects {
            motion_blur: Some(1.0),
            ..LensEffects::default()
        };
        list
    };
    let frames = [at(1.2, 0.0), at(1.8, 0.0), at(2.4, 0.6)];
    let mut blurred = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Off, MotionBlurQuality::High),
    );
    let mut sharp = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Off, MotionBlurQuality::Off),
    );
    let changed = |ours: &[u8], theirs: &[u8], area: [u32; 4]| {
        let mut changed = 0;
        let mut most = 0;
        for y in area[1]..=area[3] {
            for x in area[0]..=area[2] {
                let (a, b) = (pixel(ours, x, y), pixel(theirs, x, y));
                let difference = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                most = most.max(difference);
                changed += usize::from(difference > 20);
            }
        }
        (changed, most)
    };
    let mut drawn = Vec::new();
    for list in &frames {
        drawn.push((draw(&mut blurred, list), draw(&mut sharp, list)));
    }
    // The first frame has nothing to compare with: nothing moves.
    assert_eq!(drawn[0].0, drawn[0].1);

    // With the camera still, the moving part streaks along its motion and
    // the still one stays exactly as it was.
    let list = &frames[1];
    let (ours, theirs) = &drawn[1];
    let still = screen_box(list, &[&list.items[1]], 8.0);
    let moving = screen_box(list, &[&list.items[2]], 8.0);
    assert_eq!(
        changed(ours, theirs, still),
        (0, 0),
        "the still part is sharp"
    );
    let (streaked, _) = changed(ours, theirs, moving);
    assert!(streaked > 60, "the moving part streaks: {streaked} pixels");
    // Not up and down: the rows above and below it are untouched.
    let [left, top, right, bottom] = moving;
    assert_eq!(changed(ours, theirs, [left, top, right, top + 3]).0, 0);
    assert_eq!(
        changed(ours, theirs, [left, bottom - 3, right, bottom]).0,
        0
    );

    // The camera follows the moving part: now it's the one that stays
    // sharp, and the still part (and the wall's edges, which it has none
    // of) streaks.
    let list = &frames[2];
    let (ours, theirs) = &drawn[2];
    let still = screen_box(list, &[&list.items[1]], 8.0);
    let moving = screen_box(list, &[&list.items[2]], 8.0);
    assert!(
        changed(ours, theirs, moving).1 <= 2,
        "the followed part is sharp"
    );
    let (streaked, _) = changed(ours, theirs, still);
    assert!(
        streaked > 60,
        "the part left behind streaks: {streaked} pixels"
    );
}

#[test]
fn motion_targets_exist_only_while_taa_or_motion_blur_use_them() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let plain = list_of(vec![wall(-8.0), cube(Vec3::new(0.0, 0.0, -5.0), 1.0, 2)]);
    let streaked = DrawList {
        lens: LensEffects {
            motion_blur: Some(0.5),
            ..LensEffects::default()
        },
        ..plain.clone()
    };
    let mut renderer = renderer(
        &device,
        &queue,
        &settings(AntiAliasing::Off, MotionBlurQuality::High),
    );
    // The draws of a frame without the prepass.
    let mut without = None;
    let mut check = |settings: GraphicsSettings, list: &DrawList, alive, prepass: bool| {
        renderer.set_graphics(&settings);
        renderer.render(list).unwrap();
        renderer.render(list).unwrap();
        let scene = &renderer.scene;
        assert_eq!(scene.temporal_targets_alive(), alive, "{settings:?}");
        assert_eq!(
            scene.frame_targets.occlusion.is_some(),
            prepass,
            "{settings:?}"
        );
        assert_eq!(scene.targets_alive(), 1);
        // Nothing moved, so the prepass's draws are the only extra ones:
        // there's no moving items' pass.
        let stats = renderer.stats();
        let base = *without.get_or_insert(stats.draw_calls);
        let expected = base + if prepass { stats.batches } else { 0 };
        assert_eq!(stats.draw_calls, expected, "{settings:?}");
    };
    use AntiAliasing::{Off, Taa};
    use MotionBlurQuality as Blur;
    // Nothing to streak and no TAA: nothing kept, no prepass.
    check(settings(Off, Blur::High), &plain, (0, 0, 0), false);
    // Motion blur asked for but turned off in the settings: the same.
    check(settings(Off, Blur::Off), &streaked, (0, 0, 0), false);
    check(settings(Off, Blur::Low), &streaked, (1, 0, 1), true);
    check(settings(Taa, Blur::Off), &plain, (1, 1, 0), true);
    check(settings(Taa, Blur::High), &streaked, (1, 1, 1), true);
    check(settings(Off, Blur::High), &plain, (0, 0, 0), false);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}
