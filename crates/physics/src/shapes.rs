//! Collision shapes for each `PartType`, and for MeshParts. They match the
//! meshes the renderer draws (`render/src/mesh.rs`), including how balls and
//! cylinders stay round.

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use axcenity_assets::Model;
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

/// What a part collides as.
#[derive(Clone, PartialEq)]
pub enum Form {
    /// A Part, by its `Shape`.
    Primitive(PartType),
    /// A MeshPart, by its model, or by its box while it has none (no
    /// `MeshId`, or a model that can't be loaded). Models compare by which
    /// load they are, so a model loaded later counts as a change.
    Mesh(Option<Arc<Model>>),
}

/// The collider for a part of this form and size, centered on the part.
pub fn collider(form: &Form, size: Vec3) -> Collider {
    let half = size.abs().max(Vec3::splat(MIN_SIZE)) / 2.0;
    let builder = match form {
        Form::Primitive(PartType::Block) | Form::Mesh(None) => cuboid(half),
        // Balls use the smallest side, like the drawn mesh.
        Form::Primitive(PartType::Ball) => ColliderBuilder::ball(half.min_element()),
        // rapier's cylinders run along Y, ours along X: turn it 90° around Z.
        Form::Primitive(PartType::Cylinder) => {
            ColliderBuilder::cylinder(half.x, half.y.min(half.z))
                .rotation(Vec3::new(0.0, 0.0, FRAC_PI_2))
        }
        Form::Primitive(PartType::Wedge) => wedge(half),
        Form::Mesh(Some(model)) => hull(model, half),
    };
    builder.friction(FRICTION).restitution(RESTITUTION).build()
}

fn cuboid(half: Vec3) -> ColliderBuilder {
    ColliderBuilder::cuboid(half.x, half.y, half.z)
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
    ColliderBuilder::convex_hull(&corners).unwrap_or_else(|| cuboid(half))
}

/// The smallest convex shape around a model's corners, like shrink-wrap:
/// hollows (under a table, inside a cup) are filled in. A model with no
/// volume, such as a flat plane, can't make one and collides as its box.
fn hull(model: &Model, half: Vec3) -> ColliderBuilder {
    let flat = model.size.contains(&0.0) || model.hull_points.len() < 4;
    if flat {
        return cuboid(half);
    }
    // The model fills the 1×1×1 cube, so doubling the half size stretches it to the part.
    let corners: Vec<Vec3> = model
        .hull_points
        .iter()
        .map(|&corner| Vec3::from(corner) * half * 2.0)
        .collect();
    ColliderBuilder::convex_hull(&corners).unwrap_or_else(|| cuboid(half))
}
