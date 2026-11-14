use super::*;
use axcenity_project::branding::Picture;

#[test]
fn every_settings_page_draws_and_changes_apply_to_the_look() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.settings.open = true;
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for page in SettingsPage::ALL {
        app.settings.page = page;
        let mut draw = |ui: &mut egui::Ui| {
            let ctx = &ui.ctx().clone();
            egui::CentralPanel::default().show(ui, |_| {});
            app.settings_window(ctx);
        };
        match &snapshots {
            Some(folder)
                if matches!(
                    page,
                    SettingsPage::Appearance
                        | SettingsPage::Graphics
                        | SettingsPage::Updates
                        | SettingsPage::Ai
                ) =>
            {
                crate::snapshot::snapshot(
                    &folder.join(format!("settings-{}.png", page.label().to_lowercase())),
                    egui::vec2(900.0, 640.0),
                    vec![Vec::new(); 3],
                    &mut draw,
                )
            }
            _ => {
                let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }
    assert!(app.settings.open);

    let light = Appearance {
        theme: ThemeChoice::Light,
        accent: Accent::Orange,
        ..Appearance::default()
    };
    let _ = crate::theme::run(&ctx, Default::default(), |ui| {
        let ctx = &ui.ctx().clone();
        app.set_appearance(ctx, light.clone())
    });
    assert_eq!(app.appearance, light);
    assert!(!ctx.global_style().visuals.dark_mode);
    assert_eq!(
        theme::palette(&ctx).accent,
        Palette::new(false, Accent::Orange).accent
    );
}

/// eframe's saved settings, in memory.
#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

/// Layouts saved by name (design system V17) are there after a restart.
#[test]
fn saved_layouts_survive_a_restart() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.layout.use_preset(crate::layout::Preset::Modeling);
    app.save_layout("Props").unwrap();
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let mut restarted = EditorApp::new(None);
    assert!(restarted.saved_layouts.0.is_empty());
    restarted.load_settings(&ctx, Some(&storage));
    assert_eq!(restarted.saved_layouts, app.saved_layouts);
    restarted.use_saved_layout("Props");
    assert!(restarted.layout.is_open(Pane::ModelStudio));
}

/// The movement guide is on until it's turned off, and stays off after
/// a restart; the other viewport guides keep their own choices.
#[test]
fn the_movement_guide_choice_survives_a_restart() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    assert!(app.move_guide, "on by default");
    app.move_guide = false;
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let mut restarted = EditorApp::new(None);
    restarted.load_settings(&ctx, Some(&storage));
    assert!(!restarted.move_guide);
    assert!(restarted.grid && restarted.view_cube);

    restarted.move_guide = true;
    eframe::App::save(&mut restarted, &mut storage);
    let mut again = EditorApp::new(None);
    again.load_settings(&ctx, Some(&storage));
    assert!(again.move_guide);
}

#[test]
fn graphics_settings_survive_a_restart() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.graphics = GraphicsSettings {
        quality: Quality::Medium,
        automatic: false,
        shadow_quality: Some(ShadowQuality::Off),
        shadow_distance: Some(60.0),
        post_processing: None,
        render_scale: Some(0.5),
        texture_quality: Some(TextureQuality::Half),
        texture_budget: Some(128),
        anti_aliasing: Some(AntiAliasing::Taa),
        ambient_occlusion: Some(AmbientOcclusion::Low),
        reflection_quality: Some(ReflectionQuality::Medium),
        contact_shadows: Some(false),
        realtime_gi: Some(true),
        volumetric_fog: Some(true),
        cloud_quality: Some(CloudQuality::Ultra),
        depth_of_field: Some(DepthOfFieldQuality::Off),
        vignette: Some(false),
        film_grain: Some(true),
        chromatic_aberration: None,
        sharpen: Some(false),
        sun_rays: Some(true),
        motion_blur: Some(MotionBlurQuality::Low),
        ray_tracing_distance: Some(300.0),
        ray_traced_shadows: Some(RayTracedShadows::Medium),
        ray_traced_ambient_occlusion: Some(axcenity_render::RayTracedAmbientOcclusion::Low),
        ray_traced_global_illumination: Some(axcenity_render::RayTracedGlobalIllumination::High),
        ray_tracing: Some(true),
    };
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let mut restarted = EditorApp::new(None);
    assert_eq!(restarted.graphics, GraphicsSettings::default());
    restarted.load_settings(&ctx, Some(&storage));
    assert_eq!(restarted.graphics, app.graphics);
}

#[test]
fn changing_the_graphics_setting_changes_the_viewport_at_once() {
    let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
    let mut frame = 0;
    let mut seen = Vec::new();
    crate::snapshot::snapshot_rendered(
        &std::env::temp_dir().join(format!("axcenity-graphics-{}.png", std::process::id())),
        egui::vec2(320.0, 200.0),
        vec![Vec::new(); 3],
        |ui, render_state| {
            match frame {
                1 => app.graphics.shadow_quality = Some(ShadowQuality::Off),
                2 => app.graphics.anti_aliasing = Some(AntiAliasing::Fxaa),
                _ => {}
            }
            frame += 1;
            egui::CentralPanel::default().show(ui, |ui| {
                app.scene_pane(ui, Some(render_state));
            });
            seen.push(
                app.viewport
                    .graphics()
                    .map(|used| (used.shadow_quality, used.anti_aliasing)),
            );
        },
    );
    if seen.is_empty() {
        return; // No GPU.
    }
    let (high, off) = (ShadowQuality::High, ShadowQuality::Off);
    assert_eq!(
        seen,
        [
            Some((high, AntiAliasing::Msaa4)),
            Some((off, AntiAliasing::Msaa4)),
            Some((off, AntiAliasing::Fxaa))
        ]
    );
}

/// Lighting's Exposure and ToneMapping, set as the Properties panel sets
/// them, change what the viewport draws.
#[test]
fn exposure_and_tone_mapping_set_in_properties_change_the_viewport() {
    use axcenity_core::{ToneMapping, Value};
    let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
    let lighting = app.world.lighting();
    let mut frame = 0;
    let mut drawn = Vec::new();
    crate::snapshot::snapshot_rendered(
        &std::env::temp_dir().join(format!("axcenity-look-{}.png", std::process::id())),
        egui::vec2(320.0, 200.0),
        vec![Vec::new(); 3],
        |ui, render_state| {
            match frame {
                1 => app.apply(lighting, Edit::Property("Exposure", Value::Number(2.0))),
                2 => app.apply(
                    lighting,
                    Edit::Property("ToneMapping", Value::ToneMapping(ToneMapping::Filmic)),
                ),
                _ => {}
            }
            frame += 1;
            egui::CentralPanel::default().show(ui, |ui| {
                app.scene_pane(ui, Some(render_state));
            });
            drawn.extend(app.viewport.pixels());
        },
    );
    if drawn.is_empty() {
        return; // No GPU.
    }
    let [before, brighter, filmic] = &drawn[..] else {
        panic!("drew {} frames, not 3", drawn.len());
    };
    let changed = |a: &[u8], b: &[u8]| {
        a.chunks(4)
            .zip(b.chunks(4))
            .filter(|(a, b)| a[..3] != b[..3])
            .count()
    };
    let pixels = before.len() / 4;
    let exposed = changed(before, brighter);
    assert!(
        exposed > pixels / 4,
        "exposure changed {exposed} of {pixels}"
    );
    let curved = changed(brighter, filmic);
    assert!(
        curved > pixels / 4,
        "tone mapping changed {curved} of {pixels}"
    );
}

/// hello-graphics' LocalScript steps the quality on G. In Play that
/// changes the viewport, starting from the editor's own settings, and
/// Stop puts them back without saving the script's choice.
#[test]
fn a_local_scripts_graphics_change_lasts_until_stop() {
    use axcenity_script::KeyEvent;
    let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
    app.play();
    let press = KeyEvent {
        key: axcenity_core::KeyCode::G,
        pressed: true,
    };
    let mut frame = 0;
    let mut seen = Vec::new();
    crate::snapshot::snapshot_rendered(
        &std::env::temp_dir().join(format!("axcenity-play-graphics-{}.png", std::process::id())),
        egui::vec2(320.0, 200.0),
        vec![Vec::new(); 3],
        |ui, render_state| {
            match frame {
                1 => app.play_frame(1.0 / 60.0, &[press.into()], None, None),
                2 => app.stop(),
                _ => app.play_frame(1.0 / 60.0, &[], None, None),
            }
            frame += 1;
            egui::CentralPanel::default().show(ui, |ui| {
                app.scene_pane(ui, Some(render_state));
            });
            seen.push(app.viewport.graphics().map(|used| used.shadow_quality));
        },
    );
    if seen.is_empty() {
        return; // No GPU.
    }
    // The editor's High, then Ultra (the one after it), then High again.
    assert_eq!(
        seen,
        [
            Some(ShadowQuality::High),
            Some(ShadowQuality::Ultra),
            Some(ShadowQuality::High)
        ]
    );
    assert_eq!(app.graphics, GraphicsSettings::default());
    assert!(
        app.output
            .lines()
            .iter()
            .any(|line| line.text == "Graphics quality: Ultra"),
        "{:?}",
        app.output.lines()
    );
}

#[test]
fn branding_is_kept_in_the_manifest_used_by_branding_paths_and_previewed() {
    let folder = crate::app::tests::copy_of_example("hello-part", "branding-settings");
    let mut app = EditorApp::new(Some(&folder));
    std::fs::create_dir_all(folder.join("assets/branding")).unwrap();
    image::RgbaImage::from_pixel(256, 256, image::Rgba([30, 90, 200, 255]))
        .save(folder.join("assets/branding/icon.png"))
        .unwrap();
    // Square, so the banner is the wrong shape and says so.
    image::RgbaImage::from_pixel(400, 400, image::Rgba([200, 90, 30, 255]))
        .save(folder.join("assets/branding/banner.png"))
        .unwrap();
    let branding = axcenity_project::branding::Branding {
        icon: Some("assets/branding/icon.png".into()),
        banner: Some("assets/branding/banner.png".into()),
    };
    app.set_branding(branding.clone());
    assert_eq!(
        axcenity_project::read_manifest(&folder).unwrap().branding,
        branding
    );
    assert_eq!(
        &*app.assets.resolve("branding://icon"),
        "assets/branding/icon.png"
    );
    assert!(app.assets.texture_blocking("branding://icon").is_some());

    app.settings.open = true;
    app.settings.page = SettingsPage::Project;
    let ctx = theme::test_context();
    let mut draw = |ui: &mut egui::Ui| {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |_| {});
        app.settings_window(ctx);
    };
    for _ in 0..2 {
        let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        crate::snapshot::snapshot(
            &PathBuf::from(folder).join("settings-branding.png"),
            egui::vec2(900.0, 1100.0),
            // Scrolled down to Branding.
            std::iter::once(Vec::new())
                .chain((0..8).map(|_| {
                    vec![
                        egui::Event::PointerMoved(egui::pos2(500.0, 600.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -400.0),
                            modifiers: Default::default(),
                            phase: egui::TouchPhase::Move,
                        },
                    ]
                }))
                .chain([Vec::new(), Vec::new()])
                .collect(),
            &mut draw,
        );
    }
    let banner = &app.branding_previews;
    assert!(
        banner
            .problem(Picture::Banner)
            .is_some_and(|p| p.contains("400×400"))
    );
    assert_eq!(banner.problem(Picture::Icon), None);
}

#[test]
fn the_ray_tracing_section_says_what_a_restart_will_do() {
    use super::graphics::{needs_restart, ray_tracing_status};
    assert!(ray_tracing_status(false, false, true).contains("can't ray trace"));
    assert!(ray_tracing_status(false, true, false).contains("Turn on Hardware ray tracing"));
    assert!(ray_tracing_status(false, true, true).contains("turns on when the editor restarts"));
    assert!(ray_tracing_status(true, true, false).contains("turns off"));
    assert!(ray_tracing_status(true, true, true).contains("is on"));
    // Restart now shows only when a restart changes what the GPU was opened with.
    assert!(needs_restart(false, true, true));
    assert!(needs_restart(true, true, false));
    assert!(
        !needs_restart(false, false, true),
        "a GPU that can't, can't"
    );
    assert!(!needs_restart(true, true, true));
    assert!(!needs_restart(false, true, false));
}

#[test]
fn the_in_use_line_lists_each_ray_traced_effect() {
    use super::graphics::ray_tracing_in_use;
    use axcenity_render::{EffectiveGraphics, RayTracedAmbientOcclusion};
    let mut effective = EffectiveGraphics::default();
    assert!(ray_tracing_in_use(&effective).contains("off"));
    effective.ray_traced_global_illumination = axcenity_render::RayTracedGlobalIllumination::Low;
    effective.ray_traced_gi_in_software = true;
    let line = ray_tracing_in_use(&effective);
    assert!(
        line.contains("off") && line.contains("GI Low traced in software"),
        "{line}"
    );
    effective.ray_traced_global_illumination = axcenity_render::RayTracedGlobalIllumination::Off;
    effective.ray_traced_gi_in_software = false;
    effective.ray_queries = true;
    effective.ray_tracing_distance = 400.0;
    assert!(ray_tracing_in_use(&effective).contains("no ray traced effects"));
    effective.ray_traced_shadows = RayTracedShadows::Low;
    effective.ray_traced_ambient_occlusion = RayTracedAmbientOcclusion::High;
    let line = ray_tracing_in_use(&effective);
    assert!(line.contains("shadows Low"), "{line}");
    assert!(line.contains("ambient occlusion High (8 rays"), "{line}");
}
