//! The world's weather (graphics Phase 27): Lighting's first `Weather`.
//!
//! One state holds it all: what falls (rain or snow, and how hard), how wet
//! surfaces are and how much snow lies on them. [`settled`] is that state
//! as the world says it should be, which a draw list built from the world
//! alone shows. A host's [`WeatherState`] (inside its `Particles`) eases
//! toward it over the Weather's `TransitionTime`, so rain wets surfaces
//! over time and they dry again after, and lets the rain or snow fall in a
//! box that follows the camera, blown by the Workspace's `GlobalWind`.

use std::sync::Arc;

use axcenity_core::{ClassName, Instance, ParticleType, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Sprite, TextureSource, Weather};

use crate::particles::{Random, built_in, global_wind, rain_streak};

/// The most raindrops and snowflakes falling at once, at Intensity 1.
const MOST_DROPS: usize = 2500;
const MOST_FLAKES: usize = 2000;
/// The box around the camera they fall in, in studs. A drop leaving one
/// side comes back in the other, so the camera never runs out of them.
const BOX: Vec3 = Vec3::new(64.0, 48.0, 64.0);
/// How fast rain and snow fall in still air, in studs a second.
const RAIN_SPEED: f32 = 60.0;
const SNOW_SPEED: f32 = 5.0;
/// How far snowflakes sway from side to side as they fall, in studs a
/// second.
const SNOW_SWAY: f32 = 1.5;

/// What the world's Weather asks for, each from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Target {
    /// How hard rain and snow fall (only one of them is above 0).
    rain: f32,
    snow: f32,
    /// How wet surfaces are, and how much snow covers them: at least as
    /// much as what's falling.
    wetness: f32,
    cover: f32,
    /// Seconds a change takes to ease in.
    transition: f32,
}

impl Target {
    fn of(world: &World) -> Self {
        world
            .get(world.lighting())
            .and_then(|lighting| weather_in(world, lighting))
            .map_or_else(Self::default, Self::read)
    }

    fn read(weather: &Instance) -> Self {
        let share = |name| match weather.property(name) {
            Some(Value::Number(value)) if value.is_finite() => value.clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        let intensity = share("Intensity");
        let falling = match weather.property("Precipitation") {
            Some(Value::EnumItem(item)) => item.item.as_str(),
            _ => "None",
        };
        let rain = if falling == "Rain" { intensity } else { 0.0 };
        let snow = if falling == "Snow" { intensity } else { 0.0 };
        let transition = match weather.property("TransitionTime") {
            Some(Value::Number(value)) if value.is_finite() => value.max(0.0) as f32,
            _ => 0.0,
        };
        Self {
            rain,
            snow,
            wetness: share("Wetness").max(rain),
            cover: share("SnowCover").max(snow),
            transition,
        }
    }

    fn look(&self) -> Weather {
        Weather {
            wetness: self.wetness,
            snow: self.cover,
        }
    }
}

/// Lighting's first Weather, if it has one.
fn weather_in<'a>(world: &'a World, lighting: &Instance) -> Option<&'a Instance> {
    lighting
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .find(|child| child.class() == ClassName::Weather)
}

/// How wet and snowy the world's surfaces are once its Weather has eased
/// in. Without a Weather, dry and bare.
pub(crate) fn settled(world: &World) -> Weather {
    Target::of(world).look()
}

/// The weather now, partway to what the world asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Eased {
    rain: f32,
    snow: f32,
    wetness: f32,
    cover: f32,
}

impl Eased {
    fn settled(target: &Target) -> Self {
        Self {
            rain: target.rain,
            snow: target.snow,
            wetness: target.wetness,
            cover: target.cover,
        }
    }
}

/// One raindrop or snowflake.
#[derive(Clone, Copy, Debug)]
struct Drop {
    /// Where it is inside [`BOX`], repeated across the world.
    position: Vec3,
    /// How much faster or slower than the rest it falls.
    speed: f32,
    /// Where in its sway a snowflake is.
    phase: f32,
}

/// A host's weather: eased toward the world's, with its rain and snow.
/// See the module docs.
#[derive(Debug, Default)]
pub(crate) struct WeatherState {
    /// `None` until the first update, which takes the world's weather as it
    /// is, so a world that starts in snow is already white.
    now: Option<Eased>,
    drops: Vec<Drop>,
    flakes: Vec<Drop>,
    /// Seconds snowflakes have swayed for.
    clock: f32,
}

impl WeatherState {
    /// Starts again from the world's weather as it is, such as when Play
    /// starts or stops.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Eases on by `seconds` toward the world's weather, and moves the rain
    /// and snow.
    pub(crate) fn update(&mut self, world: &World, seconds: f32, random: &mut Random) {
        let target = Target::of(world);
        let now = match self.now {
            None => Eased::settled(&target),
            Some(now) => {
                // The share of a whole change made in `seconds`.
                let step = if target.transition > 0.0 {
                    seconds / target.transition
                } else {
                    1.0
                };
                let toward = |from: f32, to: f32| from + (to - from).clamp(-step, step);
                Eased {
                    rain: toward(now.rain, target.rain),
                    snow: toward(now.snow, target.snow),
                    wetness: toward(now.wetness, target.wetness),
                    cover: toward(now.cover, target.cover),
                }
            }
        };
        self.now = Some(now);

        let wind = global_wind(world);
        if now.rain > 0.0 {
            fill(&mut self.drops, MOST_DROPS, random);
            for drop in &mut self.drops {
                let velocity = wind - Vec3::Y * RAIN_SPEED * drop.speed;
                drop.position = (drop.position + velocity * seconds).rem_euclid(BOX);
            }
        }
        if now.snow > 0.0 {
            fill(&mut self.flakes, MOST_FLAKES, random);
            self.clock += seconds;
            for flake in &mut self.flakes {
                let sway = self.clock * 0.8 + flake.phase;
                let velocity = wind - Vec3::Y * SNOW_SPEED * flake.speed
                    + Vec3::new(sway.sin(), 0.0, sway.cos()) * SNOW_SWAY;
                flake.position = (flake.position + velocity * seconds).rem_euclid(BOX);
            }
        }
    }

    /// How wet and snowy surfaces are now. `None` before the first update.
    pub(crate) fn look(&self) -> Option<Weather> {
        self.now.map(|now| Weather {
            wetness: now.wetness,
            snow: now.cover,
        })
    }

    /// True while rain or snow falls, or the weather is still easing, so
    /// a host keeps drawing frames.
    pub(crate) fn is_active(&self, world: &World) -> bool {
        match self.now {
            None => false,
            Some(now) => {
                now.rain > 0.0 || now.snow > 0.0 || now != Eased::settled(&Target::of(world))
            }
        }
    }

    /// The raindrops and snowflakes to draw around `camera`: as many of
    /// them as how hard each falls now, fading out toward the box's sides.
    pub(crate) fn sprites(&self, camera: Vec3) -> Vec<Sprite> {
        let Some(now) = self.now else {
            return Vec::new();
        };
        let mut sprites = Vec::new();
        let rain = Look {
            size: 1.4,
            color: [0.75, 0.8, 0.9],
            opacity: 0.45,
            texture: rain_streak(),
        };
        let snow = Look {
            size: 0.3,
            color: [1.0; 3],
            opacity: 0.9,
            texture: built_in(ParticleType::Circle),
        };
        for (drops, amount, look) in [
            (&self.drops, now.rain, rain),
            (&self.flakes, now.snow, snow),
        ] {
            let shown = (amount * drops.len() as f32).round() as usize;
            for drop in &drops[..shown.min(drops.len())] {
                let position = around(drop.position, camera);
                let out = ((position - camera) / (BOX * 0.5)).length().min(1.0);
                sprites.push(Sprite {
                    position,
                    size: look.size,
                    rotation_degrees: 0.0,
                    color: look.color,
                    opacity: look.opacity * (1.0 - out * out),
                    brightness: 1.0,
                    light_emission: 0.0,
                    texture: Some(look.texture.clone()),
                    frame: None,
                });
            }
        }
        sprites
    }
}

/// How raindrops or snowflakes look.
struct Look {
    size: f32,
    color: [f32; 3],
    opacity: f32,
    texture: Arc<dyn TextureSource>,
}

/// Gives `drops` its `most` drops, spread through the box, the first time
/// they're needed.
fn fill(drops: &mut Vec<Drop>, most: usize, random: &mut Random) {
    while drops.len() < most {
        drops.push(Drop {
            position: Vec3::new(random.next(), random.next(), random.next()) * BOX,
            speed: 0.8 + 0.4 * random.next(),
            phase: random.next() * std::f32::consts::TAU,
        });
    }
}

/// The copy of a drop at `position` (inside [`BOX`]) that's in the box
/// centered on `camera`.
fn around(position: Vec3, camera: Vec3) -> Vec3 {
    let corner = camera - BOX * 0.5;
    corner + (position - corner).rem_euclid(BOX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{EnumItem, Vector3};

    fn falling(item: &str) -> Value {
        Value::EnumItem(EnumItem {
            enum_name: "Precipitation".to_owned(),
            item: item.to_owned(),
        })
    }

    fn world_with_weather() -> (World, axcenity_core::InstanceId) {
        let mut world = World::new();
        let weather = world.create(ClassName::Weather).unwrap();
        world.set_parent(weather, Some(world.lighting())).unwrap();
        (world, weather)
    }

    fn set(world: &mut World, id: axcenity_core::InstanceId, name: &str, value: Value) {
        world.set_property(id, name, value).unwrap();
    }

    #[test]
    fn a_new_world_is_dry_and_a_weathers_rain_wets_it() {
        assert!(settled(&World::new()).is_clear());
        let (mut world, weather) = world_with_weather();
        // Rain at 0.5 by default.
        assert_eq!(
            settled(&world),
            Weather {
                wetness: 0.5,
                snow: 0.0
            }
        );
        set(&mut world, weather, "Wetness", Value::Number(0.8));
        assert_eq!(settled(&world).wetness, 0.8, "wetter than the rain");
        set(&mut world, weather, "Precipitation", falling("Snow"));
        set(&mut world, weather, "Intensity", Value::Number(3.0));
        assert_eq!(
            settled(&world),
            Weather {
                wetness: 0.8,
                snow: 1.0
            },
            "snow covers, held to 1"
        );
        set(&mut world, weather, "Precipitation", falling("None"));
        set(&mut world, weather, "Wetness", Value::Number(0.0));
        set(&mut world, weather, "SnowCover", Value::Number(0.25));
        assert_eq!(settled(&world).snow, 0.25, "old snow lies");
        world.destroy(weather).unwrap();
        assert!(settled(&world).is_clear());
    }

    #[test]
    fn rain_wets_surfaces_over_the_transition_and_they_dry_after() {
        let (mut world, weather) = world_with_weather();
        set(&mut world, weather, "Precipitation", falling("None"));
        set(&mut world, weather, "Intensity", Value::Number(1.0));
        set(&mut world, weather, "TransitionTime", Value::Number(4.0));
        let mut random = Random::default();
        let mut state = WeatherState::default();
        state.update(&world, 0.1, &mut random);
        assert!(state.look().unwrap().is_clear(), "starts as it is: dry");
        assert!(!state.is_active(&world), "clear and settled: nothing to do");

        set(&mut world, weather, "Precipitation", falling("Rain"));
        let mut wetness = Vec::new();
        for _ in 0..5 {
            state.update(&world, 1.0, &mut random);
            wetness.push(state.look().unwrap().wetness);
        }
        assert_eq!(wetness, [0.25, 0.5, 0.75, 1.0, 1.0]);
        assert!(state.is_active(&world), "rain falls");

        set(&mut world, weather, "Precipitation", falling("None"));
        state.update(&world, 2.0, &mut random);
        assert_eq!(state.look().unwrap().wetness, 0.5, "drying");
        state.update(&world, 2.0, &mut random);
        assert!(state.look().unwrap().is_clear(), "dry again");
        assert!(!state.is_active(&world));
        assert!(state.sprites(Vec3::ZERO).is_empty(), "no rain left");

        // TransitionTime 0 changes at once.
        set(&mut world, weather, "TransitionTime", Value::Number(0.0));
        set(&mut world, weather, "Precipitation", falling("Snow"));
        state.update(&world, 0.01, &mut random);
        assert_eq!(state.look().unwrap().snow, 1.0);
    }

    #[test]
    fn rain_and_snow_fall_around_the_camera_blown_by_the_wind() {
        let (mut world, weather) = world_with_weather();
        set(&mut world, weather, "Intensity", Value::Number(1.0));
        let mut random = Random::default();
        let mut state = WeatherState::default();
        state.update(&world, 0.0, &mut random);
        let camera = Vec3::new(1000.0, 50.0, -3000.0);
        let drops = state.sprites(camera);
        assert_eq!(drops.len(), MOST_DROPS);
        assert!(
            drops
                .iter()
                .all(|drop| ((drop.position - camera).abs() - BOX * 0.5).max_element() <= 1e-3),
            "every drop in the box around the camera"
        );

        // They fall, and the wind blows them along.
        let workspace = world.workspace();
        set(
            &mut world,
            workspace,
            "GlobalWind",
            Value::Vector3(Vector3::new(10.0, 0.0, 0.0)),
        );
        let before = state.drops.clone();
        state.update(&world, 0.1, &mut random);
        let moved = state.drops[0].position - before[0].position;
        let fall = RAIN_SPEED * before[0].speed * 0.1;
        assert!((moved.x - 1.0).abs() < 1e-3, "{moved}");
        assert!(
            (moved.y + fall).abs() < 1e-3 || (moved.y + fall - BOX.y).abs() < 1e-3,
            "{moved}"
        );

        // Half as hard, half as many; snow in place of rain once it's eased.
        set(&mut world, weather, "Intensity", Value::Number(0.5));
        set(&mut world, weather, "Precipitation", falling("Snow"));
        set(&mut world, weather, "TransitionTime", Value::Number(0.0));
        state.update(&world, 0.1, &mut random);
        assert_eq!(state.sprites(camera).len(), MOST_FLAKES / 2);
    }
}
