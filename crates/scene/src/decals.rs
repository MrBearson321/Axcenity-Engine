//! Decals (Phase 75): a Part's `Decal`, drawn on a square laid on one face
//! of the part's box, and a `ProjectedDecal`, handed to the renderer as a
//! box that paints the surfaces inside it (`docs/rendering/decals.md`).
//! Characters' items are marked so projected decals can leave them bare.

use std::collections::HashMap;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, InstanceId, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use axcenity_render::{Decal, DecalRole, DrawItem, Material, Shape, decal_quad};

use crate::draw_list::{draw_id, image_texture};
use crate::orientation::rotation_from_orientation;

/// How far a Decal's square sits off its face, in studs: a hair, so it
/// stays on the face's side close up (the renderer's depth nudge covers it
/// far away).
const FACE_OFFSET: f32 = 0.002;

/// How rough a Decal's surface is.
const DECAL_ROUGHNESS: f32 = 0.8;

fn vector(instance: &Instance, name: &str) -> Option<Vec3> {
    let v = instance.property(name).and_then(Value::as_vector3)?;
    Some(Vec3::new(v.x, v.y, v.z))
}

fn number(instance: &Instance, name: &str, default: f32) -> f32 {
    instance
        .property(name)
        .and_then(Value::as_number)
        .map_or(default, |n| n as f32)
}

/// The box's place and turn (without its size), and its size.
fn box_of(instance: &Instance) -> Option<(Mat4, Vec3)> {
    let orientation = instance
        .property("Orientation")
        .and_then(Value::as_vector3)?;
    let placed = Mat4::from_rotation_translation(
        rotation_from_orientation(orientation),
        vector(instance, "Position")?,
    );
    Some((placed, vector(instance, "Size")?))
}

/// 1 minus `Transparency`, from 0 to 1.
fn opacity(instance: &Instance) -> f32 {
    (1.0 - number(instance, "Transparency", 0.0)).clamp(0.0, 1.0)
}

fn color(instance: &Instance) -> [f32; 3] {
    instance
        .property("Color3")
        .and_then(Value::as_color3)
        .map_or([1.0; 3], |c| [c.r, c.g, c.b])
}

fn picture(
    instance: &Instance,
    assets: &Assets,
) -> Option<std::sync::Arc<dyn axcenity_render::TextureSource>> {
    let path = instance.property("Texture").and_then(Value::as_str)?;
    if path.is_empty() {
        return None;
    }
    assets.texture(path).map(|image| image_texture(&image))
}

/// For a face (`NormalId`), its outward direction in the part's space and
/// the picture's right and up across it, seen from outside (each pair's
/// cross product is the outward direction).
fn face_axes(face: &str) -> (Vec3, Vec3, Vec3) {
    match face {
        "Right" => (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        "Left" => (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        "Top" => (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        "Bottom" => (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        "Back" => (Vec3::Z, Vec3::X, Vec3::Y),
        // Front, and anything unknown.
        _ => (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    }
}

/// The draw item for `decal`, a Decal inside `part`: a square covering the
/// face of the part's box it names. `None` when it's fully transparent or
/// not in a part.
pub(crate) fn face_decal_item(
    decal_id: InstanceId,
    decal: &Instance,
    part: &Instance,
    assets: &Assets,
) -> Option<DrawItem> {
    if !part.class().is_base_part() {
        return None;
    }
    let opacity = opacity(decal);
    if opacity <= 0.0 {
        return None;
    }
    let (placed, size) = box_of(part)?;
    let face = decal
        .property("Face")
        .and_then(|value| match value {
            Value::EnumItem(item) => Some(item.item.as_str()),
            _ => None,
        })
        .unwrap_or("Front");
    let (out, right, up) = face_axes(face);
    let (width, height) = (right.abs().dot(size), up.abs().dot(size));
    let center = out * (out.abs().dot(size) * 0.5 + FACE_OFFSET);
    // The square (1×1, facing +Z) stretched over the face, in the part.
    let on_face = Mat4::from_cols(
        (right * width).extend(0.0),
        (up * height).extend(0.0),
        out.extend(0.0),
        center.extend(1.0),
    );
    // Repeated every so many studs, or stretched once across.
    let tile = |name, across: f32| {
        let studs = number(decal, name, 0.0);
        if studs.is_finite() && studs > 0.0 {
            across / studs
        } else {
            1.0
        }
    };
    let material = Material {
        base_color: color(decal),
        base_color_texture: picture(decal, assets),
        roughness: DECAL_ROUGHNESS,
        metalness: 0.0,
        opacity,
        uv_scale: [tile("StudsPerTileU", width), tile("StudsPerTileV", height)],
        ..Material::default()
    };
    Some(DrawItem {
        shape: Shape::Mesh(decal_quad()),
        transform: placed * on_face,
        material,
        uv_tile: 0.0,
        casts_shadow: false,
        id: draw_id(decal_id),
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: DecalRole::Decal,
    })
}

/// A ProjectedDecal as the renderer draws it, or `None` when it shows
/// nothing (fully transparent, or a box with no size).
pub(crate) fn projected_decal(instance: &Instance, assets: &Assets) -> Option<Decal> {
    let opacity = opacity(instance);
    let (placed, size) = box_of(instance)?;
    if opacity <= 0.0 || size.min_element() <= 0.0 || !size.is_finite() {
        return None;
    }
    Some(Decal {
        transform: placed * Mat4::from_scale(size),
        texture: picture(instance, assets),
        color: color(instance),
        opacity,
        fade_angle: number(instance, "FadeAngle", 75.0),
        ignore_characters: instance
            .property("IgnoreCharacters")
            .and_then(Value::as_bool)
            != Some(false),
    })
}

/// Which Instances hold a character: a character controller or Humanoid
/// among their children. Remembered per Instance while one draw list is
/// built.
#[derive(Default)]
pub(crate) struct Characters(HashMap<InstanceId, bool>);

impl Characters {
    /// Whether `id` is part of a character: it or something it's inside
    /// holds a character controller or Humanoid.
    pub(crate) fn contains(&mut self, world: &World, id: InstanceId) -> bool {
        let mut at = world.get(id).and_then(Instance::parent);
        let mut path = Vec::new();
        let mut found = false;
        while let Some(ancestor) = at {
            if let Some(&known) = self.0.get(&ancestor) {
                found = known;
                break;
            }
            path.push(ancestor);
            let Some(instance) = world.get(ancestor) else {
                break;
            };
            let holds = instance.children().iter().any(|&child| {
                world.get(child).is_some_and(|child| {
                    child.class().is_character_controller() || child.class() == ClassName::Humanoid
                })
            });
            if holds {
                found = true;
                break;
            }
            at = instance.parent();
        }
        // Everything walked through is inside (or isn't inside) the same
        // character.
        for ancestor in path {
            self.0.entry(ancestor).or_insert(found);
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{Color3, EnumItem, Vector3};

    use super::*;

    fn part(world: &mut World, size: Vector3) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        for (name, value) in [
            ("Size", Value::Vector3(size)),
            ("Position", Value::Vector3(Vector3::new(10.0, 5.0, 0.0))),
            ("Orientation", Value::Vector3(Vector3::new(0.0, 90.0, 0.0))),
        ] {
            world.set_property(part, name, value).unwrap();
        }
        part
    }

    fn decal_on(world: &mut World, part: InstanceId, face: &str) -> InstanceId {
        let decal = world.create(ClassName::Decal).unwrap();
        world.set_parent(decal, Some(part)).unwrap();
        world
            .set_property(
                decal,
                "Face",
                Value::EnumItem(EnumItem {
                    enum_name: "NormalId".to_owned(),
                    item: face.to_owned(),
                }),
            )
            .unwrap();
        decal
    }

    fn item(world: &World, decal: InstanceId) -> DrawItem {
        let instance = world.get(decal).unwrap();
        let part = world.get(instance.parent().unwrap()).unwrap();
        face_decal_item(decal, instance, part, &Assets::default()).unwrap()
    }

    #[test]
    fn a_decal_covers_its_face_facing_out_with_its_top_up() {
        let mut world = World::new();
        let part = part(&mut world, Vector3::new(4.0, 2.0, 6.0));
        let placed = Mat4::from_rotation_translation(
            axcenity_render::glam::Quat::from_rotation_y(90f32.to_radians()),
            Vec3::new(10.0, 5.0, 0.0),
        );
        for face in ["Right", "Left", "Top", "Bottom", "Back", "Front"] {
            let decal = decal_on(&mut world, part, face);
            let item = item(&world, decal);
            assert_eq!(item.decal_role, DecalRole::Decal);
            assert!(!item.casts_shadow);
            // In the part's space: the square's facing is the face's
            // outward direction, its middle just off the face, and it
            // spans the face.
            let local = placed.inverse() * item.transform;
            let (out, right, up) = face_axes(face);
            let facing = local.z_axis.truncate();
            assert!(facing.abs_diff_eq(out, 1e-5), "{face}: {facing}");
            let size = Vec3::new(4.0, 2.0, 6.0);
            let middle = local.w_axis.truncate();
            let off = middle.dot(out) - out.abs().dot(size) * 0.5;
            assert!((off - FACE_OFFSET).abs() < 1e-5, "{face}: {middle}");
            assert!(
                local
                    .x_axis
                    .truncate()
                    .abs_diff_eq(right * right.abs().dot(size), 1e-5),
                "{face}"
            );
            assert!(
                local
                    .y_axis
                    .truncate()
                    .abs_diff_eq(up * up.abs().dot(size), 1e-5)
            );
            // Right × up is out, so the square faces out unmirrored.
            assert!(right.cross(up).abs_diff_eq(out, 1e-6), "{face}");
            world.destroy(decal).unwrap();
        }
    }

    #[test]
    fn a_decal_takes_its_color_transparency_and_tiling() {
        let mut world = World::new();
        let part = part(&mut world, Vector3::new(8.0, 4.0, 1.0));
        let decal = decal_on(&mut world, part, "Front");
        for (name, value) in [
            ("Color3", Value::Color3(Color3::new(1.0, 0.5, 0.0))),
            ("Transparency", Value::Number(0.25)),
            ("StudsPerTileU", Value::Number(2.0)),
        ] {
            world.set_property(decal, name, value).unwrap();
        }
        let item = item(&world, decal);
        assert_eq!(item.material.base_color, [1.0, 0.5, 0.0]);
        assert_eq!(item.material.opacity, 0.75);
        // 8 studs across at 2 studs a copy; stretched once down.
        assert_eq!(item.material.uv_scale, [4.0, 1.0]);

        world
            .set_property(decal, "Transparency", Value::Number(1.0))
            .unwrap();
        let instance = world.get(decal).unwrap();
        assert!(
            face_decal_item(
                decal,
                instance,
                world.get(part).unwrap(),
                &Assets::default()
            )
            .is_none()
        );
    }

    #[test]
    fn a_projected_decal_is_its_box() {
        let mut world = World::new();
        let id = world.create(ClassName::ProjectedDecal).unwrap();
        world.set_parent(id, Some(world.workspace())).unwrap();
        world
            .set_property(id, "Position", Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
            .unwrap();
        let decal = projected_decal(world.get(id).unwrap(), &Assets::default()).unwrap();
        // Facing up by default: its front (-Z) turned to +Y.
        let facing = -decal.transform.z_axis.truncate().normalize();
        assert!(facing.abs_diff_eq(Vec3::Y, 1e-5), "{facing}");
        assert_eq!(decal.transform.w_axis.truncate(), Vec3::new(1.0, 2.0, 3.0));
        assert!((decal.transform.z_axis.truncate().length() - 2.0).abs() < 1e-5);
        assert_eq!(decal.fade_angle, 75.0);
        assert!(decal.ignore_characters);
        assert!(decal.texture.is_none());
    }

    #[test]
    fn parts_inside_a_model_with_a_controller_or_humanoid_are_a_characters() {
        let mut world = World::new();
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(world.workspace())).unwrap();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(model)).unwrap();
        let arm = world.create(ClassName::Part).unwrap();
        world.set_parent(arm, Some(folder)).unwrap();
        let other = world.create(ClassName::Part).unwrap();
        world.set_parent(other, Some(world.workspace())).unwrap();
        let mut characters = Characters::default();
        assert!(!characters.contains(&world, arm));

        let humanoid = world.create(ClassName::Humanoid).unwrap();
        world.set_parent(humanoid, Some(model)).unwrap();
        let mut characters = Characters::default();
        assert!(characters.contains(&world, arm));
        assert!(!characters.contains(&world, other));
        // Remembered: asked again, the same.
        assert!(characters.contains(&world, arm));
    }
}
