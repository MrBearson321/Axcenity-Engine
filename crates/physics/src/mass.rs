//! How heavy a part is, where its center of mass is, and how an impulse
//! changes how it moves. Read from the world the way the simulation reads
//! it, without stepping. Scripts use it for `GetMass`, `AssemblyMass`,
//! `AssemblyCenterOfMass` and the `Apply…Impulse` methods.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Vector3, World};
use rapier3d::glamx::Vec3;
use rapier3d::parry::utils::SdpMatrix3;

use crate::{PartState, shapes, to_vec3, to_vector3};

/// A part's mass, center of mass and turning inertia, as it stands.
#[derive(Clone, Copy, Debug)]
pub struct Mass {
    /// Density times volume.
    pub mass: f32,
    /// Where the center of mass is, in the world.
    pub center: Vector3,
    /// How hard it is to spin about each axis, inverted, in world axes.
    inverse_inertia: SdpMatrix3<f32>,
}

impl Mass {
    /// The mass of the Part or MeshPart `id`, or `None` for anything else
    /// (and for a part physics leaves out, with a NaN or infinite number).
    /// Each part is its own assembly, so this is also its assembly's.
    pub fn of(world: &World, assets: &Assets, id: InstanceId) -> Option<Self> {
        if !matches!(
            world.get(id)?.class(),
            ClassName::Part | ClassName::MeshPart
        ) {
            return None;
        }
        let state = PartState::read(world, id, assets)?;
        let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
        crate::material::apply(&mut collider, state.physical);
        // The collider's mass in its own frame, moved into the part's (a
        // cylinder's collider is turned), then into the world's.
        let local = collider.mass_properties().transform_by(collider.position());
        let pose = state.pose();
        Some(Self {
            mass: local.mass(),
            center: to_vector3(local.world_com(&pose)),
            inverse_inertia: local.world_inv_inertia(&pose.rotation),
        })
    }

    /// How much an impulse (mass × velocity) through the center of mass
    /// changes the part's velocity.
    pub fn velocity_change(&self, impulse: Vector3) -> Vector3 {
        if self.mass <= 0.0 {
            return Vector3::default();
        }
        to_vector3(to_vec3(impulse) / self.mass)
    }

    /// How much an angular impulse, about the center of mass, changes how
    /// fast the part spins (radians a second about each world axis).
    pub fn spin_change(&self, angular_impulse: Vector3) -> Vector3 {
        to_vector3(self.inverse_inertia.mul_vec(to_vec3(angular_impulse)))
    }

    /// The angular impulse of `impulse` pushing at `position`: it turns
    /// the part about its center of mass.
    pub fn torque_at(&self, impulse: Vector3, position: Vector3) -> Vector3 {
        let arm: Vec3 = to_vec3(position) - to_vec3(self.center);
        to_vector3(arm.cross(to_vec3(impulse)))
    }
}
