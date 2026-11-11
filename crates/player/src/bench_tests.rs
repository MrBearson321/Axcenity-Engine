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

use std::path::PathBuf;
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
/// fastest adapter. It asks for BC compression as the player's own device
/// does, so cooked textures go up as their blocks here too; without it they
/// were decoded from their source files and the tools measured and drew
/// the uncooked path.
pub(crate) fn bench_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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
    let required_features = adapter.features()
        & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TEXTURE_COMPRESSION_BC);
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
/// GPU's frame and pass times, and the last frame's counts. Returns the
/// last frame's counts and the GPU's frame time (where it can be timed).
fn run(
    name: &str,
    mut frame: impl FnMut(&mut Steps) -> DrawList,
) -> Option<(RenderStats, Option<Duration>)> {
    let Some((device, queue)) = bench_gpu() else {
        eprintln!("skipped {name}: no graphics adapter on this machine");
        return None;
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
    println!(
        "  culled: {} items, {} sprites; shadow casters {} drawn, {} culled (summed over views)",
        stats.items_culled,
        stats.sprites_culled,
        stats.shadow_casters_drawn,
        stats.shadow_casters_culled
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
        return Some((stats, None));
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
    Some((stats, frame_time))
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
        // Kept across frames, as a host keeps one.
        let mut cache = TerrainCache::new();
        let poses = axcenity_scene::Poses::new();
        run(&format!("bench_10000_parts: {view}"), |steps| {
            steps.time("build_draw_list", || {
                axcenity_scene::build_draw_list_posed(&world, &assets, camera, &poses, &mut cache)
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
    // Kept across frames, as the player and the editor keep theirs.
    let mut cache = TerrainCache::new();
    let poses = axcenity_scene::Poses::new();
    run("bench_2000_mesh_parts", |steps| {
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list_posed(&world, &assets, camera, &poses, &mut cache)
        })
    });
    assert!(assets.take_errors().is_empty());
}

/// The case occlusion culling is for (graphics Phase 15): 2,000 helmets in
/// view but hidden behind a wall the camera stands close to, against the
/// same view with no helmets. The difference is the most that culling
/// hidden items could save.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_2000_mesh_parts_behind_a_wall() {
    for helmets in [true, false] {
        let (mut world, _scripts, folder) = start_example_in("hello-pbr");
        let workspace = world.workspace();
        let floor = world.find_first_child(workspace, "Floor").unwrap();
        for name in ["DamagedHelmet", "PbrPanel", "PhongPanel"] {
            let model = world.find_first_child(workspace, name).unwrap();
            world.destroy(model).unwrap();
        }
        world
            .set_property(floor, "Size", vector(400.0, 1.0, 400.0))
            .unwrap();
        add_part(
            &mut world,
            ClassName::Part,
            &[
                ("Position", vector(0.0, 40.0, 0.0)),
                ("Size", vector(400.0, 80.0, 2.0)),
                ("Color", Value::Color3(Color3::new(0.7, 0.68, 0.62))),
            ],
        );
        let mut random = Random(2);
        for index in 0..if helmets { 2_000 } else { 0 } {
            let (x, z) = ((index % 50) as f32, (index / 50) as f32);
            add_part(
                &mut world,
                ClassName::MeshPart,
                &[
                    (
                        "MeshId",
                        Value::String("assets/models/DamagedHelmet/DamagedHelmet.glb".into()),
                    ),
                    ("Position", vector(x * 6.0 - 147.0, 2.5, -10.0 - z * 6.0)),
                    ("Orientation", vector(0.0, random.next() * 360.0, 0.0)),
                    ("Size", vector(4.0, 4.0, 4.0)),
                ],
            );
        }
        let assets = Assets::new(&folder);
        let camera = camera(Vec3::new(0.0, 8.0, 40.0), Vec3::new(0.0, 6.0, -100.0));
        let view = if helmets { "hidden" } else { "removed" };
        run(
            &format!("bench_2000_mesh_parts_behind_a_wall: {view}"),
            |steps| {
                steps.time("build_draw_list", || {
                    axcenity_scene::build_draw_list(&world, &assets, camera)
                })
            },
        );
        assert!(assets.take_errors().is_empty());
    }
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
    animated_characters(
        "bench_20_animated_characters",
        "hello-animation",
        "Noodle",
        3.8,
    );
}

/// 20 copies of hello-character's walking character, a full rig of about
/// 30,000 triangles, each posed on the CPU every frame by its Animator.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_20_walking_characters() {
    animated_characters(
        "bench_20_walking_characters",
        "hello-character",
        "Walking",
        1.9,
    );
}

/// Draws an example's animated `character` and 19 copies of it in a grid at
/// height `y`, posing each on the CPU every frame.
fn animated_characters(bench: &str, example: &str, character: &str, y: f32) {
    let (mut world, _scripts, folder) = start_example_in(example);
    let original = world
        .find_first_child(world.workspace(), character)
        .unwrap();
    for index in 1..20 {
        let copy = world.duplicate(original).unwrap();
        world.set_parent(copy, Some(world.workspace())).unwrap();
        let (x, z) = ((index % 5) as f32, (index / 5) as f32);
        world
            .set_property(copy, "Position", vector(x * 4.0 - 8.0, y, z * -4.0))
            .unwrap();
    }
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    let camera = camera(Vec3::new(0.0, 10.0, 22.0), Vec3::new(0.0, 3.0, -6.0));
    let mut cache = TerrainCache::new();
    run(bench, |steps| {
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
    assert!(animators.poses().len() >= 20);
    assert!(animators.take_errors().is_empty());
    assert!(assets.take_errors().is_empty());
}

/// One emitter keeping `out` particles alive (Rate × Lifetime), moved and
/// sorted on the CPU, then on the GPU, to find the count above which the
/// GPU is faster (`axcenity_scene::GPU_PARTICLE_THRESHOLD`, graphics Phase
/// 36). The emitter fills a cloud in front of the camera. With the GPU
/// allowed, an emitter below the threshold still moves on the CPU, so to
/// measure the GPU there, lower the threshold first.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_particles() {
    for out in [1000.0, 2000.0, 3000.0, 4000.0] {
        for gpu in [false, true] {
            particles(out, gpu);
        }
    }
    particles(100_000.0, true);
}

fn particles(out: f32, gpu: bool) {
    let mut world = World::new();
    add_floor(&mut world, 100.0);
    let part = add_part(
        &mut world,
        ClassName::Part,
        &[
            ("Position", vector(0.0, 2.0, 0.0)),
            ("Size", vector(2.0, 1.0, 2.0)),
        ],
    );
    let emitter = world.create(ClassName::ParticleEmitter).unwrap();
    world.set_parent(emitter, Some(part)).unwrap();
    for (name, value) in [
        ("Rate", f64::from(out) / 2.0),
        ("Lifetime", 2.0),
        ("Speed", 4.0),
        ("SpreadAngle", 120.0),
        ("Size", 0.4),
    ] {
        world
            .set_property(emitter, name, Value::Number(value))
            .unwrap();
    }
    let assets = Assets::default();
    let mut particles = axcenity_scene::Particles::new();
    particles.set_gpu(gpu);
    // Filled before measuring: lives last up to 2.4 seconds.
    for _ in 0..180 {
        particles.update(&world, 1.0 / 60.0);
    }
    let camera = camera(Vec3::new(0.0, 6.0, 16.0), Vec3::new(0.0, 4.0, 0.0));
    let path = if gpu { "GPU" } else { "CPU" };
    run(&format!("bench_particles: {out} on the {path}"), |steps| {
        steps.time("Particles::update", || particles.update(&world, 1.0 / 60.0));
        let mut list = steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list(&world, &assets, camera)
        });
        steps.time("Particles::draw", || {
            particles.draw(&mut list, &world, &assets)
        });
        list
    });
    println!("  particles out: {}\n", particles.count());
}

/// Phase 75: 500 bullet holes spawned by `workspace:SpawnDecal` on the
/// graphics reference scene (hello-graphics), all kept (its cap raised),
/// against the same view without them. The holes must stay within a 60
/// frames a second budget on the GPU (where it can be timed).
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_500_bullet_holes_in_the_graphics_reference_scene() {
    let (mut world, mut scripts, folder) = start_example_in("hello-graphics");
    let workspace = world.workspace();
    world
        .set_property(workspace, "MaxSpawnedDecals", Value::Number(1000.0))
        .unwrap();
    let assets = Assets::new(&folder);
    let eye = Vec3::new(12.0, 8.0, 16.0);
    let view = camera(eye, Vec3::ZERO);
    let mut cache = TerrainCache::new();
    let mut draw_list = |world: &World, steps: &mut Steps| {
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list_posed(
                world,
                &assets,
                view,
                &axcenity_anim::Poses::new(),
                &mut cache,
            )
        })
    };
    let without = run("graphics reference scene", |steps| draw_list(&world, steps));

    // A grid of shots from the camera into the scene.
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    scripts.start_script(
        &mut world,
        script,
        "scripts/holes.server.lua",
        r#"
local eye = Vector3.new(12, 8, 16)
local spawned = 0
for i = 0, 24 do
    for j = 0, 19 do
        local aim = Vector3.new(-9 + i * 0.75, -1, -8 + j * 0.8)
        local hit = workspace:Raycast(eye, (aim - eye).Unit * 300)
        if hit then
            workspace:SpawnDecal("", CFrame.lookAt(hit.Position, hit.Position + hit.Normal),
                0.6, 600)
            spawned += 1
        end
    end
end
print(spawned)
"#,
    );
    let printed: Vec<String> = scripts.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(printed, ["500"], "every shot hits something");
    let with = run("graphics reference scene, 500 bullet holes", |steps| {
        draw_list(&world, steps)
    });
    let (Some((_, before)), Some((stats, after))) = (without, with) else {
        return;
    };
    assert_eq!(stats.decals_drawn, 500);
    if let (Some(before), Some(after)) = (before, after) {
        println!(
            "500 bullet holes add {} on the GPU",
            millis(after.saturating_sub(before))
        );
        assert!(
            after < Duration::from_micros(16_667),
            "{} is over a 60 fps frame",
            millis(after)
        );
    }
}

/// World streaming's frame budget (Phase 80): 50,000 parts over 4 km as
/// Models of 8, with streaming on, a character-sized camera in the middle
/// and a joined player's server cost (network send) counted.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_50000_parts_streaming() {
    let mut world = World::new();
    let workspace = world.workspace();
    world
        .set_property(workspace, "StreamingEnabled", Value::Bool(true))
        .unwrap();
    world
        .set_property(workspace, "StreamingTargetRadius", Value::Number(512.0))
        .unwrap();
    let mut random = Random(7);
    let mut model = None;
    let mut town = (0.0, 0.0);
    for index in 0..50_000 {
        // Each Model is a little group of parts in its own place.
        if index % 8 == 0 {
            let id = world.create(ClassName::Model).unwrap();
            world.set_parent(id, Some(workspace)).unwrap();
            model = Some(id);
            town = (
                random.next() * 4000.0 - 2000.0,
                random.next() * 4000.0 - 2000.0,
            );
        }
        let part = world.create(ClassName::Part).unwrap();
        for (name, value) in [
            ("Anchored", Value::Bool(true)),
            (
                "Position",
                vector(
                    town.0 + random.next() * 40.0,
                    2.0,
                    town.1 + random.next() * 40.0,
                ),
            ),
            ("Size", vector(4.0, 4.0, 4.0)),
        ] {
            world.set_property(part, name, value).unwrap();
        }
        world.set_parent(part, model).unwrap();
    }
    let assets = Assets::default();
    let mut physics = axcenity_physics::Physics::with_assets(assets.clone());
    let mut terrain = axcenity_scene::TerrainCache::new();
    let poses = axcenity_scene::Poses::new();
    let camera = camera(Vec3::new(0.0, 8.0, 0.0), Vec3::new(30.0, 4.0, 30.0));
    let mut replicator = axcenity_net::Replicator::default();
    let mut streams = axcenity_net::streaming::Streams::default();
    let settings = axcenity_core::streaming::Settings::of(&world).unwrap();
    streams.start_peer(
        &world,
        &settings,
        1,
        Some(Vector3::default()),
        &Default::default(),
    );
    run("bench_50000_parts_streaming", |steps| {
        steps.time("physics.step", || {
            physics.step(&mut world, 1.0 / 60.0);
        });
        steps.time("streaming.step", || {
            terrain.streaming.step(&world, Vec3::ZERO, 1.0 / 60.0);
        });
        steps.time("network send", || {
            let update = replicator.frame(&world);
            let tick = streams.frame(&world, 1.0 / 60.0);
            streams.for_peer(
                &world,
                Some(&settings),
                1,
                Some(Vector3::default()),
                &Default::default(),
                &update,
                tick,
                0.0,
            );
        });
        steps.time("build_draw_list", || {
            axcenity_scene::build_draw_list_posed(&world, &assets, camera, &poses, &mut terrain)
        })
    });
}

/// A project of the user's own, named by the `AXCENITY_BENCH_PROJECT`
/// environment variable (its folder), drawn from its SpawnLocation at eye
/// height and from an overview above it, so a slow project can be measured
/// like the scenes above. Skipped when the variable isn't set.
#[test]
#[ignore = "benchmark: run with --release -- --ignored bench_"]
fn bench_project() {
    let Some(folder) = std::env::var_os("AXCENITY_BENCH_PROJECT") else {
        eprintln!("skipped bench_project: set AXCENITY_BENCH_PROJECT to a project folder");
        return;
    };
    let folder = PathBuf::from(folder);
    let project = axcenity_project::load(&folder).unwrap();
    let world = project.world;
    let assets = Assets::new(&folder);
    let workspace = world.workspace();
    let spawn = world
        .descendants(workspace)
        .into_iter()
        .filter_map(|id| world.get(id))
        .find(|instance| instance.class() == ClassName::SpawnLocation)
        .and_then(|spawn| spawn.property("Position").and_then(Value::as_vector3))
        .map(|position| Vec3::new(position.x, position.y, position.z))
        .unwrap_or(Vec3::ZERO);
    let eyes = spawn + Vec3::new(0.0, 2.5, 0.0);
    let name = project.manifest.name.clone();
    for (view, camera) in [
        (
            "at the spawn, looking ahead",
            camera(eyes, eyes + Vec3::new(0.0, -0.1, -50.0)),
        ),
        (
            "overview",
            camera(
                spawn + Vec3::new(0.0, 120.0, 250.0),
                spawn + Vec3::new(0.0, 0.0, -100.0),
            ),
        ),
    ] {
        let mut cache = TerrainCache::new();
        run(&format!("bench_project `{name}`: {view}"), |steps| {
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
    for error in assets.take_errors() {
        eprintln!("asset error: {error}");
    }
}

/// Draws a project from named cameras and saves each view as a PNG, with
/// the frame's numbers, so a project's look can be compared with a reference.
/// `AXCENITY_BENCH_PROJECT` names the project, `AXCENITY_SHOT_DIR` the folder
/// for the pictures, `AXCENITY_SHOT_VIEWS` the cameras
/// (`name:eyeX,eyeY,eyeZ:targetX,targetY,targetZ[:fovY]`, separated by `;`),
/// and `AXCENITY_SHOT_SIZE` (`1280x720`) and `AXCENITY_SHOT_QUALITY` (`High`)
/// are optional.
///
/// `cargo test -p axcenity-player --release -- --ignored shoot_project --nocapture --test-threads 1`
#[test]
#[ignore = "tool: run with --release -- --ignored shoot_project"]
fn shoot_project() {
    let (Some(folder), Some(out), Some(views)) = (
        std::env::var_os("AXCENITY_BENCH_PROJECT"),
        std::env::var_os("AXCENITY_SHOT_DIR"),
        std::env::var("AXCENITY_SHOT_VIEWS").ok(),
    ) else {
        eprintln!("skipped shoot_project: set AXCENITY_BENCH_PROJECT, _SHOT_DIR and _SHOT_VIEWS");
        return;
    };
    let (width, height) = std::env::var("AXCENITY_SHOT_SIZE")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .unwrap_or(SIZE);
    let quality = match std::env::var("AXCENITY_SHOT_QUALITY").as_deref() {
        Ok("Low") => Quality::Low,
        Ok("Medium") => Quality::Medium,
        Ok("Ultra") => Quality::Ultra,
        _ => Quality::High,
    };
    let folder = PathBuf::from(folder);
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let loading = Instant::now();
    let world = axcenity_project::load(&folder).unwrap().world;
    println!("world read in {:.2} s", loading.elapsed().as_secs_f64());
    let assets = Assets::new(&folder);
    let vector_of = |text: &str| {
        let numbers: Vec<f32> = text.split(',').map(|n| n.trim().parse().unwrap()).collect();
        Vec3::new(numbers[0], numbers[1], numbers[2])
    };
    let (device, queue) = bench_gpu().expect("no graphics adapter");
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut settings = GraphicsSettings::preset(quality);
    if std::env::var_os("AXCENITY_SHOT_NOAA").is_some() {
        settings.anti_aliasing = Some(axcenity_render::AntiAliasing::Off);
    }
    renderer.set_graphics(&settings);
    let mut cache = TerrainCache::new();
    for view in views.split(';').filter(|view| !view.trim().is_empty()) {
        let parts: Vec<&str> = view.trim().split(':').collect();
        let camera = Camera {
            position: vector_of(parts[1]),
            look_at: vector_of(parts[2]),
            fov_y_degrees: parts.get(3).map_or(70.0, |fov| fov.parse().unwrap()),
        };
        let build = |cache: &mut TerrainCache| {
            axcenity_scene::build_draw_list_posed(
                &world,
                &assets,
                camera,
                &Default::default(),
                cache,
            )
        };
        // The first frames read models and fill the caches.
        let first = Instant::now();
        for _ in 0..24 {
            let list = build(&mut cache);
            renderer.render(&list).unwrap();
            device.poll(wgpu::PollType::Wait).unwrap();
        }
        println!(
            "{}: first 24 frames {:.2} s",
            parts[0],
            first.elapsed().as_secs_f64()
        );
        let (mut built, mut drawn, mut gpu) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..60 {
            let start = Instant::now();
            let list = build(&mut cache);
            built.push(start.elapsed());
            let start = Instant::now();
            renderer.render(&list).unwrap();
            drawn.push(start.elapsed());
            device.poll(wgpu::PollType::Wait).unwrap();
            gpu.extend(renderer.stats().gpu_time);
        }
        let median = |times: &mut Vec<Duration>| {
            times.sort();
            times.get(times.len() / 2).copied().unwrap_or_default()
        };
        let stats = renderer.stats();
        let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
        println!(
            "{} ({width} × {height}, {quality:?})\n  {} of {} items drawn in {} batches; {} draw calls, {} triangles\n  shadow casters {} drawn, {} culled\n  GPU memory: textures {:.0} MB, meshes {:.0} MB, targets {:.0} MB\n  CPU build_draw_list {}\n  CPU ScenePipeline::draw {}\n  GPU frame {}",
            parts[0],
            stats.items_drawn,
            stats.items_submitted,
            stats.batches,
            stats.draw_calls,
            stats.triangles,
            stats.shadow_casters_drawn,
            stats.shadow_casters_culled,
            mb(stats.texture_bytes),
            mb(stats.mesh_bytes),
            mb(stats.target_bytes),
            millis(median(&mut built)),
            millis(median(&mut drawn)),
            millis(median(&mut gpu)),
        );
        for pass in FramePass::ALL {
            if let Some(time) = stats.pass_time(pass) {
                println!("  GPU   {:<26} {}", pass.label(), millis(time));
            }
        }
        let debug = std::env::var("AXCENITY_SHOT_DEBUG").ok();
        if let Some(view) = debug.as_deref() {
            use axcenity_render::DebugView;
            renderer.set_debug_view(Some(match view {
                "base" => DebugView::BaseColor,
                "normals" => DebugView::Normals,
                "ao" => DebugView::AmbientOcclusion,
                "overdraw" => DebugView::Overdraw,
                "rough" => DebugView::Roughness,
                _ => DebugView::Unlit,
            }));
            let list = build(&mut cache);
            renderer.render(&list).unwrap();
            device.poll(wgpu::PollType::Wait).unwrap();
        }
        let pixels = renderer.read_pixels();
        let name = out.join(format!("{}.png", parts[0]));
        let file = std::io::BufWriter::new(std::fs::File::create(&name).unwrap());
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
    }
    for error in assets.take_errors() {
        eprintln!("asset error: {error}");
    }
}

/// Bakes a project's light probe volumes as the editor's Bake Probes does,
/// at High, without opening the editor, and says how long each took.
///
/// `AXCENITY_BENCH_PROJECT=<folder> cargo test -p axcenity-player --release -- --ignored bake_project_probes --nocapture --test-threads 1`
#[test]
#[ignore = "tool: run with --release -- --ignored bake_project_probes"]
fn bake_project_probes() {
    use axcenity_assets::{LightProbes, write_light_probes};
    use axcenity_render::LIGHT_PROBE_BAKE_SIZE;

    let Some(folder) = std::env::var_os("AXCENITY_BENCH_PROJECT") else {
        eprintln!("skipped bake_project_probes: set AXCENITY_BENCH_PROJECT");
        return;
    };
    let folder = PathBuf::from(folder);
    let world = axcenity_project::load(&folder).unwrap().world;
    let assets = Assets::new(&folder);
    let (device, queue) = bench_gpu().expect("no graphics adapter");
    let list = axcenity_scene::build_draw_list(&world, &assets, camera(Vec3::ZERO, Vec3::NEG_Z));
    let fingerprint = axcenity_scene::light_bake_fingerprint(&world);
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::LightProbeVolume {
            continue;
        }
        let Some(path) = instance
            .property("Bake")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
        else {
            eprintln!("{} has no Bake path", instance.name());
            continue;
        };
        let grid = axcenity_scene::light_probe_points(&world, id).unwrap();
        let start = Instant::now();
        let probes = TextureRenderer::bake_light_probes(
            &device,
            &queue,
            &list,
            &grid.points,
            LIGHT_PROBE_BAKE_SIZE,
            &GraphicsSettings::preset(Quality::High),
        )
        .unwrap();
        let probes = LightProbes::new(grid.counts, fingerprint, probes).unwrap();
        write_light_probes(&folder, &path, &probes).unwrap();
        println!(
            "{path}: {:?} probes in {:.1} s",
            grid.counts,
            start.elapsed().as_secs_f64()
        );
    }
}
