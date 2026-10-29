//! Shading models: Standard, Unlit and Toon side by side in one frame,
//! each lit as its model says; toon's highlight, rim and ramp; and toon
//! taking the same shadows and lamps as everything else.

use super::*;
use crate::graphics::{AmbientOcclusion, AntiAliasing, Quality};
use crate::{GraphicsSettings, Shading, Toon};
use glam::Vec2;

/// The frame size, square.
const SIZE: u32 = 128;
/// How wide each sphere is, in studs.
const WIDTH: f32 = 3.0;

fn sphere(x: f32, color: [f32; 3], shading: Shading) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(WIDTH),
            glam::Quat::IDENTITY,
            Vec3::new(x, 0.0, 0.0),
        ),
        material: Material {
            base_color: color,
            shading,
            ..Material::default()
        },
        ..item(Shape::Sphere, Vec3::ZERO, 1.0)
    }
}

/// Plain toon: `steps` hard-edged bands, no rim or highlight.
fn bands(steps: u32) -> Toon {
    Toon {
        steps,
        softness: 0.0,
        rim: 0.0,
        highlight: 0.0,
        ..Toon::default()
    }
}

/// `items` in the dark (no sky, no ambient light, no shadows) seen from
/// +Z, lit by a sun from the right.
fn scene(items: Vec<DrawItem>) -> DrawList {
    let mut list = list_of(items);
    list.camera = Camera {
        position: Vec3::new(0.0, 0.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    };
    list.sun.direction = Vec3::new(1.0, 0.2, 0.4).normalize();
    list.sun.intensity = 2.0;
    list.shadows.enabled = false;
    list
}

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(&GraphicsSettings {
        anti_aliasing: Some(AntiAliasing::Off),
        ambient_occlusion: Some(AmbientOcclusion::Off),
        ..GraphicsSettings::preset(Quality::High)
    });
    renderer
}

/// Where `point` is drawn in a `SIZE`-square frame of `list`, in pixels.
fn on_screen(list: &DrawList, point: Vec3) -> Vec2 {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * SIZE as f32
}

fn pixel(pixels: &[u8], at: Vec2) -> [u8; 4] {
    let (x, y) = (at.x as usize, at.y as usize);
    pixels[(y * SIZE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// The pixels on screen along the line from `from` to `to`.
fn along(list: &DrawList, pixels: &[u8], from: Vec3, to: Vec3) -> Vec<[u8; 4]> {
    let (start, end) = (on_screen(list, from), on_screen(list, to));
    let steps = (end - start).abs().max_element().ceil() as u32;
    (0..=steps)
        .map(|step| pixel(pixels, start.lerp(end, step as f32 / steps as f32)))
        .collect()
}

/// The pixels across the middle of the sphere at `x`, leaving out a
/// little at each edge.
fn across(list: &DrawList, pixels: &[u8], x: f32) -> Vec<[u8; 4]> {
    let reach = WIDTH / 2.0 * 0.85;
    along(
        list,
        pixels,
        Vec3::new(x - reach, 0.0, 0.0),
        Vec3::new(x + reach, 0.0, 0.0),
    )
}

fn brightness(pixel: [u8; 4]) -> i32 {
    pixel[..3].iter().map(|&c| i32::from(c)).sum()
}

/// How many different brightnesses `pixels` have, counting ones within 3
/// of each other as one.
fn levels(pixels: &[[u8; 4]]) -> usize {
    let mut values: Vec<i32> = pixels.iter().map(|&p| brightness(p)).collect();
    values.sort_unstable();
    1 + values
        .windows(2)
        .filter(|pair| pair[1] - pair[0] > 3)
        .count()
}

fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

#[test]
fn standard_unlit_and_toon_parts_draw_side_by_side_each_lit_as_its_model_says() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let (standard, unlit, toon) = (-3.5, 0.0, 3.5);
    let orange = [1.0, 0.5, 0.0];
    let side_by_side = |sun: f32| {
        let mut list = scene(vec![
            sphere(standard, [0.8; 3], Shading::Standard),
            sphere(unlit, orange, Shading::Unlit),
            sphere(toon, [0.8; 3], Shading::Toon(bands(2))),
        ]);
        list.sun.intensity = sun;
        list
    };
    let list = side_by_side(2.0);
    let pixels = draw(&mut renderer, &list);

    // Standard shades smoothly from lit to dark.
    let smooth = across(&list, &pixels, standard);
    assert!(
        levels(&smooth) > 8,
        "standard shades smoothly: {} levels",
        levels(&smooth)
    );
    // Unlit is its color everywhere, lit side and dark side alike.
    for drawn in across(&list, &pixels, unlit) {
        for (channel, expected) in drawn[..3].iter().zip([255u8, 128, 0]) {
            assert!(
                channel.abs_diff(expected) <= 2,
                "unlit shows its color: {drawn:?}"
            );
        }
    }
    // Toon has the dark side and its two bands of light, and nothing between.
    let toon_pixels = across(&list, &pixels, toon);
    assert_eq!(levels(&toon_pixels), 3, "toon is banded: {toon_pixels:?}");
    let lit = *toon_pixels.last().unwrap();
    assert!(brightness(lit) > 300, "its lit side is lit: {lit:?}");
    assert_eq!(brightness(toon_pixels[0]), 0, "its dark side is dark");

    // A brighter sun brightens standard and toon, not unlit.
    let brighter = side_by_side(4.0);
    let brighter_pixels = draw(&mut renderer, &brighter);
    let middle = |pixels: &[u8], x: f32| pixel(pixels, on_screen(&list, Vec3::new(x, 0.0, 1.4)));
    assert_eq!(
        middle(&pixels, unlit),
        middle(&brighter_pixels, unlit),
        "unlit ignores the light"
    );
    for x in [standard, toon] {
        assert!(
            brightness(middle(&brighter_pixels, x)) > brightness(middle(&pixels, x)) + 30,
            "a brighter sun brightens the sphere at {x}"
        );
    }
}

#[test]
fn a_toon_highlight_and_rim_light_show_only_when_asked_for() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    // Lit from the camera's side, so the highlight is in the middle.
    let lit_from_the_front = |toon: Toon| {
        let mut list = scene(vec![sphere(0.0, [0.5, 0.2, 0.2], Shading::Toon(toon))]);
        list.sun.direction = Vec3::Z;
        list.ambient.intensity = 0.5;
        list
    };
    let mut points = |list: &DrawList| {
        let pixels = draw(&mut renderer, list);
        let radius = WIDTH / 2.0;
        // The middle, halfway out, and close to the outline.
        [0.0, 0.5, 0.95].map(|out: f32| {
            let along = out * radius;
            let depth = (radius * radius - along * along).sqrt();
            pixel(&pixels, on_screen(list, Vec3::new(along, 0.0, depth)))
        })
    };

    let [plain_middle, plain_halfway, plain_edge] = points(&lit_from_the_front(bands(2)));
    assert_eq!(
        plain_middle, plain_halfway,
        "with no highlight, the lit band is flat"
    );
    let [middle, halfway, _] = points(&lit_from_the_front(Toon {
        highlight: 0.5,
        ..bands(2)
    }));
    assert!(
        brightness(middle) > brightness(halfway) + 60,
        "a highlight brightens the middle: {middle:?} against {halfway:?}"
    );
    assert_eq!(halfway, plain_halfway, "and only there");

    let [_, rim_halfway, rim_edge] = points(&lit_from_the_front(Toon {
        rim: 1.0,
        ..bands(2)
    }));
    assert!(
        brightness(rim_edge) > brightness(plain_edge) + 60,
        "a rim light brightens the outline: {plain_edge:?} -> {rim_edge:?}"
    );
    assert_eq!(rim_halfway, plain_halfway, "and not inside it");
}

/// A ramp image, one row: red on the left, blue on the right.
struct Ramp;

impl TextureSource for Ramp {
    fn key(&self) -> u64 {
        0x7a_3b
    }
    fn size(&self) -> (u32, u32) {
        (2, 1)
    }
    fn rgba(&self) -> &[u8] {
        &[255, 0, 0, 255, 0, 0, 255, 255]
    }
}

#[test]
fn a_toon_ramp_colors_the_light_from_none_to_full() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let ramp: Arc<dyn TextureSource> = Arc::new(Ramp);
    let list = scene(vec![sphere(
        0.0,
        [1.0; 3],
        Shading::Toon(Toon {
            ramp: Some(ramp),
            ..bands(2)
        }),
    )]);
    let pixels = across(&list, &draw(&mut renderer, &list), 0.0);
    // The sun is on the right: there the light is full, on the left little.
    let (shaded, lit) = (pixels[0], *pixels.last().unwrap());
    assert!(lit[2] > 100 && lit[0] < 10, "full light is blue: {lit:?}");
    assert!(
        shaded[0] > 100 && shaded[2] < 10,
        "little light is red: {shaded:?}"
    );
    let mut colors = pixels.clone();
    colors.sort_unstable();
    colors.dedup();
    assert_eq!(colors.len(), 2, "and nothing between: {pixels:?}");
}

#[test]
fn toon_surfaces_take_the_same_shadows_and_lamps() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue);
    let floor = DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(20.0, 1.0, 20.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
        material: Material {
            base_color: [0.8; 3],
            shading: Shading::Toon(bands(1)),
            ..Material::default()
        },
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    };
    // A block over the back of the floor, its shadow falling straight
    // down, behind the line z = -1.
    let blocker = DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(4.0, 1.0, 4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 3.0, -3.0),
        ),
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    };
    let mut list = list_of(vec![floor, blocker]);
    list.camera = Camera {
        position: Vec3::new(0.0, 12.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    };
    list.sun.direction = Vec3::Y;
    list.ambient.intensity = 0.3;
    list.shadows.enabled = true;
    let pixels = draw(&mut renderer, &list);
    let at = |list: &DrawList, pixels: &[u8], z: f32| {
        pixel(pixels, on_screen(list, Vec3::new(1.0, 0.0, z)))
    };
    let (shaded, lit) = (at(&list, &pixels, -3.0), at(&list, &pixels, 3.0));
    assert!(
        brightness(lit) > brightness(shaded) + 90,
        "the block shadows the toon floor: {shaded:?} under it, {lit:?} beside it"
    );
    // One band and hard-edged: the floor across the shadow's edge is lit
    // or shadowed, nothing between.
    let edge = along(
        &list,
        &pixels,
        Vec3::new(1.0, 0.0, -2.5),
        Vec3::new(1.0, 0.0, 0.5),
    );
    assert_eq!(levels(&edge), 2, "a hard shadow edge: {edge:?}");

    // A lamp lights it too.
    list.sun.intensity = 0.0;
    list.shadows.enabled = false;
    let dark = at(&list, &draw(&mut renderer, &list), 3.0);
    list.lights = vec![point_light(Vec3::new(3.0, 2.0, 0.0), 12.0)];
    let lamp_lit = at(&list, &draw(&mut renderer, &list), 3.0);
    assert!(
        brightness(lamp_lit) > brightness(dark) + 60,
        "a point light lights the toon floor: {dark:?} -> {lamp_lit:?}"
    );
}

#[test]
fn a_materials_shading_picks_its_binding_and_a_ramp_takes_the_specular_maps_place() {
    let plain = item(Shape::Block, Vec3::ZERO, 1.0);
    assert!(matches!(plain.material.shading, Shading::Standard));
    assert_eq!(
        material_key(&plain),
        MaterialKey::default(),
        "a standard material keeps the plain binding"
    );
    let shaded = |shading: Shading| DrawItem {
        material: Material {
            shading,
            ..Material::default()
        },
        ..plain.clone()
    };
    let unlit = material_key(&shaded(Shading::Unlit));
    let toon = material_key(&shaded(Shading::Toon(Toon::default())));
    assert_ne!(unlit, MaterialKey::default());
    assert_ne!(toon, unlit);
    assert_eq!(maps_uniform_of(&toon).shading, [2.0, 3.0, 0.05, 0.2]);
    assert_eq!(maps_uniform_of(&toon).toon[0], 0.3);

    let ramp: Arc<dyn TextureSource> = Arc::new(Ramp);
    let mut ramped = shaded(Shading::Toon(Toon {
        ramp: Some(ramp),
        ..Toon::default()
    }));
    ramped.material.maps.specular = Some(Arc::new(TestTexture(9)));
    let key = material_key(&ramped);
    assert_eq!(
        key.textures[7],
        Some(Ramp.key()),
        "the ramp, not the specular map"
    );
    assert_eq!(maps_uniform_of(&key).shading[1], 0.0, "read from the ramp");
    assert_eq!(maps_uniform_of(&key).specular[0], 0.0, "no specular map");
}
