//! The sky's environment maps: baked once per sky, reflected and lit from
//! in place of the gradient, and showing a sky image.

use super::*;
use crate::draw_list::SkyImage;
use crate::graphics::ReflectionQuality;

/// How far, on average per channel, the player's example snapshots may be
/// from their references (`REFERENCE_TOLERANCE` in the player's tests).
const HARNESS_TOLERANCE: f32 = 6.0;

fn with_reflections(quality: ReflectionQuality) -> GraphicsSettings {
    GraphicsSettings {
        reflection_quality: Some(quality),
        ..GraphicsSettings::default()
    }
}

fn sphere(position: Vec3, base_color: [f32; 3], metalness: f32, roughness: f32) -> DrawItem {
    let mut sphere = item(Shape::Sphere, position, 1.0);
    sphere.transform =
        Mat4::from_scale_rotation_translation(Vec3::splat(2.0), glam::Quat::IDENTITY, position);
    sphere.material = Material {
        base_color,
        metalness,
        roughness,
        ..Material::default()
    };
    sphere
}

/// Smooth and rough metals and plastics on a floor under the default sky
/// and sun, seen from above the floor, with post-processing on.
fn material_spheres() -> DrawList {
    let mut floor = item(Shape::Block, Vec3::new(0.0, -1.5, -8.0), 1.0);
    floor.transform = Mat4::from_scale_rotation_translation(
        Vec3::new(30.0, 1.0, 30.0),
        glam::Quat::IDENTITY,
        Vec3::new(0.0, -1.5, -8.0),
    );
    floor.material.base_color = [0.6, 0.6, 0.55];
    let mut list = list_of(vec![
        floor,
        sphere(Vec3::new(-3.3, 0.0, -7.0), [0.95, 0.95, 0.95], 1.0, 0.1),
        sphere(Vec3::new(-1.1, 0.0, -7.0), [0.95, 0.75, 0.4], 1.0, 0.5),
        sphere(Vec3::new(1.1, 0.0, -7.0), [0.8, 0.2, 0.15], 0.0, 0.8),
        sphere(Vec3::new(3.3, 0.0, -7.0), [0.2, 0.4, 0.8], 0.0, 0.3),
    ]);
    list.camera = Camera {
        position: Vec3::new(0.0, 1.5, 0.0),
        look_at: Vec3::new(0.0, -0.5, -7.0),
        fov_y_degrees: 60.0,
    };
    list.sky = Sky::default();
    list.sun = Sun {
        direction: Vec3::new(0.5, 1.0, 0.25),
        color: [1.0, 0.97, 0.92],
        intensity: 3.0,
    };
    list.ambient = AmbientLight {
        color: [0.75, 0.85, 1.0],
        intensity: 0.05,
        occlusion: 1.0,
    };
    list.post = PostProcessing::default();
    list
}

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    list: &DrawList,
    settings: &GraphicsSettings,
) -> Vec<u8> {
    let mut renderer = TextureRenderer::new(device, queue, 160, 90);
    renderer.set_graphics(settings);
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

/// The average difference per color channel between two frames.
fn average_difference(a: &[u8], b: &[u8]) -> f32 {
    let pixels = a.len() / 4;
    let total: u32 = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .map(|(a, b)| (0..3).map(|c| u32::from(a[c].abs_diff(b[c]))).sum::<u32>())
        .sum();
    total as f32 / (pixels * 3) as f32
}

#[test]
fn with_the_default_sky_metals_and_plastics_look_as_before() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = material_spheres();
    let before = draw(
        &device,
        &queue,
        &list,
        &with_reflections(ReflectionQuality::Low),
    );
    for quality in [
        ReflectionQuality::Medium,
        ReflectionQuality::High,
        ReflectionQuality::Ultra,
    ] {
        let baked = draw(&device, &queue, &list, &with_reflections(quality));
        let difference = average_difference(&before, &baked);
        eprintln!("{quality:?}: {difference:.2} per channel from the gradient");
        assert!(
            difference < HARNESS_TOLERANCE,
            "{quality:?} differs from the gradient by {difference:.2} per channel"
        );
        // The mirror sphere still shows the sky's blue (the cubemap's faces
        // point the right way) and the frame isn't blank.
        assert!(difference > 0.0, "{quality:?} didn't use the maps");
    }
}

/// A plain sky image of one color, brighter than 1.
fn plain_sky(key: u64, rgb: [f32; 3]) -> SkyImage {
    let (width, height) = (32, 16);
    SkyImage {
        key,
        width,
        height,
        rgb: Arc::new(rgb.repeat((width * height) as usize)),
    }
}

fn mirror_list(image: Option<SkyImage>) -> DrawList {
    let mut mirror = big_block();
    mirror.material = Material {
        base_color: [1.0; 3],
        metalness: 1.0,
        roughness: 0.05,
        ..Material::default()
    };
    let mut list = list_of(vec![mirror]);
    list.sun.intensity = 0.0;
    list.sky = Sky {
        environment_intensity: 1.0,
        image,
        ..Sky::default()
    };
    list
}

#[test]
fn a_sky_image_shows_in_reflections_and_behind_the_scene() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = mirror_list(Some(plain_sky(7, [1.5, 0.0, 0.0])));
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let middle: [u8; 4] = pixels[(32 * 64 + 32) * 4..][..4].try_into().unwrap();
    let corner: [u8; 4] = pixels[..4].try_into().unwrap();
    assert!(
        middle[0] > 150 && middle[1] < 40 && middle[2] < 40,
        "the mirror reflects the red sky image: {middle:?}"
    );
    assert!(
        corner[0] > 150 && corner[1] < 40 && corner[2] < 40,
        "the red sky image is behind the scene: {corner:?}"
    );

    // At Low there are no maps, so the gradient's blue shows instead.
    renderer.set_graphics(&with_reflections(ReflectionQuality::Low));
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let middle: [u8; 4] = pixels[(32 * 64 + 32) * 4..][..4].try_into().unwrap();
    assert!(middle[2] > middle[0], "the gradient at Low: {middle:?}");
    assert_eq!(
        renderer.scene.environment.bytes(),
        0,
        "nothing is kept at Low"
    );
}

#[test]
fn changing_the_sky_bakes_the_maps_once_not_every_frame() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = mirror_list(None);
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let bakes = |renderer: &TextureRenderer| renderer.scene.environment.bakes;
    for _ in 0..3 {
        renderer.render(&list).unwrap();
    }
    assert_eq!(bakes(&renderer), 1);
    assert!(renderer.stats().target_bytes >= renderer.scene.environment.bytes());
    assert!(renderer.scene.environment.bytes() > 0);

    list.sky.top_color = [0.9, 0.3, 0.2];
    for _ in 0..3 {
        renderer.render(&list).unwrap();
    }
    assert_eq!(bakes(&renderer), 2, "a new color bakes once");

    // The environment intensity and the sun aren't baked in.
    list.sky.environment_intensity = 0.3;
    list.sun.direction = Vec3::new(1.0, 1.0, 0.0);
    renderer.render(&list).unwrap();
    assert_eq!(bakes(&renderer), 2);

    // A sky image bakes once, and so does going back to the gradient.
    list.sky.image = Some(plain_sky(9, [0.2, 0.4, 1.0]));
    renderer.render(&list).unwrap();
    renderer.render(&list).unwrap();
    assert_eq!(bakes(&renderer), 3);
    list.sky.image = None;
    renderer.render(&list).unwrap();
    assert_eq!(bakes(&renderer), 4);

    // Low frees the maps and bakes nothing; coming back bakes once.
    renderer.set_graphics(&with_reflections(ReflectionQuality::Low));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.environment.bytes(), 0);
    assert_eq!(bakes(&renderer), 4);
    renderer.set_graphics(&with_reflections(ReflectionQuality::Ultra));
    renderer.render(&list).unwrap();
    renderer.render(&list).unwrap();
    assert_eq!(bakes(&renderer), 5);
}
