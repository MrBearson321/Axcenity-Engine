//! Color grading (graphics Phase 32): Lighting's enabled
//! `ColorCorrectionEffect`s, together. Without one there is none.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::{ColorGrading, ColorLookup, MAX_PALETTE_COLORS};

/// The grading Lighting's enabled `ColorCorrectionEffect`s make, if it has
/// any. Several add their brightness, contrast and saturation and multiply
/// their tints; the first (in Explorer order) with a lookup table, a
/// posterize level or a palette gives that.
pub(crate) fn grading_of(world: &World, assets: &Assets) -> Option<ColorGrading> {
    grading_in(world, world.get(world.lighting())?, assets)
}

/// Like [`grading_of`], from the enabled `ColorCorrectionEffect`s inside
/// `holder` (Lighting or a GraphicsVolume).
pub(crate) fn grading_in(
    world: &World,
    holder: &Instance,
    assets: &Assets,
) -> Option<ColorGrading> {
    let effects: Vec<&Instance> = holder
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .filter(|child| {
            child.class() == ClassName::ColorCorrectionEffect
                && child.property("Enabled") != Some(&Value::Bool(false))
        })
        .collect();
    if effects.is_empty() {
        return None;
    }
    let mut grading = ColorGrading::default();
    for effect in &effects {
        let number = |name| match effect.property(name) {
            Some(Value::Number(value)) if value.is_finite() => *value as f32,
            _ => 0.0,
        };
        grading.brightness += number("Brightness");
        grading.contrast += number("Contrast");
        grading.saturation += number("Saturation");
        if let Some(tint) = effect.property("TintColor").and_then(Value::as_color3) {
            for (channel, by) in grading.tint.iter_mut().zip([tint.r, tint.g, tint.b]) {
                *channel *= by;
            }
        }
        let path = |name| effect.property(name).and_then(Value::as_str);
        if grading.lookup.is_none()
            && let Some(table) = path("LookupTable").and_then(|path| assets.color_lookup(path))
        {
            grading.lookup = Some(ColorLookup {
                key: table.key(),
                size: table.size,
                rgb: table.rgb.clone(),
            });
        }
        if grading.posterize == 0 && number("Posterize") >= 2.0 {
            grading.posterize = number("Posterize").min(256.0) as u32;
        }
        if grading.palette.is_empty()
            && let Some(palette) = path("Palette").and_then(|path| assets.palette(path))
        {
            grading
                .palette
                .extend(palette.colors.iter().take(MAX_PALETTE_COLORS));
        }
    }
    Some(grading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Color3, InstanceId};

    fn effect(world: &mut World) -> InstanceId {
        let effect = world.create(ClassName::ColorCorrectionEffect).unwrap();
        world.set_parent(effect, Some(world.lighting())).unwrap();
        effect
    }

    fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
        world.set_property(id, name, value).unwrap();
    }

    #[test]
    fn lightings_color_correction_effects_grade_together_unless_disabled() {
        let assets = Assets::default();
        let mut world = World::new();
        assert_eq!(grading_of(&world, &assets), None, "a new world has none");
        let first = effect(&mut world);
        assert_eq!(grading_of(&world, &assets), Some(ColorGrading::default()));

        set(&mut world, first, "Brightness", Value::Number(0.1));
        set(&mut world, first, "Contrast", Value::Number(0.5));
        set(&mut world, first, "Saturation", Value::Number(-0.5));
        set(
            &mut world,
            first,
            "TintColor",
            Value::Color3(Color3::new(1.0, 0.5, 0.5)),
        );
        set(&mut world, first, "Posterize", Value::Number(4.0));
        let second = effect(&mut world);
        set(&mut world, second, "Brightness", Value::Number(0.2));
        set(
            &mut world,
            second,
            "TintColor",
            Value::Color3(Color3::new(0.5, 1.0, 0.5)),
        );
        set(&mut world, second, "Posterize", Value::Number(8.0));
        let grading = grading_of(&world, &assets).unwrap();
        assert!((grading.brightness - 0.3).abs() < 1e-6);
        assert_eq!(
            (grading.contrast, grading.saturation),
            (0.5, -0.5),
            "contrast and saturation add"
        );
        assert_eq!(grading.tint, [0.5, 0.5, 0.25], "tints multiply");
        assert_eq!(grading.posterize, 4, "the first one's levels");

        set(&mut world, first, "Enabled", Value::Bool(false));
        set(&mut world, second, "Enabled", Value::Bool(false));
        assert_eq!(grading_of(&world, &assets), None, "both off");

        // One anywhere else does nothing.
        set(&mut world, first, "Enabled", Value::Bool(true));
        world.set_parent(first, Some(world.workspace())).unwrap();
        assert_eq!(grading_of(&world, &assets), None);
    }

    #[test]
    fn a_lookup_table_and_a_palette_come_from_the_projects_files() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-scene-grading-{}", std::process::id()));
        let grading_folder = folder.join("assets/grading");
        std::fs::create_dir_all(&grading_folder).unwrap();
        let mut cube = "LUT_3D_SIZE 2\n".to_owned();
        for blue in 0..2 {
            for green in 0..2 {
                for red in 0..2 {
                    cube.push_str(&format!("{} {} {}\n", 1 - red, 1 - green, 1 - blue));
                }
            }
        }
        std::fs::write(grading_folder.join("invert.cube"), cube).unwrap();
        let palette = axcenity_assets::Image::new(2, 1, vec![0, 0, 0, 255, 255, 0, 0, 255]);
        image_file(&grading_folder.join("two.png"), &palette);

        let assets = Assets::new(&folder);
        let mut world = World::new();
        let id = effect(&mut world);
        let path = |path: &str| Value::String(path.to_owned());
        set(
            &mut world,
            id,
            "LookupTable",
            path("assets/grading/invert.cube"),
        );
        set(&mut world, id, "Palette", path("assets/grading/two.png"));
        let grading = grading_of(&world, &assets).unwrap();
        let lookup = grading.lookup.unwrap();
        assert_eq!(lookup.size, 2);
        assert_eq!(lookup.rgb[..3], [1.0, 1.0, 1.0]);
        assert_eq!(grading.palette, vec![[0.0; 3], [1.0, 0.0, 0.0]]);

        // A missing file leaves colors alone, and is reported once.
        set(
            &mut world,
            id,
            "LookupTable",
            path("assets/grading/gone.cube"),
        );
        assert_eq!(grading_of(&world, &assets).unwrap().lookup, None);
        grading_of(&world, &assets);
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("gone.cube"));
        let _ = std::fs::remove_dir_all(folder);
    }

    fn image_file(path: &std::path::Path, image: &axcenity_assets::Image) {
        let mut bytes = Vec::new();
        image::write_buffer_with_format(
            &mut std::io::Cursor::new(&mut bytes),
            image.rgba(),
            image.width,
            image.height,
            image::ExtendedColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
