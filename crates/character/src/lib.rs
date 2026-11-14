//! Kinematic character controllers.
//!
//! A [`ThirdPersonController`](axcenity_core::ClassName::ThirdPersonController)
//! or a [`FirstPersonController`](axcenity_core::ClassName::FirstPersonController)
//! is parented to a character [`Model`](axcenity_core::ClassName::Model) (or
//! to one Part). Both move the same way. During Play, [`Controllers::step`] walks or runs that
//! character from the held movement keys, jumps and crouches it, carries it
//! up slopes and steps within its limits, and drops it under gravity off an
//! edge. A custom `Movement` name stops all of that, so scripts and blocks
//! move the character instead. It is not
//! a dynamic body: [`Controllers::keep_kinematic`] anchors its parts so
//! physics does not tip or drop them, and so gravity and friction of other
//! parts stay as they are.
//!
//! The controller of the player's `Character`, or else the first such
//! controller in the Workspace that has a body, receives the input and
//! owns the camera: behind the character, zoomed with the mouse
//! wheel, for a third-person one, or at its eyes for a first-person one.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_assets::Assets;
use axcenity_core::input::{InputMap, InputState};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::{GRAVITY, QueryParams, RayHit, gravity, raycast};

mod predict;
pub mod rig;
mod seat;

pub use predict::{CharacterState, Drive};
pub use seat::SeatChange;

/// Gap left between the character and a wall or ceiling, in studs. Ground
/// probes also start this far above the surface they look for.
const SKIN: f32 = 0.05;
/// The longest slice of one step's horizontal move, in studs. A long step
/// goes in slices, so a slope or step on the way is met rather than jumped over.
const MAX_SLICE: f32 = 0.5;
const MAX_SLICES: f32 = 64.0;
/// Steeper than this, `MaxSlopeAngle` is treated as this: a wall is never ground.
const STEEPEST_SLOPE: f32 = 89.0;
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
/// The `Movement` of a dead character (Phase 46): a custom mode, so the
/// controller doesn't move it, and [`Controllers::keep_kinematic`] leaves
/// its parts to physics.
pub const DEAD: &str = "Dead";
/// The lowest a crouch squashes the character, in studs.
const MIN_CROUCH_HEIGHT: f32 = 0.5;

/// How fast a controller's right stick turns the camera, in pixels of mouse
/// movement a second when pushed all the way.
const STICK_LOOK_SPEED: f32 = 900.0;

/// Keys held for the character that owns the camera, and mouse look since
/// the last step. Right and down are positive, in pixels, as with the fly
/// camera. `jump` held on landing jumps again. `crouch` crouches the
/// character while it is held, as if `Crouch` were true. `run` runs a
/// walking character (`RunSpeed`) while it is held. `stick` is a
/// controller stick, right and forward positive, -1 to 1: pushed part way,
/// the character walks that much slower. `zoom` is mouse wheel notches
/// since the last step, toward the screen positive: it moves a third-person
/// camera closer. [`MoveInput::from_actions`] reads all of it from the
/// input map's actions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub stick: [f32; 2],
    pub jump: bool,
    pub crouch: bool,
    pub run: bool,
    pub look_dx: f32,
    pub look_dy: f32,
    pub zoom: f32,
}

impl MoveInput {
    /// What the input map's `Move`, `Look`, `Jump`, `Crouch` and `Run`
    /// actions say in `state`, `seconds` after the last frame. `look` is the
    /// mouse look in pixels, which the host decides (while the right button
    /// is held, or the cursor is locked); the `Look` axis's sticks add to it.
    pub fn from_actions(
        map: &InputMap,
        state: &InputState,
        look: [f32; 2],
        zoom: f32,
        seconds: f32,
    ) -> Self {
        let [x, y] = map.axis("Move", state);
        // The Look axis without the mouse, which `look` already has.
        let mut sticks = state.clone();
        sticks.mouse_delta = [0.0, 0.0];
        let [lx, ly] = map.axis("Look", &sticks);
        Self {
            stick: [x, y],
            jump: map.is_pressed("Jump", state),
            crouch: map.is_pressed("Crouch", state),
            run: map.is_pressed("Run", state),
            look_dx: look[0] + lx * STICK_LOOK_SPEED * seconds,
            look_dy: look[1] - ly * STICK_LOOK_SPEED * seconds,
            zoom,
            ..Self::default()
        }
    }
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
    /// True once it has been in the air for a slice: jumped, walked off an
    /// edge or fell without landing. Only such a fall fires `Landed`, so
    /// settling onto the ground when Play starts doesn't.
    airborne: bool,
    /// How fast it was falling when it last landed, until `step` takes it.
    landed: Option<f32>,
}

/// How a crouched character stood, to put back when it stands: each part's
/// height, and how far its middle was above the feet. Not saved.
struct Crouched {
    parts: Vec<(InstanceId, f32, f32)>,
    height: f32,
}

/// What a controller's `Movement` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Walk,
    Run,
    /// Walks, crouched.
    Crouch,
    /// Sits on a Seat (see `seat`). Only the controller sets it.
    Seated,
    /// Any other name: the controller doesn't move the character.
    Custom,
}

impl Mode {
    /// `None` for an empty name, which clears a custom mode. A value that
    /// isn't text walks.
    fn parse(value: Option<&Value>) -> Option<Self> {
        let Some(name) = value.and_then(Value::as_str) else {
            return Some(Mode::Walk);
        };
        let name = name.trim();
        if name.is_empty() {
            None
        } else if name.eq_ignore_ascii_case("walk") {
            Some(Mode::Walk)
        } else if name.eq_ignore_ascii_case("run") {
            Some(Mode::Run)
        } else if name.eq_ignore_ascii_case("crouch") {
            Some(Mode::Crouch)
        } else if name.eq_ignore_ascii_case("seated") {
            Some(Mode::Seated)
        } else {
            Some(Mode::Custom)
        }
    }
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
    /// The collision group of its first part: probes pass through parts in
    /// groups that don't collide with it.
    group: String,
    pivot_x: f32,
    pivot_z: f32,
    feet_y: f32,
    height: f32,
    radius: f32,
}

/// The part a character stands on and where that part was after the
/// character's last step, so the character rides along when it moves
/// (Phase 73: a platform on a SplineFollower, a tweened lift).
#[derive(Clone, Copy, Debug)]
struct Footing {
    part: InstanceId,
    position: Vector3,
    /// Its turn round Y (its `Orientation.Y`), in degrees.
    yaw: f32,
}

impl Footing {
    fn of(world: &World, part: InstanceId) -> Option<Self> {
        let instance = world.get(part)?;
        let position = instance.property("Position").and_then(Value::as_vector3)?;
        let yaw = instance
            .property("Orientation")
            .and_then(Value::as_vector3)
            .map_or(0.0, |orientation| orientation.y);
        finite(position).then_some(Self {
            part,
            position,
            yaw,
        })
    }
}

/// A part that moves further than this between two steps has been put
/// somewhere, not moved: what stood on it isn't carried there.
const MAX_CARRY: f32 = 20.0;

/// Steps every character controller. The player and the editor each own
/// one for the game they are running.
#[derive(Default)]
pub struct Controllers {
    /// What each grounded character stands on (Phase 73).
    footings: BTreeMap<InstanceId, Footing>,
    orbits: BTreeMap<InstanceId, Orbit>,
    falls: BTreeMap<InstanceId, Fall>,
    crouches: BTreeMap<InstanceId, Crouched>,
    /// The built-in `Movement` each controller last had, put back when a
    /// custom mode is cleared. Not saved.
    built_in: BTreeMap<InstanceId, String>,
    /// The controllers whose characters sit on a seat.
    seated: BTreeMap<InstanceId, seat::Seated>,
    seat_changes: Vec<SeatChange>,
    /// Controllers that landed, with how fast they were falling (Phase 43).
    landings: Vec<(InstanceId, f32)>,
    /// The controller that last received input, if it is still in the world.
    active: Option<InstanceId>,
    /// The turn of each controller's first part when it was first seen,
    /// facing -Z (radians), for `AutoRotate`.
    facing: BTreeMap<InstanceId, f32>,
    /// The Player on this device (multiplayer, Phase 66): only their
    /// character takes input. `None`: the first Player's, as before.
    player: Option<InstanceId>,
    /// Characters another device moves (other players' own), left alone.
    others: BTreeSet<InstanceId>,
    /// The Developer Console's `noclip` (Phase 71): the character that
    /// takes input flies through everything, Jump up and Crouch down.
    noclip: bool,
    /// The last step of the character that takes input, for prediction
    /// (MP6a, [`Controllers::take_drive`]).
    driven: Option<(InstanceId, Drive)>,
}

impl Controllers {
    /// In multiplayer (Phase 66): `player` is the one on this device, whose
    /// character alone takes input, and `others` are the characters other
    /// devices move, which these controllers leave alone.
    pub fn set_players(&mut self, player: Option<InstanceId>, others: BTreeSet<InstanceId>) {
        self.player = player;
        self.others = others;
    }

    /// Turns the console's `noclip` on or off: the character that takes
    /// input flies where it's steered, through walls and floors, with no
    /// gravity (Jump rises, Crouch sinks). Off, it falls from where it is.
    pub fn set_noclip(&mut self, on: bool) {
        self.noclip = on;
    }

    pub fn noclip(&self) -> bool {
        self.noclip
    }

    /// Moves the character that takes input so its feet are at `feet`
    /// (the console's `teleport`). False when there's none.
    pub fn teleport(&mut self, world: &mut World, feet: Vector3) -> bool {
        let Some(id) = self.active_of(world) else {
            return false;
        };
        let Some(mut body) = Body::read(world, id) else {
            return false;
        };
        body.shift(
            world,
            feet.x - body.pivot_x,
            feet.y - body.feet_y,
            feet.z - body.pivot_z,
        );
        // It falls from there, rather than carrying on a jump.
        self.falls.remove(&id);
        true
    }

    /// Whether another device moves the character `id` controls.
    pub(crate) fn moved_elsewhere(&self, world: &World, id: InstanceId) -> bool {
        !self.others.is_empty()
            && world
                .get(id)
                .and_then(|controller| controller.parent())
                .is_some_and(|character| self.others.contains(&character))
    }

    /// The controller that takes input: this device's player's character's
    /// when there's a player set, else the first Player's.
    fn active_of(&self, world: &World) -> Option<InstanceId> {
        match self.player {
            // Single-player: the player's character, else the first, as
            // before there were several devices.
            Some(player) if self.others.is_empty() => {
                controller_of_player(world, player).or_else(|| active_id(world))
            }
            Some(player) => controller_of_player(world, player),
            None if self.others.is_empty() => active_id(world),
            // A dedicated server: everyone's character is moved elsewhere.
            None => active_id(world).filter(|&id| !self.moved_elsewhere(world, id)),
        }
    }

    /// Moves the active character for `seconds` of time and updates its
    /// camera from `input`. Returns that camera, or `None` when no
    /// controller has a body. Other controllers walk their `MoveDirection`
    /// (Phase 46). Does not anchor; call [`Self::keep_kinematic`] after
    /// scripts and before physics.
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
        self.built_in.retain(|id, _| world.get(*id).is_some());
        self.facing.retain(|id, _| world.get(*id).is_some());
        self.footings.retain(|id, _| world.get(*id).is_some());
        let active = self.active_of(world);
        self.update_seats(world, assets, active, input.jump);
        // Controllers without input move once something asks them to,
        // and from then on keep falling and landing.
        for id in controllers(world) {
            if Some(id) != active
                && is_driver(world, id)
                && !self.moved_elsewhere(world, id)
                && self.wants_to_move(world, id)
            {
                self.move_character(world, assets, id, None, MoveInput::default(), dt);
            }
        }
        let Some(id) = active else {
            self.active = None;
            return None;
        };
        self.active = Some(id);
        let eye = Eye::of(world, id).unwrap_or(Eye::Follow);
        let turned = self.drive_seat(world, id, input).unwrap_or(0.0);
        let orbit = self.orbits.entry(id).or_insert_with(|| Orbit::start(eye));
        // A camera in a car turns with it (see `seat`).
        orbit.yaw += turned;
        orbit.look(input.look_dx, input.look_dy);
        let orbit = *orbit;
        if eye == Eye::Follow {
            zoom(world, id, input.zoom);
        }
        let view = Some((orbit.yaw, eye));
        self.move_character(world, assets, id, view, input, dt);
        self.view(world)
    }

    /// Whether a controller that doesn't receive input has been asked to
    /// move: a `MoveDirection` or a `Jump`, or it moved before.
    fn wants_to_move(&self, world: &World, id: InstanceId) -> bool {
        self.falls.contains_key(&id)
            || move_direction(world, id).is_some()
            || flag(world, id, "Jump")
    }

    /// One step of the character `id` drives: walk or run, jump and fall,
    /// crouch, and turn. `view` is the camera's yaw and kind for the
    /// controller that receives `input`; others walk their `MoveDirection`.
    fn move_character(
        &mut self,
        world: &mut World,
        assets: &Assets,
        id: InstanceId,
        view: Option<(f32, Eye)>,
        input: MoveInput,
        dt: f32,
    ) {
        if let Some((yaw, _)) = view {
            self.driven = Some((
                id,
                Drive {
                    input,
                    yaw,
                    seconds: dt,
                },
            ));
        }
        if self.seated.contains_key(&id) {
            // The seat holds it; the camera still turns and zooms.
            return;
        }
        if ragdolled(world, id) || flag(world, id, "PlatformStand") {
            // Its Ragdoll is on (Phase 41), or it's PlatformStanding: it
            // isn't moved, and lands wherever it is once let go.
            take_jump(world, id);
            self.falls.remove(&id);
            return;
        }
        if self.mode(world, id) == Mode::Custom {
            // Scripts and blocks move the character. A jump asked for now is
            // used up, and the built-in mode starts in the air, so it lands
            // wherever the character was left.
            take_jump(world, id);
            self.falls.remove(&id);
            return;
        }
        // Taken first, so a request made in the air is used up, not kept
        // for landing. A controller without input that's moving for the
        // first time settles first, and keeps the request for its next step.
        let settling = view.is_none() && !self.falls.contains_key(&id);
        let jump = (!settling && take_jump(world, id)) || input.jump;
        let crouch = wants_crouch(world, id) || input.crouch;
        // What it was driven with, scripts' jump and crouch included.
        if let Some((driven, drive)) = &mut self.driven
            && *driven == id
        {
            drive.input.jump = jump;
            drive.input.crouch = crouch;
        }
        set_crouch(world, assets, id, &mut self.crouches, crouch);
        let Some(mut body) = Body::read(world, id) else {
            return;
        };
        // Standing on something that moved since the last step, it rides
        // along first.
        if self.falls.get(&id).is_some_and(|fall| fall.grounded)
            && let Some(footing) = self.footings.get(&id)
        {
            carry(world, &mut body, footing);
        }
        let limits = Limits::read(world, id);
        let mut speed = speed_of(world, id, input.run);
        if self.crouches.contains_key(&id) {
            speed *= crouch_speed_scale(world, id);
        }
        let (mut dx, mut dz) = match view {
            Some((yaw, _)) => wish(yaw, input, speed * dt),
            None => (0.0, 0.0),
        };
        if dx == 0.0 && dz == 0.0 {
            // No key held: the way scripts point it.
            if let Some((x, z)) = move_direction(world, id) {
                (dx, dz) = (x * speed * dt, z * speed * dt);
            }
        }
        if flag(world, id, "AutoRotate") {
            let face = match view {
                Some((yaw, Eye::Eyes)) => Some(yaw),
                _ => heading(dx, dz).map(|(ux, uz)| (-ux).atan2(-uz)),
            };
            if let Some(face) = face
                && self.turn(world, &body, id, face)
                && let Some(turned) = Body::read(world, id)
            {
                body = turned;
            }
        }
        if self.noclip && view.is_some() {
            let rise = f32::from(u8::from(input.jump)) - f32::from(u8::from(input.crouch));
            body.shift(world, dx, rise * speed * dt, dz);
            self.falls.remove(&id);
            self.footings.remove(&id);
            return;
        }
        let fall = self.falls.entry(id).or_default();
        if jump && fall.grounded {
            // The same launch speed whatever the Workspace's Gravity:
            // JumpHeight is how high it reaches at the default, so in
            // lower gravity characters jump higher, as parts fly.
            fall.velocity = (2.0 * GRAVITY * limits.jump_height).sqrt();
            fall.grounded = false;
            fall.airborne = true;
        }
        let slices = (dx.hypot(dz) / MAX_SLICE).ceil().clamp(1.0, MAX_SLICES);
        for _ in 0..slices as usize {
            let (sx, sz) = (dx / slices, dz / slices);
            move_slice(world, assets, &mut body, &limits, fall, sx, sz, dt / slices);
        }
        if let Some(speed) = fall.landed.take() {
            self.landings.push((id, speed));
        }
        let footing = if fall.grounded {
            standing_on(world, assets, &body).and_then(|part| Footing::of(world, part))
        } else {
            None
        };
        match footing {
            Some(footing) => self.footings.insert(id, footing),
            None => self.footings.remove(&id),
        };
    }

    /// Turns the character `id` drives about the middle of its box, so it
    /// faces `yaw` (radians; 0 faces -Z). Where it faces now is how far its
    /// first part has turned since the controller first saw it, which was
    /// facing -Z. True if it turned.
    fn turn(&mut self, world: &mut World, body: &Body, id: InstanceId, yaw: f32) -> bool {
        let Some(&first) = body.parts.first() else {
            return false;
        };
        let Some(now) = world
            .get(first)
            .and_then(|part| part.property("Orientation"))
            .and_then(Value::as_vector3)
            .filter(|o| o.y.is_finite())
            .map(|o| o.y.to_radians())
        else {
            return false;
        };
        let base = *self.facing.entry(id).or_insert(now);
        let delta = wrap_angle(yaw - (now - base));
        if delta.abs() < 1e-4 {
            return false;
        }
        let (sin, cos) = delta.sin_cos();
        for &part in &body.parts {
            let Some(instance) = world.get(part) else {
                continue;
            };
            let (Some(position), Some(orientation)) = (
                instance.property("Position").and_then(Value::as_vector3),
                instance.property("Orientation").and_then(Value::as_vector3),
            ) else {
                continue;
            };
            let (x, z) = (position.x - body.pivot_x, position.z - body.pivot_z);
            let moved = Vector3::new(
                body.pivot_x + x * cos + z * sin,
                position.y,
                body.pivot_z - x * sin + z * cos,
            );
            let turned = Vector3::new(
                orientation.x,
                wrap_angle((orientation.y.to_radians()) + delta).to_degrees(),
                orientation.z,
            );
            let _ = world.set_property(part, "Position", Value::Vector3(moved));
            let _ = world.set_property(part, "Orientation", Value::Vector3(turned));
        }
        true
    }

    /// How fast the character `id` drives is rising (up positive), in studs
    /// a second, while it's in the air after a jump or off an edge; `None`
    /// on the ground, before its first step, or while it isn't moved.
    pub fn in_air(&self, id: InstanceId) -> Option<f32> {
        self.falls
            .get(&id)
            .filter(|fall| !fall.grounded && fall.airborne)
            .map(|fall| fall.velocity)
    }

    /// The controllers that landed since the last call, each with how fast
    /// it was falling in studs a second. Hosts fire `Landed` with
    /// `ScriptRuntime::fire_landed`, which also applies fall damage.
    pub fn take_landings(&mut self) -> Vec<(InstanceId, f32)> {
        std::mem::take(&mut self.landings)
    }

    /// The mode `Movement` names. Remembers a built-in one, and puts it back
    /// in `Movement` when a custom mode was cleared.
    fn mode(&mut self, world: &mut World, id: InstanceId) -> Mode {
        let value = world
            .get(id)
            .and_then(|instance| instance.property("Movement"))
            .cloned();
        match Mode::parse(value.as_ref()) {
            Some(Mode::Custom) => Mode::Custom,
            // "Seated" off a seat (stood up, or set by a script) goes back
            // to the built-in mode, as "" does.
            Some(mode) if mode != Mode::Seated => {
                if let Some(Value::String(name)) = value {
                    self.built_in.insert(id, name);
                }
                mode
            }
            _ => {
                let name = self
                    .built_in
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "Walk".to_owned());
                let mode = Mode::parse(Some(&Value::String(name.clone()))).unwrap_or(Mode::Walk);
                let _ = world.set_property(id, "Movement", Value::String(name));
                mode
            }
        }
    }

    /// The camera of the controller that owns it, from where the character
    /// is now. `None` when nothing has a body. Safe before the first
    /// [`step`](Self::step): the camera starts behind the character, or at
    /// its eyes.
    pub fn view(&self, world: &World) -> Option<CharacterCamera> {
        let id = match self.active {
            Some(id) if is_driver(world, id) => id,
            _ => self.active_of(world)?,
        };
        let eye = Eye::of(world, id)?;
        let orbit = self.orbits.get(&id).copied().unwrap_or(Orbit::start(eye));
        let standing = self.crouches.get(&id).map(|crouched| crouched.height);
        pose_camera(world, id, eye, orbit, standing)
    }

    /// The character (Model or part) whose controller the last
    /// [`step`](Self::step) moved with the player's input, without
    /// searching the world: `None` before the first step or when no
    /// controller has a body.
    pub fn stepped_character(&self, world: &World) -> Option<InstanceId> {
        world.get(self.active?)?.parent()
    }

    /// Anchors every part a character controller moves, so the physics
    /// step does not treat the character as a dynamic body. Parts that are
    /// already anchored are left alone. Other parts, gravity, and friction
    /// are not touched. A sitting character's parts stay unanchored, welded
    /// to its seat.
    pub fn keep_kinematic(&self, world: &mut World) {
        for id in controllers(world) {
            // A dead character (Phase 46) is left to physics.
            if self.seated.contains_key(&id) || is_dead(world, id) {
                continue;
            }
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

/// The ground a foot finds (`axcenity_anim::GroundProbe`): the first solid
/// part along a ray, passing by `skip`. Hosts give it to
/// `Animators::with_ground`.
pub fn foot_ground(
    world: &World,
    assets: &Assets,
    origin: Vector3,
    direction: Vector3,
    skip: &[InstanceId],
) -> Option<axcenity_anim::GroundHit> {
    let params = QueryParams {
        filter: skip.to_vec(),
        respect_can_collide: true,
        ..QueryParams::default()
    };
    let hit = raycast(world, assets, origin, direction, &params)?;
    Some(axcenity_anim::GroundHit {
        position: hit.position,
        normal: hit.normal,
        distance: hit.distance,
    })
}

/// Every part a controller in the Workspace moves. They are characters,
/// not ground, so the navigation mesh leaves them out.
pub fn character_parts(world: &World) -> Vec<InstanceId> {
    controllers(world)
        .into_iter()
        .flat_map(|id| parts_of(world, id))
        .collect()
}

fn controllers(world: &World) -> Vec<InstanceId> {
    world.all_below(world.workspace(), ClassName::is_character_controller)
}

fn is_driver(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .is_some_and(|instance| instance.class().is_character_controller())
        && !parts_of(world, id).is_empty()
}

/// The player character: the Model or part whose controller receives
/// movement and owns the camera. `None` when no controller has a body.
pub fn player_character(world: &World) -> Option<InstanceId> {
    world.get(active_id(world)?)?.parent()
}

/// The controller of `player`'s `Character`, if it has a body.
fn controller_of_player(world: &World, player: InstanceId) -> Option<InstanceId> {
    let character = world.get(player)?.property("Character")?.as_instance()?;
    controllers(world).into_iter().find(|&id| {
        is_driver(world, id) && world.get(id).and_then(|c| c.parent()) == Some(character)
    })
}

/// The controller of the first Player's `Character` (Phase 47), or else
/// the first controller in the Workspace whose parent is a character.
fn active_id(world: &World) -> Option<InstanceId> {
    let drivers: Vec<InstanceId> = controllers(world)
        .into_iter()
        .filter(|&id| is_driver(world, id))
        .collect();
    let characters = world.get(world.players()).into_iter().flat_map(|players| {
        players.children().iter().filter_map(|&player| {
            match world.get(player)?.property("Character")? {
                Value::Instance(character) => *character,
                _ => None,
            }
        })
    });
    for character in characters {
        let driver = drivers
            .iter()
            .copied()
            .find(|&id| world.get(id).and_then(|c| c.parent()) == Some(character));
        if driver.is_some() {
            return driver;
        }
    }
    drivers.first().copied()
}

/// The Part and MeshPart descendants of the controller's parent, plus the
/// parent when the controller is parented directly to a part.
fn parts_of(world: &World, controller: InstanceId) -> Vec<InstanceId> {
    match world.get(controller).and_then(|instance| instance.parent()) {
        Some(parent) => body_parts(world, parent),
        None => Vec::new(),
    }
}

/// Whether the character `controller` drives has a Ragdoll that's on.
fn ragdolled(world: &World, controller: InstanceId) -> bool {
    let Some(holder) = world.get(controller).and_then(|c| c.parent()) else {
        return false;
    };
    world.descendants_iter(holder).any(|id| {
        world.get(id).is_some_and(|ragdoll| {
            ragdoll.class() == ClassName::Ragdoll
                && ragdoll.property("Enabled").and_then(Value::as_bool) == Some(true)
        })
    })
}

/// True if `id` is inside a Ragdoll.
fn in_ragdoll(world: &World, id: InstanceId) -> bool {
    let mut at = world.get(id).and_then(|i| i.parent());
    while let Some(parent) = at {
        let Some(instance) = world.get(parent) else {
            return false;
        };
        if instance.class() == ClassName::Ragdoll {
            return true;
        }
        at = instance.parent();
    }
    false
}

/// The parts of the character `holder` is: a Model's Part and MeshPart
/// descendants, or a Part or MeshPart and the parts inside it, leaving out
/// a Ragdoll's bodies (Phase 41: they follow the animation, or physics)
/// and the Tools it carries (Phase 45: they follow the hand). Empty for
/// anything else. Navigation agents move their parent's body the same way.
pub fn body_parts(world: &World, holder: InstanceId) -> Vec<InstanceId> {
    let parent = holder;
    let Some(instance) = world.get(parent) else {
        return Vec::new();
    };
    let character = matches!(
        instance.class(),
        ClassName::Model
            | ClassName::Part
            | ClassName::MeshPart
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
    );
    if !character {
        return Vec::new();
    }
    let mut parts = Vec::new();
    if instance.class().is_base_part() {
        parts.push(parent);
    }
    let own_tool = axcenity_core::tool::tool_of(world, parent);
    for id in world.descendants_iter(parent) {
        if world
            .get(id)
            .is_some_and(|child| child.class().is_base_part())
            && !in_ragdoll(world, id)
            && axcenity_core::tool::tool_of(world, id) == own_tool
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
        let group = world
            .get(parts[0])
            .and_then(|part| part.property("CollisionGroup"))
            .and_then(Value::as_str)
            .unwrap_or(axcenity_core::DEFAULT_GROUP)
            .to_owned();
        Some(Self {
            parts,
            group,
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

fn movement(world: &World, id: InstanceId) -> Option<Mode> {
    Mode::parse(world.get(id)?.property("Movement"))
}

/// `RunSpeed` when `Movement` is Run, or when `run` is held while it walks;
/// otherwise `WalkSpeed`.
fn speed_of(world: &World, id: InstanceId, run: bool) -> f32 {
    let mode = movement(world, id);
    let name = if mode == Some(Mode::Run) || (run && mode != Some(Mode::Crouch)) {
        "RunSpeed"
    } else {
        "WalkSpeed"
    };
    number(world, id, name).unwrap_or(0.0).max(0.0)
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        == Some(true)
}

/// `MoveDirection` across the ground, at most 1 long, or `None` when it's
/// (nearly) zero.
fn move_direction(world: &World, id: InstanceId) -> Option<(f32, f32)> {
    let direction = world.get(id)?.property("MoveDirection")?.as_vector3()?;
    let (x, z) = (direction.x, direction.z);
    let length = x.hypot(z);
    if !length.is_finite() || length < 1e-4 {
        return None;
    }
    let scale = 1.0 / length.max(1.0);
    Some((x * scale, z * scale))
}

/// Whether `Movement` is [`DEAD`], which the Humanoid sets as its
/// character dies.
fn is_dead(world: &World, id: InstanceId) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property("Movement"))
        .and_then(Value::as_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(DEAD))
}

/// An angle in radians, from -π to π.
fn wrap_angle(angle: f32) -> f32 {
    let turn = std::f32::consts::TAU;
    angle - turn * ((angle + std::f32::consts::PI) / turn).floor()
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

/// True when `Crouch` is set or `Movement` is `"Crouch"`.
fn wants_crouch(world: &World, id: InstanceId) -> bool {
    let crouch = world
        .get(id)
        .and_then(|instance| instance.property("Crouch"))
        .and_then(Value::as_bool)
        == Some(true);
    crouch || movement(world, id) == Some(Mode::Crouch)
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

/// Horizontal movement of up to `distance` studs, relative to the camera's
/// yaw: all of it for keys, and a stick's share of it. Yaw 0 moves toward
/// -Z, the same way the fly camera looks at yaw 0.
fn wish(yaw: f32, input: MoveInput, distance: f32) -> (f32, f32) {
    let axis = |positive: bool, negative: bool| f32::from(positive) - f32::from(negative);
    let forward_amount = axis(input.forward, input.back) + input.stick[1];
    let right_amount = axis(input.right, input.left) + input.stick[0];
    let (sy, cy) = yaw.sin_cos();
    let x = -sy * forward_amount + cy * right_amount;
    let z = -cy * forward_amount - sy * right_amount;
    let len = x.hypot(z);
    if len < 1e-6 || distance == 0.0 {
        return (0.0, 0.0);
    }
    let scale = distance / len.max(1.0);
    (x * scale, z * scale)
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
                fall.airborne = true;
            }
        }
    }
    fly(world, assets, body, fall, ahead, limits.step, dt);
}

/// The first solid hit along a ray, passing through sensors (`CanCollide`
/// off), the character's own body and parts in collision groups that don't
/// collide with it.
fn solid_hit(
    world: &World,
    assets: &Assets,
    body: &Body,
    origin: Vector3,
    direction: Vector3,
) -> Option<RayHit> {
    let params = QueryParams {
        filter: body.parts.clone(),
        collision_group: Some(body.group.clone()),
        respect_can_collide: true,
        ..QueryParams::default()
    };
    raycast(world, assets, origin, direction, &params)
}

/// The first solid under a falling body's foot at `origin`. A foot
/// already inside a part lands on its top when that's no more than `step`
/// above (it fell past a ledge's side and drifted into it), and otherwise
/// passes it by (another player's character it was pushed into on this
/// device). Before, the probe stopped there, found no ground, and the
/// character fell through the floor (MP3b's bots).
fn landing_hit(
    world: &World,
    assets: &Assets,
    body: &Body,
    origin: Vector3,
    direction: Vector3,
    step: f32,
) -> Option<RayHit> {
    let mut params = QueryParams {
        filter: body.parts.clone(),
        collision_group: Some(body.group.clone()),
        respect_can_collide: true,
        ..QueryParams::default()
    };
    for _ in 0..4 {
        let hit = raycast(world, assets, origin, direction, &params)?;
        if hit.distance > 0.0 {
            return Some(hit);
        }
        let only = QueryParams {
            filter: vec![hit.instance],
            include: true,
            ..QueryParams::default()
        };
        let above = Vector3::new(origin.x, origin.y + step, origin.z);
        if let Some(top) = raycast(world, assets, above, Vector3::new(0.0, -step, 0.0), &only)
            && top.distance > 0.0
        {
            return Some(top);
        }
        params.filter.push(hit.instance);
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
    step: f32,
    dt: f32,
) {
    let start = fall.velocity;
    fall.velocity -= gravity(world) * dt;
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
            if let Some(hit) = landing_hit(world, assets, body, origin, direction, step) {
                let to = hit.position.y - body.feet_y;
                landing = Some(landing.map_or(to, |landing| landing.max(to)));
            }
        }
        match landing {
            Some(to) => {
                body.shift(world, 0.0, to, 0.0);
                if fall.airborne {
                    fall.landed = Some(-fall.velocity);
                }
                fall.grounded = true;
                fall.airborne = false;
                fall.velocity = 0.0;
            }
            None => {
                body.shift(world, 0.0, dy, 0.0);
                fall.airborne = true;
            }
        }
    }
}

/// The anchored part right under the middle of the body's feet, if any:
/// what a script, a tween or a SplineFollower moves. A part physics moves
/// isn't ridden: the character, held still for physics, would press into
/// it as it follows it down, and push it further.
fn standing_on(world: &World, assets: &Assets, body: &Body) -> Option<InstanceId> {
    let origin = Vector3::new(body.pivot_x, body.feet_y + SKIN, body.pivot_z);
    let direction = Vector3::new(0.0, -(4.0 * SKIN + 0.1), 0.0);
    let part = solid_hit(world, assets, body, origin, direction)?.instance;
    flag(world, part, "Anchored").then_some(part)
}

/// Moves the body as far as the part under it moved (and round it, as far
/// as it turned) since `footing` was taken.
fn carry(world: &mut World, body: &mut Body, footing: &Footing) {
    let Some(now) = Footing::of(world, footing.part) else {
        return;
    };
    let turn = (now.yaw - footing.yaw).to_radians();
    let (sin, cos) = turn.sin_cos();
    // Where the feet were from the part, turned with it.
    let x = body.pivot_x - footing.position.x;
    let y = body.feet_y - footing.position.y;
    let z = body.pivot_z - footing.position.z;
    let to = Vector3::new(
        now.position.x + x * cos + z * sin,
        now.position.y + y,
        now.position.z - x * sin + z * cos,
    );
    let (dx, dy, dz) = (to.x - body.pivot_x, to.y - body.feet_y, to.z - body.pivot_z);
    if (dx * dx + dy * dy + dz * dz).sqrt() > MAX_CARRY {
        return;
    }
    body.shift(world, dx, dy, dz);
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
