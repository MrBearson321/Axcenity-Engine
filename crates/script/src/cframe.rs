//! `CFrame`: a position and a rotation together, as scripts see them.
//!
//! A CFrame isn't stored anywhere. `Part.CFrame` is made from the part's
//! `Position` and `Orientation` each time it's read, and setting it writes
//! those two back. `Model:GetPivot()` and `Model:PivotTo(cf)` work the same
//! way over the Model's parts.
//!
//! The rotation is a quaternion (glam's `Quat`): four numbers that turn
//! things without the "gimbal lock" Euler angles have, and that blend
//! smoothly with `slerp` (which always takes the short way round).

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use glam::{EulerRot, Mat3, Quat, Vec3};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{LuaVector3, f32_to_lua, type_name};

pub fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

pub fn from_vec3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// The rotation an `Orientation` in degrees stands for: Z first, then X,
/// then Y, which glam calls `EulerRot::YXZ` with the angles as (y, x, z).
pub fn rotation_from_orientation(orientation: Vector3) -> Quat {
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

/// The `Orientation` in degrees for a rotation: X from -90 to 90, Y and Z
/// from -180 to 180. Each angle is rounded to 3 decimals, so turning by 15°
/// gives 15 and not 14.999999 (the same as the editor's rotate tool).
pub fn orientation_from_rotation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    // `+ 0.0` turns -0.0 into 0.0.
    let tidy = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
    Vector3::new(tidy(x), tidy(y), tidy(z))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LuaCFrame {
    pub position: Vec3,
    pub rotation: Quat,
}

impl LuaCFrame {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
    };

    pub fn new(position: Vec3, rotation: Quat) -> Self {
        // Products of many rotations drift away from length 1; this keeps
        // them a pure rotation.
        Self {
            position,
            rotation: rotation.normalize(),
        }
    }

    /// A stored CFrame (`CFrameValue.Value`, an attribute).
    pub fn from_core(cf: axcenity_core::CFrame) -> Self {
        let [x, y, z, w] = cf.rotation;
        Self::new(to_vec3(cf.position), Quat::from_xyzw(x, y, z, w))
    }

    /// This CFrame, to store.
    pub fn to_core(self) -> axcenity_core::CFrame {
        axcenity_core::CFrame {
            position: from_vec3(self.position),
            rotation: self.rotation.to_array(),
        }
    }

    /// Facing from `at` towards `target`, with its top as close to `up` as
    /// it can be. Looking straight along `up` uses -Z or +X as the top instead.
    pub fn look_at(at: Vec3, target: Vec3, up: Vec3) -> Self {
        let look = (target - at).normalize_or_zero();
        if look == Vec3::ZERO {
            return Self::new(at, Quat::IDENTITY);
        }
        let mut right = look.cross(up).normalize_or_zero();
        if right == Vec3::ZERO {
            // Looking straight up or down: any top would do, so keep the
            // one a part turned from straight ahead would have.
            right = look.cross(Vec3::NEG_Z).normalize_or_zero();
            if right == Vec3::ZERO {
                right = Vec3::X;
            }
        }
        let top = right.cross(look);
        // The columns are where X, Y and Z point; a CFrame looks along -Z.
        let rotation = Quat::from_mat3(&Mat3::from_cols(right, top, -look));
        Self::new(at, rotation)
    }

    pub fn inverse(self) -> Self {
        let rotation = self.rotation.inverse();
        Self::new(rotation * -self.position, rotation)
    }

    pub fn point(self, v: Vec3) -> Vec3 {
        self.rotation * v + self.position
    }

    /// The CFrame `alpha` of the way to `goal`, turning the short way round.
    pub fn lerp(self, goal: Self, alpha: f32) -> Self {
        Self::new(
            self.position.lerp(goal.position, alpha),
            self.rotation.slerp(goal.rotation, alpha),
        )
    }

    /// The CFrame of an Instance with a Vector3 `Position` and
    /// `Orientation`: a Part or a MeshPart.
    pub fn of_instance(instance: &Instance) -> Option<Self> {
        match (
            instance.property("Position"),
            instance.property("Orientation"),
        ) {
            (Some(Value::Vector3(position)), Some(Value::Vector3(orientation))) => Some(Self::new(
                to_vec3(*position),
                rotation_from_orientation(*orientation),
            )),
            _ => None,
        }
    }

    /// Moves `id` to this CFrame by writing its `Position` and
    /// `Orientation`. A property that this CFrame doesn't change keeps its
    /// stored value exactly, so `part.CFrame = part.CFrame` changes nothing,
    /// even for an `Orientation` such as (0, 270, 0) that reads back as (0, -90, 0).
    pub fn write_to(self, world: &mut World, id: InstanceId) -> Result<(), String> {
        let current = world
            .get(id)
            .and_then(Self::of_instance)
            .ok_or("it has no Position and Orientation")?;
        let position = from_vec3(self.position);
        if position != from_vec3(current.position) {
            world
                .set_property(id, "Position", Value::Vector3(position))
                .map_err(|err| err.to_string())?;
        }
        let orientation = orientation_from_rotation(self.rotation);
        if orientation != orientation_from_rotation(current.rotation) {
            world
                .set_property(id, "Orientation", Value::Vector3(orientation))
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

impl std::ops::Mul for LuaCFrame {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::new(self.point(other.position), self.rotation * other.rotation)
    }
}

impl UserData for LuaCFrame {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Position", |_, cf| Ok(LuaVector3(from_vec3(cf.position))));
        fields.add_field_method_get("X", |_, cf| Ok(f32_to_lua(cf.position.x)));
        fields.add_field_method_get("Y", |_, cf| Ok(f32_to_lua(cf.position.y)));
        fields.add_field_method_get("Z", |_, cf| Ok(f32_to_lua(cf.position.z)));
        fields.add_field_method_get("Rotation", |_, cf| {
            Ok(LuaCFrame::new(Vec3::ZERO, cf.rotation))
        });
        fields.add_field_method_get("LookVector", |_, cf| {
            Ok(LuaVector3(from_vec3(cf.rotation * Vec3::NEG_Z)))
        });
        fields.add_field_method_get("RightVector", |_, cf| {
            Ok(LuaVector3(from_vec3(cf.rotation * Vec3::X)))
        });
        fields.add_field_method_get("UpVector", |_, cf| {
            Ok(LuaVector3(from_vec3(cf.rotation * Vec3::Y)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Inverse", |_, cf, ()| Ok(cf.inverse()));
        methods.add_method("Lerp", |_, cf, (goal, alpha): (LuaCFrame, f64)| {
            Ok(cf.lerp(goal, alpha as f32))
        });
        methods.add_method("ToWorldSpace", |_, cf, other: LuaCFrame| Ok(*cf * other));
        methods.add_method("ToObjectSpace", |_, cf, other: LuaCFrame| {
            Ok(cf.inverse() * other)
        });
        methods.add_method("PointToWorldSpace", |_, cf, v: LuaVector3| {
            Ok(LuaVector3(from_vec3(cf.point(to_vec3(v.0)))))
        });
        methods.add_method("PointToObjectSpace", |_, cf, v: LuaVector3| {
            Ok(LuaVector3(from_vec3(cf.inverse().point(to_vec3(v.0)))))
        });
        methods.add_method("VectorToWorldSpace", |_, cf, v: LuaVector3| {
            Ok(LuaVector3(from_vec3(cf.rotation * to_vec3(v.0))))
        });
        methods.add_method("VectorToObjectSpace", |_, cf, v: LuaVector3| {
            Ok(LuaVector3(from_vec3(cf.rotation.inverse() * to_vec3(v.0))))
        });
        // The angles `CFrame.fromEulerAnglesYXZ` takes, in radians.
        methods.add_method("ToEulerAnglesYXZ", |_, cf, ()| {
            let (y, x, z) = cf.rotation.to_euler(EulerRot::YXZ);
            Ok((f32_to_lua(x), f32_to_lua(y), f32_to_lua(z)))
        });

        // `cf * other` places `other` (a CFrame or a point) in `cf`'s space.
        methods.add_meta_function(
            MetaMethod::Mul,
            |lua, (cf, other): (LuaCFrame, mlua::Value)| {
                if let Ok(other) = LuaCFrame::from_lua(other.clone(), lua) {
                    return (cf * other).into_lua(lua);
                }
                if let Ok(v) = LuaVector3::from_lua(other.clone(), lua) {
                    return LuaVector3(from_vec3(cf.point(to_vec3(v.0)))).into_lua(lua);
                }
                Err(mlua::Error::runtime(format!(
                    "a CFrame multiplies by a CFrame or a Vector3, not a {}",
                    type_name(lua, &other)
                )))
            },
        );
        methods.add_meta_function(MetaMethod::Add, |_, (cf, v): (LuaCFrame, LuaVector3)| {
            Ok(LuaCFrame::new(cf.position + to_vec3(v.0), cf.rotation))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (cf, v): (LuaCFrame, LuaVector3)| {
            Ok(LuaCFrame::new(cf.position - to_vec3(v.0), cf.rotation))
        });
        methods.add_meta_method(MetaMethod::Eq, |lua, cf, other: mlua::Value| {
            Ok(LuaCFrame::from_lua(other, lua).is_ok_and(|other| other == *cf))
        });
        // The position, then the rotation matrix row by row, as in Roblox.
        methods.add_meta_method(MetaMethod::ToString, |_, cf, ()| {
            let m = Mat3::from_quat(cf.rotation);
            let p = cf.position;
            let numbers = [
                p.x, p.y, p.z, m.x_axis.x, m.y_axis.x, m.z_axis.x, m.x_axis.y, m.y_axis.y,
                m.z_axis.y, m.x_axis.z, m.y_axis.z, m.z_axis.z,
            ];
            Ok(numbers.map(|n| (n + 0.0).to_string()).join(", "))
        });
    }
}

/// Reads a Vector3 argument of a CFrame constructor.
fn vector_arg(lua: &Lua, value: mlua::Value, function: &str, what: &str) -> mlua::Result<Vec3> {
    LuaVector3::from_lua(value.clone(), lua)
        .map(|v| to_vec3(v.0))
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "CFrame.{function} needs {what} as a Vector3, not a {}",
                type_name(lua, &value)
            ))
        })
}

/// The `CFrame` global.
pub fn library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    // `CFrame.new()`, `CFrame.new(position)` or `CFrame.new(x, y, z)`.
    library.raw_set(
        "new",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let mut args = args.into_iter();
            let position = match args.next() {
                None | Some(mlua::Value::Nil) => Vec3::ZERO,
                Some(first @ mlua::Value::UserData(_)) => {
                    vector_arg(lua, first, "new", "a position")?
                }
                Some(first) => {
                    let number = |value: Option<mlua::Value>| -> mlua::Result<f32> {
                        Ok(
                            Option::<f64>::from_lua(value.unwrap_or(mlua::Value::Nil), lua)?
                                .unwrap_or(0.0) as f32,
                        )
                    };
                    let x = number(Some(first))?;
                    Vec3::new(x, number(args.next())?, number(args.next())?)
                }
            };
            Ok(LuaCFrame::new(position, Quat::IDENTITY))
        })?,
    )?;
    library.raw_set(
        "lookAt",
        lua.create_function(
            |lua, (at, target, up): (mlua::Value, mlua::Value, Option<mlua::Value>)| {
                let at = vector_arg(lua, at, "lookAt", "the position to look from")?;
                let target = vector_arg(lua, target, "lookAt", "the position to look at")?;
                let up = match up {
                    None | Some(mlua::Value::Nil) => Vec3::Y,
                    Some(up) => vector_arg(lua, up, "lookAt", "the up direction")?,
                };
                Ok(LuaCFrame::look_at(at, target, up))
            },
        )?,
    )?;
    // Turns around X, then Y, then Z, each in the CFrame's own space.
    library.raw_set(
        "Angles",
        lua.create_function(|_, (x, y, z): (Option<f64>, Option<f64>, Option<f64>)| {
            let [x, y, z] = [x, y, z].map(|a| a.unwrap_or(0.0) as f32);
            Ok(LuaCFrame::new(
                Vec3::ZERO,
                Quat::from_euler(EulerRot::XYZ, x, y, z),
            ))
        })?,
    )?;
    // The same turn as an `Orientation`, in radians: Z, then X, then Y.
    library.raw_set(
        "fromEulerAnglesYXZ",
        lua.create_function(|_, (x, y, z): (Option<f64>, Option<f64>, Option<f64>)| {
            let [x, y, z] = [x, y, z].map(|a| a.unwrap_or(0.0) as f32);
            Ok(LuaCFrame::new(
                Vec3::ZERO,
                Quat::from_euler(EulerRot::YXZ, y, x, z),
            ))
        })?,
    )?;
    library.raw_set(
        "fromAxisAngle",
        lua.create_function(|lua, (axis, angle): (mlua::Value, f64)| {
            let axis = vector_arg(lua, axis, "fromAxisAngle", "the axis")?.normalize_or_zero();
            if axis == Vec3::ZERO {
                return Err(mlua::Error::runtime(
                    "CFrame.fromAxisAngle needs an axis that isn't zero",
                ));
            }
            Ok(LuaCFrame::new(
                Vec3::ZERO,
                Quat::from_axis_angle(axis, angle as f32),
            ))
        })?,
    )?;
    library.raw_set("identity", LuaCFrame::IDENTITY)?;
    Ok(library)
}

/// `part:GetPivot()`, `part:PivotTo(cf)`, `model:GetPivot()` and
/// `model:PivotTo(cf)`.
pub fn pivot_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, mlua::Function)>> {
    let get_pivot = lua.create_function(|lua, this: LuaInstance| {
        let world = world(lua)?;
        pivot(&world, this.0)
    })?;
    let pivot_to = lua.create_function(|lua, (this, target): (LuaInstance, mlua::Value)| {
        let target = LuaCFrame::from_lua(target.clone(), lua).map_err(|_| {
            mlua::Error::runtime(format!(
                "PivotTo needs a CFrame, such as model:PivotTo(CFrame.new(0, 5, 0)), not a {}",
                type_name(lua, &target)
            ))
        })?;
        let mut world = world_mut(lua)?;
        let pivot = pivot(&world, this.0)?;
        // Undoing the pivot and redoing it isn't exact in floating point, so
        // pivoting to where it already is leaves every part as it was.
        if target == pivot {
            return Ok(());
        }
        let delta = target * pivot.inverse();
        // A part moves alone; a Model moves every part inside it.
        let instance = world.get(this.0).ok_or_else(destroyed)?;
        let parts = if LuaCFrame::of_instance(instance).is_some() {
            vec![this.0]
        } else {
            world.descendants(this.0)
        };
        for id in parts {
            // An Attachment's Position is on its part, so it moves with it.
            let placed = world
                .get(id)
                .filter(|instance| instance.class() != ClassName::Attachment);
            if let Some(cf) = placed.and_then(LuaCFrame::of_instance) {
                (delta * cf)
                    .write_to(&mut world, id)
                    .map_err(mlua::Error::runtime)?;
            }
        }
        Ok(())
    })?;
    let mut methods = Vec::new();
    for class in [
        ClassName::Part,
        ClassName::MeshPart,
        ClassName::Seat,
        ClassName::Model,
    ] {
        methods.push((class, "GetPivot", get_pivot.clone()));
        methods.push((class, "PivotTo", pivot_to.clone()));
    }
    Ok(methods)
}

/// What `GetPivot` gives: a part's own CFrame, or for a Model the centre of
/// the box around all its parts, unturned. A Model without parts pivots
/// around the origin.
pub(crate) fn pivot(world: &World, id: InstanceId) -> mlua::Result<LuaCFrame> {
    let instance = world.get(id).ok_or_else(destroyed)?;
    if let Some(cf) = LuaCFrame::of_instance(instance) {
        return Ok(cf);
    }
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for part in world.descendants(id) {
        let Some(part) = world.get(part) else {
            continue;
        };
        let (Some(cf), Some(Value::Vector3(size))) =
            (LuaCFrame::of_instance(part), part.property("Size"))
        else {
            continue;
        };
        let half = to_vec3(*size) / 2.0;
        for corner in 0..8 {
            let sign = |bit: u32| if corner & bit == 0 { -1.0 } else { 1.0 };
            let point = cf.point(half * Vec3::new(sign(1), sign(2), sign(4)));
            bounds = Some(match bounds {
                None => (point, point),
                Some((low, high)) => (low.min(point), high.max(point)),
            });
        }
    }
    Ok(match bounds {
        Some((low, high)) => LuaCFrame::new((low + high) / 2.0, Quat::IDENTITY),
        None => LuaCFrame::IDENTITY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_round_trips_through_a_rotation() {
        for orientation in [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 60.0, 0.0),
            Vector3::new(-45.0, 170.0, 25.0),
            Vector3::new(89.0, -120.0, -179.0),
        ] {
            let back = orientation_from_rotation(rotation_from_orientation(orientation));
            assert_eq!(back, orientation);
        }
    }

    #[test]
    fn look_at_looks_along_minus_z_of_the_rotation() {
        let cf = LuaCFrame::look_at(Vec3::ZERO, Vec3::new(-5.0, 0.0, 0.0), Vec3::Y);
        let look = cf.rotation * Vec3::NEG_Z;
        assert!(look.abs_diff_eq(Vec3::NEG_X, 1e-6), "{look}");
        // Turning left by 90 degrees looks along -X.
        assert_eq!(
            orientation_from_rotation(cf.rotation),
            Vector3::new(0.0, 90.0, 0.0)
        );
        // Straight up still gives a rotation.
        let up = LuaCFrame::look_at(Vec3::ZERO, Vec3::Y, Vec3::Y);
        assert!((up.rotation * Vec3::NEG_Z).abs_diff_eq(Vec3::Y, 1e-6));
    }

    #[test]
    fn inverse_undoes_and_multiplying_composes() {
        let a = LuaCFrame::new(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_euler(EulerRot::XYZ, 0.3, 1.1, -0.4),
        );
        let point = Vec3::new(4.0, -1.0, 2.0);
        assert!(a.inverse().point(a.point(point)).abs_diff_eq(point, 1e-5));
        let b = LuaCFrame::new(Vec3::new(0.0, 5.0, 0.0), Quat::from_rotation_y(1.0));
        assert!(
            (a * b)
                .point(point)
                .abs_diff_eq(a.point(b.point(point)), 1e-5)
        );
    }
}
