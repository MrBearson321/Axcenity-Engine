//! A frame's planning shared among the CPU's threads, the cut-out surfaces'
//! depth fill, and the far cascades taking turns in a heavy scene.

use super::*;

/// A scene big enough to be shared out, with a run of see-through items and
/// several materials and shapes, so that batches cross where the shares end.
fn many_items(count: usize) -> DrawList {
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        let shape = if i % 7 == 0 {
            Shape::Sphere
        } else {
            Shape::Block
        };
        let position = Vec3::new(
            (i % 40) as f32 - 20.0,
            (i % 13) as f32 - 6.0,
            -((i % 97) as f32) - 5.0,
        );
        let opacity = if i % 5 == 0 { 0.5 } else { 1.0 };
        let mut item = item(shape, position, opacity);
        item.material.base_color = [(i % 3) as f32 * 0.4, (i % 2) as f32, 1.0];
        items.push(item);
    }
    list_of(items)
}

#[test]
fn planning_comes_out_the_same_however_many_threads_share_it() {
    let list = many_items(5000);
    // What a plan says, without the skins' addresses, which can't go
    // between threads.
    let plan_it = || {
        let plan = plan_with(&list, |_| Bounds::UNIT);
        let bytes: Vec<u8> = plan
            .instances
            .iter()
            .flatten()
            .flat_map(|raw| bytemuck::bytes_of(raw).to_vec())
            .collect();
        let casters = plan.casters.iter().flatten().count();
        (plan.batches, bytes, casters)
    };
    let shared = plan_it();
    let one_thread = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(plan_it);
    assert_eq!(shared.0, one_thread.0);
    assert_eq!(shared.1, one_thread.1);
    assert_eq!(shared.2, one_thread.2);
    let drawn: usize = shared.0.iter().map(|batch| batch.instances.len()).sum();
    assert_eq!(drawn * size_of::<InstanceRaw>(), shared.1.len());
}

/// A block glowing `color`, its left half solid and its right half cut out,
/// `z` studs along.
fn half_cut_block(z: f32, color: [f32; 3]) -> DrawItem {
    DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, z),
        ),
        material: Material {
            base_color: [0.0; 3],
            emissive: color,
            base_color_texture: Some(Arc::new(HalfCut)),
            alpha_cutoff: 0.5,
            ..Material::default()
        },
        ..big_block()
    }
}

#[test]
fn a_cut_out_surface_shows_itself_where_solid_and_what_is_behind_through_its_holes() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let front = half_cut_block(-5.0, [0.0, 1.0, 0.0]);
    let behind = DrawItem {
        material: Material {
            base_color: [0.0; 3],
            emissive: [1.0, 0.0, 0.0],
            ..Material::default()
        },
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -9.0),
        ),
        ..big_block()
    };
    let mut list = list_of(vec![front, behind]);
    list.shadows.enabled = false;
    let face = |x: f32| Vec3::new(x, 0.0, -3.0);
    for anti_aliasing in [crate::AntiAliasing::Off, crate::AntiAliasing::Msaa4] {
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.set_graphics(&GraphicsSettings {
            anti_aliasing: Some(anti_aliasing),
            ..GraphicsSettings::default()
        });
        let solid = pixel_at(&mut renderer, &list, face(-1.2));
        // Near the middle: the block behind looks smaller, being farther away.
        let hole = pixel_at(&mut renderer, &list, face(0.6));
        assert!(solid[1] > 150 && solid[0] < 80, "{solid:?}");
        assert!(hole[0] > 150 && hole[1] < 80, "{hole:?}");
    }
}

#[test]
fn the_far_cascades_of_a_heavy_scene_take_turns_and_a_small_scene_draws_them_all() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let casters = 13_000;
    // All within a stud or two of each other just ahead, inside every
    // cascade.
    let block = || item(Shape::Block, Vec3::new(0.0, 0.0, -4.0), 1.0);
    let heavy = list_of((0..casters).map(|_| block()).collect());
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let drawn = |renderer: &mut TextureRenderer| {
        renderer.render(&heavy).unwrap();
        renderer.stats().shadow_casters_drawn
    };
    // High has three cascades. The first frame doesn't know the scene is
    // heavy yet and draws them all, then the far two take turns.
    assert_eq!(drawn(&mut renderer), casters * 3);
    assert_eq!(drawn(&mut renderer), casters * 2);
    assert_eq!(drawn(&mut renderer), casters * 2);

    let small = list_of((0..100).map(|_| block()).collect());
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    for _ in 0..3 {
        renderer.render(&small).unwrap();
        assert_eq!(renderer.stats().shadow_casters_drawn, 300);
    }
}
