//! How heavy a part is, where its center of mass is, and how an impulse
//! changes how it moves. Read from the world the way the simulation reads
//! it, without stepping (welded parts weigh together, see
//! [`Mass::of_assembly`]). Scripts use it for `GetMass`, `AssemblyMass`,
//! `AssemblyCenterOfMass` and the `Apply…Impulse` methods.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use rapier3d::glamx::{Quat as Rotation, Vec3};
use rapier3d::math::Pose;
use rapier3d::parry::mass_properties::MassProperties;
use rapier3d::parry::utils::SdpMatrix3;

use crate::{PartState, assembly, shapes, to_vec3, to_vector3};

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

/// The part's collider's mass in the part's frame (a cylinder's collider
/// is turned), and where the part is.
fn part(world: &World, assets: &Assets, id: InstanceId) -> Option<(MassProperties, Pose)> {
    if !matches!(
        world.get(id)?.class(),
        ClassName::Part
            | ClassName::MeshPart
            | ClassName::UnionOperation
            | ClassName::NegateOperation
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
    ) {
        return None;
    }
    let state = PartState::read(world, id, assets)?;
    let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
    crate::material::apply(&mut collider, state.physical);
    let local = collider.mass_properties().transform_by(collider.position());
    Some((local, state.pose()))
}

impl Mass {
    /// The mass of the Part or MeshPart `id` alone, or `None` for anything
    /// else (and for a part physics leaves out, with a NaN or infinite
    /// number).
    pub fn of(world: &World, assets: &Assets, id: InstanceId) -> Option<Self> {
        let (local, pose) = part(world, assets, id)?;
        Some(Self {
            mass: local.mass(),
            center: to_vector3(local.world_com(&pose)),
            inverse_inertia: local.world_inv_inertia(&pose.rotation),
        })
    }

    /// The mass of the assembly the part `id` is welded into: every part's
    /// but a Massless one's (other than the root). A part alone is its own
    /// assembly, Massless or not.
    pub fn of_assembly(world: &World, assets: &Assets, id: InstanceId) -> Option<Self> {
        let mut members = assembly::connected_parts(world, id, true);
        if members.len() == 1 {
            return Self::of(world, assets, id);
        }
        members.sort();
        let root = assembly::root(world, &members);
        let mut total = MassProperties::default();
        for member in members {
            let massless = world
                .get(member)
                .and_then(|part| part.property("Massless"))
                .and_then(Value::as_bool)
                == Some(true);
            if member != root && massless {
                continue;
            }
            // In world space, so the parts' masses add up.
            let (local, pose) = part(world, assets, member)?;
            total += local.transform_by(&pose);
        }
        Some(Self {
            mass: total.mass(),
            center: to_vector3(total.local_com),
            inverse_inertia: total.world_inv_inertia(&Rotation::IDENTITY),
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
