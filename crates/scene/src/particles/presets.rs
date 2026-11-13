//! Ready-made effects (roadmap R17.5): a ParticleEmitter, sometimes with
//! sub-emitters inside, set up as fire, smoke, sparks and so on, and
//! suited to the place's Lighting and wind. The editor's Effect tab adds
//! them (Add preset), and `bench_effects` times them.
//!
//! Lighting-aware: by day (the sun above the horizon at the Lighting's
//! `ClockTime` and `GeographicLatitude`) glowing particles are brighter, so
//! they read against a bright sky, and by night dimmer, so they don't
//! blow out; smoke takes a quarter of the sky's horizon color, so it sits
//! in the place's haze; and while the Workspace's `GlobalWind` blows,
//! smoke and fire drift with it (`WindAffectsDrag`).

use axcenity_core::{
    ClassName, Color3, ColorKeypoint, ColorSequence, EnumItem, InstanceId, NumberKeypoint,
    NumberSequence, ParticleType, Value, Vector3, World,
};

use crate::time_of_day::sun_direction;

/// A ready-made effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectPreset {
    Fire,
    Smoke,
    Sparks,
    Magic,
    Explosion,
    Dripping,
}

impl EffectPreset {
    pub const ALL: [EffectPreset; 6] = [
        EffectPreset::Fire,
        EffectPreset::Smoke,
        EffectPreset::Sparks,
        EffectPreset::Magic,
        EffectPreset::Explosion,
        EffectPreset::Dripping,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EffectPreset::Fire => "Fire",
            EffectPreset::Smoke => "Smoke",
            EffectPreset::Sparks => "Sparks",
            EffectPreset::Magic => "Magic",
            EffectPreset::Explosion => "Explosion",
            EffectPreset::Dripping => "Dripping",
        }
    }

    /// One line on what it is.
    pub fn description(self) -> &'static str {
        match self {
            EffectPreset::Fire => "Rising flames that glow and fade",
            EffectPreset::Smoke => "Slow, growing puffs that drift with the wind",
            EffectPreset::Sparks => "Bursts of glowing sparks that bounce on the ground",
            EffectPreset::Magic => "Twinkles drawn in toward the middle",
            EffectPreset::Explosion => "A burst of fire whose particles leave smoke where they die",
            EffectPreset::Dripping => "Drops that fall and splash where they land",
        }
    }
}

/// One emitter of a preset: its name, its properties and its sub-emitters.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetEmitter {
    pub name: &'static str,
    pub properties: Vec<(&'static str, Value)>,
    pub subs: Vec<PresetEmitter>,
}

/// What a preset adapts to in `world`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    day: bool,
    horizon: Color3,
    windy: bool,
}

impl Place {
    fn of(world: &World) -> Self {
        let lighting = world.get(world.lighting());
        let number = |name, default| {
            lighting
                .and_then(|l| l.property(name))
                .and_then(Value::as_number)
                .filter(|n| n.is_finite())
                .unwrap_or(default)
        };
        let sun = sun_direction(
            number("ClockTime", axcenity_core::DEFAULT_CLOCK_TIME),
            number("GeographicLatitude", axcenity_core::DEFAULT_LATITUDE),
        );
        let horizon = match lighting.and_then(|l| l.property("SkyHorizonColor")) {
            Some(Value::Color3(color)) => *color,
            _ => Color3::new(0.63, 0.8, 0.95),
        };
        let windy = world
            .get(world.workspace())
            .and_then(|w| w.property("GlobalWind"))
            .and_then(Value::as_vector3)
            .is_some_and(|wind| wind.x != 0.0 || wind.y != 0.0 || wind.z != 0.0);
        Self {
            day: sun.y > 0.0,
            horizon,
            windy,
        }
    }

    /// Brightness for glowing particles: more by day than by night.
    fn glow(&self) -> Value {
        Value::Number(if self.day { 2.5 } else { 1.2 })
    }

    /// `grey` smoke, a quarter of the way to the sky's horizon color.
    fn smoke(&self, grey: f32) -> Value {
        let mix = |horizon: f32| grey + (horizon - grey) * 0.25;
        Value::Color3(Color3::new(
            mix(self.horizon.r),
            mix(self.horizon.g),
            mix(self.horizon.b),
        ))
    }
}

fn number(value: f64) -> Value {
    Value::Number(value)
}

fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn color(r: f32, g: f32, b: f32) -> Value {
    Value::Color3(Color3::new(r, g, b))
}

fn item(enum_name: &str, item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: enum_name.to_owned(),
        item: item.to_owned(),
    })
}

/// One burst of `count` at the start of each loop.
fn burst(count: f32) -> Value {
    Value::NumberSequence(
        NumberSequence::new(vec![
            NumberKeypoint::new(0.0, count, 0.0),
            NumberKeypoint::new(1.0, 0.0, 0.0),
        ])
        .expect("two keypoints from 0 to 1"),
    )
}

/// Gravity, as the Workspace's default pulls parts.
const FALL: f32 = -60.0;

/// `preset`'s emitter, suited to `world`'s Lighting and wind (see the
/// module docs).
pub fn preset_emitter(world: &World, preset: EffectPreset) -> PresetEmitter {
    let place = Place::of(world);
    let drift = |mut properties: Vec<(&'static str, Value)>| {
        if place.windy {
            properties.push(("WindAffectsDrag", Value::Bool(true)));
            properties.push(("Drag", number(1.0)));
        }
        properties
    };
    match preset {
        EffectPreset::Fire => PresetEmitter {
            name: "Fire",
            properties: drift(vec![
                ("ParticleType", Value::ParticleType(ParticleType::Fire)),
                ("Rate", number(45.0)),
                ("Lifetime", number(0.9)),
                ("Speed", number(3.0)),
                ("SpreadAngle", number(20.0)),
                ("Acceleration", vector(0.0, 3.0, 0.0)),
                ("Size", number(1.6)),
                ("SizeEnd", number(0.4)),
                ("Color", color(1.0, 0.65, 0.2)),
                ("ColorEnd", color(1.0, 0.2, 0.05)),
                ("Transparency", number(0.15)),
                ("TransparencyEnd", number(1.0)),
                ("RotSpeed", number(40.0)),
                ("LightEmission", number(1.0)),
                ("Brightness", place.glow()),
            ]),
            subs: Vec::new(),
        },
        EffectPreset::Smoke => PresetEmitter {
            name: "Smoke",
            properties: drift(vec![
                ("ParticleType", Value::ParticleType(ParticleType::Smoke)),
                ("Rate", number(10.0)),
                ("Lifetime", number(4.0)),
                ("Speed", number(2.0)),
                ("SpreadAngle", number(25.0)),
                ("Size", number(1.0)),
                ("SizeEnd", number(4.5)),
                ("Color", place.smoke(0.35)),
                ("ColorEnd", place.smoke(0.55)),
                ("Transparency", number(0.35)),
                ("TransparencyEnd", number(1.0)),
                ("RotSpeed", number(15.0)),
            ]),
            subs: Vec::new(),
        },
        EffectPreset::Sparks => PresetEmitter {
            name: "Sparks",
            properties: vec![
                ("ParticleType", Value::ParticleType(ParticleType::Spark)),
                ("Rate", number(0.0)),
                ("Bursts", burst(40.0)),
                ("Duration", number(1.5)),
                ("Lifetime", number(1.2)),
                ("Speed", number(16.0)),
                ("SpreadAngle", number(70.0)),
                ("Acceleration", vector(0.0, FALL, 0.0)),
                ("Size", number(0.25)),
                ("SizeEnd", number(0.1)),
                ("Color", color(1.0, 0.85, 0.4)),
                ("ColorEnd", color(1.0, 0.4, 0.1)),
                ("Transparency", number(0.0)),
                ("TransparencyEnd", number(0.6)),
                ("LightEmission", number(1.0)),
                ("Brightness", place.glow()),
                ("Collides", Value::Bool(true)),
                ("Bounce", number(0.4)),
            ],
            subs: Vec::new(),
        },
        EffectPreset::Magic => PresetEmitter {
            name: "Magic",
            properties: vec![
                ("ParticleType", Value::ParticleType(ParticleType::Magic)),
                ("Rate", number(60.0)),
                ("Lifetime", number(1.0)),
                ("Speed", number(2.5)),
                ("SpreadAngle", number(10.0)),
                ("Shape", item("ParticleEmitterShape", "Sphere")),
                ("ShapeStyle", item("ParticleEmitterShapeStyle", "Surface")),
                ("ShapeInOut", item("ParticleEmitterShapeInOut", "Inward")),
                ("Size", number(0.5)),
                ("SizeEnd", number(0.0)),
                (
                    "ColorOverLife",
                    Value::ColorSequence(
                        ColorSequence::new(vec![
                            ColorKeypoint::new(0.0, Color3::new(0.7, 0.3, 1.0)),
                            ColorKeypoint::new(0.6, Color3::new(0.3, 0.8, 1.0)),
                            ColorKeypoint::new(1.0, Color3::new(1.0, 1.0, 1.0)),
                        ])
                        .expect("three keypoints from 0 to 1"),
                    ),
                ),
                ("Transparency", number(0.0)),
                ("TransparencyEnd", number(1.0)),
                ("RotSpeed", number(90.0)),
                ("LightEmission", number(1.0)),
                ("Brightness", place.glow()),
            ],
            subs: Vec::new(),
        },
        EffectPreset::Explosion => PresetEmitter {
            name: "Explosion",
            properties: vec![
                ("ParticleType", Value::ParticleType(ParticleType::Fire)),
                ("Rate", number(0.0)),
                ("Bursts", burst(50.0)),
                ("Duration", number(3.0)),
                ("Lifetime", number(0.7)),
                ("Speed", number(24.0)),
                ("SpreadAngle", number(360.0)),
                ("Drag", number(4.0)),
                ("Size", number(2.0)),
                ("SizeEnd", number(3.5)),
                ("Color", color(1.0, 0.8, 0.35)),
                ("ColorEnd", color(1.0, 0.25, 0.05)),
                ("Transparency", number(0.0)),
                ("TransparencyEnd", number(0.8)),
                ("LightEmission", number(1.0)),
                ("Brightness", place.glow()),
            ],
            subs: vec![PresetEmitter {
                name: "Smoke",
                properties: drift(vec![
                    ("EmitOn", item("ParticleEmitterEmitOn", "Death")),
                    ("EmitCount", number(1.0)),
                    ("ParticleType", Value::ParticleType(ParticleType::Smoke)),
                    ("Lifetime", number(2.5)),
                    ("Speed", number(1.5)),
                    ("SpreadAngle", number(180.0)),
                    ("VelocityInheritance", number(0.3)),
                    ("Size", number(2.5)),
                    ("SizeEnd", number(5.0)),
                    ("Color", place.smoke(0.25)),
                    ("ColorEnd", place.smoke(0.45)),
                    ("Transparency", number(0.4)),
                    ("TransparencyEnd", number(1.0)),
                    ("RotSpeed", number(20.0)),
                ]),
                subs: Vec::new(),
            }],
        },
        EffectPreset::Dripping => PresetEmitter {
            name: "Drips",
            properties: vec![
                ("ParticleType", Value::ParticleType(ParticleType::Circle)),
                ("Rate", number(3.0)),
                ("Lifetime", number(4.0)),
                ("Speed", number(0.0)),
                ("Acceleration", vector(0.0, FALL, 0.0)),
                ("Size", number(0.2)),
                ("SizeEnd", number(0.2)),
                ("Color", color(0.6, 0.75, 0.9)),
                ("ColorEnd", color(0.6, 0.75, 0.9)),
                ("Transparency", number(0.2)),
                ("TransparencyEnd", number(0.2)),
                ("Collides", Value::Bool(true)),
                ("LifetimeLoss", number(1.0)),
            ],
            subs: vec![PresetEmitter {
                name: "Splash",
                properties: vec![
                    ("EmitOn", item("ParticleEmitterEmitOn", "Collision")),
                    ("EmitCount", number(6.0)),
                    ("ParticleType", Value::ParticleType(ParticleType::Circle)),
                    ("Lifetime", number(0.4)),
                    ("Speed", number(5.0)),
                    ("SpreadAngle", number(100.0)),
                    ("Acceleration", vector(0.0, FALL, 0.0)),
                    ("Size", number(0.12)),
                    ("SizeEnd", number(0.05)),
                    ("Color", color(0.7, 0.85, 1.0)),
                    ("ColorEnd", color(0.7, 0.85, 1.0)),
                    ("Transparency", number(0.2)),
                    ("TransparencyEnd", number(1.0)),
                ],
                subs: Vec::new(),
            }],
        },
    }
}

/// Makes `emitter` (and its sub-emitters inside it) in `world`, outside
/// the tree: the caller places it. With `position`, it stands there when
/// it isn't put in a part.
pub fn create_preset(
    world: &mut World,
    emitter: &PresetEmitter,
    position: Option<Vector3>,
) -> InstanceId {
    let id = world
        .create(ClassName::ParticleEmitter)
        .expect("not a service");
    world.set_name(id, emitter.name).expect("just made");
    for (name, value) in &emitter.properties {
        world
            .set_property(id, name, value.clone())
            .expect("presets set ParticleEmitter properties");
    }
    if let Some(position) = position {
        world
            .set_property(id, "Position", Value::Vector3(position))
            .expect("emitters stand free");
    }
    for sub in &emitter.subs {
        let sub = create_preset(world, sub, None);
        world
            .set_parent(sub, Some(id))
            .expect("emitters hold emitters");
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Particles;

    fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
        world.set_property(id, name, value).unwrap();
    }

    #[test]
    fn every_preset_makes_emitters_that_give_off_particles() {
        for preset in EffectPreset::ALL {
            let mut world = World::new();
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(world.workspace())).unwrap();
            set(
                &mut world,
                part,
                "Position",
                Value::Vector3(Vector3::new(0.0, 5.0, 0.0)),
            );
            let floor = world.create(ClassName::Part).unwrap();
            world.set_parent(floor, Some(world.workspace())).unwrap();
            let spec = preset_emitter(&world, preset);
            let emitter = create_preset(&mut world, &spec, None);
            world.set_parent(emitter, Some(part)).unwrap();
            assert_eq!(world.get(emitter).unwrap().name(), spec.name);
            assert_eq!(
                world.get(emitter).unwrap().children().len(),
                spec.subs.len()
            );
            let mut particles = Particles::new();
            let mut most = 0;
            for _ in 0..120 {
                particles.update(&world, 1.0 / 60.0);
                most = most.max(particles.count());
            }
            assert!(most > 0, "{} gives off particles", preset.name());
        }
    }

    #[test]
    fn presets_suit_the_places_lighting_and_wind() {
        let mut world = World::new();
        let glow = |world: &World| {
            preset_emitter(world, EffectPreset::Fire)
                .properties
                .iter()
                .find(|(name, _)| *name == "Brightness")
                .map(|(_, value)| value.clone())
        };
        let lighting = world.lighting();
        set(&mut world, lighting, "ClockTime", Value::Number(14.0));
        let day = glow(&world);
        set(&mut world, lighting, "ClockTime", Value::Number(1.0));
        let night = glow(&world);
        assert_eq!(day, Some(Value::Number(2.5)));
        assert_eq!(night, Some(Value::Number(1.2)));

        // Smoke leans toward the horizon's color.
        set(
            &mut world,
            lighting,
            "SkyHorizonColor",
            Value::Color3(Color3::new(1.0, 0.0, 0.0)),
        );
        let smoke = preset_emitter(&world, EffectPreset::Smoke);
        let color = smoke.properties.iter().find(|(name, _)| *name == "Color");
        let Some((_, Value::Color3(color))) = color else {
            panic!("smoke has a color");
        };
        assert!(color.r > color.g, "{color:?}");

        // Calm air: no wind settings. Windy: smoke drifts with it.
        let drifts = |world: &World| {
            preset_emitter(world, EffectPreset::Smoke)
                .properties
                .iter()
                .any(|(name, value)| *name == "WindAffectsDrag" && *value == Value::Bool(true))
        };
        assert!(!drifts(&world));
        let workspace = world.workspace();
        set(
            &mut world,
            workspace,
            "GlobalWind",
            Value::Vector3(Vector3::new(8.0, 0.0, 0.0)),
        );
        assert!(drifts(&world));
    }
}
