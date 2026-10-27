//! The tree methods every Instance has (Phase 27): finding and listing
//! Instances, `IsA`, `Clone`, `Destroy`, `ClearAllChildren` and
//! `WaitForChild`.
//!
//! `Destroy` fires `Destroying`, `DescendantRemoving` and `AncestryChanged`
//! itself, before the Instances go, so handlers can still read them. The
//! World's change log can't: by the time it fires, the IDs are stale.
//!
//! A cloned Script is only noted here ([`ClonedScripts`]); the runtime
//! knows its code and starts it once it's somewhere scripts run.

use std::collections::HashSet;

use axcenity_core::{ClassName, InstanceId, World};
use mlua::{Function, Lua, UserDataMethods, Value};

use crate::events::{self, Owner};
use crate::instance::{LuaInstance, destroyed, world, world_mut};
use crate::types::type_name;
use crate::{OutputKind, modules, push_output};

/// `WaitForChild` must pause the script, which only Lua can do (as for
/// `task.wait`). `check` and `warn` are Rust. `check` returns the child,
/// or nil and a problem with the arguments; only the first call's problem
/// is raised, so an Instance destroyed while waiting just keeps waiting.
const WAIT_FOR_CHILD: &str = r#"
local check, wait, warn, error = ...

return function(instance, name, timeout)
    local child, problem = check(instance, name, timeout)
    if problem then
        error(problem, 2)
    end
    local waited, warned = 0, false
    while child == nil do
        if timeout ~= nil and waited >= timeout then
            return nil
        end
        waited += wait()
        if timeout == nil and not warned and waited >= 5 then
            warned = true
            warn(instance, name)
        end
        child = check(instance, name)
    end
    return child
end
"#;

/// How long `WaitForChild` waits without a timeout before it warns.
const WARN_AFTER_SECONDS: u32 = 5;

/// `WaitForChild`, made once per interpreter and kept in its app data.
pub struct WaitForChild(Function);

/// Scripts copied by `Clone` since the runtime last asked: each copy, and
/// the script it was copied from.
#[derive(Default)]
pub struct ClonedScripts(pub Vec<(InstanceId, InstanceId)>);

/// The Instances whose `Destroying` has fired, so it fires once each even
/// when a handler destroys them (or an ancestor) again.
#[derive(Default)]
struct Destroying(HashSet<InstanceId>);

/// Makes `WaitForChild` with the scheduler's `task.wait`, and puts it and
/// [`ClonedScripts`] in `lua`'s app data.
pub fn install(lua: &Lua, wait: &Function) -> mlua::Result<()> {
    let check = lua.create_function(|lua, (instance, name, timeout): (Value, Value, Value)| {
        Ok(match check_wait(lua, &instance, &name, &timeout) {
            Ok(child) => (child, None),
            Err(problem) => (None, Some(problem)),
        })
    })?;
    let warn = lua.create_function(|lua, (instance, name): (LuaInstance, String)| {
        let place = world(lua)?
            .full_name(instance.0)
            .unwrap_or_else(|| "a destroyed Instance".to_owned());
        push_output(
            lua,
            OutputKind::Warning,
            format!(
                "WaitForChild has waited {WARN_AFTER_SECONDS} seconds for \"{name}\" in {place}, \
                 and keeps waiting. Give it a timeout to stop, such as WaitForChild(\"{name}\", 10)."
            ),
        );
        Ok(())
    })?;
    let error: Function = lua.globals().raw_get("error")?;
    let function =
        lua.load(WAIT_FOR_CHILD)
            .set_name("=[engine]")
            .call((check, wait.clone(), warn, error))?;
    lua.set_app_data(WaitForChild(function));
    lua.set_app_data(ClonedScripts::default());
    Ok(())
}

/// `instance.WaitForChild`, for `instance:WaitForChild(name)`.
pub fn wait_for_child(lua: &Lua) -> mlua::Result<Function> {
    lua.app_data_ref::<WaitForChild>()
        .map(|function| function.0.clone())
        .ok_or_else(|| mlua::Error::runtime("WaitForChild isn't ready"))
}

/// The child `WaitForChild` is waiting for, if it's there yet.
fn check_wait(
    lua: &Lua,
    instance: &Value,
    name: &Value,
    timeout: &Value,
) -> Result<Option<LuaInstance>, String> {
    let instance = mlua::FromLua::from_lua(instance.clone(), lua).map_err(|_: mlua::Error| {
        "WaitForChild is a method: call it with a colon, such as workspace:WaitForChild(\"Door\")"
            .to_owned()
    })?;
    let LuaInstance(id) = instance;
    let Value::String(name) = name else {
        return Err(format!(
            "WaitForChild needs a child's name, such as WaitForChild(\"Door\"), not a {}",
            type_name(lua, name)
        ));
    };
    match timeout {
        Value::Nil => {}
        Value::Integer(seconds) if *seconds >= 0 => {}
        Value::Number(seconds) if *seconds >= 0.0 => {}
        other => {
            return Err(format!(
                "WaitForChild's timeout must be a number of seconds, 0 or more, not {}",
                match other {
                    Value::Integer(_) | Value::Number(_) => "a negative number".to_owned(),
                    other => format!("a {}", type_name(lua, other)),
                }
            ));
        }
    }
    let world = world(lua).map_err(|err| err.to_string())?;
    if !world.contains(id) {
        return Err(destroyed().to_string());
    }
    Ok(world
        .find_first_child(id, &name.to_string_lossy())
        .map(LuaInstance))
}

pub fn add_methods<M: UserDataMethods<LuaInstance>>(methods: &mut M) {
    methods.add_method("GetChildren", |lua, this, ()| {
        let world = world(lua)?;
        let instance = world.get(this.0).ok_or_else(destroyed)?;
        Ok(instances(instance.children().iter().copied()))
    });
    methods.add_method("GetDescendants", |lua, this, ()| {
        let world = world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(instances(world.descendants(this.0)))
    });
    methods.add_method("IsA", |lua, this, class: String| {
        Ok(class_of(&*world(lua)?, this.0)?.is_a(&class))
    });
    methods.add_method("FindFirstChildOfClass", |lua, this, class: String| {
        let world = world(lua)?;
        let instance = world.get(this.0).ok_or_else(destroyed)?;
        Ok(first(
            &world,
            instance.children().iter().copied(),
            |found| found.as_str() == class,
        ))
    });
    methods.add_method(
        "FindFirstChildWhichIsA",
        |lua, this, (class, recursive): (String, Option<bool>)| {
            let world = world(lua)?;
            let instance = world.get(this.0).ok_or_else(destroyed)?;
            let candidates = if recursive.unwrap_or(false) {
                world.descendants(this.0)
            } else {
                instance.children().to_vec()
            };
            Ok(first(&world, candidates, |found| found.is_a(&class)))
        },
    );
    methods.add_method("FindFirstAncestor", |lua, this, name: String| {
        let world = world(lua)?;
        Ok(ancestors(&world, this.0)?
            .into_iter()
            .find(|&id| world.get(id).is_some_and(|i| i.name() == name))
            .map(LuaInstance))
    });
    methods.add_method("FindFirstAncestorOfClass", |lua, this, class: String| {
        let world = world(lua)?;
        let found = ancestors(&world, this.0)?;
        Ok(first(&world, found, |found| found.as_str() == class))
    });
    methods.add_method("FindFirstAncestorWhichIsA", |lua, this, class: String| {
        let world = world(lua)?;
        let found = ancestors(&world, this.0)?;
        Ok(first(&world, found, |found| found.is_a(&class)))
    });
    methods.add_method("IsDescendantOf", |lua, this, other: Value| {
        let ancestor = instance_arg(lua, other, "IsDescendantOf", "ancestor")?;
        let world = world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(world.is_descendant_of(this.0, ancestor))
    });
    methods.add_method("IsAncestorOf", |lua, this, other: Value| {
        let descendant = instance_arg(lua, other, "IsAncestorOf", "descendant")?;
        let world = world(lua)?;
        world.get(this.0).ok_or_else(destroyed)?;
        Ok(world.is_descendant_of(descendant, this.0))
    });
    methods.add_method("GetFullName", |lua, this, ()| {
        world(lua)?.full_name(this.0).ok_or_else(destroyed)
    });
    methods.add_method("Clone", |lua, this, ()| clone(lua, this.0));
    methods.add_method("Destroy", |lua, this, ()| destroy(lua, this.0));
    methods.add_method("ClearAllChildren", |lua, this, ()| {
        let children = {
            let world = world(lua)?;
            world.get(this.0).ok_or_else(destroyed)?.children().to_vec()
        };
        for child in children {
            let service = world(lua)?
                .get(child)
                .is_some_and(|child| child.class().is_service());
            if !service {
                destroy(lua, child)?;
            }
        }
        Ok(())
    });
}

/// Instances as a Lua array.
fn instances(ids: impl IntoIterator<Item = InstanceId>) -> Vec<LuaInstance> {
    ids.into_iter().map(LuaInstance).collect()
}

fn class_of(world: &World, id: InstanceId) -> mlua::Result<ClassName> {
    Ok(world.get(id).ok_or_else(destroyed)?.class())
}

/// The first of `ids` whose class passes `test`.
fn first(
    world: &World,
    ids: impl IntoIterator<Item = InstanceId>,
    test: impl Fn(ClassName) -> bool,
) -> Option<LuaInstance> {
    ids.into_iter()
        .find(|&id| world.get(id).is_some_and(|i| test(i.class())))
        .map(LuaInstance)
}

/// The Instances above `id`, nearest first, up to `game`.
fn ancestors(world: &World, id: InstanceId) -> mlua::Result<Vec<InstanceId>> {
    let mut found = Vec::new();
    let mut parent = world.get(id).ok_or_else(destroyed)?.parent();
    while let Some(next) = parent {
        found.push(next);
        parent = world.get(next).and_then(|instance| instance.parent());
    }
    Ok(found)
}

fn instance_arg(lua: &Lua, value: Value, method: &str, what: &str) -> mlua::Result<InstanceId> {
    match mlua::FromLua::from_lua(value.clone(), lua) {
        Ok(LuaInstance(id)) => Ok(id),
        Err(_) => Err(mlua::Error::runtime(format!(
            "{method} needs the {what} Instance, not a {}",
            type_name(lua, &value)
        ))),
    }
}

/// `instance:Clone()`: a copy of it and its descendants, with no parent.
fn clone(lua: &Lua, id: InstanceId) -> mlua::Result<LuaInstance> {
    let (copy, scripts) = {
        let mut world = world_mut(lua)?;
        let class = class_of(&world, id)?;
        if class.is_service() {
            return Err(mlua::Error::runtime(format!(
                "{class} is a service, so it can't be cloned"
            )));
        }
        let copy = world
            .duplicate(id)
            .map_err(|err| mlua::Error::runtime(format!("can't clone: {err}")))?;
        // A copy's descendants come in the same order as the original's.
        let originals = std::iter::once(id).chain(world.descendants(id));
        let copies = std::iter::once(copy).chain(world.descendants(copy));
        let scripts: Vec<(InstanceId, InstanceId, ClassName)> = originals
            .zip(copies)
            .filter_map(|(original, copy)| {
                let class = world.get(original)?.class();
                class.is_script().then_some((original, copy, class))
            })
            .collect();
        (copy, scripts)
    };
    for (original, copy, class) in scripts {
        // Requirable at once on this side; the runtime adds it to the other.
        if class == ClassName::ModuleScript {
            modules::copy(lua, original, copy);
        }
        if let Some(mut cloned) = lua.app_data_mut::<ClonedScripts>() {
            cloned.0.push((copy, original));
        }
    }
    Ok(LuaInstance(copy))
}

/// `instance:Destroy()`. `Destroying` fires on it and everything inside it,
/// then `DescendantRemoving` on its ancestors and `AncestryChanged` on the
/// Instances going, then they go. Destroying twice does nothing.
pub fn destroy(lua: &Lua, id: InstanceId) -> mlua::Result<()> {
    let going = {
        let world = world(lua)?;
        let Some(instance) = world.get(id) else {
            return Ok(());
        };
        let class = instance.class();
        if class.is_service() {
            return Err(mlua::Error::runtime(format!(
                "{class} is a service, so it can't be destroyed"
            )));
        }
        let mut going = vec![id];
        going.extend(world.descendants(id));
        going
    };
    if lua.app_data_ref::<Destroying>().is_none() {
        lua.set_app_data(Destroying::default());
    }
    let newly: Vec<InstanceId> = {
        let mut destroying = lua.app_data_mut::<Destroying>().expect("set just above");
        going
            .iter()
            .copied()
            .filter(|&instance| destroying.0.insert(instance))
            .collect()
    };
    if !newly.contains(&id) {
        // Already being destroyed, by a handler further up.
        return Ok(());
    }
    for &instance in &newly {
        events::fire(lua, Owner::Instance(instance), "Destroying", ());
    }
    // Handlers may have changed the tree, so look again.
    let (ancestors, going) = {
        let world = world(lua)?;
        if !world.contains(id) {
            return Ok(());
        }
        let mut going = vec![id];
        going.extend(world.descendants(id));
        (ancestors(&world, id)?, going)
    };
    for &ancestor in &ancestors {
        for &descendant in &going {
            let owner = Owner::Instance(ancestor);
            events::fire(lua, owner, "DescendantRemoving", LuaInstance(descendant));
        }
    }
    for &instance in &going {
        let args = (LuaInstance(id), Value::Nil);
        events::fire(lua, Owner::Instance(instance), "AncestryChanged", args);
    }
    // A handler may have destroyed it already, which is fine.
    let _ = world_mut(lua)?.destroy(id);
    if let Some(mut destroying) = lua.app_data_mut::<Destroying>() {
        for instance in newly.iter().chain(&going) {
            destroying.0.remove(instance);
        }
    }
    Ok(())
}
