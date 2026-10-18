//! Turns the Instance tree into a draw list for `render`.
//!
//! This is the only place that knows about both Instances and the renderer,
//! which keeps `render` free of engine types. The player and the editor
//! viewport both use it, so they draw a world the same way.

use std::sync::Arc;

use axcenity_assets::{AlphaMode, Assets, ChannelTexture, Image, Model, ModelMaterial};
use axcenity_core::{ClassName, Instance, PartType, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{
    AmbientLight, Camera, Channel, ChannelMap, DrawItem, DrawList, Material, MeshSource, NormalMap,
    Shape, Sun, TextureMaps, TextureSource,
};

use crate::orientation::rotation_from_orientation;

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

/// The draw list for everything in the Workspace, seen from `camera`.
/// MeshParts' models come from `assets`, loaded the first time they're drawn.
pub fn build_draw_list(world: &World, assets: &Assets, camera: Camera) -> DrawList {
    let mut items = Vec::new();
    for instance in world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| world.get(id))
    {
        match instance.class() {
            ClassName::MeshPart => mesh_part_draw_items(instance, assets, &mut items),
            _ => items.extend(part_draw_item(instance)),
        }
    }

    DrawList {
        sky_color: SKY_COLOR,
        camera,
        sun: SUN,
        ambient: AMBIENT,
        items,
    }
}

/// A Part's shape and the transform that places its 1×1×1 mesh in the world,
/// or `None` for anything that isn't a part. Drawing and picking both use
/// this, so a click hits exactly what's drawn. A MeshPart counts as a Block:
/// its model fills that box, and it's picked and outlined by the box.
pub(crate) fn part_placement(instance: &Instance) -> Option<(PartType, Mat4)> {
    let vector = |name| {
        let v = instance.property(name).and_then(Value::as_vector3)?;
        Some(Vec3::new(v.x, v.y, v.z))
    };
    // Only parts have a Shape (or are MeshParts), so anything else stops here.
    let shape = match instance.class() {
        ClassName::MeshPart => PartType::Block,
        _ => instance.property("Shape").and_then(Value::as_part_type)?,
    };
    let orientation = instance
        .property("Orientation")
        .and_then(Value::as_vector3)?;
    let transform = Mat4::from_scale_rotation_translation(
        mesh_scale(shape, vector("Size")?),
        rotation_from_orientation(orientation),
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
            ..Material::default()
        },
    })
}

/// A MeshPart's draw items, one per part of its model, each with the
/// model's own material. Until it has a model (no `MeshId` yet, or one that
/// can't be loaded), it's drawn as a plain grey box, so it can still be seen
/// and picked.
fn mesh_part_draw_items(instance: &Instance, assets: &Assets, items: &mut Vec<DrawItem>) {
    let Some((_, transform)) = part_placement(instance) else {
        return;
    };
    let mesh_id = instance
        .property("MeshId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(model) = assets.model(mesh_id) else {
        items.push(DrawItem {
            shape: Shape::Block,
            transform,
            material: Material::default(),
        });
        return;
    };
    for (index, primitive) in model.primitives.iter().enumerate() {
        items.push(DrawItem {
            shape: Shape::Mesh(Arc::new(ModelMesh {
                model: model.clone(),
                index,
            })),
            transform,
            material: model_material(&primitive.material),
        });
    }
}

/// glTF's material values, as the renderer takes them.
fn model_material(material: &ModelMaterial) -> Material {
    let [r, g, b, alpha] = material.base_color;
    let (opacity, alpha_cutoff) = match material.alpha_mode {
        AlphaMode::Opaque => (1.0, 0.0),
        AlphaMode::Mask { cutoff } => (1.0, cutoff),
        // Kept below 1 so the item is drawn see-through, even when only its
        // texture has the transparency.
        AlphaMode::Blend => (alpha.min(0.999), 0.0),
    };
    Material {
        // glTF colors are linear light; draw lists take sRGB.
        base_color: srgb([r, g, b]),
        base_color_texture: material.base_color_texture.as_ref().map(texture),
        metalness: material.metalness,
        roughness: material.roughness,
        emissive: srgb(material.emissive),
        opacity,
        alpha_cutoff,
        maps: TextureMaps {
            normal: material.normal_texture.as_ref().map(|image| NormalMap {
                texture: texture(image),
                scale: material.normal_scale,
                flip_green: material.normal_flip_green,
            }),
            metalness: material.metalness_texture.as_ref().map(channel_map),
            roughness: material.roughness_texture.as_ref().map(channel_map),
            occlusion: material.occlusion_texture.as_ref().map(channel_map),
            occlusion_strength: material.occlusion_strength,
            emissive: material.emissive_texture.as_ref().map(texture),
            opacity: material.opacity_texture.as_ref().map(channel_map),
            specular: material.specular_texture.as_ref().map(texture),
        },
    }
}

/// A model's image, handed to the renderer without copying it.
fn texture(image: &Arc<Image>) -> Arc<dyn TextureSource> {
    Arc::new(ImageTexture(image.clone()))
}

fn channel_map(map: &ChannelTexture) -> ChannelMap {
    ChannelMap {
        texture: texture(&map.image),
        channel: match map.channel {
            axcenity_assets::Channel::Red => Channel::Red,
            axcenity_assets::Channel::Green => Channel::Green,
            axcenity_assets::Channel::Blue => Channel::Blue,
            axcenity_assets::Channel::Alpha => Channel::Alpha,
        },
        invert: map.invert,
    }
}

/// Converts linear light to sRGB, the opposite of what the renderer does.
fn srgb(linear: [f32; 3]) -> [f32; 3] {
    linear.map(|c| {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// One primitive of a model, handed to the renderer without copying it.
struct ModelMesh {
    model: Arc<Model>,
    index: usize,
}

impl MeshSource for ModelMesh {
    fn key(&self) -> u64 {
        self.model.primitives[self.index].key()
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.model.primitives[self.index].positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.model.primitives[self.index].normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.model.primitives[self.index].uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.model.primitives[self.index].tangents
    }
    fn indices(&self) -> &[u32] {
        &self.model.primitives[self.index].indices
    }
}

/// A model's texture, handed to the renderer without copying it.
struct ImageTexture(Arc<Image>);

impl TextureSource for ImageTexture {
    fn key(&self) -> u64 {
        self.0.key()
    }
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
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
        only_item_with(world, &Assets::default())
    }

    fn only_item_with(world: &World, assets: &Assets) -> DrawItem {
        let list = build_draw_list(world, assets, camera());
        assert_eq!(list.items.len(), 1);
        list.items[0].clone()
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
        let shapes: Vec<Shape> = build_draw_list(&world, &Assets::default(), camera())
            .items
            .iter()
            .map(|item| item.shape.clone())
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
        assert!(
            build_draw_list(&world, &Assets::default(), camera())
                .items
                .is_empty()
        );
    }

    fn example_assets() -> Assets {
        Assets::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh"),
        )
    }

    fn mesh_part(world: &mut World, mesh_id: &str) -> InstanceId {
        let part = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "MeshId", Value::String(mesh_id.to_owned()))
            .unwrap();
        world
            .set_property(part, "Size", vector(8.0, 6.0, 8.0))
            .unwrap();
        part
    }

    #[test]
    fn a_mesh_part_draws_its_model_with_the_files_material() {
        let mut world = World::new();
        mesh_part(&mut world, "assets/models/pyramid.gltf");
        let assets = example_assets();
        let item = only_item_with(&world, &assets);
        let Shape::Mesh(mesh) = &item.shape else {
            panic!("drawn as {:?}", item.shape);
        };
        assert_eq!(mesh.positions().len(), mesh.uvs().len());
        // Size stretches the model's 1×1×1 box, like a block's.
        assert_eq!(
            item.transform.transform_point3(Vec3::splat(0.5)),
            Vec3::new(4.0, 3.0, 4.0)
        );
        assert!(item.material.base_color_texture.is_some());
        assert_eq!(item.material.roughness, 0.8);
        assert_eq!(item.material.opacity, 1.0);
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn a_mesh_part_without_a_model_is_a_grey_box_and_the_reason_is_reported() {
        let assets = example_assets();
        for mesh_id in ["", "assets/models/missing.glb"] {
            let mut world = World::new();
            mesh_part(&mut world, mesh_id);
            let item = only_item_with(&world, &assets);
            assert_eq!(item.shape, Shape::Block, "{mesh_id:?}");
        }
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("missing.glb"), "{errors:?}");
    }

    #[test]
    fn linear_to_srgb_undoes_the_renderers_conversion() {
        let percent = srgb([0.0, 1.0, 0.214]).map(|c| (c * 100.0).round());
        assert_eq!(percent, [0.0, 100.0, 50.0]);
    }
}
