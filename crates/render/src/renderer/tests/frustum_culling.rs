//! Frustum culling: what the camera, the shadow views and the sprite pass
//! leave out, and what they must keep.

use super::*;

fn block_at(position: Vec3) -> DrawItem {
    item(Shape::Block, position, 1.0)
}

fn sized_block(size: Vec3, position: Vec3) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        ..block_at(Vec3::ZERO)
    }
}

/// Where each planned scene instance is.
fn drawn_at(plan: &DrawPlan) -> Vec<Vec3> {
    plan.instances
        .iter()
        .map(|raw| Vec3::from_slice(&raw.model[3][..3]))
        .collect()
}

#[test]
fn items_behind_or_beside_the_camera_are_culled_and_ones_crossing_the_edge_are_drawn() {
    // The camera is at the origin looking down -Z, 70° high on a square
    // target, so 10 studs ahead the view's sides are 7 studs out.
    let in_front = block_at(Vec3::new(0.0, 0.0, -10.0));
    let behind = block_at(Vec3::new(0.0, 0.0, 10.0));
    let beside = block_at(Vec3::new(30.0, 0.0, -5.0));
    let past_the_edge = block_at(Vec3::new(9.0, 0.0, -10.0));
    let crossing_the_edge = block_at(Vec3::new(7.3, 0.0, -10.0));
    // Centered behind the camera, but reaching far in front of it.
    let floor = sized_block(Vec3::new(200.0, 1.0, 200.0), Vec3::new(0.0, -3.0, 50.0));
    let glass_behind = item(Shape::Sphere, Vec3::new(0.0, 0.0, 5.0), 0.5);
    let list = list_of(vec![
        in_front,
        behind,
        beside,
        past_the_edge,
        crossing_the_edge,
        floor,
        glass_behind,
    ]);

    let plan = plan(&list);
    assert_eq!(
        drawn_at(&plan),
        [
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(7.3, 0.0, -10.0),
            Vec3::new(0.0, -3.0, 50.0),
        ]
    );
    assert_eq!(plan.culled, 4);
    assert_eq!(plan.batches, [batch(MeshKey::Block, false, 0..3)]);
    // Every solid block can still cast a shadow into the view; the glass
    // casts none.
    assert_eq!(plan.casters.len(), 6);
}

#[test]
fn a_mesh_is_culled_by_the_box_its_corners_span_not_by_its_origin() {
    // Placed behind the camera, but its corners reach 40 studs ahead of
    // its origin, as a terrain chunk's do.
    let list = list_of(vec![item(
        Shape::Mesh(Arc::new(TestMesh(3))),
        Vec3::new(0.0, 0.0, 10.0),
        1.0,
    )]);
    let ahead = Bounds {
        min: Vec3::new(-1.0, -1.0, -51.0),
        max: Vec3::new(1.0, 1.0, -49.0),
    };
    assert_eq!(plan_with(&list, |_| ahead).culled, 0);
    assert_eq!(plan_with(&list, |_| Bounds::UNIT).culled, 1);
}

#[test]
fn a_bounding_sphere_holds_every_corner_of_a_turned_stretched_box() {
    let bounds = Bounds {
        min: Vec3::new(-1.0, 0.0, 2.0),
        max: Vec3::new(3.0, 0.5, 2.5),
    };
    let turns = [
        glam::Quat::IDENTITY,
        glam::Quat::from_rotation_y(0.7),
        glam::Quat::from_euler(glam::EulerRot::XYZ, 0.3, -1.1, 2.0),
    ];
    for turn in turns {
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(4.0, 0.25, 9.0),
            turn,
            Vec3::new(5.0, -2.0, 1.0),
        );
        // A shear too, which a parent's stretch can give.
        let transform = transform
            * Mat4::from_cols_array(&[
                1.0, 0.0, 0.0, 0.0, //
                0.6, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ]);
        let (center, radius) = bounds.sphere(transform);
        let mut farthest: f32 = 0.0;
        for corner in 0..8 {
            let pick = |bit: usize, min: f32, max: f32| if corner & bit == 0 { min } else { max };
            let point = Vec3::new(
                pick(1, bounds.min.x, bounds.max.x),
                pick(2, bounds.min.y, bounds.max.y),
                pick(4, bounds.min.z, bounds.max.z),
            );
            farthest = farthest.max(transform.transform_point3(point).distance(center));
        }
        // It reaches the farthest corner, and no further.
        assert!((radius - farthest).abs() < 1e-3, "{radius} vs {farthest}");
    }
    let (_, radius) = Bounds::UNIT.sphere(Mat4::from_scale(Vec3::splat(2.0)));
    assert!((radius - 3.0_f32.sqrt()).abs() < 1e-5);
}

#[test]
fn an_orthographic_view_culls_outside_its_box() {
    // A box 2 studs across, from 0.1 to 10 studs ahead along -Z.
    let view = Frustum::of(glam::camera::rh::proj::directx::orthographic(
        -1.0, 1.0, -1.0, 1.0, 0.1, 10.0,
    ));
    assert!(view.touches((Vec3::new(0.0, 0.0, -5.0), 0.1)));
    assert!(view.touches((Vec3::new(1.05, 0.0, -5.0), 0.1)), "crossing");
    assert!(!view.touches((Vec3::new(3.0, 0.0, -5.0), 0.1)), "beside");
    assert!(
        !view.touches((Vec3::new(0.0, 0.0, -12.0), 1.0)),
        "past the far end"
    );
    assert!(!view.touches((Vec3::new(0.0, 0.0, 2.0), 1.0)), "behind");
    assert!(view.touches((Vec3::NAN, 1.0)), "never culled by mistake");
}

#[test]
fn uploaded_meshes_know_the_box_their_corners_span_and_follow_a_new_pose() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    struct Posed(u64, [[f32; 3]; 3]);
    impl MeshSource for Posed {
        fn key(&self) -> u64 {
            77
        }
        fn positions(&self) -> &[[f32; 3]] {
            &self.1
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[[0.0, 0.0, 1.0]; 3]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[]
        }
        fn indices(&self) -> &[u32] {
            &[0, 1, 2]
        }
        fn version(&self) -> u64 {
            self.0
        }
    }
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let mesh = |version, corners| {
        let shape = Shape::Mesh(Arc::new(Posed(version, corners)));
        list_of(vec![item(shape, Vec3::new(0.0, 0.0, -5.0), 1.0)])
    };
    renderer
        .render(&mesh(
            0,
            [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, -3.0]],
        ))
        .unwrap();
    assert_eq!(
        renderer.scene.mesh_bounds(MeshKey::Custom(77)),
        Bounds {
            min: Vec3::new(0.0, 0.0, -3.0),
            max: Vec3::new(2.0, 1.0, 0.0),
        }
    );
    // An arm raised past the bind pose's box.
    renderer
        .render(&mesh(
            1,
            [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 4.0, -3.0]],
        ))
        .unwrap();
    assert_eq!(
        renderer.scene.mesh_bounds(MeshKey::Custom(77)).max,
        Vec3::new(2.0, 4.0, 0.0)
    );
    assert_eq!(renderer.scene.mesh_bounds(MeshKey::Block), Bounds::UNIT);
}

#[test]
fn a_caster_outside_the_view_still_shadows_what_is_in_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    // The blocker's shadow falls on the middle of the wall.
    let mut list = shadowed_wall();
    let in_view = middle_of(&mut renderer, &list);
    assert_eq!(renderer.stats().items_culled, 0);

    // Slid toward the sun until it's behind the camera, its shadow falls
    // in the same place.
    list.items[1] = sized_block(Vec3::splat(2.0), Vec3::new(12.0, 0.0, 2.0));
    let out_of_view = middle_of(&mut renderer, &list);
    let stats = renderer.stats();
    assert_eq!((stats.items_drawn, stats.items_culled), (1, 1));

    list.items[1].casts_shadow = false;
    let lit = middle_of(&mut renderer, &list);
    assert!(
        lit[0] > out_of_view[0] + 40,
        "the shadow from out of view is missing: {out_of_view:?} vs {lit:?}"
    );
    assert!(
        out_of_view[0].abs_diff(in_view[0]) <= 8,
        "{out_of_view:?} vs {in_view:?}"
    );
}

#[test]
fn shadow_views_leave_out_casters_they_cannot_reach() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut list = shadowed_wall();
    middle_of(&mut renderer, &list);
    let near = renderer.stats();

    // In view, but far past the shadow distance and away from the sun, so
    // no cascade can hold it or its shadow.
    list.items.push(block_at(Vec3::new(0.0, 0.0, -1000.0)));
    middle_of(&mut renderer, &list);
    let stats = renderer.stats();
    assert_eq!((stats.items_drawn, stats.items_culled), (3, 0));
    let cascades = renderer.graphics().shadow_quality.map().unwrap().cascades;
    assert_eq!(stats.shadow_casters_drawn, near.shadow_casters_drawn);
    assert_eq!(
        stats.shadow_casters_culled,
        near.shadow_casters_culled + cascades,
        "the far block is left out of every cascade"
    );
}

#[test]
fn sprites_out_of_view_are_culled() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut list = list_of(Vec::new());
    list.sprites = vec![red_sprite(-3.0, 0.0), red_sprite(3.0, 0.0)];
    let shown = middle_of(&mut renderer, &list);
    assert!(shown[0] > 200, "the sprite in front still shows: {shown:?}");
    assert_eq!(renderer.stats().sprites_culled, 1);
}

#[test]
fn frozen_culling_keeps_the_frozen_views_items_and_reports_their_bounds() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut list = list_of(vec![
        block_at(Vec3::new(0.0, 0.0, -10.0)),
        block_at(Vec3::new(0.0, 0.0, 10.0)),
    ]);
    renderer.render(&list).unwrap();
    assert_eq!(renderer.stats().items_culled, 1);
    assert!(
        renderer.culling_bounds().is_empty(),
        "kept only while frozen"
    );

    // Frozen looking down -Z, then turned around: still culled as before.
    renderer.freeze_culling(Some(list.camera));
    list.camera.look_at = Vec3::Z;
    renderer.render(&list).unwrap();
    assert_eq!(renderer.stats().items_drawn, 1);
    let bounds: Vec<(f32, bool)> = renderer
        .culling_bounds()
        .iter()
        .map(|bounds| (bounds.center.z, bounds.drawn))
        .collect();
    assert_eq!(bounds, [(-10.0, true), (10.0, false)]);

    renderer.freeze_culling(None);
    renderer.render(&list).unwrap();
    assert_eq!(renderer.stats().items_drawn, 1);
    assert!(renderer.culling_bounds().is_empty());
}
