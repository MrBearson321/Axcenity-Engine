//! Plants swaying in the wind: downwind, more in a stronger wind, still
//! with none, and their shadows moving with them.

use super::*;
use crate::Sway;
use glam::Vec2;

/// The frame size, square.
const SIZE: u32 = 128;
/// How tall the plant is, in studs. It stands on the ground at the origin.
const HEIGHT: f32 = 8.0;

/// A thin pole standing on the ground at the origin, swaying like grass.
fn plant(sway: bool) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(0.5, HEIGHT, 0.5),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, HEIGHT / 2.0, 0.0),
        ),
        sway: sway.then_some(Sway {
            base: 0.0,
            height: HEIGHT,
            strength: 1.0,
            flutter: 0.0,
        }),
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    }
}

/// The plant glowing white against the black sky, seen side on from +Z,
/// with no shadows.
fn plant_seen_side_on(sway: bool, wind: Vec3) -> DrawList {
    let mut plant = plant(sway);
    plant.material.base_color = [0.0; 3];
    plant.material.emissive = [1.0; 3];
    let mut list = list_of(vec![plant]);
    list.camera = Camera {
        position: Vec3::new(0.0, HEIGHT / 2.0, 20.0),
        look_at: Vec3::new(0.0, HEIGHT / 2.0, 0.0),
        fov_y_degrees: 70.0,
    };
    list.shadows.enabled = false;
    list.wind = wind;
    list
}

/// The plant over a wide white floor, lit by a sun from +Z (so its shadow
/// runs toward -Z, as long as it's tall), seen from above and behind.
fn plant_and_its_shadow(sway: bool, wind: Vec3) -> DrawList {
    let floor = DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(200.0, 1.0, 200.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
        ..item(Shape::Block, Vec3::ZERO, 1.0)
    };
    let mut list = list_of(vec![floor, plant(sway)]);
    list.camera = Camera {
        position: Vec3::new(0.0, 30.0, 2.0),
        look_at: Vec3::new(0.0, 0.0, -5.0),
        fov_y_degrees: 50.0,
    };
    list.sun.direction = Vec3::new(0.0, 1.0, 1.0).normalize();
    list.sun.intensity = 2.0;
    list.wind = wind;
    list
}

/// Where `point` is drawn in a `SIZE`-square frame of `list`, in pixels.
fn on_screen(list: &DrawList, point: Vec3) -> Vec2 {
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * SIZE as f32
}

/// The pixel `point` is drawn on in `list`, drawn by `renderer`.
fn pixel_on(renderer: &mut TextureRenderer, list: &DrawList, point: Vec3) -> [u8; 4] {
    renderer.render(list).unwrap();
    let at = on_screen(list, point);
    let (x, y) = (at.x as usize, at.y as usize);
    renderer.read_pixels()[(y * SIZE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// How many pixels one stud along X covers around `point`.
fn pixels_per_stud(list: &DrawList, point: Vec3) -> f32 {
    on_screen(list, point + Vec3::X).x - on_screen(list, point).x
}

/// The mean column of the pixels in rows `top..bottom`, each counted by
/// its `weight` (so a part-covered edge pixel counts in part), or `None`
/// if none weighs anything.
fn mean_column(
    pixels: &[u8],
    (top, bottom): (f32, f32),
    weight: impl Fn([u8; 4]) -> f32,
) -> Option<f32> {
    let (mut sum, mut total) = (0.0, 0.0);
    for y in top.round() as usize..bottom.round() as usize {
        for x in 0..SIZE as usize {
            let pixel: [u8; 4] = pixels[(y * SIZE as usize + x) * 4..][..4]
                .try_into()
                .unwrap();
            let weight = weight(pixel);
            sum += x as f32 * weight;
            total += weight;
        }
    }
    (total > 0.0).then(|| sum / total)
}

/// The rows the plant's height from `from` to `to` studs is drawn on, side on.
fn rows_of_heights(list: &DrawList, from: f32, to: f32) -> (f32, f32) {
    (
        on_screen(list, Vec3::new(0.0, to, 0.0)).y,
        on_screen(list, Vec3::new(0.0, from, 0.0)).y,
    )
}

fn brightness([r, ..]: [u8; 4]) -> f32 {
    f32::from(r)
}

/// Where the plant is drawn between `from` and `to` studs up, as the mean
/// column of its pixels.
fn plant_column(renderer: &mut TextureRenderer, list: &DrawList, from: f32, to: f32) -> f32 {
    renderer.render(list).unwrap();
    mean_column(
        &renderer.read_pixels(),
        rows_of_heights(list, from, to),
        brightness,
    )
    .expect("the plant is drawn")
}

#[test]
fn plants_lean_downwind_more_in_a_stronger_wind_and_stand_still_without_one() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);

    // A plant in no wind draws exactly as one that never sways, and so
    // does one that never sways in a gale.
    renderer
        .render(&plant_seen_side_on(false, Vec3::ZERO))
        .unwrap();
    let still = renderer.read_pixels();
    renderer
        .render(&plant_seen_side_on(true, Vec3::ZERO))
        .unwrap();
    assert!(still == renderer.read_pixels(), "no wind moved the plant");
    let gale = Vec3::new(40.0, 0.0, 0.0);
    renderer.render(&plant_seen_side_on(false, gale)).unwrap();
    assert!(
        still == renderer.read_pixels(),
        "the wind moved an item that doesn't sway"
    );

    let top = |renderer: &mut TextureRenderer, wind| {
        plant_column(renderer, &plant_seen_side_on(true, wind), 6.0, 7.8)
    };
    let foot = |renderer: &mut TextureRenderer, wind| {
        plant_column(renderer, &plant_seen_side_on(true, wind), 0.2, 1.0)
    };
    let rest = top(&mut renderer, Vec3::ZERO);
    let east = top(&mut renderer, gale);
    let west = top(&mut renderer, -gale);
    let breeze = top(&mut renderer, gale / 4.0);
    // Wind straight toward the camera bends the plant toward it, not sideways.
    let toward = top(&mut renderer, Vec3::new(0.0, 0.0, 40.0));
    assert!(
        east > rest + 2.0,
        "a wind to +X leans it right: {rest} -> {east}"
    );
    assert!(
        west < rest - 2.0,
        "a wind to -X leans it left: {rest} -> {west}"
    );
    assert!(
        rest < breeze && breeze < east - 1.0,
        "a breeze leans it less than a gale: {rest}, {breeze}, {east}"
    );
    assert!((toward - rest).abs() < 0.5, "{rest} -> {toward}");
    // Rooted: the foot barely moves while the top swings.
    let foot_shift = foot(&mut renderer, gale) - foot(&mut renderer, Vec3::ZERO);
    assert!(foot_shift.abs() < 0.5, "the foot moved {foot_shift} pixels");
}

#[test]
fn a_swaying_plants_shadow_moves_as_far_as_the_plant() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    let gale = Vec3::new(40.0, 0.0, 0.0);

    // The sun is at 45°, so the shadow of the plant's height `h` falls `h`
    // studs toward -Z: its top part, from 6 studs up, is the end of it.
    let shadow_column = |renderer: &mut TextureRenderer, sway, wind| {
        let list = plant_and_its_shadow(sway, wind);
        // The floor in the sun, well clear of the shadow.
        let lit = pixel_on(renderer, &list, Vec3::new(8.0, 0.0, -7.0))[0];
        let rows = (
            on_screen(&list, Vec3::new(0.0, 0.0, -7.8)).y,
            on_screen(&list, Vec3::new(0.0, 0.0, -6.0)).y,
        );
        // How much darker than the lit floor each pixel is.
        let column = mean_column(&renderer.read_pixels(), rows, |[r, ..]| {
            f32::from(lit.saturating_sub(r))
        });
        (list, column.expect("the shadow is drawn"))
    };
    let (_, unswayed) = shadow_column(&mut renderer, false, Vec3::ZERO);
    let (list, still) = shadow_column(&mut renderer, true, Vec3::ZERO);
    assert_eq!(unswayed, still, "no wind moved the shadow");
    let (_, swayed) = shadow_column(&mut renderer, true, gale);
    let shadow_studs = (swayed - still) / pixels_per_stud(&list, Vec3::new(0.0, 0.0, -7.0));

    let side_on = plant_seen_side_on(true, Vec3::ZERO);
    let plant_studs = (plant_column(&mut renderer, &plant_seen_side_on(true, gale), 6.0, 7.8)
        - plant_column(&mut renderer, &side_on, 6.0, 7.8))
        / pixels_per_stud(&side_on, Vec3::new(0.0, 7.0, 0.0));
    assert!(plant_studs > 0.5, "the plant leaned {plant_studs} studs");
    assert!(
        (shadow_studs - plant_studs).abs() < 0.35 * plant_studs,
        "the shadow moved {shadow_studs} studs, the plant {plant_studs}"
    );
}

#[test]
fn a_swaying_plant_is_culled_by_how_far_it_can_reach() {
    let mut list = plant_seen_side_on(true, Vec3::new(40.0, 0.0, 0.0));
    // Just out of view to the right (its bounding sphere half a stud past
    // the edge of the view), but near enough for the wind to lean it back
    // in. The view is 70° across, 20 studs from the camera.
    let half_angle = 35f32.to_radians();
    let radius = Vec3::new(0.5, HEIGHT, 0.5).length() / 2.0;
    let just_out = 20.0 * half_angle.tan() + (radius + 0.5) / half_angle.cos();
    list.items[0].transform =
        Mat4::from_translation(Vec3::new(just_out, 0.0, 0.0)) * list.items[0].transform;
    list.items[0].sway = None;
    assert_eq!(plan(&list).culled, 1, "a still item there is out of view");
    list.items[0].sway = Some(Sway {
        base: 0.0,
        height: HEIGHT,
        strength: 1.0,
        flutter: 0.0,
    });
    assert_eq!(plan(&list).culled, 0, "a swaying one may lean into view");
}

#[test]
fn leaves_flutter_at_the_top_of_a_plant_and_not_at_its_foot() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE, SIZE);
    let gale = Vec3::new(40.0, 0.0, 0.0);
    let scene = |flutter: f32, time: f32| {
        let mut list = plant_seen_side_on(true, gale);
        list.items[0].sway.as_mut().unwrap().flutter = flutter;
        list.time = time;
        list
    };
    let shift = |renderer: &mut TextureRenderer, from: f32, to: f32| {
        // The farthest the fluttering plant gets from the plain one, over
        // moments a quick wave apart.
        (0..8)
            .map(|step| {
                let time = 1.0 + step as f32 * 0.13;
                let plain = plant_column(renderer, &scene(0.0, time), from, to);
                let shaken = plant_column(renderer, &scene(0.9, time), from, to);
                (plain - shaken).abs()
            })
            .fold(0.0_f32, f32::max)
    };
    let top = shift(&mut renderer, 6.0, 7.8);
    assert!(top > 0.5, "the top only moved {top} pixels");
    let foot = shift(&mut renderer, 0.05, 0.3);
    assert!(foot < top * 0.5, "the foot moved {foot}, the top {top}");
}

#[test]
fn flutter_keeps_the_decal_role_apart_and_widens_the_reach() {
    let plain = Sway {
        base: 0.0,
        height: HEIGHT,
        strength: 1.0,
        flutter: 0.0,
    };
    let leaves = Sway {
        flutter: 0.5,
        ..plain
    };
    assert!(leaves.reach() > plain.reach());
}
