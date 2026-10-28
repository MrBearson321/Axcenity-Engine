//! The graphics foundation, checked on hello-graphics (the graphics
//! reference scene): each preset against its reference image, lighting
//! changes on each kind of material, a LocalScript changing the graphics
//! settings, and a GPU that can't do what the settings ask.

use std::path::Path;

use axcenity_core::{ClassName, Color3, Value};
use axcenity_render::glam::Vec2;
use axcenity_render::{
    AntiAliasing, Capabilities, DrawList, GraphicsSettings, Quality, ShadowQuality, TextureRenderer,
};
use axcenity_scene::CameraView;

use super::tests::{
    REFERENCE_TOLERANCE, difference_from, run_until, start_example_in, test_gpu, write_png,
};
use super::*;

pub(super) const SIZE: (u32, u32) = (480, 270);

/// hello-graphics after its scripts' first second, with its assets and
/// the player's starting camera.
fn hello_graphics() -> (World, ScriptRuntime, Assets, FlyCamera) {
    let (mut world, mut scripts, folder) = start_example_in("hello-graphics");
    let (mut physics, mut clock) = (Physics::new(), 0);
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 1.0);
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    (world, scripts, Assets::new(&folder), camera)
}

/// `list` drawn by `renderer`, as RGBA rows. Models load the first time
/// they're drawn, so it draws twice.
fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

/// The drawn frame of `world`, from `camera`.
fn frame(
    renderer: &mut TextureRenderer,
    world: &World,
    assets: &Assets,
    camera: &FlyCamera,
) -> Vec<u8> {
    let list = axcenity_scene::build_draw_list(world, assets, camera.to_render_camera());
    draw(renderer, &list)
}

/// How many pixels differ in color between two frames.
pub(super) fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count()
}

/// The pixels (by index, every second one each way) where each of `names`
/// is the nearest part on screen.
pub(super) fn pixels_of(world: &World, camera: &FlyCamera, names: &[&str]) -> Vec<Vec<usize>> {
    let (width, height) = SIZE;
    let view = CameraView::new(camera.to_render_camera(), width as f32 / height as f32);
    let ids: Vec<_> = names
        .iter()
        .map(|name| world.find_first_child(world.workspace(), name).unwrap())
        .collect();
    let mut found = vec![Vec::new(); names.len()];
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let screen = Vec2::new(
                (x as f32 + 0.5) / width as f32 * 2.0 - 1.0,
                1.0 - (y as f32 + 0.5) / height as f32 * 2.0,
            );
            let Some(hit) = axcenity_scene::pick(world, view.ray(screen)) else {
                continue;
            };
            if let Some(index) = ids.iter().position(|&id| id == hit) {
                found[index].push((y * width + x) as usize);
            }
        }
    }
    found
}

/// The average red, green and blue of `pixels` at `at`, from 0 to 255.
pub(super) fn average(pixels: &[u8], at: &[usize]) -> [f64; 3] {
    let mut sum = [0.0; 3];
    for &pixel in at {
        for (channel, total) in sum.iter_mut().enumerate() {
            *total += f64::from(pixels[pixel * 4 + channel]);
        }
    }
    sum.map(|total| total / at.len() as f64)
}

/// hello-graphics' reference image for `quality`. High's is the one the
/// every-example test also checks.
fn reference(quality: Quality) -> std::path::PathBuf {
    let name = match quality {
        Quality::High => "reference.png".to_owned(),
        other => format!("reference-{}.png", other.label().to_lowercase()),
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-graphics")
        .join(name)
}

/// Each preset draws hello-graphics as its reference image shows. With
/// `AXCENITY_WRITE_REFERENCES` set, the Low, Medium and Ultra images are
/// written first (look at them before committing); High's `reference.png`
/// is left alone, since every example's test checks it too.
#[test]
fn hello_graphics_matches_each_presets_reference() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (world, _scripts, assets, camera) = hello_graphics();
    let write = std::env::var_os("AXCENITY_WRITE_REFERENCES").is_some();
    let mut frames = Vec::new();
    for quality in Quality::ALL {
        let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
        renderer.set_graphics(&GraphicsSettings::preset(quality));
        let pixels = frame(&mut renderer, &world, &assets, &camera);
        assert!(assets.take_errors().is_empty());
        let path = reference(quality);
        if write && quality != Quality::High {
            write_png(&path, &pixels, SIZE);
        }
        let average = difference_from(&path, &pixels, SIZE.0)
            .unwrap_or_else(|| panic!("{} is missing", path.display()));
        assert!(
            average < REFERENCE_TOLERANCE,
            "{quality:?} differs from {} by {average:.1} per channel on average",
            path.display()
        );
        frames.push(pixels);
    }
    // Each preset draws a different picture. High and Ultra differ only in
    // their shadow maps, so least.
    for (a, b) in frames.iter().zip(&frames[1..]) {
        let changed = changed_pixels(a, b);
        assert!(
            changed > 100,
            "only {changed} pixels differ between presets"
        );
    }
}

/// The sun, the sky's light and a lamp's color each change metal, rough
/// plastic, glass and both textured parts the same way, from the frame
/// that matches the reference.
#[test]
fn lighting_changes_move_every_kind_of_material_the_same_way() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (mut world, _scripts, assets, camera) = hello_graphics();
    let names = ["Metal", "Plastic", "Glass", "PbrPanel", "Tiled"];
    let at = pixels_of(&world, &camera, &names);
    for (name, at) in names.iter().zip(&at) {
        assert!(at.len() > 100, "{name} covers only {} pixels", at.len());
    }
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    let base = frame(&mut renderer, &world, &assets, &camera);
    let from_reference = difference_from(&reference(Quality::High), &base, SIZE.0).unwrap();
    assert!(from_reference < REFERENCE_TOLERANCE);
    let base: Vec<[f64; 3]> = at.iter().map(|at| average(&base, at)).collect();
    // How each material's average color moved from the base frame.
    let moved = |pixels: &[u8]| -> Vec<[f64; 3]> {
        at.iter()
            .zip(&base)
            .map(|(at, base)| {
                let now = average(pixels, at);
                [0, 1, 2].map(|channel| now[channel] - base[channel])
            })
            .collect()
    };

    // More light from the sky brightens every channel of every material.
    let lighting = world.lighting();
    world
        .set_property(lighting, "EnvironmentIntensity", Value::Number(1.5))
        .unwrap();
    let brighter = moved(&frame(&mut renderer, &world, &assets, &camera));
    for (name, change) in names.iter().zip(&brighter) {
        assert!(
            change.iter().all(|&c| c > 2.0) && change.iter().sum::<f64>() > 10.0,
            "{name} with more sky light: {change:?}"
        );
    }
    world
        .set_property(lighting, "EnvironmentIntensity", Value::Number(0.5))
        .unwrap();

    // The sun turned below the horizon darkens them all.
    let mut list = axcenity_scene::build_draw_list(&world, &assets, camera.to_render_camera());
    list.sun.direction = Vec3::new(0.5, -1.0, 0.25);
    let darker = moved(&draw(&mut renderer, &list));
    for (name, change) in names.iter().zip(&darker) {
        assert!(
            change.iter().all(|&c| c < 0.5) && change.iter().sum::<f64>() < -10.0,
            "{name} with the sun down: {change:?}"
        );
    }

    // A blue lamp in place of a warm one makes them all bluer and less red.
    let lamp = world.find_first_child(world.workspace(), "Lamp").unwrap();
    let light = world.find_first_child(lamp, "PointLight").unwrap();
    world
        .set_property(light, "Color", Value::Color3(Color3::new(0.3, 0.5, 1.0)))
        .unwrap();
    let bluer = moved(&frame(&mut renderer, &world, &assets, &camera));
    for (name, [red, _, blue]) in names.iter().zip(&bluer) {
        assert!(
            *red < 0.0 && blue - red > 0.5,
            "{name} under a blue lamp: red {red:+.1}, blue {blue:+.1}"
        );
    }
}

/// A LocalScript changes the shadows and anti-aliasing through
/// `UserGameSettings` (which the player saves in `graphics.json`), and the
/// exposure and tone mapping through Lighting; each shows in the next frame.
#[test]
fn a_local_script_changes_shadows_anti_aliasing_exposure_and_tone_mapping() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (mut world, mut scripts, assets, camera) = hello_graphics();
    let data = std::env::temp_dir().join(format!("axcenity-graphics-data-{}", std::process::id()));
    let _ = fs::remove_dir_all(&data);
    let mut renderer = TextureRenderer::new(&device, &queue, 160, 90);
    let capabilities = *renderer.capabilities();
    let start = GraphicsSettings::default();
    scripts.set_graphics(&start, &capabilities);
    renderer.set_graphics(&start);
    let mut before = frame(&mut renderer, &world, &assets, &camera);
    let (mut physics, mut clock) = (Physics::new(), 60);
    for (source, at_least) in [
        (
            "game:GetService('UserGameSettings').ShadowQuality = Enum.ShadowQuality.Off",
            100,
        ),
        (
            "game:GetService('UserGameSettings').AntiAliasing = Enum.AntiAliasing.Off",
            20,
        ),
        ("game.Lighting.Exposure = 2", 5000),
        ("game.Lighting.ToneMapping = Enum.ToneMapping.Filmic", 5000),
    ] {
        let script = world.create(ClassName::LocalScript).unwrap();
        world
            .set_parent(script, Some(world.starter_player_scripts()))
            .unwrap();
        scripts.start_script(&mut world, script, "scripts/menu.client.lua", source);
        let until = f64::from(clock) / 60.0 + 0.05;
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, until);
        // As the frame loop does before drawing.
        if let Some(change) = scripts.take_graphics_change() {
            let settings = crate::graphics::changed(Some(&data), change, &capabilities);
            renderer.set_graphics(&settings);
        }
        let after = frame(&mut renderer, &world, &assets, &camera);
        let changed = changed_pixels(&before, &after);
        assert!(
            changed >= at_least,
            "`{source}` changed only {changed} pixels"
        );
        before = after;
    }
    assert_eq!(renderer.graphics().shadow_quality, ShadowQuality::Off);
    assert_eq!(renderer.graphics().anti_aliasing, AntiAliasing::Off);
    let saved = crate::graphics::load(&data).unwrap();
    assert_eq!(
        (saved.shadow_quality, saved.anti_aliasing),
        (Some(ShadowQuality::Off), Some(AntiAliasing::Off))
    );
    fs::remove_dir_all(&data).unwrap();
}

/// On a GPU without multisampling and with small textures, Ultra draws
/// with what it can (FXAA, smaller shadow maps), says why, and doesn't
/// crash. Even a tiny texture size draws the scene.
#[test]
fn a_gpu_that_cant_do_ultra_draws_hello_graphics_with_fallbacks_and_notes() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let (world, _scripts, assets, camera) = hello_graphics();
    let mut renderer = TextureRenderer::new(&device, &queue, SIZE.0, SIZE.1);
    renderer.set_graphics(&GraphicsSettings::preset(Quality::Ultra));
    let full = frame(&mut renderer, &world, &assets, &camera);
    renderer.limit_capabilities(&Capabilities {
        max_texture_size: 1024,
        msaa_samples: 1,
        ..*renderer.capabilities()
    });
    let used = renderer.graphics().clone();
    assert_eq!(used.anti_aliasing, AntiAliasing::Fxaa);
    assert_eq!(used.shadow_quality, ShadowQuality::Low);
    assert_eq!(used.notes.len(), 2, "{:?}", used.notes);
    assert!(used.notes[0].contains("Ultra to Low"), "{:?}", used.notes);
    assert!(used.notes[1].contains("to FXAA"), "{:?}", used.notes);
    let lowered = frame(&mut renderer, &world, &assets, &camera);
    assert!(assets.take_errors().is_empty());
    assert!(
        changed_pixels(&full, &lowered) > 0,
        "the fallbacks draw differently"
    );
    let from_ultra = difference_from(&reference(Quality::Ultra), &lowered, SIZE.0).unwrap();
    assert!(
        from_ultra < 2.0 * REFERENCE_TOLERANCE,
        "still the same scene: {from_ultra:.1} from Ultra's reference"
    );

    // Textures no bigger than 64 texels: no shadow map fits, and the scene
    // is drawn 64 wide and stretched to the frame.
    renderer.limit_capabilities(&Capabilities {
        max_texture_size: 64,
        ..*renderer.capabilities()
    });
    assert_eq!(renderer.graphics().shadow_quality, ShadowQuality::Off);
    let tiny = frame(&mut renderer, &world, &assets, &camera);
    let colors: std::collections::BTreeSet<[u8; 3]> = tiny
        .chunks(4)
        .map(|pixel| [pixel[0] / 16, pixel[1] / 16, pixel[2] / 16])
        .collect();
    assert!(colors.len() > 8, "draws more than one color");
}
