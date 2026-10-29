//! Lens effects: depth of field, a vignette, film grain, chromatic
//! aberration and sharpening each change the frame as they say, and cost
//! nothing (no targets, no change to the frame) while absent or turned off
//! in the graphics settings.

use super::*;
use crate::graphics::{AmbientOcclusion, AntiAliasing, DepthOfFieldQuality, Quality};
use crate::{DepthOfField, FilmGrain, GraphicsSettings, LensEffects, Shading, Vignette};

/// The frame size, square: a fifth of 1080, so a grain size of 5 is 1 pixel.
const SIZE: u32 = 216;

/// High, with nothing else that keeps the prepass alive.
fn settings() -> GraphicsSettings {
    GraphicsSettings {
        anti_aliasing: Some(AntiAliasing::Off),
        ambient_occlusion: Some(AmbientOcclusion::Off),
        contact_shadows: Some(false),
        volumetric_fog: Some(false),
        ..GraphicsSettings::preset(Quality::High)
    }
}

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(&settings());
    renderer
}

/// The same settings with every lens effect's own setting off.
fn lens_off() -> GraphicsSettings {
    GraphicsSettings {
        depth_of_field: Some(DepthOfFieldQuality::Off),
        vignette: Some(false),
        film_grain: Some(false),
        chromatic_aberration: Some(false),
        sharpen: Some(false),
        ..settings()
    }
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

/// An unlit box of `color` (sRGB), `size` studs, at `center`.
fn card(color: [f32; 3], center: Vec3, size: Vec3) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, center),
        material: Material {
            base_color: color,
            shading: Shading::Unlit,
            ..Material::default()
        },
        ..big_block()
    }
}

/// An unlit card of `color` filling the view.
fn filled(color: [f32; 3]) -> DrawList {
    list_of(vec![card(
        color,
        Vec3::new(0.0, 0.0, -5.0),
        Vec3::new(20.0, 20.0, 0.1),
    )])
}

fn with_lens(mut list: DrawList, lens: LensEffects) -> DrawList {
    list.lens = lens;
    list
}

#[test]
fn a_vignette_darkens_or_tints_toward_the_corners() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let white = filled([1.0; 3]);
    let plain = draw(&mut renderer, &white);
    let middle = SIZE / 2;
    assert_eq!(pixel(&plain, middle, middle), [255; 3]);
    assert_eq!(pixel(&plain, 0, 0), [255; 3]);

    let vignette = |color| LensEffects {
        vignette: Some(Vignette {
            intensity: 1.0,
            size: 0.5,
            color,
        }),
        ..LensEffects::default()
    };
    let dark = draw(&mut renderer, &with_lens(white.clone(), vignette([0.0; 3])));
    assert_eq!(
        pixel(&dark, middle, middle),
        [255; 3],
        "the middle is untouched"
    );
    assert!(pixel(&dark, 0, 0)[0] < 10, "{:?}", pixel(&dark, 0, 0));
    // The middle of an edge is on the way: darker than the middle, lighter
    // than the corner.
    let edge = pixel(&dark, 0, middle)[0];
    assert!((60..230).contains(&edge), "{edge}");

    let red = draw(
        &mut renderer,
        &with_lens(white.clone(), vignette([1.0, 0.0, 0.0])),
    );
    let corner = pixel(&red, SIZE - 1, SIZE - 1);
    assert!(
        corner[0] > 245 && corner[1] < 10 && corner[2] < 10,
        "{corner:?}"
    );

    // Turned off in the settings, it changes nothing.
    renderer.set_graphics(&lens_off());
    assert_eq!(
        draw(&mut renderer, &with_lens(white, vignette([0.0; 3]))),
        plain
    );
}

#[test]
fn film_grain_adds_moving_noise_and_keeps_the_average() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let grey = filled([0.5; 3]);
    let plain = draw(&mut renderer, &grey);
    let reds = |pixels: &[u8]| -> Vec<f32> {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| f32::from(p[0]))
            .collect()
    };
    let spread = |values: &[f32]| {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
        (mean, variance.sqrt())
    };
    let (mean, deviation) = spread(&reds(&plain));
    assert!(
        (mean - 128.0).abs() < 1.0 && deviation < 0.5,
        "{mean} {deviation}"
    );

    let grain = |size| LensEffects {
        film_grain: Some(FilmGrain {
            intensity: 0.5,
            size,
        }),
        ..LensEffects::default()
    };
    let grainy = with_lens(grey.clone(), grain(5.0));
    let first = draw(&mut renderer, &grainy);
    let (mean, deviation) = spread(&reds(&first));
    assert!((mean - 128.0).abs() < 3.0, "the average stays: {mean}");
    assert!(deviation > 6.0, "noisy: {deviation}");
    let second = draw(&mut renderer, &grainy);
    let moved = first
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        moved > (SIZE * SIZE / 2) as usize,
        "the grain moves each frame: {moved}"
    );

    // Bigger grains: 25 at 1080 is 5 pixels here, so the pixels of one
    // 5 × 5 cell match.
    let coarse = draw(&mut renderer, &with_lens(grey.clone(), grain(25.0)));
    assert_eq!(pixel(&coarse, 0, 0), pixel(&coarse, 4, 4));
    assert_eq!(pixel(&coarse, 5, 5), pixel(&coarse, 9, 9));

    renderer.set_graphics(&lens_off());
    assert_eq!(draw(&mut renderer, &grainy), plain);
}

#[test]
fn chromatic_aberration_parts_red_and_blue_toward_the_edges() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    // A white card whose right edge is 80% of the way across, on black.
    let half_width = 5.0 * 35f32.to_radians().tan();
    let edge = 0.6 * half_width;
    let list = list_of(vec![card(
        [1.0; 3],
        Vec3::new(edge - 5.0, 0.0, -5.0),
        Vec3::new(10.0, 20.0, 0.1),
    )]);
    let row = SIZE / 2;
    let parted = |pixels: &[u8]| {
        (0..SIZE)
            .map(|x| pixel(pixels, x, row))
            .filter(|p| p[0].abs_diff(p[2]) > 100)
            .count()
    };
    let plain = draw(&mut renderer, &list);
    assert_eq!(parted(&plain), 0);

    let aberrated = with_lens(
        list.clone(),
        LensEffects {
            chromatic_aberration: Some(5.0),
            ..LensEffects::default()
        },
    );
    let shown = draw(&mut renderer, &aberrated);
    // 5 parts them by 5% of the width at the edges: 0.03 here, 6 pixels.
    assert!(parted(&shown) >= 4, "{}", parted(&shown));
    // Just inside the edge, red is read from farther out (black) and blue
    // from farther in (white).
    let inside = pixel(&shown, (SIZE as f32 * 0.8) as u32 - 3, row);
    assert!(inside[0] < 60 && inside[2] > 200, "{inside:?}");
    // The middle doesn't move.
    assert_eq!(pixel(&shown, SIZE / 2, row), pixel(&plain, SIZE / 2, row));

    renderer.set_graphics(&lens_off());
    assert_eq!(draw(&mut renderer, &aberrated), plain);
}

#[test]
fn sharpening_steepens_edges_and_leaves_flat_areas() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    // A light grey card over the left half, in front of a darker one.
    let list = list_of(vec![
        card(
            [0.6; 3],
            Vec3::new(-5.0, 0.0, -5.0),
            Vec3::new(10.0, 20.0, 0.1),
        ),
        card(
            [0.3; 3],
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::new(20.0, 20.0, 0.1),
        ),
    ]);
    let row = SIZE / 2;
    let plain = draw(&mut renderer, &list);
    let (light, dark) = (pixel(&plain, 20, row)[0], pixel(&plain, SIZE - 20, row)[0]);
    assert!(
        light.abs_diff(153) <= 1 && dark.abs_diff(77) <= 1,
        "{light} {dark}"
    );

    let sharpened = with_lens(
        list.clone(),
        LensEffects {
            sharpen: Some(1.0),
            ..LensEffects::default()
        },
    );
    let shown = draw(&mut renderer, &sharpened);
    let values: Vec<u8> = (0..SIZE).map(|x| pixel(&shown, x, row)[0]).collect();
    let brightest = *values.iter().max().unwrap();
    let darkest = *values.iter().min().unwrap();
    assert!(
        brightest > light + 20,
        "the light side of the edge: {brightest}"
    );
    assert!(darkest + 20 < dark, "the dark side of the edge: {darkest}");
    // Away from the edge, nothing changes.
    assert_eq!(values[20], light);
    assert_eq!(values[(SIZE - 20) as usize], dark);

    renderer.set_graphics(&lens_off());
    assert_eq!(draw(&mut renderer, &sharpened), plain);
}

/// Four white bars `distance` studs away, left of the middle (`side` -1) or
/// right of it (1), on black. Each is a tenth of the view's half-width wide
/// whatever the distance (about 11 pixels), with a gap as wide between.
fn bars(distance: f32, side: f32) -> Vec<DrawItem> {
    let width = distance * 35f32.to_radians().tan() / 10.0;
    (0..4)
        .map(|i| {
            let x = side * width * (2 + 2 * i) as f32;
            card(
                [1.0; 3],
                Vec3::new(x, 0.0, -distance),
                Vec3::new(width, distance, 0.1),
            )
        })
        .collect()
}

/// The difference between the brightest and darkest red in `row` between
/// `from` and `to`.
fn contrast(pixels: &[u8], row: u32, (from, to): (u32, u32)) -> u8 {
    let values: Vec<u8> = (from..to).map(|x| pixel(pixels, x, row)[0]).collect();
    values.iter().max().unwrap() - values.iter().min().unwrap()
}

#[test]
fn depth_of_field_blurs_out_of_focus_surfaces_and_leaves_focused_ones_sharp() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    // The bars look the same near and far, so only the blur tells them apart.
    let mut items = bars(10.0, -1.0);
    items.extend(bars(60.0, 1.0));
    let list = list_of(items);
    let row = SIZE / 2;
    let (left, right) = ((0, SIZE / 2), (SIZE / 2, SIZE));
    let plain = draw(&mut renderer, &list);
    assert!(contrast(&plain, row, left) > 250);
    assert!(contrast(&plain, row, right) > 250);

    let blur = |focus_distance, in_focus_radius| LensEffects {
        depth_of_field: Some(DepthOfField {
            focus_distance,
            in_focus_radius,
            near_intensity: 1.0,
            far_intensity: 1.0,
        }),
        ..LensEffects::default()
    };
    // In focus near: the near bars stay sharp, the far ones blur.
    let focused_near = with_lens(list.clone(), blur(10.0, 3.0));
    let shown = draw(&mut renderer, &focused_near);
    assert!(contrast(&shown, row, left) > 250, "near stays sharp");
    let blurred_edges = |pixels: &[u8], (from, to): (u32, u32)| {
        (from..to)
            .filter(|&x| (30..225).contains(&pixel(pixels, x, row)[0]))
            .count()
    };
    assert!(
        blurred_edges(&shown, right) > blurred_edges(&plain, right) + 8,
        "far blurs: {} vs {}",
        blurred_edges(&shown, right),
        blurred_edges(&plain, right)
    );
    // Sharp surfaces don't smear into the blurred sky around them.
    assert!(blurred_edges(&shown, left) <= blurred_edges(&plain, left) + 8);

    // In focus far: the far bars stay as they were, the near ones blur.
    let focused_far = with_lens(list.clone(), blur(60.0, 10.0));
    let shown = draw(&mut renderer, &focused_far);
    assert!(
        blurred_edges(&shown, left) > blurred_edges(&plain, left) + 8,
        "near blurs: {} vs {}",
        blurred_edges(&shown, left),
        blurred_edges(&plain, left)
    );
    let far_middle = |pixels: &[u8]| {
        (right.0..right.1)
            .map(|x| pixel(pixels, x, row)[0])
            .collect::<Vec<_>>()
    };
    let unchanged = far_middle(&shown)
        .iter()
        .zip(far_middle(&plain))
        .filter(|(a, b)| a.abs_diff(*b) <= 2)
        .count();
    assert!(
        unchanged as u32 > (right.1 - right.0) * 9 / 10,
        "far stays sharp: {unchanged}"
    );

    // Off in the settings, nothing is blurred.
    renderer.set_graphics(&lens_off());
    assert_eq!(draw(&mut renderer, &focused_near), plain);
}

#[test]
fn lens_effects_cost_nothing_while_absent_or_turned_off() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let list = filled([0.5; 3]);
    let plain = draw(&mut renderer, &list);
    let held = |renderer: &TextureRenderer| {
        (
            renderer.scene.depth_of_field_targets_alive(),
            renderer.scene.occlusion_targets_alive(),
            renderer.stats().target_bytes,
        )
    };
    let start = held(&renderer);
    assert_eq!((start.0, start.1), (0, 0), "no targets without effects");

    let everything = with_lens(
        list.clone(),
        LensEffects {
            depth_of_field: Some(DepthOfField {
                focus_distance: 1.0,
                in_focus_radius: 0.5,
                near_intensity: 1.0,
                far_intensity: 1.0,
            }),
            vignette: Some(Vignette {
                intensity: 0.5,
                size: 0.5,
                color: [0.0; 3],
            }),
            film_grain: Some(FilmGrain {
                intensity: 0.3,
                size: 1.0,
            }),
            chromatic_aberration: Some(0.5),
            sharpen: Some(0.5),
        },
    );
    assert_ne!(draw(&mut renderer, &everything), plain);
    let used = held(&renderer);
    assert_eq!(
        (used.0, used.1),
        (1, 1),
        "depth of field's target and the prepass"
    );
    assert!(used.2 > start.2);

    // Taken away, everything they held is freed and the frame is as before.
    assert_eq!(draw(&mut renderer, &list), plain);
    assert_eq!(held(&renderer), start);

    // Depth of field that blurs nothing makes no target.
    let still = with_lens(
        list.clone(),
        LensEffects {
            depth_of_field: Some(DepthOfField {
                focus_distance: 1.0,
                in_focus_radius: 0.5,
                near_intensity: 0.0,
                far_intensity: 0.0,
            }),
            ..LensEffects::default()
        },
    );
    assert_eq!(draw(&mut renderer, &still), plain);
    assert_eq!(held(&renderer), start);

    // With every lens setting off, the world's effects cost nothing.
    renderer.set_graphics(&lens_off());
    assert_eq!(draw(&mut renderer, &everything), plain);
    assert_eq!(held(&renderer), start);
    // Low depth of field still blurs, with fewer samples.
    renderer.set_graphics(&GraphicsSettings {
        depth_of_field: Some(DepthOfFieldQuality::Low),
        ..lens_off()
    });
    draw(&mut renderer, &everything);
    assert_eq!(held(&renderer).0, 1);
}
