//! Joint constraints with `Visible` on (Phase 36), drawn in the game in
//! their `Color`: a spring as a coil, a rope or rod as a line between its
//! attachments, a hinge or cylindrical constraint as a short axle along its
//! axis, a prismatic constraint as its slide, a ball socket as a ball.

use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_render::{DrawItem, Material, Shape};

use crate::orientation::rotation_from_orientation;

/// How many straight pieces draw one turn of a spring's coil.
const PIECES_PER_COIL: usize = 12;
/// The most coils drawn, so a typo can't make millions of pieces.
const MAX_COILS: f32 = 100.0;
/// How long a drawn axle is, in studs.
const AXLE_LENGTH: f32 = 1.0;

/// The pieces that draw the joint `constraint`, if it's Visible and both its
/// attachments are on parts.
pub(crate) fn draw_items(world: &World, constraint: &Instance) -> Vec<DrawItem> {
    let number = |name| {
        constraint
            .property(name)
            .and_then(Value::as_number)
            .map(|n| n as f32)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0)
    };
    if constraint.property("Visible") != Some(&Value::Bool(true)) || !constraint.class().is_joint()
    {
        return Vec::new();
    }
    let (Some((p0, turn0)), Some((p1, _))) = (
        attachment_pose(world, constraint, "Attachment0"),
        attachment_pose(world, constraint, "Attachment1"),
    ) else {
        return Vec::new();
    };
    let color = constraint
        .property("Color")
        .and_then(Value::as_color3)
        .map_or([0.62, 0.64, 0.68], |c| [c.r, c.g, c.b]);
    let material = Material {
        base_color: color,
        roughness: 0.6,
        ..Material::default()
    };
    let thickness = number("Thickness").max(0.02);
    let axis = turn0 * Vec3::X;
    let mut pieces = Vec::new();
    match constraint.class() {
        ClassName::SpringConstraint => {
            pieces = coil(p0, p1, number("Coils"), number("Radius"), thickness);
        }
        ClassName::RopeConstraint | ClassName::RodConstraint | ClassName::PrismaticConstraint => {
            pieces.extend(piece(p0, p1, thickness));
        }
        ClassName::HingeConstraint | ClassName::CylindricalConstraint => {
            let half = axis * (AXLE_LENGTH / 2.0);
            pieces.extend(piece(p0 - half, p0 + half, 0.2));
        }
        ClassName::BallSocketConstraint => {
            pieces.push(Mat4::from_scale_rotation_translation(
                Vec3::splat(0.4),
                Quat::IDENTITY,
                p0,
            ));
        }
        _ => {}
    }
    let ball = constraint.class() == ClassName::BallSocketConstraint;
    pieces
        .into_iter()
        .map(|transform| DrawItem {
            shape: if ball { Shape::Sphere } else { Shape::Cylinder },
            transform,
            material: material.clone(),
            uv_tile: 0.0,
            casts_shadow: false,
            id: 0,
            cull_distance: 0.0,
            skin: None,
        })
        .collect()
}

/// Where the Attachment named by `constraint`'s property `name` is in the
/// world, and how it's turned, if it's on a part.
fn attachment_pose(world: &World, constraint: &Instance, name: &str) -> Option<(Vec3, Quat)> {
    let attachment = world.get(constraint.property(name)?.as_instance()?)?;
    if attachment.class() != ClassName::Attachment {
        return None;
    }
    let part = world.get(attachment.parent()?)?;
    if !matches!(part.class(), ClassName::Part | ClassName::MeshPart) {
        return None;
    }
    let vector = |instance: &Instance, name| {
        let v = instance.property(name).and_then(Value::as_vector3)?;
        Some(Vec3::new(v.x, v.y, v.z))
    };
    let turn = |instance: &Instance| {
        let v = instance
            .property("Orientation")
            .and_then(Value::as_vector3)?;
        Some(rotation_from_orientation(v))
    };
    let (at, part_turn) = (vector(part, "Position")?, turn(part)?);
    let (offset, own_turn) = (vector(attachment, "Position")?, turn(attachment)?);
    let placed = (at + part_turn * offset, part_turn * own_turn);
    placed.0.is_finite().then_some(placed)
}

/// A cylinder `thickness` across from `a` to `b`, or nothing if they meet.
/// The 1×1×1 cylinder lies along X.
fn piece(a: Vec3, b: Vec3, thickness: f32) -> Option<Mat4> {
    let along = b - a;
    let length = along.length();
    (length > 1e-4).then(|| {
        Mat4::from_scale_rotation_translation(
            Vec3::new(length, thickness, thickness),
            Quat::from_rotation_arc(Vec3::X, along / length),
            (a + b) / 2.0,
        )
    })
}

/// A coil of `coils` turns of `radius` from `a` to `b`, as straight pieces.
fn coil(a: Vec3, b: Vec3, coils: f32, radius: f32, thickness: f32) -> Vec<Mat4> {
    let along = b - a;
    if along.length() < 1e-4 {
        return Vec::new();
    }
    let forward = along.normalize();
    let side = forward.any_orthonormal_vector();
    let up = forward.cross(side);
    let count = (coils.clamp(1.0, MAX_COILS) * PIECES_PER_COIL as f32).round() as usize;
    let point = |i: usize| {
        let t = i as f32 / count as f32;
        let angle = t * coils.clamp(1.0, MAX_COILS) * std::f32::consts::TAU;
        a + along * t + (side * angle.cos() + up * angle.sin()) * radius
    };
    (0..count)
        .filter_map(|i| piece(point(i), point(i + 1), thickness))
        .collect()
}

#[cfg(test)]
mod tests {
    use axcenity_core::{Vector3, World};

    use super::*;

    #[test]
    fn a_visible_spring_is_drawn_as_a_coil_between_its_attachments() {
        let mut world = World::new();
        let mut attachment = |x: f32| {
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(world.workspace())).unwrap();
            let position = Value::Vector3(Vector3::new(x, 0.0, 0.0));
            world.set_property(part, "Position", position).unwrap();
            let attachment = world.create(ClassName::Attachment).unwrap();
            world.set_parent(attachment, Some(part)).unwrap();
            attachment
        };
        let (a0, a1) = (attachment(0.0), attachment(6.0));
        let spring = world.create(ClassName::SpringConstraint).unwrap();
        world.set_parent(spring, Some(world.workspace())).unwrap();
        world
            .set_property(spring, "Attachment0", Value::Instance(Some(a0)))
            .unwrap();
        world
            .set_property(spring, "Attachment1", Value::Instance(Some(a1)))
            .unwrap();
        let draw = |world: &World| draw_items(world, world.get(spring).unwrap());
        assert!(draw(&world).is_empty(), "hidden by default");
        world
            .set_property(spring, "Visible", Value::Bool(true))
            .unwrap();
        let pieces = draw(&world);
        assert_eq!(pieces.len(), 8 * PIECES_PER_COIL);
        // The coil runs from one attachment to the other, Radius out.
        for item in &pieces {
            let middle = item.transform.w_axis;
            assert!(middle.x > -0.01 && middle.x < 6.01, "{middle:?}");
            let out = (middle.y * middle.y + middle.z * middle.z).sqrt();
            assert!(out < 0.41, "{out}");
        }
    }
}
