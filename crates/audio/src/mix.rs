//! The mix as the world describes it (Phase 52), read fresh each frame
//! without touching Kira: the SoundGroups and their effects, the effects on
//! one Sound, the ReverbZone the listener is in, and the editor Mixer's
//! mute and solo.
//!
//! A SoundGroup whose parent is a SoundGroup is a sub-bus of it. Effects
//! are the sound effects inside a Sound or SoundGroup, higher `Priority`
//! first (equal ones in child order), with their settings kept in range.

use std::collections::HashSet;

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};

/// `Enum.ReverbType`'s items: the presets a ReverbZone can sound like.
pub const REVERB_TYPES: [&str; 7] = [
    "Room",
    "StoneRoom",
    "Hallway",
    "Cave",
    "ConcertHall",
    "Arena",
    "SewerPipe",
];

/// A reverb's settings, as a ReverbSoundEffect has them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reverb {
    /// Seconds the tail takes to die away, 0.1 to 20.
    pub decay_time: f64,
    /// 0 to 1: darker and sparser to bright and smooth.
    pub density: f64,
    /// Decibels of the sound as it is, and of the reverb, -80 to 20.
    pub dry_level: f64,
    pub wet_level: f64,
}

impl Reverb {
    /// The preset `name` (one of [`REVERB_TYPES`]), or Room's.
    pub fn preset(name: &str) -> Self {
        let (decay_time, density, wet_level) = match name {
            "StoneRoom" => (2.0, 0.6, -2.0),
            "Hallway" => (1.6, 0.8, -4.0),
            "Cave" => (3.5, 0.5, 0.0),
            "ConcertHall" => (3.0, 1.0, -3.0),
            "Arena" => (5.0, 0.8, -2.0),
            "SewerPipe" => (2.8, 0.3, 2.0),
            _ => (0.6, 1.0, -8.0),
        };
        Self {
            decay_time,
            density,
            dry_level: 0.0,
            wet_level,
        }
    }

    fn clamped(self) -> Self {
        Self {
            decay_time: self.decay_time.clamp(0.1, 20.0),
            density: self.density.clamp(0.0, 1.0),
            dry_level: self.dry_level.clamp(-80.0, 20.0),
            wet_level: self.wet_level.clamp(-80.0, 20.0),
        }
    }
}

/// What one sound effect does, its settings kept in range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectSettings {
    Reverb(Reverb),
    /// Decibels for the lows, mids and highs; the mids run from `mid_low`
    /// to `mid_high` hertz.
    Equalizer {
        low_gain: f64,
        mid_gain: f64,
        high_gain: f64,
        mid_low: f64,
        mid_high: f64,
    },
    /// Decibels, a ratio, seconds and decibels.
    Compressor {
        threshold: f64,
        ratio: f64,
        attack: f64,
        release: f64,
        makeup: f64,
    },
    /// Seconds between repeats, then decibels.
    Echo {
        delay: f64,
        feedback: f64,
        dry_level: f64,
        wet_level: f64,
    },
    Distortion {
        level: f64,
    },
    PitchShift {
        octave: f64,
    },
    LowPass {
        cutoff: f64,
        resonance: f64,
    },
    HighPass {
        cutoff: f64,
        resonance: f64,
    },
}

/// One sound effect Instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub id: InstanceId,
    pub enabled: bool,
    pub settings: EffectSettings,
}

/// What can't change on an effect without building it again: which
/// Instance it is, its class, and an echo's delay (Kira fixes it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectShape {
    id: InstanceId,
    kind: std::mem::Discriminant<EffectSettings>,
    delay: Option<u64>,
}

impl Effect {
    /// The sound effect `id`, or `None` when it isn't one.
    pub fn of(world: &World, id: InstanceId) -> Option<Self> {
        let instance = world.get(id)?;
        let number = |name, default: f64| number(instance, name, default);
        let settings = match instance.class() {
            ClassName::ReverbSoundEffect => EffectSettings::Reverb(
                Reverb {
                    decay_time: number("DecayTime", 1.5),
                    density: number("Density", 1.0),
                    dry_level: number("DryLevel", -6.0),
                    wet_level: number("WetLevel", 0.0),
                }
                .clamped(),
            ),
            ClassName::EqualizerSoundEffect => {
                let range = instance
                    .property("MidRange")
                    .and_then(Value::as_vector2)
                    .map_or((400.0, 4000.0), |range| {
                        (f64::from(range.x), f64::from(range.y))
                    });
                let (low, high) = (finite_or(range.0, 400.0), finite_or(range.1, 4000.0));
                let mid_low = low.min(high).clamp(20.0, 20_000.0);
                let mid_high = low.max(high).clamp(mid_low + 1.0, 20_001.0);
                EffectSettings::Equalizer {
                    low_gain: number("LowGain", 0.0).clamp(-80.0, 10.0),
                    mid_gain: number("MidGain", 0.0).clamp(-80.0, 10.0),
                    high_gain: number("HighGain", 0.0).clamp(-80.0, 10.0),
                    mid_low,
                    mid_high,
                }
            }
            ClassName::CompressorSoundEffect => EffectSettings::Compressor {
                threshold: number("Threshold", -40.0).clamp(-60.0, 0.0),
                ratio: number("Ratio", 40.0).clamp(1.0, 50.0),
                attack: number("Attack", 0.1).clamp(0.0001, 1.0),
                release: number("Release", 0.1).clamp(0.0001, 5.0),
                makeup: number("GainMakeup", 0.0).clamp(0.0, 30.0),
            },
            ClassName::EchoSoundEffect => EffectSettings::Echo {
                delay: number("Delay", 1.0).clamp(0.01, 5.0),
                feedback: number("Feedback", -3.0).clamp(-80.0, 0.0),
                dry_level: number("DryLevel", 0.0).clamp(-80.0, 20.0),
                wet_level: number("WetLevel", 0.0).clamp(-80.0, 20.0),
            },
            ClassName::DistortionSoundEffect => EffectSettings::Distortion {
                level: number("Level", 0.75).clamp(0.0, 1.0),
            },
            ClassName::PitchShiftSoundEffect => EffectSettings::PitchShift {
                octave: number("Octave", 1.25).clamp(0.5, 2.0),
            },
            ClassName::LowPassSoundEffect => EffectSettings::LowPass {
                cutoff: number("Cutoff", 1000.0).clamp(10.0, 20_000.0),
                resonance: number("Resonance", 0.0).clamp(0.0, 1.0),
            },
            ClassName::HighPassSoundEffect => EffectSettings::HighPass {
                cutoff: number("Cutoff", 500.0).clamp(10.0, 20_000.0),
                resonance: number("Resonance", 0.0).clamp(0.0, 1.0),
            },
            _ => return None,
        };
        Some(Self {
            id,
            enabled: instance.property("Enabled").and_then(Value::as_bool) != Some(false),
            settings,
        })
    }

    pub fn shape(&self) -> EffectShape {
        EffectShape {
            id: self.id,
            kind: std::mem::discriminant(&self.settings),
            delay: match self.settings {
                EffectSettings::Echo { delay, .. } => Some(delay.to_bits()),
                _ => None,
            },
        }
    }
}

fn finite_or(value: f64, default: f64) -> f64 {
    if value.is_finite() { value } else { default }
}

fn number(instance: &Instance, name: &str, default: f64) -> f64 {
    instance
        .property(name)
        .and_then(Value::as_number)
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

/// The sound effects inside `owner` (a Sound or SoundGroup), in the order
/// they're applied: higher `Priority` first, equal ones in child order.
pub fn effects(world: &World, owner: InstanceId) -> Vec<Effect> {
    let Some(instance) = world.get(owner) else {
        return Vec::new();
    };
    let mut found: Vec<(f64, Effect)> = instance
        .children()
        .iter()
        .filter_map(|&child| {
            let effect = Effect::of(world, child)?;
            let priority = world.get(child).map_or(0.0, |c| number(c, "Priority", 0.0));
            Some((priority, effect))
        })
        .collect();
    // A stable sort keeps child order among equal priorities.
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().map(|(_, effect)| effect).collect()
}

/// One SoundGroup.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: InstanceId,
    /// The SoundGroup it's inside, which it's a sub-bus of.
    pub parent: Option<InstanceId>,
    pub name: String,
    /// `Volume`, at least 0.
    pub volume: f64,
    pub user_adjustable: bool,
    pub effects: Vec<Effect>,
}

/// Every SoundGroup in the game, each parent before the groups inside it.
pub fn groups(world: &World) -> Vec<Group> {
    world
        .descendants_iter(world.root())
        .filter_map(|id| {
            let instance = world.get(id)?;
            if instance.class() != ClassName::SoundGroup {
                return None;
            }
            let parent = instance.parent().filter(|&parent| is_group(world, parent));
            Some(Group {
                id,
                parent,
                name: instance.name().to_owned(),
                volume: number(instance, "Volume", 1.0).max(0.0),
                user_adjustable: instance.property("UserAdjustable").and_then(Value::as_bool)
                    == Some(true),
                effects: effects(world, id),
            })
        })
        .collect()
}

pub fn is_group(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class() == ClassName::SoundGroup)
}

/// The SoundGroup a Sound plays through: its `SoundGroup`, when that is a
/// SoundGroup still in the game.
pub fn sound_group(world: &World, sound: &Instance) -> Option<InstanceId> {
    sound
        .property("SoundGroup")
        .and_then(Value::as_instance)
        .filter(|&group| is_group(world, group) && world.is_descendant_of(group, world.root()))
}

/// The editor Mixer's mute and solo, by SoundGroup. They change what's
/// heard during Play, never the world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub muted: HashSet<InstanceId>,
    pub soloed: HashSet<InstanceId>,
}

impl Monitor {
    /// The gain on `group`'s own output: 0 when it's muted, or when
    /// something is soloed and it's neither soloed, inside a soloed group,
    /// nor holding one.
    pub fn group_gain(&self, groups: &[Group], group: InstanceId) -> f64 {
        if self.muted.contains(&group) {
            return 0.0;
        }
        if self.soloed.is_empty()
            || self.carries(groups, Some(group))
            || self
                .soloed
                .iter()
                .any(|&soloed| is_above(groups, group, soloed))
        {
            1.0
        } else {
            0.0
        }
    }

    /// The gain on a Sound playing straight into `group` (or into no
    /// group): with something soloed, only a soloed group or one inside it
    /// lets its own Sounds through, so a soloed sub-bus's parent doesn't.
    pub fn sound_gain(&self, groups: &[Group], group: Option<InstanceId>) -> f64 {
        if self.soloed.is_empty() || self.carries(groups, group) {
            1.0
        } else {
            0.0
        }
    }

    /// Whether `group` is soloed or inside a soloed group.
    fn carries(&self, groups: &[Group], group: Option<InstanceId>) -> bool {
        let mut current = group;
        while let Some(id) = current {
            if self.soloed.contains(&id) {
                return true;
            }
            current = groups.iter().find(|g| g.id == id).and_then(|g| g.parent);
        }
        false
    }
}

/// Whether `above` holds `group`, directly or further up.
fn is_above(groups: &[Group], above: InstanceId, group: InstanceId) -> bool {
    let mut current = groups.iter().find(|g| g.id == group).and_then(|g| g.parent);
    while let Some(id) = current {
        if id == above {
            return true;
        }
        current = groups.iter().find(|g| g.id == id).and_then(|g| g.parent);
    }
    false
}

/// A ReverbZone the listener is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub id: InstanceId,
    pub reverb: Reverb,
    /// Seconds the reverb takes to blend in or out, at least 0.
    pub fade_time: f64,
}

/// The ReverbZone in the Workspace that `point` is inside: the smallest
/// when several are. A Ball zone is its sphere, a Cylinder its cylinder
/// (along X), and any other shape its box.
pub fn zone_at(world: &World, point: Vector3) -> Option<Zone> {
    let mut best: Option<(f64, Zone)> = None;
    for id in world.descendants_iter(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::ReverbZone {
            continue;
        }
        let Some(volume) = inside_volume(instance, point) else {
            continue;
        };
        if best
            .as_ref()
            .is_some_and(|(smallest, _)| *smallest <= volume)
        {
            continue;
        }
        let preset = match instance.property("ReverbType") {
            Some(Value::EnumItem(item)) => item.item.as_str(),
            _ => "",
        };
        let zone = Zone {
            id,
            reverb: Reverb::preset(preset),
            fade_time: number(instance, "FadeTime", 1.0).max(0.0),
        };
        best = Some((volume, zone));
    }
    best.map(|(_, zone)| zone)
}

/// The part's volume in cubic studs when `point` is inside it.
fn inside_volume(part: &Instance, point: Vector3) -> Option<f64> {
    let vector = |name| {
        part.property(name)
            .and_then(Value::as_vector3)
            .unwrap_or_default()
    };
    let size = vector("Size");
    let half = [size.x.abs() / 2.0, size.y.abs() / 2.0, size.z.abs() / 2.0];
    let local = to_part_space(vector("Position"), vector("Orientation"), point);
    let volume = f64::from(size.x.abs()) * f64::from(size.y.abs()) * f64::from(size.z.abs());
    let shape = part
        .property("Shape")
        .and_then(Value::as_part_type)
        .map(|shape| shape.as_str());
    let inside = match shape {
        Some("Ball") => {
            let radius = half[0].min(half[1]).min(half[2]);
            local[0] * local[0] + local[1] * local[1] + local[2] * local[2] <= radius * radius
        }
        Some("Cylinder") => {
            let radius = half[1].min(half[2]);
            local[0].abs() <= half[0]
                && local[1] * local[1] + local[2] * local[2] <= radius * radius
        }
        _ => (0..3).all(|axis| local[axis].abs() <= half[axis]),
    };
    inside.then_some(volume)
}

/// `point` in the part's own frame. A part turns by its `Orientation` (in
/// degrees) about Z first, then X, then Y, so this turns back in the
/// opposite order.
fn to_part_space(position: Vector3, orientation: Vector3, point: Vector3) -> [f32; 3] {
    let mut p = [
        point.x - position.x,
        point.y - position.y,
        point.z - position.z,
    ];
    let turn = |p: &mut [f32; 3], a: usize, b: usize, degrees: f32| {
        let (sin, cos) = (-degrees.to_radians()).sin_cos();
        let (x, y) = (p[a], p[b]);
        p[a] = x * cos - y * sin;
        p[b] = x * sin + y * cos;
    };
    // About Y, X turns toward -Z: (z, x) is the plane it turns in.
    turn(&mut p, 2, 0, orientation.y);
    turn(&mut p, 1, 2, orientation.x);
    turn(&mut p, 0, 1, orientation.z);
    p
}

#[cfg(test)]
mod tests;
