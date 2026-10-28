//! Scripts' spatial queries and collision groups.
//!
//! `RaycastParams.new()` and `OverlapParams.new()` say which parts a query
//! finds; the Workspace's `Raycast`, `Spherecast`, `Blockcast` and
//! `Shapecast` return the first part a ray or a swept shape meets, and
//! `GetPartBoundsInBox`, `GetPartBoundsInRadius` and `GetPartsInPart` list
//! the parts in a space. They read the world through `axcenity_physics`'
//! queries and never step or change it. Scripts' queries pass by parts with
//! `CanQuery` off.
//!
//! `game:GetService("PhysicsService")` registers collision groups and sets
//! which collide, in the world's [`CollisionGroups`](axcenity_core::CollisionGroups).
//! Both sides have it.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, DEFAULT_GROUP, InstanceId, Vector3};
use axcenity_physics::{CastShape, QueryParams, RayHit};
use mlua::{FromLua, Function, IntoLua, Lua, MetaMethod, Table, UserData, UserDataMethods};

use crate::cframe::LuaCFrame;
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{EnumType, LuaEnumItem, LuaVector3, f32_to_lua, type_name};

/// What `RaycastParams` and `OverlapParams` both hold.
#[derive(Clone, Debug)]
struct Params {
    filter: Vec<InstanceId>,
    include: bool,
    collision_group: String,
    respect_can_collide: bool,
    /// RaycastParams only. There's no water yet, so it changes nothing.
    ignore_water: bool,
    /// OverlapParams only: 0 lists every part.
    max_parts: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            filter: Vec::new(),
            include: false,
            collision_group: DEFAULT_GROUP.to_owned(),
            respect_can_collide: false,
            ignore_water: false,
            max_parts: 0,
        }
    }
}

impl Params {
    fn query(&self) -> QueryParams {
        QueryParams {
            filter: self.filter.clone(),
            include: self.include,
            collision_group: Some(self.collision_group.clone()),
            respect_can_collide: self.respect_can_collide,
            respect_can_query: true,
            max_parts: self.max_parts,
        }
    }
}

/// `RaycastParams.new()`: which parts a raycast or shape cast finds.
#[derive(Clone, Debug, Default)]
pub struct LuaRaycastParams(Params);

/// `OverlapParams.new()`: which parts an overlap query lists.
#[derive(Clone, Debug, Default)]
pub struct LuaOverlapParams(Params);

/// The fields each kind of params has, besides the shared ones.
const RAYCAST_ONLY: &str = "IgnoreWater";
const OVERLAP_ONLY: &str = "MaxParts";

fn get(lua: &Lua, params: &Params, kind: &str, key: &str) -> mlua::Result<mlua::Value> {
    match key {
        "FilterDescendantsInstances" => {
            let list = lua.create_table()?;
            for &id in &params.filter {
                list.push(LuaInstance(id))?;
            }
            Ok(mlua::Value::Table(list))
        }
        "FilterType" => {
            let name = if params.include { "Include" } else { "Exclude" };
            LuaEnumItem::find(EnumType::RaycastFilterType, name).into_lua(lua)
        }
        "CollisionGroup" => params.collision_group.as_str().into_lua(lua),
        "RespectCanCollide" => Ok(mlua::Value::Boolean(params.respect_can_collide)),
        RAYCAST_ONLY if kind == "RaycastParams" => Ok(mlua::Value::Boolean(params.ignore_water)),
        OVERLAP_ONLY if kind == "OverlapParams" => (params.max_parts as f64).into_lua(lua),
        _ => Err(not_a_member(key, kind)),
    }
}

fn set(
    lua: &Lua,
    params: &mut Params,
    kind: &str,
    key: &str,
    value: mlua::Value,
) -> mlua::Result<()> {
    let wrong = |what: &str| {
        mlua::Error::runtime(format!(
            "{kind}.{key} must be {what}, not a {}",
            type_name(lua, &value)
        ))
    };
    match key {
        "FilterDescendantsInstances" => {
            params.filter = instances(lua, &value).ok_or_else(|| wrong("a list of Instances"))?
        }
        "FilterType" => {
            let item = LuaEnumItem::from_lua(value.clone(), lua)
                .ok()
                .filter(|item| item.enum_type == EnumType::RaycastFilterType)
                .ok_or_else(|| wrong("an Enum.RaycastFilterType"))?;
            params.include = item.name == "Include";
        }
        "CollisionGroup" => match &value {
            mlua::Value::String(name) => params.collision_group = name.to_str()?.to_owned(),
            _ => return Err(wrong("a string")),
        },
        "RespectCanCollide" => match value {
            mlua::Value::Boolean(on) => params.respect_can_collide = on,
            _ => return Err(wrong("true or false")),
        },
        RAYCAST_ONLY if kind == "RaycastParams" => match value {
            mlua::Value::Boolean(on) => params.ignore_water = on,
            _ => return Err(wrong("true or false")),
        },
        OVERLAP_ONLY if kind == "OverlapParams" => {
            let count = match value {
                mlua::Value::Integer(n) => n as f64,
                mlua::Value::Number(n) => n,
                _ => return Err(wrong("a whole number")),
            };
            if !(count.is_finite() && count >= 0.0 && count.fract() == 0.0) {
                return Err(wrong("a whole number, 0 or more"));
            }
            params.max_parts = count as usize;
        }
        _ => return Err(not_a_member(key, kind)),
    }
    Ok(())
}

/// The Instances in a list (a table of them), or `None` if it holds
/// anything else.
fn instances(lua: &Lua, value: &mlua::Value) -> Option<Vec<InstanceId>> {
    let mlua::Value::Table(list) = value else {
        return None;
    };
    list.sequence_values::<mlua::Value>()
        .map(|item| {
            LuaInstance::from_lua(item.ok()?, lua)
                .ok()
                .map(|instance| instance.0)
        })
        .collect()
}

fn not_a_member(key: &str, kind: &str) -> mlua::Error {
    mlua::Error::runtime(format!("{key} is not a valid member of {kind}"))
}

/// `params:AddToFilter(instance or list)`: adds to `FilterDescendantsInstances`.
fn add_to_filter(lua: &Lua, params: &mut Params, value: mlua::Value) -> mlua::Result<()> {
    if let Ok(instance) = LuaInstance::from_lua(value.clone(), lua) {
        params.filter.push(instance.0);
        return Ok(());
    }
    let more = instances(lua, &value).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "AddToFilter takes an Instance or a list of them, not a {}",
            type_name(lua, &value)
        ))
    })?;
    params.filter.extend(more);
    Ok(())
}

/// The userdata methods both kinds share: fields read and written through
/// `__index` and `__newindex`, and `AddToFilter`.
macro_rules! params_userdata {
    ($ty:ident, $kind:literal) => {
        impl UserData for $ty {
            fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
                methods.add_method_mut("AddToFilter", |lua, this, value: mlua::Value| {
                    add_to_filter(lua, &mut this.0, value)
                });
                methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
                    get(lua, &this.0, $kind, &key)
                });
                methods.add_meta_method_mut(
                    MetaMethod::NewIndex,
                    |lua, this, (key, value): (String, mlua::Value)| {
                        set(lua, &mut this.0, $kind, &key, value)
                    },
                );
                methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok($kind));
            }
        }

        impl FromLua for $ty {
            fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
                match &value {
                    mlua::Value::UserData(data) => data
                        .borrow::<Self>()
                        .map(|params| params.clone())
                        .map_err(|_| {
                            mlua::Error::runtime(format!(
                                "expected {}, got a {}",
                                $kind,
                                type_name(lua, &value)
                            ))
                        }),
                    _ => Err(mlua::Error::runtime(format!(
                        "expected {}, got a {}",
                        $kind,
                        type_name(lua, &value)
                    ))),
                }
            }
        }
    };
}

params_userdata!(LuaRaycastParams, "RaycastParams");
params_userdata!(LuaOverlapParams, "OverlapParams");

/// `RaycastParams.new(fields?)` or `OverlapParams.new(fields?)`: `fields`
/// is an optional table of starting values, such as
/// `RaycastParams.new({FilterDescendantsInstances = {character}})`.
fn new_params(lua: &Lua, kind: &str, fields: Option<Table>) -> mlua::Result<Params> {
    let mut params = Params::default();
    if let Some(fields) = fields {
        for pair in fields.pairs::<String, mlua::Value>() {
            let (key, value) = pair?;
            set(lua, &mut params, kind, &key, value)?;
        }
    }
    Ok(params)
}

pub fn raycast_params_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, fields: Option<Table>| {
            Ok(LuaRaycastParams(new_params(lua, "RaycastParams", fields)?))
        })?,
    )?;
    Ok(library)
}

pub fn overlap_params_library(lua: &Lua) -> mlua::Result<Table> {
    let library = lua.create_table()?;
    library.raw_set(
        "new",
        lua.create_function(|lua, fields: Option<Table>| {
            Ok(LuaOverlapParams(new_params(lua, "OverlapParams", fields)?))
        })?,
    )?;
    Ok(library)
}

/// The query's params: the defaults when nil.
fn raycast_params(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<QueryParams> {
    if value.is_nil() {
        return Ok(Params::default().query());
    }
    LuaRaycastParams::from_lua(value.clone(), lua)
        .map(|params| params.0.query())
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{method}'s last argument must be a RaycastParams, not a {}",
                type_name(lua, &value)
            ))
        })
}

fn overlap_params(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<QueryParams> {
    if value.is_nil() {
        return Ok(Params::default().query());
    }
    LuaOverlapParams::from_lua(value.clone(), lua)
        .map(|params| params.0.query())
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{method}'s last argument must be an OverlapParams, not a {}",
                type_name(lua, &value)
            ))
        })
}

fn assets(lua: &Lua) -> Assets {
    lua.app_data_ref::<Assets>()
        .as_deref()
        .cloned()
        .unwrap_or_default()
}

/// Errors unless `this` is the Workspace.
fn check_workspace(lua: &Lua, this: &LuaInstance, method: &str) -> mlua::Result<()> {
    let world = world(lua)?;
    let class = world.get(this.0).ok_or_else(destroyed)?.class();
    if class != ClassName::Workspace {
        return Err(mlua::Error::runtime(format!(
            "{method} is a method of Workspace, not a {class}"
        )));
    }
    Ok(())
}

fn vector3(lua: &Lua, value: mlua::Value, method: &str, what: &str) -> mlua::Result<Vector3> {
    LuaVector3::from_lua(value.clone(), lua)
        .map(|vector| vector.0)
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{method} needs a {what} Vector3, not a {}",
                type_name(lua, &value)
            ))
        })
}

fn number(lua: &Lua, value: mlua::Value, method: &str, what: &str) -> mlua::Result<f32> {
    match value {
        mlua::Value::Integer(n) => Ok(n as f32),
        mlua::Value::Number(n) => Ok(n as f32),
        other => Err(mlua::Error::runtime(format!(
            "{method} needs a {what} number, not a {}",
            type_name(lua, &other)
        ))),
    }
}

fn cframe(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<axcenity_core::CFrame> {
    LuaCFrame::from_lua(value.clone(), lua)
        .map(LuaCFrame::to_core)
        .map_err(|_| {
            mlua::Error::runtime(format!(
                "{method} needs a CFrame, not a {}",
                type_name(lua, &value)
            ))
        })
}

fn part(lua: &Lua, value: mlua::Value, method: &str) -> mlua::Result<InstanceId> {
    let instance = LuaInstance::from_lua(value.clone(), lua).ok();
    let world = world(lua)?;
    instance
        .filter(|instance| {
            world
                .get(instance.0)
                .is_some_and(|part| part.class().is_base_part())
        })
        .map(|instance| instance.0)
        .ok_or_else(|| {
            mlua::Error::runtime(format!(
                "{method} needs a Part or MeshPart, not a {}",
                type_name(lua, &value)
            ))
        })
}

/// A `RaycastResult`, or nil for no hit.
fn result(lua: &Lua, hit: Option<RayHit>) -> mlua::Result<mlua::Value> {
    let Some(hit) = hit else {
        return Ok(mlua::Value::Nil);
    };
    let result = lua.create_table()?;
    result.set("Instance", LuaInstance(hit.instance))?;
    result.set("Position", LuaVector3(hit.position))?;
    result.set("Normal", LuaVector3(hit.normal))?;
    result.set("Distance", f32_to_lua(hit.distance))?;
    Ok(mlua::Value::Table(result))
}

fn part_list(lua: &Lua, parts: Vec<InstanceId>) -> mlua::Result<Table> {
    lua.create_sequence_from(parts.into_iter().map(LuaInstance))
}

type Args = (
    LuaInstance,
    mlua::Value,
    mlua::Value,
    mlua::Value,
    mlua::Value,
);

/// The Workspace's query methods.
pub fn workspace_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let raycast = lua.create_function(|lua, (this, origin, direction, params, _): Args| {
        check_workspace(lua, &this, "Raycast")?;
        let origin = vector3(lua, origin, "Raycast", "origin")?;
        let direction = vector3(lua, direction, "Raycast", "direction")?;
        let params = raycast_params(lua, params, "Raycast")?;
        let hit = {
            let world = world(lua)?;
            axcenity_physics::raycast(&world, &assets(lua), origin, direction, &params)
        };
        if let Some(mut rays) = lua.app_data_mut::<crate::DebugRays>() {
            rays.push(crate::DebugRay {
                origin,
                direction,
                hit,
            });
        }
        result(lua, hit)
    })?;
    let spherecast =
        lua.create_function(|lua, (this, center, radius, direction, params): Args| {
            check_workspace(lua, &this, "Spherecast")?;
            let center = vector3(lua, center, "Spherecast", "position")?;
            let radius = number(lua, radius, "Spherecast", "radius")?;
            let direction = vector3(lua, direction, "Spherecast", "direction")?;
            let params = raycast_params(lua, params, "Spherecast")?;
            let shape = CastShape::Ball { center, radius };
            sweep(lua, shape, direction, &params)
        })?;
    let blockcast = lua.create_function(|lua, (this, at, size, direction, params): Args| {
        check_workspace(lua, &this, "Blockcast")?;
        let cframe = cframe(lua, at, "Blockcast")?;
        let size = vector3(lua, size, "Blockcast", "size")?;
        let direction = vector3(lua, direction, "Blockcast", "direction")?;
        let params = raycast_params(lua, params, "Blockcast")?;
        sweep(lua, CastShape::Block { cframe, size }, direction, &params)
    })?;
    let shapecast = lua.create_function(|lua, (this, cast, direction, params, _): Args| {
        check_workspace(lua, &this, "Shapecast")?;
        let cast = part(lua, cast, "Shapecast")?;
        let direction = vector3(lua, direction, "Shapecast", "direction")?;
        let params = raycast_params(lua, params, "Shapecast")?;
        sweep(lua, CastShape::Part(cast), direction, &params)
    })?;
    let in_box = lua.create_function(|lua, (this, at, size, params, _): Args| {
        check_workspace(lua, &this, "GetPartBoundsInBox")?;
        let cframe = cframe(lua, at, "GetPartBoundsInBox")?;
        let size = vector3(lua, size, "GetPartBoundsInBox", "size")?;
        let params = overlap_params(lua, params, "GetPartBoundsInBox")?;
        let parts = {
            let world = world(lua)?;
            axcenity_physics::parts_in_box(&world, &assets(lua), cframe, size, &params)
        };
        part_list(lua, parts)
    })?;
    let in_radius = lua.create_function(|lua, (this, center, radius, params, _): Args| {
        check_workspace(lua, &this, "GetPartBoundsInRadius")?;
        let center = vector3(lua, center, "GetPartBoundsInRadius", "position")?;
        let radius = number(lua, radius, "GetPartBoundsInRadius", "radius")?;
        let params = overlap_params(lua, params, "GetPartBoundsInRadius")?;
        let parts = {
            let world = world(lua)?;
            axcenity_physics::parts_in_radius(&world, &assets(lua), center, radius, &params)
        };
        part_list(lua, parts)
    })?;
    let in_part = lua.create_function(|lua, (this, zone, params, _, _): Args| {
        check_workspace(lua, &this, "GetPartsInPart")?;
        let zone = part(lua, zone, "GetPartsInPart")?;
        let params = overlap_params(lua, params, "GetPartsInPart")?;
        let parts = {
            let world = world(lua)?;
            axcenity_physics::parts_in_part(&world, &assets(lua), zone, &params)
        };
        part_list(lua, parts)
    })?;
    Ok(vec![
        (ClassName::Workspace, "Raycast", raycast),
        (ClassName::Workspace, "Spherecast", spherecast),
        (ClassName::Workspace, "Blockcast", blockcast),
        (ClassName::Workspace, "Shapecast", shapecast),
        (ClassName::Workspace, "GetPartBoundsInBox", in_box),
        (ClassName::Workspace, "GetPartBoundsInRadius", in_radius),
        (ClassName::Workspace, "GetPartsInPart", in_part),
    ])
}

fn sweep(
    lua: &Lua,
    shape: CastShape,
    direction: Vector3,
    params: &QueryParams,
) -> mlua::Result<mlua::Value> {
    let hit = {
        let world = world(lua)?;
        axcenity_physics::shapecast(&world, &assets(lua), shape, direction, params)
    };
    result(lua, hit)
}

/// `game:GetService("PhysicsService")`.
#[derive(Clone, Copy)]
pub struct LuaPhysicsService;

impl UserData for LuaPhysicsService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("RegisterCollisionGroup", |lua, _, name: String| {
            world_mut(lua)?
                .collision_groups_mut()
                .register(&name)
                .map_err(mlua::Error::runtime)
        });
        methods.add_method(
            "CollisionGroupSetCollidable",
            |lua, _, (a, b, collidable): (String, String, bool)| {
                world_mut(lua)?
                    .collision_groups_mut()
                    .set_collidable(&a, &b, collidable)
                    .map_err(mlua::Error::runtime)
            },
        );
        methods.add_method(
            "CollisionGroupsAreCollidable",
            |lua, _, (a, b): (String, String)| {
                let world = world(lua)?;
                let groups = world.collision_groups();
                for name in [&a, &b] {
                    if !groups.contains(name) {
                        return Err(mlua::Error::runtime(format!(
                            "there's no collision group named \"{name}\""
                        )));
                    }
                }
                Ok(groups.collidable(&a, &b))
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PhysicsService"));
    }
}
