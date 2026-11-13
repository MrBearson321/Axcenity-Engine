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

/// Ray traced ambient occlusion (RT3, renderer/ray_ao.rs).
mod ambient_occlusion {
    use super::*;
    use crate::{AmbientOcclusion, RayTracedAmbientOcclusion};

    /// The floor just in front of the block's base (ambient_occlusion.rs's).
    const CORNER: Vec3 = Vec3::new(0.0, 0.0, -7.85);
    const OPEN_FLOOR: Vec3 = Vec3::new(-4.0, 0.0, -4.0);

    fn traced(
        rays: RayTracedAmbientOcclusion,
        screen_space: AmbientOcclusion,
    ) -> crate::GraphicsSettings {
        crate::GraphicsSettings {
            ambient_occlusion: Some(screen_space),
            contact_shadows: Some(false),
            ray_traced_ambient_occlusion: Some(rays),
            ..crate::GraphicsSettings::default()
        }
    }

    /// The colors `points` are drawn in, in a frame of `list` drawn by
    /// `renderer`.
    fn colors(renderer: &mut TextureRenderer, list: &DrawList, points: &[Vec3]) -> Vec<[u8; 3]> {
        renderer.render(list).unwrap();
        let pixels = renderer.read_pixels();
        points
            .iter()
            .map(|point| {
                let clip = view_projection(list, 1.0) * point.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                assert!(
                    ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0,
                    "{point} is on screen"
                );
                let x = ((ndc.x * 0.5 + 0.5) * PROBE_SIZE as f32) as usize;
                let y = ((0.5 - ndc.y * 0.5) * PROBE_SIZE as f32) as usize;
                let at = (y * PROBE_SIZE as usize + x) * 4;
                [pixels[at], pixels[at + 1], pixels[at + 2]]
            })
            .collect()
    }

    #[test]
    fn rays_darken_the_corner_and_leave_open_floor_alone() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let list = block_on_floor();
        let points = [CORNER, OPEN_FLOOR];
        let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
        renderer.set_graphics(&traced(
            RayTracedAmbientOcclusion::Off,
            AmbientOcclusion::Off,
        ));
        let off = colors(&mut renderer, &list, &points);
        for rays in [
            RayTracedAmbientOcclusion::Low,
            RayTracedAmbientOcclusion::High,
        ] {
            // With screen-space ambient occlusion off, the rays alone.
            renderer.set_graphics(&traced(rays, AmbientOcclusion::Off));
            let on = colors(&mut renderer, &list, &points);
            assert!(renderer.scene.ray_ao.is_some());
            assert!(renderer.stats().ray_tracing.instances >= 2);
            assert!(
                on[0][0] + 8 < off[0][0],
                "{rays:?}: the corner darkens: {on:?} vs {off:?}"
            );
            assert!(
                within(on[1], off[1], 2),
                "{rays:?}: open floor is unchanged: {on:?} vs {off:?}"
            );
        }
    }

    #[test]
    fn rays_see_what_is_off_screen_and_screen_space_does_not() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        // Looking straight down at the floor (which shows 7 studs each way),
        // with a wall just past the left edge of the view and the floor
        // beside it just inside. Seen from above, the wall's top is
        // further off screen than its foot.
        let mut list = block_on_floor();
        list.items.truncate(1);
        list.camera = Camera {
            position: Vec3::new(0.0, 10.0, -10.0),
            look_at: Vec3::new(0.0, 0.0, -10.01),
            fov_y_degrees: 70.0,
        };
        let beside = Vec3::new(-6.8, 0.0, -10.0);
        let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
        renderer.set_graphics(&traced(
            RayTracedAmbientOcclusion::Off,
            AmbientOcclusion::Off,
        ));
        let open = colors(&mut renderer, &list, &[beside]);
        let open_frame = renderer.read_pixels();
        list.items.push(DrawItem {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(2.0, 2.0, 8.0),
                glam::Quat::IDENTITY,
                Vec3::new(-8.3, 1.0, -10.0),
            ),
            // Darker than the floor, so it shows if it's on screen.
            material: Material {
                base_color: [0.2; 3],
                roughness: 1.0,
                ..Material::default()
            },
            ..list.items[0].clone()
        });
        let walled = colors(&mut renderer, &list, &[beside]);
        assert_eq!(
            renderer.read_pixels(),
            open_frame,
            "the wall itself is off screen"
        );
        assert_eq!(walled, open);

        // Each way of drawing ambient occlusion, without and with the wall.
        let mut with_and_without = |rays, screen_space| {
            renderer.set_graphics(&traced(rays, screen_space));
            let walled = colors(&mut renderer, &list, &[beside])[0];
            let wall = list.items.pop().unwrap();
            let open = colors(&mut renderer, &list, &[beside])[0];
            list.items.push(wall);
            (walled, open)
        };
        let (walled, open) =
            with_and_without(RayTracedAmbientOcclusion::Off, AmbientOcclusion::High);
        assert!(
            within(walled, open, 2),
            "screen-space can't see the wall: {walled:?} vs {open:?}"
        );
        let (walled, open) =
            with_and_without(RayTracedAmbientOcclusion::High, AmbientOcclusion::Off);
        assert!(
            walled[0] + 6 < open[0],
            "the rays find it: {walled:?} vs {open:?}"
        );
    }

    #[test]
    fn past_the_ray_tracing_distance_the_rays_fade_out() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        // The block 80 studs away; the rays reach 50.
        let mut list = block_on_floor();
        list.camera.position = Vec3::new(0.0, 40.0, 60.0);
        let reach = |rays| crate::GraphicsSettings {
            ray_tracing_distance: Some(50.0),
            ..traced(rays, AmbientOcclusion::Off)
        };
        let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
        renderer.set_graphics(&reach(RayTracedAmbientOcclusion::Off));
        renderer.render(&list).unwrap();
        let off = renderer.read_pixels();
        renderer.set_graphics(&reach(RayTracedAmbientOcclusion::High));
        renderer.render(&list).unwrap();
        assert!(renderer.scene.ray_ao.is_some());
        assert_eq!(renderer.read_pixels(), off);
    }

    #[test]
    fn turning_it_off_frees_it_and_draws_as_before() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let list = block_on_floor();
        let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
        renderer.render(&list).unwrap();
        let before = renderer.read_pixels();
        renderer.set_graphics(&traced(
            RayTracedAmbientOcclusion::Medium,
            AmbientOcclusion::Low,
        ));
        renderer.render(&list).unwrap();
        assert_ne!(renderer.read_pixels(), before);
        renderer.set_graphics(&crate::GraphicsSettings::default());
        renderer.render(&list).unwrap();
        assert!(renderer.scene.ray_ao.is_none());
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
        assert_eq!(renderer.read_pixels(), before);
    }
}

mod global_illumination {
    use super::super::realtime_gi::{
        PROBES, SUN_BEHIND_WALL, SUN_ON_WALL, bake, difference, probes_held, with_volume, yard,
    };
    use super::*;
    use crate::RayTracedGlobalIllumination;

    fn traced(gi: RayTracedGlobalIllumination) -> crate::GraphicsSettings {
        crate::GraphicsSettings {
            shadow_quality: Some(crate::ShadowQuality::Medium),
            ray_traced_global_illumination: Some(gi),
            ..crate::GraphicsSettings::default()
        }
    }

    #[test]
    fn a_moving_sun_relights_the_probes_with_rays_toward_a_fresh_bake() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let lit = yard(SUN_ON_WALL);
        let moved = yard(SUN_BEHIND_WALL);
        let (before, after) = (bake(&device, &queue, &lit), bake(&device, &queue, &moved));
        let change = difference(&before, &after);
        // The world still holds the bake from before the sun moved.
        let world = with_volume(&moved, before.clone());
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::Medium));
        // Medium traces all 8 probes each frame; a second's worth settles.
        for _ in 0..60 {
            renderer.render(&world).unwrap();
        }
        assert_eq!(
            renderer.scene.probes_ray_traced(),
            Some(60 * u64::from(PROBES))
        );
        assert_eq!(renderer.scene.probes_traced(), None, "no faces drawn");
        let held = probes_held(&renderer);
        let to_after = difference(&held, &after);
        let to_before = difference(&held, &before);
        assert!(
            to_after < 0.2 * change && to_after < to_before,
            "near the fresh bake ({to_after}), not the old one ({to_before}); the sun \
             moved {change}"
        );

        // Turning it off frees it and brings the bake back.
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::Off));
        renderer.render(&world).unwrap();
        assert_eq!(renderer.scene.probes_ray_traced(), None);
        assert_eq!(probes_held(&renderer), before);
        assert!(renderer.stats().pass_time(FramePass::RayTracedGi).is_none());
    }

    #[test]
    fn the_red_wall_bounces_red_light_toward_the_probes_beside_it() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let lit = yard(SUN_ON_WALL);
        // Starting from nothing, not a bake.
        let world = with_volume(&lit, vec![[[0.0; 3]; 4]; PROBES as usize]);
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::High));
        for _ in 0..40 {
            renderer.render(&world).unwrap();
        }
        let held = probes_held(&renderer);
        for probe in &held {
            let [c0, _, _, c3] = *probe;
            assert!(
                c0.iter().all(|&c| c > 0.05),
                "lit from all around: {probe:?}"
            );
            // Band 1's X coefficient: more light from +X (the sunlit red
            // wall) than -X, and redder than it is green.
            assert!(c3[0] > 0.0 && c3[0] > c3[1], "red from the wall: {probe:?}");
        }
    }

    #[test]
    fn without_light_probe_volumes_or_ray_queries_nothing_is_traced() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::High));
        renderer.render(&yard(SUN_ON_WALL)).unwrap();
        assert_eq!(renderer.scene.probes_ray_traced(), None);
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default(),
            "no volumes: no ray traced scene either"
        );
        if let Some((device, queue)) = test_device() {
            let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
            renderer.set_graphics(&traced(RayTracedGlobalIllumination::High));
            let world = with_volume(&yard(SUN_ON_WALL), vec![[[0.0; 3]; 4]; PROBES as usize]);
            renderer.render(&world).unwrap();
            assert_eq!(renderer.scene.probes_ray_traced(), None);
            assert!(
                renderer
                    .graphics()
                    .notes
                    .iter()
                    .any(|note| note.contains("GI is off"))
            );
        }
    }
}
