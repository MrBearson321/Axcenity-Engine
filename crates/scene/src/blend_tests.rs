//! Moving parts drawn between physics steps (roadmap G3): see `blend.rs`.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_render::glam::{EulerRot, Quat, Vec3};
use axcenity_render::{Camera, DrawItem, DrawList};

use crate::{Blends, Poses, TerrainCache, build_draw_list_posed};

fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 20.0, 60.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    }
}

fn part(world: &mut World, position: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    id
}

/// Both ways a host draws: keeping items between frames and not.
fn caches() -> [TerrainCache; 2] {
    let mut kept = TerrainCache::new();
    kept.set_retained_draw_list(true);
    let mut fresh = TerrainCache::new();
    fresh.set_retained_draw_list(false);
    [kept, fresh]
}

fn draw(world: &World, cache: &mut TerrainCache) -> DrawList {
    build_draw_list_posed(world, &Assets::default(), camera(), &Poses::new(), cache)
}

/// The `nth` part's item: plain parts' items have no ID, and come in the
/// order the parts were made.
fn item(list: &DrawList, nth: usize) -> &DrawItem {
    list.items.get(nth).expect("the part is drawn")
}

fn at(item: &DrawItem) -> Vec3 {
    item.transform.to_scale_rotation_translation().2
}

const ZERO: Vector3 = Vector3::new(0.0, 0.0, 0.0);

#[test]
fn a_falling_part_is_drawn_the_share_of_the_way_from_before_its_step() {
    for mut cache in caches() {
        let mut world = World::new();
        let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0));
        let stepped = draw(&world, &mut cache);

        let mut blends = Blends::new(0.25);
        blends.add(
            block,
            (Vector3::new(0.0, 12.0, 0.0), ZERO),
            (Vector3::new(0.0, 10.0, 0.0), ZERO),
        );
        cache.set_blends(blends);
        let drawn = draw(&world, &mut cache);
        assert!((at(item(&drawn, 0)) - Vec3::new(0.0, 11.5, 0.0)).length() < 1e-4);
        // Only where it's drawn: its size and look are the same.
        let (stepped_item, drawn_item) = (item(&stepped, 0), item(&drawn, 0));
        assert_eq!(
            stepped_item.transform.to_scale_rotation_translation().0,
            drawn_item.transform.to_scale_rotation_translation().0
        );
        assert_eq!(
            stepped_item.material.base_color,
            drawn_item.material.base_color
        );

        // With none, it's drawn where the world has it again: the kept item
        // was never moved.
        cache.set_blends(Blends::default());
        let after = draw(&world, &mut cache);
        assert_eq!(item(&after, 0).transform, stepped_item.transform);
    }
}

#[test]
fn a_turning_part_is_drawn_turned_part_of_the_way() {
    for mut cache in caches() {
        let mut world = World::new();
        let block = part(&mut world, ZERO);
        let turned = Vector3::new(0.0, 90.0, 0.0);
        world
            .set_property(block, "Orientation", Value::Vector3(turned))
            .unwrap();
        let mut blends = Blends::new(0.5);
        blends.add(block, (ZERO, ZERO), (ZERO, turned));
        cache.set_blends(blends);
        let drawn = draw(&world, &mut cache);
        let rotation = item(&drawn, 0).transform.to_scale_rotation_translation().1;
        let (yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
        assert!((yaw.to_degrees() - 45.0).abs() < 0.01, "{rotation:?}");
        assert!(at(item(&drawn, 0)).length() < 1e-4);
    }
}

#[test]
fn a_part_a_script_moved_after_the_step_is_drawn_where_it_is() {
    for mut cache in caches() {
        let mut world = World::new();
        let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0));
        let mut blends = Blends::new(0.5);
        blends.add(
            block,
            (Vector3::new(0.0, 12.0, 0.0), ZERO),
            (Vector3::new(0.0, 10.0, 0.0), ZERO),
        );
        cache.set_blends(blends);
        // A teleport: no sliding back from where it was.
        world
            .set_property(
                block,
                "Position",
                Value::Vector3(Vector3::new(50.0, 3.0, 0.0)),
            )
            .unwrap();
        let drawn = draw(&world, &mut cache);
        assert!((at(item(&drawn, 0)) - Vec3::new(50.0, 3.0, 0.0)).length() < 1e-4);
    }
}

#[test]
fn a_light_on_a_blended_part_moves_with_it_and_other_parts_stay() {
    for mut cache in caches() {
        let mut world = World::new();
        let block = part(&mut world, Vector3::new(0.0, 10.0, 0.0));
        part(&mut world, Vector3::new(8.0, 0.0, 0.0));
        let light = world.create(ClassName::PointLight).unwrap();
        world.set_parent(light, Some(block)).unwrap();
        let stepped = draw(&world, &mut cache);
        let mut blends = Blends::new(0.0);
        blends.add(
            block,
            (Vector3::new(0.0, 12.0, 0.0), ZERO),
            (Vector3::new(0.0, 10.0, 0.0), ZERO),
        );
        cache.set_blends(blends);
        let drawn = draw(&world, &mut cache);
        assert_eq!(drawn.lights.len(), 1);
        let lifted = drawn.lights[0].position - stepped.lights[0].position;
        assert!(
            (lifted - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-4,
            "{lifted:?}"
        );
        assert_eq!(item(&drawn, 1).transform, item(&stepped, 1).transform);
    }
}

#[test]
fn blends_rotate_about_the_part_not_the_origin() {
    let mut world = World::new();
    let far = Vector3::new(100.0, 0.0, 0.0);
    let block = part(&mut world, far);
    let mut cache = TerrainCache::new();
    let mut blends = Blends::new(0.5);
    blends.add(block, (far, Vector3::new(0.0, 90.0, 0.0)), (far, ZERO));
    cache.set_blends(blends);
    let drawn = draw(&world, &mut cache);
    let (_, rotation, position) = item(&drawn, 0).transform.to_scale_rotation_translation();
    assert!(
        (position - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-3,
        "{position:?}"
    );
    assert!(rotation.angle_between(Quat::from_rotation_y(45f32.to_radians())) < 1e-3);
}

/// How much blending adds to building the draw list: 10,000 parts, 1,000
/// of them blended (S10k's moving drops). Run with
/// `cargo test -p axcenity-scene --release blend_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn blend_cost() {
    use std::time::Instant;
    let mut world = World::new();
    let mut moving = Vec::new();
    for i in 0..10_000 {
        let at = Vector3::new((i % 100) as f32 * 4.0, 1.0, (i / 100) as f32 * 4.0);
        let id = part(&mut world, at);
        if i % 10 == 0 {
            moving.push((id, at));
        }
    }
    let blends = || {
        let mut blends = Blends::new(0.5);
        for &(id, at) in &moving {
            let from = Vector3::new(at.x, at.y + 1.0, at.z);
            blends.add(id, (from, Vector3::new(10.0, 20.0, 0.0)), (at, ZERO));
        }
        blends
    };
    let mut cache = TerrainCache::new();
    cache.set_retained_draw_list(true);
    let frames = 200;
    for with in [false, true, false, true] {
        let started = Instant::now();
        for _ in 0..frames {
            cache.set_blends(if with { blends() } else { Blends::default() });
            std::hint::black_box(draw(&world, &mut cache));
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        println!("blends {with}: {ms:.3} ms a frame");
    }
}
