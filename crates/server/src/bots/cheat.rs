//! `--cheat <kind>`: bots whose device is modified to move its character
//! where the game wouldn't (multiplayer readiness MP3b), to check that the
//! server holds them back, flags them and kicks them while the other
//! players play on. Each gives its character's controller a custom
//! `Movement` on its own device, so nothing there stops it, and moves the
//! character's parts itself.
//!
//! - `speed`: runs back and forth along X at twice `WalkSpeed`, a
//!   quarter second each way, so it stays on the map.
//! - `noclip`: sinks through the floor at `WalkSpeed`. Falling is never
//!   too fast, so only the wall check catches it.
//! - `fly`: rises at 24 studs a second (slower than a jump's launch) to
//!   30 studs over where it started, and hovers there.

use anyhow::{Result, bail};
use axcenity_core::{InstanceId, Value, Vector3, World};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cheat {
    Speed,
    Noclip,
    Fly,
}

impl Cheat {
    pub fn parse(text: &str) -> Result<Cheat> {
        Ok(match text {
            "speed" => Cheat::Speed,
            "noclip" => Cheat::Noclip,
            "fly" => Cheat::Fly,
            _ => bail!("--cheat takes speed, noclip or fly, not `{text}`"),
        })
    }

    /// Moves `character` (its root at `root`, having started at `start`)
    /// one frame of `seconds`, `time` seconds into the run, the way this
    /// cheat does.
    pub fn step(
        self,
        world: &mut World,
        character: InstanceId,
        seconds: f64,
        time: f64,
        start: Vector3,
        root: Vector3,
    ) {
        for id in world.descendants(character) {
            if world
                .get(id)
                .is_some_and(|i| i.class().is_character_controller())
            {
                let custom = Value::String("Cheat".to_owned());
                if world.get(id).and_then(|i| i.property("Movement")) != Some(&custom) {
                    let _ = world.set_property(id, "Movement", custom);
                }
            }
        }
        let walk = (16.0 * seconds) as f32;
        let (dx, dy, dz) = match self {
            Cheat::Speed => {
                let way = if ((time / 0.25) as u64).is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
                (2.0 * walk * way, 0.0, 0.0)
            }
            Cheat::Noclip => (0.0, -walk, 0.0),
            Cheat::Fly => {
                let top = start.y + 30.0;
                (
                    0.0,
                    ((24.0 * seconds) as f32).min(top - root.y).max(0.0),
                    0.0,
                )
            }
        };
        let parts: Vec<InstanceId> = world
            .descendants(character)
            .into_iter()
            .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
            .collect();
        for part in parts {
            let Some(at) = world
                .get(part)
                .and_then(|i| i.property("Position"))
                .and_then(Value::as_vector3)
            else {
                continue;
            };
            let to = Vector3::new(at.x + dx, at.y + dy, at.z + dz);
            let _ = world.set_property(part, "Position", Value::Vector3(to));
        }
    }
}
