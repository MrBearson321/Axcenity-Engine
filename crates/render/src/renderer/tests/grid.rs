//! The editor's measuring grid: lines on a surface, hidden by what's in
//! front of them, and nothing at all when the list has none.

use super::*;
use crate::GridPlane;

const SIZE: u32 = 128;

fn floor_list(with_cover: bool) -> DrawList {
    let block = |size: Vec3, position: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        material: Material {
            base_color: [0.8; 3],
            roughness: 1.0,
            ..Material::default()
        },
        ..big_block()
    };
    let mut items = vec![block(
        Vec3::new(200.0, 1.0, 200.0),
        Vec3::new(0.0, -0.5, 0.0),
    )];
    if with_cover {
        items.push(block(Vec3::new(40.0, 1.0, 40.0), Vec3::new(0.0, 5.0, 0.0)));
    }
    let mut list = list_of(items);
    list.camera = Camera {
        position: Vec3::new(0.0, 20.0, 0.01),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    list.sun.intensity = 0.0;
    list.ambient.intensity = 0.6;
    list.shadows.enabled = false;
    list
}

fn grid() -> GridPlane {
    GridPlane {
        height: 0.002,
        min: [-100.0, -100.0],
        max: [100.0, 100.0],
        spacing: 1.0,
        major_every: 10.0,
        fade_distance: 0.0,
        color: [0.0; 3],
        strength: 1.0,
        tint: None,
    }
}

fn pixels(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> Vec<u8> {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 8))
        .count()
}

#[test]
fn a_grid_draws_lines_on_the_surface_it_lies_on() {
    let Some((device, queue)) = test_device() else {
        return;
    };
    let plain = floor_list(false);
    let mut with_grid = plain.clone();
    with_grid.grid = Some(grid());
    let (without, with) = (
        pixels(&device, &queue, &plain),
        pixels(&device, &queue, &with_grid),
    );
    let total = (SIZE * SIZE) as usize;
    let changed = differing(&without, &with);
    assert!(changed > total / 50, "only {changed} pixels show lines");
    assert!(changed < total / 2, "{changed} pixels changed: not lines");
}

#[test]
fn a_part_in_front_of_the_grid_hides_it() {
    let Some((device, queue)) = test_device() else {
        return;
    };
    let plain = floor_list(true);
    let mut with_grid = plain.clone();
    with_grid.grid = Some(grid());
    // The cover fills the view from this close, so no line shows.
    assert_eq!(
        differing(
            &pixels(&device, &queue, &plain),
            &pixels(&device, &queue, &with_grid)
        ),
        0
    );
}

#[test]
fn a_grid_stops_at_its_edges() {
    let Some((device, queue)) = test_device() else {
        return;
    };
    let plain = floor_list(false);
    let mut with_grid = plain.clone();
    with_grid.grid = Some(GridPlane {
        min: [100.0, 100.0],
        max: [110.0, 110.0],
        ..grid()
    });
    assert_eq!(
        differing(
            &pixels(&device, &queue, &plain),
            &pixels(&device, &queue, &with_grid)
        ),
        0
    );
}
