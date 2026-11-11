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

/// What [`Physics`] keeps for [`Blend`]s.
#[derive(Default)]
pub(crate) struct Blending {
    /// Each awake dynamic body's pose before the last step of the frame.
    before: HashMap<RigidBodyHandle, Pose>,
    /// Where the wheels' parts were placed before that step, when an
    /// earlier step in the same frame placed them.
    wheels_before: BTreeMap<InstanceId, Pose>,
    /// The blends of the last frame that stepped, by part, each with the
    /// body that moves it.
    list: Vec<(RigidBodyHandle, Blend)>,
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
    /// car shake on screen.
    pub fn blends(&self, keep_stepped: &[InstanceId]) -> Vec<Blend> {
        let held: HashSet<RigidBodyHandle> = keep_stepped
            .iter()
            .filter_map(|id| self.parts.get(id).map(|part| part.body))
            .collect();
        self.blending
            .list
            .iter()
            .filter(|(body, _)| !held.contains(body))
            .map(|(_, blend)| *blend)
            .collect()
    }

    /// Before the frame's last step: remembers where each moving body is.
    pub(crate) fn remember_before_last_step(&mut self, wheels: &BTreeMap<InstanceId, Pose>) {
        let before = &mut self.blending.before;
        before.clear();
        for handle in self.rapier.islands.active_bodies() {
            if let Some(body) = self.rapier.bodies.get(handle)
                && body.is_dynamic()
            {
                before.insert(handle, *body.position());
            }
        }
        self.blending.wheels_before.clone_from(wheels);
    }

    /// After the write-back of a frame that stepped, and before the wheels
    /// are written: lists every part that moved in the last step.
    pub(crate) fn find_blends(&mut self, wheels: &BTreeMap<InstanceId, Pose>) {
        let mut list = std::mem::take(&mut self.blending.list);
        list.clear();
        for (&handle, &from) in &self.blending.before {
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
        // A wheel's part is placed by its car, not simulated: before the
        // last step it was where an earlier step this frame put it, or else
        // where the last frame wrote it.
        for (chassis, id) in vehicles::wheel_parts(&self.vehicles) {
            let (Some(to), Some(part)) = (wheels.get(&id), self.parts.get(&id)) else {
                continue;
            };
            let (from_position, from_orientation) = match self.blending.wheels_before.get(&id) {
                Some(from) => (
                    to_vector3(from.translation),
                    quat_to_orientation(from.rotation),
                ),
                None => (part.seen.position, part.seen.orientation),
            };
            let blend = Blend {
                part: id,
                from_position,
                from_orientation,
                to_position: to_vector3(to.translation),
                to_orientation: quat_to_orientation(to.rotation),
            };
            if moved(&blend) {
                list.push((chassis, blend));
            }
        }
        list.sort_unstable_by_key(|(_, blend)| blend.part);
        self.blending.list = list;
    }

    /// Forgets every blend: nothing is drawn between steps.
    pub(crate) fn clear_blends(&mut self) {
        self.blending.list.clear();
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
