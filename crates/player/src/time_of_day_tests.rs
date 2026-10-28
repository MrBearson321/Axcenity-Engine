//! The time of day and an Atmosphere, drawn: from noon to dusk the sun and
//! sky redden and shadows lengthen, and at night the sky darkens.

use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::{DrawList, TextureRenderer};

use super::tests::test_gpu;
use super::*;

const SIZE: (u32, u32) = (240, 135);

/// A grey floor with a 10-stud pole in the middle, under an Atmosphere, 40
/// degrees north.
fn pole_world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    for (name, size, position) in [
        (
            "Floor",
            Vector3::new(400.0, 1.0, 400.0),
            Vector3::new(0.0, -0.5, 0.0),
        ),
        (
            "Pole",
            Vector3::new(1.0, 10.0, 1.0),
            Vector3::new(0.0, 5.0, 0.0),
        ),
    ] {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, name).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        world
            .set_property(part, "Size", Value::Vector3(size))
            .unwrap();
        world
            .set_property(part, "Position", Value::Vector3(position))
            .unwrap();
        world
            .set_property(part, "Anchored", Value::Bool(true))
            .unwrap();
    }
    let lighting = world.lighting();
    world
        .set_property(lighting, "GeographicLatitude", Value::Number(40.0))
        .unwrap();
    let atmosphere = world.create(ClassName::Atmosphere).unwrap();
    world.set_parent(atmosphere, Some(lighting)).unwrap();
    world
}

fn set_clock(world: &mut World, hours: f64) {
    let lighting = world.lighting();
    world
        .set_property(lighting, "ClockTime", Value::Number(hours))
        .unwrap();
}

/// `world` from `camera`: its draw list and the frame drawn.
fn frame(renderer: &mut TextureRenderer, world: &World, camera: &FlyCamera) -> (DrawList, Vec<u8>) {
    // An empty project: these worlds read no files.
    let folder = std::env::temp_dir().join(format!("axcenity-time-of-day-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let assets = Assets::new(&folder);
    let list = axcenity_scene::build_draw_list(world, &assets, camera.to_render_camera());
    renderer.render(&list).unwrap();
    (list, renderer.read_pixels())
}

/// How many pixels are darker than 90% of the floor's usual (its median),
/// seen from straight above: the pole's shadow (and its top). A low sun's
/// shadow is faint, since the sky lights the floor about as much as it.
fn shadowed_pixels(pixels: &[u8]) -> usize {
    let mut brightness: Vec<u32> = pixels
        .chunks(4)
        .map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]))
        .collect();
    let mut sorted = brightness.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    brightness.retain(|&b| b * 10 < median * 9);
    brightness.len()
}

/// The average color of the frame's top quarter: sky, low over the horizon.
fn sky_color(pixels: &[u8]) -> [f32; 3] {
    let (width, height) = SIZE;
    let rows = (height / 4) as usize;
    let mut sum = [0.0; 3];
    for pixel in pixels.chunks(4).take(width as usize * rows) {
        for (sum, &c) in sum.iter_mut().zip(pixel) {
            *sum += f32::from(c);
        }
    }
    sum.map(|s| s / (width as usize * rows) as f32)
}

fn redness(color: [f32; 3]) -> f32 {
    color[0] / color[2].max(1.0)
}

#[test]
fn from_noon_to_dusk_the_sun_and_sky_redden_and_shadows_lengthen() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (width, height) = SIZE;
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut world = pole_world();
    // Straight down on the pole, and low toward the setting sun (west).
    let above = FlyCamera::looking_at(Vec3::new(0.0, 70.0, 0.5), Vec3::ZERO);
    let west = FlyCamera::looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::new(-100.0, 10.0, 0.0));

    set_clock(&mut world, 12.0);
    let (noon_list, noon_above) = frame(&mut renderer, &world, &above);
    let (_, noon_west) = frame(&mut renderer, &world, &west);
    set_clock(&mut world, 17.5);
    let (dusk_list, dusk_above) = frame(&mut renderer, &world, &above);
    let (_, dusk_west) = frame(&mut renderer, &world, &west);
    let folder = std::env::temp_dir();
    for (name, pixels) in [
        ("noon-above", &noon_above),
        ("noon-west", &noon_west),
        ("dusk-above", &dusk_above),
        ("dusk-west", &dusk_west),
    ] {
        tests::write_png(&folder.join(format!("axcenity-{name}.png")), pixels, SIZE);
    }

    // The sun reddens as it sinks...
    let sun_redness = |list: &DrawList| list.sun.color[0] / list.sun.color[2];
    assert!(
        sun_redness(&dusk_list) > 2.0 * sun_redness(&noon_list),
        "{:?} vs {:?}",
        dusk_list.sun,
        noon_list.sun
    );
    assert!(dusk_list.sun.direction.y < noon_list.sun.direction.y);
    // ...and so does the sky around it.
    let (noon_sky, dusk_sky) = (sky_color(&noon_west), sky_color(&dusk_west));
    assert!(
        redness(dusk_sky) > 1.5 * redness(noon_sky),
        "dusk {dusk_sky:?}, noon {noon_sky:?}"
    );
    // The pole's shadow stretches across the floor.
    let (noon_shadow, dusk_shadow) = (shadowed_pixels(&noon_above), shadowed_pixels(&dusk_above));
    assert!(noon_shadow > 20, "noon's shadow shows: {noon_shadow}");
    assert!(
        dusk_shadow > 2 * noon_shadow,
        "dusk {dusk_shadow} vs noon {noon_shadow} pixels"
    );
}

#[test]
fn at_midnight_the_atmospheres_sky_is_dark_and_the_moon_lights_the_world() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (width, height) = SIZE;
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut world = pole_world();
    let up = FlyCamera::looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 30.0, -100.0));
    set_clock(&mut world, 12.0);
    let (_, noon) = frame(&mut renderer, &world, &up);
    set_clock(&mut world, 0.0);
    let (night_list, night) = frame(&mut renderer, &world, &up);
    tests::write_png(
        &std::env::temp_dir().join("axcenity-midnight.png"),
        &night,
        SIZE,
    );
    let brightness = |pixels: &[u8]| {
        let sky = sky_color(pixels);
        sky[0] + sky[1] + sky[2]
    };
    assert!(
        brightness(&night) < 0.25 * brightness(&noon),
        "{} vs {}",
        brightness(&night),
        brightness(&noon)
    );
    // The light from far away is the moon, high in the sky.
    assert!(night_list.sun.direction.y > 0.5 && night_list.sun.intensity > 0.0);
    assert!(night_list.sky.stars > 0.99);
}
