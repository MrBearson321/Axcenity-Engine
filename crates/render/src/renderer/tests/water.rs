//! Water: the pool floor seen through it, bent by its waves and tinted by
//! its depth; the sky and nearby parts reflected; drawing alongside fog,
//! weather, clouds and wind; and scaling down on Low.

use super::super::water::WaterMode;
use super::*;
use crate::graphics::{AntiAliasing, Quality};
use crate::{Atmosphere, Clouds, Fog, GraphicsSettings, Water, Weather, water_surface};
use glam::Vec2;

/// The frame size, square.
const SIZE: u32 = 128;

/// A 40-stud square of water whose surface is at y = 0.
fn water(wave_height: f32) -> DrawItem {
    DrawItem {
        shape: Shape::Mesh(water_surface()),
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 1.0, 40.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
        material: Material {
            base_color: [1.0; 3],
            water: Some(Water {
                deep_color: [0.0, 0.1, 0.3],
                clarity: 2.0,
                wave_height,
                wave_length: 6.0,
                foam: 0.0,
                refraction: 0.4,
                ..Water::default()
            }),
            ..Material::default()
        },
        casts_shadow: false,
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    }
}

fn block(size: Vec3, position: Vec3, color: [f32; 3]) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        material: Material {
            base_color: color,
            ..Material::default()
        },
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    }
}

/// Where the shallow floor's top is, and the deep floor's.
const SHALLOW: f32 = -0.6;
const DEEP: f32 = -8.0;

/// A pool under a high sun: a white floor 0.6 studs down on its left half
/// (x < 0), striped black every 2 studs along X, and one 8 studs down on
/// its right half, with water over both (or not), seen from above its
/// near edge.
fn pool(with_water: Option<f32>) -> DrawList {
    let mut items = vec![
        block(
            Vec3::new(20.0, 1.0, 40.0),
            Vec3::new(-10.0, SHALLOW - 0.5, 0.0),
            [1.0; 3],
        ),
        block(
            Vec3::new(20.0, 1.0, 40.0),
            Vec3::new(10.0, DEEP - 0.5, 0.0),
            [1.0; 3],
        ),
    ];
    for stripe in -10..0 {
        items.push(block(
            Vec3::new(0.5, 0.02, 40.0),
            Vec3::new(stripe as f32 * 2.0, SHALLOW, 0.0),
            [0.0; 3],
        ));
    }
    items.extend(with_water.map(water));
    let mut list = list_of(items);
    list.camera = Camera {
        position: Vec3::new(0.0, 8.0, 16.0),
        look_at: Vec3::new(0.0, -2.0, -2.0),
        fov_y_degrees: 60.0,
    };
    list.sun.direction = Vec3::new(0.3, 1.0, 0.2).normalize();
    list.sun.intensity = 2.0;
    list.ambient.intensity = 0.4;
    list.shadows.enabled = false;
    list
}

/// A shallow floor point between two stripes, a stripe, and a deep floor point.
const BETWEEN: Vec3 = Vec3::new(-5.0, SHALLOW, 2.0);
const STRIPE: Vec3 = Vec3::new(-6.0, SHALLOW + 0.01, 2.0);
const FAR_FLOOR: Vec3 = Vec3::new(8.0, DEEP, 2.0);

fn graphics(quality: Quality) -> GraphicsSettings {
    GraphicsSettings {
        anti_aliasing: Some(AntiAliasing::Off),
        ..GraphicsSettings::preset(quality)
    }
}

/// Where `point` is drawn in a `SIZE`-square frame of `list`, in pixels.
fn on_screen(list: &DrawList, point: Vec3) -> Vec2 {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * SIZE as f32
}

fn pixel(pixels: &[u8], at: Vec2) -> [u8; 4] {
    let (x, y) = (at.x as usize, at.y as usize);
    pixels[(y * SIZE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// Draws `list` and gives back the pixels of `points` in it.
fn pixels_of<const N: usize>(
    renderer: &mut TextureRenderer,
    list: &DrawList,
    points: [Vec3; N],
) -> [[u8; 4]; N] {
    renderer.render(list).unwrap();
    let pixels = renderer.read_pixels();
    points.map(|point| pixel(&pixels, on_screen(list, point)))
}

fn brightness(pixel: [u8; 4]) -> i32 {
    pixel[..3].iter().map(|&c| i32::from(c)).sum()
}

/// How many pixels inside the rows `rows` differ by more than 8 in some
/// channel between `a` and `b`.
fn changed_pixels(a: &[u8], b: &[u8], rows: std::ops::Range<usize>) -> usize {
    let row = SIZE as usize * 4;
    a[rows.start * row..rows.end * row]
        .chunks(4)
        .zip(b[rows.start * row..rows.end * row].chunks(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
        .count()
}

#[test]
fn a_pool_floor_shows_through_the_water_bent_by_its_waves_and_deeper_water_is_tinted() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&graphics(Quality::Medium));

    let [between_dry, stripe_dry, deep_dry] =
        pixels_of(&mut renderer, &pool(None), [BETWEEN, STRIPE, FAR_FLOOR]);
    let [between, stripe, deep] = pixels_of(
        &mut renderer,
        &pool(Some(0.0)),
        [BETWEEN, STRIPE, FAR_FLOOR],
    );
    assert!(
        brightness(deep_dry) > 300 && brightness(between_dry) > 300,
        "the dry floor is lit white: {between_dry:?}, {deep_dry:?}"
    );
    // Through still, shallow water the floor and its stripes still show.
    assert!(
        brightness(between) > brightness(between_dry) / 2,
        "{between_dry:?} -> {between:?}"
    );
    assert!(
        brightness(between) > brightness(stripe) + 150,
        "the stripes show through: {between:?} against {stripe:?} (dry {stripe_dry:?})"
    );
    // Eight studs down, the floor is mostly lost to the water's deep blue.
    assert!(
        brightness(deep) < brightness(between) / 3,
        "deep water is darker: {between:?} shallow, {deep:?} deep"
    );
    assert!(deep[2] > deep[0] + 10, "and tinted blue: {deep:?}");

    // Still water stays still; waves move what's seen through them.
    // The rows the shallow floor shows in (on the left), all water.
    let shallow_rows = {
        let list = pool(None);
        let top = on_screen(&list, Vec3::new(-6.0, SHALLOW, -6.0)).y as usize;
        let bottom = on_screen(&list, Vec3::new(-6.0, SHALLOW, 8.0)).y as usize;
        top..bottom.min(SIZE as usize)
    };
    let frame_at = |renderer: &mut TextureRenderer, wave_height: f32, time: f32| {
        let mut list = pool(Some(wave_height));
        list.time = time;
        renderer.render(&list).unwrap();
        renderer.read_pixels()
    };
    let still = frame_at(&mut renderer, 0.0, 0.0);
    let still_later = frame_at(&mut renderer, 0.0, 1.3);
    assert!(still == still_later, "still water doesn't change");
    let waves = frame_at(&mut renderer, 0.3, 0.0);
    let waves_later = frame_at(&mut renderer, 0.3, 1.3);
    let area = shallow_rows.len() * SIZE as usize;
    let bent = changed_pixels(&still, &waves, shallow_rows.clone());
    let moving = changed_pixels(&waves, &waves_later, shallow_rows);
    assert!(
        bent * 10 > area && moving * 10 > area,
        "waves bend the floor seen through the water: {bent} and {moving} of {area} pixels"
    );
}

/// Deep water with no floor (nothing but the sky under it), seen low
/// across it, and a red glowing block standing out of it 10 studs away.
fn open_water(sky: bool, red_block: bool) -> DrawList {
    let mut items = vec![water(0.0)];
    if red_block {
        let mut red = block(
            Vec3::new(6.0, 4.0, 1.0),
            Vec3::new(0.0, 2.0, -10.0),
            [0.0; 3],
        );
        red.material.emissive = [1.0, 0.0, 0.0];
        items.push(red);
    }
    let mut list = list_of(items);
    list.camera = Camera {
        position: Vec3::new(0.0, 2.0, 12.0),
        look_at: Vec3::new(0.0, 1.0, -10.0),
        fov_y_degrees: 60.0,
    };
    list.sun.direction = Vec3::new(0.0, 1.0, 1.0).normalize();
    list.sun.intensity = 1.0;
    list.ambient.intensity = 0.2;
    list.shadows.enabled = false;
    if sky {
        list.sky.top_color = [0.2, 0.45, 1.0];
        list.sky.horizon_color = [0.6, 0.75, 1.0];
        list.sky.ground_color = [0.6, 0.75, 1.0];
        list.sky.environment_intensity = 1.0;
    }
    list
}

/// Where the water mirrors the red block's middle: below it, as far under
/// the surface as the middle is above it.
const MIRRORED_BLOCK: Vec3 = Vec3::new(0.0, -2.0, -10.0);

#[test]
fn water_reflects_the_sky_and_at_high_the_parts_nearby() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&graphics(Quality::Medium));
    let surface = Vec3::new(4.0, 0.0, -4.0);
    let [dark] = pixels_of(&mut renderer, &open_water(false, false), [surface]);
    let [blue] = pixels_of(&mut renderer, &open_water(true, false), [surface]);
    assert!(
        brightness(blue) > brightness(dark) + 60 && blue[2] > blue[0] + 20,
        "the water reflects a blue sky: {dark:?} -> {blue:?}"
    );

    // Medium reflects only the sky (black here), High what's on screen too.
    let [medium] = pixels_of(&mut renderer, &open_water(false, true), [MIRRORED_BLOCK]);
    renderer.set_graphics(&GraphicsSettings::preset(Quality::High));
    let [high] = pixels_of(&mut renderer, &open_water(false, true), [MIRRORED_BLOCK]);
    assert!(
        high[0] > medium[0] + 40 && high[0] > high[1] + 40,
        "at High the water reflects the red block: {medium:?} -> {high:?}"
    );
    // Away from the block's reflection the water stays dark.
    let [beside] = pixels_of(
        &mut renderer,
        &open_water(false, true),
        [Vec3::new(14.0, -2.0, -10.0)],
    );
    assert!(beside[0] < beside[2] + 10, "{beside:?}");
}

#[test]
fn water_draws_with_fog_weather_clouds_and_wind_and_isnt_snowed_on() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&GraphicsSettings::preset(Quality::Ultra));
    let points = [BETWEEN, FAR_FLOOR];
    let [_, clear] = pixels_of(&mut renderer, &pool(Some(0.0)), points);

    // Snow whitens surfaces facing up, but not the water.
    let mut snowy = pool(Some(0.0));
    snowy.weather = Weather {
        wetness: 1.0,
        snow: 1.0,
    };
    let [_, snowed] = pixels_of(&mut renderer, &snowy, points);
    assert!(
        clear.iter().zip(snowed).all(|(a, b)| a.abs_diff(b) <= 3),
        "deep water isn't whitened by snow: {clear:?} -> {snowed:?}"
    );

    // White fog fades the water into it.
    let mut foggy = pool(Some(0.0));
    foggy.fog = Fog {
        color: Some([1.0; 3]),
        start: 0.0,
        end: 40.0,
        ..Fog::default()
    };
    let [_, fogged] = pixels_of(&mut renderer, &foggy, points);
    assert!(
        brightness(fogged) > brightness(clear) + 60,
        "fog lightens the water: {clear:?} -> {fogged:?}"
    );

    // Everything at once: an atmosphere's sky, distance and volumetric
    // height fog, clouds, rain, and wind swaying a reed standing in the
    // water, over waves. It draws, and the water is still seen tinted.
    let mut stormy = pool(Some(0.3));
    let mut reed = block(
        Vec3::new(0.3, 4.0, 0.3),
        Vec3::new(-3.0, 1.0, -2.0),
        [0.3, 0.5, 0.2],
    );
    reed.sway = Some(crate::Sway {
        base: SHALLOW,
        height: 4.0,
        strength: 1.0,
        flutter: 0.0,
    });
    stormy.items.push(reed);
    stormy.sky.atmosphere = Some(Atmosphere::default());
    stormy.sky.environment_intensity = 1.0;
    stormy.fog = Fog {
        color: Some([0.6, 0.6, 0.65]),
        start: 20.0,
        end: 200.0,
        density: 0.01,
        height: 0.0,
        falloff: 10.0,
        sun_scattering: 1.0,
        ..Fog::default()
    };
    stormy.clouds = Clouds {
        cover: 0.6,
        density: 0.8,
        color: [1.0; 3],
        wind: Vec3::new(10.0, 0.0, 0.0),
    };
    stormy.weather = Weather {
        wetness: 1.0,
        snow: 0.0,
    };
    stormy.wind = Vec3::new(10.0, 0.0, 0.0);
    stormy.time = 3.0;
    let [shallow, deep] = pixels_of(&mut renderer, &stormy, points);
    assert_eq!(
        renderer.scene.volumetric_fogs_alive(),
        1,
        "volumetric fog is on"
    );
    assert_eq!(renderer.scene.water_copies_alive(), 1);
    assert!(
        brightness(shallow) > brightness(deep) && deep[2] >= deep[0],
        "the storm's water: {shallow:?} shallow, {deep:?} deep"
    );
}

#[test]
fn water_scales_down_on_low_and_costs_nothing_without_water() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    let caps = crate::Capabilities::default();
    let mode = |quality, list: &DrawList| {
        WaterMode::of(list, &GraphicsSettings::preset(quality).effective(&caps))
    };
    let wet = pool(Some(0.0));
    assert_eq!(mode(Quality::Low, &wet), WaterMode::Blended);
    assert_eq!(mode(Quality::Medium, &wet), WaterMode::Refracted);
    assert_eq!(mode(Quality::High, &wet), WaterMode::Reflected);
    assert_eq!(mode(Quality::Ultra, &wet), WaterMode::Reflected);
    assert_eq!(mode(Quality::Ultra, &pool(None)), WaterMode::None);

    // Low: the water is blended over the floor, with no copy of the scene.
    renderer.set_graphics(&GraphicsSettings::preset(Quality::Low));
    let [dry] = pixels_of(&mut renderer, &pool(None), [FAR_FLOOR]);
    let dry_bytes = renderer.stats().target_bytes;
    let [blended] = pixels_of(&mut renderer, &wet, [FAR_FLOOR]);
    assert_eq!(renderer.scene.water_copies_alive(), 0);
    assert_eq!(
        renderer.stats().target_bytes,
        dry_bytes,
        "no target for water at Low"
    );
    assert!(
        brightness(blended) < brightness(dry) && blended[2] > blended[0],
        "the water is drawn over the floor: {dry:?} -> {blended:?}"
    );

    // From Medium there's one copy while water is drawn, and none after.
    for quality in [Quality::Medium, Quality::High, Quality::Ultra] {
        renderer.set_graphics(&GraphicsSettings::preset(quality));
        renderer.render(&wet).unwrap();
        assert_eq!(renderer.scene.water_copies_alive(), 1, "{quality:?}");
        renderer.render(&pool(None)).unwrap();
        assert_eq!(renderer.scene.water_copies_alive(), 0, "{quality:?}");
    }
    // Switching back and forth leaves no copy behind.
    for _ in 0..10 {
        renderer.render(&wet).unwrap();
        renderer.set_graphics(&GraphicsSettings::preset(Quality::Low));
        renderer.render(&wet).unwrap();
        renderer.set_graphics(&GraphicsSettings::preset(Quality::High));
    }
    renderer.render(&pool(None)).unwrap();
    assert_eq!(renderer.scene.water_copies_alive(), 0);
}

#[test]
fn water_is_drawn_after_solid_items_and_before_other_see_through_ones() {
    let mut glass = block(Vec3::ONE, Vec3::new(0.0, 1.0, 4.0), [1.0; 3]);
    glass.material.opacity = 0.5;
    let mut list = pool(Some(0.2));
    list.items.insert(0, glass);
    let plan = plan(&list);
    let see_through: Vec<_> = plan
        .batches
        .iter()
        .skip_while(|batch| !batch.transparent)
        .map(|batch| batch.water)
        .collect();
    assert_eq!(see_through, [true, false], "water, then the glass");
    assert!(
        plan.casters
            .iter()
            .flatten()
            .all(|caster| caster.mesh != MeshKey::of(&list.items.last().unwrap().shape)),
        "water casts no shadow"
    );
}
