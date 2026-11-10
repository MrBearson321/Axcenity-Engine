//! `game:GetService("ContentProvider")` (roadmap M5): a script loads files
//! ahead of time, so they don't load the moment they're first needed.
//!
//! `ContentProvider:PreloadAsync(contentIds, callback)` pauses the script,
//! as `task.wait` does, until everything listed has loaded or failed; other
//! scripts and the game go on meanwhile, because the files load on the
//! asset workers (`Assets::request`). An entry is a project path or an
//! Instance, which stands for the files it and its descendants use. A
//! failed file doesn't raise an error: `callback`, if given, hears how
//! each one went, and why goes to the Output like any other load failure.
//! Each step, [`step`] looks for finished loads and resumes the script.

use std::collections::HashSet;

use axcenity_assets::{Assets, LoadStatus, LoadTicket};
use axcenity_core::{FoliageSource, InstanceId, TEXTURE_MAP_PROPERTIES, World};
use mlua::{
    FromLua, Function, Lua, MetaMethod, Table, Thread, UserData, UserDataFields, UserDataMethods,
    Value,
};

use crate::instance::{LuaInstance, destroyed, world};
use crate::scheduler;
use crate::types::{EnumType, LuaEnumItem, type_name};

/// The ContentProvider, as scripts see it.
#[derive(Clone, Copy)]
pub struct LuaContentProvider;

/// Pauses the script until `start`'s loads are over. Each resume is one
/// finished file for the callback (on a thread of its own, so a callback
/// that waits or fails doesn't hold up the rest), and nothing at the end.
const PRELOAD: &str = r#"
local running, yield, error, spawn, start = ...
return function(_service, contentIds, callback)
    local thread, is_main = running()
    if is_main then
        error("PreloadAsync can only pause a script", 2)
    end
    start(thread, contentIds, callback)
    while true do
        local id, status = yield()
        if id == nil then
            return
        end
        spawn(callback, id, status)
    end
end
"#;

const PRELOAD_KEY: &str = "axcenity.content_provider.preload_async";

/// One paused `PreloadAsync`.
struct Job {
    thread: Thread,
    script: Option<InstanceId>,
    /// Whether the script gave a callback to tell about each file.
    tells: bool,
    /// Each file not yet told about, by the id it's told as.
    loads: Vec<(String, LoadTicket)>,
}

/// The `PreloadAsync` calls still waiting.
#[derive(Default)]
pub struct PreloadJobs(Vec<Job>);

impl PreloadJobs {
    /// Forgets the calls `script` waits on, so nothing resumes them.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.0.retain(|job| job.script != Some(script));
    }
}

/// The files `id` and its descendants use, each as a project path. A
/// sky image goes through as `sky`, since by its name it could be a texture.
fn files_of(world: &World, id: InstanceId, files: &mut Vec<(String, bool)>) {
    const PROPERTIES: [&str; 7] = [
        "MeshId",
        "MaterialVariant",
        "Texture",
        "TextureId",
        "Image",
        "SoundId",
        "Bake",
    ];
    for id in std::iter::once(id).chain(world.descendants_iter(id)) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let text = |name: &str| {
            instance
                .property(name)
                .and_then(axcenity_core::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
        };
        files.extend(
            PROPERTIES
                .into_iter()
                .chain(TEXTURE_MAP_PROPERTIES)
                .filter_map(text)
                .map(|path| (path, false)),
        );
        files.extend(text("SkyImage").map(|path| (path, true)));
        if let Some(terrain) = world.terrain(id) {
            files.extend(
                terrain
                    .foliage()
                    .iter()
                    .filter_map(|plant| match &plant.source {
                        FoliageSource::Model(path) => Some((path.clone(), false)),
                        _ => None,
                    }),
            );
        }
    }
}

/// The Lua function `PreloadAsync` is, made the first time it's asked for.
fn preload_function(lua: &Lua) -> mlua::Result<Function> {
    if let Ok(function) = lua.named_registry_value::<Function>(PRELOAD_KEY) {
        return Ok(function);
    }
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let start = lua.create_function(|lua, (thread, ids, callback): (Thread, Value, Value)| {
        let call = "PreloadAsync(contentIds, callback)";
        let tells = match &callback {
            Value::Nil => false,
            Value::Function(_) => true,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "{call}: callback must be a function, not a {}",
                    type_name(lua, other)
                )));
            }
        };
        let not_a_list = |lua: &Lua, value: &Value| {
            mlua::Error::runtime(format!(
                "{call}: contentIds must be a list of file paths and Instances, not a {}",
                type_name(lua, value)
            ))
        };
        let Value::Table(list) = &ids else {
            return Err(not_a_list(lua, &ids));
        };
        let mut files = Vec::new();
        {
            let world = world(lua)?;
            for entry in list.sequence_values::<Value>() {
                let entry = entry?;
                if let Value::String(path) = &entry {
                    files.push((path.to_str()?.to_owned(), false));
                } else {
                    let instance = LuaInstance::from_lua(entry.clone(), lua)
                        .map_err(|_| not_a_list(lua, &entry))?;
                    world.get(instance.0).ok_or_else(destroyed)?;
                    files_of(&world, instance.0, &mut files);
                }
            }
        }
        let assets = lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        let mut seen = HashSet::new();
        let loads = files
            .into_iter()
            .filter(|(path, _)| seen.insert(path.clone()))
            .map(|(path, sky)| {
                let ticket = if sky {
                    assets.request_sky_image(&path)
                } else {
                    assets.request(&path)
                };
                (path, ticket)
            })
            .collect();
        crate::debugger::waiting(lua, &thread);
        let script = scheduler::running_script(lua);
        jobs_mut(lua)?.0.push(Job {
            thread,
            script,
            tells,
            loads,
        });
        Ok(())
    })?;
    let spawn = lua
        .app_data_ref::<crate::sandbox::Shared>()
        .map(|shared| shared.spawn().clone())
        .ok_or_else(|| mlua::Error::runtime("the task library isn't ready"))?;
    let function: Function = lua.load(PRELOAD).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("running")?,
        coroutine.raw_get::<Function>("yield")?,
        lua.globals().raw_get::<Function>("error")?,
        spawn,
        start,
    ))?;
    lua.set_named_registry_value(PRELOAD_KEY, function.clone())?;
    Ok(function)
}

fn jobs_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, PreloadJobs>> {
    lua.app_data_mut::<PreloadJobs>()
        .ok_or_else(|| mlua::Error::runtime("the ContentProvider isn't ready"))
}

impl UserData for LuaContentProvider {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        // A field holding a function: `ContentProvider:PreloadAsync(list)`
        // calls it with the service first. It's Lua so that it can pause.
        fields.add_field_method_get("PreloadAsync", |lua, _| preload_function(lua));
        fields.add_field_method_get("RequestQueueSize", |lua, _| {
            let (done, total) = lua
                .app_data_ref::<Assets>()
                .map(|assets| assets.progress())
                .unwrap_or((0, 0));
            Ok(total - done)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ContentProvider"));
    }
}

/// What a script is told next, by [`step`].
enum Resume {
    File(String, LoadStatus),
    Done,
}

/// Tells the paused scripts about every file that has finished, and resumes
/// the ones whose files have all finished.
pub fn step(lua: &Lua) {
    let mut resumes = Vec::new();
    {
        let Ok(mut jobs) = jobs_mut(lua) else {
            return;
        };
        jobs.0.retain_mut(|job| {
            job.loads.retain(|(id, ticket)| {
                let status = ticket.status();
                if status == LoadStatus::Loading {
                    return true;
                }
                if job.tells {
                    let file = Resume::File(id.clone(), status);
                    resumes.push((job.thread.clone(), job.script, file));
                }
                false
            });
            let done = job.loads.is_empty();
            if done {
                resumes.push((job.thread.clone(), job.script, Resume::Done));
            }
            !done
        });
    }
    for (thread, script, resume) in resumes {
        match resume {
            Resume::File(id, status) => {
                let status = LuaEnumItem::find(
                    EnumType::AssetFetchStatus,
                    if status == LoadStatus::Ready {
                        "Success"
                    } else {
                        "Failure"
                    },
                );
                scheduler::resume_as(lua, &thread, (id, status), script);
            }
            Resume::Done => scheduler::resume_as(lua, &thread, (), script),
        }
    }
}
