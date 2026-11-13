//! Fog: distance fog (Lighting's), height fog (an Atmosphere's) and, at
//! Ultra, volumetric fog's sun shafts.

use super::*;
use crate::{Fog, Quality};

/// The frame size these tests draw at.
const SIZE: u32 = 128;

/// A block of `size` at `middle` that gives off red light and is lit by
/// nothing, so only the fog changes it.
fn red_block(middle: Vec3, size: Vec3) -> DrawItem {
    let mut block = item(Shape::Block, middle, 1.0);
    block.transform = Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, middle);
    block.material.base_color = [0.0; 3];
    block.material.emissive = [1.0, 0.0, 0.0];
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

fn pixel_at(pixels: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    pixels[(y * SIZE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// Green fog from 10 studs out, whole from 60.
fn green_distance_fog() -> Fog {
    Fog {
        color: Some([0.0, 1.0, 0.0]),
        start: 10.0,
        end: 60.0,
        ..Fog::default()
    }
}

#[test]
fn far_parts_fade_into_the_distance_fog_and_so_do_sprites_and_the_sky() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let near = Vec3::new(-3.0, 0.0, -8.0);
    let middle = Vec3::new(0.0, 0.0, -35.0);
    let far = Vec3::new(30.0, 0.0, -100.0);
    let mut list = list_of(vec![
        red_block(near, Vec3::splat(2.0)),
        red_block(middle, Vec3::splat(6.0)),
        red_block(far, Vec3::splat(20.0)),
    ]);
    let sprite_at = Vec3::new(-30.0, 25.0, -100.0);
    list.sprites.push(crate::Sprite {
        position: sprite_at,
        size: 20.0,
        ..red_sprite(0.0, 0.0)
    });
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.render(&list).unwrap();
    let clear = renderer.read_pixels();
    for point in [near, middle, far, sprite_at] {
        let [r, g, ..] = pixel_at(&clear, screen_of(&list, point));
        assert!(r > 200 && g < 10, "red without fog at {point}: {r} {g}");
    }

    list.fog = green_distance_fog();
    renderer.render(&list).unwrap();
    let fogged = renderer.read_pixels();
    let [r, g, ..] = pixel_at(&fogged, screen_of(&list, near));
    assert!(r > 200 && g < 10, "nearer than the start is clear: {r} {g}");
    // Halfway from 10 to 60 studs: half the fog, in linear light.
    let [r, g, ..] = pixel_at(&fogged, screen_of(&list, middle));
    assert!(
        (150..=215).contains(&r) && (150..=215).contains(&g),
        "half fog: {r} {g}"
    );
    for point in [far, sprite_at] {
        let [r, g, ..] = pixel_at(&fogged, screen_of(&list, point));
        assert!(
            r < 10 && g > 245,
            "past the end is all fog at {point}: {r} {g}"
        );
    }
    // The sky, which was black, is as far as anything.
    let [r, g, b, _] = pixel_at(&fogged, (64, 2));
    assert!(
        r < 10 && g > 245 && b < 10,
        "the sky fades too: {r} {g} {b}"
    );
}

#[test]
fn height_fog_hides_far_parts_and_thins_above_its_height() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Two blocks as far away: one at the fog's height, one 30 studs up.
    let low = Vec3::new(-12.0, 0.0, -60.0);
    let high = Vec3::new(12.0, 30.0, -60.0);
    let mut list = list_of(vec![
        red_block(low, Vec3::splat(8.0)),
        red_block(high, Vec3::splat(8.0)),
    ]);
    list.camera.look_at = Vec3::new(0.0, 15.0, -60.0);
    list.fog = Fog {
        color: Some([0.0, 1.0, 0.0]),
        density: 0.05,
        height: 0.0,
        falloff: 10.0,
        ..Fog::default()
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let [low_red, low_green, ..] = pixel_at(&pixels, screen_of(&list, low)).map(u16::from);
    let [high_red, high_green, ..] = pixel_at(&pixels, screen_of(&list, high)).map(u16::from);
    assert!(
        low_green > 200,
        "60 studs of thick fog: {low_red} {low_green}"
    );
    assert!(
        high_red > low_red + 60 && high_green < low_green,
        "thinner up high: {high_red} {high_green} vs {low_red} {low_green}"
    );

    // Raised to the high block, the fog around it is as thick as low down.
    list.fog.height = 40.0;
    renderer.render(&list).unwrap();
    let raised = renderer.read_pixels();
    let [red, green, ..] = pixel_at(&raised, screen_of(&list, high)).map(u16::from);
    assert!(
        red + 60 < high_red && green > 200,
        "under the fog's height: {red} {green} vs {high_red} {high_green}"
    );
}

/// Black air full of fog that scatters only the sun's light, with the sun
/// low ahead of the camera, and a black wall over the left half of the
/// view that shadows the air in front of it.
fn shafts() -> (DrawList, Vec3, Vec3) {
    let wall_middle = Vec3::new(-25.0, 0.0, -40.0);
    let mut wall = red_block(wall_middle, Vec3::new(50.0, 60.0, 1.0));
    wall.material.emissive = [0.0; 3];
    let mut list = list_of(vec![wall]);
    list.sun.direction = Vec3::new(0.0, 0.25, -1.0);
    list.fog = Fog {
        color: Some([0.0; 3]),
        density: 0.02,
        height: 100.0,
        falloff: 50.0,
        sun_scattering: 1.0,
        forward_scattering: 0.6,
        ..Fog::default()
    };
    // In front of the wall, and the open sky beside it.
    (
        list,
        Vec3::new(-12.0, 0.0, -40.0),
        Vec3::new(12.0, 0.0, -40.0),
    )
}

fn ultra(volumetric: bool) -> GraphicsSettings {
    GraphicsSettings {
        volumetric_fog: Some(volumetric),
        ..GraphicsSettings::preset(Quality::Ultra)
    }
}

#[test]
fn volumetric_fog_shadows_the_suns_light_in_the_air_at_ultra() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (list, shadowed, open) = shafts();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    assert!(GraphicsSettings::preset(Quality::Ultra).volumetric_fog());
    renderer.set_graphics(&ultra(true));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.volumetric_fogs_alive(), 1);
    let on = renderer.read_pixels();
    renderer.set_graphics(&ultra(false));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.volumetric_fogs_alive(), 0);
    let off = renderer.read_pixels();

    let [lit_on, ..] = pixel_at(&on, screen_of(&list, open));
    let [lit_off, ..] = pixel_at(&off, screen_of(&list, open));
    assert!(lit_off > 120, "the sun glows in the fog: {lit_off}");
    assert!(
        lit_on.abs_diff(lit_off) <= 8,
        "open air scatters the same: {lit_on} vs {lit_off}"
    );
    let [dark_on, ..] = pixel_at(&on, screen_of(&list, shadowed));
    let [dark_off, ..] = pixel_at(&off, screen_of(&list, shadowed));
    assert!(
        dark_off > 90,
        "without it, the air before the wall glows: {dark_off}"
    );
    assert!(
        dark_on + 60 < dark_off,
        "the wall's shadow in the air: {dark_on} vs {dark_off}"
    );
}

#[test]
fn volumetric_fog_is_absent_below_ultra_and_costs_nothing() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (mut list, ..) = shafts();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    for quality in [Quality::Low, Quality::Medium, Quality::High] {
        renderer.set_graphics(&GraphicsSettings::preset(quality));
        list.fog = Fog::default();
        renderer.render(&list).unwrap();
        let clear = renderer.stats();
        list.fog = shafts().0.fog;
        renderer.render(&list).unwrap();
        let fogged = renderer.stats();
        assert_eq!(renderer.scene.volumetric_fogs_alive(), 0, "{quality:?}");
        assert_eq!(fogged.target_bytes, clear.target_bytes, "{quality:?}");
        assert_eq!(fogged.draw_calls, clear.draw_calls, "{quality:?}");
    }
    // At Ultra it's drawn only while there's fog for it, and freed after.
    renderer.set_graphics(&GraphicsSettings::preset(Quality::Ultra));
    renderer.render(&list).unwrap();
    let with_grid = renderer.stats().target_bytes;
    assert_eq!(renderer.scene.volumetric_fogs_alive(), 1);
    list.fog = Fog::default();
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.volumetric_fogs_alive(), 0);
    assert!(renderer.stats().target_bytes < with_grid);
}

/// `shafts()` with grey fog instead of black: its own light shows too.
fn grey_shafts() -> (DrawList, Vec3, Vec3) {
    let (mut list, shadowed, open) = shafts();
    list.fog.color = Some([0.5; 3]);
    (list, shadowed, open)
}

#[test]
fn height_fogs_own_light_looks_the_same_in_the_open_with_volumetric_fog() {
    // The grid scatters the fog's own light by how open the air is to the
    // sky; with no light probes to say otherwise that's the open sky, so the
    // picture doesn't change where nothing shadows the air.
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (list, _, open) = grey_shafts();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&ultra(true));
    renderer.render(&list).unwrap();
    let on = renderer.read_pixels();
    renderer.set_graphics(&ultra(false));
    renderer.render(&list).unwrap();
    let off = renderer.read_pixels();
    let (lit_on, lit_off) = (
        pixel_at(&on, screen_of(&list, open)),
        pixel_at(&off, screen_of(&list, open)),
    );
    for channel in 0..3 {
        assert!(
            lit_on[channel].abs_diff(lit_off[channel]) <= 8,
            "open air: {lit_on:?} vs {lit_off:?}"
        );
    }
}

/// Dust filling a box from `x0` to `x1` across, 60 studs high and from 2
/// to 60 studs in front of the camera, white and fairly thick.
fn dust_box(x0: f32, x1: f32) -> crate::Dust {
    let middle = Vec3::new((x0 + x1) * 0.5, 0.0, -31.0);
    crate::Dust {
        density: 0.04,
        color: [1.0; 3],
        forward_scattering: 0.6,
        to_box: Mat4::from_translation(-middle),
        half_size: Vec3::new((x1 - x0) * 0.5, 30.0, 29.0),
        edge: 0.5,
        ..crate::Dust::default()
    }
}

#[test]
fn dust_glows_only_inside_its_box_with_or_without_volumetric_fog() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // A black wall far behind, and the sun high ahead (low, the wall would
    // shadow the dust in front of it); dust only on the left.
    let mut wall = red_block(Vec3::new(0.0, 0.0, -80.0), Vec3::new(200.0, 100.0, 1.0));
    wall.material.emissive = [0.0; 3];
    let mut list = list_of(vec![wall]);
    list.sun.direction = Vec3::new(0.0, 1.0, -0.3);
    list.fog = Fog {
        dust: crate::DustBoxes::one(dust_box(-60.0, -2.0)),
        ..Fog::default()
    };
    let (inside, outside) = (Vec3::new(-20.0, 0.0, -80.0), Vec3::new(20.0, 0.0, -80.0));
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    for volumetric in [true, false] {
        renderer.set_graphics(&ultra(volumetric));
        renderer.render(&list).unwrap();
        assert_eq!(
            renderer.scene.volumetric_fogs_alive(),
            usize::from(volumetric),
            "dust alone draws the volumetric fog at Ultra"
        );
        let pixels = renderer.read_pixels();
        let glow = pixel_at(&pixels, screen_of(&list, inside));
        let clear = pixel_at(&pixels, screen_of(&list, outside));
        assert!(
            glow[0] > 40,
            "the dust glows (volumetric {volumetric}): {glow:?}"
        );
        assert!(
            clear[0] < 12,
            "no dust outside its box (volumetric {volumetric}): {clear:?}"
        );
    }
}

#[test]
fn dust_in_a_walls_shadow_loses_the_suns_light_at_ultra() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // `shafts()`'s wall shadows the air in front of its half of the view,
    // here filled with dust (and no fog) all the way across.
    let (mut list, shadowed, open) = shafts();
    list.fog = Fog {
        dust: crate::DustBoxes::one(dust_box(-60.0, 60.0)),
        ..Fog::default()
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    renderer.set_graphics(&ultra(true));
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let [dark, ..] = pixel_at(&pixels, screen_of(&list, shadowed));
    let [lit, ..] = pixel_at(&pixels, screen_of(&list, open));
    assert!(lit > 60, "sunlit dust glows: {lit}");
    assert!(
        dark + 30 < lit,
        "the wall's shadow in the dust: {dark} vs {lit}"
    );
}

#[test]
fn several_boxes_of_dust_glow_at_once() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut wall = red_block(Vec3::new(0.0, 0.0, -80.0), Vec3::new(200.0, 100.0, 1.0));
    wall.material.emissive = [0.0; 3];
    let mut list = list_of(vec![wall]);
    list.sun.direction = Vec3::new(0.0, 1.0, -0.3);
    list.fog = Fog {
        // Narrow, so the rays to the wall's left and right cross them.
        dust: [dust_box(-20.0, -8.0), dust_box(8.0, 20.0)]
            .into_iter()
            .collect(),
        ..Fog::default()
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    for volumetric in [true, false] {
        renderer.set_graphics(&ultra(volumetric));
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let left = pixel_at(&pixels, screen_of(&list, Vec3::new(-25.0, 0.0, -80.0)));
        let right = pixel_at(&pixels, screen_of(&list, Vec3::new(25.0, 0.0, -80.0)));
        let middle = pixel_at(&pixels, screen_of(&list, Vec3::new(0.0, 0.0, -80.0)));
        assert!(
            left[0] > 40 && right[0] > 40,
            "both glow (volumetric {volumetric}): {left:?} {right:?}"
        );
        assert!(middle[0] < 12, "none between: {middle:?}");
    }
}

#[test]
fn an_interior_clears_the_worlds_fog_inside_it_and_keeps_its_own_dust() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Thick grey height fog everywhere, and an interior over the left half
    // of the way to a black wall.
    let mut wall = red_block(Vec3::new(0.0, 0.0, -80.0), Vec3::new(200.0, 100.0, 1.0));
    wall.material.emissive = [0.0; 3];
    let mut list = list_of(vec![wall]);
    list.sun.direction = Vec3::new(0.0, 1.0, -0.3);
    // Up to just short of the wall, so little fog is left on the way.
    let interior = crate::Dust {
        interior: true,
        density: 0.0,
        to_box: Mat4::from_translation(-Vec3::new(-31.0, 0.0, -40.0)),
        half_size: Vec3::new(29.0, 30.0, 38.0),
        ..dust_box(-60.0, -2.0)
    };
    list.fog = Fog {
        color: Some([0.8; 3]),
        density: 0.04,
        height: 100.0,
        falloff: 50.0,
        dust: crate::DustBoxes::one(interior),
        ..Fog::default()
    };
    let (inside, outside) = (Vec3::new(-20.0, 0.0, -80.0), Vec3::new(20.0, 0.0, -80.0));
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    for volumetric in [true, false] {
        renderer.set_graphics(&ultra(volumetric));
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let cleared = pixel_at(&pixels, screen_of(&list, inside));
        let foggy = pixel_at(&pixels, screen_of(&list, outside));
        assert!(
            cleared[1] + 30 < foggy[1],
            "less fog through the interior (volumetric {volumetric}): {cleared:?} vs {foggy:?}"
        );
    }
    // Its own dust fills it again.
    list.fog.dust = crate::DustBoxes::one(crate::Dust {
        density: 0.04,
        ..interior
    });
    renderer.set_graphics(&ultra(false));
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let dusty = pixel_at(&pixels, screen_of(&list, inside));
    assert!(dusty[0] > 40, "the interior's own dust glows: {dusty:?}");
}
