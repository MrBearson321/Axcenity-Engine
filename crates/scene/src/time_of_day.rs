//! The sun and moon by the time of day (`Lighting.ClockTime` and
//! `GeographicLatitude`), the light they give, the night sky, and the
//! Lighting's `Atmosphere`.
//!
//! The sun follows an equinox day: it rises due east (+X) at 6, is highest
//! at 12, toward the south (+Z) north of the equator, and sets due west at
//! 18. The moon is full, opposite the sun. Whichever is above the horizon
//! is the draw list's `Sun`, the one light that casts shadows from far
//! away; both are at the horizon when it changes, where neither gives light.

use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Atmosphere, Sky, Sun};

use crate::draw_list::srgb;

/// The sun's color and brightness in the open, and the moon's.
const SUN_COLOR: [f32; 3] = [1.0, 0.97, 0.92];
const SUN_INTENSITY: f32 = 3.0;
const MOON_COLOR: [f32; 3] = [0.75, 0.85, 1.0];
const MOON_INTENSITY: f32 = 0.15;
/// How much of the day's light the sky's colors give at night, without an
/// Atmosphere.
const NIGHT_SKY: f32 = 0.06;
/// An Atmosphere's defaults, which leave the air as Earth's.
const EARTH_DENSITY: f64 = 0.395;
const DEFAULT_COLOR: [f32; 3] = [199.0 / 255.0; 3];
const DEFAULT_DECAY: [f32; 3] = [106.0 / 255.0, 112.0 / 255.0, 125.0 / 255.0];

/// The direction toward the sun at `clock_time` hours (wrapped into a day)
/// and `latitude` degrees north of the equator.
pub fn sun_direction(clock_time: f64, latitude: f64) -> Vec3 {
    let hour_angle = ((clock_time.rem_euclid(24.0) - 12.0) * 15.0).to_radians();
    let latitude = latitude.clamp(-90.0, 90.0).to_radians();
    Vec3::new(
        -hour_angle.sin() as f32,
        (latitude.cos() * hour_angle.cos()) as f32,
        (latitude.sin() * hour_angle.cos()) as f32,
    )
}

/// The sun at the default time and place, for a world without a Lighting.
pub(crate) fn default_sun() -> Sun {
    Sun {
        direction: sun_direction(
            axcenity_core::DEFAULT_CLOCK_TIME,
            axcenity_core::DEFAULT_LATITUDE,
        ),
        color: SUN_COLOR,
        intensity: SUN_INTENSITY,
    }
}

/// The Lighting's sun or moon and what they do to `sky`: its atmosphere
/// (from the Lighting's Atmosphere, if it has one) and its night.
pub(crate) fn sun_and_sky(world: &World, lighting: &Instance, sky: &mut Sky) -> Sun {
    let number_of = |name, default| match lighting.property(name) {
        Some(Value::Number(value)) if value.is_finite() => *value,
        _ => default,
    };
    let sun = sun_direction(
        number_of("ClockTime", axcenity_core::DEFAULT_CLOCK_TIME),
        number_of("GeographicLatitude", axcenity_core::DEFAULT_LATITUDE),
    );
    let moon = -sun;
    let atmosphere = atmosphere_of(world, lighting).map(|atmosphere| Atmosphere {
        sun_direction: sun,
        moon_direction: moon,
        sun_intensity: SUN_INTENSITY,
        moon_brightness: MOON_INTENSITY / SUN_INTENSITY,
        ..atmosphere
    });
    sky.atmosphere = atmosphere;
    // The sky's colors dim from a little before sunset into the night,
    // when the stars come out.
    sky.daylight = NIGHT_SKY + (1.0 - NIGHT_SKY) * smoothstep(-0.2, 0.05, sun.y);
    sky.stars = 1.0 - smoothstep(-0.15, 0.0, sun.y);
    let (direction, color, intensity) = if sun.y >= 0.0 {
        (sun, SUN_COLOR, SUN_INTENSITY)
    } else {
        (moon, MOON_COLOR, MOON_INTENSITY)
    };
    // It fades in as it rises and out as it sets.
    let intensity = intensity * smoothstep(0.0, 0.05, direction.y);
    let Some(atmosphere) = &atmosphere else {
        return Sun {
            direction,
            color,
            intensity,
        };
    };
    // Through the air: redder and dimmer the lower it is. The color stays
    // within sRGB's range and the intensity takes the rest.
    let light = linear(color) * atmosphere.sunlight(direction);
    let strongest = light.max_element();
    if strongest <= 0.0 {
        return Sun {
            direction,
            color,
            intensity: 0.0,
        };
    }
    Sun {
        direction,
        color: srgb((light / strongest).to_array()),
        intensity: intensity * strongest,
    }
}

/// The air of the Lighting's first Atmosphere, with its sun and moon left
/// at their defaults.
fn atmosphere_of(world: &World, lighting: &Instance) -> Option<Atmosphere> {
    let atmosphere = lighting
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .find(|child| child.class() == ClassName::Atmosphere)?;
    let number_of = |name, default: f64, most: f64| match atmosphere.property(name) {
        Some(Value::Number(value)) if value.is_finite() => value.clamp(0.0, most),
        _ => default,
    };
    // A tint compared with the property's default, in linear light.
    let tint_of = |name, default: [f32; 3]| {
        let color = match atmosphere.property(name).and_then(Value::as_color3) {
            Some(color) => [color.r, color.g, color.b],
            None => default,
        };
        (linear(color) / linear(default)).to_array()
    };
    Some(Atmosphere {
        density: (number_of("Density", EARTH_DENSITY, 1.0) / EARTH_DENSITY) as f32,
        altitude: number_of("Offset", 0.0, 1.0) as f32,
        tint: tint_of("Color", DEFAULT_COLOR),
        decay: tint_of("Decay", DEFAULT_DECAY),
        glare: number_of("Glare", 0.0, 10.0) as f32,
        haze: number_of("Haze", 0.0, 10.0) as f32,
        ..Default::default()
    })
}

/// An sRGB color in linear light, as the renderer turns it.
fn linear(color: [f32; 3]) -> Vec3 {
    Vec3::from(color.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }))
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::Color3;

    fn lighting(world: &World) -> &Instance {
        world.get(world.lighting()).unwrap()
    }

    fn set_clock(world: &mut World, hours: f64) {
        let lighting = world.lighting();
        world
            .set_property(lighting, "ClockTime", Value::Number(hours))
            .unwrap();
    }

    fn add_atmosphere(world: &mut World) -> axcenity_core::InstanceId {
        let atmosphere = world.create(ClassName::Atmosphere).unwrap();
        let lighting = world.lighting();
        world.set_parent(atmosphere, Some(lighting)).unwrap();
        atmosphere
    }

    fn sun_of(world: &World) -> (Sun, Sky) {
        let mut sky = Sky::default();
        let sun = sun_and_sky(world, lighting(world), &mut sky);
        (sun, sky)
    }

    #[test]
    fn the_default_time_and_place_put_the_sun_where_it_always_was() {
        let world = World::new();
        let (sun, sky) = sun_of(&world);
        let before = Vec3::new(0.5, 1.0, 0.25).normalize();
        assert!(
            (sun.direction - before).length() < 1e-6,
            "{}",
            sun.direction
        );
        assert_eq!(sun.color, SUN_COLOR);
        assert_eq!(sun.intensity, SUN_INTENSITY);
        assert_eq!((sky.daylight, sky.stars), (1.0, 0.0));
        assert!(sky.atmosphere.is_none());
    }

    #[test]
    fn the_sun_rises_in_the_east_stands_south_at_noon_and_sets_in_the_west() {
        let rise = sun_direction(6.0, 40.0);
        assert!((rise - Vec3::X).length() < 1e-6, "{rise}");
        let noon = sun_direction(12.0, 40.0);
        assert!(noon.x.abs() < 1e-6 && noon.z > 0.0);
        assert!((noon.y.asin().to_degrees() - 50.0).abs() < 1e-4);
        let set = sun_direction(18.0, 40.0);
        assert!((set - Vec3::NEG_X).length() < 1e-6, "{set}");
        // South of the equator the noon sun is to the north.
        assert!(sun_direction(12.0, -30.0).z < 0.0);
        // Past midnight it goes round again.
        assert!((sun_direction(30.0, 40.0) - rise).length() < 1e-6);
    }

    #[test]
    fn at_night_the_moon_lights_the_world_and_the_stars_come_out() {
        let mut world = World::new();
        set_clock(&mut world, 0.0);
        let (moon, sky) = sun_of(&world);
        assert!(moon.direction.y > 0.5, "the moon is up: {}", moon.direction);
        assert_eq!(moon.color, MOON_COLOR);
        assert!(moon.intensity > 0.0 && moon.intensity < SUN_INTENSITY / 4.0);
        assert_eq!(sky.stars, 1.0);
        assert!((sky.daylight - NIGHT_SKY).abs() < 1e-6);
        // At sunset neither lights the world, so changing from one to the
        // other doesn't jump.
        set_clock(&mut world, 18.0);
        assert!(sun_of(&world).0.intensity < 1e-3);
    }

    #[test]
    fn through_an_atmosphere_the_sun_reddens_toward_dusk() {
        let mut world = World::new();
        add_atmosphere(&mut world);
        let lighting = world.lighting();
        world
            .set_property(lighting, "GeographicLatitude", Value::Number(40.0))
            .unwrap();
        set_clock(&mut world, 12.0);
        let (noon, sky) = sun_of(&world);
        let air = sky.atmosphere.expect("the Lighting's Atmosphere");
        assert_eq!(air.density, 1.0);
        assert_eq!((air.tint, air.decay), ([1.0; 3], [1.0; 3]));
        set_clock(&mut world, 17.6);
        let (dusk, _) = sun_of(&world);
        let redness = |sun: &Sun| sun.color[0] / sun.color[2].max(1e-3);
        assert!(
            redness(&dusk) > 2.0 * redness(&noon),
            "{dusk:?} vs {noon:?}"
        );
        assert!(dusk.intensity < noon.intensity);
        // Lower, so shadows are longer.
        assert!(dusk.direction.y < noon.direction.y);
    }

    #[test]
    fn the_atmospheres_properties_set_its_air() {
        let mut world = World::new();
        let atmosphere = add_atmosphere(&mut world);
        for (name, value) in [
            ("Density", Value::Number(0.79)),
            ("Offset", Value::Number(0.5)),
            ("Glare", Value::Number(3.0)),
            ("Haze", Value::Number(20.0)),
            ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
        ] {
            world.set_property(atmosphere, name, value).unwrap();
        }
        let air = sun_of(&world).1.atmosphere.unwrap();
        assert!((air.density - 2.0).abs() < 1e-6);
        assert_eq!((air.altitude, air.glare, air.haze), (0.5, 3.0, 10.0));
        assert!(air.tint[0] > 1.5, "white is brighter than the default grey");
    }
}
