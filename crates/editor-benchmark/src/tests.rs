use axcenity_render::{AntiAliasing, GraphicsSettings, Quality, ShadowQuality};

use super::report::REPORT_VERSION;
use super::*;
use crate::testing::{device, quick_report};

#[test]
fn a_run_measures_each_preset_then_the_heavier_tests() {
    let Some(report) = quick_report() else {
        return;
    };
    assert_eq!(report.version, REPORT_VERSION);
    assert!(!report.adapter.name.is_empty());
    assert_eq!(report.size, (320, 180));
    assert!(report.setup_ms > 0.0);
    // Performance first, the rest in order (some may be skipped on a slow
    // software adapter).
    assert!(!report.presets.is_empty());
    for (measurement, tier) in report.presets.iter().zip(Tier::ALL) {
        assert_eq!(measurement.workload, workloads::Workload::Town);
        assert_eq!(measurement.settings, tier.settings());
        assert_eq!(measurement.frames, Plan::QUICK.preset_frames);
        assert!(measurement.frame.p95 > 0.0 && measurement.cpu.p95 > 0.0);
        assert!(measurement.draw_list.p95 <= measurement.cpu.p95);
        assert!(measurement.first_frame_ms > 0.0);
        assert!(measurement.triangles > 0 && measurement.draw_calls > 0);
        assert!(measurement.gpu_bytes > 0);
    }
    // The heavier tests ran at the recommendation (which is complete).
    let stress: Vec<_> = report.stress.iter().map(|m| m.workload).collect();
    assert_eq!(stress, workloads::Workload::STRESS);
    for measurement in &report.stress {
        let mut settings = measurement.settings.clone();
        settings.texture_budget = report.recommendation.settings.texture_budget;
        assert_eq!(settings, report.recommendation.settings);
    }
    assert!(!report.recommendation.reasons.is_empty());
    assert!(!report.recommendation.settings.automatic);
    if report.adapter.software {
        assert_eq!(report.recommendation.tier, Tier::Performance);
    }
    // And it saves and reads back.
    let saved = serde_json::to_string(&report).unwrap();
    assert_eq!(GraphicsBenchmark::load(&saved), Some(report));
}

#[test]
fn a_saved_report_of_another_version_is_not_read() {
    assert_eq!(GraphicsBenchmark::load("not json"), None);
    assert_eq!(GraphicsBenchmark::load(r#"{"version": 999}"#), None);
}

#[test]
fn changes_list_only_what_apply_would_change() {
    let high = GraphicsSettings::preset(Quality::High);
    assert!(changes(&high, &high).is_empty());

    let mut tuned = GraphicsSettings::preset(Quality::High);
    tuned.shadow_quality = Some(ShadowQuality::Medium);
    let listed = changes(&high, &tuned);
    assert_eq!(
        listed,
        vec![Change {
            setting: "Shadow quality",
            now: "High".to_owned(),
            recommended: "Medium".to_owned(),
        }]
    );

    // Each value as it would draw: an override the preset already has is
    // no change, and Automatic shows as such.
    let mut automatic = GraphicsSettings::preset(Quality::High);
    automatic.automatic = true;
    automatic.anti_aliasing = Some(AntiAliasing::Msaa4);
    let listed = changes(&automatic, &high);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].now, "High Quality (Automatic)");

    let low = changes(&high, &GraphicsSettings::preset(Quality::Low));
    let names: Vec<_> = low.iter().map(|change| change.setting).collect();
    for name in ["Preset", "Shadow quality", "Anti-aliasing", "Render scale"] {
        assert!(names.contains(&name), "{names:?}");
    }
}

#[test]
fn changes_has_a_row_for_every_setting() {
    use axcenity_render::{
        AmbientOcclusion, CloudQuality, DepthOfFieldQuality, MotionBlurQuality, RayTracedShadows,
        ReflectionQuality, TextureQuality,
    };
    /// `a` unless the setting already draws as `a`.
    fn other<T: PartialEq>(now: T, a: T, b: T) -> Option<T> {
        Some(if now == a { b } else { a })
    }
    let high = GraphicsSettings::preset(Quality::High);
    // Naming every field: a new setting doesn't compile here until it has
    // a row below and in `changes`.
    let GraphicsSettings {
        quality: _,
        automatic: _,
        shadow_quality: _,
        shadow_distance: _,
        post_processing: _,
        render_scale: _,
        texture_quality: _,
        texture_budget: _,
        anti_aliasing: _,
        ambient_occlusion: _,
        reflection_quality: _,
        contact_shadows: _,
        realtime_gi: _,
        volumetric_fog: _,
        cloud_quality: _,
        depth_of_field: _,
        vignette: _,
        film_grain: _,
        chromatic_aberration: _,
        sharpen: _,
        sun_rays: _,
        motion_blur: _,
        ray_tracing_distance: _,
        ray_traced_shadows: _,
        ray_traced_ambient_occlusion: _,
        ray_traced_global_illumination: _,
        ray_tracing: _,
    } = &high;
    let edits: [fn(&mut GraphicsSettings); 27] = [
        |s| s.quality = Quality::Low,
        |s| s.automatic = !s.automatic,
        |s| s.shadow_quality = other(s.shadow_quality(), ShadowQuality::Low, ShadowQuality::Ultra),
        |s| s.shadow_distance = Some(s.shadow_distance() + 100.0),
        |s| s.post_processing = Some(!s.post_processing()),
        |s| s.render_scale = Some(s.render_scale() * 0.5),
        |s| {
            s.texture_quality = other(
                s.texture_quality(),
                TextureQuality::Quarter,
                TextureQuality::Full,
            )
        },
        |s| s.texture_budget = Some(s.texture_budget() / 2),
        |s| s.anti_aliasing = other(s.anti_aliasing(), AntiAliasing::Off, AntiAliasing::Taa),
        |s| {
            s.ambient_occlusion = other(
                s.ambient_occlusion(),
                AmbientOcclusion::Off,
                AmbientOcclusion::High,
            )
        },
        |s| {
            s.reflection_quality = other(
                s.reflection_quality(),
                ReflectionQuality::Low,
                ReflectionQuality::Ultra,
            )
        },
        |s| s.contact_shadows = Some(!s.contact_shadows()),
        |s| s.realtime_gi = Some(!s.realtime_gi()),
        |s| s.volumetric_fog = Some(!s.volumetric_fog()),
        |s| s.cloud_quality = other(s.cloud_quality(), CloudQuality::Low, CloudQuality::Ultra),
        |s| {
            s.depth_of_field = other(
                s.depth_of_field(),
                DepthOfFieldQuality::Off,
                DepthOfFieldQuality::High,
            )
        },
        |s| s.vignette = Some(!s.vignette()),
        |s| s.film_grain = Some(!s.film_grain()),
        |s| s.chromatic_aberration = Some(!s.chromatic_aberration()),
        |s| s.sharpen = Some(!s.sharpen()),
        |s| s.sun_rays = Some(!s.sun_rays()),
        |s| {
            s.motion_blur = other(
                s.motion_blur(),
                MotionBlurQuality::Off,
                MotionBlurQuality::High,
            )
        },
        |s| s.ray_tracing_distance = Some(s.ray_tracing_distance() + 100.0),
        |s| {
            s.ray_traced_shadows = other(
                s.ray_traced_shadows(),
                RayTracedShadows::Off,
                RayTracedShadows::High,
            )
        },
        |s| {
            s.ray_traced_ambient_occlusion = other(
                s.ray_traced_ambient_occlusion(),
                axcenity_render::RayTracedAmbientOcclusion::Off,
                axcenity_render::RayTracedAmbientOcclusion::High,
            )
        },
        |s| {
            s.ray_traced_global_illumination = other(
                s.ray_traced_global_illumination(),
                axcenity_render::RayTracedGlobalIllumination::Off,
                axcenity_render::RayTracedGlobalIllumination::High,
            )
        },
        |s| s.ray_tracing = Some(!s.ray_tracing()),
    ];
    for (index, edit) in edits.iter().enumerate() {
        let mut edited = high.clone();
        edit(&mut edited);
        assert!(
            !changes(&high, &edited).is_empty(),
            "edit {index} isn't listed"
        );
    }
}

#[test]
fn the_window_draws_before_during_and_after_a_run() {
    let ctx = theme::test_context();
    let mut benchmark = GraphicsBenchmark {
        open: true,
        ..Default::default()
    };
    let current = GraphicsSettings::default();
    let draw = |benchmark: &mut GraphicsBenchmark| {
        let mut action = None;
        let _ = theme::run(&ctx, Default::default(), |ui| {
            action = benchmark.window(ui.ctx(), &current, Some("Test GPU"), false);
        });
        action
    };
    // Before: the presets and a Run button.
    assert_eq!(draw(&mut benchmark), None);
    let Some(report) = quick_report() else {
        return;
    };
    benchmark.report = Some(report.clone());
    assert_eq!(draw(&mut benchmark), None);
    benchmark.previous = Some(current.clone());
    assert_eq!(draw(&mut benchmark), None);

    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::Path::new(&folder).join("graphics-benchmark.png");
        axcenity_editor_kit::snapshot::snapshot(
            &path,
            egui::vec2(720.0, 800.0),
            vec![Vec::new(), Vec::new()],
            |ui| {
                let mut benchmark = GraphicsBenchmark {
                    open: true,
                    report: Some(report.clone()),
                    previous: Some(GraphicsSettings::default()),
                    ..Default::default()
                };
                benchmark.window(ui.ctx(), &current, Some(&report.adapter.name), false);
            },
        );
    }
}

/// The whole benchmark as the editor runs it, printed: for checking the
/// numbers and advice on a real machine.
/// `cargo test -p axcenity-editor --release -- --ignored graphics_benchmark_full --nocapture`
/// (`AXCENITY_BENCHMARK_SIZE=1920x1080` to draw at another size than 1280 × 720).
#[test]
#[ignore]
fn graphics_benchmark_full() {
    let Some((device, queue, info)) = device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let size = std::env::var("AXCENITY_BENCHMARK_SIZE")
        .ok()
        .and_then(|text| {
            let (width, height) = text.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .unwrap_or((1280, 720));
    let started = std::time::Instant::now();
    let mut run = Run::start(&device, &queue, &info, size, Plan::FULL);
    let report = loop {
        match run.step() {
            Progress::Running => {}
            Progress::Finished(report) => break *report,
            Progress::Failed(error) => panic!("{error}"),
        }
    };
    println!("Took {:.1} s", started.elapsed().as_secs_f32());
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    let measurements = report
        .presets
        .iter()
        .chain(&report.tuned)
        .chain(&report.stress);
    for m in measurements {
        println!(
            "{:<18} {:<32} frame p50 {:6.2} p95 {:6.2} | cpu p95 {:6.2} (list {:5.2}) | gpu p95 {:>6} | first {:6.1} | hitches {:3} | {:>6} MB",
            m.workload.label(),
            m.label,
            m.frame.median,
            m.frame.p95,
            m.cpu.p95,
            m.draw_list.p95,
            m.gpu.map_or("-".to_owned(), |g| format!("{:.2}", g.p95)),
            m.first_frame_ms,
            m.hitches,
            m.gpu_bytes >> 20,
        );
    }
    println!("Recommended: {}", report.recommendation.title);
    for reason in &report.recommendation.reasons {
        println!("  - {reason}");
    }
}
