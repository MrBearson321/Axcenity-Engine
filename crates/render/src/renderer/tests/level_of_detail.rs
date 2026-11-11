//! Levels of detail: which copy of a mesh an item draws, switching with
//! hysteresis and a dithered cross-fade, and foliage's cull distance.

use super::*;
use crate::renderer::lod::{FADE_FRAMES, LOD_SCREEN_SIZES};

/// A square facing +Z across the unit cube, its two triangles sent
/// `copies` times, so each level has a known triangle count and looks the
/// same. Level 0 has two simpler copies, with half and a quarter of its
/// triangles.
pub(super) struct Detailed {
    pub(super) level: usize,
}

const FULL_TRIANGLES: u64 = 64;

impl Detailed {
    fn triangles(level: usize) -> u64 {
        FULL_TRIANGLES >> level
    }
}

impl MeshSource for Detailed {
    fn key(&self) -> u64 {
        900 + self.level as u64
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[[0.0, 0.0, 1.0]; 4]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[]
    }
    fn indices(&self) -> &[u32] {
        const SQUARE: [u32; 6] = [0, 1, 2, 0, 2, 3];
        const ALL: [u32; 6 * 32] = {
            let mut all = [0; 6 * 32];
            let mut at = 0;
            while at < all.len() {
                all[at] = SQUARE[at % 6];
                at += 1;
            }
            all
        };
        &ALL[..(Detailed::triangles(self.level) * 3) as usize]
    }
    fn lod_count(&self) -> usize {
        if self.level == 0 { 2 } else { 0 }
    }
    fn lod(&self, level: usize) -> Option<&Arc<dyn MeshSource>> {
        static COPIES: std::sync::OnceLock<[Arc<dyn MeshSource>; 2]> = std::sync::OnceLock::new();
        let copies = COPIES.get_or_init(|| {
            [1, 2].map(|level| Arc::new(Detailed { level }) as Arc<dyn MeshSource>)
        });
        if self.level == 0 {
            copies.get(level.checked_sub(1)?)
        } else {
            None
        }
    }
}

/// A glowing white item of the detailed square, `distance` studs straight
/// ahead, on a black sky with shadows off.
fn square_at(distance: f32, id: u64) -> DrawList {
    let mut list = list_of(vec![DrawItem {
        shape: Shape::Mesh(Arc::new(Detailed { level: 0 })),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -distance)),
        material: Material {
            base_color: [0.0; 3],
            emissive: [1.0; 3],
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: true,
        id,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
    }]);
    list.shadows.enabled = false;
    list.sun.intensity = 0.0;
    list
}

/// The distance at which the square is `size` of the screen's height
/// (see `lod::screen_size`), with the cameras of [`list_of`].
fn distance_for(size: f32) -> f32 {
    let radius = Bounds::UNIT.sphere(Mat4::IDENTITY).1;
    radius / (size * (70f32.to_radians() * 0.5).tan())
}

/// A renderer whose triangle counts are only the scene pass's.
fn renderer(device: &wgpu::Device, queue: &wgpu::Queue, size: u32) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, size, size);
    renderer.set_graphics(&with_occlusion(crate::AmbientOcclusion::Off));
    renderer
}

/// Draws `list` for long enough that any switch has finished fading, and
/// gives the triangles of the last frame.
fn settled(renderer: &mut TextureRenderer, list: &DrawList) -> u64 {
    for _ in 0..=FADE_FRAMES {
        renderer.render(list).unwrap();
    }
    renderer.stats().triangles
}

#[test]
fn a_far_model_draws_fewer_triangles_and_switches_back_up_as_the_camera_nears() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, 32);
    let near = square_at(2.0, 1);
    let far = square_at(distance_for(LOD_SCREEN_SIZES[1] * 0.5), 1);
    let middle = square_at(distance_for(LOD_SCREEN_SIZES[0] * 0.7), 1);
    assert_eq!(settled(&mut renderer, &near), Detailed::triangles(0));
    assert_eq!(renderer.stats().lod_items, [1, 0, 0]);
    assert_eq!(settled(&mut renderer, &far), Detailed::triangles(2));
    assert_eq!(renderer.stats().lod_items, [0, 0, 1]);
    assert_eq!(settled(&mut renderer, &middle), Detailed::triangles(1));
    assert_eq!(renderer.stats().lod_items, [0, 1, 0]);
    assert_eq!(settled(&mut renderer, &near), Detailed::triangles(0));

    // While it switches, both levels draw for a few frames, then only the
    // new one. It counts at the level coming in.
    renderer.render(&far).unwrap();
    assert_eq!(
        renderer.stats().triangles,
        Detailed::triangles(0) + Detailed::triangles(2)
    );
    assert_eq!(renderer.stats().lod_items, [0, 0, 1]);
    for _ in 1..FADE_FRAMES {
        renderer.render(&far).unwrap();
        assert_eq!(renderer.stats().items_drawn, 2);
    }
    renderer.render(&far).unwrap();
    assert_eq!(renderer.stats().triangles, Detailed::triangles(2));
}

#[test]
fn a_model_sitting_on_a_threshold_keeps_its_level() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, 32);
    let threshold = LOD_SCREEN_SIZES[0];
    // Wobbling 5% either side of the threshold: no switch from either side.
    let wobble = [0.95, 1.0, 1.05, 1.0].map(|share| square_at(distance_for(threshold * share), 1));
    for (came_from, level) in [(square_at(2.0, 1), 0), (square_at(distance_for(0.2), 1), 1)] {
        settled(&mut renderer, &came_from);
        for frame in 0..60 {
            renderer.render(&wobble[frame % 4]).unwrap();
            let stats = renderer.stats();
            assert_eq!(stats.items_drawn, 1, "frame {frame}: no cross-fade");
            assert_eq!(stats.triangles, Detailed::triangles(level), "frame {frame}");
        }
    }
    // Without an ID there's nothing to remember, so the level follows the size.
    let unnamed = |share: f32| square_at(distance_for(threshold * share), 0);
    renderer.render(&unnamed(1.05)).unwrap();
    assert_eq!(renderer.stats().triangles, Detailed::triangles(0));
    renderer.render(&unnamed(0.95)).unwrap();
    assert_eq!(renderer.stats().triangles, Detailed::triangles(1));
}

#[test]
fn a_previewed_item_draws_at_its_level_whatever_its_size_and_says_so() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, 32);
    let near = square_at(2.0, 7);
    assert_eq!(settled(&mut renderer, &near), Detailed::triangles(0));
    assert_eq!(renderer.lod_level_of(7), Some(0));
    // The editor's preview: at once, with no fade, at any distance.
    renderer.preview_lod(Some((7, 2)));
    renderer.render(&near).unwrap();
    assert_eq!(renderer.stats().triangles, Detailed::triangles(2));
    assert_eq!(renderer.stats().items_drawn, 1);
    // Another item isn't touched, and a level past the last is the last.
    renderer.preview_lod(Some((8, 5)));
    renderer.render(&square_at(2.0, 8)).unwrap();
    assert_eq!(renderer.stats().triangles, Detailed::triangles(2));
    renderer.preview_lod(None);
    assert_eq!(settled(&mut renderer, &near), Detailed::triangles(0));
    assert_eq!(renderer.lod_level_of(99), None, "never drawn");
}

/// The share of the pixels in the middle `side` × `side` of a `size`²
/// image that are white.
fn white_share(pixels: &[u8], size: usize, side: usize) -> f32 {
    let start = (size - side) / 2;
    let mut white = 0;
    for y in start..start + side {
        for x in start..start + side {
            let pixel = &pixels[(y * size + x) * 4..][..3];
            assert!(
                pixel == [255; 3] || pixel == [0; 3],
                "only the square's white or the black sky: {pixel:?}"
            );
            white += usize::from(pixel == [255; 3]);
        }
    }
    white as f32 / (side * side) as f32
}

#[test]
fn a_cross_fade_draws_every_pixel_once_between_the_two_levels() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let size = 128;
    let mut renderer = renderer(&device, &queue, size);
    // Just above the threshold, the square covers well over the middle 8 pixels.
    let above = square_at(distance_for(LOD_SCREEN_SIZES[0] * 1.2), 1);
    let below = square_at(distance_for(LOD_SCREEN_SIZES[0] * 0.8), 1);
    settled(&mut renderer, &above);
    assert_eq!(white_share(&renderer.read_pixels(), size as usize, 8), 1.0);
    for frame in 0..FADE_FRAMES {
        renderer.render(&below).unwrap();
        assert_eq!(renderer.stats().items_drawn, 2, "frame {frame}");
        assert_eq!(
            white_share(&renderer.read_pixels(), size as usize, 8),
            1.0,
            "frame {frame}: no holes"
        );
    }
}

#[test]
fn foliage_fades_out_near_its_cull_distance_and_is_culled_past_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let size = 64;
    let mut renderer = renderer(&device, &queue, size);
    let plant = |distance: f32| {
        let mut list = list_of(vec![DrawItem {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(distance * 0.5),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -distance),
            ),
            material: Material {
                base_color: [0.0; 3],
                emissive: [1.0; 3],
                ..Material::default()
            },
            cull_distance: 100.0,
            ..item(Shape::Block, Vec3::ZERO, 1.0)
        }]);
        list.shadows.enabled = false;
        list.sun.intensity = 0.0;
        list
    };
    renderer.render(&plant(50.0)).unwrap();
    assert_eq!(white_share(&renderer.read_pixels(), size as usize, 16), 1.0);
    // Two thirds of the way through the last tenth: a third still drawn.
    renderer.render(&plant(96.7)).unwrap();
    let share = white_share(&renderer.read_pixels(), size as usize, 16);
    assert!((0.15..0.5).contains(&share), "{share}");
    renderer.render(&plant(101.0)).unwrap();
    assert_eq!(white_share(&renderer.read_pixels(), size as usize, 16), 0.0);
    let stats = renderer.stats();
    assert_eq!((stats.items_drawn, stats.items_culled), (0, 1));
}

#[test]
fn far_ground_cover_thins_with_the_setting_and_comes_back_without_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = renderer(&device, &queue, 32);
    // 400 small plants 80 studs away, with a 100-stud cull distance.
    let items = (1..=400u64)
        .map(|id| DrawItem {
            id,
            cull_distance: 100.0,
            ..item(Shape::Block, Vec3::new(0.0, 0.0, -80.0), 0.5)
        })
        .collect();
    let mut list = list_of(items);
    list.shadows.enabled = false;
    let drawn = |renderer: &mut TextureRenderer, thin: bool| {
        let mut settings = with_occlusion(crate::AmbientOcclusion::Off);
        settings.thin_far_ground_cover = Some(thin);
        renderer.set_graphics(&settings);
        renderer.render(&list).unwrap();
        renderer.stats().items_drawn
    };
    assert_eq!(drawn(&mut renderer, false), 400);
    let thinned = drawn(&mut renderer, true);
    // 80% of the cull distance is most of the way to a third.
    assert!((100..300).contains(&thinned), "{thinned}");
    assert_eq!(drawn(&mut renderer, false), 400);
}
