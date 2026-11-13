//! Lens effects (graphics Phase 34): Lighting's first enabled
//! `DepthOfFieldEffect`, `VignetteEffect`, `FilmGrainEffect`,
//! `ChromaticAberrationEffect`, `SharpenEffect`, (graphics Phase 35)
//! `MotionBlurEffect` and `SunRaysEffect`. Without one of a kind there is none of it.

use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::{DepthOfField, FilmGrain, LensEffects, SunRays, Vignette};

/// The lens effects inside Lighting.
pub(crate) fn lens_of(world: &World) -> LensEffects {
    let Some(lighting) = world.get(world.lighting()) else {
        return LensEffects::default();
    };
    // The first enabled effect of `class` in Lighting, in Explorer order.
    let first = |class| {
        lighting
            .children()
            .iter()
            .filter_map(|&id| world.get(id))
            .find(|child| {
                child.class() == class && child.property("Enabled") != Some(&Value::Bool(false))
            })
    };
    LensEffects {
        depth_of_field: first(ClassName::DepthOfFieldEffect).map(|effect| DepthOfField {
            focus_distance: number(effect, "FocusDistance", 20.0),
            in_focus_radius: number(effect, "InFocusRadius", 10.0),
            near_intensity: number(effect, "NearIntensity", 0.75),
            far_intensity: number(effect, "FarIntensity", 0.75),
        }),
        vignette: first(ClassName::VignetteEffect).map(|effect| Vignette {
            intensity: number(effect, "Intensity", 0.5),
            size: number(effect, "Size", 0.5),
            color: effect
                .property("Color")
                .and_then(Value::as_color3)
                .map_or([0.0; 3], |color| [color.r, color.g, color.b]),
        }),
        film_grain: first(ClassName::FilmGrainEffect).map(|effect| FilmGrain {
            intensity: number(effect, "Intensity", 0.3),
            size: number(effect, "Size", 1.0),
        }),
        chromatic_aberration: first(ClassName::ChromaticAberrationEffect)
            .map(|effect| number(effect, "Intensity", 0.5)),
        sharpen: first(ClassName::SharpenEffect).map(|effect| number(effect, "Intensity", 0.5)),
        motion_blur: first(ClassName::MotionBlurEffect)
            .map(|effect| number(effect, "Intensity", 0.5)),
        sun_rays: first(ClassName::SunRaysEffect).map(|effect| SunRays {
            intensity: number(effect, "Intensity", 0.25).clamp(0.0, 1.0),
            spread: number(effect, "Spread", 0.5).clamp(0.0, 1.0),
        }),
    }
}

/// `effect`'s number `name`, or `default` if it isn't a number.
fn number(effect: &Instance, name: &str, default: f32) -> f32 {
    match effect.property(name) {
        Some(Value::Number(value)) if value.is_finite() => *value as f32,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Color3, InstanceId};

    fn set_number(world: &mut World, id: InstanceId, value: f64) {
        world
            .set_property(id, "Intensity", Value::Number(value))
            .unwrap();
    }

    fn add(world: &mut World, class: ClassName) -> InstanceId {
        let effect = world.create(class).unwrap();
        world.set_parent(effect, Some(world.lighting())).unwrap();
        effect
    }

    #[test]
    fn each_lens_effect_in_lighting_draws_unless_disabled() {
        let mut world = World::new();
        assert_eq!(
            lens_of(&world),
            LensEffects::default(),
            "a new world has none"
        );

        let blur = add(&mut world, ClassName::DepthOfFieldEffect);
        let vignette = add(&mut world, ClassName::VignetteEffect);
        add(&mut world, ClassName::FilmGrainEffect);
        add(&mut world, ClassName::ChromaticAberrationEffect);
        add(&mut world, ClassName::SharpenEffect);
        add(&mut world, ClassName::SunRaysEffect);
        let streak = add(&mut world, ClassName::MotionBlurEffect);
        assert_eq!(
            lens_of(&world),
            LensEffects {
                depth_of_field: Some(DepthOfField {
                    focus_distance: 20.0,
                    in_focus_radius: 10.0,
                    near_intensity: 0.75,
                    far_intensity: 0.75,
                }),
                vignette: Some(Vignette {
                    intensity: 0.5,
                    size: 0.5,
                    color: [0.0; 3],
                }),
                film_grain: Some(FilmGrain {
                    intensity: 0.3,
                    size: 1.0,
                }),
                chromatic_aberration: Some(0.5),
                sharpen: Some(0.5),
                motion_blur: Some(0.5),
                sun_rays: Some(SunRays {
                    intensity: 0.25,
                    spread: 0.5,
                }),
            },
            "each class's defaults"
        );
        set_number(&mut world, streak, 1.0);
        assert_eq!(lens_of(&world).motion_blur, Some(1.0));
        world
            .set_property(streak, "Enabled", Value::Bool(false))
            .unwrap();
        assert_eq!(lens_of(&world).motion_blur, None);

        let set = |world: &mut World, id, name: &str, value| {
            world.set_property(id, name, value).unwrap();
        };
        set(&mut world, blur, "FocusDistance", Value::Number(5.0));
        set(&mut world, blur, "FarIntensity", Value::Number(1.0));
        set(
            &mut world,
            vignette,
            "Color",
            Value::Color3(Color3::new(1.0, 0.0, 0.0)),
        );
        let lens = lens_of(&world);
        let blurred = lens.depth_of_field.unwrap();
        assert_eq!((blurred.focus_distance, blurred.far_intensity), (5.0, 1.0));
        assert_eq!(lens.vignette.unwrap().color, [1.0, 0.0, 0.0]);

        // Disabled, the first is skipped for the next enabled one.
        set(&mut world, vignette, "Enabled", Value::Bool(false));
        assert_eq!(lens_of(&world).vignette, None);
        let second = add(&mut world, ClassName::VignetteEffect);
        set(&mut world, second, "Intensity", Value::Number(0.25));
        assert_eq!(lens_of(&world).vignette.unwrap().intensity, 0.25);

        // One anywhere else does nothing.
        world.set_parent(blur, Some(world.workspace())).unwrap();
        assert_eq!(lens_of(&world).depth_of_field, None);
    }
}
