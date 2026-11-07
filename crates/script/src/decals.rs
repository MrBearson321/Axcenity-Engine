//! `workspace:SpawnDecal(texture, cframe, size, lifetime)` (Phase 75):
//! places a ProjectedDecal in the Workspace that fades out over the end of
//! its `lifetime` and is then destroyed, for bullet holes, scorch marks
//! and footprints. The Workspace's `MaxSpawnedDecals` caps how many are
//! kept at once on each side: past it, the oldest goes first. The decals
//! belong to the side, not to a script, so they fade even after the
//! script that spawned them stops.

use std::collections::VecDeque;

use axcenity_core::{ClassName, InstanceId, Value, Vector3};
use mlua::{FromLua, Function, Lua};

use crate::cframe::LuaCFrame;
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::{LuaVector3, type_name};
use crate::{push_error, scheduler, tree};

/// How long a spawned decal lasts when `SpawnDecal` isn't told.
const DEFAULT_LIFETIME: f64 = 10.0;
/// Seconds a spawned decal takes to fade out at the end of its lifetime,
/// or a quarter of a shorter lifetime.
const FADE_SECONDS: f64 = 1.0;
/// `MaxSpawnedDecals` when the Workspace has no usable value.
const DEFAULT_CAP: usize = 200;

struct Spawned {
    id: InstanceId,
    born: f64,
    lifetime: f64,
    /// Its Transparency when spawned, from which it fades to 1.
    transparency: f64,
}

/// The decals `SpawnDecal` placed on this side, oldest first.
#[derive(Default)]
pub struct SpawnedDecals(VecDeque<Spawned>);

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let spawn = lua.create_function(
        |lua,
         (this, texture, cframe, size, lifetime): (
            LuaInstance,
            mlua::Value,
            mlua::Value,
            mlua::Value,
            Option<f64>,
        )| {
            let workspace = {
                let world = world(lua)?;
                let class = world.get(this.0).ok_or_else(destroyed)?.class();
                if class != ClassName::Workspace {
                    return Err(mlua::Error::runtime(
                        "SpawnDecal is a method of the Workspace: workspace:SpawnDecal(...)",
                    ));
                }
                this.0
            };
            let texture = match &texture {
                mlua::Value::String(path) => path.to_str()?.to_owned(),
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "SpawnDecal takes the picture's path first, such as \
                         \"assets/textures/hole.png\", not a {}",
                        type_name(lua, other)
                    )));
                }
            };
            let cframe = LuaCFrame::from_lua(cframe.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "SpawnDecal takes a CFrame second (where the decal is, facing \
                     its LookVector), not a {}",
                    type_name(lua, &cframe)
                ))
            })?;
            let size = decal_size(lua, &size)?;
            let lifetime = lifetime.unwrap_or(DEFAULT_LIFETIME);
            // `max` also turns NaN into 0: gone at the next step.
            let lifetime = lifetime.max(0.0);
            let id = {
                let mut world = world_mut(lua)?;
                let id = world
                    .create(ClassName::ProjectedDecal)
                    .map_err(mlua::Error::runtime)?;
                let set = |world: &mut axcenity_core::World, name: &str, value| {
                    world
                        .set_property(id, name, value)
                        .map_err(|err| mlua::Error::runtime(err.to_string()))
                };
                world
                    .set_name(id, "SpawnedDecal")
                    .map_err(|err| mlua::Error::runtime(err.to_string()))?;
                set(&mut world, "Texture", Value::String(texture))?;
                set(&mut world, "Size", Value::Vector3(size))?;
                cframe
                    .write_to(&mut world, id)
                    .map_err(mlua::Error::runtime)?;
                world
                    .set_parent(id, Some(workspace))
                    .map_err(|err| mlua::Error::runtime(err.to_string()))?;
                id
            };
            let born = scheduler::now(lua);
            let cap = cap(lua);
            let over = {
                if lua.app_data_ref::<SpawnedDecals>().is_none() {
                    lua.set_app_data(SpawnedDecals::default());
                }
                let world = world(lua)?;
                let mut spawned = lua.app_data_mut::<SpawnedDecals>().expect("set just above");
                spawned.0.retain(|decal| world.contains(decal.id));
                spawned.0.push_back(Spawned {
                    id,
                    born,
                    lifetime,
                    transparency: 0.0,
                });
                let excess = spawned.0.len().saturating_sub(cap);
                spawned
                    .0
                    .drain(..excess)
                    .map(|decal| decal.id)
                    .collect::<Vec<_>>()
            };
            for old in over {
                tree::destroy(lua, old)?;
            }
            Ok(LuaInstance(id))
        },
    )?;
    Ok(vec![(ClassName::Workspace, "SpawnDecal", spawn)])
}

/// A decal's `Size` from what a script gave: a Vector3, or a number for a
/// square that reaches as deep as it's wide.
fn decal_size(lua: &Lua, size: &mlua::Value) -> mlua::Result<Vector3> {
    let size = match size {
        mlua::Value::Integer(n) => Vector3::new(*n as f32, *n as f32, *n as f32),
        mlua::Value::Number(n) => Vector3::new(*n as f32, *n as f32, *n as f32),
        other => match LuaVector3::from_lua(other.clone(), lua) {
            Ok(v) => v.0,
            Err(_) => {
                return Err(mlua::Error::runtime(format!(
                    "SpawnDecal takes the decal's size third, a Vector3 or a number, not a {}",
                    type_name(lua, other)
                )));
            }
        },
    };
    Ok(size)
}

/// The Workspace's `MaxSpawnedDecals`, at least 0.
fn cap(lua: &Lua) -> usize {
    let Ok(world) = world(lua) else {
        return DEFAULT_CAP;
    };
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("MaxSpawnedDecals"))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .map_or(DEFAULT_CAP, |n| n.max(0.0) as usize)
}

/// Fades each spawned decal through its last moments and destroys those
/// whose lifetime is over.
pub fn step(lua: &Lua) {
    let now = scheduler::now(lua);
    let (fading, over) = {
        let Some(mut spawned) = lua.app_data_mut::<SpawnedDecals>() else {
            return;
        };
        let mut fading = Vec::new();
        let mut over = Vec::new();
        spawned.0.retain_mut(|decal| {
            let age = now - decal.born;
            if age + scheduler::TOLERANCE_SECONDS >= decal.lifetime {
                over.push(decal.id);
                return false;
            }
            let fade = FADE_SECONDS.min(decal.lifetime * 0.25);
            let left = decal.lifetime - age;
            if left < fade {
                // The share of the fade gone by, from 0 to 1.
                let gone = 1.0 - left / fade;
                let start = decal.transparency;
                fading.push((decal.id, start + (1.0 - start) * gone));
            } else if let Ok(world) = world(lua)
                && let Some(value) = world
                    .get(decal.id)
                    .and_then(|instance| instance.property("Transparency"))
                    .and_then(Value::as_number)
            {
                // A script may have changed it; the fade starts from there.
                decal.transparency = value.clamp(0.0, 1.0);
            }
            true
        });
        (fading, over)
    };
    if let Ok(mut world) = world_mut(lua) {
        for (id, transparency) in fading {
            if world.contains(id) {
                let _ = world.set_property(id, "Transparency", Value::Number(transparency));
            }
        }
    }
    for id in over {
        if let Err(err) = tree::destroy(lua, id) {
            push_error(lua, &err);
        }
    }
}
