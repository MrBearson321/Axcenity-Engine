//! Anti-aliasing: MSAA and FXAA smooth edges, every kind of item draws the
//! same with each mode, and switching frees what's no longer used.

use super::*;

fn with_anti_aliasing(mode: crate::AntiAliasing) -> GraphicsSettings {
    GraphicsSettings {
        anti_aliasing: Some(mode),
        ..GraphicsSettings::default()
    }
}

#[test]
fn capabilities_check_the_formats_the_scene_is_drawn_in() {
    assert_eq!(crate::graphics::SCENE_FORMATS, [HDR_FORMAT, DEPTH_FORMAT]);
}

#[test]
fn a_tilted_blocks_edge_over_the_sky_is_smoothed_by_msaa_and_fxaa_and_stepped_without() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = tilted_block();
    let in_between = |mode| {
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&with_anti_aliasing(mode));
        assert_eq!(renderer.graphics().anti_aliasing, mode);
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let block = pixels.chunks(4).filter(|pixel| pixel[0] > 245).count();
        assert!(block > 200, "{mode:?}: the block is drawn ({block} pixels)");
        pixels
            .chunks(4)
            .filter(|pixel| (10..=245).contains(&pixel[0]))
            .count()
    };
    use crate::AntiAliasing::{Fxaa, Msaa4, Off};
    assert_eq!(in_between(Off), 0, "hard steps");
    let msaa = in_between(Msaa4);
    assert!(msaa >= 30, "MSAA 4× blends edge pixels: {msaa}");
    let fxaa = in_between(Fxaa);
    assert!(fxaa >= 30, "FXAA blends edge pixels: {fxaa}");
}

#[test]
fn the_holes_a_texture_cuts_are_smoothed_by_msaa_and_stepped_without() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = tilted_block();
    list.items[0].material.base_color_texture = Some(Arc::new(HalfCut));
    list.items[0].material.alpha_cutoff = 0.5;
    let size = 64;
    let in_between = |mode| {
        let mut renderer = TextureRenderer::new(&device, &queue, size, size);
        renderer.set_graphics(&with_anti_aliasing(mode));
        assert_eq!(renderer.graphics().anti_aliasing, mode);
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        // The middle of the face, where only the texture's edge passes: the
        // block's own outline is smoothed by MSAA anyway.
        let middle = (27..37).flat_map(|y| (27..37).map(move |x| (x, y)));
        let values: Vec<u8> = middle
            .map(|(x, y): (u32, u32)| pixels[((y * size + x) * 4) as usize])
            .collect();
        assert!(
            values.iter().any(|&value| value > 245) && values.iter().any(|&value| value < 10),
            "{mode:?}: the edge passes through the middle: {values:?}"
        );
        values
            .iter()
            .filter(|value| (10..=245).contains(*value))
            .count()
    };
    use crate::AntiAliasing::{Msaa4, Off};
    assert_eq!(in_between(Off), 0, "hard steps");
    let msaa = in_between(Msaa4);
    assert!(msaa >= 4, "MSAA 4× blends the cut's edge: {msaa}");
}

#[test]
fn every_kind_of_item_draws_the_same_with_each_anti_aliasing() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (list, probes) = one_of_everything();
    let (width, height) = (96, 64);
    let draw = |mode| {
        let mut renderer = TextureRenderer::new(&device, &queue, width, height);
        renderer.set_graphics(&with_anti_aliasing(mode));
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        probes
            .iter()
            .map(|&(name, (x, y))| {
                let at = (y * width + x) as usize * 4;
                (name, [pixels[at], pixels[at + 1], pixels[at + 2]])
            })
            .collect::<Vec<_>>()
    };
    let off = draw(crate::AntiAliasing::Off);
    let pixel = |name| off.iter().find(|(probe, _)| *probe == name).unwrap().1;
    let [red, green, blue] = pixel("solid");
    assert!(
        red > green + 30 && red > blue + 30,
        "solid: {red} {green} {blue}"
    );
    let [red, _, blue] = pixel("see-through");
    let [sky_red, _, sky_blue] = pixel("sky");
    assert!(
        blue > red && blue.abs_diff(sky_blue) + red.abs_diff(sky_red) > 20,
        "see-through mixes blue over the sky"
    );
    let (left, right) = (pixel("cut-out left"), pixel("cut-out right"));
    let cut_difference: u32 = (0..3).map(|c| u32::from(left[c].abs_diff(right[c]))).sum();
    assert!(
        cut_difference > 30,
        "one half is cut away: {left:?} vs {right:?}"
    );
    let [red, green, _] = pixel("sprite");
    assert!(red > 200 && green < 60, "sprite: {red} {green}");
    let [red, _, blue] = pixel("terrain");
    assert!(red > blue + 20, "terrain's brown layer: {red} {blue}");

    for mode in [crate::AntiAliasing::Fxaa, crate::AntiAliasing::Msaa4] {
        for ((name, ours), (_, theirs)) in draw(mode).into_iter().zip(&off) {
            assert!(
                ours.iter().zip(theirs).all(|(a, b)| a.abs_diff(*b) <= 3),
                "{mode:?} draws the {name} like Off: {ours:?} vs {theirs:?}"
            );
        }
    }
}

#[test]
fn switching_anti_aliasing_frees_the_targets_it_no_longer_uses() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (list, _) = one_of_everything();
    let (width, height) = (96, 64);
    let fresh = |settings: &GraphicsSettings| {
        let mut renderer = TextureRenderer::new(&device, &queue, width, height);
        renderer.set_graphics(settings);
        renderer.render(&list).unwrap();
        renderer.read_pixels()
    };
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    for (step, mode) in crate::AntiAliasing::ALL.iter().cycle().take(20).enumerate() {
        let settings = with_anti_aliasing(*mode);
        renderer.set_graphics(&settings);
        renderer.render(&list).unwrap();
        // A GPU without 2× gets FXAA instead (see `effective`).
        let used = renderer.graphics().anti_aliasing;
        let multisampled = used.samples() > 1;
        assert_eq!(
            renderer.scene.anti_aliasing_targets_alive(),
            (
                usize::from(multisampled),
                usize::from(used == crate::AntiAliasing::Fxaa)
            ),
            "step {step}: {used:?}"
        );
        assert_eq!(renderer.scene.frame_targets.samples, used.samples());
        assert_eq!(renderer.scene.targets_alive(), 1, "step {step}");
        if step % 5 == 0 {
            assert_eq!(
                renderer.read_pixels(),
                fresh(&settings),
                "step {step}: {used:?}"
            );
        }
    }
    renderer.set_graphics(&with_anti_aliasing(crate::AntiAliasing::Off));
    renderer.render(&list).unwrap();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(renderer.scene.anti_aliasing_targets_alive(), (0, 0));
    assert_eq!(renderer.scene.targets_alive(), 1);
}
