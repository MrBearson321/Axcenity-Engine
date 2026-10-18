//! Turns the Instance tree into a draw list for `render`.
//!
//! This is the only place that knows about both Instances and the renderer,
//! which keeps `render` free of engine types.

use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{AmbientLight, Camera, DrawItem, DrawList, Material, Shape, Sun};

// Fixed lighting and camera until the Lighting service and camera controls exist.
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
const CAMERA: Camera = Camera {
    position: Vec3::new(7.0, 5.0, 9.0),
    look_at: Vec3::ZERO,
    fov_y_degrees: 50.0,
};

pub fn build_draw_list(world: &World) -> DrawList {
    let items = world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| world.get(id))
        .filter_map(part_draw_item)
        .collect();

    DrawList {
        sky_color: SKY_COLOR,
        camera: CAMERA,
        sun: SUN,
        ambient: AMBIENT,
        items,
    }
}

/// The draw item for a Part, or `None` for any other class.
fn part_draw_item(instance: &Instance) -> Option<DrawItem> {
    if instance.class() != ClassName::Part {
        return None;
    }

    let vector = |name| {
        let v = instance.property(name).and_then(Value::as_vector3)?;
        Some(Vec3::new(v.x, v.y, v.z))
    };
    let number = |name| instance.property(name).and_then(Value::as_number);
    let color = instance.property("Color").and_then(Value::as_color3)?;

    let position = vector("Position")?;
    let size = vector("Size")?;
    Some(DrawItem {
        shape: Shape::Block,
        transform: Mat4::from_translation(position) * Mat4::from_scale(size),
        material: Material {
            base_color: [color.r, color.g, color.b],
            metalness: number("Metalness")? as f32,
            roughness: number("Roughness")? as f32,
            ..Material::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Color3, Vector3};

    #[test]
    fn parts_become_blocks_with_their_properties() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let set = |world: &mut World, name, value| world.set_property(part, name, value).unwrap();
        set(
            &mut world,
            "Position",
            Value::Vector3(Vector3::new(1.0, 2.0, 3.0)),
        );
        set(
            &mut world,
            "Size",
            Value::Vector3(Vector3::new(4.0, 5.0, 6.0)),
        );
        set(
            &mut world,
            "Color",
            Value::Color3(Color3::new(1.0, 0.5, 0.0)),
        );
        set(&mut world, "Metalness", Value::Number(1.0));
        set(&mut world, "Roughness", Value::Number(0.25));

        let list = build_draw_list(&world);
        assert_eq!(list.items.len(), 1);
        let item = &list.items[0];
        assert_eq!(item.shape, Shape::Block);
        assert_eq!(
            item.transform.transform_point3(Vec3::splat(0.5)),
            Vec3::new(3.0, 4.5, 6.0)
        );
        assert_eq!(item.material.base_color, [1.0, 0.5, 0.0]);
        assert_eq!(item.material.metalness, 1.0);
        assert_eq!(item.material.roughness, 0.25);
    }

    #[test]
    fn parts_outside_workspace_are_not_drawn() {
        let mut world = World::new();
        world.create(ClassName::Part).unwrap();
        assert!(build_draw_list(&world).items.is_empty());
    }
}
