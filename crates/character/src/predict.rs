//! What prediction needs from the controllers (multiplayer readiness
//! MP6a, `docs/architecture/networking.md`): each step of the character
//! that takes input, as it was taken (a [`Drive`]), so the server can take
//! the same step from the player's input; a step driven from one; and a
//! character's [`CharacterState`], to read and to put back, so a device
//! can rewind its character to where the server had it and take its
//! steps since again.

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, Value, Vector3, World};

use crate::{Body, Controllers, Eye, Fall, MoveInput, set_crouch};

/// One step of a character as its device took it: the keys, the camera's
/// turn (radians) the keys walk by, and how long the step was. Mouse look
/// and zoom are the camera's, not the character's, and stay out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub input: MoveInput,
    pub yaw: f32,
    pub seconds: f32,
}

/// Where a character is and how it moves, as prediction compares and puts
/// it back: the middle of its feet, which way its first part faces
/// (`Orientation.Y`, degrees), how fast it rises (studs a second, up
/// positive), and whether it stands, has been in the air, and crouches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterState {
    pub feet: Vector3,
    pub facing: f32,
    pub rising: f32,
    pub grounded: bool,
    pub airborne: bool,
    pub crouched: bool,
}

impl CharacterState {
    /// Whether `other` is this, but for differences no one would see.
    pub fn matches(&self, other: &CharacterState) -> bool {
        let gap = Vector3::new(
            self.feet.x - other.feet.x,
            self.feet.y - other.feet.y,
            self.feet.z - other.feet.z,
        );
        let facing = crate::wrap_angle((self.facing - other.facing).to_radians()).abs();
        gap.x.abs() <= 0.01
            && gap.y.abs() <= 0.01
            && gap.z.abs() <= 0.01
            && (self.rising - other.rising).abs() <= 0.05
            && facing <= 0.01
            && self.grounded == other.grounded
            && self.crouched == other.crouched
    }
}

impl Controllers {
    /// The step [`step`](Self::step) took of the character that takes
    /// input, since the last call: its controller and how it was driven
    /// (the jump and crouch it used, scripts' included).
    pub fn take_drive(&mut self) -> Option<(InstanceId, Drive)> {
        self.driven.take()
    }

    /// One step of controller `id`'s character, driven as `drive` says,
    /// as if it took input: the server's step of a player's character
    /// from their device's input, and a device's step again after a
    /// rewind. Seats aren't sat on here, and landings are queued as
    /// [`step`](Self::step) queues them.
    pub fn drive(&mut self, world: &mut World, assets: &Assets, id: InstanceId, drive: Drive) {
        let Some(eye) = Eye::of(world, id) else {
            return;
        };
        let dt = if drive.seconds.is_finite() && drive.seconds > 0.0 {
            drive.seconds
        } else {
            0.0
        };
        let yaw = if drive.yaw.is_finite() {
            drive.yaw
        } else {
            0.0
        };
        let mut input = drive.input;
        input.look_dx = 0.0;
        input.look_dy = 0.0;
        input.zoom = 0.0;
        let [x, y] = input.stick;
        input.stick = if x.is_finite() && y.is_finite() {
            [x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)]
        } else {
            [0.0, 0.0]
        };
        // The Developer Console's noclip is this device's own character's,
        // never a step taken for another device or taken again.
        let noclip = std::mem::take(&mut self.noclip);
        self.move_character(world, assets, id, Some((yaw, eye)), input, dt);
        self.noclip = noclip;
        self.driven = None;
    }

    /// [`drive`](Self::drive) again for a step already taken once: the
    /// landing it may cause was heard the first time.
    pub fn drive_again(
        &mut self,
        world: &mut World,
        assets: &Assets,
        id: InstanceId,
        drive: Drive,
    ) {
        let landings = self.landings.len();
        self.drive(world, assets, id, drive);
        self.landings.truncate(landings);
    }

    /// Controller `id`'s character as it is now; `None` without a body.
    pub fn state(&self, world: &World, id: InstanceId) -> Option<CharacterState> {
        let body = Body::read(world, id)?;
        let fall = self.falls.get(&id).copied().unwrap_or_default();
        Some(CharacterState {
            feet: Vector3::new(body.pivot_x, body.feet_y, body.pivot_z),
            facing: facing(world, &body).unwrap_or(0.0),
            rising: fall.velocity,
            grounded: fall.grounded,
            airborne: fall.airborne,
            crouched: self.crouches.contains_key(&id),
        })
    }

    /// Puts controller `id`'s character as `state` says: crouched or not,
    /// turned, moved, and rising or standing.
    pub fn set_state(
        &mut self,
        world: &mut World,
        assets: &Assets,
        id: InstanceId,
        state: &CharacterState,
    ) {
        set_crouch(world, assets, id, &mut self.crouches, state.crouched);
        let Some(mut body) = Body::read(world, id) else {
            return;
        };
        if let Some(now) = facing(world, &body)
            && state.facing.is_finite()
        {
            let base = *self.facing.entry(id).or_insert(now.to_radians());
            let face = state.facing.to_radians() - base;
            if self.turn(world, &body, id, face)
                && let Some(turned) = Body::read(world, id)
            {
                body = turned;
            }
        }
        let feet = state.feet;
        if crate::finite(feet) {
            body.shift(
                world,
                feet.x - body.pivot_x,
                feet.y - body.feet_y,
                feet.z - body.pivot_z,
            );
        }
        self.falls.insert(
            id,
            Fall {
                velocity: if state.rising.is_finite() {
                    state.rising
                } else {
                    0.0
                },
                grounded: state.grounded,
                airborne: state.airborne,
                landed: None,
            },
        );
    }

    /// Moves controller `id`'s character by `by` and nothing else (no
    /// collisions): a device easing its character toward where it should
    /// be after a correction.
    pub fn nudge(&mut self, world: &mut World, id: InstanceId, by: Vector3) {
        if let Some(mut body) = Body::read(world, id)
            && crate::finite(by)
        {
            body.shift(world, by.x, by.y, by.z);
        }
    }
}

/// Which way `body`'s first part faces: its `Orientation.Y`, in degrees.
fn facing(world: &World, body: &Body) -> Option<f32> {
    let first = *body.parts.first()?;
    world
        .get(first)?
        .property("Orientation")
        .and_then(Value::as_vector3)
        .map(|orientation| orientation.y)
        .filter(|y| y.is_finite())
}
