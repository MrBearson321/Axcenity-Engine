//! Sun rays (a SunRaysEffect): a gap in a wall in front of the sun streams
//! into a beam away from it, a wall with no gap gets nothing, and they're
//! off without the effect, with the setting off or with the sun behind the
//! camera or below the horizon.

use super::*;
use crate::graphics::{AmbientOcclusion, AntiAliasing, Quality};
use crate::{GraphicsSettings, LensEffects, Shading, SunRays};

/// The frame size, square.
const SIZE: u32 = 128;

/// Toward the sun: ahead and up, so it shows above the middle.
const TOWARD_SUN: Vec3 = Vec3::new(0.0, 0.3, -1.0);
/// Toward the gap in the wall: between the sun and the middle.
const GAP: Vec3 = Vec3::new(0.0, 0.18, -1.0);
/// The wall's distance.
const WALL: f32 = 5.0;

fn settings(sun_rays: bool) -> GraphicsSettings {
    GraphicsSettings {
        anti_aliasing: Some(AntiAliasing::Off),
        ambient_occlusion: Some(AmbientOcclusion::Off),
        sun_rays: Some(sun_rays),
        ..GraphicsSettings::preset(Quality::High)
    }
}

/// A black unlit box, `size` studs, at `center`.
fn card(center: Vec3, size: Vec3) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, center),
        material: Material {
            base_color: [0.0; 3],
            shading: Shading::Unlit,
            ..Material::default()
        },
        ..big_block()
    }
}

/// A black wall filling the view, with a square gap `gap` studs wide where
/// [`GAP`] points (sky behind it), or none.
fn wall(gap: f32) -> Vec<DrawItem> {
    let big = 40.0;
    if gap <= 0.0 {
        return vec![card(Vec3::new(0.0, 0.0, -WALL), Vec3::new(big, big, 0.1))];
    }
    let at = GAP * (WALL / -GAP.z);
    let half = gap / 2.0;
    // Above, below, left and right of the gap.
    let side = (big - gap) / 2.0;
    vec![
        card(at + Vec3::Y * (half + big / 2.0), Vec3::new(big, big, 0.1)),
        card(at - Vec3::Y * (half + big / 2.0), Vec3::new(big, big, 0.1)),
        card(
            at - Vec3::X * (half + side / 2.0),
            Vec3::new(side, gap, 0.1),
        ),
        card(
            at + Vec3::X * (half + side / 2.0),
            Vec3::new(side, gap, 0.1),
        ),
    ]
}

fn scene(items: Vec<DrawItem>, toward_sun: Vec3, rays: Option<SunRays>) -> DrawList {
    let mut list = list_of(items);
    list.sun.direction = toward_sun;
    list.sun.color = [1.0; 3];
    list.sun.intensity = 1.0;
    list.lens = LensEffects {
        sun_rays: rays,
        ..LensEffects::default()
    };
    list
}

fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

/// The red of the pixel at `(x, y)`.
fn red(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * SIZE + x) * 4) as usize]
}

const RAYS: Option<SunRays> = Some(SunRays {
    intensity: 1.0,
    spread: 0.5,
});

#[test]
fn a_gap_in_front_of_the_sun_streams_into_a_beam() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&settings(true));
    let plain = draw(&mut renderer, &scene(wall(0.5), TOWARD_SUN, None));
    let rays = draw(&mut renderer, &scene(wall(0.5), TOWARD_SUN, RAYS));
    // The gap is above the middle, below the sun; below it, on the line
    // from the sun through the gap, the wall is black without rays and lit
    // by the beam with them.
    let middle = SIZE / 2;
    let below = middle + SIZE / 16;
    assert_eq!(red(&plain, middle, below), 0);
    let beam = red(&rays, middle, below);
    assert!(beam > 20, "the beam: {beam}");
    // Off to the side, where the way to the sun misses the gap, there's
    // much less of it.
    let side = red(&rays, middle - 24, below);
    assert!(side < beam / 2, "beside the beam: {side}, in it: {beam}");

    // A wall with no gap: no sky, so nothing is added.
    assert_eq!(
        draw(&mut renderer, &scene(wall(0.0), TOWARD_SUN, RAYS)),
        draw(&mut renderer, &scene(wall(0.0), TOWARD_SUN, None))
    );
}

#[test]
fn sun_rays_are_off_without_the_effect_the_setting_or_a_sun_in_view() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&settings(true));
    for toward_sun in [
        // Behind the camera.
        Vec3::new(0.0, 0.3, 1.0),
        // Below the horizon.
        Vec3::new(0.0, -0.3, -1.0),
    ] {
        assert_eq!(
            draw(&mut renderer, &scene(wall(0.5), toward_sun, RAYS)),
            draw(&mut renderer, &scene(wall(0.5), toward_sun, None)),
            "{toward_sun}"
        );
    }
    let plain = draw(&mut renderer, &scene(wall(0.5), TOWARD_SUN, None));
    renderer.set_graphics(&settings(false));
    assert_eq!(
        draw(&mut renderer, &scene(wall(0.5), TOWARD_SUN, RAYS)),
        plain
    );
}

/// What the pass is told: rays toward the sun while it's in view, fading
/// past the screen's edge, and none behind or below.
#[test]
fn the_pass_is_told_where_the_sun_is() {
    let rays = |toward_sun: Vec3, quality: Quality, effect: Option<SunRays>| {
        // The preset's own choice: on at High, off at Low.
        let graphics = GraphicsSettings::preset(quality).effective(&Default::default());
        let list = scene(Vec::new(), toward_sun, effect);
        super::super::sun_rays::uniform(&list, &graphics, 1.0).map(|uniform| uniform.rays)
    };
    let half = Some(SunRays {
        intensity: 0.5,
        spread: 0.5,
    });
    let ahead = rays(TOWARD_SUN, Quality::High, half).unwrap();
    assert!((ahead[0] - 0.75).abs() < 1e-4, "{ahead:?}");
    assert!((ahead[1] - 0.575).abs() < 1e-4, "{ahead:?}");
    assert!(
        (ahead[2] - 0.5).abs() < 1e-4,
        "across the middle: {ahead:?}"
    );
    assert!(ahead[3] < 0.5, "above the middle: {ahead:?}");
    let edge = rays(Vec3::new(0.9, 0.3, -1.0), Quality::High, half).unwrap();
    assert!(
        edge[0] > 0.0 && edge[0] < 0.75,
        "fading past the edge: {edge:?}"
    );
    for off in [
        rays(Vec3::new(0.0, 0.3, 1.0), Quality::High, half),
        rays(Vec3::new(0.0, -0.3, -1.0), Quality::High, half),
        rays(Vec3::new(4.0, 0.3, -0.1), Quality::High, half),
        rays(TOWARD_SUN, Quality::High, None),
        rays(TOWARD_SUN, Quality::Low, half),
    ] {
        assert_eq!(off, None);
    }
}

#[test]
#[ignore]
fn picture_for_a_look() {
    let Some((device, queue)) = test_device() else {
        return;
    };
    const W: u32 = 480;
    const H: u32 = 270;
    let mut renderer = TextureRenderer::new(&device, &queue, W, H);
    renderer.set_graphics(&settings(true));
    let mut items = Vec::new();
    let mut seed = 7u32;
    let mut next = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    let leaf = |center, size| DrawItem {
        material: Material {
            base_color: [0.1, 0.25, 0.08],
            ..Material::default()
        },
        ..card(center, size)
    };
    for _ in 0..260 {
        let x = next() * 30.0 - 15.0;
        let y = next() * 10.0 + 2.0;
        let z = -8.0 - next() * 6.0;
        items.push(leaf(Vec3::new(x, y, z), Vec3::new(1.6, 1.2, 0.2)));
    }
    items.push(leaf(
        Vec3::new(0.0, -2.0, -10.0),
        Vec3::new(60.0, 0.2, 40.0),
    ));
    let mut list = list_of(items);
    list.sky.top_color = [0.35, 0.55, 0.9];
    list.sky.horizon_color = [0.8, 0.85, 0.95];
    list.ambient.intensity = 0.4;
    list.sun.direction = Vec3::new(0.15, 0.6, -1.0);
    list.sun.color = [1.0, 0.95, 0.85];
    list.sun.intensity = 1.0;
    list.camera.look_at = Vec3::new(0.0, 0.35, -1.0);
    let out = std::env::var("SUN_RAYS_PICTURE").unwrap();
    for (name, rays) in [
        ("off", None),
        (
            "on",
            Some(SunRays {
                intensity: 0.25,
                spread: 0.5,
            }),
        ),
        (
            "strong",
            Some(SunRays {
                intensity: 0.6,
                spread: 0.8,
            }),
        ),
    ] {
        list.lens = LensEffects {
            sun_rays: rays,
            ..LensEffects::default()
        };
        renderer.render(&list).unwrap();
        std::fs::write(format!("{out}/{name}.rgba"), renderer.read_pixels()).unwrap();
    }
}
