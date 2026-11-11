//! Point light shadows (six faces per light, as many lights as the shadow
//! quality allows) and contact shadows (a short march toward the light
//! through the prepass's depth).

use super::*;

/// The frame size these tests draw at.
const SIZE: u32 = 128;

fn block(middle: Vec3, size: Vec3) -> DrawItem {
    let mut block = item(Shape::Block, middle, 1.0);
    block.transform = Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, middle);
    block.material.base_color = [0.8; 3];
    block.material.roughness = 1.0;
    block
}

/// Where `point` is drawn in a `SIZE`² frame of `list`, as (column, row).
fn screen_of(list: &DrawList, point: Vec3) -> (usize, usize) {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let x = ((ndc.x * 0.5 + 0.5) * SIZE as f32) as usize;
    let y = ((0.5 - ndc.y * 0.5) * SIZE as f32) as usize;
    (x, y)
}

/// A frame of `list`, as rows of RGBA pixels.
fn frame(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn red_at(pixels: &[u8], (x, y): (usize, usize)) -> u8 {
    pixels[(y * SIZE as usize + x) * 4]
}

/// A point light at the middle, a 2-stud block 3 studs from it along
/// `side`, and a wall 8 studs out that way, seen from just beside the
/// block looking at the wall. Lit by the light alone.
fn wall_behind_a_block(side: Vec3) -> (DrawList, Vec3) {
    // Across the wall, and a different way for the camera to stand.
    let across = if side.y.abs() > 0.5 { Vec3::X } else { Vec3::Y };
    let wall_size = Vec3::splat(24.0) - side.abs() * 23.0;
    let wall = block(side * 8.0, wall_size);
    let blocker = block(side * 3.0, Vec3::splat(2.0));
    let mut list = list_of(vec![wall, blocker]);
    list.sun.intensity = 0.0;
    list.lights.push(Light {
        brightness: 12.0,
        ..shadowing_point_light(Vec3::ZERO, 20.0)
    });
    // The wall's face toward the light, where the block's shadow falls.
    let shaded = side * 7.5;
    list.camera = Camera {
        position: side * 5.0 + across * 2.5,
        look_at: shaded,
        fov_y_degrees: 70.0,
    };
    (list, shaded)
}

#[test]
fn a_shadow_casting_point_light_shadows_a_part_on_every_side_of_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    let sides = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    for side in sides {
        let (mut list, shaded) = wall_behind_a_block(side);
        let pixel = screen_of(&list, shaded);
        let shadowed = red_at(&frame(&mut renderer, &list), pixel);
        assert_eq!(renderer.scene.point_shadow_maps_alive(), 1, "{side}");

        list.items[1].casts_shadow = false;
        let lit = red_at(&frame(&mut renderer, &list), pixel);
        assert!(
            lit > shadowed + 40,
            "the block shadows the wall toward {side}: {shadowed} vs {lit}"
        );

        // With `Shadows` false there's none, and no point shadow map.
        list.items[1].casts_shadow = true;
        list.lights[0].point_shadows = None;
        let unshadowed = red_at(&frame(&mut renderer, &list), pixel);
        assert!(
            unshadowed.abs_diff(lit) <= 2,
            "{side}: {unshadowed} vs {lit}"
        );
        assert!(renderer.scene.point_shadow_map.is_none());
        assert_eq!(renderer.scene.point_shadow_maps_alive(), 0);
    }
}

#[test]
fn only_the_nearest_shadow_casting_point_lights_get_shadow_faces() {
    let mut list = list_of(vec![]);
    let point = |z: f32, shadows: bool| Light {
        point_shadows: shadows.then_some(crate::PointShadows { near: [0.0; 6] }),
        ..point_light(Vec3::new(0.0, 0.0, z), 5.0)
    };
    // Farthest first, so the order they're listed in doesn't decide.
    list.lights = vec![point(-15.0, true), point(-10.0, true), point(-5.0, false)];
    let planned = plan_lights(&list, 1.0, (64, 64), ShadowQuality::Medium);
    let places: Vec<(f32, f32)> = planned
        .lights
        .iter()
        .map(|light| (light.position_range[2], light.shadow[0]))
        .collect();
    // Medium gives one point light its faces: the nearest that casts.
    assert_eq!(places, [(-5.0, -1.0), (-10.0, 0.0), (-15.0, -1.0)]);
    assert_eq!(planned.point_shadows.len(), 1);
    assert!(planned.lights[1].shadow[1] > 0.0, "texel width per stud");

    let high = plan_lights(&list, 1.0, (64, 64), ShadowQuality::High);
    assert_eq!(high.point_shadows.len(), 2);
    // Each face looks its own way: a point straight out along an axis is
    // in the middle of that face's view, in front of the light.
    let faces = high.point_shadows[0].faces;
    let light = Vec3::new(0.0, 0.0, -10.0);
    let axes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    for (face, axis) in faces.iter().zip(axes) {
        let clip = *face * (light + axis * 2.0).extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4, "{axis}: {ndc}");
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "{axis}: {ndc}");
    }

    let low = plan_lights(&list, 1.0, (64, 64), ShadowQuality::Low);
    assert!(low.point_shadows.is_empty());
    assert!(low.lights.iter().all(|light| light.shadow[0] == -1.0));
}

#[test]
fn low_shadows_allocate_no_point_shadow_map() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (list, _) = wall_behind_a_block(Vec3::NEG_Z);
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let point_map = |renderer: &TextureRenderer| {
        let map = renderer.scene.point_shadow_map.as_ref()?;
        Some((map.size, map.layers.len()))
    };
    renderer.render(&list).unwrap();
    let high = crate::PointShadowMapSize {
        size: 512,
        lights: 2,
    };
    assert_eq!(point_map(&renderer), Some((high, 12)), "six faces each");

    for quality in ShadowQuality::ALL.iter().cycle().take(20) {
        renderer.set_graphics(&with_shadows(*quality));
        renderer.render(&list).unwrap();
        let expected = quality.point_map();
        assert_eq!(point_map(&renderer).map(|(size, _)| size), expected);
        assert_eq!(
            renderer.scene.point_shadow_maps_alive(),
            usize::from(expected.is_some())
        );
    }
}

/// A 1-stud block resting on a wide floor, in sun from the right (+X) and
/// above at 45°, seen from the left (the shaded side) and above.
fn block_resting_on_a_floor() -> DrawList {
    let floor = block(Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 40.0));
    let small = block(Vec3::new(0.0, 0.5, 0.0), Vec3::ONE);
    let mut list = list_of(vec![floor, small]);
    list.sun.direction = Vec3::new(1.0, 1.0, 0.0);
    list.camera = Camera {
        position: Vec3::new(-3.0, 2.5, 1.5),
        look_at: Vec3::new(-0.6, 0.2, 0.0),
        fov_y_degrees: 70.0,
    };
    list
}

/// High, with the shadow maps at `shadows` and contact shadows `on` or off.
fn with_contact(shadows: ShadowQuality, on: bool) -> GraphicsSettings {
    GraphicsSettings {
        contact_shadows: Some(on),
        ..with_shadows(shadows)
    }
}

/// The pixels (column, row) that differ by more than 2 levels in any color
/// between two frames.
fn changed(a: &[u8], b: &[u8]) -> Vec<(usize, usize)> {
    let width = SIZE as usize;
    (0..a.len() / 4)
        .filter(|&i| (0..3).any(|c| a[i * 4 + c].abs_diff(b[i * 4 + c]) > 2))
        .map(|i| (i % width, i / width))
        .collect()
}

#[test]
fn contact_shadows_darken_the_gap_by_a_resting_part_and_off_removes_only_that() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = block_resting_on_a_floor();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    // The floor a quarter of a stud from the block's shaded side, and far
    // from it.
    let gap = screen_of(&list, Vec3::new(-0.75, 0.0, 0.0));
    let open = screen_of(&list, Vec3::new(-1.5, 0.0, -2.0));
    // Around the block and its gap: a contact shadow reaches 1 stud toward
    // the sun, so nothing changes past this box.
    let corners = (0..8).map(|corner| {
        let pick = |bit: i32, low: f32, high: f32| if corner & bit == 0 { low } else { high };
        Vec3::new(pick(1, -1.6, 0.6), pick(2, 0.0, 1.1), pick(4, -0.6, 0.6))
    });
    let screen: Vec<(usize, usize)> = corners.map(|corner| screen_of(&list, corner)).collect();
    let near_block = |(x, y): (usize, usize)| {
        let (left, right) = (
            screen.iter().map(|p| p.0).min(),
            screen.iter().map(|p| p.0).max(),
        );
        let (top, bottom) = (
            screen.iter().map(|p| p.1).min(),
            screen.iter().map(|p| p.1).max(),
        );
        (left.unwrap().saturating_sub(2)..=right.unwrap() + 2).contains(&x)
            && (top.unwrap().saturating_sub(2)..=bottom.unwrap() + 2).contains(&y)
    };

    // No shadow maps, so only contact shadows can darken the gap.
    renderer.set_graphics(&with_contact(ShadowQuality::Off, true));
    let on = frame(&mut renderer, &list);
    assert!(
        renderer.scene.frame_targets.occlusion.is_some(),
        "the prepass is drawn"
    );
    renderer.set_graphics(&with_contact(ShadowQuality::Off, false));
    let off = frame(&mut renderer, &list);
    assert!(
        red_at(&on, gap) + 30 < red_at(&off, gap),
        "the gap darkens: {} vs {}",
        red_at(&on, gap),
        red_at(&off, gap)
    );
    assert!(red_at(&on, open).abs_diff(red_at(&off, open)) <= 1);
    let differ = changed(&on, &off);
    assert!(!differ.is_empty());
    assert!(
        differ.iter().all(|&pixel| near_block(pixel)),
        "only around the block: {differ:?}"
    );

    // With the sun's shadow maps too, turning them off still changes
    // nothing away from the block.
    renderer.set_graphics(&with_contact(ShadowQuality::High, true));
    let on = frame(&mut renderer, &list);
    renderer.set_graphics(&with_contact(ShadowQuality::High, false));
    let off = frame(&mut renderer, &list);
    assert!(
        changed(&on, &off).iter().all(|&pixel| near_block(pixel)),
        "only around the block"
    );

    // The world's ShadowsEnabled turns them off too.
    let mut unshadowed = list.clone();
    unshadowed.shadows.enabled = false;
    renderer.set_graphics(&with_contact(ShadowQuality::Off, true));
    let world_off = frame(&mut renderer, &unshadowed);
    renderer.set_graphics(&with_contact(ShadowQuality::Off, false));
    assert!(changed(&world_off, &frame(&mut renderer, &unshadowed)).is_empty());
}

#[test]
fn contact_shadows_alone_draw_the_prepass_but_not_ambient_occlusion() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = block_resting_on_a_floor();
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let settings = |occlusion, contact| GraphicsSettings {
        ambient_occlusion: Some(occlusion),
        contact_shadows: Some(contact),
        ..with_shadows(ShadowQuality::Off)
    };
    use crate::AmbientOcclusion::{High, Off};
    renderer.set_graphics(&settings(Off, false));
    renderer.render(&list).unwrap();
    let plain = renderer.stats().draw_calls;
    assert_eq!(renderer.scene.occlusion_targets_alive(), 0);

    renderer.set_graphics(&settings(Off, true));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 1);
    assert_eq!(
        renderer.stats().draw_calls,
        plain + 1,
        "the prepass draws the solid batch again"
    );

    // Ambient occlusion on as well adds no more draws (its passes are
    // full-screen), and off again frees the targets.
    renderer.set_graphics(&settings(High, true));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 1);
    renderer.set_graphics(&settings(Off, false));
    renderer.render(&list).unwrap();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 0);
    assert_eq!(renderer.stats().draw_calls, plain);
}
