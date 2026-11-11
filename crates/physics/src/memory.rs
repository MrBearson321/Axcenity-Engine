//! What the simulation holds (roadmap G7), for the profilers.

use axcenity_core::memory::MemoryUse;
use rapier3d::dynamics::RigidBody;
use rapier3d::geometry::Collider;
use rapier3d::parry::shape::Shape;

use crate::Physics;

impl Physics {
    /// The simulated parts and about the bytes rapier keeps for their
    /// bodies and colliders, mesh and hull shapes included.
    pub fn memory(&self) -> MemoryUse {
        let bodies = self.rapier.bodies.len() * std::mem::size_of::<RigidBody>();
        let colliders: usize = self
            .rapier
            .colliders
            .iter()
            .map(|(_, collider)| std::mem::size_of::<Collider>() + shape_bytes(collider.shape()))
            .sum();
        MemoryUse {
            count: self.body_count(),
            bytes: bodies + colliders,
        }
    }
}

/// What a collider's shape holds on the heap: a mesh's corners and
/// triangles, a hull's points, edges and faces, a height field's heights,
/// and each piece of a compound.
fn shape_bytes(shape: &dyn Shape) -> usize {
    if let Some(mesh) = shape.as_trimesh() {
        std::mem::size_of_val(mesh.vertices()) + std::mem::size_of_val(mesh.indices())
    } else if let Some(hull) = shape.as_convex_polyhedron() {
        std::mem::size_of_val(hull.points())
            + std::mem::size_of_val(hull.vertices())
            + std::mem::size_of_val(hull.edges())
            + std::mem::size_of_val(hull.faces())
    } else if let Some(field) = shape.as_heightfield() {
        field.nrows() * field.ncols() * std::mem::size_of::<f32>()
    } else if let Some(compound) = shape.as_compound() {
        compound
            .shapes()
            .iter()
            .map(|(_, piece)| std::mem::size_of_val(piece) + shape_bytes(piece.as_ref()))
            .sum()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Value, Vector3, World};

    use crate::Physics;

    #[test]
    fn every_simulated_part_is_counted_with_its_body_and_collider() {
        let mut world = World::new();
        let workspace = world.workspace();
        let mut physics = Physics::new();
        physics.step(&mut world, 1.0 / 60.0);
        assert_eq!(physics.memory().count, 0);
        for x in 0..10 {
            let part = world.create(ClassName::Part).unwrap();
            let at = Vector3::new(x as f32 * 3.0, 5.0, 0.0);
            world
                .set_property(part, "Position", Value::Vector3(at))
                .unwrap();
            world.set_parent(part, Some(workspace)).unwrap();
        }
        physics.step(&mut world, 1.0 / 60.0);
        let ten = physics.memory();
        assert_eq!(ten.count, 10);
        assert!(ten.bytes > 10 * 100, "{ten:?}");

        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        physics.step(&mut world, 1.0 / 60.0);
        let eleven = physics.memory();
        assert_eq!(eleven.count, 11);
        assert!(eleven.bytes > ten.bytes);
    }
}
