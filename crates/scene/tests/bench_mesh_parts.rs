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
