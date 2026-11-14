//! Ray traced reflections (RL4, renderer/ray_reflections.rs): a glossy
//! floor shows what's off the screen above it, a rough one is left to the
//! reflection probes, Low traces at half size, and turning them off (or a
//! GPU without ray queries) frees the pass and draws as before.

use super::*;
use crate::RayTracedReflections;

/// The frame size, square.
const SIZE: u32 = 96;

fn skipped() {
    eprintln!("skipped: no GPU with hardware ray tracing on this machine");
}

fn reflections(setting: RayTracedReflections) -> crate::GraphicsSettings {
    crate::GraphicsSettings {
        anti_aliasing: Some(crate::AntiAliasing::Off),
        ray_traced_reflections: Some(setting),
        ..crate::GraphicsSettings::default()
    }
}

/// A white metal floor of `roughness` under a black sky, seen from low
/// over it, looking down, with a glowing red block high above it: above
/// the top of the frame, but where the floor mirrors it on screen.
fn mirror_floor(roughness: f32) -> DrawList {
    let block = |size: Vec3, position: Vec3, material: Material| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        material,
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    };
    let floor = block(
        Vec3::new(40.0, 1.0, 40.0),
        Vec3::new(0.0, -0.5, 0.0),
        Material {
            base_color: [1.0; 3],
            metalness: 1.0,
            roughness,
            ..Material::default()
        },
    );
    let red = block(
        Vec3::new(12.0, 4.0, 1.0),
        Vec3::new(0.0, 10.0, -10.0),
        Material {
            base_color: [0.0; 3],
            emissive: [1.0, 0.0, 0.0],
            ..Material::default()
        },
    );
    let mut list = list_of(vec![floor, red]);
    list.camera = Camera {
        position: Vec3::new(0.0, 2.0, 12.0),
        look_at: Vec3::new(0.0, -4.0, -10.0),
        fov_y_degrees: 60.0,
    };
    list.sun.direction = Vec3::new(0.0, 1.0, 1.0).normalize();
    list.sun.intensity = 0.0;
    list.ambient.intensity = 0.2;
    list.shadows.enabled = false;
    list
}

/// Where `point` is drawn in a `SIZE`-square frame of `list`, in pixels.
fn on_screen(list: &DrawList, point: Vec3) -> (f32, f32) {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    (
        (ndc.x * 0.5 + 0.5) * SIZE as f32,
        (0.5 - ndc.y * 0.5) * SIZE as f32,
    )
}

/// The pixel where the floor mirrors the red block's middle.
fn mirrored(renderer: &mut TextureRenderer, list: &DrawList) -> [u8; 4] {
    renderer.render(list).unwrap();
    let pixels = renderer.read_pixels();
    let (x, y) = on_screen(list, Vec3::new(0.0, -10.0, -10.0));
    let at = (y as usize * SIZE as usize + x as usize) * 4;
    pixels[at..at + 4].try_into().unwrap()
}

#[test]
fn a_mirror_floor_shows_the_red_block_off_the_screen_above_it() {
    let Some((device, queue)) = ray_test_device() else {
        return skipped();
    };
    let list = mirror_floor(0.0);
    let (_, block_y) = on_screen(&list, Vec3::new(0.0, 8.0, -10.0));
    assert!(block_y < 0.0, "the block is above the frame");
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&reflections(RayTracedReflections::Off));
    let probes = mirrored(&mut renderer, &list);
    for setting in [RayTracedReflections::Medium, RayTracedReflections::Low] {
        renderer.set_graphics(&reflections(setting));
        let traced = mirrored(&mut renderer, &list);
        assert!(renderer.scene.ray_reflections.is_some());
        assert!(renderer.stats().ray_tracing.instances >= 2);
        assert!(
            traced[0] > probes[0] + 60 && traced[0] > traced[1] + 60,
            "{setting:?}: rays find the red block: {probes:?} -> {traced:?}"
        );
    }
}

#[test]
fn a_rough_floor_is_left_to_the_reflection_probes() {
    let Some((device, queue)) = ray_test_device() else {
        return skipped();
    };
    // Rougher than High's cutoff (0.5).
    let list = mirror_floor(0.8);
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&reflections(RayTracedReflections::Off));
    renderer.render(&list).unwrap();
    let probes = renderer.read_pixels();
    renderer.set_graphics(&reflections(RayTracedReflections::High));
    renderer.render(&list).unwrap();
    assert!(renderer.scene.ray_reflections.is_some());
    assert_eq!(renderer.read_pixels(), probes);
}

#[test]
fn turning_them_off_frees_the_pass_and_draws_as_before() {
    let Some((device, queue)) = ray_test_device() else {
        return skipped();
    };
    let list = mirror_floor(0.0);
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&reflections(RayTracedReflections::Off));
    renderer.render(&list).unwrap();
    let before = renderer.read_pixels();
    renderer.set_graphics(&reflections(RayTracedReflections::High));
    renderer.render(&list).unwrap();
    assert_ne!(renderer.read_pixels(), before);
    renderer.set_graphics(&reflections(RayTracedReflections::Off));
    renderer.render(&list).unwrap();
    assert!(renderer.scene.ray_reflections.is_none());
    assert_eq!(
        renderer.stats().ray_tracing,
        crate::RayTracingStats::default()
    );
    assert_eq!(renderer.read_pixels(), before);
}

#[test]
fn without_ray_queries_they_are_off_with_a_note() {
    let Some((device, queue)) = test_device() else {
        return skipped();
    };
    let list = mirror_floor(0.0);
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&reflections(RayTracedReflections::Off));
    renderer.render(&list).unwrap();
    let before = renderer.read_pixels();
    renderer.set_graphics(&reflections(RayTracedReflections::High));
    renderer.render(&list).unwrap();
    assert_eq!(
        renderer.graphics().ray_traced_reflections,
        RayTracedReflections::Off
    );
    assert!(
        renderer
            .graphics()
            .notes
            .iter()
            .any(|note| note.contains("reflections are off"))
    );
    assert!(renderer.scene.ray_reflections.is_none());
    assert_eq!(renderer.read_pixels(), before);
}
