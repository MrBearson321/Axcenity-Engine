//! Graphics volumes (graphics Phase 33), drawn: hello-volumes' cave looks
//! change as the camera flies in, blended over each volume's
//! `BlendDistance`, and where they overlap the higher `Priority` wins.

use std::path::PathBuf;

use axcenity_core::Value;
use axcenity_render::TextureRenderer;

use super::graphics_tests::{SIZE, changed_pixels};
use super::tests::{start_example_in, test_gpu, write_png};
use super::*;

/// The frame from `x` studs along the glide into the cave, looking in.
fn seen_from(renderer: &mut TextureRenderer, world: &World, assets: &Assets, x: f32) -> Vec<u8> {
    let at = Vec3::new(x, 5.0, 0.0);
    let camera = FlyCamera::looking_at(at, at + Vec3::new(1.0, -0.25, 0.0));
    let list = axcenity_scene::build_draw_list(world, assets, camera.to_render_camera());
    renderer.render(&list).unwrap();
    renderer.read_pixels()
}

/// The average difference per channel between two frames.
fn difference(a: &[u8], b: &[u8]) -> f64 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    total as f64 / a.len() as f64
}

/// How much redder than blue a frame is on average.
fn warmth(pixels: &[u8]) -> f64 {
    let total: i64 = pixels
        .chunks(4)
        .map(|pixel| i64::from(pixel[0]) - i64::from(pixel[2]))
        .sum();
    total as f64 / (pixels.len() / 4) as f64
}

#[test]
fn hello_volumes_blends_the_caves_look_in_and_the_higher_priority_wins() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (mut world, _scripts, folder) = start_example_in("hello-volumes");
    let assets = Assets::new(&folder);
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    let cave = world
        .find_first_child(world.workspace(), "CaveLook")
        .unwrap();
    let embers = world.find_first_child(world.workspace(), "Embers").unwrap();

    // With `AXCENITY_SNAPSHOTS` naming a folder, the glide in is written
    // there to look at.
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        std::fs::create_dir_all(&snapshots).unwrap();
        for x in [0.0, 14.0, 17.0, 20.0, 26.0, 32.0, 34.0, 36.0] {
            let pixels = seen_from(&mut renderer, &world, &assets, x);
            write_png(
                &snapshots.join(format!("hello-volumes-{x}.png")),
                &pixels,
                SIZE,
            );
        }
    }

    // 3 studs outside the cave's volume, which blends over 6: half its look.
    let set =
        |world: &mut World, id, name: &str, value| world.set_property(id, name, value).unwrap();
    set(&mut world, cave, "BlendDistance", Value::Number(0.0));
    let none = seen_from(&mut renderer, &world, &assets, 17.0);
    set(&mut world, cave, "BlendDistance", Value::Number(6.0));
    let half = seen_from(&mut renderer, &world, &assets, 17.0);
    set(&mut world, cave, "BlendDistance", Value::Number(1.0e6));
    let whole = seen_from(&mut renderer, &world, &assets, 17.0);
    set(&mut world, cave, "BlendDistance", Value::Number(6.0));
    let (to_half, to_whole) = (difference(&none, &half), difference(&none, &whole));
    assert!(
        to_whole > 5.0,
        "the cave's look changes the frame: {to_whole:.1}"
    );
    assert!(
        to_half > to_whole * 0.25 && to_half < to_whole * 0.75,
        "halfway in, about half of it: {to_half:.1} of {to_whole:.1}"
    );
    assert!(changed_pixels(&half, &whole) > 1000);

    // Deep in the cave, inside both: the Embers (Priority 1) warm it.
    let both = seen_from(&mut renderer, &world, &assets, 36.0);
    set(&mut world, embers, "Enabled", Value::Bool(false));
    let cave_only = seen_from(&mut renderer, &world, &assets, 36.0);
    set(&mut world, embers, "Enabled", Value::Bool(true));
    assert!(
        warmth(&both) > warmth(&cave_only) + 10.0,
        "the embers win: {:.1} against {:.1}",
        warmth(&both),
        warmth(&cave_only)
    );
    // Below the cave's priority, the cave's fog and tint win instead.
    set(&mut world, embers, "Priority", Value::Number(-1.0));
    let under = seen_from(&mut renderer, &world, &assets, 36.0);
    assert_eq!(changed_pixels(&under, &cave_only), 0);
    assert!(assets.take_errors().is_empty());
}
