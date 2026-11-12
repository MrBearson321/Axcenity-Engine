//! What the simulation is doing right now, for the editor's physics debug
//! overlays (roadmap G15): where bodies touch and how hard, and where each
//! moving body's center of mass is and whether it's asleep. Read only:
//! nothing here steps, wakes or writes back.

use axcenity_core::{InstanceId, Vector3};

use super::*;

/// Most contact points [`Physics::debug_view`] lists, so a pile of parts
/// can't make the overlay the slow part of a frame.
pub const MAX_DEBUG_CONTACTS: usize = 2048;

/// One point where two colliders touch, after the last step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugContact {
    /// Where, in the world, on the first part's surface.
    pub point: Vector3,
    /// Which way the first part is pushed away from the second, as a unit
    /// vector (rapier's manifold normal, flipped to point at the first).
    pub normal: Vector3,
    /// How hard the step pushed them apart there, in mass × studs/s (0
    /// for a contact that only just began, or a sensor).
    pub impulse: f32,
    pub parts: (InstanceId, InstanceId),
}

/// One moving body (an unanchored part, or a welded assembly).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugBody {
    /// Its assembly's root part.
    pub part: InstanceId,
    /// Its center of mass in the world.
    pub center_of_mass: Vector3,
    pub mass: f32,
    /// True while rapier has put it to sleep (it stopped moving, so it
    /// isn't simulated until something touches or moves it).
    pub sleeping: bool,
}

/// The simulation's state for the overlays, in `core` types.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugView {
    pub contacts: Vec<DebugContact>,
    pub bodies: Vec<DebugBody>,
}

impl Physics {
    /// The contacts and moving bodies after the last [`step`](Self::step),
    /// at most [`MAX_DEBUG_CONTACTS`] contacts. Bodies are in part order.
    pub fn debug_view(&self) -> DebugView {
        let mut view = DebugView::default();
        'pairs: for pair in self.rapier.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let (Some(&one), Some(&two)) = (
                self.owners.get(&pair.collider1),
                self.owners.get(&pair.collider2),
            ) else {
                continue;
            };
            let Some(collider) = self.rapier.colliders.get(pair.collider1) else {
                continue;
            };
            for manifold in pair.manifolds() {
                // rapier's normal points from the first collider to the
                // second; the first is pushed the other way.
                let normal = -manifold.data.normal;
                for contact in &manifold.points {
                    if contact.dist > 0.0 {
                        continue;
                    }
                    if view.contacts.len() == MAX_DEBUG_CONTACTS {
                        break 'pairs;
                    }
                    let point = *collider.position() * contact.local_p1;
                    view.contacts.push(DebugContact {
                        point: to_vector3(point),
                        normal: to_vector3(normal),
                        impulse: contact.data.impulse.abs(),
                        parts: (one, two),
                    });
                }
            }
        }
        for (&part, body) in &self.parts {
            if !body.root {
                continue;
            }
            let Some(rigid) = self.rapier.bodies.get(body.body) else {
                continue;
            };
            if !rigid.is_dynamic() {
                continue;
            }
            view.bodies.push(DebugBody {
                part,
                center_of_mass: to_vector3(rigid.center_of_mass()),
                mass: rigid.mass(),
                sleeping: rigid.is_sleeping(),
            });
        }
        view
    }
}
