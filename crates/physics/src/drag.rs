//! Air resistance and rolling friction (cohesion pass CW6.2): a part's
//! `LinearDrag`, `AngularDrag` and `RollingFriction`. Welded parts share one body, so the
//! assembly's root part's values are the body's.
//!
//! Drag is rapier's damping: each step a body's speed is divided by
//! `1 + step × LinearDrag` (its spin by `AngularDrag`), so a drag of 1
//! takes away about 63% of a speed in a second. Rolling friction adds to
//! the spin's drag only in a step where the body touches something, so a
//! ball rolls to a stop on the ground while one in the air spins on.

use axcenity_core::{DEFAULT_ANGULAR_DRAG, Instance, Value};
use rapier3d::prelude::RigidBody;

use crate::Physics;

/// How a body loses speed and spin by itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Drag {
    linear: f32,
    angular: f32,
    pub(crate) rolling: f32,
}

impl Drag {
    /// A part's that was never changed, and a Terrain's.
    pub(crate) const DEFAULT: Self = Self {
        linear: 0.0,
        angular: DEFAULT_ANGULAR_DRAG as f32,
        rolling: 0.0,
    };

    /// `instance`'s; a missing, negative, NaN or infinite number counts as
    /// its default.
    pub(crate) fn read(instance: &Instance) -> Self {
        let number = |name, default: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map(|n| n as f32)
                .filter(|n| n.is_finite() && *n >= 0.0)
                .unwrap_or(default)
        };
        Self {
            linear: number("LinearDrag", Self::DEFAULT.linear),
            angular: number("AngularDrag", Self::DEFAULT.angular),
            rolling: number("RollingFriction", Self::DEFAULT.rolling),
        }
    }

    /// Gives `body` this drag, without rolling friction.
    pub(crate) fn apply(self, body: &mut RigidBody) {
        body.set_linear_damping(self.linear);
        body.set_angular_damping(self.angular);
    }

    /// The spin drag while the body touches something or not.
    fn spin_drag(self, touching: bool) -> f32 {
        if touching {
            self.angular + self.rolling
        } else {
            self.angular
        }
    }
}

impl Physics {
    /// Before a step: each awake body with rolling friction gets its spin
    /// drag for whether it touches anything now. Only the root parts in
    /// `rolling` are looked at, so worlds without rolling friction pay
    /// nothing; one that lost its rolling friction or its body is dropped.
    pub(crate) fn roll(&mut self) {
        if self.rolling_parts.is_empty() {
            return;
        }
        let mut dropped = Vec::new();
        for &id in &self.rolling_parts {
            let Some(part) = self.parts.get(&id) else {
                dropped.push(id);
                continue;
            };
            let drag = part.seen.drag;
            if !part.root || drag.rolling <= 0.0 {
                dropped.push(id);
                continue;
            }
            let Some(body) = self.rapier.bodies.get(part.body) else {
                dropped.push(id);
                continue;
            };
            if body.is_sleeping() || !body.is_dynamic() {
                continue;
            }
            let touching = body.colliders().iter().any(|&collider| {
                self.rapier
                    .contact_pairs_with(collider)
                    .any(|pair| pair.has_any_active_contact())
            });
            let damping = drag.spin_drag(touching);
            if let Some(body) = self.rapier.bodies.get_mut(part.body)
                && body.angular_damping() != damping
            {
                body.set_angular_damping(damping);
            }
        }
        for id in dropped {
            self.rolling_parts.remove(&id);
        }
    }
}
