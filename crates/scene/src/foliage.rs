//! Painted plants, props, and models as draw items. They sit on the terrain
//! surface and are not Instances, so sculpting moves them with the ground.

use axcenity_assets::Assets;
use axcenity_core::{
    FoliageInstance, FoliagePlant, FoliageProp, FoliageSource, InstanceId, PartType, TerrainData,
    Vector3,
};
use axcenity_render::glam::{Mat4, Quat, Vec2, Vec3};
use axcenity_render::{DrawItem, Material, Shape, Sway};
use axcenity_terrain::Placement;

use crate::draw_list::{draw_id, push_model};
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
        PartType::Cylinder | PartType::Capsule => {
            let diameter = size.y.min(size.z);
            Vec3::new(size.x, diameter, diameter)
        }
    }
}

fn shape_of(part: PartType) -> Shape {
    match part {
        PartType::Block => Shape::Block,
        PartType::Ball => Shape::Sphere,
        PartType::Cylinder | PartType::Capsule => Shape::Cylinder,
        PartType::Wedge => Shape::Wedge,
    }
}

/// How far from the camera a plant of scale 1 is drawn, in studs: small
/// plants vanish nearer than big ones. Each fades out before it's culled
/// ([`DrawItem::cull_distance`]). Props and models are drawn at any
/// distance (models use their levels of detail).
fn cull_distance(source: &FoliageSource) -> f32 {
    match source {
        FoliageSource::Plant(FoliagePlant::Grass) => 120.0,
        FoliageSource::Plant(FoliagePlant::Flower) => 150.0,
        FoliageSource::Plant(FoliagePlant::Bush) => 300.0,
        FoliageSource::Plant(FoliagePlant::Tree) | FoliageSource::Prop(FoliageProp::Tree) => 800.0,
        FoliageSource::Prop(_) | FoliageSource::Model(_) => 0.0,
    }
}

/// How much a built-in plant gives in the wind ([`Sway::strength`]):
/// grass and flowers bend most, a tree least. Props stay still, apart from
/// the prop tree, which is the plant's shape.
fn sway_strength(source: &FoliageSource) -> f32 {
    match source {
        FoliageSource::Plant(FoliagePlant::Grass) => 1.0,
        FoliageSource::Plant(FoliagePlant::Flower) => 0.8,
        FoliageSource::Plant(FoliagePlant::Bush) => 0.35,
        FoliageSource::Plant(FoliagePlant::Tree) | FoliageSource::Prop(FoliageProp::Tree) => 0.2,
        FoliageSource::Prop(_) | FoliageSource::Model(_) => 0.0,
    }
}

/// How the plant made of `pieces`, standing at `instance` (its ground at
/// the origin, scaled), sways: by its height over the ground, every piece
/// alike so they bend as one. `None` for a plant that doesn't.
fn plant_sway(pieces: &[Piece], strength: f32, instance: Mat4) -> Option<Sway> {
    if strength <= 0.0 {
        return None;
    }
    let base = instance.w_axis.y;
    let top = pieces
        .iter()
        .flat_map(|piece| {
            let transform = instance * piece_transform(piece);
            corners().map(move |corner| transform.transform_point3(corner).y)
        })
        .fold(base, f32::max);
    Some(Sway {
        base,
        height: top - base,
        strength,
    })
}

/// The corners of the unit cube every built-in shape fits.
fn corners() -> impl Iterator<Item = Vec3> {
    (0..8).map(|i| {
        Vec3::new(
            if i & 1 == 0 { -0.5 } else { 0.5 },
            if i & 2 == 0 { -0.5 } else { 0.5 },
            if i & 4 == 0 { -0.5 } else { 0.5 },
        )
    })
}

/// Adds the painted foliage of `terrain` (its data `data`) to `items`,
/// sitting on the surface.
pub(crate) fn draw_terrain(
    terrain: InstanceId,
    placement: &Placement,
    data: &TerrainData,
    assets: &Assets,
    items: &mut Vec<DrawItem>,
) {
    for (index, instance) in data.foliage().iter().enumerate() {
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
                push_model(&model, model_transform, draw_id((terrain, index)), items);
            }
            source => {
                let cull_distance = cull_distance(source) * instance.scale.max(0.01);
                let sway = plant_sway(pieces(source), sway_strength(source), transform);
                for piece in pieces(source) {
                    items.push(DrawItem {
                        cull_distance,
                        sway,
                        ..piece_item(piece, transform)
                    });
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

/// Where `piece`'s shape sits in its plant or prop.
fn piece_transform(piece: &Piece) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        mesh_scale(piece.shape, piece.size),
        rotation_from_orientation(piece.orientation),
        piece.position,
    )
}

fn piece_item(piece: &Piece, instance: Mat4) -> DrawItem {
    DrawItem {
        shape: shape_of(piece.shape),
        transform: instance * piece_transform(piece),
        material: Material {
            base_color: if piece.glow { [0.0; 3] } else { piece.color },
            emissive: if piece.glow { piece.color } else { [0.0; 3] },
            metalness: piece.metalness,
            roughness: piece.roughness,
            opacity: 1.0,
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: true,
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
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
        draw_terrain(terrain, &placement, data, &Assets::default(), &mut items);
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
        // Grass is culled past its distance; a prop never is.
        assert_eq!(crate_item.cull_distance, 0.0);
        let grass: Vec<f32> = items
            .iter()
            .filter(|item| matches!(item.shape, Shape::Wedge))
            .map(|item| item.cull_distance)
            .collect();
        assert!(!grass.is_empty());
        assert!(grass.iter().all(|&distance| distance == 120.0), "{grass:?}");
    }

    #[test]
    fn plants_sway_as_one_by_their_height_and_props_stay_still() {
        let mut world = axcenity_core::World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        let mut data = TerrainData::flat(9, 9, 0.25).unwrap();
        let plant = |source, x, scale| FoliageInstance {
            source,
            x,
            z: 4.0,
            yaw: 0.0,
            scale,
            collide: false,
        };
        data.foliage_mut().extend([
            plant(FoliageSource::Plant(FoliagePlant::Tree), 2.0, 2.0),
            plant(FoliageSource::Plant(FoliagePlant::Grass), 4.0, 1.0),
            plant(FoliageSource::Prop(FoliageProp::Crate), 6.0, 1.0),
        ]);
        world.set_terrain(terrain, std::sync::Arc::new(data));
        let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
        let data = world.terrain(terrain).unwrap();
        let mut items = Vec::new();
        draw_terrain(terrain, &placement, data, &Assets::default(), &mut items);
        let ground = |x| {
            let xz = placement.to_world_xz(data, Vec2::new(x, 4.0));
            placement.ground_y(data, xz.x, xz.y).unwrap()
        };

        // The tree's trunk and crown share one sway, from its ground to the
        // top of its crown (7.5 + 3 studs, twice over).
        let (trunk, crown) = (&items[0], &items[1]);
        let tree = trunk.sway.expect("trees sway");
        assert_eq!(crown.sway, Some(tree));
        assert!((tree.base - ground(2.0)).abs() < 1e-4, "{tree:?}");
        assert!((tree.height - 21.0).abs() < 1e-3, "{tree:?}");
        // Grass is 1.1 studs tall and gives more than a tree.
        let grass = items[2].sway.expect("grass sways");
        assert!((grass.height - 1.1).abs() < 1e-3, "{grass:?}");
        assert!(grass.strength > tree.strength);
        assert_eq!(items[4].sway, None, "crates stay still");
    }
}
