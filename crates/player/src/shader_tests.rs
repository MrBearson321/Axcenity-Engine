//! Materials' surface shaders in the player, on hello-shader: its
//! conveyor's chevrons scroll with the frame's time, and a shader with an
//! error is reported on its own line while the rest of the scene draws.

use std::path::{Path, PathBuf};

use axcenity_render::{DrawList, TextureRenderer};

use super::graphics_tests::{SIZE, average, changed_pixels, pixels_of};
use super::tests::{run_until, start_example_in, test_gpu, write_png};
use super::*;

const CONVEYOR: &str = "assets/materials/Conveyor.material.json";

/// hello-shader's folder (a copy), world, assets and the player's camera.
fn hello_shader() -> (PathBuf, World, Assets, FlyCamera) {
    let (mut world, mut scripts, folder) = start_example_in("hello-shader");
    let (mut physics, mut clock) = (Physics::new(), 0);
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.5);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    let assets = Assets::new(&folder);
    (folder, world, assets, camera)
}

/// The frame `list` draws at `time` seconds.
fn frame_at(renderer: &mut TextureRenderer, list: &mut DrawList, time: f32) -> Vec<u8> {
    list.time = time;
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn snapshot(name: &str, pixels: &[u8]) {
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        write_png(&Path::new(&folder).join(name), pixels, SIZE);
    }
}

#[test]
fn the_conveyors_shader_scrolls_its_texture_over_time() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (folder, world, assets, camera) = hello_shader();
    let mut list = axcenity_scene::build_draw_list(&world, &assets, camera.to_render_camera());
    let shaded: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| item.material.surface_shader.as_ref())
        .collect();
    assert_eq!(shaded.len(), 1, "the conveyor has the material's shader");
    assert_eq!(shaded[0].material(), CONVEYOR);

    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    let start = frame_at(&mut renderer, &mut list, 0.0);
    let later = frame_at(&mut renderer, &mut list, 0.5);
    let around = frame_at(&mut renderer, &mut list, 2.0);
    snapshot("shader-conveyor-0.png", &start);
    snapshot("shader-conveyor-0.5.png", &later);
    assert!(renderer.take_shader_errors().is_empty());
    assert!(assets.take_errors().is_empty());

    let [conveyor, crate_box] = pixels_of(&world, &camera, &["Conveyor", "Crate"])
        .try_into()
        .unwrap();
    let moved = |a: &[u8], b: &[u8], at: &[usize]| {
        at.iter()
            .filter(|&&pixel| a[pixel * 4..][..3] != b[pixel * 4..][..3])
            .count()
    };
    let scrolled = moved(&start, &later, &conveyor);
    assert!(
        scrolled * 3 > conveyor.len(),
        "a quarter of a texture along, the chevrons moved: {scrolled} of {} pixels",
        conveyor.len()
    );
    // Only the crate's edge pixels can change, by the belt's glow blooming.
    let still = moved(&start, &later, &crate_box);
    assert!(
        still * 20 < crate_box.len(),
        "the crate stays still: {still}"
    );
    // A whole texture along (a copy every 4 studs), the belt looks as it began.
    let back = changed_pixels(&start, &around);
    assert!(back * 50 < conveyor.len(), "{back} pixels differ");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_shader_with_an_error_is_reported_on_its_line_and_the_rest_of_the_scene_draws() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (folder, world, assets, camera) = hello_shader();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    let mut list = axcenity_scene::build_draw_list(&world, &assets, camera.to_render_camera());
    let working = frame_at(&mut renderer, &mut list, 0.0);

    // Line 3 loses its `;`, which is found where line 4 starts.
    let path = folder.join(CONVEYOR);
    let mut material = axcenity_assets::read_material_file(&path).unwrap();
    let code = material.shader.take().unwrap();
    let mut lines: Vec<&str> = code.lines().collect();
    assert!(lines[2].ends_with(';'), "{}", lines[2]);
    let broken_line = lines[2].trim_end_matches(';').to_owned();
    lines[2] = &broken_line;
    material.shader = Some(lines.join("\n"));
    axcenity_assets::write_material_file(&path, &material).unwrap();
    assets.forget(CONVEYOR);

    let mut list = axcenity_scene::build_draw_list(&world, &assets, camera.to_render_camera());
    let broken = frame_at(&mut renderer, &mut list, 0.0);
    snapshot("shader-error.png", &broken);
    let errors = renderer.take_shader_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].material, CONVEYOR);
    assert_eq!(errors[0].line, Some(4), "{}", errors[0]);
    assert!(
        errors[0].to_string().starts_with(&format!(
            "The shader of the material `{CONVEYOR}` has an error on line 4"
        )),
        "{}",
        errors[0]
    );

    let [conveyor, crate_box, floor] = pixels_of(&world, &camera, &["Conveyor", "Crate", "Floor"])
        .try_into()
        .unwrap();
    let [red, green, blue] = average(&broken, &conveyor);
    assert!(
        red > 2.0 * green && blue > 2.0 * green,
        "the conveyor is magenta: {red:.0} {green:.0} {blue:.0}"
    );
    for (name, at) in [("crate", &crate_box), ("floor", &floor)] {
        let (before, after) = (average(&working, at), average(&broken, at));
        let off = (0..3)
            .map(|i| (before[i] - after[i]).abs())
            .fold(0.0, f64::max);
        assert!(
            off < 2.0,
            "the {name} draws as before: {before:?} {after:?}"
        );
    }
    renderer.render(&list).unwrap();
    assert!(renderer.take_shader_errors().is_empty(), "reported once");
    let _ = std::fs::remove_dir_all(&folder);
}
