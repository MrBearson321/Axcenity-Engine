//! Outlines: a material's inverted hull, the same width on screen near and
//! far, and screen-space edges that outline a scene with no per-material
//! setup, each costing nothing while unused.

use super::*;
use crate::graphics::{AmbientOcclusion, AntiAliasing, Quality};
use crate::{Edges, GraphicsSettings, Outline, Shading, Toon};

/// The frame size, square: a fifth of 1080, so a width of 5 is 1 pixel.
const SIZE: u32 = 216;

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue, aa: AntiAliasing) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(&GraphicsSettings {
        anti_aliasing: Some(aa),
        ambient_occlusion: Some(AmbientOcclusion::Off),
        contact_shadows: Some(false),
        volumetric_fog: Some(false),
        ..GraphicsSettings::preset(Quality::High)
    });
    renderer
}

fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    pixels[((y * SIZE + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}

fn brightness(pixel: [u8; 4]) -> i32 {
    pixel[..3].iter().map(|&c| i32::from(c)).sum()
}

/// A lit toon sphere 3 studs wide with a red outline `width` wide (or
/// none), `distance` studs in front of the camera, in the dark.
fn toon_sphere(distance: f32, width: Option<f32>) -> DrawList {
    let sphere = DrawItem {
        transform: Mat4::from_scale(Vec3::splat(3.0)),
        material: Material {
            base_color: [0.9; 3],
            shading: Shading::Toon(Toon {
                steps: 2,
                rim: 0.0,
                highlight: 0.0,
                ..Toon::default()
            }),
            outline: width.map(|width| Outline {
                width,
                color: [1.0, 0.0, 0.0],
            }),
            ..Material::default()
        },
        ..item(Shape::Sphere, Vec3::ZERO, 1.0)
    };
    let mut list = list_of(vec![sphere]);
    list.camera = Camera {
        position: Vec3::new(0.0, 0.0, distance),
        look_at: Vec3::ZERO,
        fov_y_degrees: 40.0,
    };
    list.sun.direction = Vec3::new(0.3, 0.5, 1.0).normalize();
    list.sun.intensity = 3.0;
    list.ambient.intensity = 0.3;
    list.shadows.enabled = false;
    list
}

fn is_outline(pixel: [u8; 4]) -> bool {
    pixel[0] > 100 && pixel[1] < 60 && pixel[2] < 60
}

/// How many outline pixels lie on the way in from each side of the frame
/// to its middle (left, right, top, bottom), through the sphere's middle.
fn outline_widths(pixels: &[u8]) -> [usize; 4] {
    let middle = SIZE / 2;
    let count = |points: &mut dyn Iterator<Item = (u32, u32)>| {
        points
            .filter(|&(x, y)| is_outline(pixel(pixels, x, y)))
            .count()
    };
    [
        count(&mut (0..middle).map(|x| (x, middle))),
        count(&mut (middle..SIZE).map(|x| (x, middle))),
        count(&mut (0..middle).map(|y| (middle, y))),
        count(&mut (middle..SIZE).map(|y| (middle, y))),
    ]
}

#[test]
fn an_outlined_toon_sphere_has_an_even_outline_near_and_far() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    for aa in [AntiAliasing::Off, AntiAliasing::Msaa4] {
        let mut renderer = renderer(&device, &queue, aa);
        // 20 at 1080 pixels tall is 4 pixels here.
        let near = outline_widths(&draw(&mut renderer, &toon_sphere(8.0, Some(20.0))));
        let far = outline_widths(&draw(&mut renderer, &toon_sphere(30.0, Some(20.0))));
        for widths in [near, far] {
            for width in widths {
                assert!(
                    (3..=5).contains(&width),
                    "{aa:?}: about 4 pixels on every side: near {near:?}, far {far:?}"
                );
            }
        }
        let none = outline_widths(&draw(&mut renderer, &toon_sphere(8.0, None)));
        assert_eq!(none, [0; 4], "{aa:?}: no outline without one");
    }

    // The width scales with the view's height, and the item's own pixels
    // are the same with or without its outline.
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let wide = outline_widths(&draw(&mut renderer, &toon_sphere(8.0, Some(40.0))));
    assert!(
        wide.iter().all(|width| (7..=9).contains(width)),
        "twice the width: {wide:?}"
    );
    let with = draw(&mut renderer, &toon_sphere(8.0, Some(20.0)));
    let without = draw(&mut renderer, &toon_sphere(8.0, None));
    let middle = SIZE / 2;
    for (x, y) in [(middle, middle), (middle + 10, middle - 10)] {
        assert_eq!(pixel(&with, x, y), pixel(&without, x, y), "at ({x}, {y})");
    }
}

#[test]
fn an_outline_costs_one_draw_per_outlined_batch_and_nothing_without_one() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    renderer.render(&toon_sphere(8.0, None)).unwrap();
    assert_eq!(renderer.stats().draw_calls, 1);
    renderer.render(&toon_sphere(8.0, Some(10.0))).unwrap();
    assert_eq!(renderer.stats().draw_calls, 2, "the outline is one more");
    assert_eq!(renderer.stats().items_drawn, 1, "of the same item");
    // A see-through item gets no outline.
    let mut glass = toon_sphere(8.0, Some(10.0));
    glass.items[0].material.opacity = 0.5;
    renderer.render(&glass).unwrap();
    assert_eq!(renderer.stats().draw_calls, 1);
}

/// A 4-stud block on a wide floor, both the same grey under flat ambient
/// light only, so the block can't be told from the floor without outlines.
fn plain_block_on_floor(edges: Option<Edges>) -> DrawList {
    let block = |size: Vec3, position: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        material: Material {
            base_color: [0.8; 3],
            roughness: 1.0,
            ..Material::default()
        },
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    };
    let floor = block(Vec3::new(200.0, 1.0, 200.0), Vec3::new(0.0, -0.5, -10.0));
    let cube = block(Vec3::splat(4.0), Vec3::new(0.0, 2.0, -10.0));
    let mut list = list_of(vec![floor, cube]);
    // Looking down enough that the floor fills the frame.
    list.camera = Camera {
        position: Vec3::new(0.0, 10.0, 0.0),
        look_at: Vec3::new(0.0, 0.0, -10.0),
        fov_y_degrees: 70.0,
    };
    list.sun.intensity = 0.0;
    list.ambient.intensity = 0.6;
    list.shadows.enabled = false;
    list.edges = edges;
    list
}

/// Runs of dark pixels down the middle column of the frame.
fn dark_runs(pixels: &[u8]) -> usize {
    let dark: Vec<bool> = (0..SIZE)
        .map(|y| brightness(pixel(pixels, SIZE / 2, y)) < 150)
        .collect();
    dark.windows(2).filter(|pair| pair[1] && !pair[0]).count() + usize::from(dark[0])
}

#[test]
fn screen_space_edges_outline_a_scene_without_per_material_setup() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let black = Edges {
        color: [0.0; 3],
        thickness: 5.0,
    };
    for aa in [AntiAliasing::Off, AntiAliasing::Fxaa, AntiAliasing::Msaa4] {
        let mut renderer = renderer(&device, &queue, aa);
        let without = draw(&mut renderer, &plain_block_on_floor(None));
        assert_eq!(dark_runs(&without), 0, "{aa:?}: grey on grey");
        let with = draw(&mut renderer, &plain_block_on_floor(Some(black)));
        // Down the middle: the floor behind, the block's top edge against
        // it (depth), the crease along its top front edge (facing), its
        // front face, and the inside corner where it meets the floor.
        assert_eq!(
            dark_runs(&with),
            3,
            "{aa:?}: the block's silhouette and creases"
        );
        // Open floor, and the middle of the block's top, are untouched.
        let list = plain_block_on_floor(None);
        for point in [Vec3::new(-6.0, 0.0, -4.0), Vec3::new(0.0, 4.0, -10.0)] {
            let clip = view_projection(&list, 1.0) * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let x = ((ndc.x * 0.5 + 0.5) * SIZE as f32) as u32;
            let y = ((0.5 - ndc.y * 0.5) * SIZE as f32) as u32;
            assert_eq!(
                pixel(&with, x, y),
                pixel(&without, x, y),
                "{aa:?}: {point} is unchanged"
            );
        }
    }

    // Their color is the world's.
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let red = draw(
        &mut renderer,
        &plain_block_on_floor(Some(Edges {
            color: [1.0, 0.0, 0.0],
            ..black
        })),
    );
    let reddest = (0..SIZE)
        .map(|y| pixel(&red, SIZE / 2, y))
        .max_by_key(|p| i32::from(p[0]) - i32::from(p[1]))
        .unwrap();
    assert!(
        reddest[0] > 200 && reddest[1] < 60,
        "red lines: {reddest:?}"
    );
}

#[test]
fn thick_screen_space_edges_stay_on_the_block_and_off_the_floor() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // 30 at 1080 pixels tall is 6 pixels here.
    let thick = Edges {
        color: [0.0; 3],
        thickness: 30.0,
    };
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let without = draw(&mut renderer, &plain_block_on_floor(None));
    let with = draw(&mut renderer, &plain_block_on_floor(Some(thick)));
    let list = plain_block_on_floor(None);
    let screen = |point: Vec3| {
        let clip = view_projection(&list, 1.0) * point.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        (
            ((ndc.x * 0.5 + 0.5) * SIZE as f32) as u32,
            ((0.5 - ndc.y * 0.5) * SIZE as f32) as u32,
        )
    };
    // Across the block's left edge, halfway up and just above the floor:
    // the line fills the block's side of it and leaves the floor alone.
    for height in [2.0, 0.3] {
        let (edge, y) = screen(Vec3::new(-2.0, height, -8.0));
        for x in edge + 1..edge + 5 {
            assert!(
                brightness(pixel(&with, x, y)) < 150,
                "{height}: ({x}, {y}) inside the edge is dark"
            );
        }
        for x in edge - 8..edge - 1 {
            assert_eq!(
                pixel(&with, x, y),
                pixel(&without, x, y),
                "{height}: ({x}, {y}) on the floor is unchanged"
            );
        }
    }
}

#[test]
fn screen_space_edges_keep_the_prepass_alive_only_while_the_world_has_them() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, AntiAliasing::Off);
    let without = plain_block_on_floor(None);
    let with = plain_block_on_floor(Some(Edges::default()));
    renderer.render(&without).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 0);
    assert_eq!(renderer.stats().draw_calls, 1, "the two blocks' batch");
    for step in 0..10 {
        let edges = step % 2 == 0;
        renderer
            .render(if edges { &with } else { &without })
            .unwrap();
        assert_eq!(
            renderer.scene.occlusion_targets_alive(),
            usize::from(edges),
            "step {step}"
        );
        assert_eq!(
            renderer.stats().draw_calls,
            1 + u32::from(edges),
            "step {step}: the prepass draws the solid batch again"
        );
        assert_eq!(renderer.scene.targets_alive(), 1, "step {step}");
    }
}
