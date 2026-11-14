//! Procedural content along splines (Phase 74), like a small version of
//! Unreal's procedural tools, and over terrains: what a Terrain's
//! ScatterRules place ([`terrain_scatter`], procedural worlds PW1).
//!
//! A **SplineMesh** in a Spline bends a model along the curve, repeated to
//! fill it ([`bend`]); a **SplineScatter** places models along it or inside
//! a closed one ([`scatter`]). What they make is their children: anchored
//! MeshParts made again from the spline and the settings whenever either
//! changes, and when a project loads. They're never saved, sent to other
//! devices or kept in save slots (`core::spline::is_generator`). Bake (in
//! the editor) turns them into ordinary Instances.
//!
//! Every host keeps one [`Generators`] per world and calls
//! [`Generators::update`] each frame: the editor while editing, and the
//! script runtime's step in the game, the server and editor Play.

mod bend;
mod flatten;
pub mod paths;
#[cfg(test)]
mod paths_tests;
mod rng;
pub mod route;
mod scatter;
mod surface;
pub mod terrain_scatter;
#[cfg(test)]
mod terrain_scatter_tests;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use axcenity_assets::Assets;
use axcenity_core::spline::{SplineCurve, is_generator};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{EulerRot, Quat, Vec2, Vec3};

pub use bend::{MAX_PIECES, Piece, bend, frame, piece_count};
pub use flatten::flatten_along;
pub use paths::{KeepOff, PathSettings, apply_path, keep_offs};
pub use route::route;
pub use scatter::{MAX_MODELS, Placed, Settings, layout};
pub use surface::Surfaces;

/// The made-up model ids of bent pieces: `:runtime/spline<N>.glb`, never
/// a file. Shared by every world, so two never use one id.
static NEXT_MODEL: AtomicU64 = AtomicU64::new(1);

/// What each SplineMesh and SplineScatter in one world last made.
#[derive(Default)]
pub struct Generators {
    made: HashMap<InstanceId, Made>,
    /// Counts the times any SplineMesh was made again, so scatters
    /// standing on its pieces are made again after it.
    meshes_made: u64,
    assets: Option<Assets>,
    terrain_scatter: terrain_scatter::TerrainScatter,
}

struct Made {
    /// Of the spline and settings it was made from.
    signature: u64,
    /// Its children as it left them.
    results: Vec<InstanceId>,
    /// The bent models it put in `Assets`, forgotten when it's made again.
    models: Vec<String>,
}

impl Generators {
    /// Makes again what each SplineMesh and SplineScatter in `world` makes,
    /// where its spline, its settings or its children changed since the
    /// last call (all of them the first time). SplineMeshes go first, so a
    /// scatter lining a road stands on it. Returns what went wrong, such as
    /// a model that can't be loaded, once each time it's made.
    pub fn update(&mut self, world: &mut World, assets: &Assets) -> Vec<String> {
        self.assets = Some(assets.clone());
        let mut generators: Vec<InstanceId> = world.all_below(world.root(), is_generator);
        // Stable: SplineMeshes first, each kind in tree order.
        generators.sort_by_key(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::SplineScatter)
        });
        let gone: Vec<InstanceId> = self
            .made
            .keys()
            .copied()
            .filter(|id| !generators.contains(id))
            .collect();
        for id in gone {
            if let Some(made) = self.made.remove(&id) {
                forget(assets, &made.models);
            }
        }
        // Terrain scatter rules first: what they place is foliage, which
        // scatters don't stand on, so the order only saves a frame.
        self.terrain_scatter.update(world);
        let terrains = terrain_signature(world);
        let mut problems = Vec::new();
        for id in generators {
            let Some(class) = world.get(id).map(|i| i.class()) else {
                continue;
            };
            let mut hasher = DefaultHasher::new();
            inputs(world, assets, id, &mut hasher);
            if class == ClassName::SplineScatter {
                terrains.hash(&mut hasher);
                self.meshes_made.hash(&mut hasher);
            }
            let signature = hasher.finish();
            let children = world.get(id).map(|i| i.children().to_vec());
            if let Some(made) = self.made.get(&id)
                && made.signature == signature
                && children.as_ref() == Some(&made.results)
            {
                continue;
            }
            for child in children.unwrap_or_default() {
                let _ = world.destroy(child);
            }
            if let Some(made) = self.made.remove(&id) {
                forget(assets, &made.models);
            }
            let (results, models, problem) = match class {
                ClassName::SplineMesh => {
                    self.meshes_made += 1;
                    make_pieces(world, assets, id)
                }
                _ => make_scatter(world, assets, id),
            };
            problems.extend(problem);
            self.made.insert(
                id,
                Made {
                    signature,
                    results,
                    models,
                },
            );
        }
        problems
    }

    /// The bent models a SplineMesh's pieces are drawn with, in order, for
    /// Bake to write to a file. Empty for anything else.
    pub fn pieces(&self, generator: InstanceId) -> &[String] {
        self.made
            .get(&generator)
            .map_or(&[], |made| made.models.as_slice())
    }
}

impl Drop for Generators {
    fn drop(&mut self) {
        if let Some(assets) = &self.assets {
            for made in self.made.values() {
                forget(assets, &made.models);
            }
        }
    }
}

fn forget(assets: &Assets, models: &[String]) {
    for model in models {
        assets.forget(model);
    }
}

/// Feeds what `generator` makes its results from into `hasher`: its
/// properties, its Spline's `Closed` and points, and its models' loads.
fn inputs(world: &World, assets: &Assets, generator: InstanceId, hasher: &mut DefaultHasher) {
    let Some(instance) = world.get(generator) else {
        return;
    };
    for (name, value) in instance.properties() {
        name.hash(hasher);
        format!("{value:?}").hash(hasher);
    }
    let Some(spline) = instance
        .parent()
        .and_then(|parent| world.get(parent))
        .filter(|parent| parent.class() == ClassName::Spline)
    else {
        return;
    };
    format!("{:?}", spline.property("Closed")).hash(hasher);
    for &child in spline.children() {
        let Some(point) = world
            .get(child)
            .filter(|c| c.class() == ClassName::SplinePoint)
        else {
            continue;
        };
        for name in ["Position", "TangentMode", "InTangent", "OutTangent"] {
            format!("{:?}", point.property(name)).hash(hasher);
        }
    }
    for model in model_ids(instance) {
        assets.model_blocking(&model).map(|m| m.key()).hash(hasher);
    }
}

/// The model files a SplineMesh (`MeshId`) or SplineScatter (`Models`,
/// split at `;`) uses.
fn model_ids(instance: &axcenity_core::Instance) -> Vec<String> {
    let text = |name| {
        instance
            .property(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    match instance.class() {
        ClassName::SplineMesh => {
            let id = text("MeshId").trim();
            (!id.is_empty())
                .then(|| id.to_owned())
                .into_iter()
                .collect()
        }
        _ => text("Models")
            .split(';')
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

/// Each terrain's identity and edit count, so scatters stand on the
/// ground again after it's sculpted or flattened.
fn terrain_signature(world: &World) -> Vec<(u64, u64)> {
    world
        .all_below(world.workspace(), |class| class == ClassName::Terrain)
        .into_iter()
        .filter_map(|id| world.terrain(id))
        .map(|data| (data.uid(), data.revision()))
        .collect()
}

/// The curve of the Spline `generator` is in.
fn curve_of(world: &World, generator: InstanceId) -> Option<SplineCurve> {
    SplineCurve::of(world, world.get(generator)?.parent()?)
}

type Outcome = (Vec<InstanceId>, Vec<String>, Option<String>);

/// A SplineMesh's pieces: MeshParts named Piece1, Piece2, … along the
/// curve, each with its own bent model.
fn make_pieces(world: &mut World, assets: &Assets, generator: InstanceId) -> Outcome {
    let Some(instance) = world.get(generator) else {
        return (Vec::new(), Vec::new(), None);
    };
    let name = instance.name().to_owned();
    let Some(mesh_id) = model_ids(instance).pop() else {
        return (Vec::new(), Vec::new(), None);
    };
    let size = vector(instance, "Size").abs();
    let look: Vec<(&str, Value)> = ["MaterialVariant", "CanCollide", "CastShadow"]
        .into_iter()
        .filter_map(|key| Some((key, instance.property(key)?.clone())))
        .collect();
    let Some(source) = assets.model_blocking(&mesh_id) else {
        let problem = format!("The SplineMesh `{name}` can't load its model `{mesh_id}`.");
        return (Vec::new(), Vec::new(), Some(problem));
    };
    let Some(curve) = curve_of(world, generator) else {
        return (Vec::new(), Vec::new(), None);
    };
    let mut results = Vec::new();
    let mut models = Vec::new();
    for (i, piece) in bend(&curve, &source, size).into_iter().enumerate() {
        let model_id = format!(
            ":runtime/spline{}.glb",
            NEXT_MODEL.fetch_add(1, Ordering::Relaxed)
        );
        let piece_size = Vec3::from(piece.model.size);
        assets.insert(&model_id, piece.model);
        let mut properties = vec![
            ("Anchored", Value::Bool(true)),
            ("MeshId", Value::String(model_id.clone())),
            ("Position", Value::Vector3(vector3(piece.center))),
            ("Size", Value::Vector3(vector3(piece_size))),
        ];
        properties.extend(look.iter().cloned());
        if let Some(id) = add_result(world, generator, &format!("Piece{}", i + 1), properties) {
            results.push(id);
        }
        models.push(model_id);
    }
    (results, models, None)
}

/// A SplineScatter's models: MeshParts named after their files.
fn make_scatter(world: &mut World, assets: &Assets, generator: InstanceId) -> Outcome {
    let Some(instance) = world.get(generator) else {
        return (Vec::new(), Vec::new(), None);
    };
    let name = instance.name().to_owned();
    let number = |key| {
        instance
            .property(key)
            .and_then(Value::as_number)
            .unwrap_or(0.0) as f32
    };
    let flag = |key| instance.property(key) == Some(&Value::Bool(true));
    let pair = |key| {
        instance
            .property(key)
            .and_then(Value::as_vector2)
            .map_or(Vec2::ONE, |v| Vec2::new(v.x, v.y))
    };
    let settings = Settings {
        models: model_ids(instance),
        seed: instance
            .property("Seed")
            .and_then(Value::as_number)
            .unwrap_or(0.0) as i64 as u64,
        spacing: number("Spacing"),
        fill_inside: flag("FillInside"),
        density: number("Density"),
        rotation: pair("RotationRange"),
        scale: pair("ScaleRange"),
        offset: number("Offset"),
        align: flag("AlignToSurface"),
        max_slope: number("MaxSlope"),
    };
    let can_collide = instance
        .property("CanCollide")
        .cloned()
        .unwrap_or(Value::Bool(true));
    let Some(curve) = curve_of(world, generator) else {
        return (Vec::new(), Vec::new(), None);
    };
    let missing: Vec<&String> = settings
        .models
        .iter()
        .filter(|model| assets.model_blocking(model).is_none())
        .collect();
    let problem = (!missing.is_empty()).then(|| {
        let list: Vec<String> = missing.iter().map(|m| format!("`{m}`")).collect();
        format!("The SplineScatter `{name}` can't load {}.", list.join(", "))
    });
    let surfaces = Surfaces::of(world, assets);
    let placed = layout(
        &curve,
        &settings,
        |model| assets.model_blocking(model).map(|m| Vec3::from(m.size)),
        |point| surfaces.below(point),
    );
    let mut results = Vec::new();
    for spot in placed {
        let stem = spot
            .model
            .rsplit('/')
            .next()
            .and_then(|file| file.split('.').next())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("Model")
            .to_owned();
        let properties = vec![
            ("Anchored", Value::Bool(true)),
            ("MeshId", Value::String(spot.model)),
            ("Position", Value::Vector3(vector3(spot.position))),
            (
                "Orientation",
                Value::Vector3(orientation_from_rotation(spot.rotation)),
            ),
            ("Size", Value::Vector3(vector3(spot.size))),
            ("CanCollide", can_collide.clone()),
        ];
        if let Some(id) = add_result(world, generator, &stem, properties) {
            results.push(id);
        }
    }
    (results, Vec::new(), problem)
}

/// A new MeshPart named `name` with `properties`, in `generator`.
fn add_result(
    world: &mut World,
    generator: InstanceId,
    name: &str,
    properties: Vec<(&str, Value)>,
) -> Option<InstanceId> {
    let id = world.create(ClassName::MeshPart).ok()?;
    let _ = world.set_name(id, name);
    for (key, value) in properties {
        let _ = world.set_property(id, key, value);
    }
    if world.set_parent(id, Some(generator)).is_err() {
        let _ = world.destroy(id);
        return None;
    }
    Some(id)
}

fn vector(instance: &axcenity_core::Instance, name: &str) -> Vec3 {
    instance
        .property(name)
        .and_then(Value::as_vector3)
        .map(v3)
        .unwrap_or(Vec3::ONE)
}

pub(crate) fn v3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn vector3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// The rotation a part's `Orientation` (degrees; Z, then X, then Y)
/// stands for.
pub fn rotation_from_orientation(orientation: Vec3) -> Quat {
    let [x, y, z] = orientation.to_array().map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

/// The `Orientation` (degrees) of `rotation`, each angle rounded to 3
/// decimals as the editor shows them.
pub fn orientation_from_rotation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    let tidy = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
    Vector3::new(tidy(x), tidy(y), tidy(z))
}
