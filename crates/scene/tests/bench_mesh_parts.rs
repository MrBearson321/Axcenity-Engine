//! The CPU side of the player's `bench_2000_mesh_parts`, with no GPU: how
//! long `build_draw_list_posed` takes for 2,000 copies of hello-pbr's
//! helmet, with a `TerrainCache` kept across frames as hosts keep theirs.
//! Each frame's draw list is dropped before the next, as a host's is.
//! Ignored by default:
//!
//! ```text
//! cargo test -p axcenity-scene --release --test bench_mesh_parts -- --ignored --nocapture
//! ```

use std::path::Path;
use std::time::{Duration, Instant};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Value, Vector3, World};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use axcenity_scene::{Poses, TerrainCache, build_draw_list_posed};

const HELMET: &str = "assets/models/DamagedHelmet/DamagedHelmet.glb";
const WARM_UP_FRAMES: usize = 30;
/// Each number printed is the fastest batch's median.
const BATCHES: usize = 5;
const BATCH_FRAMES: usize = 60;

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn bench_build_2000_mesh_parts() {
    let mut world = World::new();
    let workspace = world.workspace();
    for index in 0..2_000 {
        let (x, z) = ((index % 50) as f32, (index / 50) as f32);
        let part = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        for (name, value) in [
            ("MeshId", Value::String(HELMET.into())),
            (
                "Position",
                Value::Vector3(Vector3::new(x * 6.0 - 147.0, 2.5, z * 6.0 - 117.0)),
            ),
            (
                "Orientation",
                Value::Vector3(Vector3::new(0.0, (index * 37 % 360) as f32, 0.0)),
            ),
            ("Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0))),
        ] {
            world.set_property(part, name, value).unwrap();
        }
    }
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let assets = Assets::new(examples.join("hello-pbr"));
    let camera = Camera {
        position: Vec3::new(0.0, 90.0, 190.0),
        look_at: Vec3::new(0.0, 0.0, 10.0),
        fov_y_degrees: 70.0,
    };
    let mut cache = TerrainCache::new();
    let poses = Poses::new();
    let mut frame = || {
        let start = Instant::now();
        let list = build_draw_list_posed(&world, &assets, camera, &poses, &mut cache);
        assert_eq!(list.items.len(), 2_000);
        drop(list);
        start.elapsed()
    };
    for _ in 0..WARM_UP_FRAMES {
        frame();
    }
    let median = (0..BATCHES)
        .map(|_| {
            let mut times: Vec<Duration> = (0..BATCH_FRAMES).map(|_| frame()).collect();
            times.sort();
            times[BATCH_FRAMES / 2]
        })
        .min()
        .unwrap();
    println!(
        "bench_build_2000_mesh_parts: build_draw_list {:.3} ms",
        median.as_secs_f64() * 1000.0
    );
    assert!(assets.take_errors().is_empty());
}

/// 10,000 parts timed as hosts draw them (with a cache kept across frames),
/// with no GPU: all still, then with 1% moved each frame. Set
/// `AXCENITY_RETAINED_DRAW_LIST=off` to time building every frame from
/// scratch instead of retained draw items.
#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn bench_build_10000_parts() {
    let mut world = World::new();
    let workspace = world.workspace();
    let parts: Vec<_> = (0..10_000)
        .map(|index| {
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(workspace)).unwrap();
            let (x, z) = ((index % 100) as f32, (index / 100) as f32);
            world
                .set_property(
                    part,
                    "Position",
                    Value::Vector3(Vector3::new(x * 4.0 - 198.0, 1.0, z * 4.0 - 198.0)),
                )
                .unwrap();
            part
        })
        .collect();
    let assets = Assets::default();
    let camera = Camera {
        position: Vec3::new(0.0, 160.0, 260.0),
        look_at: Vec3::new(0.0, 0.0, 20.0),
        fov_y_degrees: 70.0,
    };
    let mut cache = TerrainCache::new();
    let poses = Poses::new();
    let build = |world: &World, cache: &mut TerrainCache| {
        let start = Instant::now();
        let list = build_draw_list_posed(world, &assets, camera, &poses, cache);
        assert_eq!(list.items.len(), 10_000);
        drop(list);
        start.elapsed()
    };
    for _ in 0..WARM_UP_FRAMES {
        build(&world, &mut cache);
    }
    let fastest_median = |times: &mut Vec<Vec<Duration>>| {
        times
            .iter_mut()
            .map(|batch| {
                batch.sort();
                batch[BATCH_FRAMES / 2]
            })
            .min()
            .unwrap()
    };
    let mut still: Vec<Vec<Duration>> = (0..BATCHES)
        .map(|_| {
            (0..BATCH_FRAMES)
                .map(|_| build(&world, &mut cache))
                .collect()
        })
        .collect();
    let mut step = 0_usize;
    let mut moving: Vec<Vec<Duration>> = (0..BATCHES)
        .map(|_| {
            (0..BATCH_FRAMES)
                .map(|_| {
                    for _ in 0..100 {
                        step += 37;
                        let part = parts[step % parts.len()];
                        let height = 1.0 + (step % 7) as f32;
                        world
                            .set_property(
                                part,
                                "Position",
                                Value::Vector3(Vector3::new(
                                    (step % 100) as f32 * 4.0 - 198.0,
                                    height,
                                    (step % 97) as f32 * 4.0 - 198.0,
                                )),
                            )
                            .unwrap();
                    }
                    build(&world, &mut cache)
                })
                .collect()
        })
        .collect();
    println!(
        "bench_build_10000_parts: still {:.3} ms, 1% moving {:.3} ms",
        fastest_median(&mut still).as_secs_f64() * 1000.0,
        fastest_median(&mut moving).as_secs_f64() * 1000.0
    );
}
