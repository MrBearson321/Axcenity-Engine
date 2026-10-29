//! Color grading: brightness, contrast, saturation, tint, a lookup table,
//! posterizing and a palette each change the finished frame as they say,
//! a neutral table changes nothing, and the table's texture lives only
//! while one is used.

use super::*;
use crate::graphics::{AntiAliasing, Quality};
use crate::{ColorGrading, ColorLookup, GraphicsSettings, Shading};

const SIZE: u32 = 64;

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue, aa: AntiAliasing) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(&GraphicsSettings {
        anti_aliasing: Some(aa),
        ..GraphicsSettings::preset(Quality::High)
    });
    renderer
}

/// The middle pixel of an unlit card of `color` (sRGB) filling the view,
/// with post-processing off, so with no grading it shows `color` itself.
fn shown(renderer: &mut TextureRenderer, color: [f32; 3], grading: ColorGrading) -> [u8; 3] {
    let mut card = big_block();
    card.material = Material {
        base_color: color,
        shading: Shading::Unlit,
        ..Material::default()
    };
    let mut list = list_of(vec![card]);
    list.grading = Some(grading);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let at = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2]]
}

#[track_caller]
fn near(drawn: [u8; 3], expected: [u8; 3], what: &str) {
    for (channel, wanted) in drawn.iter().zip(expected) {
        assert!(
            channel.abs_diff(wanted) <= 2,
            "{what}: {drawn:?}, not {expected:?}"
        );
    }
}

/// A table `size` entries a side whose entries are `map` of where they are.
fn table(key: u64, size: u32, map: impl Fn([f32; 3]) -> [f32; 3]) -> ColorLookup {
    let step = |i: u32| i as f32 / (size - 1) as f32;
    let mut rgb = Vec::new();
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                rgb.extend(map([step(red), step(green), step(blue)]));
            }
        }
    }
    ColorLookup {
        key,
        size,
        rgb: Arc::new(rgb),
    }
}

#[test]
fn each_control_changes_the_frame_as_it_says() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let mut shown = |color, grading| shown(&mut renderer, color, grading);
    let none = ColorGrading::default();
    near(shown([0.5; 3], none.clone()), [128; 3], "the default");

    let brightness = |brightness| ColorGrading {
        brightness,
        ..ColorGrading::default()
    };
    near(shown([0.5; 3], brightness(0.2)), [179; 3], "brighter");
    near(shown([0.5; 3], brightness(-1.0)), [0; 3], "brightness -1");

    let contrast = |contrast| ColorGrading {
        contrast,
        ..ColorGrading::default()
    };
    near(
        shown([0.25; 3], contrast(0.5)),
        [32; 3],
        "dark, more contrast",
    );
    near(
        shown([0.75; 3], contrast(0.5)),
        [223; 3],
        "light, more contrast",
    );
    near(shown([0.25; 3], contrast(-1.0)), [128; 3], "contrast -1");

    let saturation = |saturation| ColorGrading {
        saturation,
        ..ColorGrading::default()
    };
    // Grey at the color's brightness (Rec. 709 weights): 0.342.
    near(
        shown([0.8, 0.2, 0.4], saturation(-1.0)),
        [87; 3],
        "saturation -1",
    );
    // Twice as far from that grey (0.450).
    near(
        shown([0.6, 0.4, 0.5], saturation(1.0)),
        [191, 89, 140],
        "saturation 1",
    );

    let tint = ColorGrading {
        tint: [1.0, 0.5, 0.0],
        ..ColorGrading::default()
    };
    near(shown([1.0; 3], tint), [255, 128, 0], "tinted orange");

    let inverted = ColorGrading {
        lookup: Some(table(1, 2, |c| c.map(|c| 1.0 - c))),
        ..ColorGrading::default()
    };
    near(
        shown([0.2, 0.6, 0.9], inverted),
        [204, 102, 26],
        "an inverting table",
    );

    let posterize = |posterize| ColorGrading {
        posterize,
        ..ColorGrading::default()
    };
    near(shown([0.3; 3], posterize(2)), [0; 3], "two levels, dark");
    near(shown([0.7; 3], posterize(2)), [255; 3], "two levels, light");
    near(shown([0.3; 3], posterize(4)), [85; 3], "four levels");
    near(shown([0.3; 3], posterize(1)), [77; 3], "one level is off");

    let palette = ColorGrading {
        palette: vec![[0.0; 3], [1.0; 3], [1.0, 0.0, 0.0]],
        ..ColorGrading::default()
    };
    near(
        shown([0.9, 0.3, 0.2], palette.clone()),
        [255, 0, 0],
        "nearest is red",
    );
    near(
        shown([0.2, 0.2, 0.25], palette.clone()),
        [0; 3],
        "nearest is black",
    );
    near(shown([0.8; 3], palette), [255; 3], "nearest is white");
}

/// A frame with many colors: lit blocks and spheres in front of the sky,
/// with bloom and tone mapping on.
fn colorful() -> DrawList {
    let colors = [
        [0.9, 0.2, 0.1],
        [0.1, 0.8, 0.3],
        [0.2, 0.3, 0.95],
        [0.95, 0.9, 0.2],
        [0.5; 3],
    ];
    let items = colors
        .iter()
        .enumerate()
        .map(|(i, &color)| {
            let shape = if i % 2 == 0 {
                Shape::Block
            } else {
                Shape::Sphere
            };
            let mut item = item(shape, Vec3::new(i as f32 * 1.4 - 2.8, 0.0, -6.0), 1.0);
            item.material.base_color = color;
            item.material.emissive = if i == 3 { [2.0; 3] } else { [0.0; 3] };
            item
        })
        .collect();
    let mut list = list_of(items);
    list.sun.direction = Vec3::new(0.4, 0.8, 0.5).normalize();
    list.sun.intensity = 2.0;
    list.ambient.intensity = 0.3;
    list.post = PostProcessing {
        tone_mapping: crate::ToneMapping::Neutral,
        ..PostProcessing::default()
    };
    list
}

#[test]
fn a_neutral_lookup_table_changes_nothing() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    for aa in [AntiAliasing::Off, AntiAliasing::Fxaa, AntiAliasing::Msaa4] {
        let mut renderer = renderer(&device, &queue, aa);
        let mut list = colorful();
        renderer.render(&list).unwrap();
        let plain = renderer.read_pixels();
        let distinct: HashSet<&[u8; 4]> = plain.as_chunks::<4>().0.iter().collect();
        assert!(distinct.len() > 200, "a colorful frame: {}", distinct.len());

        for (key, size) in [(10, 2), (11, 17), (12, 33)] {
            for grading in [
                ColorGrading::default(),
                ColorGrading {
                    lookup: Some(table(key, size, |c| c)),
                    ..ColorGrading::default()
                },
            ] {
                list.grading = Some(grading);
                renderer.render(&list).unwrap();
                let graded = renderer.read_pixels();
                let most = plain
                    .iter()
                    .zip(&graded)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    most <= 1,
                    "a neutral table of {size} ({aa:?}) changed a channel by {most}"
                );
            }
        }
    }
}

#[test]
fn a_lookup_tables_texture_lives_only_while_one_is_used() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let inverted = || ColorGrading {
        lookup: Some(table(20, 2, |c| c.map(|c| 1.0 - c))),
        ..ColorGrading::default()
    };
    assert!(!renderer.scene.post.grading.has_lookup());
    near(
        shown(&mut renderer, [0.2; 3], inverted()),
        [204; 3],
        "inverted",
    );
    assert!(renderer.scene.post.grading.has_lookup());
    // Taken away, the table goes and the frame is back as it was.
    near(
        shown(&mut renderer, [0.2; 3], ColorGrading::default()),
        [51; 3],
        "plain",
    );
    assert!(!renderer.scene.post.grading.has_lookup());
    // A broken one (the wrong number of entries) is left out.
    let broken = ColorGrading {
        lookup: Some(ColorLookup {
            key: 21,
            size: 4,
            rgb: Arc::new(vec![0.0; 3]),
        }),
        ..ColorGrading::default()
    };
    near(shown(&mut renderer, [0.2; 3], broken), [51; 3], "broken");
    assert!(!renderer.scene.post.grading.has_lookup());
}
