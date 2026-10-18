//! Axcenity physics: simulates the Parts in the Workspace with `rapier3d`.
//!
//! Anchored parts are fixed colliders, and unanchored parts are dynamic
//! bodies. [`Physics::step`] runs at a fixed 60 steps a second whatever the
//! frame rate, and writes each moved part's `Position` and `Orientation` back
//! into the [`World`].
//!
//! Physics never gets told about changes. Instead, before stepping it compares
//! every Part with what it last saw (or wrote), so a script that moves,
//! anchors, resizes or deletes a part is noticed on the next frame.

mod shapes;

use std::collections::BTreeMap;

use axcenity_core::{Instance, InstanceId, PartType, Value, Vector3, World};
use rapier3d::glamx::{EulerRot, Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::pipeline::PhysicsWorld;
use rapier3d::prelude::{ColliderHandle, RigidBodyBuilder, RigidBodyHandle, RigidBodyType};

/// How long one physics step is: physics always runs at 60 steps a second.
pub const STEP_SECONDS: f64 = 1.0 / 60.0;

/// Downward pull in studs per second squared (about 1 stud = 0.28 m).
pub const GRAVITY: f32 = 196.2;

/// The most steps one frame may run. The player already caps a frame at
/// 0.1 s (6 steps), so this only matters if a host passes a huge time: the
/// rest is dropped rather than freezing the game while physics catches up.
const MAX_STEPS_PER_FRAME: u32 = 10;

/// A frame that is this close to a whole number of steps counts as reaching
/// it, so rounding in summed frame times (1/144 + 1/144 + …) never costs a step.
const STEP_TOLERANCE: f64 = 1e-6;

/// How quickly spinning slows down by itself. rapier has no rolling
/// resistance, so without this a ball on flat ground would roll forever.
const ANGULAR_DAMPING: f32 = 0.5;

/// Owns the rapier simulation and remembers which body belongs to which Part.
pub struct Physics {
    rapier: PhysicsWorld,
    /// A `BTreeMap` (sorted) rather than a `HashMap` so bodies are always
    /// visited in the same order, which keeps the simulation repeatable.
    parts: BTreeMap<InstanceId, Body>,
    /// Time passed that hasn't been simulated yet, always less than one step
    /// after [`step`](Self::step).
    unsimulated_seconds: f64,
}

/// One Part's rapier body, plus the part's properties as physics last saw or
/// wrote them. A property that differs from these was changed by a script.
struct Body {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    seen: PartState,
}

/// The properties of a Part that physics cares about.
#[derive(Clone, Copy, PartialEq)]
struct PartState {
    anchored: bool,
    shape: PartType,
    size: Vector3,
    position: Vector3,
    orientation: Vector3,
}

impl PartState {
    /// Reads a Part, or `None` for other classes. A part with a NaN or
    /// infinite Position, Orientation or Size also gives `None`, so it is
    /// left out of physics instead of breaking the simulation.
    fn read(instance: &Instance) -> Option<Self> {
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        let anchored = match instance.property("Anchored") {
            Some(Value::Bool(anchored)) => *anchored,
            _ => return None,
        };
        let state = Self {
            anchored,
            shape: instance.property("Shape").and_then(Value::as_part_type)?,
            size: vector("Size")?,
            position: vector("Position")?,
            orientation: vector("Orientation")?,
        };
        let finite = [state.size, state.position, state.orientation]
            .iter()
            .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite());
        finite.then_some(state)
    }

    fn pose(&self) -> Pose {
        Pose::from_parts(
            to_vec3(self.position),
            orientation_to_quat(self.orientation),
        )
    }

    fn body_type(&self) -> RigidBodyType {
        if self.anchored {
            RigidBodyType::Fixed
        } else {
            RigidBodyType::Dynamic
        }
    }
}

impl Physics {
    pub fn new() -> Self {
        let mut rapier = PhysicsWorld::new();
        rapier.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        rapier.integration_parameters.dt = STEP_SECONDS as f32;
        Self {
            rapier,
            parts: BTreeMap::new(),
            unsimulated_seconds: 0.0,
        }
    }

    /// Frame step 3: catches up with script changes, then runs as many fixed
    /// steps as fit in the time passed (0 to N), writing positions back to
    /// `world` after each one. Returns how many steps ran.
    pub fn step(&mut self, world: &mut World, seconds: f64) -> u32 {
        self.sync_from_world(world);

        // `max` also turns NaN into 0, so a bad time can't stop physics forever.
        self.unsimulated_seconds += seconds.max(0.0);
        let mut steps = 0;
        while self.unsimulated_seconds + STEP_TOLERANCE >= STEP_SECONDS {
            if steps == MAX_STEPS_PER_FRAME {
                self.unsimulated_seconds = 0.0;
                break;
            }
            self.rapier.step();
            self.write_to_world(world);
            self.unsimulated_seconds = (self.unsimulated_seconds - STEP_SECONDS).max(0.0);
            steps += 1;
        }
        steps
    }

    /// True if `id` currently has a body in the simulation.
    pub fn has_body(&self, id: InstanceId) -> bool {
        self.parts.contains_key(&id)
    }

    /// How many parts are being simulated (anchored ones included).
    pub fn body_count(&self) -> usize {
        self.parts.len()
    }

    /// Adds new Parts, removes ones that left the Workspace or were destroyed,
    /// and applies anything a script changed since the last step.
    fn sync_from_world(&mut self, world: &World) {
        let current: BTreeMap<InstanceId, PartState> = world
            .descendants(world.workspace())
            .into_iter()
            .filter_map(|id| Some((id, PartState::read(world.get(id)?)?)))
            .collect();
        let mut anything_changed = false;

        let gone: Vec<InstanceId> = self
            .parts
            .keys()
            .filter(|id| !current.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(part) = self.parts.remove(&id) {
                // `true` also removes the body's collider.
                self.rapier.remove_body_with_colliders(part.body, true);
                anything_changed = true;
            }
        }

        for (id, state) in current {
            match self.parts.get(&id) {
                None => {
                    let part = self.add_body(state);
                    self.parts.insert(id, part);
                }
                Some(part) if part.seen != state => {
                    let mut part = self.parts.remove(&id).expect("just found");
                    self.apply_change(&mut part, state);
                    self.parts.insert(id, part);
                    anything_changed = true;
                }
                Some(_) => {}
            }
        }

        // Parts at rest "sleep" and stop being simulated. If a script moved
        // or removed something, a sleeping part may have lost what it was
        // resting on, so wake everything and let it settle again. Simple, and
        // cheap enough for v1 scene sizes.
        if anything_changed {
            self.rapier.wake_up_all(true);
        }
    }

    fn add_body(&mut self, state: PartState) -> Body {
        let body = RigidBodyBuilder::new(state.body_type())
            .pose(state.pose())
            // Continuous collision detection: fast parts can't skip through
            // thin ones between two steps.
            .ccd_enabled(true)
            .angular_damping(ANGULAR_DAMPING)
            .build();
        let (body, collider) = self
            .rapier
            .insert(body, shapes::collider(state.shape, to_vec3(state.size)));
        Body {
            body,
            collider,
            seen: state,
        }
    }

    /// Applies what a script changed on a part that already has a body.
    fn apply_change(&mut self, part: &mut Body, state: PartState) {
        let old = part.seen;
        part.seen = state;

        if old.shape != state.shape || old.size != state.size {
            let rapier = &mut self.rapier;
            rapier.colliders.remove(
                part.collider,
                &mut rapier.islands,
                &mut rapier.bodies,
                &mut rapier.soft_bodies,
                true,
            );
            part.collider = rapier.colliders.insert_with_parent(
                shapes::collider(state.shape, to_vec3(state.size)),
                part.body,
                &mut rapier.bodies,
            );
        }

        let Some(body) = self.rapier.bodies.get_mut(part.body) else {
            return;
        };
        if old.anchored != state.anchored {
            body.set_body_type(state.body_type(), true);
        }
        let moved = old.position != state.position || old.orientation != state.orientation;
        if moved {
            body.set_position(state.pose(), true);
        }
        // A teleported part starts again from rest (decided in Phase 6), and
        // so does a part that was just released.
        if moved || old.anchored != state.anchored {
            body.set_linvel(Vec3::ZERO, true);
            body.set_angvel(Vec3::ZERO, true);
        }
    }

    /// Copies where each moving part is now into its `Position` and
    /// `Orientation`, and remembers the values written.
    fn write_to_world(&mut self, world: &mut World) {
        for (&id, part) in &mut self.parts {
            let Some(body) = self.rapier.bodies.get(part.body) else {
                continue;
            };
            if !body.is_dynamic() || body.is_sleeping() {
                continue;
            }
            let pose = body.position();
            let position = to_vector3(pose.translation);
            let orientation = quat_to_orientation(pose.rotation);
            if position != part.seen.position {
                part.seen.position = position;
                // The part was checked to exist this frame, and scripts don't
                // run between physics steps, so this can't fail.
                let _ = world.set_property(id, "Position", Value::Vector3(position));
            }
            if orientation != part.seen.orientation {
                part.seen.orientation = orientation;
                let _ = world.set_property(id, "Orientation", Value::Vector3(orientation));
            }
        }
    }
}

impl Default for Physics {
    fn default() -> Self {
        Self::new()
    }
}

fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn to_vector3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// `Orientation` is degrees around X, Y and Z, applied Z first, then X, then
/// Y (the same rule as the renderer in `player/src/scene.rs`).
fn orientation_to_quat(orientation: Vector3) -> Quat {
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

fn quat_to_orientation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    Vector3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

#[cfg(test)]
mod tests;
