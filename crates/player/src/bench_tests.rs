//! Rendering benchmarks, measured before anything is optimized. Each
//! builds its scene in code, draws it many times at 1280 × 720 on the High
//! preset, and prints the CPU time of each step and the GPU's time per pass
//! (where the GPU can take timestamps). They're ignored by default:
//!
//! ```text
//! cargo test -p axcenity-player --release -- --ignored bench_ --nocapture --test-threads 1
//! ```
//!
//! The baseline is in `docs/rendering/performance.md`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_core::{
    ClassName, Color3, FoliageInstance, FoliagePlant, FoliageSource, PartType, TerrainData, Value,
    Vector3,
};
use axcenity_render::{
    Camera, DrawList, FramePass, GraphicsSettings, Quality, RenderStats, TextureRenderer, wgpu,
};
use axcenity_scene::TerrainCache;

use super::tests::start_example_in;
use super::*;

const SIZE: (u32, u32) = (1280, 720);
/// Drawing before measuring, for at least this long and this many frames:
/// models load, caches fill, and the GPU clocks up.
const WARM_UP: Duration = Duration::from_secs(1);
const WARM_UP_FRAMES: usize = 30;
/// Frames are measured in batches. Each number printed is the fastest
/// batch's median, so a moment of other work on the machine doesn't count.
const BATCHES: usize = 5;
const BATCH_FRAMES: usize = 60;

/// A GPU device that takes timestamps where the adapter can, on the
/// fastest adapter.
fn bench_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .ok()?;
    let info = adapter.get_info();
    println!(
        "adapter: {} ({:?}, {})",
        info.name, info.backend, info.driver_info
    );
    let required_features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("benchmark device"),
        required_features,
        ..Default::default()
    }))
    .ok()
}

/// The CPU time of a frame's named steps.
#[derive(Default)]
struct Steps(Vec<(&'static str, Duration)>);

impl Steps {
    fn time<T>(&mut self, name: &'static str, step: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let out = step();
        self.0.push((name, start.elapsed()));
        out
    }
}

/// One measured frame: its steps' CPU times and the GPU's times.
struct Measured {
    steps: Vec<(&'static str, Duration)>,
    stats: RenderStats,
}

/// Draws the frames `frame` builds, and prints each step's CPU time, the
/// GPU's frame and pass times, and the last frame's counts.
fn run(name: &str, mut frame: impl FnMut(&mut Steps) -> DrawList) {
    let Some((device, queue)) = bench_gpu() else {
        eprintln!("skipped {name}: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    renderer.set_graphics(&GraphicsSettings::preset(Quality::High));
    let mut draw = || {
        let mut steps = Steps::default();
        let list = frame(&mut steps);
        steps.time("ScenePipeline::draw", || renderer.render(&list).unwrap());
        // Waiting isn't timed: each frame starts with the GPU idle, so
        // frames don't queue up behind each other.
        device.poll(wgpu::PollType::Wait).unwrap();
        Measured {
            steps: steps.0,
            stats: renderer.stats(),
        }
    };
    let started = Instant::now();
    let mut warm = 0;
    while warm < WARM_UP_FRAMES || started.elapsed() < WARM_UP {
        draw();
        warm += 1;
    }
    let frames: Vec<Measured> = (0..BATCHES * BATCH_FRAMES).map(|_| draw()).collect();
    // The fastest batch's median of `time`, or `None` if no frame has one.
    let best = |time: &dyn Fn(&Measured) -> Option<Duration>| {
        frames
            .chunks(BATCH_FRAMES)
            .filter_map(|batch| {
                let mut times: Vec<Duration> = batch.iter().filter_map(time).collect();
                times.sort();
                times.get(times.len() / 2).copied()
            })
            .min()
    };

    let stats = frames.last().unwrap().stats;
    println!("{name} ({} × {}, High)", SIZE.0, SIZE.1);
    println!(
        "  {} of {} items drawn in {} batches; {} draw calls, {} triangles",
        stats.items_drawn, stats.items_submitted, stats.batches, stats.draw_calls, stats.triangles
    );
    let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    println!(
        "  GPU memory: textures {:.1} MB, meshes {:.1} MB, targets {:.1} MB, buffers {:.1} MB",
        mb(stats.texture_bytes),
        mb(stats.mesh_bytes),
        mb(stats.target_bytes),
        mb(stats.buffer_bytes)
    );
    for &(step, _) in &frames[0].steps {
        let time = best(&|frame| {
            let (_, time) = frame.steps.iter().find(|(name, _)| *name == step)?;
            Some(*time)
        });
        println!("  CPU {step:<28} {}", millis(time.unwrap_or_default()));
    }
    if !stats.gpu_timing {
        println!(
            "  GPU times: not available on this adapter
"
        );
        return;
    }
    let frame_time = best(&|frame| frame.stats.gpu_time);
    println!(
        "  GPU {:<28} {}",
        "frame",
        millis(frame_time.unwrap_or_default())
    );
    for pass in FramePass::ALL {
        if let Some(time) = best(&|frame| frame.stats.pass_time(pass)) {
            println!("  GPU   {:<26} {}", pass.label(), millis(time));
        }
    }
    println!();
}

fn millis(time: Duration) -> String {
    format!("{:8.3} ms", time.as_secs_f64() * 1000.0)
}

/// The same numbers every run, so every run draws the same scene.
struct Random(u64);

impl Random {
    /// From 0 up to (not including) 1.
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

/// An anchored part in the Workspace.
fn add_part(world: &mut World, class: ClassName, properties: &[(&str, Value)]) -> InstanceId {
    let part = world.create(class).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    world
        .set_property(part, "Anchored", Value::Bool(true))
        .unwrap();
    for (name, value) in properties {
        world.set_property(part, name, value.clone()).unwrap();
    }
    part
}

fn camera(position: Vec3, look_at: Vec3) -> Camera {
    Camera {
        position,
        look_at,
        fov_y_degrees: 70.0,
    }
}

/// A floor `size` studs across, its top at 0.
fn add_floor(world: &mut World, size: f32) {
    add_part(
        world,
        ClassName::Part,
        &[
            ("Position", vector(0.0, -0.5, 0.0)),
            ("Size", vector(size, 1.0, size)),
            ("Color", Value::Color3(Color3::new(0.35, 0.37, 0.4))),
        ],
    );
}

/// 10,000 parts in a 100 × 100 grid of blocks, spheres, cylinders and
/// wedges of eight colors and many heights, seen whole from above, then
/// from one edge where most of them are out of view (frustum culling,
/// Phase 12, is measured against both).
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_10000_parts() {
    let mut world = World::new();
    add_floor(&mut world, 420.0);
    let shapes = [
        PartType::Block,
        PartType::Ball,
        PartType::Cylinder,
        PartType::Wedge,
    ];
    let mut random = Random(1);
    for index in 0..10_000 {
        let (x, z) = ((index % 100) as f32, (index / 100) as f32);
        let height = 1.0 + random.next() * 5.0;
        let hue = (index % 8) as f32 / 8.0;
        add_part(
            &mut world,
            ClassName::Part,
            &[
                (
                    "Position",
                    vector(x * 4.0 - 198.0, height / 2.0, z * 4.0 - 198.0),
                ),
                ("Size", vector(2.0, height, 2.0)),
                ("Shape", Value::PartType(shapes[index % 4])),
                (
                    "Color",
                    Value::Color3(Color3::new(hue, 1.0 - hue, 0.5 + hue / 2.0)),
                ),
            ],
        );
    }
    let assets = Assets::default();
    for (view, camera) in [
        (
            "overview",
            camera(Vec3::new(0.0, 160.0, 260.0), Vec3::new(0.0, 0.0, 20.0)),
        ),
        (
            "edge, most out of view",
            camera(Vec3::new(-150.0, 6.0, 205.0), Vec3::new(-230.0, 3.0, 150.0)),
        ),
    ] {
        run(&format!("bench_10000_parts: {view}"), |steps| {
            steps.time("build_draw_list", || {
                axcenity_scene::build_draw_list(&world, &assets, camera)
            })
        });
    }
}

/// 2,000 copies of one textured PBR MeshPart (hello-pbr's helmet).
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_2000_mesh_parts() {
    let (mut world, _scripts, folder) = start_example_in("hello-pbr");
    // Only the floor, made wider, and the copies.
    let workspace = world.workspace();
    let floor = world.find_first_child(workspace, "Floor").unwrap();
    for name in ["DamagedHelmet", "PbrPanel", "PhongPanel"] {
        let model = world.find_first_child(workspace, name).unwrap();
        world.destroy(model).unwrap();
    }
    world
        .set_property(floor, "Size", vector(300.0, 1.0, 260.0))
        .unwrap();
    let mut random = Random(2);
    for index in 0..2_000 {
        let (x, z) = ((index % 50) as f32, (index / 50) as f32);
        add_part(
            &mut world,
            ClassName::MeshPart,
            &[
                (
                    "MeshId",
                    Value::String("assets/models/DamagedHelmet/DamagedHelmet.glb".into()),
                ),
                ("Position", vector(x * 6.0 - 147.0, 2.5, z * 6.0 - 117.0)),
                ("Orientation", vector(0.0, random.next() * 360.0, 0.0)),
                ("Size", vector(4.0, 4.0, 4.0)),
            ],
        );
    }
    let assets = Assets::new(&folder);
    let camera = camera(Vec3::new(0.0, 90.0, 190.0), Vec3::new(0.0, 0.0, 10.0));
    run("bench_2000_mesh_parts", |steps| {
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list(&world, &assets, camera)
        })
    });
    assert!(assets.take_errors().is_empty());
}

/// A 2,048-stud terrain of 1,025 × 1,025 samples with rolling hills, two
/// layers and 20,000 plants, seen across from one side.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_terrain_with_foliage() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    world
        .set_property(terrain, "Size", vector(2048.0, 200.0, 2048.0))
        .unwrap();
    for color in [Color3::new(0.3, 0.6, 0.2), Color3::new(0.5, 0.45, 0.35)] {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
        world
            .set_property(layer, "Color", Value::Color3(color))
            .unwrap();
    }
    const SAMPLES: usize = 1025;
    let heights = (0..SAMPLES * SAMPLES)
        .map(|index| {
            let (x, z) = ((index % SAMPLES) as f32, (index / SAMPLES) as f32);
            0.3 + 0.1 * (x / 37.0).sin() * (z / 53.0).cos() + 0.05 * (x / 11.0 + z / 17.0).sin()
        })
        .collect();
    let paint = (0..SAMPLES * SAMPLES)
        .map(|index| {
            let mut weights = [0; axcenity_core::MAX_TERRAIN_LAYERS];
            weights[index % 7 / 4] = 255;
            weights
        })
        .collect();
    let mut data = TerrainData::from_parts(SAMPLES, SAMPLES, heights, paint).unwrap();
    let mut random = Random(3);
    let plants = [
        FoliagePlant::Grass,
        FoliagePlant::Grass,
        FoliagePlant::Grass,
        FoliagePlant::Flower,
        FoliagePlant::Bush,
        FoliagePlant::Tree,
    ];
    let foliage = (0..20_000)
        .map(|index| FoliageInstance {
            source: FoliageSource::Plant(plants[index % plants.len()]),
            x: random.next() * (SAMPLES - 1) as f32,
            z: random.next() * (SAMPLES - 1) as f32,
            yaw: random.next() * 360.0,
            scale: 0.8 + random.next() * 0.4,
            collide: false,
        })
        .collect();
    data.load_foliage(foliage);
    assert!(world.set_terrain(terrain, Arc::new(data)));
    let assets = Assets::default();
    let camera = camera(Vec3::new(0.0, 120.0, 900.0), Vec3::new(0.0, 40.0, 0.0));
    let mut cache = TerrainCache::new();
    run("bench_terrain_with_foliage", |steps| {
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list_posed(
                &world,
                &assets,
                camera,
                &Default::default(),
                &mut cache,
            )
        })
    });
}

/// 20 copies of hello-animation's animated character, each posed on the
/// CPU every frame by its Animator.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_20_animated_characters() {
    let (mut world, _scripts, folder) = start_example_in("hello-animation");
    let noodle = world.find_first_child(world.workspace(), "Noodle").unwrap();
    for index in 1..20 {
        let copy = world.duplicate(noodle).unwrap();
        world.set_parent(copy, Some(world.workspace())).unwrap();
        let (x, z) = ((index % 5) as f32, (index / 5) as f32);
        world
            .set_property(copy, "Position", vector(x * 4.0 - 8.0, 3.8, z * -4.0))
            .unwrap();
    }
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    let camera = camera(Vec3::new(0.0, 10.0, 22.0), Vec3::new(0.0, 3.0, -6.0));
    let mut cache = TerrainCache::new();
    run("bench_20_animated_characters", |steps| {
        steps.time("Animators::update (posing)", || {
            animators.update(&mut world, &assets, 1.0 / 60.0)
        });
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list_posed(
                &world,
                &assets,
                camera,
                animators.poses(),
                &mut cache,
            )
        })
    });
    assert_eq!(animators.poses().len(), 20);
    assert!(animators.take_errors().is_empty());
    assert!(assets.take_errors().is_empty());
}
