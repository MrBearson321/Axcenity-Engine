//! Turns the Instance tree into a draw list for `render`.
//!
//! This is the only place that knows about both Instances and the renderer,
//! which keeps `render` free of engine types. The player and the editor
//! viewport both use it, so they draw a world the same way.

use axcenity_core::{Instance, PartType, Value, World};
use axcenity_render::glam::{EulerRot, Mat4, Quat, Vec3};
use axcenity_render::{AmbientLight, Camera, DrawItem, DrawList, Material, Shape, Sun};

// Fixed lighting until the Lighting service exists.
const SKY_COLOR: [f32; 3] = [0.63, 0.8, 0.95];
const SUN: Sun = Sun {
    direction: Vec3::new(0.5, 1.0, 0.25),
    color: [1.0, 0.97, 0.92],
    intensity: 3.0,
};
const AMBIENT: AmbientLight = AmbientLight {
    color: [0.75, 0.85, 1.0],
    intensity: 0.25,
};

pub fn build_draw_list(world: &World, camera: Camera) -> DrawList {
    let items = world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| world.get(id))
        .filter_map(part_draw_item)
        .collect();

    DrawList {
        sky_color: SKY_COLOR,
        camera,
        sun: SUN,
        ambient: AMBIENT,
        items,
    }
}

/// A Part's shape and the transform that places its 1×1×1 mesh in the world,
/// or `None` for any other class. Drawing and picking both use this, so a
/// click hits exactly what's drawn.
pub(crate) fn part_placement(instance: &Instance) -> Option<(PartType, Mat4)> {
    let vector = |name| {
        let v = instance.property(name).and_then(Value::as_vector3)?;
        Some(Vec3::new(v.x, v.y, v.z))
    };
    // Only Parts have a Shape, so anything else stops here.
    let shape = instance.property("Shape").and_then(Value::as_part_type)?;
    let [x, y, z] = vector("Orientation")?.to_array().map(f32::to_radians);
    // Z first, then X, then Y, matching how `Orientation` is defined.
    let rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
    let transform = Mat4::from_scale_rotation_translation(
        mesh_scale(shape, vector("Size")?),
        rotation,
        vector("Position")?,
    );
    Some((shape, transform))
}

/// The draw item for a Part, or `None` for any other class.
fn part_draw_item(instance: &Instance) -> Option<DrawItem> {
    let number = |name| instance.property(name).and_then(Value::as_number);
    let (shape, transform) = part_placement(instance)?;
    let color = instance.property("Color").and_then(Value::as_color3)?;
    let preset = instance
        .property("Material")
        .and_then(Value::as_material)?
        .preset();

    let color = [color.r, color.g, color.b];
    Some(DrawItem {
        shape: match shape {
            PartType::Block => Shape::Block,
            PartType::Ball => Shape::Sphere,
            PartType::Cylinder => Shape::Cylinder,
            PartType::Wedge => Shape::Wedge,
        },
        transform,
        material: Material {
            // A glowing part shows its color as light, the same on every side,
            // instead of being shaded by the sun.
            base_color: if preset.glows { [0.0; 3] } else { color },
            emissive: if preset.glows { color } else { [0.0; 3] },
            metalness: number("Metalness")? as f32,
            roughness: number("Roughness")? as f32,
            opacity: preset.opacity,
        },
    })
}

/// How much to stretch the 1×1×1 mesh. Balls stay round and cylinders stay
/// circular, whatever their `Size`: they use the smallest size that fits.
fn mesh_scale(shape: PartType, size: Vec3) -> Vec3 {
    match shape {
        PartType::Block | PartType::Wedge => size,
        PartType::Ball => Vec3::splat(size.min_element()),
        PartType::Cylinder => {
            let diameter = size.y.min(size.z);
            Vec3::new(size.x, diameter, diameter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Color3, InstanceId, Material as PartMaterial, Vector3};

    fn camera() -> Camera {
        Camera {
            position: Vec3::Z,
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        }
    }

    fn part(world: &mut World, properties: Vec<(&str, Value)>) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        for (name, value) in properties {
            world.set_property(part, name, value).unwrap();
        }
        part
    }

    fn only_item(world: &World) -> DrawItem {
        let list = build_draw_list(world, camera());
        assert_eq!(list.items.len(), 1);
        list.items[0]
    }

    fn vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3(Vector3::new(x, y, z))
    }

    #[test]
    fn parts_become_draw_items_with_their_properties() {
        let mut world = World::new();
        part(
            &mut world,
            vec![
                ("Position", vector(1.0, 2.0, 3.0)),
                ("Size", vector(4.0, 5.0, 6.0)),
                ("Color", Value::Color3(Color3::new(1.0, 0.5, 0.0))),
                ("Metalness", Value::Number(1.0)),
                ("Roughness", Value::Number(0.25)),
            ],
        );

        let item = only_item(&world);
        assert_eq!(item.shape, Shape::Block);
        assert_eq!(
            item.transform.transform_point3(Vec3::splat(0.5)),
            Vec3::new(3.0, 4.5, 6.0)
        );
        assert_eq!(item.material.base_color, [1.0, 0.5, 0.0]);
        assert_eq!(item.material.metalness, 1.0);
        assert_eq!(item.material.roughness, 0.25);
        assert_eq!(item.material.opacity, 1.0);
    }

    #[test]
    fn orientation_turns_around_y_after_x_after_z() {
        let mut world = World::new();
        part(
            &mut world,
            vec![
                ("Size", vector(4.0, 1.0, 2.0)),
                ("Orientation", vector(90.0, 90.0, 0.0)),
            ],
        );
        let item = only_item(&world);
        // The part's long (+X) side: X then Y turns it from +X to -Z.
        let end = item.transform.transform_point3(Vec3::new(0.5, 0.0, 0.0));
        assert!(end.abs_diff_eq(Vec3::new(0.0, 0.0, -2.0), 1e-5), "{end}");
        // Its top (+Y): 90° around X points it at +Z, then 90° around Y at +X.
        let top = item.transform.transform_point3(Vec3::new(0.0, 0.5, 0.0));
        assert!(top.abs_diff_eq(Vec3::new(0.5, 0.0, 0.0), 1e-5), "{top}");
    }

    #[test]
    fn balls_and_cylinders_keep_round() {
        assert_eq!(
            mesh_scale(PartType::Ball, Vec3::new(4.0, 1.0, 2.0)),
            Vec3::ONE
        );
        assert_eq!(
            mesh_scale(PartType::Cylinder, Vec3::new(4.0, 3.0, 2.0)),
            Vec3::new(4.0, 2.0, 2.0)
        );
        assert_eq!(
            mesh_scale(PartType::Wedge, Vec3::new(4.0, 3.0, 2.0)),
            Vec3::new(4.0, 3.0, 2.0)
        );
    }

    #[test]
    fn glass_is_see_through_and_neon_glows_in_its_color() {
        let color = Color3::new(0.2, 0.9, 1.0);
        for (material, opacity, glows) in [
            (PartMaterial::Glass, 0.3, false),
            (PartMaterial::Neon, 1.0, true),
        ] {
            let mut world = World::new();
            part(
                &mut world,
                vec![
                    ("Material", Value::Material(material)),
                    ("Color", Value::Color3(color)),
                ],
            );
            let item = only_item(&world);
            assert_eq!(item.material.opacity, opacity, "{material:?}");
            let expected_glow = if glows { [0.2, 0.9, 1.0] } else { [0.0; 3] };
            assert_eq!(item.material.emissive, expected_glow, "{material:?}");
        }
    }

    #[test]
    fn every_shape_has_a_mesh() {
        let mut world = World::new();
        for shape in PartType::ALL {
            part(&mut world, vec![("Shape", Value::PartType(shape))]);
        }
        let shapes: Vec<Shape> = build_draw_list(&world, camera())
            .items
            .iter()
            .map(|item| item.shape)
            .collect();
        assert_eq!(
            shapes,
            [Shape::Block, Shape::Sphere, Shape::Cylinder, Shape::Wedge]
        );
    }

    #[test]
    fn parts_outside_workspace_are_not_drawn() {
        let mut world = World::new();
        world.create(ClassName::Part).unwrap();
        assert!(build_draw_list(&world, camera()).items.is_empty());
    }
}
