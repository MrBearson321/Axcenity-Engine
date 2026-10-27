//! How parts grip and bounce off each other: each collider's
//! [`PhysicalProperties`], and the weighted average where two touch.
//!
//! rapier averages two colliders' friction and restitution evenly, which is
//! the weighted average when both weights are 1. A collider with another
//! weight keeps its weights in `user_data` and turns on the contact hook,
//! which [`Weighted`] answers with `(a * wa + b * wb) / (wa + wb)`. Pairs
//! without one never run the hook, so they simulate as before.

use axcenity_core::PhysicalProperties;
use rapier3d::prelude::{
    ActiveHooks, Collider, ContactModificationContext, PairFilterContext, PhysicsHooks, SolverFlags,
};

/// Gives `collider` these properties: its density, friction, restitution,
/// and the weights where they aren't both 1.
pub(crate) fn apply(collider: &mut Collider, physical: PhysicalProperties) {
    collider.set_density(physical.density);
    collider.set_friction(physical.friction);
    collider.set_restitution(physical.elasticity);
    let even = physical.friction_weight == 1.0 && physical.elasticity_weight == 1.0;
    collider.user_data = if even {
        0
    } else {
        pack(physical.friction_weight, physical.elasticity_weight)
    };
    collider.set_active_hooks(if even {
        ActiveHooks::empty()
    } else {
        ActiveHooks::MODIFY_SOLVER_CONTACTS
    });
}

/// The two weights in a collider's `user_data`. Bit 64 marks it as set, so
/// a collider without weights (0) reads as weights of 1.
fn pack(friction_weight: f32, elasticity_weight: f32) -> u128 {
    1 << 64 | u128::from(friction_weight.to_bits()) << 32 | u128::from(elasticity_weight.to_bits())
}

fn weights(user_data: u128) -> (f32, f32) {
    if user_data >> 64 == 0 {
        return (1.0, 1.0);
    }
    (
        f32::from_bits((user_data >> 32) as u32),
        f32::from_bits(user_data as u32),
    )
}

/// The average of `a` and `b`, weighted by `wa` and `wb`; an even one when
/// both weights are 0.
fn weighted(a: f32, wa: f32, b: f32, wb: f32) -> f32 {
    if wa + wb > 0.0 {
        (a * wa + b * wb) / (wa + wb)
    } else {
        (a + b) / 2.0
    }
}

/// The contact hook for colliders with weights.
pub(crate) struct Weighted;

impl PhysicsHooks for Weighted {
    fn filter_contact_pair(&self, _: &PairFilterContext) -> Option<SolverFlags> {
        Some(SolverFlags::default())
    }

    fn filter_intersection_pair(&self, _: &PairFilterContext) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let (Some(one), Some(two)) = (
            context.colliders.get(context.collider1),
            context.colliders.get(context.collider2),
        ) else {
            return;
        };
        let (friction_one, elasticity_one) = weights(one.user_data);
        let (friction_two, elasticity_two) = weights(two.user_data);
        let friction = weighted(one.friction(), friction_one, two.friction(), friction_two);
        let restitution = weighted(
            one.restitution(),
            elasticity_one,
            two.restitution(),
            elasticity_two,
        );
        if let Some(manifold) = context.rigid_mut() {
            *manifold.friction = friction;
            *manifold.restitution = restitution;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_round_trip_and_default_to_one() {
        assert_eq!(weights(0), (1.0, 1.0));
        assert_eq!(weights(pack(3.0, 0.5)), (3.0, 0.5));
        assert_eq!(weights(pack(0.0, 0.0)), (0.0, 0.0));
    }

    #[test]
    fn the_heavier_weight_pulls_the_average_its_way() {
        // Ice (0.02, weight 3) on Plastic (0.5, weight 1).
        assert!((weighted(0.02, 3.0, 0.5, 1.0) - 0.14).abs() < 1e-6);
        assert_eq!(weighted(0.2, 1.0, 0.6, 1.0), 0.4);
        assert_eq!(weighted(0.2, 0.0, 0.6, 0.0), 0.4);
    }
}
