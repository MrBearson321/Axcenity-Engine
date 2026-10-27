//! Reflection probes: baked from inside a room, reflected in its box with
//! parallax-corrected lookups, filtered once, and left out at Low.

use super::*;
use crate::draw_list::{ProbeImage, ReflectionProbe};
use crate::graphics::ReflectionQuality;

/// The room's inside: 12 studs wide and deep, 6 high, its floor at 0.
const ROOM_MIDDLE: Vec3 = Vec3::new(0.0, 3.0, 0.0);
const ROOM_HALF: Vec3 = Vec3::new(6.0, 3.0, 6.0);

fn with_reflections(quality: ReflectionQuality) -> GraphicsSettings {
    GraphicsSettings {
        reflection_quality: Some(quality),
        ..GraphicsSettings::default()
    }
}

fn block(middle: Vec3, size: Vec3, base_color: [f32; 3]) -> DrawItem {
    let mut block = item(Shape::Block, middle, 1.0);
    block.transform = Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, middle);
    block.material.base_color = base_color;
    block
}

/// A closed room with red walls, a grey floor and ceiling, lit by a lamp,
/// with a mirror-like metal sphere in it, seen from inside.
fn red_room() -> DrawList {
    let red = [0.9, 0.05, 0.05];
    let grey = [0.5, 0.5, 0.5];
    let (w, h) = (ROOM_HALF.x, ROOM_HALF.y);
    let mut sphere = item(Shape::Sphere, Vec3::new(0.0, 2.0, -2.0), 1.0);
    sphere.transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(2.0),
        glam::Quat::IDENTITY,
        Vec3::new(0.0, 2.0, -2.0),
    );
    sphere.material = Material {
        base_color: [0.95, 0.95, 0.95],
        metalness: 1.0,
        roughness: 0.05,
        ..Material::default()
    };
    let mut list = list_of(vec![
        block(Vec3::new(0.0, -0.25, 0.0), Vec3::new(13.0, 0.5, 13.0), grey),
        block(
            Vec3::new(0.0, 2.0 * h + 0.25, 0.0),
            Vec3::new(13.0, 0.5, 13.0),
            grey,
        ),
        block(
            Vec3::new(w + 0.25, h, 0.0),
            Vec3::new(0.5, 2.0 * h, 13.0),
            red,
        ),
        block(
            Vec3::new(-w - 0.25, h, 0.0),
            Vec3::new(0.5, 2.0 * h, 13.0),
            red,
        ),
        block(
            Vec3::new(0.0, h, w + 0.25),
            Vec3::new(13.0, 2.0 * h, 0.5),
            red,
        ),
        block(
            Vec3::new(0.0, h, -w - 0.25),
            Vec3::new(13.0, 2.0 * h, 0.5),
            red,
        ),
        sphere,
    ]);
    list.sky = Sky::default();
    list.sun = Sun {
        direction: Vec3::new(0.5, 1.0, 0.25),
        color: [1.0, 0.97, 0.92],
        intensity: 3.0,
    };
    list.ambient = AmbientLight {
        color: [1.0; 3],
        intensity: 0.3,
        occlusion: 1.0,
    };
    list.lights = vec![Light {
        brightness: 3.0,
        ..point_light(Vec3::new(0.0, 5.0, 2.0), 30.0)
    }];
    list.camera = Camera {
        position: Vec3::new(0.0, 2.0, 3.0),
        look_at: Vec3::new(0.0, 2.0, -2.0),
        fov_y_degrees: 60.0,
    };
    list
}

/// `list` with a probe filling the room, baked from its middle.
fn with_probe(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> DrawList {
    let size = 32;
    let rgb = TextureRenderer::bake_reflection_probe(
        device,
        queue,
        list,
        ROOM_MIDDLE,
        size,
        &with_reflections(ReflectionQuality::High),
    )
    .unwrap();
    let mut list = list.clone();
    list.reflection_probes.push(ReflectionProbe {
        center: ROOM_MIDDLE,
        rotation: glam::Quat::IDENTITY,
        half_size: ROOM_HALF,
        blend_distance: 0.5,
        image: ProbeImage {
            key: 1,
            size,
            rgb: Arc::new(rgb),
        },
    });
    list
}

/// The middle of the frame: the front of the sphere, which reflects what's
/// behind the camera (the room's +Z wall, or the sky toward +Z).
fn sphere_front(renderer: &mut TextureRenderer, list: &DrawList) -> [u8; 4] {
    renderer.render(list).unwrap();
    let pixels = renderer.read_pixels();
    pixels[(32 * 64 + 32) * 4..][..4].try_into().unwrap()
}

fn renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    quality: ReflectionQuality,
) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, 64, 64);
    renderer.set_graphics(&with_reflections(quality));
    renderer
}

#[test]
fn a_metal_sphere_in_a_red_room_reflects_the_walls_with_a_probe_and_the_sky_without() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let room = red_room();
    let probed = with_probe(&device, &queue, &room);
    let mut renderer = renderer(&device, &queue, ReflectionQuality::High);
    let [r, g, b, _] = sphere_front(&mut renderer, &room);
    assert!(
        b > r && b > 100,
        "without a probe the sphere reflects the blue sky: {:?}",
        [r, g, b]
    );
    let [r, g, b, _] = sphere_front(&mut renderer, &probed);
    assert!(
        r > g.saturating_add(40) && r > b.saturating_add(40),
        "with a probe it reflects the red wall: {:?}",
        [r, g, b]
    );
}

#[test]
fn a_bake_sees_each_wall_through_its_own_face() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Each wall its own color, so a face turned the wrong way shows.
    let mut room = red_room();
    let colors = [
        [0.9, 0.05, 0.05],
        [0.05, 0.9, 0.05],
        [0.05, 0.05, 0.9],
        [0.9, 0.9, 0.05],
    ];
    for (wall, color) in room.items[2..6].iter_mut().zip(colors) {
        wall.material.base_color = color;
    }
    room.items.pop();
    let size = 16;
    let rgb = TextureRenderer::bake_reflection_probe(
        &device,
        &queue,
        &room,
        ROOM_MIDDLE,
        size,
        &with_reflections(ReflectionQuality::High),
    )
    .unwrap();
    assert_eq!(rgb.len(), (size * size * 6 * 3) as usize);
    // The middle of each side face: +X red, -X green, +Z blue, -Z yellow.
    let middle = |face: u32| {
        let at = ((face * size * size + size / 2 * size + size / 2) * 3) as usize;
        Vec3::from_slice(&rgb[at..at + 3])
    };
    let strongest = |color: Vec3| {
        let channels = color.to_array();
        (0..3)
            .max_by(|&a, &b| channels[a].total_cmp(&channels[b]))
            .unwrap()
    };
    assert_eq!(strongest(middle(0)), 0, "+X: {}", middle(0));
    assert_eq!(strongest(middle(1)), 1, "-X: {}", middle(1));
    assert_eq!(strongest(middle(4)), 2, "+Z: {}", middle(4));
    let yellow = middle(5);
    assert!(
        yellow.x > 4.0 * yellow.z && yellow.y > 4.0 * yellow.z,
        "-Z: {yellow}"
    );
    // Within a face, its right side looks toward the next wall round: on
    // +X (looking out from the middle) the right edge is toward -Z.
    let right_of_x = {
        let at = ((size / 2 * size + size - 1) * 3) as usize;
        Vec3::from_slice(&rgb[at..at + 3])
    };
    assert!(
        right_of_x.x > right_of_x.z,
        "+X's right is red or yellow: {right_of_x}"
    );
}

#[test]
fn a_probe_is_filtered_once_and_left_out_at_low() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let room = red_room();
    let probed = with_probe(&device, &queue, &room);
    let mut renderer = renderer(&device, &queue, ReflectionQuality::High);
    for _ in 0..4 {
        renderer.render(&probed).unwrap();
    }
    assert_eq!(
        renderer.scene.probes.filters, 1,
        "filtered once, not each frame"
    );
    assert!(renderer.scene.probes.allocated());
    let with_array = renderer.stats().target_bytes;
    renderer.render(&room).unwrap();
    assert!(!renderer.scene.probes.allocated(), "freed with no probes");
    assert!(renderer.stats().target_bytes < with_array);

    // At Low there are no environment maps, so no probes either: the
    // frame is the same with one or without, and nothing is allocated.
    let mut low = self::renderer(&device, &queue, ReflectionQuality::Low);
    let without = sphere_front(&mut low, &room);
    let with = sphere_front(&mut low, &probed);
    assert_eq!(with, without);
    assert!(!low.scene.probes.allocated());
}
