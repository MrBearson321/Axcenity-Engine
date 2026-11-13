//! Clicking on parts: the ray from the camera through a point on the screen,
//! which part that ray hits first, and where a part's outline shows on screen.
//!
//! Screen points are in the renderer's normalized coordinates: x and y go
//! from -1 to 1 across the view, with (-1, -1) at the bottom left.

use axcenity_core::{InstanceId, PartType, TerrainData, World};
use axcenity_render::Camera;
use axcenity_render::glam::camera::rh;
use axcenity_render::glam::{Mat3, Mat4, Vec2, Vec3};
use axcenity_terrain::Placement;

use crate::draw_list::{part_pieces, part_placement};

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

    /// Where `point` shows on the screen, or `None` if it's behind the camera.
    pub fn project_point(&self, point: Vec3) -> Option<Vec2> {
        let point = self.world_to_camera.transform_point3(point);
        (point.z < -NEAR_CLIP).then(|| self.project(point))
    }

    /// How many studs at `point`'s distance from the camera fit between the
    /// middle of the screen and its top edge. Things drawn this many studs
    /// long, times a fixed share, look the same size however far away they are.
    pub fn studs_per_half_screen(&self, point: Vec3) -> f32 {
        let depth = -self.world_to_camera.transform_point3(point).z;
        depth.max(NEAR_CLIP) * self.tan_half_fov
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

/// The Workspace Part (or Terrain) that `ray` hits first, if any. Parts the
/// ray starts inside of are skipped, so a camera inside a part can still
/// pick past it.
pub fn pick(world: &World, ray: Ray) -> Option<InstanceId> {
    pick_hit(world, ray, None).map(|hit| hit.part)
}

/// Where a ray hits a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub part: InstanceId,
    /// How far along the ray, in studs.
    pub distance: f32,
    /// The direction the surface faces where it's hit, one stud long.
    pub normal: Vec3,
}

/// Like [`pick`], but also says where the part is hit and which way that
/// surface faces. `skip` is left out, so a part being dragged can look for
/// what's behind it.
pub fn pick_hit(world: &World, ray: Ray, skip: Option<InstanceId>) -> Option<Hit> {
    pick_hit_skipping(world, ray, skip.as_slice())
}

/// Like [`pick_hit`], leaving out every part in `skip` (such as all the
/// parts being dragged together). Terrains count, by their ground.
pub fn pick_hit_skipping(world: &World, ray: Ray, skip: &[InstanceId]) -> Option<Hit> {
    pick_hit_where(world, ray, |id| skip.contains(&id))
}

/// [`pick_hit_skipping`] with what to skip as a test, for a long list
/// kept in a set (the editor's hidden and locked data layers, R16.2).
pub fn pick_hit_where(world: &World, ray: Ray, skip: impl Fn(InstanceId) -> bool) -> Option<Hit> {
    let terrains: Vec<InstanceId> = world
        .all_below(world.workspace(), |class| {
            class == axcenity_core::ClassName::Terrain
        })
        .into_iter()
        .filter(|&id| skip(id))
        .collect();
    let mut nearest: Option<Hit> =
        crate::terrain::terrain_hit(world, ray.origin, ray.direction, &terrains).map(
            |(terrain, distance, normal)| Hit {
                part: terrain,
                distance,
                normal,
            },
        );
    for id in world.descendants_iter(world.workspace()) {
        if skip(id) || axcenity_core::csg::out_of_world(world, id) {
            continue;
        }
        let Some((shape, transform)) = world.get(id).and_then(part_placement) else {
            continue;
        };
        if let Some((distance, normal)) = hit_surface(shape, transform, ray)
            && nearest.is_none_or(|nearest| distance < nearest.distance)
        {
            nearest = Some(Hit {
                part: id,
                distance,
                normal,
            });
        }
    }
    nearest
}

/// Where `ray` first meets the part `id` alone, as it's drawn and picked
/// (a MeshPart by its box), or `None` if it misses or `id` isn't a part.
pub fn part_hit(world: &World, id: InstanceId, ray: Ray) -> Option<Hit> {
    let (shape, transform) = world.get(id).and_then(part_placement)?;
    hit_surface(shape, transform, ray).map(|(distance, normal)| Hit {
        part: id,
        distance,
        normal,
    })
}

/// How far a Part's drawn box reaches from its center in `direction` (one
/// stud long), or `None` if `id` isn't a Part. A part resting on a surface
/// facing `direction` has its center this far above it.
pub fn part_half_extent(world: &World, id: InstanceId, direction: Vec3) -> Option<f32> {
    let (_, transform) = part_placement(world.get(id)?)?;
    // Each column of the transform is one edge of the box (the 1×1×1 mesh
    // stretched and turned); half of each counts, however much it points
    // along `direction`.
    let edges = [transform.x_axis, transform.y_axis, transform.z_axis];
    Some(
        edges
            .iter()
            .map(|edge| edge.truncate().dot(direction).abs() / 2.0)
            .sum(),
    )
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

/// How many points along each edge [`visible_box_edges`] checks.
const EDGE_SAMPLES: usize = 16;

/// The parts of the box around a Part that can be seen from `eye`, as
/// lines in world space, for a Studio-like selection box: only the edges
/// of the box's sides that face the camera, and only where no other part is
/// in front of them. From inside the box, every edge counts as facing.
/// `None` if `id` isn't a Part.
pub fn visible_box_edges(world: &World, id: InstanceId, eye: Vec3) -> Option<Vec<(Vec3, Vec3)>> {
    Occluders::of(world).visible_box_edges(id, eye)
}

/// A part that can stand in front of a selection box.
struct Occluder {
    id: InstanceId,
    shape: PartType,
    transform: Mat4,
    center: Vec3,
    /// Half the box's diagonal: nothing of the part is further from `center`.
    radius: f32,
}

/// Everything in the world that can hide a selection box's edges, gathered
/// once a frame. Checking an edge takes up to 17 rays and a box has up to
/// 12 edges, so walking the world for each ray would cost several frames'
/// worth of time per selected part in a big scene.
pub struct Occluders<'a> {
    world: &'a World,
    parts: Vec<Occluder>,
    terrains: Vec<(InstanceId, Placement, &'a TerrainData)>,
}

impl<'a> Occluders<'a> {
    pub fn of(world: &'a World) -> Self {
        let mut parts = Vec::new();
        let mut terrains = Vec::new();
        for id in world.descendants_iter(world.workspace()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if let (Some(placement), Some(data)) = (Placement::of(instance), world.terrain(id)) {
                terrains.push((id, placement, &**data));
            }
            if axcenity_core::csg::out_of_world(world, id) {
                continue;
            }
            let Some((shape, transform)) = part_placement(instance) else {
                continue;
            };
            parts.push(Occluder {
                id,
                shape,
                transform,
                center: transform.w_axis.truncate(),
                radius: box_radius(transform),
            });
        }
        Self {
            world,
            parts,
            terrains,
        }
    }

    /// See the free function [`visible_box_edges`].
    pub fn visible_box_edges(&self, id: InstanceId, eye: Vec3) -> Option<Vec<(Vec3, Vec3)>> {
        visible_edges(self, id, eye)
    }

    /// Whether something other than `id` is in front of `point`, from
    /// `eye`, among the parts in `near`.
    fn hides(&self, near: &[&Occluder], id: InstanceId, eye: Vec3, point: Vec3) -> bool {
        let offset = point - eye;
        let distance = offset.length();
        if distance < 1e-4 {
            return false;
        }
        let direction = offset / distance;
        let ray = Ray {
            origin: eye,
            direction,
        };
        let limit = distance - (0.05_f32).max(distance * 0.002);
        near.iter().any(|part| {
            hit_surface(part.shape, part.transform, ray).is_some_and(|(hit, _)| hit < limit)
        }) || self.terrains.iter().any(|(terrain, placement, data)| {
            *terrain != id
                && axcenity_terrain::raycast::raycast_within(data, placement, eye, direction, limit)
                    .is_some_and(|hit| hit < limit)
        })
    }

    /// The parts other than `id` that a ray from `eye` to somewhere on the
    /// line from `a` to `b` can meet.
    fn near_edge(&self, id: InstanceId, eye: Vec3, a: Vec3, b: Vec3) -> Vec<&Occluder> {
        let middle = (a + b) / 2.0;
        let reach = (b - a).length() / 2.0;
        self.parts
            .iter()
            .filter(|part| {
                part.id != id
                    && distance_to_segment(part.center, eye, middle) <= part.radius + reach
            })
            .collect()
    }
}

/// Half the diagonal of the box a part's transform stretches the 1×1×1
/// mesh to.
fn box_radius(transform: Mat4) -> f32 {
    let lengths = transform.x_axis.truncate().length_squared()
        + transform.y_axis.truncate().length_squared()
        + transform.z_axis.truncate().length_squared();
    lengths.sqrt() / 2.0
}

/// How far `point` is from the line segment from `a` to `b`.
fn distance_to_segment(point: Vec3, a: Vec3, b: Vec3) -> f32 {
    let along = b - a;
    let length = along.length_squared();
    let t = if length > 0.0 {
        ((point - a).dot(along) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + along * t).distance(point)
}

fn visible_edges(occluders: &Occluders, id: InstanceId, eye: Vec3) -> Option<Vec<(Vec3, Vec3)>> {
    let world = occluders.world;
    let (_, transform) = part_placement(world.get(id)?)?;
    let center = transform.w_axis.truncate();
    let axes = [
        transform.x_axis.truncate(),
        transform.y_axis.truncate(),
        transform.z_axis.truncate(),
    ];
    // Whether the side of the box at `sign` (±1) along axis `axis` faces the eye.
    let faces = |axis: usize, sign: f32| {
        let side_center = center + axes[axis] * 0.5 * sign;
        (eye - side_center).dot(axes[axis] * sign) > 0.0
    };
    let inside = (0..3).all(|axis| !faces(axis, 1.0) && !faces(axis, -1.0));
    let corner = |i: usize| {
        let side = |bit: usize| if i & (1 << bit) == 0 { -0.5 } else { 0.5 };
        center + axes[0] * side(0) + axes[1] * side(1) + axes[2] * side(2)
    };
    let mut lines = Vec::new();
    for i in 0..8 {
        for along in 0..3 {
            if i & (1 << along) != 0 {
                continue;
            }
            // The two sides that meet at this edge.
            let seen = inside
                || (0..3).filter(|&axis| axis != along).any(|axis| {
                    let sign = if i & (1 << axis) == 0 { -1.0 } else { 1.0 };
                    faces(axis, sign)
                });
            if !seen {
                continue;
            }
            let (a, b) = (corner(i), corner(i | (1 << along)));
            // Split into the runs no other part hides.
            let near = occluders.near_edge(id, eye, a, b);
            let hidden = |t: f32| occluders.hides(&near, id, eye, a.lerp(b, t));
            let mut run_start: Option<f32> = None;
            for step in 0..=EDGE_SAMPLES {
                let t = step as f32 / EDGE_SAMPLES as f32;
                match (hidden(t), run_start) {
                    (false, None) => run_start = Some(t),
                    (true, Some(start)) => {
                        let end = t - 0.5 / EDGE_SAMPLES as f32;
                        if end > start {
                            lines.push((a.lerp(b, start), a.lerp(b, end)));
                        }
                        run_start = None;
                    }
                    _ => {}
                }
            }
            if let Some(start) = run_start {
                lines.push((a.lerp(b, start), b));
            }
        }
    }
    Some(lines)
}

/// The stretch of a ray inside a solid: the distances where it goes in and
/// comes out. Either can be negative (behind the ray's start) or infinite.
#[derive(Clone, Copy, Debug)]
struct Span {
    enter: f32,
    /// Which way the surface the ray goes in through faces (in mesh space,
    /// not yet one stud long). Zero when `enter` is minus infinity.
    normal: Vec3,
    exit: f32,
}

const EVERYWHERE: Span = Span {
    enter: f32::NEG_INFINITY,
    normal: Vec3::ZERO,
    exit: f32::INFINITY,
};

/// How far along `ray` it first enters this part, if it does.
#[cfg(test)]
fn hit_distance(shape: PartType, transform: Mat4, ray: Ray) -> Option<f32> {
    hit_surface(shape, transform, ray).map(|(distance, _)| distance)
}

/// How far along `ray` it first enters this part, and which way the surface
/// faces there (in the world, one stud long), if it does.
fn hit_surface(shape: PartType, transform: Mat4, ray: Ray) -> Option<(f32, Vec3)> {
    // A capsule is hit where the nearest of its pieces is.
    if shape == PartType::Capsule {
        return part_pieces(shape, transform)
            .into_iter()
            .filter_map(|(piece, transform)| hit_surface(piece, transform, ray))
            .min_by(|a, b| a.0.total_cmp(&b.0));
    }
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
        // A corner wedge is the box cut by its two slopes, both rising to
        // the top back right corner (0.5, 0.5, 0.5): the wedge's (y <= z)
        // and one rising from the left edge (y <= x).
        PartType::CornerWedge => overlap(
            overlap(
                inside_box(origin, direction)?,
                inside_flat_side(origin, direction, Vec3::new(0.0, 1.0, -1.0), 0.0)?,
            )?,
            inside_flat_side(origin, direction, Vec3::new(-1.0, 1.0, 0.0), 0.0)?,
        ),
        PartType::Ball => inside_radius(origin, direction),
        // Split into pieces above.
        PartType::Capsule => None,
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
    if !(span.enter >= 0.0 && span.enter.is_finite()) {
        return None;
    }
    // Directions along a surface stretch with the part, but the direction
    // straight out of it changes the other way: normals are turned by the
    // transpose of the inverse (the standard rule for normals).
    let normal = Mat3::from_mat4(to_mesh).transpose() * span.normal;
    Some((span.enter, normal.normalize_or(Vec3::Y)))
}

/// Where two spans overlap, if they do. The later entry wins, with its surface.
fn overlap(a: Span, b: Span) -> Option<Span> {
    let (enter, normal) = if a.enter >= b.enter {
        (a.enter, a.normal)
    } else {
        (b.enter, b.normal)
    };
    let exit = a.exit.min(b.exit);
    (enter <= exit).then_some(Span {
        enter,
        normal,
        exit,
    })
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
        Span {
            enter: crossing,
            normal,
            exit: f32::INFINITY,
        }
    } else {
        Span {
            exit: crossing,
            ..EVERYWHERE
        }
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
    let enter = (-b - root) / a;
    Some(Span {
        enter,
        // On a ball around the origin, the surface faces straight out from it.
        normal: origin + direction * enter,
        exit: (-b + root) / a,
    })
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
    fn points_project_onto_the_screen_unless_behind_the_camera() {
        let view = view(Vec3::ZERO, Vec3::NEG_Z);
        assert_eq!(
            view.project_point(Vec3::new(0.0, 0.0, -5.0)),
            Some(Vec2::ZERO)
        );
        assert_eq!(view.project_point(Vec3::new(0.0, 0.0, 5.0)), None);
        // At 10 studs away, the top edge of the view is 10·tan(35°) studs up.
        let half = view.studs_per_half_screen(Vec3::new(3.0, 0.0, -10.0));
        assert!((half - 10.0 * 35f32.to_radians().tan()).abs() < 1e-5);
        let top = view.project_point(Vec3::new(0.0, half, -10.0)).unwrap();
        assert!((top.y - 1.0).abs() < 1e-5);
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
                // A capsule as long as it's wide is a ball.
                PartType::Ball | PartType::Capsule => assert_eq!(corner, None),
                // A cylinder lies along X, so from the front it looks square.
                PartType::Block | PartType::Cylinder => assert_eq!(corner, Some(id), "{shape:?}"),
                // The wedge's top front corner is cut away by its slope,
                // but the ray still hits the slope further back.
                PartType::Wedge => assert_eq!(corner, Some(id)),
                // So is the corner wedge's: (0.9, 0.9) is on its edge up
                // to the point; `a_corner_wedge_is_hit_on_its_slopes` looks
                // closer.
                PartType::CornerWedge => {}
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
    fn a_capsule_has_round_ends() {
        let mut world = World::new();
        // 6 long, 2 across: a cylinder from x = -2 to 2, balls beyond.
        let id = part(
            &mut world,
            PartType::Capsule,
            Vec3::ZERO,
            Vec3::new(6.0, 2.0, 2.0),
        );
        let hit = |x: f32, y: f32| pick(&world, ray_along_z(x, y));
        assert_eq!(hit(0.0, 0.9), Some(id), "the middle is a cylinder");
        assert_eq!(hit(1.9, 0.9), Some(id));
        assert_eq!(hit(2.9, 0.0), Some(id), "the end's tip");
        assert_eq!(hit(2.9, 0.9), None, "the box's corner, outside the end");
        assert_eq!(hit(3.1, 0.0), None);
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
    fn a_corner_wedge_is_hit_on_its_slopes() {
        let mut world = World::new();
        let id = part(
            &mut world,
            PartType::CornerWedge,
            Vec3::ZERO,
            Vec3::splat(2.0),
        );
        let (shape, transform) = part_placement(world.get(id).unwrap()).unwrap();
        // Seen from above, its top is as high as the lower of x and z: it
        // rises toward +X and +Z to the point over the back right corner.
        let down = |x: f32, z: f32| Ray {
            origin: Vec3::new(x, 10.0, z),
            direction: Vec3::NEG_Y,
        };
        for (x, z) in [(0.5, -0.5), (-0.9, 0.9), (0.9, 0.9), (0.2, 0.6)] {
            let distance = hit_distance(shape, transform, down(x, z)).unwrap();
            let top = f32::min(x, z);
            assert!(
                (distance - (10.0 - top)).abs() < 1e-4,
                "({x}, {z}): {distance}"
            );
        }
        // From the front, the box's top left corner is cut away, the top
        // right one isn't (it's under the right side's upright edge).
        assert_eq!(pick(&world, ray_along_z(-0.9, 0.9)), None);
        assert_eq!(pick(&world, ray_along_z(0.95, 0.9)), Some(id));
        assert_eq!(pick(&world, ray_along_z(0.0, -0.5)), Some(id));
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
    fn hits_say_which_way_the_surface_faces() {
        let mut world = World::new();
        let block = part(
            &mut world,
            PartType::Block,
            Vec3::ZERO,
            Vec3::new(4.0, 1.0, 2.0),
        );
        let down = |x: f32, z: f32| Ray {
            origin: Vec3::new(x, 10.0, z),
            direction: Vec3::NEG_Y,
        };
        let hit = pick_hit(&world, down(1.0, 0.5), None).unwrap();
        assert_eq!(hit.part, block);
        assert!((hit.distance - 9.5).abs() < 1e-5);
        assert!(hit.normal.abs_diff_eq(Vec3::Y, 1e-5), "{}", hit.normal);
        // From the side, the side faces back toward the ray.
        let hit = pick_hit(&world, ray_along_z(0.0, 0.0), None).unwrap();
        assert!(hit.normal.abs_diff_eq(Vec3::NEG_Z, 1e-5), "{}", hit.normal);

        // A ball faces straight out from its center; a stretched wedge's
        // slope faces up and forward at the slope's real angle.
        world.destroy(block).unwrap();
        let ball = part(&mut world, PartType::Ball, Vec3::ZERO, Vec3::splat(2.0));
        let hit = pick_hit(&world, down(0.6, 0.0), None).unwrap();
        assert!(hit.normal.abs_diff_eq(Vec3::new(0.6, 0.8, 0.0), 1e-4));
        world.destroy(ball).unwrap();
        part(
            &mut world,
            PartType::Wedge,
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
        );
        let hit = pick_hit(&world, down(0.0, 0.0), None).unwrap();
        // Rises 1 stud over 2: facing (0, 2, -1) scaled to one stud.
        assert!(
            hit.normal
                .abs_diff_eq(Vec3::new(0.0, 2.0, -1.0).normalize(), 1e-4)
        );
    }

    #[test]
    fn a_skipped_part_lets_the_ray_reach_what_is_behind_it() {
        let mut world = World::new();
        let floor = part(
            &mut world,
            PartType::Block,
            Vec3::ZERO,
            Vec3::new(10.0, 1.0, 10.0),
        );
        let crate_ = part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::ONE,
        );
        let down = Ray {
            origin: Vec3::new(0.0, 10.0, 0.0),
            direction: Vec3::NEG_Y,
        };
        assert_eq!(pick_hit(&world, down, None).unwrap().part, crate_);
        assert_eq!(pick_hit(&world, down, Some(crate_)).unwrap().part, floor);
    }

    #[test]
    fn a_terrain_hides_the_box_edges_behind_it_and_only_those() {
        let mut world = World::new();
        let terrain = world.create(axcenity_core::ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        // The default terrain's ground is flat at Y = 0.
        let above = part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::splat(4.0),
        );
        let below = part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, -10.0, 0.0),
            Vec3::splat(4.0),
        );
        let eye = Vec3::new(20.0, 30.0, 20.0);
        let occluders = Occluders::of(&world);
        assert!(!occluders.visible_box_edges(above, eye).unwrap().is_empty());
        assert!(occluders.visible_box_edges(below, eye).unwrap().is_empty());
    }

    #[test]
    fn half_extent_is_how_far_the_turned_box_reaches() {
        let mut world = World::new();
        let id = part(
            &mut world,
            PartType::Block,
            Vec3::ZERO,
            Vec3::new(4.0, 1.0, 2.0),
        );
        assert_eq!(part_half_extent(&world, id, Vec3::Y), Some(0.5));
        assert_eq!(part_half_extent(&world, id, Vec3::NEG_X), Some(2.0));
        // Stood on its end (turned 90° around Z), it's 4 studs tall.
        world
            .set_property(
                id,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 0.0, 90.0)),
            )
            .unwrap();
        let up = part_half_extent(&world, id, Vec3::Y).unwrap();
        assert!((up - 2.0).abs() < 1e-5, "{up}");
        assert_eq!(part_half_extent(&world, world.workspace(), Vec3::Y), None);
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

    #[test]
    fn only_the_box_edges_facing_the_camera_and_not_hidden_show() {
        let mut world = World::new();
        let id = part(&mut world, PartType::Block, Vec3::ZERO, Vec3::splat(2.0));
        // From a corner: three sides face the camera, which share 9 edges.
        let eye = Vec3::new(10.0, 10.0, 10.0);
        let lines = visible_box_edges(&world, id, eye).unwrap();
        assert_eq!(lines.len(), 9, "{lines:?}");
        // Straight on, only the front side's 4 edges show.
        let lines = visible_box_edges(&world, id, Vec3::new(0.0, 0.0, 10.0)).unwrap();
        assert_eq!(lines.len(), 4, "{lines:?}");
        // A wall between it and the camera hides it all.
        part(
            &mut world,
            PartType::Block,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(20.0, 20.0, 1.0),
        );
        let lines = visible_box_edges(&world, id, Vec3::new(0.0, 0.0, 10.0)).unwrap();
        assert!(lines.is_empty(), "{lines:?}");
    }
}
