//! Ambient occlusion: the prepass and its occlusion darken corners and leave
//! direct light, the sky, see-through items and glow alone.

use super::*;

/// The floor just in front of the block's base, in the inside corner.
const CORNER: Vec3 = Vec3::new(0.0, 0.0, -7.85);
/// The floor well away from the block.
const OPEN_FLOOR: Vec3 = Vec3::new(-4.0, 0.0, -4.0);

#[test]
fn ambient_occlusion_darkens_the_corner_under_a_block_and_leaves_open_floor_alone() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    use crate::AmbientOcclusion::{High, Low, Off};
    let list = block_on_floor();
    let points = [CORNER, OPEN_FLOOR];
    let off = probe(&device, &queue, &list, Off, &points);
    assert!(off[0][0] > 100, "the floor is lit: {off:?}");
    assert!(within(off[0], off[1], 1), "flat light is even: {off:?}");
    for mode in [Low, High] {
        let on = probe(&device, &queue, &list, mode, &points);
        assert!(
            on[0][0] + 8 < off[0][0],
            "{mode:?}: the corner darkens: {on:?} vs {off:?}"
        );
        assert!(
            within(on[1], off[1], 2),
            "{mode:?}: open floor is unchanged: {on:?} vs {off:?}"
        );
    }

    // The world's strength: 0 is none, 2 darker than 1.
    let strength = |occlusion| {
        let mut list = block_on_floor();
        list.ambient.occlusion = occlusion;
        probe(&device, &queue, &list, High, &[CORNER])[0]
    };
    let (none, normal, double) = (strength(0.0), strength(1.0), strength(2.0));
    assert!(within(none, off[0], 1), "{none:?} vs {:?}", off[0]);
    assert!(double[0] + 5 < normal[0], "{double:?} vs {normal:?}");
}

#[test]
fn ambient_occlusion_leaves_the_sky_see_through_items_and_glow_alone() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    use crate::AmbientOcclusion::{High, Off};
    let same = |list: &DrawList, points: &[Vec3], what: &str| {
        let off = probe(&device, &queue, list, Off, points);
        let on = probe(&device, &queue, list, High, points);
        for (on, off) in on.iter().zip(&off) {
            assert!(within(*on, *off, 1), "{what}: {on:?} vs {off:?}");
        }
        off
    };

    // The sky above the block.
    let mut list = block_on_floor();
    list.sky = Sky::default();
    let sky = same(&list, &[Vec3::new(0.0, 60.0, -60.0)], "sky");
    assert!(sky[0][2] > 100, "the sky shows: {sky:?}");

    // A nearly solid see-through block in front of the corner. Were it
    // darkened like the solid floor behind it, it'd darken as much.
    let glass_face = Vec3::new(0.0, 0.5, -6.9);
    let mut list = block_on_floor();
    let behind = [
        probe(&device, &queue, &list, Off, &[glass_face])[0],
        probe(&device, &queue, &list, High, &[glass_face])[0],
    ];
    assert!(
        behind[1][0] + 4 < behind[0][0],
        "the floor behind it darkens: {behind:?}"
    );
    list.items.push(DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(3.0, 1.0, 1.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.5, -7.4),
        ),
        material: Material {
            base_color: [0.8; 3],
            roughness: 1.0,
            opacity: 0.99,
            ..Material::default()
        },
        ..big_block()
    });
    let glass = same(&list, &[glass_face], "see-through");
    assert!(glass[0][0] > 100, "the see-through block is lit: {glass:?}");

    // A glowing floor (black, so only its glow shows).
    let mut list = block_on_floor();
    list.items[0].material.base_color = [0.0; 3];
    list.items[0].material.emissive = [0.5; 3];
    let glow = same(&list, &[CORNER], "glow");
    assert!(glow[0][0] > 100, "the floor glows: {glow:?}");
}

#[test]
fn sun_and_lamp_light_are_untouched_by_ambient_occlusion_but_their_ambient_share_darkens() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    use crate::AmbientOcclusion::{High, Off};
    let corner = |list: &DrawList, mode| probe(&device, &queue, list, mode, &[CORNER])[0];
    // Linear light (post-processing is off, so the frame is sRGB of it).
    let linear = |pixel: [u8; 3]| linear_color([f32::from(pixel[0]) / 255.0; 3]).x;

    // The sun from above (no shadow map, so only ambient occlusion could
    // darken it), then a lamp in front of the block.
    let mut sun_only = block_on_floor();
    sun_only.ambient.intensity = 0.0;
    sun_only.sun.intensity = 1.0;
    let sun_lit = corner(&sun_only, Off);
    assert!(sun_lit[0] > 80, "the sun lights the corner: {sun_lit:?}");
    assert!(within(corner(&sun_only, High), sun_lit, 1));
    let mut lamp_only = block_on_floor();
    lamp_only.ambient.intensity = 0.0;
    lamp_only
        .lights
        .push(point_light(Vec3::new(0.0, 1.5, -5.0), 12.0));
    let lamp_lit = corner(&lamp_only, Off);
    assert!(lamp_lit[0] > 60, "the lamp lights the corner: {lamp_lit:?}");
    assert!(within(corner(&lamp_only, High), lamp_lit, 1));

    // Both together: only the ambient share darkens.
    let ambient_only = block_on_floor();
    let ambient_loss = linear(corner(&ambient_only, Off)) - linear(corner(&ambient_only, High));
    let mut both = block_on_floor();
    both.sun.intensity = 1.0;
    let both_loss = linear(corner(&both, Off)) - linear(corner(&both, High));
    assert!(ambient_loss > 0.02, "ambient darkens: {ambient_loss}");
    assert!(
        (both_loss - ambient_loss).abs() < 0.015,
        "with the sun on too, the same light is lost: {both_loss} vs {ambient_loss}"
    );
}

#[test]
fn with_ambient_occlusion_off_no_prepass_or_its_targets_exist() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    use crate::AmbientOcclusion::{High, Low, Off};
    let mut list = block_on_floor();
    // A see-through item, which the prepass leaves out.
    list.items
        .push(item(Shape::Sphere, Vec3::new(0.0, 5.0, -5.0), 0.5));
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_graphics(&with_occlusion(Off));
    renderer.render(&list).unwrap();
    assert!(renderer.scene.frame_targets.occlusion.is_none());
    assert_eq!(renderer.scene.occlusion_targets_alive(), 0);
    // The two solid blocks share a batch; the sphere is the other.
    let batches = plan(&list).batches;
    assert_eq!(batches.len(), 2);
    assert_eq!(renderer.stats().draw_calls, 2, "no prepass is drawn");

    renderer.set_graphics(&with_occlusion(High));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 1);
    assert_eq!(
        renderer.stats().draw_calls,
        3,
        "the prepass draws the solid batch again"
    );

    for (step, mode) in [Low, Off, High].iter().cycle().take(30).enumerate() {
        renderer.set_graphics(&with_occlusion(*mode));
        renderer.render(&list).unwrap();
        assert_eq!(
            renderer.scene.occlusion_targets_alive(),
            usize::from(*mode != Off),
            "step {step}: {mode:?}"
        );
        assert_eq!(renderer.scene.targets_alive(), 1, "step {step}");
    }
    renderer.set_graphics(&with_occlusion(Off));
    renderer.render(&list).unwrap();
    device.poll(wgpu::PollType::Wait).unwrap();
    assert_eq!(renderer.scene.occlusion_targets_alive(), 0);
    assert_eq!(renderer.stats().draw_calls, 2);
}

#[test]
fn the_prepass_and_occlusion_follow_the_render_scale() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = block_on_floor();
    let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
    renderer.set_graphics(&GraphicsSettings {
        render_scale: Some(0.5),
        ..with_occlusion(crate::AmbientOcclusion::High)
    });
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.frame_targets.size, (64, 64));
    assert_eq!(renderer.scene.occlusion_targets_alive(), 1);
    let off = probe(
        &device,
        &queue,
        &list,
        crate::AmbientOcclusion::Off,
        &[CORNER],
    );
    let pixels = renderer.read_pixels();
    let clip = view_projection(&list, 1.0) * CORNER.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let x = ((ndc.x * 0.5 + 0.5) * PROBE_SIZE as f32) as usize;
    let y = ((0.5 - ndc.y * 0.5) * PROBE_SIZE as f32) as usize;
    let at = (y * PROBE_SIZE as usize + x) * 4;
    assert!(
        pixels[at] + 4 < off[0][0],
        "the corner still darkens at half scale: {} vs {:?}",
        pixels[at],
        off[0]
    );
}
