//! Fractures (Phase 42): a part with a generated Fracture in it is
//! replaced by its pieces when something hits it faster than the
//! Fracture's `Threshold`, or when a script calls `Shatter(point, force)`.
//!
//! The pieces are the objects `Piece1`, `Piece2`, … of the Fracture's
//! `PiecesId` file (made by `mesh_edit`), in the part's own space: the
//! whole file is the part's shape, so it's stretched to the part's `Size`
//! the way a MeshPart's model is. Each piece becomes a MeshPart where it
//! was in the part, in a Model named after the part, moving as the part
//! did and flying away from the point. The part leaves the game (its
//! parent becomes nil), keeping its Fracture, so `Shattered` can fire on it.
//!
//! After `DebrisLifetime` seconds the pieces are gone: they fade out over
//! the last second (or the whole time, if it's shorter) and their Model is
//! destroyed.

use std::collections::BTreeMap;

use axcenity_assets::{Assets, ModelFormat, list_objects};
use axcenity_core::{ClassName, Instance, InstanceId, PartType, Value, Vector3, World};
use rapier3d::glamx::Vec3;

use crate::{Body, Mass, finite, orientation_to_quat, to_vec3, to_vector3};

/// How long the pieces take to fade out at the end of their lifetime.
const FADE_SECONDS: f64 = 1.0;

/// A part that shatters when hit hard enough, read at every sync.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Breakable {
    pub(crate) fracture: InstanceId,
    /// Studs a second. Impacts never shatter one whose `Threshold` isn't
    /// above 0; only `Shatter` does.
    pub(crate) threshold: f32,
}

/// The parts in `parts` with a generated Fracture in them, by part.
pub(crate) fn breakables(
    world: &World,
    parts: &BTreeMap<InstanceId, Body>,
) -> BTreeMap<InstanceId, Breakable> {
    // From the Fractures, which are few, rather than from every part.
    let parents = world
        .all_of(ClassName::Fracture)
        .filter_map(|fracture| world.get(fracture)?.parent())
        .filter(|part| parts.contains_key(part));
    parents
        .filter_map(|part| {
            let fracture = fracture_in(world, part)?;
            let instance = world.get(fracture)?;
            let threshold = instance
                .property("Threshold")
                .and_then(Value::as_number)
                .unwrap_or(0.0) as f32;
            let threshold = if threshold > 0.0 {
                threshold
            } else {
                f32::INFINITY
            };
            Some((
                part,
                Breakable {
                    fracture,
                    threshold,
                },
            ))
        })
        .collect()
}

/// The first Fracture directly in `part` whose pieces have been generated.
fn fracture_in(world: &World, part: InstanceId) -> Option<InstanceId> {
    world.get(part)?.children().iter().copied().find(|&child| {
        world.get(child).is_some_and(|fracture| {
            fracture.class() == ClassName::Fracture && !pieces_id(fracture).is_empty()
        })
    })
}

fn pieces_id(fracture: &Instance) -> &str {
    fracture
        .property("PiecesId")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// What one shatter left: the Fracture it was, the Model holding the
/// pieces, and the pieces with how see-through each started.
pub(crate) struct Debris {
    pub(crate) fracture: InstanceId,
    model: InstanceId,
    pieces: Vec<(InstanceId, f64)>,
    /// Seconds since the shatter, and how long the pieces last (infinite
    /// for a `DebrisLifetime` of 0).
    age: f64,
    lifetime: f64,
}

impl Debris {
    /// The pieces, in order.
    pub(crate) fn pieces(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.pieces.iter().map(|&(id, _)| id)
    }
}

/// Replaces the part the Fracture `fracture` is in with its pieces, which
/// move at `velocity` (the part's) plus up to `force` ÷ the part's mass
/// away from `point`: that much for the pieces nearest it, half for the
/// farthest. `None`, changing nothing, if the Fracture isn't in a part, its
/// pieces haven't been generated or can't be loaded, or the part has
/// broken numbers.
pub(crate) fn shatter(
    world: &mut World,
    assets: &Assets,
    fracture: InstanceId,
    point: Vector3,
    force: f32,
    velocity: Vector3,
) -> Option<Debris> {
    let instance = world.get(fracture)?;
    if instance.class() != ClassName::Fracture {
        return None;
    }
    let file = pieces_id(instance).to_owned();
    let lifetime = instance
        .property("DebrisLifetime")
        .and_then(Value::as_number)
        .filter(|seconds| *seconds > 0.0)
        .unwrap_or(f64::INFINITY);
    let part_id = instance.parent()?;
    let part = world.get(part_id)?;
    if !part.class().is_base_part() {
        return None;
    }
    let vector = |name| part.property(name).and_then(Value::as_vector3);
    let (position, orientation, size) =
        (vector("Position")?, vector("Orientation")?, vector("Size")?);
    if ![position, orientation, size, point, velocity]
        .iter()
        .all(|v| finite(*v))
    {
        return None;
    }
    let pieces = load_pieces(assets, &file)?;
    let whole = assets.model_blocking(&file)?;
    let mass = Mass::of(world, assets, part_id).map_or(1.0, |mass| mass.mass.max(1e-3));
    let copied: Vec<(&str, Value)> = [
        "Transparency",
        "CastShadow",
        "CanCollide",
        "CanTouch",
        "CanQuery",
        "CollisionGroup",
    ]
    .into_iter()
    .filter_map(|name| Some((name, part.property(name)?.clone())))
    .collect();
    // MeshParts weigh what Plastic does unless told otherwise, so pieces
    // of a Metal part keep their weight this way.
    let physical = part
        .property("CustomPhysicalProperties")
        .and_then(Value::as_physical_properties)
        .flatten()
        .or_else(|| {
            part.property("Material")
                .and_then(Value::as_material)
                .map(|material| material.physical())
        });
    let name = format!("{}Pieces", part.name());
    let parent = part.parent();
    let transparency = part
        .property("Transparency")
        .and_then(Value::as_number)
        .unwrap_or(0.0);

    // The file's units stretched to the part's shape, as for a MeshPart's
    // model.
    let stretch = Vec3::from(whole.size)
        .to_array()
        .map(|axis| if axis > 0.0 { axis } else { 1.0 });
    let shape = part.property("Shape").and_then(Value::as_part_type);
    let scale = shape_bounds(shape, to_vec3(size)) / Vec3::from(stretch);
    let middle = Vec3::from(whole.center);
    let turn = orientation_to_quat(orientation);
    let placed: Vec<(String, Vec3, Vec3)> = pieces
        .iter()
        .map(|(object, model)| {
            let center = to_vec3(position) + turn * ((Vec3::from(model.center) - middle) * scale);
            (object.clone(), center, Vec3::from(model.size) * scale)
        })
        .collect();
    let farthest = placed
        .iter()
        .map(|(_, center, _)| center.distance(to_vec3(point)))
        .fold(0.0f32, f32::max)
        .max(1e-3);

    let model = world.create(ClassName::Model).ok()?;
    let _ = world.set_name(model, name);
    let mut made = Vec::new();
    for (object, center, piece_size) in placed {
        let Ok(piece) = world.create(ClassName::MeshPart) else {
            continue;
        };
        let _ = world.set_name(piece, object.clone());
        let away = (center - to_vec3(point)).normalize_or(Vec3::Y);
        let closeness = 1.0 - 0.5 * center.distance(to_vec3(point)) / farthest;
        let throw = to_vec3(velocity) + away * (force.max(0.0) / mass) * closeness;
        let mut properties = vec![
            ("MeshId", Value::String(format!("{file}#{object}"))),
            ("Size", Value::Vector3(to_vector3(piece_size))),
            ("Position", Value::Vector3(to_vector3(center))),
            ("Orientation", Value::Vector3(orientation)),
            ("Anchored", Value::Bool(false)),
            ("AssemblyLinearVelocity", Value::Vector3(to_vector3(throw))),
            (
                "CustomPhysicalProperties",
                Value::PhysicalProperties(physical),
            ),
        ];
        properties.extend(copied.iter().cloned());
        for (name, value) in properties {
            let _ = world.set_property(piece, name, value);
        }
        let _ = world.set_parent(piece, Some(model));
        made.push((piece, transparency));
    }
    let _ = world.set_parent(model, parent);
    let _ = world.set_parent(part_id, None);
    Some(Debris {
        fracture,
        model,
        pieces: made,
        age: 0.0,
        lifetime,
    })
}

/// The box a part's shape fills at `size`: a Ball's diameter is its
/// smallest side, and a Cylinder or Capsule is as wide as the smaller of
/// Y and Z. A MeshPart (no `Shape`) and a Block or Wedge fill the whole box.
fn shape_bounds(shape: Option<PartType>, size: Vec3) -> Vec3 {
    let round = size.y.min(size.z);
    match shape {
        Some(PartType::Ball) => Vec3::splat(size.min_element()),
        Some(PartType::Cylinder | PartType::Capsule) => Vec3::new(size.x, round, round),
        _ => size,
    }
}

/// The pieces in the fracture file `file`, by object name, in order.
fn load_pieces(
    assets: &Assets,
    file: &str,
) -> Option<Vec<(String, std::sync::Arc<axcenity_assets::Model>)>> {
    let format = ModelFormat::of(std::path::Path::new(file))?;
    let bytes = assets.read_file(file).ok()?;
    let objects = list_objects(&bytes, format).ok()?;
    let pieces: Vec<_> = objects
        .into_iter()
        .filter(|object| object.has_mesh())
        .filter_map(|object| {
            let model = assets.model_blocking(&format!("{file}#{}", object.path))?;
            Some((object.path, model))
        })
        .collect();
    (!pieces.is_empty()).then_some(pieces)
}

/// Ages every shatter's pieces by `seconds`: fading them out toward the
/// end of their lifetime, and destroying them at its end. Drops the ones
/// that are gone (destroyed here, or by a script).
pub(crate) fn age(world: &mut World, debris: &mut Vec<Debris>, seconds: f64) {
    debris.retain_mut(|shatter| {
        if !world.contains(shatter.model) {
            return false;
        }
        shatter.age += seconds;
        if shatter.age >= shatter.lifetime {
            let _ = world.destroy(shatter.model);
            return false;
        }
        let fade = FADE_SECONDS.min(shatter.lifetime);
        let faded = ((shatter.age - (shatter.lifetime - fade)) / fade).clamp(0.0, 1.0);
        if faded > 0.0 {
            for &(piece, start) in &shatter.pieces {
                let now = start + (1.0 - start) * faded;
                if world.contains(piece) {
                    let _ = world.set_property(piece, "Transparency", Value::Number(now));
                }
            }
        }
        true
    });
}
