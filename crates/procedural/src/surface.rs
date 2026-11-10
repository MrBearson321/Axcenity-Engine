//! The surface below a point, for SplineScatter: the nearest of the
//! Workspace's terrains and colliding parts a ray straight down meets.
//! Parts are hit as their boxes, MeshParts and unions as their models'
//! triangles. What scatters made is left out, so a layout doesn't stand on
//! itself; SplineMesh pieces count, so a scatter can line a road.

use std::collections::HashMap;
use std::sync::Arc;

use axcenity_assets::{Assets, Model};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_terrain::Placement;
use glam::{Quat, Vec3};

use crate::{rotation_from_orientation, v3};

/// How far above a point the ray starts, and how far below it it looks.
pub const ABOVE: f32 = 20.0;
pub const BELOW: f32 = 500.0;

/// What a ray can stand things on, read from the world once per layout.
pub struct Surfaces {
    terrains: Vec<(Placement, Arc<axcenity_core::TerrainData>)>,
    parts: Vec<Solid>,
}

struct Solid {
    position: Vec3,
    rotation: Quat,
    half: Vec3,
    /// A MeshPart's or union's model, hit by its triangles.
    model: Option<Arc<Model>>,
}

impl Surfaces {
    /// The terrains and colliding parts in `world`'s Workspace, but for
    /// what scatters made and anything out of the world.
    pub fn of(world: &World, assets: &Assets) -> Surfaces {
        let mut terrains = Vec::new();
        let mut parts = Vec::new();
        let mut models: HashMap<String, Option<Arc<Model>>> = HashMap::new();
        for id in world.descendants_iter(world.workspace()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if let (Some(placement), Some(data)) = (Placement::of(instance), world.terrain(id)) {
                terrains.push((placement, data.clone()));
                continue;
            }
            let class = instance.class();
            if !class.is_base_part()
                || instance.property("CanCollide") == Some(&Value::Bool(false))
                || axcenity_core::csg::out_of_world(world, id)
                || made_by_scatter(world, id)
            {
                continue;
            }
            let vector = |name| {
                instance
                    .property(name)
                    .and_then(Value::as_vector3)
                    .map(v3)
                    .unwrap_or_default()
            };
            let model = class
                .has_model()
                .then(|| {
                    let mesh_id = instance.property("MeshId").and_then(Value::as_str)?;
                    models
                        .entry(mesh_id.to_owned())
                        .or_insert_with(|| assets.model_blocking(mesh_id))
                        .clone()
                })
                .flatten();
            parts.push(Solid {
                position: vector("Position"),
                rotation: rotation_from_orientation(vector("Orientation")),
                half: vector("Size").abs() / 2.0,
                model,
            });
        }
        Surfaces { terrains, parts }
    }

    /// Where a ray straight down from [`ABOVE`] `point` first meets a
    /// surface, and which way that surface faces (one stud long).
    pub fn below(&self, point: Vec3) -> Option<(Vec3, Vec3)> {
        let origin = point + Vec3::Y * ABOVE;
        let reach = ABOVE + BELOW;
        let mut nearest: Option<(f32, Vec3)> = None;
        let mut keep = |distance: f32, normal: Vec3| {
            if distance <= reach && nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, normal));
            }
        };
        for (placement, data) in &self.terrains {
            if let Some(distance) =
                axcenity_terrain::raycast::raycast(data, placement, origin, Vec3::NEG_Y)
            {
                let hit = origin - Vec3::Y * distance;
                let grid = placement.to_grid(data, hit.x, hit.z).round();
                let (size_x, size_z) = data.size();
                let normal = placement.normal(
                    data,
                    (grid.x.max(0.0) as usize).min(size_x - 1),
                    (grid.y.max(0.0) as usize).min(size_z - 1),
                );
                keep(distance, normal);
            }
        }
        for part in &self.parts {
            if let Some((distance, normal)) = part.hit(origin, Vec3::NEG_Y) {
                keep(distance, normal);
            }
        }
        nearest.map(|(distance, normal)| (origin - Vec3::Y * distance, normal))
    }
}

/// True when a SplineScatter made `id`.
fn made_by_scatter(world: &World, id: InstanceId) -> bool {
    axcenity_core::spline::generator_of(world, id)
        .and_then(|generator| world.get(generator))
        .is_some_and(|generator| generator.class() == ClassName::SplineScatter)
}

impl Solid {
    /// How far along the ray it's hit, and the surface's direction there.
    fn hit(&self, origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
        let to_local = self.rotation.inverse();
        let o = to_local * (origin - self.position);
        let d = to_local * direction;
        let (enter, axis) = box_hit(o, d, self.half)?;
        let Some(model) = &self.model else {
            let mut normal = Vec3::ZERO;
            normal[axis] = -d[axis].signum();
            return Some((enter, (self.rotation * normal).normalize_or(Vec3::Y)));
        };
        // The model fills the 1×1×1 cube stretched to the part's size.
        let size = self.half * 2.0;
        let mut nearest: Option<(f32, Vec3)> = None;
        for primitive in &model.primitives {
            for triangle in primitive.indices.as_chunks::<3>().0 {
                let corner = |k: u32| {
                    primitive
                        .positions
                        .get(k as usize)
                        .map(|&p| Vec3::from(p) * size)
                };
                let (Some(a), Some(b), Some(c)) = (
                    corner(triangle[0]),
                    corner(triangle[1]),
                    corner(triangle[2]),
                ) else {
                    continue;
                };
                if let Some(t) = triangle_hit(o, d, a, b, c)
                    && nearest.is_none_or(|(best, _)| t < best)
                {
                    let mut normal = (b - a).cross(c - a).normalize_or(Vec3::Y);
                    if normal.dot(d) > 0.0 {
                        normal = -normal;
                    }
                    nearest = Some((t, normal));
                }
            }
        }
        nearest.map(|(t, normal)| (t, (self.rotation * normal).normalize_or(Vec3::Y)))
    }
}

/// Where a ray enters a box of half-size `half` centered on the origin, and
/// the axis of the face it enters by, if it hits it ahead.
fn box_hit(origin: Vec3, direction: Vec3, half: Vec3) -> Option<(f32, usize)> {
    let mut enter = (f32::NEG_INFINITY, 0);
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let (o, d) = (origin[axis], direction[axis]);
        if d.abs() < 1e-9 {
            if o.abs() > half[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((-half[axis] - o) / d, (half[axis] - o) / d);
        let (near, far) = (a.min(b), a.max(b));
        if near > enter.0 {
            enter = (near, axis);
        }
        exit = exit.min(far);
    }
    (exit >= enter.0.max(0.0)).then_some((enter.0.max(0.0), enter.1))
}

/// Möller–Trumbore: how far along the ray it meets the triangle, if ahead.
fn triangle_hit(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inverse = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(q) * inverse;
    (t >= 0.0).then_some(t)
}
