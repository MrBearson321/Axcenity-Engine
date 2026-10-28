//! Materials' own surface shaders: drawn with their own pipelines, moving
//! with the draw list's time, and drawn in the error color (and reported
//! once) when they don't compile.

use super::*;
use crate::{AmbientOcclusion, AntiAliasing, SurfaceShader};

/// Scrolls the material's color map along U a quarter of the texture per
/// second, and gives it off as light so the sun doesn't matter.
const SCROLLING: &str = "\
fn surface(input: SurfaceInput) -> Surface {
    var moved = input;
    moved.uv = input.uv + vec2<f32>(input.time * 0.25, 0.0);
    var out = standard_surface(moved);
    out.emissive = out.base_color;
    out.base_color = vec3<f32>(0.0);
    return out;
}
";

fn shaded_block(code: &str) -> DrawItem {
    let mut block = textured_block(Checker::texture(30, 2));
    block.material.surface_shader = Some(SurfaceShader::new("scrolling", code));
    block
}

#[test]
fn a_surface_shader_scrolls_its_texture_with_the_draw_lists_time() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![shaded_block(SCROLLING)]);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut frame_at = |time: f32| {
        list.time = time;
        renderer.render(&list).unwrap();
        renderer.read_pixels()
    };
    let start = frame_at(0.0);
    let half_way = frame_at(2.0);
    let around = frame_at(4.0);
    assert_ne!(start, half_way, "half a texture along, the checks swap");
    assert_eq!(start, around, "a whole texture along, it's back");
    assert!(renderer.take_shader_errors().is_empty());
    assert_eq!(renderer.scene.surface_shaders.alive(), (1, false));
}

#[test]
fn a_surface_shader_returning_the_standard_surface_draws_like_none() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut block = textured_block(Checker::texture(31, 4));
    block.material.metalness = 0.6;
    block.material.roughness = 0.3;
    let mut list = list_of(vec![block]);
    list.ambient.intensity = 0.5;
    list.sun.direction = Vec3::new(0.3, 0.5, 1.0);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.render(&list).unwrap();
    let standard = renderer.read_pixels();
    list.items[0].material.surface_shader = Some(SurfaceShader::new(
        "same",
        include_str!("../../surface.wgsl"),
    ));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.read_pixels(), standard);
}

#[test]
fn a_shader_with_an_error_is_reported_once_on_its_line_and_draws_magenta_alone() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let block = |x: f32| DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            glam::Quat::IDENTITY,
            Vec3::new(x, 0.0, -6.0),
        ),
        ..big_block()
    };
    let mut broken = block(-2.0);
    let code = SCROLLING.replace("var out =", "var out ==");
    broken.material.surface_shader = Some(SurfaceShader::new(
        "assets/materials/broken.material.json",
        &code,
    ));
    let mut green = block(2.0);
    green.material.emissive = [0.0, 1.0, 0.0];
    let list = list_of(vec![broken, green]);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);

    let fine = pixel_at(&mut renderer, &list, Vec3::new(2.0, 0.0, -5.0));
    assert!(
        fine[1] > 200 && fine[0] < 40,
        "the other block draws: {fine:?}"
    );
    let errors = renderer.take_shader_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].material, "assets/materials/broken.material.json");
    assert_eq!(errors[0].line, Some(4), "{}", errors[0]);
    let pixels = renderer.read_pixels();
    let magenta = pixels
        .chunks(4)
        .filter(|pixel| pixel[0] > 200 && pixel[1] < 40 && pixel[2] > 200)
        .count();
    assert!(magenta > 20, "{magenta} magenta pixels");

    renderer.render(&list).unwrap();
    assert!(renderer.take_shader_errors().is_empty(), "reported once");
    assert_eq!(renderer.scene.surface_shaders.alive(), (0, true));
}

#[test]
fn surface_shaders_draw_with_ambient_occlusion_and_msaa_and_are_compiled_again_for_new_samples() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![shaded_block(SCROLLING)]);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut frames = Vec::new();
    for (anti_aliasing, ambient_occlusion) in [
        (AntiAliasing::Off, AmbientOcclusion::High),
        (AntiAliasing::Msaa4, AmbientOcclusion::Low),
        (AntiAliasing::Fxaa, AmbientOcclusion::Off),
    ] {
        renderer.set_graphics(&GraphicsSettings {
            anti_aliasing: Some(anti_aliasing),
            ambient_occlusion: Some(ambient_occlusion),
            ..GraphicsSettings::default()
        });
        list.time = 0.0;
        renderer.render(&list).unwrap();
        frames.push(renderer.read_pixels());
        assert_eq!(renderer.scene.surface_shaders.alive(), (1, false));
    }
    assert!(renderer.take_shader_errors().is_empty());
    // Nothing undrawn is kept past its time.
    list.items.clear();
    for _ in 0..KEEP_UNDRAWN_FRAMES + 1 {
        renderer.scene.frame += 1;
    }
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.surface_shaders.alive(), (0, false));
}

#[test]
fn materials_differing_only_in_parameters_share_a_shader_and_draw_their_own_values() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Glows in the color in parameter 1, times the number in parameter 0.
    let code = "\
fn surface(input: SurfaceInput) -> Surface {
    var out = standard_surface(input);
    out.base_color = vec3<f32>(0.0);
    out.emissive = srgb(parameter(1).xyz) * parameter(0).x;
    return out;
}
";
    let block = |x: f32, color: [f32; 3]| {
        let mut block = big_block();
        block.transform = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 4.0, 4.0),
            glam::Quat::IDENTITY,
            Vec3::new(x, 0.0, -5.0),
        );
        block.material.surface_shader = Some(SurfaceShader::new("glow", code));
        block.material.surface_parameters[0] = [1.0, 0.0, 0.0, 0.0];
        block.material.surface_parameters[1] = [color[0], color[1], color[2], 0.0];
        block
    };
    let list = list_of(vec![
        block(-1.5, [1.0, 0.0, 0.0]),
        block(1.5, [0.0, 0.0, 1.0]),
    ]);
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let left = pixel_at(&mut renderer, &list, Vec3::new(-1.5, 0.0, -3.0));
    let right = pixel_at(&mut renderer, &list, Vec3::new(1.5, 0.0, -3.0));
    assert!(
        left[0] > 200 && left[2] < 30,
        "the left one is red: {left:?}"
    );
    assert!(
        right[2] > 200 && right[0] < 30,
        "the right one is blue: {right:?}"
    );
    assert!(renderer.take_shader_errors().is_empty());
    assert_eq!(
        renderer.scene.surface_shaders.alive(),
        (1, false),
        "they share the shader"
    );
}
