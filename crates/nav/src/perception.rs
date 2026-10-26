//! What a navigation agent notices: the player character coming into its
//! sight or its hearing.
//!
//! Sight reaches `SightRange` studs from the agent's eyes, `SightAngle`
//! degrees across, centred on the way it faces, and colliders block it.
//! Hearing reaches `HearingRange` studs, through walls, and hears a `Sound`
//! playing anywhere in the player character. The player character is the
//! one whose controller owns the camera. Ranges of zero sense nothing.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::cast_ray_excluding;

use crate::agents::body_box;

/// How high up its body an agent's eyes are, and the player character's
/// head: this share of the body's height above its feet.
const EYE_HEIGHT: f32 = 0.9;
/// The most sensors (`CanCollide` off, which don't block sight) one line of
/// sight looks through before it gives up and counts as clear.
const MAX_SENSORS: usize = 8;

/// One of an agent's senses, as `PlayerSensed` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Sight,
    Hearing,
}

impl Sense {
    pub fn as_str(self) -> &'static str {
        match self {
            Sense::Sight => "Sight",
            Sense::Hearing => "Hearing",
        }
    }
}

/// An agent started seeing or hearing the player character. Hosts fire its
/// `PlayerSensed` with the sense's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sensed {
    pub agent: InstanceId,
    pub sense: Sense,
}

/// What one agent senses. Not saved.
#[derive(Default)]
pub(crate) struct Senses {
    /// The way it faces, across (a unit X and Z): the way it last moved.
    /// `None` until it moves, while it faces its body's front.
    facing: Option<(f32, f32)>,
    sees: bool,
    hears: bool,
}

/// The player character, as agents sense it.
pub(crate) struct Player {
    parts: Vec<InstanceId>,
    /// The middle of its body's box, and its head.
    points: [Vector3; 2],
    /// True while a Sound in it is playing.
    sounding: bool,
}

impl Player {
    /// The player character, when a controller has a body.
    pub(crate) fn find(world: &World) -> Option<Self> {
        let holder = axcenity_character::player_character(world)?;
        let parts = axcenity_character::body_parts(world, holder);
        let (min, max) = body_box(world, &parts)?;
        let (x, z) = ((min.x + max.x) * 0.5, (min.z + max.z) * 0.5);
        let middle = Vector3::new(x, (min.y + max.y) * 0.5, z);
        let head = Vector3::new(x, min.y + (max.y - min.y) * EYE_HEIGHT, z);
        let sounding = world.descendants(holder).into_iter().any(|id| {
            world.get(id).is_some_and(|instance| {
                instance.class() == ClassName::Sound
                    && instance.property("Playing").and_then(Value::as_bool) == Some(true)
            })
        });
        Some(Self {
            parts,
            points: [middle, head],
            sounding,
        })
    }
}

/// Updates what the agent `id`, whose body is `parts`, senses of `player`
/// after it moved `moved` across this step. Writes `SeesPlayer` and
/// `HearsPlayer` when they change, and returns the senses that have just
/// started.
pub(crate) fn perceive(
    world: &mut World,
    assets: &Assets,
    id: InstanceId,
    parts: &[InstanceId],
    senses: &mut Senses,
    player: Option<&Player>,
    moved: (f32, f32),
) -> Vec<Sense> {
    let across = moved.0.hypot(moved.1);
    if across > 1e-4 {
        senses.facing = Some((moved.0 / across, moved.1 / across));
    }
    let (sees, hears) = match (player, body_box(world, parts)) {
        (Some(player), Some((min, max))) if !player.parts.iter().any(|p| parts.contains(p)) => {
            let eyes = Vector3::new(
                (min.x + max.x) * 0.5,
                min.y + (max.y - min.y) * EYE_HEIGHT,
                (min.z + max.z) * 0.5,
            );
            let facing = senses.facing.unwrap_or_else(|| front(world, parts));
            let mut skip = parts.to_vec();
            skip.extend(&player.parts);
            (
                sees(world, assets, id, eyes, facing, player, &skip),
                hears(world, id, eyes, player),
            )
        }
        _ => (false, false),
    };
    let mut started = Vec::new();
    for (sense, now, was, property) in [
        (Sense::Sight, sees, &mut senses.sees, "SeesPlayer"),
        (Sense::Hearing, hears, &mut senses.hears, "HearsPlayer"),
    ] {
        if now && !*was {
            started.push(sense);
        }
        *was = now;
        let shown = world
            .get(id)
            .and_then(|instance| instance.property(property))
            .and_then(Value::as_bool);
        if shown != Some(now) {
            let _ = world.set_property(id, property, Value::Bool(now));
        }
    }
    started
}

/// Whether the agent sees either of the player's points: in range, inside
/// the angle across, with no collider between.
fn sees(
    world: &World,
    assets: &Assets,
    id: InstanceId,
    eyes: Vector3,
    facing: (f32, f32),
    player: &Player,
    skip: &[InstanceId],
) -> bool {
    let range = number(world, id, "SightRange");
    let angle = number(world, id, "SightAngle").min(360.0);
    if range <= 0.0 || angle <= 0.0 {
        return false;
    }
    let least_cos = (angle * 0.5).to_radians().cos();
    player.points.iter().any(|&to| {
        let (dx, dy, dz) = (to.x - eyes.x, to.y - eyes.y, to.z - eyes.z);
        if (dx * dx + dy * dy + dz * dz).sqrt() > range {
            return false;
        }
        // Straight above or below counts as in front.
        let across = dx.hypot(dz);
        if across > 1e-4 && (dx * facing.0 + dz * facing.1) / across < least_cos - 1e-6 {
            return false;
        }
        clear_line(world, assets, eyes, to, skip)
    })
}

/// Whether nothing that collides is between `from` and `to`. Sensors
/// (`CanCollide` off) don't block it.
fn clear_line(
    world: &World,
    assets: &Assets,
    from: Vector3,
    to: Vector3,
    skip: &[InstanceId],
) -> bool {
    let direction = Vector3::new(to.x - from.x, to.y - from.y, to.z - from.z);
    let mut skip = skip.to_vec();
    for _ in 0..MAX_SENSORS {
        match cast_ray_excluding(world, assets, from, direction, &skip) {
            None => return true,
            Some(hit) if hit.can_collide => return false,
            Some(hit) => skip.push(hit.instance),
        }
    }
    true
}

/// Whether the agent hears the player: a Sound in it is playing, within
/// range of the agent's eyes, whatever is between.
fn hears(world: &World, id: InstanceId, eyes: Vector3, player: &Player) -> bool {
    let range = number(world, id, "HearingRange");
    let [middle, _] = player.points;
    let (dx, dy, dz) = (middle.x - eyes.x, middle.y - eyes.y, middle.z - eyes.z);
    player.sounding && range > 0.0 && (dx * dx + dy * dy + dz * dz).sqrt() <= range
}

/// The way the body's first part faces across: its front (-Z), turned by
/// its `Orientation`'s Y.
fn front(world: &World, parts: &[InstanceId]) -> (f32, f32) {
    let yaw = parts
        .first()
        .and_then(|&part| world.get(part))
        .and_then(|instance| instance.property("Orientation"))
        .and_then(Value::as_vector3)
        .map_or(0.0, |orientation| orientation.y)
        .to_radians();
    if yaw.is_finite() {
        (-yaw.sin(), -yaw.cos())
    } else {
        (0.0, -1.0)
    }
}

/// A number property, or 0 when it's missing or not finite.
fn number(world: &World, id: InstanceId, name: &str) -> f32 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0) as f32
}
