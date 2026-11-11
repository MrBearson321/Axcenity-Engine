//! `profile_project`: where a project's GPU frame goes, measured by leaving
//! one cost out at a time and timing the frame again (a "subtract one"
//! profile). Each variant changes only the draw list or the graphics
//! settings for the measurement; the project is never written.
//!
//! ```text
//! AXCENITY_BENCH_PROJECT=<folder> AXCENITY_SHOT_VIEWS='walker:-39.6,6,39.6:0,4,0' \
//!   AXCENITY_SHOT_SIZE=2560x1440 AXCENITY_SHOT_QUALITY=Ultra \
//!   cargo test -p axcenity-player --release -- --ignored profile_project --nocapture --test-threads 1
//! ```
//!
//! `AXCENITY_PROFILE_VARIANTS` (names from [`VARIANTS`], comma-separated)
//! picks some; all run by default. `AXCENITY_SHOT_DIR` saves a PNG of each
//! variant and of the overdraw view. How to read the numbers:
//! `docs/rendering/performance.md` ("Where the frame goes").

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_render::{
    AmbientOcclusion, AntiAliasing, Camera, DebugView, DrawItem, DrawList, FramePass,
    GraphicsSettings, ItemChunk, OverdrawStats, Quality, ShadowQuality, Shape, TextureRenderer,
    glam::Vec3, wgpu,
};
use axcenity_scene::TerrainCache;

/// Each variant: its name, and the cost it leaves out.
pub const VARIANTS: &[(&str, &str)] = &[
    ("base", "the frame as drawn"),
    ("no-shadows", "no shadow maps (sun, spot and point)"),
    (
        "no-foliage-shadows",
        "painted foliage (items with a cull distance) casts no shadow",
    ),
    ("no-wind", "no wind, so nothing bends or flutters"),
    ("no-sway", "no item has a sway (the vertex shader skips it)"),
    (
        "no-cutout",
        "cut-out materials drawn solid (no alpha test, no depth fill)",
    ),
    ("single-sided", "double-sided materials drawn one-sided"),
    (
        "no-foliage",
        "painted foliage (items with a cull distance) left out",
    ),
    (
        "no-large-meshes",
        "meshes of more than 100,000 triangles left out",
    ),
    ("no-msaa", "anti-aliasing off"),
    ("no-ao", "ambient occlusion off"),
];

/// Meshes bigger than this are "large" for `no-large-meshes`.
const LARGE_MESH_TRIANGLES: usize = 100_000;
const WARM_UP_FRAMES: usize = 30;
const MEASURED_FRAMES: usize = 60;
const ROUNDS: usize = 3;
/// Frames drawn before waiting for the GPU, so it always has work queued.
const IN_FLIGHT: usize = 3;

/// One variant's medians over one round.
#[derive(Clone)]
struct Measured {
    gpu: Option<Duration>,
    passes: [Option<Duration>; FramePass::COUNT],
    cpu: Duration,
    triangles: u64,
    draw_calls: u32,
    casters: u32,
}

/// Draws `list` for a warm-up, then times it.
fn measure(renderer: &mut TextureRenderer, device: &wgpu::Device, list: &DrawList) -> Measured {
    for frame in 0..WARM_UP_FRAMES {
        renderer.render(list).unwrap();
        if frame % IN_FLIGHT == 0 {
            device.poll(wgpu::PollType::Wait).unwrap();
        }
    }
    let (mut gpu, mut cpu) = (Vec::new(), Vec::new());
    let mut passes: Vec<Vec<Duration>> = vec![Vec::new(); FramePass::COUNT];
    let mut readings = renderer.stats().gpu_readings;
    for frame in 0..MEASURED_FRAMES {
        let start = Instant::now();
        renderer.render(list).unwrap();
        cpu.push(start.elapsed());
        if frame % IN_FLIGHT == 0 {
            device.poll(wgpu::PollType::Wait).unwrap();
        }
        let stats = renderer.stats();
        if stats.gpu_readings != readings {
            readings = stats.gpu_readings;
            gpu.extend(stats.gpu_time);
            for (index, pass) in FramePass::ALL.into_iter().enumerate() {
                passes[index].extend(stats.pass_time(pass));
            }
        }
    }
    device.poll(wgpu::PollType::Wait).unwrap();
    let stats = renderer.stats();
    Measured {
        gpu: median(&mut gpu),
        passes: std::array::from_fn(|index| median(&mut passes[index])),
        cpu: median(&mut cpu).unwrap_or_default(),
        triangles: stats.triangles,
        draw_calls: stats.draw_calls,
        casters: stats.shadow_casters_drawn,
    }
}

/// The median of the readings there are.
fn middle(times: Vec<Option<Duration>>) -> Option<Duration> {
    let mut times: Vec<Duration> = times.into_iter().flatten().collect();
    median(&mut times)
}

/// Changes `settings` as the variant `name` says.
fn vary_settings(name: &str, settings: &mut GraphicsSettings) {
    match name {
        "no-shadows" => settings.shadow_quality = Some(ShadowQuality::Off),
        "no-msaa" => settings.anti_aliasing = Some(AntiAliasing::Off),
        "no-ao" => settings.ambient_occlusion = Some(AmbientOcclusion::Off),
        _ => {}
    }
}

/// Changes `list` as the variant `name` says.
fn vary_list(name: &str, list: &mut DrawList) {
    let foliage = |item: &DrawItem| item.cull_distance > 0.0;
    match name {
        "no-foliage-shadows" => each_item(list, |item| {
            if foliage(item) {
                item.casts_shadow = false;
            }
        }),
        "no-wind" => list.wind = Vec3::ZERO,
        "no-sway" => each_item(list, |item| item.sway = None),
        "no-cutout" => each_item(list, |item| item.material.alpha_cutoff = 0.0),
        "single-sided" => each_item(list, |item| item.material.double_sided = false),
        "no-foliage" => keep_items(list, |item| !foliage(item)),
        "no-large-meshes" => keep_items(list, |item| match &item.shape {
            Shape::Mesh(mesh) => mesh.indices().len() / 3 <= LARGE_MESH_TRIANGLES,
            _ => true,
        }),
        _ => {}
    }
}

/// Changes every item, in the list and in its groups.
fn each_item(list: &mut DrawList, change: impl Fn(&mut DrawItem)) {
    list.items.iter_mut().for_each(&change);
    for chunk in &mut list.chunks {
        let mut items: Vec<DrawItem> = chunk.items.iter().cloned().collect();
        items.iter_mut().for_each(&change);
        chunk.items = Arc::from(items);
    }
}

/// Keeps only the items `keep` says, in the list and in its groups.
fn keep_items(list: &mut DrawList, keep: impl Fn(&DrawItem) -> bool) {
    list.items.retain(&keep);
    let chunks = std::mem::take(&mut list.chunks);
    for chunk in chunks {
        let items: Vec<DrawItem> = chunk
            .items
            .iter()
            .filter(|item| keep(item))
            .cloned()
            .collect();
        if !items.is_empty() {
            list.chunks.push(ItemChunk::of(items));
        }
    }
}

fn median(times: &mut [Duration]) -> Option<Duration> {
    times.sort();
    times.get(times.len() / 2).copied()
}

fn ms(time: Option<Duration>) -> String {
    time.map_or("-".to_owned(), |time| {
        format!("{:.2}", time.as_secs_f64() * 1000.0)
    })
}

#[test]
#[ignore = "tool: run with --release -- --ignored profile_project"]
fn profile_project() {
    let (Some(folder), Some(views)) = (
        std::env::var_os("AXCENITY_BENCH_PROJECT"),
        std::env::var("AXCENITY_SHOT_VIEWS").ok(),
    ) else {
        eprintln!("skipped profile_project: set AXCENITY_BENCH_PROJECT and AXCENITY_SHOT_VIEWS");
        return;
    };
    let (width, height) = std::env::var("AXCENITY_SHOT_SIZE")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .unwrap_or((1280, 720));
    let quality = std::env::var("AXCENITY_SHOT_QUALITY")
        .ok()
        .and_then(|name| Quality::ALL.into_iter().find(|q| q.label() == name))
        .unwrap_or(Quality::High);
    let chosen: Vec<String> = std::env::var("AXCENITY_PROFILE_VARIANTS")
        .map(|names| {
            names
                .split(',')
                .map(|name| name.trim().to_owned())
                .collect()
        })
        .unwrap_or_default();
    let out = std::env::var_os("AXCENITY_SHOT_DIR").map(PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).unwrap();
    }
    let folder = PathBuf::from(folder);
    let world = axcenity_project::load(&folder).unwrap().world;
    let assets = axcenity_assets::Assets::new(&folder);
    let (device, queue) = super::bench_tests::bench_gpu().expect("no graphics adapter");
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let mut cache = TerrainCache::new();
    let vector_of = |text: &str| {
        let numbers: Vec<f32> = text.split(',').map(|n| n.trim().parse().unwrap()).collect();
        Vec3::new(numbers[0], numbers[1], numbers[2])
    };
    println!("{} at {width} × {height}, {quality:?}", folder.display());
    for view in views.split(';').filter(|view| !view.trim().is_empty()) {
        let parts: Vec<&str> = view.trim().split(':').collect();
        let camera = Camera {
            position: vector_of(parts[1]),
            look_at: vector_of(parts[2]),
            fov_y_degrees: parts.get(3).map_or(70.0, |fov| fov.parse().unwrap()),
        };
        println!("\nview `{}`", parts[0]);
        // Each variant's list is made once and drawn again and again: the
        // same `Arc`s every frame, as a still scene is, and a CPU that keeps
        // the GPU busy (an idle GPU clocks down and times slower).
        let base = axcenity_scene::build_draw_list_posed(
            &world,
            &assets,
            camera,
            &Default::default(),
            &mut cache,
        );
        let runs: Vec<(&str, DrawList, GraphicsSettings)> = VARIANTS
            .iter()
            .filter(|(name, _)| {
                *name == "base" || chosen.is_empty() || chosen.iter().any(|chosen| chosen == name)
            })
            .map(|&(name, _)| {
                let mut list = base.clone();
                vary_list(name, &mut list);
                let mut settings = GraphicsSettings::preset(quality);
                vary_settings(name, &mut settings);
                (name, list, settings)
            })
            .collect();
        // Rounds through every variant, so a slow moment of the machine
        // falls on all of them, then the median of the rounds' medians.
        let mut results: Vec<Vec<Measured>> = vec![Vec::new(); runs.len()];
        for _ in 0..ROUNDS {
            for (index, (_, list, settings)) in runs.iter().enumerate() {
                renderer.set_graphics(settings);
                results[index].push(measure(&mut renderer, &device, list));
            }
        }
        println!(
            "| variant | GPU frame (change) | Shadows | Prepass | AO | Scene | Post | CPU draw | triangles (M) | draw calls | casters |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|");
        let base_gpu = middle(results[0].iter().map(|m| m.gpu).collect());
        for ((name, list, settings), rounds) in runs.iter().zip(&results) {
            let gpu = middle(rounds.iter().map(|m| m.gpu).collect());
            let change = match (base_gpu, gpu) {
                (Some(base), Some(now)) if *name != "base" => format!(
                    " ({:+.2})",
                    (now.as_secs_f64() - base.as_secs_f64()) * 1000.0
                ),
                _ => String::new(),
            };
            let pass =
                |pass: FramePass| middle(rounds.iter().map(|m| m.passes[pass as usize]).collect());
            let last = rounds.last().expect("at least one round");
            println!(
                "| {name} | {}{change} | {} | {} | {} | {} | {} | {} | {:.2} | {} | {} |",
                ms(gpu),
                ms(pass(FramePass::Shadows)),
                ms(pass(FramePass::Prepass)),
                ms(pass(FramePass::AmbientOcclusion)),
                ms(pass(FramePass::Scene)),
                ms(pass(FramePass::PostProcessing)),
                ms(middle(rounds.iter().map(|m| Some(m.cpu)).collect())),
                last.triangles as f64 / 1e6,
                last.draw_calls,
                last.casters,
            );
            if let Some(out) = &out {
                renderer.set_graphics(settings);
                renderer.render(list).unwrap();
                device.poll(wgpu::PollType::Wait).unwrap();
                save_png(
                    &out.join(format!("{}-{name}.png", parts[0])),
                    &renderer.read_pixels(),
                    (width, height),
                );
            }
        }
        // Overdraw: how many surfaces cover each pixel, as the overdraw view
        // counts them (see `OverdrawStats`).
        renderer.set_graphics(&GraphicsSettings::preset(quality));
        renderer.set_debug_view(Some(DebugView::Overdraw));
        let list = axcenity_scene::build_draw_list_posed(
            &world,
            &assets,
            camera,
            &Default::default(),
            &mut cache,
        );
        renderer.render(&list).unwrap();
        device.poll(wgpu::PollType::Wait).unwrap();
        let pixels = renderer.read_pixels();
        renderer.set_debug_view(None);
        let overdraw = OverdrawStats::from_pixels(&pixels, width, height);
        println!("\noverdraw ({}):\n{}", parts[0], overdraw.report());
        if let Some(out) = &out {
            save_png(
                &out.join(format!("{}-overdraw.png", parts[0])),
                &pixels,
                (width, height),
            );
        }
    }
    for error in assets.take_errors() {
        eprintln!("asset error: {error}");
    }
}

fn save_png(path: &std::path::Path, pixels: &[u8], (width, height): (u32, u32)) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
}

#[test]
fn the_settings_variants_turn_their_setting_off() {
    for (name, changed) in [
        ("no-shadows", true),
        ("no-msaa", true),
        ("no-ao", true),
        ("base", false),
        ("no-wind", false),
    ] {
        let mut settings = GraphicsSettings::preset(Quality::Ultra);
        vary_settings(name, &mut settings);
        assert_eq!(
            settings != GraphicsSettings::preset(Quality::Ultra),
            changed,
            "{name}"
        );
    }
    assert!(VARIANTS.iter().any(|(name, _)| *name == "base"));
}

/// Every model a project's MeshParts show, with its levels of detail and
/// their warnings ([`axcenity_assets::LodReport`]), biggest first.
///
/// `AXCENITY_BENCH_PROJECT=<folder> cargo test -p axcenity-player --release -- --ignored lod_report_project --nocapture`
#[test]
#[ignore = "tool: run with --release -- --ignored lod_report_project"]
fn lod_report_project() {
    let Some(folder) = std::env::var_os("AXCENITY_BENCH_PROJECT") else {
        eprintln!("skipped lod_report_project: set AXCENITY_BENCH_PROJECT");
        return;
    };
    let folder = PathBuf::from(folder);
    let world = axcenity_project::load(&folder).unwrap().world;
    let assets = axcenity_assets::Assets::new(&folder);
    let mut uses: std::collections::BTreeMap<String, usize> = Default::default();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != axcenity_core::ClassName::MeshPart {
            continue;
        }
        if let Some(mesh) = instance
            .property("MeshId")
            .and_then(axcenity_core::Value::as_str)
            .filter(|mesh| !mesh.is_empty())
        {
            *uses.entry(mesh.to_owned()).or_default() += 1;
        }
    }
    let mut reports: Vec<(String, usize, axcenity_assets::LodReport)> = uses
        .into_iter()
        .filter_map(|(mesh, count)| {
            let model = assets.model_blocking(&mesh)?;
            Some((mesh, count, axcenity_assets::LodReport::of(&model)))
        })
        .collect();
    reports.sort_by_key(|(_, count, report)| std::cmp::Reverse(report.triangles_at(0) * count));
    for (mesh, count, report) in &reports {
        println!("{mesh} (x{count})\n{}", report.text());
    }
}
