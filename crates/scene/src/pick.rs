//! Clicking on parts: the ray from the camera through a point on the screen,
//! which part that ray hits first, and where a part's outline shows on screen.
//!
//! Screen points are in the renderer's normalized coordinates: x and y go
//! from -1 to 1 across the view, with (-1, -1) at the bottom left.

use axcenity_core::{InstanceId, PartType, World};
use axcenity_render::Camera;
use axcenity_render::glam::camera::rh;
use axcenity_render::glam::{Mat4, Vec2, Vec3};

use crate::draw_list::part_placement;

/// Lines are cut off this close to the camera before they're projected,
/// because points at or behind the camera have no place on the screen.
const NEAR_CLIP: f32 = 0.05;

/// A half-line starting at `origin` and going in `direction`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Has length 1, so distances along the ray are in studs.
    pub direction: Vec3,
}

impl Ray {
    /// The point `distance` studs along the ray.
    pub fn at(&self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }
}

/// A camera together with the shape of the view it draws into, so screen
/// points and world points can be turned into each other.
#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    /// Turns world positions into camera space, where the camera is at the
    /// origin looking down -Z with Y up (the same as the renderer).
    world_to_camera: Mat4,
    position: Vec3,
    /// How far the top edge of the view is above the middle, one stud in
    /// front of the camera.
    tan_half_fov: f32,
    /// The view's width divided by its height.
    aspect: f32,
}

impl CameraView {
    pub fn new(camera: Camera, aspect: f32) -> Self {
        Self {
            // The same view matrix as the renderer, so rays match what is drawn.
            world_to_camera: rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y),
            position: camera.position,
            tan_half_fov: (camera.fov_y_degrees.to_radians() / 2.0).tan(),
            aspect,
        }
    }

    /// The ray from the camera through a point on the screen.
    pub fn ray(&self, screen: Vec2) -> Ray {
        // The point on the screen one stud in front of the camera, in camera space.
        let toward = Vec3::new(
            screen.x * self.tan_half_fov * self.aspect,
            screen.y * self.tan_half_fov,
            -1.0,
        );
        let direction = self.world_to_camera.inverse().transform_vector3(toward);
        Ray {
            origin: self.position,
            direction: direction.normalize(),
        }
    }

    /// Where the line from `a` to `b` shows on the screen. Any part of it
    /// behind the camera is cut off, and `None` means all of it is.
    pub fn project_line(&self, a: Vec3, b: Vec3) -> Option<(Vec2, Vec2)> {
        let mut a = self.world_to_camera.transform_point3(a);
        let mut b = self.world_to_camera.transform_point3(b);
        // In camera space, "in front" is a z below -NEAR_CLIP.
        let limit = -NEAR_CLIP;
        if a.z > limit && b.z > limit {
            return None;
        }
        // Move the end that's behind the camera along the line to the limit.
        if a.z > limit {
            a = a.lerp(b, (limit - a.z) / (b.z - a.z));
        } else if b.z > limit {
            b = b.lerp(a, (limit - b.z) / (a.z - b.z));
        }
        Some((self.project(a), self.project(b)))
    }

    /// The screen point of a camera-space point in front of the camera.
    fn project(&self, point: Vec3) -> Vec2 {
        let depth = -point.z;
        Vec2::new(
            point.x / (depth * self.tan_half_fov * self.aspect),
            point.y / (depth * self.tan_half_fov),
        )
    }
}

/// The Workspace Part that `ray` hits first, if any. Parts the ray starts
/// inside of are skipped, so a camera inside a part can still pick past it.
pub fn pick(world: &World, ray: Ray) -> Option<InstanceId> {
    let mut nearest: Option<(InstanceId, f32)> = None;
    for id in world.descendants(world.workspace()) {
        let Some((shape, transform)) = world.get(id).and_then(part_placement) else {
            continue;
        };
        if let Some(distance) = hit_distance(shape, transform, ray)
            && nearest.is_none_or(|(_, nearest)| distance < nearest)
        {
            nearest = Some((id, distance));
        }
    }
    nearest.map(|(id, _)| id)
}

/// The 12 edges of the box around a Part as it's drawn, in world space, or
/// `None` if `id` isn't a Part.
pub fn part_box_edges(world: &World, id: InstanceId) -> Option<[(Vec3, Vec3); 12]> {
    let (_, transform) = part_placement(world.get(id)?)?;
    let corner = |x: f32, y: f32, z: f32| transform.transform_point3(Vec3::new(x, y, z) * 0.5);
    // The corners of the 1×1×1 cube: bit 0 picks the X side, bit 1 Y, bit 2 Z.
    let corners: [Vec3; 8] = std::array::from_fn(|i| {
        let side = |bit: usize| if i & (1 << bit) == 0 { -1.0 } else { 1.0 };
        corner(side(0), side(1), side(2))
    });
    // Two corners share an edge when their numbers differ in exactly one bit.
    let mut edges = [(Vec3::ZERO, Vec3::ZERO); 12];
    let mut next = 0;
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                edges[next] = (corners[i], corners[i | bit]);
                next += 1;
            }
        }
    }
    Some(edges)
}

/// The stretch of a ray inside a solid: the distances where it goes in and
/// comes out. Either can be negative (behind the ray's start) or infinite.
type Span = (f32, f32);

const EVERYWHERE: Span = (f32::NEG_INFINITY, f32::INFINITY);

/// How far along `ray` it first enters this part, if it does.
fn hit_distance(shape: PartType, transform: Mat4, ray: Ray) -> Option<f32> {
    // A part with no size (or broken numbers) can't be hit, and its
    // transform can't be undone.
    if !transform.determinant().is_normal() {
        return None;
    }
    // Test against the 1×1×1 mesh instead of the stretched, turned part.
    // The transform is affine, so the point `t` along the world ray lands at
    // the point `t` along the local ray: distances stay world distances.
    let to_mesh = transform.inverse();
    let origin = to_mesh.transform_point3(ray.origin);
    let direction = to_mesh.transform_vector3(ray.direction);

    let span = match shape {
        PartType::Block => inside_box(origin, direction),
        // A wedge is the box cut by its slope, which rises from the bottom
        // front (y = z = -0.5) to the top back (y = z = 0.5): inside is y <= z.
        PartType::Wedge => overlap(
            inside_box(origin, direction)?,
            inside_flat_side(origin, direction, Vec3::new(0.0, 1.0, -1.0), 0.0)?,
        ),
        PartType::Ball => inside_radius(origin, direction),
        // A cylinder along X: round in Y and Z, cut off at x = ±0.5.
        PartType::Cylinder => {
            let round = Vec3::new(0.0, 1.0, 1.0);
            overlap(
                inside_radius(origin * round, direction * round)?,
                overlap(
                    inside_flat_side(origin, direction, Vec3::X, 0.5)?,
                    inside_flat_side(origin, direction, Vec3::NEG_X, 0.5)?,
                )?,
            )
        }
    }?;
    let (enter, _) = span;
    (enter >= 0.0 && enter.is_finite()).then_some(enter)
}

/// Where two spans overlap, if they do.
fn overlap(a: Span, b: Span) -> Option<Span> {
    let span = (a.0.max(b.0), a.1.min(b.1));
    (span.0 <= span.1).then_some(span)
}

/// The span inside the 1×1×1 cube centered on the origin.
fn inside_box(origin: Vec3, direction: Vec3) -> Option<Span> {
    let mut span = EVERYWHERE;
    for normal in [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ] {
        span = overlap(span, inside_flat_side(origin, direction, normal, 0.5)?)?;
    }
    Some(span)
}

/// The span on the inner side of a flat side: the points `p` where
/// `normal.dot(p) <= offset`.
fn inside_flat_side(origin: Vec3, direction: Vec3, normal: Vec3, offset: f32) -> Option<Span> {
    let toward_outside = normal.dot(direction);
    let room = offset - normal.dot(origin);
    if toward_outside == 0.0 {
        // Running alongside the side: always inside, or never.
        return (room >= 0.0).then_some(EVERYWHERE);
    }
    let crossing = room / toward_outside;
    Some(if toward_outside < 0.0 {
        (crossing, f32::INFINITY)
    } else {
        (f32::NEG_INFINITY, crossing)
    })
}

/// The span within 0.5 of the origin (a ball 1 stud across).
fn inside_radius(origin: Vec3, direction: Vec3) -> Option<Span> {
    // Solve |origin + t·direction|² = 0.25 for t, a quadratic a·t² + 2b·t + c = 0.
    let a = direction.length_squared();
    let b = origin.dot(direction);
    let c = origin.length_squared() - 0.25;
    if a == 0.0 {
        return (c <= 0.0).then_some(EVERYWHERE);
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    Some(((-b - root) / a, (-b + root) / a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Value, Vector3};

    fn view(position: Vec3, look_at: Vec3) -> CameraView {
        let camera = Camera {
            position,
            look_at,
            fov_y_degrees: 70.0,
        };
        CameraView::new(camera, 16.0 / 9.0)
    }

    fn part(world: &mut World, shape: PartType, position: Vec3, size: Vec3) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let vector = |v: Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
        world
            .set_property(part, "Shape", Value::PartType(shape))
            .unwrap();
        world
            .set_property(part, "Position", vector(position))
            .unwrap();
        world.set_property(part, "Size", vector(size)).unwrap();
        part
    }

    /// A ray from far down -Z toward +Z, crossing the XY plane at (x, y).
    fn ray_along_z(x: f32, y: f32) -> Ray {
        Ray {
            origin: Vec3::new(x, y, -100.0),
            direction: Vec3::Z,
        }
    }

    #[test]
    fn the_middle_of_the_screen_looks_where_the_camera_looks() {
        let view = view(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 2.0, -1.0));
        let ray = view.ray(Vec2::ZERO);
        assert_eq!(ray.origin, Vec3::new(1.0, 2.0, 3.0));
        assert!(ray.direction.abs_diff_eq(Vec3::new(0.6, 0.0, -0.8), 1e-5));
    }

    #[test]
    fn a_point_along_a_screen_ray_projects_back_to_that_screen_point() {
        let view = view(Vec3::new(5.0, 8.0, 12.0), Vec3::ZERO);
        for screen in [Vec2::new(0.3, -0.7), Vec2::new(-1.0, 1.0), Vec2::ZERO] {
            let ray = view.ray(screen);
            let (a, b) = view.project_line(ray.at(2.0), ray.at(50.0)).unwrap();
            assert!(a.abs_diff_eq(screen, 1e-4), "{a} vs {screen}");
            assert!(b.abs_diff_eq(screen, 1e-4), "{b} vs {screen}");
        }
    }

    #[test]
    fn lines_behind_the_camera_are_cut_off_or_left_out() {
        let view = view(Vec3::ZERO, Vec3::NEG_Z);
        assert_eq!(view.project_line(Vec3::Z, Vec3::new(1.0, 0.0, 5.0)), None);
        // From in front to behind: the far end stops just in front of the camera.
        let (front, cut) = view
            .project_line(Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 1.0, 10.0))
            .unwrap();
        assert_eq!(front, Vec2::ZERO);
        assert!(cut.y > 1.0, "the cut end is above the top of the screen");
        assert!(cut.is_finite());
    }

    #[test]
    fn picks_the_nearest_part_along_the_ray() {
        let mut world = World::new();
        let far = part(&mut world, PartType::Block, Vec3::ZERO, Vec3::ONE);
        let near = part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::ONE,
        );
        assert_eq!(pick(&world, ray_along_z(0.0, 0.0)), Some(near));
        // Turned around, the other one is nearest.
        let backward = Ray {
            origin: Vec3::new(0.0, 0.0, 100.0),
            direction: Vec3::NEG_Z,
        };
        assert_eq!(pick(&world, backward), Some(far));
    }

    #[test]
    fn missing_every_part_picks_nothing() {
        let mut world = World::new();
        part(&mut world, PartType::Block, Vec3::ZERO, Vec3::ONE);
        assert_eq!(pick(&world, ray_along_z(0.0, 3.0)), None);
        // Pointing away from the part.
        let away = Ray {
            origin: Vec3::new(0.0, 0.0, -100.0),
            direction: Vec3::NEG_Z,
        };
        assert_eq!(pick(&world, away), None);
    }

    #[test]
    fn hits_the_drawn_shape_not_just_its_box() {
        let mut world = World::new();
        // Each shape is 2 studs across. (0.9, 0.9) is inside the box's corner
        // but outside anything round.
        for shape in PartType::ALL {
            let id = part(&mut world, shape, Vec3::ZERO, Vec3::splat(2.0));
            let middle = pick(&world, ray_along_z(0.0, 0.0));
            let corner = pick(&world, ray_along_z(0.9, 0.9));
            match shape {
                PartType::Ball => assert_eq!(corner, None),
                // A cylinder lies along X, so from the front it looks square.
                PartType::Block | PartType::Cylinder => assert_eq!(corner, Some(id), "{shape:?}"),
                // The wedge's top front corner is cut away by its slope,
                // but the ray still hits the slope further back.
                PartType::Wedge => assert_eq!(corner, Some(id)),
            }
            assert_eq!(middle, Some(id), "{shape:?}");
            world.destroy(id).unwrap();
        }
    }

    #[test]
    fn a_cylinder_is_round_seen_from_its_end() {
        let mut world = World::new();
        let id = part(&mut world, PartType::Cylinder, Vec3::ZERO, Vec3::splat(2.0));
        let along_x = |y: f32, z: f32| Ray {
            origin: Vec3::new(-100.0, y, z),
            direction: Vec3::X,
        };
        assert_eq!(pick(&world, along_x(0.0, 0.0)), Some(id));
        assert_eq!(pick(&world, along_x(0.9, 0.0)), Some(id));
        assert_eq!(pick(&world, along_x(0.9, 0.9)), None);
    }

    #[test]
    fn a_wedge_is_hit_on_its_slope() {
        let mut world = World::new();
        let wedge = part(&mut world, PartType::Wedge, Vec3::ZERO, Vec3::splat(2.0));
        let id = world.get(wedge).unwrap();
        let (shape, transform) = part_placement(id).unwrap();
        // From the front, a ray at mid-height meets the slope halfway back
        // (z = 0), not the box's front face at z = -1.
        let distance = hit_distance(shape, transform, ray_along_z(0.0, 0.0)).unwrap();
        assert!((distance - 100.0).abs() < 1e-4, "{distance}");
        // Straight down from above the front edge, the slope is at the bottom.
        let down = Ray {
            origin: Vec3::new(0.0, 10.0, -0.9),
            direction: Vec3::NEG_Y,
        };
        let distance = hit_distance(shape, transform, down).unwrap();
        assert!((distance - 10.9).abs() < 1e-4, "{distance}");
    }

    #[test]
    fn turned_parts_are_hit_where_they_are_drawn() {
        let mut world = World::new();
        // 10 studs long along X, then turned 90° so it's long along Z.
        let long = part(
            &mut world,
            PartType::Block,
            Vec3::ZERO,
            Vec3::new(10.0, 1.0, 1.0),
        );
        let from_above = |x: f32| Ray {
            origin: Vec3::new(x, 10.0, 0.0),
            direction: Vec3::NEG_Y,
        };
        assert_eq!(pick(&world, from_above(4.0)), Some(long));
        world
            .set_property(
                long,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
            )
            .unwrap();
        assert_eq!(pick(&world, from_above(4.0)), None);
    }

    #[test]
    fn a_part_the_camera_is_inside_is_skipped() {
        let mut world = World::new();
        part(&mut world, PartType::Block, Vec3::ZERO, Vec3::splat(10.0));
        let beyond = part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, 0.0, 20.0),
            Vec3::ONE,
        );
        let from_inside = Ray {
            origin: Vec3::ZERO,
            direction: Vec3::Z,
        };
        assert_eq!(pick(&world, from_inside), Some(beyond));
    }

    #[test]
    fn parts_with_no_size_are_never_hit() {
        let mut world = World::new();
        part(
            &mut world,
            PartType::Block,
            Vec3::ZERO,
            Vec3::new(4.0, 0.0, 4.0),
        );
        assert_eq!(pick(&world, ray_along_z(0.0, 0.0)), None);
    }

    #[test]
    fn box_edges_follow_the_part() {
        let mut world = World::new();
        let id = part(
            &mut world,
            PartType::Block,
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(2.0, 4.0, 6.0),
        );
        let edges = part_box_edges(&world, id).unwrap();
        for (a, b) in edges {
            // Every edge runs along one axis, as long as the size on that axis.
            let length = (b - a).abs();
            assert!([2.0, 4.0, 6.0].contains(&length.max_element()), "{length}");
            assert_eq!(length.min_element(), 0.0);
            for corner in [a, b] {
                assert_eq!(
                    (corner - Vec3::new(10.0, 0.0, 0.0)).abs(),
                    Vec3::new(1.0, 2.0, 3.0)
                );
            }
        }
        assert_eq!(part_box_edges(&world, world.workspace()), None);
    }
}
