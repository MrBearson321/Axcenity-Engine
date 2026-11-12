//! The Home tab's geometric snapping, like Studio's: parts snap flush
//! against and in line with the faces of parts near them (Surfaces), can be
//! kept from being moved into each other (Collisions), and are welded to
//! the parts they're left touching (Join Surfaces).
//!
//! Everything here is plain math on oriented boxes ([`Obb`]), each a part's
//! `Size` box turned by its `Orientation`, so a ball or a cylinder counts as
//! its box, as in Studio. The viewport works out the boxes of every other
//! part once when a drag starts ([`obstacles`]); the drag then asks how far
//! it may go ([`limit_travel`]) and where it should land ([`flush_moves`],
//! [`flush_faces`]).

use axcenity_core::{ClassName, InstanceId, Value, World};
#[cfg(any(test, feature = "test-support"))]
use axcenity_render::glam::Quat;
use axcenity_render::glam::Vec3;

use crate::gizmo::Pose;

pub mod points;

/// How deep two boxes must go into each other to count as overlapping, in
/// studs: parts left exactly flush, give or take rounding, don't.
pub const PENETRATION: f32 = 0.01;
/// How close two faces must be to count as touching for Join Surfaces.
pub const TOUCHING: f32 = 0.02;
/// How far, in points on screen, a part is pulled to line up with a face.
pub const SNAP_POINTS: f32 = 10.0;
/// How close a direction must be to one of a box's axes for its faces to
/// count as square to it (about 2.5°).
const SQUARE: f32 = 0.999;
/// Along a move, how far apart the places checked for collisions are, in
/// studs. Between two, a binary search finds the last free place.
const COLLISION_STEP: f32 = 0.25;

/// A box: its middle, its three axes (one stud long) and how far it
/// reaches along each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub center: Vec3,
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

impl Obb {
    pub fn of(pose: Pose) -> Obb {
        Obb {
            center: pose.position,
            axes: [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| pose.rotation * axis),
            half: pose.size.abs() / 2.0,
        }
    }

    /// The same box, `by` further along.
    pub fn moved(self, by: Vec3) -> Obb {
        Obb {
            center: self.center + by,
            ..self
        }
    }

    /// How far the box reaches from its middle along `direction` (one stud long).
    pub fn reach(&self, direction: Vec3) -> f32 {
        (0..3)
            .map(|i| self.axes[i].dot(direction).abs() * self.half[i])
            .sum()
    }

    /// Where the box starts and ends along `direction`.
    pub fn extent(&self, direction: Vec3) -> (f32, f32) {
        let middle = self.center.dot(direction);
        let reach = self.reach(direction);
        (middle - reach, middle + reach)
    }

    /// The world-aligned box around it, as its lowest and highest corners.
    fn bounds(&self) -> (Vec3, Vec3) {
        let reach = Vec3::new(
            self.reach(Vec3::X),
            self.reach(Vec3::Y),
            self.reach(Vec3::Z),
        );
        (self.center - reach, self.center + reach)
    }

    /// Whether the box has a face square to `direction`.
    fn square_to(&self, direction: Vec3) -> bool {
        self.axes
            .iter()
            .any(|axis| axis.dot(direction).abs() > SQUARE)
    }

    /// The box stretched `length` studs both ways along its axis nearest
    /// `direction`: everything it could meet moving that way.
    fn swept(self, direction: Vec3, length: f32) -> Obb {
        let nearest = (0..3)
            .max_by(|&a, &b| {
                let a = self.axes[a].dot(direction).abs();
                let b = self.axes[b].dot(direction).abs();
                a.total_cmp(&b)
            })
            .expect("three axes");
        let mut half = self.half;
        half[nearest] += length;
        Obb { half, ..self }
    }
}

/// Whether `a` and `b` overlap once each is grown by `margin` studs on
/// every side (shrunk for a negative margin), by the separating axis test:
/// two boxes are apart if, along one of their 15 axes (each box's own
/// three, and the 9 crossings of one's with the other's), their shadows
/// don't meet.
pub fn overlap(a: &Obb, b: &Obb, margin: f32) -> bool {
    let between = b.center - a.center;
    let mut axes: Vec<Vec3> = a.axes.iter().chain(&b.axes).copied().collect();
    for x in a.axes {
        for y in b.axes {
            let cross = x.cross(y);
            // Parallel edges cross at nothing; their axes are tried above.
            if cross.length_squared() > 1e-6 {
                axes.push(cross.normalize());
            }
        }
    }
    axes.iter()
        .all(|&axis| between.dot(axis).abs() <= a.reach(axis) + b.reach(axis) + 2.0 * margin)
}

/// Whether `a` and `b` touch (are within [`TOUCHING`] of each other)
/// without going into each other.
pub fn touching(a: &Obb, b: &Obb) -> bool {
    overlap(a, b, TOUCHING / 2.0) && !overlap(a, b, -TOUCHING / 2.0)
}

/// Another part, as something a dragged part meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub id: InstanceId,
    pub obb: Obb,
    /// Its `CanCollide`, so Collisions lets parts pass through it if false.
    pub solid: bool,
    /// Seen at all (not fully transparent), so parts can snap to it.
    pub visible: bool,
}

/// The boxes of every part in the Workspace except `moving` (and Terrain,
/// which is ground, not a box).
pub fn obstacles(world: &World, moving: &[InstanceId]) -> Vec<Obstacle> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|id| !moving.contains(id))
        .filter_map(|id| {
            let instance = world.get(id)?;
            if instance.class() == ClassName::Terrain || !instance.class().is_base_part() {
                return None;
            }
            let pose = Pose::of(instance)?;
            let flag = |name| instance.property(name).and_then(Value::as_bool);
            let transparency = instance
                .property("Transparency")
                .and_then(Value::as_number)
                .unwrap_or(0.0);
            Some(Obstacle {
                id,
                obb: Obb::of(pose),
                solid: flag("CanCollide") != Some(false),
                visible: transparency < 0.99,
            })
        })
        .collect()
}

/// How far `moving` can go along `direction` (one stud long), up to
/// `wanted` studs (either way), before one of them goes into a solid
/// obstacle. Obstacles they're already inside are let through, so a stuck
/// part can always be moved out.
pub fn limit_travel(moving: &[Obb], direction: Vec3, wanted: f32, obstacles: &[Obstacle]) -> f32 {
    if wanted == 0.0 || moving.is_empty() {
        return wanted;
    }
    let near = in_the_way(moving, direction * wanted, obstacles);
    let blocked = |t: f32| {
        moving.iter().any(|obb| {
            let moved = obb.moved(direction * t);
            near.iter()
                .any(|other| overlap(&moved, other, -PENETRATION))
        })
    };
    let steps = (wanted.abs() / COLLISION_STEP).ceil().max(1.0) as usize;
    let mut free = 0.0;
    for step in 1..=steps {
        let t = wanted * step as f32 / steps as f32;
        if blocked(t) {
            let mut hit = t;
            for _ in 0..16 {
                let middle = (free + hit) / 2.0;
                if blocked(middle) {
                    hit = middle;
                } else {
                    free = middle;
                }
            }
            // That's where it's [`PENETRATION`] deep on each side; back
            // off by as much, so it stops flush.
            let back = 2.0 * PENETRATION;
            return if free.abs() > back {
                free - back * free.signum()
            } else {
                0.0
            };
        }
        free = t;
    }
    wanted
}

/// Whether `moving` would go into a solid obstacle (one they aren't
/// already inside) if each moved to `to`, at the same index.
pub fn collides(moving: &[Obb], to: &[Obb], obstacles: &[Obstacle]) -> bool {
    moving.iter().zip(to).any(|(from, to)| {
        obstacles.iter().any(|other| {
            other.solid
                && overlap(to, &other.obb, -PENETRATION)
                && !overlap(from, &other.obb, -PENETRATION)
        })
    })
}

/// The solid obstacles near `moving`'s path (moved by `travel`) that
/// they aren't already inside.
fn in_the_way(moving: &[Obb], travel: Vec3, obstacles: &[Obstacle]) -> Vec<Obb> {
    let (mut low, mut high) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for obb in moving {
        for end in [*obb, obb.moved(travel)] {
            let (a, b) = end.bounds();
            low = low.min(a);
            high = high.max(b);
        }
    }
    obstacles
        .iter()
        .filter(|other| other.solid)
        .filter(|other| {
            let (a, b) = other.obb.bounds();
            a.cmple(high).all() && b.cmpge(low).all()
        })
        .filter(|other| {
            !moving
                .iter()
                .any(|obb| overlap(obb, &other.obb, -PENETRATION))
        })
        .map(|other| other.obb)
        .collect()
}

/// Where the faces square to `direction` of the visible obstacles in line
/// with `obb` along it are, as distances along `direction`: the places a
/// face of `obb` could go flush with or against them.
fn face_planes(obb: &Obb, direction: Vec3, obstacles: &[Obstacle]) -> Vec<f32> {
    if !obb.square_to(direction) {
        return Vec::new();
    }
    // Only parts it would meet going that way (or nearly), so a part
    // doesn't jump into line with something across the map.
    let lane = obb.swept(direction, 1e4);
    obstacles
        .iter()
        .filter(|other| other.visible && other.obb.square_to(direction))
        .filter(|other| overlap(&lane, &other.obb, 0.5))
        .flat_map(|other| {
            let (start, end) = other.obb.extent(direction);
            [start, end]
        })
        .collect()
}

/// For `moving` going `raw` studs along `direction`, the move nearest it
/// (within `reach` studs) that leaves a face of one of them flush against
/// or in line with a face of a part in its way, or `None` if there's none
/// that close.
pub fn flush_moves(
    moving: &[Obb],
    direction: Vec3,
    raw: f32,
    reach: f32,
    obstacles: &[Obstacle],
) -> Option<f32> {
    moving
        .iter()
        .flat_map(|obb| {
            let (start, end) = obb.extent(direction);
            face_planes(obb, direction, obstacles)
                .into_iter()
                .flat_map(move |plane| [plane - end, plane - start])
        })
        .filter(|t| (t - raw).abs() <= reach)
        .min_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()))
}

/// For the face of `obb` facing `outward` pulled `raw` studs out (a
/// resize), the pull nearest it (within `reach`) that leaves the face
/// flush with a face of a part in its way, or `None`.
pub fn flush_faces(
    obb: &Obb,
    outward: Vec3,
    raw: f32,
    reach: f32,
    obstacles: &[Obstacle],
) -> Option<f32> {
    let (_, face) = obb.extent(outward);
    face_planes(obb, outward, obstacles)
        .into_iter()
        .map(|plane| plane - face)
        .filter(|t| (t - raw).abs() <= reach)
        .min_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()))
}

/// The parts `moved` are left touching (and not inside), as `(other,
/// moved)` pairs, for Join Surfaces. Parts already welded together aren't
/// listed again.
pub fn joins(world: &World, moved: &[InstanceId]) -> Vec<(InstanceId, InstanceId)> {
    let others = obstacles(world, moved);
    let mut pairs = Vec::new();
    for &part in moved {
        let Some(obb) = world.get(part).and_then(Pose::of).map(Obb::of) else {
            continue;
        };
        for other in &others {
            if touching(&obb, &other.obb) && !welded(world, part, other.id) {
                pairs.push((other.id, part));
            }
        }
    }
    pairs
}

/// Whether a WeldConstraint inside either part already holds both.
fn welded(world: &World, a: InstanceId, b: InstanceId) -> bool {
    [a, b].iter().any(|&part| {
        world.get(part).is_some_and(|instance| {
            instance.children().iter().any(|&child| {
                world.get(child).is_some_and(|weld| {
                    let held = |name| match weld.property(name) {
                        Some(Value::Instance(Some(id))) => Some(*id),
                        _ => None,
                    };
                    weld.class() == ClassName::WeldConstraint
                        && [held("Part0"), held("Part1")]
                            .iter()
                            .all(|id| *id == Some(a) || *id == Some(b))
                })
            })
        })
    })
}

/// A box `size` studs big at `center`, turned `rotation`.
#[cfg(any(test, feature = "test-support"))]
pub fn obb(center: Vec3, size: Vec3, rotation: Quat) -> Obb {
    Obb::of(Pose {
        position: center,
        rotation,
        size,
    })
}

/// A visible, solid obstacle of `obb`, standing in for a part.
#[cfg(any(test, feature = "test-support"))]
pub fn wall(obb: Obb) -> Obstacle {
    Obstacle {
        // Any ID: the tests only look at the box.
        id: World::new().workspace(),
        obb,
        solid: true,
        visible: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(center: Vec3) -> Obb {
        obb(center, Vec3::splat(2.0), Quat::IDENTITY)
    }

    #[test]
    fn boxes_overlap_touch_or_stand_apart() {
        let a = cube(Vec3::ZERO);
        assert!(overlap(&a, &cube(Vec3::new(1.5, 0.0, 0.0)), -PENETRATION));
        // Flush: touching, not overlapping.
        let flush = cube(Vec3::new(2.0, 0.0, 0.0));
        assert!(!overlap(&a, &flush, -PENETRATION));
        assert!(touching(&a, &flush));
        let apart = cube(Vec3::new(2.5, 0.0, 0.0));
        assert!(!overlap(&a, &apart, 0.0) && !touching(&a, &apart));
        // A cube turned 45° reaches √2 (1.414) out along X with its edge:
        // at 2.3 that edge is inside the first cube, at 2.5 it's clear.
        let turned = |x| {
            obb(
                Vec3::new(x, 0.0, 0.0),
                Vec3::splat(2.0),
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            )
        };
        assert!(overlap(&a, &turned(2.3), 0.0));
        assert!(!overlap(&a, &turned(2.5), 0.0));
    }

    #[test]
    fn a_move_stops_flush_against_a_solid_part_and_passes_a_ghost() {
        let block = cube(Vec3::ZERO);
        let wall_at = |x| {
            wall(obb(
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(2.0, 10.0, 10.0),
                Quat::IDENTITY,
            ))
        };
        let free = limit_travel(&[block], Vec3::X, 10.0, &[wall_at(6.0)]);
        // The wall's near face is at 5, the block's far face starts at 1.
        assert!((free - 4.0).abs() < 1e-3, "{free}");
        // Backward is clear.
        assert_eq!(
            limit_travel(&[block], Vec3::X, -10.0, &[wall_at(6.0)]),
            -10.0
        );
        let ghost = Obstacle {
            solid: false,
            ..wall_at(6.0)
        };
        assert_eq!(limit_travel(&[block], Vec3::X, 10.0, &[ghost]), 10.0);
        // A wall it's already in doesn't hold it.
        assert_eq!(limit_travel(&[block], Vec3::X, 10.0, &[wall_at(0.5)]), 10.0);
    }

    #[test]
    fn a_fast_move_does_not_pass_through_a_thin_part() {
        let block = cube(Vec3::ZERO);
        let sheet = wall(obb(
            Vec3::new(20.0, 0.0, 0.0),
            Vec3::new(0.05, 10.0, 10.0),
            Quat::IDENTITY,
        ));
        let free = limit_travel(&[block], Vec3::X, 100.0, &[sheet]);
        assert!(free < 19.0 && free > 18.9, "{free}");
    }

    #[test]
    fn a_move_snaps_flush_against_or_in_line_with_a_part_in_its_way() {
        let block = cube(Vec3::ZERO);
        let next = wall(cube(Vec3::new(6.0, 0.0, 0.0)));
        // Its far face (1) meets the other's near face (5) after 4 studs.
        assert_eq!(flush_moves(&[block], Vec3::X, 3.7, 0.5, &[next]), Some(4.0));
        // Its near face (-1) lines up with the other's (5) after 6.
        assert_eq!(flush_moves(&[block], Vec3::X, 5.8, 0.5, &[next]), Some(6.0));
        // Too far from any face to be pulled.
        assert_eq!(flush_moves(&[block], Vec3::X, 2.0, 0.5, &[next]), None);
        // Nothing in line: a part off to the side doesn't pull.
        let aside = wall(cube(Vec3::new(6.0, 0.0, 20.0)));
        assert_eq!(flush_moves(&[block], Vec3::X, 3.7, 0.5, &[aside]), None);
    }

    #[test]
    fn a_resized_face_snaps_flush_with_a_face_in_its_way() {
        let block = cube(Vec3::ZERO);
        let next = wall(cube(Vec3::new(6.0, 0.0, 0.0)));
        // The +X face (1) pulled to the other's near face (5) is 4 studs.
        assert_eq!(flush_faces(&block, Vec3::X, 4.2, 0.5, &[next]), Some(4.0));
        assert_eq!(flush_faces(&block, Vec3::X, 1.0, 0.5, &[next]), None);
    }
}
