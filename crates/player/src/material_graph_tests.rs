//! Material graphs in the player, on hello-material-graph: the player reads
//! only the WGSL the editor's graph made. Its two walls mix stone and moss
//! by noise; the second is an instance that changes only the moss's tint.

use std::path::Path;

use axcenity_render::TextureRenderer;

use super::graphics_tests::{SIZE, average, pixels_of};
use super::tests::{run_until, start_example_in, test_gpu, write_png};
use super::*;

const MOSSY: &str = "assets/materials/MossyStone.material.json";

/// How many of `at` are mostly green, and how many orange.
fn greens_and_oranges(pixels: &[u8], at: &[usize]) -> (usize, usize) {
    let mut counts = (0, 0);
    for &pixel in at {
        let [r, g, b] = [0, 1, 2].map(|i| i32::from(pixels[pixel * 4 + i]));
        if g > r + 12 && g > b + 12 {
            counts.0 += 1;
        }
        if r > g + 12 && r > b + 12 {
            counts.1 += 1;
        }
    }
    counts
}

#[test]
fn a_graph_mixing_stone_and_moss_by_noise_draws_and_an_instance_changes_only_its_tint() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let (mut world, mut scripts, folder) = start_example_in("hello-material-graph");
    let (mut physics, mut clock) = (Physics::new(), 0);
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 0.2);
    let camera = FlyCamera::looking_at(Vec3::new(0.0, 4.0, 16.0), Vec3::new(0.0, 2.5, 0.0));
    let assets = Assets::new(&folder);
    let list = axcenity_scene::build_draw_list(&world, &assets, camera.to_render_camera());

    let walls: Vec<_> = list
        .items
        .iter()
        .filter(|item| item.material.surface_shader.is_some())
        .collect();
    assert_eq!(walls.len(), 2, "both walls draw the graph's shader");
    let shaders: Vec<_> = walls
        .iter()
        .map(|item| item.material.surface_shader.clone().unwrap())
        .collect();
    assert_eq!(shaders[0].key(), shaders[1].key(), "the instance shares it");
    assert_eq!(shaders[0].material(), MOSSY);
    let tints: Vec<[f32; 4]> = walls
        .iter()
        .map(|item| item.material.surface_parameters[0])
        .collect();
    assert!(tints.contains(&[0.35, 0.7, 0.2, 0.0]), "{tints:?}");
    assert!(tints.contains(&[1.0, 0.45, 0.1, 0.0]), "{tints:?}");
    for wall in &walls {
        assert_eq!(
            wall.material.surface_parameters[1][0], 0.85,
            "its roughness"
        );
        assert!(
            wall.material.base_color_texture.is_some() && wall.material.maps.emissive.is_some()
        );
    }

    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        write_png(
            &Path::new(&snapshots).join("material-graph.png"),
            &pixels,
            SIZE,
        );
    }
    assert!(renderer.take_shader_errors().is_empty());
    assert!(
        assets.take_errors().is_empty(),
        "{:?}",
        assets.take_errors()
    );

    let [mossy, autumn] = pixels_of(&world, &camera, &["MossyWall", "AutumnWall"])
        .try_into()
        .unwrap();
    let (green, orange) = greens_and_oranges(&pixels, &mossy);
    assert!(
        green * 6 > mossy.len() && green * 6 < mossy.len() * 5,
        "the mossy wall is part moss: {green} of {} green",
        mossy.len()
    );
    assert!(orange * 20 < mossy.len(), "and none orange: {orange}");
    let (green, orange) = greens_and_oranges(&pixels, &autumn);
    assert!(
        orange * 6 > autumn.len() && orange * 6 < autumn.len() * 5,
        "the instance's moss is orange: {orange} of {}",
        autumn.len()
    );
    assert!(green * 20 < autumn.len(), "and none green: {green}");
    let stone = |at: &[usize]| {
        let grey: Vec<usize> = at
            .iter()
            .copied()
            .filter(|&pixel| {
                let [r, g, b] = [0, 1, 2].map(|i| i32::from(pixels[pixel * 4 + i]));
                (r - g).abs() < 8 && (g - b).abs() < 10
            })
            .collect();
        average(&pixels, &grey)
    };
    let (a, b) = (stone(&mossy), stone(&autumn));
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 12.0),
        "the stone is the same on both: {a:?} {b:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
