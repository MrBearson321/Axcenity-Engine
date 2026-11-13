//! Light probe volumes: baked from inside a room with a red and a green
//! wall, lighting a part that moves through it with the walls' color, and
//! leaving the sky's light outside.

use super::*;
use crate::draw_list::{LightProbeGrid, LightProbeVolume};

/// The room's inside: 12 studs wide and deep, 6 high, its floor at 0.
const ROOM_MIDDLE: Vec3 = Vec3::new(0.0, 3.0, 0.0);
const ROOM_HALF: Vec3 = Vec3::new(6.0, 3.0, 6.0);
/// A probe every 3 studs.
const COUNTS: [u32; 3] = [4, 2, 4];

fn block(middle: Vec3, size: Vec3, base_color: [f32; 3]) -> DrawItem {
    let mut block = item(Shape::Block, middle, 1.0);
    block.transform = Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, middle);
    block.material.base_color = base_color;
    block
}

/// A closed room, its +X wall red and its -X wall green, the rest white,
/// lit by a lamp under the ceiling, under the default sky and a sun.
fn colored_room() -> DrawList {
    let (red, green, white) = ([0.9, 0.05, 0.05], [0.05, 0.9, 0.05], [0.8, 0.8, 0.8]);
    let (w, h) = (ROOM_HALF.x, ROOM_HALF.y);
    let mut list = list_of(vec![
        block(
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(13.0, 0.5, 13.0),
            white,
        ),
        block(
            Vec3::new(0.0, 2.0 * h + 0.25, 0.0),
            Vec3::new(13.0, 0.5, 13.0),
            white,
        ),
        block(
            Vec3::new(w + 0.25, h, 0.0),
            Vec3::new(0.5, 2.0 * h, 13.0),
            red,
        ),
        block(
            Vec3::new(-w - 0.25, h, 0.0),
            Vec3::new(0.5, 2.0 * h, 13.0),
            green,
        ),
        block(
            Vec3::new(0.0, h, w + 0.25),
            Vec3::new(13.0, 2.0 * h, 0.5),
            white,
        ),
        block(
            Vec3::new(0.0, h, -w - 0.25),
            Vec3::new(13.0, 2.0 * h, 0.5),
            white,
        ),
    ]);
    list.sky = Sky::default();
    list.sun = Sun {
        direction: Vec3::new(0.5, 1.0, 0.25),
        color: [1.0, 0.97, 0.92],
        intensity: 3.0,
    };
    list.lights = vec![Light {
        brightness: 6.0,
        ..point_light(Vec3::new(0.0, 5.0, 0.0), 30.0)
    }];
    list
}

/// Where each probe of a grid of `counts` filling the box stands, x
/// first, then y, then z.
fn probe_centers(counts: [u32; 3]) -> Vec<Vec3> {
    let mut centers = Vec::new();
    for z in 0..counts[2] {
        for y in 0..counts[1] {
            for x in 0..counts[0] {
                let cell = (Vec3::new(x as f32, y as f32, z as f32) + 0.5)
                    / Vec3::from_array(counts.map(|c| c as f32));
                centers.push(ROOM_MIDDLE + (cell - 0.5) * 2.0 * ROOM_HALF);
            }
        }
    }
    centers
}

fn bake(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> Vec<[[f32; 3]; 4]> {
    TextureRenderer::bake_light_probes(
        device,
        queue,
        list,
        &probe_centers(COUNTS),
        LIGHT_PROBE_BAKE_SIZE,
        &GraphicsSettings::default(),
    )
    .unwrap()
}

/// `list` with a volume filling the room, its probes `probes`.
fn with_volume(list: &DrawList, probes: Vec<[[f32; 3]; 4]>) -> DrawList {
    let mut list = list.clone();
    list.light_probe_volumes.push(LightProbeVolume {
        center: ROOM_MIDDLE,
        rotation: glam::Quat::IDENTITY,
        half_size: ROOM_HALF,
        blend_distance: 0.5,
        probes: LightProbeGrid {
            key: 1,
            counts: COUNTS,
            probes: Arc::new(probes),
            invalid: Arc::default(),
            sky: None,
        },
    });
    list
}

/// `list` with a white block at `at`, seen from 3 studs toward +Z: the
/// middle pixel, on the block's face toward the camera.
fn white_block_at(renderer: &mut TextureRenderer, list: &DrawList, at: Vec3) -> [i32; 3] {
    let mut list = list.clone();
    list.items
        .push(block(at, Vec3::splat(1.5), [0.8, 0.8, 0.8]));
    list.camera = Camera {
        position: at + Vec3::new(0.0, 0.5, 3.0),
        look_at: at,
        fov_y_degrees: 30.0,
    };
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let at = (32 * 64 + 32) * 4;
    [0, 1, 2].map(|channel| i32::from(pixels[at + channel]))
}

#[test]
fn a_part_moving_through_a_lit_room_takes_the_color_of_the_wall_beside_it_and_the_sky_outside() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let room = colored_room();
    let probed = with_volume(&room, bake(&device, &queue, &room));
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);

    // How much redder than green the block looks, with and without probes.
    let redness = |[r, g, _]: [i32; 3]| r - g;
    let near_red = Vec3::new(4.0, 1.0, 0.0);
    let near_green = Vec3::new(-4.0, 1.0, 0.0);
    let sky_red = white_block_at(&mut renderer, &room, near_red);
    let sky_green = white_block_at(&mut renderer, &room, near_green);
    let probe_red = white_block_at(&mut renderer, &probed, near_red);
    let probe_green = white_block_at(&mut renderer, &probed, near_green);
    assert!(
        (redness(sky_red) - redness(sky_green)).abs() <= 3,
        "lit by the sky alone it looks the same anywhere: {sky_red:?}, {sky_green:?}"
    );
    let [r, g, _] = probe_red;
    assert!(
        r > g,
        "beside the red wall the probes light it red: {probe_red:?}"
    );
    let [r, g, _] = probe_green;
    assert!(g > r, "beside the green wall, green: {probe_green:?}");
    assert!(
        redness(probe_red) >= redness(probe_green) + 16,
        "moving it across the room changes its color: {probe_red:?} then {probe_green:?}"
    );

    // Outside the volume the sky lights it, just as with no volume at all.
    let outside = Vec3::new(30.0, 1.0, 0.0);
    assert_eq!(
        white_block_at(&mut renderer, &probed, outside),
        white_block_at(&mut renderer, &room, outside)
    );
}

#[test]
fn baking_is_repeatable_and_each_probe_sees_the_wall_beside_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let room = colored_room();
    let first = bake(&device, &queue, &room);
    let again = bake(&device, &queue, &room);
    assert_eq!(first.len(), probe_centers(COUNTS).len());
    for (a, b) in first.iter().zip(&again) {
        for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
            assert!((a - b).abs() < 1e-4, "{a} then {b}");
        }
    }
    // The first probe is at the -X end of its row (by the green wall), the
    // fourth at the +X end (by the red one). A white surface facing the
    // wall beside each takes the wall's color.
    let facing = |light: &[[f32; 3]; 4], x: f32| {
        let [c0, _, _, c3] = light.map(Vec3::from_array);
        c0 * 0.282095 + c3 * x * 0.488603
    };
    let green = facing(&first[0], -1.0);
    assert!(
        green.y > 2.0 * green.x && green.y > 2.0 * green.z,
        "{green}"
    );
    let red = facing(&first[3], 1.0);
    assert!(red.x > 2.0 * red.y && red.x > 2.0 * red.z, "{red}");
}

#[test]
fn probes_are_uploaded_once_and_their_buffer_shrinks_without_them() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let room = colored_room();
    let probed = with_volume(&room, vec![[[0.5; 3]; 4]; probe_centers(COUNTS).len()]);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let blank = renderer.scene.light_probes.buffer().size();
    for _ in 0..4 {
        renderer.render(&probed).unwrap();
    }
    assert_eq!(
        renderer.scene.light_probes.uploads, 1,
        "once, not each frame"
    );
    assert!(renderer.scene.light_probes.buffer().size() > blank);
    renderer.render(&room).unwrap();
    assert_eq!(renderer.scene.light_probes.buffer().size(), blank);
}

/// R18.2: a probe baked inside a wall (here, glowing red with the light it
/// saw there) leaks into the room until it's marked invalid; then the blend
/// leaves it out and makes the others' shares whole. With every probe
/// around a point invalid, the sky lights it as if there were no volume.
#[test]
fn a_probe_marked_inside_a_wall_no_longer_lights_the_room() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let sky_only = list_of(Vec::new());
    // Two probes along X, at -3 and 3: a dark one in the room, a red one
    // in the "wall".
    let volume = |invalid: Vec<u32>, key: u64| {
        let mut list = sky_only.clone();
        list.light_probe_volumes.push(LightProbeVolume {
            center: ROOM_MIDDLE,
            rotation: glam::Quat::IDENTITY,
            half_size: ROOM_HALF,
            blend_distance: 0.5,
            probes: LightProbeGrid {
                key,
                counts: [2, 1, 1],
                probes: Arc::new(vec![
                    [[0.0; 3]; 4],
                    [[4.0, 0.0, 0.0], [0.0; 3], [0.0; 3], [0.0; 3]],
                ]),
                invalid: Arc::new(invalid),
                sky: None,
            },
        });
        list
    };
    let at = Vec3::new(-1.5, 3.0, 0.0);
    let leaking = white_block_at(&mut renderer, &volume(Vec::new(), 1), at);
    let walled = white_block_at(&mut renderer, &volume(vec![1], 2), at);
    let neither = white_block_at(&mut renderer, &volume(vec![0, 1], 3), at);
    let sky = white_block_at(&mut renderer, &sky_only, at);
    assert!(
        leaking[0] - leaking[1] > 20,
        "the wall's red leaks: {leaking:?}"
    );
    assert!(
        (walled[0] - walled[1]).abs() <= 3 && walled[0] < leaking[0] - 20,
        "only the dark probe lights it: {walled:?} (leaking {leaking:?})"
    );
    let most = (0..3).map(|c| (neither[c] - sky[c]).abs()).max().unwrap();
    assert!(most <= 2, "{neither:?} against the sky's {sky:?}");
}

/// The light a reflection probe at `at` + 3 studs toward +Z sees in the
/// middle of its -Z face (a white block at `at`, lit by `list`'s volume),
/// in linear light, so shares of light add up as they do in the shader.
fn linear_block_light(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    list: &DrawList,
    at: Vec3,
) -> Vec3 {
    const SIZE: u32 = 16;
    let mut list = list.clone();
    list.items
        .push(block(at, Vec3::splat(1.5), [0.8, 0.8, 0.8]));
    let faces = TextureRenderer::bake_reflection_probe(
        device,
        queue,
        &list,
        at + Vec3::new(0.0, 0.0, 3.0),
        SIZE,
        &GraphicsSettings::default(),
    )
    .unwrap();
    let texel = ((5 * SIZE + SIZE / 2) * SIZE + SIZE / 2) as usize * 3;
    Vec3::from_slice(&faces[texel..texel + 3])
}

/// R18.3: a bake that keeps its sky light apart follows the sky drawn now.
/// The room is open to the sky and lit by a lamp; with the sky at half its
/// baked light, a block in the room loses half the sky's share of its
/// light (the share the probes keep apart), and keeps the lamp's. A bake
/// without it (as before R18.3) stays as baked, and so does one with it
/// while the sky is as baked.
#[test]
fn a_sky_dimmed_to_half_halves_the_light_probes_sky_share() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut room = colored_room();
    room.items.remove(1); // The ceiling: open to the sky.
    room.sun.intensity = 0.0;
    let centers = probe_centers(COUNTS);
    let mut baker = crate::LightProbeBaker::keeping_sky_apart(
        &device,
        &queue,
        &room,
        LIGHT_PROBE_BAKE_SIZE,
        &GraphicsSettings::default(),
    );
    let baked: Vec<crate::BakedProbe> = centers.iter().map(|&c| baker.bake(c).unwrap()).collect();
    let band0 = baker.baked_sky();
    assert!(band0.iter().all(|&c| c > 0.0), "{band0:?}");
    let light: Vec<_> = baked.iter().map(|probe| probe.light).collect();
    let sky: Vec<_> = baked.iter().map(|probe| probe.sky.unwrap()).collect();
    let fixed: Vec<[[f32; 3]; 4]> = light
        .iter()
        .zip(&sky)
        .map(|(light, sky)| {
            std::array::from_fn(|k| std::array::from_fn(|c| light[k][c] - sky[k][c]))
        })
        .collect();
    // The sky lights the open room a good deal, and the lamp the rest.
    let share = |probes: &[[[f32; 3]; 4]]| probes.iter().map(|p| p[0][2]).sum::<f32>();
    assert!(
        share(&sky) > 0.2 * share(&light) && share(&fixed) > 0.1 * share(&light),
        "sky {} of {}",
        share(&sky),
        share(&light)
    );

    let volume = |list: &DrawList, probes: &[[[f32; 3]; 4]], sky: Option<&[[[f32; 3]; 4]]>| {
        let mut list = list.clone();
        list.light_probe_volumes.push(LightProbeVolume {
            center: ROOM_MIDDLE,
            rotation: glam::Quat::IDENTITY,
            half_size: ROOM_HALF,
            blend_distance: 0.5,
            probes: LightProbeGrid {
                key: 1,
                counts: COUNTS,
                probes: Arc::new(probes.to_vec()),
                invalid: Arc::default(),
                sky: sky.map(|sky| crate::ProbeSky {
                    band0,
                    probes: Arc::new(sky.to_vec()),
                }),
            },
        });
        list
    };
    let mut dim = room.clone();
    for color in [
        &mut dim.sky.top_color,
        &mut dim.sky.horizon_color,
        &mut dim.sky.ground_color,
    ] {
        // Half the light: sRGB colors, so through linear light.
        let linear = crate::renderer::lighting::linear_color(*color) * 0.5;
        *color = linear.to_array().map(|c| {
            if c <= 0.0031308 {
                c * 12.92
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        });
    }
    let at = Vec3::new(0.0, 2.0, -1.0);
    let measure = |list: &DrawList| linear_block_light(&device, &queue, list, at);
    let old = measure(&volume(&room, &light, None));
    let apart = measure(&volume(&room, &light, Some(&sky)));
    let without_sky = measure(&volume(&room, &fixed, None));
    let old_dim = measure(&volume(&dim, &light, None));
    let apart_dim = measure(&volume(&dim, &light, Some(&sky)));
    let close =
        |a: Vec3, b: Vec3, of: Vec3| (a - b).abs().max_element() <= 0.02 * of.max_element() + 1e-3;
    assert!(close(apart, old, old), "as baked: {apart} against {old}");
    // The dimmer sky changes what both see the same way (its reflection);
    // the bake that keeps the sky apart loses half the sky's share too.
    let lost = old_dim - apart_dim;
    let half_sky = (old - without_sky) * 0.5;
    assert!(half_sky.max_element() > 0.01, "{half_sky}");
    assert!(
        close(lost, half_sky, old),
        "lost {lost}, half the sky's share {half_sky}"
    );
}
