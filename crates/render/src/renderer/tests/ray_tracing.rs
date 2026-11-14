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

/// The editor's Path Traced view (LE6, renderer/path_tracer.rs).
#[cfg(feature = "debug-views")]
mod path_traced {
    use super::*;
    use crate::DebugView;

    fn red(pixels: &[u8], x: usize, y: usize) -> u8 {
        pixels[(y * 64 + x) * 4]
    }

    /// Renders `list` path traced `frames` times and gives the last picture.
    fn traced(renderer: &mut TextureRenderer, list: &DrawList, frames: u32) -> Vec<u8> {
        for _ in 0..frames {
            renderer.render(list).unwrap();
        }
        renderer.read_pixels()
    }

    #[test]
    fn the_view_shades_by_each_surfaces_own_normals() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_debug_view(Some(DebugView::PathTraced));
        // A big ball lit from the right: its right side faces the sun, its
        // left side away (rays would see it evenly lit without normals).
        let ball = DrawItem {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -6.0),
            ),
            ..item(Shape::Sphere, Vec3::ZERO, 1.0)
        };
        let mut list = list_of(vec![ball]);
        list.sun.direction = Vec3::X;
        let pixels = traced(&mut renderer, &list, 4);
        let (left, right) = (red(&pixels, 22, 32), red(&pixels, 42, 32));
        assert!(right > left + 60, "left {left}, right {right}");
        assert_eq!(renderer.stats().ray_tracing.path_traced_paths, 4);
    }

    #[test]
    fn shadows_fall_only_from_items_that_cast_them_and_paths_add_up_until_something_changes() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        let mut list = shadowed_wall();
        let lit = traced(&mut renderer, &list, 1);

        renderer.set_debug_view(Some(DebugView::PathTraced));
        let shaded = traced(&mut renderer, &list, 3);
        assert_eq!(renderer.stats().ray_tracing.path_traced_paths, 3);
        // The wall's middle is in the block's shadow; its left edge isn't.
        let (shadow, sunny) = (red(&shaded, 32, 32), red(&shaded, 6, 32));
        assert!(sunny > shadow + 40, "shadow {shadow}, sunny {sunny}");

        // A block that casts no shadow: the middle is lit, and the paths so
        // far (of the old scene) are let go.
        list.items[1].casts_shadow = false;
        let unshaded = traced(&mut renderer, &list, 1);
        assert!(
            red(&unshaded, 32, 32) > shadow + 40,
            "{} vs {shadow}",
            red(&unshaded, 32, 32)
        );
        let stats = renderer.stats().ray_tracing;
        assert_eq!(stats.path_traced_paths, 1, "{stats:?}");

        // The camera moves: the sums start again too.
        traced(&mut renderer, &list, 2);
        assert_eq!(renderer.stats().ray_tracing.path_traced_paths, 3);
        list.camera.position = Vec3::new(0.5, 0.0, 0.0);
        traced(&mut renderer, &list, 1);
        assert_eq!(renderer.stats().ray_tracing.path_traced_paths, 1);

        // Back to the lit view: everything is freed, and the frame is as it
        // was.
        renderer.set_debug_view(None);
        list.camera.position = Vec3::ZERO;
        list.items[1].casts_shadow = true;
        assert_eq!(traced(&mut renderer, &list, 1), lit);
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
    }

    #[test]
    fn a_lamp_alone_lights_the_dark() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_debug_view(Some(DebugView::PathTraced));
        // A floor and a wall, no sun or sky: only a lamp near the floor.
        let block = |size: Vec3, position: Vec3| DrawItem {
            transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
            ..big_block()
        };
        let wall = block(Vec3::new(30.0, 30.0, 1.0), Vec3::new(0.0, 0.0, -10.0));
        let mut list = list_of(vec![wall]);
        list.sun.color = [0.0; 3];
        let dark = traced(&mut renderer, &list, 2);
        assert!(red(&dark, 32, 32) < 8, "{}", red(&dark, 32, 32));
        list.lights.push(crate::Light {
            position: Vec3::new(0.0, 0.0, -6.0),
            color: [1.0; 3],
            brightness: 40.0,
            range: 30.0,
            cone: None,
            rect: None,
            point_shadows: None,
        });
        let lamp_lit = traced(&mut renderer, &list, 2);
        assert!(
            red(&lamp_lit, 32, 32) > 60,
            "the lamp left the wall at {}",
            red(&lamp_lit, 32, 32)
        );
    }

    #[test]
    fn without_ray_queries_the_view_is_plain() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_debug_view(Some(DebugView::PathTraced));
        renderer.render(&list_of(vec![big_block()])).unwrap();
        assert_eq!(
            renderer.stats().ray_tracing,
            crate::RayTracingStats::default()
        );
        let pixels = renderer.read_pixels();
        assert!(
            pixels.chunks(4).all(|p| p[..3] == pixels[..3]),
            "one plain color"
        );
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
    fn a_posed_character_casts_a_ray_traced_shadow_where_its_joints_put_it() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        use super::super::skinning::{skin, square};
        // The wall's blocker is a skinned square whose mesh rests 30 studs
        // to the side: only posed does it stand between the wall and the sun.
        let character = |posed: bool| {
            let offset = Vec3::new(-30.0, 0.0, 0.0);
            let mut item = square(0, posed.then(|| skin(1, offset)));
            item.transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(2.0),
                glam::Quat::IDENTITY,
                Vec3::new(4.0, 0.0, -6.0),
            );
            item.material = Default::default();
            item
        };
        let mut list = shadowed_wall();
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));

        list.items[1] = character(false);
        let lit = middle_of(&mut renderer, &list);
        list.items[1] = character(true);
        let shaded = middle_of(&mut renderer, &list);
        let stats = renderer.stats().ray_tracing;
        assert_eq!((stats.posed, stats.posed_left_out), (1, 0), "{stats:?}");
        assert!(
            lit[0] > shaded[0] + 40,
            "posed {shaded:?} should shade the wall, rest pose {lit:?} not"
        );

        // A new pose moves the shadow with it, the next frame.
        list.items[1] = DrawItem {
            skin: Some(skin(1, Vec3::new(-20.0, 0.0, 0.0))),
            ..character(true)
        };
        let moved = middle_of(&mut renderer, &list);
        assert!(moved[0] > shaded[0] + 40, "{moved:?} vs {shaded:?}");
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
    fn a_soft_shadow_on_a_slope_seen_side_on_is_smooth_not_dithered() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        // The wall turned almost edge-on to the camera: neighbouring pixels
        // lie far apart in depth, though on one surface. (Smoothing used to
        // leave such pixels out, so sloped faces showed the rays'
        // per-pixel pattern as a dither.)
        let mut list = shadowed_wall();
        let turn = glam::Quat::from_rotation_y(80f32.to_radians());
        list.items[0].transform = Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 30.0, 1.0),
            turn,
            Vec3::new(0.0, 0.0, -12.0),
        );
        list.sun.direction = turn * Vec3::new(0.3, 0.0, 1.0);
        list.shadows.softness = 1.0;
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));
        let row = middle_row(&mut renderer, &list);
        let (low, high) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
        assert!(high - low > 30, "a shadow falls across the row: {row:?}");
        // A smooth soft edge goes one way across it; a dithered one turns
        // back and forth from pixel to pixel.
        let steps: Vec<i32> = row
            .windows(2)
            .map(|pair| i32::from(pair[1]) - i32::from(pair[0]))
            .filter(|step| step.abs() > 3)
            .collect();
        let turns = steps
            .windows(2)
            .filter(|pair| pair[0].signum() != pair[1].signum())
            .count();
        assert!(turns <= 6, "dithered: {turns} turns\n{row:?}");
        // And the edge is smoothed into a soft one, as on a surface facing
        // the camera (left unsmoothed it's hard, or noisy with more rays).
        let between = row
            .iter()
            .filter(|&&value| value > low + 8 && value + 8 < high)
            .count();
        assert!(between >= 4, "an unsmoothed edge: {row:?}");
    }

    #[test]
    fn a_cut_out_right_on_a_surface_shades_it_up_close_but_not_far_away() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));
        // Cut-outs are traced as solid cards: one farther off still shades.
        let mut list = shadowed_wall();
        list.items[1].material.alpha_cutoff = 0.5;
        let far = middle_of(&mut renderer, &list);
        list.items[1].casts_shadow = false;
        let open = middle_of(&mut renderer, &list);
        assert!(open[0] > far[0] + 40, "{far:?} vs {open:?}");

        // A thin card standing out of the wall beside the middle (a grass
        // blade at its foot), with the sun low across the wall, about a
        // fifth of a stud from the middle along the ray.
        let mut list = shadowed_wall();
        list.sun.direction = Vec3::new(1.0, 0.0, 0.2);
        list.items[1].transform = Mat4::from_scale_rotation_translation(
            Vec3::new(0.01, 4.0, 0.3),
            glam::Quat::IDENTITY,
            Vec3::new(0.25, 0.0, -9.35),
        );
        list.items[1].material.alpha_cutoff = 0.5;
        let shade = |renderer: &mut TextureRenderer, list: &DrawList| {
            let mut without = list.clone();
            without.items[1].casts_shadow = false;
            (middle_of(renderer, list), middle_of(renderer, &without))
        };
        // Up close (10 studs) it shades the wall at its foot.
        let (beside, without) = shade(&mut renderer, &list);
        assert!(
            without[0] > beside[0] + 40,
            "up close a card shades its foot: {beside:?} vs {without:?}"
        );
        // On a cut-out's own pixel (a blade beside a blade), a card that
        // near isn't counted at any distance: a blade's own card and the
        // ones crossing it at its foot would speckle it.
        let mut blade = list.clone();
        blade.items[0].material.alpha_cutoff = 0.5;
        blade.items[0].material.base_color_texture =
            Some(std::sync::Arc::new(AlphaTexture::new(4, 255)));
        let (beside, without) = shade(&mut renderer, &blade);
        assert!(
            beside[0].abs_diff(without[0]) <= 4,
            "a cut-out isn't shaded by a card on it: {beside:?} vs {without:?}"
        );
        // From 70 studs (through a narrow lens, so the middle stays beside
        // it), a card that near the surface is let off: depth is coarse
        // there, and a card past the bent budget is traced at rest.
        list.camera.position = Vec3::new(0.0, 0.0, 60.0);
        list.camera.fov_y_degrees = 2.0;
        let (beside, without) = shade(&mut renderer, &list);
        assert!(
            beside[0].abs_diff(without[0]) <= 4,
            "far away a card on the surface doesn't blot it: {beside:?} vs {without:?}"
        );
        // A solid block there still shades it: only cut-outs are let off.
        list.items[1].material.alpha_cutoff = 0.0;
        let solid = middle_of(&mut renderer, &list);
        assert!(without[0] > solid[0] + 40, "{solid:?} vs {without:?}");
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

    /// A texture 4 pixels square whose alpha is `alpha` everywhere.
    pub(super) struct AlphaTexture {
        key: u64,
        rgba: Vec<u8>,
    }

    impl AlphaTexture {
        pub(super) fn new(key: u64, alpha: u8) -> Self {
            Self {
                key,
                rgba: [255, 255, 255, alpha].repeat(16),
            }
        }
    }

    impl crate::TextureSource for AlphaTexture {
        fn key(&self) -> u64 {
            self.key
        }
        fn size(&self) -> (u32, u32) {
            (4, 4)
        }
        fn rgba(&self) -> &[u8] {
            &self.rgba
        }
    }

    #[test]
    fn rays_pass_through_a_cut_outs_holes() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        // The wall's blocker made a cut-out, wholly cut out (a leaf card's
        // empty corner) or not at all.
        let blocker_with = |alpha: u8, key: u64| {
            let mut list = shadowed_wall();
            let material = &mut list.items[1].material;
            material.alpha_cutoff = 0.5;
            material.base_color_texture = Some(std::sync::Arc::new(AlphaTexture::new(key, alpha)));
            list
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));
        let solid = middle_of(&mut renderer, &blocker_with(255, 1));
        let holes = middle_of(&mut renderer, &blocker_with(0, 2));
        let mut open = shadowed_wall();
        open.items.truncate(1);
        let lit = middle_of(&mut renderer, &open);
        assert!(
            solid[0] + 40 < lit[0],
            "where it isn't cut out it shades: {solid:?} vs {lit:?}"
        );
        assert!(
            holes[0].abs_diff(lit[0]) <= 6,
            "rays pass where it's cut out: {holes:?} vs {lit:?}"
        );
    }

    /// Up close, a cut-out shades the ground right at its foot: a card
    /// standing out from the wall a third of a stud beside the pixel
    /// (nearer than the old fixed 0.35 studs skipped), between it and the
    /// sun.
    #[test]
    fn a_cut_out_shades_the_ground_at_its_foot_up_close() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut list = shadowed_wall();
        let card = &mut list.items[1];
        card.transform = Mat4::from_scale_rotation_translation(
            Vec3::new(0.05, 2.0, 2.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.325, 0.0, -8.5),
        );
        card.material.alpha_cutoff = 0.5;
        card.material.base_color_texture = Some(std::sync::Arc::new(AlphaTexture::new(3, 255)));
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced(RayTracedShadows::Low));
        let foot = middle_of(&mut renderer, &list);
        let mut open = shadowed_wall();
        open.items.truncate(1);
        let lit = middle_of(&mut renderer, &open);
        assert!(
            foot[0] + 40 < lit[0],
            "the card's foot is in its shadow: {foot:?} vs {lit:?}"
        );
    }
}

/// Ray traced ambient occlusion (RT3, renderer/ray_ao.rs).
/// Lamps' ray traced shadows (LE1): point, spot and rect lights that cast
/// shadows are traced with the sun's, with no shadow map drawn.
mod lamp_shadows {
    use super::*;
    use crate::RayTracedShadows;

    fn traced() -> crate::GraphicsSettings {
        crate::GraphicsSettings {
            // No shadow maps, so any shadow seen is ray traced.
            shadow_quality: Some(ShadowQuality::Off),
            ray_traced_shadows: Some(RayTracedShadows::Low),
            ..crate::GraphicsSettings::default()
        }
    }

    /// `shadowed_wall` lit by `lamp` alone, from in front of the block, so
    /// the block's shadow falls on the wall's middle.
    fn lamp_lit_wall(lamp: Light) -> DrawList {
        let mut list = shadowed_wall();
        list.sun.intensity = 0.0;
        list.ambient.intensity = 0.0;
        list.lights.push(lamp);
        list
    }

    fn lamps() -> Vec<(&'static str, Light)> {
        let base = Light {
            position: Vec3::new(8.0, 0.0, -2.0),
            color: [1.0; 3],
            brightness: 40.0,
            range: 40.0,
            cone: None,
            rect: None,
            point_shadows: Some(crate::PointShadows { near: [0.0; 6] }),
        };
        // Toward the wall's middle, through the block.
        let toward = Vec3::new(0.0, 0.0, -9.5) - base.position;
        vec![
            ("point", base),
            (
                "spot",
                Light {
                    cone: Some(crate::Cone {
                        direction: toward,
                        angle_degrees: 90.0,
                        shadows: Some(crate::SpotShadows { near: 0.0 }),
                    }),
                    point_shadows: None,
                    ..base
                },
            ),
            (
                "rect",
                Light {
                    rect: Some(crate::RectArea {
                        direction: toward,
                        right: Vec3::Y.cross(toward),
                        width: 0.5,
                        height: 0.5,
                    }),
                    ..base
                },
            ),
        ]
    }

    #[test]
    fn point_spot_and_rect_lights_cast_ray_traced_shadows_without_shadow_maps() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced());
        for (kind, lamp) in lamps() {
            let mut list = lamp_lit_wall(lamp);
            let shaded = middle_of(&mut renderer, &list);
            assert!(renderer.scene.spot_shadow_map.is_none());
            assert!(renderer.scene.point_shadow_map.is_none());
            list.items[1].casts_shadow = false;
            let lit = middle_of(&mut renderer, &list);
            assert!(
                lit[0] > shaded[0] + 40,
                "{kind}: shaded {shaded:?}, lit {lit:?}"
            );
        }
    }

    #[test]
    fn a_lamp_without_shadows_traces_none_and_lights_through() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced());
        let (_, lamp) = lamps().remove(0);
        let mut list = lamp_lit_wall(Light {
            point_shadows: None,
            ..lamp
        });
        let through = middle_of(&mut renderer, &list);
        list.items[1].casts_shadow = false;
        let lit = middle_of(&mut renderer, &list);
        assert!(through[0].abs_diff(lit[0]) <= 4, "{through:?} vs {lit:?}");
    }

    #[test]
    fn a_lamps_own_part_does_not_shade_its_light() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&traced());
        let (_, lamp) = lamps().remove(0);
        let mut list = lamp_lit_wall(lamp);
        list.items[1].casts_shadow = false;
        let open = middle_of(&mut renderer, &list);
        // A fixture around the light, reaching under 0.9 studs from it every way:
        // its shadows start past it.
        let mut fixture = list.items[1].clone();
        fixture.casts_shadow = true;
        fixture.transform = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.0),
            glam::Quat::IDENTITY,
            lamp.position,
        );
        list.items.push(fixture);
        list.lights[0].point_shadows = Some(crate::PointShadows { near: [0.9; 6] });
        let fixed = middle_of(&mut renderer, &list);
        assert!(fixed[0].abs_diff(open[0]) <= 6, "{fixed:?} vs {open:?}");
    }
}

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

    /// LE7: rays pass through cut-outs' holes, as they do for shadows.
    #[test]
    fn rays_pass_through_a_cut_outs_holes() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let block_with = |alpha: u8, key: u64| {
            let mut list = block_on_floor();
            let material = &mut list.items[1].material;
            material.alpha_cutoff = 0.5;
            material.base_color_texture = Some(std::sync::Arc::new(
                super::sun_shadows::AlphaTexture::new(key, alpha),
            ));
            list
        };
        let mut renderer = TextureRenderer::new(&device, &queue, PROBE_SIZE, PROBE_SIZE);
        renderer.set_graphics(&traced(
            RayTracedAmbientOcclusion::High,
            AmbientOcclusion::Off,
        ));
        let [solid] = colors(&mut renderer, &block_with(255, 1), &[CORNER])[..] else {
            unreachable!()
        };
        let [holes] = colors(&mut renderer, &block_with(0, 2), &[CORNER])[..] else {
            unreachable!()
        };
        let mut open = block_on_floor();
        open.items.truncate(1);
        let [none] = colors(&mut renderer, &open, &[CORNER])[..] else {
            unreachable!()
        };
        assert!(
            solid[0] + 6 < none[0],
            "where it isn't cut out it hides the corner: {solid:?} vs {none:?}"
        );
        assert!(
            within(holes, none, 2),
            "rays pass where it's cut out: {holes:?} vs {none:?}"
        );
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
        PROBES, SUN_BEHIND_WALL, SUN_ON_WALL, bake, block, difference, probes_held, texels_held,
        with_volume, yard,
    };
    use super::*;
    use crate::RayTracedGlobalIllumination;
    use crate::renderer::light_probes::{CAMERA_PROBES, CAMERA_SPACINGS};

    /// How far apart the camera's finest probes stand.
    const CAMERA_SPACING: f32 = CAMERA_SPACINGS[0];

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
        // Medium traces 256 probes a frame: half through the camera's finest
        // 2,048 and the volume's 8, so each comes round every 16 frames, and
        // half through its coarser grids; two seconds settle them.
        for _ in 0..120 {
            renderer.render(&world).unwrap();
        }
        assert_eq!(renderer.scene.probes_ray_traced(), Some(120 * 256));
        assert_eq!(renderer.scene.probes_traced(), None, "no faces drawn");
        let held = probes_held(&renderer);
        let to_after = difference(&held, &after);
        let to_before = difference(&held, &before);
        // Not all the way: rays light what they hit with the camera's
        // probes too (a further bounce off the floor and wall, which the
        // bake leaves out), so the probes come out a little brighter.
        assert!(
            to_after < 0.35 * change && to_after < 0.5 * to_before,
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
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::Medium));
        // Three times round the camera's finest probes and the volume's.
        for _ in 0..48 {
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

    /// The yard with no light probe volume, seen from `camera`, with a
    /// solid block of 4 studs around `(-4.5, 4.5, 1.5)`: one of the
    /// camera's probes from (-8, 3, 0), whose grid's middle is (-9, 3, 0).
    fn yard_with_block(camera: Vec3) -> DrawList {
        let mut list = yard(SUN_ON_WALL);
        list.items.push(block(
            Vec3::new(-4.5, 4.5, 1.5),
            Vec3::splat(4.0),
            [0.5, 0.5, 0.5],
        ));
        list.camera = Camera {
            position: camera,
            look_at: Vec3::new(0.0, 1.5, 0.0),
            fov_y_degrees: 60.0,
        };
        list
    }

    /// Which of the camera's probes stands at `point`, for a grid whose
    /// middle is `middle`.
    fn camera_probe_at(middle: Vec3, point: Vec3) -> usize {
        let [nx, ny, _] = CAMERA_PROBES.map(|count| count as usize);
        let half = Vec3::from_array(CAMERA_PROBES.map(|count| count as f32)) * 0.5;
        let at = ((point - middle) / CAMERA_SPACING + half - 0.5).round();
        at.x as usize + nx * (at.y as usize + ny * at.z as usize)
    }

    #[test]
    fn without_a_volume_the_camera_brings_probes_and_one_inside_a_block_is_left_out() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let world = yard_with_block(Vec3::new(-8.0, 3.0, 0.0));
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::Medium));
        // Medium traces 256 a frame, half of them near: every one of the
        // finest 2,048 within 16.
        for _ in 0..17 {
            renderer.render(&world).unwrap();
        }
        assert_eq!(renderer.scene.probes_ray_traced(), Some(17 * 256));
        assert!(renderer.stats().ray_tracing.instances > 0);
        let all = texels_held(&renderer);
        assert_eq!(
            all.len(),
            4 * 2048,
            "the camera's grids out to High's 400 studs and no others"
        );
        // The finest grid first.
        let held = &all[..2048];
        let middle = Vec3::new(-9.0, 3.0, 0.0);
        let inside = held[camera_probe_at(middle, Vec3::new(-4.5, 4.5, 1.5))];
        assert_eq!(inside[0][3], 2.0, "inside the block: left out: {inside:?}");
        let open = held[camera_probe_at(middle, Vec3::new(-4.5, 1.5, -4.5))];
        assert_eq!(open[0][3], 0.0, "in the open: used: {open:?}");
        assert!(open[0][..3].iter().all(|&c| c > 0.05), "and lit: {open:?}");
        let left_out = held.iter().filter(|probe| probe[0][3] > 0.5).count();
        assert!(
            left_out < 64,
            "only the few inside things are left out: {left_out}"
        );
    }

    #[test]
    fn the_camera_probes_move_with_it_and_keep_their_light() {
        let Some((device, queue)) = ray_test_device() else {
            return skipped();
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.set_graphics(&traced(RayTracedGlobalIllumination::Medium));
        for _ in 0..17 {
            renderer
                .render(&yard_with_block(Vec3::new(-8.0, 3.0, 0.0)))
                .unwrap();
        }
        let before = texels_held(&renderer);
        // One spacing along +X: the grid's middle goes from -9 to -6.
        renderer
            .render(&yard_with_block(Vec3::new(-8.0 + CAMERA_SPACING, 3.0, 0.0)))
            .unwrap();
        assert_eq!(renderer.scene.light_probes.moves, 1);
        let after = texels_held(&renderer);
        let [nx, ny, nz] = CAMERA_PROBES.map(|count| count as usize);
        let (mut kept, mut untraced) = (0, 0);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let index = x + nx * (y + ny * z);
                    if x + 1 < nx {
                        // Where the probe one along stood before.
                        kept += usize::from(after[index] == before[index + 1]);
                    } else {
                        untraced += usize::from(after[index][0][3] == 2.0);
                    }
                }
            }
        }
        // This frame's batch (128 of the finest grid's probes, a layer
        // along Z) was traced after the move, so those differ.
        assert!(kept >= 1920 - 256, "kept their light: {kept} of 1920");
        assert!(
            untraced >= 128 - 16,
            "the new row starts untraced: {untraced} of 128"
        );
        // Back where it was: copied along again, not started over.
        renderer
            .render(&yard_with_block(Vec3::new(-8.0, 3.0, 0.0)))
            .unwrap();
        assert_eq!(renderer.scene.light_probes.moves, 2);
        assert_eq!(renderer.scene.light_probes.uploads, 1);
    }

    #[test]
    fn without_ray_queries_nothing_is_traced() {
        let Some((device, queue)) = test_device() else {
            return skipped();
        };
        for world in [
            with_volume(&yard(SUN_ON_WALL), vec![[[0.0; 3]; 4]; PROBES as usize]),
            yard(SUN_ON_WALL),
        ] {
            let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
            renderer.set_graphics(&traced(RayTracedGlobalIllumination::High));
            renderer.render(&world).unwrap();
            assert_eq!(renderer.scene.probes_ray_traced(), None);
            let probes = world.light_probe_volumes.len() * PROBES as usize;
            assert_eq!(
                texels_held(&renderer).len(),
                probes.max(1),
                "no camera probes"
            );
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
