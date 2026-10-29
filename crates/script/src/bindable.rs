//! BindableEvents and BindableFunctions: scripts on one side talking to
//! each other. `event:Fire(...)` fires `event.Event` on the same side with
//! the same values (tables aren't copied), and `func:Invoke(...)` calls
//! `func.OnInvoke` and returns what it returns.
//!
//! `OnInvoke` and a RemoteFunction's `OnServerInvoke` and `OnClientInvoke` are callbacks: a
//! script sets one function, which replaces the one before. They're kept
//! per side in [`Callbacks`], owned by the script that set them, like
//! connected handlers.

use std::collections::BTreeMap;

use axcenity_core::{ClassName, InstanceId};
use mlua::{Function, Lua, MultiValue, Value};

use crate::events::{self, Owner};
use crate::instance::world;
use crate::remote::instance_self;
use crate::scheduler;
use crate::types::type_name;

/// The callback members of each class, which scripts set but can't read.
const CALLBACKS: &[(ClassName, &str)] = &[
    (ClassName::BindableFunction, "OnInvoke"),
    (ClassName::RemoteFunction, "OnServerInvoke"),
    (ClassName::RemoteFunction, "OnClientInvoke"),
    (ClassName::TextChatService, "OnIncomingMessage"),
];

/// The callback member `key` of `class`, if it has one by that name.
pub fn callback_member(class: ClassName, key: &str) -> Option<&'static str> {
    CALLBACKS
        .iter()
        .find(|(c, name)| *c == class && *name == key)
        .map(|(_, name)| *name)
}

/// The callbacks set on this side: each Instance's callback member, with
/// the script that set it.
#[derive(Default)]
pub struct Callbacks(BTreeMap<(InstanceId, &'static str), (Function, Option<InstanceId>)>);

impl Callbacks {
    /// Drops the callbacks `script` set.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.0.retain(|_, (_, owner)| *owner != Some(script));
    }

    /// Drops the callbacks of Instances that no longer exist.
    pub fn forget(&mut self, exists: impl Fn(InstanceId) -> bool) {
        self.0.retain(|(instance, _), _| exists(*instance));
    }
}

/// The function set as `member` of `instance` on this side, with the
/// script that set it.
pub fn callback(
    lua: &Lua,
    instance: InstanceId,
    member: &'static str,
) -> Option<(Function, Option<InstanceId>)> {
    lua.app_data_ref::<Callbacks>()?
        .0
        .get(&(instance, member))
        .cloned()
}

/// `instance.member = value`: a function sets the callback, nil removes it.
pub fn set_callback(
    lua: &Lua,
    instance: InstanceId,
    class: ClassName,
    member: &'static str,
    value: Value,
) -> mlua::Result<()> {
    crate::sides::check(lua, member)?;
    let function = match value {
        Value::Function(function) => Some(function),
        Value::Nil => None,
        other => {
            return Err(mlua::Error::runtime(format!(
                "{class}.{member} must be a function, not a {}",
                type_name(lua, &other)
            )));
        }
    };
    if lua.app_data_ref::<Callbacks>().is_none() {
        lua.set_app_data(Callbacks::default());
    }
    let mut callbacks = lua.app_data_mut::<Callbacks>().expect("made just above");
    match function {
        Some(function) => {
            let script = scheduler::running_script(lua);
            callbacks.0.insert((instance, member), (function, script));
        }
        None => {
            callbacks.0.remove(&(instance, member));
        }
    }
    Ok(())
}

/// The error for reading a callback member, as in Roblox.
pub fn read_callback_error(class: ClassName, member: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{member} is a callback member of {class}; you can only set it, such as \
         {member} = function(...) ... end"
    ))
}

/// `Invoke` calls the callback in the caller's own coroutine, so a callback
/// that waits pauses the caller until it returns, and its errors are the
/// caller's. Lua, because only Lua can let a callback pause.
const INVOKE: &str = r#"
local callback_of = ...

return function(bindable, ...)
    return callback_of(bindable)(...)
end
"#;

/// The BindableEvent's `Fire` and the BindableFunction's `Invoke`, made
/// once per interpreter.
pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let fire = lua.create_function(|lua, (this, args): (Value, MultiValue)| {
        let event = instance_self(lua, "Fire", ClassName::BindableEvent, "event", this)?;
        events::fire(lua, Owner::Instance(event), "Event", args);
        Ok(())
    })?;
    let callback_of = lua.create_function(|lua, this: Value| {
        let function = instance_self(lua, "Invoke", ClassName::BindableFunction, "func", this)?;
        match callback(lua, function, "OnInvoke") {
            Some((callback, _)) => Ok(callback),
            None => {
                let name = world(lua)?
                    .get(function)
                    .map(|instance| instance.name().to_owned())
                    .unwrap_or_default();
                Err(mlua::Error::runtime(format!(
                    "BindableFunction \"{name}\" has no OnInvoke on this side; set it first: \
                     {name}.OnInvoke = function(...) ... end"
                )))
            }
        }
    })?;
    let invoke = lua
        .load(INVOKE)
        .set_name("=[engine]")
        .call::<Function>(callback_of)?;
    Ok(vec![
        (ClassName::BindableEvent, "Fire", fire),
        (ClassName::BindableFunction, "Invoke", invoke),
    ])
}
