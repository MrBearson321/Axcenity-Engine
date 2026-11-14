//! Tests for `UserGameSettings`: LocalScripts reading and changing the
//! player's graphics settings, and the host taking the change.

use axcenity_core::{ClassName, World};
use axcenity_render_types::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, GraphicsSettings, Quality,
    ReflectionQuality, ShadowQuality, TextureQuality,
};

use super::*;

/// Starts `source` as a LocalScript (or a Script, for a `.server.lua` path)
/// and returns what it printed, errors starting with `error: `.
fn run(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) -> Vec<String> {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

#[test]
fn a_local_script_reads_the_settings_the_host_gave() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let settings = GraphicsSettings {
        shadow_quality: Some(ShadowQuality::Low),
        render_scale: Some(1.5),
        ..GraphicsSettings::preset(Quality::Medium)
    };
    runtime.set_graphics(&settings, &Capabilities::default());
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/read.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.GraphicsQuality, s.ShadowQuality, s.ShadowDistance, s.PostProcessing)
        print(s.RenderScale, s.TextureQuality, s.AntiAliasing, s.AmbientOcclusion)
        print(s.ReflectionQuality, s.ContactShadows, s.RealtimeGI, s.VolumetricFog)
        print(s.CloudQuality)
        print(s == game.UserGameSettings, tostring(s))",
    );
    assert_eq!(
        printed,
        [
            "Enum.GraphicsQuality.Medium\tEnum.ShadowQuality.Low\t100\ttrue",
            "1.5\tEnum.TextureQuality.Half\tEnum.AntiAliasing.FXAA\tEnum.AmbientOcclusion.Low",
            "Enum.ReflectionQuality.Medium\tfalse\tfalse\tfalse",
            "Enum.CloudQuality.Medium",
            "false\tUserGameSettings",
        ]
    );
    // Telling scripts isn't a change for the host to apply.
    assert_eq!(runtime.take_graphics_change(), None);
}

#[test]
fn setting_the_quality_and_overrides_is_a_change_the_host_takes_once() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_graphics(&GraphicsSettings::default(), &Capabilities::default());
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/menu.client.lua",
        "local s = game:GetService('UserGameSettings')
        s.ShadowQuality = Enum.ShadowQuality.Off
        s.GraphicsQuality = Enum.GraphicsQuality.Low
        s.AntiAliasing = Enum.AntiAliasing.MSAA2
        s.TextureQuality = Enum.TextureQuality.Full
        s.AmbientOcclusion = Enum.AmbientOcclusion.High
        s.ReflectionQuality = Enum.ReflectionQuality.Ultra
        s.CloudQuality = Enum.CloudQuality.Low
        s.PostProcessing = true
        s.ContactShadows = false
        s.RealtimeGI = true
        s.VolumetricFog = true
        s.ShadowDistance = 40
        s.RenderScale = 5
        print(s.GraphicsQuality, s.ShadowQuality, s.RenderScale)",
    );
    assert_eq!(
        printed,
        ["Enum.GraphicsQuality.Low\tEnum.ShadowQuality.Off\t2"]
    );
    // Changing the preset kept the shadow override, as in the editor.
    let expected = GraphicsSettings {
        quality: Quality::Low,
        automatic: false,
        shadow_quality: Some(ShadowQuality::Off),
        shadow_distance: Some(40.0),
        post_processing: Some(true),
        render_scale: Some(2.0),
        texture_quality: Some(TextureQuality::Full),
        texture_budget: None,
        anti_aliasing: Some(AntiAliasing::Msaa2),
        ambient_occlusion: Some(AmbientOcclusion::High),
        reflection_quality: Some(ReflectionQuality::Ultra),
        contact_shadows: Some(false),
        realtime_gi: Some(true),
        volumetric_fog: Some(true),
        cloud_quality: Some(CloudQuality::Low),
        ..GraphicsSettings::default()
    };
    assert_eq!(runtime.take_graphics_change(), Some(expected.clone()));
    assert_eq!(runtime.take_graphics_change(), None);

    // nil goes back to the preset's value.
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/reset.client.lua",
        "local s = game:GetService('UserGameSettings')
        s.ShadowQuality = nil
        s.ShadowDistance = nil
        print(s.ShadowQuality, s.ShadowDistance)",
    );
    assert_eq!(printed, ["Enum.ShadowQuality.Low\t75"]);
    let change = runtime.take_graphics_change().unwrap();
    assert_eq!(
        (change.shadow_quality, change.shadow_distance),
        (None, None)
    );
}

#[test]
fn each_lens_effect_has_its_own_setting_for_local_scripts() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_graphics(
        &GraphicsSettings::preset(Quality::Low),
        &Capabilities::default(),
    );
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/lens.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.DepthOfField, s.Vignette, s.FilmGrain, s.ChromaticAberration, s.Sharpen, s.SunRays)
        s.DepthOfField = Enum.DepthOfField.High
        s.Vignette = true
        s.FilmGrain = true
        s.ChromaticAberration = false
        s.Sharpen = true
        s.SunRays = true
        print(s.DepthOfField, s.Vignette, s.FilmGrain, s.ChromaticAberration, s.Sharpen, s.SunRays)
        s.DepthOfField = Enum.DepthOfField.Low
        print(s.DepthOfField)
        print(pcall(function() s.DepthOfField = Enum.CloudQuality.Low end))",
    );
    assert_eq!(
        printed[0],
        "Enum.DepthOfField.Off	false	false	false	false	false"
    );
    assert_eq!(
        printed[1],
        "Enum.DepthOfField.High	true	true	false	true	true"
    );
    assert_eq!(printed[2], "Enum.DepthOfField.Low");
    assert!(printed[3].starts_with("false	"), "{printed:?}");
    let change = runtime.take_graphics_change().unwrap();
    assert_eq!(
        change,
        GraphicsSettings {
            depth_of_field: Some(axcenity_render_types::DepthOfFieldQuality::Low),
            vignette: Some(true),
            film_grain: Some(true),
            chromatic_aberration: Some(false),
            sharpen: Some(true),
            sun_rays: Some(true),
            ..GraphicsSettings::preset(Quality::Low)
        }
    );
}

#[test]
fn taa_and_motion_blur_are_settings_for_local_scripts() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_graphics(
        &GraphicsSettings::preset(Quality::Medium),
        &Capabilities::default(),
    );
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/motion.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.AntiAliasing, s.MotionBlur)
        s.AntiAliasing = Enum.AntiAliasing.TAA
        s.MotionBlur = Enum.MotionBlur.High
        print(s.AntiAliasing, s.MotionBlur)
        s.MotionBlur = Enum.MotionBlur.Off
        print(s.MotionBlur)
        print(pcall(function() s.MotionBlur = Enum.DepthOfField.Low end))",
    );
    assert_eq!(printed[0], "Enum.AntiAliasing.FXAA	Enum.MotionBlur.Low");
    assert_eq!(printed[1], "Enum.AntiAliasing.TAA	Enum.MotionBlur.High");
    assert_eq!(printed[2], "Enum.MotionBlur.Off");
    assert!(printed[3].starts_with("false	"), "{printed:?}");
    let change = runtime.take_graphics_change().unwrap();
    assert_eq!(
        change,
        GraphicsSettings {
            anti_aliasing: Some(axcenity_render_types::AntiAliasing::Taa),
            motion_blur: Some(axcenity_render_types::MotionBlurQuality::Off),
            ..GraphicsSettings::preset(Quality::Medium)
        }
    );
}

#[test]
fn ray_traced_shadows_are_a_setting_for_local_scripts() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_graphics(
        &GraphicsSettings::preset(Quality::High),
        &Capabilities::default(),
    );
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/rays.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.RayTracedShadows, s.RayTracedAmbientOcclusion, s.RayTracedGlobalIllumination)
        s.RayTracedShadows = Enum.RayTracedShadows.Medium
        s.RayTracedAmbientOcclusion = Enum.RayTracedAmbientOcclusion.High
        s.RayTracedGlobalIllumination = Enum.RayTracedGlobalIllumination.Low
        print(s.RayTracedShadows, s.RayTracedAmbientOcclusion, s.RayTracedGlobalIllumination)
        print(pcall(function() s.RayTracedShadows = Enum.MotionBlur.Low end))",
    );
    assert_eq!(
        printed[0],
        "Enum.RayTracedShadows.Off\tEnum.RayTracedAmbientOcclusion.Off\tEnum.RayTracedGlobalIllumination.Off"
    );
    assert_eq!(
        printed[1],
        "Enum.RayTracedShadows.Medium\tEnum.RayTracedAmbientOcclusion.High\tEnum.RayTracedGlobalIllumination.Low"
    );
    assert!(printed[2].starts_with("false\t"), "{printed:?}");
    let change = runtime.take_graphics_change().unwrap();
    assert_eq!(
        change,
        GraphicsSettings {
            ray_traced_shadows: Some(axcenity_render_types::RayTracedShadows::Medium),
            ray_traced_ambient_occlusion: Some(
                axcenity_render_types::RayTracedAmbientOcclusion::High
            ),
            ray_traced_global_illumination: Some(
                axcenity_render_types::RayTracedGlobalIllumination::Low
            ),
            ..GraphicsSettings::preset(Quality::High)
        }
    );
}

#[test]
fn automatic_picks_the_machines_quality() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let software = Capabilities {
        software: true,
        ..Capabilities::default()
    };
    runtime.set_graphics(&GraphicsSettings::preset(Quality::Ultra), &software);
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/auto.client.lua",
        "local s = game:GetService('UserGameSettings')
        s.GraphicsQuality = Enum.GraphicsQuality.Automatic
        print(s.GraphicsQuality, s.ShadowQuality)",
    );
    assert_eq!(
        printed,
        ["Enum.GraphicsQuality.Automatic\tEnum.ShadowQuality.Low"]
    );
    let change = runtime.take_graphics_change().unwrap();
    assert_eq!((change.quality, change.automatic), (Quality::Low, true));
}

#[test]
fn wrong_values_are_errors_that_change_nothing() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    for (source, message) in [
        (
            "s.GraphicsQuality = 'High'",
            "UserGameSettings.GraphicsQuality must be an Enum.GraphicsQuality, such as \
             Enum.GraphicsQuality.High, not a string",
        ),
        ("s.GraphicsQuality = nil", "must be an Enum.GraphicsQuality"),
        (
            "s.ShadowQuality = Enum.AntiAliasing.Off",
            "UserGameSettings.ShadowQuality must be an Enum.ShadowQuality, such as \
             Enum.ShadowQuality.Ultra, or nil for the preset's, not a AntiAliasing",
        ),
        (
            "s.ShadowDistance = 'far'",
            "UserGameSettings.ShadowDistance must be a number, or nil for the preset's, not a string",
        ),
        (
            "s.RenderScale = 0/0",
            "UserGameSettings.RenderScale must be a finite number",
        ),
        (
            "s.PostProcessing = 1",
            "UserGameSettings.PostProcessing must be true, false, or nil for the preset's",
        ),
        (
            "s.ContactShadows = 'yes'",
            "UserGameSettings.ContactShadows must be true, false, or nil for the preset's, not a string",
        ),
    ] {
        let printed = run(
            &mut runtime,
            &mut world,
            "scripts/wrong.client.lua",
            &format!("local s = game:GetService('UserGameSettings')\n{source}"),
        );
        assert_eq!(printed.len(), 1, "{source}: {printed:?}");
        assert!(printed[0].contains(message), "{source}: {printed:?}");
    }
    assert_eq!(runtime.take_graphics_change(), None);
}

#[test]
fn a_script_on_the_server_is_told_to_use_a_local_script() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    for source in [
        "local s = game:GetService('UserGameSettings')",
        "local s = game.UserGameSettings",
    ] {
        let printed = run(
            &mut runtime,
            &mut world,
            "scripts/server.server.lua",
            source,
        );
        assert_eq!(printed.len(), 1, "{printed:?}");
        assert!(
            printed[0].contains("UserGameSettings only works in a LocalScript"),
            "{printed:?}"
        );
    }
}

#[test]
fn a_game_turns_hardware_ray_tracing_on_for_its_next_start() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // A GPU that could ray trace, opened without it this run.
    let gpu = Capabilities {
        ray_tracing_offered: true,
        ..Capabilities::default()
    };
    runtime.set_graphics(&GraphicsSettings::preset(Quality::High), &gpu);
    let printed = run(
        &mut runtime,
        &mut world,
        "scripts/rays.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.HardwareRayTracing, s.RayTracingSupported, s.RayTracingActive)
        s.HardwareRayTracing = true
        print(s.HardwareRayTracing, s.RayTracingActive)
        print(pcall(function() s.RayTracingActive = true end))
        print(pcall(function() s.HardwareRayTracing = 'yes' end))",
    );
    assert_eq!(printed[0], "false\ttrue\tfalse");
    assert_eq!(printed[1], "true\tfalse", "active only after a restart");
    assert!(printed[2].starts_with("false"), "{printed:?}");
    assert!(printed[3].starts_with("false\t") && printed[3].contains("HardwareRayTracing"));
    let change = runtime
        .take_graphics_change()
        .expect("the host sees the change");
    assert_eq!(change.ray_tracing, Some(true));
}
