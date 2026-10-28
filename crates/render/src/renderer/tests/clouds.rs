//! Clouds: drawn in the sky, shadowing the ground beneath, carried by the
//! wind, and costing nothing without them.

use super::*;
use crate::{CloudQuality, Clouds, Quality};

/// The frame size these tests draw at.
const SIZE: u32 = 128;

/// A place where the clouds' noise has edges (cloud and clear sky) close
/// together, at the ground.
const CLOUDY: Vec3 = Vec3::new(-1750.0, 0.0, 1150.0);

/// Half the sky cloudy, drifting `wind` studs a second.
fn cloudy(wind: Vec3) -> Clouds {
    Clouds {
        cover: 0.5,
        wind,
        ..Clouds::default()
    }
}

/// A wide white floor under a sun straight overhead, seen from `height`
/// studs up, looking down and a little ahead. The sky is the default
/// gradient.
fn floor_from(height: f32) -> DrawList {
    let mut floor = item(Shape::Block, Vec3::new(0.0, -0.5, 0.0), 1.0);
    floor.transform = Mat4::from_scale_rotation_translation(
        Vec3::new(20_000.0, 1.0, 20_000.0),
        glam::Quat::IDENTITY,
        Vec3::new(0.0, -0.5, 0.0),
    );
    let mut list = list_of(vec![floor]);
    list.camera = Camera {
        position: CLOUDY + Vec3::new(0.0, height, 0.0),
        look_at: CLOUDY + Vec3::new(0.0, 0.0, -0.2 * height),
        fov_y_degrees: 70.0,
    };
    list.sun.intensity = 3.0;
    list
}

/// The sky straight above, seen from the ground, with the default sky's
/// colors.
fn sky_overhead() -> DrawList {
    let mut list = list_of(Vec::new());
    list.sky = Sky::default();
    list.camera = Camera {
        position: CLOUDY,
        look_at: CLOUDY + Vec3::new(0.0, 100.0, -20.0),
        fov_y_degrees: 70.0,
    };
    list.sun.intensity = 3.0;
    list.sun.direction = Vec3::new(0.3, 1.0, 0.2).normalize();
    list
}

/// How bright each pixel of the middle row is (the sum of its channels).
fn middle_row(pixels: &[u8]) -> Vec<f32> {
    let row = SIZE as usize / 2;
    pixels[row * SIZE as usize * 4..][..SIZE as usize * 4]
        .chunks(4)
        .map(|pixel| pixel[..3].iter().map(|&c| f32::from(c)).sum())
        .collect()
}

/// The number of pixels `after` is `before` moved along by: the shift that
/// lines the two up best, from `-most` to `most`.
fn best_shift(before: &[f32], after: &[f32], most: i32) -> i32 {
    let difference = |shift: i32| {
        let pairs = (0..before.len() as i32)
            .filter_map(|x| {
                let from = usize::try_from(x - shift).ok()?;
                Some((after[x as usize], *before.get(from)?))
            })
            .collect::<Vec<_>>();
        pairs.iter().map(|(a, b)| (a - b).abs()).sum::<f32>() / pairs.len() as f32
    };
    (-most..=most)
        .min_by(|&a, &b| difference(a).total_cmp(&difference(b)))
        .unwrap()
}

/// How many pixels along the middle row `studs` along X are at the
/// floor, in `list`'s view.
fn pixels_per_stud(list: &DrawList, at: Vec3) -> f32 {
    let screen_x = |point: Vec3| {
        let clip = view_projection(list, 1.0) * point.extend(1.0);
        (clip.x / clip.w * 0.5 + 0.5) * SIZE as f32
    };
    screen_x(at + Vec3::X) - screen_x(at)
}

fn spread(row: &[f32]) -> f32 {
    let most = row.iter().copied().fold(f32::MIN, f32::max);
    let least = row.iter().copied().fold(f32::MAX, f32::min);
    most - least
}

#[test]
fn clouds_and_their_shadows_move_with_the_wind() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let wind = Vec3::new(20.0, 0.0, 0.0);
    for (what, mut list, at) in [
        (
            "the clouds",
            sky_overhead(),
            CLOUDY + Vec3::new(0.0, Clouds::BOTTOM, -160.0),
        ),
        (
            "their shadows",
            floor_from(700.0),
            CLOUDY + Vec3::new(0.0, 0.0, -140.0),
        ),
    ] {
        let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
        list.clouds = cloudy(wind);
        list.time = 0.0;
        renderer.render(&list).unwrap();
        let before = middle_row(&renderer.read_pixels());
        assert!(spread(&before) > 40.0, "{what} show: {before:?}");
        // Five seconds on, the wind has carried them 100 studs along X.
        list.time = 5.0;
        renderer.render(&list).unwrap();
        let after = middle_row(&renderer.read_pixels());
        let expected = (100.0 * pixels_per_stud(&list, at)).round() as i32;
        assert!(expected >= 4, "{what}: {expected} pixels");
        let shift = best_shift(&before, &after, 2 * expected);
        assert!(
            (shift - expected).abs() <= 1,
            "{what} moved {shift} pixels, not {expected}"
        );

        // Without wind, they stay where they are.
        list.clouds.wind = Vec3::ZERO;
        list.time = 10.0;
        renderer.render(&list).unwrap();
        let still = renderer.read_pixels();
        list.time = 15.0;
        renderer.render(&list).unwrap();
        assert!(renderer.read_pixels() == still, "{what} stay still");
    }
}

#[test]
fn cover_zero_is_a_clear_sky_and_no_clouds_cost_nothing() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    for mut list in [sky_overhead(), floor_from(700.0)] {
        let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
        renderer.render(&list).unwrap();
        let clear = renderer.read_pixels();
        let clear_stats = renderer.stats();

        list.clouds = Clouds {
            cover: 0.0,
            ..cloudy(Vec3::X)
        };
        renderer.render(&list).unwrap();
        assert!(renderer.read_pixels() == clear, "cover 0 is a clear sky");

        list.clouds = cloudy(Vec3::X);
        renderer.render(&list).unwrap();
        assert!(renderer.read_pixels() != clear, "clouds show");
        let cloudy_stats = renderer.stats();
        assert_eq!(cloudy_stats.draw_calls, clear_stats.draw_calls);
        assert_eq!(cloudy_stats.target_bytes, clear_stats.target_bytes);

        list.clouds = Clouds::default();
        renderer.render(&list).unwrap();
        assert!(renderer.read_pixels() == clear, "removed, they're gone");
    }
}

#[test]
fn every_cloud_quality_draws_clouds_and_casts_the_same_shadows() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.render(&sky_overhead()).unwrap();
    let clear_sky = renderer.read_pixels();
    let mut shadows = Vec::new();
    for quality in CloudQuality::ALL {
        renderer.set_graphics(&GraphicsSettings {
            cloud_quality: Some(quality),
            ..GraphicsSettings::preset(Quality::High)
        });
        let mut sky = sky_overhead();
        sky.clouds = cloudy(Vec3::ZERO);
        renderer.render(&sky).unwrap();
        let clouded = renderer.read_pixels();
        let changed = clouded
            .chunks(4)
            .zip(clear_sky.chunks(4))
            .filter(|(a, b)| {
                a[..3]
                    .iter()
                    .zip(b.iter())
                    .any(|(x, y)| x.abs_diff(*y) > 20)
            })
            .count();
        assert!(
            changed > clouded.len() / 4 / 10,
            "{quality:?}: clouds over {changed} pixels"
        );
        let mut floor = floor_from(700.0);
        floor.clouds = cloudy(Vec3::ZERO);
        renderer.render(&floor).unwrap();
        shadows.push(renderer.read_pixels());
    }
    assert!(
        shadows.windows(2).all(|pair| pair[0] == pair[1]),
        "every quality shades the ground the same"
    );
}
