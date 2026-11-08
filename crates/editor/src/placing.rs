//! Dragging a part by its body to place it, like Studio: the part rests on
//! whatever surface is under the mouse (on top of the floor, against the
//! side of a wall, on a wedge's slope), without turning.
//!
//! The point where the part was grabbed stays under the mouse (sideways
//! along the surface), so the part doesn't jump when the drag starts. Over
//! empty sky the part stays where it last was.
//!
//! With "Snap move" on, the part's position lands on the world grid along
//! the surface, so parts line up. Its height off the surface always comes
//! from the surface. Surfaces that aren't flat or upright (slopes, balls)
//! have no grid, so there it follows the mouse exactly.
//!
//! With Surfaces on, a part near the edge of a face lines up with it: its
//! own faces snap flush with (or in line with) those of the parts around
//! it, along the surface, from up to `reach` studs away. With Collisions
//! on, it won't go where it would be inside a solid part; it waits at the
//! last place it fit (see `snapping.rs`).

use axcenity_core::{InstanceId, Value, Vector3, World};
use axcenity_render::glam::Vec3;
use axcenity_scene::Ray;

use crate::gizmo::{Pose, Snap};
use crate::snapping::points::{self, DragReport, Freedom, Sites};
use crate::snapping::{self, Obb, Obstacle};

/// How close a surface's facing must be to an axis to count as flat or
/// upright for snapping (about 2.5°).
/// How far around a move markers show, as a multiple of the snap reach.
const MARKER_REACH: f32 = 5.0;
const AXIS_ALIGNED: f32 = 0.999;

/// A part being dragged by its body.
#[derive(Clone, Debug)]
pub struct PlaceDrag {
    pub part: InstanceId,
    /// Where the part was when the drag started.
    pub start: Vec3,
    /// From the point where the part was grabbed to its center.
    grab_offset: Vec3,
    /// Other parts being dragged along with it, which it can't rest on.
    pub skip: Vec<InstanceId>,
    /// The parts it can line up with or bump into (see `snapping::obstacles`).
    pub obstacles: Vec<Obstacle>,
    /// How far (studs) it's pulled to line up with a face.
    pub reach: f32,
    /// Where it last fit, with Collisions on.
    last_free: Option<Vec3>,
    /// The Attachments it can click to.
    pub sites: Box<Sites>,
    /// How far it has come and what it can click to, for drawing.
    pub report: Option<Box<DragReport>>,
}

impl PlaceDrag {
    /// Starts dragging `part`, grabbed at the world point `grabbed`.
    pub fn start(world: &World, part: InstanceId, grabbed: Vec3) -> Option<PlaceDrag> {
        let position = world.get(part)?.property("Position")?.as_vector3()?;
        let start = Vec3::new(position.x, position.y, position.z);
        Some(PlaceDrag {
            part,
            start,
            grab_offset: start - grabbed,
            skip: Vec::new(),
            obstacles: Vec::new(),
            reach: 0.0,
            last_free: None,
            sites: Box::default(),
            report: None,
        })
    }

    /// The part's new Position for the mouse pointing along `ray`, or `None`
    /// if there's no surface under the mouse (the part then stays put).
    pub fn update(&mut self, world: &World, ray: Ray, snap: &Snap) -> Option<Value> {
        // Skip the part itself, so it looks for what's behind it.
        let mut skip = self.skip.clone();
        skip.push(self.part);
        let hit = axcenity_scene::pick_hit_skipping(world, ray, &skip)?;
        let normal = hit.normal;
        // Only the sideways part of the grab offset: across the surface,
        // not off it.
        let sideways = self.grab_offset - normal * self.grab_offset.dot(normal);
        let mut position = ray.at(hit.distance) + sideways;
        if snap.move_on && snap.move_step > 0.0 {
            snap_along_surface(&mut position, normal, snap.move_step);
        }
        // Lift the part off the surface so its box just touches it.
        let lift = axcenity_scene::part_half_extent(world, self.part, normal)?;
        let mut position = position + normal * lift;
        let pose = world.get(self.part).and_then(Pose::of);
        if snap.surfaces
            && let Some(pose) = pose
        {
            // Along the two ways across a flat or upright surface.
            let off = (0..3).find(|&i| normal[i].abs() > AXIS_ALIGNED);
            for i in (0..3).filter(|&i| Some(i) != off && off.is_some()) {
                let along = Vec3::AXES[i];
                let obb = Obb::of(Pose { position, ..pose });
                if let Some(shift) =
                    snapping::flush_moves(&[obb], along, 0.0, self.reach, &self.obstacles)
                {
                    position += along * shift;
                }
            }
        }
        let mut link = None;
        if snap.points
            && let Some(pose) = pose
        {
            let by = position - self.start;
            let at = [Obb::of(Pose { position, ..pose })];
            let own = points::own_points(&at, &self.sites.own, by);
            let around = points::nearby(&at, &self.obstacles, &self.sites.others, self.reach);
            if let Some(found) = points::nearest(&own, &around, Freedom::Across(normal), self.reach)
            {
                position += found.shift;
                link = Some(found.to);
            }
        }
        if snap.collisions
            && let Some(pose) = pose
        {
            let from = Obb::of(Pose {
                position: self.start,
                ..pose
            });
            let to = Obb::of(Pose { position, ..pose });
            if snapping::collides(&[from], &[to], &self.obstacles) {
                position = self.last_free?;
                link = None;
            } else {
                self.last_free = Some(position);
            }
        }
        self.note_move(position, pose, link, snap);
        Some(Value::Vector3(Vector3::new(
            position.x, position.y, position.z,
        )))
    }
}

impl PlaceDrag {
    /// Notes how far it's come, with the markers to show.
    fn note_move(&mut self, position: Vec3, pose: Option<Pose>, link: Option<Vec3>, snap: &Snap) {
        let at = pose.map(|pose| Obb::of(Pose { position, ..pose }));
        let targets = match at.filter(|_| snap.points) {
            Some(at) => {
                let radius = self.reach * MARKER_REACH;
                points::nearby(&[at], &self.obstacles, &self.sites.others, radius)
            }
            None => Vec::new(),
        };
        self.report = Some(Box::new(DragReport::new(
            self.start,
            position - self.start,
            at.as_ref(),
            targets,
            link,
        )));
    }
}

/// Rounds the two coordinates that run along a flat or upright surface to
/// the grid. The third (off the surface) is left alone. Nothing changes on
/// a slope.
fn snap_along_surface(position: &mut Vec3, normal: Vec3, step: f32) {
    let Some(off_surface) = (0..3).find(|&i| normal[i].abs() > AXIS_ALIGNED) else {
        return;
    };
    for i in (0..3).filter(|&i| i != off_surface) {
        position[i] = (position[i] / step).round() * step;
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    fn vector(v: Vec3) -> Value {
        Value::Vector3(Vector3::new(v.x, v.y, v.z))
    }

    fn part(world: &mut World, position: Vec3, size: Vec3) -> InstanceId {
        let id = world.create(ClassName::Part).unwrap();
        world.set_parent(id, Some(world.workspace())).unwrap();
        world
            .set_property(id, "Position", vector(position))
            .unwrap();
        world.set_property(id, "Size", vector(size)).unwrap();
        id
    }

    /// A 40×1×40 floor with its top at y = 0, a 2-stud-thick wall standing
    /// on it at x = 10, and a 4×1×2 block on the floor at the origin.
    fn scene() -> (World, InstanceId) {
        let mut world = World::new();
        part(
            &mut world,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(40.0, 1.0, 40.0),
        );
        part(
            &mut world,
            Vec3::new(10.0, 5.0, 0.0),
            Vec3::new(2.0, 10.0, 20.0),
        );
        let block = part(
            &mut world,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(4.0, 1.0, 2.0),
        );
        (world, block)
    }

    fn straight_down(x: f32, z: f32) -> Ray {
        Ray {
            origin: Vec3::new(x, 50.0, z),
            direction: Vec3::NEG_Y,
        }
    }

    const NO_SNAP: Snap = Snap {
        move_on: false,
        move_step: 1.0,
        rotate_on: false,
        rotate_step: 15.0,
        surfaces: false,
        collisions: false,
        join: false,
        points: false,
    };

    #[test]
    fn with_snap_points_a_body_drag_clicks_a_corner_to_a_nearby_corner_and_stays_on_the_floor() {
        let (mut world, block) = scene();
        let post = part(
            &mut world,
            Vec3::new(10.0, 2.0, 10.0),
            Vec3::new(4.0, 4.0, 4.0),
        );
        let grabbed = Vec3::new(0.0, 0.0, 0.0);
        let mut drag = PlaceDrag::start(&world, block, grabbed).unwrap();
        drag.obstacles = snapping::obstacles(&world, &[block]);
        drag.reach = 0.5;
        let snap = Snap {
            points: true,
            ..NO_SNAP
        };
        let size = world
            .get(block)
            .unwrap()
            .property("Size")
            .unwrap()
            .as_vector3()
            .unwrap();
        // Near where the block's corner meets the post's corner at (8, 8).
        let (x, z) = (8.0 - size.x / 2.0 + 0.2, 8.0 - size.z / 2.0 - 0.1);
        let value = drag.update(&world, straight_down(x, z), &snap);
        let at = value.unwrap().as_vector3().unwrap();
        assert!((at.x - (8.0 - size.x / 2.0)).abs() < 0.001, "x {}", at.x);
        assert!((at.z - (8.0 - size.z / 2.0)).abs() < 0.001, "z {}", at.z);
        assert!((at.y - size.y / 2.0).abs() < 0.001, "still on the floor");
        let report = drag.report.as_ref().unwrap();
        assert!(report.link.is_some());
        assert!(
            report
                .targets
                .iter()
                .any(|p| p.at == Vec3::new(8.0, 0.0, 8.0))
        );
        assert!(report.distance() > 1.0);
        let _ = post;
    }

    fn assert_at(value: Option<Value>, expected: Vec3) {
        let v = value
            .expect("a surface under the mouse")
            .as_vector3()
            .unwrap();
        let v = Vec3::new(v.x, v.y, v.z);
        assert!(v.abs_diff_eq(expected, 1e-4), "{v} is not {expected}");
    }

    #[test]
    fn dragging_over_the_floor_rests_the_part_on_top_of_it() {
        let (world, block) = scene();
        // Grabbed in the middle of its top.
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        let position = drag.update(&world, straight_down(-6.3, 4.2), &NO_SNAP);
        assert_at(position, Vec3::new(-6.3, 0.5, 4.2));
    }

    #[test]
    fn the_point_grabbed_stays_under_the_mouse() {
        let (world, block) = scene();
        // Grabbed near its right end, 1.5 studs right of its center.
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(1.5, 1.0, 0.0)).unwrap();
        let position = drag.update(&world, straight_down(5.0, 0.0), &NO_SNAP);
        assert_at(position, Vec3::new(3.5, 0.5, 0.0));
    }

    #[test]
    fn dragging_onto_another_part_stacks_it_and_against_a_wall_puts_it_beside() {
        let (world, block) = scene();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        // On top of the 10-stud wall.
        let position = drag.update(&world, straight_down(10.0, 3.0), &NO_SNAP);
        assert_at(position, Vec3::new(10.0, 10.5, 3.0));
        // Pointing at the wall's side from the left: the block's right end
        // touches the wall (at x = 9), 2 studs from its center. It was
        // grabbed half a stud above its center, which now runs along the
        // wall, so the center is half a stud below the mouse.
        let at_wall = Ray {
            origin: Vec3::new(-20.0, 4.0, 1.0),
            direction: Vec3::X,
        };
        let position = drag.update(&world, at_wall, &NO_SNAP);
        assert_at(position, Vec3::new(7.0, 3.5, 1.0));
    }

    #[test]
    fn over_empty_sky_there_is_nowhere_to_put_it() {
        let (world, block) = scene();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        let up = Ray {
            origin: Vec3::new(0.0, 5.0, 0.0),
            direction: Vec3::Y,
        };
        assert_eq!(drag.update(&world, up, &NO_SNAP), None);
    }

    #[test]
    fn snapping_puts_it_on_the_world_grid_along_the_surface() {
        let (world, block) = scene();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        // Across the floor: X and Z snap, the height comes from the floor.
        let position = drag.update(&world, straight_down(-6.3, 4.2), &Snap::default());
        assert_at(position, Vec3::new(-6.0, 0.5, 4.0));
        let half = Snap {
            move_step: 0.5,
            ..Snap::default()
        };
        let position = drag.update(&world, straight_down(-6.3, 4.2), &half);
        assert_at(position, Vec3::new(-6.5, 0.5, 4.0));
        // Against the wall: Y and Z snap, X comes from the wall.
        let at_wall = Ray {
            origin: Vec3::new(-20.0, 4.3, 1.4),
            direction: Vec3::X,
        };
        let position = drag.update(&world, at_wall, &Snap::default());
        assert_at(position, Vec3::new(7.0, 4.0, 1.0));
    }

    #[test]
    fn a_turned_part_rests_by_its_turned_size() {
        let (mut world, block) = scene();
        // Stood on its end: 4 studs tall.
        world
            .set_property(block, "Orientation", vector(Vec3::new(0.0, 0.0, 90.0)))
            .unwrap();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 2.5, 0.0)).unwrap();
        let position = drag.update(&world, straight_down(-3.0, -3.0), &NO_SNAP);
        assert_at(position, Vec3::new(-3.0, 2.0, -3.0));
    }

    /// The floor and the wall, as the viewport gives them when the block's
    /// drag starts.
    fn around_block(world: &World, block: InstanceId) -> Vec<Obstacle> {
        snapping::obstacles(world, &[block])
    }

    #[test]
    fn with_surfaces_it_lines_up_flush_with_a_wall_it_comes_near() {
        let (world, block) = scene();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        drag.obstacles = around_block(&world, block);
        drag.reach = 0.5;
        let surfaces = Snap {
            surfaces: true,
            ..NO_SNAP
        };
        // Its right face (8.7) is 0.3 from the wall's (9): pulled flush.
        let position = drag.update(&world, straight_down(6.7, 0.2), &surfaces);
        assert_at(position, Vec3::new(7.0, 0.5, 0.2));
        // Further off, it follows the mouse.
        let position = drag.update(&world, straight_down(5.0, 0.2), &surfaces);
        assert_at(position, Vec3::new(5.0, 0.5, 0.2));
    }

    #[test]
    fn with_collisions_it_waits_where_it_last_fit_instead_of_going_into_the_wall() {
        let (world, block) = scene();
        let mut drag = PlaceDrag::start(&world, block, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        drag.obstacles = around_block(&world, block);
        let collisions = Snap {
            collisions: true,
            ..NO_SNAP
        };
        let position = drag.update(&world, straight_down(5.0, 0.0), &collisions);
        assert_at(position, Vec3::new(5.0, 0.5, 0.0));
        // At 8 it would reach x = 10, into the wall (9 to 11).
        let position = drag.update(&world, straight_down(8.0, 0.0), &collisions);
        assert_at(position, Vec3::new(5.0, 0.5, 0.0));
        // Resting on the floor never counts as going into it.
        let position = drag.update(&world, straight_down(-3.0, 2.0), &collisions);
        assert_at(position, Vec3::new(-3.0, 0.5, 2.0));
    }
}
