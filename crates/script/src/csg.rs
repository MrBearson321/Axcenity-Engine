//! `part:UnionAsync(others)`, `part:SubtractAsync(others)` and
//! `part:IntersectAsync(others)` (Phase 50).
//!
//! The call reads each part's solid from the world (`mesh_edit::solid_of`)
//! and starts the boolean on a thread of its own, then pauses the script,
//! as `task.wait` does (only Lua can pause, so it's a short Lua function).
//! Each step, [`step`] looks for finished jobs: the result's model goes
//! into the `Assets` under a made-up id (it's never a file), a new
//! UnionOperation (Parent nil, as in Roblox) gets it, and the script
//! carries on with it. A boolean that fails raises its reason in the
//! script.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;

use axcenity_assets::{Assets, Model, ModelFormat};
use axcenity_core::{ClassName, InstanceId, Value, Vector3};
use axcenity_mesh_edit::{Combined, Operation, Solid, combine, solid_of};
use mlua::{Function, Lua, Table, Thread};

use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::scheduler;

/// Pauses the script until its job is done; the answer resumes it with
/// `true` and the union, or `false` and why, which is raised here.
const ASYNC: &str = r#"
local running, yield, error, start = ...

local function finish(ok, ...)
    if ok then
        return ...
    end
    error((...), 0)
end

return function(name)
    return function(part, others, options)
        local thread, is_main = running()
        if is_main then
            error(name .. " can only pause a script", 2)
        end
        start(thread, name, part, others, options)
        return finish(yield())
    end
end
"#;

/// The made-up model ids of runtime unions: `:runtime/union<N>.glb`.
static NEXT_UNION: AtomicU64 = AtomicU64::new(1);

/// One boolean running on its own thread.
struct Job {
    thread: Thread,
    script: Option<InstanceId>,
    /// The part the method was called on, whose properties the union takes.
    part: InstanceId,
    fidelity: Option<Value>,
    work: JoinHandle<Result<(Combined, Model), String>>,
}

/// The jobs started and not yet handed back.
#[derive(Default)]
pub struct CsgJobs(Vec<Job>);

impl CsgJobs {
    /// Forgets the jobs `script` waits on, so their results resume nothing.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.0.retain(|job| job.script != Some(script));
    }
}

/// The three methods, for every BasePart class.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    lua.set_app_data(CsgJobs::default());
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let start = lua.create_function(
        |lua,
         (thread, name, part, others, options): (
            Thread,
            String,
            LuaInstance,
            mlua::Value,
            Option<Table>,
        )| {
            let call = format!("{name}(parts)");
            let operation = match name.as_str() {
                "SubtractAsync" => Operation::Subtract,
                "IntersectAsync" => Operation::Intersect,
                _ => Operation::Union,
            };
            let others: Vec<LuaInstance> = match others {
                mlua::Value::Table(table) => table
                    .sequence_values::<LuaInstance>()
                    .collect::<mlua::Result<_>>()
                    .map_err(|_| {
                        mlua::Error::runtime(format!("{call}: parts must be a list of parts"))
                    })?,
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "{call}: parts must be a list of parts"
                    )));
                }
            };
            let fidelity = match options {
                Some(options) => match options.raw_get::<mlua::Value>("CollisionFidelity")? {
                    mlua::Value::Nil => None,
                    value => Some(
                        crate::types::enum_item_from_lua(lua, value, "CollisionFidelity").map_err(
                            |err| mlua::Error::runtime(format!("{call}: CollisionFidelity {err}")),
                        )?,
                    ),
                },
                None => None,
            };
            let assets = lua
                .app_data_ref::<Assets>()
                .map(|assets| assets.clone())
                .unwrap_or_default();
            let (keep, cut) = {
                let world = world(lua)?;
                world.get(part.0).ok_or_else(destroyed)?;
                let solid = |id: InstanceId| {
                    solid_of(&world, &assets, id)
                        .map_err(|err| mlua::Error::runtime(format!("{call}: {err}")))
                };
                let negated = |id: InstanceId| {
                    world.get(id).map(|instance| instance.class())
                        == Some(ClassName::NegateOperation)
                };
                let mut keep: Vec<Solid> = vec![solid(part.0)?];
                let mut cut: Vec<Solid> = Vec::new();
                for other in &others {
                    world.get(other.0).ok_or_else(destroyed)?;
                    if operation == Operation::Subtract || negated(other.0) {
                        cut.push(solid(other.0)?);
                    } else {
                        keep.push(solid(other.0)?);
                    }
                }
                (keep, cut)
            };
            let together = match operation {
                Operation::Intersect => Operation::Intersect,
                _ => Operation::Union,
            };
            let folder = assets.folder().unwrap_or_default();
            let precise = matches!(
                &fidelity,
                Some(Value::EnumItem(item)) if item.item == "PreciseConvexDecomposition"
            );
            // The model is read back and, for Precise, its convex pieces
            // made here too: both take a while.
            let work = std::thread::spawn(move || {
                let combined = combine(&keep, &cut, together)?;
                let model =
                    axcenity_assets::load_model(&combined.glb(), &folder, ModelFormat::Gltf)
                        .map_err(|err| format!("the solid couldn't be read back: {err}"))?;
                if precise {
                    let size = combined.size;
                    axcenity_physics::prepare_precise(&model, Vector3::new(size.x, size.y, size.z));
                }
                Ok((combined, model))
            });
            crate::debugger::waiting(lua, &thread);
            let script = scheduler::running_script(lua);
            jobs_mut(lua)?.0.push(Job {
                thread,
                script,
                part: part.0,
                fidelity,
                work,
            });
            Ok(())
        },
    )?;
    let make: Function = lua.load(ASYNC).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("running")?,
        coroutine.raw_get::<Function>("yield")?,
        lua.globals().raw_get::<Function>("error")?,
        start,
    ))?;
    let mut methods = Vec::new();
    for name in ["UnionAsync", "SubtractAsync", "IntersectAsync"] {
        let method: Function = make.call(name)?;
        for class in ClassName::ALL.into_iter().filter(|c| c.is_base_part()) {
            methods.push((class, name, method.clone()));
        }
    }
    Ok(methods)
}

fn jobs_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, CsgJobs>> {
    lua.app_data_mut::<CsgJobs>()
        .ok_or_else(|| mlua::Error::runtime("solid modeling isn't ready"))
}

/// Hands back every finished job: the scripts carry on with their unions.
pub fn step(lua: &Lua) {
    let finished: Vec<Job> = {
        let Ok(mut jobs) = jobs_mut(lua) else {
            return;
        };
        let (done, running) = std::mem::take(&mut jobs.0)
            .into_iter()
            .partition(|job| job.work.is_finished());
        jobs.0 = running;
        done
    };
    for job in finished {
        let result = job
            .work
            .join()
            .unwrap_or_else(|_| Err("the solid modeling job failed".to_owned()))
            .and_then(|(combined, model)| {
                make_union(lua, job.part, job.fidelity, &combined, model)
            });
        match result {
            Ok(union) => scheduler::resume_as(lua, &job.thread, (true, union), job.script),
            Err(message) => scheduler::resume_as(lua, &job.thread, (false, message), job.script),
        }
    }
}

/// A new UnionOperation with `combined` as its model, placed where it is,
/// with the look and physics of `part` (if it's still there).
fn make_union(
    lua: &Lua,
    part: InstanceId,
    fidelity: Option<Value>,
    combined: &Combined,
    model: Model,
) -> Result<LuaInstance, String> {
    let assets = lua
        .app_data_ref::<Assets>()
        .map(|assets| assets.clone())
        .unwrap_or_default();
    let mesh_id = format!(
        ":runtime/union{}.glb",
        NEXT_UNION.fetch_add(1, Ordering::Relaxed)
    );
    assets.insert(&mesh_id, model);

    let mut world = world_mut(lua).map_err(|err| err.to_string())?;
    let union = world
        .create(ClassName::UnionOperation)
        .map_err(|err| err.to_string())?;
    let v = |v: glam::Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
    let mut properties = vec![
        ("MeshId", Value::String(mesh_id)),
        ("Position", v(combined.position)),
        ("Orientation", Value::Vector3(combined.orientation())),
        ("Size", v(combined.size)),
    ];
    if let Some(instance) = world.get(part) {
        for name in [
            "Anchored",
            "Color",
            "CastShadow",
            "Transparency",
            "CanCollide",
            "CanTouch",
            "CanQuery",
            "CollisionGroup",
            "CustomPhysicalProperties",
        ] {
            if let Some(value) = instance.property(name) {
                properties.push((name, value.clone()));
            }
        }
    }
    if let Some(fidelity) = fidelity {
        properties.push(("CollisionFidelity", fidelity));
    }
    for (name, value) in properties {
        let _ = world.set_property(union, name, value);
    }
    Ok(LuaInstance(union))
}
