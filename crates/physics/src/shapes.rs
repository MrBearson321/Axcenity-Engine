//! Collision shapes for each `PartType`, for MeshParts and for Terrains.
//! They match the meshes the renderer draws (`render/src/mesh.rs`),
//! including how balls and cylinders stay round, and how terrain cells are
//! split into triangles (`terrain/src/mesh.rs`).

use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use axcenity_assets::{Assets, Model};
use axcenity_core::{FoliageSource, PartType, PhysicalProperties, TerrainData, Vector3};
use rapier3d::geometry::Array2;
use rapier3d::glamx::{Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::prelude::{Collider, ColliderBuilder};

/// Parts thinner than this collide as if they were this thick. A shape with
/// no volume has no mass, which rapier can't simulate.
const MIN_SIZE: f32 = 0.01;

/// How much terrains and foliage grip (0 is ice) and bounce (0 is no
/// bounce): what every part had before materials had their own. Parts get
/// theirs afterwards (`material::apply`).
const FRICTION: f32 = PhysicalProperties::LEGACY.friction;
const RESTITUTION: f32 = PhysicalProperties::LEGACY.elasticity;

/// What a part collides as.
#[derive(Clone, PartialEq)]
pub enum Form {
    /// A Part, by its `Shape`.
    Primitive(PartType),
    /// A MeshPart, by its model, or by its box while it has none (no
    /// `MeshId`, or a model that can't be loaded). Models compare by which
    /// load they are, so a model loaded later counts as a change.
    Mesh(Option<Arc<Model>>),
    /// A Terrain's heightfield, stretched to its `Size`.
    Terrain(TerrainShape),
}

/// A Terrain's landscape. Two compare equal while they're the same data,
/// unchanged, so the collider is only rebuilt after an edit. Colliding
/// foliage models are listed by load key so a model that arrives later
/// rebuilds those colliders.
#[derive(Clone)]
pub struct TerrainShape {
    pub data: Arc<TerrainData>,
    pub foliage_models: Vec<(String, Option<u64>)>,
}

impl PartialEq for TerrainShape {
    fn eq(&self, other: &Self) -> bool {
        self.data.uid() == other.data.uid()
            && self.data.revision() == other.data.revision()
            && self.foliage_models == other.foliage_models
    }
}

/// Unique colliding model paths on this terrain, with the loaded model's
/// key (or none while it can't be read).
pub fn colliding_model_keys(data: &TerrainData, assets: &Assets) -> Vec<(String, Option<u64>)> {
    let mut paths: Vec<String> = data
        .foliage()
        .iter()
        .filter(|instance| instance.collide)
        .filter_map(|instance| match &instance.source {
            FoliageSource::Model(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let key = assets.model(&path).map(|model| model.key());
            (path, key)
        })
        .collect()
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
        // rapier's capsule along X: its round ends' middles, and its radius.
        Form::Primitive(PartType::Capsule) => {
            let (reach, radius) = capsule(half);
            ColliderBuilder::capsule_x(reach, radius)
        }
        Form::Mesh(Some(model)) => hull(model, half),
        Form::Terrain(terrain) => heightfield(&terrain.data, size),
    };
    builder.friction(FRICTION).restitution(RESTITUTION).build()
}

fn cuboid(half: Vec3) -> ColliderBuilder {
    ColliderBuilder::cuboid(half.x, half.y, half.z)
}

/// Local-space edges of the collider for `form` at `size`, matching
/// [`collider`]. Terrain is drawn per chunk by [`terrain_chunks`].
pub fn wireframe(form: &Form, size: Vec3) -> Vec<(Vec3, Vec3)> {
    let half = size.abs().max(Vec3::splat(MIN_SIZE)) / 2.0;
    match form {
        Form::Primitive(PartType::Block) | Form::Mesh(None) => cuboid_edges(half),
        Form::Primitive(PartType::Ball) => ball_edges(half.min_element()),
        Form::Primitive(PartType::Cylinder) => cylinder_edges(half),
        Form::Primitive(PartType::Wedge) => wedge_edges(half),
        Form::Primitive(PartType::Capsule) => capsule_edges(half),
        Form::Mesh(Some(model)) => hull_edges(model, half),
        Form::Terrain(_) => Vec::new(),
    }
}

/// One chunk of a Terrain's heightfield, as the collider really is.
pub struct TerrainChunkWire {
    pub center: Vec3,
    pub edges: Vec<(Vec3, Vec3)>,
}

/// Heightfield edges for each terrain chunk, in the Terrain's local space
/// (centered on its Position, Y up from the bottom). Interior lines are
/// strided so a large terrain stays drawable; chunk borders stay at every
/// sample, so the collider of a chunk is still visible.
pub fn terrain_chunks(data: &TerrainData, size: Vec3) -> Vec<TerrainChunkWire> {
    let (size_x, size_z) = data.size();
    let chunks_x = (size_x - 1).div_ceil(axcenity_core::CHUNK_CELLS).max(1);
    let chunks_z = (size_z - 1).div_ceil(axcenity_core::CHUNK_CELLS).max(1);
    let scale = size.abs().max(Vec3::splat(MIN_SIZE));
    let mut chunks = Vec::with_capacity(chunks_x * chunks_z);
    for cz in 0..chunks_z {
        for cx in 0..chunks_x {
            let x0 = cx * axcenity_core::CHUNK_CELLS;
            let z0 = cz * axcenity_core::CHUNK_CELLS;
            let x1 = (x0 + axcenity_core::CHUNK_CELLS).min(size_x - 1);
            let z1 = (z0 + axcenity_core::CHUNK_CELLS).min(size_z - 1);
            if x0 >= x1 || z0 >= z1 {
                continue;
            }
            let sample = |x: usize, z: usize| -> Vec3 {
                let sx = x as f32 / (size_x - 1) as f32 - 0.5;
                let sz = z as f32 / (size_z - 1) as f32 - 0.5;
                Vec3::new(
                    sx * scale.x,
                    data.height(x as isize, z as isize) * scale.y,
                    sz * scale.z,
                )
            };
            let mut edges = Vec::new();
            let stride = ((x1 - x0).max(z1 - z0) / 8).max(1);
            for z in z0..=z1 {
                let step = if z == z0 || z == z1 { 1 } else { stride };
                let mut x = x0;
                while x < x1 {
                    let next = (x + step).min(x1);
                    edges.push((sample(x, z), sample(next, z)));
                    x = next;
                }
            }
            for x in x0..=x1 {
                let step = if x == x0 || x == x1 { 1 } else { stride };
                let mut z = z0;
                while z < z1 {
                    let next = (z + step).min(z1);
                    edges.push((sample(x, z), sample(x, next)));
                    z = next;
                }
            }
            let center = (sample(x0, z0) + sample(x1, z1)) * 0.5;
            chunks.push(TerrainChunkWire { center, edges });
        }
    }
    chunks
}

fn cuboid_edges(half: Vec3) -> Vec<(Vec3, Vec3)> {
    let corners = [
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(-half.x, half.y, -half.z),
        Vec3::new(half.x, half.y, -half.z),
        Vec3::new(half.x, half.y, half.z),
        Vec3::new(-half.x, half.y, half.z),
    ];
    let pairs = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    pairs
        .into_iter()
        .map(|(a, b)| (corners[a], corners[b]))
        .collect()
}

fn ball_edges(radius: f32) -> Vec<(Vec3, Vec3)> {
    let mut edges = Vec::new();
    edges.extend(circle(Vec3::ZERO, Vec3::X, Vec3::Y, radius));
    edges.extend(circle(Vec3::ZERO, Vec3::X, Vec3::Z, radius));
    edges.extend(circle(Vec3::ZERO, Vec3::Y, Vec3::Z, radius));
    edges
}

fn cylinder_edges(half: Vec3) -> Vec<(Vec3, Vec3)> {
    let radius = half.y.min(half.z);
    let mut edges = circle(Vec3::new(-half.x, 0.0, 0.0), Vec3::Y, Vec3::Z, radius);
    edges.extend(circle(
        Vec3::new(half.x, 0.0, 0.0),
        Vec3::Y,
        Vec3::Z,
        radius,
    ));
    for turn in 0..4 {
        let around = turn as f32 / 4.0 * TAU;
        let rim = Vec3::Y * around.cos() * radius + Vec3::Z * around.sin() * radius;
        edges.push((
            Vec3::new(-half.x, 0.0, 0.0) + rim,
            Vec3::new(half.x, 0.0, 0.0) + rim,
        ));
    }
    edges
}

/// A capsule of half-size `half` (along X): how far from its middle the
/// centers of its round ends are, and its radius. A capsule shorter than
/// it is wide is a ball.
pub fn capsule(half: Vec3) -> (f32, f32) {
    let radius = half.y.min(half.z);
    ((half.x - radius).max(0.0), radius)
}

fn capsule_edges(half: Vec3) -> Vec<(Vec3, Vec3)> {
    let (reach, radius) = capsule(half);
    let mut edges = cylinder_edges(Vec3::new(reach, radius, radius));
    for end in [-reach, reach] {
        let middle = Vec3::new(end, 0.0, 0.0);
        // Only the half of each circle beyond the cylinder's end.
        let outside = |p: Vec3| (p.x - end) * end.signum() >= -1e-4;
        for b in [Vec3::Y, Vec3::Z] {
            edges.extend(
                circle(middle, Vec3::X, b, radius)
                    .into_iter()
                    .filter(|(p, q)| outside(*p) && outside(*q)),
            );
        }
    }
    edges
}

fn wedge_edges(half: Vec3) -> Vec<(Vec3, Vec3)> {
    let corners = [
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(-half.x, half.y, half.z),
        Vec3::new(half.x, half.y, half.z),
    ];
    // Bottom rectangle, back top edge, the two sloping sides, and the two
    // upright back corners: the hull of the same six points the collider uses.
    [
        (0, 1),
        (1, 3),
        (3, 2),
        (2, 0),
        (4, 5),
        (0, 4),
        (1, 5),
        (2, 4),
        (3, 5),
    ]
    .into_iter()
    .map(|(a, b)| (corners[a], corners[b]))
    .collect()
}

fn hull_edges(model: &Model, half: Vec3) -> Vec<(Vec3, Vec3)> {
    let collider = hull(model, half).build();
    let Some(poly) = collider.shape().as_convex_polyhedron() else {
        return cuboid_edges(half);
    };
    poly.edges()
        .iter()
        .map(|edge| {
            let a = poly.points()[edge.vertices[0] as usize];
            let b = poly.points()[edge.vertices[1] as usize];
            (a, b)
        })
        .collect()
}

const CIRCLE_SEGMENTS: usize = 24;

fn circle(center: Vec3, a: Vec3, b: Vec3, radius: f32) -> Vec<(Vec3, Vec3)> {
    let point = |i: usize| {
        let turn = i as f32 / CIRCLE_SEGMENTS as f32 * TAU;
        center + (a * turn.cos() + b * turn.sin()) * radius
    };
    (0..CIRCLE_SEGMENTS)
        .map(|i| (point(i), point(i + 1)))
        .collect()
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

/// A Terrain's heights as a heightfield `size` studs across, centered on
/// its position in X and Z, rising from it in Y. parry wants the heights
/// with a row per Z sample, stored column by column (X), and splits each
/// cell from (x, z+1) to (x+1, z), as the drawn chunks do.
fn heightfield(data: &TerrainData, size: Vec3) -> ColliderBuilder {
    let (size_x, size_z) = data.size();
    let mut heights = Vec::with_capacity(size_x * size_z);
    for x in 0..size_x {
        for z in 0..size_z {
            heights.push(data.height(x as isize, z as isize));
        }
    }
    let scale = size.abs().max(Vec3::splat(MIN_SIZE));
    ColliderBuilder::heightfield(Array2::new(size_z, size_x, heights), scale)
}

/// Solid colliders for colliding foliage, in the terrain body's local space.
pub fn foliage_colliders(data: &TerrainData, size: Vec3, assets: &Assets) -> Vec<Collider> {
    foliage_forms(data, size, assets)
        .into_iter()
        .map(|(pose, form, form_size)| {
            let mut collider = collider(&form, form_size);
            collider.set_position(pose);
            collider
        })
        .collect()
}

/// Local pose, form, and size of each colliding foliage instance.
pub fn foliage_forms(data: &TerrainData, size: Vec3, assets: &Assets) -> Vec<(Pose, Form, Vec3)> {
    let mut forms = Vec::new();
    for instance in data.foliage() {
        if !instance.collide {
            continue;
        }
        let Some(ground) = foliage_ground(data, size, instance.x, instance.z) else {
            continue;
        };
        let yaw = Quat::from_rotation_y(instance.yaw);
        let scale = instance.scale.max(0.01);
        let (form, form_size, center) = match &instance.source {
            FoliageSource::Model(path) => {
                let model = assets.model(path);
                let native = model
                    .as_ref()
                    .map(|model| Vec3::from(model.size))
                    .unwrap_or(Vec3::ONE);
                let form_size = (native * scale).abs().max(Vec3::splat(MIN_SIZE));
                (
                    Form::Mesh(model),
                    form_size,
                    Vec3::new(0.0, form_size.y * 0.5, 0.0),
                )
            }
            other => {
                let Some((bounds, center)) = other.bounds() else {
                    continue;
                };
                let form_size = (to_vec3(bounds) * scale).abs().max(Vec3::splat(MIN_SIZE));
                (
                    Form::Primitive(PartType::Block),
                    form_size,
                    to_vec3(center) * scale,
                )
            }
        };
        let pose = Pose::from_parts(ground + yaw * center, yaw);
        forms.push((pose, form, form_size));
    }
    forms
}

fn foliage_ground(data: &TerrainData, size: Vec3, x: f32, z: f32) -> Option<Vec3> {
    let (size_x, size_z) = data.size();
    if size_x < 2 || size_z < 2 {
        return None;
    }
    let max_x = (size_x - 1) as f32;
    let max_z = (size_z - 1) as f32;
    if !(0.0..=max_x).contains(&x) || !(0.0..=max_z).contains(&z) {
        return None;
    }
    Some(Vec3::new(
        -size.x * 0.5 + x / max_x * size.x,
        data.height_at(x, z) * size.y,
        -size.z * 0.5 + z / max_z * size.z,
    ))
}

fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
