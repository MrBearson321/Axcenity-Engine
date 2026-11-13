//! Real-time GI: a light probe volume beside a red wall, baked with the sun
//! on the wall's face, then the sun moved behind the wall. With real-time
//! GI on, the probes are traced again within a few frames and match a fresh
//! bake; with it off, they keep the old bake.

use super::*;
use crate::draw_list::{LightProbeGrid, LightProbeVolume};
use crate::graphics::Capabilities;

/// Reaching below the floor, so the floor is inside it.
pub(super) const VOLUME_MIDDLE: Vec3 = Vec3::new(0.0, 1.5, 0.0);
const VOLUME_HALF: Vec3 = Vec3::new(3.0, 2.0, 3.0);
const COUNTS: [u32; 3] = [2, 2, 2];
pub(super) const PROBES: u32 = 8;

pub(super) fn block(middle: Vec3, size: Vec3, base_color: [f32; 3]) -> DrawItem {
    let mut block = item(Shape::Block, middle, 1.0);
    block.transform = Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, middle);
    block.material.base_color = base_color;
    block
}

/// A white floor and a red wall at +X facing the volume, under the default
/// sky, lit by a sun from `toward` (the way to the sun).
pub(super) fn yard(toward: Vec3) -> DrawList {
    let mut list = list_of(vec![
        block(
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(20.0, 0.5, 20.0),
            [0.8, 0.8, 0.8],
        ),
        block(
            Vec3::new(3.5, 3.0, 0.0),
            Vec3::new(0.5, 6.0, 10.0),
            [0.9, 0.05, 0.05],
        ),
    ]);
    list.sky = Sky::default();
    list.sun = Sun {
        direction: toward,
        color: [1.0, 0.97, 0.92],
        intensity: 3.0,
    };
    list
}

/// On the wall's face toward the volume.
pub(super) const SUN_ON_WALL: Vec3 = Vec3::new(-1.0, 1.0, 0.3);
/// Behind the wall, so its face toward the volume is in its own shadow.
pub(super) const SUN_BEHIND_WALL: Vec3 = Vec3::new(1.0, 1.0, 0.3);

/// Medium shadows, as the probe camera draws them at any quality, so its
/// faces and a bake's are drawn alike.
fn settings(realtime_gi: bool) -> GraphicsSettings {
    GraphicsSettings {
        shadow_quality: Some(ShadowQuality::Medium),
        realtime_gi: Some(realtime_gi),
        ..GraphicsSettings::default()
    }
}

pub(super) fn probe_centers() -> Vec<Vec3> {
    let mut centers = Vec::new();
    for z in 0..COUNTS[2] {
        for y in 0..COUNTS[1] {
            for x in 0..COUNTS[0] {
                let cell = (Vec3::new(x as f32, y as f32, z as f32) + 0.5)
                    / Vec3::from_array(COUNTS.map(|c| c as f32));
                centers.push(VOLUME_MIDDLE + (cell - 0.5) * 2.0 * VOLUME_HALF);
            }
        }
    }
    centers
}

pub(super) fn bake(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    list: &DrawList,
) -> Vec<[[f32; 3]; 4]> {
    TextureRenderer::bake_light_probes(
        device,
        queue,
        list,
        &probe_centers(),
        LIGHT_PROBE_BAKE_SIZE,
        &settings(false),
    )
    .unwrap()
}

/// `list` with a volume around the probes, baked as `probes`.
pub(super) fn with_volume(list: &DrawList, probes: Vec<[[f32; 3]; 4]>) -> DrawList {
    let mut list = list.clone();
    list.light_probe_volumes.push(LightProbeVolume {
        center: VOLUME_MIDDLE,
        rotation: glam::Quat::IDENTITY,
        half_size: VOLUME_HALF,
        blend_distance: 0.5,
        probes: LightProbeGrid {
            key: 1,
            counts: COUNTS,
            probes: Arc::new(probes),
        },
    });
    list.camera = Camera {
        position: Vec3::new(-8.0, 3.0, 0.0),
        look_at: VOLUME_MIDDLE,
        fov_y_degrees: 60.0,
    };
    list
}

/// The volume's probes in the renderer's buffer now: its last `PROBES`
/// (after the camera's own, with ray traced GI).
pub(super) fn probes_held(renderer: &TextureRenderer) -> Vec<[[f32; 3]; 4]> {
    let texels = texels_held(renderer);
    texels[texels.len() - PROBES as usize..]
        .iter()
        .map(|probe| probe.map(|[r, g, b, _]| [r, g, b]))
        .collect()
}

/// Every probe in the renderer's buffer now, as the shaders read them.
pub(super) fn texels_held(renderer: &TextureRenderer) -> Vec<[[f32; 4]; 4]> {
    let scene = &renderer.scene;
    let source = scene.light_probes.buffer();
    let buffer = scene.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light probes read back"),
        size: source.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = scene.device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(source, 0, &buffer, 0, source.size());
    scene.queue.submit([encoder.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    scene
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let floats: Vec<f32> = bytemuck::cast_slice(
        &buffer
            .slice(..)
            .get_mapped_range()
            .expect("the buffer is mapped"),
    )
    .to_vec();
    floats
        .chunks(16)
        .map(|probe| std::array::from_fn(|k| std::array::from_fn(|c| probe[k * 4 + c])))
        .collect()
}

/// The largest difference between two sets of probes' coefficients.
pub(super) fn difference(a: &[[[f32; 3]; 4]], b: &[[[f32; 3]; 4]]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .flatten()
        .flatten()
        .zip(b.iter().flatten().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_moving_sun_updates_the_probes_within_a_few_frames_with_realtime_gi_on() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let lit = yard(SUN_ON_WALL);
    let moved = yard(SUN_BEHIND_WALL);
    let (before, after) = (bake(&device, &queue, &lit), bake(&device, &queue, &moved));
    let change = difference(&before, &after);
    assert!(change > 0.3, "moving the sun changes the bake: {change}");

    // The world still holds the bake from before the sun moved.
    let world = with_volume(&moved, before.clone());
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(true));
    // Two probes a frame: 8 probes in 4 frames, well within a few seconds.
    for _ in 0..PROBES / realtime_gi_probes_per_frame() {
        renderer.render(&world).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), Some(PROBES));
    let traced = probes_held(&renderer);
    let error = difference(&traced, &after);
    assert!(
        error < 0.01,
        "traced at run time like a fresh bake: off by {error} (the sun moved {change})"
    );

    // Tracing again where nothing changes keeps them the same.
    renderer.render(&world).unwrap();
    assert!(difference(&probes_held(&renderer), &after) < 0.01);
    let traced_frame = renderer.read_pixels();

    // Turning it off frees it and brings the bake back.
    renderer.set_graphics(&settings(false));
    renderer.render(&world).unwrap();
    assert_eq!(renderer.scene.probes_traced(), None);
    assert_eq!(probes_held(&renderer), before);
    // The floor in the volume, lit by the wall's red bounce in the bake
    // but not once the sun has moved behind the wall.
    let redness = |pixels: &[u8]| {
        pixels
            .chunks(4)
            .map(|p| i64::from(p[0]) - i64::from(p[1]))
            .sum::<i64>()
    };
    let (baked, traced) = (redness(&renderer.read_pixels()), redness(&traced_frame));
    assert!(
        traced < baked - 64 * 64,
        "the traced frame is less red: {traced} against {baked}"
    );
}

#[test]
fn still_light_is_traced_once_and_no_more() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let lit = yard(SUN_ON_WALL);
    let world = with_volume(&lit, bake(&device, &queue, &lit));
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(true));
    for _ in 0..PROBES / realtime_gi_probes_per_frame() + 3 {
        renderer.render(&world).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), Some(PROBES), "one pass");
    // The light moves: every probe is traced again.
    let moved = with_volume(&yard(SUN_BEHIND_WALL), bake(&device, &queue, &lit));
    for _ in 0..PROBES / realtime_gi_probes_per_frame() + 3 {
        renderer.render(&moved).unwrap();
    }
    assert_eq!(
        renderer.scene.probes_traced(),
        Some(2 * PROBES),
        "a second pass"
    );
}

/// `yard` with enough small blocks to count as a heavy scene.
fn heavy_yard(toward: Vec3) -> DrawList {
    let mut list = yard(toward);
    for index in 0..4001 {
        list.items.push(item(
            Shape::Block,
            Vec3::new(index as f32 * 0.01, -2.0, 0.0),
            0.01,
        ));
    }
    list
}

fn a_sky_picture() -> crate::SkyImage {
    crate::SkyImage {
        key: 7,
        width: 2,
        height: 1,
        rgb: Arc::new(vec![1.0; 6]),
    }
}

#[test]
fn a_heavy_scene_keeps_its_bake_when_its_sky_picture_arrives_late() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let lit = heavy_yard(SUN_ON_WALL);
    let baked = bake(&device, &queue, &yard(SUN_ON_WALL));
    let opening = with_volume(&lit, baked);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(true));
    renderer.render(&opening).unwrap();
    // The picture is read on another thread: it's there a frame later.
    let mut loaded = opening.clone();
    loaded.sky.image = Some(a_sky_picture());
    for _ in 0..12 {
        renderer.render(&loaded).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), Some(0));
}

#[test]
fn a_heavy_scene_traces_a_face_at_a_time_when_its_light_moves() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let baked = bake(&device, &queue, &yard(SUN_ON_WALL));
    let opening = with_volume(&heavy_yard(SUN_ON_WALL), baked.clone());
    let moved = with_volume(&heavy_yard(SUN_BEHIND_WALL), baked);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(true));
    renderer.render(&opening).unwrap();
    // One face every four frames: a probe's six in 24 frames, not two
    // probes (twelve whole draws of the scene) in every one.
    for _ in 0..23 {
        renderer.render(&moved).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), Some(0), "five faces so far");
    renderer.render(&moved).unwrap();
    assert_eq!(renderer.scene.probes_traced(), Some(1), "the sixth face");
}

#[test]
fn with_realtime_gi_off_a_moving_sun_leaves_the_baked_probes_and_the_frame_alone() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let before = bake(&device, &queue, &yard(SUN_ON_WALL));
    let world = with_volume(&yard(SUN_BEHIND_WALL), before.clone());
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(false));
    renderer.render(&world).unwrap();
    let first = renderer.read_pixels();
    for _ in 0..PROBES {
        renderer.render(&world).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), None, "nothing is traced");
    assert_eq!(probes_held(&renderer), before);
    assert_eq!(renderer.scene.light_probes.uploads, 1, "written once");
    assert_eq!(renderer.read_pixels(), first, "the frame doesn't change");
}

#[test]
fn a_gpu_without_compute_shaders_keeps_the_bake_and_notes_why() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let before = bake(&device, &queue, &yard(SUN_ON_WALL));
    let world = with_volume(&yard(SUN_BEHIND_WALL), before.clone());
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&settings(true));
    renderer.limit_capabilities(&Capabilities {
        compute_shaders: false,
        ..*renderer.capabilities()
    });
    assert!(!renderer.graphics().realtime_gi);
    assert!(
        renderer
            .graphics()
            .notes
            .iter()
            .any(|note| note.contains("Real-time GI is off")),
        "{:?}",
        renderer.graphics().notes
    );
    for _ in 0..PROBES {
        renderer.render(&world).unwrap();
    }
    assert_eq!(renderer.scene.probes_traced(), None);
    assert_eq!(probes_held(&renderer), before);
}

fn realtime_gi_probes_per_frame() -> u32 {
    super::super::realtime_gi::PROBES_PER_FRAME
}
