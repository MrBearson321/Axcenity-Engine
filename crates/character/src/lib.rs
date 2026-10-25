//! Kinematic character controllers.
//!
//! A [`ThirdPersonController`](axcenity_core::ClassName::ThirdPersonController)
//! or a [`FirstPersonController`](axcenity_core::ClassName::FirstPersonController)
//! is parented to a character [`Model`](axcenity_core::ClassName::Model) (or
//! to one Part). Both move the same way. During Play, [`Controllers::step`] walks or runs that
//! character from the held movement keys, jumps and crouches it, carries it
//! up slopes and steps within its limits, and drops it under gravity off an
//! edge. It is not
//! a dynamic body: [`Controllers::keep_kinematic`] anchors its parts so
//! physics does not tip or drop them, and so gravity and friction of other
//! parts stay as they are.
//!
//! The first such controller in the Workspace that has a body receives the
//! input and owns the camera: behind the character, zoomed with the mouse
//! wheel, for a third-person one, or at its eyes for a first-person one.

use std::collections::BTreeMap;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::{GRAVITY, RayHit, cast_ray_excluding};

/// Gap left between the character and a wall or ceiling, in studs. Ground
/// probes also start this far above the surface they look for.
const SKIN: f32 = 0.05;
/// The longest slice of one step's horizontal move, in studs. A long step
/// goes in slices, so a slope or step on the way is met rather than jumped over.
const MAX_SLICE: f32 = 0.5;
const MAX_SLICES: f32 = 64.0;
/// Steeper than this, `MaxSlopeAngle` is treated as this: a wall is never ground.
const STEEPEST_SLOPE: f32 = 89.0;
/// How many sensors (`CanCollide` off) one probe passes through before giving up.
const MAX_SENSORS: usize = 8;
/// Radians turned per pixel of mouse movement. The same as the fly camera.
const MOUSE_SENSITIVITY: f32 = 0.003;
const MAX_PITCH: f32 = 89.0_f32.to_radians();
/// Looks slightly down, so the camera sits above the character as well as behind.
const DEFAULT_PITCH: f32 = -18.0_f32.to_radians();
/// The closest a third-person camera comes, whatever `CameraMinZoomDistance` says.
const MIN_CAMERA_DISTANCE: f32 = 0.5;
/// One wheel notch zooms a fifth of the distance, and at least a stud, so
/// zooming feels the same near the character and far from it.
const ZOOM_PER_NOTCH: f32 = 0.2;
const MIN_ZOOM_STEP: f32 = 1.0;
/// The lowest a crouch squashes the character, in studs.
const MIN_CROUCH_HEIGHT: f32 = 0.5;

/// Keys held for the character that owns the camera, and mouse look since
/// the last step. Right and down are positive, in pixels, as with the fly
/// camera. `jump` is Space: held on landing, it jumps again. `crouch` is C:
/// the character crouches while it is held, as if `Crouch` were true.
/// `zoom` is mouse wheel notches since the last step, toward the screen
/// positive: it moves a third-person camera closer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub crouch: bool,
    pub look_dx: f32,
    pub look_dy: f32,
    pub zoom: f32,
}

/// Where the character camera is, in the same terms as `FlyCamera::view`:
/// position in studs, and orientation in degrees (X looks up and down, Y
/// turns, Z is zero).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterCamera {
    pub position: Vector3,
    pub orientation: Vector3,
}

/// Where a controller's camera is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eye {
    /// Behind the character: a ThirdPersonController.
    Follow,
    /// At its eyes: a FirstPersonController.
    Eyes,
}

impl Eye {
    fn of(world: &World, id: InstanceId) -> Option<Self> {
        match world.get(id)?.class() {
            ClassName::ThirdPersonController => Some(Eye::Follow),
            ClassName::FirstPersonController => Some(Eye::Eyes),
            _ => None,
        }
    }
}

/// Yaw and pitch of one controller's camera. Not saved; Play starts behind
/// the character looking slightly down, or at its eyes looking ahead.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    yaw: f32,
    pitch: f32,
}

impl Orbit {
    fn start(eye: Eye) -> Self {
        let pitch = match eye {
            Eye::Follow => DEFAULT_PITCH,
            Eye::Eyes => 0.0,
        };
        Self { yaw: 0.0, pitch }
    }

    fn look(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw -= dx * MOUSE_SENSITIVITY;
        self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }
}

/// One controller's vertical motion. Not saved; Play starts in the air, and
/// the first step lands the character if it stands on something.
#[derive(Clone, Copy, Debug, Default)]
struct Fall {
    /// Studs a second, up positive.
    velocity: f32,
    grounded: bool,
}

/// How a crouched character stood, to put back when it stands: each part's
/// height, and how far its middle was above the feet. Not saved.
struct Crouched {
    parts: Vec<(InstanceId, f32, f32)>,
    height: f32,
}

/// What one controller can cross, read from its properties each step.
struct Limits {
    jump_height: f32,
    /// Cosine of `MaxSlopeAngle`: ground whose normal points at least this
    /// far up is walkable.
    slope_cos: f32,
    slope_tan: f32,
    step: f32,
}

impl Limits {
    fn read(world: &World, id: InstanceId) -> Self {
        let slope = number(world, id, "MaxSlopeAngle")
            .unwrap_or(45.0)
            .clamp(0.0, STEEPEST_SLOPE)
            .to_radians();
        Self {
            jump_height: number(world, id, "JumpHeight").unwrap_or(7.2).max(0.0),
            slope_cos: slope.cos(),
            slope_tan: slope.tan(),
            step: number(world, id, "MaxStepHeight").unwrap_or(1.0).max(0.0),
        }
    }

    fn walkable(&self, normal: Vector3) -> bool {
        normal.y >= self.slope_cos - 1e-4
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

/// Steps every character controller. The player and the editor each own
/// one for the game they are running.
#[derive(Default)]
pub struct Controllers {
    orbits: BTreeMap<InstanceId, Orbit>,
    falls: BTreeMap<InstanceId, Fall>,
    crouches: BTreeMap<InstanceId, Crouched>,
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
        self.falls.retain(|id, _| world.get(*id).is_some());
        self.crouches.retain(|id, _| world.get(*id).is_some());
        let Some(id) = active_id(world) else {
            self.active = None;
            return None;
        };
        self.active = Some(id);
        let eye = Eye::of(world, id).unwrap_or(Eye::Follow);
        let orbit = self.orbits.entry(id).or_insert_with(|| Orbit::start(eye));
        orbit.look(input.look_dx, input.look_dy);
        let orbit = *orbit;
        if eye == Eye::Follow {
            zoom(world, id, input.zoom);
        }
        // Taken first, so a request made in the air is used up, not kept for landing.
        let jump = take_jump(world, id) || input.jump;
        let crouch = wants_crouch(world, id) || input.crouch;
        set_crouch(world, assets, id, &mut self.crouches, crouch);
        if let Some(mut body) = Body::read(world, id) {
            let limits = Limits::read(world, id);
            let fall = self.falls.entry(id).or_default();
            if jump && fall.grounded {
                fall.velocity = (2.0 * GRAVITY * limits.jump_height).sqrt();
                fall.grounded = false;
            }
            let mut speed = speed_of(world, id);
            if self.crouches.contains_key(&id) {
                speed *= crouch_speed_scale(world, id);
            }
            let (dx, dz) = wish(orbit.yaw, input, speed * dt);
            let slices = (dx.hypot(dz) / MAX_SLICE).ceil().clamp(1.0, MAX_SLICES);
            for _ in 0..slices as usize {
                let (sx, sz) = (dx / slices, dz / slices);
                move_slice(world, assets, &mut body, &limits, fall, sx, sz, dt / slices);
            }
        }
        self.view(world)
    }

    /// The camera of the controller that owns it, from where the character
    /// is now. `None` when nothing has a body. Safe before the first
    /// [`step`](Self::step): the camera starts behind the character, or at
    /// its eyes.
    pub fn view(&self, world: &World) -> Option<CharacterCamera> {
        let id = match self.active {
            Some(id) if is_driver(world, id) => id,
            _ => active_id(world)?,
        };
        let eye = Eye::of(world, id)?;
        let orbit = self.orbits.get(&id).copied().unwrap_or(Orbit::start(eye));
        let standing = self.crouches.get(&id).map(|crouched| crouched.height);
        pose_camera(world, id, eye, orbit, standing)
    }

    /// Anchors every part a character controller moves, so the physics
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
                .is_some_and(|instance| instance.class().is_character_controller())
        })
        .collect()
}

fn is_driver(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class().is_character_controller())
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

    /// Moves every part, and the box with them.
    fn shift(&mut self, world: &mut World, dx: f32, dy: f32, dz: f32) {
        if dx == 0.0 && dy == 0.0 && dz == 0.0 {
            return;
        }
        translate(world, &self.parts, dx, dy, dz);
        self.pivot_x += dx;
        self.feet_y += dy;
        self.pivot_z += dz;
    }

    /// Where ground is looked for: under the middle and, when moving, under
    /// the leading edge, so the character steps up as its front reaches a
    /// step and stays up until its middle has left an edge.
    fn ground_points(&self, ahead: Option<(f32, f32)>) -> Vec<(f32, f32)> {
        let mut points = vec![(self.pivot_x, self.pivot_z)];
        if let Some((ux, uz)) = ahead {
            let reach = self.radius - SKIN;
            points.push((self.pivot_x + ux * reach, self.pivot_z + uz * reach));
        }
        points
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

/// True when a script or block set `Jump`. Sets it back to false.
fn take_jump(world: &mut World, id: InstanceId) -> bool {
    let requested = world
        .get(id)
        .and_then(|instance| instance.property("Jump"))
        .and_then(Value::as_bool)
        == Some(true);
    if requested {
        let _ = world.set_property(id, "Jump", Value::Bool(false));
    }
    requested
}

fn wants_crouch(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property("Crouch"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn crouch_speed_scale(world: &World, id: InstanceId) -> f32 {
    number(world, id, "CrouchSpeedScale")
        .unwrap_or(0.5)
        .clamp(0.0, 1.0)
}

/// Crouches the character, or stands it back up, to match `wanted`. A
/// crouch squashes every part toward the feet until the body is
/// `CrouchHeight` tall. Standing waits while something overhead would cut
/// into the standing height.
fn set_crouch(
    world: &mut World,
    assets: &Assets,
    id: InstanceId,
    crouches: &mut BTreeMap<InstanceId, Crouched>,
    wanted: bool,
) {
    let crouched = crouches.contains_key(&id);
    if wanted == crouched {
        return;
    }
    let Some(body) = Body::read(world, id) else {
        crouches.remove(&id);
        return;
    };
    if wanted {
        let target = number(world, id, "CrouchHeight")
            .unwrap_or(3.0)
            .max(MIN_CROUCH_HEIGHT)
            .min(body.height);
        let ratio = target / body.height;
        let mut parts = Vec::new();
        for &part in &body.parts {
            let Some((position, size)) = placement(world, part) else {
                continue;
            };
            let above = position.y - body.feet_y;
            parts.push((part, size.y, above));
            place(world, part, body.feet_y + above * ratio, size.y * ratio);
        }
        crouches.insert(
            id,
            Crouched {
                parts,
                height: body.height,
            },
        );
        return;
    }
    let Some(standing) = crouches.get(&id) else {
        return;
    };
    let rise = standing.height - body.height;
    if rise > 0.0 && !room_to_stand(world, assets, &body, rise) {
        return;
    }
    for &(part, height, above) in &standing.parts {
        place(world, part, body.feet_y + above, height);
    }
    crouches.remove(&id);
}

/// A part's position and size, when both are finite.
fn placement(world: &World, part: InstanceId) -> Option<(Vector3, Vector3)> {
    let instance = world.get(part)?;
    let position = instance.property("Position")?.as_vector3()?;
    let size = instance.property("Size")?.as_vector3()?;
    (finite(position) && finite(size)).then_some((position, size))
}

/// Sets a part's height and the height of its middle, keeping the rest.
fn place(world: &mut World, part: InstanceId, y: f32, height: f32) {
    let Some((position, size)) = placement(world, part) else {
        return;
    };
    let position = Vector3::new(position.x, y, position.z);
    let size = Vector3::new(size.x, height, size.z);
    let _ = world.set_property(part, "Size", Value::Vector3(size));
    let _ = world.set_property(part, "Position", Value::Vector3(position));
}

/// True when nothing solid is within `rise` studs above the top of the
/// body, under its middle or the edges of its footprint.
fn room_to_stand(world: &World, assets: &Assets, body: &Body, rise: f32) -> bool {
    let reach = body.radius - SKIN;
    let top = body.feet_y + body.height - SKIN;
    let offsets = [
        (0.0, 0.0),
        (reach, 0.0),
        (-reach, 0.0),
        (0.0, reach),
        (0.0, -reach),
    ];
    offsets.into_iter().all(|(ox, oz)| {
        let origin = Vector3::new(body.pivot_x + ox, top, body.pivot_z + oz);
        let direction = Vector3::new(0.0, rise + 2.0 * SKIN, 0.0);
        solid_hit(world, assets, body, origin, direction).is_none()
    })
}

/// How close and how far a third-person camera may be.
fn zoom_limits(world: &World, id: InstanceId) -> (f32, f32) {
    let min = number(world, id, "CameraMinZoomDistance")
        .unwrap_or(0.5)
        .max(MIN_CAMERA_DISTANCE);
    let max = number(world, id, "CameraMaxZoomDistance")
        .unwrap_or(128.0)
        .max(min);
    (min, max)
}

fn camera_distance(world: &World, id: InstanceId) -> f32 {
    let (min, max) = zoom_limits(world, id);
    number(world, id, "CameraDistance")
        .unwrap_or(12.0)
        .clamp(min, max)
}

/// Moves a third-person camera `notches` wheel notches closer (out when
/// negative), within its zoom limits, and keeps the distance in
/// `CameraDistance`, so a script reads the zoom and can set it.
fn zoom(world: &mut World, id: InstanceId, notches: f32) {
    if notches == 0.0 || !notches.is_finite() {
        return;
    }
    let distance = camera_distance(world, id);
    let (min, max) = zoom_limits(world, id);
    let step = (distance * ZOOM_PER_NOTCH).max(MIN_ZOOM_STEP);
    let zoomed = (distance - notches * step).clamp(min, max);
    let _ = world.set_property(id, "CameraDistance", Value::Number(f64::from(zoomed)));
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

/// The unit direction of a move, or `None` for no move.
fn heading(dx: f32, dz: f32) -> Option<(f32, f32)> {
    let len = dx.hypot(dz);
    (len >= 1e-5).then(|| (dx / len, dz / len))
}

/// One slice of a step: across, then up or down.
#[allow(clippy::too_many_arguments)]
fn move_slice(
    world: &mut World,
    assets: &Assets,
    body: &mut Body,
    limits: &Limits,
    fall: &mut Fall,
    dx: f32,
    dz: f32,
    dt: f32,
) {
    let (dx, dz) = walls(world, assets, body, limits, dx, dz);
    body.shift(world, dx, 0.0, dz);
    let ahead = heading(dx, dz);
    if fall.grounded {
        match ground(world, assets, body, limits, ahead, dx.hypot(dz)) {
            Some(dy) => {
                body.shift(world, 0.0, dy, 0.0);
                return;
            }
            // Walked off an edge.
            None => {
                fall.grounded = false;
                fall.velocity = 0.0;
            }
        }
    }
    fly(world, assets, body, fall, ahead, dt);
}

/// The first solid hit along a ray, passing through sensors (`CanCollide`
/// off) and the character's own body.
fn solid_hit(
    world: &World,
    assets: &Assets,
    body: &Body,
    origin: Vector3,
    direction: Vector3,
) -> Option<RayHit> {
    let mut skip = body.parts.clone();
    for _ in 0..MAX_SENSORS {
        let hit = cast_ray_excluding(world, assets, origin, direction, &skip)?;
        if hit.can_collide {
            return Some(hit);
        }
        skip.push(hit.instance);
    }
    None
}

/// A horizontal ray from the middle of the body at height `y`, reaching the
/// move plus the body's radius.
fn ray_across(
    world: &World,
    assets: &Assets,
    body: &Body,
    y: f32,
    dx: f32,
    dz: f32,
) -> Option<RayHit> {
    let len = dx.hypot(dz);
    let reach = len + body.radius;
    let origin = Vector3::new(body.pivot_x, y, body.pivot_z);
    let direction = Vector3::new(dx / len * reach, 0.0, dz / len * reach);
    solid_hit(world, assets, body, origin, direction)
}

/// Heights above the step limit where anything that isn't walkable ground
/// is a wall: just above a step, the middle, and just under the top.
fn wall_heights(body: &Body, limits: &Limits) -> Vec<f32> {
    let lowest = body.feet_y + limits.step + 2.0 * SKIN;
    let top = body.feet_y + body.height - SKIN;
    let mut heights: Vec<f32> = Vec::new();
    for y in [lowest, body.feet_y + body.height * 0.5, top] {
        if y >= lowest && y <= top && heights.last().is_none_or(|last| y - last > SKIN) {
            heights.push(y);
        }
    }
    heights
}

/// The move left after walls and slopes too steep to climb, which stop the
/// character and let it slide along them. A low face it can stand on top
/// of is a step, and does not stop it.
fn walls(
    world: &World,
    assets: &Assets,
    body: &Body,
    limits: &Limits,
    dx: f32,
    dz: f32,
) -> (f32, f32) {
    let (mut dx, mut dz) = (dx, dz);
    for y in wall_heights(body, limits) {
        if heading(dx, dz).is_none() {
            return (dx, dz);
        }
        if let Some(hit) = ray_across(world, assets, body, y, dx, dz)
            && !limits.walkable(hit.normal)
        {
            (dx, dz) = slide(dx, dz, dx.hypot(dz), body.radius, hit);
        }
    }
    let Some(ahead) = heading(dx, dz) else {
        return (dx, dz);
    };
    if let Some(hit) = ray_across(world, assets, body, body.feet_y + SKIN, dx, dz)
        && !limits.walkable(hit.normal)
        && !can_step(world, assets, body, limits, hit, ahead)
    {
        (dx, dz) = slide(dx, dz, dx.hypot(dz), body.radius, hit);
    }
    (dx, dz)
}

/// True when the low face that `hit` found has walkable ground on top of
/// it, no higher above the feet than the step limit.
fn can_step(
    world: &World,
    assets: &Assets,
    body: &Body,
    limits: &Limits,
    hit: RayHit,
    (ux, uz): (f32, f32),
) -> bool {
    let up = limits.step + SKIN;
    // Just past the face, looking down for its top.
    let origin = Vector3::new(
        hit.position.x + ux * 2.0 * SKIN,
        body.feet_y + up,
        hit.position.z + uz * 2.0 * SKIN,
    );
    let Some(top) = solid_hit(world, assets, body, origin, Vector3::new(0.0, -up, 0.0)) else {
        return false;
    };
    // A ray that starts inside something has found a face taller than a step.
    top.distance > 0.0 && limits.walkable(top.normal) && top.position.y - body.feet_y <= limits.step
}

fn slide(dx: f32, dz: f32, len: f32, radius: f32, hit: RayHit) -> (f32, f32) {
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

/// How far to move the feet up (positive) or down to stay on the ground
/// after moving `moved` studs across, or `None` when there's no ground in
/// reach: the character walked off an edge.
fn ground(
    world: &World,
    assets: &Assets,
    body: &Body,
    limits: &Limits,
    ahead: Option<(f32, f32)>,
    moved: f32,
) -> Option<f32> {
    let up = limits.step + SKIN;
    // Far enough down to follow the steepest walkable slope, and to step
    // down a ledge no taller than a step.
    let down = limits.step.max(moved * limits.slope_tan) + SKIN;
    let mut best: Option<f32> = None;
    for (x, z) in body.ground_points(ahead) {
        let origin = Vector3::new(x, body.feet_y + up, z);
        let direction = Vector3::new(0.0, -(up + down), 0.0);
        let Some(hit) = solid_hit(world, assets, body, origin, direction) else {
            continue;
        };
        // Starting inside something means a ledge taller than a step.
        if hit.distance <= 0.0 {
            continue;
        }
        let dy = hit.position.y - body.feet_y;
        // Moving onto ground steeper than the limit is refused. Ground the
        // feet are already on is kept, so a character that landed on a
        // steep slope stands still.
        if dy > limits.step || (dy.abs() > SKIN && !limits.walkable(hit.normal)) {
            continue;
        }
        best = Some(best.map_or(dy, |best| best.max(dy)));
    }
    best
}

/// Rises or falls under gravity for `dt` seconds, stopping under a ceiling
/// and landing on whatever solid is below.
fn fly(
    world: &mut World,
    assets: &Assets,
    body: &mut Body,
    fall: &mut Fall,
    ahead: Option<(f32, f32)>,
    dt: f32,
) {
    let start = fall.velocity;
    fall.velocity -= GRAVITY * dt;
    let dy = (start + fall.velocity) * 0.5 * dt;
    if dy > 0.0 {
        let origin = Vector3::new(body.pivot_x, body.feet_y + body.height - SKIN, body.pivot_z);
        let rise = match solid_hit(
            world,
            assets,
            body,
            origin,
            Vector3::new(0.0, dy + SKIN, 0.0),
        ) {
            Some(hit) => {
                fall.velocity = 0.0;
                (hit.distance - 2.0 * SKIN).clamp(0.0, dy)
            }
            None => dy,
        };
        body.shift(world, 0.0, rise, 0.0);
    } else if dy < 0.0 {
        let mut landing: Option<f32> = None;
        for (x, z) in body.ground_points(ahead) {
            let origin = Vector3::new(x, body.feet_y + SKIN, z);
            let direction = Vector3::new(0.0, dy - SKIN, 0.0);
            if let Some(hit) = solid_hit(world, assets, body, origin, direction)
                && hit.distance > 0.0
            {
                let to = hit.position.y - body.feet_y;
                landing = Some(landing.map_or(to, |landing| landing.max(to)));
            }
        }
        match landing {
            Some(to) => {
                body.shift(world, 0.0, to, 0.0);
                fall.grounded = true;
                fall.velocity = 0.0;
            }
            None => body.shift(world, 0.0, dy, 0.0),
        }
    }
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

/// `standing` is the standing height of a crouched character: the camera
/// drops as far as its top did. A first-person camera sits at the eyes,
/// above the middle of the body; a third-person one sits behind the point
/// it looks at.
fn pose_camera(
    world: &World,
    id: InstanceId,
    eye: Eye,
    orbit: Orbit,
    standing: Option<f32>,
) -> Option<CharacterCamera> {
    let body = Body::read(world, id)?;
    let drop = standing.map_or(0.0, |height| (height - body.height).max(0.0));
    let (sy, cy) = orbit.yaw.sin_cos();
    let (sp, cp) = orbit.pitch.sin_cos();
    let forward = (-sy * cp, sp, -cy * cp);
    let (height, distance) = match eye {
        Eye::Follow => (camera_height(world, id), camera_distance(world, id)),
        Eye::Eyes => (number(world, id, "EyeHeight").unwrap_or(4.5), 0.0),
    };
    let look_y = body.feet_y + height - drop;
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
mod tests;
