//! Upscaling (roadmap G9): below a render scale of 1 the finished picture is
//! upscaled to the output with FSR 1-style EASU and RCAS (`upscale.wgsl`),
//! which keeps edges closer to a full-size frame than stretching does.

use super::*;

fn scaled(render_scale: f32, anti_aliasing: crate::AntiAliasing) -> GraphicsSettings {
    GraphicsSettings {
        render_scale: Some(render_scale),
        anti_aliasing: Some(anti_aliasing),
        ..GraphicsSettings::default()
    }
}

/// The red channel of each pixel.
fn reds(pixels: &[u8]) -> Vec<u8> {
    pixels.chunks(4).map(|pixel| pixel[0]).collect()
}

/// `small` (`width` × `height` reds) stretched to twice its size with
/// bilinear filtering, sampling where the GPU's linear sampler would.
fn stretched(small: &[u8], (width, height): (u32, u32)) -> Vec<u8> {
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, width as i64 - 1);
        let y = y.clamp(0, height as i64 - 1);
        f32::from(small[(y * width as i64 + x) as usize])
    };
    let mut out = Vec::new();
    for y in 0..height * 2 {
        for x in 0..width * 2 {
            let sx = (x as f32 + 0.5) / 2.0 - 0.5;
            let sy = (y as f32 + 0.5) / 2.0 - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
            let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
            out.push((top * (1.0 - fy) + bottom * fy).round() as u8);
        }
    }
    out
}

fn difference(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum()
}

#[test]
fn half_scale_is_upscaled_closer_to_full_size_than_stretching() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = tilted_block();
    let (width, height) = (160, 96);
    let draw = |size: (u32, u32), settings: &GraphicsSettings| {
        let mut renderer = TextureRenderer::new(&device, &queue, size.0, size.1);
        renderer.set_graphics(settings);
        renderer.render(&list).unwrap();
        reds(&renderer.read_pixels())
    };
    let off = crate::AntiAliasing::Off;
    let full = draw((width, height), &scaled(1.0, off));
    let small = draw((width / 2, height / 2), &scaled(1.0, off));
    let upscaled = draw((width, height), &scaled(0.5, off));
    let stretched = stretched(&small, (width / 2, height / 2));
    let (upscaled_off, stretched_off) =
        (difference(&upscaled, &full), difference(&stretched, &full));
    assert!(
        upscaled_off < stretched_off,
        "upscaled is {upscaled_off} from full size, stretched {stretched_off}"
    );
    // The block is still where it was: its bright pixels nearly all match.
    let bright = |reds: &[u8]| reds.iter().filter(|&&red| red > 200).count();
    let (a, b) = (bright(&full), bright(&upscaled));
    assert!(
        a.abs_diff(b) * 20 < a,
        "{a} bright pixels at full size, {b} upscaled"
    );
}

#[test]
fn only_a_scale_below_one_makes_upscaling_targets_and_they_are_freed() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = tilted_block();
    let mut renderer = TextureRenderer::new(&device, &queue, 96, 64);
    let alive =
        |renderer: &TextureRenderer| Arc::strong_count(&renderer.scene.targets_alive.upscale) - 1;
    for (scale, anti_aliasing, upscaled) in [
        (1.0, crate::AntiAliasing::Fxaa, false),
        (0.5, crate::AntiAliasing::Fxaa, true),
        (0.75, crate::AntiAliasing::Off, true),
        (0.67, crate::AntiAliasing::Msaa4, true),
        (1.5, crate::AntiAliasing::Fxaa, false),
        (1.0, crate::AntiAliasing::Off, false),
    ] {
        renderer.set_graphics(&scaled(scale, anti_aliasing));
        renderer.render(&list).unwrap();
        let targets = &renderer.scene.frame_targets;
        assert_eq!(targets.upscale.is_some(), upscaled, "at {scale}");
        assert_eq!(alive(&renderer), usize::from(upscaled), "at {scale}");
        // With FXAA, it smooths the picture at the size it's drawn at.
        assert_eq!(
            renderer.scene.anti_aliasing_targets_alive().1,
            usize::from(anti_aliasing == crate::AntiAliasing::Fxaa)
        );
        let pixels = renderer.read_pixels();
        let block = pixels.chunks(4).filter(|pixel| pixel[0] > 245).count();
        assert!(
            block > 200,
            "at {scale} with {anti_aliasing:?}: the block is drawn ({block} pixels)"
        );
    }
}

#[test]
fn flat_colors_come_through_upscaling_unchanged() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // The middle of a big lit block, away from its edges.
    let list = list_of(vec![big_block()]);
    let draw = |scale| {
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&scaled(scale, crate::AntiAliasing::Off));
        renderer.render(&list).unwrap();
        renderer.read_pixels()
    };
    let (full, upscaled) = (draw(1.0), draw(0.5));
    let middle = |pixels: &[u8]| -> Vec<u8> {
        (28..36)
            .flat_map(|y| (28..36).map(move |x| (y * 64 + x) * 4))
            .flat_map(|at| pixels[at..at + 3].to_vec())
            .collect()
    };
    for (a, b) in middle(&full).iter().zip(middle(&upscaled)) {
        assert!(a.abs_diff(b) <= 2, "{a} vs {b}");
    }
}

/// GPU time of upscaling at 0.67 and 0.5 scale, for `docs/rendering/post.md`:
/// `cargo test -p axcenity-render --release --lib -- --ignored
/// bench_upscaling --nocapture` on a device with timestamps.
///
/// For each output size it draws a field of blocks at full size, then at
/// each scale twice: upscaled to the output, and at the smaller size
/// itself. The upscaled frame less the small one is what EASU and RCAS
/// cost; the full-size frame less the upscaled one is what the scale saves.
#[test]
#[ignore]
fn bench_upscaling() {
    let instance = crate::adapters::instance();
    let Ok(adapter) = pollster::block_on(crate::adapters::adapter(&instance, None)) else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        eprintln!("skipped: the adapter has no timestamps");
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let mut items = vec![big_block()];
    for x in -12..12 {
        for z in 0..24 {
            items.push(item(
                Shape::Block,
                Vec3::new(
                    x as f32 * 1.5,
                    -2.0 + (x * z % 3) as f32 * 0.5,
                    -4.0 - z as f32 * 1.5,
                ),
                1.0,
            ));
        }
    }
    let mut list = list_of(items);
    list.shadows.enabled = true;
    list.post.enabled = true;
    list.ambient.intensity = 0.3;
    // Median GPU frame and post-processing (which includes the upscale).
    let measure = |size: (u32, u32), scale: f32| {
        let mut renderer = TextureRenderer::new(&device, &queue, size.0, size.1);
        renderer.set_graphics(&scaled(scale, crate::AntiAliasing::Fxaa));
        let (mut frames, mut posts) = (Vec::new(), Vec::new());
        for frame in 0..230 {
            renderer.render(&list).unwrap();
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let stats = renderer.stats();
            if frame >= 30
                && let (Some(gpu), Some(post)) =
                    (stats.gpu_time, stats.pass_time(FramePass::PostProcessing))
            {
                frames.push(gpu.as_secs_f64() * 1000.0);
                posts.push(post.as_secs_f64() * 1000.0);
            }
        }
        let median = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        };
        (median(&mut frames), median(&mut posts))
    };
    println!("{}", adapter.get_info().name);
    println!(
        "output     scale  frame ms  post ms  | same size native: frame  post | upscale cost  saved vs 1.0"
    );
    for size in [(1920, 1080), (2560, 1440), (3840, 2160)] {
        let (full, full_post) = measure(size, 1.0);
        println!(
            "{:>4}x{:<4}  1.00  {full:>8.3}  {full_post:>7.3}  |",
            size.0, size.1
        );
        for scale in [0.67_f32, 0.5] {
            let (frame, post) = measure(size, scale);
            let small = (
                (size.0 as f32 * scale).round() as u32,
                (size.1 as f32 * scale).round() as u32,
            );
            let (native, native_post) = measure(small, 1.0);
            println!(
                "{:>4}x{:<4}  {scale:.2}  {frame:>8.3}  {post:>7.3}  | {:>4}x{:<4} {native:>6.3} {native_post:>6.3} | {:>11.3}  {:>11.3}",
                size.0,
                size.1,
                small.0,
                small.1,
                frame - native,
                full - frame,
            );
        }
    }
}
