//! Scene queries that do not step the simulation: raycasts, shape casts
//! and overlap queries for scripts, and collider outlines for the editor's
//! visual-debug overlays.

use std::collections::HashSet;

use axcenity_assets::Assets;
use axcenity_core::{CFrame, InstanceId, Vector3, World};
use rapier3d::glamx::{Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::parry::query::{self, ShapeCastOptions, ShapeCastStatus};
use rapier3d::parry::shape::{Ball, Cuboid, Shape};
use rapier3d::prelude::{Collider, Ray};

use super::{Groups, PartState, to_vec3, to_vector3};
use crate::shapes::{self, Form};

/// Where a ray or a cast shape first meets a collider, including a sensor
/// (`CanCollide` off).
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

/// Which parts a query finds: scripts' `RaycastParams` and
/// `OverlapParams`. The default finds every part, as [`cast_ray`] does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryParams {
    /// `FilterDescendantsInstances`: these, and everything inside them.
    pub filter: Vec<InstanceId>,
    /// `FilterType` Include: only the parts in `filter` are found. Else
    /// (Exclude) they're passed by.
    pub include: bool,
    /// Only parts in groups that collide with this one are found; `None`
    /// finds every group.
    pub collision_group: Option<String>,
    /// Passes by parts with `CanCollide` off.
    pub respect_can_collide: bool,
    /// Passes by parts with `CanQuery` off. Scripts' queries do; the
    /// characters' probes don't, so a solid part always stops them.
    pub respect_can_query: bool,
    /// The most parts an overlap query lists; 0 lists them all.
    pub max_parts: usize,
}

/// [`QueryParams`] made ready to test parts against.
struct Filter<'a> {
    params: &'a QueryParams,
    listed: HashSet<InstanceId>,
    groups: Option<Groups>,
}

impl<'a> Filter<'a> {
    fn new(world: &World, params: &'a QueryParams) -> Self {
        let mut listed = HashSet::new();
        for &id in &params.filter {
            if world.contains(id) {
                listed.insert(id);
                listed.extend(world.descendants(id));
            }
        }
        Self {
            params,
            listed,
            groups: params
                .collision_group
                .as_deref()
                .map(|name| Groups::of(world, name)),
        }
    }

    /// True if the query finds `id`. `solid` is false for a sensor.
    fn admits(&self, id: InstanceId, state: &PartState, solid: bool) -> bool {
        self.listed.contains(&id) == self.params.include
            && (solid || !self.params.respect_can_collide)
            && (state.can_query || !self.params.respect_can_query)
            && self
                .groups
                .is_none_or(|groups| groups.collide(state.groups))
    }
}

/// Each collider a query can find, with its part, its state and whether
/// it's solid: a part's own, and a Terrain's colliding foliage.
fn candidates(
    world: &World,
    assets: &Assets,
    filter: &Filter,
    mut visit: impl FnMut(InstanceId, &PartState, &Collider, bool),
) {
    for id in world.descendants(world.workspace()) {
        let Some(state) = PartState::read(world, id, assets) else {
            continue;
        };
        if filter.admits(id, &state, state.can_collide) {
            let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
            collider.set_position(state.pose() * *collider.position());
            visit(id, &state, &collider, state.can_collide);
        }
        if let Form::Terrain(terrain) = &state.shape
            && filter.admits(id, &state, true)
        {
            for (local, form, form_size) in
                shapes::foliage_forms(&terrain.data, to_vec3(state.size), assets)
            {
                let mut extra = shapes::collider(&form, form_size);
                extra.set_position(state.pose() * local * *extra.position());
                // Colliding foliage is a solid collider of its own.
                visit(id, &state, &extra, true);
            }
        }
    }
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
    raycast(world, assets, origin, direction, &QueryParams::default())
}

/// Like [`cast_ray`], but parts in `skip` (and inside them) are not hit.
/// Navigation agents' sight skips both bodies this way. Does not move
/// anything.
pub fn cast_ray_excluding(
    world: &World,
    assets: &Assets,
    origin: Vector3,
    direction: Vector3,
    skip: &[InstanceId],
) -> Option<RayHit> {
    let params = QueryParams {
        filter: skip.to_vec(),
        ..QueryParams::default()
    };
    raycast(world, assets, origin, direction, &params)
}

/// The first part along a ray that `params` finds (`workspace:Raycast`).
/// The direction's length is how far it reaches. Does not move anything.
pub fn raycast(
    world: &World,
    assets: &Assets,
    origin: Vector3,
    direction: Vector3,
    params: &QueryParams,
) -> Option<RayHit> {
    let dir = to_vec3(direction);
    let max_toi = dir.length();
    if !finite(origin) || !max_toi.is_finite() || max_toi <= f32::EPSILON {
        return None;
    }
    let ray = Ray::new(to_vec3(origin), dir / max_toi);
    let filter = Filter::new(world, params);
    let mut best: Option<RayHit> = None;
    candidates(world, assets, &filter, |id, _, collider, solid| {
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
    });
    best
}

/// The shape a [`shapecast`] sweeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastShape {
    /// `workspace:Spherecast`: a ball of `radius` around `center`.
    Ball { center: Vector3, radius: f32 },
    /// `workspace:Blockcast`: a box of `size` placed at `cframe`.
    Block { cframe: CFrame, size: Vector3 },
    /// `workspace:Shapecast`: the part's own collider, where it is. The
    /// part never hits itself.
    Part(InstanceId),
}

/// Sweeps `shape` along `direction` (its length is how far) and returns
/// the first part `params` finds that it meets, as a ray would. Parts the
/// shape is already inside at the start aren't hits, as in Roblox. Does
/// not move anything.
pub fn shapecast(
    world: &World,
    assets: &Assets,
    shape: CastShape,
    direction: Vector3,
    params: &QueryParams,
) -> Option<RayHit> {
    let dir = to_vec3(direction);
    let max_toi = dir.length();
    if !max_toi.is_finite() || max_toi <= f32::EPSILON {
        return None;
    }
    let (cast, pose, itself) = query_shape(world, assets, shape)?;
    let velocity = dir / max_toi;
    let options = ShapeCastOptions {
        max_time_of_impact: max_toi,
        compute_impact_geometry_on_penetration: false,
        ..ShapeCastOptions::default()
    };
    let filter = Filter::new(world, params);
    let mut best: Option<RayHit> = None;
    candidates(world, assets, &filter, |id, _, collider, solid| {
        if Some(id) == itself {
            return;
        }
        let other = collider.position();
        let Ok(Some(hit)) = query::cast_shapes(
            &pose,
            velocity,
            cast.as_ref(),
            other,
            Vec3::ZERO,
            collider.shape(),
            options,
        ) else {
            return;
        };
        if hit.status == ShapeCastStatus::PenetratingOrWithinTargetDist
            || best.is_some_and(|best| hit.time_of_impact >= best.distance)
        {
            return;
        }
        best = Some(RayHit {
            instance: id,
            position: to_vector3(*other * hit.witness2),
            normal: to_vector3(other.rotation * hit.normal2),
            distance: hit.time_of_impact,
            can_collide: solid,
        });
    });
    best
}

/// The parts whose bounding boxes overlap the box of `size` at `cframe`
/// (`workspace:GetPartBoundsInBox`). Terrains aren't listed. Does not move
/// anything.
pub fn parts_in_box(
    world: &World,
    assets: &Assets,
    cframe: CFrame,
    size: Vector3,
    params: &QueryParams,
) -> Vec<InstanceId> {
    let Some(cuboid) = box_shape(size) else {
        return Vec::new();
    };
    overlapping(world, assets, &cuboid, pose_of(cframe), None, true, params)
}

/// The parts whose bounding boxes are within `radius` of `center`
/// (`workspace:GetPartBoundsInRadius`). Terrains aren't listed. Does not
/// move anything.
pub fn parts_in_radius(
    world: &World,
    assets: &Assets,
    center: Vector3,
    radius: f32,
    params: &QueryParams,
) -> Vec<InstanceId> {
    if !(radius.is_finite() && radius >= 0.0) || !finite(center) {
        return Vec::new();
    }
    let pose = Pose::from_translation(to_vec3(center));
    overlapping(world, assets, &Ball::new(radius), pose, None, true, params)
}

/// The parts whose colliders overlap `part`'s (`workspace:GetPartsInPart`),
/// not counting the part itself. Terrains aren't listed. Does not move
/// anything.
pub fn parts_in_part(
    world: &World,
    assets: &Assets,
    part: InstanceId,
    params: &QueryParams,
) -> Vec<InstanceId> {
    let Some((shape, pose, itself)) = query_shape(world, assets, CastShape::Part(part)) else {
        return Vec::new();
    };
    overlapping(world, assets, shape.as_ref(), pose, itself, false, params)
}

/// The parts `params` finds that overlap `shape` at `pose`, each once, in
/// tree order. With `bounds`, a part counts by its bounding box.
fn overlapping(
    world: &World,
    assets: &Assets,
    shape: &dyn Shape,
    pose: Pose,
    itself: Option<InstanceId>,
    bounds: bool,
    params: &QueryParams,
) -> Vec<InstanceId> {
    let filter = Filter::new(world, params);
    let mut found: Vec<InstanceId> = Vec::new();
    candidates(world, assets, &filter, |id, state, collider, _| {
        if Some(id) == itself
            || matches!(state.shape, Form::Terrain(_))
            || found.last() == Some(&id)
            || (params.max_parts > 0 && found.len() >= params.max_parts)
        {
            return;
        }
        let touches = if bounds {
            let aabb = collider.compute_aabb();
            let center = Pose::from_translation(aabb.center());
            query::intersection_test(&pose, shape, &center, &Cuboid::new(aabb.half_extents()))
        } else {
            query::intersection_test(&pose, shape, collider.position(), collider.shape())
        };
        if touches.is_ok_and(|touches| touches.intersecting) {
            found.push(id);
        }
    });
    found
}

/// The shape a query sweeps or tests, where it is, and the part it's made
/// from (which it never finds).
fn query_shape(
    world: &World,
    assets: &Assets,
    shape: CastShape,
) -> Option<(Box<dyn Shape>, Pose, Option<InstanceId>)> {
    match shape {
        CastShape::Ball { center, radius } => {
            (radius.is_finite() && radius > 0.0 && finite(center)).then(|| {
                let ball: Box<dyn Shape> = Box::new(Ball::new(radius));
                (ball, Pose::from_translation(to_vec3(center)), None)
            })
        }
        CastShape::Block { cframe, size } => {
            let cuboid: Box<dyn Shape> = Box::new(box_shape(size)?);
            Some((cuboid, pose_of(cframe), None))
        }
        CastShape::Part(id) => {
            let state = PartState::read(world, id, assets)?;
            if matches!(state.shape, Form::Terrain(_)) {
                return None;
            }
            let collider = shapes::collider(&state.shape, to_vec3(state.size));
            let pose = state.pose() * *collider.position();
            Some((collider.shape().clone_dyn(), pose, Some(id)))
        }
    }
}

/// A box of `size`, or `None` if a side isn't a positive finite number.
fn box_shape(size: Vector3) -> Option<Cuboid> {
    let size = to_vec3(size);
    (size.is_finite() && size.min_element() > 0.0).then(|| Cuboid::new(size / 2.0))
}

fn pose_of(cframe: CFrame) -> Pose {
    let [x, y, z, w] = cframe.rotation;
    let rotation = Quat::from_xyzw(x, y, z, w);
    let rotation = if rotation.is_finite() && rotation.length_squared() > 0.0 {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    };
    Pose::from_parts(to_vec3(cframe.position), rotation)
}

fn finite(v: Vector3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
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
