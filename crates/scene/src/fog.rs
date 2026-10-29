//! The world's fog (graphics Phase 25): an Atmosphere's height fog, or
//! else Lighting's distance fog (`FogStart`, `FogEnd`, `FogColor`).
//!
//! An Atmosphere's fog is densest at the bottom of the air and thins with
//! height; its `Offset` lifts the world above that bottom, so the higher
//! it is, the thinner the fog. Far things fade into the sky's color around
//! the horizon, and the sun glows in it. With an Atmosphere, Lighting's own
//! fog is left out.

use axcenity_core::{Instance, Value, World};
use axcenity_render::Fog;

/// Lighting's `FogEnd` default: further than anything is seen, so no fog.
const NO_FOG_END: f64 = 100_000.0;
/// How much light an Atmosphere of Earth's density (0.395) takes away per
/// stud at the bottom of its air: half of it over 500 studs.
const EARTH_FOG_DENSITY: f64 = std::f64::consts::LN_2 / 500.0;
const EARTH_DENSITY: f64 = 0.395;
/// An Atmosphere's fog thins by `e` every this many studs up.
const FOG_FALLOFF: f32 = 50.0;
/// How far below the world an Offset of 1 puts the bottom of the air, in
/// studs: five falloffs, so almost no fog is left.
const OFFSET_DEPTH: f64 = 250.0;
/// How much of the sun's light an Atmosphere's fog scatters, and how much
/// of it goes forward without Glare, and with each step of Glare.
const SUN_SCATTERING: f32 = 0.8;
const FORWARD_SCATTERING: f64 = 0.6;
const GLARE_FORWARD: f64 = 0.03;

/// The fog the Lighting asks for.
pub(crate) fn fog_of(world: &World, lighting: &Instance) -> Fog {
    if let Some(atmosphere) = crate::time_of_day::atmosphere_instance(world, lighting) {
        return atmosphere_fog(atmosphere);
    }
    let number_of = |name, default: f64| match lighting.property(name) {
        Some(Value::Number(value)) if value.is_finite() => *value,
        _ => default,
    };
    let end = number_of("FogEnd", NO_FOG_END);
    if end >= NO_FOG_END {
        return Fog::default();
    }
    let color = lighting
        .property("FogColor")
        .and_then(Value::as_color3)
        .map(|color| [color.r, color.g, color.b]);
    let start = number_of("FogStart", 0.0).max(0.0);
    Fog {
        color: color.or(Fog::default().color),
        start: start as f32,
        end: end.max(start) as f32,
        ..Fog::default()
    }
}

/// An Atmosphere's height fog, in the sky's color.
pub(crate) fn atmosphere_fog(atmosphere: &Instance) -> Fog {
    let number_of = |name, default: f64, most: f64| match atmosphere.property(name) {
        Some(Value::Number(value)) if value.is_finite() => value.clamp(0.0, most),
        _ => default,
    };
    let density = number_of("Density", EARTH_DENSITY, 1.0) / EARTH_DENSITY;
    let haze = number_of("Haze", 0.0, 10.0);
    let glare = number_of("Glare", 0.0, 10.0);
    Fog {
        color: None,
        density: (EARTH_FOG_DENSITY * density * (1.0 + haze)) as f32,
        height: (-number_of("Offset", 0.0, 1.0) * OFFSET_DEPTH) as f32,
        falloff: FOG_FALLOFF,
        sun_scattering: SUN_SCATTERING,
        forward_scattering: (FORWARD_SCATTERING + GLARE_FORWARD * glare) as f32,
        ..Fog::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Color3};

    fn fog_in(world: &World) -> Fog {
        fog_of(world, world.get(world.lighting()).unwrap())
    }

    fn set(world: &mut World, id: axcenity_core::InstanceId, name: &str, value: Value) {
        world.set_property(id, name, value).unwrap();
    }

    #[test]
    fn a_new_world_has_no_fog() {
        let fog = fog_in(&World::new());
        assert!(!fog.is_drawn(), "{fog:?}");
    }

    #[test]
    fn lightings_fog_runs_from_its_start_to_its_end_in_its_color() {
        let mut world = World::new();
        let lighting = world.lighting();
        set(&mut world, lighting, "FogStart", Value::Number(20.0));
        set(&mut world, lighting, "FogEnd", Value::Number(80.0));
        set(
            &mut world,
            lighting,
            "FogColor",
            Value::Color3(Color3::new(1.0, 0.0, 0.5)),
        );
        let fog = fog_in(&world);
        assert!(fog.has_distance_fog() && !fog.has_height_fog());
        assert_eq!((fog.start, fog.end), (20.0, 80.0));
        assert_eq!(fog.color, Some([1.0, 0.0, 0.5]));
        // An end before the start is whole from the start.
        set(&mut world, lighting, "FogEnd", Value::Number(5.0));
        assert_eq!(fog_in(&world).end, 20.0);
    }

    #[test]
    fn an_atmosphere_gives_height_fog_in_place_of_lightings() {
        let mut world = World::new();
        let lighting = world.lighting();
        set(&mut world, lighting, "FogEnd", Value::Number(80.0));
        let atmosphere = world.create(ClassName::Atmosphere).unwrap();
        world.set_parent(atmosphere, Some(lighting)).unwrap();
        let earth = fog_in(&world);
        assert!(
            earth.has_height_fog() && !earth.has_distance_fog(),
            "{earth:?}"
        );
        assert_eq!(earth.color, None, "the sky's color");
        assert!((earth.density as f64 - EARTH_FOG_DENSITY).abs() < 1e-9);
        assert_eq!((earth.height, earth.falloff), (0.0, FOG_FALLOFF));
        assert!(earth.sun_scattering > 0.0);

        set(&mut world, atmosphere, "Haze", Value::Number(3.0));
        set(&mut world, atmosphere, "Glare", Value::Number(5.0));
        set(&mut world, atmosphere, "Offset", Value::Number(0.5));
        let hazy = fog_in(&world);
        assert!(
            (hazy.density / earth.density - 4.0).abs() < 1e-5,
            "Haze thickens it"
        );
        assert!(
            hazy.forward_scattering > earth.forward_scattering,
            "Glare tightens the glow"
        );
        assert_eq!(hazy.height, -125.0, "Offset lifts the world out of it");

        set(&mut world, atmosphere, "Density", Value::Number(0.0));
        assert!(!fog_in(&world).is_drawn(), "no air, no fog");
    }
}
