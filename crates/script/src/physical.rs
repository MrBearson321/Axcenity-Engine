//! Bodies from scripts (Phase 34): the `PhysicalProperties` type and
//! library, a part's `ApplyImpulse`, `ApplyImpulseAtPosition`,
//! `ApplyAngularImpulse` and `GetMass`, and its read-only `AssemblyMass`,
//! `AssemblyCenterOfMass` and `CurrentPhysicalProperties`; and a Ragdoll's
//! `ApplyImpulse(bone, impulse)` (Phase 41), which pushes one body; and a
//! Fracture's `Shatter(point, force)` (Phase 42), which the host hands to
//! physics before its next step.
//!
//! An impulse doesn't reach physics directly: it changes the part's
//! `AssemblyLinearVelocity` and `AssemblyAngularVelocity` by the impulse
//! over the part's mass (and inertia), and physics, which compares each
//! part with what it last wrote, gives the body those velocities before
//! its next step. Impulses in one frame add up. Anchored parts don't move.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Material, PhysicalProperties, Value, Vector3, World};
use axcenity_physics::{Mass, assembly};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{LuaEnumItem, LuaVector3, f32_to_lua, type_name};

#[derive(Clone, Copy, Debug)]
pub struct LuaPhysicalProperties(pub PhysicalProperties);

impl UserData for LuaPhysicalProperties {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Density", |_, p| Ok(f32_to_lua(p.0.density)));
        fields.add_field_method_get("Friction", |_, p| Ok(f32_to_lua(p.0.friction)));
        fields.add_field_method_get("Elasticity", |_, p| Ok(f32_to_lua(p.0.elasticity)));
        fields.add_field_method_get("FrictionWeight", |_, p| Ok(f32_to_lua(p.0.friction_weight)));
        fields.add_field_method_get("ElasticityWeight", |_, p| {
            Ok(f32_to_lua(p.0.elasticity_weight))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |lua, p, other: mlua::Value| {
            Ok(LuaPhysicalProperties::from_lua(other, lua).is_ok_and(|other| other.0 == p.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, p, ()| {
            let numbers: Vec<String> = p.0.to_array().iter().map(f32::to_string).collect();
            Ok(numbers.join(", "))
        });
    }
}

impl FromLua for LuaPhysicalProperties {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        match &value {
            mlua::Value::UserData(data) => Ok(*data.borrow::<Self>()?),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "PhysicalProperties".to_owned(),
                message: None,
            }),
        }
    }
}

/// The `PhysicalProperties` global: `new(material)`, or `new(density,
/// friction, elasticity, frictionWeight?, elasticityWeight?)` with the
/// weights 1 when left out. Numbers out of range are brought into it.
pub fn library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let args: Vec<mlua::Value> = args.into_iter().collect();
            if let [only] = args.as_slice()
                && let Ok(item) = LuaEnumItem::from_lua(only.clone(), lua)
                && let Some(Value::Material(material)) = item.to_value()
            {
                return Ok(LuaPhysicalProperties(material.physical()));
            }
            let number = |index: usize, default: Option<f64>| match args.get(index) {
                Some(mlua::Value::Number(n)) => Some(*n),
                Some(mlua::Value::Integer(n)) => Some(*n as f64),
                None | Some(mlua::Value::Nil) => default,
                Some(_) => None,
            };
            let numbers = [
                number(0, None),
                number(1, None),
                number(2, None),
                number(3, Some(1.0)),
                number(4, Some(1.0)),
            ];
            let Some(numbers) = numbers.into_iter().collect::<Option<Vec<f64>>>() else {
                return Err(mlua::Error::runtime(
                    "PhysicalProperties.new takes a Material, such as \
                     PhysicalProperties.new(Enum.Material.Ice), or numbers: \
                     PhysicalProperties.new(density, friction, elasticity, frictionWeight, \
                     elasticityWeight)",
                ));
            };
            let [d, f, e, fw, ew] = [0, 1, 2, 3, 4].map(|i| numbers[i] as f32);
            Ok(LuaPhysicalProperties(
                PhysicalProperties::new(d, f, e, fw, ew).clamped(),
            ))
        })?,
    )?;
    Ok(library)
}

/// Whether `id` is a Part or MeshPart, the classes with bodies.
fn is_part(lua: &Lua, id: InstanceId) -> mlua::Result<bool> {
    let world = world(lua)?;
    let class = world.get(id).ok_or_else(destroyed)?.class();
    Ok(class.is_base_part())
}

/// The mass of the part's assembly (everything welded to it) as physics
/// would simulate it now.
fn mass(lua: &Lua, id: InstanceId) -> mlua::Result<Mass> {
    weigh(lua, id, Mass::of_assembly)
}

/// The part's own mass, for `GetMass`.
fn part_mass(lua: &Lua, id: InstanceId) -> mlua::Result<Mass> {
    weigh(lua, id, Mass::of)
}

fn weigh(
    lua: &Lua,
    id: InstanceId,
    how: fn(&World, &Assets, InstanceId) -> Option<Mass>,
) -> mlua::Result<Mass> {
    let assets = lua
        .app_data_ref::<Assets>()
        .as_deref()
        .cloned()
        .unwrap_or_default();
    let world = world(lua)?;
    how(&world, &assets, id).ok_or_else(|| {
        mlua::Error::runtime(
            "this part has a NaN or infinite Position, Orientation or Size, so it has no mass",
        )
    })
}

/// Adds `change` to the part's velocity property `name`, unless it's
/// anchored.
fn add_velocity(lua: &Lua, id: InstanceId, name: &str, change: Vector3) -> mlua::Result<()> {
    let mut world = world_mut(lua)?;
    let part = world.get(id).ok_or_else(destroyed)?;
    if part.property("Anchored") == Some(&Value::Bool(true)) {
        return Ok(());
    }
    let now = part
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap_or_default();
    let new = Vector3::new(now.x + change.x, now.y + change.y, now.z + change.z);
    world
        .set_property(id, name, Value::Vector3(new))
        .map_err(mlua::Error::runtime)
}

fn vector_arg(lua: &Lua, value: mlua::Value, call: &str) -> mlua::Result<Vector3> {
    LuaVector3::from_lua(value.clone(), lua)
        .map(|v| v.0)
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{call} needs a Vector3, not a {}",
                type_name(lua, &value)
            ))
        })
}

/// `ApplyImpulse`, `ApplyImpulseAtPosition`, `ApplyAngularImpulse`,
/// `GetMass`, `GetConnectedParts` and `GetRootPart`, on Parts and
/// MeshParts, and a Ragdoll's `ApplyImpulse(bone, impulse)`. Impulses push
/// the part's whole assembly.
pub fn body_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, mlua::Function)>> {
    let apply_impulse =
        lua.create_function(|lua, (this, impulse): (LuaInstance, mlua::Value)| {
            let impulse = vector_arg(lua, impulse, "ApplyImpulse(impulse)")?;
            let change = mass(lua, this.0)?.velocity_change(impulse);
            add_velocity(lua, this.0, "AssemblyLinearVelocity", change)
        })?;
    let apply_at = lua.create_function(
        |lua, (this, impulse, position): (LuaInstance, mlua::Value, mlua::Value)| {
            let call = "ApplyImpulseAtPosition(impulse, position)";
            let impulse = vector_arg(lua, impulse, call)?;
            let position = vector_arg(lua, position, call)?;
            let mass = mass(lua, this.0)?;
            let spin = mass.spin_change(mass.torque_at(impulse, position));
            add_velocity(
                lua,
                this.0,
                "AssemblyLinearVelocity",
                mass.velocity_change(impulse),
            )?;
            add_velocity(lua, this.0, "AssemblyAngularVelocity", spin)
        },
    )?;
    let apply_angular =
        lua.create_function(|lua, (this, impulse): (LuaInstance, mlua::Value)| {
            let impulse = vector_arg(lua, impulse, "ApplyAngularImpulse(impulse)")?;
            let spin = mass(lua, this.0)?.spin_change(impulse);
            add_velocity(lua, this.0, "AssemblyAngularVelocity", spin)
        })?;
    let get_mass =
        lua.create_function(|lua, this: LuaInstance| Ok(f32_to_lua(part_mass(lua, this.0)?.mass)))?;
    let get_connected =
        lua.create_function(|lua, (this, recursive): (LuaInstance, Option<bool>)| {
            let world = world(lua)?;
            world.get(this.0).ok_or_else(destroyed)?;
            let parts = assembly::connected_parts(&world, this.0, recursive.unwrap_or(false));
            drop(world);
            lua.create_sequence_from(parts.into_iter().map(LuaInstance))
        })?;
    let get_root = lua.create_function(|lua, this: LuaInstance| {
        let world = world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(LuaInstance(assembly::root_part(&world, this.0)))
    })?;
    let knock = lua.create_function(
        |lua, (this, bone, impulse): (LuaInstance, String, mlua::Value)| {
            let impulse = vector_arg(lua, impulse, "ApplyImpulse(bone, impulse)")?;
            let body = {
                let world = world(lua)?;
                world.get(this.0).ok_or_else(destroyed)?;
                axcenity_anim::ragdoll::body_of(&world, this.0, &bone)
            };
            let body = body.ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "ApplyImpulse: the Ragdoll has no body for the bone `{bone}` (Build makes one per main bone, named after it)"
                ))
            })?;
            let change = mass(lua, body)?.velocity_change(impulse);
            add_velocity(lua, body, "AssemblyLinearVelocity", change)
        },
    )?;
    let shatter = lua.create_function(
        |lua, (this, point, force): (LuaInstance, mlua::Value, Option<f64>)| {
            let call = "Shatter(point, force)";
            let point = match point {
                mlua::Value::Nil => None,
                point => Some(vector_arg(lua, point, call)?),
            };
            let force = force.unwrap_or(0.0);
            if !force.is_finite()
                || point.is_some_and(|p| ![p.x, p.y, p.z].iter().all(|c| c.is_finite()))
            {
                return Err(mlua::Error::runtime(
                    "Shatter needs a finite point and force",
                ));
            }
            world(lua)?.get(this.0).ok_or_else(destroyed)?;
            let mut shatters = lua
                .app_data_mut::<crate::Shatters>()
                .ok_or_else(|| mlua::Error::runtime("physics isn't ready"))?;
            shatters.0.push(crate::Shatter {
                fracture: this.0,
                point,
                force: force as f32,
            });
            Ok(())
        },
    )?;
    let mut methods = vec![
        (ClassName::Ragdoll, "ApplyImpulse", knock),
        (ClassName::Fracture, "Shatter", shatter),
    ];
    for class in [
        ClassName::Part,
        ClassName::MeshPart,
        ClassName::Seat,
        ClassName::VehicleSeat,
    ] {
        methods.push((class, "ApplyImpulse", apply_impulse.clone()));
        methods.push((class, "ApplyImpulseAtPosition", apply_at.clone()));
        methods.push((class, "ApplyAngularImpulse", apply_angular.clone()));
        methods.push((class, "GetMass", get_mass.clone()));
        methods.push((class, "GetConnectedParts", get_connected.clone()));
        methods.push((class, "GetRootPart", get_root.clone()));
    }
    Ok(methods)
}

/// The read-only fields worked out from a part: `AssemblyMass`,
/// `AssemblyCenterOfMass` and `CurrentPhysicalProperties`.
pub const COMPUTED: [&str; 3] = [
    "AssemblyMass",
    "AssemblyCenterOfMass",
    "CurrentPhysicalProperties",
];

/// One of [`COMPUTED`] for the part `id`, or `None` if `key` isn't one or
/// `id` isn't a part.
pub fn computed(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<Option<mlua::Value>> {
    if !COMPUTED.contains(&key) || !is_part(lua, id)? {
        return Ok(None);
    }
    let value = match key {
        // Everything welded to it; a part alone keeps its own mass even if
        // it's Massless, as a root part does.
        "AssemblyMass" => f32_to_lua(mass(lua, id)?.mass).into_lua(lua)?,
        "AssemblyCenterOfMass" => LuaVector3(mass(lua, id)?.center).into_lua(lua)?,
        _ => LuaPhysicalProperties(current(lua, id)?).into_lua(lua)?,
    };
    Ok(Some(value))
}

/// The part's `CustomPhysicalProperties`, or its material's.
fn current(lua: &Lua, id: InstanceId) -> mlua::Result<PhysicalProperties> {
    let world = world(lua)?;
    let part = world.get(id).ok_or_else(destroyed)?;
    let custom = part
        .property("CustomPhysicalProperties")
        .and_then(Value::as_physical_properties)
        .flatten();
    let material = part
        .property("Material")
        .and_then(Value::as_material)
        .unwrap_or(Material::Plastic);
    Ok(custom.unwrap_or_else(|| material.physical()))
}
