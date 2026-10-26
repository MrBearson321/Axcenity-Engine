//! Scene queries that do not step the simulation: raycasts for scripts, and
//! collider outlines for the editor's visual-debug overlays.

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, Vector3, World};
use rapier3d::glamx::Vec3;
use rapier3d::math::Pose;
use rapier3d::prelude::Ray;

use super::{PartState, to_vec3, to_vector3};
use crate::shapes::{self, Form};

/// Where a ray first meets a collider, including a sensor (`CanCollide` off).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub instance: InstanceId,
    pub position: Vector3,
    pub normal: Vector3,
    pub distance: f32,
    /// False when the hit is a sensor (`CanCollide` off). The ray still
    /// reports it. A kinematic character ignores those hits.
    pub can_collide: bool,
}

/// One collider's outline in world space, for the editor overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct ColliderOutline {
    pub instance: InstanceId,
    /// False when the collider is a sensor: nothing bounces off it, but it
    /// still has the same shape.
    pub can_collide: bool,
    pub edges: Vec<(Vector3, Vector3)>,
    /// Used to hide far terrain chunks. For a part, its Position.
    pub center: Vector3,
    /// True for one chunk of a Terrain: the editor hides it when the camera
    /// is far away.
    pub chunk: bool,
}

/// Casts from `origin` along `direction`. The direction's length is how far
/// the ray reaches. Hits sensors as well as solid colliders, so a part that
/// does not collide is still found. Does not move anything.
pub fn cast_ray(
    world: &World,
    assets: &Assets,
    origin: Vector3,
    direction: Vector3,
) -> Option<RayHit> {
    cast_ray_excluding(world, assets, origin, direction, &[])
}

/// Like [`cast_ray`], but parts in `skip` are not hit. The character
/// controller skips its own body so a probe from inside it finds the floor
/// or a wall. Does not move anything.
pub fn cast_ray_excluding(
    world: &World,
    assets: &Assets,
    origin: Vector3,
    direction: Vector3,
    skip: &[InstanceId],
) -> Option<RayHit> {
    let dir = to_vec3(direction);
    let max_toi = dir.length();
    if !origin.x.is_finite()
        || !origin.y.is_finite()
        || !origin.z.is_finite()
        || !max_toi.is_finite()
        || max_toi <= f32::EPSILON
    {
        return None;
    }
    let ray = Ray::new(to_vec3(origin), dir / max_toi);
    let mut best: Option<RayHit> = None;
    for id in world.descendants(world.workspace()) {
        if skip.contains(&id) {
            continue;
        }
        let Some(state) = PartState::read(world, id, assets) else {
            continue;
        };
        let mut consider = |collider: &rapier3d::prelude::Collider, solid: bool| {
            let Some(hit) =
                collider
                    .shape()
                    .cast_ray_and_get_normal(collider.position(), &ray, max_toi, true)
            else {
                return;
            };
            if best.is_some_and(|best| hit.time_of_impact >= best.distance) {
                return;
            }
            let point = ray.origin + ray.dir * hit.time_of_impact;
            best = Some(RayHit {
                instance: id,
                position: to_vector3(point),
                normal: to_vector3(hit.normal),
                distance: hit.time_of_impact,
                can_collide: solid,
            });
        };
        let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
        collider.set_position(state.pose());
        consider(&collider, state.can_collide);
        if let Form::Terrain(terrain) = &state.shape {
            for (local, form, form_size) in
                shapes::foliage_forms(&terrain.data, to_vec3(state.size), assets)
            {
                let mut extra = shapes::collider(&form, form_size);
                extra.set_position(state.pose() * local);
                // Colliding foliage is a solid collider of its own.
                consider(&extra, true);
            }
        }
    }
    best
}

/// Collider outlines for every Part, MeshPart and Terrain in the Workspace,
/// including sensors. Does not create a physics world or change the tree.
pub fn debug_colliders(world: &World, assets: &Assets) -> Vec<ColliderOutline> {
    let mut outlines = Vec::new();
    for id in world.descendants(world.workspace()) {
        let Some(state) = PartState::read(world, id, assets) else {
            continue;
        };
        match &state.shape {
            Form::Terrain(terrain) => {
                let pose = state.pose();
                for chunk in shapes::terrain_chunks(&terrain.data, to_vec3(state.size)) {
                    outlines.push(ColliderOutline {
                        instance: id,
                        can_collide: state.can_collide,
                        edges: chunk
                            .edges
                            .into_iter()
                            .map(|(a, b)| (transform(pose, a), transform(pose, b)))
                            .collect(),
                        center: transform(pose, chunk.center),
                        chunk: true,
                    });
                }
                for (local, form, form_size) in
                    shapes::foliage_forms(&terrain.data, to_vec3(state.size), assets)
                {
                    let world = pose * local;
                    let edges = shapes::wireframe(&form, form_size)
                        .into_iter()
                        .map(|(a, b)| (transform(world, a), transform(world, b)))
                        .collect();
                    outlines.push(ColliderOutline {
                        instance: id,
                        can_collide: true,
                        edges,
                        center: transform(world, Vec3::ZERO),
                        chunk: false,
                    });
                }
            }
            form => {
                let pose = state.pose();
                let edges = shapes::wireframe(form, to_vec3(state.size))
                    .into_iter()
                    .map(|(a, b)| (transform(pose, a), transform(pose, b)))
                    .collect();
                outlines.push(ColliderOutline {
                    instance: id,
                    can_collide: state.can_collide,
                    edges,
                    center: state.position,
                    chunk: false,
                });
            }
        }
    }
    outlines
}

/// One solid collider, for seeing the world from above (the navigation
/// mesh). Sensors (`CanCollide` off) and visual foliage are not solids.
pub struct Solid {
    pub instance: InstanceId,
    /// The corners of the box around the collider, in world space.
    pub min: Vector3,
    pub max: Vector3,
    collider: rapier3d::prelude::Collider,
    /// A Terrain's ground: solid all the way down from its surface.
    open_below: bool,
}

/// Where a vertical line passes through a [`Solid`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    /// Where the line enters from below: `f32::NEG_INFINITY` under terrain.
    pub bottom: f32,
    /// Where it leaves at the top.
    pub top: f32,
    /// Which way the surface faces at the top.
    pub normal: Vector3,
}

impl Solid {
    /// The part of the vertical line at (`x`, `z`) inside this solid, or
    /// `None` if the line misses it. Every Part, MeshPart and foliage
    /// collider is convex, so a line passes through it once.
    pub fn column(&self, x: f32, z: f32) -> Option<Column> {
        let reach = self.max.y - self.min.y + 2.0;
        let cast = |from: f32, dir: f32| {
            let ray = Ray::new(Vec3::new(x, from, z), Vec3::new(0.0, dir, 0.0));
            self.collider.shape().cast_ray_and_get_normal(
                self.collider.position(),
                &ray,
                reach,
                true,
            )
        };
        let from_top = self.max.y + 1.0;
        let top = cast(from_top, -1.0)?;
        let bottom = if self.open_below {
            f32::NEG_INFINITY
        } else {
            let from_bottom = self.min.y - 1.0;
            cast(from_bottom, 1.0).map_or(self.min.y, |hit| from_bottom + hit.time_of_impact)
        };
        Some(Column {
            bottom,
            top: from_top - top.time_of_impact,
            normal: to_vector3(top.normal),
        })
    }
}

/// Every solid collider in the Workspace: Parts and MeshParts with
/// `CanCollide` on, Terrains and their colliding foliage, except the parts
/// in `skip`. Does not create a physics world or change the tree.
pub fn solids(world: &World, assets: &Assets, skip: &[InstanceId]) -> Vec<Solid> {
    let mut solids = Vec::new();
    let mut add = |instance, collider: rapier3d::prelude::Collider, open_below| {
        let aabb = collider.compute_aabb();
        solids.push(Solid {
            instance,
            min: to_vector3(aabb.mins),
            max: to_vector3(aabb.maxs),
            collider,
            open_below,
        });
    };
    for id in world.descendants(world.workspace()) {
        if skip.contains(&id) {
            continue;
        }
        let Some(state) = PartState::read(world, id, assets) else {
            continue;
        };
        let pose = state.pose();
        if state.can_collide {
            let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
            collider.set_position(pose);
            add(id, collider, matches!(state.shape, Form::Terrain(_)));
        }
        if let Form::Terrain(terrain) = &state.shape {
            // Colliding foliage is solid even when the ground isn't.
            for (local, form, form_size) in
                shapes::foliage_forms(&terrain.data, to_vec3(state.size), assets)
            {
                let mut collider = shapes::collider(&form, form_size);
                collider.set_position(pose * local);
                add(id, collider, false);
            }
        }
    }
    solids
}

fn transform(pose: Pose, local: Vec3) -> Vector3 {
    to_vector3(pose * local)
}
