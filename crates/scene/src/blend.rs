//! Moving parts drawn between physics steps (roadmap G3).
//!
//! Hosts hand [`TerrainCache::set_blends`](crate::TerrainCache::set_blends)
//! each frame's [`Blends`], from the physics step's. A blended part's draw
//! items, its face decals and the lights on it are drawn `alpha` of the
//! way from where it was before the last step to where that step left it.
//! Nothing else changes: picking, the world and kept draw items all stay
//! where the step put the part, and each frame only shifts what it draws.

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use axcenity_render::glam::{Mat4, Vec3};
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
    /// What moves the part's draw items from where it was written to
    /// where it's drawn, worked out once when it's added.
    shift: Mat4,
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
        let to_position = vec3(to.0);
        let to_rotation = rotation_from_orientation(to.1);
        let position = vec3(from.0).lerp(to_position, self.alpha);
        // A step turns a part only a little, so a normalized straight-line
        // blend of the rotations looks the same as a slerp, without the trig.
        let rotation = rotation_from_orientation(from.1).lerp(to_rotation, self.alpha);
        // Undo where it was written, then place it where it's drawn: turn
        // by the difference, about the written position.
        let turn = rotation * to_rotation.conjugate();
        let shift = Mat4::from_rotation_translation(turn, position - turn * to_position);
        if !shift.is_finite() {
            return;
        }
        self.parts.insert(
            part,
            Blended {
                shift,
                to_position: to.0,
                to_orientation: to.1,
            },
        );
    }

    /// Makes room for `more` parts.
    pub fn reserve(&mut self, more: usize) {
        self.parts.reserve(more);
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
        let (blended, part) = match self.parts.get(&id) {
            Some(blended) => (blended, instance),
            None => {
                let class = instance.class();
                if class != ClassName::Decal && !class.is_light() {
                    return None;
                }
                let parent = instance.parent()?;
                (self.parts.get(&parent)?, world.get(parent)?)
            }
        };
        let vector = |name| part.property(name).and_then(Value::as_vector3);
        (vector("Position") == Some(blended.to_position)
            && vector("Orientation") == Some(blended.to_orientation))
        .then_some(blended.shift)
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
