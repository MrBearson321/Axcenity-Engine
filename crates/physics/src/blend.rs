//! Render interpolation (roadmap G3): where moving parts are drawn between
//! fixed steps.
//!
//! Physics steps at 60 Hz, so at 144 or 240 frames a second most frames run
//! no step and a falling part would be drawn in 60 Hz jumps. Instead, the
//! frame's last step remembers where each moving part was before it
//! ([`Blend::from_position`]) as well as where it left it (`to_*`, the
//! values written to the world), and hosts draw each part that far along
//! between the two as the time not yet simulated is of a step
//! ([`Physics::blend_alpha`]). So what's drawn trails the simulation by
//! under one step, smoothly.
//!
//! Only drawing changes: the world, scripts, touches and the network keep
//! the stepped values, and results are the same with it on or off. The
//! Workspace's `PhysicsInterpolation` (on by default) turns it off.

use std::cell::OnceCell;

use super::*;

/// One moving part, before and after the frame's last step, as the world's
/// `Position` and `Orientation` (degrees) give them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blend {
    pub part: InstanceId,
    pub from_position: Vector3,
    pub from_orientation: Vector3,
    pub to_position: Vector3,
    pub to_orientation: Vector3,
}

/// What [`Physics`] keeps for [`Blend`]s. Only the poses before the last
/// step are kept while stepping: the list is worked out the first time a
/// host asks for it, so hosts that don't draw (the server, the test runner)
/// don't pay for it.
#[derive(Default)]
pub(crate) struct Blending {
    /// Each awake dynamic body's pose before the last step of the frame.
    before: Vec<(RigidBodyHandle, Pose)>,
    /// Where each wheel's part was before that step: where an earlier step
    /// in the same frame placed it, or else where the last frame wrote it.
    wheels_before: BTreeMap<InstanceId, Pose>,
    /// The blends of the last frame that stepped, by part, each with the
    /// body that moves it, once a host has asked for them.
    list: OnceCell<Vec<(RigidBodyHandle, Blend)>>,
}

impl Blending {
    /// At the start of a frame: a list no host asked for while the world
    /// still matched the last step is forgotten, because syncing scripts'
    /// changes could make it wrong. One a host read is kept for the frames
    /// without a step.
    pub(crate) fn start_frame(&mut self) {
        if self.list.get().is_none() {
            self.forget();
        }
    }

    /// Forgets every blend: nothing is drawn between steps.
    pub(crate) fn forget(&mut self) {
        self.before.clear();
        self.wheels_before.clear();
        self.list = OnceCell::new();
    }
}

impl Physics {
    /// How far along each [`Blend`] parts are drawn now: the time passed
    /// since the last step, as a share of a step (0 to 1).
    pub fn blend_alpha(&self) -> f32 {
        (self.unsimulated_seconds / STEP_SECONDS).clamp(0.0, 1.0) as f32
    }

    /// The parts to draw between steps, by part ID, leaving out every body
    /// that has one of `keep_stepped`'s parts (and so every part welded to
    /// it). Hosts pass what the camera follows: a camera that follows a
    /// stepped part while the car under it is drawn blended would make the
    /// car shake on screen. Ask after the frame's [`step`](Self::step); a
    /// list nobody asked for by the next step is gone.
    pub fn blends(&self, keep_stepped: &[InstanceId]) -> Vec<Blend> {
        let held: HashSet<RigidBodyHandle> = keep_stepped
            .iter()
            .filter_map(|id| self.parts.get(id).map(|part| part.body))
            .collect();
        self.blending
            .list
            .get_or_init(|| self.find_blends())
            .iter()
            .filter(|(body, _)| !held.contains(body))
            .map(|(_, blend)| *blend)
            .collect()
    }

    /// Before the frame's last step: remembers where each moving body is,
    /// and where the steps before it this frame placed the wheels.
    pub(crate) fn remember_before_last_step(&mut self, wheels: &BTreeMap<InstanceId, Pose>) {
        let blending = &mut self.blending;
        blending.list = OnceCell::new();
        blending.before.clear();
        for handle in self.rapier.islands.active_bodies() {
            if let Some(body) = self.rapier.bodies.get(handle)
                && body.is_dynamic()
            {
                blending.before.push((handle, *body.position()));
            }
        }
        blending.wheels_before.clone_from(wheels);
    }

    /// After the frame's steps, before the wheels are written: a wheel no
    /// earlier step this frame placed was where the last frame wrote it.
    pub(crate) fn remember_wheels_before_writing(&mut self) {
        for (_, id) in vehicles::wheel_parts(&self.vehicles) {
            if let Some(part) = self.parts.get(&id) {
                self.blending.wheels_before.entry(id).or_insert_with(|| {
                    Pose::from_parts(
                        to_vec3(part.seen.position),
                        orientation_to_quat(part.seen.orientation),
                    )
                });
            }
        }
    }

    /// Every part that moved in the last step, from where it was before
    /// that step to where it was written (its `seen`).
    fn find_blends(&self) -> Vec<(RigidBodyHandle, Blend)> {
        let mut list = Vec::new();
        for &(handle, from) in &self.blending.before {
            let Some(body) = self.rapier.bodies.get(handle) else {
                continue;
            };
            // A body that fell asleep has come to rest where it's written.
            if !body.is_dynamic() || body.is_sleeping() {
                continue;
            }
            for collider in body.colliders() {
                let Some(&id) = self.owners.get(collider) else {
                    continue;
                };
                let Some(part) = self.parts.get(&id).filter(|part| part.body == handle) else {
                    continue;
                };
                let pose = if part.root { from } else { from * part.offset };
                let blend = Blend {
                    part: id,
                    from_position: to_vector3(pose.translation),
                    from_orientation: quat_to_orientation(pose.rotation),
                    to_position: part.seen.position,
                    to_orientation: part.seen.orientation,
                };
                if moved(&blend) {
                    list.push((handle, blend));
                }
            }
        }
        // A wheel's part is placed by its car, not simulated.
        for (chassis, id) in vehicles::wheel_parts(&self.vehicles) {
            let (Some(from), Some(part)) =
                (self.blending.wheels_before.get(&id), self.parts.get(&id))
            else {
                continue;
            };
            let blend = Blend {
                part: id,
                from_position: to_vector3(from.translation),
                from_orientation: quat_to_orientation(from.rotation),
                to_position: part.seen.position,
                to_orientation: part.seen.orientation,
            };
            if moved(&blend) {
                list.push((chassis, blend));
            }
        }
        list.sort_unstable_by_key(|(_, blend)| blend.part);
        list
    }
}

fn moved(blend: &Blend) -> bool {
    blend.from_position != blend.to_position || blend.from_orientation != blend.to_orientation
}

/// The Workspace's `PhysicsInterpolation`: true (the default) draws moving
/// parts between steps.
pub fn interpolates(world: &World) -> bool {
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("PhysicsInterpolation"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}
