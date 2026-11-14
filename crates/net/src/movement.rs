//! The server's checks of where a device says its character went, beyond
//! speed (multiplayer readiness MP3b): through a wall, or kept in the air
//! longer than a jump lasts. A refused move is moved back
//! ([`CharacterCheck`](crate::CharacterCheck)); many in a row are
//! [`Strikes`], which flag the player.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::{CastShape, QueryParams, RayHit, gravity, raycast, shapecast};

/// Why a device is flagged for its character's or parts' moves: what
/// follows "their device" in the log and the kick message.
pub const TOO_FAST: &str = "moved its character faster than it can go";
pub const THROUGH_WALL: &str = "moved its character through a wall";
pub const IN_THE_AIR: &str = "kept its character in the air";
pub const PART_TOO_FAST: &str = "moved a part faster than physics does";

/// How far into solid a move's path may go before it's through a wall:
/// room for a path that cuts a corner between two updates.
pub const WALL_DEPTH: f64 = 0.3;
/// How close to a part's side a character's middle may be inside it: a
/// character falling past an edge brushes it (MP1's bots, 0.06 studs in).
/// Walls thinner than twice this aren't checked.
const SHALLOW: f32 = 0.25;

/// How far below its feet a character is still standing on something.
const SUPPORT_REACH: f64 = 1.5;
/// Seconds a character may seem in the air before the check starts, for
/// packets that arrive late.
const AIR_GRACE: f64 = 0.3;
/// Studs over the highest a jump reaches.
const AIR_SLACK: f64 = 2.0;
/// Parts an unanchored part on a path is passed by, at most.
const SKIPS: usize = 4;

/// Refused moves a device may make before it's flagged, and how many a
/// second it's forgiven: a lag spike's are a round trip's updates (18 at
/// 300 ms), and a cheat going twice as fast as it may has one every few
/// updates, for as long as it cheats.
const STRIKES: f64 = 40.0;
const FORGIVEN_PER_SECOND: f64 = 4.0;
/// Seconds between flags for refused moves.
const FLAG_EVERY: f64 = 1.0;

/// Whether the path from `was` to `to` (a character's root, where the
/// server had it and where the device says it went) goes through an
/// anchored solid part or Terrain, or ends inside one. Only deep counts:
/// more than [`WALL_DEPTH`] along the path and more than [`SHALLOW`] from
/// every side, since a character's middle brushes a part's edge as it
/// falls past it. `skip` is passed by: the players' characters. Parts the
/// path starts deep inside are passed by too, so a character stuck in a
/// part may walk out of it.
pub fn through_wall(
    world: &World,
    assets: &Assets,
    group: Option<String>,
    skip: Vec<InstanceId>,
    was: Vector3,
    to: Vector3,
) -> bool {
    let path = Vector3::new(to.x - was.x, to.y - was.y, to.z - was.z);
    let length = path.x.hypot(path.y).hypot(path.z);
    if length <= f32::EPSILON {
        return false;
    }
    let along = |from: Vector3, by: f32| {
        let t = by / length;
        Vector3::new(
            from.x + path.x * t,
            from.y + path.y * t,
            from.z + path.z * t,
        )
    };
    let mut params = QueryParams {
        filter: skip,
        collision_group: group,
        respect_can_collide: true,
        ..QueryParams::default()
    };
    let Some(enters) = wall_hit(world, assets, &mut params, was, path, true) else {
        return false;
    };
    // From the far end back: where the path leaves solid.
    let back = Vector3::new(-path.x, -path.y, -path.z);
    let Some(leaves) = wall_hit(world, assets, &mut params, to, back, false) else {
        return false;
    };
    if leaves.distance <= f32::EPSILON && deep_inside(world, assets, to, leaves.instance) {
        return true;
    }
    let inside = length - enters.distance - leaves.distance;
    if f64::from(inside) <= WALL_DEPTH {
        return false;
    }
    let entry = along(was, enters.distance);
    let exit = along(was, length - leaves.distance);
    if enters.instance == leaves.instance {
        let middle = along(entry, 0.5 * inside);
        deep_inside(world, assets, middle, enters.instance)
    } else {
        let depth = WALL_DEPTH as f32;
        deep_inside(world, assets, along(entry, depth), enters.instance)
            || deep_inside(world, assets, along(exit, -depth), leaves.instance)
    }
}

/// Whether `point` is inside `part` further than [`SHALLOW`] from it
/// along each axis: a ray from each of the six points that far away back
/// to it starts inside too.
fn deep_inside(world: &World, assets: &Assets, point: Vector3, part: InstanceId) -> bool {
    let params = QueryParams {
        filter: vec![part],
        include: true,
        ..QueryParams::default()
    };
    let axes = [
        (1.0, 0.0, 0.0),
        (-1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, -1.0, 0.0),
        (0.0, 0.0, 1.0),
        (0.0, 0.0, -1.0),
    ];
    axes.into_iter().all(|(x, y, z)| {
        let out = Vector3::new(
            point.x + x * SHALLOW,
            point.y + y * SHALLOW,
            point.z + z * SHALLOW,
        );
        let back = Vector3::new(-x * SHALLOW, -y * SHALLOW, -z * SHALLOW);
        raycast(world, assets, out, back, &params).is_some_and(|hit| hit.distance <= f32::EPSILON)
    })
}

/// The first anchored solid part or Terrain along the ray, passing by
/// unanchored parts (which a character may push) and, when `escape`, any
/// it starts deep inside (only just inside is a way in); both are added
/// to `params`' filter.
fn wall_hit(
    world: &World,
    assets: &Assets,
    params: &mut QueryParams,
    origin: Vector3,
    direction: Vector3,
    escape: bool,
) -> Option<RayHit> {
    for _ in 0..=SKIPS {
        let hit = raycast(world, assets, origin, direction, params)?;
        let instance = world.get(hit.instance)?;
        let fixed = instance.class() == ClassName::Terrain
            || instance.property("Anchored").and_then(Value::as_bool) == Some(true);
        let stuck = escape
            && hit.distance <= f32::EPSILON
            && deep_inside(world, assets, origin, hit.instance);
        if fixed && !stuck {
            return Some(hit);
        }
        params.filter.push(hit.instance);
    }
    None
}

/// Whether the character whose root is at `at` stands on something, or
/// nearly: a solid part or Terrain within [`SUPPORT_REACH`] under its
/// lowest part. `character` (where the server has it) is passed by.
pub fn supported(world: &World, assets: &Assets, character: InstanceId, at: Vector3) -> bool {
    let Some((feet, width)) = feet_below(world, character) else {
        return true;
    };
    let params = QueryParams {
        filter: vec![character],
        respect_can_collide: true,
        ..QueryParams::default()
    };
    let down = Vector3::new(0.0, -(feet + SUPPORT_REACH) as f32, 0.0);
    // Under its middle first; then its width, for one on an edge.
    if raycast(world, assets, at, down, &params).is_some() {
        return true;
    }
    let radius = (0.5 * width).max(0.5) as f32;
    let ball = CastShape::Ball { center: at, radius };
    let down = Vector3::new(
        0.0,
        -(feet + SUPPORT_REACH - f64::from(radius)).max(0.1) as f32,
        0.0,
    );
    shapecast(world, assets, ball, down, &params).is_some()
}

/// How far its root is above the bottom of `character`'s lowest part, and
/// how wide its root is, as the server has them.
fn feet_below(world: &World, character: InstanceId) -> Option<(f64, f64)> {
    let root = crate::ownership::root_of(world, character)?;
    let vector = |id: InstanceId, name: &str| world.get(id)?.property(name)?.as_vector3();
    let root_at = vector(root, "Position")?;
    let root_size = vector(root, "Size")?;
    let lowest = world
        .descendants_iter(character)
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .filter_map(|id| Some((vector(id, "Position")?, vector(id, "Size")?)))
        // Half its diagonal: low enough however it's turned.
        .map(|(at, s)| f64::from(at.y) - 0.5 * f64::from(s.x.hypot(s.y).hypot(s.z)))
        .fold(f64::INFINITY, f64::min);
    let feet = (f64::from(root_at.y) - lowest).max(0.0);
    Some((feet, f64::from(root_size.x.max(root_size.z))))
}

/// The highest a character may be `seconds` after it last stood at
/// `height`, with `jump_height`: a jump's arc, flat at its top for as long
/// as it rises, then falling at `Workspace.Gravity`. `None` when nothing
/// pulls it down.
pub fn highest(world: &World, height: f64, jump_height: f64, seconds: f64) -> Option<f64> {
    let fall = f64::from(gravity(world));
    if fall <= 0.0 {
        return None;
    }
    let launch = (2.0 * f64::from(axcenity_physics::GRAVITY) * jump_height.max(0.0)).sqrt();
    let rises = launch / fall;
    let top = height + launch * launch / (2.0 * fall) * crate::ownership::TOLERANCE + AIR_SLACK;
    let falling = (seconds - AIR_GRACE - rises).max(0.0);
    Some(top - 0.5 * fall * falling * falling)
}

/// Refused moves, forgiven over time: a device that keeps making them is
/// flagged once a second.
#[derive(Clone, Copy, Debug)]
pub struct Strikes {
    left: f64,
    at: f64,
    flagged: Option<f64>,
}

impl Default for Strikes {
    fn default() -> Self {
        Strikes {
            left: STRIKES,
            at: 0.0,
            flagged: None,
        }
    }
}

impl Strikes {
    /// One refused move at `now`: true when the device is to be flagged.
    pub fn strike(&mut self, now: f64) -> bool {
        self.left = (self.left + (now - self.at).max(0.0) * FORGIVEN_PER_SECOND).min(STRIKES);
        self.at = now;
        self.left -= 1.0;
        if self.left > 0.0 || self.flagged.is_some_and(|at| now - at < FLAG_EVERY) {
            return false;
        }
        self.flagged = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lag_spikes_refusals_are_forgiven_but_a_cheats_are_flagged_once_a_second() {
        let mut strikes = Strikes::default();
        // A lag spike's 18 refusals (300 ms at 60 updates) every five
        // seconds: never flagged.
        for spike in 0..10 {
            for n in 0..18 {
                assert!(!strikes.strike(spike as f64 * 5.0 + n as f64 / 60.0));
            }
        }
        // Every update refused for 5 seconds: flagged about once a second
        // once the strikes run out.
        let start = 60.0;
        let flags = (0..300)
            .filter(|&n| strikes.strike(start + n as f64 / 60.0))
            .count();
        assert!((3..=5).contains(&flags), "{flags}");
    }

    /// A floor 60 studs deep (its top at y = 0, its edge at z = -30), a
    /// wall a stud thick on it, and a slab a stud thick at x = 20.
    fn floor_and_wall() -> World {
        let mut world = World::new();
        for (position, size) in [
            (Vector3::new(0.0, -0.5, 0.0), Vector3::new(60.0, 1.0, 60.0)),
            (Vector3::new(0.0, 5.0, 10.0), Vector3::new(20.0, 10.0, 1.0)),
            (Vector3::new(20.0, 0.5, 0.0), Vector3::new(12.0, 1.0, 12.0)),
        ] {
            let id = world.create(ClassName::Part).unwrap();
            world.set_parent(id, Some(world.workspace())).unwrap();
            world
                .set_property(id, "Position", Value::Vector3(position))
                .unwrap();
            world
                .set_property(id, "Size", Value::Vector3(size))
                .unwrap();
            world
                .set_property(id, "Anchored", Value::Bool(true))
                .unwrap();
        }
        world
    }

    #[test]
    fn deep_paths_through_parts_are_walls_and_brushing_an_edge_is_not() {
        let world = floor_and_wall();
        let assets = Assets::default();
        let wall = |was: (f32, f32, f32), to: (f32, f32, f32)| {
            let v = |(x, y, z)| Vector3::new(x, y, z);
            through_wall(&world, &assets, None, Vec::new(), v(was), v(to))
        };
        // Across the wall in one update, and into it.
        assert!(wall((0.0, 3.0, 9.0), (0.0, 3.0, 11.0)));
        assert!(wall((0.0, 3.0, 9.3), (0.0, 3.0, 10.0)));
        // Up to it, and along it.
        assert!(!wall((0.0, 3.0, 9.0), (0.0, 3.0, 9.4)));
        assert!(!wall((-2.0, 3.0, 9.3), (2.0, 3.0, 9.3)));
        // Falling past the floor's edge, a twentieth of a stud inside it
        // (MP1's bots).
        assert!(!wall((5.0, 0.24, -29.95), (5.0, -0.7, -29.95)));
        assert!(!wall((5.0, -0.7, -29.95), (5.0, -3.0, -29.95)));
        // Sinking through the floor: the first step in is allowed, the
        // next isn't.
        assert!(!wall((5.0, 0.1, 0.0), (5.0, -0.2, 0.0)));
        assert!(wall((5.0, -0.2, 0.0), (5.0, -0.47, 0.0)));
        // Stuck deep inside the wall, it may walk out.
        assert!(!wall((0.0, 3.0, 10.0), (0.0, 3.0, 9.0)));
        // Down through a slab on the floor (a SpawnLocation), from just
        // inside its top to just inside the floor: each end is shallow,
        // the path isn't (the noclip bot).
        assert!(wall((20.0, 0.77, 0.0), (20.0, -0.03, 0.0)));
    }

    #[test]
    fn a_jumps_arc_is_allowed_and_hovering_is_not() {
        let world = World::new();
        // JumpHeight 7.2: the top is reached in about a quarter second.
        let top = highest(&world, 0.0, 7.2, 0.4).unwrap();
        assert!(top > 7.2, "{top}");
        assert!(highest(&world, 0.0, 7.2, 1.0).unwrap() < 3.0);
        assert!(highest(&world, 10.0, 7.2, 2.0).unwrap() < 0.0);
    }
}
