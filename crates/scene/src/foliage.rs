//! Painted plants, props, and models as draw items. They sit on the terrain
//! surface and are not Instances, so sculpting moves them with the ground.

use axcenity_assets::Assets;
use axcenity_core::{
    FoliageInstance, FoliagePlant, FoliageProp, FoliageSource, PartType, TerrainData, Vector3,
};
use axcenity_render::glam::{Mat4, Quat, Vec2, Vec3};
use axcenity_render::{DrawItem, Material, Shape};
use axcenity_terrain::Placement;

use crate::draw_list::push_model;
use crate::orientation::rotation_from_orientation;

/// One part of a built-in plant or prop, in the instance's local space:
/// origin on the ground, Y up.
struct Piece {
    shape: PartType,
    size: Vec3,
    position: Vec3,
    orientation: Vector3,
    color: [f32; 3],
    metalness: f32,
    roughness: f32,
    glow: bool,
}

const GRASS: [Piece; 2] = [
    Piece {
        shape: PartType::Wedge,
        size: Vec3::new(0.12, 1.1, 0.55),
        position: Vec3::new(0.0, 0.55, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.32, 0.62, 0.22],
        metalness: 0.0,
        roughness: 0.85,
        glow: false,
    },
    Piece {
        shape: PartType::Wedge,
        size: Vec3::new(0.55, 1.1, 0.12),
        position: Vec3::new(0.0, 0.55, 0.0),
        orientation: Vector3::new(0.0, 90.0, 0.0),
        color: [0.28, 0.56, 0.2],
        metalness: 0.0,
        roughness: 0.85,
        glow: false,
    },
];

const BUSH: [Piece; 1] = [Piece {
    shape: PartType::Ball,
    size: Vec3::new(2.2, 2.2, 2.2),
    position: Vec3::new(0.0, 1.0, 0.0),
    orientation: Vector3::new(0.0, 0.0, 0.0),
    color: [0.22, 0.48, 0.2],
    metalness: 0.0,
    roughness: 0.8,
    glow: false,
}];

const FLOWER: [Piece; 2] = [
    Piece {
        shape: PartType::Cylinder,
        size: Vec3::new(0.7, 0.08, 0.08),
        position: Vec3::new(0.0, 0.35, 0.0),
        orientation: Vector3::new(0.0, 0.0, 90.0),
        color: [0.28, 0.55, 0.22],
        metalness: 0.0,
        roughness: 0.8,
        glow: false,
    },
    Piece {
        shape: PartType::Ball,
        size: Vec3::new(0.45, 0.45, 0.45),
        position: Vec3::new(0.0, 0.85, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.9, 0.35, 0.55],
        metalness: 0.0,
        roughness: 0.45,
        glow: false,
    },
];

const TREE: [Piece; 2] = [
    Piece {
        shape: PartType::Cylinder,
        size: Vec3::new(6.0, 1.2, 1.2),
        position: Vec3::new(0.0, 3.0, 0.0),
        orientation: Vector3::new(0.0, 0.0, 90.0),
        color: [0.45, 0.3, 0.18],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Ball,
        size: Vec3::new(6.0, 6.0, 6.0),
        position: Vec3::new(0.0, 7.5, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.29, 0.56, 0.25],
        metalness: 0.0,
        roughness: 0.8,
        glow: false,
    },
];

const CRATE: [Piece; 1] = [Piece {
    shape: PartType::Block,
    size: Vec3::new(3.0, 3.0, 3.0),
    position: Vec3::new(0.0, 1.5, 0.0),
    orientation: Vector3::new(0.0, 0.0, 0.0),
    color: [0.66, 0.48, 0.28],
    metalness: 0.0,
    roughness: 0.8,
    glow: false,
}];

const LAMP: [Piece; 3] = [
    Piece {
        shape: PartType::Block,
        size: Vec3::new(1.6, 0.4, 1.6),
        position: Vec3::new(0.0, 0.2, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.17, 0.17, 0.19],
        metalness: 1.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Cylinder,
        size: Vec3::new(8.0, 0.4, 0.4),
        position: Vec3::new(0.0, 4.4, 0.0),
        orientation: Vector3::new(0.0, 0.0, 90.0),
        color: [0.17, 0.17, 0.19],
        metalness: 1.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Ball,
        size: Vec3::new(1.2, 1.2, 1.2),
        position: Vec3::new(0.0, 8.9, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [1.0, 0.86, 0.55],
        metalness: 0.0,
        roughness: 0.5,
        glow: true,
    },
];

const TABLE: [Piece; 5] = [
    Piece {
        shape: PartType::Block,
        size: Vec3::new(6.0, 0.4, 4.0),
        position: Vec3::new(0.0, 3.2, 0.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.55, 0.38, 0.22],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Block,
        size: Vec3::new(0.4, 3.0, 0.4),
        position: Vec3::new(2.6, 1.5, 1.6),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.55, 0.38, 0.22],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Block,
        size: Vec3::new(0.4, 3.0, 0.4),
        position: Vec3::new(-2.6, 1.5, 1.6),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.55, 0.38, 0.22],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Block,
        size: Vec3::new(0.4, 3.0, 0.4),
        position: Vec3::new(2.6, 1.5, -1.6),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.55, 0.38, 0.22],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
    Piece {
        shape: PartType::Block,
        size: Vec3::new(0.4, 3.0, 0.4),
        position: Vec3::new(-2.6, 1.5, -1.6),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.55, 0.38, 0.22],
        metalness: 0.0,
        roughness: 0.5,
        glow: false,
    },
];

const FLOOR: [Piece; 1] = [Piece {
    shape: PartType::Block,
    size: Vec3::new(16.0, 1.0, 16.0),
    position: Vec3::new(0.0, 0.5, 0.0),
    orientation: Vector3::new(0.0, 0.0, 0.0),
    color: [0.39, 0.6, 0.34],
    metalness: 0.0,
    roughness: 0.8,
    glow: false,
}];

const WALL: [Piece; 1] = [Piece {
    shape: PartType::Block,
    size: Vec3::new(16.0, 8.0, 1.0),
    position: Vec3::new(0.0, 4.0, 0.0),
    orientation: Vector3::new(0.0, 0.0, 0.0),
    color: [0.82, 0.79, 0.72],
    metalness: 0.0,
    roughness: 0.8,
    glow: false,
}];

const RAMP: [Piece; 1] = [Piece {
    shape: PartType::Wedge,
    size: Vec3::new(4.0, 4.0, 8.0),
    position: Vec3::new(0.0, 2.0, 0.0),
    orientation: Vector3::new(0.0, 0.0, 0.0),
    color: [0.6, 0.6, 0.62],
    metalness: 0.0,
    roughness: 0.8,
    glow: false,
}];

const STAIRS: [Piece; 6] = [
    step(1.0, 0.5, 0.0),
    step(2.0, 1.0, -1.5),
    step(3.0, 1.5, -3.0),
    step(4.0, 2.0, -4.5),
    step(5.0, 2.5, -6.0),
    step(6.0, 3.0, -7.5),
];

const fn step(height: f32, y: f32, z: f32) -> Piece {
    Piece {
        shape: PartType::Block,
        size: Vec3::new(4.0, height, 1.5),
        position: Vec3::new(0.0, y, z),
        orientation: Vector3::new(0.0, 0.0, 0.0),
        color: [0.6, 0.6, 0.62],
        metalness: 0.0,
        roughness: 0.8,
        glow: false,
    }
}

fn pieces(source: &FoliageSource) -> &'static [Piece] {
    match source {
        FoliageSource::Plant(FoliagePlant::Grass) => &GRASS,
        FoliageSource::Plant(FoliagePlant::Bush) => &BUSH,
        FoliageSource::Plant(FoliagePlant::Flower) => &FLOWER,
        FoliageSource::Plant(FoliagePlant::Tree) => &TREE,
        FoliageSource::Prop(FoliageProp::Crate) => &CRATE,
        FoliageSource::Prop(FoliageProp::Lamp) => &LAMP,
        FoliageSource::Prop(FoliageProp::Table) => &TABLE,
        FoliageSource::Prop(FoliageProp::Tree) => &TREE,
        FoliageSource::Prop(FoliageProp::Floor) => &FLOOR,
        FoliageSource::Prop(FoliageProp::Wall) => &WALL,
        FoliageSource::Prop(FoliageProp::Stairs) => &STAIRS,
        FoliageSource::Prop(FoliageProp::Ramp) => &RAMP,
        FoliageSource::Model(_) => &[],
    }
}

/// A tint the editor brush can show for `source`.
pub fn brush_color(source: &FoliageSource) -> [f32; 3] {
    pieces(source)
        .last()
        .map(|piece| piece.color)
        .unwrap_or([0.3, 0.6, 0.2])
}

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

fn shape_of(part: PartType) -> Shape {
    match part {
        PartType::Block => Shape::Block,
        PartType::Ball => Shape::Sphere,
        PartType::Cylinder => Shape::Cylinder,
        PartType::Wedge => Shape::Wedge,
    }
}

/// Adds this terrain's painted foliage to `items`, sitting on the surface.
pub(crate) fn draw_terrain(
    placement: &Placement,
    data: &TerrainData,
    assets: &Assets,
    items: &mut Vec<DrawItem>,
) {
    for instance in data.foliage() {
        let Some(transform) = instance_transform(placement, data, instance) else {
            continue;
        };
        match &instance.source {
            FoliageSource::Model(path) => {
                let Some(model) = assets.model(path) else {
                    continue;
                };
                let size = Vec3::from(model.size) * instance.scale.max(0.01);
                let Some(ground) = instance_ground(placement, data, instance) else {
                    continue;
                };
                let model_transform = Mat4::from_scale_rotation_translation(
                    size.max(Vec3::splat(0.01)),
                    Quat::from_rotation_y(instance.yaw),
                    ground + Vec3::new(0.0, size.y.abs() * 0.5, 0.0),
                );
                push_model(&model, model_transform, items);
            }
            _ => {
                for piece in pieces(&instance.source) {
                    items.push(piece_item(piece, transform));
                }
            }
        }
    }
}

fn instance_ground(
    placement: &Placement,
    data: &TerrainData,
    instance: &FoliageInstance,
) -> Option<Vec3> {
    let xz = placement.to_world_xz(data, Vec2::new(instance.x, instance.z));
    let y = placement.ground_y(data, xz.x, xz.y)?;
    Some(Vec3::new(xz.x, y, xz.y))
}

fn instance_transform(
    placement: &Placement,
    data: &TerrainData,
    instance: &FoliageInstance,
) -> Option<Mat4> {
    let ground = instance_ground(placement, data, instance)?;
    let scale = instance.scale.max(0.01);
    Some(Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        Quat::from_rotation_y(instance.yaw),
        ground,
    ))
}

fn piece_item(piece: &Piece, instance: Mat4) -> DrawItem {
    let local = Mat4::from_scale_rotation_translation(
        mesh_scale(piece.shape, piece.size),
        rotation_from_orientation(piece.orientation),
        piece.position,
    );
    DrawItem {
        shape: shape_of(piece.shape),
        transform: instance * local,
        material: Material {
            base_color: if piece.glow { [0.0; 3] } else { piece.color },
            emissive: if piece.glow { piece.color } else { [0.0; 3] },
            metalness: piece.metalness,
            roughness: piece.roughness,
            opacity: 1.0,
            ..Material::default()
        },
        casts_shadow: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, FoliagePlant, TerrainData, Value, Vector3};

    #[test]
    fn foliage_sits_on_the_terrain_surface() {
        let mut world = axcenity_core::World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        world
            .set_property(
                terrain,
                "Size",
                Value::Vector3(Vector3::new(64.0, 20.0, 64.0)),
            )
            .unwrap();
        let mut data = TerrainData::flat(9, 9, 0.25).unwrap();
        let all = data.bounds();
        data.heights_mut_in(all)[4 * 9 + 4] = 0.75;
        data.foliage_mut().push(FoliageInstance {
            source: FoliageSource::Plant(FoliagePlant::Grass),
            x: 4.0,
            z: 4.0,
            yaw: 0.0,
            scale: 1.0,
            collide: false,
        });
        data.foliage_mut().push(FoliageInstance {
            source: FoliageSource::Prop(FoliageProp::Crate),
            x: 0.0,
            z: 0.0,
            yaw: 0.0,
            scale: 1.0,
            collide: true,
        });
        world.set_terrain(terrain, std::sync::Arc::new(data));
        let mut items = Vec::new();
        let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
        let data = world.terrain(terrain).unwrap();
        draw_terrain(&placement, data, &Assets::default(), &mut items);
        assert!(items.len() >= 3, "{} items", items.len());
        let xz = placement.to_world_xz(data, Vec2::ZERO);
        let top = placement.ground_y(data, xz.x, xz.y).unwrap();
        // The crate's bottom is at the ground; its center is 1.5 studs up.
        let crate_item = items
            .iter()
            .find(|item| matches!(item.shape, Shape::Block))
            .expect("a crate");
        let crate_y = crate_item.transform.to_scale_rotation_translation().2.y;
        assert!((crate_y - (top + 1.5)).abs() < 0.05, "{crate_y} vs {top}");
    }
}
