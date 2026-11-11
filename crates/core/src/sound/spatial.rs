//! Where a spatial Sound is heard from, how loud it is at a distance
//! (`RollOffMode`, roadmap G8) and how walls between it and the listener
//! muffle it. Shared by `audio`, which plays it, and `sim`, which casts the
//! rays.

use crate::{ClassName, Instance, InstanceId, Value, Vector3, World};

/// `Enum.RollOffMode`'s items, as Roblox has them.
pub const ROLL_OFF_MODES: [&str; 4] = ["Inverse", "Linear", "LinearSquare", "InverseTapered"];

/// How a spatial Sound gets quieter with distance, between
/// `RollOffMinDistance` (full) and `RollOffMaxDistance` (silent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RollOff {
    /// `min / distance`, as sound behaves in the open: halving at twice
    /// the min distance. Cut off at the max distance.
    #[default]
    Inverse,
    /// A straight line from full to silent.
    Linear,
    /// The square of [`Linear`](RollOff::Linear): quieter sooner.
    LinearSquare,
    /// Inverse while that's quieter, then LinearSquare, so it fades out
    /// smoothly before the max distance.
    InverseTapered,
}

impl RollOff {
    /// The mode `name` (one of [`ROLL_OFF_MODES`]), or Inverse.
    pub fn named(name: &str) -> RollOff {
        match name {
            "Linear" => RollOff::Linear,
            "LinearSquare" => RollOff::LinearSquare,
            "InverseTapered" => RollOff::InverseTapered,
            _ => RollOff::Inverse,
        }
    }

    /// A Sound's `RollOffMode`.
    pub fn of(sound: &Instance) -> RollOff {
        match sound.property("RollOffMode") {
            Some(Value::EnumItem(item)) => RollOff::named(&item.item),
            Some(Value::String(name)) => RollOff::named(name),
            _ => RollOff::default(),
        }
    }

    /// The amplitude, 0 to 1, at `distance` studs from the listener.
    pub fn gain(self, distance: f32, min: f32, max: f32) -> f32 {
        if !distance.is_finite() || distance >= max {
            return 0.0;
        }
        if distance <= min {
            return 1.0;
        }
        let linear = 1.0 - (distance - min) / (max - min).max(f32::EPSILON);
        let inverse = min / distance;
        match self {
            RollOff::Inverse => inverse,
            RollOff::Linear => linear,
            RollOff::LinearSquare => linear * linear,
            RollOff::InverseTapered => inverse.min(linear * linear),
        }
        .clamp(0.0, 1.0)
    }
}

/// The most walls counted between a Sound and the listener.
pub const MAX_WALLS: u32 = 3;

/// What `walls` solid parts between a spatial Sound and the listener do to
/// it: the amplitude it keeps and the low-pass cutoff in hertz. Each wall
/// halves it (-6 dB) and takes off more of the highs.
pub fn muffle(walls: u32) -> (f32, f64) {
    let walls = walls.min(MAX_WALLS);
    if walls == 0 {
        return (1.0, OPEN_CUTOFF);
    }
    (
        0.5f32.powi(walls as i32),
        4_000.0 / f64::from(1u32 << walls),
    )
}

/// The low-pass cutoff of a Sound nothing is in front of: above hearing.
pub const OPEN_CUTOFF: f64 = 20_000.0;

/// Where a spatial Sound is heard from, in studs: the nearest ancestor
/// Part, MeshPart or Terrain, or the Sound's own `Position`.
pub fn emitter_position(world: &World, sound: InstanceId) -> Vector3 {
    let at = |instance: &Instance| {
        let position = instance
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap_or(Vector3::new(0.0, 0.0, 0.0));
        let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
        Vector3::new(finite(position.x), finite(position.y), finite(position.z))
    };
    match emitter_part(world, sound).and_then(|part| world.get(part)) {
        Some(part) => at(part),
        None => world.get(sound).map_or(Vector3::new(0.0, 0.0, 0.0), at),
    }
}

/// The part a spatial Sound is heard from: its nearest ancestor Part,
/// MeshPart (or another part class with a `Position`) or Terrain.
pub fn emitter_part(world: &World, sound: InstanceId) -> Option<InstanceId> {
    let mut id = world.get(sound).and_then(|instance| instance.parent());
    while let Some(current) = id {
        let instance = world.get(current)?;
        if matches!(
            instance.class(),
            ClassName::Part
                | ClassName::MeshPart
                | ClassName::Seat
                | ClassName::VehicleSeat
                | ClassName::DamageZone
                | ClassName::Terrain
        ) && instance.property("Position").is_some()
        {
            return Some(current);
        }
        id = instance.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn each_roll_off_mode_has_its_curve_between_the_distances() {
        let (min, max) = (10.0, 110.0);
        for mode in ROLL_OFF_MODES.map(RollOff::named) {
            assert_eq!(mode.gain(0.0, min, max), 1.0, "{mode:?} close up");
            assert_eq!(mode.gain(min, min, max), 1.0, "{mode:?} at min");
            assert_eq!(mode.gain(max, min, max), 0.0, "{mode:?} at max");
            assert_eq!(mode.gain(500.0, min, max), 0.0, "{mode:?} past max");
            // Never louder further away.
            let mut last = 1.0;
            for step in 0..=120 {
                let gain = mode.gain(step as f32, min, max);
                assert!(gain <= last + 1e-6, "{mode:?} at {step}");
                last = gain;
            }
        }
        // Halfway: 60 studs, so linear is 0.5 and inverse 10/60.
        assert!(close(RollOff::Inverse.gain(60.0, min, max), 1.0 / 6.0));
        assert!(
            close(RollOff::Inverse.gain(20.0, min, max), 0.5),
            "half at twice min"
        );
        assert!(close(RollOff::Linear.gain(60.0, min, max), 0.5));
        assert!(close(RollOff::LinearSquare.gain(60.0, min, max), 0.25));
        // Tapered: inverse near (quieter than LinearSquare there), then
        // LinearSquare near the end.
        assert!(close(RollOff::InverseTapered.gain(20.0, min, max), 0.5));
        let near_end = RollOff::InverseTapered.gain(100.0, min, max);
        assert!(close(near_end, 0.01), "{near_end}");
        assert!(near_end < RollOff::Inverse.gain(100.0, min, max));
    }

    #[test]
    fn a_sound_reads_its_mode_and_unknown_names_are_inverse() {
        let mut world = World::new();
        let sound = world.create(ClassName::Sound).unwrap();
        assert_eq!(
            RollOff::of(world.get(sound).unwrap()),
            RollOff::Inverse,
            "the default"
        );
        let item = |name: &str| {
            Value::EnumItem(crate::EnumItem {
                enum_name: "RollOffMode".to_owned(),
                item: name.to_owned(),
            })
        };
        world
            .set_property(sound, "RollOffMode", item("InverseTapered"))
            .unwrap();
        assert_eq!(
            RollOff::of(world.get(sound).unwrap()),
            RollOff::InverseTapered
        );
        assert_eq!(RollOff::named("Sideways"), RollOff::Inverse);
    }

    #[test]
    fn each_wall_halves_the_sound_and_dulls_it_more() {
        assert_eq!(muffle(0), (1.0, OPEN_CUTOFF));
        assert_eq!(muffle(1), (0.5, 2_000.0));
        assert_eq!(muffle(2), (0.25, 1_000.0));
        assert_eq!(muffle(3), (0.125, 500.0));
        assert_eq!(muffle(9), muffle(MAX_WALLS));
    }

    #[test]
    fn a_sound_is_heard_from_its_nearest_part_or_its_own_position() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        let at = Vector3::new(4.0, 5.0, 6.0);
        world
            .set_property(part, "Position", Value::Vector3(at))
            .unwrap();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(part)).unwrap();
        let sound = world.create(ClassName::Sound).unwrap();
        world.set_parent(sound, Some(folder)).unwrap();
        assert_eq!(emitter_part(&world, sound), Some(part));
        assert_eq!(emitter_position(&world, sound), at);

        let loose = world.create(ClassName::Sound).unwrap();
        world.set_parent(loose, Some(workspace)).unwrap();
        let own = Vector3::new(-1.0, 2.0, 3.0);
        world
            .set_property(loose, "Position", Value::Vector3(own))
            .unwrap();
        assert_eq!(emitter_part(&world, loose), None);
        assert_eq!(emitter_position(&world, loose), own);
    }
}
