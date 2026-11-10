//! Seats (Phase 39). A character whose controller touches an empty Seat
//! that isn't `Disabled` sits on it, and `seat:Sit(controller)` (which
//! writes the seat's `Occupant`) sits it on any seat. While it sits, its
//! controller is in the built-in Seated mode: `Movement` reads
//! `"Seated"`, its parts are unanchored and welded to the seat, so physics
//! carries them with it, and move keys do nothing. Jump, `Sit = false`, a
//! new `Occupant`, or a new `Movement` stands it up a little in front of
//! the seat.
//!
//! A VehicleSeat is a Seat that drives its car (Phase 40): while the
//! character that receives input sits on it, the Move action sets its
//! `Throttle` and `Steer`, and the camera turns with the car, so it stays
//! behind it until the driver gets out. Getting out lets go of both.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, PlayRequest, Value, Vector3, World};

use crate::{
    Body, Controllers, Eye, Mode, MoveInput, Orbit, controllers, is_driver, movement, take_jump,
    translate,
};

/// How close a body's box comes to a seat's to touch it, in studs: more
/// than the gap walls leave, so walking into a seat touches it.
const TOUCH_REACH: f32 = 0.1;
/// The gap between the seat and the box of a character that stands up.
const STAND_GAP: f32 = 0.5;
/// The Animator state a sitting character plays, when it has one.
const SIT_STATE: &str = "Sit";
const SEATED: &str = "Seated";

/// A controller that sits on a seat. Not saved.
pub(crate) struct Seated {
    seat: InstanceId,
    /// The WeldConstraints holding each body part to the seat.
    welds: Vec<InstanceId>,
    /// The Animator playing `Sit`, and the state to play when it stands.
    animator: Option<(InstanceId, String)>,
    /// A VehicleSeat's heading when the camera last turned with it, in
    /// radians (the camera's yaw that looks the way it faces).
    heading: Option<f32>,
}

/// A character that sat down (`seated`) or stood up, for the host to fire
/// `Seated(seated, seat)` on its controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeatChange {
    pub controller: InstanceId,
    pub seated: bool,
    pub seat: InstanceId,
}

impl Controllers {
    /// Stands up every character that was asked to, then sits the ones a
    /// script sat or that touch a free seat. `jump` is the held Jump of the
    /// controller that receives input, `active`.
    pub(crate) fn update_seats(
        &mut self,
        world: &mut World,
        assets: &Assets,
        active: Option<InstanceId>,
        jump: bool,
    ) {
        let gone: Vec<InstanceId> = self
            .seated
            .keys()
            .copied()
            .filter(|&id| !is_driver(world, id))
            .collect();
        for id in gone {
            self.stand(world, id);
        }
        let mut stood = Vec::new();
        for (&id, seated) in &self.seated {
            let asked = take_jump(world, id) || (active == Some(id) && jump);
            let in_seat = seat_in_workspace(world, seated.seat)
                && occupant(world, seated.seat) == Some(id)
                && flag(world, id, "Sit")
                && movement(world, id) == Some(Mode::Seated);
            if asked || !in_seat {
                stood.push(id);
            }
        }
        for &id in &stood {
            self.stand(world, id);
        }
        for id in controllers(world) {
            if self.seated.contains_key(&id)
                || !is_driver(world, id)
                || self.moved_elsewhere(world, id)
            {
                continue;
            }
            let asked = seats(world)
                .into_iter()
                .find(|&seat| occupant(world, seat) == Some(id));
            let touched = || {
                if stood.contains(&id) || movement(world, id) == Some(Mode::Custom) {
                    return None;
                }
                let body = Body::read(world, id)?;
                seats(world)
                    .into_iter()
                    .find(|&seat| free(world, seat) && touches(world, &body, seat))
            };
            match asked.or_else(touched) {
                Some(seat) => self.sit(world, assets, id, seat),
                // `Sit = true` without a seat does nothing.
                None if flag(world, id, "Sit") => {
                    let _ = world.set_property(id, "Sit", Value::Bool(false));
                }
                None => {}
            }
        }
    }

    /// For the controller that receives input, sitting on a VehicleSeat:
    /// sets the seat's `Throttle` and `Steer` from the Move action, and
    /// says how far the car turned since the last call (radians, for the
    /// camera's yaw).
    pub(crate) fn drive_seat(
        &mut self,
        world: &mut World,
        id: InstanceId,
        input: MoveInput,
    ) -> Option<f32> {
        let seated = self.seated.get_mut(&id)?;
        let last = seated.heading?;
        let axis = |positive: bool, negative: bool| f32::from(positive) - f32::from(negative);
        let throttle = (axis(input.forward, input.back) + input.stick[1]).clamp(-1.0, 1.0);
        let steer = (axis(input.right, input.left) + input.stick[0]).clamp(-1.0, 1.0);
        let _ = world.set_property(seated.seat, "Throttle", Value::Number(throttle.into()));
        let _ = world.set_property(seated.seat, "Steer", Value::Number(steer.into()));
        let now = vehicle_heading(world, seated.seat)?;
        seated.heading = Some(now);
        let turned = (now - last + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        Some(turned)
    }

    /// True while `controller` sits on a seat.
    pub fn is_seated(&self, controller: InstanceId) -> bool {
        self.seated.contains_key(&controller)
    }

    /// The characters that sat down or stood up since the last call.
    pub fn take_seat_changes(&mut self) -> Vec<SeatChange> {
        std::mem::take(&mut self.seat_changes)
    }

    fn sit(&mut self, world: &mut World, assets: &Assets, id: InstanceId, seat: InstanceId) {
        let Some(body) = Body::read(world, id) else {
            return;
        };
        let Some(point) = sit_point(world, seat) else {
            return;
        };
        translate(
            world,
            &body.parts,
            point.x - body.pivot_x,
            point.y - body.feet_y,
            point.z - body.pivot_z,
        );
        let mut welds = Vec::new();
        for &part in &body.parts {
            let _ = world.set_property(part, "Anchored", Value::Bool(false));
            let Ok(weld) = world.create(ClassName::WeldConstraint) else {
                continue;
            };
            let _ = world.set_name(weld, "SeatWeld");
            let _ = world.set_property(weld, "Part0", Value::Instance(Some(seat)));
            let _ = world.set_property(weld, "Part1", Value::Instance(Some(part)));
            let _ = world.set_parent(weld, Some(seat));
            welds.push(weld);
        }
        if let Some(Value::String(name)) = world.get(id).and_then(|c| c.property("Movement"))
            && matches!(
                Mode::parse(Some(&Value::String(name.clone()))),
                Some(Mode::Walk | Mode::Run | Mode::Crouch)
            )
        {
            self.built_in.insert(id, name.clone());
        }
        let _ = world.set_property(id, "Movement", Value::String(SEATED.to_owned()));
        let _ = world.set_property(id, "Sit", Value::Bool(true));
        let _ = world.set_property(seat, "Occupant", Value::Instance(Some(id)));
        self.falls.remove(&id);
        let animator = sit_animator(world, assets, id).map(|(animator, back)| {
            play(world, animator, SIT_STATE);
            (animator, back)
        });
        // The camera starts behind a car, looking the way it faces.
        let heading = vehicle_heading(world, seat);
        if let Some(heading) = heading {
            let eye = Eye::of(world, id).unwrap_or(Eye::Follow);
            self.orbits
                .entry(id)
                .or_insert_with(|| Orbit::start(eye))
                .yaw = heading;
        }
        self.seated.insert(
            id,
            Seated {
                seat,
                welds,
                animator,
                heading,
            },
        );
        self.seat_changes.push(SeatChange {
            controller: id,
            seated: true,
            seat,
        });
    }

    fn stand(&mut self, world: &mut World, id: InstanceId) {
        let Some(seated) = self.seated.remove(&id) else {
            return;
        };
        for weld in seated.welds {
            if world.contains(weld) {
                let _ = world.destroy(weld);
            }
        }
        if occupant(world, seated.seat) == Some(id) {
            let _ = world.set_property(seated.seat, "Occupant", Value::Instance(None));
        }
        if seated.heading.is_some() && world.contains(seated.seat) {
            for name in ["Throttle", "Steer"] {
                let _ = world.set_property(seated.seat, name, Value::Number(0.0));
            }
        }
        if world.contains(id) {
            let _ = world.set_property(id, "Sit", Value::Bool(false));
            if movement(world, id) == Some(Mode::Seated) {
                let name = self
                    .built_in
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "Walk".to_owned());
                let _ = world.set_property(id, "Movement", Value::String(name));
            }
        }
        if let Some(body) = Body::read(world, id)
            && let Some((x, y, z)) = stand_point(world, seated.seat, &body)
        {
            translate(
                world,
                &body.parts,
                x - body.pivot_x,
                y - body.feet_y,
                z - body.pivot_z,
            );
        }
        // It starts in the air, so it lands wherever it stood up.
        self.falls.remove(&id);
        if let Some((animator, back)) = seated.animator
            && !back.is_empty()
        {
            play(world, animator, &back);
        }
        self.seat_changes.push(SeatChange {
            controller: id,
            seated: false,
            seat: seated.seat,
        });
    }
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Every Seat in the Workspace.
fn seats(world: &World) -> Vec<InstanceId> {
    world.all_below(world.workspace(), ClassName::is_seat)
}

fn seat_in_workspace(world: &World, seat: InstanceId) -> bool {
    world.get(seat).is_some_and(|i| i.class().is_seat())
        && world.is_descendant_of(seat, world.workspace())
}

fn occupant(world: &World, seat: InstanceId) -> Option<InstanceId> {
    world
        .get(seat)?
        .property("Occupant")?
        .as_instance()
        .filter(|&id| world.contains(id))
}

/// Empty and not Disabled: a touch sits on it.
fn free(world: &World, seat: InstanceId) -> bool {
    occupant(world, seat).is_none() && !flag(world, seat, "Disabled")
}

/// A seat's middle, the half size of the box around it as it's turned,
/// and its Orientation.
fn seat_box(world: &World, seat: InstanceId) -> Option<(Vector3, Vector3, Vector3)> {
    let instance = world.get(seat)?;
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let (position, size, orientation) =
        (vector("Position")?, vector("Size")?, vector("Orientation")?);
    let mut half = Vector3::new(0.0, 0.0, 0.0);
    for axis in [
        Vector3::new(size.x.abs() * 0.5, 0.0, 0.0),
        Vector3::new(0.0, size.y.abs() * 0.5, 0.0),
        Vector3::new(0.0, 0.0, size.z.abs() * 0.5),
    ] {
        let turned = rotate(orientation, axis);
        half.x += turned.x.abs();
        half.y += turned.y.abs();
        half.z += turned.z.abs();
    }
    let all = [position, half, orientation];
    all.iter()
        .all(|v| crate::finite(*v))
        .then_some((position, half, orientation))
}

/// Whether the body's box comes within [`TOUCH_REACH`] of the seat's.
fn touches(world: &World, body: &Body, seat: InstanceId) -> bool {
    let Some((center, half, _)) = seat_box(world, seat) else {
        return false;
    };
    let reach = TOUCH_REACH;
    (body.pivot_x - center.x).abs() <= body.radius + half.x + reach
        && (body.pivot_z - center.z).abs() <= body.radius + half.z + reach
        && body.feet_y <= center.y + half.y + reach
        && body.feet_y + body.height >= center.y - half.y - reach
}

/// Where a sitting character's feet go: the SeatOffset Attachment, or the
/// middle of the seat's top face.
fn sit_point(world: &World, seat: InstanceId) -> Option<Vector3> {
    let offset = world.get(seat)?.property("SeatOffset")?.as_instance();
    if let Some(point) = offset.and_then(|a| axcenity_physics::joints::attachment_point(world, a)) {
        return Some(point);
    }
    let instance = world.get(seat)?;
    let size = instance.property("Size")?.as_vector3()?;
    let (center, _, orientation) = seat_box(world, seat)?;
    let up = rotate(orientation, Vector3::new(0.0, size.y.abs() * 0.5, 0.0));
    Some(Vector3::new(
        center.x + up.x,
        center.y + up.y,
        center.z + up.z,
    ))
}

/// Where a character that stands up goes: in front of the seat (the way
/// its front face looks, along the ground), clear of it, its feet as high
/// as they were on the seat.
fn stand_point(world: &World, seat: InstanceId, body: &Body) -> Option<(f32, f32, f32)> {
    let (center, half, orientation) = seat_box(world, seat)?;
    let front = rotate(orientation, Vector3::new(0.0, 0.0, -1.0));
    let len = front.x.hypot(front.z);
    let (fx, fz) = if len > 1e-3 {
        (front.x / len, front.z / len)
    } else {
        (0.0, -1.0)
    };
    let reach = half.x * fx.abs() + half.z * fz.abs() + body.radius + STAND_GAP;
    let feet = sit_point(world, seat).map_or(center.y + half.y, |point| point.y);
    Some((center.x + fx * reach, feet, center.z + fz * reach))
}

/// The yaw of a camera looking the way a VehicleSeat faces (its -Z along
/// the ground), or `None` for a plain Seat.
fn vehicle_heading(world: &World, seat: InstanceId) -> Option<f32> {
    let instance = world.get(seat)?;
    if instance.class() != ClassName::VehicleSeat {
        return None;
    }
    let orientation = instance.property("Orientation")?.as_vector3()?;
    let front = rotate(orientation, Vector3::new(0.0, 0.0, -1.0));
    // The camera's yaw 0 looks along -Z, turning toward -X as it grows.
    let heading = (-front.x).atan2(-front.z);
    heading.is_finite().then_some(heading)
}

/// `v` turned by an Orientation in degrees: Z first, then X, then Y.
fn rotate(orientation: Vector3, v: Vector3) -> Vector3 {
    let (sx, cx) = orientation.x.to_radians().sin_cos();
    let (sy, cy) = orientation.y.to_radians().sin_cos();
    let (sz, cz) = orientation.z.to_radians().sin_cos();
    let (x, y, z) = (v.x * cz - v.y * sz, v.x * sz + v.y * cz, v.z);
    let (y, z) = (y * cx - z * sx, y * sx + z * cx);
    Vector3::new(x * cy + z * sy, y, -x * sy + z * cy)
}

/// The character's Animator whose state machine has a `Sit` state, and the
/// state it's in (its starting state if it hasn't run yet).
fn sit_animator(world: &World, assets: &Assets, id: InstanceId) -> Option<(InstanceId, String)> {
    let holder = world.get(id)?.parent()?;
    world.descendants_iter(holder).find_map(|animator| {
        let instance = world.get(animator)?;
        if instance.class() != ClassName::Animator {
            return None;
        }
        let path = instance.property("StateMachine")?.as_str()?;
        if path.is_empty() {
            return None;
        }
        let file = axcenity_anim::read_state_machine(assets, path).ok()?;
        file.state(SIT_STATE)?;
        let current = world
            .animator(animator)
            .map(|state| state.current_state.clone())
            .filter(|state| !state.is_empty() && state != SIT_STATE);
        let back = current.or_else(|| file.starting_state().map(|state| state.name.clone()));
        Some((animator, back.unwrap_or_default()))
    })
}

fn play(world: &mut World, animator: InstanceId, state: &str) {
    if let Some(live) = world.animator_mut(animator) {
        live.play = Some(PlayRequest {
            state: state.to_owned(),
            blend: None,
        });
    }
}
