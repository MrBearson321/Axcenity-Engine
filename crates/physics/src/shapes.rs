//! Collision shapes for each `PartType`. They match the meshes the renderer
//! draws (`render/src/mesh.rs`), including how balls and cylinders stay round.

use std::f32::consts::FRAC_PI_2;

use axcenity_core::PartType;
use rapier3d::glamx::Vec3;
use rapier3d::prelude::{Collider, ColliderBuilder};

/// Parts thinner than this collide as if they were this thick. A shape with
/// no volume has no mass, which rapier can't simulate.
const MIN_SIZE: f32 = 0.01;

/// How much parts grip each other (0 is ice) and bounce (0 is no bounce).
/// The same for every material until materials get their own.
const FRICTION: f32 = 0.5;
const RESTITUTION: f32 = 0.25;

/// The collider for a part of this shape and size, centered on the part.
pub fn collider(shape: PartType, size: Vec3) -> Collider {
    let half = size.abs().max(Vec3::splat(MIN_SIZE)) / 2.0;
    let builder = match shape {
        PartType::Block => ColliderBuilder::cuboid(half.x, half.y, half.z),
        // Balls use the smallest side, like the drawn mesh.
        PartType::Ball => ColliderBuilder::ball(half.min_element()),
        // rapier's cylinders run along Y, ours along X: turn it 90° around Z.
        PartType::Cylinder => ColliderBuilder::cylinder(half.x, half.y.min(half.z))
            .rotation(Vec3::new(0.0, 0.0, FRAC_PI_2)),
        PartType::Wedge => wedge(half),
    };
    builder.friction(FRICTION).restitution(RESTITUTION).build()
}

/// A ramp: flat bottom, upright back (+Z), sloping down to the front (-Z).
fn wedge(half: Vec3) -> ColliderBuilder {
    let corners: Vec<Vec3> = [
        (-1.0, -1.0, -1.0),
        (1.0, -1.0, -1.0),
        (-1.0, -1.0, 1.0),
        (1.0, -1.0, 1.0),
        (-1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0),
    ]
    .into_iter()
    .map(|(x, y, z)| Vec3::new(x, y, z) * half)
    .collect();
    // Six distinct corners always make a hull, but fall back to a box rather than panic.
    ColliderBuilder::convex_hull(&corners)
        .unwrap_or_else(|| ColliderBuilder::cuboid(half.x, half.y, half.z))
}
