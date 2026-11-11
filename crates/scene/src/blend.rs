//! Moving parts drawn between physics steps (roadmap G3).
//!
//! Hosts hand [`TerrainCache::set_blends`](crate::TerrainCache::set_blends)
//! each frame's [`Blends`], from the physics step's. A blended part's draw
//! items, its face decals and the lights on it are drawn `alpha` of the
//! way from where it was before the last step to where that step left it.
//! Nothing else changes: picking, the world and kept draw items all stay
//! where the step put the part, and each frame only shifts what it draws.

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_render::{DrawItem, Light};
use rustc_hash::FxHashMap;

use crate::orientation::rotation_from_orientation;

/// The parts to draw between steps this frame, and how far along.
#[derive(Clone, Debug, Default)]
pub struct Blends {
    alpha: f32,
    parts: FxHashMap<InstanceId, Blended>,
}

#[derive(Clone, Copy, Debug)]
struct Blended {
    from_position: Vec3,
    from_rotation: Quat,
    to_position: Vector3,
    to_orientation: Vector3,
}

impl Blends {
    /// None yet, drawn `alpha` (0 to 1) of the way from before the last
    /// step to after it.
    pub fn new(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            parts: FxHashMap::default(),
        }
    }

    /// Draws `part` between `from` and `to`, each a `Position` and an
    /// `Orientation` (degrees) as the world gives them. `to` is what the
    /// step wrote: a part that's somewhere else by the time it's drawn (a
    /// script moved it) is drawn where it is.
    pub fn add(&mut self, part: InstanceId, from: (Vector3, Vector3), to: (Vector3, Vector3)) {
        self.parts.insert(
            part,
            Blended {
                from_position: vec3(from.0),
                from_rotation: rotation_from_orientation(from.1),
                to_position: to.0,
                to_orientation: to.1,
            },
        );
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// How what `instance` draws moves from where the world has it to
    /// where it's drawn this frame: `None` for anything not blended. A
    /// Decal and a light move with the part they're on.
    pub(crate) fn shift(&self, world: &World, id: InstanceId, instance: &Instance) -> Option<Mat4> {
        if self.parts.is_empty() {
            return None;
        }
        let class = instance.class();
        let (part_id, part) = if class == ClassName::Decal || class.is_light() {
            let parent = instance.parent()?;
            (parent, world.get(parent)?)
        } else {
            (id, instance)
        };
        let blended = self.parts.get(&part_id)?;
        let vector = |name| part.property(name).and_then(Value::as_vector3);
        if vector("Position") != Some(blended.to_position)
            || vector("Orientation") != Some(blended.to_orientation)
        {
            return None;
        }
        let to_position = vec3(blended.to_position);
        let to_rotation = rotation_from_orientation(blended.to_orientation);
        let position = blended.from_position.lerp(to_position, self.alpha);
        let rotation = blended.from_rotation.slerp(to_rotation, self.alpha);
        let written = Mat4::from_rotation_translation(to_rotation, to_position);
        let shift = Mat4::from_rotation_translation(rotation, position) * written.inverse();
        shift.is_finite().then_some(shift)
    }

    /// Moves `items`, which `instance` just drew, to where it's drawn.
    pub(crate) fn shift_items(
        &self,
        world: &World,
        id: InstanceId,
        instance: &Instance,
        items: &mut [DrawItem],
    ) {
        if items.is_empty() {
            return;
        }
        if let Some(shift) = self.shift(world, id, instance) {
            for item in items {
                item.transform = shift * item.transform;
            }
        }
    }

    /// Moves `lights`, which the light `instance` just added, with its part.
    pub(crate) fn shift_lights(
        &self,
        world: &World,
        id: InstanceId,
        instance: &Instance,
        lights: &mut [Light],
    ) {
        if lights.is_empty() {
            return;
        }
        let Some(shift) = self.shift(world, id, instance) else {
            return;
        };
        for light in lights {
            light.position = shift.transform_point3(light.position);
            if let Some(cone) = &mut light.cone {
                cone.direction = shift.transform_vector3(cone.direction);
            }
            if let Some(rect) = &mut light.rect {
                rect.direction = shift.transform_vector3(rect.direction);
                rect.right = shift.transform_vector3(rect.right);
            }
        }
    }
}

fn vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
