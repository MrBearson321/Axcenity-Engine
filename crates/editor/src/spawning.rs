//! Where Insert puts a new part: in front of the camera, resting on what
//! the middle of the view looks at, and not inside anything already there.
//!
//! 1. A ray from the camera straight ahead finds the surface the view is
//!    centred on (a part, the baseplate or terrain), up to [`REACH`] studs
//!    away. The new part rests on it, on the side facing the camera.
//! 2. Looking at the sky, at a wall or at something too far away, the part
//!    goes [`COMFORTABLE`] studs ahead instead, and drops onto whatever is
//!    under that point if the landing spot is still in view, so it doesn't
//!    fall the moment Play starts.
//! 3. If it would be inside another solid part there, it's lifted along the
//!    surface's facing until it's clear (on top of the pile, say).
//!
//! Duplicates don't come through here: a copy stays where its original is
//! (`app/tree_ops.rs`, `duplicate`).

use axcenity_core::{InstanceId, Value, Vector3, World};
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_scene::{CameraView, Ray};

use crate::gizmo::Pose;
use crate::snapping::{self, Obb};

/// How far ahead a surface can be and still be built on, in studs.
pub const REACH: f32 = 120.0;
/// How far ahead a part goes with nothing to rest on, in studs: close
/// enough to see and grab, far enough not to fill the view.
pub const COMFORTABLE: f32 = 16.0;
/// How far below that point it can drop onto something, in studs.
const DROP_REACH: f32 = 48.0;
/// A surface facing up at least this much (cos of about 45°) is ground a
/// part can rest on; anything steeper is a wall.
const GROUND: f32 = 0.7;
/// How far out from a wall the spot steps before dropping to the floor.
const WALL_GAP: f32 = 2.0;
/// How many times a part is lifted clear of others before it stays put.
const MAX_LIFTS: usize = 16;

/// Where a new thing goes: the point on a surface (or in the air) and the
/// way that surface faces. A part's middle goes half its size out along
/// `normal` from `point`, so it sits on the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub point: Vec3,
    pub normal: Vec3,
    /// Nothing is under it: the part hangs in the air where the camera
    /// looks, centred on `point`.
    pub floating: bool,
}

/// The spot in front of a camera at `eye` looking along `forward` (one
/// stud long). `view` is the camera's view, to keep a dropped spot on the
/// screen; without one, any drop is taken.
pub fn spot(world: &World, eye: Vec3, forward: Vec3, view: Option<&CameraView>) -> Spot {
    let ray = Ray {
        origin: eye,
        direction: forward.normalize_or(Vec3::NEG_Z),
    };
    if let Some(hit) = axcenity_scene::pick_hit(world, ray, None)
        && hit.distance <= REACH
        && hit.normal.y >= GROUND
    {
        return Spot {
            point: ray.at(hit.distance),
            normal: hit.normal,
            floating: false,
        };
    }
    // At a wall, in front of it; at the sky or far away, a comfortable
    // way ahead. Then onto whatever is below, if that's still in view.
    let ahead = match axcenity_scene::pick_hit(world, ray, None) {
        // A step out from the wall (or ceiling), so the part isn't in it.
        Some(hit) if hit.distance <= REACH => ray.at(hit.distance) + hit.normal * WALL_GAP,
        _ => ray.at(COMFORTABLE),
    };
    let down = Ray {
        origin: ahead,
        direction: Vec3::NEG_Y,
    };
    if let Some(hit) = axcenity_scene::pick_hit(world, down, None)
        && hit.distance <= DROP_REACH
        && hit.normal.y >= GROUND
    {
        let point = down.at(hit.distance);
        if view.is_none_or(|view| on_screen(view, point)) {
            return Spot {
                point,
                normal: hit.normal,
                floating: false,
            };
        }
    }
    Spot {
        point: ahead,
        normal: Vec3::Y,
        floating: true,
    }
}

/// Whether `point` is in front of the camera and inside its view.
fn on_screen(view: &CameraView, point: Vec3) -> bool {
    view.project_point(point)
        .is_some_and(|at| at.abs().cmple(Vec2::splat(0.95)).all())
}

/// Where the middle of a part sized `size` (not turned) goes at `spot`:
/// resting on the surface, with the two directions along it on the grid of
/// `snap` (a function rounding studs) when the surface is square to an axis.
pub fn center(spot: Spot, size: Vec3, snap: impl Fn(f32) -> f32) -> Vec3 {
    if spot.floating {
        return spot.point.map(&snap);
    }
    let half = size.abs() / 2.0;
    let normal = spot.normal;
    let lift = half.x * normal.x.abs() + half.y * normal.y.abs() + half.z * normal.z.abs();
    let center = spot.point + normal * lift;
    // Only along a flat (or upright) surface does a grid line up.
    let axis = [Vec3::X, Vec3::Y, Vec3::Z]
        .into_iter()
        .position(|axis| normal.dot(axis).abs() > 0.999);
    match axis {
        Some(across) => {
            let mut snapped = center.map(&snap);
            snapped[across] = center[across];
            snapped
        }
        None => center,
    }
}

/// Moves the new part `id` out of any solid part it's inside, by lifting it
/// along `normal` onto the top of what it's in, as many times as that
/// takes (it may land in another), up to [`MAX_LIFTS`]. Non-solid parts
/// (CanCollide off) don't count, and nor does `id` itself.
pub fn lift_clear(world: &mut World, id: InstanceId, normal: Vec3) {
    let Some(pose) = world.get(id).and_then(Pose::of) else {
        return;
    };
    let solid = |id: &InstanceId| {
        world
            .get(*id)
            .and_then(|instance| instance.property("CanCollide"))
            .and_then(Value::as_bool)
            != Some(false)
    };
    if !solid(&id) {
        return;
    }
    let obstacles: Vec<_> = snapping::obstacles(world, &[id])
        .into_iter()
        .filter(|obstacle| obstacle.solid)
        .collect();
    let mut obb = Obb::of(pose);
    for _ in 0..MAX_LIFTS {
        let lift = obstacles
            .iter()
            .filter(|other| snapping::overlap(&obb, &other.obb, -snapping::PENETRATION))
            .map(|other| {
                // From the part's bottom (along `normal`) to the top of the other.
                let (bottom, _) = obb.extent(normal);
                let (_, top) = other.obb.extent(normal);
                top - bottom
            })
            .fold(0.0_f32, f32::max);
        if lift <= 0.0 {
            break;
        }
        obb = obb.moved(normal * lift);
    }
    let at = obb.center;
    let _ = world.set_property(
        id,
        "Position",
        Value::Vector3(Vector3::new(at.x, at.y, at.z)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, PartType};

    fn part(world: &mut World, position: Vec3, size: Vec3) -> InstanceId {
        let id = world.create(ClassName::Part).unwrap();
        let v = |v: Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
        world.set_property(id, "Position", v(position)).unwrap();
        world.set_property(id, "Size", v(size)).unwrap();
        world
            .set_property(id, "Anchored", Value::Bool(true))
            .unwrap();
        let workspace = world.workspace();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    }

    /// A world with a baseplate whose top is at Y = 0.
    fn baseplate() -> World {
        let mut world = World::new();
        part(
            &mut world,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(512.0, 1.0, 512.0),
        );
        world
    }

    fn whole(value: f32) -> f32 {
        value.round()
    }

    #[test]
    fn a_new_part_rests_on_the_baseplate_where_the_camera_looks() {
        let world = baseplate();
        let eye = Vec3::new(0.0, 10.0, 20.0);
        let looking = (Vec3::new(3.0, 0.0, 0.0) - eye).normalize();
        let spot = spot(&world, eye, looking, None);
        assert!(!spot.floating);
        assert!(
            (spot.point - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-3,
            "{spot:?}"
        );
        let center = center(spot, Vec3::new(4.0, 1.0, 2.0), whole);
        // Its bottom on the baseplate's top, not halfway into it.
        assert!((center.y - 0.5).abs() < 1e-4, "{center}");
        assert_eq!((center.x, center.z), (3.0, 0.0));
    }

    #[test]
    fn looking_at_the_sky_puts_it_ahead_and_drops_it_onto_the_ground() {
        let world = baseplate();
        let eye = Vec3::new(0.0, 5.0, 0.0);
        // A little above the horizon: nothing ahead but sky.
        let up = Vec3::new(0.0, 0.2, -1.0).normalize();
        let spot = spot(&world, eye, up, None);
        assert!(!spot.floating, "{spot:?}");
        assert!(spot.point.y.abs() < 1e-4, "on the ground: {spot:?}");
        assert!(spot.point.z < -10.0, "ahead of the camera: {spot:?}");
    }

    #[test]
    fn with_nothing_below_it_hangs_where_the_camera_looks() {
        let world = World::new();
        let eye = Vec3::new(0.0, 5.0, 0.0);
        let spot = spot(&world, eye, Vec3::NEG_Z, None);
        assert!(spot.floating);
        assert!((spot.point - Vec3::new(0.0, 5.0, -COMFORTABLE)).length() < 1e-4);
        // Centred there, not lifted.
        assert_eq!(
            center(spot, Vec3::ONE * 2.0, whole),
            Vec3::new(0.0, 5.0, -16.0)
        );
    }

    #[test]
    fn a_drop_out_of_view_is_not_taken() {
        let world = baseplate();
        // Looking almost straight up: the ground below the spot is off screen.
        let eye = Vec3::new(0.0, 40.0, 0.0);
        let up = Vec3::new(0.0, 1.0, -0.05).normalize();
        let camera = axcenity_render::Camera {
            position: eye,
            look_at: eye + up,
            fov_y_degrees: 50.0,
        };
        let view = CameraView::new(camera, 16.0 / 9.0);
        let spot = spot(&world, eye, up, Some(&view));
        assert!(spot.floating, "{spot:?}");
    }

    #[test]
    fn a_far_surface_is_too_far_to_build_on() {
        let world = baseplate();
        let eye = Vec3::new(0.0, 100.0, 0.0);
        // Hits the baseplate 200 studs away.
        let ahead = Vec3::new(0.0, -100.0, -173.2).normalize();
        let spot = spot(&world, eye, ahead, None);
        let distance = (spot.point - eye).length();
        assert!(distance <= REACH, "{distance} studs away: {spot:?}");
    }

    #[test]
    fn at_a_wall_it_goes_on_the_floor_in_front_of_it() {
        let mut world = baseplate();
        // A wall 10 studs ahead, 8 high.
        part(
            &mut world,
            Vec3::new(0.0, 4.0, -10.0),
            Vec3::new(16.0, 8.0, 1.0),
        );
        let eye = Vec3::new(0.0, 3.0, 0.0);
        let spot = spot(&world, eye, Vec3::NEG_Z, None);
        assert!(!spot.floating, "{spot:?}");
        assert_eq!(spot.normal, Vec3::Y);
        assert!(spot.point.z > -9.5, "in front of the wall: {spot:?}");
    }

    #[test]
    fn a_new_part_is_lifted_out_of_parts_already_there() {
        let mut world = baseplate();
        // A crate where the part will land, and another on top of it.
        part(&mut world, Vec3::new(0.0, 1.0, 0.0), Vec3::splat(2.0));
        part(&mut world, Vec3::new(0.0, 3.0, 0.0), Vec3::splat(2.0));
        let new = part(
            &mut world,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(4.0, 1.0, 2.0),
        );
        lift_clear(&mut world, new, Vec3::Y);
        let y = world
            .get(new)
            .unwrap()
            .property("Position")
            .unwrap()
            .as_vector3()
            .unwrap()
            .y;
        // On top of both crates, which reach Y = 4.
        assert!((y - 4.5).abs() < 1e-3, "{y}");
    }

    #[test]
    fn parts_that_dont_collide_are_not_in_the_way() {
        let mut world = baseplate();
        let ghost = part(&mut world, Vec3::new(0.0, 1.0, 0.0), Vec3::splat(2.0));
        world
            .set_property(ghost, "CanCollide", Value::Bool(false))
            .unwrap();
        let new = part(
            &mut world,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(4.0, 1.0, 2.0),
        );
        lift_clear(&mut world, new, Vec3::Y);
        let shape = world.get(new).unwrap().property("Shape").cloned();
        assert_eq!(shape, Some(Value::PartType(PartType::Block)));
        let y = world
            .get(new)
            .unwrap()
            .property("Position")
            .unwrap()
            .as_vector3()
            .unwrap()
            .y;
        assert!((y - 0.5).abs() < 1e-4, "{y}");
    }
}
