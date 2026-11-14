//! Collision shapes for each `PartType`, for MeshParts and UnionOperations
//! (by their `CollisionFidelity`), and for Terrains.
//! They match the meshes the renderer draws (`render/src/mesh.rs`),
//! including how balls and cylinders stay round, and how terrain cells are
//! split into triangles (`terrain/src/mesh.rs`).

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::{Arc, LazyLock, Mutex};

use axcenity_assets::{Assets, Model};
use axcenity_core::{
    FoliageSource, InstanceId, PartType, PhysicalProperties, TerrainData, Vector3, World,
};
use rapier3d::geometry::Array2;
use rapier3d::glamx::{Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::parry::shape::{HeightField, HeightFieldCellStatus};
use rapier3d::parry::transformation::vhacd::VHACDParameters;
use rapier3d::parry::transformation::voxelization::FillMode;
use rapier3d::prelude::{Collider, ColliderBuilder, SharedShape};

/// Parts thinner than this collide as if they were this thick. A shape with
/// no volume has no mass, which rapier can't simulate.
pub(crate) const MIN_SIZE: f32 = 0.01;

/// How much terrains and foliage grip (0 is ice) and bounce (0 is no
/// bounce): what every part had before materials had their own. Parts get
/// theirs afterwards (`material::apply`).
const FRICTION: f32 = PhysicalProperties::CLASSIC.friction;
const RESTITUTION: f32 = PhysicalProperties::CLASSIC.elasticity;

/// What a part collides as.
#[derive(Clone, PartialEq)]
pub enum Form {
    /// A Part, by its `Shape`.
    Primitive(PartType),
    /// A MeshPart or UnionOperation, by its model as `Fidelity` says, or
    /// by its box while it has none (no `MeshId`, or a model that can't be
    /// loaded). Models compare by which load they are, so a model loaded
    /// later counts as a change.
    Mesh(Option<Arc<Model>>, Fidelity),
    /// A Terrain's heightfield, stretched to its `Size`.
    Terrain(TerrainShape),
}

impl Form {
    /// True if the Instance `id` this was read from may collide differently
    /// now without having changed itself: a model loaded again
    /// (`assets_changed`) or a Terrain's data edited.
    pub(crate) fn outdated(&self, world: &World, id: InstanceId, assets_changed: bool) -> bool {
        match self {
            Form::Primitive(_) => false,
            Form::Mesh(..) => assets_changed,
            Form::Terrain(terrain) => {
                assets_changed
                    || world.terrain(id).is_none_or(|data| {
                        data.uid() != terrain.data.uid()
                            || data.revision() != terrain.data.revision()
                    })
            }
        }
    }
}

/// How closely a model's collider follows it: a MeshPart's or
/// UnionOperation's `CollisionFidelity` (Phase 50, R15.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fidelity {
    /// The model's own collision pieces (its `_COL` objects, see
    /// `assets::Model::collision`) when it has any, else its hull.
    #[default]
    Default,
    /// Its box.
    Box,
    /// The convex hull of its corners, hollows filled in.
    Hull,
    /// Convex pieces that follow its hollows (parry's convex
    /// decomposition), so a doorway can be walked through.
    Precise,
}

impl Fidelity {
    /// The fidelity a `CollisionFidelity` item names; Default for anything
    /// else.
    pub fn named(item: &str) -> Self {
        match item {
            "Box" => Fidelity::Box,
            "Hull" => Fidelity::Hull,
            "PreciseConvexDecomposition" => Fidelity::Precise,
            _ => Fidelity::Default,
        }
    }
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
            && self.data.generated_version() == other.data.generated_version()
            && self.foliage_models == other.foliage_models
    }
}

/// Unique colliding model paths on this terrain, with the loaded model's
/// key (or none while it can't be read).
pub fn colliding_model_keys(data: &TerrainData, assets: &Assets) -> Vec<(String, Option<u64>)> {
    let mut paths: Vec<String> = data
        .all_foliage()
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
            let key = assets.model_blocking(&path).map(|model| model.key());
            (path, key)
        })
        .collect()
}

/// The collider for a part of this form and size, centered on the part.
pub fn collider(form: &Form, size: Vec3) -> Collider {
    let half = size.abs().max(Vec3::splat(MIN_SIZE)) / 2.0;
    let builder = match form {
        Form::Primitive(PartType::Block) | Form::Mesh(None, _) | Form::Mesh(_, Fidelity::Box) => {
            cuboid(half)
        }
        // Balls use the smallest side, like the drawn mesh.
        Form::Primitive(PartType::Ball) => ColliderBuilder::ball(half.min_element()),
        // rapier's cylinders run along Y, ours along X: turn it 90° around Z.
        Form::Primitive(PartType::Cylinder) => {
            ColliderBuilder::cylinder(half.x, half.y.min(half.z))
                .rotation(Vec3::new(0.0, 0.0, FRAC_PI_2))
        }
        Form::Primitive(PartType::Wedge) => wedge(half),
        Form::Primitive(PartType::CornerWedge) => corner_wedge(half),
        // rapier's capsule along X: its round ends' middles, and its radius.
        Form::Primitive(PartType::Capsule) => {
            let (reach, radius) = capsule(half);
            ColliderBuilder::capsule_x(reach, radius)
        }
        Form::Mesh(Some(model), Fidelity::Hull) => hull(model, half),
        Form::Mesh(Some(model), Fidelity::Default) => own_pieces(model, half),
        Form::Mesh(Some(model), Fidelity::Precise) => decomposed(model, half),
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
        Form::Primitive(PartType::Block) | Form::Mesh(None, _) | Form::Mesh(_, Fidelity::Box) => {
            cuboid_edges(half)
        }
        Form::Primitive(PartType::Ball) => ball_edges(half.min_element()),
        Form::Primitive(PartType::Cylinder) => cylinder_edges(half),
        Form::Primitive(PartType::Wedge) => wedge_edges(half),
        Form::Primitive(PartType::CornerWedge) => corner_wedge_edges(half),
        Form::Primitive(PartType::Capsule) => capsule_edges(half),
        Form::Mesh(Some(model), Fidelity::Hull) => hull_edges(model, half),
        Form::Mesh(Some(model), Fidelity::Default) if model.collision.is_empty() => {
            hull_edges(model, half)
        }
        Form::Mesh(Some(model), Fidelity::Default) => {
            compound_edges(&own_pieces(model, half).build(), half)
        }
        Form::Mesh(Some(model), Fidelity::Precise) => {
            compound_edges(&decomposed(model, half).build(), half)
        }
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

/// The bottom rectangle and the four edges up to the point.
fn corner_wedge_edges(half: Vec3) -> Vec<(Vec3, Vec3)> {
    let corners = corner_wedge_corners(half);
    [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (0, 4),
        (1, 4),
        (2, 4),
        (3, 4),
    ]
    .into_iter()
    .map(|(a, b)| (corners[a], corners[b]))
    .collect()
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

/// The corners of a corner wedge: the bottom rectangle, then the point
/// above its back right corner (+X, +Z).
fn corner_wedge_corners(half: Vec3) -> [Vec3; 5] {
    [
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(half.x, half.y, half.z),
    ]
}

/// A corner wedge: flat bottom, rising to a point over the back right corner.
fn corner_wedge(half: Vec3) -> ColliderBuilder {
    // Five corners that aren't flat always make a hull, but fall back to a
    // box rather than panic.
    ColliderBuilder::convex_hull(&corner_wedge_corners(half)).unwrap_or_else(|| cuboid(half))
}

/// The smallest convex shape around a model's corners, like shrink-wrap:
/// hollows (under a table, inside a cup) are filled in. A model with no
/// volume, such as a flat plane, can't make one and collides as its box.
///
/// A hull takes a pass over every corner of the model, millions in a scanned
/// tree, and colliders are built again for each query, so the shapes are kept
/// by model and size. A collider only shares the shape, which rapier copies
/// before changing.
fn hull(model: &Model, half: Vec3) -> ColliderBuilder {
    let key = (model.key(), half.to_array().map(f32::to_bits));
    static HULLS: LazyLock<Mutex<HashMap<ShapeKey, SharedShape>>> = LazyLock::new(Mutex::default);
    if let Some(shape) = HULLS.lock().ok().and_then(|hulls| hulls.get(&key).cloned()) {
        return ColliderBuilder::new(shape);
    }
    let shape = build_hull(model, half).shape;
    if let Ok(mut hulls) = HULLS.lock() {
        // Old sizes of parts being scaled aren't needed again.
        if hulls.len() >= KEPT_HULLS {
            hulls.clear();
        }
        hulls.insert(key, shape.clone());
    }
    ColliderBuilder::new(shape)
}

/// How many hulls are kept: enough for the models and sizes of a large
/// scene, and a limit on what a script that keeps resizing a part can hold.
const KEPT_HULLS: usize = 4096;

/// Makes [`hull`]'s shape.
fn build_hull(model: &Model, half: Vec3) -> ColliderBuilder {
    let flat = model.size.contains(&0.0) || model.hull_points.len() < 4;
    if flat {
        return cuboid(half);
    }
    if let Some(own) = own_convex_mesh(model, half) {
        return own;
    }
    // The model fills the 1×1×1 cube, so doubling the half size stretches it to the part.
    let corners: Vec<Vec3> = model
        .hull_points
        .iter()
        .map(|&corner| Vec3::from(corner) * half * 2.0)
        .collect();
    ColliderBuilder::convex_hull(&corners).unwrap_or_else(|| cuboid(half))
}

/// parry's convex decomposition, tighter than its defaults: with them, a
/// doorway's corner was bridged by a slanted piece a character couldn't
/// pass. Full-resolution cutting planes and exact hulls of the model's own
/// triangles follow straight edges exactly.
const PRECISE: VHACDParameters = VHACDParameters {
    resolution: 64,
    concavity: 0.001,
    plane_downsampling: 1,
    convex_hull_downsampling: 1,
    alpha: 0.05,
    beta: 0.05,
    convex_hull_approximation: false,
    max_convex_hulls: 1024,
    fill_mode: FillMode::FloodFill {
        detect_cavities: false,
    },
};

/// A model's load key and the half size its shape was made for.
type ShapeKey = (u64, [u32; 3]);

/// Convex pieces following a model's hollows, stretched like the hull.
/// Decomposing takes a while, and colliders are built again for queries
/// and masses, so the pieces are kept by model and size.
fn decomposed(model: &Model, half: Vec3) -> ColliderBuilder {
    if model.size.contains(&0.0) {
        return cuboid(half);
    }
    let key = (model.key(), half.to_array().map(f32::to_bits));
    static PIECES: LazyLock<Mutex<HashMap<ShapeKey, SharedShape>>> = LazyLock::new(Mutex::default);
    if let Some(shape) = PIECES
        .lock()
        .ok()
        .and_then(|pieces| pieces.get(&key).cloned())
    {
        return ColliderBuilder::new(shape);
    }
    let (points, triangles) = welded_triangles(model, half);
    if triangles.is_empty() {
        return cuboid(half);
    }
    let shape = SharedShape::convex_decomposition_with_params(&points, &triangles, &PRECISE);
    if let Ok(mut pieces) = PIECES.lock() {
        // Old sizes of a part being scaled aren't needed again.
        if pieces.len() >= 256 {
            pieces.clear();
        }
        pieces.insert(key, shape.clone());
    }
    ColliderBuilder::new(shape)
}

/// Builds the convex pieces a part with `model` at `size` collides as with
/// `PreciseConvexDecomposition`, ahead of time, so the physics step that
/// first needs them (which would otherwise wait for them) finds them made.
/// Safe to call from any thread.
pub fn prepare_precise(model: &Model, size: Vector3) {
    let half = to_vec3(size).abs().max(Vec3::splat(MIN_SIZE)) / 2.0;
    let _ = decomposed(model, half);
}

/// The edges of each convex piece of a collider made of them
/// ([`decomposed`], [`own_pieces`]): its box's if it isn't.
fn compound_edges(collider: &Collider, half: Vec3) -> Vec<(Vec3, Vec3)> {
    let Some(compound) = collider.shape().as_compound() else {
        return cuboid_edges(half);
    };
    let mut edges = Vec::new();
    for (pose, shape) in compound.shapes() {
        if let Some(poly) = shape.as_convex_polyhedron() {
            for edge in poly.edges() {
                let [a, b] = edge.vertices.map(|v| *pose * poly.points()[v as usize]);
                edges.push((a, b));
            }
        }
    }
    edges
}

/// A model's own collision pieces (R15.2: its `_COL` objects), each the
/// convex hull of its corners, stretched like the hull. A model without
/// any, or whose pieces are all too flat to make one, collides as its hull.
/// Kept by model and size, like the hull.
fn own_pieces(model: &Model, half: Vec3) -> ColliderBuilder {
    if model.collision.is_empty() {
        return hull(model, half);
    }
    let key = (model.key(), half.to_array().map(f32::to_bits));
    static PIECES: LazyLock<Mutex<HashMap<ShapeKey, Option<SharedShape>>>> =
        LazyLock::new(Mutex::default);
    let kept = PIECES
        .lock()
        .ok()
        .and_then(|pieces| pieces.get(&key).cloned());
    let shape = match kept {
        Some(shape) => shape,
        None => {
            let shapes: Vec<(Pose, SharedShape)> = model
                .collision
                .iter()
                .filter_map(|piece| {
                    let corners: Vec<Vec3> = piece
                        .iter()
                        .map(|&corner| Vec3::from(corner) * half * 2.0)
                        .collect();
                    SharedShape::convex_hull(&corners)
                })
                .map(|shape| (Pose::IDENTITY, shape))
                .collect();
            let shape = (!shapes.is_empty()).then(|| SharedShape::compound(shapes));
            if let Ok(mut pieces) = PIECES.lock() {
                if pieces.len() >= KEPT_HULLS {
                    pieces.clear();
                }
                pieces.insert(key, shape.clone());
            }
            shape
        }
    };
    match shape {
        Some(shape) => ColliderBuilder::new(shape),
        None => hull(model, half),
    }
}

/// A model's triangles stretched to `half` (it fills the 1×1×1 cube), with
/// corners at the same place made one vertex.
fn welded_triangles(model: &Model, half: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut index: HashMap<[u32; 3], u32> = HashMap::new();
    let mut points: Vec<Vec3> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for primitive in &model.primitives {
        let mut corner = |i: u32| {
            let at = *primitive.positions.get(i as usize)?;
            let next = points.len() as u32;
            let id = *index.entry(at.map(f32::to_bits)).or_insert(next);
            if id == next {
                points.push(Vec3::from(at) * half * 2.0);
            }
            Some(id)
        };
        for &triangle in primitive.indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(&mut corner);
            if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                triangles.push([a, b, c]);
            }
        }
    }
    (points, triangles)
}

/// The most triangles a model may have to be checked for being convex.
const OWN_MESH_TRIANGLES: usize = 512;

/// A small model that's already convex (a Fracture's piece, Phase 42)
/// collides as its own triangles, stretched like the hull. parry's hull of
/// the corners can come out with faces that sink through the floor when an
/// outline has nearly straight corners; the model's own faces don't.
/// `None` for a bigger model, or one that isn't convex.
fn own_convex_mesh(model: &Model, half: Vec3) -> Option<ColliderBuilder> {
    let count: usize = model.primitives.iter().map(|p| p.indices.len() / 3).sum();
    if count > OWN_MESH_TRIANGLES || model.rig.is_some() {
        return None;
    }
    // Corners shared between primitives (a piece's outside and inside) are
    // the same bits, so they become one vertex.
    let mut index: HashMap<[u32; 3], u32> = HashMap::new();
    let mut points: Vec<Vec3> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for primitive in &model.primitives {
        let mut corner = |i: u32| {
            let at = *primitive.positions.get(i as usize)?;
            let next = points.len() as u32;
            let id = *index.entry(at.map(f32::to_bits)).or_insert(next);
            if id == next {
                points.push(Vec3::from(at) * half * 2.0);
            }
            Some(id)
        };
        for &triangle in primitive.indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(&mut corner);
            triangles.push([a?, b?, c?]);
        }
    }
    // Convex: every corner is on or behind every face.
    let reach = (half * 2.0).max_element() * 1e-4;
    for &[a, b, c] in &triangles {
        let (a, b, c) = (points[a as usize], points[b as usize], points[c as usize]);
        let normal = (b - a).cross(c - a);
        if normal.length_squared() == 0.0 {
            continue;
        }
        let normal = normal.normalize();
        if points.iter().any(|&p| (p - a).dot(normal) > reach) {
            return None;
        }
    }
    ColliderBuilder::convex_mesh(points, &triangles)
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
    if !data.has_holes() {
        return ColliderBuilder::heightfield(Array2::new(size_z, size_x, heights), scale);
    }
    // Holes (procedural worlds PW7): their cells are taken out, so things
    // fall and walk through them.
    let mut field = HeightField::new(Array2::new(size_z, size_x, heights), scale);
    for z in 0..size_z.saturating_sub(1) {
        for x in 0..size_x.saturating_sub(1) {
            if data.is_hole(x, z) {
                field.set_cell_status(z, x, HeightFieldCellStatus::CELL_REMOVED);
            }
        }
    }
    ColliderBuilder::new(SharedShape::new(field))
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
    for instance in data.all_foliage() {
        if !instance.collide {
            continue;
        }
        let Some(ground) = foliage_ground(data, size, instance.x, instance.z) else {
            continue;
        };
        let ground = ground - Vec3::new(0.0, instance.sink, 0.0);
        let (size_x, size_z) = data.size();
        let cell = (size.x / (size_x - 1) as f32, size.z / (size_z - 1) as f32);
        let up = to_vec3(instance.up(data, cell, size.y));
        // Turned round its up, then leaned with the ground.
        let yaw = Quat::from_rotation_arc(Vec3::Y, up) * Quat::from_rotation_y(instance.yaw);
        let scale = instance.scale.max(0.01);
        let (form, form_size, center) = match &instance.source {
            FoliageSource::Model(path) => {
                let model = assets.model_blocking(path);
                let native = model
                    .as_ref()
                    .map(|model| Vec3::from(model.size))
                    .unwrap_or(Vec3::ONE);
                let form_size = (native * scale).abs().max(Vec3::splat(MIN_SIZE));
                (
                    Form::Mesh(model, Fidelity::Hull),
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

/// A triangle mesh collider from flat vertex and index lists, such as a
/// terrain volume chunk's surface (the PW8 spike). `None` for an empty or
/// broken mesh.
pub fn triangle_mesh(positions: &[[f32; 3]], indices: &[u32]) -> Option<ColliderBuilder> {
    if indices.len() < 3 {
        return None;
    }
    let vertices = positions.iter().map(|&p| Vec3::from_array(p)).collect();
    let triangles = indices
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]])
        .collect();
    ColliderBuilder::trimesh(vertices, triangles).ok()
}
