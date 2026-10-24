//! Kinematic character controllers.
//!
//! A [`ThirdPersonController`](axcenity_core::ClassName::ThirdPersonController)
//! is parented to a character [`Model`](axcenity_core::ClassName::Model) (or
//! to one Part). During Play, [`Controllers::step`] walks or runs that
//! character on flat ground from the held movement keys. It is not a dynamic
//! body: [`Controllers::keep_kinematic`] anchors its parts so physics does
//! not tip or drop them, and so gravity and friction of other parts stay as
//! they are.
//!
//! The first such controller in the Workspace that has a body receives the
//! input and owns the camera. Jump, slopes, steps, crouch, and a
//! first-person controller are later phases.

use std::collections::BTreeMap;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::{RayHit, cast_ray_excluding};

/// How far a flat floor may be from the feet, in studs, and still count as
/// the ground the character is already standing on. Steeper ground and
/// steps are a later phase.
const SNAP: f32 = 0.75;
/// A hit this close to straight up is flat ground. About 18 degrees.
const FLAT_NORMAL_Y: f32 = 0.95;
/// A hit flatter than this is not a wall, so the character is not stopped
/// by the floor under its feet.
const WALL_NORMAL_Y: f32 = 0.4;
/// Gap left between the character and a wall, in studs.
const SKIN: f32 = 0.05;
/// Radians turned per pixel of mouse movement. The same as the fly camera.
const MOUSE_SENSITIVITY: f32 = 0.003;
const MAX_PITCH: f32 = 89.0_f32.to_radians();
/// Looks slightly down, so the camera sits above the character as well as behind.
const DEFAULT_PITCH: f32 = -18.0_f32.to_radians();
const MIN_CAMERA_DISTANCE: f32 = 1.0;

/// Keys held for the character that owns the camera, and mouse look since
/// the last step. Right and down are positive, in pixels, as with the fly
/// camera.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub look_dx: f32,
    pub look_dy: f32,
}

/// Where the third-person camera is, in the same terms as `FlyCamera::view`:
/// position in studs, and orientation in degrees (X looks up and down, Y
/// turns, Z is zero).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterCamera {
    pub position: Vector3,
    pub orientation: Vector3,
}

/// Yaw and pitch of one controller's camera. Not saved; Play starts behind
/// the character, looking slightly down.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    yaw: f32,
    pitch: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: DEFAULT_PITCH,
        }
    }
}

impl Orbit {
    fn look(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw -= dx * MOUSE_SENSITIVITY;
        self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }
}

/// The parts one controller moves, and the box around them. The box ignores
/// rotation: characters stand upright, and turning them is not this phase.
struct Body {
    parts: Vec<InstanceId>,
    pivot_x: f32,
    pivot_z: f32,
    feet_y: f32,
    height: f32,
    radius: f32,
}

/// Steps every third-person controller. The player and the editor each own
/// one for the game they are running.
#[derive(Default)]
pub struct Controllers {
    orbits: BTreeMap<InstanceId, Orbit>,
    /// The controller that last received input, if it is still in the world.
    active: Option<InstanceId>,
}

impl Controllers {
    /// Moves the active character for `seconds` of time and updates its
    /// camera from `input`. Returns that camera, or `None` when no
    /// controller has a body. Does not anchor; call [`Self::keep_kinematic`]
    /// after scripts and before physics.
    pub fn step(
        &mut self,
        world: &mut World,
        assets: &Assets,
        input: MoveInput,
        seconds: f32,
    ) -> Option<CharacterCamera> {
        let dt = if seconds.is_finite() && seconds > 0.0 {
            seconds
        } else {
            0.0
        };
        self.orbits.retain(|id, _| world.get(*id).is_some());
        let Some(id) = active_id(world) else {
            self.active = None;
            return None;
        };
        self.active = Some(id);
        let orbit = self.orbits.entry(id).or_default();
        orbit.look(input.look_dx, input.look_dy);
        let orbit = *orbit;
        if let Some(body) = Body::read(world, id) {
            let distance = speed_of(world, id) * dt;
            let (dx, dz) = wish(orbit.yaw, input, distance);
            let (dx, dz) = walls(world, assets, &body, dx, dz);
            if dx != 0.0 || dz != 0.0 {
                translate(world, &body.parts, dx, 0.0, dz);
            }
            let dy = ground(world, assets, &body, dx, dz);
            if dy != 0.0 {
                translate(world, &body.parts, 0.0, dy, 0.0);
            }
        }
        self.view(world)
    }

    /// The follow camera of the controller that owns it, from where the
    /// character is now. `None` when nothing has a body. Safe before the
    /// first [`step`](Self::step): the camera starts behind the character.
    pub fn view(&self, world: &World) -> Option<CharacterCamera> {
        let id = match self.active {
            Some(id) if is_driver(world, id) => id,
            _ => active_id(world)?,
        };
        let orbit = self.orbits.get(&id).copied().unwrap_or_default();
        pose_camera(world, id, orbit)
    }

    /// Anchors every part a third-person controller moves, so the physics
    /// step does not treat the character as a dynamic body. Parts that are
    /// already anchored are left alone. Other parts, gravity, and friction
    /// are not touched.
    pub fn keep_kinematic(&self, world: &mut World) {
        for id in controllers(world) {
            for part in parts_of(world, id) {
                let anchored = world
                    .get(part)
                    .and_then(|instance| instance.property("Anchored"))
                    .and_then(Value::as_bool);
                if anchored == Some(false) {
                    let _ = world.set_property(part, "Anchored", Value::Bool(true));
                }
            }
        }
    }
}

fn controllers(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::ThirdPersonController)
        })
        .collect()
}

fn is_driver(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class() == ClassName::ThirdPersonController)
        && !parts_of(world, id).is_empty()
}

/// The first controller in the Workspace whose parent is a character.
fn active_id(world: &World) -> Option<InstanceId> {
    controllers(world)
        .into_iter()
        .find(|&id| is_driver(world, id))
}

/// The Part and MeshPart descendants of the controller's parent, plus the
/// parent when the controller is parented directly to a part.
fn parts_of(world: &World, controller: InstanceId) -> Vec<InstanceId> {
    let Some(parent) = world.get(controller).and_then(|instance| instance.parent()) else {
        return Vec::new();
    };
    let Some(instance) = world.get(parent) else {
        return Vec::new();
    };
    let character = matches!(
        instance.class(),
        ClassName::Model | ClassName::Part | ClassName::MeshPart
    );
    if !character {
        return Vec::new();
    }
    let mut parts = Vec::new();
    if matches!(instance.class(), ClassName::Part | ClassName::MeshPart) {
        parts.push(parent);
    }
    for id in world.descendants(parent) {
        if world
            .get(id)
            .is_some_and(|child| matches!(child.class(), ClassName::Part | ClassName::MeshPart))
        {
            parts.push(id);
        }
    }
    parts
}

impl Body {
    fn read(world: &World, controller: InstanceId) -> Option<Self> {
        let parts = parts_of(world, controller);
        if parts.is_empty() {
            return None;
        }
        let mut min = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for id in &parts {
            let Some(instance) = world.get(*id) else {
                continue;
            };
            let Some(position) = instance.property("Position").and_then(Value::as_vector3) else {
                continue;
            };
            let Some(size) = instance.property("Size").and_then(Value::as_vector3) else {
                continue;
            };
            if !finite(position) || !finite(size) {
                continue;
            }
            any = true;
            let half = Vector3::new(size.x.abs() * 0.5, size.y.abs() * 0.5, size.z.abs() * 0.5);
            min.x = min.x.min(position.x - half.x);
            min.y = min.y.min(position.y - half.y);
            min.z = min.z.min(position.z - half.z);
            max.x = max.x.max(position.x + half.x);
            max.y = max.y.max(position.y + half.y);
            max.z = max.z.max(position.z + half.z);
        }
        if !any {
            return None;
        }
        Some(Self {
            parts,
            pivot_x: (min.x + max.x) * 0.5,
            pivot_z: (min.z + max.z) * 0.5,
            feet_y: min.y,
            height: (max.y - min.y).max(0.5),
            radius: ((max.x - min.x).max(max.z - min.z) * 0.5).max(0.25),
        })
    }
}

fn finite(value: Vector3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn number(world: &World, id: InstanceId, name: &str) -> Option<f32> {
    let value = world.get(id)?.property(name)?.as_number()?;
    value.is_finite().then_some(value as f32)
}

fn is_run(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property("Movement"))
        .and_then(Value::as_str)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("run"))
}

fn speed_of(world: &World, id: InstanceId) -> f32 {
    let name = if is_run(world, id) {
        "RunSpeed"
    } else {
        "WalkSpeed"
    };
    number(world, id, name).unwrap_or(0.0).max(0.0)
}

fn camera_distance(world: &World, id: InstanceId) -> f32 {
    number(world, id, "CameraDistance")
        .unwrap_or(12.0)
        .max(MIN_CAMERA_DISTANCE)
}

fn camera_height(world: &World, id: InstanceId) -> f32 {
    number(world, id, "CameraHeight").unwrap_or(4.0)
}

/// Horizontal movement of `distance` studs, relative to the camera's yaw.
/// Yaw 0 moves toward -Z, the same way the fly camera looks at yaw 0.
fn wish(yaw: f32, input: MoveInput, distance: f32) -> (f32, f32) {
    let axis = |positive: bool, negative: bool| f32::from(positive) - f32::from(negative);
    let forward_amount = axis(input.forward, input.back);
    let right_amount = axis(input.right, input.left);
    let (sy, cy) = yaw.sin_cos();
    let x = -sy * forward_amount + cy * right_amount;
    let z = -cy * forward_amount - sy * right_amount;
    let len = x.hypot(z);
    if len < 1e-6 || distance == 0.0 {
        return (0.0, 0.0);
    }
    (x / len * distance, z / len * distance)
}

fn walls(world: &World, assets: &Assets, body: &Body, dx: f32, dz: f32) -> (f32, f32) {
    let len = dx.hypot(dz);
    if len < 1e-5 {
        return (dx, dz);
    }
    let reach = len + body.radius;
    let origin = Vector3::new(body.pivot_x, body.feet_y + body.height * 0.5, body.pivot_z);
    let direction = Vector3::new(dx / len * reach, 0.0, dz / len * reach);
    let Some(hit) = cast_ray_excluding(world, assets, origin, direction, &body.parts) else {
        return (dx, dz);
    };
    slide(dx, dz, len, body.radius, hit)
}

fn slide(dx: f32, dz: f32, len: f32, radius: f32, hit: RayHit) -> (f32, f32) {
    if !hit.can_collide || hit.normal.y >= WALL_NORMAL_Y {
        return (dx, dz);
    }
    let allowed = (hit.distance - radius - SKIN).clamp(0.0, len);
    let moved_x = dx / len * allowed;
    let moved_z = dz / len * allowed;
    let nlen = hit.normal.x.hypot(hit.normal.z);
    if nlen < 1e-4 {
        return (moved_x, moved_z);
    }
    let nx = hit.normal.x / nlen;
    let nz = hit.normal.z / nlen;
    // Whatever is left after reaching the wall slides along it.
    let remain_x = dx - moved_x;
    let remain_z = dz - moved_z;
    let into = remain_x * nx + remain_z * nz;
    if into >= 0.0 {
        return (moved_x, moved_z);
    }
    (
        moved_x + remain_x - into * nx,
        moved_z + remain_z - into * nz,
    )
}

fn ground(world: &World, assets: &Assets, body: &Body, dx: f32, dz: f32) -> f32 {
    let origin = Vector3::new(body.pivot_x + dx, body.feet_y + 1.0, body.pivot_z + dz);
    let Some(hit) = cast_ray_excluding(
        world,
        assets,
        origin,
        Vector3::new(0.0, -2.0, 0.0),
        &body.parts,
    ) else {
        return 0.0;
    };
    if !hit.can_collide || hit.normal.y < FLAT_NORMAL_Y {
        return 0.0;
    }
    let dy = hit.position.y - body.feet_y;
    if dy.abs() <= SNAP { dy } else { 0.0 }
}

fn translate(world: &mut World, parts: &[InstanceId], dx: f32, dy: f32, dz: f32) {
    for &id in parts {
        let Some(position) = world
            .get(id)
            .and_then(|instance| instance.property("Position"))
            .and_then(Value::as_vector3)
        else {
            continue;
        };
        if !finite(position) {
            continue;
        }
        let next = Vector3::new(position.x + dx, position.y + dy, position.z + dz);
        let _ = world.set_property(id, "Position", Value::Vector3(next));
    }
}

fn pose_camera(world: &World, id: InstanceId, orbit: Orbit) -> Option<CharacterCamera> {
    let body = Body::read(world, id)?;
    let (sy, cy) = orbit.yaw.sin_cos();
    let (sp, cp) = orbit.pitch.sin_cos();
    let forward = (-sy * cp, sp, -cy * cp);
    let distance = camera_distance(world, id);
    let look_y = body.feet_y + camera_height(world, id);
    let yaw = (orbit.yaw.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
    Some(CharacterCamera {
        position: Vector3::new(
            body.pivot_x - forward.0 * distance,
            look_y - forward.1 * distance,
            body.pivot_z - forward.2 * distance,
        ),
        orientation: Vector3::new(orbit.pitch.to_degrees(), yaw, 0.0),
    })
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Value, Vector3, World};
    use axcenity_physics::Physics;

    use super::*;

    fn pos(world: &World, id: InstanceId) -> Vector3 {
        world
            .get(id)
            .unwrap()
            .property("Position")
            .unwrap()
            .as_vector3()
            .unwrap()
    }

    fn orientation(world: &World, id: InstanceId) -> Vector3 {
        world
            .get(id)
            .unwrap()
            .property("Orientation")
            .unwrap()
            .as_vector3()
            .unwrap()
    }

    /// A wide floor and a character standing on it: a Model, a 2×2×2 body
    /// whose feet are at y = 0, and a controller on the model.
    fn standing() -> (World, InstanceId, InstanceId) {
        let mut world = World::new();
        let workspace = world.workspace();
        let floor = world.create(ClassName::Part).unwrap();
        world.set_parent(floor, Some(workspace)).unwrap();
        world
            .set_property(floor, "Anchored", Value::Bool(true))
            .unwrap();
        world
            .set_property(floor, "Size", Value::Vector3(Vector3::new(80.0, 1.0, 80.0)))
            .unwrap();
        world
            .set_property(
                floor,
                "Position",
                Value::Vector3(Vector3::new(0.0, -0.5, 0.0)),
            )
            .unwrap();
        let model = world.create(ClassName::Model).unwrap();
        world.set_name(model, "Character").unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let body = world.create(ClassName::Part).unwrap();
        world.set_name(body, "Body").unwrap();
        world.set_parent(body, Some(model)).unwrap();
        world
            .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0)))
            .unwrap();
        world
            .set_property(
                body,
                "Position",
                Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
            )
            .unwrap();
        let controller = world.create(ClassName::ThirdPersonController).unwrap();
        world.set_parent(controller, Some(model)).unwrap();
        (world, controller, body)
    }

    fn forward() -> MoveInput {
        MoveInput {
            forward: true,
            ..MoveInput::default()
        }
    }

    #[test]
    fn walk_and_run_move_at_different_speeds_and_stay_upright() {
        let assets = Assets::default();
        let (mut world, controller, body) = standing();
        world
            .set_property(controller, "WalkSpeed", Value::Number(16.0))
            .unwrap();
        world
            .set_property(controller, "RunSpeed", Value::Number(32.0))
            .unwrap();
        let mut controllers = Controllers::default();
        controllers.step(&mut world, &assets, forward(), 1.0);
        let walked = pos(&world, body);
        assert!((walked.z + 16.0).abs() < 1e-3, "{walked:?}");
        assert!((walked.y - 1.0).abs() < 1e-3, "{walked:?}");
        assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));

        world
            .set_property(
                body,
                "Position",
                Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
            )
            .unwrap();
        world
            .set_property(controller, "Movement", Value::String("Run".to_owned()))
            .unwrap();
        controllers.step(&mut world, &assets, forward(), 1.0);
        let ran = pos(&world, body);
        assert!((ran.z + 32.0).abs() < 1e-3, "{ran:?}");
        assert!((ran.y - 1.0).abs() < 1e-3, "{ran:?}");
        assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn the_camera_starts_behind_the_character() {
        let (world, _, _) = standing();
        let camera = Controllers::default().view(&world).unwrap();
        assert!(camera.position.z > 8.0, "{:?}", camera.position);
        assert!(camera.position.y > 4.0, "{:?}", camera.position);
        assert!(camera.position.x.abs() < 1e-3, "{:?}", camera.position);
    }

    #[test]
    fn a_wall_stops_the_character_and_a_crate_still_falls() {
        let assets = Assets::default();
        let (mut world, controller, body) = standing();
        world
            .set_property(controller, "WalkSpeed", Value::Number(16.0))
            .unwrap();
        let wall = world.create(ClassName::Part).unwrap();
        world.set_parent(wall, Some(world.workspace())).unwrap();
        world
            .set_property(wall, "Anchored", Value::Bool(true))
            .unwrap();
        world
            .set_property(wall, "Size", Value::Vector3(Vector3::new(6.0, 4.0, 1.0)))
            .unwrap();
        world
            .set_property(
                wall,
                "Position",
                Value::Vector3(Vector3::new(0.0, 1.0, -4.0)),
            )
            .unwrap();
        let crate_id = world.create(ClassName::Part).unwrap();
        world.set_parent(crate_id, Some(world.workspace())).unwrap();
        world
            .set_property(
                crate_id,
                "Position",
                Value::Vector3(Vector3::new(0.0, 10.0, 20.0)),
            )
            .unwrap();
        world
            .set_property(
                crate_id,
                "Size",
                Value::Vector3(Vector3::new(2.0, 2.0, 2.0)),
            )
            .unwrap();

        let mut controllers = Controllers::default();
        controllers.step(&mut world, &assets, forward(), 1.0);
        let stopped = pos(&world, body);
        // The wall's near face is at z = -3.5. The body is 2 studs deep, so
        // its center stops short of that.
        assert!(stopped.z < -1.0 && stopped.z > -3.2, "{stopped:?}");
        assert_eq!(orientation(&world, body), Vector3::new(0.0, 0.0, 0.0));

        controllers.keep_kinematic(&mut world);
        assert_eq!(
            world
                .get(body)
                .unwrap()
                .property("Anchored")
                .and_then(Value::as_bool),
            Some(true)
        );
        let mut physics = Physics::with_assets(assets);
        // One call runs at most ten fixed steps, so repeat until a second
        // of falling has been simulated.
        for _ in 0..6 {
            physics.step(&mut world, 0.5);
        }
        let landed = pos(&world, crate_id);
        assert!(landed.y < 5.0, "{landed:?}");
        assert!((pos(&world, body).y - stopped.y).abs() < 1e-3);
        assert_eq!(axcenity_physics::GRAVITY, 196.2);
    }

    #[test]
    fn walk_speed_does_not_change_how_a_crate_falls() {
        fn fallen(walk_speed: f64) -> Vector3 {
            let assets = Assets::default();
            let (mut world, controller, _) = standing();
            world
                .set_property(controller, "WalkSpeed", Value::Number(walk_speed))
                .unwrap();
            let crate_id = world.create(ClassName::Part).unwrap();
            world.set_parent(crate_id, Some(world.workspace())).unwrap();
            world
                .set_property(
                    crate_id,
                    "Position",
                    Value::Vector3(Vector3::new(0.0, 10.0, 20.0)),
                )
                .unwrap();
            let mut controllers = Controllers::default();
            controllers.step(&mut world, &assets, MoveInput::default(), 1.0 / 60.0);
            controllers.keep_kinematic(&mut world);
            let mut physics = Physics::with_assets(assets);
            physics.step(&mut world, 0.5);
            pos(&world, crate_id)
        }
        assert_eq!(fallen(16.0), fallen(200.0));
    }
}
