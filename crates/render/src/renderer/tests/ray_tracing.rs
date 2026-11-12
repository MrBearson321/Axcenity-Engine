//! Hardware ray tracing's scene (renderer/ray_tracing.rs): built only while
//! something traces rays, rebuilt only when an item changes, within the ray
//! tracing distance, and nothing at all on a GPU without ray queries.

use super::*;

fn skipped() {
    eprintln!("skipped: no GPU with hardware ray tracing on this machine");
}

#[test]
fn a_device_with_ray_queries_says_so_and_one_without_does_not() {
    if let Some((device, queue)) = test_device() {
        let renderer = TextureRenderer::new(&device, &queue, 16, 16);
        assert!(!renderer.capabilities().ray_queries);
    }
    let Some((device, queue)) = ray_test_device() else {
        return skipped();
    };
    let renderer = TextureRenderer::new(&device, &queue, 16, 16);
    assert!(renderer.capabilities().ray_queries);
    assert!(renderer.graphics().ray_queries);
}

#[test]
fn nothing_is_built_while_nothing_traces_rays() {
    let Some((device, queue)) = ray_test_device() else {
        return skipped();
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    renderer.render(&list_of(vec![big_block()])).unwrap();
    assert_eq!(
        renderer.stats().ray_tracing,
        crate::RayTracingStats::default()
    );
}

#[cfg(feature = "debug-views")]
mod ray_hits {
    use super::*;
    use crate::DebugView;

    fn pixel(pixels: &[u8], size: usize, x: usize, y: usize) -> [u8; 3] {
        pixels[(y * size + x) * 4..][..3].try_into().unwrap()
    }

    #[test]
    fn the_scene_is_rebuilt_only_when_an_item_changes_and_freed_after() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_debug_view(Some(DebugView::RayHits));
        let mut list = list_of(vec![
            big_block(),
            item(Shape::Sphere, Vec3::new(2.0, 0.0, -4.0), 1.0),
        ]);
        renderer.render(&list).unwrap();
        let first = renderer.stats().ray_tracing;
        assert_eq!(first.instances, 2, "{first:?}");
        assert_eq!(first.meshes, 2, "a block and a ball: {first:?}");
        assert!(first.triangles > 12, "{first:?}");
        assert!(first.rebuilt);

        // The same items again: nothing to build.
        renderer.render(&list).unwrap();
        assert!(!renderer.stats().ray_tracing.rebuilt);

        // One moves: the TLAS is built again, the meshes aren't.
        list.items[1].transform = Mat4::from_translation(Vec3::new(-2.0, 0.0, -4.0));
        renderer.render(&list).unwrap();
        let moved = renderer.stats().ray_tracing;
        assert!(moved.rebuilt);
        assert_eq!((moved.instances, moved.meshes), (2, 2));

        // A see-through copy of the block shares its mesh.
        list.items
            .push(item(Shape::Block, Vec3::new(0.0, 2.0, -4.0), 0.5));
        renderer.render(&list).unwrap();
        let more = renderer.stats().ray_tracing;
        assert_eq!((more.instances, more.meshes), (3, 2));

        // Back to the lit view: everything is freed.
        renderer.set_debug_view(None);
        renderer.render(&list).unwrap();
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
    }

    #[test]
    fn items_past_the_ray_tracing_distance_are_left_out() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_graphics(&crate::GraphicsSettings {
            ray_tracing_distance: Some(60.0),
            ..crate::GraphicsSettings::default()
        });
        renderer.set_debug_view(Some(DebugView::RayHits));
        // An empty scene traces too: every ray misses.
        renderer.render(&list_of(Vec::new())).unwrap();
        assert_eq!(renderer.stats().ray_tracing.instances, 0);
        let near = big_block();
        let far = item(Shape::Block, Vec3::new(0.0, 0.0, -300.0), 1.0);
        renderer.render(&list_of(vec![near, far])).unwrap();
        assert_eq!(renderer.stats().ray_tracing.instances, 1);
    }

    #[test]
    fn ray_hits_colors_what_rays_hit_and_leaves_the_rest_dark() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        const SIZE: usize = 64;
        let mut renderer = TextureRenderer::new(&device, &queue, SIZE as u32, SIZE as u32);
        // The lit frame, before and after, to show the view leaves no trace.
        let list = list_of(vec![big_block()]);
        renderer.render(&list).unwrap();
        let lit = renderer.read_pixels();

        renderer.set_debug_view(Some(DebugView::RayHits));
        renderer.render(&list).unwrap();
        let hits = renderer.read_pixels();
        // The block fills the middle; the corner sees past it.
        let middle = pixel(&hits, SIZE, 32, 32);
        let corner = pixel(&hits, SIZE, 1, 1);
        assert!(
            middle.iter().map(|&c| u32::from(c)).sum::<u32>() > 150,
            "the block came back as {middle:?}"
        );
        assert!(
            corner.iter().all(|&c| c < 80),
            "a miss came back as {corner:?}"
        );
        assert!(corner[2] >= corner[0], "a miss is blue-grey: {corner:?}");

        renderer.set_debug_view(None);
        renderer.render(&list).unwrap();
        assert_eq!(
            renderer.read_pixels(),
            lit,
            "the lit frame came back changed"
        );
    }

    #[test]
    fn without_ray_queries_the_view_is_plain_and_builds_nothing() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_debug_view(Some(DebugView::RayHits));
        renderer.render(&list_of(vec![big_block()])).unwrap();
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
        let pixels = renderer.read_pixels();
        let first = pixel(&pixels, 32, 0, 0);
        assert!(pixels.chunks(4).all(|p| p[..3] == first), "one plain color");
    }
}

/// Ray traced sun shadows (RT1, renderer/ray_shadows.rs).
mod sun_shadows {
    use super::*;
    use crate::RayTracedShadows;

    fn traced(shadows: RayTracedShadows) -> crate::GraphicsSettings {
        crate::GraphicsSettings {
            // No shadow maps, so any shadow seen is ray traced.
            shadow_quality: Some(ShadowQuality::Off),
            ray_traced_shadows: Some(shadows),
            ..crate::GraphicsSettings::default()
        }
    }

    /// The red channel along the middle row of a 64 × 64 frame.
    fn middle_row(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
        renderer.render(list).unwrap();
        let pixels = renderer.read_pixels();
        (0..64).map(|x| pixels[(32 * 64 + x) * 4]).collect()
    }

    #[test]
    fn ray_traced_shadows_shade_like_the_shadow_maps_without_any_map() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut list = shadowed_wall();
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        // Today's shadow maps, to compare with.
        let mapped = middle_of(&mut renderer, &list);

        renderer.set_graphics(&traced(RayTracedShadows::Low));
        let shaded = middle_of(&mut renderer, &list);
        assert!(renderer.scene.shadow_map.is_none(), "no shadow map is made");
        assert!(renderer.scene.ray_shadows.is_some());
        assert!(renderer.stats().ray_tracing.instances >= 2);
        assert!(
            shaded[0].abs_diff(mapped[0]) <= 24,
            "ray traced {shaded:?}, mapped {mapped:?}"
        );

        list.items[1].casts_shadow = false;
        let lit = middle_of(&mut renderer, &list);
        assert!(lit[0] > shaded[0] + 40, "{shaded:?} vs {lit:?}");
    }

    #[test]
    fn soft_ray_traced_shadows_have_soft_edges_and_hard_ones_do_not() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut list = shadowed_wall();
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::High));
        let hard = middle_row(&mut renderer, &list);
        list.shadows.softness = 1.0;
        let soft = middle_row(&mut renderer, &list);
        // Between the darkest and lightest of the row: the soft edge.
        let between = |row: &[u8]| {
            let (low, high) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
            row.iter()
                .filter(|&&value| value > low + 12 && value + 12 < high)
                .count()
        };
        assert!(
            between(&soft) > between(&hard) + 2,
            "hard {hard:?}\nsoft {soft:?}"
        );
        assert!(*soft.iter().min().unwrap() + 40 < *soft.iter().max().unwrap());
    }

    #[test]
    fn past_their_reach_the_shadow_maps_take_over() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        // The wall is 10 studs away; the world's shadows reach 5.
        let mut list = shadowed_wall();
        list.shadows.distance = 5.0;
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));
        let far = middle_of(&mut renderer, &list);
        list.items[1].casts_shadow = false;
        let lit = middle_of(&mut renderer, &list);
        assert_eq!(far, lit, "no shadow past the reach without shadow maps");
    }

    #[test]
    fn turning_them_off_frees_them_and_draws_as_before() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let list = shadowed_wall();
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.render(&list).unwrap();
        let before = renderer.read_pixels();
        renderer.set_graphics(&traced(RayTracedShadows::Medium));
        renderer.render(&list).unwrap();
        assert!(renderer.stats().buffer_bytes > 64 * 64 * 4);
        renderer.set_graphics(&crate::GraphicsSettings::default());
        renderer.render(&list).unwrap();
        assert!(renderer.scene.ray_shadows.is_none());
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
        assert_eq!(renderer.read_pixels(), before);
    }

    #[test]
    fn a_world_without_shadows_traces_nothing() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut list = shadowed_wall();
        list.shadows.enabled = false;
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::High));
        renderer.render(&list).unwrap();
        assert!(renderer.scene.ray_shadows.is_none());
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
    }

    #[test]
    fn without_ray_queries_the_setting_is_off_with_a_note() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::High));
        assert_eq!(
            renderer.graphics().ray_traced_shadows,
            RayTracedShadows::Off
        );
        assert!(
            renderer
                .graphics()
                .notes
                .iter()
                .any(|note| note.contains("can't ray trace"))
        );
        renderer.render(&shadowed_wall()).unwrap();
        assert!(renderer.scene.ray_shadows.is_none());
    }
}
